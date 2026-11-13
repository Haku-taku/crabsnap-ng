//! A GDI blit of the desktop: the rung that always answers.
//!
//! `BitBlt` from the screen's device context copies what the display shows, including layered
//! windows once `CAPTUREBLT` is set. It is 8-bit and cannot see an HDR desktop's highlights,
//! but it needs no Direct3D device, so it answers where the other rungs refuse.

use std::time::Duration;

use crabsnap_core::capture::{Backend, Frame, PixelFormat, Started};
use ui_core::colour::ContentColour;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC,
    HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};

pub(crate) struct Blit {
    monitor: super::Monitor,
}

impl Blit {
    /// `None` when this session has no monitor at `output`.
    pub(crate) fn new(output: usize) -> Result<Option<Self>, String> {
        Ok(Some(Self {
            monitor: super::monitor(output)?,
        }))
    }
}

impl Backend for Blit {
    fn name(&self) -> &'static str {
        "gdi-blit"
    }

    fn grab(&mut self, _timeout: Duration) -> Result<Frame, String> {
        let (width, height) = (self.monitor.width(), self.monitor.height());
        // The screen's context is the whole virtual desktop, so a secondary monitor's pixels
        // are the ones at its own corner of it.
        let origin = (self.monitor.rect.left, self.monitor.rect.top);
        // SAFETY: every handle the desktop owns is released when it drops.
        let desktop = unsafe { Desktop::open(width as i32, height as i32, origin)? };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        // SAFETY: the buffer is exactly the bitmap's size, packed BGRA, top-down.
        unsafe { desktop.copy(&mut pixels, width, height) }?;
        Ok(Frame {
            width,
            height,
            stride: width * 4,
            pixels,
            // A GDI copy has no alpha channel: the fourth byte is zero, not transparent.
            format: PixelFormat::Bgrx8,
            colour: ContentColour::SDR,
            output: Some(self.monitor.device.clone()),
            scale: self.monitor.scale,
        })
    }

    fn begin(self: Box<Self>, _timeout: Duration) -> Result<Started, String> {
        // A blit answers at once; there is nothing for the loop to poll.
        Ok(Started::Blocking(self))
    }
}

/// The screen's device context, a memory context to copy into, and the bitmap it selects.
struct Desktop {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    origin: (i32, i32),
}

impl Desktop {
    unsafe fn open(width: i32, height: i32, origin: (i32, i32)) -> Result<Self, String> {
        // SAFETY: no window handle means the whole screen; every failure releases what opened
        // before it.
        let screen = unsafe { GetDC(None) };
        if screen.is_invalid() {
            return Err("GetDC: the session has no screen device context".into());
        }
        let memory = unsafe { CreateCompatibleDC(Some(screen)) };
        if memory.is_invalid() {
            unsafe { ReleaseDC(None, screen) };
            return Err("CreateCompatibleDC: no memory device context".into());
        }
        let bitmap = unsafe { CreateCompatibleBitmap(screen, width, height) };
        if bitmap.is_invalid() {
            // The failure being reported is the bitmap's; a context that will not release
            // cannot change that answer.
            unsafe {
                let _ = DeleteDC(memory);
                let _ = ReleaseDC(None, screen);
            }
            return Err(format!(
                "CreateCompatibleBitmap: no {width}x{height} bitmap"
            ));
        }
        let previous = unsafe { SelectObject(memory, bitmap.into()) };
        Ok(Self {
            screen,
            memory,
            bitmap,
            previous,
            origin,
        })
    }

    /// Copy the desktop into `pixels` as top-down BGRA rows.
    unsafe fn copy(&self, pixels: &mut [u8], width: u32, height: u32) -> Result<(), String> {
        let (x, y) = self.origin;
        // SAFETY: both contexts are ours and the rectangle is the bitmap's own size.
        unsafe {
            BitBlt(
                self.memory,
                0,
                0,
                width as i32,
                height as i32,
                Some(self.screen),
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            )
        }
        .map_err(|error| format!("BitBlt: {error}"))?;
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // A negative height asks for top-down rows, the order every frame carries.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        // SAFETY: `pixels` holds width*height*4 bytes, which is what 32-bit rows need.
        let copied = unsafe {
            GetDIBits(
                self.memory,
                self.bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if copied == 0 {
            return Err("GetDIBits: the desktop copy could not be read back".into());
        }
        Ok(())
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // SAFETY: each handle is released once, in the reverse order of creation. A release
        // that fails here has no caller left to report to.
        unsafe {
            SelectObject(self.memory, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.memory);
            let _ = ReleaseDC(None, self.screen);
        }
    }
}
