//! The annotation document: marks in the captured image's pixels, and the history that owns
//! them.
//!
//! Tools propose an [`Edit`] and the document commits it; undo and redo live here rather than
//! in each tool, so every mark reverses the same way. Positions are image pixels, never
//! surface points: a mark must stay on the content it was drawn over when the picture is
//! shown at another size. The document is drawn into an engine canvas at those same pixels,
//! so the card, the editor and the export all sample one composition.

use ui_core::draw::{DrawList, EllipsePrim, LinePrim, RectPrim};
use ui_core::geom::{Color, Corners, Point, Rect};

pub mod path;
pub mod text;

pub use path::PathMark;
pub use text::TextMark;

/// A mark's identity, stable across undo and redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkId(u64);

/// The bytes an input method is composing over one text mark: drawn, not committed.
#[derive(Clone, Debug, PartialEq)]
pub struct Composing {
    pub id: MarkId,
    /// Where the composition starts, in bytes of the mark's committed text.
    pub caret: usize,
    pub preedit: ui_core::Preedit,
}

/// How a mark is painted, in image pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Stroke colour, and a text mark's glyph colour.
    pub ink: Color,
    /// Interior colour; `None` leaves the inside empty. A text mark paints its own wash, so
    /// this is a shape's alone.
    pub fill: Option<Color>,
    /// Stroke width in image pixels; zero strokes nothing.
    pub width: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            ink: Color::from_rgba8(235, 60, 50, 255),
            fill: None,
            width: 3.0,
        }
    }
}

/// What a mark draws, in the image's own pixels.
#[allow(dead_code)] // The tool session constructs these; draw matches every kind.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkKind {
    Rect(Rect),
    Ellipse(Rect),
    Line(Line),
    Pen(Vec<Point>),
    Bezier(PathMark),
    Text(TextMark),
}

/// Which ends of a straight mark carry an arrow head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heads {
    pub start: bool,
    pub end: bool,
}

impl Heads {
    pub const NONE: Heads = Heads {
        start: false,
        end: false,
    };
    pub const END: Heads = Heads {
        start: false,
        end: true,
    };
    pub const BOTH: Heads = Heads {
        start: true,
        end: true,
    };
}

/// A straight mark and its heads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub from: Point,
    pub to: Point,
    pub heads: Heads,
}

/// One annotation.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub kind: MarkKind,
    pub style: Style,
}

/// The four sides a resize maps a mark onto, each carrying its own coordinate: a `Rect`
/// normalises to a positive size, which would clamp a grip at the side opposite it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sides {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Sides {
    pub(crate) fn of(rect: Rect) -> Self {
        Self {
            left: rect.min_x(),
            top: rect.min_y(),
            right: rect.max_x(),
            bottom: rect.max_y(),
        }
    }
}

impl Mark {
    /// The same mark moved by `(dx, dy)` image pixels.
    pub fn translated(&self, dx: f32, dy: f32) -> Mark {
        let map = |p: Point| Point::new(p.x + dx, p.y + dy);
        let kind = match &self.kind {
            MarkKind::Rect(rect) | MarkKind::Ellipse(rect) => {
                let moved = Rect::new(
                    rect.origin.x + dx,
                    rect.origin.y + dy,
                    rect.size.w,
                    rect.size.h,
                );
                match &self.kind {
                    MarkKind::Rect(_) => MarkKind::Rect(moved),
                    _ => MarkKind::Ellipse(moved),
                }
            }
            MarkKind::Line(line) => MarkKind::Line(Line {
                from: map(line.from),
                to: map(line.to),
                heads: line.heads,
            }),
            MarkKind::Pen(points) => MarkKind::Pen(points.iter().copied().map(map).collect()),
            MarkKind::Bezier(path) => MarkKind::Bezier(path.translated(map)),
            MarkKind::Text(text) => MarkKind::Text(text.translated(map)),
        };
        Mark {
            kind,
            style: self.style.clone(),
        }
    }

    /// The mark a resize grip leaves: a text frame's side grip sets the width its lines may
    /// run in while the type keeps its size, and every other grip maps the whole mark from
    /// the old box onto the new one.
    pub(crate) fn resized(
        &self,
        from: Rect,
        to: Sides,
        edges: (bool, bool, bool, bool),
        scale_stroke: bool,
    ) -> Mark {
        let vertical = edges.1 || edges.3;
        if let MarkKind::Text(text) = &self.kind
            && !vertical
        {
            let width = (to.right - to.left).abs();
            return Mark {
                kind: MarkKind::Text(text.widened(width, edges.0.then_some(to.right))),
                style: self.style.clone(),
            };
        }
        self.rebased(from, to, scale_stroke)
    }

    /// The same mark mapped from one box onto another, so a resize keeps the stroke's shape
    /// instead of rebuilding it from the box alone; `scale_stroke` makes its width follow.
    pub(crate) fn rebased(&self, from: Rect, to: Sides, scale_stroke: bool) -> Mark {
        if from.size.w <= 0.0 || from.size.h <= 0.0 {
            return self.clone();
        }
        let sx = (to.right - to.left) / from.size.w;
        let sy = (to.bottom - to.top) / from.size.h;
        let map = |p: Point| {
            Point::new(
                to.left + (p.x - from.origin.x) * sx,
                to.top + (p.y - from.origin.y) * sy,
            )
        };
        let kind = match &self.kind {
            MarkKind::Rect(rect) | MarkKind::Ellipse(rect) => {
                // Both corners are mapped: a mirrored box is still a box.
                let (near, far) = (map(rect.origin), map(Point::new(rect.max_x(), rect.max_y())));
                let mapped = Rect::new(
                    near.x.min(far.x),
                    near.y.min(far.y),
                    (far.x - near.x).abs(),
                    (far.y - near.y).abs(),
                );
                match &self.kind {
                    MarkKind::Rect(_) => MarkKind::Rect(mapped),
                    _ => MarkKind::Ellipse(mapped),
                }
            }
            MarkKind::Line(line) => MarkKind::Line(Line {
                from: map(line.from),
                to: map(line.to),
                heads: line.heads,
            }),
            MarkKind::Pen(points) => MarkKind::Pen(points.iter().copied().map(map).collect()),
            MarkKind::Bezier(path) => MarkKind::Bezier(path.translated(map)),
            // A text mark's box is its frame, so it always follows the drag; the font takes
            // the box's proportions with it.
            MarkKind::Text(text) => MarkKind::Text(text.rebased(map, sx, sy)),
        };
        // A corner may drag the two axes by different factors, so a scaled stroke takes
        // the smaller one and stays inside its own box.
        let width = if scale_stroke {
            self.style.width * sx.abs().min(sy.abs())
        } else {
            self.style.width
        };
        Mark {
            kind,
            style: Style {
                width,
                ..self.style.clone()
            },
        }
    }

    /// The box the mark paints inside, in image pixels: the control points' own envelope, or
    /// a text mark's frame as the last measurement sized it.
    #[allow(dead_code)] // Handle placement reads it once the tool session exists.
    pub fn bounds(&self) -> Rect {
        match &self.kind {
            // The outline is the box: a stroke straddles it, and a box grown by the stroke
            // would map the corner opposite a grip a half-stroke short of where it stood.
            MarkKind::Rect(rect) | MarkKind::Ellipse(rect) => *rect,
            MarkKind::Line(line) => envelope([line.from, line.to]),
            MarkKind::Pen(points) => envelope(points.iter().copied()),
            MarkKind::Bezier(path) => path.bounds(),
            MarkKind::Text(text) => text.frame(),
        }
    }
}

/// The smallest height or width a flat envelope is widened to. A horizontal line's points
/// share a `y`, so without it its four grips coincide and no drag could give it height.
const FLAT_ENVELOPE: f32 = 6.0;

/// The box through a set of control points, widened to `FLAT_ENVELOPE` where it is flat.
fn envelope(points: impl IntoIterator<Item = Point>) -> Rect {
    let (mut left, mut top) = (f32::INFINITY, f32::INFINITY);
    let (mut right, mut bottom) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for point in points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
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

/// The arrow head's length, as a multiple of the stroke width.
const ARROW_HEAD: f32 = 5.0;

/// The arrow head's spread from the shaft, in radians.
const ARROW_SPREAD: f32 = 0.5;

fn line_head(width: f32) -> f32 {
    width.max(1.0) * ARROW_HEAD
}

/// One reversible change.
///
/// `before` and `after` are the whole mark on either side, so undo is redo with the two
/// swapped and a move or restyle needs no new history code.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub id: MarkId,
    /// Where the mark sat, so undoing a removal puts it back in paint order instead of on top.
    pub index: usize,
    pub before: Option<Mark>,
    pub after: Option<Mark>,
}

/// Marks in paint order plus the two history stacks.
#[derive(Debug, Default)]
pub struct Document {
    marks: Vec<(MarkId, Mark)>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    next_id: u64,
    /// Bumped by every change that moves a pixel; a canvas uses it as its cache key.
    revision: u64,
}

#[allow(dead_code)] // History is written by the tool session; the canvas reads draw and revision.
impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// The version a canvas records: anything that changes the picture changes this.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// The same marks and history moved by `(dx, dy)` pixels: the overlay draws on the whole
    /// frozen frame, and the crop a pin keeps starts at the selection's corner instead.
    pub fn translated(self, dx: f32, dy: f32) -> Document {
        let moved = |edit: Edit| Edit {
            id: edit.id,
            index: edit.index,
            before: edit.before.map(|mark| mark.translated(dx, dy)),
            after: edit.after.map(|mark| mark.translated(dx, dy)),
        };
        Document {
            marks: self
                .marks
                .into_iter()
                .map(|(id, mark)| (id, mark.translated(dx, dy)))
                .collect(),
            undo: self.undo.into_iter().map(moved).collect(),
            redo: self.redo.into_iter().map(moved).collect(),
            next_id: self.next_id,
            revision: self.revision,
        }
    }

    /// A proposal to draw `mark` on top. The document mints the id; committing is separate.
    pub fn insert_edit(&mut self, mark: Mark) -> Edit {
        self.next_id += 1;
        Edit {
            id: MarkId(self.next_id),
            index: self.marks.len(),
            before: None,
            after: Some(mark),
        }
    }

    /// A proposal to take `id` out, or `None` when no such mark is drawn.
    pub fn remove_edit(&self, id: MarkId) -> Option<Edit> {
        let index = self.marks.iter().position(|(mark, _)| *mark == id)?;
        Some(Edit {
            id,
            index,
            before: Some(self.marks[index].1.clone()),
            after: None,
        })
    }

    /// Apply an edit and remember it. Anything undone stops being redoable: the history is a
    /// line, not a tree.
    pub fn commit(&mut self, edit: Edit) {
        self.place(&edit.after, edit.id, edit.index);
        self.undo.push(edit);
        self.redo.clear();
        self.revision += 1;
    }

    /// Take back the last commit; `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<Edit> {
        let edit = self.undo.pop()?;
        self.place(&edit.before, edit.id, edit.index);
        self.redo.push(edit.clone());
        self.revision += 1;
        Some(edit)
    }

    /// Put back the last undone commit; `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<Edit> {
        let edit = self.redo.pop()?;
        self.place(&edit.after, edit.id, edit.index);
        self.undo.push(edit.clone());
        self.revision += 1;
        Some(edit)
    }

    /// How much history the document holds. A session records this when it opens, so it can
    /// take back exactly its own work later.
    pub fn depth(&self) -> usize {
        self.undo.len()
    }

    /// Take back every commit past `depth`, and forget the ones taken back: a discarded
    /// session must not come back through a redo.
    pub fn rewind(&mut self, depth: usize) {
        if self.undo.len() <= depth {
            return;
        }
        for _ in depth..self.undo.len() {
            let Some(edit) = self.undo.pop() else {
                break;
            };
            self.place(&edit.before, edit.id, edit.index);
            self.revision += 1;
        }
        self.redo.clear();
    }

    /// The drawn marks, bottom to top.
    pub fn marks(&self) -> impl Iterator<Item = (MarkId, &Mark)> {
        self.marks.iter().map(|(id, mark)| (*id, mark))
    }

    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// The mark at `index` in paint order, with its id.
    pub fn at(&self, index: usize) -> Option<(MarkId, &Mark)> {
        self.marks.get(index).map(|(id, mark)| (*id, mark))
    }

    pub fn get(&self, id: MarkId) -> Option<&Mark> {
        self.marks
            .iter()
            .find(|(mark, _)| *mark == id)
            .map(|(_, mark)| mark)
    }

    /// Replace a mark in place without history, for a drag the user has not released or a
    /// field still typing: the pixels follow the pointer and the keystroke, and the release
    /// or the field's close decides whether it becomes an edit.
    pub fn preview(&mut self, id: MarkId, mark: Mark) {
        if let Some(slot) = self.marks.iter_mut().find(|(existing, _)| *existing == id) {
            slot.1 = mark;
            self.revision += 1;
        }
    }

    /// The index `id` sits at, for an edit that restores it where it was.
    pub fn index_of(&self, id: MarkId) -> Option<usize> {
        self.marks.iter().position(|(existing, _)| *existing == id)
    }

    /// Shape every text mark against the host's measurer, so its frame is the one it paints.
    ///
    /// Run once per build, after whatever the pointer changed and before the list is made.
    /// The revision deliberately does not move: a re-measure follows text the document
    /// already counted, so bumping here would repaint the canvas on every keystroke.
    ///
    /// Answers whether any frame moved: a mark measured differently changes the controls
    /// read off it, and the declaration cache cannot see that by itself.
    pub fn measure(&mut self, cx: &ui_core::cx::Cx<'_>) -> bool {
        let mut moved = false;
        for (_, mark) in &mut self.marks {
            if let MarkKind::Text(text) = &mut mark.kind {
                moved |= text.measure(cx);
            }
        }
        moved
    }

    /// Draw every mark at its image pixels into a canvas list.
    ///
    /// Nothing here maps coordinates: the list is already in image pixels, which is what the
    /// canvas samples and what an export rasterizes. A mark being typed is one of them:
    /// the field over it adds the caret, and nothing redraws the text a second way.
    pub fn draw(&self, list: &mut DrawList) {
        self.draw_composing(list, None);
    }

    /// The same, with an input method's unfinished bytes shown over one mark.
    ///
    /// A preedit is not part of the document: it is drawn where its commit would land, and
    /// an export never sees it because that path calls [`Document::draw`].
    pub fn draw_composing(&self, list: &mut DrawList, composing: Option<&Composing>) {
        let owner = DrawList::canvas_owner();
        for (id, mark) in &self.marks {
            draw_mark(
                list,
                owner,
                id.0,
                self.revision,
                mark,
                composing.filter(|composing| composing.id == *id),
            );
        }
    }

    /// The revision a canvas built with `composing` paints, so a preedit repaints.
    pub fn version(&self, composing: Option<&Composing>) -> u64 {
        let Some(composing) = composing else {
            return self.revision;
        };
        let mut version = self.revision;
        for byte in composing.preedit.text.bytes() {
            version = version.rotate_left(5) ^ u64::from(byte);
        }
        version ^ ((composing.caret as u64) << 32) ^ composing.preedit.cursor as u64
    }

    /// Place a mark history does not know yet: a text mark exists while it is typed, and
    /// the edit that ends the typing is the one history keeps.
    pub fn insert_preview(&mut self, mark: Mark) -> MarkId {
        self.next_id += 1;
        let id = MarkId(self.next_id);
        self.marks.push((id, mark));
        self.revision += 1;
        id
    }

    /// Record the edit a preview has already placed; undo puts `before` back.
    pub fn commit_preview(&mut self, id: MarkId, before: Option<Mark>) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let Some((_, mark)) = self.at(index) else {
            return false;
        };
        self.undo.push(Edit {
            id,
            index,
            before,
            after: Some(mark.clone()),
        });
        self.redo.clear();
        // The mark takes part in the picture from now on, so the canvas has to paint it.
        self.revision += 1;
        true
    }

    /// Take a previewed mark out again, leaving nothing in the history.
    pub fn drop_preview(&mut self, id: MarkId) {
        if let Some(index) = self.index_of(id) {
            self.marks.remove(index);
            self.revision += 1;
        }
    }

    /// One side of an edit: an existing mark is replaced in place, a missing one is inserted
    /// where it sat, and `None` removes it.
    fn place(&mut self, mark: &Option<Mark>, id: MarkId, index: usize) {
        match mark {
            Some(mark) => match self.marks.iter().position(|(existing, _)| *existing == id) {
                Some(at) => self.marks[at].1 = mark.clone(),
                None => self
                    .marks
                    .insert(index.min(self.marks.len()), (id, mark.clone())),
            },
            None => {
                if let Some(at) = self.marks.iter().position(|(existing, _)| *existing == id) {
                    self.marks.remove(at);
                }
            }
        }
    }
}

fn draw_mark(
    list: &mut DrawList,
    owner: ui_core::NodeId,
    id: u64,
    revision: u64,
    mark: &Mark,
    composing: Option<&Composing>,
) {
    let style = &mark.style;
    match &mark.kind {
        MarkKind::Rect(rect) => {
            list.push_rect(
                owner,
                RectPrim {
                    rect: *rect,
                    radius: Corners::ZERO,
                    fill: style.fill.unwrap_or(Color::TRANSPARENT),
                    gradient: None,
                    border: stroke(style),
                    border_gradient: None,
                    notch: None,
                },
            );
        }
        MarkKind::Ellipse(rect) => {
            let radii = (rect.size.w * 0.5, rect.size.h * 0.5);
            list.push_ellipse(
                owner,
                EllipsePrim {
                    bounds: rect.inflate(style.width * 0.5),
                    centre: Point::new(rect.origin.x + radii.0, rect.origin.y + radii.1),
                    radii,
                    thickness: style.width,
                    fill: style.fill.unwrap_or(Color::TRANSPARENT),
                    colour: style.ink,
                },
            );
        }
        MarkKind::Line(line) => {
            segment(list, owner, line.from, line.to, style);
            if line.heads.end {
                for head in arrow_barbs(line.from, line.to, style.width) {
                    segment(list, owner, line.to, head, style);
                }
            }
            if line.heads.start {
                for head in arrow_barbs(line.to, line.from, style.width) {
                    segment(list, owner, line.from, head, style);
                }
            }
        }
        MarkKind::Pen(points) => {
            for pair in points.windows(2) {
                segment(list, owner, pair[0], pair[1], style);
            }
        }
        MarkKind::Bezier(path) => path.paint(list, owner, id, revision, style),
        MarkKind::Text(text) => {
            let composing = composing.map(|composing| (composing.caret, &composing.preedit));
            text.paint(list, owner, style, composing);
        }
    }
}

fn stroke(style: &Style) -> Option<(Color, f32)> {
    (style.width > 0.0).then_some((style.ink, style.width))
}

fn segment(list: &mut DrawList, owner: ui_core::NodeId, from: Point, to: Point, style: &Style) {
    if style.width <= 0.0 {
        return;
    }
    list.push_line(
        owner,
        LinePrim {
            from,
            to,
            thickness: style.width,
            colour: style.ink,
        },
    );
}

/// The two barbs of an arrow's head, each a rotation of the reversed shaft.
pub(crate) fn arrow_barbs(from: Point, to: Point, width: f32) -> [Point; 2] {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (ux, uy) = (dx / length, dy / length);
    let head = line_head(width).min(length);
    let (cos, sin) = (ARROW_SPREAD.cos(), ARROW_SPREAD.sin());
    [(cos, sin), (cos, -sin)].map(|(c, s)| {
        Point::new(
            to.x - (ux * c - uy * s) * head,
            to.y - (ux * s + uy * c) * head,
        )
    })
}

/// The one place a surface coordinate becomes an image pixel and back.
///
/// A pin's node covers `rect` on the surface and the image covers `image` pixels, so the two
/// scales are the same mapping stated twice. Marks and pointer positions pass through here
/// and nowhere else.
#[allow(dead_code)] // The editor converts pointer positions through it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    pub rect: Rect,
    pub image: (u32, u32),
}

#[allow(dead_code)]
impl Space {
    pub fn new(rect: Rect, image: (u32, u32)) -> Self {
        Self { rect, image }
    }

    pub fn scale(&self) -> (f32, f32) {
        (
            self.rect.size.w / self.image.0.max(1) as f32,
            self.rect.size.h / self.image.1.max(1) as f32,
        )
    }

    /// The image pixel under a surface point.
    pub fn to_image(self, at: Point) -> Point {
        let (sx, sy) = self.scale();
        Point::new(
            (at.x - self.rect.origin.x) / sx.max(f32::EPSILON),
            (at.y - self.rect.origin.y) / sy.max(f32::EPSILON),
        )
    }

    /// The surface point an image pixel paints at.
    pub fn to_surface(self, at: Point) -> Point {
        let (sx, sy) = self.scale();
        Point::new(
            self.rect.origin.x + at.x * sx,
            self.rect.origin.y + at.y * sy,
        )
    }

    /// A pixel a pointer landed on, as the pixel's centre: a mark drawn at 0.5 lands inside
    /// the first pixel rather than on the seam between two.
    pub fn snap(&self, at: Point) -> Point {
        let p = self.to_image(at);
        Point::new(p.x.floor() + 0.5, p.y.floor() + 0.5)
    }

    /// A tolerance in image pixels, as surface points.
    pub fn to_surface_length(self, length: f32) -> f32 {
        let (sx, sy) = self.scale();
        length * sx.min(sy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(n: f32) -> Mark {
        Mark {
            kind: MarkKind::Rect(Rect::new(n, n, 10.0, 10.0)),
            style: Style::default(),
        }
    }

    fn add(document: &mut Document, n: f32) -> MarkId {
        let edit = document.insert_edit(mark(n));
        let id = edit.id;
        document.commit(edit);
        id
    }

    fn drawn(document: &Document) -> Vec<MarkId> {
        document.marks().map(|(id, _)| id).collect()
    }

    /// Undo and redo walk the same history in both directions, a restored mark returns to its
    /// paint order, and a new commit after an undo discards the redo branch.
    #[test]
    fn history_walks_back_and_forward_without_reordering() {
        let mut document = Document::new();
        let a = add(&mut document, 1.0);
        let b = add(&mut document, 2.0);
        let c = add(&mut document, 3.0);
        assert_eq!(drawn(&document), vec![a, b, c]);

        let removal = document.remove_edit(b).expect("b is drawn");
        document.commit(removal);
        assert_eq!(drawn(&document), vec![a, c]);

        assert!(document.undo().is_some());
        assert_eq!(
            drawn(&document),
            vec![a, b, c],
            "b came back between a and c"
        );

        assert!(document.undo().is_some());
        assert_eq!(drawn(&document), vec![a, b]);

        assert!(document.redo().is_some());
        assert_eq!(drawn(&document), vec![a, b, c]);

        assert!(document.undo().is_some());
        let d = add(&mut document, 4.0);
        assert_eq!(drawn(&document), vec![a, b, d]);
        assert!(
            document.redo().is_none(),
            "a commit after an undo makes the undone edit unreachable"
        );
        assert_eq!(drawn(&document), vec![a, b, d]);
        assert_eq!(d, MarkId(4));
    }

    /// The revision is the canvas key: every change that moves a pixel bumps it once.
    #[test]
    fn every_change_bumps_the_revision() {
        let mut document = Document::new();
        assert_eq!(document.revision(), 0);
        let id = add(&mut document, 1.0);
        assert_eq!(document.revision(), 1);
        document.undo();
        assert_eq!(document.revision(), 2);
        document.redo();
        assert_eq!(document.revision(), 3);
        let removal = document.remove_edit(id).expect("drawn");
        document.commit(removal);
        assert_eq!(document.revision(), 4);
    }

    /// A session records the depth it opened at, so cancelling takes back its own commits and
    /// leaves the marks that were drawn before it, with nothing left for a redo to restore.
    #[test]
    fn rewinding_a_session_keeps_the_marks_that_were_already_drawn() {
        let mut document = Document::new();
        let before = add(&mut document, 1.0);
        let opened_at = document.depth();
        add(&mut document, 2.0);
        assert_eq!(document.depth(), opened_at + 1);

        document.rewind(opened_at);
        assert_eq!(drawn(&document), vec![before]);
        assert!(document.redo().is_none(), "a cancelled mark stays gone");

        // A session that committed nothing leaves the document exactly as it found it.
        document.rewind(opened_at);
        assert_eq!(drawn(&document), vec![before]);
        assert_eq!(document.depth(), opened_at);
    }

    /// A translated document moves every mark and the history that owns them: the overlay
    /// draws on the whole frame and a pin starts at the crop's corner, so an undo must put a
    /// mark back in the pin's grid rather than where the frame had it.
    #[test]
    fn a_translated_document_moves_its_marks_and_its_history() {
        let mut document = Document::new();
        let mut mark = mark(0.0);
        mark.kind = MarkKind::Rect(Rect::new(10.0, 20.0, 5.0, 6.0));
        let edit = document.insert_edit(mark);
        let id = edit.id;
        document.commit(edit);

        let mut moved = document.translated(-10.0, -20.0);
        let rect = |document: &Document| match document.get(id).map(|mark| &mark.kind) {
            Some(MarkKind::Rect(rect)) => *rect,
            other => panic!("the mark is a rect, got {other:?}"),
        };
        assert_eq!(rect(&moved), Rect::new(0.0, 0.0, 5.0, 6.0));

        moved.undo();
        assert!(moved.is_empty(), "undo still finds the mark it moved");
        moved.redo();
        assert_eq!(
            rect(&moved),
            Rect::new(0.0, 0.0, 5.0, 6.0),
            "redo restores it in the moved grid, not in the frame's"
        );
    }

    /// Marks are drawn in image pixels, never in surface points: a rect at the image's own
    /// corner arrives at that corner, borders included, with no scale applied.
    #[test]
    fn marks_are_drawn_in_image_pixels() {
        let mut document = Document::new();
        let mut mark = mark(0.0);
        mark.kind = MarkKind::Rect(Rect::new(10.0, 20.0, 30.0, 40.0));
        mark.style.width = 2.0;
        let edit = document.insert_edit(mark);
        document.commit(edit);

        let mut list = DrawList::new(Rect::new(0.0, 0.0, 64.0, 64.0));
        document.draw(&mut list);
        let [prim] = list.prims.rects.as_slice() else {
            panic!("one mark must be one rect, got {:?}", list.prims.rects);
        };
        assert_eq!(prim.rect, Rect::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(prim.border, Some((Style::default().ink, 2.0)));
    }

    /// The space is one mapping stated twice: a point survives the round trip, a snapped
    /// pointer lands inside its pixel, and a tolerance shrinks with the picture.
    #[test]
    fn the_space_maps_both_ways() {
        let space = Space::new(Rect::new(100.0, 50.0, 400.0, 200.0), (800, 400));
        let surface = Point::new(300.0, 150.0);
        assert_eq!(space.to_image(surface), Point::new(400.0, 200.0));
        assert_eq!(space.to_surface(space.to_image(surface)), surface);
        assert_eq!(
            space.snap(Point::new(301.0, 150.0)),
            Point::new(402.5, 200.5)
        );
        assert_eq!(space.to_surface_length(4.0), 2.0);
    }

    /// An input method's unfinished bytes join the paragraph where their commit would land,
    /// and the list an export paints stays the committed text.
    #[test]
    fn a_composition_paints_where_its_commit_would_land() {
        let mut document = Document::new();
        let mut text = TextMark::new(Point::ZERO);
        text.text = "abc".to_owned();
        let edit = document.insert_edit(Mark {
            kind: MarkKind::Text(text),
            style: Style::default(),
        });
        let id = edit.id;
        document.commit(edit);

        let composing = Composing {
            id,
            caret: 1,
            preedit: ui_core::Preedit::new("xy", 1),
        };
        let mut shown = DrawList::new(Rect::new(0.0, 0.0, 64.0, 64.0));
        document.draw_composing(&mut shown, Some(&composing));
        let [prim] = shown.prims.texts.as_slice() else {
            panic!("one mark must be one text, got {:?}", shown.prims.texts);
        };
        assert_eq!(prim.block.runs[0].text, "axybc");
        assert_eq!(prim.composing, Some(1..3));
        assert_ne!(
            document.version(Some(&composing)),
            document.version(None),
            "the canvas must repaint while the composition moves"
        );

        let mut committed = DrawList::new(Rect::new(0.0, 0.0, 64.0, 64.0));
        document.draw(&mut committed);
        let [prim] = committed.prims.texts.as_slice() else {
            panic!("one mark must be one text, got {:?}", committed.prims.texts);
        };
        assert_eq!(prim.block.runs[0].text, "abc");
        assert_eq!(prim.composing, None);
    }
}
