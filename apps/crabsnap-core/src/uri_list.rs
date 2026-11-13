//! File lists from a clipboard, as paths.
//!
//! The Web convention is `text/uri-list`; GNOME prefixes the same lines with a `copy` or
//! `cut` line. Both are pure text, so this module is tested without a clipboard.

use std::path::PathBuf;

/// What GNOME's `x-special/gnome-copied-files` says the paste would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pasted {
    Copy,
    Cut,
}

/// The paths a `text/uri-list` payload names.
pub fn parse(text: &str) -> Vec<PathBuf> {
    paths(text.lines())
}

/// The action and paths of an `x-special/gnome-copied-files` payload.
pub fn parse_gnome(text: &str) -> (Option<Pasted>, Vec<PathBuf>) {
    let mut lines = text.lines();
    let action = match lines.clone().next().map(str::trim) {
        Some("copy") => Some(Pasted::Copy),
        Some("cut") => Some(Pasted::Cut),
        _ => None,
    };
    if action.is_some() {
        lines.next();
    }
    (action, paths(lines))
}

fn paths<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<PathBuf> {
    lines
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(line_path)
        .collect()
}

/// One line as a local path.
///
/// A `file:` URI with no host or `localhost` gives its decoded path. Anything else is kept
/// as the line reads — a foreign host or another scheme has no local path, and opening the
/// text fails and names it, which is better than silently reading a different file.
fn line_path(line: &str) -> Option<PathBuf> {
    let Some(rest) = line.strip_prefix("file:") else {
        return Some(PathBuf::from(line));
    };
    if let Some(after) = rest.strip_prefix("//") {
        let (host, path) = match after.find('/') {
            Some(at) => after.split_at(at),
            None => (after, ""),
        };
        if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
            let decoded = percent_decode(path)?;
            return (!decoded.is_empty()).then(|| PathBuf::from(decoded));
        }
        return Some(PathBuf::from(line));
    }
    // `file:/path` names the same file with no authority.
    percent_decode(rest).map(PathBuf::from)
}

/// The bytes a URI escapes as `%XX`; `None` when they are not UTF-8, which a path built
/// from text cannot carry.
fn percent_decode(text: &str) -> Option<String> {
    if !text.contains('%') {
        return Some(text.to_owned());
    }
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let [first, tail @ ..] = rest {
        if *first == b'%'
            && let [high, low, tail @ ..] = tail
            && let (Some(high), Some(low)) = (hex(*high), hex(*low))
        {
            bytes.push(high << 4 | low);
            rest = tail;
            continue;
        }
        bytes.push(*first);
        rest = tail;
    }
    String::from_utf8(bytes).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_list_parses_every_line_form() {
        let list = "# copied from a file manager\r\n\
                    file:///home/ollie/shot.png\r\n\
                    file://localhost/home/ollie/two%20words.txt\n\
                    \n\
                    /home/ollie/bare\n\
                    file:///C:/Users/ollie/kept.txt\n";
        assert_eq!(
            parse(list),
            vec![
                PathBuf::from("/home/ollie/shot.png"),
                PathBuf::from("/home/ollie/two words.txt"),
                PathBuf::from("/home/ollie/bare"),
                PathBuf::from("/C:/Users/ollie/kept.txt"),
            ]
        );
    }

    #[test]
    fn a_gnome_list_states_its_action_and_reuses_the_uri_lines() {
        let list = "copy\nfile:///home/ollie/one.png\nfile:///home/ollie/two.png";
        let (action, paths) = parse_gnome(list);
        assert_eq!(action, Some(Pasted::Copy));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/ollie/one.png"),
                PathBuf::from("/home/ollie/two.png"),
            ]
        );
        let (action, _) = parse_gnome("cut\nfile:///home/ollie/one.png");
        assert_eq!(action, Some(Pasted::Cut));
    }
}
