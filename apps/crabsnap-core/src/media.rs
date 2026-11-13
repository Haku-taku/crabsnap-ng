//! What a material is: a signature first, the name inside the text family, UTF-8 last.
//!
//! Portable on purpose: the classifier reads bytes and a name, and every kind-specific
//! decoder lives in the app. `docs/src/apps/pins.md` owns the table and the order.

use serde::{Deserialize, Serialize};

/// What a material is, in the terms the pin's factory dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Bitmap,
    Vector,
    Text(TextKind),
    Unsupported,
}

/// The kinds a material can have once it is known to be text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextKind {
    Plain,
    Markdown,
    Latex,
    /// Source in a language this build may know how to highlight.
    Code,
    Palette,
}

/// The verdict for one material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detected {
    pub kind: Kind,
    /// The MIME this build would call the material.
    pub mime: &'static str,
    /// The extension this build knows the name by, lowercased; `None` for an unknown one.
    pub name_hint: Option<&'static str>,
}

impl Kind {
    /// The tokens `--as` accepts, in the order help and errors list them.
    pub const NAMES: [&'static str; 7] = [
        "bitmap", "vector", "text", "markdown", "code", "latex", "palette",
    ];

    /// The token `pin list` prints and `--as` accepts.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bitmap => "bitmap",
            Self::Vector => "vector",
            Self::Text(TextKind::Plain) => "text",
            Self::Text(TextKind::Markdown) => "markdown",
            Self::Text(TextKind::Latex) => "latex",
            Self::Text(TextKind::Code) => "code",
            Self::Text(TextKind::Palette) => "palette",
            Self::Unsupported => "unsupported",
        }
    }

    /// The kind `token` names; `unsupported` is a verdict, not something to ask for.
    pub fn from_name(token: &str) -> Option<Self> {
        let kind = match token {
            "bitmap" => Self::Bitmap,
            "vector" => Self::Vector,
            "text" => Self::Text(TextKind::Plain),
            "markdown" => Self::Text(TextKind::Markdown),
            "latex" => Self::Text(TextKind::Latex),
            "code" => Self::Text(TextKind::Code),
            "palette" => Self::Text(TextKind::Palette),
            _ => return None,
        };
        Some(kind)
    }
}

/// Every name this build knows, and what it says about the text family.
///
/// `None` is a name that is not text at all (an image or a document format); a text head
/// under such a name is plain text, because the name only decides inside the text family.
const NAMES: &[(&str, Option<TextKind>)] = &[
    ("md", Some(TextKind::Markdown)),
    ("markdown", Some(TextKind::Markdown)),
    ("mdown", Some(TextKind::Markdown)),
    ("tex", Some(TextKind::Latex)),
    ("latex", Some(TextKind::Latex)),
    ("rs", Some(TextKind::Code)),
    ("rust", Some(TextKind::Code)),
    ("json", Some(TextKind::Code)),
    ("toml", Some(TextKind::Code)),
    ("yaml", Some(TextKind::Code)),
    ("yml", Some(TextKind::Code)),
    ("py", Some(TextKind::Code)),
    ("js", Some(TextKind::Code)),
    ("mjs", Some(TextKind::Code)),
    ("jsx", Some(TextKind::Code)),
    ("ts", Some(TextKind::Code)),
    ("tsx", Some(TextKind::Code)),
    ("c", Some(TextKind::Code)),
    ("h", Some(TextKind::Code)),
    ("cc", Some(TextKind::Code)),
    ("cpp", Some(TextKind::Code)),
    ("hpp", Some(TextKind::Code)),
    ("sh", Some(TextKind::Code)),
    ("bash", Some(TextKind::Code)),
    ("zsh", Some(TextKind::Code)),
    ("fish", Some(TextKind::Code)),
    ("css", Some(TextKind::Code)),
    ("html", Some(TextKind::Code)),
    ("xml", Some(TextKind::Code)),
    ("sql", Some(TextKind::Code)),
    ("lua", Some(TextKind::Code)),
    ("go", Some(TextKind::Code)),
    ("java", Some(TextKind::Code)),
    ("kt", Some(TextKind::Code)),
    ("swift", Some(TextKind::Code)),
    ("patch", Some(TextKind::Code)),
    ("diff", Some(TextKind::Code)),
    ("txt", Some(TextKind::Plain)),
    ("log", Some(TextKind::Plain)),
    ("text", Some(TextKind::Plain)),
    // A checklist is Markdown whose lines are tasks; its own extension only says so.
    ("todo", Some(TextKind::Markdown)),
    ("tasks", Some(TextKind::Markdown)),
    ("png", None),
    ("jpg", None),
    ("jpeg", None),
    ("gif", None),
    ("webp", None),
    ("bmp", None),
    ("tiff", None),
    ("tif", None),
    ("qoi", None),
    ("avif", None),
    ("heic", None),
    ("jxl", None),
    ("svg", None),
    ("gpl", None),
    ("ase", None),
    ("pdf", None),
];

/// `head` is the first few kilobytes; `name` is the file's extension or the claimed
/// MIME's subtype.
pub fn classify(head: &[u8], name: Option<&str>) -> Detected {
    let hint = name.and_then(known_name);
    let text_kind = hint
        .and_then(|name| NAMES.iter().find(|(known, _)| *known == name))
        .and_then(|(_, kind)| *kind)
        .unwrap_or(TextKind::Plain);
    let (kind, mime) = match signature(head) {
        Some(found) => found,
        None if decodes_as_text(head) => (
            Kind::Text(text_kind),
            if text_kind == TextKind::Markdown {
                "text/markdown"
            } else {
                "text/plain"
            },
        ),
        None => (Kind::Unsupported, "application/octet-stream"),
    };
    Detected {
        kind,
        mime,
        name_hint: hint,
    }
}

/// The name this build knows, as its static token: a file extension or a MIME's subtype,
/// case and separator insensitive.
fn known_name(name: &str) -> Option<&'static str> {
    let name = name.trim();
    // `text/markdown` and `image/svg+xml` name the subtype; a file name names the extension.
    let name = name.rsplit('/').next()?;
    let name = name.split('+').next()?;
    // `text/x-markdown` and its kin state the same thing with an experimental prefix.
    let name = name.strip_prefix("x-").unwrap_or(name);
    let name = name.trim_start_matches('.');
    if name.is_empty() || name.len() > 16 || !name.is_ascii() {
        return None;
    }
    let mut lowered = [0u8; 16];
    for (slot, byte) in lowered.iter_mut().zip(name.bytes()) {
        if !byte.is_ascii_alphanumeric() {
            return None;
        }
        *slot = byte.to_ascii_lowercase();
    }
    let lowered = std::str::from_utf8(&lowered[..name.len()]).ok()?;
    NAMES
        .iter()
        .find(|(known, _)| *known == lowered)
        .map(|(known, _)| *known)
}

/// The formats whose bytes identify them. Signature first, as `file(1)` and the WHATWG
/// sniffer both order it.
fn signature(head: &[u8]) -> Option<(Kind, &'static str)> {
    fn at(head: &[u8], range: std::ops::Range<usize>) -> Option<&[u8]> {
        head.get(range)
    }
    let starts = |prefix: &[u8]| head.starts_with(prefix);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some((Kind::Bitmap, "image/png"))
    } else if starts(b"\xff\xd8\xff") {
        Some((Kind::Bitmap, "image/jpeg"))
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some((Kind::Bitmap, "image/gif"))
    } else if starts(b"RIFF") && at(head, 8..12) == Some(b"WEBP") {
        Some((Kind::Bitmap, "image/webp"))
    } else if starts(b"BM") {
        Some((Kind::Bitmap, "image/bmp"))
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Some((Kind::Bitmap, "image/tiff"))
    } else if starts(b"qoif") {
        Some((Kind::Bitmap, "image/qoi"))
    } else if at(head, 4..8) == Some(b"ftyp") {
        match at(head, 8..12) {
            Some(b"avif") | Some(b"avis") => Some((Kind::Bitmap, "image/avif")),
            Some(b"heic") | Some(b"heix") | Some(b"mif1") | Some(b"msf1") => {
                Some((Kind::Bitmap, "image/heic"))
            }
            // Some other `ftyp` box: a video container, not a picture.
            _ => None,
        }
    } else if starts(b"\xff\x0a") || at(head, 4..8) == Some(b"JXL ") {
        Some((Kind::Bitmap, "image/jxl"))
    } else if svg_root(head) {
        Some((Kind::Vector, "image/svg+xml"))
    } else if starts(b"GIMP Palette") || starts(b"ASEF") {
        Some((Kind::Text(TextKind::Palette), "text/plain"))
    } else {
        None
    }
}

/// Whether the root element is `<svg`, after any prolog, comment or doctype. A document
/// that merely *contains* an SVG stays what its name says: HTML has inline SVG too.
fn svg_root(head: &[u8]) -> bool {
    let mut rest = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
    loop {
        rest = rest.trim_ascii_start();
        let Some(open) = rest.first() else {
            return false;
        };
        let closer: &[u8] = match (open, rest) {
            (b'<', [b'<', b'?', ..]) => b"?>",
            (b'<', [b'<', b'!', b'-', b'-', ..]) => b"-->",
            (b'<', [b'<', b'!', ..]) => b">",
            _ => break,
        };
        match find(rest, closer) {
            Some(at) => rest = &rest[at + closer.len()..],
            None => return false,
        }
    }
    rest.len() >= 4
        && rest[..4].eq_ignore_ascii_case(b"<svg")
        && !matches!(rest.get(4), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Whether a head with no signature is text: no binary data byte (WHATWG's list, which a
/// text file never carries), and UTF-8 up to a character the head cut in half.
fn decodes_as_text(head: &[u8]) -> bool {
    let binary = |byte: &u8| {
        matches!(*byte, 0x00..=0x08 | 0x0b | 0x0e..=0x1a | 0x1c..=0x1f)
    };
    if head.iter().any(binary) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none(),
    }
}

/// The first `limit` bytes of a material, for an error message: printable ASCII as it is,
/// the rest as an escape such as `\x00`.
pub fn first_bytes(head: &[u8], limit: usize) -> String {
    let mut out = String::new();
    for byte in head.iter().take(limit) {
        match byte {
            b' '..=b'~' => out.push(*byte as char),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            other => out.push_str(&format!("\\x{other:02x}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_prefers_the_signature_over_the_name() {
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        let seen = classify(png, Some("txt"));
        assert_eq!(seen.kind, Kind::Bitmap);
        assert_eq!(seen.mime, "image/png");

        let svg = b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        let seen = classify(svg, Some("png"));
        assert_eq!(seen.kind, Kind::Vector);
        assert_eq!(seen.mime, "image/svg+xml");

        let pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n";
        let seen = classify(pdf, Some("pdf"));
        assert_eq!(seen.kind, Kind::Unsupported);
        assert_eq!(first_bytes(pdf, 4), "%PDF");
    }

    #[test]
    fn the_text_family_is_decided_by_extension() {
        assert_eq!(
            classify(b"# Title\n", Some("md")).kind,
            Kind::Text(TextKind::Markdown)
        );
        assert_eq!(
            classify(b"# Title\n", Some("text/markdown")).kind,
            Kind::Text(TextKind::Markdown)
        );
        assert_eq!(
            classify(b"\\frac{1}{2}", Some("tex")).kind,
            Kind::Text(TextKind::Latex)
        );
        assert_eq!(
            classify(b"fn main() {}", Some("rs")).kind,
            Kind::Text(TextKind::Code)
        );
        assert_eq!(
            classify(b"hello\n", Some("txt")).kind,
            Kind::Text(TextKind::Plain)
        );
        assert_eq!(
            classify(b"hello\n", Some("weird")).kind,
            Kind::Text(TextKind::Plain)
        );
        assert_eq!(
            classify(b"- [ ] one\n", Some("todo")).kind,
            Kind::Text(TextKind::Markdown)
        );
        assert_eq!(classify(b"hello\n", None).kind, Kind::Text(TextKind::Plain));

        // No signature, and bytes a text file never carries: nothing here can show it.
        assert_eq!(classify(&[0x00, 0xff, 0xfe, 0x01], None).kind, Kind::Unsupported);
        // A control byte a text file never carries, with no signature and valid UTF-8.
        let bell = b"text\x07\n";
        assert_eq!(classify(bell, None).kind, Kind::Unsupported);
    }
}
