//! Output capture, newest protocol first.
//!
//! `ext-image-copy-capture-v1` is the standard path: a session reports its buffer constraints
//! and a frame copies into a buffer the client allocated. wlroots' `zwlr_screencopy_manager_v1`
//! is what a compositor without the ext protocol still offers, and the desktop portal is the
//! last resort for a compositor that offers neither. The tool walks the chain rather than
//! refusing to run; every rung yields the same [`Frame`].
//!
//! One grab is one handshake, and the wait is bounded: a compositor that never answers must
//! not hang the app. The portal rung is the exception in one respect: it captures the whole
//! screen through the desktop, which may ask the user for permission.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ui_platform_wayland::{ShmBuffer, WaylandLink};
use wayland_client::backend::WaylandError;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::{
    ExtImageCopyCaptureSessionV1, Event as SessionEvent,
};
use wayland_protocols::ext::image_capture_source::v1::client::ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{
    Event as ScreencopyEvent, Flags as ScreencopyFlags, ZwlrScreencopyFrameV1,
};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

/// One captured frame, top-down rows in the compositor's own packing.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Bytes between rows; always tight (`width * 4`) because the frame owns its buffer.
    pub stride: u32,
    /// Native pixels: the bytes the compositor wrote, in `colour.storage`'s packing.
    pub pixels: Vec<u8>,
    /// The shm format the compositor sent, or the one a decoded frame was converted to.
    pub format: Format,
    /// What `pixels` mean. Carried, never applied: converting here would destroy an HDR
    /// frame for every later use, including a surface that could have shown it.
    pub colour: ui_core::colour::ContentColour,
    /// The `wl_output` this frame came from; `None` when the protocol does not name one
    /// (the portal captures whatever the desktop shows).
    pub output: Option<String>,
    /// The output's device pixels per logical point: what a saved file's density says, and
    /// nothing about how the pixels are packed.
    pub scale: f32,
}

#[derive(Default)]
struct CaptureState {
    /// `None` until the session reports its source dimensions.
    size: Option<(u32, u32)>,
    /// The formats the session accepts; the first one our buffer layout can hold is used.
    formats: Vec<Format>,
    constraints_done: bool,
    stopped: bool,
    ready: bool,
    failed: Option<u32>,
    /// The output's `name` event, which arrives once at bind time.
    output_name: Option<String>,
    /// The output's `scale` event: device pixels per logical point.
    scale: Option<f32>,
    /// The output's colour description, read when the backend was bound.
    colour: Option<crate::color::Description>,
}

impl CaptureState {
    /// Clear what a grab produces, keeping what the bind reported once.
    fn reset(&mut self) {
        let output_name = self.output_name.take();
        let scale = self.scale.take();
        let colour = self.colour.take();
        *self = Self {
            output_name,
            scale,
            colour,
            ..Self::default()
        };
    }
}

/// The capture backend chosen at startup.
pub struct Capture {
    backend: Backend,
}

enum Backend {
    Ext(ExtCapture),
    Screencopy(ScreencopyCapture),
    Portal(PortalCapture),
}

impl Capture {
    /// Bind the first capture protocol this compositor offers on the app's connection.
    ///
    /// `pinned` names one rung (`ext`, `screencopy`, `portal`), so the fallback chain can be
    /// exercised on a compositor that offers more than one; `None` walks the chain in order.
    pub fn new(
        link: &WaylandLink,
        pinned: Option<&str>,
        output: usize,
    ) -> Result<Option<Self>, String> {
        let pinned = pinned.unwrap_or_default();
        let ext = matches!(pinned, "" | "ext");
        let screencopy = matches!(pinned, "" | "screencopy");
        let portal = matches!(pinned, "" | "portal");
        if !ext && !screencopy && !portal {
            return Err(format!(
                "unknown capture backend {pinned:?}; expected ext, screencopy or portal"
            ));
        }
        if ext && let Some(capture) = ExtCapture::new(link, output)? {
            return Ok(Some(Self {
                backend: Backend::Ext(capture),
            }));
        }
        if screencopy && let Some(capture) = ScreencopyCapture::new(link, output)? {
            return Ok(Some(Self {
                backend: Backend::Screencopy(capture),
            }));
        }
        if portal && let Some(capture) = PortalCapture::new(link)? {
            return Ok(Some(Self {
                backend: Backend::Portal(capture),
            }));
        }
        Ok(None)
    }

    /// Ask for one frame without waiting for it.
    ///
    /// The ext and screencopy rungs hand back a [`Pending`] the loop polls, so a turn never
    /// waits on the compositor; the portal rung answers over D-Bus and can only be grabbed
    /// while the caller waits, so it comes back as [`Started::Blocking`].
    pub fn begin(self, timeout: Duration) -> Result<Started, String> {
        let deadline = Instant::now() + timeout;
        match self.backend {
            Backend::Ext(mut ext) => {
                ext.state.reset();
                let qh = ext.queue.handle();
                let source = ext.sources.create_source(&ext.output, &qh, ());
                let session = ext
                    .manager
                    .create_session(&source, capture_options(), &qh, ());
                let _ = ext.connection.flush();
                Ok(Started::Pending(Pending {
                    inner: PendingBackend::Ext(ExtPending {
                        connection: ext.connection,
                        queue: ext.queue,
                        state: ext.state,
                        shm: ext.shm,
                        session: Some(session),
                        source: Some(source),
                        frame: None,
                        deadline,
                    }),
                }))
            }
            Backend::Screencopy(mut capture) => {
                capture.state.reset();
                let qh = capture.queue.handle();
                let frame = capture.manager.capture_output(0, &capture.output, &qh, ());
                let _ = capture.connection.flush();
                Ok(Started::Pending(Pending {
                    inner: PendingBackend::Screencopy(ScreencopyPending {
                        connection: capture.connection,
                        queue: capture.queue,
                        state: capture.state,
                        shm: capture.shm,
                        frame: Some(frame),
                        copy: None,
                        needs_done: capture.manager.version() >= 3,
                        deadline,
                    }),
                }))
            }
            Backend::Portal(capture) => Ok(Started::Blocking(Capture {
                backend: Backend::Portal(capture),
            })),
        }
    }

    /// Which rung this capture bound, as `info status` reports it.
    pub fn name(&self) -> &'static str {
        match &self.backend {
            Backend::Ext(_) => "ext-image-copy-capture-v1",
            Backend::Screencopy(_) => "wlr-screencopy",
            Backend::Portal(_) => "portal",
        }
    }

    /// Capture the first output once, giving up after `timeout`.
    pub fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        match &mut self.backend {
            Backend::Ext(capture) => capture.grab(timeout),
            Backend::Screencopy(capture) => capture.grab(timeout),
            Backend::Portal(capture) => capture.grab(timeout),
        }
    }
}

/// Bind the output at `index` in the compositor's list, not just the first one.
///
/// `GlobalList::bind` always takes the first global of an interface, which cannot name the
/// second monitor; the registry itself can.
fn bind_output<State>(
    globals: &wayland_client::globals::GlobalList,
    queue: &wayland_client::QueueHandle<State>,
    index: usize,
) -> Result<WlOutput, String>
where
    State: wayland_client::Dispatch<WlOutput, ()> + Send + Sync + 'static,
{
    let outputs: Vec<(u32, u32)> = globals
        .contents()
        .clone_list()
        .iter()
        .filter(|global| global.interface == "wl_output")
        .map(|global| (global.name, global.version))
        .collect();
    let Some((name, version)) = outputs.get(index) else {
        return Err(format!(
            "this compositor reports {} output(s); there is no output {}",
            outputs.len(),
            index + 1
        ));
    };
    Ok(globals
        .registry()
        .bind::<WlOutput, (), State>(*name, (*version).min(4), queue, ()))
}

struct ExtCapture {
    connection: Connection,
    queue: EventQueue<CaptureState>,
    state: CaptureState,
    manager: ExtImageCopyCaptureManagerV1,
    sources: ExtOutputImageCaptureSourceManagerV1,
    shm: WlShm,
    /// First output, and the one every grab captures: the overlay later picks per surface.
    output: WlOutput,
}

impl ExtCapture {
    /// Bind the ext protocol's globals on a queue of our own.
    ///
    /// Absent globals mean this compositor does not offer the protocol; the chain moves to
    /// the next rung rather than treating it as a failure to start.
    fn new(link: &WaylandLink, output: usize) -> Result<Option<Self>, String> {
        let queue = link.new_queue::<CaptureState>();
        let qh = queue.handle();
        let globals = link.globals();
        let manager =
            match globals.bind::<ExtImageCopyCaptureManagerV1, CaptureState, ()>(&qh, 1..=1, ()) {
                Ok(manager) => manager,
                Err(_) => return Ok(None),
            };
        let sources = match globals.bind::<ExtOutputImageCaptureSourceManagerV1, CaptureState, ()>(
            &qh,
            1..=1,
            (),
        ) {
            Ok(sources) => sources,
            Err(_) => return Ok(None),
        };
        let shm = globals
            .bind::<WlShm, CaptureState, ()>(&qh, 1..=1, ())
            .map_err(|e| format!("wl_shm: {e}"))?;
        let output = bind_output(globals, &qh, output)?;
        // The pixels this output hands out are described by the compositor, not by us; a
        // compositor without the protocol leaves the description absent.
        let colour = crate::color::Description::read(link, &output);
        Ok(Some(Self {
            connection: link.connection().clone(),
            queue,
            state: CaptureState {
                colour,
                ..CaptureState::default()
            },
            manager,
            sources,
            shm,
            output,
        }))
    }

    fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        self.state.reset();
        let deadline = Instant::now() + timeout;
        let qh = self.queue.handle();
        let source = self.sources.create_source(&self.output, &qh, ());
        let session = self
            .manager
            .create_session(&source, capture_options(), &qh, ());
        // The compositor owns these two; a failure below must still hand them back.
        let outcome = self.grab_from(&session, &qh, &deadline);
        session.destroy();
        source.destroy();
        outcome
    }

    fn grab_from(
        &mut self,
        session: &ExtImageCopyCaptureSessionV1,
        qh: &QueueHandle<CaptureState>,
        deadline: &Instant,
    ) -> Result<Frame, String> {
        pump(
            &mut self.queue,
            &self.connection,
            &mut self.state,
            deadline,
            |state| state.constraints_done || state.stopped,
        )?;
        if self.state.stopped {
            return Err("the compositor stopped the capture session".into());
        }
        let (width, height) = self
            .state
            .size
            .ok_or_else(|| "the session reported no size".to_string())?;
        // Every format we accept is 32 bits per pixel: the buffer layout is the same, only
        // the packing of the colour channels changes.
        let format = self
            .state
            .formats
            .iter()
            .copied()
            .find(|format| packable(*format))
            .ok_or_else(|| format!("no 32-bit format among {:?}", self.state.formats))?;

        let buffer =
            ShmBuffer::new(&self.shm, qh, width, height).ok_or("cannot allocate a shm buffer")?;
        let wl_buffer = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            buffer.stride as i32,
            format,
            qh,
            (),
        );

        let frame = session.create_frame(qh, ());
        frame.attach_buffer(&wl_buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        let _ = self.connection.flush();
        let waited = pump(
            &mut self.queue,
            &self.connection,
            &mut self.state,
            deadline,
            |state| state.ready || state.failed.is_some(),
        );
        frame.destroy();
        wl_buffer.destroy();
        waited?;
        if let Some(reason) = self.state.failed {
            return Err(format!("the compositor failed the frame: {reason}"));
        }

        // The frame owns its buffer, so rows are tight whatever the wire stride was.
        Ok(described(
            self.state.colour.as_ref(),
            self.state.output_name.as_ref(),
            self.state.scale,
            width,
            height,
            buffer.bytes().to_vec(),
            format,
        ))
    }
}

/// Dispatch until `done` holds or the deadline passes.
///
/// The socket is non-blocking, so the wait is a 1 ms nap between reads: a capture
/// handshake is two round trips, and there is no frame to pace here.
fn pump<State>(
    queue: &mut EventQueue<State>,
    connection: &Connection,
    state: &mut State,
    deadline: &Instant,
    done: impl Fn(&State) -> bool,
) -> Result<(), String> {
    loop {
        queue
            .dispatch_pending(state)
            .map_err(|e| format!("capture dispatch: {e}"))?;
        // The requests that answer this wait are ours; the window's turn may be a while off.
        connection
            .flush()
            .map_err(|e| format!("capture flush: {e}"))?;
        if done(state) {
            return Ok(());
        }
        if Instant::now() >= *deadline {
            return Err("capture timed out".into());
        }
        match queue.prepare_read() {
            Some(guard) => match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => return Err(format!("capture read: {error}")),
            },
            // Another read is in flight; dispatch first, then try again.
            None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

#[derive(Default)]
struct ScreencopyState {
    /// `(format, width, height, stride)` from the frame's `buffer` event.
    buffer: Option<(WEnum<Format>, u32, u32, u32)>,
    /// Version 3 announces all buffer types before letting the client pick one.
    buffer_done: bool,
    y_invert: bool,
    ready: bool,
    failed: bool,
    /// The output's `name` event, which arrives once at bind time.
    output_name: Option<String>,
    /// The output's `scale` event: device pixels per logical point.
    scale: Option<f32>,
    /// The output's colour description, read when the backend was bound.
    colour: Option<crate::color::Description>,
}

impl ScreencopyState {
    /// Clear what a grab produces, keeping what the bind reported once.
    fn reset(&mut self) {
        let output_name = self.output_name.take();
        let scale = self.scale.take();
        let colour = self.colour.take();
        *self = Self {
            output_name,
            scale,
            colour,
            ..Self::default()
        };
    }
}

struct ScreencopyCapture {
    connection: Connection,
    queue: EventQueue<ScreencopyState>,
    state: ScreencopyState,
    manager: ZwlrScreencopyManagerV1,
    shm: WlShm,
    output: WlOutput,
}

impl ScreencopyCapture {
    /// Bind the wlroots protocol's globals on a queue of our own.
    fn new(link: &WaylandLink, output: usize) -> Result<Option<Self>, String> {
        let queue = link.new_queue::<ScreencopyState>();
        let qh = queue.handle();
        let globals = link.globals();
        let Ok(manager) =
            globals.bind::<ZwlrScreencopyManagerV1, ScreencopyState, ()>(&qh, 1..=3, ())
        else {
            return Ok(None);
        };
        let shm = globals
            .bind::<WlShm, ScreencopyState, ()>(&qh, 1..=1, ())
            .map_err(|e| format!("wl_shm: {e}"))?;
        let output = bind_output(globals, &qh, output)?;
        let colour = crate::color::Description::read(link, &output);
        Ok(Some(Self {
            connection: link.connection().clone(),
            queue,
            state: ScreencopyState {
                colour,
                ..ScreencopyState::default()
            },
            manager,
            shm,
            output,
        }))
    }

    fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        self.state.reset();
        let deadline = Instant::now() + timeout;
        let qh = self.queue.handle();
        // The cursor is the overlay's to draw, so the compositor leaves it out.
        let frame = self.manager.capture_output(0, &self.output, &qh, ());
        let outcome = self.copy_from(&frame, &qh, &deadline);
        frame.destroy();
        outcome
    }

    fn copy_from(
        &mut self,
        frame: &ZwlrScreencopyFrameV1,
        qh: &QueueHandle<ScreencopyState>,
        deadline: &Instant,
    ) -> Result<Frame, String> {
        // Version 3 may offer dma-buf instead, so it announces the set before `buffer_done`;
        // older versions always offer the wl_shm `buffer` event and have no `buffer_done`.
        if self.manager.version() >= 3 {
            pump(
                &mut self.queue,
                &self.connection,
                &mut self.state,
                deadline,
                |state| state.buffer_done || state.failed,
            )?;
        } else {
            pump(
                &mut self.queue,
                &self.connection,
                &mut self.state,
                deadline,
                |state| state.buffer.is_some() || state.failed,
            )?;
        }
        if self.state.failed {
            return Err("the compositor failed the frame".into());
        }
        let (format, width, height, stride) = self
            .state
            .buffer
            .ok_or_else(|| "the frame offered no wl_shm buffer".to_string())?;
        let format = match format {
            WEnum::Value(format) => format,
            // A compositor may hand out a fourcc that wl_shm does not name.
            WEnum::Unknown(raw) => {
                return Err(format!(
                    "the frame asked for unknown wl_shm format {raw:#x}"
                ));
            }
        };
        if !packable(format) {
            return Err(format!(
                "the frame asked for {format:?}, which we cannot read"
            ));
        }

        let buffer = ShmBuffer::new_with_stride(&self.shm, qh, width, height, stride)
            .ok_or_else(|| format!("cannot allocate {width}x{height} at stride {stride}"))?;
        let wl_buffer = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            qh,
            (),
        );
        frame.copy(&wl_buffer);
        let _ = self.connection.flush();
        let waited = pump(
            &mut self.queue,
            &self.connection,
            &mut self.state,
            deadline,
            |state| state.ready || state.failed,
        );
        wl_buffer.destroy();
        waited?;
        if self.state.failed {
            return Err("the compositor failed the copy".into());
        }

        let pixels = rows(buffer.bytes(), width, height, stride, self.state.y_invert);
        Ok(described(
            self.state.colour.as_ref(),
            self.state.output_name.as_ref(),
            self.state.scale,
            width,
            height,
            pixels,
            format,
        ))
    }
}

/// What a capture request turned into.
pub enum Started {
    /// The loop polls it; no turn waits on the compositor.
    Pending(Pending),
    /// The rung can only grab while the caller waits (the portal's round trip).
    Blocking(Capture),
}

/// A capture asked for and not yet ready: one non-blocking pass per turn.
pub struct Pending {
    inner: PendingBackend,
}

enum PendingBackend {
    Ext(ExtPending),
    Screencopy(ScreencopyPending),
}

impl Pending {
    /// One non-blocking pass. `None` means "not yet"; an error ends the capture.
    pub fn poll(&mut self) -> Option<Result<Frame, String>> {
        match &mut self.inner {
            PendingBackend::Ext(pending) => pending.poll(),
            PendingBackend::Screencopy(pending) => pending.poll(),
        }
    }

    /// When the caller should give up on this capture.
    pub fn deadline(&self) -> Instant {
        match &self.inner {
            PendingBackend::Ext(pending) => pending.deadline,
            PendingBackend::Screencopy(pending) => pending.deadline,
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        match &mut self.inner {
            PendingBackend::Ext(pending) => pending.release(),
            PendingBackend::Screencopy(pending) => pending.release(),
        }
    }
}

/// One non-blocking pass over a capture queue: dispatch what arrived, then read once.
///
/// The loop's own wait polls the same connection, so a pending capture is woken by the same
/// event that wakes everything else; this pass only moves bytes the compositor already sent.
fn step<State>(
    queue: &mut EventQueue<State>,
    connection: &Connection,
    state: &mut State,
) -> Result<(), String> {
    queue
        .dispatch_pending(state)
        .map_err(|e| format!("capture dispatch: {e}"))?;
    connection
        .flush()
        .map_err(|e| format!("capture flush: {e}"))?;
    if let Some(guard) = queue.prepare_read() {
        match guard.read() {
            Ok(_) => {}
            Err(WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("capture read: {error}")),
        }
    }
    // No guard means another queue is reading this turn; the next pass tries again.
    Ok(())
}

struct ExtPending {
    connection: Connection,
    queue: EventQueue<CaptureState>,
    state: CaptureState,
    shm: WlShm,
    session: Option<ExtImageCopyCaptureSessionV1>,
    source: Option<ExtImageCaptureSourceV1>,
    frame: Option<ExtPendingFrame>,
    deadline: Instant,
}

struct ExtPendingFrame {
    frame: ExtImageCopyCaptureFrameV1,
    buffer: WlBuffer,
    pixels: ShmBuffer,
    width: u32,
    height: u32,
    format: Format,
}

impl ExtPending {
    fn poll(&mut self) -> Option<Result<Frame, String>> {
        if Instant::now() >= self.deadline {
            return Some(Err("capture timed out".into()));
        }
        if let Err(error) = step(&mut self.queue, &self.connection, &mut self.state) {
            return Some(Err(error));
        }
        if self.state.stopped {
            return Some(Err("the compositor stopped the capture session".into()));
        }
        if self.frame.is_none() {
            if !self.state.constraints_done {
                return None;
            }
            return match self.start_frame() {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            };
        }
        let frame = self.frame.take()?;
        if let Some(reason) = self.state.failed {
            frame.release();
            return Some(Err(format!("the compositor failed the frame: {reason}")));
        }
        if !self.state.ready {
            self.frame = Some(frame);
            return None;
        }
        let ExtPendingFrame {
            frame: capture,
            buffer,
            pixels,
            width,
            height,
            format,
        } = frame;
        capture.destroy();
        buffer.destroy();
        Some(Ok(described(
            self.state.colour.as_ref(),
            self.state.output_name.as_ref(),
            self.state.scale,
            width,
            height,
            pixels.bytes().to_vec(),
            format,
        )))
    }

    /// The first round trip answered: attach a buffer and ask for the frame.
    fn start_frame(&mut self) -> Result<(), String> {
        let (width, height) = self
            .state
            .size
            .ok_or_else(|| "the session reported no size".to_string())?;
        let format = self
            .state
            .formats
            .iter()
            .copied()
            .find(|format| packable(*format))
            .ok_or_else(|| format!("no 32-bit format among {:?}", self.state.formats))?;
        let qh = self.queue.handle();
        let pixels =
            ShmBuffer::new(&self.shm, &qh, width, height).ok_or("cannot allocate a shm buffer")?;
        let buffer = pixels.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            pixels.stride as i32,
            format,
            &qh,
            (),
        );
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| "the capture session is gone".to_string())?;
        let frame = session.create_frame(&qh, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        let _ = self.connection.flush();
        self.frame = Some(ExtPendingFrame {
            frame,
            buffer,
            pixels,
            width,
            height,
            format,
        });
        Ok(())
    }

    fn release(&mut self) {
        if let Some(frame) = self.frame.take() {
            frame.release();
        }
        if let Some(session) = self.session.take() {
            session.destroy();
        }
        if let Some(source) = self.source.take() {
            source.destroy();
        }
    }
}

impl ExtPendingFrame {
    fn release(self) {
        self.frame.destroy();
        self.buffer.destroy();
    }
}

struct ScreencopyPending {
    connection: Connection,
    queue: EventQueue<ScreencopyState>,
    state: ScreencopyState,
    shm: WlShm,
    frame: Option<ZwlrScreencopyFrameV1>,
    copy: Option<ScreencopyCopy>,
    /// Version 3 announces every buffer type before the client may pick one.
    needs_done: bool,
    deadline: Instant,
}

struct ScreencopyCopy {
    buffer: ShmBuffer,
    pixels: WlBuffer,
    width: u32,
    height: u32,
    stride: u32,
    format: Format,
}

impl ScreencopyPending {
    fn poll(&mut self) -> Option<Result<Frame, String>> {
        if Instant::now() >= self.deadline {
            return Some(Err("capture timed out".into()));
        }
        if let Err(error) = step(&mut self.queue, &self.connection, &mut self.state) {
            return Some(Err(error));
        }
        if self.state.failed {
            return Some(Err("the compositor failed the copy".into()));
        }
        if self.copy.is_none() {
            // Version 3 announces the whole set before `buffer_done`; older ones always
            // offer the shm buffer and have no such event, so the buffer alone is the signal.
            let announced = if self.needs_done {
                self.state.buffer_done
            } else {
                self.state.buffer.is_some()
            };
            if !announced {
                return None;
            }
            return match self.start_copy() {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            };
        }
        if !self.state.ready {
            return None;
        }
        let copy = self.copy.take()?;
        copy.pixels.destroy();
        let pixels = rows(
            copy.buffer.bytes(),
            copy.width,
            copy.height,
            copy.stride,
            self.state.y_invert,
        );
        Some(Ok(described(
            self.state.colour.as_ref(),
            self.state.output_name.as_ref(),
            self.state.scale,
            copy.width,
            copy.height,
            pixels,
            copy.format,
        )))
    }

    fn start_copy(&mut self) -> Result<(), String> {
        let Some(raw) = self.state.buffer else {
            return Ok(());
        };
        let (format, width, height, stride) = raw;
        let format = match format {
            WEnum::Value(format) => format,
            WEnum::Unknown(raw) => {
                return Err(format!(
                    "the frame asked for unknown wl_shm format {raw:#x}"
                ));
            }
        };
        if !packable(format) {
            return Err(format!(
                "the frame asked for {format:?}, which we cannot read"
            ));
        }
        let qh = self.queue.handle();
        let buffer = ShmBuffer::new_with_stride(&self.shm, &qh, width, height, stride)
            .ok_or_else(|| format!("cannot allocate {width}x{height} at stride {stride}"))?;
        let pixels = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            &qh,
            (),
        );
        if let Some(frame) = &self.frame {
            frame.copy(&pixels);
        }
        let _ = self.connection.flush();
        self.copy = Some(ScreencopyCopy {
            buffer,
            pixels,
            width,
            height,
            stride,
            format,
        });
        Ok(())
    }

    fn release(&mut self) {
        self.copy = None;
        if let Some(frame) = self.frame.take() {
            frame.destroy();
        }
    }
}

/// Copy a captured buffer's rows into a tight, top-down image.
///
/// wl_shm rows are `stride` bytes apart and may be padded, and `y_invert` puts the bottom of
/// the screen first in memory. The buffer is ours, so it holds exactly `stride * height` bytes.
fn rows(bytes: &[u8], width: u32, height: u32, stride: u32, y_invert: bool) -> Vec<u8> {
    let row = width as usize * 4;
    let stride = stride as usize;
    let height = height as usize;
    if stride == row && !y_invert {
        return bytes[..row * height].to_vec();
    }
    let mut out = Vec::with_capacity(row * height);
    for index in 0..height {
        let source = if y_invert { height - 1 - index } else { index };
        let start = source * stride;
        out.extend_from_slice(&bytes[start..start + row]);
    }
    out
}

/// Bus name owning every portal object.
const DESKTOP_DEST: &str = "org.freedesktop.portal.Desktop";
/// Object path of the portal's desktop root.
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
/// Interface that takes screenshots.
const SCREENSHOT_IFACE: &str = "org.freedesktop.portal.Screenshot";
/// Interface of a portal request, which answers with `Response`.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// Whole-screen capture through `org.freedesktop.portal.Screenshot`.
///
/// The desktop takes the picture itself and hands back a file URI, so this rung needs no
/// Wayland protocol at all. It is the last resort: it cannot choose an output, and
/// `interactive: false` is a request, not a promise — a portal may still ask the user.
struct PortalCapture {
    connection: zbus::blocking::Connection,
}

impl PortalCapture {
    fn new(_link: &WaylandLink) -> Result<Option<Self>, String> {
        let Ok(connection) = zbus::blocking::Connection::session() else {
            return Ok(None);
        };
        // A machine without the portal, or one too old to have `Screenshot`, is the same
        // answer to us: this rung is unavailable.
        let Ok(portal) = screenshot_proxy(&connection) else {
            return Ok(None);
        };
        match portal.get_property::<u32>("version") {
            Ok(version) if version >= 1 => Ok(Some(Self { connection })),
            _ => Ok(None),
        }
    }

    fn grab(&self, timeout: Duration) -> Result<Frame, String> {
        // The token names the request object path; one grab per process id is enough.
        let token = format!("crabsnap{}", std::process::id());
        let path = request_path(&self.connection, &token);
        // Subscribe before the call: the answer may arrive before the call returns.
        let request = zbus::blocking::Proxy::new_owned(
            self.connection.clone(),
            DESKTOP_DEST,
            path.clone(),
            REQUEST_IFACE,
        )
        .map_err(|error| format!("portal request: {error}"))?;
        let signals = request
            .receive_signal("Response")
            .map_err(|error| format!("portal response stream: {error}"))?;

        let mut options: HashMap<String, OwnedValue> = HashMap::new();
        options.insert("handle_token".into(), owned_str(&token)?);
        // Our overlay owns the selection; a portal picker would be a second one.
        options.insert(
            "interactive".into(),
            Value::from(false)
                .try_to_owned()
                .map_err(|error| format!("portal options: {error}"))?,
        );
        // No parent: the capture happens before this process has a window.
        let handle: OwnedObjectPath = screenshot_proxy(&self.connection)?
            .call("Screenshot", &("", options))
            .map_err(|error| format!("portal Screenshot: {error}"))?;
        // The response is emitted on the returned handle. A portal that chose another one
        // (older than 0.9) will never answer the path we are listening on.
        if handle.as_str() != path {
            return Err(format!(
                "the portal answered on {} instead of {path}",
                handle.as_str()
            ));
        }

        let answer = match wait_response(signals, timeout) {
            Ok(answer) => answer,
            Err(error) => {
                // Nobody is waiting for this request any more; leaving it open would leave
                // the desktop's prompt on screen after we gave up.
                let _ = request.call_noreply("Close", &());
                return Err(error);
            }
        };
        let (code, results) = answer;
        if code == 1 {
            return Err("the user cancelled the portal screenshot".into());
        }
        if code != 0 {
            return Err(format!("the portal failed the screenshot: code {code}"));
        }
        let uri = results
            .get("uri")
            .and_then(|value| <&str>::try_from(value).ok())
            .ok_or_else(|| "the portal returned no screenshot uri".to_string())?;
        let path = file_uri_to_path(uri)?;
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        decode_png(&bytes)
    }
}

/// A proxy on the desktop root for the `Screenshot` interface.
fn screenshot_proxy(
    connection: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'static>, String> {
    zbus::blocking::Proxy::new(connection, DESKTOP_DEST, DESKTOP_PATH, SCREENSHOT_IFACE)
        .map_err(|error| format!("portal: {error}"))
}

/// The object path a portal request will live at for `token`.
fn request_path(connection: &zbus::blocking::Connection, token: &str) -> String {
    // The portal derives the path from our own bus name: the leading `:` goes and every `.`
    // becomes `_`.
    let unique = connection
        .unique_name()
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "{DESKTOP_PATH}/request/{}/{}",
        unique.trim_start_matches(':').replace('.', "_"),
        token
    )
}

/// An `OwnedValue` holding a string, for the vardicts the portal methods take.
fn owned_str(text: &str) -> Result<OwnedValue, String> {
    Value::from(text)
        .try_to_owned()
        .map_err(|error| format!("portal options: {error}"))
}

/// Read the request's `Response` signal, or give up at `timeout`.
///
/// The blocking iterator has no deadline of its own, so it waits on a thread while the caller
/// waits on the channel: a portal that never answers parks one thread instead of hanging us.
fn wait_response(
    mut signals: zbus::blocking::proxy::SignalIterator<'static>,
    timeout: Duration,
) -> Result<(u32, HashMap<String, OwnedValue>), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("portal-screenshot".into())
        .spawn(move || {
            let _ = sender.send(signals.next());
        })
        .map_err(|error| format!("portal reader: {error}"))?;
    let message = receiver
        .recv_timeout(timeout)
        .map_err(|_| "the portal did not answer in time".to_string())?
        .ok_or_else(|| "the portal closed the request".to_string())?;
    message
        .body()
        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
        .map_err(|error| format!("malformed portal response: {error}"))
}

/// The portal's PNG as the BGRA rows the protocol rungs produce.
fn decode_png(bytes: &[u8]) -> Result<Frame, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| format!("decode the portal screenshot: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut pixels = image.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Frame {
        width,
        height,
        stride: width * 4,
        pixels,
        format: Format::Argb8888,
        // A PNG is sRGB by definition, whatever the desktop it photographed was.
        colour: ui_core::colour::ContentColour::SDR,
        // The desktop names no output: the portal captures whatever it shows.
        output: None,
        // Nor does it say what a point was worth; a PNG pixel is taken as a point.
        scale: 1.0,
    })
}

/// The local path behind the portal's `file://` URI.
fn file_uri_to_path(uri: &str) -> Result<std::path::PathBuf, String> {
    let rest = uri
        .strip_prefix("file://")
        .ok_or_else(|| format!("the portal returned {uri:?}, not a file"))?;
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => return Err(format!("the portal returned {uri:?}, not a file path")),
    };
    if !host.is_empty() && host != "localhost" {
        return Err(format!("{uri:?} names host {host:?}, not this machine"));
    }
    let bytes = percent_decode(path)?;
    use std::os::unix::ffi::OsStringExt;
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_vec(
        bytes,
    )))
}

/// Undo the `%XX` escapes a file URI puts on bytes outside the URI alphabet.
fn percent_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let high = bytes.next().and_then(hex_digit);
        let low = bytes.next().and_then(hex_digit);
        match (high, low) {
            (Some(high), Some(low)) => out.push(high << 4 | low),
            _ => return Err(format!("{text:?} has a truncated percent escape")),
        }
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl Frame {
    /// The frame as a picture: the compositor's channel order, its own packing, its own
    /// description.
    pub fn into_image(self) -> Image {
        let mut data = self.pixels;
        match self.format {
            // The shm names 8-bit formats in BGRA order, which is not this engine's.
            Format::Argb8888 | Format::Xrgb8888 => {
                let opaque = self.format == Format::Xrgb8888;
                for pixel in data.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                    // `X` means the alpha byte is undefined, not "transparent".
                    if opaque {
                        pixel[3] = 0xff;
                    }
                }
            }
            // 10-bit words keep their layout; only the alpha bits are made opaque. A screen
            // capture has no transparency, and `Abgr`'s two bits are 0 in practice, which
            // would make the whole frame invisible.
            Format::Xbgr2101010 | Format::Abgr2101010 => {
                for word in data.as_chunks_mut::<4>().0 {
                    word[3] |= 0xc0;
                }
            }
            _ => {}
        }
        Image {
            width: self.width,
            height: self.height,
            data: std::sync::Arc::new(data),
            colour: self.colour,
            scale: self.scale,
        }
    }
}

/// A picture the engine can draw: straight alpha, top-down rows, packed as `colour` says.
///
/// One `Arc` serves both the texture upload and every later crop, so marking up a capture
/// never copies the screen.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: std::sync::Arc<Vec<u8>>,
    /// What the pixels mean; a renderer converts for the display it draws on.
    pub colour: ui_core::colour::ContentColour,
    /// Device pixels per logical point where the picture was taken. It says how large the
    /// picture was meant to be, not how to draw it: layout uses the surface's own scale.
    pub scale: f32,
}

impl Image {
    /// The device-pixel rectangle `rect` covers, rounded outwards, as `(x0, y0, x1, y1)`.
    ///
    /// `rect` is in logical points at `scale` device pixels each. This is the one rule that
    /// turns a selection into pixels: [`Image::crop`] takes its pixels from it, and a surface
    /// that draws marks over the selection builds its [`Space`] from it, so the two cannot
    /// disagree about where the first pixel is.
    pub fn crop_of(&self, rect: ui_core::geom::Rect, scale: f32) -> (u32, u32, u32, u32) {
        // Layout arithmetic is f32: an edge meant for 864 arrives as 864.00006, and a bare
        // `ceil` would add a row nobody selected. Snap to a thousandth of a pixel first.
        let snap = |value: f32| (value * 1000.0).round() / 1000.0;
        let x0 = (snap(rect.min_x() * scale).floor().max(0.0) as u32).min(self.width);
        let y0 = (snap(rect.min_y() * scale).floor().max(0.0) as u32).min(self.height);
        let x1 = (snap(rect.max_x() * scale).ceil().max(0.0) as u32).clamp(x0, self.width);
        let y1 = (snap(rect.max_y() * scale).ceil().max(0.0) as u32).clamp(y0, self.height);
        (x0, y0, x1, y1)
    }

    /// The pixels under `rect`, rounded outwards so nothing the rectangle touches is lost.
    ///
    /// `rect` is in the picture's own pixels and may be fractional. The crop is a smaller
    /// copy of the same picture, not a rendition: the packing, the description and the scale
    /// travel with it.
    pub fn crop(&self, rect: ui_core::geom::Rect) -> Cropped {
        // `rect` is already in device pixels, so the grid is that of a scale of one.
        let (x0, y0, x1, y1) = self.crop_of(rect, 1.0);
        let (width, height) = (x1 - x0, y1 - y0);
        // A whole-frame selection is the frame: a pin of the entire screen does not need a
        // second 33 MB copy of pixels that are never written again.
        if (x0, y0, width, height) == (0, 0, self.width, self.height) {
            return Cropped {
                origin: (0, 0),
                width,
                height,
                data: std::sync::Arc::clone(&self.data),
                colour: self.colour,
                scale: self.scale,
            };
        }
        // A crop is a smaller copy of the same picture, not a rendition: the packing and the
        // description travel with it, so a pin of an HDR selection stays HDR.
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for row in y0..y1 {
            let start = ((row * self.width + x0) * 4) as usize;
            let end = start + (width * 4) as usize;
            data.extend_from_slice(&self.data[start..end]);
        }
        Cropped {
            origin: (x0, y0),
            width,
            height,
            // Shared, not copied on: a pin's texture is this same buffer, and the crop is
            // never written after it is taken.
            data: std::sync::Arc::new(data),
            colour: self.colour,
            scale: self.scale,
        }
    }
}

/// The rendition a file gets: sRGB 8-bit, clipped at the SDR peak.
///
/// Which curve an export should use is the export design's question, not this one; clip is
/// what the tool has always produced, so a save looks the way it did before.
const SDR_FILE_DESTINATION: ui_core::colour::Destination = ui_core::colour::Destination {
    domain: ui_core::colour::Domain::Encoded,
    tone: ui_core::colour::ToneMapping::Clip,
};

/// A region of an [`Image`], in the picture's own packing.
pub struct Cropped {
    /// Where the region starts in the source's pixel grid: the corner the rectangle was
    /// rounded to, which is where these pixels actually came from.
    pub origin: (u32, u32),
    pub width: u32,
    pub height: u32,
    pub data: std::sync::Arc<Vec<u8>>,
    pub colour: ui_core::colour::ContentColour,
    /// What the picture's points were worth when it was cropped, for the file's density.
    pub scale: f32,
}

impl Cropped {
    /// The region as sRGB 8-bit RGBA: what every file format this tool writes takes.
    ///
    /// Borrowed when the picture is already SDR, so the common case copies nothing.
    pub fn rendition(&self) -> std::borrow::Cow<'_, [u8]> {
        let placement = self.colour.placement(SDR_FILE_DESTINATION);
        if placement.is_identity() {
            return std::borrow::Cow::Borrowed(self.data.as_slice());
        }
        let placement = placement.tabulated();
        let mut rgba = Vec::with_capacity(self.data.len());
        for texel in self.data.as_chunks::<4>().0 {
            let [r, g, b, alpha] = placement.texel(texel);
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[byte(r), byte(g), byte(b), byte(alpha)]);
        }
        std::borrow::Cow::Owned(rgba)
    }

    /// The region in the SDR rendition a file gets, as a picture marks can be drawn on.
    ///
    /// Every file this tool writes is sRGB 8-bit whatever the capture was (`rendition`), so a
    /// mark composites on that rendition; refusing instead would drop it from the file.
    pub fn sdr(&self) -> Cropped {
        Cropped {
            origin: self.origin,
            width: self.width,
            height: self.height,
            data: std::sync::Arc::new(self.rendition().into_owned()),
            colour: ui_core::colour::ContentColour::SDR,
            scale: self.scale,
        }
    }

    /// One texel as straight sRGB bytes, converted like `rendition`; `None` outside the
    /// region. What an eyedropper reads.
    pub fn texel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width as usize || y >= self.height as usize {
            return None;
        }
        let rgba = self.rendition();
        let index = (y * self.width as usize + x) * 4;
        let texel = rgba.get(index..index + 4)?;
        Some([texel[0], texel[1], texel[2], texel[3]])
    }
}

/// A logical point is 96 dpi, the web's own convention: a 2× capture says 192 dpi, which is
/// exactly the scale it was taken at, in a field every viewer understands.
pub(crate) const POINT_DPI: f32 = 96.0;

/// Decode PNG bytes into straight-alpha RGBA8, with the same crate this program writes them.
///
/// The ffmpeg program can only hand a picture back as a PNG, so reading that one goes
/// through a decoder we control instead of a second list of what ffmpeg can read.
pub(crate) fn read_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let capacity = reader
        .output_buffer_size()
        .ok_or_else(|| "the PNG is too large to decode".to_string())?;
    let mut pixels = vec![0; capacity];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    // The encoder on the other side was told to write RGBA8; anything else is a producer
    // that did not keep its promise, and converting here would hide that.
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "the PNG came back as {:?} {:?}, not 8-bit RGBA",
            frame.color_type, frame.bit_depth
        ));
    }
    pixels.truncate(frame.buffer_size());
    Ok((frame.width, frame.height, pixels))
}

/// What density a PNG says it was saved at, as the scale this app records; 1 when it says
/// nothing, or in a unit other than metres.
pub(crate) fn png_scale(bytes: &[u8]) -> f32 {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let Ok(reader) = decoder.read_info() else {
        return 1.0;
    };
    let Some(dims) = reader.info().pixel_dims else {
        return 1.0;
    };
    if dims.unit != png::Unit::Meter {
        return 1.0;
    }
    let dpi = dims.xppu as f32 * 0.0254;
    // A file claiming a density outside anything a screen has is not a scale to trust.
    if (24.0..=960.0).contains(&dpi) {
        dpi / POINT_DPI
    } else {
        1.0
    }
}

/// The frame those pixels are, described by whatever the compositor said about the output.
///
/// The pixels stay the compositor's own bytes: a 10-bit frame keeps its 10-bit words,
/// because the packing is not a colour and converting at ingest loses what cannot come back.
fn described(
    colour: Option<&crate::color::Description>,
    output: Option<&String>,
    scale: Option<f32>,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    format: Format,
) -> Frame {
    let storage = match format {
        Format::Xbgr2101010 | Format::Abgr2101010 => ui_core::colour::PixelStorage::Packed10,
        _ => ui_core::colour::PixelStorage::Sdr8,
    };
    let colour = match colour {
        Some(description) => description.content(storage),
        // With no description the codes are all there is; read them as sRGB.
        None => ui_core::colour::ContentColour {
            storage,
            ..ui_core::colour::ContentColour::SDR
        },
    };
    Frame {
        width,
        height,
        stride: width * 4,
        pixels,
        format,
        colour,
        output: output.cloned(),
        scale: scale.unwrap_or(1.0),
    }
}

/// Formats with the same 32-bit footprint as `Argb8888`.
fn packable(format: Format) -> bool {
    matches!(
        format,
        Format::Argb8888 | Format::Xrgb8888 | Format::Xbgr2101010 | Format::Abgr2101010
    )
}

/// Cursors are composited by the caller, not the compositor: the overlay draws its own.
fn capture_options() -> wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options
{
    use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options;
    Options::empty()
}

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &ExtImageCopyCaptureManagerV1,
        _: <ExtImageCopyCaptureManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &ExtOutputImageCaptureSourceManagerV1,
        _: <ExtOutputImageCaptureSourceManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCaptureSourceV1, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &ExtImageCaptureSourceV1,
        _: <ExtImageCaptureSourceV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: SessionEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            SessionEvent::BufferSize { width, height } => state.size = Some((width, height)),
            SessionEvent::ShmFormat { format } => {
                if let Ok(format) = format.into_result() {
                    state.formats.push(format);
                }
            }
            SessionEvent::Done => state.constraints_done = true,
            SessionEvent::Stopped => state.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event;
        match event {
            Event::Ready => state.ready = true,
            Event::Failed { reason } => {
                // An unknown code still means failed; the number only colours the message.
                state.failed = Some(match reason.into_result() {
                    Ok(reason) => reason as u32,
                    Err(_) => u32::MAX,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<WlShm, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: <WlShm as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: <WlShmPool as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: <WlOutput as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The name says where the shot came from; the scale says what one of its points was.
        match event {
            wayland_client::protocol::wl_output::Event::Name { name } => {
                state.output_name = Some(name);
            }
            wayland_client::protocol::wl_output::Event::Scale { factor } => {
                state.scale = Some(factor as f32);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for ScreencopyState {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: <ZwlrScreencopyManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for ScreencopyState {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: ScreencopyEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ScreencopyEvent::Buffer {
                format,
                width,
                height,
                stride,
            } => state.buffer = Some((format, width, height, stride)),
            ScreencopyEvent::BufferDone => state.buffer_done = true,
            ScreencopyEvent::Flags { flags } => {
                state.y_invert = match flags {
                    WEnum::Value(flags) => flags.contains(ScreencopyFlags::YInvert),
                    WEnum::Unknown(bits) => bits & 1 != 0,
                };
            }
            ScreencopyEvent::Ready { .. } => state.ready = true,
            ScreencopyEvent::Failed => state.failed = true,
            _ => {}
        }
    }
}

impl Dispatch<WlShm, ()> for ScreencopyState {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: <WlShm as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for ScreencopyState {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: <WlShmPool as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for ScreencopyState {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for ScreencopyState {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: <WlOutput as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The name says where the shot came from; the scale says what one of its points was.
        match event {
            wayland_client::protocol::wl_output::Event::Name { name } => {
                state.output_name = Some(name);
            }
            wayland_client::protocol::wl_output::Event::Scale { factor } => {
                state.scale = Some(factor as f32);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    /// A selection is rounded outwards onto the pixel grid: the user picked the pixels the
    /// rectangle covers, not the ones its corners land inside.
    #[test]
    fn a_crop_covers_every_pixel_the_selection_touches() {
        use ui_core::geom::Rect;
        let source = image(4, 2);
        let cropped = source.crop(Rect::new(1.2, 0.4, 1.5, 1.2));
        assert_eq!((cropped.width, cropped.height), (2, 2));
        // The selection is where the pointer went; the region is where the pixels are. A pin
        // placed at the former sits a fraction of a pixel off and resamples every frame.
        assert_eq!(cropped.origin, (1, 0));
        // Row-major from (1, 0): source indices 1, 2, 5, 6.
        assert_eq!(
            cropped.data.as_slice(),
            vec![1, 0, 0, 255, 2, 0, 0, 255, 5, 0, 0, 255, 6, 0, 0, 255]
        );

        let narrow = source.crop(Rect::new(1.6, 0.0, 1.0, 1.0));
        assert_eq!(
            narrow.width, 2,
            "1.6..2.6 covers pixels 1 and 2, not only 2"
        );
        let noisy = source.crop(Rect::new(0.0, 0.0, 2.000_061, 1.000_061));
        assert_eq!(
            (noisy.width, noisy.height),
            (2, 1),
            "float noise below a thousandth of a pixel is not an extra row"
        );

        let outside = source.crop(Rect::new(-5.0, -5.0, 3.0, 3.0));
        assert_eq!(
            (outside.width, outside.height),
            (0, 0),
            "nothing on screen is empty"
        );
    }

    /// The selection a surface annotates and the crop it pins are the same pixels: both take
    /// their grid from `crop_of`, so a mark placed through a `Space` built from it lands on
    /// the exact device pixels the cropped picture carries. A second rounding rule anywhere
    /// would leave the annotation a fraction of a pixel off the picture it was drawn on.
    #[test]
    fn the_annotated_selection_and_its_crop_agree_on_where_the_pixels_start() {
        use ui_core::geom::{Point, Rect};
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

    /// A padded, bottom-up buffer becomes tight, top-down rows: the capture protocols differ
    /// exactly in these two ways, and both differences are silent when mishandled.
    #[test]
    fn a_padded_bottom_up_frame_becomes_tight_rows() {
        let row = |tag: u8| -> Vec<u8> {
            let mut row: Vec<u8> = (0..8).map(|i| tag + i).collect();
            row.extend_from_slice(&[0xaa; 4]);
            row
        };
        let mut bytes = row(10);
        bytes.extend_from_slice(&row(20));
        let out = rows(&bytes, 2, 2, 12, true);
        assert_eq!(out, (20..28).chain(10..18).collect::<Vec<u8>>());

        let tight: Vec<u8> = (0..16).collect();
        assert_eq!(rows(&tight, 2, 2, 8, false), tight);
    }

    /// The portal names its file as a URI, and the path behind it may carry percent escapes.
    #[test]
    fn a_file_uri_decodes_to_its_path() {
        assert_eq!(
            file_uri_to_path("file:///run/user/1000/Screenshot%20from%202026-11-09.png").unwrap(),
            std::path::PathBuf::from("/run/user/1000/Screenshot from 2026-11-09.png")
        );
        assert_eq!(
            file_uri_to_path("file://localhost/a%2Bb.png").unwrap(),
            std::path::PathBuf::from("/a+b.png")
        );
        assert!(
            file_uri_to_path("file://otherhost/a.png").is_err(),
            "another host's path is not ours"
        );
        assert!(
            percent_decode("%zz").is_err(),
            "a truncated escape is not a byte we can guess"
        );
    }

    /// The portal answers with a PNG; the frame it becomes must have the same BGRA layout as
    /// the protocol rungs, alpha included.
    #[test]
    fn a_portal_png_decodes_to_bgra_rows() {
        let source =
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        let mut png = Vec::new();
        source
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let frame = decode_png(&png).unwrap();
        assert_eq!((frame.width, frame.height, frame.stride), (2, 1, 8));
        assert_eq!(frame.format, Format::Argb8888);
        assert_eq!(frame.pixels, vec![0, 0, 255, 255, 255, 0, 0, 128]);
    }

    /// A 10-bit frame keeps the compositor's words; only the undefined alpha becomes opaque.
    #[test]
    fn a_ten_bit_frame_keeps_its_words_and_gains_opaque_alpha() {
        let red = 0x3ffu32;
        let mid = 512u32 << 10;
        let bytes: Vec<u8> = [red, mid]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let frame = Frame {
            width: 2,
            height: 1,
            stride: 8,
            pixels: bytes,
            format: Format::Xbgr2101010,
            colour: ui_core::colour::ContentColour {
                storage: ui_core::colour::PixelStorage::Packed10,
                ..ui_core::colour::ContentColour::SDR
            },
            output: None,
            scale: 1.0,
        };
        let image = frame.into_image();
        let (codes, alpha) = ui_core::colour::PixelStorage::Packed10.unpack(&image.data[0..4]);
        assert_eq!(
            (codes[0], codes[1], codes[2]),
            (1.0, 0.0, 0.0),
            "red is bits 0-9"
        );
        assert_eq!(alpha, 1.0, "an X format is opaque, whatever its bits said");
        let (codes, _) = ui_core::colour::PixelStorage::Packed10.unpack(&image.data[4..8]);
        assert_eq!(codes[1], 512.0 / 1023.0, "green is bits 10-19");
        assert_eq!(
            image.colour.storage,
            ui_core::colour::PixelStorage::Packed10
        );
    }
}
