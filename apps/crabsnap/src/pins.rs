//! 贴图的落点与归属：一个输出一块 canvas，先来先开、后到加纹理。
//!
//! daemon 与一次性的 `shot` 命令共用这一份逻辑：canvas 属于连接，不属于某一条命令，
//! 但两者的连接不同，所以这里只处理「这张贴图放到哪块 canvas、要不要新开一块」。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use crabsnap_core::protocol::PinInfo;
use ui_core::Id;
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::draw::ImageEntry;
use ui_core::geom::{Insets, Point};
use ui_platform::SurfaceId;
use ui_platform::host::{MessageCtx, OpenOptions};
use ui_platform::window::{
    Anchor, AutoHide, ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceSpec,
};

use crate::codec::Codecs;
use crate::pin::ExportAsk;
use crate::{Pin, PinBoard, PinCanvas};
use crabsnap_core::config::PinLook;

/// How far each item of one list steps down-right from the one before it, in logical points.
const CASCADE_STEP: f32 = 24.0;

/// Where a picture goes on an output: centred, never off the top-left corner.
pub(crate) fn centered_at(screen: (u32, u32), size: (f32, f32)) -> Point {
    Point::new(
        (screen.0 as f32 - size.0).max(0.0) * 0.5,
        (screen.1 as f32 - size.1).max(0.0) * 0.5,
    )
}

/// Where item `index` of one list goes: the single-pin spot, stepped down-right so each pin
/// keeps its title bar in reach, and stopped at the output's edge.
pub(crate) fn cascade_at(screen: (u32, u32), size: (f32, f32), index: usize) -> Point {
    let centred = centered_at(screen, size);
    let step = CASCADE_STEP * index as f32;
    Point::new(
        (centred.x + step).min((screen.0 as f32 - size.0).max(0.0)),
        (centred.y + step).min((screen.1 as f32 - size.1).max(0.0)),
    )
}

/// The layer a canvas lives on: its output, keyboard following the pin under the pointer.
///
/// The output's name is not decoration: a canvas sized for one monitor but opened on another
/// would put its pins where the user is not looking.
pub(crate) fn canvas_surface(screen: (u32, u32), output: Option<&str>) -> SurfaceSpec {
    SurfaceSpec::layer(
        "dev.crabsnap",
        "crabsnap-pins",
        LayerSpec {
            output: output.map(str::to_owned),
            level: Layer::Overlay,
            anchor: Anchor {
                top: true,
                bottom: false,
                left: true,
                right: false,
            },
            size: Some(screen),
            exclusive: ExclusiveZone::Ignore,
            margin: Insets {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
            // The keyboard belongs to the pin under the pointer, and to nothing else: with
            // the canvas covering the output, a held keyboard would swallow typing meant for
            // the window below wherever no pin is.
            keyboard: KeyboardMode::FollowPointer,
            auto_hide: AutoHide::Yield,
        },
    )
}

/// The pins of every canvas this process opened, for answers that own no surface.
///
/// `pin list` must read what the canvases are showing; the daemon has no app state of its
/// own, so it keeps the boards and borrows them from here.
pub(crate) struct Pins {
    ids: Rc<Cell<u64>>,
    canvases: Vec<Canvas>,
    /// The look a newly opened canvas paints with; it comes from the config the process
    /// started with.
    look: PinLook,
    /// The keys a canvas answers, resolved once: a canvas opened later must answer the same
    /// chords as one opened at startup.
    shortcuts: Vec<(KeyChord, ShortcutId)>,
}

/// One output's pins: the list they live in, and where the canvas was opened.
struct Canvas {
    output: String,
    /// Set once the surface exists; pins that arrive later become extra textures on it.
    surface: Option<SurfaceId>,
    board: Rc<RefCell<PinBoard>>,
}

impl Pins {
    pub(crate) fn new(look: PinLook, shortcuts: Vec<(KeyChord, ShortcutId)>) -> Self {
        Self {
            ids: Rc::new(Cell::new(0)),
            canvases: Vec::new(),
            look,
            shortcuts,
        }
    }

    /// The id source every canvas of this owner shares.
    pub(crate) fn ids(&self) -> Rc<Cell<u64>> {
        Rc::clone(&self.ids)
    }

    /// The canvas covering `output`, if one is already open.
    pub(crate) fn canvas_for(&self, output: &str) -> Option<usize> {
        self.canvases
            .iter()
            .position(|canvas| canvas.output == output)
    }

    pub(crate) fn surface_of(&self, canvas: usize) -> Option<SurfaceId> {
        self.canvases.get(canvas).and_then(|canvas| canvas.surface)
    }

    /// The first palette waiting for a screen pixel, if any canvas has one.
    pub(crate) fn take_pick(&mut self) -> Option<crate::pin::PickRequest> {
        self.canvases
            .iter_mut()
            .find_map(|canvas| canvas.board.borrow_mut().take_pick())
    }

    /// Answer a pick with the colour the screen gave, or `None` when it could not be read.
    /// The canvas holding the pin is the one to redraw.
    pub(crate) fn picked(
        &mut self,
        request: &crate::pin::PickRequest,
        colour: Option<[u8; 4]>,
    ) -> Option<SurfaceId> {
        for canvas in &self.canvases {
            let mut board = canvas.board.borrow_mut();
            if let Some(pin) = board.pins.iter_mut().find(|pin| pin.id == request.pin) {
                pin.content.picked(colour);
                return canvas.surface;
            }
        }
        None
    }

    /// Record a canvas and return its index.
    pub(crate) fn open(&mut self, output: String, board: Rc<RefCell<PinBoard>>) -> usize {
        self.canvases.push(Canvas {
            output,
            surface: None,
            board,
        });
        self.canvases.len() - 1
    }

    /// The surfaces the canvases are open on: a reload that changes words repaints them.
    pub(crate) fn surfaces(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.canvases.iter().filter_map(|canvas| canvas.surface)
    }

    pub(crate) fn attach(&mut self, canvas: usize, surface: SurfaceId) {
        if let Some(canvas) = self.canvases.get_mut(canvas) {
            canvas.surface = Some(surface);
        }
    }

    /// Add a pin to an open canvas; a picture's texture must then reach that surface.
    pub(crate) fn push_pin(
        &mut self,
        canvas: usize,
        pin: Pin,
    ) -> Result<Option<ImageEntry>, String> {
        let canvas = self.canvases.get_mut(canvas).ok_or("no such canvas")?;
        let mut board = canvas.board.borrow_mut();
        board.add(pin);
        let pin = board.pins.last_mut().ok_or("the pin was not added")?;
        Ok(pin.texture_entry())
    }

    pub(crate) fn count(&self) -> usize {
        self.canvases
            .iter()
            .map(|canvas| {
                canvas
                    .board
                    .borrow()
                    .pins
                    .iter()
                    .filter(|pin| !pin.closed)
                    .count()
            })
            .sum()
    }

    /// Run `change` on the pin `id` names; the board's borrow cannot outlive this call.
    ///
    /// The canvas's surface comes back with the result: a change made from outside the app's
    /// turn is only seen when that surface builds its next frame.
    fn change<T>(
        &mut self,
        id: Option<u64>,
        change: impl FnOnce(&mut Pin) -> T,
    ) -> Result<(T, Option<SurfaceId>), String> {
        let id =
            id.ok_or("name the pin with `--id`: the pointer is a client's, not the daemon's")?;
        for canvas in &mut self.canvases {
            let mut board = canvas.board.borrow_mut();
            if let Some(pin) = board.pins.iter_mut().find(|pin| pin.id == id) {
                return Ok((change(pin), canvas.surface));
            }
        }
        Err(format!("no pin {id}"))
    }

    /// The surface to redraw after this call's change, if it has one yet.
    pub(crate) fn close(&mut self, id: Option<u64>) -> Result<Option<SurfaceId>, String> {
        self.change(id, |pin| pin.closed = true)
            .map(|(_, surface)| surface)
    }

    /// Open the annotation editor over one pin, as its menu's Edit row does.
    ///
    /// A kind with no editor answers with the reason; one already being edited is a
    /// success that draws nothing.
    pub(crate) fn edit(&mut self, id: u64) -> Result<Option<SurfaceId>, String> {
        let (changed, surface) = self.change(Some(id), |pin| {
            if pin.content.editing() {
                return Ok(false);
            }
            if !pin.content.act(Id::str("pin-menu/edit")) {
                return Err(format!(
                    "pin {id} holds {} and has no editor yet",
                    pin.content.kind().name()
                ));
            }
            Ok(true)
        })?;
        changed.map(|_| surface)
    }

    /// `shown` is `None` for a toggle.
    pub(crate) fn set_shown(
        &mut self,
        id: Option<u64>,
        shown: Option<bool>,
    ) -> Result<Option<SurfaceId>, String> {
        self.change(id, |pin| pin.shown = shown.unwrap_or(!pin.shown))
            .map(|(_, surface)| surface)
    }

    /// An export writes a file and draws nothing, so it needs no frame.
    pub(crate) fn export(
        &mut self,
        id: Option<u64>,
        path: &std::path::Path,
        codecs: &Codecs,
        configured: Option<&str>,
        format: Option<&str>,
    ) -> Result<(), String> {
        self.change(id, |pin| {
            let drawn = pin.drawn_pixels();
            let exported = pin.content.export(&ExportAsk {
                codecs,
                configured,
                path: Some(path),
                format,
                drawn,
            })?;
            std::fs::write(path, exported.bytes)
                .map_err(|error| format!("write {}: {error}", path.display()))
        })
        .map(|_| ())
    }

    pub(crate) fn list(&self) -> Vec<PinInfo> {
        let mut pins = Vec::new();
        for canvas in &self.canvases {
            for pin in canvas.board.borrow().pins.iter().filter(|pin| !pin.closed) {
                let (width, height) = pin.logical_size();
                pins.push(PinInfo {
                    id: pin.id,
                    kind: pin.content.kind(),
                    title: pin.content.title(),
                    // The material's own pixel size: a bitmap's pixels, a text's laid out
                    // points, both stated in the units the pin list prints.
                    width: (width * pin.dots).round() as u32,
                    height: (height * pin.dots).round() as u32,
                    x: pin.position.x,
                    y: pin.position.y,
                    origin: pin.origin.clone(),
                    output: pin.output.clone(),
                    shown: pin.shown,
                });
            }
        }
        pins
    }
}

/// Put a pin on its output's canvas: an existing canvas takes one more texture, otherwise
/// this turn's context opens one.
pub(crate) fn place(
    pins: &mut Pins,
    pin: Pin,
    cx: &mut MessageCtx<'_>,
) -> Result<SurfaceId, String> {
    // A canvas whose surface closed is forgotten before a texture is placed: its pins went
    // with the surface, and its id must not be handed another one.
    pins.canvases
        .retain(|canvas| canvas.surface.is_some_and(|id| cx.is_open(id)));
    let screen = pin.screen;
    let Some(output) = pin.output.clone() else {
        return Err("the pin does not say which output it came from".into());
    };
    // One canvas per output: a second pin joins the one already there, so pressing it can
    // raise it above the first.
    if let Some(canvas) = pins.canvas_for(&output) {
        let surface = pins.surface_of(canvas).ok_or("the canvas has no surface")?;
        match pins.push_pin(canvas, pin)? {
            Some(entry) => cx.add_image(surface, entry)?,
            // A content without pixels (text, a palette's card) brings no texture, so the
            // canvas only learns about it from a rebuild: ask for one.
            None => cx.request_frame(surface)?,
        }
        return Ok(surface);
    }
    let catalog = Arc::clone(&pin.catalog);
    let mut canvas = PinCanvas::sharing(catalog, pins.ids(), pins.look);
    canvas.add(pin);
    let board = Rc::clone(canvas.board());
    let entry = {
        let mut board = board.borrow_mut();
        board
            .pins
            .last_mut()
            .ok_or("the pin was not added")?
            .texture_entry()
    };
    let surface = cx.open(
        canvas_surface(screen, Some(output.as_str())),
        Box::new(canvas),
        OpenOptions {
            uncapped: false,
            images: entry.into_iter().collect(),
            shortcuts: pins.shortcuts.clone(),
            fonts: vec![crate::fonts::icon_face()],
            ..OpenOptions::default()
        },
    )?;
    let index = pins.open(output, board);
    pins.attach(index, surface);
    Ok(surface)
}

/// Take every pin an overlay handed over and put it on its output.
pub(crate) fn place_pending(
    pins: &mut Pins,
    outbox: &Rc<RefCell<Vec<Pin>>>,
    cx: &mut MessageCtx<'_>,
) {
    let pending: Vec<Pin> = outbox.borrow_mut().drain(..).collect();
    for pin in pending {
        if let Err(error) = place(pins, pin, cx) {
            log::warn!("pin: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list is a staircase: the first item sits where a single pin would, each later one
    /// steps down-right, and nothing leaves the output. The ninth item must not fall back
    /// onto the first, which an eight-step wrap would do.
    #[test]
    fn a_cascade_steps_and_stops_at_the_edge() {
        let screen = (1000, 800);
        let size = (200.0, 100.0);
        let base = centered_at(screen, size);
        assert_eq!(cascade_at(screen, size, 0), base);
        assert_eq!(
            cascade_at(screen, size, 1),
            Point::new(base.x + 24.0, base.y + 24.0)
        );

        let stepped: Vec<Point> = (0..9)
            .map(|index| cascade_at(screen, size, index))
            .collect();
        for (index, point) in stepped.iter().enumerate() {
            assert!(
                !stepped[..index].contains(point),
                "item {index} landed on an earlier one"
            );
        }
        assert_eq!(
            cascade_at(screen, size, 100),
            Point::new(800.0, 700.0),
            "the far edge clamps instead of pushing the pin off the output"
        );
    }
}
