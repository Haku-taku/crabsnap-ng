//! Screen capture on Windows, one rung per file.
//!
//! Each rung answers the app's `Backend` trait, so the caller holds one `Capture` whatever
//! produced the pixels. A Windows Graphics Capture session neither holds the output
//! exclusively nor waits for the desktop to change, so it leads the chain. Desktop Duplication
//! hands over the composed desktop with one device already made. A GDI blit is the last
//! resort: it is 8-bit, it needs no device, and it answers on a session where the others
//! refuse.

pub mod duplication;
pub mod gdi;
pub mod wgc;

use crabsnap_core::capture::{Capture, Frame, PixelFormat};
use ui_core::colour::PixelStorage;
use ui_platform_win32::ScreenFrame;
use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::BOOL;

/// The display connection a grab borrows from the session that opened it.
///
/// Windows gives every rung its own device and session, so there is nothing to borrow and this
/// carries no state; it exists so a grab is opened the same way on both platforms.
pub struct Session;

impl Session {
    /// Nothing to take from the backend; the rungs find their own device.
    pub fn of(_backend: &ui_platform_win32::Win32Backend) -> Result<Self, String> {
        Ok(Self)
    }
}

/// Bind the first capture rung this session offers.
///
/// `pinned` names one rung (`wgc`, `duplication`, `gdi`), so the fallback chain can be
/// exercised on a machine that offers more than one; `None` and `auto` walk the chain in order.
pub fn open(
    _session: &Session,
    pinned: Option<&str>,
    output: usize,
) -> Result<Option<Capture>, String> {
    let pinned = pinned.unwrap_or_default();
    let allowed = |name: &str| matches!(pinned, "" | "auto") || pinned == name;
    if !matches!(pinned, "" | "auto" | "wgc" | "duplication" | "gdi") {
        return Err(format!(
            "unknown capture backend {pinned:?}; expected wgc, duplication or gdi"
        ));
    }
    if allowed("wgc")
        && let Some(rung) = wgc::Wgc::new(output)?
    {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    if allowed("duplication")
        && let Some(rung) = duplication::Duplication::new(output)?
    {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    if allowed("gdi")
        && let Some(rung) = gdi::Blit::new(output)?
    {
        return Ok(Some(Capture::new(Box::new(rung))));
    }
    Ok(None)
}

/// The app's frame for an engine grab: the packing follows the description the engine set.
pub(crate) fn frame_of(frame: ScreenFrame) -> Frame {
    let format = match frame.colour.storage {
        // A float desktop keeps its half floats: the light above white is the picture's.
        PixelStorage::Float16 => PixelFormat::Rgba16F,
        _ => PixelFormat::Rgba8,
    };
    Frame {
        width: frame.width,
        height: frame.height,
        stride: frame.stride(),
        pixels: frame.pixels,
        format,
        colour: frame.colour,
        reference_nits: frame.reference_nits,
        output: Some(frame.output),
        scale: frame.scale,
    }
}

/// One monitor and what the app calls it.
pub(crate) struct Monitor {
    /// The monitor's area in virtual-screen coordinates.
    pub(crate) rect: RECT,
    /// The GDI device name, such as `\\.\DISPLAY1`.
    pub(crate) device: String,
    /// Device pixels per logical point: what a saved file's density says.
    pub(crate) scale: f32,
}

impl Monitor {
    pub(crate) fn width(&self) -> u32 {
        (self.rect.right - self.rect.left) as u32
    }

    pub(crate) fn height(&self) -> u32 {
        (self.rect.bottom - self.rect.top) as u32
    }
}

/// The monitor at `index`: the primary one first, then the rest as Windows lists them.
pub(crate) fn monitor(index: usize) -> Result<Monitor, String> {
    let mut found: Vec<(Monitor, bool)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(visit),
            LPARAM(&mut found as *mut _ as isize),
        )
        .ok()
        .map_err(|error| format!("EnumDisplayMonitors: {error}"))?;
    }
    // Index 0 is what every caller that does not name an output means, so the primary leads;
    // the sort is stable, so the enumeration order survives behind it.
    found.sort_by_key(|(_, primary)| !primary);
    let count = found.len();
    found
        .into_iter()
        .map(|(monitor, _)| monitor)
        .nth(index)
        .ok_or_else(|| {
            format!(
                "this session reports {count} monitor(s); there is no monitor {}",
                index + 1
            )
        })
}

/// The enumeration callback: describe one monitor into the list behind `data`.
unsafe extern "system" fn visit(handle: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `monitor` passes the address of a vector that outlives the enumeration.
    let found = unsafe { &mut *(data.0 as *mut Vec<(Monitor, bool)>) };
    if let Some(entry) = describe(handle) {
        found.push(entry);
    }
    TRUE
}

fn describe(handle: HMONITOR) -> Option<(Monitor, bool)> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let described =
        unsafe { GetMonitorInfoW(handle, std::ptr::from_mut(&mut info).cast::<MONITORINFO>()) };
    if !described.as_bool() {
        return None;
    }
    let end = info
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szDevice.len());
    let device = String::from_utf16_lossy(&info.szDevice[..end]);
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    // A monitor with no DPI answer is drawn at 100%, which is what the engine assumes too.
    let _ = unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
    Some((
        Monitor {
            rect: info.monitorInfo.rcMonitor,
            device,
            scale: dpi_x as f32 / 96.0,
        },
        primary,
    ))
}
