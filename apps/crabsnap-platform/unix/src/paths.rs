//! Where a capture goes and what it is called on a Unix desktop.
//!
//! The pictures directory is the user's answer, not ours: the environment variable, then the
//! renamed entry in `user-dirs.dirs` (often in another language), then `~/Pictures`. Keeping
//! shots in `Screenshots/` under it is the desktop convention, and it makes "delete the
//! screenshots" one directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use crabsnap_core::paths::now_secs;

/// The directory captures belong in: the pictures directory with `Screenshots/` under it.
pub fn picture_dir() -> Option<PathBuf> {
    user_dir("PICTURES", "Pictures").map(|dir| dir.join("Screenshots"))
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

/// The configuration file: `XDG_CONFIG_HOME`, else `~/.config`, else temporary.
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    crabsnap_core::paths::config_path(&base)
}

/// A screenshot's file name, without a directory.
pub fn shot_name(extension: &str, unix_secs: i64, output: Option<&str>) -> String {
    crabsnap_core::paths::shot_name(extension, &stamp(unix_secs), output)
}

/// `YYYYMMDD-HHMMSS` in local time.
pub fn stamp(unix_secs: i64) -> String {
    let local = local_secs(unix_secs);
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (year, month, day) = crabsnap_core::paths::civil_from_days(days);
    crabsnap_core::paths::stamp_from(
        year,
        month,
        day,
        (secs / 3600) as u32,
        ((secs % 3600) / 60) as u32,
        (secs % 60) as u32,
    )
}

/// `unix_secs` shifted into local time. No calendar library: `localtime_r` already knows the
/// offset for that instant, including daylight saving.
fn local_secs(unix_secs: i64) -> i64 {
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    let instant = unix_secs as libc::time_t;
    // SAFETY: `localtime_r` writes only into the `tm` we own, and the pointer is valid.
    let converted = unsafe { !libc::localtime_r(&instant, &mut broken_down).is_null() };
    if !converted {
        return unix_secs;
    }
    unix_secs + broken_down.tm_gmtoff
}

/// Resolve one user directory: `$XDG_<KEY>_DIR`, then `user-dirs.dirs`, then `$HOME/<fallback>`.
fn user_dir_from(
    xdg: Option<&str>,
    dirs: Option<&str>,
    home: Option<&str>,
    key: &str,
    fallback: &str,
) -> Option<PathBuf> {
    if let Some(dir) = xdg.filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let name = format!("XDG_{key}_DIR");
    if let Some(path) = dirs.and_then(|text| parse_user_dirs(text).remove(&name))
        && let Some(expanded) = expand_home(&path, home)
    {
        return Some(expanded);
    }
    home.filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(fallback))
}

fn user_dir(key: &str, fallback: &str) -> Option<PathBuf> {
    let xdg = std::env::var(format!("XDG_{key}_DIR")).ok();
    let dirs = std::fs::read_to_string(user_dirs_path()?).ok();
    let home = std::env::var("HOME").ok();
    user_dir_from(
        xdg.as_deref(),
        dirs.as_deref(),
        home.as_deref(),
        key,
        fallback,
    )
}

fn user_dirs_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("user-dirs.dirs"))
}

/// The `KEY="value"` lines of `user-dirs.dirs`.
///
/// Comments and lines that do not parse are skipped: the file is optional external input,
/// and one malformed line must not take the whole default path down with it.
fn parse_user_dirs(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        out.insert(
            key.trim().to_string(),
            value.trim().trim_matches('"').to_string(),
        );
    }
    out
}

/// Expand a leading `$HOME`, which `user-dirs.dirs` writes by convention.
fn expand_home(path: &str, home: Option<&str>) -> Option<PathBuf> {
    if let Some(rest) = path.strip_prefix("$HOME") {
        let home = home.filter(|home| !home.is_empty())?;
        return Some(PathBuf::from(home).join(rest.trim_start_matches('/')));
    }
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three rungs of the fallback chain, and the one input that makes the chain give up.
    #[test]
    fn a_user_directory_falls_back_from_environment_to_config_to_home() {
        let resolve = |xdg, dirs, home| user_dir_from(xdg, dirs, home, "PICTURES", "Pictures");
        let config = "XDG_PICTURES_DIR=\"$HOME/图\"\nXDG_VIDEOS_DIR=\"$HOME/Videos\"\n";
        assert_eq!(
            resolve(Some("/shots"), Some(config), Some("/home/u")),
            Some(PathBuf::from("/shots")),
            "the environment is the user's explicit answer and wins"
        );
        assert_eq!(
            resolve(Some(""), Some(config), Some("/home/u")),
            Some(PathBuf::from("/home/u/图")),
            "an empty variable is not an answer; the config's renamed directory is"
        );
        assert_eq!(
            resolve(None, Some("# a comment\nbroken line\n"), Some("/home/u")),
            Some(PathBuf::from("/home/u/Pictures")),
            "a config with no answer falls through to home"
        );
        assert_eq!(
            resolve(None, None, None),
            None,
            "without a home there is nowhere to put the file"
        );
    }
}
