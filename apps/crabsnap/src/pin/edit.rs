//! The annotation session: what the pointer does to a bitmap pin's document.
//!
//! The card stays the pin's node; the session adds chrome in surface points and turns drags
//! inside the picture into marks in image pixels. Coordinates cross [`Space`] exactly here,
//! so the tools and the document never see a surface point.

use ui_core::cx::Cx;
use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Point, Rect};
use ui_core::layout::SizeClass;
use ui_core::menu::MenuItem;
use ui_core::{CursorShape, Id, Key, KeyChord, NodeKind, Sense, ShortcutId, TextAlign};
use ui_i18n::Catalog;
use ui_platform::Container;

use crate::capture::Cropped;
use crate::document::path::{PathAnchor, PathMark};
use crate::document::text::{MAX_PADDING, MAX_RADIUS, MAX_SIZE, MIN_SIZE};
use crate::document::{
    Composing, Document, Edit, Heads, Line, Mark, MarkId, MarkKind, Sides, Space, Style, TextMark,
};
use crate::loupe;
use crate::toolbar;

/// The alignments the wheel and the parameter chip walk, in the order [`ALIGN`] names them.
const ALIGNED: [TextAlign; 3] = [TextAlign::Start, TextAlign::Center, TextAlign::End];

/// The drag band inside the picture's border, in surface points. A press here moves the pin.
const BAND: f32 = 8.0;
/// The editing frame's stroke, in surface points.
const BORDER: f32 = 1.0;
/// One resize handle's side, in surface points.
const HANDLE: f32 = 9.0;
/// The smallest drag that becomes a mark, in image pixels: a slip of the hand is not a mark.
const MIN_MARK: f32 = 2.0;
/// A fill picked from the palette is translucent, so the picture stays readable under it.
const FILL_ALPHA: f32 = 0.42;
/// The stroke widths the wheel walks between, in image pixels.
const MIN_WIDTH: f32 = 1.0;
const MAX_WIDTH: f32 = 24.0;
/// The heads the arrow's parameter offers, in the order the wheel cycles them.
const HEADS: [&str; 2] = ["toolbar-head-end", "toolbar-head-both"];
/// The text alignments the parameter cycles, in [`TextAlign`] order.
const ALIGN: [&str; 3] = ["toolbar-align-left", "toolbar-align-center", "toolbar-align-right"];
/// Chords a path under construction owns before the app's confirm/cancel bindings.
const PATH_FINISH: ShortcutId = ShortcutId("crabsnap.path.finish");
const PATH_CANCEL: ShortcutId = ShortcutId("crabsnap.path.cancel");

/// What a drag inside the picture draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Rect,
    Ellipse,
    Line,
    Arrow,
    Pen,
    Bezier,
    Text,
}

/// What a toolbar mark draws, or `None` for the cursor, which draws nothing.
fn tool_of(mark: toolbar::Mark) -> Option<Tool> {
    match mark {
        toolbar::Mark::Cursor => None,
        toolbar::Mark::Rect => Some(Tool::Rect),
        toolbar::Mark::Ellipse => Some(Tool::Ellipse),
        toolbar::Mark::Line => Some(Tool::Line),
        toolbar::Mark::Arrow => Some(Tool::Arrow),
        toolbar::Mark::Pen => Some(Tool::Pen),
        toolbar::Mark::Bezier => Some(Tool::Bezier),
        toolbar::Mark::Text => Some(Tool::Text),
    }
}

fn mark_of(tool: Tool) -> toolbar::Mark {
    match tool {
        Tool::Rect => toolbar::Mark::Rect,
        Tool::Ellipse => toolbar::Mark::Ellipse,
        Tool::Line => toolbar::Mark::Line,
        Tool::Arrow => toolbar::Mark::Arrow,
        Tool::Pen => toolbar::Mark::Pen,
        Tool::Bezier => toolbar::Mark::Bezier,
        Tool::Text => toolbar::Mark::Text,
    }
}

/// The family a mark belongs to, when it is one a family collapses.
fn family_of(mark: toolbar::Mark) -> Option<toolbar::Family> {
    match mark {
        toolbar::Mark::Rect | toolbar::Mark::Ellipse => Some(toolbar::Family::Shape),
        toolbar::Mark::Line | toolbar::Mark::Arrow => Some(toolbar::Family::Line),
        toolbar::Mark::Pen | toolbar::Mark::Bezier => Some(toolbar::Family::Pen),
        toolbar::Mark::Cursor | toolbar::Mark::Text => None,
    }
}

/// A mark being drawn, in image pixels.
struct Drawing {
    from: Point,
    to: Point,
    points: Vec<Point>,
}

/// A mark being moved, with the state the press found it in.
struct Moving {
    id: MarkId,
    before: Mark,
    from: Point,
}

/// One handle's drag: which edges it moves and the state the press found.
struct Resizing {
    id: MarkId,
    before: Mark,
    from: Rect,
    edges: (bool, bool, bool, bool),
}

/// A text mark being typed. The mark itself lives in the document, measured this build, so
/// the field draws the very text and frame the canvas would: there is no second copy to keep
/// in step, only the id of the one that is open.
struct TextEdit {
    id: MarkId,
    /// What undo restores; `None` when this session created the mark.
    before: Option<Mark>,
}

/// A mark whose own points are its controls; the eight box handles stand down while one
/// is open, so one mark never offers two sets of controls for the same press.
struct Nodes {
    id: MarkId,
    /// The mark the mode opened on: a point drag is one step from here.
    before: Mark,
    dragging: Option<usize>,
}

/// A path whose anchors are still being placed. The mark lives in the document as a
/// preview, so the canvas, the loupe and export share one drawing throughout creation.
#[derive(Clone, Copy, Debug)]
struct PathEdit {
    id: MarkId,
}

pub(crate) struct Session {
    /// What a drag inside the picture draws; `None` is the cursor, which only selects.
    tool: Option<Tool>,
    /// The child each annotation family shows, in [`toolbar::Family::ALL`] order.
    family: [usize; 3],
    /// The colour field whose picker is open.
    open: Option<toolbar::Field>,
    /// The eyedropper is waiting for the next press inside the picture.
    pick: bool,
    /// The ends an arrow is drawn with.
    heads: Heads,
    selected: Option<MarkId>,
    drawing: Option<Drawing>,
    /// The mark `drawing` would land, previewed in the document: the canvas and the disc
    /// paint the marks, so a stroke in flight has to be one of them or neither shows it.
    draft: Option<MarkId>,
    moving: Option<Moving>,
    resizing: Option<Resizing>,
    text: Option<TextEdit>,
    nodes: Option<Nodes>,
    path: Option<PathEdit>,
    /// Which newly placed anchor a live press is pulling into a handle.
    path_dragging: Option<usize>,
    /// The previous build saw the path's interaction node pressed.
    path_pressed: bool,
    /// What the next mark is drawn with, and what a restyle applies.
    style: Style,
    /// The frame and glyph dressing the next text mark takes, empty of text: the document's
    /// mark owns what is drawn, this owns what the next one starts from.
    text_style: TextMark,
    /// The press landed on the band, so the canvas moves the pin rather than the editor.
    band: bool,
    /// This surface draws the picture's frame, so it has a band for the canvas to drag. The
    /// overlay's selection is its own frame: a press near its edge draws like any other.
    frame: bool,
    /// The card's node id from the last build: every chrome id derives from it.
    node: Option<Id>,
    /// The loupe the secondary button holds up.
    loupe: loupe::Loupe,
}

/// What one build's chrome is drawn against.
pub(crate) struct Chrome<'a> {
    /// The surface's root id: every chrome id derives from it.
    pub node: Id,
    pub space: Space,
    pub document: &'a Document,
    pub image: &'a Cropped,
    pub catalog: &'a Catalog,
    /// The surface draws the editing frame; the overlay wears one of its own.
    pub frame: bool,
    /// This surface shows the session's in-picture loupe.
    pub loupe: bool,
    /// The secondary button is held on this surface.
    pub held: bool,
}

/// Where the drag's box has its sides now. A side no grip holds keeps its old coordinate,
/// so the side opposite the grip stays exactly where it was even when the grip crosses it.
fn box_to(drag: &Resizing, at: Point) -> Sides {
    let mut sides = Sides::of(drag.from);
    if drag.edges.0 {
        sides.left = at.x;
    }
    if drag.edges.1 {
        sides.top = at.y;
    }
    if drag.edges.2 {
        sides.right = at.x;
    }
    if drag.edges.3 {
        sides.bottom = at.y;
    }
    if drag.edges.0 {
        sides.left = sliver(sides.right, sides.left);
    }
    if drag.edges.1 {
        sides.top = sliver(sides.bottom, sides.top);
    }
    if drag.edges.2 {
        sides.right = sliver(sides.left, sides.right);
    }
    if drag.edges.3 {
        sides.bottom = sliver(sides.top, sides.bottom);
    }
    sides
}

/// A dragged side landing on the one opposite leaves nothing to grab, so it stops a sliver
/// past: the mark keeps a size to drag back, on the side of the edge it had reached.
fn sliver(fixed: f32, moving: f32) -> f32 {
    match moving - fixed {
        span if span.abs() >= MIN_MARK => moving,
        span if span > 0.0 => fixed + MIN_MARK,
        _ => fixed - MIN_MARK,
    }
}

/// The target box with the drag's proportions kept, for a held Shift: a corner scales about
/// the corner opposite it, a side spreads the untouched axis about the middle.
fn proportional(from: Rect, sides: Sides, edges: (bool, bool, bool, bool)) -> Sides {
    if from.is_empty() {
        return sides;
    }
    let (horizontal, vertical) = (edges.0 || edges.2, edges.1 || edges.3);
    let scale_x = (sides.right - sides.left) / from.size.w;
    let scale_y = (sides.bottom - sides.top) / from.size.h;
    let scale = match (horizontal, vertical) {
        (false, false) => return sides,
        (true, false) => scale_x,
        (false, true) => scale_y,
        // A corner moves both axes: the one the hand dragged further decides the factor.
        (true, true) if (scale_x - 1.0).abs() >= (scale_y - 1.0).abs() => scale_x,
        (true, true) => scale_y,
    };
    let across_x = if edges.0 {
        from.max_x()
    } else if edges.2 {
        from.min_x()
    } else {
        from.center().x
    };
    let across_y = if edges.1 {
        from.max_y()
    } else if edges.3 {
        from.min_y()
    } else {
        from.center().y
    };
    Sides {
        left: across_x + (from.min_x() - across_x) * scale,
        top: across_y + (from.min_y() - across_y) * scale,
        right: across_x + (from.max_x() - across_x) * scale,
        bottom: across_y + (from.max_y() - across_y) * scale,
    }
}

impl Session {
    pub(crate) fn new() -> Self {
        Self {
            tool: None,
            family: [0; 3],
            open: None,
            pick: false,
            heads: Heads::END,
            selected: None,
            drawing: None,
            draft: None,
            moving: None,
            resizing: None,
            text: None,
            nodes: None,
            path: None,
            path_dragging: None,
            path_pressed: false,
            style: Style::default(),
            text_style: TextMark::new(Point::ZERO),
            band: false,
            frame: false,
            node: None,
            loupe: loupe::Loupe::new(),
        }
    }

    /// The chrome root this build: the frame, the selection and its controls.
    pub(crate) fn chrome(&mut self, cx: &mut Cx<'_>, chrome: Chrome<'_>) -> Container {
        let Chrome {
            node,
            space,
            document,
            image,
            catalog,
            frame,
            loupe,
            held,
        } = chrome;
        self.node = Some(node);
        self.frame = frame;
        self.loupe.set_held(held && loupe);
        let drawn = space.rect;
        let mut own = ui_core::Decl::container(chrome_id(node), NodeKind::Stack)
            .size(SizeClass::fixed(drawn.size.w, drawn.size.h))
            .offset(drawn.origin.x, drawn.origin.y)
            // A wheel step over the chrome is the loupe's; a press is the picture's.
            .sense(Sense::PASS_THROUGH | Sense::SCROLL);
        let mut shapes = Vec::new();
        // The frame hugs the picture from the outside: a border paints inward, so a rect grown
        // by the stroke puts the whole line in the band just past the picture and none of it on
        // a pixel. It is drawn here rather than on the card so a picture keeps every pixel it
        // has, the same way the skin ring outside the card does.
        if frame {
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    -BORDER,
                    -BORDER,
                    drawn.size.w + BORDER * 2.0,
                    drawn.size.h + BORDER * 2.0,
                ),
                corners: Corners::ZERO,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((Color::from_rgba8(255, 255, 255, 200), BORDER)),
            });
        }
        // While a field is over its mark the field is the control: an outline and eight
        // handles around a box being typed into would sit on top of the text.
        if let Some(mark) = self.selected.filter(|_| self.text.is_none()).and_then(|id| document.get(id)) {
            let box_ = surface_box(mark.bounds(), space);
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    box_.origin.x - drawn.origin.x,
                    box_.origin.y - drawn.origin.y,
                    box_.size.w,
                    box_.size.h,
                ),
                corners: Corners::ZERO,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((Color::from_rgba8(80, 160, 255, 255), 1.0)),
            });
        }
        let path_active = self.path.is_some();
        let path_shown = self.bezier_shown();
        own.shapes = shapes;
        let mut chrome = Container::from_decl(own);
        if !path_shown {
            for (index, mark) in document.marks().enumerate() {
                if self.path.as_ref().is_some_and(|edit| {
                    document
                        .index_of(edit.id)
                        .is_some_and(|at| at == index)
                }) {
                    continue;
                }
                let box_ = surface_box(mark.1.bounds(), space);
                let delete = catalog.text_by_id("pin-edit-delete").to_string();
                let mut rows = Vec::new();
                if matches!(mark.1.kind, MarkKind::Text { .. }) {
                    let edit = catalog.text_by_id("pin-edit-edit-text").to_string();
                    rows.push(MenuItem::new(Id::str("pin-edit/edit-text"), edit));
                }
                if matches!(mark.1.kind, MarkKind::Pen(_)) {
                    let points = catalog.text_by_id("pin-edit-edit-points").to_string();
                    rows.push(MenuItem::new(Id::str("pin-edit/edit-points"), points));
                }
                rows.push(MenuItem::new(Id::str("pin-edit/delete"), delete));
                chrome = chrome.child(Box::new(
                    Container::new(mark_id(node, index), NodeKind::Box)
                        .size(SizeClass::fixed(box_.size.w.max(1.0), box_.size.h.max(1.0)))
                        .offset(
                            box_.origin.x - drawn.origin.x,
                            box_.origin.y - drawn.origin.y,
                        )
                        .sense(Sense::DRAG | Sense::HOVER)
                        .cursor(CursorShape::Move)
                        .menu(rows),
                ));
            }
        }
        if let Some(edit) = &self.text
            && let Some(mark) = document.get(edit.id).cloned()
            && let MarkKind::Text(text) = &mark.kind
        {
            let composing = self.composing(cx, node, document);
            let shown = composing.as_ref().map_or(text, |composing| &composing.shown);
            chrome = chrome.child_decl(text_field(
                cx,
                node,
                space,
                drawn,
                FieldText {
                    committed: text,
                    shown,
                },
                mark.style.ink,
                catalog,
            ));
        }
        if let Some(points) = self.nodes.as_ref().and_then(|nodes| {
            document.get(nodes.id).and_then(|mark| match &mark.kind {
                MarkKind::Pen(points) => Some(points.clone()),
                _ => None,
            })
        }) {
            // Round controls for a stroke's points: a pen's nodes and a box's handles mean
            // different things, so they must not look alike.
            for (index, point) in points.iter().enumerate() {
                let at = space.to_surface(*point);
                chrome = chrome.child(Box::new(
                    Container::new(anchor_id(node, index), NodeKind::Box)
                        .size(SizeClass::fixed(HANDLE, HANDLE))
                        .offset(
                            at.x - drawn.origin.x - HANDLE * 0.5,
                            at.y - drawn.origin.y - HANDLE * 0.5,
                        )
                        .sense(Sense::DRAG | Sense::HOVER)
                        .cursor(CursorShape::Move)
                        .background(
                            Color::from_rgba8(255, 255, 255, 255),
                            Corners::all(HANDLE * 0.5),
                        )
                        .map(|decl| decl.border(Color::from_rgba8(20, 20, 20, 200), 1.0)),
                ));
            }
        }
        if self.nodes.is_none()
            && self.text.is_none()
            && let Some(mark) = self.selected.and_then(|id| document.get(id))
        {
            let box_ = surface_box(mark.bounds(), space);
            for (index, (at, cursor)) in handles(box_).into_iter().enumerate() {
                chrome = chrome.child(Box::new(
                    Container::new(handle_id(node, index), NodeKind::Box)
                        .size(SizeClass::fixed(HANDLE, HANDLE))
                        .offset(
                            at.x - drawn.origin.x - HANDLE * 0.5,
                            at.y - drawn.origin.y - HANDLE * 0.5,
                        )
                        .sense(Sense::DRAG | Sense::HOVER)
                        .cursor(cursor)
                        .background(Color::from_rgba8(255, 255, 255, 255), Corners::all(2.0))
                        .map(|decl| decl.border(Color::from_rgba8(20, 20, 20, 200), 1.0)),
                ));
            }
        }
        if path_shown {
            let mut control_shapes = Vec::new();
            if let Some(mark) = self
                .path
                .as_ref()
                .and_then(|edit| document.get(edit.id))
                && let MarkKind::Bezier(path) = &mark.kind
            {
                control_shapes.extend(path_controls(path, space, drawn.origin));
            }
            let control_node = node.with("edit-path-controls");
            let mut controls_decl = ui_core::Decl::container(control_node, NodeKind::Box)
                .size(SizeClass::fixed(drawn.size.w, drawn.size.h))
                .sense(Sense::PASS_THROUGH);
            controls_decl.shapes = control_shapes;
            chrome = chrome.child_decl(controls_decl);
            let mut decl = ui_core::Decl::container(path_id(node), NodeKind::Box)
                .size(SizeClass::fixed(drawn.size.w, drawn.size.h))
                .sense(Sense::DRAG | Sense::CLICK | Sense::SECONDARY | Sense::FOCUSABLE)
                .focus_stop(true)
                .cursor(CursorShape::Crosshair);
            if path_active {
                let rows = vec![
                    MenuItem::new(
                        Id::str("pin-edit/finish-path"),
                        catalog.text_by_id("pin-edit-finish-path").to_string(),
                    ),
                    MenuItem::new(
                        Id::str("pin-edit/discard-path"),
                        catalog.text_by_id("pin-edit-discard-path").to_string(),
                    ),
                ];
                decl = decl
                    .menu(rows)
                    .shortcut(
                        KeyChord::new(Key::Enter, ui_core::Modifiers::NONE),
                        PATH_FINISH,
                    )
                    .shortcut(
                        KeyChord::new(Key::Escape, ui_core::Modifiers::NONE),
                        PATH_CANCEL,
                    );
                if cx.focused() != Some(path_id(node)) {
                    cx.request_focus(path_id(node));
                }
            }
            chrome = chrome.child_decl(decl);
        }
        if let Some(pointer) = cx.pointer()
            && loupe
            // A mark drag never raises the disc; the switch and the secondary button do.
            && self.loupe.visible(false)
            && let Some(decl) = self.loupe_node(cx, node, space, document, image, pointer)
        {
            chrome = chrome.child_decl(decl);
        }
        chrome
    }

    /// The pen family owns the whole picture while its Bezier tool is armed or a path
    /// is under construction: no existing mark may take a press meant for an anchor.
    fn bezier_shown(&self) -> bool {
        self.tool == Some(Tool::Bezier) || self.path.is_some()
    }

    fn path_pointer(
        &mut self,
        cx: &mut Cx<'_>,
        document: &mut Document,
        node: Id,
        space: Space,
    ) -> bool {
        let shown = self.bezier_shown();
        let pressed = shown
            && (cx.pressed_within(path_id(node))
                || cx.pressed_within(node.with("edit-path-controls")));
        let rising = pressed && !self.path_pressed;
        self.path_pressed = pressed;
        if !pressed {
            // A tap's release ends its handle drag; the next press must place another
            // anchor rather than pulling the previous one's handle again.
            self.path_dragging = None;
            self.band = false;
        }
        if !shown {
            return false;
        }
        if let Some(index) = self.path_dragging
            && pressed
            && let Some(pointer) = cx.pointer()
        {
            return self.drag_path_handle(document, index, space.to_image(pointer));
        }
        if rising
            && let Some(origin) = cx.press_origin()
            && self.tool == Some(Tool::Bezier)
        {
            let at = space.snap(origin);
            return self.path_press(document, space, at);
        }
        false
    }

    fn path_press(&mut self, document: &mut Document, space: Space, at: Point) -> bool {
        if self.tool != Some(Tool::Bezier) {
            return false;
        }
        let Some(edit) = self.path.as_ref().copied() else {
            let mark = Mark {
                kind: MarkKind::Bezier(PathMark::default()),
                style: self.style.clone(),
            };
            let id = document.insert_preview(mark);
            self.path = Some(PathEdit { id });
            self.selected = Some(id);
            let _ = self.append_path_anchor(document, at);
            return true;
        };
        let Some(mark) = document.get(edit.id).cloned() else {
            self.path = None;
            return false;
        };
        let MarkKind::Bezier(path) = &mark.kind else {
            return false;
        };
        if path.anchors.len() >= 2
            && !path.closed
            && let Some(first) = path.anchors.first()
            && distance(space.to_surface(first.at), space.to_surface(at)) <= HANDLE
        {
            let mut closed = path.clone();
            closed.closed = true;
            document.preview(
                edit.id,
                Mark {
                    kind: MarkKind::Bezier(closed),
                    style: mark.style,
                },
            );
            self.finish_path(document);
            return true;
        }
        self.append_path_anchor(document, at)
    }

    fn append_path_anchor(&mut self, document: &mut Document, at: Point) -> bool {
        let Some(edit) = self.path.as_ref().copied() else {
            return false;
        };
        let Some(mark) = document.get(edit.id).cloned() else {
            return false;
        };
        let MarkKind::Bezier(mut path) = mark.kind else {
            return false;
        };
        if path.closed {
            return false;
        }
        path.anchors.push(PathAnchor::new(at));
        self.path_dragging = Some(path.anchors.len() - 1);
        document.preview(
            edit.id,
            Mark {
                kind: MarkKind::Bezier(path),
                style: mark.style,
            },
        );
        true
    }

    fn drag_path_handle(&mut self, document: &mut Document, index: usize, at: Point) -> bool {
        let Some(edit) = self.path.as_ref().copied() else {
            return false;
        };
        let Some(mark) = document.get(edit.id).cloned() else {
            return false;
        };
        let MarkKind::Bezier(mut path) = mark.kind else {
            return false;
        };
        let Some(anchor) = path.anchors.get_mut(index) else {
            return false;
        };
        if anchor.handle_out == at {
            return false;
        }
        anchor.handle_out = at;
        document.preview(
            edit.id,
            Mark {
                kind: MarkKind::Bezier(path),
                style: mark.style,
            },
        );
        true
    }

    pub(crate) fn finish_path(&mut self, document: &mut Document) -> bool {
        let Some(edit) = self.path.take() else {
            return false;
        };
        self.path_dragging = None;
        self.path_pressed = false;
        let drawn = document
            .get(edit.id)
            .is_some_and(|mark| match &mark.kind {
                MarkKind::Bezier(path) => !path.anchors.is_empty(),
                _ => false,
            });
        if drawn {
            document.commit_preview(edit.id, None);
            self.selected = Some(edit.id);
            return true;
        }
        document.drop_preview(edit.id);
        if self.selected == Some(edit.id) {
            self.selected = None;
        }
        true
    }

    fn discard_path(&mut self, document: &mut Document) -> bool {
        let Some(edit) = self.path.take() else {
            return false;
        };
        self.path_dragging = None;
        self.path_pressed = false;
        document.drop_preview(edit.id);
        if self.selected == Some(edit.id) {
            self.selected = None;
        }
        true
    }

    /// The loupe over this frame, when the button holds it up.
    fn loupe_node(
        &self,
        cx: &mut Cx<'_>,
        node: Id,
        space: Space,
        document: &Document,
        image: &Cropped,
        pointer: Point,
    ) -> Option<ui_core::Decl> {
        let (sx, _) = space.scale();
        if sx <= f32::EPSILON {
            return None;
        }
        // The disc counts image pixels; the guide is the mark being drawn or the one the
        // session has selected, both already in that space.
        let aim = loupe::Aim::at(space.to_image(pointer));
        let guide = self.drawing_bounds(document).or_else(|| {
            self.selected
                .and_then(|id| document.get(id))
                .map(|mark| mark.bounds())
        });
        let surface = Rect::new(0.0, 0.0, cx.width() as f32, cx.height() as f32);
        // The disc shows the picture with the marks in it, so it reads a raster of both:
        // the window is a few cells, not the picture.
        let window = loupe::window(cx.scale(), aim, self.loupe.zoom());
        let marked = super::content::window(image, document, window);
        let source = loupe::Source::of_window(
            &marked,
            (window.origin.x as i32, window.origin.y as i32),
        );
        loupe::build(
            cx,
            loupe::View {
                state: &self.loupe,
                source: &source,
                aim,
                guide,
                pointer,
                surface,
                origin: space.rect.origin,
                id: node.with("loupe"),
            },
        )
    }

    /// The box a live drag covers, in image pixels.
    fn drawing_bounds(&self, document: &Document) -> Option<Rect> {
        if let Some(edit) = &self.path
            && let Some(mark) = document.get(edit.id)
        {
            return Some(mark.bounds());
        }
        let drawing = self.drawing.as_ref()?;
        let xs = drawing
            .points
            .iter()
            .map(|p| p.x)
            .chain([drawing.from.x, drawing.to.x]);
        let ys = drawing
            .points
            .iter()
            .map(|p| p.y)
            .chain([drawing.from.y, drawing.to.y]);
        let (x0, x1) = (
            xs.clone().fold(f32::INFINITY, f32::min),
            xs.fold(f32::NEG_INFINITY, f32::max),
        );
        let (y0, y1) = (
            ys.clone().fold(f32::INFINITY, f32::min),
            ys.fold(f32::NEG_INFINITY, f32::max),
        );
        Some(Rect::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)))
    }

    /// A wheel step while the loupe is up: one notch is one zoom level. The switch and the
    /// secondary button raise the same disc, so the wheel follows what the user sees.
    pub(crate) fn wheel(&mut self, value: f64) -> bool {
        self.loupe.wheel(value, self.loupe.visible(false))
    }

    /// Handle this build's pointer; returns whether the document changed.
    pub(crate) fn pointer(
        &mut self,
        cx: &mut Cx<'_>,
        document: &mut Document,
        node: Id,
        space: Space,
    ) -> bool {
        let mut changed = false;
        let path_shown = self.bezier_shown();
        let path_changed = self.path_pointer(cx, document, node, space);
        if path_shown {
            return path_changed;
        }
        // A pen's own points come first: while its mode is open they are the only controls
        // inside the picture, so a press on one must never be read as a box handle.
        let anchors = (0..self.node_points(document))
            .find(|index| cx.dragging_within(anchor_id(node, *index)));
        if let Some(index) = anchors {
            let Some(pointer) = cx.pointer() else {
                return changed;
            };
            // `snap` takes the surface point and converts once itself.
            let at = space.snap(pointer);
            changed |= self.drag_point(document, index, at);
            return changed;
        }
        changed |= self.finish_point(document);

        let resizing = (self.nodes.is_none())
            .then(|| (0..8).find(|index| cx.dragging_within(handle_id(node, *index))))
            .flatten();
        let moving = if self.nodes.is_none() {
            (0..document.len()).find(|index| cx.dragging_within(mark_id(node, *index)))
        } else {
            None
        };
        let drawing = cx.dragging_within(content_id(node));
        if self.nodes.is_some() {
            // The mode owns the pointer: nothing inside the picture starts a gesture, and
            // only the band still moves the pin.
            if let (true, Some(origin)) = (drawing, cx.press_origin()) {
                self.band = self.frame && in_band(space.to_image(origin), space);
            } else {
                self.band = false;
            }
            return changed;
        }

        if let Some(index) = resizing {
            let Some(pointer) = cx.pointer() else {
                return changed;
            };
            let at = space.to_image(pointer);
            if self.resizing.is_none() {
                self.resizing = self.begin_resize(index, document);
            }
            if let Some(drag) = &mut self.resizing {
                // The keys are read every frame, so either one can be taken or let go
                // halfway through a drag and the mark follows at once.
                let modifiers = cx.modifiers();
                let mut box_ = box_to(drag, at);
                if modifiers.shift {
                    box_ = proportional(drag.from, box_, drag.edges);
                }
                let after = drag
                    .before
                    .resized(drag.from, box_, drag.edges, modifiers.ctrl);
                document.preview(drag.id, after);
                changed = true;
            }
            return changed;
        }
        changed |= self.finish_resize(document);

        if let Some(index) = moving {
            let Some(pointer) = cx.pointer() else {
                return changed;
            };
            let at = space.to_image(pointer);
            if self.moving.is_none() {
                let Some((id, mark)) = document.at(index) else {
                    return changed;
                };
                let from = cx
                    .press_origin()
                    .map_or(at, |origin| space.to_image(origin));
                self.selected = Some(id);
                self.moving = Some(Moving {
                    id,
                    before: mark.clone(),
                    from,
                });
            }
            if let Some(drag) = &self.moving {
                let after = drag
                    .before
                    .translated(at.x - drag.from.x, at.y - drag.from.y);
                document.preview(drag.id, after);
                changed = true;
            }
            return changed;
        }
        changed |= self.finish_move(document);

        if drawing {
            let (Some(origin), Some(pointer)) = (cx.press_origin(), cx.pointer()) else {
                return changed;
            };
            let from = space.to_image(origin);
            let to = space.to_image(pointer);
            if self.drawing.is_none() {
                if self.frame && in_band(from, space) {
                    self.band = true;
                    return changed;
                }
                if self.tool == Some(Tool::Text) {
                    // A press places a text mark and opens its field; there is nothing to
                    // drag, so the stroke the pointer makes is ignored. The mark lands on
                    // the pixel the press did, so the field's node and the mark agree.
                    changed |= self.finish_text(document);
                    changed |= self.begin_text(document, space.snap(origin));
                    return changed;
                }
                self.drawing = Some(Drawing {
                    from,
                    to: from,
                    points: vec![from],
                });
            }
            if let Some(drawing) = &mut self.drawing {
                drawing.to = to;
                let last = drawing.points.last().copied().unwrap_or(from);
                if self.tool == Some(Tool::Pen) && distance(last, to) >= 1.0 {
                    drawing.points.push(to);
                }
            }
            // The stroke joins the document as a preview the moment it has a shape: the
            // canvas, the disc and the file all read the document, so none of them can
            // miss what the hand is drawing.
            let draft = self
                .drawing
                .as_ref()
                .and_then(|drawing| self.drawing_mark(drawing));
            if let Some(mark) = draft {
                match self.draft {
                    Some(id) => document.preview(id, mark),
                    None => self.draft = Some(document.insert_preview(mark)),
                }
                changed = true;
            }
        } else {
            self.band = false;
            if let Some(drawing) = self.drawing.take() {
                changed |= self.commit_drawing(document, drawing);
            }
        }
        changed
    }

    pub(crate) fn band_drag(&self) -> bool {
        self.band
    }

    /// A tool is armed: presses inside the picture draw instead of selecting.
    pub(crate) fn armed(&self) -> bool {
        self.tool.is_some()
    }

    /// What the bar should show while this session is open.
    pub(crate) fn bar_state(&self) -> toolbar::State {
        toolbar::State {
            armed: self.tool.map(mark_of),
            family: self.family,
            loupe: if self.loupe.toggled() {
                toolbar::LoupeState::On
            } else if self.loupe.held() {
                toolbar::LoupeState::Temporary
            } else {
                toolbar::LoupeState::Off
            },
            drawing: self.drawing.is_some() || self.path.is_some(),
        }
    }

    /// The armed tool's parameters, in the order the parameter row shows them.
    pub(crate) fn params(&self) -> Vec<toolbar::Param> {
        if self.text_mode() {
            return self.text_params();
        }
        let Some(tool) = self.tool else {
            return Vec::new();
        };
        let fillable = matches!(tool, Tool::Rect | Tool::Ellipse);
        let mut out = vec![
            toolbar::Param::Paint {
                ink: self.style.ink,
                fill: self.style.fill,
                fillable,
                open: self.open == Some(toolbar::Field::Stroke),
            },
            toolbar::Param::Number {
                field: toolbar::Field::Width,
                value: self.style.width,
                message: "toolbar-width",
            },
        ];
        if tool == Tool::Arrow {
            out.push(toolbar::Param::Choice {
                field: toolbar::Field::Heads,
                message: "toolbar-heads",
                options: &HEADS,
                current: usize::from(self.heads.start),
            });
        }
        out
    }

    /// Whether the text tool, or an open field, owns the parameters. Both dress text, so a
    /// press on a chip belongs to the same place whichever armed the field.
    fn text_mode(&self) -> bool {
        self.tool == Some(Tool::Text) || self.text.is_some()
    }

    /// The text marks' parameters: ink and wash in the paint chip, the glyph outline and the
    /// frame's hairline as chips of their own, then the size, the padding, the corner and
    /// the alignment.
    fn text_params(&self) -> Vec<toolbar::Param> {
        let text = &self.text_style;
        vec![
            toolbar::Param::Paint {
                ink: self.style.ink,
                fill: text.background,
                fillable: true,
                open: self.open == Some(toolbar::Field::Stroke),
            },
            toolbar::Param::Swatch {
                field: toolbar::Field::Outline,
                colour: text.outline,
                open: self.open == Some(toolbar::Field::Outline),
            },
            toolbar::Param::Swatch {
                field: toolbar::Field::Border,
                colour: text.border,
                open: self.open == Some(toolbar::Field::Border),
            },
            toolbar::Param::Number {
                field: toolbar::Field::Size,
                value: text.size,
                message: "toolbar-size",
            },
            toolbar::Param::Number {
                field: toolbar::Field::Padding,
                value: text.padding,
                message: "toolbar-padding",
            },
            toolbar::Param::Number {
                field: toolbar::Field::Radius,
                value: text.radius,
                message: "toolbar-radius",
            },
            toolbar::Param::Choice {
                field: toolbar::Field::Align,
                message: "toolbar-align",
                options: &ALIGN,
                current: align_index(text.align),
            },
        ]
    }

    /// A mark is being drawn, so the bar steps out of the way.
    pub(crate) fn drawing(&self) -> bool {
        self.drawing.is_some() || self.path.is_some()
    }

    /// The eyedropper is waiting for the next press inside the picture.
    pub(crate) fn picking(&self) -> bool {
        self.pick
    }

    /// The colour the eyedropper landed on, applied to the next mark's stroke.
    pub(crate) fn picked(&mut self, document: &mut Document, colour: Option<[u8; 4]>) -> bool {
        if !self.pick {
            return false;
        }
        self.pick = false;
        let Some([red, green, blue, _]) = colour else {
            return true;
        };
        let style = Style {
            ink: Color::from_rgba8(red, green, blue, 255),
            ..self.style.clone()
        };
        self.restyle(document, style)
    }

    /// A press the chrome reported: the bar's controls, a mark's menu, or the typing field.
    pub(crate) fn change(
        &mut self,
        id: Id,
        change: ui_core::change::Change,
        document: &mut Document,
    ) -> bool {
        let Some(node) = self.node else {
            return false;
        };
        match change {
            ui_core::change::Change::Value(value) => self.wheel_or_adjust(id, value, document),
            ui_core::change::Change::Text(next) => {
                if id != text_field_id(node) {
                    return false;
                }
                self.update_text(document, next)
            }
            // A press outside the picker, or Escape, is the app's word to close it.
            ui_core::change::Change::Dismissed => {
                let was = self.open.is_some() || self.pick;
                self.open = None;
                self.pick = false;
                was
            }
            ui_core::change::Change::Activated => self.activate(id, document),
            ui_core::change::Change::MenuItem(item) => self.menu(id, item, document),
            ui_core::change::Change::Shortcut(shortcut) if id == path_id(node) => {
                match shortcut {
                    PATH_FINISH => self.finish_path(document),
                    PATH_CANCEL => self.discard_path(document),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The wheel walks an annotation value, cycles a family or zooms the loupe.
    fn wheel_or_adjust(&mut self, id: Id, value: f64, document: &mut Document) -> bool {
        let step = if value < 0.0 { -1.0 } else { 1.0 };
        if self.text_mode()
            && let Some(next) = text_wheel(&self.text_style, id, step)
        {
            return self.restyle_text(document, next);
        }
        if id == toolbar::field(toolbar::Field::Width) {
            let width = (self.style.width + step).clamp(MIN_WIDTH, MAX_WIDTH);
            if (width - self.style.width).abs() < f32::EPSILON {
                return false;
            }
            let style = Style {
                width,
                ..self.style.clone()
            };
            return self.restyle(document, style);
        }
        if id == toolbar::field(toolbar::Field::Heads) {
            self.heads = if self.heads.start {
                Heads::END
            } else {
                Heads::BOTH
            };
            return true;
        }
        for family in toolbar::Family::ALL {
            if id == family.slot() {
                let count = family.children().len();
                let current = self.family[family.index()].min(count - 1);
                let index = (current as i32 + step as i32).rem_euclid(count as i32) as usize;
                self.family[family.index()] = index;
                return self.arm(family.children()[index], document);
            }
        }
        self.wheel(value)
    }

    /// A control was activated: a tool armed, a colour chosen, the loupe switched.
    fn activate(&mut self, id: Id, document: &mut Document) -> bool {
        if id == toolbar::loupe() {
            self.loupe.toggle();
            return true;
        }
        // A plain Enter on the field: the mark is done, and the next press starts another.
        if Some(id) == self.node.map(text_field_id) {
            return self.finish_text(document);
        }
        // The chips that open a picker: the paint chip carries stroke and fill, a swatch chip
        // one ink of its own, and pressing it again closes what it opened.
        for which in [toolbar::Field::Stroke, toolbar::Field::Outline, toolbar::Field::Border] {
            if id == toolbar::field(which) {
                self.open = if self.open == Some(which) {
                    None
                } else {
                    Some(which)
                };
                return true;
            }
        }
        if let Some((which, colour)) = palette_colour(id) {
            self.open = None;
            return self.painted(document, which, Some(colour));
        }
        for which in [toolbar::Field::Fill, toolbar::Field::Outline, toolbar::Field::Border] {
            if id == toolbar::no_fill(which) {
                self.open = None;
                return self.painted(document, which, None);
            }
        }
        if id == toolbar::dropper(toolbar::Field::Stroke) {
            self.pick = true;
            self.open = None;
            return true;
        }
        for mark in toolbar::Mark::ALL {
            if id == mark.id() {
                return self.arm(mark, document);
            }
        }
        for family in toolbar::Family::ALL {
            if id == family.id() {
                let index = self.family[family.index()].min(family.children().len() - 1);
                return self.arm(family.children()[index], document);
            }
        }
        false
    }

    /// A family's menu chose one of its children.
    fn menu(&mut self, id: Id, item: Id, document: &mut Document) -> bool {
        for family in toolbar::Family::ALL {
            for index in 0..family.children().len() {
                if item == toolbar::option_id(family, index) {
                    self.family[family.index()] = index;
                    return self.arm(family.children()[index], document);
                }
            }
        }
        if item == toolbar::option(toolbar::Field::Heads, 0) {
            self.heads = Heads::END;
            return true;
        }
        if item == toolbar::option(toolbar::Field::Heads, 1) {
            self.heads = Heads::BOTH;
            return true;
        }
        if self.text_mode() {
            for (index, align) in ALIGNED.iter().enumerate() {
                if item == toolbar::option(toolbar::Field::Align, index) {
                    let mut text = self.text_style.clone();
                    text.align = *align;
                    return self.restyle_text(document, text);
                }
            }
        }
        // The path's own two rows: the anchors stand, or the preview goes away.
        // They hang off its interaction node, so they are answered before any mark.
        if item == Id::str("pin-edit/finish-path") {
            return self.finish_path(document);
        }
        if item == Id::str("pin-edit/discard-path") {
            return self.discard_path(document);
        }
        // The field's own two rows: what it wrote stands, or the mark goes back to what it
        // was. They hang off the field, so they are answered before any mark is looked for.
        if item == Id::str("pin-edit/finish-text") {
            return self.finish_text(document);
        }
        if item == Id::str("pin-edit/discard-text") {
            return self.discard_text(document);
        }
        let Some(node) = self.node else {
            return false;
        };
        let Some(index) = (0..document.len()).find(|index| mark_id(node, *index) == id) else {
            return false;
        };
        let Some((mark, _)) = document.at(index) else {
            return false;
        };
        if item == Id::str("pin-edit/edit-text") {
            return self.begin_text_of(document, mark);
        }
        if item == Id::str("pin-edit/edit-points") {
            return self.begin_points(document, mark);
        }
        if item != Id::str("pin-edit/delete") {
            return false;
        }
        let Some(edit) = document.remove_edit(mark) else {
            return false;
        };
        document.commit(edit);
        if self.selected == Some(mark) {
            self.selected = None;
        }
        // Deleting the mark a field is over closes the field: nothing is left to type into.
        if self.text.as_ref().is_some_and(|open| open.id == mark) {
            self.text = None;
        }
        true
    }

    /// Arm a tool; the cursor arms nothing and leaves the selection alone.
    fn arm(&mut self, mark: toolbar::Mark, document: &mut Document) -> bool {
        self.finish_text(document);
        self.finish_path(document);
        self.selected = None;
        self.tool = tool_of(mark);
        if let Some(family) = family_of(mark)
            && let Some(index) = family.children().iter().position(|child| *child == mark)
        {
            self.family[family.index()] = index;
        }
        true
    }

    pub(crate) fn owns(&self, id: Id, document: &Document) -> bool {
        let Some(node) = self.node else {
            return false;
        };
        if id == content_id(node) || id == chrome_id(node) || id == path_id(node) || id == node.with("edit-path-controls") {
            return true;
        }
        if toolbar::owns(id) {
            return true;
        }
        if (0..8).any(|index| id == handle_id(node, index)) {
            return true;
        }
        if id == text_field_id(node) {
            return true;
        }
        if (0..self.node_points(document)).any(|index| id == anchor_id(node, index)) {
            return true;
        }
        (0..document.len()).any(|index| id == mark_id(node, index))
    }

    /// Escape layering: what is still an unfinished gesture, and nothing that can be put down
    /// on its own. The tool and the selection stay, so Escape always means "leave".
    pub(crate) fn escape(&mut self, document: &mut Document) -> bool {
        // A popover is over everything, so it leaves first.
        if self.open.is_some() || self.pick {
            self.open = None;
            self.pick = false;
            return true;
        }
        if self.finish_text(document) {
            return true;
        }
        if self.discard_path(document) {
            return true;
        }
        if self.nodes.take().is_some() {
            return true;
        }
        if self.drawing.take().is_some() {
            if let Some(id) = self.draft.take() {
                document.drop_preview(id);
            }
            return true;
        }
        if let Some(moving) = self.moving.take() {
            document.preview(moving.id, moving.before);
            return true;
        }
        if let Some(resizing) = self.resizing.take() {
            document.preview(resizing.id, resizing.before);
            return true;
        }
        false
    }

    /// Put the mark selection down, leaving the tool armed: the next press draws again.
    pub(crate) fn deselect(&mut self) -> bool {
        self.selected.take().is_some()
    }

    /// Put the cursor back: no tool armed and no mark selected, which is what Escape used to
    /// do on its way out.
    pub(crate) fn cursor(&mut self, document: &mut Document) -> bool {
        self.arm(toolbar::Mark::Cursor, document)
    }

    /// Shed everything the session holds uncommitted. A previewed mark is not in the history,
    /// so a session that dropped one on its way out would leave it drawn forever.
    pub(crate) fn abandon(&mut self, document: &mut Document) {
        while self.escape(document) {}
    }

    /// Walk the document's history; the selection follows what is still drawn.
    pub(crate) fn history(&mut self, document: &mut Document, redo: bool) -> bool {
        // The field's own history takes the chord while it holds the caret.
        if self.text.is_some() || self.path.is_some() {
            return false;
        }
        let walked = if redo {
            document.redo()
        } else {
            document.undo()
        };
        if walked.is_none() {
            return false;
        }
        if self.selected.is_some_and(|id| document.get(id).is_none()) {
            self.selected = None;
        }
        if self
            .nodes
            .as_ref()
            .is_some_and(|nodes| document.get(nodes.id).is_none())
        {
            self.nodes = None;
        }
        true
    }

    fn begin_resize(&mut self, index: usize, document: &Document) -> Option<Resizing> {
        let id = self.selected?;
        let mark = document.get(id)?;
        let from = mark.bounds();
        let edges = (
            matches!(index, 0 | 6..=7),
            matches!(index, 0..=2),
            matches!(index, 2..=4),
            matches!(index, 4..=6),
        );
        Some(Resizing {
            id,
            before: mark.clone(),
            from,
            edges,
        })
    }

    fn finish_resize(&mut self, document: &mut Document) -> bool {
        let Some(drag) = self.resizing.take() else {
            return false;
        };
        let Some(after) = document.get(drag.id).cloned() else {
            return false;
        };
        if after == drag.before {
            return false;
        }
        let index = document.index_of(drag.id).unwrap_or(0);
        document.commit(Edit {
            id: drag.id,
            index,
            before: Some(drag.before),
            after: Some(after),
        });
        true
    }

    fn finish_move(&mut self, document: &mut Document) -> bool {
        let Some(drag) = self.moving.take() else {
            return false;
        };
        let Some(after) = document.get(drag.id).cloned() else {
            return false;
        };
        if after == drag.before {
            return false;
        }
        let index = document.index_of(drag.id).unwrap_or(0);
        document.commit(Edit {
            id: drag.id,
            index,
            before: Some(drag.before),
            after: Some(after),
        });
        true
    }

    /// Take `style` as the session's, and restyle the selected mark in one history step.
    fn restyle(&mut self, document: &mut Document, style: Style) -> bool {
        self.style = style.clone();
        // A text mark or a path is not in the history until its own session closes, so
        // the style goes into the preview the canvas is painting.
        if let Some(id) = self
            .text
            .as_ref()
            .map(|edit| edit.id)
            .or_else(|| self.path.as_ref().map(|edit| edit.id))
        {
            if let Some(mark) = document.get(id).cloned() {
                document.preview(
                    id,
                    Mark {
                        kind: mark.kind,
                        style,
                    },
                );
            }
            return true;
        }
        let Some(id) = self.selected else {
            return true;
        };
        let Some(mark) = document.get(id).cloned() else {
            return true;
        };
        if mark.style == style {
            return true;
        }
        let Some(index) = document.index_of(id) else {
            return true;
        };
        let after = Mark {
            kind: mark.kind.clone(),
            style,
        };
        document.commit(Edit {
            id,
            index,
            before: Some(mark),
            after: Some(after.clone()),
        });
        if let Some(nodes) = &mut self.nodes
            && nodes.id == id
        {
            nodes.before = after;
        }
        true
    }

    /// How many points the open mode's mark has; zero when no mode is open.
    fn node_points(&self, document: &Document) -> usize {
        let Some(nodes) = &self.nodes else {
            return 0;
        };
        match document.get(nodes.id).map(|mark| &mark.kind) {
            Some(MarkKind::Pen(points)) => points.len(),
            _ => 0,
        }
    }

    /// Open the point controls over a pen stroke the user asked to adjust.
    fn begin_points(&mut self, document: &mut Document, id: MarkId) -> bool {
        let Some(mark) = document.get(id).cloned() else {
            return false;
        };
        if !matches!(mark.kind, MarkKind::Pen(_)) {
            return false;
        }
        self.finish_text(document);
        self.nodes = Some(Nodes {
            id,
            before: mark,
            dragging: None,
        });
        self.selected = Some(id);
        true
    }

    /// Move one of the open mode's points; the release commits the whole drag.
    fn drag_point(&mut self, document: &mut Document, index: usize, at: Point) -> bool {
        let Some(nodes) = &mut self.nodes else {
            return false;
        };
        nodes.dragging.get_or_insert(index);
        let Some(after) = point_moved(&nodes.before, index, at) else {
            return false;
        };
        document.preview(nodes.id, after);
        true
    }

    /// One point drag is one step of history, whichever way it went.
    fn finish_point(&mut self, document: &mut Document) -> bool {
        let Some(nodes) = &mut self.nodes else {
            return false;
        };
        if nodes.dragging.take().is_none() {
            return false;
        }
        let Some(after) = document.get(nodes.id).cloned() else {
            return false;
        };
        if after == nodes.before {
            return false;
        }
        let Some(index) = document.index_of(nodes.id) else {
            return false;
        };
        document.commit(Edit {
            id: nodes.id,
            index,
            before: Some(nodes.before.clone()),
            after: Some(after.clone()),
        });
        nodes.before = after;
        true
    }

    fn begin_text(&mut self, document: &mut Document, at: Point) -> bool {
        let mut text = self.text_style.clone();
        text.at = at;
        text.text.clear();
        // The template dresses a new mark; a pulled-out width belongs to the mark it was
        // dragged on, and carrying it here would wrap a fresh mark at a stale width.
        text.width = None;
        let id = document.insert_preview(Mark {
            kind: MarkKind::Text(text),
            style: self.style.clone(),
        });
        self.text = Some(TextEdit { id, before: None });
        self.selected = Some(id);
        true
    }

    /// Open a field over a text mark the user asked to change. The parameters follow the
    /// mark, so what the bar shows is that mark's own dressing rather than the last one's.
    fn begin_text_of(&mut self, document: &mut Document, id: MarkId) -> bool {
        let Some(mark) = document.get(id).cloned() else {
            return false;
        };
        let MarkKind::Text(text) = &mark.kind else {
            return false;
        };
        self.finish_text(document);
        self.nodes = None;
        self.style = mark.style.clone();
        self.text_style = text.clone();
        self.text = Some(TextEdit {
            id,
            before: Some(mark),
        });
        self.selected = Some(id);
        true
    }

    /// The composition an open text field is showing, if any.
    ///
    /// The bytes live in the engine, not the document: the mark keeps only what the input
    /// method committed, so a cancelled composition leaves no edit behind. The mark that
    /// comes back has the preedit spliced in and measured, so its frame is the box the
    /// commit will produce and the canvas and the field measure one paragraph.
    pub(crate) fn composing(
        &self,
        cx: &Cx<'_>,
        node: Id,
        document: &Document,
    ) -> Option<Composing> {
        let edit = self.text.as_ref()?;
        let field = text_field_id(node);
        let preedit = cx
            .preedit_for(field)
            .filter(|preedit| !preedit.is_empty())?;
        let caret = cx.caret_for(field)?;
        let MarkKind::Text(mark) = &document.get(edit.id)?.kind else {
            return None;
        };
        let (mut shown, at) = mark.composed_with(caret, &preedit);
        shown.measure(cx);
        Some(Composing {
            id: edit.id,
            caret,
            preedit,
            shown,
            at,
        })
    }

    /// Close the field: an empty mark leaves nothing behind, a typed one becomes one step
    /// of history.
    fn finish_text(&mut self, document: &mut Document) -> bool {
        let Some(edit) = self.text.take() else {
            return false;
        };
        let typed = text_of(document, edit.id).is_some_and(|text| !text.text.trim().is_empty());
        if typed {
            document.commit_preview(edit.id, edit.before);
        } else {
            document.drop_preview(edit.id);
            if self.selected == Some(edit.id) {
                self.selected = None;
            }
        }
        true
    }

    /// Put the field's mark back the way the session found it, and close the field. The
    /// canvas takes the mark over again, so this is a repaint rather than a hidden change.
    fn discard_text(&mut self, document: &mut Document) -> bool {
        let Some(edit) = self.text.take() else {
            return false;
        };
        match edit.before {
            Some(before) => document.preview(edit.id, before),
            None => document.drop_preview(edit.id),
        }
        if self.selected == Some(edit.id) {
            self.selected = None;
        }
        true
    }

    /// Write the buffer into the open mark. The canvas paints it from there, so a keystroke
    /// is a preview like any other: the picture and the field over it stay one drawing.
    fn update_text(&mut self, document: &mut Document, next: String) -> bool {
        let Some(id) = self.text.as_ref().map(|edit| edit.id) else {
            return false;
        };
        let Some(mark) = document.get(id).cloned() else {
            return false;
        };
        let MarkKind::Text(mut text) = mark.kind else {
            return false;
        };
        if text.text == next {
            return false;
        }
        text.text = next;
        document.preview(
            id,
            Mark {
                kind: MarkKind::Text(text),
                style: mark.style,
            },
        );
        true
    }

    /// Take `template` as the dressing the next text mark starts from, and dress the mark the
    /// field is over — or the selected text mark — in one step of history.
    fn restyle_text(&mut self, document: &mut Document, template: TextMark) -> bool {
        self.text_style = template.clone();
        let editing = self.text.as_ref().map(|edit| edit.id);
        let Some(id) = editing.or(self.selected) else {
            return true;
        };
        let Some(mark) = document.get(id).cloned() else {
            return true;
        };
        let MarkKind::Text(before) = &mark.kind else {
            return true;
        };
        let after = before.dressed_as(&template);
        if *before == after {
            return true;
        }
        let after = Mark {
            kind: MarkKind::Text(after),
            style: mark.style.clone(),
        };
        if editing.is_some() {
            document.preview(id, after);
            return true;
        }
        let Some(index) = document.index_of(id) else {
            return true;
        };
        document.commit(Edit {
            id,
            index,
            before: Some(mark),
            after: Some(after),
        });
        true
    }

    /// Take one colour as the value of `which`: a text mark's inks live on the mark, the
    /// shared style's on the session.
    fn painted(
        &mut self,
        document: &mut Document,
        which: toolbar::Field,
        colour: Option<Color>,
    ) -> bool {
        let faded = colour.map(|colour| colour.faded(FILL_ALPHA));
        if self.text_mode() {
            let mut text = self.text_style.clone();
            return match which {
                toolbar::Field::Stroke => match colour {
                    Some(ink) => self.restyle(
                        document,
                        Style {
                            ink,
                            ..self.style.clone()
                        },
                    ),
                    None => false,
                },
                toolbar::Field::Fill => {
                    text.background = faded;
                    self.restyle_text(document, text)
                }
                toolbar::Field::Outline => {
                    text.outline = colour;
                    self.restyle_text(document, text)
                }
                toolbar::Field::Border => {
                    text.border = colour;
                    self.restyle_text(document, text)
                }
                _ => false,
            };
        }
        match which {
            toolbar::Field::Stroke => match colour {
                Some(ink) => self.restyle(
                    document,
                    Style {
                        ink,
                        ..self.style.clone()
                    },
                ),
                None => false,
            },
            toolbar::Field::Fill => self.restyle(
                document,
                Style {
                    fill: faded,
                    ..self.style.clone()
                },
            ),
            _ => false,
        }
    }

    /// The mark a drag's geometry draws, in picture pixels; `None` while it has none yet.
    fn drawing_mark(&self, drawing: &Drawing) -> Option<Mark> {
        let (from, to) = (drawing.from, drawing.to);
        let kind = match self.tool {
            Some(Tool::Rect) | Some(Tool::Ellipse) => {
                let box_ = Rect::new(
                    from.x.min(to.x),
                    from.y.min(to.y),
                    (to.x - from.x).abs(),
                    (to.y - from.y).abs(),
                );
                if self.tool == Some(Tool::Rect) {
                    MarkKind::Rect(box_)
                } else {
                    MarkKind::Ellipse(box_)
                }
            }
            Some(Tool::Line) | Some(Tool::Arrow) => MarkKind::Line(Line {
                from,
                to,
                heads: if self.tool == Some(Tool::Arrow) {
                    self.heads
                } else {
                    Heads::NONE
                },
            }),
            Some(Tool::Pen) => {
                if drawing.points.len() < 2 {
                    return None;
                }
                MarkKind::Pen(drawing.points.clone())
            }
            Some(Tool::Bezier) | Some(Tool::Text) | None => return None,
        };
        Some(Mark {
            kind,
            style: self.style.clone(),
        })
    }

    fn commit_drawing(&mut self, document: &mut Document, drawing: Drawing) -> bool {
        let draft = self.draft.take();
        let mark = self.drawing_mark(&drawing);
        // Too small to keep: the drag leaves nothing behind, and neither does its preview.
        let too_small = match mark.as_ref().map(|mark| &mark.kind) {
            Some(MarkKind::Rect(box_)) | Some(MarkKind::Ellipse(box_)) => {
                box_.size.w < MIN_MARK && box_.size.h < MIN_MARK
            }
            Some(MarkKind::Line(line)) => distance(line.from, line.to) < MIN_MARK,
            _ => false,
        };
        let Some(mark) = mark.filter(|_| !too_small) else {
            if let Some(id) = draft {
                document.drop_preview(id);
            }
            return false;
        };
        let id = match draft {
            Some(id) => {
                document.preview(id, mark.clone());
                if !document.commit_preview(id, None) {
                    return false;
                }
                id
            }
            None => {
                let edit = document.insert_edit(mark.clone());
                let id = edit.id;
                document.commit(edit);
                id
            }
        };
        self.selected = Some(id);
        true
    }
}

pub(crate) fn content_id(node: Id) -> Id {
    node.with("edit-content")
}

fn anchor_id(node: Id, index: usize) -> Id {
    node.with(("edit-anchor", index))
}





fn path_id(node: Id) -> Id {
    node.with("edit-path")
}

fn text_field_id(node: Id) -> Id {
    node.with("edit-text")
}

/// The text mark a field is open over, read back out of the document. It was measured this
/// build, so this is the frame and the paragraph the picture itself would draw.
fn text_of(document: &Document, id: MarkId) -> Option<TextMark> {
    match document.get(id).map(|mark| &mark.kind) {
        Some(MarkKind::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// The buffer a field edits and the paragraph it draws; they differ while a composition
/// is in flight.
struct FieldText<'a> {
    /// What the engine edits and commits.
    committed: &'a TextMark,
    /// The same mark showing the composition, whose frame the box and the text use.
    shown: &'a TextMark,
}

/// The field over a text mark: the mark's own box, given the caret and nothing else. The
/// canvas paints the frame and the paragraph — the same primitives the exported picture
/// rasterizes — so typing cannot drift from what will be saved. Only the caret and the
/// selection are chrome, and they hang off this transparent box.
fn text_field(
    cx: &mut Cx<'_>,
    node: Id,
    space: Space,
    drawn: Rect,
    text: FieldText<'_>,
    ink: Color,
    catalog: &Catalog,
) -> ui_core::Decl {
    let mark = text.shown;
    let committed = text.committed.text.as_str();
    let (sx, sy) = space.scale();
    let frame = mark.frame();
    let at = space.to_surface(frame.origin);
    let field = text_field_id(node);
    let size = (mark.size * sy).max(6.0);
    let padding = Point::new(mark.padding * sx, mark.padding * sy);
    // The field wraps where the canvas's primitive does, including a composition: its box
    // is the measured mark's, so the caret stays on the line the glyphs are on.
    let width = mark.wrap() * sx;
    let caret = cx.caret_for(field).unwrap_or(committed.len());
    let preedit = cx.preedit_for(field);
    // The composition draws where its commit would land, and the caret after it; the
    // canvas beneath draws the same splice, so the two cannot part company.
    let (shown, composing, caret_at) =
        ui_widgets::text::typed_input(committed, Some(caret), preedit.as_ref());
    let selection = cx
        .selection_for(field)
        .filter(|selection| !selection.is_collapsed())
        .map(|selection| ui_core::SelectionPrim {
            range: selection.range(),
            fill: Color::from_rgba8(80, 160, 255, 110),
            painter: None,
        });
    let mut text = ui_widgets::text::Text::new(field.with("text"), shown, size, Color::TRANSPARENT)
        .max_width(width)
        .align(mark.align)
        .composing(composing)
        .painting(selection);
    // A hugged mark has no break of its own, and its box is only as wide as the paragraph:
    // laid out against that box, a line rounding a hair wider would fold and take the caret
    // with it. The field draws one line, as the canvas primitive does.
    if mark.width.is_none() {
        text = text.nowrap();
    }
    if let Some(at) = caret_at {
        text = text.caret(at, ink);
    }
    let rows = vec![
        MenuItem::new(
            Id::str("pin-edit/finish-text"),
            catalog.text_by_id("pin-edit-finish-text").to_string(),
        ),
        MenuItem::new(
            Id::str("pin-edit/discard-text"),
            catalog.text_by_id("pin-edit-discard-text").to_string(),
        ),
    ];
    let decl = ui_core::Decl::container(field, NodeKind::Stack)
        .size(SizeClass::fixed(frame.size.w * sx, frame.size.h * sy))
        .sense(Sense::CLICK | Sense::FOCUSABLE | Sense::SECONDARY)
        // The field paints no shape of its own — its frame does — so it has to say it is a
        // keyboard stop, or no focus request would ever land on it.
        .focus_stop(true)
        .cursor(CursorShape::Text)
        .text_input(committed.to_owned())
        .text_input_multiline(true)
        .menu(rows)
        .offset(at.x - drawn.origin.x, at.y - drawn.origin.y)
        .child(text.resolve().offset(padding.x, padding.y));
    if cx.focused() != Some(field) {
        cx.request_focus(field);
    }
    decl
}

/// The text dressing after one wheel step on `id`, or `None` when `id` is no text value.
fn text_wheel(text: &TextMark, id: Id, step: f32) -> Option<TextMark> {
    let mut next = text.clone();
    if id == toolbar::field(toolbar::Field::Size) {
        next.size = (text.size + step).clamp(MIN_SIZE, MAX_SIZE);
    } else if id == toolbar::field(toolbar::Field::Padding) {
        next.padding = (text.padding + step).clamp(0.0, MAX_PADDING);
    } else if id == toolbar::field(toolbar::Field::Radius) {
        next.radius = (text.radius + step).clamp(0.0, MAX_RADIUS);
    } else if id == toolbar::field(toolbar::Field::Align) {
        let index = (align_index(text.align) as i32 + step as i32).rem_euclid(3) as usize;
        next.align = ALIGNED[index];
    } else {
        return None;
    }
    Some(next)
}

/// Which of [`ALIGN`] an alignment is shown as.
fn align_index(align: TextAlign) -> usize {
    match align {
        TextAlign::Center => 1,
        TextAlign::End => 2,
        TextAlign::Start | TextAlign::Justify => 0,
    }
}

/// The palette swatch `id` names: the field it sets, and its colour.
fn palette_colour(id: Id) -> Option<(toolbar::Field, Color)> {
    for which in toolbar::Field::ALL {
        for (index, colour) in toolbar::PALETTE.iter().enumerate() {
            if id == toolbar::swatch(which, index) {
                return Some((which, *colour));
            }
        }
    }
    None
}

fn chrome_id(node: Id) -> Id {
    node.with("edit-chrome")
}



fn mark_id(node: Id, index: usize) -> Id {
    node.with(("edit-mark", index))
}

fn handle_id(node: Id, index: usize) -> Id {
    node.with(("edit-handle", index))
}

/// The eight handle positions of a surface box, in the order NW, N, NE, E, SE, S, SW, W.
fn handles(box_: Rect) -> Vec<(Point, CursorShape)> {
    let (l, t, r, b) = (box_.min_x(), box_.min_y(), box_.max_x(), box_.max_y());
    let (cx, cy) = (l + box_.size.w * 0.5, t + box_.size.h * 0.5);
    vec![
        (Point::new(l, t), CursorShape::ResizeDiagonalDownRight),
        (Point::new(cx, t), CursorShape::ResizeVertical),
        (Point::new(r, t), CursorShape::ResizeDiagonalUpRight),
        (Point::new(r, cy), CursorShape::ResizeHorizontal),
        (Point::new(r, b), CursorShape::ResizeDiagonalDownRight),
        (Point::new(cx, b), CursorShape::ResizeVertical),
        (Point::new(l, b), CursorShape::ResizeDiagonalUpRight),
        (Point::new(l, cy), CursorShape::ResizeHorizontal),
    ]
}

fn surface_box(image: Rect, space: Space) -> Rect {
    let origin = space.to_surface(image.origin);
    let (sx, sy) = space.scale();
    Rect::new(origin.x, origin.y, image.size.w * sx, image.size.h * sy)
}

/// Anchors and their handles, painted into the chrome rather than the mark's raster:
/// they are controls during creation and vanish when the path is committed.
fn path_controls(path: &PathMark, space: Space, origin: Point) -> Vec<Shape> {
    let mut out = Vec::new();
    let local = |at: Point| {
        let at = space.to_surface(at);
        Point::new(at.x - origin.x, at.y - origin.y)
    };
    for (index, anchor) in path.anchors.iter().enumerate() {
        let centre = local(anchor.at);
        if index == 0 && path.anchors.len() >= 2 && !path.closed {
            out.push(Shape::Ellipse {
                rect: Rect::new(
                    centre.x - HANDLE,
                    centre.y - HANDLE,
                    HANDLE * 2.0,
                    HANDLE * 2.0,
                ),
                thickness: 1.0,
                fill: Color::TRANSPARENT,
                colour: Color::from_rgba8(80, 160, 255, 255),
            });
        }
        out.push(Shape::Ellipse {
            rect: Rect::new(
                centre.x - HANDLE * 0.5,
                centre.y - HANDLE * 0.5,
                HANDLE,
                HANDLE,
            ),
            thickness: 1.0,
            fill: Color::from_rgba8(255, 255, 255, 255),
            colour: Color::from_rgba8(20, 20, 20, 200),
        });
        if anchor.handle_out != anchor.at {
            let handle = local(anchor.handle_out);
            out.push(Shape::Line {
                from: centre,
                to: handle,
                thickness: 1.0,
                colour: Color::from_rgba8(80, 160, 255, 220),
            });
            out.push(Shape::Ellipse {
                rect: Rect::new(
                    handle.x - HANDLE * 0.25,
                    handle.y - HANDLE * 0.25,
                    HANDLE * 0.5,
                    HANDLE * 0.5,
                ),
                thickness: 1.0,
                fill: Color::from_rgba8(80, 160, 255, 255),
                colour: Color::from_rgba8(20, 20, 20, 200),
            });
        }
    }
    out
}

fn in_band(at: Point, space: Space) -> bool {
    let image = Rect::new(0.0, 0.0, space.image.0 as f32, space.image.1 as f32);
    let band = BAND / space.scale().0.max(f32::EPSILON);
    !image.inflate(-band).contains(at)
}

/// The mark with one pen point moved; `None` for a mark that has no points.
fn point_moved(mark: &Mark, index: usize, at: Point) -> Option<Mark> {
    let MarkKind::Pen(points) = &mark.kind else {
        return None;
    };
    let mut points = points.clone();
    *points.get_mut(index)? = at;
    Some(Mark {
        kind: MarkKind::Pen(points),
        style: mark.style.clone(),
    })
}

fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::MarkKind;
    use ui_core::geom::{Rect, Size};

    fn draw(tool: Tool, from: Point, to: Point, points: Vec<Point>) -> (Document, bool) {
        let mut session = Session::new();
        session.tool = Some(tool);
        let mut document = Document::new();
        let changed = session.commit_drawing(&mut document, Drawing { from, to, points });
        (document, changed)
    }

    fn kind(document: &Document) -> MarkKind {
        document
            .marks()
            .next()
            .map(|(_, mark)| mark.kind.clone())
            .expect("one mark")
    }

    /// Each tool turns one drag into one mark, in image pixels, and selects it: the geometry
    /// never sees a surface point.
    #[test]
    fn a_drag_becomes_one_mark_at_image_pixels() {
        let (document, changed) = draw(
            Tool::Rect,
            Point::new(10.0, 20.0),
            Point::new(40.0, 60.0),
            Vec::new(),
        );
        assert!(changed);
        assert_eq!(
            kind(&document),
            MarkKind::Rect(Rect::new(10.0, 20.0, 30.0, 40.0))
        );

        let (document, _) = draw(
            Tool::Ellipse,
            Point::new(40.0, 60.0),
            Point::new(10.0, 20.0),
            Vec::new(),
        );
        assert_eq!(
            kind(&document),
            MarkKind::Ellipse(Rect::new(10.0, 20.0, 30.0, 40.0)),
            "a drag drawn up and left is the same box"
        );

        let (document, _) = draw(
            Tool::Arrow,
            Point::new(1.0, 2.0),
            Point::new(30.0, 40.0),
            Vec::new(),
        );
        assert_eq!(
            kind(&document),
            MarkKind::Line(Line {
                from: Point::new(1.0, 2.0),
                to: Point::new(30.0, 40.0),
                heads: Heads::END,
            })
        );

        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 6.0),
            Point::new(9.0, 3.0),
        ];
        let (document, _) = draw(Tool::Pen, points[0], points[2], points.clone());
        assert_eq!(kind(&document), MarkKind::Pen(points));
    }

    /// A slip of the hand is not a mark, and it never enters the history.
    #[test]
    fn a_slip_of_the_hand_draws_nothing() {
        let (document, changed) = draw(
            Tool::Rect,
            Point::new(10.0, 10.0),
            Point::new(11.0, 11.0),
            Vec::new(),
        );
        assert!(!changed);
        assert_eq!(document.len(), 0);
        assert_eq!(document.revision(), 0, "nothing to repaint");
    }

    /// The band is the picture's outer points: a press there belongs to the pin, not to the
    /// tool. It is stated in surface points, so it covers fewer image pixels as the pin grows.
    #[test]
    fn the_band_is_the_pictures_outer_points() {
        let drawn = Rect::new(100.0, 100.0, 400.0, 200.0);
        let zoomed = Space::new(drawn, (800, 400));
        assert!(in_band(Point::new(3.0, 3.0), zoomed), "the corner is band");
        assert!(
            in_band(Point::new(400.0, 8.0), zoomed),
            "so is the top strip"
        );
        assert!(
            !in_band(Point::new(400.0, 200.0), zoomed),
            "the middle draws"
        );

        let large = Space::new(Rect::new(0.0, 0.0, 800.0, 400.0), (800, 400));
        assert!(
            in_band(Point::new(6.0, 200.0), large),
            "eight points at 1:1"
        );
        assert!(
            !in_band(Point::new(12.0, 200.0), large),
            "and no further in"
        );
    }

    /// A text frame's side grip sets how wide its lines may run and leaves the type its
    /// size: the frame only ever widens, so an alignment has slack to show inside it.
    #[test]
    fn a_text_side_grip_widens_the_frame_and_leaves_the_type() {
        let mut text = TextMark::new(Point::new(10.0, 20.0));
        text.content = Size::new(40.0, 20.0);
        let mark = Mark {
            kind: MarkKind::Text(text.clone()),
            style: Style::default(),
        };
        let from = mark.bounds();
        let edge = Sides {
            left: from.origin.x,
            top: from.origin.y,
            right: 200.0,
            bottom: from.max_y(),
        };
        let MarkKind::Text(wider) = mark.resized(from, edge, (false, false, true, false), false).kind
        else {
            panic!("a text mark stays text")
        };
        assert_eq!(wider.size, text.size, "a side grip leaves the type alone");
        assert_eq!(wider.frame().size.w, 200.0 - from.origin.x);
        assert_eq!(wider.at.x, from.origin.x, "the far edge stays where it was");

        // The left grip is the mirror: the right edge holds and the frame grows leftwards.
        let left = Sides {
            left: 2.0,
            right: from.max_x(),
            ..edge
        };
        let MarkKind::Text(pulled) = mark.resized(from, left, (true, false, false, false), false).kind
        else {
            panic!("a text mark stays text")
        };
        assert_eq!(pulled.frame().max_x(), from.max_x());
        assert_eq!(pulled.frame().size.w, from.max_x() - 2.0);

        // A grip that also moves vertically maps the whole frame, so the type follows.
        let corner = Sides {
            bottom: 200.0,
            ..edge
        };
        let MarkKind::Text(scaled) = mark.resized(from, corner, (false, true, true, false), false).kind
        else {
            panic!("a text mark stays text")
        };
        assert!(scaled.size > text.size, "a corner still scales the type");
    }

    /// A resize keeps the mark's shape and maps it onto the new box, and Escape puts the
    /// press's mark back without an edit.
    #[test]
    fn a_resize_rebases_the_mark_and_escape_restores_it() {
        let mut document = Document::new();
        let mark = Mark {
            kind: MarkKind::Rect(Rect::new(10.0, 20.0, 30.0, 40.0)),
            style: Style::default(),
        };
        let edit = document.insert_edit(mark.clone());
        let id = edit.id;
        document.commit(edit);

        let from = mark.bounds();
        let resizing = Resizing {
            id,
            before: mark.clone(),
            from,
            edges: (true, true, false, false),
        };
        let box_ = box_to(&resizing, Point::new(25.0, 30.0));
        let anchor = Point::new(from.max_x(), from.max_y());
        assert_eq!(Point::new(box_.left, box_.top), Point::new(25.0, 30.0));
        assert!(
            (box_.right - anchor.x).abs() < 1e-4 && (box_.bottom - anchor.y).abs() < 1e-4,
            "the corner away from the handle stays put"
        );

        // The stroke keeps its width unless the hand asks for it to follow the box; the
        // shape's own corners still land on the box, since the box is the outline itself.
        let after = mark.rebased(from, box_, false);
        assert_eq!(
            after.style.width, mark.style.width,
            "a resize leaves the stroke alone"
        );
        assert_eq!(
            after.bounds(),
            Rect::new(
                box_.left,
                box_.top,
                box_.right - box_.left,
                box_.bottom - box_.top
            ),
            "the corners meet the dragging box, got {:?} for {box_:?}",
            after.bounds()
        );
        assert!(
            matches!(after.kind, MarkKind::Rect(_)),
            "a rect stays a rect"
        );

        // With Ctrl held the parameters follow the box, so scaling both axes alike leaves
        // no gap at all.
        let doubled = Rect::new(
            from.origin.x,
            from.origin.y,
            from.size.w * 2.0,
            from.size.h * 2.0,
        );
        assert_eq!(
            mark.rebased(from, Sides::of(doubled), true).bounds(),
            doubled
        );
        assert_eq!(
            mark.rebased(from, Sides::of(doubled), true).style.width,
            mark.style.width * 2.0,
            "the stroke scales with the box"
        );

        // Shift keeps the proportion: the corner the hand drags away from stays fixed and
        // the axis that moved further sets the factor of both.
        let locked = proportional(from, box_, resizing.edges);
        assert!(
            (locked.right - from.max_x()).abs() < 1e-4
                && (locked.bottom - from.max_y()).abs() < 1e-4,
            "the opposite corner is the pivot"
        );
        let (wide, tall) = (locked.right - locked.left, locked.bottom - locked.top);
        assert!(
            (wide / from.size.w - tall / from.size.h).abs() < 1e-4,
            "both axes scale alike, got {locked:?}"
        );
        assert!(
            (wide - (box_.right - box_.left)).abs() < 1e-4,
            "the axis the hand dragged keeps the size it asked for"
        );

        let mut session = Session::new();
        session.selected = Some(id);
        session.resizing = Some(Resizing {
            id,
            before: mark.clone(),
            from,
            edges: (true, true, false, false),
        });
        document.preview(id, after);
        assert!(session.escape(&mut document));
        assert_eq!(
            document.get(id),
            Some(&mark),
            "Escape takes the resize back"
        );
    }

    /// A grip dragged past the side opposite it mirrors the mark about that side. Clamping
    /// the box at its opposite would leave the endpoint away from the hand dragged along
    /// with it, and a line could never turn round.
    #[test]
    fn a_grip_dragged_past_its_opposite_mirrors_the_line() {
        let mut document = Document::new();
        let line = Mark {
            kind: MarkKind::Line(Line {
                from: Point::new(100.0, 100.0),
                to: Point::new(200.0, 200.0),
                heads: Heads::END,
            }),
            style: Style::default(),
        };
        let edit = document.insert_edit(line.clone());
        let id = edit.id;
        document.commit(edit);
        let from = line.bounds();
        let resizing = Resizing {
            id,
            before: line.clone(),
            from,
            edges: (true, true, false, false),
        };
        // The top-left grip goes past the bottom-right corner.
        let target = box_to(&resizing, Point::new(300.0, 300.0));
        let MarkKind::Line(after) = line.rebased(from, target, false).kind else {
            panic!("a line stays a line");
        };
        assert_eq!(
            after.to,
            Point::new(200.0, 200.0),
            "the endpoint away from the hand keeps its place"
        );
        assert_eq!(
            after.from,
            Point::new(300.0, 300.0),
            "the endpoint under the grip follows it"
        );

        // A grip dropped onto the side opposite leaves nothing to grab, so it stops a
        // sliver before it: the mark keeps a size and the way round it faced.
        let pinched = box_to(&resizing, Point::new(200.0, 200.0));
        assert_eq!(pinched.left, 200.0 - MIN_MARK);
        let MarkKind::Line(pinched) = line.rebased(from, pinched, false).kind else {
            panic!("a line stays a line");
        };
        assert_eq!(pinched.to, Point::new(200.0, 200.0));
        assert_eq!(pinched.from, Point::new(200.0 - MIN_MARK, 200.0 - MIN_MARK));
    }

    /// A box runs through the points that define the shape: no half-stroke stand-off and no
    /// arrow-head allowance, so the grips sit on the line's own ends.
    #[test]
    fn a_box_runs_through_the_control_points() {
        let line = |from, to, heads| Mark {
            kind: MarkKind::Line(Line { from, to, heads }),
            style: Style::default(),
        };
        let box_ = Rect::new(10.0, 20.0, 80.0, 40.0);
        assert_eq!(
            line(Point::new(10.0, 20.0), Point::new(90.0, 60.0), Heads::NONE).bounds(),
            box_
        );
        assert_eq!(
            line(Point::new(10.0, 20.0), Point::new(90.0, 60.0), Heads::BOTH).bounds(),
            box_,
            "the head lies inside the segment, so it adds nothing"
        );

        // A flat line's box is squared about the line, or no grip could pull it taller.
        let flat = line(Point::new(10.0, 50.0), Point::new(90.0, 50.0), Heads::END).bounds();
        assert_eq!(flat.size.w, 80.0);
        assert!(
            (flat.center().y - 50.0).abs() < 1e-4 && flat.size.h > 1.0,
            "a horizontal line still has a height to drag, got {flat:?}"
        );

        let pen = Mark {
            kind: MarkKind::Pen(vec![Point::new(5.0, 5.0), Point::new(40.0, 90.0)]),
            style: Style::default(),
        };
        assert_eq!(pen.bounds(), Rect::new(5.0, 5.0, 35.0, 85.0));

        let bezier = Mark {
            kind: MarkKind::Bezier(PathMark {
                anchors: vec![
                    PathAnchor {
                        at: Point::new(10.0, 20.0),
                        handle_out: Point::new(90.0, 20.0),
                    },
                    PathAnchor::new(Point::new(90.0, 60.0)),
                ],
                closed: false,
            }),
            style: Style::default(),
        };
        assert_eq!(bezier.bounds(), Rect::new(-70.0, 20.0, 160.0, 40.0));
    }

    /// A style choice restyles the selection in one step, and the next mark is drawn with
    /// it: the row is both the selection's control and the session's default.
    #[test]
    fn a_style_choice_restyles_the_selection_and_arms_the_next_mark() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.tool = Some(Tool::Rect);
        session.commit_drawing(
            &mut document,
            Drawing {
                from: Point::new(0.0, 0.0),
                to: Point::new(20.0, 20.0),
                points: Vec::new(),
            },
        );
        let id = session.selected.expect("the rect is selected");
        let drawn = document.get(id).expect("drawn").style.clone();

        let chosen = Style {
            ink: Color::from_rgba8(80, 160, 255, 255),
            fill: None,
            width: 8.0,
        };
        assert!(session.restyle(&mut document, chosen.clone()));
        assert_eq!(
            document.get(id).map(|mark| mark.style.clone()),
            Some(chosen.clone()),
            "the selected mark takes the style"
        );
        assert!(document.undo().is_some(), "one step");
        assert_eq!(
            document.get(id).map(|mark| mark.style.clone()),
            Some(drawn),
            "and undo takes the restyle back"
        );
        assert!(document.redo().is_some());

        session.commit_drawing(
            &mut document,
            Drawing {
                from: Point::new(30.0, 30.0),
                to: Point::new(50.0, 50.0),
                points: Vec::new(),
            },
        );
        let next = session.selected.expect("the second rect is selected");
        assert_eq!(
            document.get(next).map(|mark| mark.style.clone()),
            Some(chosen),
            "a new mark is drawn with the chosen style"
        );
    }

    /// A stroke is complete on release: its own node mode is opened explicitly, so
    /// creating a mark never leaves the session holding an exclusive mode.
    #[test]
    fn a_freehand_stroke_finishes_on_release() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.tool = Some(Tool::Pen);
        let points = vec![
            Point::new(10.0, 10.0),
            Point::new(40.0, 20.0),
            Point::new(70.0, 60.0),
        ];
        assert!(session.commit_drawing(
            &mut document,
            Drawing {
                from: points[0],
                to: points[2],
                points: points.clone(),
            },
        ));
        assert_eq!(session.node_points(&document), 0);
        let id = session.selected.expect("the stroke is selected");

        assert!(session.begin_points(&mut document, id));
        assert!(session.drag_point(&mut document, 1, Point::new(55.0, 30.0)));
        assert!(
            session.finish_point(&mut document),
            "the release commits the drag"
        );
        let moved = document.get(id).expect("the stroke is drawn");
        let MarkKind::Pen(after) = &moved.kind else {
            panic!("a pen stays a pen");
        };
        assert_eq!(after[1], Point::new(55.0, 30.0));
        assert_eq!(after.len(), 3, "a drag moves a point, it does not add one");

        assert!(document.undo().is_some(), "one step for the whole drag");
        let restored = document.get(id).expect("the stroke is back");
        let MarkKind::Pen(before) = &restored.kind else {
            panic!("a pen stays a pen");
        };
        assert_eq!(before, &points, "and it restores the drawn shape");
        assert_eq!(
            session.node_points(&document),
            3,
            "the explicitly opened mode survives an undo"
        );
    }

    /// Closing a Bezier path commits one mark and leaves the exclusive session on that
    /// same press; a second press starts another path rather than extending a finished one.
    #[test]
    fn a_closed_bezier_path_leaves_its_session() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.tool = Some(Tool::Bezier);
        let space = Space::new(Rect::new(0.0, 0.0, 100.0, 100.0), (100, 100));
        assert!(session.path_press(&mut document, space, Point::new(10.0, 10.0)));
        assert!(session.path_press(&mut document, space, Point::new(60.0, 10.0)));
        assert!(session.path_press(&mut document, space, Point::new(60.0, 60.0)));
        assert!(session.path.is_some());
        assert!(session.path_press(&mut document, space, Point::new(10.0, 10.0)));
        assert!(session.path.is_none(), "the ring ends the session");
        assert_eq!(document.len(), 1);
        let (_, mark) = document.at(0).expect("one closed path");
        let MarkKind::Bezier(path) = &mark.kind else {
            panic!("a Bezier tool draws a Bezier mark");
        };
        assert!(path.closed);
        assert_eq!(path.anchors.len(), 3);
        assert_eq!(document.depth(), 1, "the whole path is one history step");
    }

    /// An open path is one step when it is finished, and Escape leaves no preview or
    /// history step at all. These are the two ways an unfinished exclusive session ends.
    #[test]
    fn an_open_bezier_path_is_one_step_of_history() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.tool = Some(Tool::Bezier);
        let space = Space::new(Rect::new(0.0, 0.0, 100.0, 100.0), (100, 100));
        assert!(session.path_press(&mut document, space, Point::new(10.0, 10.0)));
        assert!(session.path_press(&mut document, space, Point::new(60.0, 60.0)));
        assert!(session.finish_path(&mut document));
        assert_eq!(document.depth(), 1);
        assert_eq!(document.len(), 1);

        assert!(session.path_press(&mut document, space, Point::new(20.0, 20.0)));
        assert!(session.escape(&mut document), "Escape discards the preview");
        assert_eq!(document.len(), 1);
        assert_eq!(document.depth(), 1);
        assert!(session.path.is_none());
    }

    /// Escape takes the unfinished layers and stops there: the tool and the selection have
    /// chords of their own, so one Escape is enough to leave the session.
    #[test]
    fn escape_leaves_the_tool_and_the_selection_to_their_own_chords() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.tool = Some(Tool::Pen);
        session.commit_drawing(
            &mut document,
            Drawing {
                from: Point::new(0.0, 0.0),
                to: Point::new(9.0, 9.0),
                points: vec![Point::new(0.0, 0.0), Point::new(9.0, 9.0)],
            },
        );
        let id = document.marks().next().map(|(id, _)| id).expect("one mark");
        assert!(session.begin_points(&mut document, id));
        assert!(session.node_points(&document) > 0);
        assert!(session.escape(&mut document), "points close first");
        assert_eq!(session.node_points(&document), 0);
        assert!(session.selected.is_some(), "the mark stays selected");
        assert!(
            !session.escape(&mut document),
            "nothing unfinished is left, so Escape falls through to leaving"
        );
        assert!(session.deselect(), "Ctrl+D puts the mark down");
        assert!(session.selected.is_none());
        assert!(session.cursor(&mut document), "V puts the cursor back");
        assert!(!session.armed(), "no tool is left armed");
        assert!(!session.deselect(), "and there is nothing left to put down");
    }

    /// A typed mark exists while it is typed and becomes one step of history when the
    /// field closes; nothing typed is left in the document as a second step.
    #[test]
    fn a_typed_mark_is_one_step_of_history() {
        let mut document = Document::new();
        let mut session = Session::new();
        assert!(session.begin_text(&mut document, Point::new(12.0, 34.0)));
        assert_eq!(document.len(), 1, "the mark is drawn while it is empty");
        assert_eq!(
            document
                .get(session.text.as_ref().expect("a field is open").id)
                .map(|mark| mark.bounds().origin),
            Some(Point::new(12.0, 34.0)),
            "an empty mark has no width, so its box starts at its point"
        );

        let typed = session.text.as_ref().expect("a field is open").id;
        let painted = document.revision();
        assert!(session.update_text(&mut document, "first".into()));
        assert!(session.update_text(&mut document, "first line".into()));
        assert!(
            document.revision() > painted,
            "the canvas paints the mark as it is typed, so a keystroke repaints it"
        );
        assert!(
            session.escape(&mut document),
            "Escape closes the field first"
        );
        assert!(session.text.is_none());
        assert_eq!(document.len(), 1);
        assert!(
            document.revision() > painted,
            "the closed mark joins the picture, so the canvas repaints once"
        );

        let undo = document.undo().expect("the typed mark is one step");
        assert_eq!(undo.id, typed, "and it is the mark's own insertion");
        assert_eq!(document.len(), 0);
        assert!(document.redo().is_some());
        assert!(
            matches!(
                document.marks().next().map(|(_, mark)| &mark.kind),
                Some(MarkKind::Text(text)) if text.text == "first line"
            ),
            "redo puts the typed text back"
        );
    }

    /// A field nobody typed into leaves no mark and no history step behind.
    #[test]
    fn an_untouched_text_mark_leaves_nothing() {
        let mut document = Document::new();
        let mut session = Session::new();
        assert!(session.begin_text(&mut document, Point::new(4.0, 4.0)));
        session.update_text(&mut document, "   ".into());
        assert!(session.escape(&mut document));
        assert_eq!(document.len(), 0, "whitespace is not a mark");
        assert!(
            document.undo().is_none(),
            "and it never entered the history"
        );
    }

    /// Reopening a text mark keeps the old text as what undo returns to, not the empty
    /// document the mark was born in.
    #[test]
    fn a_text_mark_reopens_with_its_text() {
        let mut document = Document::new();
        let mut session = Session::new();
        session.begin_text(&mut document, Point::new(0.0, 0.0));
        session.update_text(&mut document, "one".into());
        session.finish_text(&mut document);
        let id = document.marks().next().map(|(id, _)| id).expect("one mark");

        assert!(session.begin_text_of(&mut document, id));
        session.update_text(&mut document, "two".into());
        assert!(session.escape(&mut document), "Escape commits the new text");
        assert!(matches!(
            document.get(id).map(|mark| &mark.kind),
            Some(MarkKind::Text(text)) if text.text == "two"
        ));

        document.undo();
        assert!(
            matches!(
                document.get(id).map(|mark| &mark.kind),
                Some(MarkKind::Text(text)) if text.text == "one"
            ),
            "undo returns the text the field opened with"
        );
    }

    /// Escape steps back through the unfinished layers, and history walks the document under
    /// them; a selected mark is not one of those layers.
    #[test]
    fn escape_and_history_walk_their_own_layers() {
        let mut document = Document::new();
        let mut session = Session::new();
        assert!(!session.escape(&mut document), "nothing to step back to");

        session.drawing = Some(Drawing {
            from: Point::new(0.0, 0.0),
            to: Point::new(4.0, 4.0),
            points: vec![Point::new(0.0, 0.0)],
        });
        assert!(session.escape(&mut document), "a live drag goes first");
        assert!(session.drawing.is_none());

        let edit = document.insert_edit(Mark {
            kind: MarkKind::Rect(Rect::new(0.0, 0.0, 4.0, 4.0)),
            style: Style::default(),
        });
        let id = edit.id;
        document.commit(edit);
        session.selected = Some(id);
        assert!(session.deselect(), "Ctrl+D puts the selection down");
        assert!(session.selected.is_none());
        assert!(
            !session.escape(&mut document),
            "a selected mark is no layer: the session is the last one"
        );

        assert!(session.history(&mut document, false));
        assert_eq!(document.len(), 0);
        assert!(session.history(&mut document, true));
        assert_eq!(document.len(), 1);
    }
}
