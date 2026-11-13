//! What a capture hands back, and which rung this build can bind.
//!
//! The frame, the picture and the crop are the portable core's; the rungs belong to the
//! platform crate the target tables chose, and each one answers the same `Backend` trait.
//! This module is the single name the app uses for both, so no other file names a capture
//! protocol.

pub use crabsnap_core::capture::{
    Cropped, Frame, Image, Light, Pending, Started, brightest_nits, density, pixels_per_metre,
    png_scale, read_png,
};
pub use crabsnap_platform::capture::{Session, open};

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::geom::{Point, Rect};

    fn image(width: u32, height: u32) -> Image {
        let data = (0..width * height)
            .flat_map(|i| [(i % 251) as u8, 0, 0, 255])
            .collect();
        Image {
            width,
            height,
            data: std::sync::Arc::new(data),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
            light: Light::sdr(),
        }
    }

    /// The selection a surface annotates and the crop it pins are the same pixels: both take
    /// their grid from `crop_of`, so a mark placed through a `Space` built from it lands on
    /// the exact device pixels the cropped picture carries. A second rounding rule anywhere
    /// would leave the annotation a fraction of a pixel off the picture it was drawn on.
    #[test]
    fn the_annotated_selection_and_its_crop_agree_on_where_the_pixels_start() {
        let source = image(8, 8);
        let scale = 2.0;
        let logical = Rect::new(1.3, 2.7, 3.4, 2.9);
        let (x0, y0, x1, y1) = source.crop_of(logical, scale);
        let cropped = source.crop(Rect::new(
            logical.min_x() * scale,
            logical.min_y() * scale,
            logical.size.w * scale,
            logical.size.h * scale,
        ));
        assert_eq!(cropped.origin, (x0, y0));
        assert_eq!((cropped.width, cropped.height), (x1 - x0, y1 - y0));

        // The space a surface would build from the same rule: its rect is the crop, restated
        // in logical points.
        let space = crate::document::Space::new(
            Rect::new(
                x0 as f32 / scale,
                y0 as f32 / scale,
                (x1 - x0) as f32 / scale,
                (y1 - y0) as f32 / scale,
            ),
            (x1 - x0, y1 - y0),
        );
        // A mark at crop pixel `(px, py)` paints on the frame's device pixel `(x0 + px,
        // y0 + py)`, and reading it back gives the same pixel: no fraction survives the trip.
        for (px, py) in [(0, 0), (1, 3), (5, 4)] {
            let centre = Point::new(px as f32 + 0.5, py as f32 + 0.5);
            let surface = space.to_surface(centre);
            let back = space.to_image(surface);
            assert!((back.x - centre.x).abs() < 1e-3, "{back:?} != {centre:?}");
            assert!((back.y - centre.y).abs() < 1e-3, "{back:?} != {centre:?}");
            assert_eq!((surface.x * scale).floor() as u32, x0 + px as u32);
            assert_eq!((surface.y * scale).floor() as u32, y0 + py as u32);
        }
    }
}
