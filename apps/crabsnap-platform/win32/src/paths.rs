//! Where a capture goes and what it is called on Windows.
//!
//! The pictures directory is a known folder: the user may have moved it to another drive, so
//! it is asked for rather than built from the profile path. The local clock is the system's
//! time zone, which likewise cannot be derived from the offset-free epoch seconds.

use std::path::{Path, PathBuf};

use windows::core::GUID;
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Pictures, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT,
    SHGetKnownFolderPath,
};

pub use crabsnap_core::paths::now_secs;

/// The directory captures belong in: the pictures folder with `Screenshots/` under it.
pub fn picture_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Pictures).map(|pictures| pictures.join("Screenshots"))
}

/// Where a screenshot goes: the probe's override, else the configured directory, else the
/// pictures directory, else the temporary directory. A scripted run sets the override so it
/// never writes into the user's own folder.
pub fn shot_dir(configured: Option<&Path>) -> PathBuf {
    let override_dir = std::env::var_os("CRABSNAP_SAVE_DIR").map(PathBuf::from);
    crabsnap_core::paths::shot_dir(
        override_dir.as_deref().or(configured),
        picture_dir(),
        std::env::temp_dir(),
    )
}

/// The configuration file: the roaming profile, so a user's settings follow them to another
/// machine, with `%APPDATA%` as the answer when the known folder cannot be read.
pub fn config_path() -> PathBuf {
    let base = known_folder(&FOLDERID_RoamingAppData)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    crabsnap_core::paths::config_path(&base)
}

/// Where a process's log goes: the local profile, because a log describes this machine's run.
pub fn log_dir() -> PathBuf {
    let base = known_folder(&FOLDERID_LocalAppData)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    crabsnap_core::paths::log_dir(&base)
}

/// A screenshot's file name, without a directory.
pub fn shot_name(extension: &str, unix_secs: i64, output: Option<&str>) -> String {
    crabsnap_core::paths::shot_name(extension, &stamp(unix_secs), output)
}

/// `YYYYMMDD-HHMMSS` in local time.
pub fn stamp(unix_secs: i64) -> String {
    // A machine with no usable time zone still gets a name; it is filed in UTC.
    let local = local_time(unix_secs).unwrap_or_else(|| utc_system_time(unix_secs));
    crabsnap_core::paths::stamp_from(
        local.wYear as i64,
        local.wMonth as u32,
        local.wDay as u32,
        local.wHour as u32,
        local.wMinute as u32,
        local.wSecond as u32,
    )
}

fn local_time(unix_secs: i64) -> Option<SYSTEMTIME> {
    let utc = utc_system_time(unix_secs);
    let mut local = SYSTEMTIME::default();
    // SAFETY: both pointers are to `SYSTEMTIME`s this function owns.
    unsafe { SystemTimeToTzSpecificLocalTime(None, &utc, &mut local) }.ok()?;
    Some(local)
}

fn utc_system_time(unix_secs: i64) -> SYSTEMTIME {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    let (year, month, day) = crabsnap_core::paths::civil_from_days(days);
    SYSTEMTIME {
        // SYSTEMTIME counts from 1601; a clock outside its range is not a capture's clock.
        wYear: u16::try_from(year).unwrap_or(1601),
        wMonth: month as u16,
        wDayOfWeek: 0,
        wDay: day as u16,
        wHour: (secs / 3600) as u16,
        wMinute: ((secs % 3600) / 60) as u16,
        wSecond: (secs % 60) as u16,
        wMilliseconds: 0,
    }
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: the shell returns a NUL-terminated string it allocated for us.
    let text = unsafe { path.to_string() }.ok();
    // SAFETY: that string came from the shell's allocator, which `CoTaskMemFree` releases.
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    text.map(PathBuf::from)
}
