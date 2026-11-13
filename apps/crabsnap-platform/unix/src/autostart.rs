//! Starting the daemon at login: the desktop entry an XDG session launches at sign-in.
//!
//! `~/.config/autostart` is the freedesktop convention every Wayland session implements, and
//! the desktop's own startup settings list the entry, so the user can also turn it off there.

use std::fs;
use std::path::PathBuf;

use crate::paths;

/// The entry's file name under `autostart/`.
const ENTRY: &str = "crabsnap.desktop";

/// Whether a login starts the daemon of this installation.
///
/// An entry whose command is not this executable's belongs to one that has since moved, so it
/// reads as off; turning autostart on rewrites it to this one.
pub fn autostart_enabled() -> Result<bool, String> {
    let path = entry_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("{} could not be read: {error}", path.display())),
    };
    Ok(exec(&text).as_deref() == Some(launch_command()?.as_str()))
}

/// Write the login entry, or remove it.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = entry_path();
    if !enabled {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("{} could not be removed: {error}", path.display())),
        };
    }
    let command = launch_command()?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("{} could not be created: {error}", directory.display()))?;
    }
    fs::write(&path, entry(&command))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

fn entry_path() -> PathBuf {
    paths::config_base().join("autostart").join(ENTRY)
}

/// The entry's body. `Name` is what the desktop's startup settings list.
fn entry(command: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Crabsnap\n\
         Comment=Take a screenshot or pin a picture\n\
         Exec={command}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// The `Exec=` value of an entry file.
fn exec(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("Exec=").map(str::to_owned))
}

/// The command a login runs: this executable, as the daemon.
fn launch_command() -> Result<String, String> {
    let path = std::env::current_exe()
        .map_err(|error| format!("the executable's own path is unknown: {error}"))?;
    Ok(format!("{} daemon", quote(&path.to_string_lossy())))
}

/// Quote one `Exec` argument so a path holding a space stays one argument.
///
/// The four characters the entry format reserves are backslash-escaped, and a literal backslash
/// needs the escape doubled because the entry's own escape rule runs before the quoting rule.
fn quote(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('"');
    for character in argument.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' | '`' | '$' => {
                quoted.push('\\');
                quoted.push(character);
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}
