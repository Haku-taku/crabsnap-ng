//! A vector pin: an SVG, rasterized at the size the pin draws.
//!
//! A vector has no pixels of its own on screen. The wheel scales the pin's node, so the
//! drawn size changes without the content knowing; the canvas passes it back through
//! [`PinContent::image`], and this module keeps one raster for the zoom it is at. The raster
//! is rebuilt when the requested size has moved more than one step away from it on the
//! ladder in [`rung_for`], which is what keeps a wheel sweep from rebuilding every notch.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use crabsnap_core::media::Kind;
use resvg::tiny_skia;
use ui_core::NodeKind;
use ui_core::colour::ContentColour;
use ui_core::draw::{ImageEntry, Sampling};
use ui_platform::element::Element;

use crate::capture;
use crate::document::path::{fitted, straight_alpha};
use crate::pin::{ExportAsk, Exported, PinContent, PinFrame, card};

/// Steps per doubling of the raster's scale. A wheel notch is about one step, so the
/// hysteresis below absorbs every other notch instead of rasterizing each one.
const RUNGS_PER_OCTAVE: f32 = 8.0;
/// What every vector raster in the process may take together.
const CACHE_BUDGET: usize = 64 << 20;

pub(crate) struct Vector {
    tree: usvg::Tree,
    /// The document's own size in CSS pixels: usvg's unit, which `size` divides by `dots`.
    document: (f32, f32),
    /// Device pixels per logical point on the output this pin lives on.
    dots: f32,
    title: Option<String>,
    /// The file the bytes came from, when they came from one: what the shared cache is
    /// keyed by. A vector handed over as bytes keeps its raster to itself.
    asset: Option<Asset>,
    /// The raster the canvas is drawing, and the rung it was made at.
    raster: Option<Raster>,
}

struct Raster {
    width: u32,
    height: u32,
    rung: i32,
    /// Straight-alpha RGBA8, the `ImageEntry` convention; premultiplied by the renderer.
    data: Arc<Vec<u8>>,
}

impl Vector {
    /// Parse `bytes`; `path` names the file they were read from, for relative references
    /// and for the shared cache. A file that does not parse is the whole error, with the
    /// parser's position in it.
    pub(crate) fn read(
        bytes: &[u8],
        path: &Path,
        dots: f32,
        title: Option<String>,
    ) -> Result<Self, String> {
        Self::from_data(
            bytes,
            path.parent().map(Path::to_path_buf),
            asset_of(path),
            dots,
            title,
        )
    }

    /// Parse `bytes` handed over without a file, as a clipboard selection is: no directory
    /// to resolve relative references against, and nothing to key the shared cache on.
    pub(crate) fn read_blob(bytes: &[u8], dots: f32) -> Result<Self, String> {
        Self::from_data(bytes, None, None, dots, None)
    }

    fn from_data(
        bytes: &[u8],
        resources_dir: Option<PathBuf>,
        asset: Option<Asset>,
        dots: f32,
        title: Option<String>,
    ) -> Result<Self, String> {
        // usvg resolves text while parsing, so the fonts must be there before the parse;
        // only a document that has text pays for scanning the system's fonts, which costs
        // more than every other part of opening an SVG together.
        let fontdb = if has_text(bytes) {
            fonts()
        } else {
            Arc::new(fontdb::Database::new())
        };
        let tree = usvg::Tree::from_data(bytes, &options(resources_dir, fontdb))
            .map_err(|error| error.to_string())?;
        let size = tree.size();
        Ok(Self {
            tree,
            document: (size.width(), size.height()),
            dots: dots.max(f32::EPSILON),
            title,
            asset,
            raster: None,
        })
    }

    /// Make the raster match `scale`, in device pixels per document pixel.
    fn show(&mut self, scale: f32) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let rung = rung_for(scale, self.raster.as_ref().map(|raster| raster.rung));
        if self
            .raster
            .as_ref()
            .is_some_and(|raster| raster.rung == rung)
        {
            return;
        }
        match self.rasterize(rung) {
            Ok(raster) => self.raster = Some(raster),
            // Nothing to draw is the honest answer when a raster cannot be made; the pin
            // still drags, copies and exports, and the terminal says why it is blank.
            Err(error) => eprintln!("pin: {error}"),
        }
    }

    fn rasterize(&self, rung: i32) -> Result<Raster, String> {
        let (width, height) = fitted((
            self.document.0 * rung_scale(rung),
            self.document.1 * rung_scale(rung),
        ));
        let key = self.asset.clone().map(|asset| (asset, width, height));
        if let Some(key) = &key
            && let Some(data) = cached(key)
        {
            return Ok(Raster {
                width,
                height,
                rung,
                data,
            });
        }
        let data = render(&self.tree, width, height)?;
        if let Some(key) = key {
            remember(key, &data);
        }
        Ok(Raster {
            width,
            height,
            rung,
            data,
        })
    }

    /// The picture at the size the canvas asked for, for copying and exporting: exact
    /// rather than a rung of the ladder, because nothing will stretch it afterwards.
    fn picture(&self, drawn: (u32, u32)) -> Result<capture::Image, String> {
        let (width, height) = fitted((drawn.0 as f32, drawn.1 as f32));
        Ok(capture::Image {
            width,
            height,
            data: render(&self.tree, width, height)?,
            colour: ContentColour::SDR,
            scale: self.dots,
        })
    }
}

impl PinContent for Vector {
    fn kind(&self) -> Kind {
        Kind::Vector
    }

    fn title(&self) -> Option<String> {
        self.title.clone()
    }

    fn size(&self) -> (f32, f32) {
        (self.document.0 / self.dots, self.document.1 / self.dots)
    }

    fn image(&mut self, texture: u64, drawn: (f32, f32)) -> Option<ImageEntry> {
        let scale = drawn.0 * self.dots / self.document.0.max(f32::EPSILON);
        self.show(scale);
        let raster = self.raster.as_ref()?;
        Some(ImageEntry {
            id: texture,
            width: raster.width,
            height: raster.height,
            source: ui_core::draw::ImageSource::Pixels(Arc::clone(&raster.data)),
            colour: ContentColour::SDR,
            // Every zoom reads a raster made for it, so an enlarged vector is never a set
            // of blocks the way an enlarged bitmap is.
            sampling: Sampling::Smooth,
        })
    }

    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>, frame: PinFrame) -> Box<dyn Element> {
        Box::new(card::pictured(
            card::shell(&frame, NodeKind::Box, card::boxed(frame.size)),
            frame.texture,
            Sampling::Smooth,
        ))
    }

    fn rounded(&self) -> bool {
        false
    }

    fn clipboard(&mut self, drawn: (u32, u32)) -> Result<Vec<(String, Vec<u8>)>, String> {
        let image = self.picture(drawn)?;
        let full = ui_core::geom::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
        Ok(vec![("image/png".to_string(), image.crop(full).png()?)])
    }

    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String> {
        let encoding = crate::codec::Encoding::resolve(ask.format, ask.path, ask.configured)?;
        let image = self.picture(ask.drawn)?;
        let full = ui_core::geom::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
        Ok(Exported {
            bytes: ask.codecs.encode(&image.crop(full), encoding)?,
            extension: encoding.extension(),
        })
    }
}

/// Whether the document has a text element, and so needs fonts: `<textPath>` opens with the
/// same bytes, and a `<tspan>` only ever lives inside one.
fn has_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| text.contains("<text"))
}

fn options(
    resources_dir: Option<PathBuf>,
    fontdb: Arc<fontdb::Database>,
) -> usvg::Options<'static> {
    usvg::Options {
        resources_dir,
        fontdb,
        ..usvg::Options::default()
    }
}

/// The fonts `<text>` resolves against.
///
/// resvg shapes SVG text with its own font database, not with our text pass; an empty one
/// drops every text element without a word of complaint. Scanning is done once per process.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    Arc::clone(FONTS.get_or_init(|| {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        Arc::new(database)
    }))
}

/// Rasterize `tree` into `width`×`height` RGBA8.
fn render(tree: &usvg::Tree, width: u32, height: u32) -> Result<Arc<Vec<u8>>, String> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("a {width}×{height} vector raster could not be allocated"))?;
    let size = tree.size();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(
            width as f32 / size.width(),
            height as f32 / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    Ok(Arc::new(straight_alpha(pixmap.data())))
}

/// The rung `scale` belongs on: the nearest step of `2^(rung / RUNGS_PER_OCTAVE)`.
fn rung_of(scale: f32) -> i32 {
    (scale.log2() * RUNGS_PER_OCTAVE).round() as i32
}

fn rung_scale(rung: i32) -> f32 {
    2f32.powf(rung as f32 / RUNGS_PER_OCTAVE)
}

/// The rung to rasterize at: the current one while `scale` is within one step of it, else
/// the nearest rung. One step of hysteresis is what turns a wheel sweep into a handful of
/// rasters instead of one per notch, and between two of them the raster is stretched by at
/// most a step — 9 % of its scale, invisible on a picture that smooth.
fn rung_for(scale: f32, current: Option<i32>) -> i32 {
    let wanted = rung_of(scale);
    match current {
        Some(rung) if (RUNGS_PER_OCTAVE * (scale / rung_scale(rung)).log2()).abs() <= 1.0 => rung,
        _ => wanted,
    }
}

/// The file a vector came from: a rewritten file is a different asset, so a stale raster is
/// never served for it.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Asset {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
}

fn asset_of(path: &Path) -> Option<Asset> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Asset {
        // The path the user opened is not canonical when it goes through a symlink; the
        // file is the same asset either way, and two pins must land on one key.
        path: std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        modified: metadata.modified().ok(),
        length: metadata.len(),
    })
}

type RasterKey = (Asset, u32, u32);

/// One process-wide cache of rasters, because the same file commonly becomes several pins.
#[derive(Default)]
struct Rasters {
    entries: HashMap<RasterKey, Arc<Vec<u8>>>,
    /// Least recently used first.
    order: VecDeque<RasterKey>,
    bytes: usize,
}

fn rasters() -> &'static Mutex<Rasters> {
    static RASTERS: OnceLock<Mutex<Rasters>> = OnceLock::new();
    RASTERS.get_or_init(|| Mutex::new(Rasters::default()))
}

fn cached(key: &RasterKey) -> Option<Arc<Vec<u8>>> {
    // A poisoned cache is skipped rather than propagated: its contents are an optimisation
    // and the panic that poisoned it did not come from here.
    let mut cache = rasters().lock().ok()?;
    let data = cache.entries.get(key).cloned()?;
    cache.order.retain(|held| held != key);
    cache.order.push_back(key.clone());
    Some(data)
}

fn remember(key: RasterKey, data: &Arc<Vec<u8>>) {
    let Ok(mut cache) = rasters().lock() else {
        return;
    };
    if let Some(previous) = cache.entries.insert(key.clone(), Arc::clone(data)) {
        cache.bytes -= previous.len();
        cache.order.retain(|held| *held != key);
    }
    cache.bytes += data.len();
    cache.order.push_back(key);
    while cache.bytes > CACHE_BUDGET {
        let Some(oldest) = cache.order.pop_front() else {
            break;
        };
        if let Some(gone) = cache.entries.remove(&oldest) {
            cache.bytes -= gone.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cache key is the ladder: one step away keeps the raster, further rebuilds it, and
    /// both directions behave alike so a wheel turned either way costs the same.
    #[test]
    fn a_vector_raster_is_reused_within_a_step_and_rebuilt_across_one() {
        assert_eq!(rung_of(1.0), 0);
        assert_eq!(rung_of(2.0), 8);
        assert_eq!(rung_of(0.5), -8);
        assert!((rung_scale(8) - 2.0).abs() < 1e-6);
        // Both of these round onto a different rung; keeping the current raster is the
        // hysteresis, not the rounding.
        assert_eq!(rung_of(1.06), 1);
        assert_eq!(rung_of(0.95), -1);

        assert_eq!(rung_for(1.06, Some(0)), 0, "a step up keeps the raster");
        assert_eq!(rung_for(0.95, Some(0)), 0, "a step down keeps it too");
        assert_eq!(rung_for(1.12, Some(0)), rung_of(1.12), "two steps rebuild");
        assert_eq!(rung_for(0.88, Some(0)), rung_of(0.88));
    }

    /// The pin hands the canvas the raster for the size it draws at, and only a step of zoom
    /// makes a new one: a picture that never rebuilt would stay soft as the wheel turns, and
    /// one that rebuilt every frame would spend a resvg pass per frame.
    #[test]
    fn a_zoom_keeps_the_raster_until_it_crosses_a_rung() {
        // Blue on the top half, red under it: the raster's rows must be read from the top,
        // and a resvg pixmap's premultiplied channels must arrive as straight ones.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="0" y="0" width="100" height="100" fill="#ff0000"/>
            <rect x="0" y="0" width="100" height="50" fill="#0000ff"/>
        </svg>"##;
        let path = std::path::Path::new("fixture.svg");
        let mut vector = Vector::read(svg, path, 1.0, None).expect("the fixture parses");

        let first = vector.image(7, (100.0, 100.0)).expect("a raster");
        assert_eq!((first.width, first.height), (100, 100));
        let first_pixels = first.pixels().expect("a raster holds pixels");
        let texel = |x: usize, y: usize| {
            let at = 4 * (y * first.width as usize + x);
            [
                first_pixels[at],
                first_pixels[at + 1],
                first_pixels[at + 2],
                first_pixels[at + 3],
            ]
        };
        assert_eq!(texel(50, 25), [0, 0, 255, 255], "the top half is blue");
        assert_eq!(texel(50, 75), [255, 0, 0, 255], "the bottom half is red");

        let kept = vector.image(7, (104.0, 104.0)).expect("a raster");
        assert!(
            Arc::ptr_eq(first_pixels, kept.pixels().expect("a raster holds pixels")),
            "4 % of zoom reuses the raster"
        );

        let rebuilt = vector.image(7, (120.0, 120.0)).expect("a raster");
        assert!(
            rebuilt.width > first.width,
            "crossing a rung makes a bigger raster: {} vs {}",
            rebuilt.width,
            first.width
        );
        assert!(!Arc::ptr_eq(
            first_pixels,
            rebuilt.pixels().expect("a raster holds pixels")
        ));
    }
}
