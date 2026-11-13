//! DXGI Desktop Duplication: the composed desktop, HDR included.
//!
//! The engine's own duplication turns the desktop's float surface into sRGB codes against the
//! display's SDR white, which is where an HDR frame's highlights survive. It also holds the
//! output exclusively and reports a new frame only when the desktop changes, so one grab opens
//! its own duplication: the frame a fresh one delivers is the desktop as it is now, measured
//! on a static desktop at 2560x1440 in 157 ms.

use std::time::Duration;

use crabsnap_core::capture::{Backend, Frame, PixelFormat, Started};
use ui_platform_win32::ScreenCapture;

pub(crate) struct Duplication {
    /// The output's GDI device name, which is how the engine's duplication names one.
    device: String,
}

impl Duplication {
    /// `None` when this session has no monitor at `output`.
    pub(crate) fn new(output: usize) -> Result<Option<Self>, String> {
        let monitor = super::monitor(output)?;
        Ok(Some(Self {
            device: monitor.device,
        }))
    }
}

impl Backend for Duplication {
    fn name(&self) -> &'static str {
        "dxgi-desktop-duplication"
    }

    fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        let mut capture = ScreenCapture::open(Some(&self.device))?;
        let frame = capture.grab(timeout)?;
        Ok(Frame {
            width: frame.width,
            height: frame.height,
            stride: frame.width * 4,
            pixels: frame.pixels,
            format: PixelFormat::Rgba8,
            colour: frame.colour,
            output: Some(frame.output),
            scale: frame.scale,
        })
    }

    fn begin(self: Box<Self>, _timeout: Duration) -> Result<Started, String> {
        // Nothing to poll: the duplication answers when it answers, and the caller's own wait
        // is the only thing that can bound it.
        Ok(Started::Blocking(self))
    }
}
