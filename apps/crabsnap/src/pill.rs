//! The overlay's readout pill: a number over an arbitrary picture.
//!
//! Pins wear MD3 badge chips because they sit on the desktop. These sit on a screenshot,
//! where a light chip disappears over a bright page, so they keep the legacy project's dark
//! body (`src/ui/chrome/mod.rs:110-122` there). The loupe and the selection readout share
//! it so one overlay never shows two kinds of number.

use ui_core::geom::{Color, Corners, Insets, Size};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeData, NodeKind, Sense, Weight};

/// Space between two pills, in points.
pub(crate) const GAP: f32 = 6.0;
/// Padding inside a pill: horizontal, then vertical.
const PAD_X: f32 = 6.0;
pub(crate) const PAD_Y: f32 = 3.0;
/// Type size, in points. Larger than the toolbar's labels: these are numbers to read.
const TEXT: f32 = 13.0;
pub(crate) const RADIUS: f32 = 4.0;
const FILL: Color = Color::rgba(20.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0, 225.0 / 255.0);
const INK: Color = Color::WHITE;

/// The pill's own text. Built by the same call the pill makes, so measuring it is
/// measuring the node layout will see.
fn body(id: Id, text: impl Into<String>, ink: Color) -> Decl {
    ui_widgets::text::leaf(
        id,
        text.into(),
        TEXT,
        ink,
        Weight::REGULAR,
        f32::INFINITY,
        0.0,
        0.0,
    )
}

/// One pill whose body and ink the caller picks: a colour swatch, a plate on a rim.
pub(crate) fn plate(id: Id, label: impl Into<String>, fill: Color, ink: Color, pad_y: f32) -> Decl {
    let decl = Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets {
            left: PAD_X,
            top: pad_y,
            right: PAD_X,
            bottom: pad_y,
        })
        .background(fill, Corners::all(RADIUS))
        // Chrome must never take a press meant for the picture under it.
        .sense(Sense::PASS_THROUGH);
    decl.child(body(id.with("label"), label, ink))
}

/// One pill in the overlay's own colours.
pub(crate) fn pill(id: Id, label: impl Into<String>) -> Decl {
    plate(id, label, FILL, INK, PAD_Y)
}

/// The pills of `labels` in one row, left to right.
pub(crate) fn row(id: Id, labels: &[String]) -> Decl {
    let mut row = Decl::container(id, NodeKind::Row)
        .gap(GAP)
        .sense(Sense::PASS_THROUGH);
    for (index, label) in labels.iter().enumerate() {
        row = row.child(pill(id.with(index), label.clone()));
    }
    row
}

/// One pill's box as layout will measure it, for a caller that places it by that.
///
/// `None` where the context has no text measurer, which is every test context.
pub(crate) fn measure_one(cx: &ui_core::cx::Cx<'_>, label: &str, pad_y: f32) -> Option<Size> {
    let decl = body(Id::str("pill/measure"), label.to_string(), INK);
    let NodeData::Text { block, .. } = &decl.data else {
        return None;
    };
    let measured = cx.text_size(block, f32::INFINITY)?;
    // Layout raises every text node to its line box, so the ink height alone would place
    // the pill a fraction of a point too short.
    let line = measured.h.max(decl.size_class.height.hint);
    Some(Size::new(measured.w + PAD_X * 2.0, line + pad_y * 2.0))
}

/// What [`row`] will measure to, before layout: the caller places it by this.
pub(crate) fn measure(cx: &ui_core::cx::Cx<'_>, labels: &[String]) -> Option<Size> {
    let mut width = GAP * labels.len().saturating_sub(1) as f32;
    let mut height = 0.0f32;
    for label in labels {
        let size = measure_one(cx, label, PAD_Y)?;
        width += size.w;
        height = height.max(size.h);
    }
    Some(Size::new(width, height))
}

/// What a [`column`] of `labels` will measure to: as wide as its widest pill, and as tall
/// as the pills and the gaps between them.
pub(crate) fn measure_column(cx: &ui_core::cx::Cx<'_>, labels: &[String]) -> Option<Size> {
    let mut width = 0.0f32;
    let mut height = GAP * labels.len().saturating_sub(1) as f32;
    for label in labels {
        let size = measure_one(cx, label, PAD_Y)?;
        width = width.max(size.w);
        height += size.h;
    }
    Some(Size::new(width, height))
}
