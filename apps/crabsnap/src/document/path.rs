//! A Bezier mark: its anchors, the geometry they define, and the one raster every
//! composition samples.
//!
//! The engine has typed primitives but no cubic path, so a path paints into its own
//! straight-alpha RGBA tile and joins the `DrawList` as an image. The tile is clipped
//! to the picture and keeps the mark's stable id, so undo, the loupe and export all
//! read one drawing without a second path backend.

use std::sync::Arc;

use resvg::tiny_skia;
use ui_core::colour::ContentColour;
use ui_core::draw::{DrawList, ImageEntry, ImagePrim, ImageSource, Sampling};
use ui_core::geom::{Color, Corners, Point, Rect};
use ui_core::{Id, NodeId};

use super::Style;

/// What one raster may take. A path can cover a 4K output, so its tile is capped like a
/// vector pin's instead of allocating from unbounded control-point coordinates.
const MAX_RASTER_BYTES: usize = 32 << 20;
/// A closed path's interior wash: translucent, so the picture reads through it.
const FILL_ALPHA: f32 = 0.5;
/// Room beyond the curve for a round cap and antialiasing, in image pixels.
const RASTER_PAD: f32 = 2.0;
/// The smallest height or width a flat envelope is widened to.
const FLAT_ENVELOPE: f32 = 6.0;

/// One anchor and its outgoing handle. The incoming handle is its mirror, so the two
/// sides of a smooth anchor cannot drift into a corner by accident.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathAnchor {
    pub at: Point,
    pub handle_out: Point,
}

impl PathAnchor {
    pub fn new(at: Point) -> Self {
        Self {
            at,
            handle_out: at,
        }
    }

    pub fn handle_in(&self) -> Point {
        Point::new(
            2.0 * self.at.x - self.handle_out.x,
            2.0 * self.at.y - self.handle_out.y,
        )
    }

    fn translated(&self, map: &impl Fn(Point) -> Point) -> Self {
        Self {
            at: map(self.at),
            handle_out: map(self.handle_out),
        }
    }
}

/// A cubic path through its anchors, in image pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathMark {
    pub anchors: Vec<PathAnchor>,
    pub closed: bool,
}

impl PathMark {
    pub fn translated(&self, map: impl Fn(Point) -> Point) -> Self {
        Self {
            anchors: self
                .anchors
                .iter()
                .map(|anchor| anchor.translated(&map))
                .collect(),
            closed: self.closed,
        }
    }

    /// The box through every control point, widened where it is flat so the resize
    /// handles can still be grabbed. Handles are kept so a curve's controls never sit
    /// outside the box a grip maps.
    pub fn bounds(&self) -> Rect {
        let (mut left, mut top) = (f32::INFINITY, f32::INFINITY);
        let (mut right, mut bottom) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for anchor in &self.anchors {
            for point in [anchor.at, anchor.handle_out, anchor.handle_in()] {
                left = left.min(point.x);
                top = top.min(point.y);
                right = right.max(point.x);
                bottom = bottom.max(point.y);
            }
        }
        if !left.is_finite() || !top.is_finite() {
            return Rect::ZERO;
        }
        let center = Point::new((left + right) * 0.5, (top + bottom) * 0.5);
        let width = (right - left).max(FLAT_ENVELOPE);
        let height = (bottom - top).max(FLAT_ENVELOPE);
        Rect::new(
            center.x - width * 0.5,
            center.y - height * 0.5,
            width,
            height,
        )
    }

    fn path(&self, width: f32) -> Option<tiny_skia::Path> {
        let mut builder = tiny_skia::PathBuilder::new();
        match self.anchors.as_slice() {
            [] => return None,
            [only] => {
                return tiny_skia::PathBuilder::from_circle(
                    only.at.x,
                    only.at.y,
                    (width * 0.5).max(0.5),
                );
            }
            _ => {}
        }
        let first = self.anchors.first()?;
        builder.move_to(first.at.x, first.at.y);
        let segments = if self.closed {
            self.anchors.len()
        } else {
            self.anchors.len() - 1
        };
        for index in 0..segments {
            let from = self.anchors.get(index)?;
            let to = self.anchors.get((index + 1) % self.anchors.len())?;
            let handle_in = to.handle_in();
            builder.cubic_to(
                from.handle_out.x,
                from.handle_out.y,
                handle_in.x,
                handle_in.y,
                to.at.x,
                to.at.y,
            );
        }
        if self.closed {
            builder.close();
        }
        builder.finish()
    }

    /// Draw the path into `list` as one image tile. The tile id follows the document
    /// revision: a changed preview gets a fresh upload, and the old texture is evicted.
    pub fn paint(
        &self,
        list: &mut DrawList,
        owner: NodeId,
        id: u64,
        revision: u64,
        style: &Style,
    ) {
        let width = style.width.max(0.0);
        let Some(path) = self.path(width) else {
            return;
        };
        let tight = path.compute_tight_bounds().unwrap_or_else(|| path.bounds());
        let padded = Rect::new(
            tight.left() - width * 0.5 - RASTER_PAD,
            tight.top() - width * 0.5 - RASTER_PAD,
            tight.width() + width + RASTER_PAD * 2.0,
            tight.height() + width + RASTER_PAD * 2.0,
        );
        let Some(visible) = padded.intersection(list.canvas) else {
            return;
        };
        let visible = Rect::new(
            visible.origin.x.floor(),
            visible.origin.y.floor(),
            visible.size.w.ceil().max(1.0),
            visible.size.h.ceil().max(1.0),
        )
        .intersection(list.canvas)
        .unwrap_or(visible);
        let (raster_width, raster_height) = fitted((visible.size.w, visible.size.h));
        let Some(mut pixmap) = tiny_skia::Pixmap::new(raster_width, raster_height) else {
            // An oversized tile is not a mark: leaving it out keeps the picture and the
            // rest of the session usable, and the terminal says why it is absent.
            eprintln!("pin: a Bezier path raster could not be allocated");
            return;
        };
        let scale_x = raster_width as f32 / visible.size.w.max(f32::EPSILON);
        let scale_y = raster_height as f32 / visible.size.h.max(f32::EPSILON);
        let scale = scale_x.min(scale_y);
        let transform = tiny_skia::Transform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            -visible.origin.x * scale,
            -visible.origin.y * scale,
        );
        let mut stroke = tiny_skia::Stroke {
            width: width / scale,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        if scale < 1.0 {
            // A hairline is enough when a capped tile has been reduced to fit memory;
            // keeping style.width there would fill the whole path with ink.
            stroke.width = 1.0 / scale;
        }
        if self.closed && self.anchors.len() > 1 {
            let mut fill = tiny_skia::Paint::default();
            let wash = style.ink.faded(FILL_ALPHA);
            let (red, green, blue, alpha) = rgba8(wash);
            fill.set_color_rgba8(red, green, blue, alpha);
            pixmap.fill_path(
                &path,
                &fill,
                tiny_skia::FillRule::Winding,
                transform,
                None,
            );
        } else if self.anchors.len() == 1 && width > 0.0 {
            let mut fill = tiny_skia::Paint::default();
            let (red, green, blue, alpha) = rgba8(style.ink);
            fill.set_color_rgba8(red, green, blue, alpha);
            pixmap.fill_path(
                &path,
                &fill,
                tiny_skia::FillRule::Winding,
                transform,
                None,
            );
        }
        if width > 0.0 && self.anchors.len() > 1 {
            let mut ink = tiny_skia::Paint::default();
            let (red, green, blue, alpha) = rgba8(style.ink);
            ink.set_color_rgba8(red, green, blue, alpha);
            pixmap.stroke_path(&path, &ink, &stroke, transform, None);
        }
        let entry = ImageEntry {
            id: Id::of(("crabsnap/bezier", id, revision)).as_u64(),
            width: raster_width,
            height: raster_height,
            source: ImageSource::Pixels(Arc::new(straight_alpha(pixmap.data()))),
            colour: ContentColour::SDR,
            sampling: Sampling::Smooth,
        };
        let texture = list.push_image(entry);
        // A capped tile uses only the part its uniform scale could reach; mapping that
        // sub-rect back onto the visible box keeps the path's aspect ratio intact.
        let used = Rect::new(
            0.0,
            0.0,
            (visible.size.w * scale).min(raster_width as f32),
            (visible.size.h * scale).min(raster_height as f32),
        );
        list.push_image_prim(
            owner,
            ImagePrim {
                rect: visible,
                corners: Corners::ZERO,
                texture,
                src: used,
            },
        );
    }
}

/// `Color` is straight sRGB in `0..=1`; tiny-skia takes straight RGBA8.
fn rgba8(colour: Color) -> (u8, u8, u8, u8) {
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    (
        channel(colour.r),
        channel(colour.g),
        channel(colour.b),
        channel(colour.a),
    )
}

/// A tiny-skia pixmap is premultiplied; an `ImageEntry` is straight alpha. Vector
/// pins use this same conversion, so both raster sources carry one alpha convention.
pub(crate) fn straight_alpha(premultiplied: &[u8]) -> Vec<u8> {
    let mut pixels = premultiplied.to_vec();
    for texel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(texel[3]);
        // An opaque texel is its own straight form, and a fully transparent one has no
        // colour to recover.
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut texel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha) as u8;
        }
    }
    pixels
}

/// Truncate to whole pixels, reducing both axes alike when the tile would not fit
/// [`MAX_RASTER_BYTES`]. The destination keeps its own size, so a reduced tile is
/// sampled back onto the whole visible box.
pub(crate) fn fitted(size: (f32, f32)) -> (u32, u32) {
    let (mut width, mut height) = (size.0.max(0.0), size.1.max(0.0));
    let room = (MAX_RASTER_BYTES / 4) as f32;
    if width * height > room {
        let reduce = (room / (width * height)).sqrt();
        width *= reduce;
        height *= reduce;
    }
    (
        (width.round().max(1.0)) as u32,
        (height.round().max(1.0)) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cubic's extremes can pass outside its anchors; the raster envelope is derived
    /// from the path itself, so a looping curve is not cut at its control points.
    #[test]
    fn a_curve_is_bounded_by_its_control_points_and_not_its_anchors() {
        let path = PathMark {
            anchors: vec![
                PathAnchor {
                    at: Point::new(10.0, 10.0),
                    handle_out: Point::new(100.0, 100.0),
                },
                PathAnchor::new(Point::new(20.0, 10.0)),
            ],
            closed: false,
        };
        assert_eq!(
            path.bounds(),
            Rect::new(-80.0, -80.0, 180.0, 180.0),
            "the mirrored incoming handle is a control point too",
        );
    }

    /// The path is one contour with mirrored incoming handles; a closed contour joins
    /// its last anchor to the first rather than leaving a gap.
    #[test]
    fn the_incoming_handle_mirrors_the_outgoing_one() {
        let anchor = PathAnchor {
            at: Point::new(10.0, 20.0),
            handle_out: Point::new(14.0, 26.0),
        };
        assert_eq!(anchor.handle_in(), Point::new(6.0, 14.0));
        let open = PathMark {
            anchors: vec![anchor, PathAnchor::new(Point::new(30.0, 20.0))],
            closed: false,
        };
        let closed = PathMark {
            closed: true,
            ..open.clone()
        };
        assert!(open.path(3.0).is_some());
        assert!(closed.path(3.0).is_some());
    }
}
