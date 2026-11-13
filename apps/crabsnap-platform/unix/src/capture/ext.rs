//! `ext-image-copy-capture-v1`: the standard path.
//!
//! A session reports its buffer constraints and a frame copies into a wl_shm buffer we
//! allocated, so the handshake is two round trips and the caller can wait in it or poll it.

use std::time::{Duration, Instant};

use crabsnap_core::capture::{Backend, Frame, PixelFormat, Started};
use ui_platform_wayland::{ShmBuffer, WaylandLink};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;
use wayland_protocols::ext::image_capture_source::v1::client::ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::{
    ExtImageCopyCaptureManagerV1, Options,
};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::{
    Event as SessionEvent, ExtImageCopyCaptureSessionV1,
};

use super::{bind_output, described, pixel_format, pump, step};
use crate::colour::Description;

#[derive(Default)]
struct State {
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
    /// The output's colour description, read when the rung was bound.
    colour: Option<Description>,
}

impl State {
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

    /// The first offered format this build can read, as the wire value and the portable name.
    fn pick_format(&self) -> Result<(Format, PixelFormat), String> {
        self.formats
            .iter()
            .copied()
            .find_map(|format| pixel_format(format).map(|portable| (format, portable)))
            .ok_or_else(|| format!("no 32-bit format among {:?}", self.formats))
    }
}

/// The bound rung.
pub(crate) struct Ext {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    manager: ExtImageCopyCaptureManagerV1,
    sources: ExtOutputImageCaptureSourceManagerV1,
    shm: WlShm,
    /// The output every grab captures: the overlay later picks per surface.
    output: WlOutput,
}

impl Ext {
    /// Bind the ext protocol's globals on a queue of our own.
    ///
    /// Absent globals mean this compositor does not offer the protocol; the chain moves to
    /// the next rung rather than treating it as a failure to start.
    pub(crate) fn new(link: &WaylandLink, output: usize) -> Result<Option<Self>, String> {
        let queue = link.new_queue::<State>();
        let qh = queue.handle();
        let globals = link.globals();
        let manager =
            match globals.bind::<ExtImageCopyCaptureManagerV1, State, ()>(&qh, 1..=1, ()) {
                Ok(manager) => manager,
                Err(_) => return Ok(None),
            };
        let sources = match globals.bind::<ExtOutputImageCaptureSourceManagerV1, State, ()>(
            &qh, 1..=1, (),
        ) {
            Ok(sources) => sources,
            Err(_) => return Ok(None),
        };
        let shm = globals
            .bind::<WlShm, State, ()>(&qh, 1..=1, ())
            .map_err(|e| format!("wl_shm: {e}"))?;
        let output = bind_output(globals, &qh, output)?;
        // The pixels this output hands out are described by the compositor, not by us; a
        // compositor without the protocol leaves the description absent.
        let colour = Description::read(link, &output);
        Ok(Some(Self {
            connection: link.connection().clone(),
            queue,
            state: State {
                colour,
                ..State::default()
            },
            manager,
            sources,
            shm,
            output,
        }))
    }

    fn grab_from(
        &mut self,
        session: &ExtImageCopyCaptureSessionV1,
        qh: &QueueHandle<State>,
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
        let (wl_format, format) = self.state.pick_format()?;

        let buffer = ShmBuffer::new(&self.shm, qh, width, height)
            .ok_or("cannot allocate a shm buffer")?;
        let wl_buffer = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            buffer.stride as i32,
            wl_format,
            qh,
            (),
        );

        let frame = session.create_frame(qh, ());
        frame.attach_buffer(&wl_buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        let _ = self.connection.flush();
        let waited = pump(&mut self.queue, &self.connection, &mut self.state, deadline, |state| {
            state.ready || state.failed.is_some()
        });
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

impl Backend for Ext {
    fn name(&self) -> &'static str {
        "ext-image-copy-capture-v1"
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

    fn begin(self: Box<Self>, timeout: Duration) -> Result<Started, String> {
        let mut ext = *self;
        ext.state.reset();
        let deadline = Instant::now() + timeout;
        let qh = ext.queue.handle();
        let source = ext.sources.create_source(&ext.output, &qh, ());
        let session = ext
            .manager
            .create_session(&source, capture_options(), &qh, ());
        let _ = ext.connection.flush();
        Ok(Started::Pending(Box::new(Pending {
            connection: ext.connection,
            queue: ext.queue,
            state: ext.state,
            shm: ext.shm,
            session: Some(session),
            source: Some(source),
            frame: None,
            deadline,
        })))
    }
}

/// A capture asked for and not yet ready.
struct Pending {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    shm: WlShm,
    session: Option<ExtImageCopyCaptureSessionV1>,
    source: Option<ExtImageCaptureSourceV1>,
    frame: Option<PendingFrame>,
    deadline: Instant,
}

struct PendingFrame {
    frame: ExtImageCopyCaptureFrameV1,
    buffer: WlBuffer,
    pixels: ShmBuffer,
    width: u32,
    height: u32,
    format: PixelFormat,
}

impl crabsnap_core::capture::Pending for Pending {
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
        let PendingFrame {
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

    fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl Pending {
    /// The first round trip answered: attach a buffer and ask for the frame.
    fn start_frame(&mut self) -> Result<(), String> {
        let (width, height) = self
            .state
            .size
            .ok_or_else(|| "the session reported no size".to_string())?;
        let (wl_format, format) = self.state.pick_format()?;
        let qh = self.queue.handle();
        let pixels =
            ShmBuffer::new(&self.shm, &qh, width, height).ok_or("cannot allocate a shm buffer")?;
        let buffer = pixels.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            pixels.stride as i32,
            wl_format,
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
        self.frame = Some(PendingFrame {
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

impl Drop for Pending {
    fn drop(&mut self) {
        self.release();
    }
}

impl PendingFrame {
    fn release(self) {
        self.frame.destroy();
        self.buffer.destroy();
    }
}

/// Cursors are composited by the caller, not the compositor: the overlay draws its own.
fn capture_options() -> Options {
    Options::empty()
}

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtImageCopyCaptureManagerV1,
        _: <ExtImageCopyCaptureManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtOutputImageCaptureSourceManagerV1,
        _: <ExtOutputImageCaptureSourceManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCaptureSourceV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtImageCaptureSourceV1,
        _: <ExtImageCaptureSourceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for State {
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

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for State {
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

impl Dispatch<WlShm, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: <WlShm as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: <WlShmPool as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: <WlOutput as Proxy>::Event,
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
