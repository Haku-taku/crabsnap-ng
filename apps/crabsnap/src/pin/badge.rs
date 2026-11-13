//! The chrome a pin wears: a small pill naming one fact about it.
//!
//! A badge is not content. It is drawn at a fixed size whatever the pin's zoom, it takes
//! no input, and it stays on the output even when the pin hangs off it. The canvas asks for
//! a list and this module decides where each chip goes; the design is
//! `docs/src/apps/pin-badges.md`.

use std::path::Path;

use crabsnap_core::protocol::PinOrigin;
use ui_core::anim::{AnimSpec, Easing};
use ui_core::cx::Cx;
use ui_core::geom::{Color, Corners, Insets, Point, Rect, Size};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind, Sense, Weight};
use ui_design::CommonColor;
use ui_i18n::Catalog;
use ui_style::Style;

/// Space between a badge and the pin's edge, in logical points.
pub(crate) const MARGIN: f32 = 6.0;
/// Space between two chips of one stack.
pub(crate) const GAP: f32 = 4.0;
/// The longest label a chip carries, in characters: a chip, not a banner.
const LABEL_LIMIT: usize = 48;

/// How far a block is folded out of the pin: reflected across the pin's own edges at the
/// anchor, so the same corner of the picture carries it from the other side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fold {
    /// On the picture, at the declared corner or a mirror of it.
    None,
    /// Outside the vertical edge the anchor sits on.
    X,
    /// Outside the horizontal edge the anchor sits on.
    Y,
    /// Outside both: past the corner itself.
    Both,
}

/// Where on its pin a badge sits, read against the pin's drawn rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

/// What happens when the declared anchor leaves the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    /// Stay at the anchor even when the pin has carried it off the output. No built-in
    /// forces a chip: the variant exists for a kind whose chrome must keep its corner.
    #[cfg_attr(not(test), allow(dead_code))]
    Forced,
    /// The declared anchor when it stays on the output and clear of the pointer; then a
    /// mirror, then a fold across the pin's own edge; nothing at all when none of them does.
    Preferred,
}

/// When a badge shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Show {
    /// Always visible: the ordinary fact.
    Always,
    /// While the reveal key is up. Paired with `AdvancedOnly`, the two swap.
    NormalOnly,
    /// Only while the reveal key is held: the detail behind an ordinary badge.
    AdvancedOnly,
}

impl Show {
    /// Whether this badge shows with the reveal key in `reveal`'s state.
    pub(crate) fn visible(self, reveal: bool) -> bool {
        match self {
            Self::Always => true,
            Self::NormalOnly => !reveal,
            Self::AdvancedOnly => reveal,
        }
    }
}

/// One pill of chrome a pin wears.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Badge {
    /// Stable within the pin: the node id and the measured sizes are keyed by it.
    pub(crate) id: &'static str,
    pub(crate) label: String,
    pub(crate) anchor: Anchor,
    pub(crate) policy: Policy,
    pub(crate) show: Show,
    /// The pill's fill and the ink on it.
    pub(crate) tone: Tone,
    /// Whether the fill sits on a blurred copy of what is behind the chip.
    pub(crate) material: Material,
    /// How solid the whole chip is; a badge on its way out carries its fade here.
    pub(crate) opacity: f32,
}

impl Badge {
    pub(crate) fn new(
        id: &'static str,
        label: impl Into<String>,
        anchor: Anchor,
        policy: Policy,
        show: Show,
    ) -> Self {
        Self {
            id,
            label: abbreviate(&label.into(), LABEL_LIMIT),
            anchor,
            policy,
            show,
            // Chrome must not shout: the neutral surface is the default, and a kind that
            // wants attention asks for `Tone::Error`.
            tone: Tone::Surface,
            material: Material::Frosted,
            opacity: 1.0,
        }
    }

    /// The colours this chip is painted with.
    pub(crate) fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// What the chip does with the pixels behind it. No built-in asks for flat paint yet;
    /// the knob is the declarer's, like the rest of the badge's look.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }
}

/// What a chip's fill and ink come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tone {
    /// The pack's raised surface: chrome that stays quiet over a picture.
    Surface,
    /// The pack's error pair, the MD3 badge's own colour: a fact worth noticing.
    Error,
    /// The declarer's own pair: a fill, and an ink that reads on it. No built-in states one
    /// yet; the variant is what "a badge may carry its own colours" means.
    #[cfg_attr(not(test), allow(dead_code))]
    Custom { fill: Color, ink: Color },
}

impl Tone {
    /// The fill and the ink, resolved against the pack in force.
    fn resolve(self, pack: &Style) -> (Color, Color) {
        match self {
            Self::Surface => (
                pack.colour_or(
                    CommonColor::SurfaceRaised,
                    Color::from_rgba8(236, 230, 240, 255),
                ),
                pack.colour_or(
                    CommonColor::OnSurfaceRaised,
                    Color::from_rgba8(29, 27, 32, 255),
                ),
            ),
            Self::Error => (
                pack.colour_or(CommonColor::Error, Color::from_rgba8(179, 38, 30, 255)),
                pack.colour_or(CommonColor::OnError, Color::WHITE),
            ),
            Self::Custom { fill, ink } => (fill, ink),
        }
    }
}

/// What a chip does with the pixels behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Material {
    /// The tone's colour over a blurred backdrop: the default, so a chip never punches an
    /// opaque hole in the picture it names. Opaque where the backend cannot sample.
    Frosted,
    /// The tone's colour, flat. No built-in declares it; it is the other half of the knob.
    #[cfg_attr(not(test), allow(dead_code))]
    Opaque,
}

/// The blurred backdrop's sigma, in points. A chip is 16 points tall, so a wider blur would
/// smear across the whole pill.
const FROST: f32 = 8.0;
/// How much of the tone survives over the blurred pixels: enough for the ink to read on any
/// picture, little enough that the pill still reads as glass.
const TINT: f32 = 0.78;

/// The chip itself: a pill of the spec's height carrying the label, in the badge's tone and
/// material.
///
/// The geometry comes from the MD3 badge tokens, so this pill and any MD3 one cannot drift;
/// only the surface differs, because chrome floats over a picture rather than over a page.
pub(crate) fn pill(id: Id, badge: &Badge, pack: &Style) -> Decl {
    let spec = ui_widgets_md3::spec::badge_spec(true);
    let height = spec.root.size;
    let size = spec.label_text.size;
    // The pill pads out the difference between the label's line box and its own height.
    let inset = ((height - size * 1.4) * 0.5).max(0.0);
    let (fill, ink) = badge.tone.resolve(pack);
    let corners = Corners::all(height * 0.5);
    let mut decl = Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            // `minWidth: largeSize`: a one-character label still reads as a pill.
            width: AxisSize::hug(height, 0.0),
            height: AxisSize::fixed(height),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        // A chip is a label, not a control: it must never take the pin's hover from the
        // pointer, or resting on it would make the very chip under the cursor disappear.
        .sense(Sense::PASS_THROUGH)
        .padding(Insets {
            left: 4.0,
            top: inset,
            right: 4.0,
            bottom: inset,
        });
    decl = match badge.material {
        Material::Frosted if pack.frost_allowed(pack.device) => decl
            .background(fill.faded(TINT), corners)
            .backdrop_blur(FROST),
        // A backend that cannot sample, or a chip asked for flat: the tone, opaque.
        _ => decl.background(fill, corners),
    };
    if badge.opacity < 1.0 {
        decl = decl.opacity(badge.opacity);
    }
    decl.child(ui_widgets::text::leaf(
        id.with("label"),
        badge.label.clone(),
        size,
        ink,
        Weight(spec.label_text.weight as u16),
        f32::INFINITY,
        0.0,
        0.0,
    ))
}

/// Which way a stack grows from its anchor: toward the pin's centre, along the edge it is
/// anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Growth {
    Down,
    Up,
    Right,
    Left,
}

impl Anchor {
    fn growth(self) -> Growth {
        match self {
            Self::NorthWest | Self::North | Self::NorthEast => Growth::Down,
            Self::SouthWest | Self::South | Self::SouthEast => Growth::Up,
            Self::West => Growth::Right,
            Self::East => Growth::Left,
        }
    }

    fn mirror_x(self) -> Self {
        match self {
            Self::NorthWest => Self::NorthEast,
            Self::NorthEast => Self::NorthWest,
            Self::West => Self::East,
            Self::East => Self::West,
            Self::SouthWest => Self::SouthEast,
            Self::SouthEast => Self::SouthWest,
            Self::North | Self::South => self,
        }
    }

    fn mirror_y(self) -> Self {
        match self {
            Self::NorthWest => Self::SouthWest,
            Self::SouthWest => Self::NorthWest,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::NorthEast => Self::SouthEast,
            Self::SouthEast => Self::NorthEast,
            Self::West | Self::East => self,
        }
    }
}

/// Where the chips of one declared block go: its anchor as declared, its policy, and the
/// sizes measured last build.
pub(crate) type Block = (Anchor, Policy, Vec<(f32, f32)>);

/// Where every block's chips go, in the order the blocks were declared.
///
/// Anchors are resolved per block first: the output keeps a block on screen, and the pointer
/// keeps it off the pixels the user is pointing at. Blocks that resolved to one anchor then
/// stack as one, in declaration order — a pin larger than its output can carry two declared
/// corners to the same mirror, and two stacks at one corner would hide each other.
pub(crate) fn layout(
    blocks: &[Block],
    pin: Rect,
    output: Rect,
    pointer: Option<Point>,
) -> Vec<Vec<Rect>> {
    let mut corners: Vec<((Anchor, Fold), bool, Vec<usize>)> = Vec::new();
    for (index, (anchor, policy, sizes)) in blocks.iter().enumerate() {
        let at = match policy {
            Policy::Forced => Some((*anchor, Fold::None)),
            Policy::Preferred => resolve(*anchor, sizes, pin, output, pointer),
        };
        // Nowhere to stand that both clears the pointer and shows: the block is not drawn.
        let Some(at) = at else {
            continue;
        };
        // A block clamped into the output is one any of its chips asked to be.
        let clamp = *policy == Policy::Preferred;
        match corners.iter_mut().find(|(corner, _, _)| *corner == at) {
            Some((_, held, members)) => {
                *held |= clamp;
                members.push(index);
            }
            None => corners.push((at, clamp, vec![index])),
        }
    }
    let inner = Rect::new(
        pin.min_x() + MARGIN,
        pin.min_y() + MARGIN,
        (pin.size.w - MARGIN * 2.0).max(0.0),
        (pin.size.h - MARGIN * 2.0).max(0.0),
    );
    let mut out = vec![Vec::new(); blocks.len()];
    for ((anchor, fold), clamp, members) in corners {
        let sizes: Vec<(f32, f32)> = members
            .iter()
            .flat_map(|index| blocks[*index].2.iter().copied())
            .collect();
        let block = stack_box(anchor, pin, stack_extent(anchor, &sizes), fold);
        let block = if clamp {
            clamp_rect(block, output)
        } else {
            block
        };
        // A folded block stands outside the picture on purpose, so the output is what has to
        // hold it; one on the picture is still cut at the picture's own edge.
        let limit = if fold == Fold::None { inner } else { output };
        let mut rest = stack_rects(anchor, block, &sizes).into_iter();
        for index in members {
            // `rest` runs dry where a chip no longer fits: the block is cut there, and so
            // are the members after it.
            out[index] = rest
                .by_ref()
                .take_while(|rect| inside(*rect, limit))
                .take(blocks[index].2.len())
                .collect();
        }
    }
    out
}

/// The size of the block the chips take together.
fn stack_extent(anchor: Anchor, sizes: &[(f32, f32)]) -> Size {
    let widest = sizes.iter().map(|size| size.0).fold(0.0, f32::max);
    let tallest = sizes.iter().map(|size| size.1).fold(0.0, f32::max);
    let gaps = GAP * sizes.len().saturating_sub(1) as f32;
    match anchor.growth() {
        Growth::Down | Growth::Up => {
            Size::new(widest, sizes.iter().map(|size| size.1).sum::<f32>() + gaps)
        }
        Growth::Right | Growth::Left => {
            Size::new(sizes.iter().map(|size| size.0).sum::<f32>() + gaps, tallest)
        }
    }
}

/// The rectangle the whole block takes when it is anchored at `anchor` on `pin`, folded out
/// of the picture by `fold`.
fn stack_box(anchor: Anchor, pin: Rect, extent: Size, fold: Fold) -> Rect {
    let mut x = match anchor {
        Anchor::NorthWest | Anchor::West | Anchor::SouthWest => pin.min_x() + MARGIN,
        Anchor::North | Anchor::South => pin.min_x() + (pin.size.w - extent.w) * 0.5,
        Anchor::NorthEast | Anchor::East | Anchor::SouthEast => pin.max_x() - MARGIN - extent.w,
    };
    let mut y = match anchor {
        Anchor::NorthWest | Anchor::North | Anchor::NorthEast => pin.min_y() + MARGIN,
        Anchor::West | Anchor::East => pin.min_y() + (pin.size.h - extent.h) * 0.5,
        Anchor::SouthWest | Anchor::South | Anchor::SouthEast => pin.max_y() - MARGIN - extent.h,
    };
    // The fold is the same block reflected across the edge the anchor hangs from: the corner
    // keeps the chip, and the chip stops covering the picture the pointer is on.
    if matches!(fold, Fold::X | Fold::Both) {
        if matches!(
            anchor,
            Anchor::NorthWest | Anchor::West | Anchor::SouthWest
        ) {
            x = pin.min_x() - MARGIN - extent.w;
        } else if matches!(
            anchor,
            Anchor::NorthEast | Anchor::East | Anchor::SouthEast
        ) {
            x = pin.max_x() + MARGIN;
        }
    }
    if matches!(fold, Fold::Y | Fold::Both) {
        if matches!(
            anchor,
            Anchor::NorthWest | Anchor::North | Anchor::NorthEast
        ) {
            y = pin.min_y() - MARGIN - extent.h;
        } else if matches!(
            anchor,
            Anchor::SouthWest | Anchor::South | Anchor::SouthEast
        ) {
            y = pin.max_y() + MARGIN;
        }
    }
    Rect::new(x, y, extent.w, extent.h)
}

/// Where one block goes: the first corner, mirror or fold whose chips stay on the output
/// and clear the pointer; the declared corner pulled back into the output when nothing else
/// does; nothing at all when even that is under the pointer.
///
/// A chip under the cursor is not a chip: it takes the pin's hover, the pin's visit fades,
/// and the chip that took the hover goes with it — which reads as a blink. So the pointer
/// outranks a placement the output cannot hold, and a block left with neither is absent
/// until the pointer moves on.
fn resolve(
    anchor: Anchor,
    sizes: &[(f32, f32)],
    pin: Rect,
    output: Rect,
    pointer: Option<Point>,
) -> Option<(Anchor, Fold)> {
    let candidates = [
        (anchor, Fold::None),
        (anchor.mirror_x(), Fold::None),
        (anchor.mirror_y(), Fold::None),
        (anchor.mirror_x().mirror_y(), Fold::None),
        (anchor, Fold::X),
        (anchor, Fold::Y),
        (anchor, Fold::Both),
    ];
    let placed = |(candidate, fold): (Anchor, Fold)| {
        let block = stack_box(candidate, pin, stack_extent(candidate, sizes), fold);
        let chips = stack_rects(candidate, block, sizes);
        (block, chips)
    };
    let clear = |candidate: (Anchor, Fold)| {
        let (block, chips) = placed(candidate);
        inside(block, output)
            && pointer.is_none_or(|at| !chips.iter().any(|chip| chip.contains(at)))
    };
    if let Some(found) = candidates.iter().copied().find(|candidate| clear(*candidate)) {
        return Some(found);
    }
    // The declared corner is the block's own home, and the output carries it when the pin
    // cannot: that is only refused while the pointer is standing in the very spot it takes.
    let declared = (anchor, Fold::None);
    let block = clamp_rect(placed(declared).0, output);
    let chips = stack_rects(anchor, block, sizes);
    let taken = pointer.is_some_and(|at| chips.iter().any(|chip| chip.contains(at)));
    (!taken).then_some(declared)
}

/// The chips of one block, the first at the anchor and each later one `GAP` past its
/// predecessor.
fn stack_rects(anchor: Anchor, block: Rect, sizes: &[(f32, f32)]) -> Vec<Rect> {
    let growth = anchor.growth();
    let mut cursor = match growth {
        Growth::Down => block.min_y(),
        Growth::Up => block.max_y(),
        Growth::Right => block.min_x(),
        Growth::Left => block.max_x(),
    };
    let mut rects = Vec::with_capacity(sizes.len());
    for &(width, height) in sizes {
        let rect = match growth {
            Growth::Down => {
                let rect = Rect::new(aligned_x(anchor, block, width), cursor, width, height);
                cursor += height + GAP;
                rect
            }
            Growth::Up => {
                cursor -= height;
                let rect = Rect::new(aligned_x(anchor, block, width), cursor, width, height);
                cursor -= GAP;
                rect
            }
            Growth::Right => {
                let rect = Rect::new(cursor, centred_y(block, height), width, height);
                cursor += width + GAP;
                rect
            }
            Growth::Left => {
                cursor -= width;
                let rect = Rect::new(cursor, centred_y(block, height), width, height);
                cursor -= GAP;
                rect
            }
        };
        rects.push(rect);
    }
    rects
}

fn aligned_x(anchor: Anchor, block: Rect, width: f32) -> f32 {
    match anchor {
        Anchor::NorthWest | Anchor::West | Anchor::SouthWest => block.min_x(),
        Anchor::North | Anchor::South => block.min_x() + (block.size.w - width) * 0.5,
        Anchor::NorthEast | Anchor::East | Anchor::SouthEast => block.max_x() - width,
    }
}

fn centred_y(block: Rect, height: f32) -> f32 {
    block.min_y() + (block.size.h - height) * 0.5
}

fn inside(rect: Rect, bounds: Rect) -> bool {
    rect.min_x() >= bounds.min_x() - 0.01
        && rect.min_y() >= bounds.min_y() - 0.01
        && rect.max_x() <= bounds.max_x() + 0.01
        && rect.max_y() <= bounds.max_y() + 0.01
}

fn clamp_rect(rect: Rect, bounds: Rect) -> Rect {
    let x = rect.min_x().clamp(
        bounds.min_x(),
        (bounds.max_x() - rect.size.w).max(bounds.min_x()),
    );
    let y = rect.min_y().clamp(
        bounds.min_y(),
        (bounds.max_y() - rect.size.h).max(bounds.min_y()),
    );
    Rect::new(x, y, rect.size.w, rect.size.h)
}

/// `text` capped at `limit` characters, with a middle ellipsis: a quarter of the head and
/// the rest of the tail, because a path's tail is the part a user reads.
pub(crate) fn abbreviate(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit || limit == 0 {
        return text.to_string();
    }
    let head = (limit - 1) / 4;
    let tail = limit - 1 - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

/// A rough size for a pill no build has measured yet.
///
/// The real size comes from `rect_of` once the pill has been laid out; this only places the
/// frame it first appears in, so a guess within a few points is enough.
pub(crate) fn rough_size(label: &str) -> (f32, f32) {
    let spec = ui_widgets_md3::spec::badge_spec(true);
    let height = spec.root.size;
    // 4dp of padding each side, and an average advance a little over half the type size.
    let text = label.chars().count() as f32 * spec.label_text.size * 0.6;
    ((text + 8.0).max(height), height)
}

/// The zoom badge: how much larger than native the pin is drawn, while it is not native.
///
/// Whole percent: the wheel's 1.1 step does not need a fraction, and a chip is read at a
/// glance. `opacity` is the visit's fade, and a chip that has faded out is not declared at
/// all: a scale of 1 says nothing, and neither does a chip nobody can see.
pub(crate) fn zoom(zoom: f32, opacity: f32) -> Option<Badge> {
    if zoom == 1.0 || opacity <= 0.01 {
        return None;
    }
    let mut badge = Badge::new(
        "zoom",
        format!("{:.0}%", zoom * 100.0),
        Anchor::SouthEast,
        Policy::Preferred,
        Show::Always,
    );
    badge.opacity = opacity;
    Some(badge)
}

/// A wheel step's landing: no travel, the chip is simply up.
const VISIT_LAND: AnimSpec = AnimSpec::new(0.0, Easing::Linear);
/// After the last step: hold the chip still for a moment, then fade it. The downward leg
/// states its own curve, or the fade would inherit the landing's.
const VISIT_DEPART: AnimSpec = AnimSpec::new(0.3, Easing::EaseIn)
    .delayed(1.1)
    .down(0.3, Easing::EaseIn);

/// The zoom chip's opacity this build: a wheel step lands it at full, and the departure
/// holds it and lets it go. A ramp never plays — the next build's departure retargets it.
pub(crate) fn visited(cx: &mut Cx<'_>, id: Id, stepped: bool) -> f32 {
    if stepped {
        cx.animate(id, 1.0, VISIT_LAND);
    }
    cx.animate(id, 0.0, VISIT_DEPART)
}

/// The badges that say where the material came from.
///
/// The short form and the full one swap rather than stack: a file's path ends in its own
/// name, and one chip at a corner is less to read than two. A capture the user just took
/// needs no badge saying so, and a colour pin already shows its code in the card.
pub(crate) fn origin(origin: &PinOrigin, catalog: &Catalog) -> Vec<Badge> {
    let anchor = Anchor::SouthWest;
    let badge = |id, label: String, show| Badge::new(id, label, anchor, Policy::Preferred, show);
    match origin {
        PinOrigin::File { path } => {
            let name = Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            vec![
                badge("origin", name, Show::NormalOnly),
                badge("origin-detail", path.clone(), Show::AdvancedOnly),
            ]
        }
        PinOrigin::Selection { mime } => vec![
            badge(
                "origin",
                catalog.text_by_id("pin-badge-clipboard").to_string(),
                Show::NormalOnly,
            ),
            badge("origin-detail", mime.clone(), Show::AdvancedOnly),
        ],
        PinOrigin::Capture {
            output: Some(output),
        } => vec![badge("origin-detail", output.clone(), Show::AdvancedOnly)],
        PinOrigin::Capture { output: None } | PinOrigin::Colour { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where one declared block's chips go, with nothing under the pointer.
    fn place(
        anchor: Anchor,
        policy: Policy,
        sizes: &[(f32, f32)],
        pin: Rect,
        output: Rect,
    ) -> Vec<Rect> {
        layout(&[(anchor, policy, sizes.to_vec())], pin, output, None)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// The declared anchor wins while it fits, a badge on a pin that left the output flips
    /// to the mirror that fits, and a forced one stays where the pin carried it.
    #[test]
    fn a_badge_prefers_the_anchor_that_stays_on_the_output() {
        let output = Rect::new(0.0, 0.0, 500.0, 500.0);
        let sizes = [(40.0, 16.0)];
        let chip = |rects: Vec<Rect>| rects.first().copied().expect("a chip");

        let inside_pin = Rect::new(100.0, 100.0, 200.0, 100.0);
        assert_eq!(
            chip(place(
                Anchor::NorthWest,
                Policy::Preferred,
                &sizes,
                inside_pin,
                output
            )),
            Rect::new(106.0, 106.0, 40.0, 16.0),
            "a pin wholly on the output keeps its anchor"
        );

        let off_left = Rect::new(-100.0, 100.0, 200.0, 100.0);
        assert_eq!(
            chip(place(
                Anchor::NorthWest,
                Policy::Preferred,
                &sizes,
                off_left,
                output
            )),
            Rect::new(54.0, 106.0, 40.0, 16.0),
            "the mirrored corner is the one that shows"
        );
        assert_eq!(
            chip(place(
                Anchor::NorthWest,
                Policy::Forced,
                &sizes,
                off_left,
                output
            )),
            Rect::new(-94.0, 106.0, 40.0, 16.0),
            "a forced badge stays with the pin"
        );

        // A pin larger than the output whose every corner is outside it: no mirror helps,
        // so the block comes back to the output's own corner.
        let covering = Rect::new(-1000.0, -1000.0, 2000.0, 2000.0);
        assert_eq!(
            chip(place(
                Anchor::NorthWest,
                Policy::Preferred,
                &sizes,
                covering,
                output
            )),
            Rect::new(0.0, 0.0, 40.0, 16.0),
            "nothing fits, so the block clamps to the output"
        );
    }

    /// Chips of one anchor stack toward the pin's centre, and a chip that no longer fits
    /// the pin is dropped with the ones after it.
    #[test]
    fn badges_stack_toward_the_pin_centre_and_stop_at_its_edge() {
        let output = Rect::new(0.0, 0.0, 800.0, 600.0);
        let pin = Rect::new(100.0, 100.0, 300.0, 200.0);
        let sizes = [(60.0, 16.0), (40.0, 16.0)];

        let down = place(Anchor::NorthWest, Policy::Preferred, &sizes, pin, output);
        assert_eq!(
            down,
            vec![
                Rect::new(106.0, 106.0, 60.0, 16.0),
                Rect::new(106.0, 126.0, 40.0, 16.0),
            ],
            "NorthWest stacks down"
        );

        let up = place(Anchor::SouthEast, Policy::Preferred, &sizes, pin, output);
        assert_eq!(
            up,
            vec![
                Rect::new(334.0, 278.0, 60.0, 16.0),
                Rect::new(354.0, 258.0, 40.0, 16.0),
            ],
            "SouthEast puts the first chip at the corner and the next one above it"
        );

        let right = place(Anchor::West, Policy::Preferred, &sizes, pin, output);
        assert_eq!(
            right,
            vec![
                Rect::new(106.0, 192.0, 60.0, 16.0),
                Rect::new(170.0, 192.0, 40.0, 16.0),
            ],
            "West stacks sideways, the first chip nearest the edge"
        );

        // A pin too short for the second chip: the stack stops at the first.
        let short = Rect::new(100.0, 100.0, 300.0, 30.0);
        let kept = place(Anchor::NorthWest, Policy::Preferred, &sizes, short, output);
        assert_eq!(
            kept.len(),
            1,
            "a chip with no place is dropped, and so are the ones after it"
        );
    }

    /// A long name keeps its head and its tail: a path's tail names the file.
    #[test]
    fn a_long_label_keeps_its_ends() {
        let path = "/home/ollie/Pictures/Screenshots/202611/Screenshot_2026.11.06_16-44-25.avif";
        let short = abbreviate(path, 48);
        assert_eq!(short.chars().count(), 48);
        assert!(short.starts_with("/home/"), "{short}");
        assert!(short.ends_with("16-44-25.avif"), "{short}");
        assert!(short.contains('…'), "{short}");
        assert_eq!(abbreviate("short.png", 48), "short.png");
        assert_eq!(
            abbreviate("anything", 0),
            "anything",
            "a zero cap is no cap"
        );
    }

    /// Native zoom says nothing; anything else reads as whole percent, and a chip whose
    /// visit has faded out is not declared at all.
    #[test]
    fn the_zoom_badge_appears_only_away_from_native() {
        assert!(zoom(1.0, 1.0).is_none());
        assert_eq!(zoom(1.1, 1.0).expect("a badge").label, "110%");
        assert_eq!(zoom(0.077, 1.0).expect("a badge").label, "8%");
        assert_eq!(zoom(1.5, 0.5).expect("a badge").opacity, 0.5);
        assert!(zoom(1.5, 0.0).is_none(), "a chip nobody can see is no chip");
    }

    /// Two declared corners that mirror onto one share a stack: the later block's chips
    /// follow the earlier ones instead of hiding under them.
    #[test]
    fn blocks_that_mirror_onto_one_corner_form_one_stack() {
        // A pin wider and taller than its output: its west and south corners are off the
        // output, and every declared anchor but the north-west mirrors there.
        let output = Rect::new(0.0, 0.0, 500.0, 500.0);
        let pin = Rect::new(100.0, 100.0, 800.0, 800.0);
        let blocks: Vec<Block> = vec![
            (Anchor::NorthWest, Policy::Preferred, vec![(40.0, 16.0)]),
            (Anchor::SouthWest, Policy::Preferred, vec![(30.0, 16.0)]),
            (Anchor::SouthEast, Policy::Preferred, vec![(50.0, 16.0)]),
        ];
        let laid = layout(&blocks, pin, output, None);
        assert_eq!(laid[0], vec![Rect::new(106.0, 106.0, 40.0, 16.0)]);
        assert_eq!(
            laid[1],
            vec![Rect::new(106.0, 126.0, 30.0, 16.0)],
            "the south-west block mirrors up and stacks under the first"
        );
        assert_eq!(
            laid[2],
            vec![Rect::new(106.0, 146.0, 50.0, 16.0)],
            "and the south-east block after that"
        );
    }

    /// A chip under the pointer steps to the mirror that clears it; a forced one stays, and
    /// keeping to the output still wins when both cannot be had.
    #[test]
    fn a_chip_steps_off_the_pointer() {
        let output = Rect::new(0.0, 0.0, 500.0, 500.0);
        let pin = Rect::new(100.0, 100.0, 200.0, 100.0);
        let sizes = [(40.0, 16.0)];
        let at = |pointer| {
            layout(
                &[(Anchor::NorthWest, Policy::Preferred, sizes.to_vec())],
                pin,
                output,
                pointer,
            )
        };
        assert_eq!(
            at(Some(Point::new(120.0, 112.0)))[0],
            vec![Rect::new(254.0, 106.0, 40.0, 16.0)],
            "the pointer over the north-west chip sends it to the north-east"
        );
        assert_eq!(
            at(Some(Point::new(200.0, 150.0)))[0],
            vec![Rect::new(106.0, 106.0, 40.0, 16.0)],
            "a pointer elsewhere changes nothing"
        );
        let forced = layout(
            &[(Anchor::NorthWest, Policy::Forced, sizes.to_vec())],
            pin,
            output,
            Some(Point::new(120.0, 112.0)),
        );
        assert_eq!(forced[0], vec![Rect::new(106.0, 106.0, 40.0, 16.0)]);
    }

    /// A pin of which only the declared corner is on the output: no mirror can dodge the
    /// pointer, so the chip folds across the pin's own edge; with no room to fold either it
    /// is not drawn, rather than standing where the pointer already is.
    #[test]
    fn a_chip_with_no_mirror_left_folds_across_its_corner_or_is_not_drawn() {
        let output = Rect::new(0.0, 0.0, 500.0, 500.0);
        let sizes = [(40.0, 16.0)];
        let at = |pin, pointer| {
            layout(
                &[(Anchor::NorthWest, Policy::Preferred, sizes.to_vec())],
                pin,
                output,
                Some(pointer),
            )
        };
        let room = Rect::new(100.0, 100.0, 800.0, 800.0);
        assert_eq!(
            at(room, Point::new(120.0, 112.0))[0],
            vec![Rect::new(54.0, 106.0, 40.0, 16.0)],
            "the chip reflects past the pin's left edge, which is the corner it belongs to"
        );
        let cornered = Rect::new(10.0, 10.0, 800.0, 800.0);
        assert!(
            at(cornered, Point::new(20.0, 20.0))[0].is_empty(),
            "nowhere clears the pointer, so the chip is absent until it moves on"
        );
    }

    /// Frost is the default: a tinted blur of what the chip covers. A chip that asks for
    /// flat paint draws the tone outright, and a custom tone carries its own ink.
    #[test]
    fn a_chip_is_tinted_glass_unless_it_asks_for_flat_paint() {
        use ui_core::NodeData;
        use ui_core::draw::Shape;

        let pack = ui_widgets_md3::pack();
        let id = Id::str("chip");
        let plain = Badge::new(
            "z",
            "110%",
            Anchor::SouthEast,
            Policy::Preferred,
            Show::Always,
        );
        let glass = pill(id, &plain, pack);
        let NodeData::Container(params) = &glass.data else {
            panic!("a pill is a container");
        };
        assert!(
            params.backdrop_blur.is_some(),
            "the default samples behind it"
        );

        let flat = pill(id, &plain.clone().material(Material::Opaque), pack);
        let NodeData::Container(params) = &flat.data else {
            panic!("a pill is a container");
        };
        assert!(
            params.backdrop_blur.is_none(),
            "a flat chip samples nothing"
        );

        let fill_of = |decl: &Decl| match &decl.shapes[0] {
            Shape::Box { fill, .. } => *fill,
            other => panic!("a pill paints one box, not {other:?}"),
        };
        let (flat_fill, glass_fill) = (fill_of(&flat), fill_of(&glass));
        assert_eq!(
            flat_fill,
            pack.colour_or(CommonColor::SurfaceRaised, Color::WHITE),
            "flat paint is the tone itself"
        );
        assert_eq!(
            (glass_fill.r, glass_fill.g, glass_fill.b),
            (flat_fill.r, flat_fill.g, flat_fill.b),
            "glass keeps the tone's hue"
        );
        assert!(
            glass_fill.a > 0.5 && glass_fill.a < flat_fill.a,
            "glass lets the picture through, and still backs the ink: {}",
            glass_fill.a
        );

        let custom = plain.tone(Tone::Custom {
            fill: Color::from_rgba8(0, 0, 255, 255),
            ink: Color::from_rgba8(255, 255, 0, 255),
        });
        let decl = pill(id, &custom, pack);
        let NodeData::Text { ink, .. } = &decl.children[0].data else {
            panic!("the pill carries a label");
        };
        assert_eq!(*ink, Color::from_rgba8(255, 255, 0, 255));
    }

    /// A file pin names the file and keeps the path behind the reveal key; a selection names
    /// itself and its MIME; a capture states only its output name; a colour says nothing.
    #[test]
    fn an_origin_names_the_file_it_came_from() {
        let language =
            crate::load_language(ui_i18n::LanguageChoice::Fixed("en".parse().expect("en")))
                .expect("the app's English catalog");
        let catalog = language.catalog();
        let file = origin(
            &PinOrigin::File {
                path: "/home/ollie/Pictures/deep/a-picture.png".into(),
            },
            &catalog,
        );
        assert_eq!(file[0].label, "a-picture.png");
        assert_eq!(file[0].show, Show::NormalOnly);
        assert_eq!(file[1].label, "/home/ollie/Pictures/deep/a-picture.png");
        assert_eq!(file[1].show, Show::AdvancedOnly);

        let selection = origin(
            &PinOrigin::Selection {
                mime: "image/png".into(),
            },
            &catalog,
        );
        assert_eq!(selection[0].label, "Clipboard");
        assert_eq!(selection[1].label, "image/png");

        let capture = origin(
            &PinOrigin::Capture {
                output: Some("DP-6".into()),
            },
            &catalog,
        );
        assert_eq!(capture.len(), 1);
        assert_eq!(capture[0].show, Show::AdvancedOnly);
        assert!(origin(&PinOrigin::Capture { output: None }, &catalog).is_empty());
        assert!(
            origin(
                &PinOrigin::Colour {
                    code: "#ff0000".into()
                },
                &catalog
            )
            .is_empty()
        );
    }
}
