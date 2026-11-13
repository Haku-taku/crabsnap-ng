//! What the app asks of a Windows session: the daemon channel, the user's directories and
//! clock, and the names of the capture backends this build can be told to use.
//!
//! The crate is empty off Windows: a named pipe, a monitor enumeration and a DXGI grab cannot
//! exist without those calls, so a Unix workspace builds it empty.

#![cfg(windows)]

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetConsoleCP, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
};
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

/// 把从终端启动的进程接回父进程的控制台，并回答它现在有没有一个。
///
/// A GUI-subsystem binary owns no console, and a shell hands a GUI child no handles either, so
/// `GetStdHandle` answers nothing and `--help` has nowhere to print. Attaching is what gives
/// the process a console; reopening `CONOUT$` is what gives `println!` a stream, because the
/// standard slots were empty and a console handle has to be named. A desktop start has no
/// parent console to attach to and answers false.
pub fn attach_terminal() -> bool {
    // Fails when the process already has a console, which is not a failure here.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    // The window handle cannot answer this: a pseudoconsole — Windows Terminal, and every
    // terminal that is not conhost's own window — has none. The code page is the console
    // itself, and a process attached to none answers zero.
    if unsafe { GetConsoleCP() } == 0 {
        return false;
    }
    reopen(STD_OUTPUT_HANDLE, "CONOUT$");
    reopen(STD_ERROR_HANDLE, "CONOUT$");
    reopen(STD_INPUT_HANDLE, "CONIN$");
    true
}

/// Point one standard slot at the console, unless a parent already put something there.
fn reopen(which: STD_HANDLE, name: &str) {
    if unsafe { GetStdHandle(which) }.is_ok_and(|handle| !handle.is_invalid()) {
        return;
    }
    let name = wide(name);
    let access = GENERIC_READ.0 | GENERIC_WRITE.0;
    let opened = unsafe {
        CreateFileW(
            PCWSTR::from_raw(name.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    if let Ok(handle) = opened {
        let _ = unsafe { SetStdHandle(which, handle) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
