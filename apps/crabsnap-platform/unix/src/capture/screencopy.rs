//! `zwlr_screencopy_manager_v1`: what a compositor without the ext protocol offers.
//!
//! Version 3 announces every buffer type before the client may pick one, which is why the
//! wait has two shapes; older versions always offer the wl_shm buffer and have no such event.

use std::time::{Duration, Instant};

use crabsnap_core::capture::{Backend, Frame, PixelFormat, Started};
use ui_platform_wayland::{ShmBuffer, WaylandLink};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{
    Event as ScreencopyEvent, Flags as ScreencopyFlags, ZwlrScreencopyFrameV1,
};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

use super::{bind_output, described, pixel_format, pump, rows, step};
use crate::colour::Description;

#[derive(Default)]
struct State {
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
}

/// The bound rung.
pub(crate) struct Screencopy {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    manager: ZwlrScreencopyManagerV1,
    shm: WlShm,
    output: WlOutput,
}

impl Screencopy {
    /// Bind the wlroots protocol's globals on a queue of our own.
    pub(crate) fn new(link: &WaylandLink, output: usize) -> Result<Option<Self>, String> {
        let queue = link.new_queue::<State>();
        let qh = queue.handle();
        let globals = link.globals();
        let Ok(manager) = globals.bind::<ZwlrScreencopyManagerV1, State, ()>(&qh, 1..=3, ()) else {
            return Ok(None);
        };
        let shm = globals
            .bind::<WlShm, State, ()>(&qh, 1..=1, ())
            .map_err(|e| format!("wl_shm: {e}"))?;
        let output = bind_output(globals, &qh, output)?;
        let colour = Description::read(link, &output);
        Ok(Some(Self {
            connection: link.connection().clone(),
            queue,
            state: State {
                colour,
                ..State::default()
            },
            manager,
            shm,
            output,
        }))
    }

    fn copy_from(
        &mut self,
        frame: &ZwlrScreencopyFrameV1,
        qh: &QueueHandle<State>,
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
        let (wl_format, width, height, stride) = self
            .state
            .buffer
            .ok_or_else(|| "the frame offered no wl_shm buffer".to_string())?;
        let format = offered_format(wl_format)?;

        let buffer = ShmBuffer::new_with_stride(&self.shm, qh, width, height, stride)
            .ok_or_else(|| format!("cannot allocate {width}x{height} at stride {stride}"))?;
        let wl_buffer = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format.0,
            qh,
            (),
        );
        frame.copy(&wl_buffer);
        let _ = self.connection.flush();
        let waited = pump(&mut self.queue, &self.connection, &mut self.state, deadline, |state| {
            state.ready || state.failed
        });
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
            format.1,
        ))
    }
}

impl Backend for Screencopy {
    fn name(&self) -> &'static str {
        "wlr-screencopy"
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

    fn begin(self: Box<Self>, timeout: Duration) -> Result<Started, String> {
        let mut capture = *self;
        capture.state.reset();
        let deadline = Instant::now() + timeout;
        let qh = capture.queue.handle();
        let frame = capture.manager.capture_output(0, &capture.output, &qh, ());
        let _ = capture.connection.flush();
        Ok(Started::Pending(Box::new(Pending {
            connection: capture.connection,
            queue: capture.queue,
            state: capture.state,
            shm: capture.shm,
            frame: Some(frame),
            copy: None,
            needs_done: capture.manager.version() >= 3,
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
    frame: Option<ZwlrScreencopyFrameV1>,
    copy: Option<Copy>,
    /// Version 3 announces every buffer type before the client may pick one.
    needs_done: bool,
    deadline: Instant,
}

struct Copy {
    buffer: ShmBuffer,
    pixels: WlBuffer,
    width: u32,
    height: u32,
    stride: u32,
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

    fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl Pending {
    fn start_copy(&mut self) -> Result<(), String> {
        let Some(raw) = self.state.buffer else {
            return Ok(());
        };
        let (offered, width, height, stride) = raw;
        let (wl_format, format) = offered_format(offered)?;
        let qh = self.queue.handle();
        let buffer = ShmBuffer::new_with_stride(&self.shm, &qh, width, height, stride)
            .ok_or_else(|| format!("cannot allocate {width}x{height} at stride {stride}"))?;
        let pixels = buffer.pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_format,
            &qh,
            (),
        );
        if let Some(frame) = &self.frame {
            frame.copy(&pixels);
        }
        let _ = self.connection.flush();
        self.copy = Some(Copy {
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

impl Drop for Pending {
    fn drop(&mut self) {
        self.release();
    }
}

/// The wire format the frame offered, as the value a buffer is created with and the portable
/// name the frame carries.
fn offered_format(offered: WEnum<Format>) -> Result<(Format, PixelFormat), String> {
    let format = match offered {
        WEnum::Value(format) => format,
        // A compositor may hand out a fourcc that wl_shm does not name.
        WEnum::Unknown(raw) => {
            return Err(format!("the frame asked for unknown wl_shm format {raw:#x}"));
        }
    };
    let portable = pixel_format(format)
        .ok_or_else(|| format!("the frame asked for {format:?}, which we cannot read"))?;
    Ok((format, portable))
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for State {
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
