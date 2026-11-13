//! Palette pins: a GIMP palette's swatches, or one CSS colour, as a card.
//!
//! The card has two blocks: the selected colour beside the swatches it was picked from,
//! and the same colour spelled in every code a person might need. Selecting is the pin's
//! own interaction — the swatches are live nodes, and the canvas routes their clicks here.

use std::sync::Arc;

use crabsnap_core::media::{Kind, TextKind};
use ui_color_hct::Scheme;
use ui_core::anim::{AnimSpec, Easing};
use ui_core::change::Change;
use ui_core::draw::ImageEntry;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::text::{Family, GenericFamily, TextBlock, TextStyle};
use ui_core::tree::Presence;
use ui_core::{CursorShape, Decl, Id, NodeData, NodeKind, Sense};
use ui_i18n::Catalog;
use ui_platform::element::Element;
use ui_platform::{Container, WidgetState};

use crate::pin::card;
use crate::pin::content::colour_code::{self, Rgba};
use crate::pin::content::text::text_decl;
use crate::pin::ink::Ink;
use crate::pin::{ExportAsk, Exported, PinContent, PinFrame};

/// Padding inside the card, the swatch grid's cell and gap, and the selected colour's box.
const SWATCH: f32 = 28.0;
const GAP: f32 = 8.0;
const SELECTED: f32 = 88.0;
/// The code rows keep this wide even when the palette itself is narrow.
const CODES_WIDTH: f32 = 210.0;
const CODE_GAP: f32 = 4.0;
const CODE_LINE: f32 = 18.0;
const LABEL_SIZE: f32 = 10.0;
const CODE_SIZE: f32 = 12.0;
/// Room between the code field's border and its text.
const FIELD_PAD: f32 = 6.0;
/// Columns a palette falls back on when it does not say, which most `.gpl` files do not.
const DEFAULT_COLUMNS: usize = 8;
/// The selection swap: short enough to read as a move, not a transition to wait for.
const SWAP: AnimSpec = AnimSpec::new(0.16, Easing::EaseOut);

pub(crate) struct Palette {
    colours: Vec<Entry>,
    selected: usize,
    columns: usize,
    title: Option<String>,
    /// The swatch nodes this frame drew, with the colour each one selects.
    hits: Vec<(Id, usize)>,
    /// A payload a menu row asked to copy; the canvas drains it after the action.
    copy: Option<(String, Vec<u8>)>,
    /// The editing session; `None` leaves the card read-only.
    edit: Option<Editing>,
}

/// What an open session holds: the code field's own text, and the field it draws.
///
/// The text is the field's buffer, not a colour: a half-typed `#22` must stay on screen
/// while the card keeps showing the colour it had.
struct Editing {
    code: String,
    /// The field node this session drew; a change from it is the field's.
    field: Option<Id>,
    /// The caret was asked for and not yet handed over: the first build of a session.
    focus_pending: bool,
    /// The session waits for a press on another pin's picture to take its colour.
    picking: bool,
}

#[derive(Clone)]
struct Entry {
    colour: Rgba,
    /// The palette file's own name for the colour; empty when it gave none.
    label: String,
}

impl Entry {
    fn of(colour: Rgba) -> Self {
        Self {
            colour,
            label: String::new(),
        }
    }
}

impl Palette {
    pub(crate) fn read(bytes: &[u8], title: Option<String>) -> Result<Self, String> {
        if bytes.starts_with(b"ASEF") {
            return Err(
                "ASEF palettes are not readable yet; export one as a GIMP palette (.gpl)".into(),
            );
        }
        let source = String::from_utf8_lossy(bytes);
        let mut lines = source.lines();
        if lines.next().map(str::trim) != Some("GIMP Palette") {
            return Err("not a GIMP palette: the first line must be `GIMP Palette`".into());
        }
        let mut columns = DEFAULT_COLUMNS;
        let mut name = None;
        let mut colours = Vec::new();
        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("Name:") {
                name = Some(rest.trim().to_string());
                continue;
            }
            if let Some(rest) = line.strip_prefix("Columns:") {
                if let Ok(found) = rest.trim().parse::<usize>() {
                    columns = found.clamp(1, 16);
                }
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(red), Some(green), Some(blue)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(red), Ok(green), Ok(blue)) =
                (red.parse::<u8>(), green.parse::<u8>(), blue.parse::<u8>())
            else {
                continue;
            };
            colours.push(Entry {
                colour: Rgba {
                    r: red,
                    g: green,
                    b: blue,
                    a: 255,
                },
                label: parts.collect::<Vec<_>>().join(" "),
            });
        }
        if colours.is_empty() {
            return Err("the palette names no colours".into());
        }
        Ok(Self {
            colours,
            selected: 0,
            columns,
            title: name.filter(|name| !name.is_empty()).or(title),
            hits: Vec::new(),
            copy: None,
            edit: None,
        })
    }

    /// A palette of a picture's own colours: the `count` most populated clusters, heaviest
    /// first. What `pin add --palette-from` opens.
    pub(crate) fn from_picture(
        image: &crate::capture::Cropped,
        title: Option<String>,
        count: usize,
    ) -> Result<Self, String> {
        let rgba = image.rendition();
        let colours: Vec<Entry> = ui_color_hct::quantize(&rgba, count.max(1))
            .into_iter()
            .map(|cluster| Entry::of(from_argb(cluster.argb)))
            .collect();
        if colours.is_empty() {
            return Err("the picture has no opaque pixels to make a palette from".into());
        }
        let columns = DEFAULT_COLUMNS.min(colours.len()).max(1);
        Ok(Self {
            colours,
            selected: 0,
            columns,
            title,
            hits: Vec::new(),
            copy: None,
            edit: None,
        })
    }

    /// A palette of one colour, written as a CSS code.
    pub(crate) fn from_colour(code: &str) -> Result<Self, String> {
        let colour = colour_code::parse(code)?;
        Ok(Self {
            colours: vec![Entry::of(colour)],
            selected: 0,
            columns: 1,
            title: Some(colour.hex()),
            hits: Vec::new(),
            copy: None,
            edit: None,
        })
    }

    fn selected(&self) -> Entry {
        self.colours
            .get(self.selected)
            .cloned()
            .unwrap_or_else(|| Entry::of(Rgba::BLACK))
    }

    /// How many columns the swatch list uses: the file's, but never more than it holds.
    fn list_columns(&self) -> usize {
        let others = self.colours.len().saturating_sub(1).max(1);
        self.columns.min(others).max(1)
    }

    /// The swatches the list shows: every colour except the one on show.
    fn listed(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.colours.len()).filter(|index| *index != self.selected)
    }

    /// The colours as the clipboard and the export spell them: one `#RRGGBB` per line.
    fn hex_list(&self) -> String {
        let mut out = String::with_capacity(self.colours.len() * 9);
        for entry in &self.colours {
            out.push_str(&entry.colour.hex());
            out.push('\n');
        }
        out
    }

    /// The palette as a GIMP file, which is what it was read as and what it writes back.
    fn gpl(&self) -> String {
        let mut out = String::from("GIMP Palette\n");
        out.push_str(&format!(
            "Name: {}\n",
            self.title.as_deref().unwrap_or("Palette")
        ));
        out.push_str(&format!("Columns: {}\n#\n", self.columns));
        for entry in &self.colours {
            out.push_str(&format!(
                "{:3} {:3} {:3}\t{}\n",
                entry.colour.r, entry.colour.g, entry.colour.b, entry.label
            ));
        }
        out
    }

    /// Replace the list with the MD3 roles the shown colour derives, the seed itself.
    fn generate(&mut self) {
        let scheme = Scheme::tonal_spot(to_argb(self.selected().colour));
        self.colours = scheme
            .roles()
            .map(|(role, colour)| Entry {
                colour: from_argb(colour),
                label: role.name().to_string(),
            })
            .collect();
        self.selected = 0;
        self.columns = self.columns.clamp(1, 8);
        self.sync_code();
    }

    /// Append the shown colour and show the copy: the next keystroke edits the new one.
    fn add(&mut self) {
        let colour = self.selected().colour;
        self.colours.push(Entry::of(colour));
        self.selected = self.colours.len() - 1;
        self.sync_code();
    }

    /// Drop the shown colour while another remains.
    fn remove(&mut self) {
        if self.colours.len() <= 1 {
            return;
        }
        self.colours.remove(self.selected);
        self.selected = self.selected.min(self.colours.len() - 1);
        self.sync_code();
    }

    /// Show the selection's colour in the field, keeping the user's text until the
    /// selection itself moves.
    fn sync_code(&mut self) {
        let hex = self.selected().colour.hex();
        if let Some(edit) = &mut self.edit {
            edit.code = hex;
        }
    }

    /// The session's field: one line holding the shown colour's code. The engine owns the
    /// caret, the selection and the input history; the session owns the text.
    fn code_field(
        &mut self,
        cx: &mut ui_core::cx::Cx<'_>,
        frame: &PinFrame,
        inner: f32,
        ink: &Ink,
    ) -> Decl {
        let field = frame.id.with("code");
        let view = field.with("text");
        let Some(editing) = &mut self.edit else {
            return Decl::container(field, NodeKind::Box);
        };
        editing.field = Some(field);
        if std::mem::take(&mut editing.focus_pending) {
            cx.request_focus(field);
        }
        let focused = cx.focused() == Some(field);
        let caret = focused.then(|| cx.caret_for(field)).flatten();
        let preedit = focused.then(|| cx.preedit_for(field)).flatten();
        let (shown, composing, caret_at) =
            ui_widgets::text::typed_input(&editing.code, caret, preedit.as_ref());
        let selection = cx
            .selection_for(field)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: ink.selection,
                painter: None,
            });
        let style = TextStyle::new(CODE_SIZE).family(Family::Generic(GenericFamily::Monospace));
        let room = (inner - FIELD_PAD * 2.0).max(1.0);
        let mut text = ui_widgets::text::Text::new(view, shown, CODE_SIZE, ink.body)
            .max_width(room)
            .composing(composing)
            .painting(selection);
        if let (true, Some(at)) = (focused, caret_at) {
            text = text.caret(at, ink.body);
        }
        let mut field_decl = text.resolve();
        if let NodeData::Text { block, .. } = &mut field_decl.data {
            std::rc::Rc::make_mut(block).input_base = Some(0);
        }
        field_decl.size_class = SizeClass {
            width: AxisSize::fixed(room),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        };

        let label = text_decl(
            field.with("label"),
            TextBlock::plain(
                frame.catalog.text_by_id("pin-palette-code").to_string(),
                style,
            ),
            LABEL_SIZE,
            ink.muted,
            inner,
        );
        let mut input = Decl::container(field, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(inner),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(FIELD_PAD))
            .background(ink.code_fill, Corners::all(6.0))
            .border(ink.outline, 1.0)
            .sense(Sense::CLICK | Sense::FOCUSABLE)
            .focus_stop(true)
            .cursor(CursorShape::Text)
            .text_input(editing.code.clone());
        input.children = vec![std::rc::Rc::new(field_decl)];
        let mut column = Decl::container(field.with("column"), NodeKind::Column).gap(CODE_GAP);
        column.children = vec![std::rc::Rc::new(label), std::rc::Rc::new(input)];
        column
    }
}

fn to_argb(colour: Rgba) -> u32 {
    (u32::from(colour.a) << 24)
        | (u32::from(colour.r) << 16)
        | (u32::from(colour.g) << 8)
        | u32::from(colour.b)
}

fn from_argb(argb: u32) -> Rgba {
    Rgba {
        r: (argb >> 16) as u8,
        g: (argb >> 8) as u8,
        b: argb as u8,
        a: (argb >> 24) as u8,
    }
}

/// One code row: the whole colour vocabulary the palette can spell.
#[derive(Clone, Copy)]
enum Code {
    Hex,
    Rgb,
    Hsl,
}

impl Code {
    const ALL: [Self; 3] = [Self::Hex, Self::Rgb, Self::Hsl];

    fn label_key(self) -> &'static str {
        match self {
            Self::Hex => "pin-code-hex",
            Self::Rgb => "pin-code-rgb",
            Self::Hsl => "pin-code-hsl",
        }
    }

    fn spell(self, colour: Rgba) -> String {
        match self {
            Self::Hex => colour.hex(),
            Self::Rgb => colour.rgb(),
            Self::Hsl => colour.hsl(),
        }
    }

    fn menu_key(self) -> &'static str {
        match self {
            Self::Hex => "pin-menu/copy-hex",
            Self::Rgb => "pin-menu/copy-rgb",
            Self::Hsl => "pin-menu/copy-hsl",
        }
    }
}

fn ink_of(colour: Rgba) -> Color {
    Color::from_rgba8(colour.r, colour.g, colour.b, colour.a)
}

impl PinContent for Palette {
    fn kind(&self) -> Kind {
        Kind::Text(TextKind::Palette)
    }

    fn title(&self) -> Option<String> {
        self.title.clone()
    }

    fn size(&self) -> (f32, f32) {
        let others = self.colours.len().saturating_sub(1);
        let columns = self.list_columns();
        let rows = others.div_ceil(columns);
        let list_width = if others == 0 {
            0.0
        } else {
            columns as f32 * SWATCH + (columns - 1) as f32 * GAP
        };
        let list_height = if rows == 0 {
            0.0
        } else {
            rows as f32 * SWATCH + (rows - 1) as f32 * GAP
        };
        let codes = Code::ALL.len() as f32 * CODE_LINE + (Code::ALL.len() - 1) as f32 * CODE_GAP;
        (
            card::PAD * 2.0 + (SELECTED + GAP + list_width).max(CODES_WIDTH),
            card::PAD * 2.0 + list_height.max(SELECTED) + GAP + codes,
        )
    }

    fn image(&mut self, _texture: u64, _drawn: (f32, f32)) -> Option<ImageEntry> {
        None
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>, frame: PinFrame) -> Box<dyn Element> {
        let ink = Ink::card();
        // Laid out at native size: the canvas scales the card, so nothing here multiplies by
        // the zoom — the swatches and their labels grow together instead of the card
        // stretching under fixed-size text.
        let gap = GAP;
        let swatch = SWATCH;
        let selected = self.selected();
        self.hits.clear();

        // One block whose fill travels, not a new node per colour: an opaque block has no
        // background of its own to arrive from, so a replacement fading in over the card
        // would read as a flash of the card between the two colours.
        let fill = cx.animate_colour(
            frame.id.with("selected-fill"),
            ink_of(selected.colour),
            SWAP,
        );
        let big = Container::new(frame.id.with("selected"), NodeKind::Box)
            .size(SizeClass::fixed(SELECTED, SELECTED))
            .background(fill, Corners::all(10.0))
            .border(ink.outline, 1.0);

        // The rest of the palette, in the order the file gave them.
        let columns = self.list_columns();
        let listed: Vec<usize> = self.listed().collect();
        let mut list = Container::new(frame.id.with("list"), NodeKind::Column).gap(gap);
        for (row, indices) in listed.chunks(columns).enumerate() {
            let mut line = Container::new(frame.id.with(("row", row)), NodeKind::Row).gap(gap);
            for index in indices {
                let id = frame.id.with(("swatch", *index));
                self.hits.push((id, *index));
                let decl = Decl::container(id, NodeKind::Box)
                    .size(SizeClass::fixed(swatch, swatch))
                    .background(ink_of(self.colours[*index].colour), Corners::all(4.0))
                    .border(ink.outline, 1.0)
                    .sense(Sense::CLICK)
                    .cursor(CursorShape::PointingHand)
                    .enter(Presence::fade(SWAP, 0.0).scaled_over(0.6, SWAP));
                line = line.child(Box::new(Swatch { decl }));
            }
            list = list.child(Box::new(line));
        }

        let codes = Code::ALL.iter().fold(
            Container::new(frame.id.with("codes"), NodeKind::Column).gap(CODE_GAP),
            |codes, code| {
                codes.child_decl(code_line(
                    frame.id.with(("code", code.label_key())),
                    &frame.catalog,
                    *code,
                    selected.colour,
                    &ink,
                ))
            },
        );

        let mut card = card::filled(
            card::shell(&frame, NodeKind::Column, card::hug_height(frame.size.0)),
            &frame,
            ink.surface,
        )
        .padding(Insets::all(card::PAD))
        .gap(gap)
        .child(Box::new(
            Container::new(frame.id.with("body"), NodeKind::Row)
                .gap(gap)
                .child(Box::new(big))
                .child(Box::new(list)),
        ))
        .child(Box::new(codes));
        // The session's field goes below the codes, where the colour it edits is in view.
        if self.edit.is_some() {
            let inner = (frame.size.0 - card::PAD * 2.0).max(1.0);
            let field = self.code_field(cx, &frame, inner, &ink);
            card = card.child(Box::new(ui_widgets::from_decl(field)));
        }
        Box::new(card)
    }

    fn clipboard(&mut self, _drawn: (u32, u32)) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(vec![(
            "text/plain".to_string(),
            self.hex_list().into_bytes(),
        )])
    }

    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String> {
        if crate::pin::asks_for_image(ask) {
            return Err("a palette pin writes text, not an image; name a .gpl file".into());
        }
        Ok(Exported {
            bytes: self.gpl().into_bytes(),
            extension: "gpl",
        })
    }

    fn menu(&self) -> Vec<&'static str> {
        let mut rows = Vec::with_capacity(4 + Code::ALL.len());
        rows.push(if self.edit.is_some() {
            "pin-menu/done"
        } else {
            "pin-menu/edit"
        });
        if self.edit.is_some() {
            rows.push("pin-menu/roles");
            rows.push("pin-menu/pick");
            rows.push("pin-menu/add-colour");
            rows.push("pin-menu/remove-colour");
        }
        rows.extend(Code::ALL.iter().map(|code| code.menu_key()));
        rows
    }

    /// The session's rows act on the list it edits; the copy rows work either way.
    fn act(&mut self, item: Id) -> bool {
        if item == Id::str("pin-menu/edit") {
            if self.edit.is_none() {
                self.edit = Some(Editing {
                    code: self.selected().colour.hex(),
                    field: None,
                    focus_pending: true,
                    picking: false,
                });
            }
            return true;
        }
        if item == Id::str("pin-menu/done") {
            return self.edit.take().is_some();
        }
        if self.edit.is_some() {
            if item == Id::str("pin-menu/roles") {
                self.generate();
                return true;
            }
            if item == Id::str("pin-menu/pick") {
                if let Some(editing) = &mut self.edit {
                    editing.picking = !editing.picking;
                }
                return true;
            }
            if item == Id::str("pin-menu/add-colour") {
                self.add();
                return true;
            }
            if item == Id::str("pin-menu/remove-colour") {
                self.remove();
                return true;
            }
        }
        let Some(code) = Code::ALL
            .iter()
            .find(|code| item == Id::str(code.menu_key()))
        else {
            return false;
        };
        let text = code.spell(self.selected().colour);
        self.copy = Some(("text/plain".to_string(), text.into_bytes()));
        true
    }

    fn take_copy(&mut self) -> Option<(String, Vec<u8>)> {
        self.copy.take()
    }

    fn owns(&self, id: Id) -> bool {
        self.hits.iter().any(|(hit, _)| *hit == id)
            || self
                .edit
                .as_ref()
                .is_some_and(|editing| editing.field == Some(id))
    }

    /// A swatch press moves the selection; the field's text only moves the colour when it
    /// parses, so a code half typed leaves the card as it was.
    fn change(&mut self, id: Id, change: Change) -> bool {
        let field = self.edit.as_ref().and_then(|editing| editing.field);
        if field == Some(id) {
            let Change::Text(text) = change else {
                return false;
            };
            let Some(editing) = self.edit.as_mut() else {
                return false;
            };
            if text == editing.code {
                return false;
            }
            editing.code = text;
            if let (Ok(colour), Some(entry)) = (
                colour_code::parse(&editing.code),
                self.colours.get_mut(self.selected),
            ) {
                entry.colour = colour;
            }
            return true;
        }
        if !matches!(change, Change::Activated) {
            return false;
        }
        let Some((_, index)) = self.hits.iter().find(|(hit, _)| *hit == id) else {
            return false;
        };
        if *index == self.selected {
            return false;
        }
        self.selected = *index;
        self.sync_code();
        true
    }

    fn editing(&self) -> bool {
        self.edit.is_some()
    }

    fn picking(&self) -> bool {
        self.edit.as_ref().is_some_and(|editing| editing.picking)
    }

    /// The colour the pointer found becomes the shown one, and the wait ends either way.
    fn picked(&mut self, colour: Option<[u8; 4]>) -> bool {
        if !self.picking() {
            return false;
        }
        if let Some([r, g, b, a]) = colour {
            let picked = Rgba { r, g, b, a };
            if let Some(entry) = self.colours.get_mut(self.selected) {
                entry.colour = picked;
            }
            if let Some(editing) = &mut self.edit {
                editing.code = picked.hex();
            }
        }
        if let Some(editing) = &mut self.edit {
            editing.picking = false;
        }
        true
    }

    /// Escape cancels a wait before it closes the session.
    fn edit_escape(&mut self) -> bool {
        let Some(editing) = &mut self.edit else {
            return false;
        };
        if !editing.picking {
            return false;
        }
        editing.picking = false;
        true
    }
}

/// A swatch the pointer can pick: the only node here that reports a change.
struct Swatch {
    decl: Decl,
}

impl Element for Swatch {
    fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        self.decl.clone()
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        state.clicked.then_some(Change::Activated)
    }
}

/// `HEX  #6750A4`: the format's name, then the colour spelled in it.
fn code_line(id: Id, catalog: &Arc<Catalog>, code: Code, colour: Rgba, ink: &Ink) -> Decl {
    let label = catalog.text_by_id(code.label_key()).to_string();
    let label = text_decl(
        id.with("label"),
        TextBlock::plain(label, TextStyle::new(LABEL_SIZE)),
        LABEL_SIZE,
        ink.muted,
        64.0,
    );
    let style = TextStyle::new(CODE_SIZE).family(Family::Generic(GenericFamily::Monospace));
    let value = text_decl(
        id.with("value"),
        TextBlock::plain(code.spell(colour), style),
        CODE_SIZE,
        ink.body,
        CODES_WIDTH,
    );
    let mut line = Decl::container(id, NodeKind::Row).gap(GAP);
    line.children = vec![std::rc::Rc::new(label), std::rc::Rc::new(value)];
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gimp_palette_reads_its_columns_and_names() {
        let source = b"GIMP Palette\nName: Ink\nColumns: 3\n#\n255 0 0\tred\n 0 255 0\tgreen\n";
        let palette = Palette::read(source, None).expect("the palette parses");
        assert_eq!(palette.title.as_deref(), Some("Ink"));
        assert_eq!(palette.columns, 3);
        assert_eq!(palette.colours.len(), 2);
        assert_eq!(palette.colours[0].colour.hex(), "#FF0000");
        assert_eq!(palette.colours[1].label, "green");
    }

    /// The session's whole state machine: read-only refuses keystrokes, the field's text
    /// only moves the colour when it parses, and the three rows edit the list that the
    /// clipboard and the file are written from.
    #[test]
    fn a_palette_session_edits_the_colour_it_shows_and_the_list_it_writes() {
        let mut palette = Palette::from_colour("#6750A4").expect("the code parses");
        assert!(!palette.editing());
        assert_eq!(
            palette.menu(),
            vec![
                "pin-menu/edit",
                "pin-menu/copy-hex",
                "pin-menu/copy-rgb",
                "pin-menu/copy-hsl"
            ]
        );
        // Read-only: the field does not exist, so nothing reports a change.
        assert!(!palette.change(Id::str("body"), Change::Text("#22CC88".into())));

        assert!(palette.act(Id::str("pin-menu/edit")));
        assert!(palette.editing());
        assert_eq!(
            palette.menu()[..6],
            [
                "pin-menu/done",
                "pin-menu/roles",
                "pin-menu/pick",
                "pin-menu/add-colour",
                "pin-menu/remove-colour",
                "pin-menu/copy-hex"
            ][..]
        );
        let field = Id::str("field");
        palette.edit.as_mut().expect("a session").field = Some(field);
        assert!(palette.owns(field));
        assert_eq!(
            palette.edit.as_ref().map(|editing| editing.code.as_str()),
            Some("#6750A4"),
            "the field opens on the shown colour"
        );

        // A code that cannot parse yet stays in the field and leaves the card alone.
        assert!(palette.change(field, Change::Text("#2".into())));
        assert_eq!(palette.selected().colour.hex(), "#6750A4");
        assert_eq!(
            palette.edit.as_ref().map(|editing| editing.code.as_str()),
            Some("#2")
        );

        assert!(palette.change(field, Change::Text("#22CC88".into())));
        assert_eq!(palette.selected().colour.hex(), "#22CC88");
        let copied = palette.clipboard((0, 0)).expect("a palette copies");
        assert_eq!(copied[0].1, b"#22CC88\n");

        // Roles: the list becomes the MD3 tonal-spot roles of the shown colour.
        assert!(palette.act(Id::str("pin-menu/roles")));
        assert_eq!(palette.colours.len(), ui_color_hct::Role::ALL.len());
        assert_eq!(palette.selected().label, "primary");
        let primary = palette.selected().colour;
        assert_ne!(
            primary.hex(),
            "#22CC88",
            "the seed is the source, not the primary"
        );
        assert_eq!(
            palette.edit.as_ref().map(|editing| editing.code.as_str()),
            Some(primary.hex().as_str()),
            "the field follows the new selection"
        );
        let gpl = palette.gpl();
        assert!(gpl.contains("on-primary-container"), "{gpl}");
        assert_eq!(
            gpl.lines().count(),
            4 + palette.colours.len(),
            "the header plus a row per colour"
        );

        assert!(palette.act(Id::str("pin-menu/add-colour")));
        assert_eq!(palette.colours.len(), ui_color_hct::Role::ALL.len() + 1);
        assert_eq!(palette.selected, palette.colours.len() - 1);
        assert!(palette.act(Id::str("pin-menu/remove-colour")));
        assert_eq!(palette.colours.len(), ui_color_hct::Role::ALL.len());

        assert!(palette.act(Id::str("pin-menu/done")));
        assert!(!palette.editing());
        assert!(!palette.owns(field));
        assert!(!palette.change(field, Change::Text("#000000".into())));
    }

    /// The pick is a mode: the row arms it, a colour ends it on the shown colour, a press
    /// that found nothing still ends it, and Escape cancels the wait before the session.
    #[test]
    fn a_palette_picks_a_colour_or_ends_the_wait() {
        let mut palette = Palette::from_colour("#6750A4").expect("the code parses");
        assert!(!palette.picked(Some([1, 2, 3, 255])), "no session, no pick");
        assert!(palette.act(Id::str("pin-menu/edit")));
        assert!(!palette.picking());
        assert!(palette.menu().contains(&"pin-menu/pick"));
        assert!(palette.act(Id::str("pin-menu/pick")));
        assert!(palette.picking());
        assert!(palette.picked(Some([0x12, 0x34, 0x56, 255])));
        assert!(!palette.picking());
        assert_eq!(palette.selected().colour.hex(), "#123456");
        assert_eq!(
            palette.edit.as_ref().map(|editing| editing.code.as_str()),
            Some("#123456"),
            "the field follows the picked colour"
        );

        assert!(palette.act(Id::str("pin-menu/pick")));
        assert!(palette.picked(None));
        assert!(!palette.picking());
        assert_eq!(palette.selected().colour.hex(), "#123456");

        assert!(palette.act(Id::str("pin-menu/pick")));
        assert!(palette.edit_escape(), "Escape cancels the wait");
        assert!(!palette.picking());
        assert!(palette.editing(), "and leaves the session open");
        assert!(!palette.edit_escape(), "nothing left to cancel");
    }

    /// `--palette-from`: a picture opens as the colours it is mostly made of, heaviest
    /// first, and a picture with nothing opaque has no palette to open.
    #[test]
    fn a_picture_opens_as_the_palette_it_is_mostly_made_of() {
        let data: Vec<u8> = [[255u8, 0, 0, 255], [255, 0, 0, 255], [0, 0, 255, 255]].concat();
        let image = crate::capture::Cropped {
            origin: (0, 0),
            width: 3,
            height: 1,
            data: Arc::new(data),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
        };
        let palette =
            Palette::from_picture(&image, Some("shot.png".into()), 2).expect("a picture quantizes");
        assert_eq!(palette.colours.len(), 2);
        assert_eq!(palette.colours[0].colour.hex(), "#FF0000");
        assert_eq!(palette.colours[1].colour.hex(), "#0000FF");
        assert_eq!(palette.title.as_deref(), Some("shot.png"));

        let blank = crate::capture::Cropped {
            origin: (0, 0),
            width: 0,
            height: 0,
            data: Arc::new(Vec::new()),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
        };
        assert!(Palette::from_picture(&blank, None, 8).is_err());
    }

    /// One colour cannot be removed: the card always has something to show.
    #[test]
    fn the_last_colour_stays() {
        let mut palette = Palette::from_colour("#6750A4").expect("the code parses");
        assert!(palette.act(Id::str("pin-menu/edit")));
        assert!(palette.act(Id::str("pin-menu/remove-colour")));
        assert_eq!(palette.colours.len(), 1);
    }

    #[test]
    fn a_single_colour_pin_lists_no_swatches_but_keeps_its_codes() {
        let palette = Palette::from_colour("hsl(0, 100%, 50%)").expect("the code parses");
        assert_eq!(palette.colours.len(), 1);
        assert_eq!(palette.selected().colour.hex(), "#FF0000");
        assert_eq!(
            palette.listed().count(),
            0,
            "the shown colour leaves the list"
        );
        let (width, height) = palette.size();
        assert!(width > CODES_WIDTH && height > SELECTED, "{width}x{height}");
    }
}
