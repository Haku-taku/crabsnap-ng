//! What a pin holds, and the one place a material becomes a kind.
//!
//! The canvas owns where a pin sits and how the pointer treats it. A content owns what is
//! inside: its pixels or its layout, the node that draws it, and the bytes it hands to the
//! clipboard or to a file. A new kind is a module beside this one and one arm in `open`.

pub(crate) mod badge;
pub(crate) mod card;
pub(crate) mod content;
pub(crate) mod edit;
pub(crate) mod ink;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crabsnap_core::config::PinLook;
use crabsnap_core::media::{self, Kind, TextKind};
use ui_core::draw::ImageEntry;
use ui_core::geom::Point;
use ui_core::menu::MenuItem;
use ui_core::{CursorShape, Id};
use ui_platform::element::Element;
use ui_style::Env;

use crate::codec::{Codecs, Encoding};

pub(crate) use content::Bitmap;

/// Two completed clicks on one pin are a double click when they are this close in
/// time; the engine's word selection in read-only text uses the same 400 ms.
pub(crate) const DOUBLE_CLICK_SECONDS: f64 = 0.4;

/// Whether `now` follows `last` inside the double-click window.
pub(crate) fn double_click(last: Option<f64>, now: f64) -> bool {
    last.is_some_and(|at| (0.0..=DOUBLE_CLICK_SECONDS).contains(&(now - at)))
}

/// One pin's material, as the canvas sees it.
pub(crate) trait PinContent {
    /// What this content holds, for `pin list`.
    fn kind(&self) -> Kind;

    /// The content's own name, when the material gave one.
    fn title(&self) -> Option<String>;

    /// The material's size in logical points at zoom 1.
    fn size(&self) -> (f32, f32);

    /// The picture the canvas should register under `texture`, when this content draws
    /// pixels at all. `drawn` is the size the pin paints at, in logical points: a vector is
    /// rasterized for it, while a bitmap already has its pixels and ignores it. How the
    /// picture is sampled is the drawing node's declaration, not this.
    fn image(&mut self, texture: u64, drawn: (f32, f32)) -> Option<ImageEntry>;

    /// The node this pin draws: the one the engine reports its changes against.
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>, frame: PinFrame) -> Box<dyn Element>;

    /// Whether the pin's outline rounds its corners. A picture's pixels are its own: cutting
    /// a corner away would change what it is a picture of.
    fn rounded(&self) -> bool {
        true
    }

    /// What this content offers the clipboard, most specific MIME first. `drawn` is the
    /// device-pixel size the pin paints at, for a kind that renders rather than holds pixels.
    fn clipboard(&mut self, drawn: (u32, u32)) -> Result<Vec<(String, Vec<u8>)>, String>;

    /// The bytes this content writes for `ask`, and the extension they belong under.
    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String>;

    /// Rows this kind adds to the pin's menu, after the canvas's own. The id is also the
    /// catalog key that names the row: one string, so a row cannot label itself with another.
    fn menu(&self) -> Vec<&'static str> {
        Vec::new()
    }

    /// The chrome this kind wants on its pin, in declaration order. A label that is words
    /// comes from `catalog`; a fact about the material may be data.
    fn badges(&self, _catalog: &ui_i18n::Catalog) -> Vec<badge::Badge> {
        Vec::new()
    }

    /// Queue whatever this kind has to measure off the frame's thread.
    ///
    /// Called once, when the pin is added, with the worker the canvas already runs jobs on;
    /// the result is read back by whatever it feeds, so nothing waits for it in a frame.
    fn measure(&self, _jobs: &ui_platform::jobs::Jobs) {}

    /// Do what one of this kind's own rows asked; the canvas has handled the shared rows.
    fn act(&mut self, _item: Id) -> bool {
        false
    }

    /// Whether `id` names a node this content drew, so its interaction can be routed here.
    fn owns(&self, _id: Id) -> bool {
        false
    }

    /// Apply one interaction that a node this content drew reported.
    fn change(&mut self, _id: Id, _change: ui_core::change::Change) -> bool {
        false
    }

    /// A wheel step while this content holds an editing session: `true` when it used it.
    fn wheel(&mut self, _value: f64) -> bool {
        false
    }

    /// A payload this content wants on the clipboard, drained by the canvas after `act`.
    fn take_copy(&mut self) -> Option<(String, Vec<u8>)> {
        None
    }

    /// Whether an editing session is open on this content.
    fn editing(&self) -> bool {
        false
    }

    /// Whether the live press landed on the band, so the canvas moves the pin.
    fn edit_band_drag(&self) -> bool {
        false
    }

    /// Chrome in surface points, drained by the canvas after `view`.
    fn take_overlay(&mut self) -> Vec<ui_platform::Container> {
        Vec::new()
    }

    /// True when the session consumed Escape: an unfinished gesture, never the tool.
    fn edit_escape(&mut self) -> bool {
        false
    }

    /// True when the session put its mark selection down; the tool stays armed.
    fn edit_deselect(&mut self) -> bool {
        false
    }

    /// True when the session put the cursor tool back.
    fn edit_cursor(&mut self) -> bool {
        false
    }

    /// Undo or redo inside the session; false when there is nothing to walk.
    fn edit_history(&mut self, _redo: bool) -> bool {
        false
    }

    /// Close the session keeping what it drew; false when no session was open.
    fn edit_commit(&mut self) -> bool {
        false
    }

    /// Finish an exclusive element without closing the whole editor; false when none is
    /// still under construction.
    fn edit_finish_exclusive(&mut self) -> bool {
        false
    }

    /// Close the session and take back what it drew; false when no session was open.
    fn edit_discard(&mut self) -> bool {
        false
    }

    /// This content's own pixel at `at`, in surface points, inside the rectangle it is drawn
    /// in. A kind with no pixels answers `None`, and a pick over it finds nothing.
    fn colour_at(&self, _drawn: ui_core::geom::Rect, _at: Point) -> Option<[u8; 4]> {
        None
    }

    /// What the annotation bar shows while this content's session is open.
    fn edit_bar(&self) -> crate::toolbar::State {
        crate::toolbar::State::default()
    }

    /// The armed tool's parameters, in the row's own order.
    fn edit_params(&self) -> Vec<crate::toolbar::Param> {
        Vec::new()
    }

    /// Whether a session is waiting for a colour picked from another pin's content.
    fn picking(&self) -> bool {
        false
    }

    /// End a pick: `colour` is what the pointer found, or `None` when it found nothing.
    fn picked(&mut self, _colour: Option<[u8; 4]>) -> bool {
        false
    }
}

/// The canvas's facts about one pin, for the build of its node.
pub(crate) struct PinFrame {
    /// The node the content draws under.
    pub id: Id,
    /// The pin's top-left in the canvas's own space.
    pub at: Point,
    /// The content's own size at native zoom: what the card is laid out at. The canvas
    /// scales the node by `zoom`, so a content never multiplies this by it.
    pub size: (f32, f32),
    /// The texture the canvas registered this pin's picture under.
    pub texture: u64,
    /// The look every pin on the canvas shares.
    pub look: PinLook,
    /// This frame's skin: the look's two states blended by how far the pointer's visit has
    /// travelled, so a shadow and a border arrive instead of switching.
    pub skin: crabsnap_core::config::PinSkin,
    /// The scope a widget resolves in: the app's catalog and MD3's options.
    pub env: Arc<Env>,
    /// The app's words, for a content that labels something of its own.
    pub catalog: Arc<ui_i18n::Catalog>,
    /// How much larger than native the pin is drawn. It is the engine's paint scale on the
    /// card, so a content reads it only to pick a sampler, never to size itself.
    pub zoom: f32,
    pub cursor: CursorShape,
    /// Copy, Save and Close, then whatever the kind adds.
    pub menu: Vec<MenuItem>,
    /// The secondary button is held with the pointer on this pin: the loupe is up.
    pub loupe: bool,
}

/// A palette waiting for the colour under a press that found no pin to read.
///
/// The canvas records it and the daemon answers with a capture of the output it names: the
/// press point is in the canvas's logical space, and the frame's pixels are what it maps to.
#[derive(Clone, Debug)]
pub(crate) struct PickRequest {
    /// The palette pin whose session waits.
    pub pin: u64,
    /// The output the canvas covers, as `Placement` names it.
    pub output: Option<String>,
    /// The canvas's logical size, for the press point's share of the frame.
    pub screen: (u32, u32),
    pub at: Point,
}

/// What an export is written as, and with what writes it.
///
/// The image encoding is resolved by the kind that writes one: a text kind has no use for
/// it, and `--format md` must not fail in the codec before the text has seen it.
pub(crate) struct ExportAsk<'a> {
    pub codecs: &'a Codecs,
    /// The configured default image format, when nothing else decides.
    pub configured: Option<&'a str>,
    /// Where the bytes go, when a path is already known.
    pub path: Option<&'a Path>,
    /// `--format`.
    pub format: Option<&'a str>,
    /// The device-pixel size the pin paints at: what a vector rasterizes for. A kind that
    /// holds its own pixels ignores it.
    pub drawn: (u32, u32),
}

/// What a content wrote: the bytes, and the extension such a file carries.
pub(crate) struct Exported {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

/// Whether `ask` names an image format: the one thing no text kind can write.
pub(crate) fn asks_for_image(ask: &ExportAsk<'_>) -> bool {
    match ask.format {
        Some(format) => Encoding::resolve(Some(format), None, None).is_ok(),
        None => ask.path.and_then(Encoding::of_path).is_some(),
    }
}

/// How many bytes detection reads; every signature and a text sample fit well inside.
const HEAD_BYTES: usize = 4096;

/// Colours `pin add --palette-from` quantizes a picture to: the seeds a person picks from.
const PALETTE_COLOURS: usize = 8;

/// A decoded picture at 1:1: what a bitmap pin shows, and what a palette is made from.
fn decode_picture(
    bytes: &[u8],
    scale: f32,
    ffmpeg: &crate::ffmpeg::Ffmpeg,
) -> Result<crate::capture::Cropped, String> {
    let decoded = ffmpeg.decode(bytes)?;
    let image = crate::capture::Image {
        width: decoded.width,
        height: decoded.height,
        data: decoded.data,
        colour: decoded.colour,
        // Only a PNG states the density it was saved at, and that is the one thing a
        // re-encode through another decoder would lose.
        scale,
        // A file does not say which display will show it, so the model's own white decides
        // whether its light counts as more than SDR.
        light: crate::capture::Light::of(ui_core::colour::SDR_WHITE_NITS),
    };
    let full = ui_core::geom::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
    Ok(image.crop(full))
}

/// Read a file into a pin's content: the only place a material becomes an implementation.
///
/// `dots` is the output's device ratio, which is what turns a picture's pixels into the
/// logical points a canvas places; `as_kind` is the user's `--as`, which outweighs detection.
/// A picture is read by `ffmpeg`, the authority that also decides what this machine writes.
pub(crate) fn open(
    path: &Path,
    dots: f32,
    as_kind: Option<Kind>,
    ffmpeg: &crate::ffmpeg::Ffmpeg,
) -> Result<Box<dyn PinContent>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let name = path.extension().and_then(|extension| extension.to_str());
    let detected = media::classify(&bytes[..bytes.len().min(HEAD_BYTES)], name);
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    match as_kind.unwrap_or(detected.kind) {
        // The head was read for detection, so the decode reads the same bytes: a
        // screenshot is tens of megabytes, and a second read would be the largest copy
        // the command makes.
        Kind::Bitmap => decode_picture(&bytes, crate::capture::png_scale(&bytes), ffmpeg)
            .map(|image| Box::new(Bitmap::new(image, dots)) as Box<dyn PinContent>)
            .map_err(|error| format!("{}: {error}", path.display())),
        Kind::Vector => content::vector::Vector::read(&bytes, path, dots, title)
            .map(|vector| Box::new(vector) as Box<dyn PinContent>)
            .map_err(|error| format!("{}: {error}", path.display())),
        // `--palette-from`: a picture asked for as a palette is quantized, not parsed.
        Kind::Text(TextKind::Palette) if detected.kind == Kind::Bitmap => {
            let image = decode_picture(&bytes, crate::capture::png_scale(&bytes), ffmpeg)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            content::palette::Palette::from_picture(&image, title, PALETTE_COLOURS)
                .map(|palette| Box::new(palette) as Box<dyn PinContent>)
                .map_err(|error| format!("{}: {error}", path.display()))
        }
        Kind::Text(TextKind::Palette) if detected.kind == Kind::Vector => Err(format!(
            "{}: an SVG's colours are not read yet; export a GIMP palette",
            path.display()
        )),
        Kind::Text(TextKind::Palette) => content::palette::Palette::read(&bytes, title)
            .map(|palette| Box::new(palette) as Box<dyn PinContent>)
            .map_err(|error| format!("{}: {error}", path.display())),
        Kind::Text(kind) => Ok(Box::new(content::text::Text::read(
            kind,
            bytes,
            detected.name_hint,
            title,
        ))),
        Kind::Unsupported => Err(format!(
            "{}: no decoder for these bytes ({})",
            path.display(),
            media::first_bytes(&bytes, 8)
        )),
    }
}

/// A palette pin from one CSS colour code, as `pin add --color` takes it.
///
/// No file, no detection: the user stated the material itself, and a code that does not
/// parse is the whole error.
pub(crate) fn open_colour(code: &str) -> Result<Box<dyn PinContent>, String> {
    content::palette::Palette::from_colour(code)
        .map(|palette| Box::new(palette) as Box<dyn PinContent>)
}

/// A pin from bytes the app was handed, as a clipboard selection is: no file, so no path to
/// name a title or resolve resources against. The MIME's subtype is the classifier's name
/// hint, the way a file name would be.
pub(crate) fn open_blob(
    bytes: &[u8],
    mime: &str,
    dots: f32,
    as_kind: Option<Kind>,
    ffmpeg: &crate::ffmpeg::Ffmpeg,
) -> Result<Box<dyn PinContent>, String> {
    let subtype = mime.rsplit('/').next().unwrap_or(mime);
    let detected = media::classify(&bytes[..bytes.len().min(HEAD_BYTES)], Some(subtype));
    match as_kind.unwrap_or(detected.kind) {
        // Blob pixels were not read from a file, so there is no density to read.
        Kind::Bitmap => decode_picture(bytes, 1.0, ffmpeg)
            .map(|image| Box::new(Bitmap::new(image, dots)) as Box<dyn PinContent>),
        Kind::Vector => content::vector::Vector::read_blob(bytes, dots)
            .map(|vector| Box::new(vector) as Box<dyn PinContent>),
        Kind::Text(TextKind::Palette) if detected.kind == Kind::Bitmap => {
            let image = decode_picture(bytes, 1.0, ffmpeg)?;
            content::palette::Palette::from_picture(&image, None, PALETTE_COLOURS)
                .map(|palette| Box::new(palette) as Box<dyn PinContent>)
        }
        Kind::Text(TextKind::Palette) => content::palette::Palette::read(bytes, None)
            .map(|palette| Box::new(palette) as Box<dyn PinContent>),
        Kind::Text(kind) => Ok(Box::new(content::text::Text::read(
            kind,
            bytes.to_vec(),
            detected.name_hint,
            None,
        ))),
        Kind::Unsupported => Err(format!(
            "no decoder for these bytes ({})",
            media::first_bytes(bytes, 8)
        )),
    }
}

/// One item of a list that did open, with the place it keeps in the cascade.
pub(crate) struct Opened {
    pub index: usize,
    pub content: Box<dyn PinContent>,
    /// What the pin is made from; a file list names one path per item.
    pub origin: crabsnap_core::protocol::PinOrigin,
}

/// Open every path of one list, keeping each item's place; a failure names its path and does
/// not stop the rest.
pub(crate) fn open_all(
    paths: &[PathBuf],
    dots: f32,
    as_kind: Option<Kind>,
    ffmpeg: &crate::ffmpeg::Ffmpeg,
) -> (Vec<Opened>, Vec<String>) {
    let mut opened = Vec::with_capacity(paths.len());
    let mut errors = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        match open(path, dots, as_kind, ffmpeg) {
            Ok(content) => opened.push(Opened {
                index,
                content,
                origin: crabsnap_core::protocol::PinOrigin::File {
                    path: path.to_string_lossy().into_owned(),
                },
            }),
            Err(error) => errors.push(error),
        }
    }
    (opened, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One bad path does not stop the list, and the items after it keep their place: the
    /// cascade index is the position in the request, not in the successes.
    #[test]
    fn a_path_list_opens_every_readable_file_and_names_the_bad_one() {
        let dir = std::env::temp_dir().join(format!("crabsnap-pin-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let png = {
            let image = crate::capture::Image {
                width: 1,
                height: 1,
                data: std::sync::Arc::new(vec![255u8, 0, 0, 255]),
                colour: ui_core::colour::ContentColour::SDR,
                scale: 1.0,
                light: crate::capture::Light::sdr(),
            };
            image
                .crop(ui_core::geom::Rect::new(0.0, 0.0, 1.0, 1.0))
                .png()
                .expect("encode the fixture")
        };
        let good = dir.join("good.png");
        let missing = dir.join("missing.png");
        let later = dir.join("later.png");
        std::fs::write(&good, &png).expect("write the fixture");
        std::fs::write(&later, &png).expect("write the fixture");

        let paths = vec![good.clone(), missing.clone(), later.clone()];
        let ffmpeg = crate::ffmpeg::Ffmpeg::probe(None, None);
        let (opened, errors) = open_all(&paths, 1.0, None, &ffmpeg);
        assert_eq!(
            opened.iter().map(|item| item.index).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(opened[0].content.size(), (1.0, 1.0));
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains("missing.png"),
            "the error must name the path: {}",
            errors[0]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A double click is two clicks inside the window and nothing else: a stale click, a
    /// clock that went back and the very first click all start a new pair.
    #[test]
    fn two_clicks_are_a_double_click_only_inside_the_window() {
        assert!(double_click(Some(1.0), 1.1));
        assert!(
            double_click(Some(0.0), DOUBLE_CLICK_SECONDS),
            "the window closes inclusively"
        );
        assert!(!double_click(Some(1.0), 1.5));
        assert!(!double_click(Some(1.0), 0.9), "time does not run backwards");
        assert!(!double_click(None, 1.0));
    }
}
