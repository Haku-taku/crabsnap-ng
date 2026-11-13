//! Where a capture goes and what it is called.
//!
//! The policy is the same on every desktop: keep shots in a directory of their own, name them
//! so two captures in one second stay apart, and never let an output's name leave the
//! directory the file is joined to. Which directory is the pictures one, and what the clock
//! reads in local time, are the platform's answers and arrive as arguments.

use std::path::{Path, PathBuf};

/// Where a capture goes: the configured directory, else the pictures one, else temporary.
pub fn shot_dir(
    configured: Option<&Path>,
    pictures: Option<PathBuf>,
    temporary: PathBuf,
) -> PathBuf {
    configured
        .map(Path::to_path_buf)
        .or(pictures)
        .unwrap_or(temporary)
}

/// Where the configuration lives under `base`, which the platform answers for.
pub fn config_path(base: &Path) -> PathBuf {
    base.join("crabsnap").join("config.toml")
}

/// Where a process's log lives under `base`: local, not roaming, because it describes this
/// machine's run and is not worth carrying to another one.
pub fn log_dir(base: &Path) -> PathBuf {
    base.join("crabsnap").join("logs")
}

/// A screenshot's file name, without a directory.
///
/// `output` is the screen the picture came from: with several outputs, two shots in the same
/// second would otherwise be indistinguishable. Whatever the platform calls the output is
/// reduced to `[A-Za-z0-9_-]`, so it cannot leave the directory it is joined to.
pub fn shot_name(extension: &str, stamp: &str, output: Option<&str>) -> String {
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

/// Seconds since the epoch, which the platform turns into a [`stamp_from`].
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// `YYYYMMDD-HHMMSS` from calendar fields already in local time: a file name is read by a
/// person, and UTC would file tonight's shots under yesterday for half the world.
pub fn stamp_from(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Days since the epoch to `(year, month, day)`: Howard Hinnant's `civil_from_days`, which
/// needs no calendar library and is exact for negative days too.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

    /// A file name says which screen the shot came from, and a compositor's name cannot
    /// leave the directory the name is joined to.
    #[test]
    fn a_shot_name_carries_the_output_inside_its_directory() {
        assert_eq!(
            shot_name("png", "20240101-120000", Some("DP-6")),
            "Screenshot-DP-6-20240101-120000.png"
        );
        assert_eq!(
            shot_name("png", "20240101-120000", None),
            "Screenshot-20240101-120000.png",
            "a capture that names no output keeps the old name"
        );
        assert_eq!(
            shot_name("png", "20240101-120000", Some("")),
            "Screenshot-20240101-120000.png",
            "an empty name is no name"
        );
        let hostile = shot_name("png", "20240101-120000", Some("../a b/c"));
        assert!(
            !hostile.contains('/') && !hostile.contains(' '),
            "the name stays inside its directory: {hostile}"
        );
    }

    /// The chain gives up at temporary only when no other answer exists.
    #[test]
    fn a_shot_directory_falls_back_from_configured_to_pictures_to_temporary() {
        let temporary = PathBuf::from("tmp");
        let pictures = Some(PathBuf::from("pictures/Screenshots"));
        assert_eq!(
            shot_dir(Some(Path::new("shots")), pictures.clone(), temporary.clone()),
            PathBuf::from("shots")
        );
        assert_eq!(
            shot_dir(None, pictures, temporary.clone()),
            PathBuf::from("pictures/Screenshots")
        );
        assert_eq!(shot_dir(None, None, temporary.clone()), temporary);
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
