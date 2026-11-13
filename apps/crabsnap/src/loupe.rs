//! The loupe: a disc that magnifies the picture under the pointer.
//!
//! The disc samples the picture and its marks, not the surface they are drawn on: the
//! caller rasters that one window, `bake` turns it into a straight-alpha texture, and
//! everything else here is placement and the chrome drawn over it.

use std::sync::Arc;

use ui_core::colour::{ContentColour, Destination, Placement};
use ui_core::cx::Cx;
use ui_core::draw::{ImageEntry, ImageSource, Sampling, Shape};
use ui_core::geom::{Color, Corners, Point, Rect, Size};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ImageFit, ShadowSpec};
use ui_core::{Decl, Id, NodeKind, Sense};

use crate::capture::Cropped;
#[cfg(test)]
use crate::capture::Image;
use crate::pill;
use crate::zoom;

/// The disc's side, in surface points.
pub(crate) const DISC: f32 = 120.0;
/// The zoom range. Two is the least a loupe is worth; sixteen shows a few pixels.
pub(crate) const MIN_ZOOM: u32 = 2;
pub(crate) const MAX_ZOOM: u32 = 16;
pub(crate) const DEFAULT_ZOOM: u32 = 8;
/// Where the disc's centre sits relative to the pointer, as a multiple of its radius.
const OFFSET: f32 = 1.1;
/// The zoom plate keeps no vertical padding of its own: the line box around the digits is
/// air enough on a plate this short, and every point of it cuts into the rim.
const PLATE_PAD: f32 = 0.0;
/// The white line on the disc's edge, in points.
const RIM_HAIRLINE: f32 = 1.5;
/// The thin shadow the disc and the plate each cast: what separates them from the picture.
const SHADOW: ShadowSpec = ShadowSpec::black(4.0, 0.35, 1.0);
/// The texture the disc is registered under, per surface.
const TEXTURE: u64 = 0x6c6f_7570_6500_0001;

/// The guide's dark edge, and the ink the plate and the readings avoid.
const RIM: Color = Color::rgb(20.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0);
/// The guide's light edge, and the plate's white body.
const RIM_LIGHT: Color = Color::WHITE;

/// The loupe's state: whether the button holds it up, whether it was switched on, and how far
/// it zooms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Loupe {
    /// The secondary button is down over the surface: a visit, not a setting.
    held: bool,
    /// The toolbar's button switched the disc on: it stays up until switched off.
    toggled: bool,
    zoom: u32,
    /// The wheel's remainder, banked until it is worth a whole zoom step.
    carry: f32,
}

impl Loupe {
    pub(crate) fn new() -> Self {
        Self {
            held: false,
            toggled: false,
            zoom: DEFAULT_ZOOM,
            carry: 0.0,
        }
    }

    pub(crate) fn held(&self) -> bool {
        self.held
    }

    pub(crate) fn set_held(&mut self, held: bool) {
        self.held = held;
    }

    /// Whether the toolbar's button has the disc switched on.
    pub(crate) fn toggled(&self) -> bool {
        self.toggled
    }

    /// Flip the button's switch and report the new state.
    pub(crate) fn toggle(&mut self) -> bool {
        self.toggled = !self.toggled;
        self.toggled
    }

    pub(crate) fn zoom(&self) -> u32 {
        self.zoom
    }

    /// Whether the disc is up: switched on by the button, held by the secondary button, or
    /// called for by a drag in progress.
    pub(crate) fn visible(&self, dragging: bool) -> bool {
        self.toggled || self.held || dragging
    }

    /// One wheel delta, in engine pixels. `false` when the disc is not up to be zoomed.
    pub(crate) fn wheel(&mut self, value: f64, visible: bool) -> bool {
        if !visible {
            return false;
        }
        let owed = self.carry + zoom::notches(value);
        let step = owed.trunc() as i32;
        self.carry = owed - step as f32;
        let zoom = (self.zoom as i32 + step).clamp(MIN_ZOOM as i32, MAX_ZOOM as i32) as u32;
        if zoom == self.zoom {
            return false;
        }
        self.zoom = zoom;
        true
    }
}

/// The disc's texel side at a surface's device ratio.
fn texels(dots: f32) -> u32 {
    (DISC * dots).round().max(1.0) as u32
}

/// The cells the window holds on either side of the aimed one: what the disc's width leaves
/// beside the aimed block, rounded up so the rim never asks for a cell not fetched.
fn cells_beside(side: u32, zoom: u32) -> u32 {
    side.saturating_sub(zoom).div_ceil(2 * zoom)
}

/// The window the disc reads around `aim`, in picture pixels, at `dots` device pixels per
/// point: what a caller must rasterize to give the disc the marks as well as the picture.
pub(crate) fn window(dots: f32, aim: Aim, zoom: u32) -> Rect {
    let beside = cells_beside(texels(dots), zoom);
    Rect::new(
        (aim.pixel.0 - beside as i32) as f32,
        (aim.pixel.1 - beside as i32) as f32,
        (2 * beside + 1) as f32,
        (2 * beside + 1) as f32,
    )
}

/// The cell a disc texel shows, relative to the aimed pixel. The aimed cell's middle sits on
/// the disc's centre, so a texel `du` texels from there belongs to `floor((du + zoom/2) /
/// zoom)` — whole blocks of `zoom` texels, whatever the window holds.
fn cell_at(side: u32, zoom: u32, texel: u32) -> i32 {
    let du = (texel as f32 + 0.5) - side as f32 * 0.5;
    ((du + zoom as f32 * 0.5) / zoom as f32).floor() as i32
}

/// The pixel being magnified.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Aim {
    /// The image pixel under the pointer.
    pub(crate) pixel: (i32, i32),
}

impl Aim {
    /// The pixel a pointer at `at` (in the picture's own pixels) lands in.
    pub(crate) fn at(at: Point) -> Self {
        Self {
            pixel: (at.x.floor() as i32, at.y.floor() as i32),
        }
    }
}

/// The picture the disc reads: pixels in their own packing, what they mean, and the picture
/// pixel its first one is.
pub(crate) struct Source<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
    colour: ContentColour,
    origin: (i32, i32),
}

impl<'a> Source<'a> {
    /// A window of the picture, `origin` being the picture pixel its first one is: what
    /// carries the marks into the disc.
    pub(crate) fn of_window(image: &'a Cropped, origin: (i32, i32)) -> Self {
        Self {
            width: image.width,
            height: image.height,
            data: image.data.as_slice(),
            colour: image.colour,
            origin,
        }
    }

    /// One pixel as straight sRGB8, with the edge repeated past the picture.
    fn texel(&self, placement: &Placement, x: i32, y: i32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 255];
        }
        let x = (x - self.origin.0).clamp(0, self.width as i32 - 1) as usize;
        let y = (y - self.origin.1).clamp(0, self.height as i32 - 1) as usize;
        let texel = self.colour.storage.bytes_per_texel();
        let index = (y * self.width as usize + x) * texel;
        let Some(bytes) = self.data.get(index..index + texel) else {
            return [0, 0, 0, 255];
        };
        let [r, g, b, a] = placement.texel(bytes);
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [to8(r), to8(g), to8(b), to8(a)]
    }
}

/// One baked disc: the texture, and where its window sat in the picture.
pub(crate) struct Disc {
    pub(crate) pixels: Vec<u8>,
    pub(crate) side: u32,
    /// Whole texels per source pixel.
    pub(crate) zoom: u32,
    /// The window's top-left in image pixels, and its width in cells.
    origin: (i32, i32),
    span: i32,
    /// The aimed pixel's colour, which the readout pill states.
    pub(crate) aim: [u8; 4],
}

/// Turn the window around `aim` into the disc's texture, `side` texels across.
///
/// Every source pixel becomes a block of `zoom` texels, so the picture's own grid is what the
/// disc shows. The rim is coverage, not a clip.
pub(crate) fn bake(source: &Source<'_>, aim: Aim, zoom: u32, side: u32) -> Disc {
    let side = side.max(1);
    let zoom = zoom.max(1);
    let placement = source.colour.placement(Destination::SDR);
    // An odd window keeps the aimed cell in the middle; its width follows the disc rather
    // than a division of it, so every zoom draws whole blocks.
    let beside = cells_beside(side, zoom);
    let span = (2 * beside + 1) as i32;
    let (x0, y0) = (aim.pixel.0 - beside as i32, aim.pixel.1 - beside as i32);
    let mut cells = vec![0u8; (span * span * 4) as usize];
    for y in 0..span {
        for x in 0..span {
            let texel = source.texel(&placement, x0 + x, y0 + y);
            let at = ((y * span + x) * 4) as usize;
            cells[at..at + 4].copy_from_slice(&texel);
        }
    }
    let centre = span / 2;
    let aimed = {
        let at = ((centre * span + centre) * 4) as usize;
        [cells[at], cells[at + 1], cells[at + 2], cells[at + 3]]
    };
    let radius = side as f32 * 0.5;
    let middle = radius;
    let last = beside as i32;
    let mut pixels = vec![0u8; (side * side * 4) as usize];
    for j in 0..side {
        for i in 0..side {
            let du = (i as f32 + 0.5) - middle;
            let dv = (j as f32 + 0.5) - middle;
            let coverage = (radius - (du * du + dv * dv).sqrt() + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let x = cell_at(side, zoom, i).clamp(-last, last) + last;
            let y = cell_at(side, zoom, j).clamp(-last, last) + last;
            let from = ((y * span + x) * 4) as usize;
            let to = ((j * side + i) * 4) as usize;
            pixels[to] = cells[from];
            pixels[to + 1] = cells[from + 1];
            pixels[to + 2] = cells[from + 2];
            pixels[to + 3] = (cells[from + 3] as f32 * coverage).round() as u8;
        }
    }
    Disc {
        pixels,
        side,
        zoom,
        origin: (x0, y0),
        span,
        aim: aimed,
    }
}

/// The disc's rectangle: at the pointer's bottom-right, flipped per axis at an edge.
pub(crate) fn place_disc(pointer: Point, surface: Rect) -> Rect {
    let radius = DISC * 0.5;
    let mut centre = Point::new(pointer.x + radius * OFFSET, pointer.y + radius * OFFSET);
    if centre.x + radius > surface.max_x() {
        centre.x = pointer.x - radius * OFFSET;
    }
    if centre.y + radius > surface.max_y() {
        centre.y = pointer.y - radius * OFFSET;
    }
    let x = (centre.x - radius).clamp(
        surface.min_x(),
        (surface.max_x() - DISC).max(surface.min_x()),
    );
    let y = (centre.y - radius).clamp(
        surface.min_y(),
        (surface.max_y() - DISC).max(surface.min_y()),
    );
    Rect::new(x, y, DISC, DISC)
}

/// Where the stacked readings go: under the loupe, or above it when the space below is
/// taken by the surface's edge or by the action bar floating over the overlay.
///
/// `extent` is the loupe's own bounds — the disc plus whatever rides its top — so that a
/// reading placed above clears the zoom plate instead of covering it.
pub(crate) fn place_readout(size: Size, extent: Rect, surface: Rect, avoid: Option<Rect>) -> Point {
    let x = (extent.center().x - size.w * 0.5).clamp(
        surface.min_x(),
        (surface.max_x() - size.w).max(surface.min_x()),
    );
    let below = extent.max_y() + pill::GAP;
    let above = extent.min_y() - pill::GAP - size.h;
    let room = |y: f32| {
        y >= surface.min_y()
            && y + size.h <= surface.max_y()
            && avoid.is_none_or(|bar| !Rect::new(x, y, size.w, size.h).intersects(bar))
    };
    let y = if room(below) || !room(above) {
        below
    } else {
        above
    };
    let y = y.clamp(
        surface.min_y(),
        (surface.max_y() - size.h).max(surface.min_y()),
    );
    Point::new(x, y)
}

/// Where the zoom plate goes: on the rim's top, centred on the disc, and pushed onto the
/// disc rather than off the surface when the disc touches a screen edge.
pub(crate) fn place_plate(size: Size, disc: Rect, surface: Rect) -> Point {
    let x = (disc.center().x - size.w * 0.5).clamp(
        surface.min_x(),
        (surface.max_x() - size.w).max(surface.min_x()),
    );
    // The rim under the plate is an arc: over the plate's half-width it falls a sagitta
    // below its apex, so a plate centred on the apex reads as lifted off the lens. Dropping
    // it by that sagitta puts the rim's entry points at the plate's own middle.
    let radius = DISC * 0.5;
    let half = size.w * 0.5;
    let sagitta = radius - (radius * radius - half * half).max(0.0).sqrt();
    let y = (disc.min_y() + sagitta - size.h * 0.5).max(surface.min_y());
    Point::new(x, y)
}

/// Black or white, whichever reads better on `fill`: WCAG relative luminance decides.
fn ink_on(fill: Color) -> Color {
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(fill.r) + 0.7152 * linear(fill.g) + 0.0722 * linear(fill.b);
    // Contrast is (L + 0.05) / 0.05 against black and 1.05 / (L + 0.05) against white;
    // the two meet at L = 0.179.
    if luminance > 0.179 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// The part of `a`→`b` inside the circle, or `None` when none of it is.
///
/// The paint list clips to axis-aligned rectangles only, and the disc is static: solving the
/// intersection here is cheaper than a path clip in both backends.
fn clip_segment(a: Point, b: Point, centre: Point, radius: f32) -> Option<(Point, Point)> {
    let d = Point::new(b.x - a.x, b.y - a.y);
    let f = Point::new(a.x - centre.x, a.y - centre.y);
    let qa = d.x * d.x + d.y * d.y;
    let qb = 2.0 * (f.x * d.x + f.y * d.y);
    let qc = f.x * f.x + f.y * f.y - radius * radius;
    if qa <= 1e-12 {
        return (qc <= 0.0).then_some((a, a));
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (t0, t1) = ((-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa));
    let (lo, hi) = (t0.max(0.0), t1.min(1.0));
    if lo > hi {
        return None;
    }
    Some((
        Point::new(a.x + d.x * lo, a.y + d.y * lo),
        Point::new(a.x + d.x * hi, a.y + d.y * hi),
    ))
}

/// Where a picture point draws on the disc, in disc points.
///
/// `at` is a continuous image coordinate: one pixel is `zoom` texels and the aimed pixel's
/// middle is the disc's centre, which is the rule the sampling follows, so the guide's edges
/// land on the boundaries the disc draws.
fn on_disc(disc: &Disc, at: Point) -> Point {
    let cells = (disc.span / 2) as f32;
    let per_pixel = DISC * disc.zoom as f32 / disc.side as f32;
    Point::new(
        DISC * 0.5 + (at.x - disc.origin.0 as f32 - cells - 0.5) * per_pixel,
        DISC * 0.5 + (at.y - disc.origin.1 as f32 - cells - 0.5) * per_pixel,
    )
}

/// The disc's own chrome over its texture: its white edge, the crosshair, and the guide
/// rectangle's edges.
fn chrome(disc: &Disc, guide: Option<Rect>) -> Vec<Shape> {
    let rect = Rect::new(0.0, 0.0, DISC, DISC);
    let centre = rect.center();
    let arm = DISC / 2.5;
    // The sampling puts the aimed cell's middle on the disc's centre, so the crosshair
    // belongs there and never moves.
    let aim = centre;
    let mut shapes = vec![
        Shape::Ellipse {
            rect,
            thickness: RIM_HAIRLINE,
            fill: Color::TRANSPARENT,
            colour: RIM_LIGHT,
        },
        Shape::Line {
            from: Point::new(aim.x - arm, aim.y),
            to: Point::new(aim.x + arm, aim.y),
            thickness: 1.0,
            colour: Color::WHITE,
        },
        Shape::Line {
            from: Point::new(aim.x, aim.y - arm),
            to: Point::new(aim.x, aim.y + arm),
            thickness: 1.0,
            colour: Color::WHITE,
        },
    ];
    let Some(guide) = guide else {
        return shapes;
    };
    let map = |x: f32, y: f32| on_disc(disc, Point::new(x, y));
    let corners = [
        map(guide.min_x(), guide.min_y()),
        map(guide.max_x(), guide.min_y()),
        map(guide.max_x(), guide.max_y()),
        map(guide.min_x(), guide.max_y()),
    ];
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let Some((a, b)) = clip_segment(a, b, centre, DISC * 0.5) else {
            continue;
        };
        for (thickness, colour) in [(2.4, RIM), (1.2, RIM_LIGHT)] {
            shapes.push(Shape::Line {
                from: a,
                to: b,
                thickness,
                colour,
            });
        }
    }
    shapes
}

/// One frame's loupe: what to read, where the pointer and the surface are, and where the
/// parent node sits.
pub(crate) struct View<'a> {
    pub(crate) state: &'a Loupe,
    pub(crate) source: &'a Source<'a>,
    pub(crate) aim: Aim,
    /// The rectangle drawn inside the disc, in the picture's own pixels.
    pub(crate) guide: Option<Rect>,
    /// The pointer in surface points.
    pub(crate) pointer: Point,
    /// The surface's own bounds.
    pub(crate) surface: Rect,
    /// The parent node's surface position: every offset is expressed against it.
    pub(crate) origin: Point,
    pub(crate) id: Id,
}

/// Build the loupe for this frame, or `None` where it cannot be shown.
///
/// The disc is registered as a texture before the frame is painted, so the frame that baked
/// it is also the frame that shows it.
pub(crate) fn build(cx: &mut Cx<'_>, view: View<'_>) -> Option<Decl> {
    let View {
        state,
        source,
        aim,
        guide,
        pointer,
        surface,
        origin,
        id,
    } = view;
    if source.width == 0 || source.height == 0 {
        return None;
    }
    let side = texels(cx.scale());
    let zoom = state.zoom();
    let disc = bake(source, aim, zoom, side);
    let rect = place_disc(pointer, surface);
    let label = format!("{zoom}×");
    let coords = format!("{}, {}", aim.pixel.0, aim.pixel.1);
    let hex = format!("#{:02X}{:02X}{:02X}", disc.aim[0], disc.aim[1], disc.aim[2]);
    let plate_size = pill::measure_one(cx, &label, PLATE_PAD)?;
    let readout_size = pill::measure_column(cx, &[coords.clone(), hex.clone()])?;
    let plate_at = place_plate(plate_size, rect, surface);
    let top = plate_at.y.min(rect.min_y());
    let extent = Rect::new(rect.min_x(), top, rect.size.w, rect.max_y() - top);
    let readout_at = place_readout(readout_size, extent, surface, cx.rect_of(crate::toolbar::id()));
    cx.register_image(ImageEntry {
        id: TEXTURE,
        width: disc.side,
        height: disc.side,
        source: ImageSource::Pixels(Arc::new(disc.pixels.clone())),
        colour: ContentColour::SDR,
        sampling: Sampling::Pixelated,
    });

    // The shadow follows the box's corners, so the disc's box is rounded to a circle: its
    // shadow is then the circle's own, at the same radius the texture fades at.
    let mut picture = Decl::container(id.with("disc"), NodeKind::Box)
        .size(SizeClass::fixed(DISC, DISC))
        .offset(rect.origin.x - origin.x, rect.origin.y - origin.y)
        .sense(Sense::PASS_THROUGH)
        .background(Color::TRANSPARENT, Corners::all(DISC * 0.5))
        .shadow(SHADOW)
        .background_image(TEXTURE, ImageFit::Fill)
        .image_sampling(Sampling::Pixelated);
    picture.shapes.extend(chrome(&disc, guide));

    let mut root = Decl::container(id, NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::fixed(0.0),
            height: AxisSize::fixed(0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .sense(Sense::PASS_THROUGH);
    let swatch = Color::from_rgba8(disc.aim[0], disc.aim[1], disc.aim[2], 255);
    let plate_ink = ink_on(RIM_LIGHT);
    // The plate's shadow falls on the disc as on anything else: no exception for the rim.
    let zoom_plate =
        pill::plate(id.with("zoom"), label, RIM_LIGHT, plate_ink, PLATE_PAD).shadow(SHADOW);
    // The colour reading is a swatch: its body is the pixel, its ink whatever stays legible
    // on that body. The coordinates sit above it, so the two numbers read as one column.
    let readout = Decl::container(id.with("readout"), NodeKind::Column)
        .gap(pill::GAP)
        .sense(Sense::PASS_THROUGH)
        .child(pill::pill(id.with("coords"), coords))
        .child(pill::plate(
            id.with("colour"),
            hex,
            swatch,
            ink_on(swatch),
            pill::PAD_Y,
        ))
        .offset(readout_at.x - origin.x, readout_at.y - origin.y);
    root = root
        .child(picture)
        .child(zoom_plate.offset(plate_at.x - origin.x, plate_at.y - origin.y));
    root = root.child(readout);
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×3 picture whose pixels are distinguishable: red rises with x, green with y.
    fn picture() -> Image {
        let mut data = Vec::new();
        for y in 0..3u8 {
            for x in 0..3u8 {
                data.extend_from_slice(&[x * 100, y * 100, 40, 255]);
            }
        }
        Image {
            width: 3,
            height: 3,
            data: Arc::new(data),
            colour: ContentColour::SDR,
            scale: 1.0,
        }
    }

    /// The whole picture as the disc reads it: the tests draw no marks.
    fn source(image: &Image) -> Source<'_> {
        Source {
            width: image.width,
            height: image.height,
            data: image.data.as_slice(),
            colour: image.colour,
            origin: (0, 0),
        }
    }

    fn texel(disc: &Disc, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * disc.side + x) * 4) as usize;
        [
            disc.pixels[at],
            disc.pixels[at + 1],
            disc.pixels[at + 2],
            disc.pixels[at + 3],
        ]
    }

    #[test]
    fn the_disc_centre_is_the_aimed_pixel_and_the_rim_fades() {
        let image = picture();
        let source = source(&image);
        // One surface point is one image pixel, so the disc covers 15 pixels at 8×.
        let aim = Aim::at(Point::new(1.0, 1.0));
        let disc = bake(&source, aim, 8, 120);
        let middle = disc.side / 2;
        assert_eq!(
            texel(&disc, middle, middle),
            [100, 100, 40, 255],
            "the aimed pixel lands in the middle of the disc"
        );
        assert_eq!(texel(&disc, 0, 0)[3], 0, "the corner is outside the circle");
        // A diagonal run crosses the rim, where coverage must be partial for one pixel.
        let partial = (0..middle).any(|k| {
            let alpha = texel(&disc, k, k)[3];
            alpha > 0 && alpha < 255
        });
        assert!(partial, "the rim is coverage, not a hard cut");
        assert_eq!(
            disc.aim,
            [100, 100, 40, 255],
            "the pill states the same pixel"
        );
    }

    #[test]
    fn a_picture_edge_repeats_instead_of_leaving_a_hole() {
        let image = picture();
        let source = source(&image);
        let disc = bake(&source, Aim::at(Point::new(0.0, 0.0)), 8, 120);
        let middle = disc.side / 2;
        // The window reaches outside the 3×3 picture on both axes; every outside cell must
        // still be a picture pixel, so the disc has no transparent interior.
        assert_eq!(texel(&disc, middle, middle)[3], 255);
        assert_eq!(texel(&disc, middle - 10, middle - 10)[3], 255);
    }

    /// A loupe's promise is that one source pixel is one readable block. At a zoom that
    /// does not divide the disc evenly the blocks must still all be the same width.
    #[test]
    fn a_source_pixel_is_one_block_of_zoom_texels() {
        let (width, height) = (40u32, 40u32);
        let mut data = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[(x * 6) as u8, 0, 0, 255]);
            }
        }
        let image = Image {
            width,
            height,
            data: Arc::new(data),
            colour: ContentColour::SDR,
            scale: 1.0,
        };
        let source = source(&image);
        let disc = bake(&source, Aim::at(Point::new(20.0, 20.0)), 3, 120);
        let middle = disc.side / 2;
        let mut runs = Vec::new();
        let mut run = 1;
        for i in 1..disc.side {
            if texel(&disc, i, middle)[..3] == texel(&disc, i - 1, middle)[..3] {
                run += 1;
            } else {
                runs.push(run);
                run = 1;
            }
        }
        runs.push(run);
        // The outermost blocks repeat the picture's edge, so only the middle ones are
        // one-to-one with a source pixel.
        let inner = &runs[1..runs.len() - 1];
        assert!(
            inner.iter().all(|run| *run == 3),
            "a source pixel is three texels at 3x: {runs:?}"
        );
    }

    #[test]
    fn the_disc_flips_at_the_edges_and_stays_on_the_surface() {
        let surface = Rect::new(0.0, 0.0, 800.0, 600.0);
        let corner = Rect::new(0.0, 0.0, 800.0, 600.0);
        let inside = |rect: Rect| {
            rect.min_x() >= surface.min_x()
                && rect.min_y() >= surface.min_y()
                && rect.max_x() <= surface.max_x()
                && rect.max_y() <= surface.max_y()
        };
        let middle = place_disc(Point::new(400.0, 300.0), surface);
        assert_eq!(
            middle.origin,
            Point::new(400.0 + 60.0 * OFFSET - 60.0, 300.0 + 60.0 * OFFSET - 60.0),
            "the pointer sits inside the disc's top-left quadrant"
        );
        // At the bottom-right the disc flips to the pointer's top-left, and at the corner it
        // still fits: three points across, this is the whole fallback chain.
        let edge = place_disc(Point::new(790.0, 590.0), surface);
        assert!(inside(edge), "the disc stays on the surface: {edge:?}");
        assert!(
            edge.max_x() <= 790.0 && edge.max_y() <= 590.0,
            "both axes flipped: {edge:?}"
        );
        assert!(inside(place_disc(Point::new(0.0, 0.0), corner)));
    }

    #[test]
    fn a_wheel_notch_moves_the_zoom_to_the_next_window_and_stops_at_the_limits() {
        let mut loupe = Loupe::new();
        assert_eq!(loupe.zoom(), DEFAULT_ZOOM);
        assert!(
            !loupe.wheel(40.0, false),
            "a wheel with no disc up changes nothing"
        );
        // A notch is forty engine pixels; the bank must survive a fractional one.
        assert!(
            !loupe.wheel(20.0, true),
            "half a notch is banked, not applied"
        );
        assert!(loupe.wheel(20.0, true), "the second half makes one step");
        assert_eq!(loupe.zoom(), 9, "one notch is one step, on every surface");
        for _ in 0..40 {
            loupe.wheel(40.0, true);
        }
        assert_eq!(loupe.zoom(), MAX_ZOOM, "the top of the range");
        for _ in 0..40 {
            loupe.wheel(-40.0, true);
        }
        assert_eq!(loupe.zoom(), MIN_ZOOM, "and a floor");
    }

    /// Two neighbouring zooms never show the same picture, which they did while the window
    /// was the disc's width divided by the zoom: 13× and 14× both rounded to nine cells.
    #[test]
    fn no_two_zoom_steps_show_the_same_picture() {
        let (width, height) = (64u32, 64u32);
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, 40, 255]);
            }
        }
        let image = Image {
            width,
            height,
            data: Arc::new(data),
            colour: ContentColour::SDR,
            scale: 1.0,
        };
        let source = source(&image);
        let aim = Aim::at(Point::new(32.0, 32.0));
        for dots in [1.0f32, 1.25, 1.5, 2.0] {
            let side = texels(dots);
            let mut shown: Vec<Vec<u8>> = Vec::new();
            for zoom in MIN_ZOOM..=MAX_ZOOM {
                let disc = bake(&source, aim, zoom, side);
                assert_eq!(disc.span % 2, 1, "the window keeps the aim in its middle");
                assert!(
                    !shown.contains(&disc.pixels),
                    "{zoom}× draws a picture an earlier zoom already drew, at {dots} dots"
                );
                shown.push(disc.pixels);
            }
        }
    }

    /// The window is the least odd count of cells that covers the disc: the rim reads a cell
    /// the window holds, and no cell is fetched that the disc never shows.
    #[test]
    fn the_window_holds_every_cell_the_disc_shows_and_no_more() {
        for side in [120u32, 240] {
            for zoom in MIN_ZOOM..=MAX_ZOOM {
                let beside = cells_beside(side, zoom) as i32;
                let reach = (0..side)
                    .map(|texel| cell_at(side, zoom, texel).abs())
                    .max()
                    .unwrap_or(0);
                assert_eq!(
                    reach, beside,
                    "at {zoom}× on {side} texels the disc reaches {reach} cells"
                );
            }
        }
    }

    /// The swatch's ink is whatever stays legible on the pixel it shows: black on light
    /// bodies, white on dark ones, by WCAG contrast.
    #[test]
    fn the_swatch_ink_keeps_its_contrast() {
        assert_eq!(ink_on(Color::BLACK), Color::WHITE);
        assert_eq!(ink_on(Color::WHITE), Color::BLACK);
        let grey = |value: u8| Color::from_rgba8(value, value, value, 255);
        assert_eq!(ink_on(grey(128)), Color::BLACK, "mid grey");
        assert_eq!(
            ink_on(Color::from_rgba8(0, 0, 128, 255)),
            Color::WHITE,
            "navy"
        );
        assert_eq!(
            ink_on(RIM_LIGHT),
            Color::BLACK,
            "the plate carries black on the rim's white outline"
        );
    }

    /// The rim is an arc, not a level line: where it meets the plate's sides it lies a
    /// sagitta below its apex, so the plate drops by that much and the rim then enters the
    /// plate at the plate's own middle — which is what reads as centred.
    #[test]
    fn the_zoom_plate_sits_where_the_rim_meets_its_middle() {
        let surface = Rect::new(0.0, 0.0, 800.0, 600.0);
        let size = Size::new(38.0, 20.0);
        let disc = Rect::new(300.0, 200.0, DISC, DISC);
        let at = place_plate(size, disc, surface);
        let radius = DISC * 0.5;
        let half = size.w * 0.5;
        let entry = disc.min_y() + radius - (radius * radius - half * half).sqrt();
        let centre = at.y + size.h * 0.5;
        assert!(
            (centre - entry).abs() < 0.01,
            "the rim meets the plate's sides at the plate's middle: {at:?}"
        );
        let top = place_plate(size, Rect::new(300.0, 0.0, DISC, DISC), surface);
        assert_eq!(top.y, 0.0, "a disc at the top edge keeps the plate inside");
    }

    /// The readings stand under the disc and step above it when the action bar floats over
    /// the place they would take: the bar paints over the overlay, so a reading under it is
    /// a reading nobody sees.
    #[test]
    fn the_readings_step_above_the_disc_for_the_bar() {
        let surface = Rect::new(0.0, 0.0, 800.0, 600.0);
        // The disc with a plate riding its top.
        let extent = Rect::new(300.0, 182.0, DISC, 138.0);
        let size = Size::new(147.0, 54.0);
        let under = place_readout(size, extent, surface, None);
        assert_eq!(
            under,
            Point::new(300.0 + DISC * 0.5 - size.w * 0.5, 320.0 + pill::GAP)
        );
        let bar = Rect::new(300.0, under.y, 200.0, 48.0);
        let over = place_readout(size, extent, surface, Some(bar));
        assert_eq!(
            over.y,
            182.0 - pill::GAP - size.h,
            "above the plate rather than under the bar"
        );
    }

    /// The crosshair is the disc's centre at every zoom, and the block around it is the
    /// aimed pixel: the window is odd and the sampling centres the aimed cell, so the two
    /// always name the same pixel.
    #[test]
    fn the_crosshair_is_the_disc_centre_and_shows_the_aimed_pixel() {
        let image = picture();
        let source = source(&image);
        let aim = Aim::at(Point::new(1.0, 1.0));
        let middle = Point::new(DISC * 0.5, DISC * 0.5);
        for zoom in MIN_ZOOM..=MAX_ZOOM {
            let disc = bake(&source, aim, zoom, 120);
            assert_eq!(crosshair(&disc), middle, "at {zoom}x");
            let row = disc.side / 2;
            assert_eq!(
                texel(&disc, row, row),
                disc.aim,
                "the disc's centre shows the aimed pixel at {zoom}x"
            );
            // The aimed cell is one whole block around the centre, not a sliver beside it.
            let aimed: Vec<u32> = (0..disc.side)
                .filter(|i| texel(&disc, *i, row)[0] == disc.aim[0])
                .collect();
            assert_eq!(
                aimed.len(),
                zoom as usize,
                "the aimed cell is {zoom} texels at {zoom}x: {aimed:?}"
            );
            assert!(aimed.contains(&row), "at {zoom}x the aim misses the centre");
        }
    }

    /// A guide edge draws where the block it separates is drawn: the aimed pixel's own
    /// rectangle covers exactly the aimed block, edge to edge.
    #[test]
    fn the_guide_edges_sit_on_the_block_boundaries_the_disc_draws() {
        let image = picture();
        let source = source(&image);
        let aim = Aim::at(Point::new(1.0, 1.0));
        let centre = DISC * 0.5;
        for zoom in MIN_ZOOM..=MAX_ZOOM {
            let disc = bake(&source, aim, zoom, 120);
            let half = zoom as f32 * 0.5;
            let low = on_disc(&disc, Point::new(1.0, 1.0));
            let high = on_disc(&disc, Point::new(2.0, 2.0));
            for (got, want) in [
                (low.x, centre - half),
                (low.y, centre - half),
                (high.x, centre + half),
                (high.y, centre + half),
            ] {
                assert!(
                    (got - want).abs() < 1e-3,
                    "at {zoom}x the aimed pixel's edge draws at {got}, not {want}"
                );
            }
            // The texture agrees: the aimed block begins at the texel that edge points at.
            let row = disc.side / 2;
            let first = (0..disc.side)
                .find(|i| texel(&disc, *i, row)[0] == disc.aim[0])
                .expect("the aimed block is drawn");
            assert!(
                (first as f32 - low.x).abs() <= 0.5,
                "at {zoom}x the block starts at {first} but the edge draws at {}",
                low.x
            );
        }
    }

    /// The horizontal segment's midpoint, which is where the crosshair claims to point.
    fn crosshair(disc: &Disc) -> Point {
        let shapes = chrome(disc, None);
        let Shape::Line {
            from,
            to,
            thickness,
            ..
        } = shapes[1]
        else {
            panic!("the crosshair is a line");
        };
        assert_eq!(thickness, 1.0);
        Point::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5)
    }

    #[test]
    fn the_guide_is_clipped_to_the_circle() {
        let centre = Point::new(60.0, 60.0);
        let across = clip_segment(
            Point::new(-10.0, 60.0),
            Point::new(130.0, 60.0),
            centre,
            60.0,
        )
        .expect("a chord through the centre");
        assert!((across.0.x - 0.0).abs() < 1e-3 && (across.1.x - 120.0).abs() < 1e-3);
        assert!(
            clip_segment(Point::new(0.0, 0.0), Point::new(10.0, 0.0), centre, 60.0).is_none(),
            "a segment entirely outside the disc draws nothing"
        );
    }
}
