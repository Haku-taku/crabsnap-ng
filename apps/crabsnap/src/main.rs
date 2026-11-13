//! CrabSnap: a screenshot and pin tool on the ui engine.
//!
//! The process owns one compositor connection; every surface — this toolbar, the capture
//! overlay and the pin canvases — is opened on it.
//!
//! Commands: `crabsnap shot output|region`, `crabsnap pin …`, `crabsnap info …`; run
//! `crabsnap --help` for the tree. With no argument the app shows its toolbar.
//! `CRABSNAP_SECONDS=n` still bounds a scripted run.

mod toolbar;
mod capture;
mod cli;
mod codec;
mod color;
mod daemon;
mod document;
mod ffmpeg;
mod hotkeys;
mod loupe;
mod notice;
mod pill;
mod pin;
mod pins;
mod readout;
mod run;
mod zoom;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crabsnap_core::config::{Config, PinLook};
use crabsnap_core::protocol::PinOrigin;
use crabsnap_platform::paths;
use pin::badge::{self, Badge};
use pin::edit;
use pin::{Bitmap, ExportAsk, PinContent, PinFrame};
use ui_core::anim::{AnimSpec, Easing};
use ui_core::colour::ContentColour;
use ui_core::draw::{Canvas, CanvasId, ImageEntry, ImageSource, Sampling, Shape};
use ui_core::geom::{Color, Corners, Insets, Point, Rect, Size};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ImageFit;
use ui_core::{CursorShape, Id, NodeKind, Sense};
use ui_i18n::{Language, LanguageChoice};
use ui_platform::element::Element;
use ui_platform::host::{Host, OpenOptions};
use ui_platform::view::App;
use ui_platform::window::{Anchor, ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceSpec};
use ui_platform_wayland::{WaylandBackend, WaylandLink};
use ui_style::Env;
use ui_widgets_md3::Md3Kit;

use crate::document::{Document, Space};

const EN: &str = include_str!("../locales/en/crabsnap.ftl");
const ZH_CN: &str = include_str!("../locales/zh-CN/crabsnap.ftl");

fn text(id: &'static str, body: Arc<str>, size: f32, ink: Color) -> ui_core::Decl {
    let mut decl = ui_core::Decl::container(Id::str(id), NodeKind::Text).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(size, size * 1.4),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    decl.data = ui_core::NodeData::Text {
        block: ui_core::TextBlock::plain(body.to_string(), ui_core::TextStyle::new(size)).into(),
        ink,
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl
}

/// The toolbar the overlay shows before a capture exists.
struct Toolbar {
    language: Arc<Language>,
}

impl App for Toolbar {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let catalog = self.language.catalog();
        let title = catalog.text_by_id("toolbar-title");
        let hint = catalog.text_by_id("toolbar-hint");
        let bar = ui_core::Decl::container(Id::str("toolbar"), NodeKind::Row)
            .size(SizeClass::fixed(480.0, 56.0))
            .padding(Insets::all(14.0))
            .gap(12.0)
            .background(Color::from_rgba8(24, 26, 32, 240), Corners::all(14.0))
            .border(Color::from_rgba8(96, 102, 116, 255), 1.0)
            .child(text(
                "toolbar/title",
                title,
                15.0,
                Color::from_rgba8(232, 236, 244, 255),
            ))
            .child(text(
                "toolbar/hint",
                hint,
                13.0,
                Color::from_rgba8(150, 158, 172, 255),
            ));
        Box::new(
            ui_platform::Container::new(Id::str("root"), NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                .child_decl(bar),
        )
    }
}

/// What a surface needs from the user's environment: the configuration and the encoders it
/// implies. Probing ffmpeg once per process is what makes a later save cheap.
pub(crate) struct Settings {
    pub(crate) config: Config,
    pub(crate) codecs: codec::Codecs,
    /// The chords the file resolves to, parsed once: every surface and tooltip reads these.
    pub(crate) shortcuts: hotkeys::Plan,
    /// How pins look; resolved once, because a canvas paints the same look every frame.
    pub(crate) look: PinLook,
    /// The language every surface of this process builds under. `reload` changes its
    /// choice in place, so a window that already holds it follows the switch.
    pub(crate) language: Arc<Language>,
}

impl Settings {
    pub(crate) fn load(language: Arc<Language>) -> Result<Arc<Settings>, String> {
        let config = Config::load()?;
        let codecs = codec::Codecs::probe(
            config.codec.program.as_deref(),
            config.codec.library_directory.as_deref(),
            &config.codec.options,
        );
        let (look, _) = config.pin_look();
        let shortcuts = hotkeys::Plan::resolve(&config);
        Ok(Arc::new(Self {
            config,
            codecs,
            shortcuts,
            look,
            language,
        }))
    }
}

/// The captured output, dimmed outside the selection.
///
/// The texture is uploaded on registration and never again: every later frame samples it,
/// so dragging the selection never re-encodes the screenshot.
struct Overlay {
    texture: u64,
    /// The same pixels the texture was uploaded from, kept for cropping.
    image: capture::Image,
    /// Device pixels per logical point on this surface, read from each frame. The capture's
    /// own idea of it is the compositor's integer scale; this is what the overlay is drawn
    /// at, and a selection must be converted with it.
    surface_scale: f32,
    /// The screen the frame came from; a saved file carries its name.
    source: Option<String>,
    language: Arc<Language>,
    /// Where a save goes, and what this machine can encode.
    settings: Arc<Settings>,
    env: Arc<Env>,
    kit: Md3Kit,
    /// A PNG the user asked to copy; the next build hands it to the engine.
    pending_copy: Option<Vec<u8>>,
    /// The encode behind `pending_copy`, when it is still running.
    copy_job: Option<ui_platform::jobs::Job<Result<Vec<u8>, String>>>,
    /// The write behind a save, when it is still running.
    save_job: Option<ui_platform::jobs::Job<Result<std::path::PathBuf, String>>>,
    /// The bar a copy or a save answers with; both leave nothing else on screen.
    notice: notice::Notice,
    /// Background work: saves and copies must not stall the frame they start in.
    jobs: ui_platform::jobs::Jobs,
    /// Pins this overlay asked for, drained by whoever owns the canvases.
    pending_pins: Rc<RefCell<Vec<Pin>>>,
    /// Set once the selection is finished with, so the driver can drop this surface.
    close: bool,
    /// Where `Save` writes; `None` means the screenshot directory.
    file: Option<std::path::PathBuf>,
    /// What the live press is doing, if anything.
    gesture: Option<Gesture>,
    /// Where the pointer is during that press.
    current: Option<Point>,
    /// The rectangle a finished drag left behind; the toolbar will act on it.
    settled: Option<Rect>,
    /// Whether the previous frame saw this root pressed, so a press is caught at its edge.
    ///
    /// A button already held when the overlay appears began before the overlay existed —
    /// the hotkey that opened it, most likely — so the first press it sees is not a drag.
    pressed_before: bool,
    /// The loupe's button, zoom and wheel remainder.
    loupe: loupe::Loupe,
    /// The floating toolbar: where it hangs, and its drag.
    toolbar: toolbar::Toolbar,
    /// Marks drawn over the frozen frame, in the picture's own pixel grid. The selection is
    /// only the region an export takes; what is marked lives on the whole frame, so moving
    /// the rectangle never moves a mark.
    document: Document,
    /// The annotation session over the frozen frame.
    annotate: edit::Session,
    /// The press that answered the eyedropper is the eyedropper's: it draws nothing and
    /// moves nothing, for as long as the button is down.
    pick_press: bool,
    /// A cursor key's pending movement in fine steps, banked until the next build applies it.
    nudge: Option<(f32, f32)>,
    /// Whether the middle button is down, and whether this press has already been offered to
    /// the selection. A press that began outside it never grabs one later.
    middle: bool,
    middle_offered: bool,
    /// What the middle button grabbed: where it went down, and the rectangle then.
    middle_drag: Option<(Point, Rect)>,
}

/// The texture every overlay uploads its frozen frame under.
pub(crate) const FROZEN_TEXTURE: u64 = 0x6372_6162_736e_6170;

/// The texture the overlay's annotation canvas paints into: the marks alone, transparent
/// where the frozen frame shows through.
const MARKS_TEXTURE: u64 = 0x6372_6162_736e_6d6b;

/// Below this a drag is a click: a sliver is not something a user meant to select.
const MIN_SELECTION: f32 = 4.0;
/// One cursor-key step, in pixels of the picture the pointer is over: the unit an edge is
/// placed in, and the grid of the capture at native zoom.
const CURSOR_STEP: f32 = 1.0;
/// With Shift the same key moves ten times as far: the coarse half of one gesture.
const CURSOR_STEP_FAST: f32 = 10.0;

/// The direction a cursor action names, in canvas pixels.
fn cursor_direction(id: ui_core::ShortcutId) -> Option<(f32, f32)> {
    if id == hotkeys::CURSOR_LEFT {
        Some((-1.0, 0.0))
    } else if id == hotkeys::CURSOR_RIGHT {
        Some((1.0, 0.0))
    } else if id == hotkeys::CURSOR_UP {
        Some((0.0, -1.0))
    } else if id == hotkeys::CURSOR_DOWN {
        Some((0.0, 1.0))
    } else {
        None
    }
}

/// How many fine steps one press covers: one, or ten with Shift held.
///
/// A press banks this count rather than a distance, so the size is settled by the press
/// and the picture under the pointer only says what one step measures.
fn cursor_steps(shift: bool) -> f32 {
    if shift {
        CURSOR_STEP_FAST / CURSOR_STEP
    } else {
        1.0
    }
}

/// The points `pixels` of a picture cover, for a picture drawn at `zoom` over a canvas of
/// `dots` device pixels per point: one canvas pixel is not one screen pixel unless both
/// are 1.
fn cursor_step(pixels: f32, zoom: f32, dots: f32) -> f32 {
    pixels * zoom / dots.max(f32::EPSILON)
}
/// The square a handle draws, and the radius it answers within.
const HANDLE: f32 = 8.0;
const HANDLE_REACH: f32 = 7.0;

/// What the press that is currently down is doing.
enum Gesture {
    /// Drawing a new rectangle from `start`.
    Select { start: Point },
    /// Moving one edge or corner of `origin`, which stays put until the release.
    Resize { handle: Handle, origin: Rect },
    /// Sliding the whole rectangle: where it was grabbed, and the rectangle at that moment.
    Move { grab: Point, origin: Rect },
}

/// One of the eight grips on a settled selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

impl Handle {
    /// Corners before edges, so a grip in a corner is not stolen by the two edges meeting it.
    const ALL: [Handle; 8] = [
        Handle::NorthWest,
        Handle::NorthEast,
        Handle::SouthEast,
        Handle::SouthWest,
        Handle::North,
        Handle::East,
        Handle::South,
        Handle::West,
    ];

    /// The point this grip sits on.
    fn at(self, rect: Rect) -> Point {
        let (x0, y0, x1, y1) = (rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y());
        let (x, y) = match self {
            Handle::NorthWest => (x0, y0),
            Handle::North => (rect.center().x, y0),
            Handle::NorthEast => (x1, y0),
            Handle::East => (x1, rect.center().y),
            Handle::SouthEast => (x1, y1),
            Handle::South => (rect.center().x, y1),
            Handle::SouthWest => (x0, y1),
            Handle::West => (x0, rect.center().y),
        };
        Point::new(x, y)
    }

    /// The grip `at` holds, if the pointer is close enough to one.
    fn grabbed(rect: Rect, at: Point) -> Option<Handle> {
        Handle::ALL.into_iter().find(|handle| {
            let grip = handle.at(rect);
            (at.x - grip.x).abs() <= HANDLE_REACH && (at.y - grip.y).abs() <= HANDLE_REACH
        })
    }

    /// The rect after this grip is dragged to `to`.
    ///
    /// Only its own edges move, the opposite ones stay still, and the minimum keeps the
    /// rectangle from turning inside out when the pointer crosses it.
    fn resized(self, rect: Rect, to: Point, bounds: Size) -> Rect {
        let to = Point::new(to.x.clamp(0.0, bounds.w), to.y.clamp(0.0, bounds.h));
        let (mut x0, mut y0) = (rect.min_x(), rect.min_y());
        let (mut x1, mut y1) = (rect.max_x(), rect.max_y());
        if matches!(self, Handle::NorthWest | Handle::West | Handle::SouthWest) {
            x0 = to.x.min(x1 - MIN_SELECTION);
        }
        if matches!(self, Handle::NorthEast | Handle::East | Handle::SouthEast) {
            x1 = to.x.max(x0 + MIN_SELECTION);
        }
        if matches!(self, Handle::NorthWest | Handle::North | Handle::NorthEast) {
            y0 = to.y.min(y1 - MIN_SELECTION);
        }
        if matches!(self, Handle::SouthWest | Handle::South | Handle::SouthEast) {
            y1 = to.y.max(y0 + MIN_SELECTION);
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    fn cursor(self) -> CursorShape {
        match self {
            Handle::North | Handle::South => CursorShape::ResizeVertical,
            Handle::East | Handle::West => CursorShape::ResizeHorizontal,
            Handle::NorthWest | Handle::SouthEast => CursorShape::ResizeDiagonalDownRight,
            Handle::NorthEast | Handle::SouthWest => CursorShape::ResizeDiagonalUpRight,
        }
    }
}

impl Overlay {
    /// An overlay over a frozen frame. The pins it asks for leave through `pins`: the app
    /// decides the selection, but only the owner of the connection can open a canvas.
    pub(crate) fn new(
        image: capture::Image,
        source: Option<String>,
        settings: Arc<Settings>,
        language: Arc<Language>,
        pins: Rc<RefCell<Vec<Pin>>>,
        jobs: ui_platform::jobs::Jobs,
    ) -> Self {
        Self {
            texture: FROZEN_TEXTURE,
            // The capture's scale is the best guess until the first frame says better.
            surface_scale: image.scale,
            image,
            source,
            env: toolbar::env(&language),
            language,
            settings,
            kit: Md3Kit::new(),
            pending_copy: None,
            copy_job: None,
            save_job: None,
            notice: notice::Notice::new(),
            jobs,
            file: None,
            pending_pins: pins,
            close: false,
            gesture: None,
            current: None,
            settled: None,
            // A button held while the overlay opens is not a drag: wait for a fresh press.
            pressed_before: true,
            loupe: loupe::Loupe::new(),
            toolbar: toolbar::Toolbar::new(),
            document: Document::new(),
            annotate: edit::Session::new(),
            pick_press: false,
            nudge: None,
            middle: false,
            middle_offered: false,
            middle_drag: None,
        }
    }

    /// The picture's size in the overlay's own coordinates.
    fn logical_size(&self) -> (f32, f32) {
        let scale = self.surface_scale;
        (
            self.image.width as f32 / scale,
            self.image.height as f32 / scale,
        )
    }

    /// The settled selection, cut out of the picture in its own pixels.
    ///
    /// The pointer and the overlay work in logical points; the picture's grid is device
    /// pixels, so the rectangle is converted once here and nowhere else.
    fn cropped(&self) -> Option<capture::Cropped> {
        let (width, height) = self.logical_size();
        let rect = self.selection(width, height)?;
        let scale = self.surface_scale;
        let mut cropped = self.image.crop(Rect::new(
            rect.origin.x * scale,
            rect.origin.y * scale,
            rect.size.w * scale,
            rect.size.h * scale,
        ));
        // The capture's scale is the compositor's integer one; this is the density the
        // picture was actually shown at, which is what a file should say.
        cropped.scale = scale;
        Some(cropped)
    }

    /// Turn the settled selection into a pin: the pixels, and where they belong on screen.
    fn pin(&mut self) {
        let Some(cropped) = self.cropped() else {
            return;
        };
        // The crop's own corner, not the selection's: the rectangle was rounded outwards to
        // the pixel grid, and a pin at the raw origin would sit a fraction of a pixel off.
        let (x, y) = cropped.origin;
        // The marks travel with the picture rather than in it: the pin keeps the document,
        // so a later edit can still move and undo them. The pin's grid starts at the crop's
        // corner and the overlay's at the frame's, so the marks move with the picture.
        let opened_at = self.document.depth();
        let document = std::mem::take(&mut self.document)
            .translated(-(x as f32), -(y as f32));
        let scale = self.surface_scale;
        let (width, height) = self.logical_size();
        self.pending_pins.borrow_mut().push(Pin::new(
            Box::new(Bitmap::with_marks(cropped, scale, document, opened_at)),
            Placement {
                screen: (width.round() as u32, height.round() as u32),
                dots: scale,
                position: Point::new(x as f32 / scale, y as f32 / scale),
                output: self.source.clone(),
            },
            PinOrigin::Capture {
                output: self.source.clone(),
            },
            Arc::clone(&self.language),
            Arc::clone(&self.settings),
        ));
        self.close = true;
    }

    /// The settled selection as it should leave the app: its pixels, and its marks in them.
    fn baked(&self) -> Option<capture::Cropped> {
        let cropped = self.cropped()?;
        // A file is sRGB whatever the capture was, so marks go on that rendition: refusing an
        // HDR frame would change the file only by leaving the user's marks out of it.
        let picture = if cropped.colour.transfer.is_hdr() {
            cropped.sdr()
        } else {
            cropped
        };
        // The document is drawn on the frozen frame's grid; the file holds the crop, so its
        // marks enter the crop's grid before they are rasterized into it.
        let offset = Point::new(
            -(picture.origin.0 as f32),
            -(picture.origin.1 as f32),
        );
        match pin::content::baked_in(&picture, &self.document, offset) {
            Ok(baked) => Some(baked),
            Err(error) => {
                // The bare crop is still worth keeping.
                eprintln!("marks: {error}");
                Some(picture)
            }
        }
    }

    /// Copy the settled selection as a PNG; `view` hands it to the engine next build.
    fn copy_image(&mut self) {
        let Some(cropped) = self.baked() else {
            return;
        };
        match self.jobs.submit(move || cropped.png()) {
            Ok(job) => self.copy_job = Some(job),
            Err(error) => {
                eprintln!("copy: {error}");
                self.notice
                    .show(notice::failed(&self.language.catalog(), notice::Action::Copy));
            }
        }
    }

    /// Save the settled selection as a PNG and report where it went.
    fn save(&mut self) {
        let Some(cropped) = self.baked() else {
            return;
        };
        let file = self.file.clone();
        let source = self.source.clone();
        let settings = Arc::clone(&self.settings);
        let submitted = self.jobs.submit(move || match &file {
            Some(path) => {
                let encoding = codec::Encoding::resolve(
                    None,
                    Some(path),
                    settings.config.shots.format.as_deref(),
                )?;
                settings.codecs.write(&cropped, path, encoding)?;
                Ok(path.clone())
            }
            None => save_shot_path(&cropped, source.as_deref(), &settings),
        });
        match submitted {
            Ok(job) => self.save_job = Some(job),
            Err(error) => {
                eprintln!("save: {error}");
                self.notice
                    .show(notice::failed(&self.language.catalog(), notice::Action::Save));
            }
        }
    }

    /// The shape the pointer shows: a grip's own arrows over a grip, a crosshair elsewhere.
    fn cursor_at(&self, cx: &ui_core::cx::Cx<'_>) -> CursorShape {
        // The selection is locked while a tool is armed: it draws, it never resizes.
        if self.annotate.armed() {
            return CursorShape::Crosshair;
        }
        if let Some(Gesture::Resize { handle, .. }) = self.gesture {
            return handle.cursor();
        }
        match (cx.pointer(), self.settled) {
            (Some(at), Some(rect)) => match Handle::grabbed(rect, at) {
                Some(handle) => handle.cursor(),
                None if rect.contains(at) => CursorShape::Move,
                None => CursorShape::Crosshair,
            },
            _ => CursorShape::Crosshair,
        }
    }

    fn selection(&self, width: f32, height: f32) -> Option<Rect> {
        let bounds = Size::new(width, height);
        let raw = match (&self.gesture, self.current) {
            (Some(Gesture::Select { start }), Some(current)) => between(*start, current),
            (Some(Gesture::Resize { handle, origin }), Some(current)) => {
                handle.resized(*origin, current, bounds)
            }
            _ => self.settled?,
        };
        let clamped = clamp(raw, width, height);
        (clamped.size.w >= MIN_SELECTION && clamped.size.h >= MIN_SELECTION).then_some(clamped)
    }
}

/// The rectangle two drag points span, in either direction.
fn between(a: Point, b: Point) -> Rect {
    Rect::new(
        a.x.min(b.x),
        a.y.min(b.y),
        (a.x - b.x).abs(),
        (a.y - b.y).abs(),
    )
}

/// A pointer dragged past the surface edge still selects only what the surface shows.
fn clamp(rect: Rect, width: f32, height: f32) -> Rect {
    let x0 = rect.min_x().clamp(0.0, width);
    let y0 = rect.min_y().clamp(0.0, height);
    let x1 = rect.max_x().clamp(x0, width);
    let y1 = rect.max_y().clamp(y0, height);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// A selection slid by `delta`, stopping at the surface edge instead of leaving it.
fn slid(rect: Rect, delta: Point, bounds: Size) -> Rect {
    let x = (rect.origin.x + delta.x).clamp(0.0, (bounds.w - rect.size.w).max(0.0));
    let y = (rect.origin.y + delta.y).clamp(0.0, (bounds.h - rect.size.h).max(0.0));
    Rect::new(x, y, rect.size.w, rect.size.h)
}

/// The size a content of `size` logical points takes on screen at `zoom`.
///
/// A content already states its size at native zoom, so a 2× output has halved it there
/// rather than here; this only applies the zoom.
fn drawn_size(size: (f32, f32), zoom: f32) -> (f32, f32) {
    // Exact, not rounded: the pin's node is laid out at `size` and the engine scales it, so
    // the drawn box is this product and rounding it here would only disagree with the paint.
    (size.0 * zoom, size.1 * zoom)
}

/// The screen point a picture's corner is drawn at.
///
/// At native zoom the corner snaps to the device grid, so each texel lands inside exactly
/// one device pixel instead of being blended across two. A zoomed pin cannot be texel-exact
/// whatever its origin, so it keeps the pointer's own sub-pixel position.
fn drawn_at(position: Point, dots: f32, zoom: f32) -> Point {
    if zoom != 1.0 {
        return position;
    }
    let snap = |v: f32| (v * dots).round() / dots;
    Point::new(snap(position.x), snap(position.y))
}

/// Where a pin sits: the output it belongs to, that output's device ratio, and the picture's
/// top-left in logical points.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placement {
    /// The output's logical size: what the canvas surface covers.
    pub(crate) screen: (u32, u32),
    /// Device pixels per logical point on that output.
    pub(crate) dots: f32,
    /// The picture's top-left on screen, in logical points.
    pub(crate) position: Point,
    /// The output's name: which canvas takes the pin, and what a saved file is named after.
    pub(crate) output: Option<String>,
}

/// A pinned image on the output's canvas: its pixels, where they sit, and how large.
///
/// The canvas is the screen itself, so `position` is a screen coordinate and never has to be
/// translated through a surface placement the app cannot observe.
struct Pin {
    /// Stable for the pin's life: node ids and the engine's reports name it.
    id: u64,
    /// The texture the canvas registered this pin's picture under.
    texture: u64,
    /// What the pin holds: its pixels or its layout, behind the canvas's only view of it.
    content: Box<dyn PinContent>,
    /// The output the selection came from: which canvas takes the pin.
    screen: (u32, u32),
    /// The size each badge's pill measured, keyed by badge id. A stack is placed from the
    /// previous build's sizes, so its own growth cannot feed back into the frame that grew it.
    badge_sizes: Vec<(&'static str, (f32, f32))>,
    /// A wheel step landed since the last build: the zoom chip's visit has just begun.
    zoom_visited: bool,
    /// When the previous completed click on this pin landed, in engine seconds.
    last_click: Option<f64>,
    /// Device pixels per logical point on that output: the grid the pin must land on.
    dots: f32,
    /// The image's top-left on screen, in logical points.
    position: Point,
    /// How much larger than native the picture is drawn, as lattice steps: the wheel moves
    /// this count by whole steps, so a path that comes back draws at the same scale exactly.
    zoom_steps: i32,
    /// The fraction of a step a trackpad's deltas have earned past the last whole one.
    zoom_carry: f32,
    /// The size layout gave this pin's node at native zoom, once a frame has laid it out.
    measured: Option<(f32, f32)>,
    /// Whether the placement's provisional size has been corrected by the measured one.
    settled: bool,
    /// A pull-back in flight: where it started and where it ends. A release that leaves the
    /// pin too far off its output sets this, and the canvas tweens the position until it
    /// arrives; a new drag clears it.
    settle: Option<(Point, Point)>,
    /// What the pin is made from: what its badge says and what `pin list` names.
    origin: PinOrigin,
    /// The output the pin sits on: the canvas it belongs to, and the name a save carries.
    output: Option<String>,
    /// The app's language for every pin's menu; a surface has no `Env` of its own.
    catalog: Arc<Language>,
    /// Where a save goes, and what this machine can encode.
    settings: Arc<Settings>,
    /// A payload the menu asked to copy; the next build hands it to the engine.
    pending_copy: Option<(String, Vec<u8>)>,
    /// Set by the menu's Close; the canvas drops the pin on the next build.
    closed: bool,
    /// Hidden by `pin hide`: still held, still listed, not painted.
    shown: bool,
}

impl Pin {
    /// A pin around `content`, at `position` on the output of size `screen`.
    ///
    /// The texture stays unset: whoever registers the pixels decides the id, so the canvas
    /// and the daemon can each open their own.
    pub(crate) fn new(
        content: Box<dyn PinContent>,
        at: Placement,
        origin: PinOrigin,
        catalog: Arc<Language>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            id: 0,
            texture: 0,
            content,
            screen: at.screen,
            badge_sizes: Vec::new(),
            zoom_visited: false,
            last_click: None,
            dots: at.dots.max(f32::EPSILON),
            position: at.position,
            zoom_steps: 0,
            zoom_carry: 0.0,
            measured: None,
            settled: false,
            settle: None,
            origin,
            output: at.output,
            catalog,
            settings,
            pending_copy: None,
            closed: false,
            shown: true,
        }
    }

    /// The content's size at native zoom: the laid-out one once there is one.
    fn logical_size(&self) -> (f32, f32) {
        self.measured.unwrap_or_else(|| self.content.size())
    }

    /// Records a completed click on this pin and says whether it completes a double click.
    ///
    /// The gesture lives here, not in the content, because it opens the editor before the
    /// menu is composed: this build's menu already offers Done.
    fn clicked_twice(&mut self, cx: &ui_core::cx::Cx<'_>) -> bool {
        if !cx.clicked_within(self.node_id()) {
            return false;
        }
        let now = cx.now();
        let twice = crate::pin::double_click(self.last_click, now);
        self.last_click = if twice { None } else { Some(now) };
        twice
    }

    /// How much larger than native the picture is drawn.
    fn zoom(&self) -> f32 {
        zoom::scale(self.zoom_steps)
    }

    /// The content's size on screen: one content pixel per device pixel at native zoom.
    fn display_size(&self) -> (f32, f32) {
        drawn_size(self.logical_size(), self.zoom())
    }

    /// The size this pin covers in device pixels: what a kind that renders for the screen
    /// rasterizes, copies and exports at.
    pub(crate) fn drawn_pixels(&self) -> (u32, u32) {
        let (width, height) = self.display_size();
        (
            (width * self.dots).round().max(1.0) as u32,
            (height * self.dots).round().max(1.0) as u32,
        )
    }

    /// Where the picture's corner is drawn.
    fn draw_at(&self) -> Point {
        drawn_at(self.position, self.dots, self.zoom())
    }

    fn node_id(&self) -> Id {
        Id::of(("pin/image", self.id))
    }

    /// The texture entry for this pin's picture, when it draws one.
    fn texture_entry(&mut self) -> Option<ImageEntry> {
        let drawn = self.display_size();
        self.content.image(self.texture, drawn)
    }

    fn ring_id(&self) -> Id {
        Id::of(("pin/ring", self.id))
    }

    /// How far this pin may zoom, in steps: what its own content can usefully be drawn at.
    fn zoom_limits(&self) -> (i32, i32) {
        zoom::range(self.logical_size())
    }

    /// The chrome this pin wears: the canvas's own, then the origin's, then the kind's.
    ///
    /// Badges are chrome for a visit: they arrive with the pointer and fade with it, so a pin
    /// nobody points at wears nothing. `reveal` picks among them inside that, and the zoom
    /// chip has a visit of its own on top.
    fn badges(&self, reveal: bool, shown: f32, zoom_opacity: f32) -> Vec<Badge> {
        let mut badges: Vec<Badge> = badge::zoom(self.zoom(), zoom_opacity).into_iter().collect();
        let catalog = self.catalog.catalog();
        badges.extend(badge::origin(&self.origin, &catalog));
        badges.extend(self.content.badges(&catalog));
        badges.retain(|badge| badge.show.visible(reveal));
        for badge in &mut badges {
            badge.opacity *= shown;
        }
        // A chip the pointer's departure has faded out is no chip.
        badges.retain(|badge| badge.opacity > 0.01);
        badges
    }

    /// The size to place `badge` by: what its pill measured last build, or an estimate for
    /// the first build it appears in.
    fn badge_size(&self, badge: &Badge) -> (f32, f32) {
        self.badge_sizes
            .iter()
            .find(|(id, _)| *id == badge.id)
            .map(|(_, size)| *size)
            .unwrap_or_else(|| badge::rough_size(&badge.label))
    }

    /// Remember what a badge's pill measured, so the next build places its stack exactly.
    fn note_badge_size(&mut self, id: &'static str, size: (f32, f32)) {
        match self.badge_sizes.iter_mut().find(|(key, _)| *key == id) {
            Some((_, measured)) => *measured = size,
            None => self.badge_sizes.push((id, size)),
        }
    }

    /// Write this pin's content where shots go; the path it landed at.
    fn save(&mut self, settings: &Settings) -> Result<std::path::PathBuf, String> {
        let exported = self.content.export(&ExportAsk {
            codecs: &settings.codecs,
            configured: settings.config.shots.format.as_deref(),
            path: None,
            format: None,
            drawn: self.drawn_pixels(),
        })?;
        let dir = paths::shot_dir(settings.config.shots.directory.as_deref());
        let path = dir.join(paths::shot_name(
            exported.extension,
            paths::now_secs(),
            self.output.as_deref(),
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
        std::fs::write(&path, exported.bytes)
            .map_err(|error| format!("write {}: {error}", path.display()))?;
        Ok(path)
    }

    /// Do what a right-click row asked: copy, save or close this pin.
    ///
    /// The bar belongs to the canvas, so an action that leaves nothing on screen reports
    /// through the `notice` it is handed; the reason a failure has stays on the terminal.
    fn menu_action(&mut self, item: Id, notice: &mut notice::Notice) -> bool {
        if item == Id::str("pin-menu/copy") {
            let catalog = self.catalog.catalog();
            // One payload per request today; a kind that offers several MIMEs is the
            // platform's selection step, not this one.
            match self.content.clipboard(self.drawn_pixels()) {
                Ok(payloads) => match payloads.into_iter().next() {
                    Some(payload) => self.pending_copy = Some(payload),
                    None => {
                        eprintln!("pin copy: nothing to offer");
                        notice.show(notice::failed(&catalog, notice::Action::Copy));
                    }
                },
                Err(error) => {
                    eprintln!("pin copy: {error}");
                    notice.show(notice::failed(&catalog, notice::Action::Copy));
                }
            }
            true
        } else if item == Id::str("pin-menu/save") {
            let settings = Arc::clone(&self.settings);
            match self.save(&settings) {
                Ok(path) => notice.show(notice::saved(&self.catalog.catalog(), &path)),
                Err(error) => {
                    eprintln!("pin save: {error}");
                    notice.show(notice::failed(
                        &self.catalog.catalog(),
                        notice::Action::Save,
                    ));
                }
            }
            true
        } else if item == Id::str("pin-menu/reset-zoom") {
            // Back to native size about the card's centre, so the detail the user was
            // looking at stays under their eye instead of jumping to the corner.
            let (width, height) = self.display_size();
            let centre = Point::new(
                self.position.x + width * 0.5,
                self.position.y + height * 0.5,
            );
            self.zoom_steps = 0;
            self.zoom_carry = 0.0;
            let (width, height) = self.display_size();
            self.position = Point::new(centre.x - width * 0.5, centre.y - height * 0.5);
            true
        } else if item == Id::str("pin-menu/close") {
            self.closed = true;
            true
        } else {
            let acted = self.content.act(item);
            if acted && let Some(payload) = self.content.take_copy() {
                self.pending_copy = Some(payload);
            }
            acted
        }
    }

    /// Where a release brings a pin back to, when too little of it is over the output.
    ///
    /// A drag has no limit — the card follows the hand — so the one place that can decide a
    /// pin is lost is the release. "Lost" is a pin with too little of itself over the
    /// output to grab again, not one hanging a few pixels past an edge.
    fn settle_target(&self) -> Option<Point> {
        settle_position(
            self.position,
            self.display_size(),
            (self.screen.0 as f32, self.screen.1 as f32),
        )
    }

    /// Advance a pull-back, or prime one for the next release.
    ///
    /// A drag writes the animation's zero every frame it runs, so the value has somewhere to
    /// travel from when the button comes up: an animation whose id is new snaps to its
    /// target rather than animating to it.
    fn advance_settle(&mut self, cx: &mut ui_core::cx::Cx<'_>, dragging: bool) {
        let id = self.node_id().with("settle");
        if dragging {
            self.settle = None;
            let _ = cx.animate(id, 0.0, SETTLE_BACK);
            return;
        }
        let Some((from, to)) = self.settle else {
            return;
        };
        let t = cx.animate(id, 1.0, SETTLE_BACK);
        self.position = Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        if t >= 1.0 {
            self.position = to;
            self.settle = None;
        }
    }
}

/// Where a released pin goes so a corner worth `KEEP_AREA` of it stays over the output.
///
/// The two axes are independent, and each is measured against the square's side: an axis
/// already showing that much is left where it is, so a pin that kept one long edge moves
/// only its short one. A pin shorter than the side on an axis is asked to stay wholly on
/// its output there, which is the only reach it has.
fn settle_position(position: Point, size: (f32, f32), screen: (f32, f32)) -> Option<Point> {
    let side = KEEP_AREA.sqrt();
    let seen = |start: f32, length: f32, edge: f32| {
        ((start + length).min(edge) - start.max(0.0)).clamp(0.0, length.min(edge))
    };
    let seen_x = seen(position.x, size.0, screen.0);
    let seen_y = seen(position.y, size.1, screen.1);
    let keep_x = size.0.min(side);
    let keep_y = size.1.min(side);
    if seen_x >= keep_x && seen_y >= keep_y {
        return None;
    }
    // Slightly more than the handhold, so a rounded position cannot land a hair short and be
    // pulled again on the next release; the check above still uses the handhold itself.
    let aim_x = (keep_x + 0.5).min(size.0);
    let aim_y = (keep_y + 0.5).min(size.1);
    let place = |start: f32, length: f32, edge: f32, aim: f32, shown: f32| {
        if shown >= aim {
            start
        } else if start < 0.0 {
            // Off the top or left: slide in until `aim` of the far edge shows.
            aim - length
        } else {
            // Off the bottom or right: slide in until `aim` of the near edge shows.
            edge - aim
        }
    };
    Some(Point::new(
        place(position.x, size.0, screen.0, aim_x, seen_x),
        place(position.y, size.1, screen.1, aim_y, seen_y),
    ))
}

/// Every pin on one output, in a single surface the size of that output.
///
/// The surface never moves, which is the point: a pin is a node inside it, so the pointer's
/// surface coordinates are screen coordinates and a drag is the absolute
/// `position at press + pointer - press_origin`. A surface that moved itself would make every
/// pointer sample stale — the compositor measures it against the surface as it was — and
/// re-reading that sample after the move feeds the pin's own motion into the next frame.
struct PinCanvas {
    /// The pins, in stacking order, shared with whoever owns the canvas.
    board: Rc<RefCell<PinBoard>>,
    language: Arc<Language>,
    /// The look every pin on this canvas is painted with.
    look: PinLook,
    /// The scope a widget inside a pin resolves in: the app's catalog and MD3's options.
    env: Arc<Env>,
    /// The pills a pin's badges are drawn with, resolved in `env` like any pin control.
    kit: Md3Kit,
    /// Where the pointer is this build, in canvas coordinates: a wheel zooms about it.
    pointer: Option<Point>,
    /// The pin a live press grabbed, and where it sat when the press landed.
    drag: Option<(u64, Point)>,
    /// The pin the last press or wheel touched; Escape falls back to it.
    focused: Option<u64>,
    /// The pin under the pointer this build: the keyboard is here because of it, so Escape
    /// closes it.
    hovered: Option<u64>,
    /// A pick consumed the press in progress: the pin under it must not start dragging once
    /// the wait ends.
    pick_press: bool,
    /// The secondary button is down: a pin under it shows its loupe while the button is held.
    secondary: bool,
    /// A cursor key's pending movement in fine steps, banked until the next build applies it.
    nudge: Option<(f32, f32)>,
    /// The bar of the annotation session in progress. One session runs at a time, so one bar
    /// serves them all, exactly as the selection's.
    toolbar: toolbar::Toolbar,
    /// The right-click menu's look, resolved in the canvas's own scope during `view`: the
    /// engine asks for it after the build and paints with what it gets.
    menu_style: Option<ui_widgets::MenuStyle>,
    /// The bar a pin's copy or save answers with; both leave nothing else on screen.
    notice: notice::Notice,
}

/// The pins one canvas carries, in stacking order.
///
/// It sits behind a shared handle because the pins outlive the surface's owner: the daemon
/// must be able to list and close them without owning the canvas, and the canvas needs the
/// same list to paint.
pub(crate) struct PinBoard {
    pub(crate) pins: Vec<Pin>,
    /// Shared by every canvas of one owner: a daemon addresses its pins by id, so two
    /// canvases must not both hand out 1.
    ids: Rc<Cell<u64>>,
    /// A palette waiting for a colour the screen must answer: the daemon takes it on its
    /// next turn, because a capture belongs to the loop that owns the connection.
    pub(crate) pick: Option<crate::pin::PickRequest>,
}

impl PinBoard {
    fn new(ids: Rc<Cell<u64>>) -> Self {
        Self {
            pins: Vec::new(),
            ids,
            pick: None,
        }
    }

    /// The wait a capture can answer, if a palette asked for one.
    pub(crate) fn take_pick(&mut self) -> Option<crate::pin::PickRequest> {
        self.pick.take()
    }

    pub(crate) fn add(&mut self, mut pin: Pin) {
        let id = self.ids.get() + 1;
        self.ids.set(id);
        pin.id = id;
        // One texture per pin, derived from its id: a canvas opened later can register just
        // the texture it needs without having seen the pin before.
        pin.texture = PIN_TEXTURE_BASE.wrapping_add(id);
        self.pins.push(pin);
    }

    /// Put a pin above the others and return where it now sits.
    fn raise(&mut self, index: usize) -> usize {
        if index + 1 == self.pins.len() {
            return index;
        }
        let pin = self.pins.remove(index);
        self.pins.push(pin);
        self.pins.len() - 1
    }
}

impl PinCanvas {
    fn new(language: Arc<Language>, look: PinLook) -> Self {
        Self::sharing(language, Rc::new(Cell::new(0)), look)
    }

    /// A canvas whose pin ids come from `ids`, for an owner that has several canvases.
    pub(crate) fn sharing(language: Arc<Language>, ids: Rc<Cell<u64>>, look: PinLook) -> Self {
        let env = crate::toolbar::env(&language);
        Self {
            board: Rc::new(RefCell::new(PinBoard::new(ids))),
            language,
            look,
            env,
            kit: Md3Kit::new(),
            pointer: None,
            drag: None,
            focused: None,
            hovered: None,
            pick_press: false,
            secondary: false,
            nudge: None,
            toolbar: toolbar::Toolbar::new(),
            menu_style: None,
            notice: notice::Notice::new(),
        }
    }

    fn add(&mut self, pin: Pin) {
        self.board.borrow_mut().add(pin);
    }

    /// The shared list; the caller borrows it for as long as it needs the pins.
    pub(crate) fn board(&self) -> &Rc<RefCell<PinBoard>> {
        &self.board
    }

    fn close_targeted(&mut self) {
        let Some(id) = self.hovered.or(self.focused) else {
            return;
        };
        if let Some(pin) = self
            .board
            .borrow_mut()
            .pins
            .iter_mut()
            .find(|pin| pin.id == id)
        {
            pin.closed = true;
        }
    }

    /// The pin a key speaks for: the open session's, else the one under the pointer or the
    /// last one touched.
    fn aim(&self) -> Option<u64> {
        let session = self
            .board
            .borrow()
            .pins
            .iter()
            .find(|pin| pin.content.editing())
            .map(|pin| pin.id);
        session.or(self.hovered).or(self.focused)
    }
}

impl App for PinCanvas {
    /// The menu look `view` resolved this frame; the engine copies it before painting.
    fn menu_style(&self) -> Option<ui_widgets::MenuStyle> {
        self.menu_style.clone()
    }

    fn shortcut(&mut self, id: ui_core::ShortcutId, held: ui_core::Modifiers) -> bool {
        if id == hotkeys::UNDO || id == hotkeys::REDO {
            let mut board = self.board.borrow_mut();
            let Some(pin) = board.pins.iter_mut().find(|pin| pin.content.editing()) else {
                return false;
            };
            return pin.content.edit_history(id == hotkeys::REDO);
        }
        if id == hotkeys::CONFIRM {
            // An unfinished Bezier path owns Enter first; Confirm closes the whole
            // editor only after its own session has finished.
            let mut board = self.board.borrow_mut();
            let Some(pin) = board.pins.iter_mut().find(|pin| pin.content.editing()) else {
                return false;
            };
            if pin.content.edit_finish_exclusive() {
                return true;
            }
            // The same key the confirm mark runs: whatever session is open keeps its marks.
            return pin.content.edit_commit();
        }
        if id == hotkeys::COPY || id == hotkeys::SAVE {
            // The pin menu's own rows carry these chords, so the keys must work shut as well.
            let Some(target) = self.aim() else {
                return false;
            };
            let mut board = self.board.borrow_mut();
            let Some(pin) = board.pins.iter_mut().find(|pin| pin.id == target) else {
                return false;
            };
            let row = if id == hotkeys::SAVE {
                "pin-menu/save"
            } else {
                "pin-menu/copy"
            };
            return pin.menu_action(Id::str(row), &mut self.notice);
        }
        if let Some((dx, dy)) = cursor_direction(id) {
            // The press fixes the size; the build that applies it only says what one step
            // measures on the picture under the pointer.
            let steps = cursor_steps(held.shift);
            let owed = self.nudge.unwrap_or((0.0, 0.0));
            self.nudge = Some((owed.0 + dx * steps, owed.1 + dy * steps));
            return true;
        }
        if id == hotkeys::CURSOR || id == hotkeys::CLEAR {
            // Both put the bar back a step, and both belong to the open session: with none
            // open there is no armed tool to disarm and no mark to put down.
            let mut board = self.board.borrow_mut();
            let Some(pin) = board.pins.iter_mut().find(|pin| pin.content.editing()) else {
                return false;
            };
            return if id == hotkeys::CLEAR {
                pin.content.edit_deselect()
            } else {
                pin.content.edit_cursor()
            };
        }
        if id == hotkeys::CANCEL {
            // A session owns Escape first, for its unfinished layers; with none left the
            // key leaves the session, so leaving is one press whatever the bar holds.
            if let Some(target) = self.hovered.or(self.focused)
                && self
                    .board
                    .borrow()
                    .pins
                    .iter()
                    .any(|pin| pin.id == target && pin.content.editing())
            {
                let mut board = self.board.borrow_mut();
                if let Some(pin) = board.pins.iter_mut().find(|pin| pin.id == target)
                    && !pin.content.edit_escape()
                {
                    pin.content.act(Id::str("pin-menu/done"));
                }
                return true;
            }
            self.close_targeted();
            return true;
        }
        false
    }

    fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        // The canvas owns the secondary button for its pins: the loupe belongs to the pin
        // under the pointer, and only the next build knows which one that is.
        if let ui_core::change::Change::Secondary { down } = change {
            self.secondary = down;
            return true;
        }
        // The session's bar is the canvas's own chrome: its grip owns a drag, and its marks
        // are commands to the session that is open.
        if self.toolbar.dragged(id, &change) {
            return true;
        }
        if let ui_core::change::Change::Activated = change
            && (id == toolbar::Action::Confirm.id() || id == toolbar::Action::Cancel.id())
        {
            let mut board = self.board.borrow_mut();
            let Some(pin) = board.pins.iter_mut().find(|pin| pin.content.editing()) else {
                return false;
            };
            return if id == toolbar::Action::Confirm.id() {
                pin.content.edit_commit()
            } else {
                pin.content.edit_discard()
            };
        }
        let mut board = self.board.borrow_mut();
        let board = &mut *board;
        let Some(index) = board.pins.iter().position(|pin| pin.node_id() == id) else {
            // A node a kind drew for itself: a swatch, a link, whatever it made interactive.
            return board
                .pins
                .iter_mut()
                .find(|pin| pin.content.owns(id))
                .is_some_and(|pin| pin.content.change(id, change));
        };
        match change {
            // A menu row names itself; the node it was reported with is the picture.
            ui_core::change::Change::MenuItem(item) => {
                board.pins[index].menu_action(item, &mut self.notice)
            }
            ui_core::change::Change::Value(value) => {
                // A wheel step is the engine's fixed number of pixels; a trackpad's deltas are
                // banked until they are worth a whole lattice step. Zooming about the pointer
                // keeps the detail under it put.
                let Some(anchor) = self.pointer else {
                    return false;
                };
                let pin = &mut board.pins[index];
                // While a session is open the wheel belongs to its loupe, not to the pin's
                // own zoom: the picture is being marked up, not placed.
                if pin.content.editing() {
                    return pin.content.wheel(value);
                }
                let owed = pin.zoom_carry + zoom::travel(value);
                let step = owed.trunc();
                pin.zoom_carry = owed - step;
                let (lowest, highest) = pin.zoom_limits();
                let next = ((pin.zoom_steps as f32 + step) as i32).clamp(lowest, highest);
                if next == pin.zoom_steps {
                    return false;
                }
                pin.position = zoomed(pin.position, pin.zoom_steps, next, anchor);
                pin.zoom_steps = next;
                pin.zoom_visited = true;
                self.focused = Some(pin.id);
                true
            }
            _ => false,
        }
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        // The engine asks for the menu's look after this build, so it is resolved here, in
        // the scope the pins themselves build under.
        {
            let mut wcx = ui_widgets::Cx::new(cx, Arc::clone(&self.env));
            self.menu_style = Some(self.kit.menu_style(&mut wcx));
        }
        // A cursor key is a request: the surface moves the real pointer after this build. The
        // presses already counted their steps; this measures what one step covers where the
        // pointer is, which is the picture it lands on.
        if let Some((dx, dy)) = self.nudge.take() {
            let aim = self.aim();
            let board = self.board.borrow();
            let picture = aim.and_then(|target| board.pins.iter().find(|pin| pin.id == target));
            // A pin pixel is `zoom / dots` points, so the step lands on the picture's grid:
            // at native zoom, that is the output's own grid.
            let (zoom, dots) = picture.map_or((1.0, 1.0), |pin| (pin.zoom(), pin.dots));
            let unit = cursor_step(CURSOR_STEP, zoom, dots);
            cx.nudge_pointer(dx * unit, dy * unit);
        }
        let mut board = self.board.borrow_mut();
        let board = &mut *board;
        board.pins.retain(|pin| !pin.closed);
        // Every pin lands on this surface's device grid, whatever scale it was created under.
        let dots = cx.scale().max(f32::EPSILON);
        for pin in &mut board.pins {
            pin.dots = dots;
        }
        // Nothing left to show: the surface existed for its pins, and the overlay layer it
        // covers would otherwise outlive them as an invisible, pass-through one.
        if board.pins.is_empty() {
            cx.close_window();
        }
        // A copy is a request: the engine drains it after this build and the host applies it.
        for pin in &mut board.pins {
            if let Some((mime, bytes)) = pin.pending_copy.take() {
                cx.copy_bytes(mime, bytes);
                self.notice
                    .show(notice::copied(&self.language.catalog()));
            }
        }
        // A session's bar belongs to the pin it annotates: while no session is open it is not
        // on screen at all, so it goes back under whatever the next one annotates.
        if !board.pins.iter().any(|pin| pin.content.editing()) {
            self.toolbar.reset();
        }
        // A press outside an open popover closes it, as every flyout does.
        if let Some(dismissed) = cx.dismissed()
            && let Some(pin) = board.pins.iter_mut().find(|pin| pin.content.editing())
        {
            pin.content
                .change(dismissed, ui_core::change::Change::Dismissed);
        }
        self.pointer = cx.pointer();
        self.hovered = cx.hovered().and_then(|id| {
            board
                .pins
                .iter()
                .find(|pin| pin.node_id() == id)
                .map(|pin| pin.id)
        });
        // A palette waiting for a colour takes the next press: the pin under the pointer
        // gives up its pixel and the press moves nothing. The claim lasts the whole press,
        // or the pin under it would start dragging the moment the wait ended.
        let press = cx.press_origin().is_some();
        if !press {
            self.pick_press = false;
        }
        if press && let Some(index) = board.pins.iter().position(|pin| pin.content.picking()) {
            let picker = board.pins[index].id;
            let mut over_pin = false;
            let colour = cx.pointer().and_then(|at| {
                board.pins.iter().rev().find_map(|pin| {
                    if !pin.shown || pin.id == picker {
                        return None;
                    }
                    let (x, y) = (pin.draw_at().x, pin.draw_at().y);
                    let (width, height) = pin.display_size();
                    let drawn = Rect::new(x, y, width, height);
                    if !drawn.contains(at) {
                        return None;
                    }
                    over_pin = true;
                    pin.content.colour_at(drawn, at)
                })
            });
            if over_pin || colour.is_some() {
                // A pin under the press answers, or ends the wait when it holds no pixels.
                board.pins[index].content.picked(colour);
            } else if let Some(at) = cx.pointer() {
                // Nothing of ours is under the press: the screen answers, and only the
                // daemon can capture it.
                board.pick = Some(crate::pin::PickRequest {
                    pin: picker,
                    output: board.pins[index].output.clone(),
                    screen: board.pins[index].screen,
                    at,
                });
            } else {
                board.pins[index].content.picked(None);
            }
            self.pick_press = true;
        }
        // A drag is absolute: the pin's position at the press plus the pointer's travel since
        // it. Nothing is accumulated and nothing depends on the surface moving, so a frame
        // with no new input cannot move the pin at all.
        //
        // The press lands on whichever node the pointer hit — a text run, a swatch — and the
        // engine names the drag its nearest `DRAG` ancestor, which is the card. Matching the
        // pressed node's id would only drag a picture, whose box is the topmost hit.
        let pressed = if self.pick_press {
            None
        } else {
            board
                .pins
                .iter()
                .position(|pin| {
                    // A session's content owns the presses inside the picture, so only its
                    // band still moves the pin.
                    if pin.content.editing() {
                        pin.content.edit_band_drag()
                    } else {
                        cx.dragging_within(pin.node_id())
                    }
                })
                // Every pin shares one surface, so stacking is the vector's order: the last
                // one built paints on top. A press is what promotes the pin under the pointer.
                .map(|index| board.raise(index))
        };
        if let (Some(index), Some(origin), Some(pointer)) =
            (pressed, cx.press_origin(), cx.pointer())
        {
            let pin = &mut board.pins[index];
            let from = match self.drag {
                Some((drag_id, from)) if drag_id == pin.id => from,
                _ => {
                    self.focused = Some(pin.id);
                    self.drag = Some((pin.id, pin.position));
                    pin.position
                }
            };
            // No limit while the drag runs: the pointer follows the hand, and a pin dragged
            // off the output comes back when it is let go (see `Pin::settle_target`).
            pin.position = Point::new(from.x + pointer.x - origin.x, from.y + pointer.y - origin.y);
        } else if pressed.is_none()
            && let Some((drag_id, _)) = self.drag.take()
            && let Some(pin) = board.pins.iter_mut().find(|pin| pin.id == drag_id)
        {
            // The release decides once; the move itself is the tween below.
            pin.settle = pin.settle_target().map(|to| (pin.position, to));
        }
        // A pull-back is a move like any other the user watches: it runs on the engine's
        // clock, and the dragged pin keeps its animation at zero so the next release has
        // somewhere to travel from.
        for (index, pin) in board.pins.iter_mut().enumerate() {
            pin.advance_settle(cx, pressed == Some(index));
        }

        let mut container = ui_platform::Container::new(Id::str("pin/canvas"), NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            // The canvas itself consumes nothing: a click between pins belongs to the
            // desktop, and the engine publishes an input region of exactly the nodes that
            // take input. It still hears the secondary button for the pin under it.
            .sense(Sense::PASS_THROUGH | Sense::SECONDARY);
        let catalog = self.language.catalog();
        let look = self.look;
        for pin in board.pins.iter_mut().filter(|pin| pin.shown) {
            // A laid-out pin knows its real height; the first frame's estimate only placed
            // it, so the centre it was placed by is kept by shifting for the difference.
            if let Some(rect) = cx.rect_of(pin.node_id()) {
                // Layout is unaware of the paint scale, so this rect is the content's own
                // size at native zoom whatever the wheel has done since.
                let size = (rect.size.w, rect.size.h);
                if !pin.settled {
                    let provisional = pin.content.size();
                    pin.position = Point::new(
                        pin.position.x + (provisional.0 - size.0) * 0.5,
                        pin.position.y + (provisional.1 - size.1) * 0.5,
                    );
                    pin.settled = true;
                }
                pin.measured = Some(size);
            }
            let pressed = cx.dragging_within(pin.node_id());
            // Anywhere on the pin, not only its card: a kind's own controls are part of it,
            // and hovering one must not make the pin look abandoned.
            let hovered = cx.hovered_within(pin.node_id());
            let active = pressed || hovered;
            // The pointer's visit is one transition, not a switch: every border, shadow and
            // badge reads this progress, so they move together.
            let visit = cx.animate(
                pin.node_id().with("skin"),
                if active { 1.0 } else { 0.0 },
                SKIN_SWAP,
            );
            let skin = crate::pin::card::blended(look.idle, look.active, visit);
            // The card is laid out at its own size and scaled by the wheel, so text is shaped
            // at the drawn size instead of being stretched with the box.
            let (width, height) = pin.logical_size();
            let zoom = pin.zoom();
            // A picture that renders for the screen — a vector — is made here, where the
            // drawn size is known; the engine takes it after the view returns and paints it
            // this frame, so a zoom never shows a raster from before the wheel.
            if let Some(entry) = pin.texture_entry() {
                cx.register_image(entry);
            }
            let at = pin.draw_at();
            // The loupe belongs to the pin the pointer is over: holding the button above one
            // pin must not raise a disc over its neighbour.
            let held = self.secondary
                && self.pointer.is_some_and(|pointer| {
                    let (w, h) = pin.display_size();
                    Rect::new(at.x, at.y, w, h).contains(pointer)
                });
            // Before the menu below is composed: the session's entry row changes with it.
            if pin.clicked_twice(cx) && !pin.content.editing() {
                pin.content.act(Id::str("pin-menu/edit"));
            }
            // The shared rows come first, then whatever the kind adds, then Close: the one
            // row a user hunts for stays at the end whatever the kind put above it.
            let row = |id: &'static str| {
                // The id and the message key agree by construction: `pin-menu/copy` reads
                // `pin-menu-copy`.
                let key = id.replace('/', "-");
                ui_core::menu::MenuItem::new(Id::str(id), catalog.text_by_id(&key).to_string())
            };
            // A row whose action has a chord prints it and answers it; a cleared action
            // shows nothing, and the row still runs from the mouse alone.
            let plan = &pin.settings.shortcuts;
            let chorded = |item: ui_core::menu::MenuItem, action: crabsnap_core::hotkeys::Shortcut| {
                match (plan.hint(action), plan.chord(action)) {
                    (Some(text), Some(chord)) => item.with_chord(chord, text),
                    _ => item,
                }
            };
            let mut menu = vec![
                chorded(row("pin-menu/copy"), crabsnap_core::hotkeys::Shortcut::Copy),
                chorded(row("pin-menu/save"), crabsnap_core::hotkeys::Shortcut::Save),
                row("pin-menu/reset-zoom"),
            ];
            menu.extend(pin.content.menu().into_iter().map(|id| match id {
                // Closing the session is the confirm mark's own action.
                "pin-menu/done" => chorded(row(id), crabsnap_core::hotkeys::Shortcut::Confirm),
                _ => row(id),
            }));
            menu.push(row("pin-menu/close"));
            let frame = PinFrame {
                id: pin.node_id(),
                at,
                size: (width, height),
                texture: pin.texture,
                look,
                skin,
                env: Arc::clone(&self.env),
                catalog: Arc::clone(&catalog),
                zoom,
                cursor: if pressed {
                    CursorShape::Grabbing
                } else {
                    CursorShape::Grab
                },
                menu,
                loupe: held,
            };
            let content = pin.content.view(cx, frame);
            // A box paints its border before its image, so a picture would hide a border on
            // its own node: the canvas draws the state's border as a sibling above it. The
            // shadow stays on the card, which is the node with a surface to cast it.
            let (border, border_width) = crate::pin::card::border(skin);
            let rounded = pin.content.rounded();
            let radius = if rounded { look.radius } else { 0.0 };
            // The border sits outside the card: the ring is the card grown by the border on
            // every side, and the stroke it paints inward fills exactly that band. Its corner
            // radius grows with it, so the two curves stay concentric. It takes the same
            // scale about its own top-left, which is the card's grown by the same ring, so
            // the border's width grows with the pin and the two edges stay together.
            let mut ring = ui_platform::Container::new(pin.ring_id(), NodeKind::Box)
                .sense(Sense::PASS_THROUGH)
                .scale(zoom)
                .size(SizeClass::fixed(
                    width + border_width * 2.0,
                    height + border_width * 2.0,
                ))
                // The stroke's outer edge is `border_width` past the card at the drawn size,
                // so the ring's own origin moves out by that much *scaled*: anchored there,
                // the band it paints lands exactly between the drawn card and the old edge.
                .offset(at.x - border_width * zoom, at.y - border_width * zoom)
                .map(|decl| {
                    decl.background(
                        Color::TRANSPARENT,
                        crate::pin::card::corners(radius + border_width, rounded),
                    )
                });
            if border_width > 0.0 {
                ring = ring.border(border, border_width);
            }
            container = container.child(content).child(Box::new(ring));
            for overlay in pin.content.take_overlay() {
                container = container.child(Box::new(overlay));
            }
            // The zoom chip is a visitor: the wheel calls it, and it leaves a moment after the
            // last step. Its own fade is the clock, so nothing else has to watch the time.
            let visiting = pin.zoom_visited;
            pin.zoom_visited = false;
            let zoom_opacity = badge::visited(cx, pin.node_id().with("zoom-visit"), visiting);
            // The pin's chrome comes last in its own slot: no other pin can paint between a
            // card and the badges that name it. Badges are drawn at a fixed size whatever the
            // zoom, so they are siblings of the card, not part of the scaled node.
            let badges = pin.badges(cx.modifiers().alt, visit, zoom_opacity);
            if !badges.is_empty() {
                let (drawn_w, drawn_h) = pin.display_size();
                let pin_rect = Rect::new(at.x, at.y, drawn_w, drawn_h);
                let output_rect = Rect::new(0.0, 0.0, pin.screen.0 as f32, pin.screen.1 as f32);
                let grouped = groups(&badges);
                let blocks: Vec<badge::Block> = grouped
                    .iter()
                    .map(|(anchor, policy, members)| {
                        let sizes = members
                            .iter()
                            .map(|index| pin.badge_size(&badges[*index]))
                            .collect();
                        (*anchor, *policy, sizes)
                    })
                    .collect();
                let rects = badge::layout(&blocks, pin_rect, output_rect, self.pointer);
                let wcx = ui_widgets::Cx::new(cx, Arc::clone(&self.env));
                let pack = self.kit.pack_for(&wcx);
                for ((_, _, members), rects) in grouped.into_iter().zip(rects) {
                    // `layout` drops the chips it cannot fit, and the members after them.
                    for (&index, rect) in members.iter().zip(rects) {
                        let chip = &badges[index];
                        let id = Id::of(("pin/badge", pin.id, chip.id));
                        let decl = badge::pill(id, chip, &pack).offset(rect.min_x(), rect.min_y());
                        container = container.child_decl(decl);
                        // Measured after the fact, as a pin's own size is: this frame's rect
                        // is the previous build's, and the one after will be exact.
                        if let Some(measured) = wcx.rect_of(id) {
                            pin.note_badge_size(chip.id, (measured.size.w, measured.size.h));
                        }
                    }
                }
            }
            // A session's bar hangs around the pin being annotated. The same module the
            // selection uses, with the session's own table: what it offers here is keep or
            // drop the marks, not capture the selection. Last in the pin's slot, so the
            // chrome above (a mark's box, its handles) cannot take a press meant for it.
            if pin.content.editing() {
                let (drawn_w, drawn_h) = pin.display_size();
                let area = Rect::new(at.x, at.y, drawn_w, drawn_h);
                let screen = Rect::new(0.0, 0.0, pin.screen.0 as f32, pin.screen.1 as f32);
                let bar = pin.content.edit_bar();
                let actions: Vec<toolbar::Tool> = toolbar::pin_edit()
                    .into_iter()
                    .map(|tool| tool.hinted(&pin.settings.shortcuts))
                    .collect();
                let entries = toolbar::entries(bar, &actions, &pin.settings.shortcuts);
                let params = pin.content.edit_params();
                container = container.child(toolbar::build(
                    cx,
                    &self.env,
                    &self.kit,
                    toolbar::View {
                        state: &mut self.toolbar,
                        screen,
                        area,
                        entries: &entries,
                        params: &params,
                        catalog: &catalog,
                        drawing: bar.drawing,
                    },
                ));
            }
        }
        // The bar is the surface's last child, out of the flow at its bottom: a pin's copy
        // or save leaves nothing else on screen, and this is where it says so.
        let surface_width = cx.width() as f32;
        let mut wcx = ui_widgets::Cx::new(cx, Arc::clone(&self.env));
        container = container.child(self.notice.build(&mut wcx, &self.kit, surface_width));
        Box::new(container)
    }
}

/// Badges gathered into the blocks they share: one anchor and one policy, in declaration
/// order.
///
/// The anchor and the policy are a block's geometry and its rule, so chips that disagree on
/// either cannot form one stack.
fn groups(badges: &[Badge]) -> Vec<(badge::Anchor, badge::Policy, Vec<usize>)> {
    let mut groups: Vec<(badge::Anchor, badge::Policy, Vec<usize>)> = Vec::new();
    for (index, badge) in badges.iter().enumerate() {
        let found = groups
            .iter_mut()
            .find(|(anchor, policy, _)| *anchor == badge.anchor && *policy == badge.policy);
        match found {
            Some((_, _, members)) => members.push(index),
            None => groups.push((badge.anchor, badge.policy, vec![index])),
        }
    }
    groups
}

/// Where pin textures start; each pin registers under its own id on its canvas.
const PIN_TEXTURE_BASE: u64 = 0x7069_6e00_0000_0000;

/// The smallest corner of a pin that stays over its output after a release, in square
/// logical points: a square 48 points a side.
///
/// A fixed handhold, not a share of the pin: a full-screen pin dragged nearly off keeps an
/// edge in reach instead of leaping back. Its side is the one reference for both axes, so a
/// released pin shows a square-sized corner: an axis already showing more than that side is
/// left alone and only the shorter one is brought in, which reads better than pulling a long
/// bar further across. Points are the unit the compositor scales, so the same area is the
/// same physical area at any output density.
const KEEP_AREA: f32 = 48.0 * 48.0;
/// How long a pin's skin takes to arrive when the pointer enters or leaves it.
const SKIN_SWAP: AnimSpec = AnimSpec::new(0.16, Easing::EaseOut);
/// How long a pin takes to come back onto its output after a release.
const SETTLE_BACK: AnimSpec = AnimSpec::new(0.18, Easing::EaseOut);
/// Zoom about `anchor`: the content point under it stays where it is on screen.
///
/// The count is clamped by the caller, so the ratio here is the one that actually takes
/// effect: at a limit the two differ, and moving by the request would push the content out
/// from under the pointer in the one step that stops.
fn zoomed(origin: Point, from: i32, to: i32, anchor: Point) -> Point {
    if to == from {
        return origin;
    }
    let ratio = zoom::ratio(from, to);
    Point::new(
        anchor.x - (anchor.x - origin.x) * ratio,
        anchor.y - (anchor.y - origin.y) * ratio,
    )
}

/// Where a shot goes and what it is encoded as.
fn shot_target(
    source: Option<&str>,
    settings: &Settings,
) -> Result<(std::path::PathBuf, codec::Encoding), String> {
    let encoding = codec::Encoding::resolve(None, None, settings.config.shots.format.as_deref())?;
    let dir = paths::shot_dir(settings.config.shots.directory.as_deref());
    let path = dir.join(paths::shot_name(
        encoding.extension(),
        paths::now_secs(),
        source,
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    Ok((path, encoding))
}

/// Write `image` where screenshots go and say where it landed.
pub(crate) fn save_shot(
    image: &capture::Cropped,
    source: Option<&str>,
    settings: &Settings,
) -> Result<String, String> {
    let path = save_shot_path(image, source, settings)?;
    Ok(format!(
        "{} {}x{}",
        path.display(),
        image.width,
        image.height
    ))
}

/// Write `image` where screenshots go; the path it landed at.
pub(crate) fn save_shot_path(
    image: &capture::Cropped,
    source: Option<&str>,
    settings: &Settings,
) -> Result<std::path::PathBuf, String> {
    let (path, encoding) = shot_target(source, settings)?;
    settings.codecs.write(image, &path, encoding)?;
    Ok(path)
}

impl App for Overlay {
    fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        match change {
            // The secondary button holds the loupe up; the release puts it away.
            ui_core::change::Change::Secondary { down } => {
                self.loupe.set_held(down);
                return true;
            }
            // The middle button moves the selection: a press inside it is otherwise the
            // annotation canvas's, so the button has no meaning of its own to take.
            ui_core::change::Change::Middle { down } => {
                self.middle = down;
                self.middle_offered = false;
                return true;
            }
            // The bar's grip is the only thing that drags the bar; the selection's own
            // drags are the root's.
            ui_core::change::Change::DragStarted
            | ui_core::change::Change::Moved { .. }
            | ui_core::change::Change::Dropped(_)
            | ui_core::change::Change::Cancelled => return self.toolbar.dragged(id, &change),
            _ => {}
        }
        if let ui_core::change::Change::Activated = change {
            // The magnifier here is the overlay's own: it looks through the frozen frame,
            // not through a pin's picture, so its button drives `self.loupe`.
            if id == toolbar::loupe() {
                return self.loupe.toggle();
            }
            if id == toolbar::Action::Save.id() {
                self.save();
                return true;
            }
            if id == toolbar::Action::Copy.id() {
                self.copy_image();
                return true;
            }
            if id == toolbar::Action::Pin.id() {
                self.pin();
                return true;
            }
            // Cancel is its chord's command, so the tooltip naming the chord holds: the
            // button and Escape do one thing.
            if id == toolbar::Action::Cancel.id() {
                // A click holds no modifiers; cancel reads none.
                return self.shortcut(hotkeys::CANCEL, ui_core::Modifiers::NONE);
            }
        }
        // The rest of the bar — a tool, a family, a parameter, a colour — belongs to the
        // session annotating the selection.
        let wheel = match &change {
            ui_core::change::Change::Value(value) => Some(*value),
            _ => None,
        };
        if self.annotate.owns(id, &self.document)
            && self.annotate.change(id, change, &mut self.document)
        {
            return true;
        }
        // A wheel step zooms the loupe, and only while it is up to be looked through. The
        // session here draws no disc of its own, so one it declines is the overlay's, wherever
        // the pointer is: over the bar, a mark, or the bare picture.
        if let Some(value) = wheel {
            return self.loupe.wheel(value, self.loupe.visible(self.gesture.is_some()));
        }
        false
    }

    fn shortcut(&mut self, id: ui_core::ShortcutId, held: ui_core::Modifiers) -> bool {
        if id == hotkeys::UNDO || id == hotkeys::REDO {
            return self.annotate.history(&mut self.document, id == hotkeys::REDO);
        }
        if id == hotkeys::CANCEL {
            // Escape only takes what is unfinished; the tool and the selection have chords
            // of their own, so the key that leaves the overlay never spends itself elsewhere.
            if self.annotate.escape(&mut self.document) {
                return true;
            }
            self.close = true;
            return true;
        }
        // The bar's marks and their chords run the same action: a tooltip that names a chord
        // promises the button and the key do one thing.
        let (width, height) = self.logical_size();
        if id == hotkeys::SAVE || id == hotkeys::COPY || id == hotkeys::PIN {
            if self.selection(width, height).is_none() {
                return false;
            }
            if id == hotkeys::SAVE {
                self.save();
            } else if id == hotkeys::COPY {
                self.copy_image();
            } else {
                self.pin();
            }
            return true;
        }
        if id == hotkeys::CURSOR {
            // The bar's own cursor mark, from the keyboard: nothing armed, nothing selected.
            return self.annotate.cursor(&mut self.document);
        }
        if id == hotkeys::CLEAR {
            // The session's layers first: a selected mark goes before the selection it sits
            // in, so choosing again is the second press of the same chord.
            if self.annotate.deselect() {
                return true;
            }
            // Choosing again: the selection goes and the bar returns to its corner. The
            // marks stay — they live on the frame, not on the rectangle.
            if self.settled.is_none() {
                return false;
            }
            self.settled = None;
            self.gesture = None;
            self.current = None;
            self.middle_drag = None;
            self.toolbar.reset();
            return true;
        }
        let Some((dx, dy)) = cursor_direction(id) else {
            return false;
        };
        // The press fixes the size; the build that applies it measures one step on this
        // surface's grid.
        let steps = cursor_steps(held.shift);
        let owed = self.nudge.unwrap_or((0.0, 0.0));
        self.nudge = Some((owed.0 + dx * steps, owed.1 + dy * steps));
        true
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        // The app's own close is the engine's to apply: the surface, not the app, ends.
        if self.close {
            cx.close_window();
        }
        let catalog = self.language.catalog();
        // The job's completion forced this build, so its result is here if it is ready.
        if let Some(result) = self.copy_job.as_ref().and_then(|job| job.take()) {
            self.copy_job = None;
            match result {
                Ok(png) => self.pending_copy = Some(png),
                Err(error) => {
                    eprintln!("copy: {error}");
                    self.notice
                        .show(notice::failed(&catalog, notice::Action::Copy));
                }
            }
        }
        if let Some(result) = self.save_job.as_ref().and_then(|job| job.take()) {
            self.save_job = None;
            match result {
                Ok(path) => self.notice.show(notice::saved(&catalog, &path)),
                Err(error) => {
                    eprintln!("save: {error}");
                    self.notice
                        .show(notice::failed(&catalog, notice::Action::Save));
                }
            }
        }
        // A copy is a request: the engine drains it after this build and the host applies it.
        if let Some(png) = self.pending_copy.take() {
            cx.copy_bytes("image/png", png);
            self.notice.show(notice::copied(&catalog));
        }
        // A cursor key is a request too: the surface moves the real pointer after this build,
        // and the next frame — which follows, so the chrome keeps up — sees it there. The
        // overlay draws the capture one pixel per device pixel, so a canvas pixel is
        // `1 / scale` points: at any scale the pointer lands on the capture's own grid.
        if let Some((dx, dy)) = self.nudge.take() {
            let unit = cursor_step(CURSOR_STEP, 1.0, cx.scale());
            cx.nudge_pointer(dx * unit, dy * unit);
        }
        let root = Id::str("overlay/root");
        // A press outside an open popover closes it, as every flyout does.
        if let Some(dismissed) = cx.dismissed()
            && self.annotate.change(
                dismissed,
                ui_core::change::Change::Dismissed,
                &mut self.document,
            )
        {
            cx.invalidate_decls();
        }
        // The picture's own node covers the whole frozen frame, so a press on the picture is
        // that node's, not the root's; the bar and a mark's chrome keep the presses they take.
        let content = edit::content_id(root);
        let pressed = matches!(cx.pressed(), Some(id) if id == root || id == content);
        let width = cx.width() as f32;
        let height = cx.height() as f32;
        // Every selection below is in these points; the pixels are one conversion away.
        self.surface_scale = cx.scale();
        let bounds = Size::new(width, height);
        // A tool changes what a press means: it draws instead of selecting, so the selection
        // stays where it is while the picture is marked up.
        let armed = self.annotate.armed();
        // The eyedropper spends the press on the frozen picture's own pixel, so neither the
        // selection nor a tool may read that press.
        let press = cx.press_origin().is_some();
        if !press {
            self.pick_press = false;
        }
        if press && self.annotate.picking() {
            let colour = cx.pointer().and_then(|at| {
                let scale = self.surface_scale;
                let (x, y) = (at.x * scale, at.y * scale);
                if x < 0.0 || y < 0.0 {
                    return None;
                }
                let whole = Rect::new(0.0, 0.0, self.image.width as f32, self.image.height as f32);
                self.image.crop(whole).texel(x as usize, y as usize)
            });
            if self.annotate.picked(&mut self.document, colour) {
                cx.invalidate_decls();
            }
            self.pick_press = true;
        }
        let claimed = self.pick_press || self.annotate.picking();
        // The middle button drags the settled rectangle wherever it went down, even over the
        // marks: only the rectangle's own frame and grips are the primary button's.
        if self.middle && !self.middle_offered {
            self.middle_offered = true;
            if self.gesture.is_none()
                && let Some(at) = cx.local()
                && let Some(origin) = self.settled.filter(|rect| rect.contains(at))
            {
                self.middle_drag = Some((at, origin));
            }
        }
        match (self.middle_drag, self.middle) {
            (Some(_), false) => {
                self.middle_drag = None;
                self.middle_offered = false;
            }
            (Some((grab, origin)), true) => {
                if let Some(local) = cx.local() {
                    let delta = Point::new(local.x - grab.x, local.y - grab.y);
                    self.settled = Some(slid(origin, delta, bounds));
                }
            }
            (None, _) => {}
        }
        // While the middle button drags, the primary button's press is not read: a gesture
        // that began under it would take the rectangle away from the button holding it.
        let pressed = pressed && self.middle_drag.is_none();
        if pressed && !self.pressed_before {
            if let (false, Some(at)) = (armed || claimed, cx.local()) {
                // A grip resizes, the inside of the rectangle moves it, and anywhere
                // else starts a new selection over the old one.
                self.gesture = match self
                    .settled
                    .and_then(|rect| Handle::grabbed(rect, at).map(|handle| (handle, rect)))
                {
                    Some((handle, origin)) => Some(Gesture::Resize { handle, origin }),
                    None => match self.settled.filter(|rect| rect.contains(at)) {
                        Some(origin) => Some(Gesture::Move { grab: at, origin }),
                        None => {
                            self.settled = None;
                            Some(Gesture::Select { start: at })
                        }
                    },
                };
                self.current = Some(at);
            }
        } else if pressed {
            if let (false, Some(local)) = (armed || claimed, cx.local()) {
                self.current = Some(local);
                // A moved rectangle follows the pointer rather than jumping on release.
                if let Some(Gesture::Move { grab, origin }) = self.gesture {
                    let delta = Point::new(local.x - grab.x, local.y - grab.y);
                    self.settled = Some(slid(origin, delta, bounds));
                }
            }
        } else if self.pressed_before {
            // Release ends the drag: the rectangle stays until the next press.
            if let (Some(gesture), Some(end)) = (self.gesture.take(), self.current.take()) {
                self.settled = Some(match gesture {
                    Gesture::Select { start } => {
                        // A new rectangle is a new export region: the bar goes back under it.
                        // The marks stay where they were drawn; the rectangle only says what
                        // leaves with them.
                        self.toolbar.reset();
                        between(start, end)
                    }
                    Gesture::Resize { handle, origin } => handle.resized(origin, end, bounds),
                    // It already moved with the pointer; the release only ends the gesture.
                    Gesture::Move { origin, .. } => self.settled.unwrap_or(origin),
                });
            }
        }
        self.pressed_before = pressed;

        // The frozen frame's own grid is the session's: a mark lands on the pixel it was
        // drawn over, wherever on the screen that pixel is. The crop box stays at hand — it
        // is the selection's content, in whole picture pixels, and what an export takes.
        let scale = self.surface_scale;
        let crop = self
            .selection(width, height)
            .map(|rect| self.image.crop_of(rect, scale));
        let (picture_width, picture_height) = self.logical_size();
        let space = Space::new(
            Rect::new(0.0, 0.0, picture_width, picture_height),
            (self.image.width, self.image.height),
        );
        // The session runs before anything is declared, so this build paints the drag in
        // progress rather than the frame before it.
        // What the session changed is app state the declaration cache cannot see: the entries
        // it kept from the frame before still describe the document the session replaced.
        if !claimed
            && (armed || !self.document.is_empty())
            && self.annotate.pointer(cx, &mut self.document, root, space)
        {
            cx.invalidate_decls();
        }
        // Text marks take their frame from their shaped text, so they are measured against
        // this build's measurer before anything reads a box off them.
        if self.document.measure(cx) {
            cx.invalidate_decls();
        }
        // The marks are a layer of their own over the frame, holding nothing of the picture:
        // the frame is painted once, so a selection's inside and outside are the same
        // picture. A frame with no marks declares no canvas at all.
        let picture = (self.image.width, self.image.height);
        if !self.document.is_empty() {
            cx.canvas(Canvas {
                id: CanvasId(MARKS_TEXTURE),
                size: picture,
                version: self.document.revision(),
                list: pin::content::marks(&self.document, picture),
            });
            cx.register_image(ImageEntry {
                id: MARKS_TEXTURE,
                width: picture.0,
                height: picture.1,
                source: ImageSource::Canvas(CanvasId(MARKS_TEXTURE)),
                // Marks are ink rather than a picture: SDR whatever the frame beneath them.
                colour: ContentColour::SDR,
                sampling: Sampling::Smooth,
            });
        }

        let dim = Color::from_rgba8(0, 0, 0, 110);
        let mut chrome = ui_core::Decl::container(Id::str("overlay/chrome"), NodeKind::Box)
            .sense(Sense::PASS_THROUGH)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            });
        let mut shapes = Vec::new();
        match self.selection(width, height) {
            Some(rect) => {
                // Four bands rather than one hole: the paint list has no clip subtraction.
                shapes.push(band(Rect::new(0.0, 0.0, width, rect.min_y()), dim));
                shapes.push(band(
                    Rect::new(0.0, rect.max_y(), width, (height - rect.max_y()).max(0.0)),
                    dim,
                ));
                shapes.push(band(
                    Rect::new(0.0, rect.min_y(), rect.min_x(), rect.size.h),
                    dim,
                ));
                shapes.push(band(
                    Rect::new(
                        rect.max_x(),
                        rect.min_y(),
                        (width - rect.max_x()).max(0.0),
                        rect.size.h,
                    ),
                    dim,
                ));
                shapes.push(Shape::Rect {
                    rect,
                    corners: Corners::ZERO,
                    fill: Color::from_rgba8(0, 0, 0, 0),
                    gradient: None,
                    border: Some((Color::from_rgba8(255, 255, 255, 230), 1.0)),
                });
                // A locked selection has nothing to drag: the handles would only promise a
                // resize that a press no longer performs.
                if !matches!(self.gesture, Some(Gesture::Select { .. })) && !armed && !claimed {
                    for handle in Handle::ALL {
                        let at = handle.at(rect);
                        shapes.push(Shape::Rect {
                            rect: Rect::new(
                                at.x - HANDLE * 0.5,
                                at.y - HANDLE * 0.5,
                                HANDLE,
                                HANDLE,
                            ),
                            corners: Corners::all(2.0),
                            fill: Color::from_rgba8(255, 255, 255, 255),
                            gradient: None,
                            border: Some((Color::from_rgba8(20, 20, 20, 200), 1.0)),
                        });
                    }
                }
            }
            None => shapes.push(band(Rect::new(0.0, 0.0, width, height), dim)),
        }
        chrome.shapes = shapes;
        let mut container = ui_platform::Container::new(root, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .background_image(self.texture, ImageFit::Fill)
            // The wheel zooms the loupe and the secondary button holds it up; the drag is
            // the selection's own.
            .sense(Sense::DRAG | Sense::SCROLL | Sense::SECONDARY | Sense::MIDDLE)
            .cursor(self.cursor_at(cx));
        // The picture's own node, which the session's drags land on: the whole frozen frame
        // is the canvas, so a mark may be drawn anywhere on it. It wears the surface's
        // cursor, or it would hide the Move and grip cursors the rectangle promises.
        container = container.child(Box::new(
            pin::content::content_area(root, None).cursor(self.cursor_at(cx)),
        ));
        if !self.document.is_empty() {
            container = container.child_decl(
                ui_core::Decl::container(Id::str("overlay/marks"), NodeKind::Box)
                    .sense(Sense::PASS_THROUGH)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fill(0.0, 0.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::Yes,
                    })
                    .background_image(MARKS_TEXTURE, ImageFit::Fill),
            );
        }
        container = container.child_decl(chrome);
        // The session's own controls — a mark's box, its handles, the typing field — sit over
        // the marks they belong to, and under the disc and the bar: a press where either of
        // those is belongs to it, whatever mark lies beneath.
        let whole = Rect::new(0.0, 0.0, self.image.width as f32, self.image.height as f32);
        let picture = self.image.crop(whole);
        container = container.child(Box::new(self.annotate.chrome(
            cx,
            edit::Chrome {
                node: root,
                space,
                document: &self.document,
                image: &picture,
                catalog: &catalog,
                // The overlay draws the selection's frame itself, above.
                frame: false,
                // Its own magnifier already looks through the frozen frame.
                loupe: false,
                held: self.loupe.held(),
            },
        )));
        let surface = Rect::new(0.0, 0.0, width, height);
        let selection = self.selection(width, height);
        let dots = self.surface_scale;
        // The disc sits where a pin's own disc sits: over the chrome, under the bar, so the
        // bar the pointer is working with never hides behind it.
        let dragging = self.gesture.is_some();
        if let Some(pointer) = cx.pointer().filter(|_| self.loupe.visible(dragging)) {
            let aim = loupe::Aim::at(Point::new(pointer.x * dots, pointer.y * dots));
            // The guide hugs the pixels the crop actually took: a line drawn from the raw
            // selection would sit between the magnified blocks at a fraction of a pixel.
            let guide = crop.map(|(x0, y0, x1, y1)| {
                Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
            });
            // What the marks painter draws is what the disc shows: the window raster takes
            // the same list, so the disc never parts company with the picture.
            let window = loupe::window(cx.scale(), aim, self.loupe.zoom());
            let whole = Rect::new(
                0.0,
                0.0,
                self.image.width as f32,
                self.image.height as f32,
            );
            let picture = self.image.crop(whole);
            let marked = pin::content::window(&picture, &self.document, window);
            let source = loupe::Source::of_window(
                &marked,
                (window.origin.x as i32, window.origin.y as i32),
            );
            if let Some(node) = loupe::build(
                cx,
                loupe::View {
                    state: &self.loupe,
                    source: &source,
                    aim,
                    guide,
                    pointer,
                    surface,
                    origin: Point::ZERO,
                    id: Id::str("overlay/loupe"),
                },
            ) {
                container = container.child_decl(node);
            }
        }
        if let Some(rect) = selection.filter(|_| !matches!(self.gesture, Some(Gesture::Select { .. })))
        {
            // The bar here is the annotation session's: its tools, its parameters, and the
            // overlay's own magnifier and capture actions.
            let mut bar = self.annotate.bar_state();
            bar.loupe = if self.loupe.toggled() {
                toolbar::LoupeState::On
            } else if self.loupe.held() {
                toolbar::LoupeState::Temporary
            } else {
                toolbar::LoupeState::Off
            };
            let actions: Vec<toolbar::Tool> = toolbar::selection()
                .into_iter()
                .map(|tool| tool.hinted(&self.settings.shortcuts))
                .collect();
            let entries = toolbar::entries(bar, &actions, &self.settings.shortcuts);
            let params = self.annotate.params();
            container = container.child(toolbar::build(
                cx,
                &self.env,
                &self.kit,
                toolbar::View {
                    state: &mut self.toolbar,
                    screen: surface,
                    area: rect,
                    entries: &entries,
                    params: &params,
                    catalog: &catalog,
                    drawing: self.annotate.drawing(),
                },
            ));
        }
        if let Some(rect) = selection
            && let Some(readout) = readout::build(
                cx,
                Id::str("overlay/readout"),
                rect,
                dots,
                surface,
                cx.rect_of(toolbar::id()),
            )
        {
            container = container.child_decl(readout);
        }
        // The bar is the surface's last child, out of the flow at its bottom: a copy or a
        // save leaves nothing else on screen, and this is where it says so.
        let mut wcx = ui_widgets::Cx::new(cx, Arc::clone(&self.env));
        container = container.child(self.notice.build(&mut wcx, &self.kit, width));
        Box::new(container)
    }
}

fn band(rect: Rect, colour: Color) -> Shape {
    Shape::Rect {
        rect,
        corners: Corners::ZERO,
        fill: colour,
        gradient: None,
        border: None,
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The language must be known before clap renders `--help`. A file that does not load
    // costs only the configured language here; parsing reports it properly afterwards.
    let config = Config::load().ok();
    let choice = language_choice(cli::prescan_lang(&args), config.as_ref())?;
    let language = load_language(choice)?;
    if args.is_empty() {
        return toolbar(&language);
    }
    let catalog = language.catalog();
    let Some(invocation) = cli::parse(&args, &catalog)? else {
        return Ok(());
    };
    run::run(&language, invocation)
}

/// The app's own chrome: a layer-surface toolbar, until commands take over.
fn toolbar(language: &Arc<Language>) -> Result<(), String> {
    let start = Instant::now();
    let deadline = std::env::var("CRABSNAP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map(|seconds| start + Duration::from_secs_f32(seconds));
    let backend = WaylandBackend::connect().map_err(|error| format!("wayland: {error}"))?;
    let mut host = Host::new(Box::new(backend));
    host.open(
        SurfaceSpec::layer(
            "dev.crabsnap",
            "crabsnap-toolbar",
            LayerSpec {
                level: Layer::Overlay,
                anchor: Anchor {
                    top: true,
                    bottom: false,
                    left: true,
                    right: false,
                },
                size: Some((480, 56)),
                exclusive: ExclusiveZone::Ignore,
                margin: Insets::ZERO,
                keyboard: KeyboardMode::None,
            },
        ),
        Toolbar {
            language: Arc::clone(language),
        },
        OpenOptions::default(),
    )?;
    let stats = host.run_until(deadline)?;
    println!("crabsnap: {} frames in {} turns", stats.frames, stats.turns);
    Ok(())
}

/// The language the run uses: `--lang`, then `[ui] language`, then the system list.
///
/// A bad `--lang` is refused, because it was typed on purpose. A configured value that
/// does not parse costs the setting and not the tool: it is reported and read as `auto`,
/// so a typo never keeps the daemon from starting.
pub(crate) fn language_choice(
    flag: Option<String>,
    config: Option<&Config>,
) -> Result<LanguageChoice, String> {
    if let Some(flag) = flag {
        return LanguageChoice::parse(&flag).map_err(|error| format!("--lang: {error}"));
    }
    let Some(configured) = config.and_then(|config| config.ui.language.as_deref()) else {
        return Ok(LanguageChoice::Auto);
    };
    match LanguageChoice::parse(configured) {
        Ok(choice) => Ok(choice),
        Err(error) => {
            eprintln!("ui.language: {error}; following the system");
            Ok(LanguageChoice::Auto)
        }
    }
}

/// The app's language: one catalog for the app and the workspace's widgets.
pub(crate) fn load_language(choice: LanguageChoice) -> Result<Arc<Language>, String> {
    let system = ui_platform::posix_system_languages();
    // A pin's Markdown card asks for the same `Msg` the gallery does, so the app's own
    // messages are merged into the widget catalog's locale rather than kept apart.
    let mut sources: Vec<(String, String)> = ui_i18n::LOCALES
        .iter()
        .map(|files| (files.tag.to_string(), files.source.to_string()))
        .collect();
    for (tag, source) in [("en", EN), ("zh-CN", ZH_CN)] {
        match sources.iter_mut().find(|(known, _)| known == tag) {
            Some((_, merged)) => {
                merged.push('\n');
                merged.push_str(source);
            }
            None => sources.push((tag.to_string(), source.to_string())),
        }
    }
    let sources: Vec<(&str, &str)> = sources
        .iter()
        .map(|(tag, source)| (tag.as_str(), source.as_str()))
        .collect();
    Language::from_sources(choice, &system, &sources)
        .map_err(|error| format!("language: {error:?}"))
}

/// What `crabsnap info status` can say today.
pub(crate) fn status() -> Result<(), String> {
    let link = WaylandLink::connect().map_err(|e| format!("connect: {e}"))?;
    let capture = match capture::Capture::new(&link, None, 0)? {
        Some(_) => "ready",
        None => "none",
    };
    println!("capture: {capture}");
    // No daemon means nothing is bound: the plan is what a daemon would bind.
    let plan = hotkeys::Plan::resolve(&Config::load()?);
    for action in crabsnap_core::hotkeys::Shortcut::ALL {
        println!(
            "hotkey: {} = {} (not bound: no daemon)",
            action.name(),
            plan.describe(action)
        );
    }
    let outputs = link.outputs();
    if outputs.is_empty() {
        println!("outputs: none reported");
    }
    for output in outputs {
        println!(
            "output: {} {}x{} at ({}, {}) scale {} refresh {} mHz{}",
            output.label(),
            output.width,
            output.height,
            output.x,
            output.y,
            output.scale,
            output.refresh_mhz,
            if output.rotated { " rotated" } else { "" }
        );
    }
    Ok(())
}

/// Capture one frame without opening any surface: the tool must not photograph itself.
/// Grab one output and deliver it: a PNG file, or a pin on the screen.
fn shoot_output(
    common: &cli::ShotCommon,
    args: &cli::OutputArgs,
    language: &Arc<Language>,
) -> Result<(), String> {
    wait_delay(common.delay)?;
    let settings = Settings::load(Arc::clone(language))?;
    let backend = WaylandBackend::connect().map_err(|e| format!("wayland: {e}"))?;
    let link = backend.link().clone();
    let rung = args
        .backend
        .clone()
        .or_else(|| settings.config.capture.backend.clone());
    let mut grabber =
        capture::Capture::new(&link, rung.as_deref(), args.output.unwrap_or(0) as usize)?
            .ok_or_else(|| "this compositor offers no screen capture protocol".to_string())?;
    let frame = grabber.grab(Duration::from_secs(2))?;
    let source = frame.output.clone();
    let format = frame.format;
    let stride = frame.stride;
    let image = frame.into_image();
    // The output's own coordinates: the engine works in logical points, the frame arrives in
    // device pixels.
    let dots = image.scale;
    let screen = (
        (image.width as f32 / dots).round() as u32,
        (image.height as f32 / dots).round() as u32,
    );
    let full = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
    let cropped = image.crop(full);
    if args.clipboard {
        copy_to_clipboard(&cropped)?;
    }
    // A target the command was given; with none, the file is the one every tool assumes.
    let write_file = common.file.is_some() || (!common.pin && !args.clipboard);
    if common.pin {
        return show_pins(
            backend,
            vec![Staged {
                content: Box::new(Bitmap::new(cropped, dots)),
                origin: PinOrigin::Capture {
                    output: source.clone(),
                },
                position: Point::new(0.0, 0.0),
            }],
            screen,
            dots,
            source,
            language,
            Arc::clone(&settings),
        );
    }
    let encoding = codec::Encoding::resolve(
        common.format.as_deref(),
        common.file.as_deref(),
        settings.config.shots.format.as_deref(),
    )?;
    if !write_file {
        return Ok(());
    }
    let path =
        match &common.file {
            Some(path) => path.clone(),
            None => paths::shot_dir(settings.config.shots.directory.as_deref()).join(
                paths::shot_name(encoding.extension(), paths::now_secs(), source.as_deref()),
            ),
        };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
    }
    settings.codecs.write(&cropped, &path, encoding)?;
    println!(
        "{} {}x{} {format:?} stride {stride}",
        path.display(),
        cropped.width,
        cropped.height
    );
    Ok(())
}

/// Hand the frame to the daemon, which keeps serving the selection after this exits.
fn copy_to_clipboard(image: &capture::Cropped) -> Result<(), String> {
    let png = image.png()?;
    let path = std::env::temp_dir().join(format!("crabsnap-copy-{}.png", std::process::id()));
    std::fs::write(&path, &png).map_err(|error| format!("write {}: {error}", path.display()))?;
    let answer = run::copy_image(&path.to_string_lossy());
    // The daemon has read the file by the time it answers; a failed cleanup is not the user's.
    let _ = std::fs::remove_file(&path);
    answer
}

/// A content, its provenance and the position it takes on its canvas: one item of a
/// `pin add` list.
pub(crate) struct Staged {
    pub(crate) content: Box<dyn PinContent>,
    pub(crate) origin: PinOrigin,
    pub(crate) position: Point,
}

/// Put a batch of contents on one canvas and run until the pins close.
pub(crate) fn show_pins(
    backend: WaylandBackend,
    staged: Vec<Staged>,
    screen: (u32, u32),
    dots: f32,
    source: Option<String>,
    language: &Arc<Language>,
    settings: Arc<Settings>,
) -> Result<(), String> {
    let mut canvas = PinCanvas::new(Arc::clone(language), settings.look);
    for item in staged {
        let at = Placement {
            screen,
            dots,
            position: item.position,
            output: source.clone(),
        };
        canvas.add(Pin::new(
            item.content,
            at,
            item.origin,
            Arc::clone(language),
            Arc::clone(&settings),
        ));
    }
    // The board numbers the pins and their textures together; an entry composed here would
    // name a texture the canvas never draws.
    let entries: Vec<ImageEntry> = {
        let mut board = canvas.board().borrow_mut();
        board
            .pins
            .iter_mut()
            .filter_map(Pin::texture_entry)
            .collect()
    };
    let mut host = Host::new(Box::new(backend));
    host.open(
        pins::canvas_surface(screen),
        canvas,
        OpenOptions {
            uncapped: false,
            images: entries,
            shortcuts: settings.shortcuts.chords(crabsnap_core::hotkeys::Place::Pin),
        },
    )?;
    let start = Instant::now();
    let deadline = std::env::var("CRABSNAP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map(|seconds| start + Duration::from_secs_f32(seconds));
    host.run_until(deadline)?;
    Ok(())
}

/// The layer the capture overlay lives on: every edge, its own keyboard, no room reserved.
pub(crate) fn overlay_surface() -> SurfaceSpec {
    SurfaceSpec::layer(
        "dev.crabsnap",
        "crabsnap-overlay",
        LayerSpec {
            level: Layer::Overlay,
            anchor: Anchor::ALL,
            size: None,
            exclusive: ExclusiveZone::Ignore,
            margin: Insets::ZERO,
            keyboard: KeyboardMode::Exclusive,
        },
    )
}

/// Wait out `--delay`, giving the user time to arrange what is about to be captured.
fn wait_delay(delay: Option<u64>) -> Result<(), String> {
    let Some(seconds) = delay.filter(|seconds| *seconds > 0) else {
        return Ok(());
    };
    std::thread::sleep(Duration::from_secs(seconds));
    Ok(())
}

/// Capture, then show the frozen screen as one texture and take a selection over it.
///
/// The capture happens before the surface exists, so the overlay can never contain itself.
fn shoot_region(
    common: &cli::ShotCommon,
    demo: bool,
    language: &Arc<Language>,
) -> Result<(), String> {
    wait_delay(common.delay)?;
    let settings = Settings::load(Arc::clone(language))?;
    let backend = WaylandBackend::connect().map_err(|e| format!("wayland: {e}"))?;
    // The capture runs before the surface exists, on the connection the loop will own.
    let link = backend.link().clone();
    let mut grabber = capture::Capture::new(&link, settings.config.capture.backend.as_deref(), 0)?
        .ok_or_else(|| "this compositor offers no screen capture protocol".to_string())?;
    let frame = grabber.grab(Duration::from_secs(2))?;
    // The screen's name outlives the frame: it is what a saved file says the shot came from.
    let source = frame.output.clone();
    let image = frame.into_image();
    let (width, height) = (image.width, image.height);
    let pixels = std::sync::Arc::clone(&image.data);
    let colour = image.colour;

    // The selection lives in the overlay's logical points, not the frame's pixels.
    let scale = image.scale;
    let (logical_width, logical_height) = (width as f32 / scale, height as f32 / scale);
    let demo_rect = Rect::new(
        logical_width * 0.3,
        logical_height * 0.2,
        logical_width * 0.4,
        logical_height * 0.4,
    );
    // This process owns the canvases it is about to be asked for, so it holds their inbox.
    let outbox = Rc::new(RefCell::new(Vec::new()));
    let look = settings.look;
    // Read before the settings object moves into the overlay: a key pressed before the first
    // frame must already mean something.
    let overlay_chords = settings
        .shortcuts
        .chords(crabsnap_core::hotkeys::Place::Overlay);
    let pin_chords = settings.shortcuts.chords(crabsnap_core::hotkeys::Place::Pin);
    let mut host = Host::new(Box::new(backend));
    let mut overlay = Overlay::new(
        image,
        source,
        settings,
        Arc::clone(language),
        Rc::clone(&outbox),
        host.jobs(),
    );
    overlay.file = common.file.clone();
    overlay.settled = demo.then_some(demo_rect);

    host.open(
        overlay_surface(),
        overlay,
        OpenOptions {
            uncapped: false,
            images: vec![ImageEntry {
                id: FROZEN_TEXTURE,
                width,
                height,
                source: ui_core::draw::ImageSource::Pixels(pixels),
                colour,
                sampling: ui_core::draw::Sampling::Smooth,
            }],
            shortcuts: overlay_chords,
        },
    )?;
    println!("overlay {width}x{height}");

    // A pin becomes a canvas a turn after the overlay asked for it, so the turn handler is
    // what carries it over.
    let owned = Rc::new(RefCell::new(pins::Pins::new(look, pin_chords)));
    {
        let outbox = Rc::clone(&outbox);
        let owned = Rc::clone(&owned);
        host.on_turn(move |cx, _| {
            pins::place_pending(&mut owned.borrow_mut(), &outbox, cx);
        });
    }
    let start = Instant::now();
    let deadline = std::env::var("CRABSNAP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map(|seconds| start + Duration::from_secs_f32(seconds));
    host.run_until(deadline)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag wins over the file; a typo in the file costs the setting, not the tool.
    #[test]
    fn the_language_comes_from_the_flag_then_the_file_then_the_system() {
        let configured = |tag: &str| {
            let mut config = Config::default();
            config.ui.language = Some(tag.to_string());
            config
        };
        let fixed = |tag: &str| LanguageChoice::Fixed(tag.parse().expect("a tag"));

        assert_eq!(
            language_choice(Some("zh-CN".into()), Some(&configured("en"))),
            Ok(fixed("zh-CN")),
            "the flag is the user's word on this run"
        );
        assert_eq!(
            language_choice(None, Some(&configured("zh-CN"))),
            Ok(fixed("zh-CN")),
            "the file speaks when the flag does not"
        );
        assert_eq!(
            language_choice(None, Some(&configured("auto"))),
            Ok(LanguageChoice::Auto),
            "`auto` in the file is the system list"
        );
        assert_eq!(
            language_choice(None, None),
            Ok(LanguageChoice::Auto),
            "no file and no flag is the system list"
        );
        assert_eq!(
            language_choice(None, Some(&configured("not a tag"))),
            Ok(LanguageChoice::Auto),
            "a typo in the file follows the system rather than stopping the tool"
        );
        assert!(
            language_choice(Some("not a tag".into()), None).is_err(),
            "a bad flag was typed on purpose, so it is reported"
        );
    }

    /// Moving a selection keeps its size and stops at the surface instead of leaving it.
    #[test]
    fn a_moved_selection_keeps_its_size_and_stays_on_the_surface() {
        let rect = Rect::new(100.0, 100.0, 200.0, 150.0);
        let bounds = Size::new(1000.0, 800.0);
        assert_eq!(
            slid(rect, Point::new(50.0, -50.0), bounds),
            Rect::new(150.0, 50.0, 200.0, 150.0)
        );
        assert_eq!(
            slid(rect, Point::new(9999.0, 9999.0), bounds),
            Rect::new(800.0, 650.0, 200.0, 150.0),
            "the far edges stop at the surface, the near ones do not collapse it"
        );
        assert_eq!(
            slid(rect, Point::new(-9999.0, -9999.0), bounds),
            Rect::new(0.0, 0.0, 200.0, 150.0)
        );
    }

    /// A released pin keeps a fixed handhold of itself over the output, whatever its size:
    /// the axes move independently, and a huge pin comes back by the handhold rather than by
    /// a share of itself.
    #[test]
    fn a_release_keeps_a_fixed_corner_on_the_output() {
        let screen = (500.0, 500.0);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let aim = KEEP_AREA.sqrt() + 0.5;

        let size = (200.0, 100.0);
        assert_eq!(
            settle_position(Point::new(100.0, 100.0), size, screen),
            None,
            "a pin well over the output stays where it was let go"
        );
        assert_eq!(
            settle_position(Point::new(450.0, 400.0), size, screen),
            None,
            "a corner showing more than the handhold on both axes is enough"
        );

        let past = settle_position(Point::new(700.0, 200.0), size, screen)
            .expect("a pin past the right edge comes back");
        assert!(
            near(past.x, 500.0 - aim) && past.y == 200.0,
            "only the x axis moves, and only as far as the handhold: {past:?}"
        );

        let corner = settle_position(Point::new(-700.0, 700.0), size, screen)
            .expect("a pin past a corner comes back");
        assert!(
            near(corner.x, aim - size.0) && near(corner.y, 500.0 - aim),
            "each axis moves by its own deficit: {corner:?}"
        );

        // Smaller than the handhold: the only reach it has is all of itself.
        let small = (30.0, 20.0);
        assert_eq!(
            settle_position(Point::new(495.0, 490.0), small, screen),
            Some(Point::new(470.0, 480.0))
        );

        // A full-screen pin dragged up leaves a handhold, not a quarter of itself: the old
        // share rule threw it most of the way down the screen.
        let huge = (2000.0, 1000.0);
        let top = settle_position(Point::new(100.0, -980.0), huge, screen).expect("comes down");
        assert!(
            top.x == 100.0 && near(top.y, aim - huge.1),
            "a huge pin moves by the handhold: {top:?}"
        );

        // One edge already longer than the square's side: only the short one comes in, so
        // the visible corner reads as a square rather than as a bar pulled further across.
        let bar = (600.0, 100.0);
        let slide = settle_position(Point::new(300.0, 490.0), bar, screen).expect("comes up");
        assert!(
            slide.x == 300.0 && near(slide.y, 500.0 - aim),
            "the long edge stays put and the short one is brought in: {slide:?}"
        );

        // A very wide pin showing a few points of its height has plenty of visible *area*;
        // the handhold is per axis, so its short side still has to come in.
        let wide = (5000.0, 100.0);
        let bottom = settle_position(Point::new(-2000.0, 495.0), wide, screen).expect("comes up");
        assert!(
            bottom.x == -2000.0 && near(bottom.y, 500.0 - aim),
            "area is not the test: the short axis must show the handhold: {bottom:?}"
        );

        for (at, size) in [
            (past, size),
            (corner, size),
            (top, huge),
            (slide, bar),
            (bottom, wide),
        ] {
            assert_eq!(
                settle_position(at, size, screen),
                None,
                "a pin that arrived must not be pulled a second time: {at:?}"
            );
        }
    }

    /// The zoom range follows the content's own size: a picture larger than the output can
    /// still grow, a tiny one has a floor of its own, and a content with no size falls back
    /// to the shared range.
    #[test]
    fn a_contents_zoom_range_comes_from_its_own_size() {
        // 3840×2160 on a 2560×1440 output: the old screen-fit cap made the maximum 0.67,
        // below the pin's own 1.0, so the first wheel notch shrank it and nothing grew it.
        let (lowest, highest) = zoom::range((3840.0, 2160.0));
        assert!(
            zoom::scale(highest) > 1.0,
            "a picture bigger than the output must zoom in"
        );
        assert!(zoom::scale(lowest) <= 1.0);
        // Both ends are lattice points, so neither may step outside the drawn-size bounds.
        assert!(zoom::scale(lowest) * 3840.0 >= 32.0);
        assert!(zoom::scale(highest) * 3840.0 <= 16384.0);

        let (lowest, highest) = zoom::range((2.0, 2.0));
        assert_eq!(
            zoom::scale(lowest),
            16.0,
            "a dot starts at sixteen times its size"
        );
        assert_eq!(
            zoom::scale(highest),
            32.0,
            "and the shared ceiling still caps it"
        );

        // A content with no size to measure falls back to the shared envelope.
        let (lowest, highest) = zoom::range((0.0, 0.0));
        assert!(
            (0.05..0.06).contains(&zoom::scale(lowest)),
            "the envelope floor: {}",
            zoom::scale(lowest)
        );
        assert_eq!(zoom::scale(highest), 32.0, "and its ceiling");

        // Whatever the size, the range orders and both ends are scales a wheel can land on.
        for size in [(1.0, 1.0), (700.0, 300.0), (100_000.0, 4.0)] {
            let (lowest, highest) = zoom::range(size);
            assert!(lowest <= highest, "{size:?} → ({lowest}, {highest})");
            assert!(
                (0.05..=32.0).contains(&zoom::scale(lowest))
                    && (0.05..=32.0).contains(&zoom::scale(highest)),
                "{size:?} → ({lowest}, {highest})"
            );
        }
    }

    /// A pin at native zoom covers one device pixel per content pixel and starts on the
    /// device grid; anything else blends every texel with its neighbour.
    #[test]
    fn a_native_pin_lands_on_the_device_grid() {
        // A fractional position must not survive to the quad: the sampler maps the box onto
        // the texture's edges, so half a pixel of origin is half a pixel of blend.
        assert_eq!(
            drawn_at(Point::new(1180.4, 660.6), 1.0, 1.0),
            Point::new(1180.0, 661.0)
        );
        // At 2× a logical point is two device pixels; the grid is the device one.
        assert_eq!(
            drawn_at(Point::new(10.4, 7.2), 2.0, 1.0),
            Point::new(10.5, 7.0)
        );
        // A picture shows one content pixel per device pixel, so 2× halves its size.
        let picture = |width: u32, height: u32, dots: f32| {
            Bitmap::new(
                capture::Cropped {
                    origin: (0, 0),
                    width,
                    height,
                    data: Arc::new(Vec::new()),
                    colour: ui_core::colour::ContentColour::SDR,
                    scale: dots,
                },
                dots,
            )
            .size()
        };
        assert_eq!(picture(200, 120, 1.0), (200.0, 120.0));
        assert_eq!(picture(200, 120, 2.0), (100.0, 60.0));
        // Zooming resamples whatever the origin, so the pointer's own position is kept.
        assert_eq!(
            drawn_at(Point::new(10.4, 7.2), 2.0, 1.5),
            Point::new(10.4, 7.2)
        );
        assert_eq!(drawn_size(picture(200, 120, 2.0), 1.5), (150.0, 90.0));
    }

    /// The content under the pointer stays put when a pin zooms: the wheel is a request to
    /// see more of *that* point, and a zoom about the centre would slide it away.
    #[test]
    fn zoom_keeps_the_content_under_the_pointer() {
        let origin = Point::new(200.0, 100.0);
        // Content (20, 30) sits on screen at (220, 130) while the scale is 1.
        let anchor = Point::new(220.0, 130.0);
        let under =
            |origin: Point, zoom: f32| ((anchor.x - origin.x) / zoom, (anchor.y - origin.y) / zoom);

        let moved = zoomed(origin, 0, zoom::STEPS_PER_NOTCH, anchor);
        let (x, y) = under(moved, zoom::scale(zoom::STEPS_PER_NOTCH));
        assert!(
            (x - 20.0).abs() < 1e-3 && (y - 30.0).abs() < 1e-3,
            "one notch moved the content to ({x}, {y})"
        );

        // A step the limit refuses arrives already clamped, so the ratio used is the one
        // that took effect and the content stays put in the step that stops.
        let near = 158;
        let asked = near + zoom::STEPS_PER_NOTCH;
        let clamped = asked.min(160);
        assert_eq!(clamped, 160, "the request must have been clamped");
        let (was_x, was_y) = under(origin, zoom::scale(near));
        let moved = zoomed(origin, near, clamped, anchor);
        let (x, y) = under(moved, zoom::scale(clamped));
        assert!(
            (x - was_x).abs() < 1e-3 && (y - was_y).abs() < 1e-3,
            "clamping drifted the content from ({was_x}, {was_y}) to ({x}, {y})"
        );

        // Already at the limit: nothing moves at all.
        assert_eq!(zoomed(origin, 160, 160, anchor), origin);
    }

    /// A grip pulls one edge, never both: the opposite side is the user's anchor, and a
    /// drag across it stops at the minimum instead of turning the rectangle inside out.
    #[test]
    fn a_handle_moves_only_its_own_edges_and_keeps_a_minimum() {
        let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
        let bounds = Size::new(1000.0, 1000.0);
        assert_eq!(
            Handle::SouthEast.resized(rect, Point::new(150.0, 100.0), bounds),
            Rect::new(10.0, 10.0, 140.0, 90.0)
        );
        assert_eq!(
            Handle::North.resized(rect, Point::new(999.0, 80.0), bounds),
            Rect::new(10.0, 56.0, 100.0, 4.0),
            "the top stops at the minimum below the bottom, and x does not move"
        );
        assert_eq!(
            Handle::NorthWest.resized(rect, Point::new(500.0, 500.0), bounds),
            Rect::new(106.0, 56.0, 4.0, 4.0)
        );
        assert_eq!(
            Handle::SouthEast.resized(rect, Point::new(9999.0, 9999.0), Size::new(200.0, 180.0)),
            Rect::new(10.0, 10.0, 190.0, 170.0),
            "a grip cannot leave the surface"
        );

        // A corner wins over the two edges that meet it.
        assert_eq!(
            Handle::grabbed(rect, Point::new(11.0, 12.0)),
            Some(Handle::NorthWest)
        );
        assert_eq!(
            Handle::grabbed(rect, Point::new(60.0, 11.0)),
            Some(Handle::North)
        );
        assert_eq!(Handle::grabbed(rect, Point::new(500.0, 500.0)), None);
    }

    /// A drag is a rectangle, not a direction: the two corners name it either way round,
    /// and a pointer outside the surface still selects only what is on screen.
    #[test]
    fn a_selection_is_the_same_rect_from_either_corner_and_stays_on_the_surface() {
        let forward = between(Point::new(100.0, 80.0), Point::new(400.0, 300.0));
        let backward = between(Point::new(400.0, 300.0), Point::new(100.0, 80.0));
        assert_eq!(forward, backward);
        assert_eq!(forward, Rect::new(100.0, 80.0, 300.0, 220.0));

        let outside = between(Point::new(-50.0, -20.0), Point::new(3000.0, 2000.0));
        assert_eq!(
            clamp(outside, 2560.0, 1440.0),
            Rect::new(0.0, 0.0, 2560.0, 1440.0)
        );
    }

    /// One frame at 60 Hz, so every scripted build advances the clock by the same step.
    const FRAME: f32 = 1.0 / 60.0;

    /// A job worker's wake-up: a scripted build is driven by the test, not by a completion.
    struct NoWake;

    impl ui_platform::jobs::Waker for NoWake {
        fn wake(&self) {}
    }

    /// An overlay over a 1280×720 picture with a selection already settled on it.
    fn settled_overlay() -> ui_platform::AppLoop<Overlay> {
        let language = crate::load_language(LanguageChoice::Fixed("en".parse().expect("en")))
            .expect("language");
        let config = crabsnap_core::config::Config::default();
        let look = config.pin_look().0;
        let shortcuts = hotkeys::Plan::resolve(&config);
        let settings = Arc::new(Settings {
            config,
            codecs: codec::Codecs::probe(None, None, &std::collections::BTreeMap::new()),
            shortcuts,
            look,
            language: Arc::clone(&language),
        });
        let image = capture::Image {
            width: 1280,
            height: 720,
            data: Arc::new(vec![0u8; 1280 * 720 * 4]),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
        };
        let mut overlay = Overlay::new(
            image,
            None,
            settings,
            language,
            Rc::new(RefCell::new(Vec::new())),
            ui_platform::jobs::Jobs::new(Arc::new(NoWake)),
        );
        overlay.settled = Some(Rect::new(400.0, 200.0, 400.0, 300.0));
        let mut view = ui_platform::AppLoop::new(overlay);
        // The chords the surface binds are the file's, as `OpenOptions` installs them.
        let plan = hotkeys::Plan::resolve(&crabsnap_core::config::Config::default());
        for (chord, id) in plan.chords(crabsnap_core::hotkeys::Place::Overlay) {
            view.bind_shortcut(chord, id);
        }
        view
    }

    /// Clearing is the selection's own command: the chord from the file drops the rectangle
    /// the toolbar hangs under, without touching anything else.
    #[test]
    fn the_clear_chord_drops_the_selection() {
        let screen = (1280, 720);
        let at = Point::new(500.0, 300.0);
        let mut view = settled_overlay();
        view.build_with_dt(screen, Some(at), FRAME);

        view.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            ctrl: true,
            ..ui_core::Modifiers::default()
        }));
        view.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('d'),
        });
        view.build_with_dt(screen, Some(at), FRAME);
        assert_eq!(
            view.app_mut().settled,
            None,
            "Ctrl+D goes back to choosing a rectangle"
        );
    }

    /// A bar mark's tooltip names the chord that runs it, and a cleared action names none:
    /// the file is the one place either comes from.
    #[test]
    fn a_bar_mark_names_the_chord_that_runs_it() {
        let mark = |plan: &hotkeys::Plan, action: toolbar::Action| {
            toolbar::selection()
                .into_iter()
                .find(|tool| tool.id == action.id())
                .expect("the bar carries the mark")
                .hinted(plan)
                .hint
        };
        let plan = hotkeys::Plan::resolve(&crabsnap_core::config::Config::default());
        assert_eq!(
            mark(&plan, toolbar::Action::Copy).as_deref(),
            Some("Ctrl+C")
        );

        let mut config = crabsnap_core::config::Config::default();
        config
            .shortcuts
            .0
            .insert("copy".to_string(), String::new());
        let cleared = hotkeys::Plan::resolve(&config);
        assert_eq!(
            mark(&cleared, toolbar::Action::Copy),
            None,
            "a cleared action leaves the tooltip without a chord"
        );
    }

    /// The middle button moves a settled selection, because a press inside it belongs to the
    /// annotation canvas; a press outside the rectangle grabs nothing.
    #[test]
    fn the_middle_button_drags_a_settled_selection() {
        let screen = (1280, 720);
        let settled = Rect::new(400.0, 200.0, 400.0, 300.0);
        let inside = Point::new(500.0, 300.0);
        let mut view = settled_overlay();
        view.build_with_dt(screen, Some(inside), FRAME);

        let primary = ui_core::PointerButton::Primary;
        view.dispatch(ui_core::Event::PointerPressed {
            pos: inside,
            button: primary,
        });
        view.build_with_dt(screen, Some(inside), FRAME);
        view.dispatch(ui_core::Event::PointerReleased {
            pos: inside,
            button: primary,
        });
        view.build_with_dt(screen, Some(inside), FRAME);
        assert_eq!(
            view.app_mut().settled,
            Some(settled),
            "the primary button inside the selection draws, it does not move"
        );

        let middle = ui_core::PointerButton::Middle;
        view.dispatch(ui_core::Event::PointerPressed {
            pos: inside,
            button: middle,
        });
        view.build_with_dt(screen, Some(inside), FRAME);
        let dragged = Point::new(560.0, 340.0);
        view.dispatch(ui_core::Event::PointerMoved { pos: dragged });
        view.build_with_dt(screen, Some(dragged), FRAME);
        assert_eq!(
            view.app_mut().settled,
            Some(Rect::new(460.0, 240.0, 400.0, 300.0)),
            "the rectangle follows the middle button"
        );

        view.dispatch(ui_core::Event::PointerReleased {
            pos: dragged,
            button: middle,
        });
        view.build_with_dt(screen, Some(dragged), FRAME);
        assert_eq!(
            view.app_mut().settled,
            Some(Rect::new(460.0, 240.0, 400.0, 300.0)),
            "and the release leaves it where it was dragged"
        );

        // A press outside the rectangle is not a grab, however far it then travels.
        let outside = Point::new(150.0, 100.0);
        view.dispatch(ui_core::Event::PointerPressed {
            pos: outside,
            button: middle,
        });
        view.build_with_dt(screen, Some(outside), FRAME);
        let far = Point::new(900.0, 600.0);
        view.dispatch(ui_core::Event::PointerMoved { pos: far });
        view.build_with_dt(screen, Some(far), FRAME);
        assert_eq!(
            view.app_mut().settled,
            Some(Rect::new(460.0, 240.0, 400.0, 300.0)),
            "the middle button only moves a rectangle it started inside"
        );
    }

    /// A canvas carrying one 400×300 picture pin on a 1280×720 output, and a point on the
    /// pin away from every corner its chrome uses.
    fn wheel_canvas() -> (ui_platform::AppLoop<PinCanvas>, (u32, u32), Point) {
        let language = crate::load_language(LanguageChoice::Fixed("en".parse().expect("en")))
            .expect("language");
        let config = crabsnap_core::config::Config::default();
        let look = config.pin_look().0;
        let shortcuts = hotkeys::Plan::resolve(&config);
        let settings = Arc::new(Settings {
            config,
            codecs: codec::Codecs::probe(None, None, &std::collections::BTreeMap::new()),
            shortcuts,
            look,
            language: Arc::clone(&language),
        });
        let screen = (1280, 720);
        let image = capture::Cropped {
            origin: (0, 0),
            width: 400,
            height: 300,
            data: Arc::new(vec![128u8; 400 * 300 * 4]),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
        };
        let pin = Pin::new(
            Box::new(Bitmap::new(image, 1.0)),
            Placement {
                screen,
                dots: 1.0,
                position: Point::new(200.0, 150.0),
                output: None,
            },
            PinOrigin::Capture { output: None },
            Arc::clone(&language),
            settings,
        );
        let canvas = PinCanvas::new(Arc::clone(&language), look);
        canvas.board.borrow_mut().add(pin);
        (
            ui_platform::AppLoop::new(canvas),
            screen,
            Point::new(300.0, 250.0),
        )
    }

    /// The zoom chip's rect and opacity in the last build.
    fn zoom_chip(view: &ui_platform::AppLoop<PinCanvas>) -> Option<(Rect, f32)> {
        let node = view
            .tree()
            .find_by_id(Id::of(("pin/badge", 1u64, "zoom")))?;
        let node = view.tree().get(node)?;
        let opacity = node.data.params().map(|params| params.opacity)?;
        Some((node.visual_rect(), opacity))
    }

    /// The zoom chip lands at full on the frame the wheel turns, holds while the wheel is
    /// live and for a moment after it, and is gone once that hold and its fade have run.
    #[test]
    fn a_wheel_step_shows_the_zoom_chip_at_full_and_the_departure_lets_it_go() {
        let (mut view, screen, at) = wheel_canvas();
        for _ in 0..20 {
            view.build_with_dt(screen, Some(at), FRAME);
        }
        for _ in 0..5 {
            view.dispatch(ui_core::Event::Scroll {
                pos: at,
                delta: Point::new(0.0, -40.0),
            });
            view.build_with_dt(screen, Some(at), FRAME);
            let (_, opacity) = zoom_chip(&view).expect("a chip while the wheel turns");
            assert!(opacity > 0.99, "a step lands the chip at full: {opacity}");
        }
        // 100 frames is longer than the hold and its fade together.
        for _ in 0..100 {
            view.build_with_dt(screen, Some(at), FRAME);
        }
        assert!(
            zoom_chip(&view).is_none(),
            "the visit ends a moment after the wheel"
        );
    }

    /// The scale a canvas's only pin is drawn at.
    fn pin_zoom(view: &mut ui_platform::AppLoop<PinCanvas>) -> f32 {
        view.app_mut().board.borrow().pins[0].zoom()
    }

    /// The wheel's count is what the scale comes from, so a path that ends on the count it
    /// started from draws at native size — exactly, not 99.9999 % — and one octave is 2×.
    #[test]
    fn a_wheel_there_and_back_returns_to_native_exactly() {
        let (mut view, screen, at) = wheel_canvas();
        for _ in 0..5 {
            view.build_with_dt(screen, Some(at), FRAME);
        }
        let wheel = |view: &mut ui_platform::AppLoop<PinCanvas>, delta: f32| {
            view.dispatch(ui_core::Event::Scroll {
                pos: at,
                delta: Point::new(0.0, delta),
            });
        };
        // Eight notches is one octave: 32 steps, exactly twice the picture.
        for _ in 0..8 {
            wheel(&mut view, -40.0);
        }
        assert_eq!(pin_zoom(&mut view), 2.0);
        // A wandering path that nets no steps leaves the scale where it was.
        for delta in [-40.0, -40.0, 40.0, 40.0, -40.0, 40.0] {
            wheel(&mut view, delta);
        }
        assert_eq!(pin_zoom(&mut view), 2.0);
        // Eight notches back out is native again, and the drawn box is the content's own.
        for _ in 0..8 {
            wheel(&mut view, 40.0);
        }
        assert_eq!(pin_zoom(&mut view), 1.0, "back at native, exactly");
        let board = view.app_mut().board.borrow();
        assert_eq!(board.pins[0].display_size(), board.pins[0].logical_size());
    }

    /// A palette waiting for a colour takes the press: the pin under the pointer gives it a
    /// pixel, and the same gesture does not drag that pin.
    #[test]
    fn a_pick_takes_the_pixel_under_the_press_and_moves_nothing() {
        let language = crate::load_language(LanguageChoice::Fixed("en".parse().expect("en")))
            .expect("language");
        let config = crabsnap_core::config::Config::default();
        let look = config.pin_look().0;
        let shortcuts = hotkeys::Plan::resolve(&config);
        let settings = Arc::new(Settings {
            config,
            codecs: codec::Codecs::probe(None, None, &std::collections::BTreeMap::new()),
            shortcuts,
            look,
            language: Arc::clone(&language),
        });
        let screen = (1280, 720);
        // Left half red, right half blue: the picked pixel is known by where the press is.
        let mut data = Vec::with_capacity(400 * 300 * 4);
        for _ in 0..300 {
            for x in 0..400 {
                let texel = if x < 200 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                data.extend_from_slice(&texel);
            }
        }
        let picture = capture::Cropped {
            origin: (0, 0),
            width: 400,
            height: 300,
            data: Arc::new(data),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
        };
        let placed = |position| Placement {
            screen,
            dots: 1.0,
            position,
            output: None,
        };
        let bitmap = Pin::new(
            Box::new(Bitmap::new(picture, 1.0)),
            placed(Point::new(200.0, 150.0)),
            PinOrigin::Capture { output: None },
            Arc::clone(&language),
            Arc::clone(&settings),
        );
        let mut palette =
            crate::pin::content::palette::Palette::from_colour("#000000").expect("a code");
        assert!(palette.act(Id::str("pin-menu/edit")));
        assert!(palette.act(Id::str("pin-menu/pick")));
        let palette = Pin::new(
            Box::new(palette),
            placed(Point::new(700.0, 150.0)),
            PinOrigin::Colour {
                code: "#000000".into(),
            },
            Arc::clone(&language),
            Arc::clone(&settings),
        );
        let canvas = PinCanvas::new(Arc::clone(&language), look);
        {
            let mut board = canvas.board.borrow_mut();
            board.add(bitmap);
            board.add(palette);
        }
        let mut view = ui_platform::AppLoop::new(canvas);
        let at = Point::new(300.0, 250.0);
        for _ in 0..5 {
            view.build_with_dt(screen, Some(at), FRAME);
        }
        // Where each pin settled before the gesture: ids are 1 for the picture and 2 for
        // the palette, and a raise reorders the list.
        let settled = {
            let canvas = view.app_mut();
            let board = canvas.board.borrow();
            let position = |id: u64| {
                board
                    .pins
                    .iter()
                    .find(|pin| pin.id == id)
                    .map(|pin| pin.position)
            };
            (position(1), position(2))
        };

        view.dispatch(ui_core::Event::PointerMoved { pos: at });
        view.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        view.build_with_dt(screen, Some(at), FRAME);
        // The rest of the same press: a drag would move the picture it started on.
        let moved = Point::new(360.0, 300.0);
        view.dispatch(ui_core::Event::PointerMoved { pos: moved });
        view.build_with_dt(screen, Some(moved), FRAME);

        let canvas = view.app_mut();
        let mut board = canvas.board.borrow_mut();
        let copied = board
            .pins
            .iter_mut()
            .find(|pin| pin.id == 2)
            .expect("the palette")
            .content
            .clipboard((0, 0))
            .expect("a palette copies");
        assert_eq!(
            copied[0].1, b"#FF0000\n",
            "the shown colour is the picked pixel"
        );
        assert!(
            !board.pins.iter().any(|pin| pin.content.picking()),
            "the wait ended with the press"
        );
        let position = |id: u64| {
            board
                .pins
                .iter()
                .find(|pin| pin.id == id)
                .map(|pin| pin.position)
        };
        assert_eq!(
            position(1),
            settled.0,
            "the pin the pixel came from stays where it was"
        );
        assert_eq!(position(2), settled.1, "and the palette stays too");
    }

    /// A press with nothing of ours under it leaves the wait open and asks the daemon for
    /// the screen: the request carries the press, the canvas it landed on and the output.
    #[test]
    fn a_pick_on_bare_canvas_asks_for_the_screen() {
        let language = crate::load_language(LanguageChoice::Fixed("en".parse().expect("en")))
            .expect("language");
        let config = crabsnap_core::config::Config::default();
        let look = config.pin_look().0;
        let shortcuts = hotkeys::Plan::resolve(&config);
        let settings = Arc::new(Settings {
            config,
            codecs: codec::Codecs::probe(None, None, &std::collections::BTreeMap::new()),
            shortcuts,
            look,
            language: Arc::clone(&language),
        });
        let screen = (1280, 720);
        let at = Point::new(200.0, 150.0);
        let mut palette =
            crate::pin::content::palette::Palette::from_colour("#000000").expect("a code");
        assert!(palette.act(Id::str("pin-menu/edit")));
        assert!(palette.act(Id::str("pin-menu/pick")));
        let palette = Pin::new(
            Box::new(palette),
            Placement {
                screen,
                dots: 1.0,
                position: at,
                output: Some("WAYLAND-1".into()),
            },
            PinOrigin::Colour {
                code: "#000000".into(),
            },
            Arc::clone(&language),
            Arc::clone(&settings),
        );
        let canvas = PinCanvas::new(Arc::clone(&language), look);
        canvas.board.borrow_mut().add(palette);
        let mut view = ui_platform::AppLoop::new(canvas);
        let press = Point::new(900.0, 600.0);
        for _ in 0..5 {
            view.build_with_dt(screen, Some(press), FRAME);
        }
        let settled = view.app_mut().board.borrow().pins[0].position;

        view.dispatch(ui_core::Event::PointerMoved { pos: press });
        view.dispatch(ui_core::Event::PointerPressed {
            pos: press,
            button: ui_core::PointerButton::Primary,
        });
        view.build_with_dt(screen, Some(press), FRAME);

        let canvas = view.app_mut();
        let board = canvas.board.borrow();
        let request = board.pick.clone().expect("the screen was asked for");
        assert_eq!(request.pin, 1);
        assert_eq!(
            request.output.as_deref(),
            Some("WAYLAND-1"),
            "the frame comes from the output the canvas covers"
        );
        assert_eq!(request.screen, screen);
        assert_eq!(request.at, press);
        assert!(
            board.pins[0].content.picking(),
            "the wait stays open for the answer"
        );
        assert_eq!(
            board.pins[0].position, settled,
            "and the press moved nothing"
        );
    }
}
