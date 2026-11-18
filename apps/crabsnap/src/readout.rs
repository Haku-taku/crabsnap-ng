//! The two pills that state a selection: where it starts and how large it is.
//!
//! Both count the frozen frame's own pixels, because that is what `Save` writes; the
//! numbers are the crop's own bounds, so the pill and the file never disagree.
//!
//! Placement is the legacy project's fallback chain (`src/ui/chrome/mod.rs:637-675` there),
//! with the action bar as the obstacle the pills must not hide under.

use ui_core::geom::{Point, Rect, Size};
use ui_core::{Decl, Id};

use crate::pill;

/// Where a pill may sit: above the corner, beside it, below, then inside.
const EDGE: f32 = 2.0;

/// The origin and the size, in image pixels, of the pixels the crop takes.
pub(crate) fn labels(bounds: (u32, u32, u32, u32)) -> [String; 2] {
    let (x0, y0, x1, y1) = bounds;
    [
        format!("{x0}, {y0}"),
        format!("{} × {}", x1 - x0, y1 - y0),
    ]
}

/// The row's top-left: the first candidate that covers neither the selection nor `avoid`.
pub(crate) fn place(size: Size, selection: Rect, surface: Rect, avoid: Option<Rect>) -> Point {
    let (w, h) = (size.w, size.h);
    let clamp = |x: f32, y: f32| -> Rect {
        let x = x.clamp(
            surface.min_x() + EDGE,
            (surface.max_x() - w - EDGE).max(surface.min_x() + EDGE),
        );
        let y = y.clamp(
            surface.min_y() + EDGE,
            (surface.max_y() - h - EDGE).max(surface.min_y() + EDGE),
        );
        Rect::new(x, y, w, h)
    };
    let candidates = [
        clamp(selection.min_x(), selection.min_y() - pill::GAP - h),
        clamp(selection.max_x() + pill::GAP, selection.min_y()),
        clamp(selection.min_x(), selection.max_y() + pill::GAP),
    ];
    for candidate in candidates {
        let free =
            !candidate.intersects(selection) && avoid.is_none_or(|bar| !candidate.intersects(bar));
        if free {
            return candidate.origin;
        }
    }
    // Nothing outside works: a selection that fills the surface has no outside. Sit inside
    // its corner, which is the one place the pill cannot hide the edge being dragged.
    clamp(selection.min_x() + pill::GAP, selection.min_y() + pill::GAP).origin
}

/// The readout for `selection`, placed on `surface` and clear of `avoid`.
///
/// `None` where no measurer is attached, which is every test context.
pub(crate) fn build(
    cx: &ui_core::cx::Cx<'_>,
    id: Id,
    selection: Rect,
    bounds: (u32, u32, u32, u32),
    surface: Rect,
    avoid: Option<Rect>,
) -> Option<Decl> {
    let labels = labels(bounds);
    let size = pill::measure(cx, &labels)?;
    let at = place(size, selection, surface, avoid);
    Some(pill::row(id, &labels).offset(at.x, at.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size() -> Size {
        Size::new(120.0, 22.0)
    }

    #[test]
    fn the_readout_prefers_a_place_that_covers_neither_the_selection_nor_the_bar() {
        let surface = Rect::new(0.0, 0.0, 800.0, 600.0);
        let selection = Rect::new(200.0, 100.0, 300.0, 200.0);
        let above = place(size(), selection, surface, None);
        assert_eq!(
            above,
            Point::new(200.0, 100.0 - pill::GAP - 22.0),
            "a free strip above the corner is the first choice"
        );

        // The action bar sits exactly where the first choice would go, as it does when the
        // selection reaches the top edge and both the bar and the pill would flip upward.
        let bar = Rect::new(above.x, above.y, 300.0, 40.0);
        let beside = place(size(), selection, surface, Some(bar));
        assert_eq!(
            beside,
            Point::new(200.0 + 300.0 + pill::GAP, 100.0),
            "the pill steps to the right of the corner rather than under the bar"
        );
    }

    #[test]
    fn a_selection_that_fills_the_surface_keeps_the_readout_inside_it() {
        let surface = Rect::new(0.0, 0.0, 800.0, 600.0);
        let selection = surface;
        let at = place(size(), selection, surface, None);
        assert!(
            selection.contains(at) && selection.contains(Point::new(at.x + 120.0, at.y + 22.0)),
            "with no outside, the pill sits inside the corner: {at:?}"
        );
    }
}
