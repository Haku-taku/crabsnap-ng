//! A selection's payload, normalised into the two shapes a pin can take.
//!
//! The clipboard hands over bytes under a claimed MIME. A file list says "these are files",
//! whether it arrives as `text/uri-list` or as plain text that is nothing but `file:` URIs;
//! everything else is one blob. Both become materials downstream, so the reader (in the
//! platform layer) stays a plain pipe while this module owns the decision.

use std::path::PathBuf;

use crate::uri_list;

/// What a selection offers, as the app sees it. Platform backends normalise their own
/// clipboard forms into these two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// A file list: `text/uri-list`, `x-special/gnome-copied-files`, and their kin.
    Files(Vec<PathBuf>),
    /// One payload with a MIME.
    Bytes { mime: String, data: Vec<u8> },
}

impl Transfer {
    /// What `bytes`, claimed under `mime`, become.
    ///
    /// The MIME is a hint, never the decision: `image/png` bytes that start with `<svg` are
    /// still classified an SVG downstream, and a uri-list item that names a PNG is read as
    /// that file. This step only splits "paths" from "one blob".
    pub fn from_selection(mime: &str, bytes: Vec<u8>) -> Self {
        match mime {
            "text/uri-list" => Self::Files(uri_list::parse(&String::from_utf8_lossy(&bytes))),
            "x-special/gnome-copied-files" => {
                let (_, paths) = uri_list::parse_gnome(&String::from_utf8_lossy(&bytes));
                Self::Files(paths)
            }
            _ => {
                let text = String::from_utf8_lossy(&bytes);
                let files = file_uris(mime, &text);
                if !files.is_empty() {
                    return Self::Files(files);
                }
                Self::Bytes {
                    mime: mime.to_string(),
                    data: bytes,
                }
            }
        }
    }
}

/// The paths when `mime` is a text form and every line is a `file:` URI.
///
/// Some file managers and browsers offer a copied file only as plain text. Reading that text
/// as a pin would show the user a URI instead of the file, so a payload of nothing but file
/// URIs names files. Anything mixed with prose stays text: a sentence mentioning a path is
/// not a selection of one.
fn file_uris(mime: &str, text: &str) -> Vec<PathBuf> {
    let is_text = mime.starts_with("text/")
        || matches!(mime, "UTF8_STRING" | "STRING" | "TEXT");
    if !is_text {
        return Vec::new();
    }
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.is_empty() || !lines.iter().all(|line| line.starts_with("file:")) {
        return Vec::new();
    }
    uri_list::parse(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A uri-list is a list of files, a GNOME list the same lines with an action; either
    /// becomes one pin per path. Everything else is a single blob under its MIME.
    #[test]
    fn a_selection_splits_into_files_and_blobs() {
        let files = Transfer::from_selection(
            "text/uri-list",
            "file:///home/ollie/one.png\r\nfile:///home/ollie/two.png".into(),
        );
        assert_eq!(
            files,
            Transfer::Files(vec![
                PathBuf::from("/home/ollie/one.png"),
                PathBuf::from("/home/ollie/two.png"),
            ])
        );

        let gnome = Transfer::from_selection(
            "x-special/gnome-copied-files",
            "copy\nfile:///home/ollie/one.png".into(),
        );
        assert_eq!(gnome, Transfer::Files(vec![PathBuf::from("/home/ollie/one.png")]));

        let image = Transfer::from_selection("image/png", vec![1, 2, 3]);
        assert_eq!(
            image,
            Transfer::Bytes {
                mime: "image/png".to_string(),
                data: vec![1, 2, 3],
            }
        );

        // A claimed image whose bytes are an SVG is still one blob: the classifier, not the
        // MIME, decides what it is.
        let lying = Transfer::from_selection("image/png", b"<svg/>".to_vec());
        assert!(matches!(lying, Transfer::Bytes { .. }), "a MIME is a hint");
    }

    /// A file manager that offers only `text/plain` still hands a file over as its `file:`
    /// URI, and pinning the URI as a line of text is not what the user copied. Text that
    /// merely mentions a file is text.
    #[test]
    fn plain_text_that_is_only_file_uris_is_a_file_list() {
        let names = |mime: &str, text: &str| match Transfer::from_selection(mime, text.into()) {
            Transfer::Files(paths) => Some(paths),
            Transfer::Bytes { .. } => None,
        };

        assert_eq!(
            names("text/plain", "file:///home/ollie/one.png\n"),
            Some(vec![PathBuf::from("/home/ollie/one.png")])
        );
        assert_eq!(
            names(
                "text/plain;charset=utf-8",
                "file:///home/ollie/one.png\r\nfile:///home/ollie/two.png\r\n"
            ),
            Some(vec![
                PathBuf::from("/home/ollie/one.png"),
                PathBuf::from("/home/ollie/two.png"),
            ])
        );
        for text in [
            "open file:///home/ollie/one.png",
            "file:///home/ollie/one.png\nand a note about it",
            "https://example.com/one.png",
            "",
        ] {
            assert_eq!(names("text/plain", text), None, "{text:?} is text, not a list");
        }
    }
}
