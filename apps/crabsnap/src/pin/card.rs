//! The card every pin kind sits in: one shape, one shadow, and the pointer.
//!
//! The border is not the card's own: the canvas draws it as a sibling *outside* the card's
//! rectangle, so it never eats into the content and a picture keeps every pixel it has. This
//! module answers what that border and the shadow are.

use crabsnap_core::config::PinSkin;
use ui_core::draw::Sampling;
use ui_core::geom::{Color, Corners};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ImageFit, ShadowSpec};
use ui_core::{NodeKind, Sense};
use ui_platform::Container;

use crate::pin::PinFrame;

/// The card's shell: the node that takes the pointer, opens the menu and casts the shadow.
///
/// A pin of any kind drags, zooms and dismisses the same way; a kind that adds interaction
/// gives its own nodes a `Sense`, and the press lands there instead of on the card.
pub(crate) fn shell(frame: &PinFrame, kind: NodeKind, size: SizeClass) -> Container {
    Container::new(frame.id, kind)
        .menu(frame.menu.clone())
        // CLICK as well: a pin is a clickable object, and the editor's double click is read
        // from the click the engine records rather than from raw presses.
        .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::SCROLL)
        .cursor(frame.cursor)
        .size(size)
        .offset(frame.at.x, frame.at.y)
        // The wheel's zoom is the engine's paint scale: the card keeps its laid-out size and
        // everything inside it — text, swatches, the border — is drawn at the drawn size.
        .scale(frame.zoom)
        .shadow(shadow(frame.skin))
}

/// The card's own box is filled and rounded; the content sits inside it.
///
/// A card's corner is a squircle: the curve of a physical card, not the quarter circle of a
/// plumbing fitting, and the extra area is what keeps a hairline border from looking pinched.
pub(crate) fn filled(card: Container, frame: &PinFrame, fill: Color) -> Container {
    card.map(|decl| decl.background(fill, Corners::all(frame.look.radius).squircle()))
}

/// The card's own box carries the picture instead of a fill. The corners stay square: a
/// rounded one would cut pixels away, and the picture is not a card.
pub(crate) fn pictured(card: Container, texture: u64, sampling: Sampling) -> Container {
    card.background_image(texture, ImageFit::Fill)
        .image_sampling(sampling)
        .map(|decl| decl.background(Color::TRANSPARENT, Corners::all(0.0)))
}

/// Which sampler a picture at `zoom` wants.
///
/// Enlarging, the texels are what the user is looking at, so each one shows as a block;
/// shrinking, a single texel would throw away most of what lies under the destination pixel,
/// so the sampler averages. One rule for the texture entry and the node that draws it, or
/// the uploaded filter and the declared one could disagree.
pub(crate) fn picture_sampling(zoom: f32) -> Sampling {
    if zoom >= 1.0 {
        Sampling::Pixelated
    } else {
        Sampling::Smooth
    }
}

/// The corners a card-shaped outline of this radius draws; a picture's outline stays square.
pub(crate) fn corners(radius: f32, rounded: bool) -> Corners {
    let corners = Corners::all(radius);
    if rounded { corners.squircle() } else { corners }
}

/// The skin partway between the idle and active states.
///
/// Every number moves together, the colours included: one clock for one transition, so a
/// border cannot arrive before the shadow that lifts it.
pub(crate) fn blended(idle: PinSkin, active: PinSkin, t: f32) -> PinSkin {
    let number = |from: f32, to: f32| from + (to - from) * t;
    let channel =
        |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * t).round() as u8;
    let channels = |from: [u8; 4], to: [u8; 4]| {
        [
            channel(from[0], to[0]),
            channel(from[1], to[1]),
            channel(from[2], to[2]),
            channel(from[3], to[3]),
        ]
    };
    PinSkin {
        border_width: number(idle.border_width, active.border_width),
        border: channels(idle.border, active.border),
        shadow_blur: number(idle.shadow_blur, active.shadow_blur),
        shadow_spread: number(idle.shadow_spread, active.shadow_spread),
        shadow_alpha: number(idle.shadow_alpha, active.shadow_alpha),
        shadow: [
            channel(idle.shadow[0], active.shadow[0]),
            channel(idle.shadow[1], active.shadow[1]),
            channel(idle.shadow[2], active.shadow[2]),
        ],
    }
}

/// The border the canvas draws above a card, in the state the pin is in.
pub(crate) fn border(skin: PinSkin) -> (Color, f32) {
    let [r, g, b, a] = skin.border;
    (Color::from_rgba8(r, g, b, a), skin.border_width)
}

fn shadow(skin: PinSkin) -> ShadowSpec {
    let [r, g, b] = skin.shadow;
    ShadowSpec {
        colour: Color::from_rgba8(
            r,
            g,
            b,
            (skin.shadow_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        ),
        blur: skin.shadow_blur,
        spread: skin.shadow_spread,
        // Downwards: a card sits above the desktop, and light comes from above it.
        dy: 3.0,
        inset: false,
    }
}

/// Room a card keeps between its edge and its content, in logical points.
pub(crate) const PAD: f32 = 14.0;

/// The size class a card whose height its content decides uses.
pub(crate) fn hug_height(width: f32) -> SizeClass {
    SizeClass {
        width: AxisSize::fixed(width),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    }
}

/// The size class a card whose content is already measured uses.
pub(crate) fn boxed(size: (f32, f32)) -> SizeClass {
    SizeClass {
        width: AxisSize::fixed(size.0),
        height: AxisSize::fixed(size.1),
        grow_x: Grow::No,
        grow_y: Grow::No,
    }
}
