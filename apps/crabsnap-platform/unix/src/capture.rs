//! Output capture on Wayland, newest protocol first.
//!
//! Each rung lives in its own file and answers the same [`Backend`] trait, so the app holds
//! one [`Capture`] whatever produced the pixels. `ext-image-copy-capture-v1` is the standard
//! path; wlroots' `zwlr_screencopy_manager_v1` is what a compositor without it still offers,
//! and the desktop portal is the last resort for a compositor that offers neither. The tool
//! walks the chain rather than refusing to run.
//!
//! One grab is one handshake, and the wait is bounded: a compositor that never answers must
//! not hang the app. The portal rung differs in one respect: it captures the whole screen
//! through the desktop, which may ask the user for permission.

pub mod ext;
pub mod portal;
pub mod screencopy;

use std::time::{Duration, Instant};

use crabsnap_core::capture::{Capture, Frame, PixelFormat};
use ui_platform_wayland::{WaylandBackend, WaylandLink};
use wayland_client::backend::WaylandError;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_shm::Format;
use wayland_client::{Connection, EventQueue};

use crate::colour::Description;

/// The display connection a grab borrows from the session that opened it.
///
/// It has to be the loop's own: the loop waits on that connection, so the frame's arrival wakes
/// the same wait that drives the surfaces, where a connection of the grab's own would only be
/// read when the loop's next deadline expired.
#[derive(Clone)]
pub struct Session(WaylandLink);

impl Session {
    /// The connection of the backend an app opened through `ui_host::connect`.
    pub fn of(backend: &WaylandBackend) -> Result<Self, String> {
        Ok(Self(backend.link().clone()))
    }
}

/// Bind the first capture rung this compositor offers on `session`'s connection.
///
/// `pinned` names one rung (`ext`, `screencopy`, `portal`), so the fallback chain can be
/// exercised on a compositor that offers more than one; `None` walks the chain in order.
pub fn open(
    session: &Session,
    pinned: Option<&str>,
    output: usize,
) -> Result<Option<Capture>, String> {
    let link = &session.0;
    let pinned = pinned.unwrap_or_default();
    let ext = matches!(pinned, "" | "ext");
    let screencopy = matches!(pinned, "" | "screencopy");
    let portal = matches!(pinned, "" | "portal");
    if !ext && !screencopy && !portal {
        return Err(format!(
            "unknown capture backend {pinned:?}; expected ext, screencopy or portal"
        ));
    }
    if ext && let Some(rung) = ext::Ext::new(link, output)? {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    if screencopy && let Some(rung) = screencopy::Screencopy::new(link, output)? {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    if portal && let Some(rung) = portal::Portal::new()? {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    Ok(None)
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

/// The 32-bit wl_shm packings this build can read, as the portable name a frame carries.
///
/// Every one of them has the same footprint, so the buffer layout is identical and only the
/// channel order and the alpha bits differ.
fn pixel_format(format: Format) -> Option<PixelFormat> {
    Some(match format {
        Format::Argb8888 => PixelFormat::Bgra8,
        Format::Xrgb8888 => PixelFormat::Bgrx8,
        Format::Xbgr2101010 | Format::Abgr2101010 => PixelFormat::Bgr10,
        _ => return None,
    })
}

/// The frame those pixels are, described by whatever the compositor said about the output.
///
/// The pixels stay the compositor's own bytes: a 10-bit frame keeps its 10-bit words,
/// because the packing is not a colour and converting at ingest loses what cannot come back.
fn described(
    colour: Option<&Description>,
    output: Option<&String>,
    scale: Option<f32>,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    format: PixelFormat,
) -> Frame {
    let colour = match colour {
        Some(description) => description.content(format.storage()),
        // With no description the codes are all there is; read them as sRGB.
        None => ui_core::colour::ContentColour {
            storage: format.storage(),
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
