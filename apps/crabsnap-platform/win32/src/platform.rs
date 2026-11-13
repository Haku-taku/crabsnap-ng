//! What the app asks of a Windows session: the daemon channel, the user's directories and
//! clock, and the names of the capture backends this build can be told to use.
//!
//! The crate is empty off Windows: a named pipe, a monitor enumeration and a DXGI grab cannot
//! exist without those calls, so a Unix workspace builds it empty.

#![cfg(windows)]

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

pub mod capture;
pub mod channel;
pub mod paths;

pub use channel::{Client, ClientError, address};

/// The capture backends a Windows build offers, `auto` first.
///
/// A Windows Graphics Capture session holds nothing exclusively and answers on a still
/// desktop, so it leads; Desktop Duplication carries a float frame's highlights; a GDI blit
/// always answers but is 8-bit.
pub const CAPTURE_BACKENDS: &[&str] = &["auto", "wgc", "duplication", "gdi"];

/// 说一句终端之外也得让人看见的话：桌面图标启动的进程没有控制台，只能开一个消息框。
pub fn alert(message: &str) {
    let text = wide(message);
    let title = wide("Crabsnap");
    // SAFETY: both buffers are NUL-terminated and live until the call returns.
    unsafe {
        MessageBoxW(
            None,
            PCWSTR::from_raw(text.as_ptr()),
            PCWSTR::from_raw(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
