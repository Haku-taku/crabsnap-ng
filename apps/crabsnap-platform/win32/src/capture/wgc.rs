//! Windows Graphics Capture: the compositor's own surface, one output at a time.
//!
//! The engine's capture session photographs the display without holding it exclusively and
//! without waiting for the desktop to change, so one grab opens its own session. An
//! advanced-colour display is captured in half floats, which travel with the frame as the
//! picture's own light, exactly as a duplication's do.

use std::time::Duration;

use crabsnap_core::capture::{Backend, Frame, Started};
use ui_platform_win32::GraphicsCapture;

pub(crate) struct Wgc {
    /// The output's GDI device name, which is how the engine's capture names one.
    device: String,
}

impl Wgc {
    /// `None` when this Windows has no capture sessions or no monitor at `output`.
    pub(crate) fn new(output: usize) -> Result<Option<Self>, String> {
        if !GraphicsCapture::available() {
            return Ok(None);
        }
        let monitor = super::monitor(output)?;
        Ok(Some(Self {
            device: monitor.device,
        }))
    }
}

impl Backend for Wgc {
    fn name(&self) -> &'static str {
        "windows-graphics-capture"
    }

    fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        let mut capture = GraphicsCapture::open(Some(&self.device))?;
        Ok(super::frame_of(capture.grab(timeout)?))
    }

    fn begin(self: Box<Self>, _timeout: Duration) -> Result<Started, String> {
        // The session answers when the compositor hands a frame over, which is not something
        // the caller's loop can poll; it waits in `grab`, as a duplication does.
        Ok(Started::Blocking(self))
    }
}
