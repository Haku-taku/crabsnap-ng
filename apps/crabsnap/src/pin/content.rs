//! A bitmap pin: pixels, and the density they are shown at.
//!
//! Corner curve and shadow come from the pin's look, so every kind presents as the same
//! object on the canvas while the material inside differs.

use std::cell::RefCell;
use std::sync::Arc;

use ui_core::colour::{Chromaticities, ContentColour, PixelStorage, TransferFunction};
use ui_core::draw::{
    Canvas, CanvasId, DrawList, ImageEntry, ImagePrim, ImageSource, Sampling, Transform,
};
use ui_core::geom::{Corners, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Id, NodeKind, Sense};
use ui_paint::{ColorAtlas, CoverageAtlas, CpuRenderer, Pixels};
use ui_platform::Container;
use ui_platform::TextPass;
use ui_platform::element::Element;

use crabsnap_core::media::Kind;

use crate::capture::{Cropped, Light};
use crate::document::{Composing, Document, Space};
use crate::pin::badge::{Anchor, Badge, Policy, Show, Tone};
use crate::pin::edit::{self, Session};
use crate::pin::{ExportAsk, Exported, PinContent, PinFrame, card};

pub(crate) mod colour_code;
pub(crate) mod palette;
pub(crate) mod text;
pub(crate) mod vector;

/// The picture's content node while a session is open: the node tool drags land on.
fn content_node(node: Id) -> Container {
    content_area(node, None)
}

/// The same node over a box of its own: the overlay's picture is a selection, not a card
/// the layout sized, so it is placed rather than made to fill.
pub(crate) fn content_area(node: Id, area: Option<Rect>) -> Container {
    let decl = match area {
        Some(area) => Container::new(edit::content_id(node), NodeKind::Box)
            .size(SizeClass::fixed(area.size.w, area.size.h))
            .offset(area.origin.x, area.origin.y),
        None => Container::new(edit::content_id(node), NodeKind::Box).size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        }),
    };
    decl.sense(Sense::DRAG | Sense::HOVER)
        .cursor(ui_core::CursorShape::Crosshair)
}

/// Pixels a pin draws one content pixel per device pixel from.
pub(crate) struct Bitmap {
    image: Cropped,
    /// Device pixels per logical point on the output this pin lives on.
    dots: f32,
    /// Marks over the picture. An empty document draws the decoded pixels directly, so an
    /// unannotated pin pays for no second copy; one mark moves the pin onto a canvas.
    document: Document,
    /// The annotation session, when one is open.
    edit: Option<Session>,
    /// The document's history depth when the session opened: cancelling takes back exactly
    /// the commits made since, and none of the marks that were already there.
    opened_at: usize,
    /// Chrome the session built this frame, drained by the canvas after `view`.
    overlay: Vec<Container>,
}

impl Bitmap {
    pub(crate) fn new(image: Cropped, dots: f32) -> Self {
        Self::with_marks(image, dots, Document::new(), 0)
    }

    /// A picture that arrived with marks already on it: a region annotated before it became
    /// a pin. `opened_at` is the history depth they leave the document at, so a later edit
    /// can still be cancelled back to exactly this picture.
    pub(crate) fn with_marks(image: Cropped, dots: f32, document: Document, opened_at: usize) -> Self {
        Self {
            image,
            dots: dots.max(f32::EPSILON),
            document,
            edit: None,
            opened_at,
            overlay: Vec::new(),
        }
    }

    /// The picture with its marks rasterized at 1:1, for a clipboard or a file.
    ///
    /// The same list the screen paints, through the same CPU rasterizer: a preview and an
    /// export that took different routes would eventually disagree about a pixel.
    fn flattened(&self) -> Result<Cropped, String> {
        // Marks cannot be rasterized into a float picture, so the refusal below has to see a
        // measurement; the rendition measures its own bound when no marks are in the way.
        self.image.measure_peak();
        baked(&self.image, &self.document)
    }

    /// The list a canvas paints: the picture at 1:1, then the marks over it.
    fn composed(&self, texture: u64, composing: Option<&Composing>) -> DrawList {
        painted(&self.image, &self.document, texture, composing)
    }
}

/// The picture as an entry a list can sample.
fn picture(image: &Cropped, texture: u64) -> ImageEntry {
    ImageEntry {
        id: texture,
        width: image.width,
        height: image.height,
        source: ImageSource::Pixels(Arc::clone(&image.data)),
        colour: image.colour,
        // A template: the node that draws it declares the sampler for its own zoom.
        sampling: Sampling::Smooth,
    }
}

/// A bake's rasterizer, with the text pass its glyphs come from.
///
/// A bake renders outside the engine's own frame — the loupe wants a few cells, the export
/// wants the whole picture — so neither has a surface that resolves text for it. The pass and
/// the atlas uploaded from it therefore live beside the renderer, one per thread, and the
/// atlas is handed over only when it grew: copying a megabyte of coverage per loupe frame is
/// not free.
struct Bake {
    renderer: CpuRenderer,
    text: Option<TextPass>,
    coverage: u64,
    colour: u64,
}

impl Default for Bake {
    fn default() -> Self {
        Self {
            // The renderer's own default leaves its scale at nought, which draws nothing.
            renderer: CpuRenderer::new(),
            text: None,
            coverage: 0,
            colour: 0,
        }
    }
}

thread_local! {
    static BAKE: RefCell<Bake> = RefCell::new(Bake::default());
}

/// Rasterize `list` into `pixels`, its text resolved to glyphs first.
fn rasterize(list: &DrawList, pixels: &mut Pixels) {
    BAKE.with_borrow_mut(|bake| {
        let text = bake.text.get_or_insert_with(TextPass::new);
        let mut resolved = DrawList::new(list.canvas);
        text.resolve_into(list, 1.0, &mut resolved);
        if text.version() != bake.coverage {
            bake.coverage = text.version();
            let (width, height) = text.dimensions();
            bake.renderer.set_atlas(CoverageAtlas {
                width,
                height,
                data: text.atlas().to_vec(),
            });
        }
        if text.color_version() != bake.colour {
            bake.colour = text.color_version();
            if !text.color_atlas().is_empty() {
                let (width, height) = text.color_dimensions();
                bake.renderer.set_color_atlas(ColorAtlas {
                    width,
                    height,
                    data: text.color_atlas().to_vec(),
                });
            }
        }
        bake.renderer
            .render(&resolved, pixels, &ui_core::damage::Damage::Full);
    });
}

/// The picture at 1:1, then the marks over it.
fn painted(
    image: &Cropped,
    document: &Document,
    texture: u64,
    composing: Option<&Composing>,
) -> DrawList {
    shifted(image, document, texture, Point::ZERO, composing)
}

/// The same list, its primitives moved by `offset` so that a raster of it starts at the
/// window the caller wants.
fn shifted(
    image: &Cropped,
    document: &Document,
    texture: u64,
    offset: Point,
    composing: Option<&Composing>,
) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, image.width as f32, image.height as f32));
    // The list transforms each primitive as it is pushed, so this has to be in force first.
    list.transform = Some(Transform { scale: 1.0, offset });
    push_picture(&mut list, image, texture);
    document.draw_composing(&mut list, composing);
    list
}

/// The marks alone, on the picture's own pixel grid: the overlay's layer over the frozen
/// frame, so the frame is painted once and a mark cannot part company with it.
pub(crate) fn marks(
    document: &Document,
    size: (u32, u32),
    composing: Option<&Composing>,
) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32));
    document.draw_composing(&mut list, composing);
    list
}

/// The picture itself, filling the whole of its own pixel grid, pushed into `list`.
fn push_picture(list: &mut DrawList, image: &Cropped, texture: u64) {
    let bounds = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
    let index = list.push_image(picture(image, texture));
    list.push_image_prim(
        DrawList::canvas_owner(),
        ImagePrim {
            rect: bounds,
            corners: Corners::ZERO,
            texture: index,
            src: bounds,
        },
    );
}

/// One small window of the picture with `document`'s marks in it, `win` in picture pixels.
///
/// The loupe reads a handful of pixels per frame; rasterizing only that window is what lets
/// the disc show the marks without flattening the whole picture every frame.
pub(crate) fn window(image: &Cropped, document: &Document, win: Rect) -> Cropped {
    let width = win.size.w.max(1.0) as u32;
    let height = win.size.h.max(1.0) as u32;
    let mut pixels = Pixels::new(width, height);
    // The window rides with the pointer, so it may hang past the picture: the primitives are
    // clipped to the target, and the picture repeats its edge there.
    let list = shifted(
        image,
        document,
        0,
        Point::new(-win.origin.x, -win.origin.y),
        None,
    );
    rasterize(&list, &mut pixels);
    Cropped {
        // Not a region of a larger source: the window stands on its own, and the disc
        // samples it against the picture pixel its caller names.
        origin: (0, 0),
        width,
        height,
        data: Arc::new(ui_paint::encode_rgba_straight(&pixels)),
        colour: ContentColour::SDR,
        scale: image.scale,
        light: Light::sdr(),
    }
}

/// A picture with `document`'s marks rasterized into it, for a clipboard or a file.
///
/// The overlay annotates a selection before it is a pin; baking its marks here is what
/// makes the two paths produce the same picture, since both go through `painted`.
pub(crate) fn baked(image: &Cropped, document: &Document) -> Result<Cropped, String> {
    baked_in(image, document, Point::ZERO)
}

/// The same, with the document drawn from another grid: the overlay's starts at the frozen
/// frame's corner and the crop it writes starts at the selection's.
pub(crate) fn baked_in(
    image: &Cropped,
    document: &Document,
    offset: Point,
) -> Result<Cropped, String> {
    let whole = Cropped {
        origin: image.origin,
        width: image.width,
        height: image.height,
        data: Arc::clone(&image.data),
        colour: image.colour,
        scale: image.scale,
        // The same pixels: a measurement of the picture answers for this copy too.
        light: image.light.clone(),
    };
    if document.is_empty() {
        return Ok(whole);
    }
    if image.light.is_hdr() {
        return Err(String::from(
            "marks on an HDR picture are not exported yet: flattening writes SDR",
        ));
    }
    let mut list = DrawList::new(Rect::new(0.0, 0.0, image.width as f32, image.height as f32));
    push_picture(&mut list, image, 0);
    // Only the marks move: the picture is already the crop the file holds.
    list.transform = Some(Transform { scale: 1.0, offset });
    document.draw(&mut list);
    let mut pixels = Pixels::new(image.width, image.height);
    rasterize(&list, &mut pixels);
    Ok(Cropped {
        origin: image.origin,
        width: image.width,
        height: image.height,
        data: Arc::new(ui_paint::encode_rgba_straight(&pixels)),
        colour: ContentColour::SDR,
        scale: image.scale,
        light: Light::sdr(),
    })
}

impl PinContent for Bitmap {
    fn kind(&self) -> Kind {
        Kind::Bitmap
    }

    fn measure(&self, jobs: &ui_platform::jobs::Jobs) {
        measure_light(jobs, &self.image);
    }

    fn title(&self) -> Option<String> {
        None
    }

    fn size(&self) -> (f32, f32) {
        (
            self.image.width as f32 / self.dots,
            self.image.height as f32 / self.dots,
        )
    }

    fn badges(&self, catalog: &ui_i18n::Catalog) -> Vec<Badge> {
        hdr_badges(
            &self.image,
            catalog.text_by_id("pin-badge-hdr").to_string(),
        )
    }

    fn image(&mut self, texture: u64, _drawn: (f32, f32)) -> Option<ImageEntry> {
        let mut entry = picture(&self.image, texture);
        if !self.document.is_empty() {
            entry.source = ImageSource::Canvas(CanvasId(texture));
        }
        Some(entry)
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>, frame: PinFrame) -> Box<dyn Element> {
        let drawn = Rect::new(
            frame.at.x,
            frame.at.y,
            frame.size.0 * frame.zoom,
            frame.size.1 * frame.zoom,
        );
        let space = Space::new(drawn, (self.image.width, self.image.height));
        // The editor's pointer runs before anything is declared, so this build paints what
        // the drag in progress produced rather than the frame before it.
        // What the session changed is app state the declaration cache cannot see: the entries
        // it kept from the frame before still describe the document the session replaced.
        if let Some(session) = &mut self.edit
            && session.pointer(cx, &mut self.document, frame.id, space)
        {
            cx.invalidate_decls();
        }
        // A text mark's frame is derived from its shaped text: measuring here, after the
        // pointer and before the list, is what keeps the pixels and the boxes one answer.
        if self.document.measure(cx) {
            cx.invalidate_decls();
        }
        let composing = self
            .edit
            .as_ref()
            .and_then(|session| session.composing(cx, frame.id, &self.document));
        if !self.document.is_empty() {
            cx.canvas(Canvas {
                id: CanvasId(frame.texture),
                size: (self.image.width, self.image.height),
                version: self.document.version(composing.as_ref()),
                list: self.composed(frame.texture, composing.as_ref()),
            });
        }
        let sampling = if self.edit.is_some() {
            Sampling::Pixelated
        } else {
            card::picture_sampling(frame.zoom)
        };
        let mut card = card::pictured(
            card::shell(&frame, NodeKind::Box, card::boxed(frame.size)),
            frame.texture,
            sampling,
        );
        if let Some(session) = &mut self.edit {
            self.overlay = vec![session.chrome(
                cx,
                edit::Chrome {
                    node: frame.id,
                    space,
                    document: &self.document,
                    image: &self.image,
                    catalog: &frame.catalog,
                    frame: true,
                    loupe: true,
                    held: frame.loupe,
                },
            )];
            card = card.child(Box::new(content_node(frame.id)));
        }
        Box::new(card)
    }

    fn menu(&self) -> Vec<&'static str> {
        vec![
            self.edit
                .as_ref()
                .map_or("pin-menu/edit", |_| "pin-menu/done"),
        ]
    }

    fn act(&mut self, item: Id) -> bool {
        if item == Id::str("pin-menu/edit") {
            self.opened_at = self.document.depth();
            self.edit = Some(Session::new());
            return true;
        }
        if item == Id::str("pin-menu/done") {
            return self.edit_commit();
        }
        false
    }

    fn owns(&self, id: Id) -> bool {
        self.edit
            .as_ref()
            .is_some_and(|session| session.owns(id, &self.document))
    }

    fn change(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.change(id, change, &mut self.document))
    }

    fn editing(&self) -> bool {
        self.edit.is_some()
    }

    fn wheel(&mut self, value: f64) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.wheel(value))
    }

    fn edit_bar(&self) -> crate::toolbar::State {
        self.edit
            .as_ref()
            .map(edit::Session::bar_state)
            .unwrap_or_default()
    }

    fn edit_params(&self) -> Vec<crate::toolbar::Param> {
        self.edit
            .as_ref()
            .map(edit::Session::params)
            .unwrap_or_default()
    }

    fn edit_band_drag(&self) -> bool {
        self.edit
            .as_ref()
            .is_some_and(|session| session.band_drag())
    }

    fn take_overlay(&mut self) -> Vec<Container> {
        std::mem::take(&mut self.overlay)
    }

    fn edit_escape(&mut self) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.escape(&mut self.document))
    }

    fn edit_deselect(&mut self) -> bool {
        self.edit.as_mut().is_some_and(|session| session.deselect())
    }

    fn edit_cursor(&mut self) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.cursor(&mut self.document))
    }

    fn edit_history(&mut self, redo: bool) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.history(&mut self.document, redo))
    }

    /// Close the session keeping every mark it committed, like the menu's Done row.
    /// An unfinished Bezier path is saved first, so Confirm cannot leave a preview
    /// outside history.
    fn edit_commit(&mut self) -> bool {
        let Some(session) = self.edit.as_mut() else {
            return false;
        };
        session.finish_path(&mut self.document);
        self.edit.take().is_some()
    }

    fn edit_finish_exclusive(&mut self) -> bool {
        let Some(session) = self.edit.as_mut() else {
            return false;
        };
        session.finish_path(&mut self.document)
    }

    /// Close the session and take back what it drew, leaving the pin as the session found it.
    fn edit_discard(&mut self) -> bool {
        let Some(mut session) = self.edit.take() else {
            return false;
        };
        session.abandon(&mut self.document);
        self.document.rewind(self.opened_at);
        true
    }

    fn rounded(&self) -> bool {
        false
    }

    /// Whether the session's eyedropper is waiting for a colour.
    fn picking(&self) -> bool {
        self.edit.as_ref().is_some_and(edit::Session::picking)
    }

    /// Hand the session the colour the pointer found.
    fn picked(&mut self, colour: Option<[u8; 4]>) -> bool {
        self.edit
            .as_mut()
            .is_some_and(|session| session.picked(&mut self.document, colour))
    }

    /// The decoded picture's own texel under the pointer: a palette's eyedropper reads it.
    fn colour_at(&self, drawn: Rect, at: ui_core::geom::Point) -> Option<[u8; 4]> {
        let (width, height) = (self.image.width, self.image.height);
        if width == 0 || height == 0 || drawn.size.w <= 0.0 || drawn.size.h <= 0.0 {
            return None;
        }
        let u = (at.x - drawn.origin.x) / drawn.size.w * width as f32;
        let v = (at.y - drawn.origin.y) / drawn.size.h * height as f32;
        if !(0.0..width as f32).contains(&u) || !(0.0..height as f32).contains(&v) {
            return None;
        }
        self.image.texel(u as usize, v as usize)
    }

    fn clipboard(&mut self, _drawn: (u32, u32)) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(vec![("image/png".to_string(), self.flattened()?.png()?)])
    }

    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String> {
        let encoding = crate::codec::Encoding::resolve(ask.format, ask.path, ask.configured)?;
        Ok(Exported {
            bytes: ask.codecs.encode(&self.flattened()?, encoding)?,
            extension: encoding.extension(),
        })
    }
}

/// Measure a picture's absolute light on the job worker.
///
/// Nothing in the frame that shows the pin waits for it: the pixels are shared through the
/// `Arc` the pin already holds, the result lands in the picture's own `Light`, and the wake
/// that follows brings the badge in on the next frame. A worker that refused the job leaves
/// it unmeasured, and the export measures then.
pub(crate) fn measure_light(jobs: &ui_platform::jobs::Jobs, image: &Cropped) {
    if !image.colour.holds_absolute_light() || image.light.peak_nits().is_some() {
        return;
    }
    let pixels = Arc::clone(&image.data);
    let light = image.light.clone();
    let colour = image.colour;
    let _ = jobs.submit(move || {
        if let Some(nits) = crate::capture::brightest_nits(&pixels, colour) {
            light.measured(nits);
        }
    });
}

/// The pair a picture wears when it holds more than SDR: the word, and the detail behind it
/// while the reveal key is held.
///
/// What decides is the light measured from the pixels against the display's own white, not
/// the transfer function: a float capture is scRGB whether or not anything in it is bright,
/// and a PQ picture of a grey desktop is HDR-tagged without holding a single highlight.
fn hdr_badges(image: &Cropped, label: String) -> Vec<Badge> {
    if !image.light.is_hdr() {
        return Vec::new();
    }
    let anchor = Anchor::NorthWest;
    vec![
        Badge::new("hdr", label, anchor, Policy::Preferred, Show::Always).tone(Tone::Error),
        Badge::new(
            "hdr-detail",
            hdr_detail(image),
            anchor,
            Policy::Preferred,
            Show::AdvancedOnly,
        )
        .tone(Tone::Error),
    ]
}

/// What the HDR detail says: the curve, the primaries, and the measured peak.
fn hdr_detail(image: &Cropped) -> String {
    let colour = image.colour;
    let transfer = match colour.transfer {
        TransferFunction::Pq => "PQ",
        TransferFunction::Hlg => "HLG",
        TransferFunction::Srgb => "sRGB",
        // Float linear light over sRGB primaries is scRGB, the name people know it by.
        TransferFunction::Linear if colour.storage == PixelStorage::Float16 => "scRGB",
        TransferFunction::Linear => "linear",
        TransferFunction::Gamma(_) => "gamma",
    };
    let primaries = if colour.primaries == Chromaticities::BT2020 {
        "BT.2020"
    } else if colour.primaries == Chromaticities::DISPLAY_P3 {
        "Display P3"
    } else if colour.primaries == Chromaticities::SRGB {
        "sRGB"
    } else {
        "custom primaries"
    };
    match image.light.peak_nits() {
        Some(peak) => format!("{transfer} · {primaries} · {peak:.0} nit"),
        None => format!("{transfer} · {primaries}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The export is the canvas composition: a mark lands inside the picture's pixels and
    /// the bytes outside it are the decoded picture's own.
    #[test]
    fn marks_are_in_the_export_and_leave_the_rest_alone() {
        let data: Vec<u8> = std::iter::repeat_n([0u8, 0, 0, 255], 4 * 4)
            .flatten()
            .collect();
        let image = Cropped {
            origin: (0, 0),
            width: 4,
            height: 4,
            data: Arc::new(data),
            colour: ContentColour::SDR,
            scale: 1.0,
            light: Light::sdr(),
        };
        let mut bitmap = Bitmap::new(image, 1.0);
        let mark = crate::document::Mark {
            kind: crate::document::MarkKind::Rect(Rect::new(1.0, 1.0, 2.0, 2.0)),
            style: crate::document::Style {
                fill: Some(ui_core::geom::Color::from_rgba8(255, 0, 0, 255)),
                width: 0.0,
                ..crate::document::Style::default()
            },
        };
        let edit = bitmap.document.insert_edit(mark);
        bitmap.document.commit(edit);

        let flat = bitmap.flattened().expect("an SDR picture flattens");
        let at = |x: usize, y: usize| {
            let i = (y * 4 + x) * 4;
            [
                flat.data[i],
                flat.data[i + 1],
                flat.data[i + 2],
                flat.data[i + 3],
            ]
        };
        assert_eq!(at(2, 2), [255, 0, 0, 255], "the mark is in the bytes");
        assert_eq!(at(0, 0), [0, 0, 0, 255], "and the picture is untouched");
    }

    /// The export bakes marks from the frame's grid into the crop's: the overlay's document
    /// starts at the screen's corner and the file starts at the selection's, so a mark must
    /// move by the crop's origin or it lands somewhere else in the file.
    #[test]
    fn an_offset_bake_moves_the_marks_into_the_crop() {
        let crop = Cropped {
            origin: (2, 2),
            width: 2,
            height: 2,
            data: Arc::new(std::iter::repeat_n([0u8, 0, 0, 255], 4).flatten().collect()),
            colour: ContentColour::SDR,
            scale: 1.0,
            light: Light::sdr(),
        };
        let mut document = Document::new();
        let mark = crate::document::Mark {
            kind: crate::document::MarkKind::Rect(Rect::new(2.0, 2.0, 2.0, 2.0)),
            style: crate::document::Style {
                fill: Some(ui_core::geom::Color::from_rgba8(255, 0, 0, 255)),
                width: 0.0,
                ..crate::document::Style::default()
            },
        };
        let edit = document.insert_edit(mark);
        document.commit(edit);

        let flat = baked_in(&crop, &document, Point::new(-2.0, -2.0)).expect("an SDR crop bakes");
        for (index, pixel) in flat.data.chunks(4).enumerate() {
            assert_eq!(pixel, [255, 0, 0, 255], "pixel {index} lies inside the mark");
        }
        let plain = baked_in(&crop, &document, Point::ZERO).expect("an SDR crop bakes");
        assert_eq!(
            &plain.data[..4],
            [0, 0, 0, 255],
            "the frame's grid places the mark outside this crop"
        );
    }

    /// A picture holding more than the display's white, with that white measured against it.
    fn lit(colour: ContentColour, reference_nits: f32, peak_nits: Option<f32>) -> Cropped {
        let light = Light::of(reference_nits);
        if let Some(peak) = peak_nits {
            light.measured(peak);
        }
        Cropped {
            origin: (0, 0),
            width: 4,
            height: 4,
            data: Arc::new(std::iter::repeat_n([0u8, 0, 0, 255], 4 * 4).flatten().collect()),
            colour,
            scale: 1.0,
            light,
        }
    }

    /// Marks over a picture that holds light above the display's white are refused rather
    /// than flattened to SDR; with no marks the export is the decoded bytes, with no copy.
    #[test]
    fn marks_over_an_hdr_picture_are_refused_and_a_plain_export_copies_nothing() {
        let pq = ContentColour {
            storage: PixelStorage::Packed10,
            primaries: Chromaticities::BT2020,
            transfer: TransferFunction::Pq,
            max_nits: 1000.0,
        };
        let image = lit(pq, 240.0, Some(1000.0));
        let mut bitmap = Bitmap::new(image, 1.0);

        let plain = bitmap.flattened().expect("no marks needs no rasterizer");
        assert!(
            Arc::ptr_eq(&plain.data, &bitmap.image.data),
            "an unannotated picture exports its own bytes"
        );
        assert_eq!(plain.colour.transfer, TransferFunction::Pq);

        let mark = crate::document::Mark {
            kind: crate::document::MarkKind::Rect(Rect::new(1.0, 1.0, 2.0, 2.0)),
            style: crate::document::Style::default(),
        };
        let edit = bitmap.document.insert_edit(mark);
        bitmap.document.commit(edit);

        let error = bitmap
            .flattened()
            .err()
            .expect("a PQ flatten has no linear target");
        assert!(
            error.contains("HDR"),
            "the refusal names what it cannot do: {error}"
        );
    }

    /// The measured light decides, not the transfer: a float capture of an SDR desktop is
    /// tagged scRGB and claims nothing, and one nobody measured says nothing yet.
    #[test]
    fn only_light_measured_above_the_display_white_says_hdr() {
        let pq = ContentColour {
            storage: PixelStorage::Packed10,
            primaries: Chromaticities::BT2020,
            transfer: TransferFunction::Pq,
            max_nits: 1000.0,
        };
        let badges = hdr_badges(&lit(pq, 240.0, Some(1000.0)), "HDR".into());
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[0].label, "HDR");
        assert_eq!(badges[0].show, Show::Always);
        assert_eq!(badges[1].show, Show::AdvancedOnly);
        assert_eq!(badges[1].label, "PQ · BT.2020 · 1000 nit");

        let captured_sdr = lit(ContentColour::scrgb(470.0), 240.0, Some(240.0));
        assert!(
            hdr_badges(&captured_sdr, "HDR".into()).is_empty(),
            "a float capture of an SDR desktop is not an HDR claim"
        );
        let unmeasured = lit(ContentColour::scrgb(470.0), 240.0, None);
        assert!(
            hdr_badges(&unmeasured, "HDR".into()).is_empty(),
            "the badge waits for the measurement rather than guessing"
        );
    }
}
