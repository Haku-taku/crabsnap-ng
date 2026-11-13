//! Starting the daemon at login: the `Run` value Windows reads for the signed-in user.
//!
//! A user's own startup entry needs no elevation and no shortcut file, and it is the one
//! place the shell, Task Manager and the startup settings all show: anything else would be a
//! second, invisible way to start the same process.

use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::PCWSTR;

/// Where Windows keeps the entries a login runs, for this user only.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The entry's name, which is what the startup settings list.
const VALUE: &str = "crabsnap";

/// Whether a login starts the daemon of this installation.
///
/// An entry that names another path belongs to an executable that has since moved, so it reads
/// as off; turning autostart on rewrites it to this one.
pub fn autostart_enabled() -> Result<bool, String> {
    Ok(match entry()? {
        Some(found) => found == launch_command()?,
        None => false,
    })
}

/// Write the login entry, or remove it.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let key = wide(RUN_KEY);
    let value = wide(VALUE);
    let result = unsafe {
        if enabled {
            let command = wide_bytes(&launch_command()?);
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key.as_ptr()),
                PCWSTR::from_raw(value.as_ptr()),
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                command.len() as u32,
            )
        } else {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key.as_ptr()),
                PCWSTR::from_raw(value.as_ptr()),
            )
        }
    };
    // Removing an entry that is not there already leaves the state asked for.
    let also_done = if enabled {
        ERROR_SUCCESS
    } else {
        ERROR_FILE_NOT_FOUND
    };
    if result == ERROR_SUCCESS || result == also_done {
        return Ok(());
    }
    let verb = if enabled { "written" } else { "removed" };
    Err(format!(
        "the login entry could not be {verb}: {}",
        windows::core::Error::from(result)
    ))
}

/// The command a login runs: this executable, as the daemon.
///
/// The path is quoted because an installation directory may hold a space.
fn launch_command() -> Result<String, String> {
    let path = std::env::current_exe()
        .map_err(|error| format!("the executable's own path is unknown: {error}"))?;
    Ok(format!("\"{}\" daemon", path.display()))
}

/// What the login entry holds, when there is one.
fn entry() -> Result<Option<String>, String> {
    let key = wide(RUN_KEY);
    let value = wide(VALUE);
    let mut length = 0u32;
    let sized = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(key.as_ptr()),
            PCWSTR::from_raw(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut length),
        )
    };
    match sized {
        // No entry is a state, not a failure.
        ERROR_FILE_NOT_FOUND => return Ok(None),
        // The value exists and its size is now known; either code says just that.
        ERROR_SUCCESS | ERROR_MORE_DATA => {}
        failure => return Err(read_failure(failure)),
    }
    // A `REG_SZ` length is in bytes and counts the terminator.
    let mut buffer = vec![0u16; (length as usize).div_ceil(2).max(1)];
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(key.as_ptr()),
            PCWSTR::from_raw(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut length),
        )
    };
    if read != ERROR_SUCCESS {
        return Err(read_failure(read));
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(String::from_utf16_lossy(&buffer[..end])))
}

fn read_failure(code: WIN32_ERROR) -> String {
    format!(
        "the login entry could not be read: {}",
        windows::core::Error::from(code)
    )
}

/// A NUL-terminated UTF-16 string for a registry call.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The bytes `REG_SZ` stores: the string's UTF-16 code units and their terminator.
fn wide_bytes(text: &str) -> Vec<u8> {
    wide(text).into_iter().flat_map(u16::to_ne_bytes).collect()
}
