//! The process's log: what a launch from the desktop would otherwise lose.
//!
//! A GUI-subsystem binary started by an icon has no console handle, so every word it prints
//! goes nowhere — a daemon crash included. Records go to stderr as well when the process has
//! one, and always to a file under the local application data, which a daemon left running
//! for weeks cannot fill because a full file is moved aside and a new one started.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Bytes a file may hold before the next record starts a new one.
const BUDGET: u64 = 1 << 20;

/// Generations kept beside the current file: `daemon.log.1` through `daemon.log.3`.
const KEPT: u32 = 3;

/// Start logging for `role` under `dir`, and report panics through the same file.
///
/// The first install wins: a process has one log, and a second call would only replace the
/// file the first is writing.
pub fn install(role: &str, dir: &Path) {
    let logger = FileLog {
        path: dir.join(format!("{role}.log")),
        current: Mutex::new(None),
    };
    if log::set_boxed_logger(Box::new(logger)).is_err() {
        return;
    }
    log::set_max_level(LevelFilter::Info);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The default hook writes to stderr, which a desktop launch does not have.
        log::error!("panic: {info}");
        previous(info);
    }));
}

struct FileLog {
    path: PathBuf,
    /// Opened with the first record: a command that reports nothing leaves no file behind.
    current: Mutex<Option<Current>>,
}

/// The file records are appended to, and how full it is.
struct Current {
    path: PathBuf,
    /// `None` once writing failed: the disk refused the file and will refuse it again.
    file: Option<File>,
    written: u64,
}

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {:<5} {}\n", stamp(), record.level(), record.args());
        // A process with no console has an invalid handle, which std discards on write.
        eprint!("{line}");
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        current
            .get_or_insert_with(|| Current::open(&self.path))
            .write(&line);
    }

    fn flush(&self) {}
}

impl Current {
    /// Open `path` for appending, creating its directory; a refusal leaves `file` empty,
    /// which keeps the records on stderr without failing anything.
    fn open(path: &Path) -> Self {
        let mut this = Self {
            path: path.to_path_buf(),
            file: None,
            written: 0,
        };
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("[log] cannot create {}: {error}", parent.display());
            return this;
        }
        this.written = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => this.file = Some(file),
            Err(error) => eprintln!("[log] cannot open {}: {error}", path.display()),
        }
        this
    }

    fn write(&mut self, line: &str) {
        if self.written >= BUDGET {
            self.rotate();
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        match file.write_all(line.as_bytes()) {
            Ok(()) => self.written += line.len() as u64,
            Err(error) => {
                // A log that cannot be written must not take the failing path with it.
                eprintln!("[log] {}: {error}", self.path.display());
                self.file = None;
            }
        }
    }

    /// Move the full file aside and start a new one.
    ///
    /// Windows will not rename a file that is open, so this closes it first; a file that
    /// cannot be started leaves records on stderr alone rather than failing anything.
    fn rotate(&mut self) {
        let path = self.path.clone();
        self.file = None;
        shift_generations(&path);
        self.written = 0;
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => self.file = Some(file),
            Err(error) => eprintln!("[log] cannot start {}: {error}", path.display()),
        }
    }
}

/// `daemon.log` becomes `daemon.log.1`, and every older generation moves up one step.
fn shift_generations(path: &Path) {
    for generation in (1..=KEPT).rev() {
        let from = if generation == 1 {
            path.to_path_buf()
        } else {
            numbered(path, generation - 1)
        };
        // A generation that was never written is nothing to move.
        let _ = fs::rename(from, numbered(path, generation));
    }
}

fn numbered(path: &Path, generation: u32) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.{generation}"))
}

/// `YYYY-MM-DD hh:mm:ssZ`, read from the clock rather than a platform's local time: a log
/// line's order matters, and a UTC stamp stays comparable when a log is attached to a report.
fn stamp() -> String {
    let secs = crabsnap_core::paths::now_secs();
    let days = secs.div_euclid(86_400);
    let day = secs.rem_euclid(86_400);
    let (year, month, date) = crabsnap_core::paths::civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{date:02} {:02}:{:02}:{:02}Z",
        day / 3600,
        day % 3600 / 60,
        day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fill one file's budget with `mark`, so each generation is identifiable.
    fn fill(current: &mut Current, mark: char) {
        let mut line = String::with_capacity(BUDGET as usize + 1);
        line.push(mark);
        line.extend(std::iter::repeat_n('x', BUDGET as usize - 1));
        line.push('\n');
        current.write(&line);
    }

    /// A daemon left running must not grow one file: a full one moves aside, and only the
    /// newest few are kept. Windows refuses to rename an open file, so the order of the
    /// close and the shift is what this pins.
    #[test]
    fn a_full_log_moves_aside_and_keeps_three_generations() {
        let dir = std::env::temp_dir().join(format!("crabsnap-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("daemon.log");
        let mut current = Current::open(&path);

        for mark in ['a', 'b', 'c', 'd'] {
            fill(&mut current, mark);
        }

        let read = |file: PathBuf| fs::read_to_string(file).expect("the file was rotated");
        assert!(read(path.clone()).starts_with('d'), "the newest file is current");
        assert!(read(numbered(&path, 1)).starts_with('c'));
        assert!(read(numbered(&path, 2)).starts_with('b'));
        assert!(read(numbered(&path, 3)).starts_with('a'));
        assert!(!numbered(&path, 4).exists(), "only the kept generations exist");

        let _ = fs::remove_dir_all(&dir);
    }
}
