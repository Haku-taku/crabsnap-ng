//! Where a capture goes and what it is called.
//!
//! The decisions here are pure: the environment, the user-directories file and the clock are
//! arguments, so the fallback chain is testable without a home directory or a timezone set to
//! match the author's.

use std::path::PathBuf;

/// The directory captures belong in: the pictures directory with `Screenshots/` under it.
///
/// Keeping shots apart from the user's own pictures is the desktop convention, and it makes
/// "delete the screenshots" one directory.
pub fn picture_dir() -> Option<PathBuf> {
    user_dir("PICTURES", "Pictures").map(|dir| dir.join("Screenshots"))
}

/// Where a screenshot goes: the probe's override, else the configured directory, else the
/// pictures directory, else the temporary directory. A scripted run sets the override so it
/// never writes into the user's own folder.
pub fn shot_dir(configured: Option<&std::path::Path>) -> PathBuf {
    std::env::var_os("CRABSNAP_SAVE_DIR")
        .map(PathBuf::from)
        .or_else(|| configured.map(PathBuf::from))
        .or_else(picture_dir)
        .unwrap_or_else(std::env::temp_dir)
}

/// A screenshot's file name, without a directory.
///
/// `output` is the screen the picture came from: with several outputs, two shots in the same
/// second would otherwise be indistinguishable. Whatever the compositor calls the output is
/// reduced to `[A-Za-z0-9_-]`, so it cannot leave the directory it is joined to.
pub fn shot_name(extension: &str, unix_secs: i64, output: Option<&str>) -> String {
    let stamp = stamp(unix_secs);
    match output.filter(|name| !name.is_empty()) {
        Some(output) => format!("Screenshot-{}-{stamp}.{extension}", sanitized(output)),
        None => format!("Screenshot-{stamp}.{extension}"),
    }
}

/// What an output name may contribute to a file name.
fn sanitized(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect()
}

/// Seconds since the epoch, for [`shot_name`] and [`stamp`].
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// `YYYYMMDD-HHMMSS` in local time: a file name is read by a person, and UTC would file
/// tonight's shots under yesterday for half the world.
pub fn stamp(unix_secs: i64) -> String {
    let (year, month, day, hour, minute, second) = local_ymdhms(unix_secs);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Resolve one user directory: `$XDG_<KEY>_DIR`, then `user-dirs.dirs`, then `$HOME/<fallback>`.
///
/// The user-directories file records directories the user renamed (often in another
/// language), so it beats a hard-coded `Pictures`.
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
fn parse_user_dirs(text: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
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

fn local_ymdhms(unix_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let local = local_secs(unix_secs);
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
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

/// Days since the epoch to `(year, month, day)`: Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
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

    /// A file name says which screen the shot came from, and a compositor's name cannot
    /// leave the directory the name is joined to.
    #[test]
    fn a_shot_name_carries_the_output_inside_its_directory() {
        let seconds = 1_704_164_645;
        assert_eq!(
            shot_name("png", seconds, Some("DP-6")),
            format!("Screenshot-DP-6-{}.png", stamp(seconds))
        );
        assert_eq!(
            shot_name("png", seconds, None),
            format!("Screenshot-{}.png", stamp(seconds)),
            "a capture that names no output keeps the old name"
        );
        assert_eq!(
            shot_name("png", seconds, Some("")),
            format!("Screenshot-{}.png", stamp(seconds)),
            "an empty name is no name"
        );
        let hostile = shot_name("png", seconds, Some("../a b/c"));
        assert!(
            !hostile.contains('/') && !hostile.contains(' '),
            "the name stays inside its directory: {hostile}"
        );
    }

    /// The calendar arithmetic the file name depends on, at the two boundaries that break a
    /// naive implementation: the epoch and a leap day.
    #[test]
    fn days_convert_to_the_civil_calendar() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
