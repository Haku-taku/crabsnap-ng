//! The app's floating toolbar: a draggable pill of icon buttons that docks to the edges of
//! whatever it serves, with the armed tool's parameters inside the same pill.
//!
//! The toolbar knows no action and no subject of its own: a caller hands it the marks it
//! should show ([`Entry`], built from an annotation [`State`] and the surface's own buttons)
//! and says which rectangle it hangs around. The region overlay and a pin's annotation
//! session are the two callers today; a third costs a table and a `Toolbar` of its own.
//!
//! Interaction is not the toolbar's: every control carries an id, and the surface routes the
//! engine's reports back to the object that owns the value — an annotation session, or the
//! overlay itself. This module owns the surface the controls sit on, where it can be dragged
//! to, which edges it snaps to, and what the parameter chips and the colour picker look like.
//!
//! Placement is the module's own rather than the layout engine's anchored overlay: a pill
//! that follows a drag, hangs off any of four edges, and turns its layout a quarter turn
//! between the vertical and horizontal ones needs to know where it is between frames. The
//! members sit on offsets computed here, which is also the only place the turn exists at
//! all — the engine paints no rotational transform. The colour picker is the exception: it
//! must escape the bar's own box and flip at a viewport edge, which is exactly what the
//! engine's placement already does.

use std::sync::Arc;

use crabsnap_core::hotkeys::Shortcut;
use ui_core::cx::Cx as EngineCx;
use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect, Size, lerp};
use ui_core::layout::{Anchor, AxisSize, Grow, OverlayPlacement, SizeClass};
use ui_core::menu::MenuItem;
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, CachedKey, Change, CursorShape, Decl, Easing, Id, NodeKind, Sense, Weight};
use ui_i18n::{Catalog, Language};
use ui_platform::{Container, WidgetState};
use ui_platform::element::Element;
use ui_style::{Env, RoleName};
use ui_widgets::build::IMMEDIATE;
use ui_widgets::{Cx, IconSet, IconSource};
use ui_widgets_md3::spec::IconButtonSelection;
use ui_widgets_md3::{
    IconButtonParams, IconButtonSize, IconButtonStyle, IconButtonWidth, Md3IconButton, Md3Kit,
    Md3Options, ToolTipParams, icon_button_size,
};

const BAR: &str = "toolbar";

/// Padding inside the pill, in points.
const PAD: f32 = 4.0;
/// The gap between two members, in points.
const GAP: f32 = 2.0;
/// The grip's length along the bar, in points.
const GRIP: f32 = 24.0;
/// The divider's extent along the bar, in points.
const DIVIDER: f32 = 11.0;
/// How far a divider's line stops short of the pill's edge, in points.
const DIVIDER_INSET: f32 = 9.0;
/// A grip dot's radius, and the distance between two dot centres, in points.
const DOT: f32 = 1.25;
const DOT_STEP: f32 = 4.0;

/// The parameter chips' height, the gap between two of them, and the room the block keeps
/// from the members before it, in points.
const CHIP: f32 = 28.0;
const ROW_GAP: f32 = 6.0;
const PARAM_GAP: f32 = 6.0;
/// Each chip's extent along the bar, in points. None is wider than a slot is tall, so a
/// turned bar holds the block across its own thickness.
const CHIP_PAINT: f32 = 28.0;
const CHIP_NUMBER: f32 = 36.0;
const CHIP_CHOICE: f32 = 48.0;
/// The ink share of a chip's own body, so a chip reads on the pill under it.
const WASH: f32 = 0.06;
/// How long the parameter block takes to travel in or out of the pill.
const PARAMS: AnimSpec = AnimSpec::new(0.18, Easing::EaseOut);
/// The colour chip's disc radius and the checkerboard cell inside it, in points.
const DISC: f32 = 9.0;
const CHECK: f32 = 4.0;
/// The colour picker's swatch radius, cell, and padding, in points.
const SWATCH: f32 = 10.0;
const SWATCH_CELL: f32 = 26.0;
const PICKER_PAD: f32 = 8.0;

/// The gap between a docked bar and the edge it hangs from, in points.
const DOCK_GAP: f32 = 12.0;
/// How deep the snap band outside an edge reaches, in points.
const ZONE_DEPTH: f32 = 18.0;
/// The shortest and longest snap zone along an edge, and the gap between two, in points.
const ZONE_MIN: f32 = 120.0;
const ZONE_MAX: f32 = 240.0;
const ZONE_GAP: f32 = 12.0;
/// An edge at least this long is split into three zones; a shorter one gets a single one.
const ZONE_THREE: f32 = 3.0 * ZONE_MIN + 2.0 * ZONE_GAP;
/// The snap zones' own corner, in points.
const ZONE_CORNER: f32 = 3.0;

/// The snap bands' wash and hairline, and the one the bar would take.
const ZONE_FILL: Color = Color::rgba(1.0, 1.0, 1.0, 0.08);
const ZONE_EDGE: Color = Color::rgba(1.0, 1.0, 1.0, 0.5);
const ZONE_ACTIVE_FILL: Color = Color::rgba(0.867, 0.882, 1.0, 0.28);
const ZONE_ACTIVE_EDGE: Color = Color::rgba(0.867, 0.882, 1.0, 0.95);

/// The wash a loupe held up by the secondary button shows: a colour of its own, so the
/// temporary visit is never mistaken for the switch left on.
const LOUPE_TEMPORARY: Color = Color::rgba(1.0, 0.72, 0.22, 0.30);

/// A transparent fill's checkerboard: light and dark, and the hairline around it.
const CHECK_LIGHT: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
const CHECK_DARK: Color = Color::rgba(188.0 / 255.0, 188.0 / 255.0, 192.0 / 255.0, 1.0);

/// The quarter turn between the two layouts, linear like the legacy app's 260 ms.
const TURN: AnimSpec = AnimSpec::new(0.26, Easing::Linear);
/// How long the bar takes to travel the rest of the way after a drop commits it.
const SNAP: AnimSpec = AnimSpec::new(0.20, Easing::EaseOut);
/// How a bar fades out of the way while a mark is being drawn.
const FADE: AnimSpec = AnimSpec::new(0.14, Easing::EaseOut);
/// How long the pointer must rest on a button before its tooltip shows, in seconds: a write
/// that lands once the wait is over, and one that undoes itself at once when it leaves.
const DWELL: AnimSpec = AnimSpec::new(0.0, Easing::Linear)
    .delayed(0.6)
    .down(0.0, Easing::Linear);

/// The pill's own shadow: a floating bar must read as above the picture.
const SHADOW: ShadowSpec = ShadowSpec::black(18.0, 0.25, 4.0);

/// The one button shape the bar uses, so the pill's spacing and the buttons' boxes are one
/// decision.
const BUTTON_STYLE: IconButtonStyle = IconButtonStyle::Standard;
const BUTTON_SIZE: IconButtonSize = IconButtonSize::Small;
const BUTTON_WIDTH: IconButtonWidth = IconButtonWidth::Default;

/// The colours the picker offers, in reading order.
pub const PALETTE: [Color; 8] = [
    Color::rgba(240.0 / 255.0, 240.0 / 255.0, 240.0 / 255.0, 1.0),
    Color::rgba(24.0 / 255.0, 26.0 / 255.0, 30.0 / 255.0, 1.0),
    Color::rgba(235.0 / 255.0, 60.0 / 255.0, 50.0 / 255.0, 1.0),
    Color::rgba(240.0 / 255.0, 150.0 / 255.0, 40.0 / 255.0, 1.0),
    Color::rgba(240.0 / 255.0, 210.0 / 255.0, 50.0 / 255.0, 1.0),
    Color::rgba(60.0 / 255.0, 190.0 / 255.0, 110.0 / 255.0, 1.0),
    Color::rgba(70.0 / 255.0, 150.0 / 255.0, 250.0 / 255.0, 1.0),
    Color::rgba(165.0 / 255.0, 110.0 / 255.0, 240.0 / 255.0, 1.0),
];

/// The bar's node, which the app also reads the measured rect of.
pub fn id() -> Id {
    Id::str(BAR)
}

/// The node the pointer grabs to drag the bar.
pub fn grip() -> Id {
    id().with("grip")
}

/// The parameter row, and the colour picker it can open.
fn params_id() -> Id {
    id().with("params")
}

fn picker_id() -> Id {
    id().with("picker")
}

/// One mark on a toolbar: the id its press is reported under, its icon, and the message
/// whose text its tooltip shows. The toolbar itself carries no text.
#[derive(Clone, Debug)]
pub struct Tool {
    pub id: Id,
    pub icon: IconSource,
    /// A Fluent message id, resolved at build time so a language switch reaches the tooltip.
    pub message: &'static str,
    /// An inert mark is drawn disabled rather than promising something the surface cannot do.
    pub enabled: bool,
    /// A toggle's state; `None` for a plain button.
    pub selected: Option<bool>,
    /// The bindable action this mark runs; marks that no chord reaches have none.
    pub shortcut: Option<Shortcut>,
    /// The chord its tooltip shows, resolved from the plan by [`Tool::hinted`].
    pub hint: Option<String>,
}

impl Tool {
    pub fn new(id: Id, icon: IconSource, message: &'static str) -> Self {
        Self {
            id,
            icon,
            message,
            enabled: true,
            selected: None,
            shortcut: None,
            hint: None,
        }
    }

    /// Resolve this mark's chord from the file's plan, for the tooltip.
    ///
    /// An action cleared in the file names no chord, and then the tooltip simply has none.
    pub fn hinted(mut self, plan: &crate::hotkeys::Plan) -> Self {
        self.hint = self.shortcut.and_then(|action| plan.hint(action));
        self
    }

    fn toggled(mut self, on: bool) -> Self {
        self.selected = Some(on);
        self
    }
}

/// What a toolbar mark can mean to this app.
///
/// The module above paints whatever it is handed; this table and the two below are the app's
/// own use of it. A surface that needs another mark adds it here and to its list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pin,
    Copy,
    Save,
    Cancel,
    Confirm,
}

impl Action {
    pub fn id(self) -> Id {
        Id::of((BAR, self.message()))
    }

    fn message(self) -> &'static str {
        match self {
            Action::Pin => "action-pin",
            Action::Copy => "action-copy",
            Action::Save => "action-save",
            Action::Cancel => "action-cancel",
            Action::Confirm => "action-confirm",
        }
    }

    fn tool(self) -> Tool {
        let mut tool = Tool::new(
            self.id(),
            IconSource::glyph(
                match self {
                    Action::Pin => "push_pin",
                    Action::Copy => "content_copy",
                    Action::Save => "save",
                    Action::Cancel => "close",
                    Action::Confirm => "check",
                },
                IconSet::Outlined,
            ),
            self.message(),
        );
        tool.shortcut = self.shortcut();
        tool
    }

    /// The bindable action this mark runs; only the session's own buttons have one.
    fn shortcut(self) -> Option<Shortcut> {
        match self {
            Action::Pin => Some(Shortcut::Pin),
            Action::Copy => Some(Shortcut::Copy),
            Action::Save => Some(Shortcut::Save),
            Action::Cancel => Some(Shortcut::Cancel),
            Action::Confirm => Some(Shortcut::Confirm),
        }
    }
}

/// The marks a finished selection offers, in the order they run.
pub fn selection() -> Vec<Tool> {
    [Action::Pin, Action::Copy, Action::Save, Action::Cancel]
        .map(Action::tool)
        .to_vec()
}

/// The marks an annotation session offers: keep the marks drawn, or drop them.
pub fn pin_edit() -> Vec<Tool> {
    [Action::Confirm, Action::Cancel]
        .map(Action::tool)
        .to_vec()
}

/// The cursor: the default, which moves the selection instead of drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Cursor,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Pen,
    Bezier,
    Text,
}

impl Mark {
    pub const ALL: [Mark; 8] = [
        Mark::Cursor,
        Mark::Rect,
        Mark::Ellipse,
        Mark::Line,
        Mark::Arrow,
        Mark::Pen,
        Mark::Bezier,
        Mark::Text,
    ];

    pub fn id(self) -> Id {
        Id::of((BAR, "mark", self.key()))
    }

    fn key(self) -> &'static str {
        match self {
            Mark::Cursor => "cursor",
            Mark::Rect => "rect",
            Mark::Ellipse => "ellipse",
            Mark::Line => "line",
            Mark::Arrow => "arrow",
            Mark::Pen => "pen",
            Mark::Bezier => "bezier",
            Mark::Text => "text",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Mark::Cursor => "toolbar-cursor",
            Mark::Rect => "pin-edit-rect",
            Mark::Ellipse => "pin-edit-ellipse",
            Mark::Line => "pin-edit-line",
            Mark::Arrow => "pin-edit-arrow",
            Mark::Pen => "pin-edit-pen",
            Mark::Bezier => "pin-edit-bezier",
            Mark::Text => "pin-edit-text",
        }
    }

    fn icon(self) -> IconSource {
        IconSource::glyph(
            match self {
                Mark::Cursor => "arrow_selector_tool",
                Mark::Rect => "rectangle",
                Mark::Ellipse => "circle",
                Mark::Line => "horizontal_rule",
                Mark::Arrow => "arrow_right_alt",
                Mark::Pen => "ink_pen",
                Mark::Bezier => "polyline",
                Mark::Text => "text_fields",
            },
            IconSet::Outlined,
        )
    }

    fn tool(self, armed: Option<Mark>) -> Tool {
        let mut tool = Tool::new(self.id(), self.icon(), self.message()).toggled(armed == Some(self));
        // A drawing tool is chosen by pressing it or by nothing at all; the cursor is the one
        // the keyboard reaches on its own, so it is the one whose tooltip can name a chord.
        if self == Mark::Cursor {
            tool.shortcut = Some(Shortcut::Cursor);
        }
        tool
    }
}

/// The annotation families a bar shows as one slot each, switched by wheel or menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Shape,
    Line,
    Pen,
}

impl Family {
    pub const ALL: [Family; 3] = [Family::Shape, Family::Line, Family::Pen];

    pub fn children(self) -> &'static [Mark] {
        match self {
            Family::Shape => &[Mark::Rect, Mark::Ellipse],
            Family::Line => &[Mark::Line, Mark::Arrow],
            Family::Pen => &[Mark::Pen, Mark::Bezier],
        }
    }

    pub fn index(self) -> usize {
        match self {
            Family::Shape => 0,
            Family::Line => 1,
            Family::Pen => 2,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Family::Shape => "shape",
            Family::Line => "line",
            Family::Pen => "pen",
        }
    }

    /// The cluster's slot id: a wheel over it and its menu are reported here, while a click
    /// is reported at the button's own id, the one the family's current child carries.
    pub fn slot(self) -> Id {
        Id::of((BAR, "family", self.key(), "slot"))
    }

    /// The family's button id, which a click reports.
    pub fn id(self) -> Id {
        Id::of((BAR, "family", self.key()))
    }
}

/// The option a family's menu lists for its `index`-th child.
pub fn option_id(family: Family, index: usize) -> Id {
    Id::of((BAR, "family", family.key(), "option", index))
}

/// Which colour the loupe button is in: the switch off, the secondary button holding it up
/// for the moment, and the switch left on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoupeState {
    #[default]
    Off,
    Temporary,
    On,
}

/// The loupe's button id.
pub fn loupe() -> Id {
    Id::of((BAR, "loupe"))
}

/// The state a bar is built from: which mode is armed, which child each family shows, the
/// loupe's state, and whether a mark is being drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub armed: Option<Mark>,
    pub family: [usize; 3],
    pub loupe: LoupeState,
    pub drawing: bool,
}

impl State {
    /// The child a family shows, clamped to the family's own children.
    fn current(&self, family: Family) -> usize {
        self.family[family.index()].min(family.children().len() - 1)
    }
}

/// One member of the bar, in reading order after the grip.
pub enum Entry {
    /// A plain mark: the cursor, a standalone tool, a session action.
    Tool(Tool),
    /// A family collapsed to its current child, with every child in its menu.
    Group {
        family: Family,
        /// The button's label, which is the shown child's.
        message: &'static str,
        icon: IconSource,
        selected: bool,
    },
    /// The loupe switch, whose colour says how it is up.
    Loupe { state: LoupeState },
    /// The hairline between the tools and the session's own buttons.
    Divider,
}

/// The bar's entries for one surface: the cursor, the three families, the text tool, the
/// loupe, a divider, then the session's own actions.
pub fn entries(state: State, actions: &[Tool], plan: &crate::hotkeys::Plan) -> Vec<Entry> {
    let mut out = Vec::with_capacity(actions.len() + 7);
    out.push(Entry::Tool(Mark::Cursor.tool(state.armed).hinted(plan)));
    for family in Family::ALL {
        let children = family.children();
        let current = state.current(family);
        let shown = children[current];
        out.push(Entry::Group {
            family,
            message: shown.message(),
            icon: shown.icon(),
            selected: state.armed == Some(shown),
        });
    }
    out.push(Entry::Tool(Mark::Text.tool(state.armed)));
    out.push(Entry::Loupe { state: state.loupe });
    out.push(Entry::Divider);
    out.extend(actions.iter().cloned().map(Entry::Tool));
    out
}

/// A value field's identity: the chip, the swatches it can open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Stroke,
    Fill,
    Width,
    Heads,
    /// The ink drawn around a text mark's glyphs.
    Outline,
    /// The hairline around a text mark's frame.
    Border,
    Size,
    Padding,
    Radius,
    Align,
}

impl Field {
    pub const ALL: [Field; 10] = [
        Field::Stroke,
        Field::Fill,
        Field::Width,
        Field::Heads,
        Field::Outline,
        Field::Border,
        Field::Size,
        Field::Padding,
        Field::Radius,
        Field::Align,
    ];

    fn key(self) -> &'static str {
        match self {
            Field::Stroke => "stroke",
            Field::Fill => "fill",
            Field::Width => "width",
            Field::Heads => "heads",
            Field::Outline => "outline",
            Field::Border => "border",
            Field::Size => "size",
            Field::Padding => "padding",
            Field::Radius => "radius",
            Field::Align => "align",
        }
    }

    /// Whether the field's colour may also be "none at all".
    fn clearable(self) -> bool {
        matches!(self, Field::Fill | Field::Outline | Field::Border)
    }
}

/// The chip a field is set at.
pub fn field(field: Field) -> Id {
    Id::of((BAR, "field", field.key()))
}

/// The `index`-th palette swatch of `field`.
pub fn swatch(field: Field, index: usize) -> Id {
    Id::of((BAR, "swatch", field.key(), index))
}

/// The "no fill" swatch of `field`.
pub fn no_fill(field: Field) -> Id {
    Id::of((BAR, "swatch", field.key(), "none"))
}

/// The `index`-th row of a choice field's menu.
pub fn option(field: Field, index: usize) -> Id {
    Id::of((BAR, "option", field.key(), index))
}

/// The eyedropper of `field`.
pub fn dropper(field: Field) -> Id {
    Id::of((BAR, "dropper", field.key()))
}

/// Whether `id` names a control this module drew. A surface that shows the bar routes these
/// to whatever owns the value; nothing else may claim them.
pub fn owns(id: Id) -> bool {
    if id == loupe() || id == params_id() || id == picker_id() {
        return true;
    }
    if Mark::ALL.iter().any(|mark| mark.id() == id) {
        return true;
    }
    if Family::ALL
        .iter()
        .any(|family| family.id() == id || family.slot() == id)
    {
        return true;
    }
    for family in Family::ALL {
        if (0..family.children().len()).any(|index| option_id(family, index) == id) {
            return true;
        }
    }
    for which in Field::ALL {
        if field(which) == id
            || dropper(which) == id
            || no_fill(which) == id
            || (0..PALETTE.len()).any(|index| swatch(which, index) == id)
        {
            return true;
        }
    }
    false
}

/// A parameter chip's content, in the order the block lays them out.
#[derive(Clone)]
pub enum Param {
    /// The paint: a ring for the stroke and a disc for the fill, both picked from the
    /// palette in one popup. `fillable` says whether the tool paints an interior at all.
    Paint {
        ink: Color,
        fill: Option<Color>,
        fillable: bool,
        open: bool,
    },
    /// One ink of its own, beyond the paint's two: a text mark's glyph outline or its
    /// frame's hairline. `None` is that ink switched off, which the picker can also choose.
    Swatch {
        field: Field,
        colour: Option<Color>,
        open: bool,
    },
    /// A number, raised and lowered by the wheel.
    Number {
        field: Field,
        value: f32,
        message: &'static str,
    },
    /// One of a fixed set, cycled by the wheel or chosen from the chip's own menu.
    Choice {
        field: Field,
        message: &'static str,
        options: &'static [&'static str],
        current: usize,
    },
}

impl Param {
    /// The chip's node and the message that names it in a tooltip.
    fn label(&self) -> (Id, &'static str) {
        match self {
            Param::Paint { .. } => (field(Field::Stroke), "toolbar-colour"),
            Param::Swatch { field: which, .. } => (field(*which), swatch_message(*which)),
            Param::Number {
                field: which,
                message,
                ..
            }
            | Param::Choice {
                field: which,
                message,
                ..
            } => (field(*which), message),
        }
    }
}

/// The message a swatch chip's tooltip shows.
fn swatch_message(which: Field) -> &'static str {
    match which {
        Field::Outline => "toolbar-outline",
        Field::Border => "toolbar-border",
        _ => "toolbar-colour",
    }
}

/// The sigma of the frost behind the right-click menu, in points.
///
/// A pin's menu covers the picture it annotates, so the surface gives it back as a blur
/// instead of hiding it under an opaque slab.
const MENU_FROST: f32 = 16.0;

/// The scope every control in this app builds under: the app's language, then MD3's options.
pub fn env(language: &Arc<Language>) -> Arc<Env> {
    let catalogued = Env::with(Some(Env::root()), Arc::clone(language));
    let options = Md3Options {
        menu_blur: MENU_FROST,
        ..Md3Options::default()
    };
    Env::with(Some(catalogued), options)
}

/// An edge of the area a docked bar hangs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left];

    /// The edge on the far side of the area: where a bar with no room here flips to.
    fn opposite(self) -> Edge {
        match self {
            Edge::Top => Edge::Bottom,
            Edge::Bottom => Edge::Top,
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
        }
    }

    /// Which way a bar docked here runs: along the area's width, or its height.
    fn orientation(self) -> Orientation {
        match self {
            Edge::Top | Edge::Bottom => Orientation::Horizontal,
            Edge::Left | Edge::Right => Orientation::Vertical,
        }
    }

    /// Where this edge starts along itself, and how long it is.
    fn span(self, area: Rect) -> (f32, f32) {
        match self {
            Edge::Top | Edge::Bottom => (area.min_x(), area.size.w),
            Edge::Left | Edge::Right => (area.min_y(), area.size.h),
        }
    }
}

/// Where along an edge a docked bar sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

impl Align {
    /// The offset of a `size`-long bar within an edge `span` long, from `origin`.
    fn along(self, origin: f32, span: f32, size: f32) -> f32 {
        match self {
            Align::Start => origin,
            Align::Center => origin + (span - size) * 0.5,
            Align::End => origin + span - size,
        }
    }
}

/// The two layouts: members left to right, or top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    /// How far the layout has turned: nought across, one down.
    fn turn(self) -> f32 {
        match self {
            Orientation::Horizontal => 0.0,
            Orientation::Vertical => 1.0,
        }
    }
}

/// Where the bar hangs: docked to an edge, or free on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Dock {
    Anchored { edge: Edge, align: Align },
    Floating(Point),
}

impl Dock {
    fn orientation(self) -> Orientation {
        match self {
            Dock::Anchored { edge, .. } => edge.orientation(),
            // Nothing suggests the other axis for a bar that hangs from nothing.
            Dock::Floating(_) => Orientation::Horizontal,
        }
    }
}

/// One snap target: the band outside an edge, and where on that edge a bar would land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub edge: Edge,
    pub align: Align,
    pub rect: Rect,
}

/// The snap zones along one edge, from left to right or top to bottom.
pub fn zones(area: Rect, edge: Edge) -> Vec<Zone> {
    let (origin, span) = edge.span(area);
    // Each zone has to sit over the place `Align::along` docks the bar, or the band a drop
    // lands on and the place it lands disagree. Start and End therefore hug the edge ends
    // rather than following the centre zone; a span too short for three bands gets one
    // centred band, which still says "dock here" without crowding.
    if span < ZONE_THREE {
        return vec![Zone {
            edge,
            align: Align::Center,
            rect: band(edge, area, origin + (span - span.min(ZONE_MAX)) * 0.5, span.min(ZONE_MAX)),
        }];
    }
    let length = ((span - 2.0 * ZONE_GAP) / 3.0).min(ZONE_MAX);
    [
        (Align::Start, origin),
        (Align::Center, origin + (span - length) * 0.5),
        (Align::End, origin + span - length),
    ]
    .into_iter()
    .map(|(align, at)| Zone {
        edge,
        align,
        rect: band(edge, area, at, length),
    })
    .collect()
}

/// The band `depth` outside `edge`, `length` long from `at` along it.
fn band(edge: Edge, area: Rect, at: f32, length: f32) -> Rect {
    match edge {
        Edge::Top => Rect::new(at, area.min_y() - ZONE_DEPTH, length, ZONE_DEPTH),
        Edge::Bottom => Rect::new(at, area.max_y(), length, ZONE_DEPTH),
        Edge::Left => Rect::new(area.min_x() - ZONE_DEPTH, at, ZONE_DEPTH, length),
        Edge::Right => Rect::new(area.max_x(), at, ZONE_DEPTH, length),
    }
}

/// The zone `bar` overlaps most, if it overlaps one at all: the drop's decision.
pub fn snap(area: Rect, bar: Rect) -> Option<Zone> {
    let mut best: Option<(f32, Zone)> = None;
    for edge in Edge::ALL {
        for zone in zones(area, edge) {
            let overlap = zone
                .rect
                .intersection(bar)
                .map_or(0.0, |shared| shared.size.w * shared.size.h);
            if overlap > 0.0 && best.is_none_or(|(most, _)| overlap > most) {
                best = Some((overlap, zone));
            }
        }
    }
    best.map(|(_, zone)| zone)
}

/// Where a bar of `size` hangs when docked to `edge` with `align`.
fn docked(area: Rect, edge: Edge, align: Align, size: Size) -> Point {
    match edge {
        Edge::Top => Point::new(
            align.along(area.min_x(), area.size.w, size.w),
            area.min_y() - DOCK_GAP - size.h,
        ),
        Edge::Bottom => Point::new(
            align.along(area.min_x(), area.size.w, size.w),
            area.max_y() + DOCK_GAP,
        ),
        Edge::Left => Point::new(
            area.min_x() - DOCK_GAP - size.w,
            align.along(area.min_y(), area.size.h, size.h),
        ),
        Edge::Right => Point::new(
            area.max_x() + DOCK_GAP,
            align.along(area.min_y(), area.size.h, size.h),
        ),
    }
}

/// The docked position: this edge when the viewport holds it, else the far edge, else
/// pushed back inside.
fn fitted(screen: Rect, area: Rect, edge: Edge, align: Align, size: Size) -> Point {
    let near = docked(area, edge, align, size);
    if holds(screen, Rect::from_origin_size(near, size)) {
        return near;
    }
    let far = docked(area, edge.opposite(), align, size);
    if holds(screen, Rect::from_origin_size(far, size)) {
        return far;
    }
    clamped(screen, Rect::from_origin_size(near, size))
}

/// Whether `rect` lies entirely within `outer`.
fn holds(outer: Rect, rect: Rect) -> bool {
    rect.min_x() >= outer.min_x()
        && rect.min_y() >= outer.min_y()
        && rect.max_x() <= outer.max_x()
        && rect.max_y() <= outer.max_y()
}

/// `rect`'s origin pulled back inside `outer`, as far as its size allows.
fn clamped(outer: Rect, rect: Rect) -> Point {
    Point::new(
        rect
            .origin
            .x
            .clamp(outer.min_x(), (outer.max_x() - rect.size.w).max(outer.min_x())),
        rect
            .origin
            .y
            .clamp(outer.min_y(), (outer.max_y() - rect.size.h).max(outer.min_y())),
    )
}

/// Which side of the buttons lane the parameter block takes: the one facing away from what
/// the bar hangs around, so the block opens into free screen space.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Before,
    After,
}

impl Side {
    fn other(self) -> Self {
        match self {
            Side::Before => Side::After,
            Side::After => Side::Before,
        }
    }

    /// The pill's origin when its buttons lane stands at `at`. The buttons must not move
    /// when the block opens, so a block on the near side takes its thickness off the
    /// origin instead.
    fn origin(self, at: Point, grown: f32, orientation: Orientation) -> Point {
        match self {
            Side::After => at,
            Side::Before => match orientation {
                Orientation::Horizontal => Point::new(at.x, at.y - grown),
                Orientation::Vertical => Point::new(at.x - grown, at.y),
            },
        }
    }
}

/// How far the block has opened and the side it took: the pill's second lane this frame.
#[derive(Clone, Copy, Debug)]
struct Open {
    grown: f32,
    side: Side,
}

/// The pill's size in one layout, `extents` being each button's extent along it, the grip's
/// included, and `grown` the thickness the block has opened across it.
fn pill_size(orientation: Orientation, extents: &[f32], slot: Size, grown: f32) -> Size {
    let thickness = slot.h + 2.0 * PAD + grown;
    let length = 2.0 * PAD + extents.iter().sum::<f32>() + GAP * (extents.len() - 1) as f32;
    match orientation {
        Orientation::Horizontal => Size::new(length, thickness),
        Orientation::Vertical => Size::new(thickness, length),
    }
}

/// The pill's box `turn` of the way from one layout to the other.
fn turning(turn: f32, extents: &[f32], slot: Size, open: Open) -> Size {
    let across = pill_size(Orientation::Horizontal, extents, slot, open.grown);
    let down = pill_size(Orientation::Vertical, extents, slot, open.grown);
    Size::new(lerp(across.w, down.w, turn), lerp(across.h, down.h, turn))
}

/// The two lanes' cross offsets from the pill's near edge: the buttons lane's, and the
/// block strip's origin and thickness. The strip is laid out at its full thickness from the
/// first frame and revealed by the pill's own box, so its chips never squeeze.
fn lanes(open: Open, slot: Size) -> (f32, (f32, f32)) {
    let grown = open.grown;
    match open.side {
        Side::After => (PAD, (PAD + slot.h + PARAM_GAP, slot.h)),
        Side::Before => (PAD + grown, (PAD + grown - PARAM_GAP - slot.h, slot.h)),
    }
}

/// Member `k`'s centre measured from the pill's leading edge: the grip first, then the rest.
fn member_at(k: usize, extents: &[f32]) -> f32 {
    let before: f32 = extents[..k].iter().sum();
    PAD + before + GAP * k as f32 + extents[k] * 0.5
}

/// Member `k`'s box at `turn`: its extent runs along the bar before the turn, down after.
fn member_size(k: usize, turn: f32, extents: &[f32], slot: Size) -> Size {
    Size::new(lerp(extents[k], slot.h, turn), lerp(slot.h, extents[k], turn))
}

/// Member `k`'s centre inside the pill's box, `turn` of the way from a row to a column.
///
/// The centre travels straight between the two layouts rather than round an arc. With the
/// box interpolated between the two sizes, the straight path is the one that keeps every
/// member inside the box at every step; an arc swings the ends outside it.
fn member_centre(k: usize, turn: f32, extents: &[f32], slot: Size, open: Open) -> Point {
    let box_ = turning(turn, extents, slot, open);
    let across = pill_size(Orientation::Horizontal, extents, slot, open.grown);
    // How far from the pill's centre the button sits: along the bar, and off its centre
    // line by however much of the block's lane the buttons are pushed aside for.
    let reach = member_at(k, extents) - across.w * 0.5;
    let (buttons, _) = lanes(open, slot);
    let off = buttons + slot.h * 0.5 - across.h * 0.5;
    Point::new(
        box_.w * 0.5 + lerp(reach, off, turn),
        box_.h * 0.5 + lerp(off, reach, turn),
    )
}

/// The block strip's box inside the pill, `turn` of the way from a row to a column: it
/// spans the pill along the bar and sits in the block's lane across it.
fn strip_box(turn: f32, extents: &[f32], slot: Size, open: Open) -> Rect {
    let across = pill_size(Orientation::Horizontal, extents, slot, open.grown);
    let down = pill_size(Orientation::Vertical, extents, slot, open.grown);
    let box_ = turning(turn, extents, slot, open);
    let (_, (offset, thickness)) = lanes(open, slot);
    let span = across.w - 2.0 * PAD;
    // The two centres relative to the pill's own, so the strip travels with the box the
    // same way its buttons do.
    let row = Point::new(
        PAD + span * 0.5 - across.w * 0.5,
        offset + thickness * 0.5 - across.h * 0.5,
    );
    let column = Point::new(
        offset + thickness * 0.5 - down.w * 0.5,
        PAD + span * 0.5 - down.h * 0.5,
    );
    let centre = Point::new(
        box_.w * 0.5 + lerp(row.x, column.x, turn),
        box_.h * 0.5 + lerp(row.y, column.y, turn),
    );
    let size = Size::new(
        lerp(span, thickness, turn),
        lerp(thickness, span, turn),
    );
    Rect::from_origin_size(
        Point::new(centre.x - size.w * 0.5, centre.y - size.h * 0.5),
        size,
    )
}

/// Each member's extent along the bar, grip first.
fn extents(entries: &[Entry], slot: Size) -> Vec<f32> {
    let mut out = Vec::with_capacity(entries.len() + 1);
    out.push(GRIP);
    for entry in entries {
        out.push(match entry {
            Entry::Divider => DIVIDER,
            _ => slot.w,
        });
    }
    out
}

/// One member's box, straight from the icon button's own tokens so the two cannot drift.
fn slot_size() -> Size {
    let (width, height, _) = icon_button_size(
        BUTTON_STYLE,
        IconButtonSelection::Base,
        BUTTON_SIZE,
        BUTTON_WIDTH,
    );
    Size::new(width, height)
}

/// The drag in progress: where it started, and what to restore if it is cancelled.
struct Drag {
    /// The bar's top-left when the drag began. The engine reports travel from the press, so
    /// the placement is this point plus that travel, never an accumulation of steps.
    from: Point,
    before: Dock,
    /// The layout a live drag keeps, so the bar does not turn under the hand.
    orientation: Orientation,
}

/// The bar's own state: where it hangs, and whether the grip is held.
pub struct Toolbar {
    dock: Dock,
    drag: Option<Drag>,
    /// Whether the bar is still travelling to the place a drop committed it to.
    flight: bool,
    /// The parameter block the bar last showed. A block that vanished the frame the caller
    /// stopped offering it would be cut off rather than close, so it outlives the offer for
    /// as long as its fade takes.
    params: Vec<Param>,
    /// The box the bar took last frame, and the two frames it was placed against. The drag
    /// handlers run outside a build, so they need both remembered.
    placed: Rect,
    /// The buttons lane's origin last frame. A drag starts from here rather than from the
    /// pill's own corner, so an open block's lane does not shift the bar under the hand.
    base: Point,
    screen: Rect,
    area: Rect,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            // A fresh selection hangs the bar under itself, centred: where a reader looks.
            dock: Dock::Anchored {
                edge: Edge::Bottom,
                align: Align::Center,
            },
            drag: None,
            flight: false,
            params: Vec::new(),
            placed: Rect::new(0.0, 0.0, 0.0, 0.0),
            base: Point::new(0.0, 0.0),
            screen: Rect::new(0.0, 0.0, 0.0, 0.0),
            area: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// The chips this frame draws: what the caller offers, else whatever is still closing.
    fn block(&mut self, offered: &[Param], presence: f32) -> Vec<Param> {
        if !offered.is_empty() {
            self.params = offered.to_vec();
        } else if presence <= 0.01 {
            self.params.clear();
        }
        self.params.clone()
    }

    /// A new selection is a new subject: the bar goes back under it, centred.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The layout the bar is drawing towards, which a live drag holds still.
    fn orientation(&self) -> Orientation {
        match &self.drag {
            Some(drag) => drag.orientation,
            None => self.dock.orientation(),
        }
    }

    /// Where the buttons lane wants to stand this frame, before the block's own offset: the
    /// animation runs on this point so an opening block can never drag the lane with it. The
    /// side the block takes is the one facing away from the area, or the other one when the
    /// screen holds only that.
    fn place(
        &mut self,
        screen: Rect,
        area: Rect,
        extents: &[f32],
        slot: Size,
        grown: f32,
    ) -> (Point, Side, Size) {
        let orientation = self.orientation();
        let base = pill_size(orientation, extents, slot, 0.0);
        let full = pill_size(orientation, extents, slot, grown);
        let at = match &self.dock {
            Dock::Anchored { edge, align } => fitted(screen, area, *edge, *align, base),
            // A drag must follow the hand without fighting it; the drop puts a free bar
            // back on the screen.
            Dock::Floating(at) => *at,
        };
        let preferred = match &self.dock {
            Dock::Anchored { edge, align } => {
                // `fitted` hangs a bar that does not fit from the far edge instead; the
                // block follows it, or it would open over the area it just left.
                let anchor = if at == docked(area, *edge, *align, base) {
                    *edge
                } else {
                    edge.opposite()
                };
                match anchor {
                    Edge::Top | Edge::Left => Side::Before,
                    Edge::Bottom | Edge::Right => Side::After,
                }
            }
            Dock::Floating(_) => Side::After,
        };
        let fits = |side: Side| {
            let origin = side.origin(at, grown, orientation);
            holds(screen, Rect::from_origin_size(origin, full))
        };
        let side = if fits(preferred) || !fits(preferred.other()) {
            preferred
        } else {
            preferred.other()
        };
        self.screen = screen;
        self.area = area;
        (at, side, full)
    }

    /// The box the bar was drawn in, and the lane origin it was drawn from: the drag handlers
    /// run outside a build and must take their bearings from what is on the screen.
    fn settled(&mut self, origin: Point, base: Point, full: Size) {
        self.placed = Rect::from_origin_size(origin, full);
        self.base = base;
    }

    /// The grip is pressed: the bar comes loose where it stands.
    fn begin(&mut self) {
        if self.drag.is_some() {
            return;
        }
        self.drag = Some(Drag {
            from: self.base,
            before: self.dock,
            orientation: self.orientation(),
        });
        self.dock = Dock::Floating(self.base);
        self.flight = false;
    }

    /// The grip travelled: the bar follows, keeping the grip under the pointer.
    fn moved(&mut self, dx: f32, dy: f32) {
        if let Some(drag) = &self.drag {
            self.dock = Dock::Floating(Point::new(drag.from.x + dx, drag.from.y + dy));
        }
    }

    /// The grip is released: dock to the zone the bar overlaps most, or stay free on screen.
    fn drop(&mut self) {
        if self.drag.take().is_none() {
            return;
        }
        let bar = self.placed;
        self.dock = match snap(self.area, bar) {
            Some(zone) => Dock::Anchored {
                edge: zone.edge,
                align: zone.align,
            },
            None => Dock::Floating(clamped(self.screen, bar)),
        };
        // The hand let go somewhere else: the rest of the way is the bar's own travel.
        self.flight = true;
    }

    /// The drag was cancelled: back where it started, docked as it was.
    fn cancel(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.dock = drag.before;
            self.flight = true;
        }
    }

    /// Whether this frame's placement is a landing rather than a position to hold still.
    fn landing(&self) -> bool {
        self.flight && self.drag.is_none()
    }

    /// What the bar does with the engine's drag reports. Only the grip owns a drag.
    pub fn dragged(&mut self, id: Id, change: &ui_core::change::Change) -> bool {
        if id != grip() {
            return false;
        }
        match change {
            ui_core::change::Change::DragStarted => self.begin(),
            ui_core::change::Change::Moved { dx, dy } => self.moved(*dx as f32, *dy as f32),
            ui_core::change::Change::Dropped { .. } => self.drop(),
            ui_core::change::Change::Cancelled => self.cancel(),
            _ => return false,
        }
        true
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

/// What one build draws: the state that outlives it, the space it is placed in, and the
/// marks this surface offers.
pub struct View<'a> {
    pub state: &'a mut Toolbar,
    /// The surface's own box; a docked bar must stay inside it.
    pub screen: Rect,
    /// What the bar hangs around: the selection, or the pin being annotated.
    pub area: Rect,
    pub entries: &'a [Entry],
    pub params: &'a [Param],
    pub catalog: &'a Catalog,
    /// A mark is being drawn: the bar steps out of the way.
    pub drawing: bool,
}

/// Lay the bar out for this frame and build it: the snap zones, the pill, the parameter row,
/// the colour picker and a tooltip.
pub fn build(
    cx: &mut EngineCx<'_>,
    env: &Arc<Env>,
    kit: &Md3Kit,
    view: View<'_>,
) -> Box<dyn Element> {
    let View {
        state,
        screen,
        area,
        entries,
        params,
        catalog,
        drawing,
    } = view;
    let mut wcx = Cx::new(cx, Arc::clone(env));
    let pack = kit.pack_for(&wcx);
    let role = |name, fallback| pack.roles.get(RoleName(name)).unwrap_or(fallback);
    let surface = role(
        "surface-container-high",
        Color::from_rgba8(236, 230, 240, 255),
    );
    let outline = role("outline-variant", Color::from_rgba8(150, 150, 160, 255));
    // The grip sits on the pill, not on the picture: its dots are the pill's ink, or a white
    // mark on a light surface is no mark at all.
    let ink = role("on-surface-variant", Color::from_rgba8(73, 69, 79, 255));

    let slot = slot_size();
    // The block opens a second lane across the pill: the bar gets thicker, rather than a
    // second surface appearing beside it. `presence` is that opening, and it is the block's
    // opacity and its share of the new lane at once, so the two cannot disagree.
    let presence = wcx.at(id()).animate(
        id().with("params"),
        if params.is_empty() { 0.0 } else { 1.0 },
        PARAMS,
    );
    let block = state.block(params, presence);
    let grown = if block.is_empty() {
        0.0
    } else {
        (slot.h + PARAM_GAP) * presence
    };
    let extents = extents(entries, slot);
    let (target, side, full) = state.place(screen, area, &extents, slot, grown);
    // A held grip must sit under the hand; the travel to a snap point after the drop is what
    // the ease is for. The target is recomputed every frame, so a bar that turns while it
    // flies lands where the new layout puts it, not where the one it left did.
    let flight = if state.landing() { SNAP } else { IMMEDIATE };
    let base = Point::new(
        wcx.at(id()).animate(id().with("snap-x"), target.x, flight),
        wcx.at(id()).animate(id().with("snap-y"), target.y, flight),
    );
    if (base.x - target.x).abs() < 0.25 && (base.y - target.y).abs() < 0.25 {
        state.flight = false;
    }
    let mut at = side.origin(base, grown, state.orientation());
    if matches!(state.dock, Dock::Anchored { .. }) {
        at = clamped(screen, Rect::from_origin_size(at, full));
    }
    state.settled(at, base, full);
    let open = Open { grown, side };
    let turn = wcx
        .at(id())
        .animate(id().with("turn"), state.orientation().turn(), TURN);
    let opacity = wcx
        .at(id())
        .animate(id().with("fade"), if drawing { 0.0 } else { 1.0 }, FADE);
    let pill = turning(turn, &extents, slot, open);
    // The pill is a stadium in either layout, so its corner does not turn with the rest.
    let radius = (slot.h + 2.0 * PAD) * 0.5;

    let mut layer = Container::new(id().with("layer"), NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        // The zones and the tooltip are decoration: what the app selects under them is the
        // surface's, and the pill and its buttons take their own input.
        .sense(Sense::PASS_THROUGH);
    if state.dragging() {
        layer = layer.child_decl(zone_decl(area, state.placed, screen));
    }

    let mut shell = Container::new(id(), NodeKind::Stack)
        .size(SizeClass::fixed(pill.w, pill.h))
        .offset(at.x, at.y)
        .background(surface, Corners::all(radius))
        .border(outline, 1.0)
        .shadow(SHADOW)
        .opacity(opacity)
        // The block's lane is declared at its full thickness while the pill is still
        // opening, and keeps its place while the pill closes: the pill's own box is what
        // reveals it, so a chip can never paint past the bar it belongs to.
        .map(|decl| {
            decl.clip(ui_core::layout::Clip {
                x: ui_core::layout::Overflow::Hidden,
                y: ui_core::layout::Overflow::Hidden,
            })
        });
    let held = wcx.state(grip()).hovered;
    let hover = wcx.at(grip()).animate(
        grip().with("hover"),
        if held { 1.0 } else { 0.0 },
        AnimSpec::quick(),
    );
    shell = shell.child_decl(grip_decl(
        turn,
        &extents,
        slot,
        open,
        hover,
        state.dragging(),
        ink,
    ));
    let mut labels = vec![(Mark::Cursor.id(), Mark::Cursor.message(), None)];
    for (index, entry) in entries.iter().enumerate() {
        let k = index + 1;
        let seat = Seat {
            centre: member_centre(k, turn, &extents, slot, open),
            size: member_size(k, turn, &extents, slot),
            turn,
        };
        let member = build_member(&mut wcx, &pack, entry, seat, ink, catalog);
        if let Some(leaf) = member.leaf {
            labels.push((leaf.id, leaf.message, leaf.hint));
        }
        shell = shell.child(Box::new(member.container));
    }
    if !block.is_empty() {
        // One container for the whole block, in its own lane: it opens and fades as a unit.
        let strip = strip_box(turn, &extents, slot, open);
        shell = shell.child(Box::new(param_block(
            &block, strip, turn, presence, catalog, ink,
        )));
    }
    layer = layer.child(Box::new(shell));

    if !block.is_empty() {
        labels.extend(block.iter().map(|param| {
            let (id, message) = param.label();
            (id, message, None)
        }));
        let palette = Palette {
            surface,
            outline,
            ink,
        };
        if let Some(picker) = colour_picker(&mut wcx, &pack, &block, slot, catalog, palette) {
            layer = layer.child(Box::new(picker));
        }
    }

    if let Some(tip) = tool_tip(&mut wcx, kit, catalog, &labels, state.dragging()) {
        layer = layer.child(tip);
    }
    Box::new(layer)
}

/// Where one member sits: the box the bar's geometry gives it, and which way the bar runs.
struct Seat {
    centre: Point,
    size: Size,
    turn: f32,
}

/// One member's container, its box placed on the bar, and the label a tooltip would show.
struct Member {
    container: Container,
    leaf: Option<Tool>,
}

fn build_member(
    wcx: &mut Cx<'_, '_>,
    pack: &ui_style::Style,
    entry: &Entry,
    seat: Seat,
    ink: Color,
    catalog: &Catalog,
) -> Member {
    let Seat {
        centre,
        size,
        turn,
    } = seat;
    let at = Point::new(centre.x - size.w * 0.5, centre.y - size.h * 0.5);
    match entry {
        Entry::Tool(tool) => {
            let button = icon_button(wcx, pack, tool);
            Member {
                container: Container::new(tool.id.with("slot"), NodeKind::Box)
                    .offset(at.x, at.y)
                    .size(SizeClass::fixed(size.w, size.h))
                    .child(wcx.element(Box::new(button))),
                leaf: Some(tool.clone()),
            }
        }
        Entry::Group {
            family,
            message,
            icon,
            selected,
        } => {
            // The button keeps the family's id, so a click names the family; the slot around
            // it takes the wheel and the menu, which have nowhere else to land.
            let tool = Tool {
                shortcut: None,
                hint: None,
                ..Tool::new(family.id(), *icon, message).toggled(*selected)
            };
            let button = icon_button(wcx, pack, &tool);
            let rows = family
                .children()
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    MenuItem::new(
                        option_id(*family, index),
                        catalog.text_by_id(child.message()).to_string(),
                    )
                })
                .collect();
            Member {
                container: Container::new(family.slot(), NodeKind::Box)
                    .offset(at.x, at.y)
                    .size(SizeClass::fixed(size.w, size.h))
                    .sense(Sense::SCROLL)
                    .menu(rows)
                    .child(wcx.element(Box::new(button))),
                leaf: Some(tool),
            }
        }
        Entry::Loupe { state } => {
            let tool = Tool {
                id: loupe(),
                icon: IconSource::glyph("zoom_in", IconSet::Outlined),
                message: match state {
                    LoupeState::Temporary => "toolbar-loupe-temporary",
                    _ => "toolbar-loupe",
                },
                enabled: true,
                selected: Some(*state == LoupeState::On),
                shortcut: None,
                hint: None,
            };
            let button = icon_button(wcx, pack, &tool);
            let mut container = Container::new(loupe().with("slot"), NodeKind::Box)
                .offset(at.x, at.y)
                .size(SizeClass::fixed(size.w, size.h));
            if *state == LoupeState::Temporary {
                container = container.background(LOUPE_TEMPORARY, Corners::all(12.0));
            }
            Member {
                container: container.child(wcx.element(Box::new(button))),
                leaf: Some(tool),
            }
        }
        Entry::Divider => {
            let decl = Decl::container(id().with("divider"), NodeKind::Box)
                .size(SizeClass::fixed(size.w, size.h))
                .offset(at.x, at.y)
                .sense(Sense::PASS_THROUGH)
                .shapes(vec![divider_line(size, turn, ink)]);
            Member {
                container: Container::from_decl(decl),
                leaf: None,
            }
        }
    }
}

/// The divider's hairline across its member box.
fn divider_line(size: Size, turn: f32, ink: Color) -> Shape {
    let inset = DIVIDER_INSET.min(size.w.min(size.h) * 0.5);
    let from = Point::new(
        lerp(size.w * 0.5, inset, turn),
        lerp(inset, size.h * 0.5, turn),
    );
    let to = Point::new(
        lerp(size.w * 0.5, size.w - inset, turn),
        lerp(size.h - inset, size.h * 0.5, turn),
    );
    Shape::Line {
        from,
        to,
        thickness: 1.0,
        colour: ink.faded(0.5),
    }
}

/// A chip that answers a click: it draws the declaration it was built from and reports the
/// one change a press means. A `Container` draws and takes the press but reports nothing,
/// so a chip built as one opens no picker however it looks.
struct Chip {
    decl: Decl,
}

impl Chip {
    fn new(decl: Decl) -> Self {
        Self { decl }
    }
}

impl Element for Chip {
    fn decl(&self, _cx: &mut EngineCx<'_>) -> Decl {
        self.decl.clone()
    }

    // The shell is app input, so only a comparison can prove a cached copy still holds.
    fn cache_key(&self) -> Option<CachedKey> {
        Some(CachedKey::new(self.decl.id, 0))
    }

    fn cache_matches(&self, cached: &Decl) -> bool {
        self.decl.own_eq(cached)
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        state.activation().then_some(Change::Activated)
    }
}

/// One MD3 icon button in the bar's own shape. An armed tool takes the tonal shape, which
/// fills its container: a tinted glyph alone is not a selection anyone can find at a glance.
fn icon_button(wcx: &mut Cx<'_, '_>, pack: &ui_style::Style, tool: &Tool) -> Md3IconButton {
    let style = if tool.selected == Some(true) {
        IconButtonStyle::Tonal
    } else {
        BUTTON_STYLE
    };
    let mut params = IconButtonParams::new(tool.id, tool.icon)
        .style(style)
        .size(BUTTON_SIZE)
        .width(BUTTON_WIDTH)
        .disabled(!tool.enabled);
    if let Some(selected) = tool.selected {
        params = params.selected(selected);
    }
    Md3IconButton::new_with(params, wcx, pack)
}

/// The block: one container in its own lane, its chips laid out from the bar's leading end
/// and centred across the lane, faded as a whole.
fn param_block(
    params: &[Param],
    strip: Rect,
    turn: f32,
    presence: f32,
    catalog: &Catalog,
    ink: Color,
) -> Container {
    let mut block = Container::new(
        params_id(),
        if turn < 0.5 {
            NodeKind::Row
        } else {
            NodeKind::Column
        },
    )
    .size(SizeClass::fixed(strip.size.w, strip.size.h))
    .offset(strip.origin.x, strip.origin.y)
    .gap(ROW_GAP)
    // Chips keep their own thickness; what they do not take of the lane is split either
    // side of them.
    .cross_align(0.0)
    .cross_pos(0.5)
    .sense(Sense::PASS_THROUGH)
    .opacity(presence);
    for param in params {
        let chip: Box<dyn Element> = match param {
            Param::Paint {
                ink: stroke,
                fill,
                fillable,
                open,
            } => Box::new(paint_chip(*stroke, *fill, *fillable, *open, ink)),
            Param::Swatch {
                field,
                colour,
                open,
            } => Box::new(swatch_chip(*field, *colour, *open, ink)),
            Param::Number { field, value, .. } => Box::new(number_chip(*field, *value, ink)),
            Param::Choice {
                field,
                options,
                current,
                ..
            } => Box::new(choice_chip(*field, options, *current, catalog, ink)),
        };
        block = block.child(chip);
    }
    block
}

/// The paint chip: a ring for the stroke around a disc for the fill, over a checkerboard
/// when the fill lets the picture through. A mark that has no fill is one disc of its ink:
/// a ring there would say "nothing inside" about a tool that has no inside.
fn paint_chip(
    stroke: Color,
    fill: Option<Color>,
    fillable: bool,
    open: bool,
    ink: Color,
) -> Chip {
    let side = CHIP_PAINT;
    let centre = Point::new(side * 0.5, side * 0.5);
    let disc = Rect::new(centre.x - DISC, centre.y - DISC, DISC * 2.0, DISC * 2.0);
    let mut shapes = Vec::new();
    if fillable {
        shapes.extend(checker(centre, DISC));
        if let Some(fill) = fill {
            shapes.push(Shape::Ellipse {
                rect: disc,
                thickness: 0.0,
                fill,
                colour: Color::TRANSPARENT,
            });
        }
        shapes.push(Shape::Ellipse {
            rect: disc,
            thickness: 2.0,
            fill: Color::TRANSPARENT,
            colour: stroke,
        });
    } else {
        shapes.push(Shape::Ellipse {
            rect: disc,
            thickness: 0.0,
            fill: stroke,
            colour: Color::TRANSPARENT,
        });
    }
    let mut decl = Decl::container(field(Field::Stroke), NodeKind::Box)
        .size(SizeClass::fixed(side, side))
        .background(ink.faded(WASH), Corners::all(side * 0.5))
        .sense(Sense::CLICK | Sense::HOVER | Sense::SCROLL)
        .cursor(CursorShape::PointingHand)
        .shapes(shapes);
    if open {
        decl = decl.border(Color::from_rgba8(120, 160, 255, 255), 1.5);
    }
    Chip::new(decl)
}

/// One ink of its own: a disc of the colour, or the checkerboard when it is switched off,
/// because a chip has to say both what it sets and whether it sets anything.
fn swatch_chip(field_id: Field, colour: Option<Color>, open: bool, ink: Color) -> Chip {
    let side = CHIP_PAINT;
    let centre = Point::new(side * 0.5, side * 0.5);
    let disc = Rect::new(centre.x - DISC, centre.y - DISC, DISC * 2.0, DISC * 2.0);
    let mut shapes = vec![Shape::Ellipse {
        rect: disc,
        thickness: 2.0,
        fill: colour.unwrap_or(Color::TRANSPARENT),
        colour: colour.map_or(Color::from_rgba8(120, 120, 128, 255), |_| ink.faded(0.35)),
    }];
    if colour.is_none() {
        shapes.extend(checker(centre, DISC));
    }
    let mut decl = Decl::container(field(field_id), NodeKind::Box)
        .size(SizeClass::fixed(side, side))
        .background(ink.faded(WASH), Corners::all(side * 0.5))
        .sense(Sense::CLICK | Sense::HOVER | Sense::SCROLL)
        .cursor(CursorShape::PointingHand)
        .shapes(shapes);
    if open {
        decl = decl.border(Color::from_rgba8(120, 160, 255, 255), 1.5);
    }
    Chip::new(decl)
}

/// The checkerboard a translucent area is drawn over, clipped to the disc by its cells.
fn checker(centre: Point, radius: f32) -> Vec<Shape> {
    let cells = ((radius * 2.0) / CHECK).ceil() as i32;
    let mut out = Vec::new();
    for row in 0..cells {
        for column in 0..cells {
            let x = centre.x - radius + column as f32 * CHECK;
            let y = centre.y - radius + row as f32 * CHECK;
            let cell = Point::new(x + CHECK * 0.5, y + CHECK * 0.5);
            let dx = cell.x - centre.x;
            let dy = cell.y - centre.y;
            if dx * dx + dy * dy > (radius - 0.5) * (radius - 0.5) {
                continue;
            }
            out.push(Shape::Rect {
                rect: Rect::new(x, y, CHECK, CHECK),
                corners: Corners::ZERO,
                fill: if (row + column) % 2 == 0 {
                    CHECK_LIGHT
                } else {
                    CHECK_DARK
                },
                gradient: None,
                border: None,
            });
        }
    }
    out
}

/// The number chip: its value, raised and lowered by the wheel.
fn number_chip(field_id: Field, value: f32, ink: Color) -> Container {
    let label = if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    let id = field(field_id);
    let body = ui_widgets::text::leaf(
        id.with("label"),
        label,
        12.0,
        ink,
        Weight(500),
        CHIP_NUMBER - 8.0,
        0.0,
        0.0,
    );
    Container::new(id, NodeKind::Box)
        .size(SizeClass::fixed(CHIP_NUMBER, CHIP))
        .background(ink.faded(WASH), Corners::all(CHIP * 0.5))
        .sense(Sense::CLICK | Sense::HOVER | Sense::SCROLL)
        .cursor(CursorShape::PointingHand)
        .child_decl(body)
        .map(|decl| decl.stack_align(ui_core::layout::OverlayAlign::Center))
}

/// The choice chip: the option in force, cycled by the wheel or chosen from its own menu.
fn choice_chip(
    field_id: Field,
    options: &'static [&'static str],
    current: usize,
    catalog: &Catalog,
    ink: Color,
) -> Container {
    let id = field(field_id);
    let label = options
        .get(current)
        .map(|message| catalog.text_by_id(message).to_string())
        .unwrap_or_default();
    let rows = options
        .iter()
        .enumerate()
        .map(|(index, message)| {
            MenuItem::new(
                option(field_id, index),
                catalog.text_by_id(message).to_string(),
            )
        })
        .collect();
    let body = ui_widgets::text::leaf(
        id.with("label"),
        label,
        12.0,
        ink,
        Weight(500),
        CHIP_CHOICE - 8.0,
        0.0,
        0.0,
    );
    Container::new(id, NodeKind::Box)
        .size(SizeClass::fixed(CHIP_CHOICE, CHIP))
        .background(ink.faded(WASH), Corners::all(CHIP * 0.5))
        .sense(Sense::CLICK | Sense::HOVER | Sense::SCROLL | Sense::SECONDARY)
        .cursor(CursorShape::PointingHand)
        .menu(rows)
        .child_decl(body)
        .map(|decl| decl.stack_align(ui_core::layout::OverlayAlign::Center))
}

/// The field whose picker a param row has open, if any. The paint chip opens the stroke's,
/// and a swatch chip its own.
fn open_colour(params: &[Param]) -> Option<Field> {
    params.iter().find_map(|param| match param {
        Param::Paint { open: true, .. } => Some(Field::Stroke),
        Param::Swatch {
            field, open: true, ..
        } => Some(*field),
        _ => None,
    })
}

/// The colours the pill and everything it carries are painted in.
#[derive(Clone, Copy)]
struct Palette {
    surface: Color,
    outline: Color,
    ink: Color,
}

/// The colour picker, when the paint chip has it open: a stroke row, a fill row when the
/// tool paints one, and the eyedropper. Anchored to the chip, so the engine flips it.
fn colour_picker(
    wcx: &mut Cx<'_, '_>,
    pack: &ui_style::Style,
    params: &[Param],
    slot: Size,
    catalog: &Catalog,
    palette: Palette,
) -> Option<Container> {
    let Palette {
        surface,
        outline,
        ink,
    } = palette;
    let which = open_colour(params)?;
    let fillable = params.iter().any(|param| match param {
        Param::Paint { fillable, .. } => *fillable,
        _ => false,
    });
    let mut panel = Container::new(picker_id(), NodeKind::Column)
        .padding(Insets {
            left: PICKER_PAD,
            top: PICKER_PAD,
            right: PICKER_PAD,
            bottom: PICKER_PAD,
        })
        .gap(4.0)
        .background(surface, Corners::all(10.0))
        .border(outline, 1.0)
        .shadow(SHADOW)
        // The chip sits in the block's lane, so the panel has to clear the lane's own
        // padding before it is past the bar's edge: anchored to the chip and offset that
        // far, it hangs off the bar rather than over it.
        .overlay(
            OverlayPlacement::below(Anchor::Node(field(which)))
                .offset((slot.h - CHIP) * 0.5 + PAD)
                .flip(true),
        );
    panel = panel.child(Box::new(swatch_rows(which, catalog, ink)));
    // The stroke's picker carries the fill beside it: a mark's two paints are chosen in one
    // visit rather than two, and the eyedropper that fills both is there too.
    if which == Field::Stroke {
        if fillable {
            panel = panel.child(Box::new(swatch_rows(Field::Fill, catalog, ink)));
        }
        let mut tools = Container::new(picker_id().with("tools"), NodeKind::Row)
            .gap(6.0)
            .sense(Sense::PASS_THROUGH);
        let dropper = Tool::new(
            dropper(Field::Stroke),
            IconSource::glyph("colorize", IconSet::Outlined),
            "toolbar-pick",
        );
        let button = icon_button(wcx, pack, &dropper);
        tools = tools.child(wcx.element(Box::new(button)));
        panel = panel.child(Box::new(tools));
    }
    Some(panel)
}

/// Two rows of palette swatches for one field; the fill field also offers "no fill".
/// One field's palette: its name, then the swatches, then the way to choose no fill at all.
/// Two rows of identical dots with nothing to tell them apart would leave the user guessing
/// which of them sets the line and which sets the inside.
fn swatch_rows(field_id: Field, catalog: &Catalog, ink: Color) -> Container {
    let per_row = 4;
    let mut column = Container::new(picker_id().with(field_id.key()), NodeKind::Column)
        .gap(4.0)
        .sense(Sense::PASS_THROUGH)
        .child(Box::new(field_label(field_id, catalog, ink)));
    let mut index = 0;
    for _ in 0..PALETTE.len().div_ceil(per_row) {
        let mut row = Container::new(
            picker_id().with((field_id.key(), index)),
            NodeKind::Row,
        )
        .gap(4.0)
        .sense(Sense::PASS_THROUGH);
        for _ in 0..per_row {
            if index >= PALETTE.len() {
                break;
            }
            row = row.child(Box::new(palette_chip(field_id, index, PALETTE[index])));
            index += 1;
        }
        column = column.child(Box::new(row));
    }
    if field_id.clearable() {
        let label = catalog.text_by_id("toolbar-none").to_string();
        column = column.child(Box::new(no_fill_chip(field_id, &label, ink)));
    }
    column
}

/// The name of the field a palette sets.
fn field_label(field_id: Field, catalog: &Catalog, ink: Color) -> Container {
    let message = match field_id {
        Field::Stroke => "toolbar-stroke",
        Field::Fill => "pin-edit-fill",
        Field::Width => "toolbar-width",
        Field::Heads => "toolbar-heads",
        Field::Outline => "toolbar-outline",
        Field::Border => "toolbar-border",
        Field::Size => "toolbar-size",
        Field::Padding => "toolbar-padding",
        Field::Radius => "toolbar-radius",
        Field::Align => "toolbar-align",
    };
    Container::new(picker_id().with((field_id.key(), "label")), NodeKind::Box)
        .sense(Sense::PASS_THROUGH)
        .child_decl(ui_widgets::text::leaf(
            picker_id().with((field_id.key(), "text")),
            catalog.text_by_id(message).to_string(),
            11.0,
            ink,
            Weight(500),
            f32::INFINITY,
            0.0,
            0.0,
        ))
}

/// One palette swatch: the chip already shows the result, so the swatch carries only colour.
fn palette_chip(field_id: Field, index: usize, colour: Color) -> Chip {
    Chip::new(
        Decl::container(swatch(field_id, index), NodeKind::Box)
            .size(SizeClass::fixed(SWATCH_CELL, SWATCH_CELL))
            .sense(Sense::CLICK | Sense::HOVER)
            .cursor(CursorShape::PointingHand)
            .shapes(vec![Shape::Ellipse {
                rect: Rect::new(
                    (SWATCH_CELL - SWATCH * 2.0) * 0.5,
                    (SWATCH_CELL - SWATCH * 2.0) * 0.5,
                    SWATCH * 2.0,
                    SWATCH * 2.0,
                ),
                thickness: 0.0,
                fill: colour,
                colour: Color::TRANSPARENT,
            }]),
    )
}

/// The "no fill" chip: a checkerboard disc, meaning the picture shows through.
fn no_fill_chip(field_id: Field, label: &str, ink: Color) -> Chip {
    let mut shapes = checker(Point::new(SWATCH + 3.0, SWATCH_CELL * 0.5), SWATCH);
    shapes.push(Shape::Ellipse {
        rect: Rect::new(3.0, SWATCH_CELL * 0.5 - SWATCH, SWATCH * 2.0, SWATCH * 2.0),
        thickness: 1.0,
        fill: Color::TRANSPARENT,
        colour: Color::from_rgba8(120, 120, 128, 255),
    });
    Chip::new(
        Decl::container(no_fill(field_id), NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::hug(SWATCH_CELL * 2.0, 160.0),
                height: AxisSize::fixed(SWATCH_CELL),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .sense(Sense::CLICK | Sense::HOVER)
            .cursor(CursorShape::PointingHand)
            .shapes(shapes)
            .child(ui_widgets::text::leaf(
                no_fill(field_id).with("label"),
                label.to_string(),
                12.0,
                ink,
                Weight(500),
                f32::INFINITY,
                0.0,
                0.0,
            ))
            .padding(Insets {
                left: SWATCH * 2.0 + 8.0,
                top: 0.0,
                right: 4.0,
                bottom: 0.0,
            }),
    )
}

/// The snap bands of the edge the bar would take, that one brightest.
fn zone_decl(area: Rect, bar: Rect, screen: Rect) -> Decl {
    let mut shapes = Vec::new();
    if let Some(chosen) = snap(area, bar) {
        for zone in zones(area, chosen.edge) {
            let active = zone.align == chosen.align;
            shapes.push(Shape::Rect {
                rect: zone.rect,
                corners: Corners::all(ZONE_CORNER),
                fill: if active { ZONE_ACTIVE_FILL } else { ZONE_FILL },
                gradient: None,
                border: Some((
                    if active { ZONE_ACTIVE_EDGE } else { ZONE_EDGE },
                    1.0,
                )),
            });
        }
    }
    Decl::container(id().with("zones"), NodeKind::Box)
        .sense(Sense::PASS_THROUGH)
        .size(SizeClass::fixed(screen.size.w, screen.size.h))
        .shapes(shapes)
}

/// The grip: a pair of dot columns the pointer grabs, and the bar's only claim on a drag.
fn grip_decl(
    turn: f32,
    extents: &[f32],
    slot: Size,
    open: Open,
    hover: f32,
    dragging: bool,
    ink: Color,
) -> Decl {
    let size = member_size(0, turn, extents, slot);
    let centre = member_centre(0, turn, extents, slot, open);
    let mut shapes = Vec::new();
    if hover > 0.0 {
        shapes.push(Shape::Rect {
            rect: Rect::new(0.0, 0.0, size.w, size.h),
            corners: Corners::all(size.w.min(size.h) * 0.5),
            // A state layer over the pill, like the icon buttons' own.
            fill: ink.faded(0.08),
            gradient: None,
            border: None,
        });
    }
    for row in -1..=1 {
        for column in [-1.0f32, 1.0] {
            // Dots are described in the row layout and travel with the turn.
            let along = row as f32 * DOT_STEP;
            let across = column * DOT_STEP * 0.5;
            let x = size.w * 0.5 + lerp(along, across, turn);
            let y = size.h * 0.5 + lerp(across, along, turn);
            shapes.push(Shape::Ellipse {
                rect: Rect::new(x - DOT, y - DOT, DOT * 2.0, DOT * 2.0),
                thickness: 0.0,
                fill: ink.faded(0.9),
                colour: Color::TRANSPARENT,
            });
        }
    }
    Decl::container(grip(), NodeKind::Box)
        .size(SizeClass::fixed(size.w, size.h))
        .offset(centre.x - size.w * 0.5, centre.y - size.h * 0.5)
        .sense(Sense::HOVER | Sense::DRAG)
        .cursor(if dragging {
            CursorShape::Grabbing
        } else {
            CursorShape::Grab
        })
        .shapes(shapes)
}

/// The tooltip of the button the pointer has rested on, once it has rested long enough.
fn tool_tip(
    cx: &mut Cx<'_, '_>,
    kit: &Md3Kit,
    catalog: &Catalog,
    labels: &[(Id, &'static str, Option<String>)],
    quiet: bool,
) -> Option<Box<dyn Element>> {
    // A bar under the hand is not being read: the pointer is on its way somewhere, and a
    // label appearing under it is noise.
    if quiet {
        return None;
    }
    let mut shown = None;
    for (id, message, hint) in labels {
        let hovered = cx.state(*id).hovered;
        // Every button's dwell is retargeted each frame, not only the hovered one: a value
        // left at one would show the next tooltip the moment it was reached.
        let rest = cx
            .at(*id)
            .animate(id.with("tip"), if hovered { 1.0 } else { 0.0 }, DWELL);
        if hovered && rest >= 1.0 {
            shown = Some((*id, *message, hint));
        }
    }
    let (id, message, hint) = shown?;
    let text = catalog.text_by_id(message);
    let mut params = ToolTipParams::new(
        id.with("tip-panel"),
        &text,
        ui_core::layout::Anchor::Node(id),
    );
    if let Some(hint) = hint {
        params = params.hint(hint);
    }
    let control = kit.tool_tip(params, cx);
    Some(cx.element(control))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: Size = Size::new(40.0, 40.0);

    /// The real bar, so the turn is tested over the member widths the app actually draws —
    /// the divider's narrow one included.
    fn bar() -> Vec<Entry> {
        let plan = crate::hotkeys::Plan::resolve(&crabsnap_core::config::Config::default());
        entries(State::default(), &selection(), &plan)
    }

    /// The bar's extents along its own axis.
    fn widths() -> Vec<f32> {
        extents(&bar(), SLOT)
    }

    /// The turn's invariant: a member that left the pill on its way round would paint
    /// outside the surface it sits on. The buttons must not shift either when the block
    /// opens, so they are checked in both lanes and with the block at either end.
    #[test]
    fn every_member_stays_inside_the_pill_through_the_whole_turn() {
        for grown in [0.0, SLOT.h + PARAM_GAP] {
            for side in [Side::Before, Side::After] {
                inside_through_the_turn(&widths(), Open { grown, side });
            }
        }
    }

    fn inside_through_the_turn(extents: &[f32], open: Open) {
        let count = extents.len();
        for step in 0..=20 {
            let turn = step as f32 / 20.0;
            let pill = Rect::from_origin_size(Point::ZERO, turning(turn, extents, SLOT, open));
            for k in 0..count {
                let centre = member_centre(k, turn, extents, SLOT, open);
                let size = member_size(k, turn, extents, SLOT);
                let member = Rect::from_origin_size(
                    Point::new(centre.x - size.w * 0.5, centre.y - size.h * 0.5),
                    size,
                );
                assert!(
                    holds(pill, member),
                    "member {k} leaves the pill at turn {turn}: {member:?} outside {pill:?}"
                );
            }
        }
        // The ends of the turn are the two layouts: a row across, a column down.
        let (first, last) = (
            member_centre(1, 0.0, extents, SLOT, open),
            member_centre(count - 1, 0.0, extents, SLOT, open),
        );
        assert!(last.x > first.x && last.y == first.y, "turn 0 is a row");
        let (first, last) = (
            member_centre(1, 1.0, extents, SLOT, open),
            member_centre(count - 1, 1.0, extents, SLOT, open),
        );
        assert!(last.y > first.y && last.x == first.x, "turn 1 is a column");
    }

    /// The block is a lane of its own, not a run on the buttons' line: open, it fills the
    /// bar's length inside the pill, and no button shares its band across the bar.
    #[test]
    fn the_block_takes_a_lane_of_its_own() {
        let extents = widths();
        let grown = SLOT.h + PARAM_GAP;
        for side in [Side::Before, Side::After] {
            let open = Open { grown, side };
            let pill = Rect::from_origin_size(
                Point::ZERO,
                pill_size(Orientation::Horizontal, &extents, SLOT, grown),
            );
            let strip = strip_box(0.0, &extents, SLOT, open);
            assert!(holds(pill, strip), "{side:?}: {strip:?} leaves {pill:?}");
            assert!((strip.size.w - (pill.size.w - 2.0 * PAD)).abs() < 1e-3);
            assert!((strip.size.h - SLOT.h).abs() < 1e-3);
            for k in 0..extents.len() {
                let centre = member_centre(k, 0.0, &extents, SLOT, open);
                let size = member_size(k, 0.0, &extents, SLOT);
                let button = Rect::from_origin_size(
                    Point::new(centre.x - size.w * 0.5, centre.y - size.h * 0.5),
                    size,
                );
                assert!(
                    button.max_y() <= strip.min_y() || button.min_y() >= strip.max_y(),
                    "{side:?}: button {k} shares the block's band"
                );
            }
        }
    }

    /// The divider is the one member narrower than a button, and it must not be laid out as
    /// if it were one: the bar's length has to count its own width.
    #[test]
    fn a_divider_takes_its_own_width_not_a_slots() {
        let extents = widths();
        let divider = bar()
            .iter()
            .position(|entry| matches!(entry, Entry::Divider))
            .expect("the bar divides its tools from its actions");
        let member = divider + 1;
        assert_eq!(extents[member], DIVIDER);
        let buttons = extents.len() - 2;
        let length = 2.0 * PAD + GRIP + buttons as f32 * SLOT.w + DIVIDER
            + GAP * (extents.len() - 1) as f32;
        assert!((pill_size(Orientation::Horizontal, &extents, SLOT, 0.0).w - length).abs() < 1e-3);
    }

    #[test]
    fn a_bar_over_a_zone_docks_to_that_zone_and_nowhere_else() {
        let area = Rect::new(100.0, 100.0, 800.0, 600.0);
        let extents = widths();
        let size = pill_size(Orientation::Horizontal, &extents, SLOT, 0.0);
        let over = |x: f32, y: f32| Rect::from_origin_size(Point::new(x, y), size);

        let centred = snap(area, over(500.0 - size.w * 0.5, 706.0));
        assert_eq!(
            centred.map(|zone| (zone.edge, zone.align)),
            Some((Edge::Bottom, Align::Center))
        );
        let start = snap(area, over(100.0, 706.0));
        assert_eq!(
            start.map(|zone| (zone.edge, zone.align)),
            Some((Edge::Bottom, Align::Start))
        );
        // Inside the selection, or past the band: no dock at all.
        assert_eq!(snap(area, over(400.0, 300.0)), None);
        assert_eq!(snap(area, over(400.0, 720.0)), None);
    }

    /// A zone's band has to sit over the place its own alignment docks the bar: a band that
    /// gathers in the middle of a long edge offers a landing the drop does not take.
    #[test]
    fn a_zone_band_sits_where_its_alignment_docks_the_bar() {
        let area = Rect::new(0.0, 0.0, 2560.0, 1440.0);
        let extents = widths();
        for edge in Edge::ALL {
            let size = pill_size(edge.orientation(), &extents, SLOT, 0.0);
            let along = |rect: Rect| match edge.orientation() {
                Orientation::Horizontal => (rect.min_x(), rect.max_x()),
                Orientation::Vertical => (rect.min_y(), rect.max_y()),
            };
            for zone in zones(area, edge) {
                let bar = Rect::from_origin_size(docked(area, edge, zone.align, size), size);
                let (band, bar) = (along(zone.rect), along(bar));
                match zone.align {
                    Align::Start => assert!((band.0 - bar.0).abs() < 1e-3, "{edge:?}"),
                    Align::End => assert!((band.1 - bar.1).abs() < 1e-3, "{edge:?}"),
                    Align::Center => {
                        assert!(((band.0 + band.1) - (bar.0 + bar.1)).abs() < 1e-3, "{edge:?}")
                    }
                }
            }
        }
    }

    #[test]
    fn a_docked_bar_flips_or_clamps_rather_than_leaving_the_screen() {
        let screen = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let extents = widths();
        let size = pill_size(Orientation::Horizontal, &extents, SLOT, 0.0);

        let roomy = Rect::new(100.0, 100.0, 600.0, 400.0);
        let below = fitted(screen, roomy, Edge::Bottom, Align::Center, size);
        assert_eq!(below.y, 500.0 + DOCK_GAP);
        assert_eq!(below.x, 100.0 + (600.0 - size.w) * 0.5);

        // No room below, room above: the far edge takes it instead.
        let low = Rect::new(100.0, 500.0, 600.0, 280.0);
        let above = fitted(screen, low, Edge::Bottom, Align::Center, size);
        assert_eq!(above.y, 500.0 - DOCK_GAP - size.h);

        // Neither side has room: pushed inside the bottom edge rather than lost.
        let full = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let inside = fitted(screen, full, Edge::Bottom, Align::Center, size);
        assert_eq!(inside.y, 800.0 - size.h);
    }

    #[test]
    fn the_block_opens_away_from_the_area_even_when_the_bar_flips() {
        let screen = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let extents = widths();
        let base = pill_size(Orientation::Horizontal, &extents, SLOT, 0.0);
        let grown = SLOT.h + PARAM_GAP;

        // No room below a selection that reaches the bottom: the bar hangs above it, and the
        // block has to open upward too rather than over the selection's top edge.
        let low = Rect::new(100.0, 400.0, 600.0, 380.0);
        let (lane, side, full) = Toolbar::new().place(screen, low, &extents, SLOT, grown);
        assert_eq!(side, Side::Before);
        let hanging = fitted(screen, low, Edge::Bottom, Align::Center, base);
        assert_eq!(lane.y, hanging.y, "the buttons keep the hanging place");
        let at = side.origin(lane, grown, Orientation::Horizontal);
        assert_eq!(at.y, hanging.y - grown);
        assert!(holds(screen, Rect::from_origin_size(at, full)));
    }

}
