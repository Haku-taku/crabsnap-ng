//! The text family: a source laid out as a card, read-only.
//!
//! Plain, Markdown, code, LaTeX and the checklist share the card, the clipboard and the
//! export; only the node inside differs, so one module keeps them from drifting apart.
//!
//! `size()` is an estimate until the first frame has laid the card out: the canvas reads the
//! real height back from the layout and shifts the pin so the centre it was placed by stays.

use std::sync::Arc;

use crabsnap_core::media::{Kind, TextKind};
use ui_core::geom::{Color, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::text::{Family, GenericFamily, TextBlock, TextRun, TextStyle};
use ui_core::{Decl, Id, NodeData, NodeKind};
use ui_platform::element::Element;
use ui_widgets::Widget;

use crate::pin::card;
use crate::pin::ink::Ink;
use crate::pin::{ExportAsk, Exported, PinContent, PinFrame};

/// The widest a text pin lays its column out at, in logical points.
const MAX_WIDTH: f32 = 480.0;
const PAD: f32 = 14.0;
const BODY_SIZE: f32 = 14.0;
const CODE_SIZE: f32 = 13.0;
const MATH_SIZE: f32 = 18.0;

pub(crate) struct Text {
    kind: TextKind,
    source: String,
    /// The extension's token: what names the language a code pin is written in.
    name_hint: Option<&'static str>,
    title: Option<String>,
    /// Parsed once at open; a checklist parses to the same rows the Markdown widget draws.
    document: Option<Arc<ui_richtext::Document>>,
    /// Whether this pin's editor is open. Only Markdown has one so far.
    edit: bool,
    /// The node the control draws, recorded at build time: a change from it is ours.
    body: Option<Id>,
    /// The caret was asked for and not yet handed over: the first build of a session.
    focus_pending: bool,
}

impl Text {
    pub(crate) fn read(
        kind: TextKind,
        bytes: Vec<u8>,
        name_hint: Option<&'static str>,
        title: Option<String>,
    ) -> Self {
        // The head was valid UTF-8 when it was classified; the rest may not be, and a pin
        // that shows the readable parts is better than one that refuses to open.
        let source = String::from_utf8_lossy(&bytes).into_owned();
        let document = (kind == TextKind::Markdown).then(|| Arc::new(ui_richtext::parse(&source)));
        Self {
            kind,
            source,
            name_hint,
            title,
            document,
            edit: false,
            body: None,
            focus_pending: false,
        }
    }

    /// Whether this kind carries an editor. A palette is not this content at all — the
    /// factory builds its own, whose session edits swatches instead of one source string.
    fn has_editor(&self) -> bool {
        self.kind != TextKind::Palette
    }

    /// The extension a save of this kind writes.
    fn extension(&self) -> &'static str {
        match self.kind {
            TextKind::Plain | TextKind::Palette => "txt",
            TextKind::Markdown => "md",
            TextKind::Latex => "tex",
            TextKind::Code => self.name_hint.unwrap_or("txt"),
        }
    }

    fn line_height(&self) -> f32 {
        let size = match self.kind {
            TextKind::Code => CODE_SIZE,
            TextKind::Latex => MATH_SIZE,
            _ => BODY_SIZE,
        };
        size * ui_widgets::LINE_HINT
    }

    /// The card's node, laid out at the content's own size; the canvas scales it.
    fn node(&self, cx: &mut ui_core::cx::Cx<'_>, frame: &PinFrame) -> Box<dyn Element> {
        let ink = Ink::card();
        let inner = (frame.size.0 - PAD * 2.0).max(1.0);
        let body = match self.kind {
            TextKind::Markdown => self.markdown(cx, frame, inner, &ink),
            TextKind::Latex => self.math(cx, frame, &ink),
            TextKind::Code if self.edit => self.code_input(cx, frame, inner, &ink),
            TextKind::Code => Box::new(ui_widgets::from_decl(self.code(frame, inner, &ink))),
            TextKind::Plain if self.edit => self.plain_input(cx, frame, inner, &ink),
            // A palette never reaches here; the factory routes it to its own content.
            TextKind::Plain | TextKind::Palette => {
                Box::new(ui_widgets::from_decl(self.plain(frame, inner, &ink)))
            }
        };
        // The height is the laid-out one: the canvas learns it from the rect and draws the
        // border there, so nothing here has to guess twice.
        Box::new(
            card::filled(
                card::shell(frame, NodeKind::Column, card::hug_height(frame.size.0)),
                frame,
                ink.surface,
            )
            .padding(Insets::all(PAD))
            .child(body),
        )
    }

    fn plain(&self, frame: &PinFrame, inner: f32, ink: &Ink) -> Decl {
        let block = TextBlock::plain(self.source.clone(), TextStyle::new(BODY_SIZE));
        text_decl(frame.id.with("body"), block, BODY_SIZE, ink.body, inner)
    }

    /// The plain text session: one engine field over the whole source, its single view at
    /// base 0, so a press lands a byte and Enter opens a line. The engine owns the caret,
    /// the selection and the undo history of an input.
    fn plain_input(
        &self,
        cx: &mut ui_core::cx::Cx<'_>,
        frame: &PinFrame,
        inner: f32,
        ink: &Ink,
    ) -> Box<dyn Element> {
        let body = frame.id.with("body");
        let view = body.with("text");
        let focused = cx.focused() == Some(body);
        let caret = focused.then(|| cx.caret_for(body)).flatten();
        let preedit = focused.then(|| cx.preedit_for(body)).flatten();
        // The preedit is drawn in the view, not painted over it: the input method's bytes
        // sit where the committed ones would.
        let (shown, composing, caret_at) =
            ui_widgets::text::typed_input(&self.source, caret, preedit.as_ref());
        let selection = cx
            .selection_for(body)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: ink.selection,
                painter: None,
            });
        let mut text = ui_widgets::text::Text::new(view, shown, BODY_SIZE, ink.body)
            .max_width(inner)
            .composing(composing)
            .painting(selection);
        if let (true, Some(at)) = (focused, caret_at) {
            text = text.caret(at, ink.body);
        }
        let mut decl = text.resolve();
        if let NodeData::Text { block, .. } = &mut decl.data {
            std::rc::Rc::make_mut(block).input_base = Some(0);
        }
        self.field(frame, inner, decl)
    }

    /// The input container both text sessions share: one field over the source, its view
    /// inside. The field declares the stop itself, because the view draws the only ink.
    fn field(&self, frame: &PinFrame, inner: f32, view: Decl) -> Box<dyn Element> {
        Box::new(ui_widgets::from_decl(
            Decl::container(frame.id.with("body"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fixed(inner),
                    // A fit hint caps a container, so a line-box hint would hold a
                    // multi-line field to one line while its view overflowed.
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                .focus_stop(true)
                .cursor(ui_core::CursorShape::Text)
                .text_input(self.source.clone())
                .text_input_enter_newline()
                .child(view),
        ))
    }

    /// The code session: the same field over the highlighter's runs, so every token keeps
    /// its ink while the user types through it.
    fn code_input(
        &self,
        cx: &mut ui_core::cx::Cx<'_>,
        frame: &PinFrame,
        inner: f32,
        ink: &Ink,
    ) -> Box<dyn Element> {
        let body = frame.id.with("body");
        let view = body.with("text");
        let base = TextStyle::new(CODE_SIZE).family(Family::Generic(GenericFamily::Monospace));
        let mut runs = highlighted(base, &self.source, self.name_hint, ink);
        let focused = cx.focused() == Some(body);
        let caret = focused.then(|| cx.caret_for(body)).flatten();
        let preedit = focused.then(|| cx.preedit_for(body)).flatten();
        let mut composing = None;
        let mut caret_at = caret;
        if let Some(preedit) = preedit.as_ref().filter(|preedit| !preedit.is_empty()) {
            let at = caret.unwrap_or(self.source.len());
            // A composition typed inside a token keeps that token's ink, as it would if the
            // input method's bytes were committed there.
            let (style, run_ink) = run_look(&runs, at, base);
            let run = TextRun {
                ink: run_ink,
                ..TextRun::new(&preedit.text, style)
            };
            runs = ui_widgets::text::splice_run(runs, at, run);
            composing = Some(at..at + preedit.text.len());
            caret_at = Some(at + preedit.cursor.min(preedit.text.len()));
        }
        let selection = cx
            .selection_for(body)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: ink.selection,
                painter: None,
            });
        let mut block = TextBlock {
            runs,
            ..TextBlock::plain(String::new(), base)
        };
        block.input_base = Some(0);
        let mut decl = text_decl(view, block, CODE_SIZE, ink.code_ink, inner);
        if let NodeData::Text {
            caret: node_caret,
            selection: node_selection,
            composing: node_composing,
            ..
        } = &mut decl.data
        {
            *node_caret = caret_at.map(|at| ui_core::Caret {
                offset: at,
                color: ink.code_ink,
            });
            *node_selection = selection;
            *node_composing = composing;
        }
        self.field(frame, inner, decl)
    }

    fn code(&self, frame: &PinFrame, inner: f32, ink: &Ink) -> Decl {
        let style = TextStyle::new(CODE_SIZE).family(Family::Generic(GenericFamily::Monospace));
        let runs = highlighted(style, &self.source, self.name_hint, ink);
        let block = TextBlock {
            runs,
            ..TextBlock::plain(String::new(), style)
        };
        text_decl(frame.id.with("body"), block, CODE_SIZE, ink.code_ink, inner)
    }

    /// The formula the pin is: read as a formula, or edited by structure in place. The
    /// engine keeps the structure, the caret and the history; every edit returns as the
    /// formula's source, so the pin still owns one string.
    fn math(&self, cx: &mut ui_core::cx::Cx<'_>, frame: &PinFrame, ink: &Ink) -> Box<dyn Element> {
        let formula = ui_widgets::Math {
            id: frame.id.with("body"),
            source: self.source.clone(),
            size: MATH_SIZE,
            ink: ink.body,
            family: None,
            display: true,
            error: None,
            editable: self.edit,
            owner: None,
            caret_ends: true,
        };
        let wcx = ui_widgets::Cx::new(cx, Arc::clone(&frame.env));
        Box::new(ui_widgets::from_decl(formula.decl(&wcx)))
    }

    fn markdown(
        &self,
        cx: &mut ui_core::cx::Cx<'_>,
        frame: &PinFrame,
        inner: f32,
        ink: &Ink,
    ) -> Box<dyn Element> {
        let document = Arc::clone(
            self.document
                .as_ref()
                .expect("a markdown pin parses its source when it opens"),
        );
        let control = ui_widgets::Markdown {
            id: frame.id.with("body"),
            document,
            prepared: None,
            // Read-only draws the same rows, without syntax, caret or selection.
            editable: self.edit,
            math_editing: ui_widgets::MathEditing::Structural,
            // An opaque surface: a translucent one would show the document through a formula.
            popover_fill: ink.surface,
            width: inner,
            size: BODY_SIZE,
            ink: ink.body,
            marker_ink: ink.muted,
            link_ink: ink.link,
            code_ink: ink.code_ink,
            code_fill: ink.code_fill,
            code_style: Some(fenced_code),
            code: ui_widgets::CodeOptions::default(),
            image: None,
            muted: ink.muted,
            caret_ink: ink.body,
            selection_fill: ink.selection,
            selection_painter: None,
        };
        let wcx = ui_widgets::Cx::new(cx, Arc::clone(&frame.env));
        Box::new(ui_widgets::from_decl(control.decl(&wcx)))
    }
}

impl PinContent for Text {
    fn kind(&self) -> Kind {
        Kind::Text(self.kind)
    }

    fn title(&self) -> Option<String> {
        self.title.clone()
    }

    fn size(&self) -> (f32, f32) {
        // A provisional height, replaced by the laid-out one on the first frame: only the
        // centring and the first cascade step read it before then.
        let lines = self.source.lines().count().max(1) as f32;
        (MAX_WIDTH, PAD * 2.0 + lines * self.line_height())
    }

    fn image(&mut self, _texture: u64, _drawn: (f32, f32)) -> Option<ui_core::draw::ImageEntry> {
        None
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>, frame: PinFrame) -> Box<dyn Element> {
        let body = frame.id.with("body");
        self.body = Some(body);
        // The session opens with the caret, as a press inside the control would give it.
        if self.edit && std::mem::take(&mut self.focus_pending) {
            cx.request_focus(body);
        }
        self.node(cx, &frame)
    }

    fn menu(&self) -> Vec<&'static str> {
        if !self.has_editor() {
            return Vec::new();
        }
        vec![if self.edit {
            "pin-menu/done"
        } else {
            "pin-menu/edit"
        }]
    }

    fn act(&mut self, item: Id) -> bool {
        if !self.has_editor() {
            return false;
        }
        if item == Id::str("pin-menu/edit") {
            self.edit = true;
            self.focus_pending = true;
            return true;
        }
        if item == Id::str("pin-menu/done") {
            self.edit = false;
            return true;
        }
        false
    }

    fn owns(&self, id: Id) -> bool {
        self.edit && self.body == Some(id)
    }

    /// A keystroke is the whole buffer, so the source is what the user sees and the parse
    /// follows it; copy and export then write the edited bytes without asking again.
    fn change(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
        let ui_core::change::Change::Text(source) = change else {
            return false;
        };
        if !self.edit || source == self.source {
            return false;
        }
        self.document = self
            .document
            .as_ref()
            .map(|document| Arc::new(document.reparse(&source).document));
        self.source = source;
        true
    }

    fn editing(&self) -> bool {
        self.edit
    }

    fn clipboard(&mut self, _drawn: (u32, u32)) -> Result<Vec<(String, Vec<u8>)>, String> {
        let bytes = self.source.clone().into_bytes();
        let mut payloads = vec![("text/plain".to_string(), bytes.clone())];
        // Markdown-aware editors ask for this one; the plain payload stays first for the
        // editors that would not recognise it.
        if self.kind == TextKind::Markdown {
            payloads.push(("text/markdown".to_string(), bytes));
        }
        Ok(payloads)
    }

    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String> {
        if crate::pin::asks_for_image(ask) {
            return Err(format!(
                "a {} pin writes text, not an image; name a .{} file",
                self.kind().name(),
                self.extension()
            ));
        }
        Ok(Exported {
            bytes: self.source.clone().into_bytes(),
            extension: self.extension(),
        })
    }
}

/// The style and ink of the run covering byte `at`, or the base style past the last run.
fn run_look(runs: &[TextRun], at: usize, base: TextStyle) -> (TextStyle, Option<Color>) {
    let mut seen = 0;
    for run in runs {
        let end = seen + run.text.len();
        if at >= seen && at <= end {
            return (run.style, run.ink);
        }
        seen = end;
    }
    (base, None)
}

/// A text node in the card: the run list laid out at `max_width`, hugging its height.
pub(crate) fn text_decl(id: Id, block: TextBlock, size: f32, ink: Color, max_width: f32) -> Decl {
    let mut decl = Decl::container(id, NodeKind::Text);
    decl.data = NodeData::Text {
        ink,
        block: std::rc::Rc::new(block),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::hug(0.0, max_width),
        height: AxisSize::hug(size, size * ui_widgets::LINE_HINT),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    decl
}

/// The fenced block highlighter the Markdown card uses.
fn fenced_code(language: Option<&str>, source: &str, base: TextStyle) -> Option<Vec<TextRun>> {
    let language = language?;
    Some(highlighted(base, source, Some(language), &Ink::card()))
}

/// `source` cut at the highlighted ranges; the gaps keep the base style.
fn highlighted(base: TextStyle, source: &str, language: Option<&str>, ink: &Ink) -> Vec<TextRun> {
    let Some(grammar) = language.and_then(ui_highlight::Language::from_info) else {
        return vec![TextRun::new(source, base)];
    };
    let tokens = match ui_highlight::tokens(grammar, source) {
        Ok(tokens) => tokens,
        Err(error) => {
            eprintln!("pin: cannot highlight this source: {error}");
            return vec![TextRun::new(source, base)];
        }
    };
    let mut runs = Vec::with_capacity(tokens.len() + 1);
    let mut end = 0;
    for token in tokens.iter() {
        // A range off a character boundary would panic to slice; the grammar is not trusted.
        let Some(text) = source.get(token.range.clone()) else {
            continue;
        };
        if token.range.start > end {
            runs.push(TextRun::new(&source[end..token.range.start], base));
        }
        let mut run = TextRun::new(text, base).inked(ink.token(token.class));
        run.style.italic = token.class == ui_highlight::TokenClass::Comment;
        runs.push(run);
        end = token.range.end;
    }
    if end < source.len() {
        runs.push(TextRun::new(&source[end..], base));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::change::Change;

    /// The session's state machine: only the session takes a keystroke, a keystroke is the
    /// whole buffer, and the parse follows it — copy and export read the same source.
    #[test]
    fn a_markdown_session_owns_the_source_it_edits() {
        let mut pin = Text::read(
            TextKind::Markdown,
            b"# Title\n\nbody\n".to_vec(),
            None,
            None,
        );
        assert!(!pin.editing());
        assert_eq!(pin.menu(), vec!["pin-menu/edit"]);
        // Read-only: a change is nobody's and moves nothing.
        assert!(!pin.change(Id::str("body"), Change::Text("# changed\n".into())));
        assert_eq!(pin.source, "# Title\n\nbody\n");

        assert!(pin.act(Id::str("pin-menu/edit")));
        assert!(pin.editing());
        assert_eq!(pin.menu(), vec!["pin-menu/done"]);
        assert!(
            pin.focus_pending,
            "the session opens with the caret asked for"
        );
        pin.body = Some(Id::str("body"));
        assert!(pin.owns(Id::str("body")));

        assert!(pin.change(Id::str("body"), Change::Text("# Changed\n\n- a\n".into())));
        assert_eq!(pin.source, "# Changed\n\n- a\n");
        let document = pin.document.as_ref().expect("a markdown pin keeps a parse");
        assert_eq!(&*document.source, "# Changed\n\n- a\n");
        assert!(document.blocks.len() >= 2, "the edited list is parsed");
        let payloads = pin.clipboard((0, 0)).expect("text copies");
        assert_eq!(payloads[0].1, b"# Changed\n\n- a\n");

        assert!(pin.act(Id::str("pin-menu/done")));
        assert!(!pin.editing());
        assert!(!pin.owns(Id::str("body")));
        assert_eq!(pin.menu(), vec!["pin-menu/edit"]);
    }

    /// Every text kind enters its session but a palette, whose editor is still missing:
    /// the row a pin draws and the editor behind it never disagree.
    #[test]
    fn every_text_kind_but_a_palette_offers_the_session() {
        let mut plain = Text::read(TextKind::Plain, b"hello\n".to_vec(), None, None);
        assert_eq!(plain.menu(), vec!["pin-menu/edit"]);
        assert!(plain.act(Id::str("pin-menu/edit")));
        assert!(plain.editing());
        assert!(plain.change(Id::str("body"), Change::Text("hello there\n".into())));
        assert_eq!(plain.source, "hello there\n");
        assert!(plain.document.is_none(), "plain text keeps no parse");

        let mut code = Text::read(TextKind::Code, b"fn main() {}\n".to_vec(), Some("rs"), None);
        assert_eq!(code.menu(), vec!["pin-menu/edit"]);
        assert!(code.act(Id::str("pin-menu/edit")));
        assert!(code.change(Id::str("body"), Change::Text("fn main() { }\n".into())));
        assert_eq!(code.source, "fn main() { }\n");

        let mut latex = Text::read(TextKind::Latex, b"x^2\n".to_vec(), None, None);
        assert_eq!(latex.menu(), vec!["pin-menu/edit"]);
        assert!(latex.act(Id::str("pin-menu/edit")));
        // A structural edit arrives as the formula's own source.
        assert!(latex.change(Id::str("body"), Change::Text("x^3\n".into())));
        assert_eq!(latex.source, "x^3\n");

        let mut palette = Text::read(TextKind::Palette, b"#000\n".to_vec(), None, None);
        assert!(
            palette.menu().is_empty(),
            "a palette is another content, with a session of its own"
        );
        assert!(!palette.act(Id::str("pin-menu/edit")));
        assert!(!palette.editing());
    }
}
