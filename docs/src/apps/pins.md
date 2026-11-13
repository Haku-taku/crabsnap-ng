# Pins: Content, Sources and Editing

A pin is a piece of the user's material held on an output until they dismiss it.
Today it holds one kind of material — a picture cut from a capture or read from
a PNG — and offers three actions: copy, save, close. This page designs the pin
as a container for any small thing a user pins: an image in a format ffmpeg can
read, an SVG, plain text, Markdown, a formula, code, or a colour palette — and
the editing surface each of those kinds opens.

## Problem

Three failures, each reproducible today.

`crabsnap pin add --file notes.md` fails. The file is decoded with
`image::load_from_memory` and this build enables only the `png` feature
(`apps/crabsnap/Cargo.toml`), so every format the encoder can write, the pin
cannot read back.

A file manager copies five files. The selection is a `text/uri-list` payload of
five paths. Nothing in the app reads a selection, and `pin add` takes exactly
one `--file`, so there is no route from that payload to pins.

A pin has no notion of content kind. The canvas draws every pin as a texture
with one menu, so no kind can be rendered other than a picture, none can differ
in its menu, and none can be edited.

## Comparison

**File lists already exist on every platform.** The Web convention is
`text/uri-list`: one URI per line, CRLF-separated, `#` lines are comments ([RFC
2483](https://www.rfc-editor.org/rfc/rfc2483)). GNOME adds
`x-special/gnome-copied-files`, whose first line is `copy` or `cut`. Windows
puts a `CF_HDROP` on the clipboard, a `DROPFILES` header followed by a
double-NUL-terminated list of wide-character paths
([CF_HDROP](https://learn.microsoft.com/windows/win32/shell/clipboard#cf_hdrop)).
macOS offers `public.file-url`, a list of `file:` URLs, with
[`NSFilenamesPboardType`](https://developer.apple.com/documentation/appkit/nsfilenamespboardtype)
as the legacy property list. None of these carries file contents; all of them
carry paths. The app should receive the one shape they share — a list of paths —
and let each backend do the translation.

**Type detection starts from bytes.** `file(1)` calls libmagic's database; the
WHATWG MIME Sniffing standard puts the image signatures first and treats the
extension as a tie-breaker ([MIME
Sniffing](https://mimesniff.spec.whatwg.org/)). A `.txt` that begins with
`\x89PNG` is a PNG; a file with no signature at all is text only if it decodes
as UTF-8. The pin's helper follows that order.

**Sampling belongs to the item, not the texture.** Qt Quick's `Image` carries
`smooth` and `mipmap` on the item, so the same source drawn twice can be
filtered twice; egui's `TextureOptions::NEAREST` is chosen per texture handle.
Our engine uploads every image with `LINEAR`
(`ui/crates/ui-gpu-scene/src/renderer.rs`, `upload_image`), which is the
smoothing half of what a zoomable pin needs.

**A pin that is not a picture is still a preview.** macOS Quick Look renders any
file kind without opening an editor, and file managers show a rendered preview
beside an unedited source. Both keep two things apart: the rendered form and the
source. The pin model does the same — content renders through the UI framework
when it is not pixels, and editing is a session that writes back to the source.

**Palette generation has a reference implementation.** Material Color Utilities'
`DynamicScheme` derives the MD3 role palettes from one seed colour and a
contrast level (`SchemeTonalSpot` is the default), and Monet quantizes a
wallpaper to seeds
([material-color-utilities](https://github.com/material-foundation/material-color-utilities)).
`ui-color-hct` has the HCT maths — `TonalPalette`, `KeyColor`, contrast ratios —
and now the tonal-spot scheme beside them; the contrast curves and the quantizer
are still missing.

## Decisions

Four choices shape the contract below. Each names the alternative it replaces.

**Pixels are read through ffmpeg.** The C shim gains `cs_decode` beside
`cs_encode`, so the reader and the writer answer "what can this machine show"
from one table, and a decoded stream brings its colour description with it. The
alternative — enabling more `image` features — is smaller, but it covers fewer
formats than the writer offers and puts a second capability table next to the
first, which is the split this app removed from the write side.

**A text pin does not export a picture yet.** Rendering the pin to a texture and
reading it back needs an offscreen readback the renderer does not have. Until it
exists, an image extension on a text pin is refused with the kind named, rather
than writing a file no viewer can open.

**`pin add --clipboard` answers before its pins exist.** The selection's MIME
list is known synchronously, so the command fails at once when nothing usable is
offered; the bytes arrive on the loop afterwards and the pins appear in the same
session. Holding the reply until the pipe delivers would either block the
daemon's turn or need a reply that spans turns, and `pin list` is the record
either way.

**Monet extraction is tried with `quantette` first.** It is a maintained k-means
quantizer with no C dependency. If its seeds move an MD3 scheme away from
material-color-utilities' own output by more than the scheme tolerates, porting
that library's WSMeans quantizer follows.

## Contract

### Acquisition

A pin is made from a material. Two shapes arrive, and everything downstream sees
the same one:

```rust
/// Where a pin begins.
pub enum Material {
    /// A file on disk, named by a path: a CLI argument, or one item of a file list.
    Path(PathBuf),
    /// Bytes a peer handed over, with the MIME it claimed.
    Blob { mime: String, bytes: Vec<u8> },
}
```

A selection is read as its MIME and bytes, and the app splits it into one of two
shapes (the uri-list parser is the app's, so this lives in `crabsnap-core`):

```rust
/// What a selection offers, as the app sees it.
pub enum Transfer {
    /// A file list: `text/uri-list`, `x-special/gnome-copied-files`, and their kin.
    Files(Vec<PathBuf>),
    /// One payload with a MIME.
    Bytes { mime: String, data: Vec<u8> },
}
```

Rules:

- A `Files` transfer yields one material per item, so a five-file copy becomes
  five pins. A failure in one item is reported with its path and does not stop
  the rest.
- A `Bytes` transfer is one material. An image blob is decoded from memory; a
  text blob is classified by the same helper, with its MIME as the name hint.
- The claimed MIME is a hint, never the decision. `image/png` bytes that start
  with `<svg` are an SVG; a `text/uri-list` item that names a PNG is a bitmap.
- A text payload that is nothing but `file:` URIs is a file list, even when the
  selection claims only `text/plain`: that is how some applications hand a
  copied file over, and pinning the URI would show the user a path where they
  copied a picture. Any other text stays one blob, so a sentence that mentions a
  file is still text.
- The uri-list parser handles CRLF, `#` comments, percent-encoding, `file://`
  with an empty or `localhost` host, and `file:///C:/…` on a host that is not
  Windows (the path is kept verbatim and fails to open there, which is what the
  user asked for). It is a pure function over text, so it is tested without a
  clipboard.
- Wayland reads the selection through `ext_data_control_v1` when the compositor
  offers it: it needs no keyboard focus, so the daemon can read without taking
  the keyboard from anyone. The seat `data_device` would need focus and is not
  wired for reading. Reading is polled like a capture; the pins exist when the
  pipe has delivered.

Placement of a list is a cascade: item `k` — its place in the request, so a file
that failed does not shift the ones after it — is centred, then displaced by
`(24, 24) * k` logical points and clamped onto the output. The first item stays
where a single pin would go, and the step never wraps.

### Detection

One helper decides what a material is. It reads the head of the material, not
the file's name, and falls back to the name only inside the text family.

```rust
// crabsnap-core/src/media.rs — portable: no UI crate, no platform crate.
pub enum Kind {
    Bitmap,
    Vector,
    Text(TextKind),
    Unsupported,
}

pub enum TextKind {
    Plain,
    Markdown,
    Latex,
    Code,     // the extension is the language hint; ui-highlight may know it
    Palette,
}

pub struct Detected {
    pub kind: Kind,
    pub mime: &'static str,
    pub name_hint: Option<&'static str>,  // the extension, lowercased
}

/// `head` is the first few kilobytes; `name` is the file's extension or the claimed
/// MIME's subtype. Magic decides first, the name second, UTF-8 last.
pub fn classify(head: &[u8], name: Option<&str>) -> Detected;
```

The magic table covers what the encoder offers: PNG, JPEG, GIF, WebP
(`RIFF`…`WEBP`), BMP, TIFF (`II*\0`, `MM\0*`), QOI (`qoif`), AVIF/HEIF (`ftyp`
with `avif`, `avis`, `heic`, `mif1`), JPEG XL (codestream `FF0A`, container with
a `JXL ` signature), SVG (`<svg` as the root element, after any prolog, comment
or doctype), and the palette formats as they are added (`GIMP Palette`, `ASEF`).

Inside the text family the name decides: `.md`, `.markdown`, `.mdown`, `.todo`
and `.tasks` are Markdown (the last two are habit from the legacy app, and their
rows draw their task boxes either way); `.tex`, `.latex` are LaTeX; an extension
in the classifier's code table is code; `.txt`, `.log` and everything else that
decodes as UTF-8 is plain text. A file with no signature that does not decode as
UTF-8 in its head is `Unsupported`, and the error names the first bytes
(`media::first_bytes`) so the user can tell what the tool saw.

### Content

A pin owns one content value. The canvas speaks to it through one object, so a
new kind is a new module and one arm in the factory — no change to the canvas,
the protocol or the command line:

```rust
// apps/crabsnap/src/pin.rs — the subset the kinds need today.
pub trait PinContent {
    fn kind(&self) -> Kind;
    fn title(&self) -> Option<String>;

    /// Logical size at scale 1. Bitmaps use their own size, texts their laid-out size
    /// capped at a maximum width.
    fn size(&self) -> (f32, f32);

    /// The picture the canvas registers under `texture`, for a kind that draws pixels.
    fn image(&self, texture: u64) -> Option<ImageEntry>;

    /// The pin's node. `frame` carries the zoom, the look and the app's words.
    fn view(&mut self, cx: &mut Cx<'_>, frame: PinFrame) -> Box<dyn Element>;

    /// Whether the pin's outline rounds its corners; a picture's pixels stay square.
    fn rounded(&self) -> bool { true }

    /// Payloads for the clipboard, in the order the platform should offer them.
    fn clipboard(&mut self) -> Result<Vec<(String, Vec<u8>)>, String>;

    /// The file this kind writes: pixels through the codecs, text as text.
    fn export(&mut self, ask: &ExportAsk<'_>) -> Result<Exported, String>;

    /// Rows this kind adds after Copy, Save and Actual size. The row id is also the
    /// catalog key that names it, so a row cannot label itself with another's words.
    fn menu(&self) -> Vec<&'static str> { Vec::new() }

    /// Do what one of this kind's own rows asked; the canvas has handled the shared ones.
    fn act(&mut self, _item: Id) -> bool { false }

    /// A payload `act` asked for, drained by the canvas after the action.
    fn take_copy(&mut self) -> Option<(String, Vec<u8>)> { None }

    /// Whether `id` names a node this content drew, so its interaction can come here.
    fn owns(&self, _id: Id) -> bool { false }

    /// Apply one interaction a node this kind drew reported.
    fn change(&mut self, _id: Id, _change: Change) -> bool { false }
}
```

The default methods are empty on purpose: a kind that adds no row and draws
nothing interactive says nothing, and the palette is the one kind that does both
today. `owns`/`change` are routed from `App::changed` for ids the canvas does
not recognise as a pin, so a swatch's press never looks like a click on the
card. Editing (`begin_edit`, capabilities) arrives with the editors; the trait
is the exact subset the kinds use, not a catalogue of what they might.

The kinds:

| Kind | Holds | `size()` | Edit |
| --- | --- | --- | --- |
| Bitmap | decoded pixels, `ContentColour`, sampling, marks | pixels / `dots` | annotate |
| Vector | parsed `usvg::Tree`, raster cache | document size | no |
| Plain text | source string | layout, max width | text |
| Markdown | source string, parsed document, token cache | layout, max width | live-preview source |
| Code | source string, language, tokens | layout, max width | source + highlight |
| LaTeX | source string, math layout | formula box | formula field |
| Palette | swatches, optional seed, optional scheme | grid | picker + generation |

### The card

Every kind but a picture sits in a card: the same surface, padding, shadow and
outline, so a markdown note and a palette look like two objects of one family. A
picture is not a card and keeps its pixels exactly — its corners stay square and
no pixel is cut away.

The card's look is the config's, not the code's (`[pin]` in [the command
line](./cli.md#configuration-and-hot-reload)): a radius, and two skins — idle
and active — each stating a border width and colour and a shadow's blur, spread,
opacity and colour. The canvas blends the two skins on one clock while the
pointer arrives or leaves, so the border and the shadow move together instead of
switching; a skin is a look, and a look that snaps reads as a redraw. The
transition is `0.16 s` ease-out, long enough to be seen and short enough not to
lag the hand.

The border is drawn **outside** the card's rectangle. The canvas paints it as a
sibling node grown by the border's width on every side, with a corner radius to
match, so the stroke fills exactly that outer band. Drawn inside, a border would
eat the first pixels of the content and, on a picture, would hide them; outside,
the card's own size is never negotiated with its decoration.

The corner is a squircle (`CornerShape::SQUIRCLE`, exponent 4), not a quarter
circle. The curve is continuous with the straight edges, which is what a card
looks like when it is cut from one material rather than assembled from two
shapes, and it leaves more area for a thin border to sit in.

### Dragging and where a pin lands

A press anywhere on a card drags the pin. The press lands on whichever node the
pointer hit — a text run, a link, a swatch — and the engine names the drag its
nearest `DRAG` ancestor, which is the card; the canvas asks
`Cx::dragging_within(pin)` rather than comparing the pressed node's id with the
card's. Matching the id works only for a picture, whose box is the topmost hit,
which is why text pins used to ignore the hand.

A drag has no limit while the button is down: the card follows the pointer off
the output and comes back when it is let go. The one place that can decide a pin
is lost is the release. A pin with less than a quarter of itself over its output
is pulled back until exactly that share is over it — no further, so it stays
where the user dropped it as far as it can — and the pull-back is a tween on the
engine's clock, not a jump: the pin slides in over 0.18 s with an ease-out, the
same motion a hand would make. A pin hanging a few pixels past an edge is left
where the user put it.

The shortest way to the threshold is one of three moves, because overlap is the
product of the two axes' overlaps: take the area on one axis (moving in from the
side), or, when neither axis alone has room, take the square root of it on both
(a corner). The candidate that travels the least distance wins, and the pin only
ever moves toward the output. A release during a drag is not a special case: the
settle id is held at zero on every frame of the drag, so the tween has a start
value to travel from.

### The palette's own interaction

A palette is the first kind with behaviour of its own, so it is where the
interaction half of the trait is exercised. Its card has two blocks: the colour
on show, large, beside the swatches it was picked from; and that colour spelled
as hex, `rgb()` and `hsl()`. A swatch is a node the content draws with a `CLICK`
sense; the canvas routes a change on an id it does not recognise to the content
that owns it (`owns`/`change`), and the content moves its selection. The large
block is one node whose fill travels: it asks `Cx::animate_colour` for the
colour the selection calls for and paints the blend it gets back, so a pick
changes a value instead of replacing a node. The list is the tree changing: the
picked swatch leaves it, and the swatch the pick displaced arrives with its own
presence.

The menu's last section belongs to the kind. The canvas always ends with Close,
puts its own Copy, Save and Actual size first, and lets the content's rows sit
between: a palette offers Copy HEX, Copy RGB and Copy HSL for the selected
colour. A row's id is its catalog key, so the words and the action cannot
disagree.

Decoding lives in the factory, not in the kinds:

```rust
pub struct OpenCtx<'a> {
    pub codecs: &'a Codecs,          // the encoder, for export
    pub decoder: &'a Decoder,        // ffmpeg libraries or program
    pub catalog: &'a Arc<Catalog>,
    pub look: PinLook,
}

/// Reads the material, classifies it and builds its content. The only place that maps
/// a `Kind` onto an implementation.
pub fn open(material: Material, ctx: &OpenCtx<'_>) -> Result<Box<dyn PinContent>, String>;
```

### Decoding

Pixels are read through the same ffmpeg authority that writes them. The C shim
gains `cs_decode(path or memory, ...)` beside `cs_encode`: `avformat_open_input`
(a custom `AVIOContext` when the call holds the bytes instead of a path),
`avcodec` for the stream, `sws_scale` to straight-alpha RGBA, and the stream's
colour tags into a `ContentColour`. That keeps one answer to "what can this
machine show", and it reads 10-bit and HDR files with their description instead
of flattening them: a source deeper than 8 bits, or one tagged PQ or HLG, is
converted to `RGBA64LE` and packed into the 2-10-10-10 words
`PixelStorage::Packed10` names, with its primaries and transfer carried along;
everything else arrives as `Sdr8`. Only the first frame of an animated file is
read — a pin is a still. The YUV→RGB matrix follows the stream's own
`color_space` tag (BT.709, BT.601, SMPTE 240M, FCC, BT.2020), because
libswscale's guess is right for the first two and wrong for the last.

The application reads the file once, to classify it, and hands the shim those
bytes, so the Rust side has no path-and-memory pair to choose between. The
memory source seeks within those bytes: an ISO-BMFF picture (AVIF, HEIF) keeps
its index after the picture, and a demuxer that cannot seek back to it reports
the file as partial.

The buffer `sws_scale` writes into carries 128 bytes of tail padding, because
its SIMD converters write whole blocks of 16 pixels and a narrower last row
spills past a tightly packed picture.

When the libraries are missing, the `ffmpeg` program is the same fallback the
writer uses, and only under that condition: `-i - -frames:v 1 -f image2pipe -c:v
png -pix_fmt rgba -`. The program can describe a picture only through a file,
and PNG is the one format both sides already agree on; the answer is read back
by the `png` crate this program writes with. That path is 8-bit sRGB whatever
the file carried, which is what the program can promise.

A PNG's density is context the decoder does not return: the shim hands over
pixels, not metadata, and the program's re-encode drops the `pHYs` chunk. The
pin reads the scale from the bytes it already holds, before either decoder runs.

SVG is parsed by `usvg` and rasterized by `resvg` at the size the pin draws, not
by ffmpeg. A vector that fails to parse reports the parser's line and column.

The `image` crate stays for PNG reading inside the portal and for the settings
process's own pictures; it stops being the pin's decoder, and the daemon's
clipboard conversion now goes through the same authority.

### Rendering and zoom

Zoom is one thing: a paint-time scale of the pin's node, about the pin's origin,
with the hit box derived from the same scale. The canvas does not multiply the
node's *size*: that works for a texture and breaks for a laid-out subtree, whose
glyphs would keep their size inside a larger box. The engine's static scale does
the work.

What the pin stores is a count, not a scale. `apps/crabsnap/src/zoom.rs` draws
at `2^(steps / 32)`: the wheel adds whole steps, so native size (`0`) and every
power of two (`32·x`) are states of the count rather than the product of a long
multiplication. The steps are uniform in log₂ — ×2.18 % each, four of them to a
notch — so the stops between two octaves are spaced by that one metric, and a
path that ends on the count it started from draws at that scale to the bit: nine
notches up and nine down is 100.000 %, not 99.99998 %. Both ends of the range
are lattice points too, so a pin held at a limit is held at a scale another path
can return to.

- `ContainerParams.scale` (default 1) joins `presence_scale` in the paint path;
  the two multiply. The presence part turns about the node's centre, the
  declared part about its own top-left, so a pin grows down and right from where
  it was laid out and the canvas sets the node's offset to the pin's position
  unchanged.
- The text pass rasterizes glyphs at the surface scale times the *declared*
  scale (`TextPrim::shape_scale`), then places them with the full transform. A
  presence tween still stretches the shaped glyphs rather than rasterizing a new
  size on every frame of the animation, which would fill the atlas.
- `Tree::scaled_visual_rect` composes the declared scales with the presence ones
  and the hit index follows it, so a drag starts and a hit lands where the pin
  is painted.
- A drag and the zoom limits use the scaled rect (`Pin::display_size`).
- The zoom range follows the content's own size (`zoom::range`): its long side
  is drawn between 32 and 16384 logical points, inside the shared 0.05–32
  envelope, rounded inward to whole steps. A picture larger than the output can
  still be zoomed in, a pin may hang off its output, and a content too small or
  too large for both ends keeps the one scale the envelope leaves it.
- An image node declares its sampling (`Smooth` or `Pixelated`) on its
  `ImageSpec`; the batch entry carries it, the GPU applies it with
  `glTexParameteri`, the CPU painter takes the nearest texel instead of
  blending, and the texture cache key gains the sampling so one file drawn both
  ways uploads once each. A pin reads its pictures enlarged as blocks and shrunk
  as averages: `card::picture_sampling(zoom)`.

How each kind stays sharp while scaling:

| Kind | What changes on zoom | Cost | Cache |
| --- | --- | --- | --- |
| Bitmap | the sampler; pixels never change | one GPU filter | decoded once per file version |
| Bitmap with marks | nothing: the picture and its marks are one canvas in image pixels | one textured quad | canvas version |
| Vector | a new raster at the drawn pixel size | one resvg pass | LRU by (asset, pixel size) |
| Text, Markdown, Code, LaTeX, Palette | the paint scale; glyphs are shaped at it | shape cache miss while scaling | parse/token caches by source hash |

Vector rasterization quantizes the requested pixel size to eighth steps with one
step of hysteresis, so a wheel sweep rebuilds the raster at a handful of sizes
rather than at every intermediate one. The new raster is made in the build that
first needs it, and that frame paints it; between two rungs the picture is
stretched by less than one step. A raster that would exceed 32 MiB is reduced
uniformly instead of allocated, so a pin covering a 4K output at 2× still works.
`<text>` resolves against a font database scanned once per process, and only a
document that has one pays for the scan: resvg shapes with its own fonts, not
with the text pass's, and usvg needs them before it parses.

Caches are owned by the content, not by a global registry, and are dropped with
the pin. Two exceptions are shared because the same file commonly becomes
several pins: decoded bitmaps (LRU, 256 MiB budget) and vector rasters (LRU, 64
MiB). Both are keyed by canonical path, modification time and length, so a file
rewritten on disk is read again and an untouched one is not read twice. Text
caches — Markdown parse, syntax tokens, math layout — are keyed by the source
string's hash and cleared by the edit that changes it.

### Editing

An editing session is the content's own state: the canvas routes to it and knows
nothing of tools. While it is open:

- The card stays the pin's node, so layout, zoom and the canvas's drag of the
  pin are unchanged. The content draws its chrome in surface points as siblings
  of the card: a one-point frame just outside the picture, an eight-point drag
  band inside it, and the selected mark's box with eight handles. Chrome is not
  part of the canvas, so no export contains it. The tools and their parameters
  are not chrome: they are the floating bar the canvas draws, the same one a
  selection carries ([The Screenshot App](./screenshot-app.md#decisions)).
- The content node takes its own presses, so a drag inside the band draws with
  the active tool instead of moving the pin. The band is a drag node of its own:
  a press there moves the pin exactly as a press on the card does outside the
  session.
- The picture is sampled nearest for the whole session, whatever the pin's zoom.
- The wheel belongs to the editor; zoom is disabled and resumes on exit.
- Entering: the menu's Edit row, `pin edit --id`, or a double click on the
  picture.
- Escape takes only what is unfinished: a popover, a text field (committing it),
  node editing, a live drag. A complex element's exclusive edit ends at its
  commit: a freehand stroke on release, text on save, a Bezier path on its
  first anchor, Enter or the path menu's Finish Path row. Escape or Discard
  Path drops an open Bezier preview; a node mode opened again by hand still
  leaves on Escape. An armed tool stays armed and is not a session. The armed
  tool and the mark selection have chords of their own — `cursor` (`V`) arms
  nothing, `clear` (`Ctrl+D`) puts a selected mark down — so a session holding
  neither closes on one press, and Escape never closes the pin.
- History is the document's: Ctrl+Z and Ctrl+Shift+Z are registered on the
  canvas and reach the editing pin's document.
- The session is view state. Marks are committed to the document as they are
  drawn, and leaving changes no bytes: nothing reaches disk until Save.

```rust
pub trait PinContent {
    fn editing(&self) -> bool { false }
    /// What the floating bar shows: the armed tool, the families, the loupe switch.
    fn edit_bar(&self) -> toolbar::State { toolbar::State::default() }
    /// The armed tool's parameters, in the order the parameter block shows them.
    fn edit_params(&self) -> Vec<toolbar::Param> { Vec::new() }
    /// Whether a press on the content node drags the pin (the band) instead of drawing.
    fn edit_band_drag(&self) -> bool { false }
    /// Chrome in surface points, drained by the canvas after `view`.
    fn take_overlay(&mut self) -> Vec<Container> { Vec::new() }
    /// True when the session consumed Escape: an unfinished gesture, never the tool.
    fn edit_escape(&mut self) -> bool { false }
    /// True when the session put its mark selection down; the tool stays armed.
    fn edit_deselect(&mut self) -> bool { false }
    /// True when the session put the cursor tool back.
    fn edit_cursor(&mut self) -> bool { false }
    /// True when the session consumed Ctrl+Z or Ctrl+Shift+Z.
    fn edit_history(&mut self, redo: bool) -> bool { false }
}
```

The band is a flag rather than a node of its own: the editor's chrome sits over
the content node, so a press that reaches the node while the session is open is
a band press only when the pointer is inside the outer eight points, and the
session reports that through `edit_band_drag`, which is what the canvas asks
before it starts a pin drag. Hit-testing the chrome would put a second drag
owner on the same gesture.

A double click is two completed clicks on the same pin within 400 ms — the
window and the four-point distance the engine already uses to select a word in
read-only text. The canvas reads them from `clicked_within` and the frame clock
before it composes the menu, so the entry row already offers Done in the build
that opens the session, and the gesture keeps no engine state of its own; a pin
whose editor is already open ignores it.

The bitmap editor's tools are the bar's: a rectangle and an ellipse in one
family slot, a line and an arrow in a second, a pen family with the freehand
`Mark::Pen` and the Bezier `Mark::Bezier` in a third, and text on its own. The
bar keeps three family slots; a wheel notch over the pen slot turns to the next
child, and its menu lists both by name. A drag creates one mark and selects it.
The tool stays armed until the cursor is chosen or another tool is.
A mark is hit-tested by its box: the box through its control points for a line,
an arrow, a freehand stroke or a Bezier path, and the shape's own rect for a
rectangle or an ellipse, whose stroke straddles the outline rather than widening
the box; the eight handles resize the selected mark and its interior moves it. A
right-click on a mark offers Delete, on a text mark also Edit Text, and on a
freehand mark Edit Points. The path's right-click menu adds Finish Path and
Discard Path as `MenuItem` rows, `pin-edit/finish-path` and
`pin-edit/discard-path`. `Change::Secondary` still belongs to the loupe, so the
global right-click meaning stays unchanged.

The bar's classes, its families and its parameters are described once, in [The
Screenshot App](./screenshot-app.md#decisions).

The text tool does not drag: a press places a text mark at the snapped pixel
and opens a field over it; saving that field ends its exclusive edit. The field
takes the keyboard without a second click. The field's node owns the buffer and
the mark's text is its one view, the way every field in the engine is built, and
it declares itself a focus stop because it paints no shape of its own. A
keystroke runs through `Change::Text` and the document holds what is on screen;
the field draws the mark itself while it is open, and the canvas leaves it out,
so the text shows once. For that reason a keystroke bumps no canvas version: the
picture under the field does not change. Closing the field makes the mark part
of the picture again — one repaint, and one step of history for the whole
typing, or none at all when the text is blank. A mark reopened through Edit
Text keeps its committed text as what undo returns to. While the field holds
the caret, Ctrl+Z is the input's own undo; the document's history takes it back
once the field closes.

The parameter block belongs to the armed tool and rides inside the bar's pill,
in a lane of its own across the buttons: the paint (a stroke, and a fill for a
rectangle or an ellipse), the stroke width, and an arrow's heads. A choice is
the session's state, so the next mark is drawn with it; when a mark is selected
the same choice restyles that mark in one step of history, which undo takes back
like any other edit. Styles are stored in image pixels like the marks
themselves, so a restyle survives a change of zoom. A text mark takes the ink;
its type size stays the picture's own default until the panel offers one.

A resize maps the mark's bounds onto the dragged box, so the handles stay under
the pointer. The box is four independent sides rather than an origin and a size:
a grip dragged past the side opposite it takes the mark across, which mirrors it
about the side that did not move, and a drag landing square on that side keeps a
2 px sliver there so the mark still has something to grab. The box is the
mark's geometry, not its ink, so the corners land on it exactly and the side
the hand did not take keeps its coordinate; the stroke straddles the line the
box is drawn on, and the selection frame is drawn from those same bounds, so
the next drag starts where the last one ended. The width stays as it was:
`Ctrl` held during the drag makes it follow the box, taking the smaller of the
two axes' factors, and the same rule will cover any later parameter that is
not geometry. `Shift`
holds the box's proportions, a corner scaling about the corner opposite it and a
side spreading the axis it does not touch about the middle. Both keys are read
on every frame of the drag, so either can be taken or let go halfway.

Each kind's editor reuses the model that already exists rather than growing a
second one:

| Kind | Editor | Model | Undo |
| --- | --- | --- | --- |
| Bitmap | annotation tools over the picture | `apps/crabsnap/src/document.rs` — marks plus `Edit` history | the document's |
| Text | multiline text field | the engine's `text_input_multiline` and selection | session |
| Markdown | source, syntax hidden, live preview | `ui_widgets::Markdown::editable` over `ui-richtext` | session |
| Code | source with token ink | `ui-richtext` plus `ui-highlight` | session |
| LaTeX | formula field with preview | `ui-widgets::MathField`, `ui-math` | session |
| Palette | swatches, picker, generation | `ui-color-hct` | session |

The Markdown pin's session is the same control the gallery's editing card draws:
`editable` turns the whole source into one input, a press places the caret, and
syntax shows only where the caret is. The pin enters through its Edit row or
`pin edit --id` and asks the engine for the caret on the frame the session opens
(`Cx::request_focus`), so the first keystroke lands without a second click.
Every keystroke arrives as `Change::Text`: the content replaces its source,
reparses from the previous document (`Document::reparse`, so a keystroke recuts
only the edited window), and the card's measured height follows the next layout.
The source is the buffer, so copy and export write exactly what is on screen.
Escape or the Done row leaves the session, and the pin is read-only again with
no byte changed.

Plain text takes the same session over the engine's multiline field: one input
container over the whole source with a single view at base 0, so a press lands a
byte, Enter inserts a break, and the engine's own history and selection work as
in any field. The bytes remain the pin's source, so the card re-measures and
copy and export write the edited text.

A code pin takes the same field over its highlighted runs: the tokens keep their
ink, the caret and selection are painted over them, and a composition is spliced
into the run under the caret so the composed text keeps that run's ink instead
of flashing back to the base style.

A LaTeX pin is one display formula, so its session edits the formula by
structure rather than showing a source field beside it: the card is
`ui_widgets::Math` with `editable` on, and the engine's math editor keeps the
structure, the caret and its history. Every structural edit comes back as the
formula's own source through `Change::Text`, so the pin stores one string and
copy and export write the edited LaTeX. A formula the parser refuses still draws
its refused runs marked, exactly as the read-only card does.

The annotation of a bitmap is non-destructive: marks live beside the pixels, and
copy and export composite them. Flattening is an explicit action, not a side
effect of leaving the editor.

A pin may also arrive with marks on it: a selection annotated in the overlay
hands over its document rather than a rasterized copy, so the pin opens its
editor on the same marks. The overlay draws on the whole frozen frame and the
pin's grid starts at the crop's corner, so the document arrives translated by
that corner — marks and their history together, or an undo would put one back
in the frame's grid. A cancel in that editor rewinds to the depth the document
had when the session opened, which is the picture as the overlay left it, not an
empty one.

The picture and its marks render as one [canvas](../engine/canvas.md) sized in
image pixels, so the pin draws one quad however many marks it has, and the card,
the editor and the export sample the same target. A pin with no marks draws its
decoded pixels directly, so the canvas costs nothing until something is drawn. A
mark is stored in image pixels and never sees a surface coordinate: the editor
converts the pointer through the pin's rectangle once, and tools draw only in
canvas pixels. Copy and export flatten the same list with the CPU rasterizer at
1:1, including the image, so a file cannot show a different composition from the
card; marks over an HDR picture are refused until the export has a linear
target. While editing, the canvas is sampled nearest, so the mark sits on the
pixel the user pointed at; outside the editor the pin's own sampling applies.

A tool declares what surrounds its mark rather than drawing it into the canvas:
the bounding box, the eight handles, its context-menu items, and any controls of
its own (a pen's anchors and their handles). Those live in surface space at
point sizes, so they stay legible at any zoom and never enter the export. Some
marks need an exclusive edit while they are created, but creation ends the
session at its commit: a freehand stroke exits on release, a text field on save,
and a Bezier path on its first anchor closing the loop. Enter or the path
menu's Finish Path row saves an open Bezier path and exits; Escape or Discard
Path drops the uncommitted preview. An armed tool stays armed and is not an
exclusive session.

A Bezier path draws through `Document::draw`: an `ImageEntry` whose
`ImageSource::Pixels(Arc<Vec<u8>>)` holds its raster, plus an `ImagePrim`.
`resvg 0.46` (`apps/crabsnap/Cargo.toml:22`) supplies `resvg::tiny_skia`,
which rasterizes the cubic path into straight-alpha pixels through the shared
helper in `apps/crabsnap/src/document/path.rs`. The canvas, the loupe and the
export read one `DrawList`, so the preview, the disc and the file show
one path. One path commits one history step. Freehand node editing is a mode
opened again by hand through the mark's Edit Points row. Each of the stroke's
points is a control at its surface position; a drag moves that point, previewed
live in image pixels, and the release commits one step of history for the whole
drag. While the mode is open the eight box handles stand down, because two sets
of controls for one mark would make a press on a shared edge ambiguous, and a
press inside the picture draws nothing until Escape ends the reopened mode. The
band still drags the pin, since leaving the session would otherwise strand the
user. A pen stroke reopens its points through the mark's row, the same way a
text mark reopens its field.

The palette session opens from the Edit row or `pin edit --id` and adds a code
field under the colour it shows: the field's own text is a session buffer, and
only a text that parses as a CSS colour moves the selection, so a half-typed
`#22` leaves the card on the colour it had. While the session is open the menu
grows three rows: Roles replaces the swatch list with the MD3 tonal-spot roles
the shown colour derives, Add appends the shown colour as a swatch, and Remove
drops it while another colour remains. `ui-color-hct`'s scheme layer answers
those roles at the standard contrast level; the curves for the other levels are
not implemented yet. A palette may also be born from a picture: `--palette-from`
quantizes it (Wu's quantizer over a 5-bit RGB histogram, in `ui-color-hct`) to
its eight most populated colours, which are the seed candidates a person picks
from.

The eyedropper is a mode of the session: a menu row arms it, and the next press
is the pick. The pin under the pointer gives up the pixel it draws there — its
decoded bytes, at the point the press covers — and the shown colour takes it.
Where no pin is under the press, the canvas asks the daemon for a fresh capture
of the output the press landed on, and that frame's pixel under the pointer is
the answer: the canvas's own surface is transparent wherever no pin paints, so
the frame already shows the desktop there. The press moves nothing, and a press
on a pin with no pixels (a text card) ends the wait without a colour, as does
one on a compositor that offers no capture protocol. Escape cancels the wait
before it closes the session. A pin's shadow reaches past its card, so a pick in
that ring reads the shadowed desktop rather than the card.

### Saving and export

Edits are in memory until saved. `Save` writes the origin file only when the
file on disk still matches the version the pin read (path, mtime, length);
otherwise and for pins born from a blob it is `Save As`. Nothing rewrites a
user's file without being asked, and nothing auto-saves.

| Kind | Clipboard | `pin export` default | Image encoding |
| --- | --- | --- | --- |
| Bitmap | `image/png` | any image extension | yes, with marks |
| Vector | `image/png` at the drawn size | any image extension | yes |
| Text, Markdown, Code, LaTeX | `text/plain`, plus `text/markdown` for Markdown | the source text, extension by kind | no |
| Palette | the hex list, one per line; the kind's own rows copy the selected colour as HEX, RGB or HSL | `.gpl` | no |

An annotated HDR picture is the one composition a pin refuses: flattening runs
through the SDR rasterizer, and blending marks into PQ or HLG bytes without a
linear pass would shift every pixel around them. `pin export` and the clipboard
answer with that reason instead of writing a wrong file, and a picture with no
marks exports its own bytes untouched. The overlay does not hit this: it
converts an annotated HDR selection to the SDR rendition a file gets before the
marks go on, because a file is SDR either way. A linear flatten is what would
lift the limit for a pin.

Exporting an image format from a text pin is refused with a message naming the
kind, rather than writing an unreadable file. The offscreen readback that would
make it work is a renderer feature with no user yet; a palette therefore writes
back the GIMP file it was read as, not an image strip.

### Protocol and command line

Sources cross the socket, never pixels:

```rust
pub enum PinSource {
    Paths(Vec<String>),   // `pin add --file`: paths the daemon opens itself
    Colour(String),       // `pin add --color`: one CSS colour, parsed by the daemon
    Clipboard,            // `pin add --clipboard`: the selection, read on the loop
}

PinAdd { source: PinSource, as_kind: Option<Kind>, output: Option<usize> }
PinEdit { id: u64 }                  // `pin edit --id`: opens the editor
PinSave { id: u64, path: Option<String> }   // planned
```

`PinEdit` is answered once the session is open, so a client can follow it with
another command; the pin's own menu row calls the same entry, which is why the
two cannot drift. It is idempotent — editing a pin that is already being edited
is a success that changes nothing — and a kind with no editor answers with the
reason instead of pretending. `PinSave` is planned: today `pin export` already
flattens the marks into the file, since copy and export share one composition.

`as_kind` is `--as`: detection and decoding happen in the daemon, so the
override has to cross with the request. It carries the typed `Kind`, which is
also what `PinInfo.kind` reports; both sides therefore spell the kinds with the
classifier's vocabulary and `pin list` prints `Kind::name`.

`VERSION` is 11: `PinEdit` is the eleventh operation. `PinInfo` carries `kind`,
`zoom`, `editable` and `title`, so `pin list` says what each pin holds. A
`Selection` source is answered as soon as the offer is known to carry a usable
MIME — the bytes arrive afterwards, on the loop, and the pins appear in the same
session; `pin list` is the record, not the reply.

The command surface follows the sources:

| Command | Meaning |
| --- | --- |
| `pin add --file PATH...` | One pin per path, cascaded; a bad path is reported by name and the others still pin |
| `pin add --clipboard` | Read the selection: a file list pins one per file, an image pins once |
| `pin add --color '#6750A4'` | A palette pin from a seed |
| `pin add --palette-from PATH` | A palette pin from an image's eight most populated colours; the picture crosses as `--file` with the palette kind pinned, so the daemon quantizes what it decoded |
| `pin edit --id N` | Open the pin's editor |
| `pin save --id N [--file PATH]` | Write the edited content back |
| `pin add --as KIND` | Override detection, for a file whose extension lies |

## Implementation sites

| Where | Responsibility |
| --- | --- |
| `apps/crabsnap-core/src/media.rs` | kind, magic table, `classify` |
| `apps/crabsnap-core/src/uri_list.rs` | `text/uri-list` and `gnome-copied-files` parsing |
| `apps/crabsnap-core/src/transfer.rs` | `Transfer::from_selection`: files or one blob |
| `apps/crabsnap-core/src/protocol.rs` | v11 messages, `PinSource::Clipboard`, `PinEdit`, `PinInfo` fields |
| `ui/crates/ui-platform/src/transfer.rs` | `Selection` and the `SelectionReader` trait |
| `ui/crates/ui-platform-wayland/src/clipboard_host.rs` | `ext_data_control_v1` writing and reading the selection |
| `apps/crabsnap-platform/unix/ffi/ffmpeg_shim.c`, its crate's `shim.rs` | `cs_decode`: demuxer, decoder and `sws_scale` to RGBA or 2-10-10-10, with the stream's colour tags |
| `apps/crabsnap/src/ffmpeg/decode.rs`, `capture.rs` | the authority's read side: libraries or program, tags to `ContentColour`, the program's PNG read back |
| `apps/crabsnap/src/pin.rs`, `pin/content.rs`, `pin/source.rs`, `pin/edit.rs` | pin, content kinds, acquisition, sessions |
| `apps/crabsnap/src/pin/content/text.rs`, `pin/content/palette.rs`, `pin/content/colour_code.rs`, `pin/ink.rs` | the text family's card, the palette's two blocks, the CSS colour forms, the card's colours |
| `apps/crabsnap/src/pin/content/vector.rs` | the SVG: `usvg` parse, `resvg` raster on the rung ladder, the shared raster LRU, PNG at the drawn size |
| `apps/crabsnap/src/document/path.rs` | Bezier anchors, cubic geometry, the capped straight-alpha raster, and the helper vector pins share |
| `ui/crates/ui-core/src/cx.rs` | `register_image`: a picture a build made, drained into the surface's texture table before it paints |
| `apps/crabsnap/src/pin/card.rs` | the shared card: shell, fill, shadow, outside border, skin blend, squircle corners |
| `apps/crabsnap/src/capture.rs` | `Image` as the bitmap content's source |
| `apps/crabsnap/src/zoom.rs` | the zoom lattice: a step count to a scale, a wheel delta to steps, a content's range as step bounds |
| `ui/crates/ui-core/src/tree.rs` | static `scale` declaration |
| `ui/crates/ui-gpu-scene/src/renderer.rs` | per-entry sampling |
| `ui/crates/ui-color-hct/src/scheme.rs` | MD3 role palettes from a seed |
| `ui/crates/ui-color-hct/src/quantize.rs` | a picture's most populated colours (Wu) |
| `apps/crabsnap/locales/{en,zh-CN}/crabsnap.ftl` | menu rows and every error |

## Verification

Pure logic, unit-tested:

- `two_clicks_are_a_double_click_only_inside_the_window` — a click 100 ms and
  400 ms after the last one is the second half of a gesture, 500 ms after it is
  the first click of a new pair, and a clock that went back is not a double
  click either.
- `marks_over_an_hdr_picture_are_refused_and_a_plain_export_copies_nothing` — a
  PQ picture with no marks exports its own bytes (`Arc::ptr_eq`), and one with a
  mark refuses with a reason naming HDR instead of flattening to SDR.
- `a_pin_edit_survives_the_wire_and_its_ok_is_an_answer` — the request keeps its
  id across the socket, and its `Ok` passes the gate that pairs a response with
  its operation.
- `a_markdown_session_owns_the_source_it_edits` — a keystroke is refused while
  the pin is read-only, the session's whole-buffer change replaces the source
  and the parse with it, copy reads the same bytes, and Done leaves the session.
- `every_text_kind_but_a_palette_offers_the_session` — plain text, code and
  LaTeX take the session and its whole-buffer change; the text family has no
  palette arm, because a palette pin is another content whose session edits
  swatches.
- `a_palette_session_edits_the_colour_it_shows_and_the_list_it_writes` — the
  session refuses keystrokes before it opens, takes a code that parses while
  keeping one that does not, generates the roles from the shown colour, adds and
  removes swatches, and the clipboard and export read the edited list.
- `a_picture_quantizes_to_the_colours_it_is_mostly_made_of` — a picture of three
  known blocks comes back as their colours in population order, a two-colour
  request merges the smaller blocks into one, and the populations always sum to
  the pixels counted.
- `a_palette_picks_a_colour_or_ends_the_wait` — the row arms the pick, a colour
  off another pin's pixel moves the shown colour and the field, a press that
  found nothing ends the wait without changing it, and Escape cancels the wait
  before the session.
- `a_pick_takes_the_pixel_under_the_press_and_moves_nothing` — through the
  engine: a press over a bitmap while a palette waits gives the palette that
  pixel's colour and leaves the bitmap where it was.
- `a_pick_on_bare_canvas_asks_for_the_screen` — through the engine: the same
  press where no pin lies leaves the wait open and asks the daemon for the
  output the canvas covers, at the press point.
- `a_press_maps_to_the_captured_pixel` — a press in a canvas's logical space
  lands on the frame's pixel at the same share of both axes, and a press outside
  it finds none.
- `a_typed_mark_is_one_step_of_history` — a text mark is drawn while it is
  typed, a keystroke bumps no canvas version, and closing the field leaves one
  undo step that restores the text and ends its exclusive edit.
- `a_freehand_stroke_finishes_on_release` — a freehand stroke commits on
  release and leaves its exclusive edit; the armed pen tool stays armed.
- `a_closed_bezier_path_leaves_its_session` — a press on the first anchor
  commits the closed path and exits; Escape and Discard Path drop an open
  preview.
- `an_open_bezier_path_is_one_step_of_history` — Enter or Finish Path saves
  the open path and exits, and undo restores the whole path in one step.
- `a_box_runs_through_the_control_points` — the box is the envelope of the
  control points, including a Bezier path's anchors and handles.
- `escape_leaves_the_tool_and_the_selection_to_their_own_chords` — Escape
  leaves a node mode reopened by hand, but leaves the armed tool and the mark
  selection to `V` and `Ctrl+D`; a session holding neither closes on one press.
- `a_uri_list_parses_every_line_form` — CRLF, LF, comments, percent-encoded
  spaces, `file://localhost/…`, `file:///C:/…` kept verbatim, a bare absolute
  path.
- `a_selection_splits_into_files_and_blobs` — `text/uri-list` and a GNOME list
  become one path each, `image/png` stays one blob, and an image claim whose
  bytes are an SVG stays a blob (the classifier, not the MIME, decides).
- `plain_text_that_is_only_file_uris_is_a_file_list` — one and two `file:` lines
  under `text/plain` become paths; a sentence, a mixed list, an `https` URI and
  an empty payload stay text.
- `detection_prefers_the_signature_over_the_name` — PNG bytes named `.txt` are a
  bitmap, `<svg` bytes named `.png` are a vector, `%PDF` is unsupported with its
  first bytes named.
- `the_text_family_is_decided_by_extension` — `.md`, `.tex`, `.rs`, `.txt`,
  `.todo` (Markdown), and an unknown extension over valid UTF-8.
- `a_colour_round_trips_through_every_spelling` — hex, `rgb()` and `hsl()` of
  the same colour; hex and `rgb()` are exact, and `hsl()` comes back within the
  two steps per channel that whole degrees and percent can cost.
- `the_css_spellings_people_write_all_parse` — commas, spaces, slash-alpha,
  uppercase, percent channels, `deg`, and a colour space this build refuses by
  name.
- `a_gimp_palette_reads_its_columns_and_names`, and
  `a_single_colour_pin_lists_no_swatches_but_keeps_its_codes` — a `.gpl`'s own
  columns and colour names; a `--color` pin has no list to show but keeps its
  three code rows.
- `a_path_list_opens_every_readable_file_and_names_the_bad_one` — three paths,
  one missing; two contents keeping their request indices and one error naming
  it.
- `a_cascade_steps_and_stops_at_the_edge` — the first item is the single-pin
  spot, each later one steps 24 points, nine of them stay distinct, and the far
  edge clamps.
- `a_count_draws_at_one_scale_however_it_was_reached` — zero steps is exactly
  1.0, every 2^-4 … 2^5 is exactly a step multiple, and a wheel path that nets
  no notches is native again.
- `the_wheel_and_the_range_land_on_the_lattice` — one notch is four steps, a
  trackpad's quarter notch one, both ends of a content's range are lattice
  points inside the envelope, and a content outside it keeps one scale.
- `a_contents_zoom_range_comes_from_its_own_size` — a picture larger than the
  output keeps a maximum above 1, a dot gets a floor of its own, both ends stay
  inside the drawn-size bounds, and a size of nothing falls back to the shared
  range.
- `a_release_keeps_a_fixed_corner_on_the_output` — each axis moves by its own
  deficit, an axis already showing the handhold's side stays put, and a huge pin
  comes back by the handhold rather than by a share of itself.
- `only_the_kinds_with_an_editor_say_so` — the capability table, one assertion
  per kind.
- `a_seed_yields_the_md3_roles_of_the_tonal_spot_scheme` — every role of one
  seed sits at the tone material-color-utilities' `MaterialDynamicColors` names,
  each role is drawn from the palette the table gives it (the primary keeps the
  seed's hue at chroma 36, so it is a duller colour than MD3's chroma-47.9
  baseline purple, not the seed itself), and the roles that carry a contrast
  curve meet it against the surface or container they sit on.
- `a_vector_raster_is_reused_within_a_step_and_rebuilt_across_one` — the cache
  key.
- `ffmpeg_colour_tags_become_the_description_the_renderer_reads` — PQ over
  BT.2020 primaries arrives as `Pq`, `BT2020`, `Packed10`; a Display P3 tag
  keeps its primaries; an untagged 8-bit picture is `ContentColour::SDR`; and
  gamma 2.2 stays a power curve rather than becoming sRGB.
- `a_picture_whose_index_sits_at_the_end_decodes_from_memory` — a 2×2 AVIF with
  its `meta` box moved last and 128 KiB of padding, so the demuxer must seek
  back past the read buffer; decodes to its two red texels.
- `a_zoom_keeps_the_raster_until_it_crosses_a_rung` — the entry the pin hands
  the canvas: the same `Arc` within a step, a bigger one across it, and a raster
  whose rows read from the top with straight alpha.

Through the engine, headlessly in `apps/crabsnap/src/main.rs`:

- `a_wheel_there_and_back_returns_to_native_exactly` — a real canvas, synthetic
  wheel events: eight notches is exactly 2×, a wandering path that nets no steps
  keeps the scale, and eight notches back is 1.0 with the drawn box equal to the
  content's own size.

Through the engine, in a nested session:

- `a_pixelated_pin_has_hard_pixel_edges_and_a_smooth_one_does_not` — the same 2×
  enlarged picture in both modes, sampled at a colour boundary.
- `zoom_keeps_text_sharp` — a text pin at 3×, glyph edges compared with the same
  text laid out at 3×.
- Pins opened from a JPEG, a WebP, a 16-bit PNG and a PQ-tagged matroska frame.
  Sampled one pixel — the same coordinate in the source and in the pin's export:
  the JPEG, the WebP and the 16-bit PNG land within two codes of the source, and
  the matroska frame comes back as its tag says it should, brighter and shifted,
  because its codes were sRGB values labelled PQ.
- `pin edit --id` on a Markdown pin or a plain text pin: the session opens with
  the caret on the last line (plain text's caret sits on the empty line a
  trailing break leaves), and the Markdown source is revealed where the caret
  stands; a code pin keeps its token ink with the caret on the line after the
  source; a LaTeX pin opens with the structural caret inside the formula; a
  second request changes nothing; `pin export` writes the source byte for byte;
  a palette pin opens with the code field under its codes, the field focused and
  the caret after the hex, and `pin export` writes its GIMP palette; a vector
  pin still refuses with "no editor yet".
- `pin add --palette-from` on a picture of three flat blocks: the pin lists the
  three colours in population order, each exactly the block's own bytes (the
  five-bit clustering does not round a flat block), and `pin export` writes them
  as a `.gpl`.
- `pin add --clipboard` against the nested compositor's `ext_data_control_v1`
  offer: a two-item `text/uri-list` from `wl-copy` pins two bitmaps with their
  cascaded positions, an `image/png` pins one bitmap, and a plain-text selection
  pins one text card with its two lines. An empty selection and a second source
  are refused by name.

Checked outside the suite: `cs_decode` under address sanitizer (PNG, JPEG, WebP,
16-bit PNG, PQ matroska) — no leak and no double free, and the deep path returns
`Packed10` with alpha 3 for an opaque texel.

Not testable here, and verified by hand: the exclusive-keyboard handover while
editing. A file-manager selection is the `text/uri-list` the nested `wl-copy`
reproduces above; its payload is read through the same data-control offer.

The double click is not driven either: nothing in the nested session injects a
pointer, so its window is covered by the predicate test and the wiring by the
engine's own click record, which lives for the one build that reads it.

The eyedropper's screen half has the same limit: the press that asks for the
capture cannot be injected, so what the canvas records and how a press maps to
the frame's pixels are covered by tests. The daemon's capture path was run for
its other uses in a nested session — `shot output` wrote a 2560×1440 frame
through the same `start_capture` the pick now takes.

## Corrections

**The zoom was a product, so it never came back and never reached 2×.** The
wheel multiplied the scale by `1.1^(value / 40)` on every event, so 100 % was a
value the arithmetic could approach but not resume: nine notches in and nine out
left the pin at 0.9999998, a trackpad's seven small deltas each way at
1.0000005. The octave was not a state either — `1.1^7` is 1.9487 and `1.1^8` is
2.1436, so the label read 195 % then 214 % and never said 200 % however slowly
the wheel turned. The pin now stores an integer step count and derives the scale
from it, which makes native size and every power of two exact by construction;
`zoom::tests::a_count_draws_at_one_scale_however_it_was_reached` and
`a_wheel_there_and_back_returns_to_native_exactly` (a real canvas and synthetic
wheel events) pin it. The position keeps a float recurrence about the pointer,
where a round trip stays within a thousandth of a pixel: the scale is what the
label and the drawn box come from, and that is the part that had to be exact.

**The palette signature is `ASEF`, not `ACSE`.** Adobe Swatch Exchange files
begin with the four bytes `41 53 45 46`; the design table had them transposed.
The classifier reads `ASEF`, and a `.gpl` still begins with `GIMP Palette`.
Checked against the reverse-engineered format description at [carl.camera's ASE
page](https://carl.camera/default.aspx?id=109) and the magic bytes listed by
[file-extension.info](https://file-extension.info/format/ase).

**An SVG is the root element, not a byte sequence somewhere in the head.** The
standard sniffs `<svg` anywhere in the first kilobytes, which classifies an HTML
page with an inline `<svg>` as a vector. The classifier skips a BOM, a prolog, a
comment and a doctype and then requires `<svg` to open the root element; HTML
keeps being code, by its extension.

**The code table is the classifier's, not the highlighter's.** The design said
"an extension `ui-highlight` names is code"; that grammar set is two languages
of the pin's many, and a `.py` pin must still be a code pin (monospace, no
colour) rather than plain text. `crabsnap-core` owns the extension table, and
`ui-highlight` only decides how a language it knows is coloured.

**The cascade does not wrap after eight items.** The paragraph asked for a `(24,
24) * k` step that wraps every eight steps and for two pins of one list never
landing exactly on each other. Both cannot hold: item 8 wrapped lands exactly on
item 0 and hides it, which is the one outcome a cascade exists to prevent. The
step is unwrapped and clamped to the output, and
`a_cascade_steps_and_stops_at_the_edge` fails if a wrap is put back.

**`--as` needed a field on `PinAdd`.** The command table had `--as KIND`, and
detection runs where the file is read — in the daemon — but the request carried
no override, so the option could only have worked without a daemon. The request
now carries `as_kind`, typed as `Kind` so the command line, the socket and `pin
list` share one spelling. The compiler keeps the two ends agreeing: a request
that forgets the field no longer builds.

**A text pin's size is learned from its first layout.** The trait said `size()`
is the laid-out size capped at the maximum width, but nothing has laid the card
out when `size()` first runs: the pin is placed, and the cascade stepped, before
any frame exists. `size()` now answers a provisional height (lines × line
height), the canvas reads the real rect back after the first layout, and the pin
shifts by half the difference once so the centre it was placed by stays. A
`settled` flag makes the shift run exactly once; a per-frame shift would walk
the pin off the output.

**A palette pin writes a GIMP palette, not an image strip.** The saving table
promised an image strip, but no content can read its own rendering back yet, so
the palette writes the `.gpl` it was read from and refuses image formats with
the same message as the rest of the text family. The strip arrives with the
offscreen readback.

**ASEF is detected but not read.** The magic table names `ASEF` a palette and
the reader parses only GIMP files, so an `.ase` pin fails with a message that
names the way out — export it as a `.gpl`. Detection and reading are separate
steps, and the error belongs to the second; refusing it earlier would hide the
kind the user has.

**A pin drags from anything on its card, not only from the card's own box.** The
canvas compared the pressed node's id with the pin's node id, which holds only
for a picture: its box is the topmost thing hit. A press on a markdown paragraph
lands on a text node, so the comparison failed and the drag was never armed —
the pin could be hovered and scrolled but not moved. The canvas now asks
`Cx::dragging_within(pin.node_id())`, which the engine answers from the nearest
`DRAG` ancestor of the pressed node, exactly the node it armed the drag on.
Checked in a nested session by dragging a markdown pin by its text (it moves)
and by the palette by a swatch (a press that travels moves the pin, a press that
does not selects).

**A change made from outside the app's turn must wake the surface that shows
it.** `pin close` and `pin hide` set a flag on a board the daemon shares with a
running canvas; the canvas reads that flag in `view`, and `view` ran only when
input or a new texture woke it. The closed pin stayed painted until the pointer
crossed the canvas, which reads as a command that did nothing.
`MessageCtx::request_frame` hands a handler the loop's own door: it marks the
surface for a build and wakes the loop, and the daemon calls it with the
canvas's surface after any pin change. A screenshot after `close`, with no input
in between, now shows the pin gone.

**The border sits outside the card.** The first card drew the ring at the card's
own size, so the stroke covered the outermost pixels of the surface — on a
picture, of the picture. The ring is now the card grown by the border width on
every side, offset by the same amount, and its radius grows with it; the stroke
fills exactly the outer band and the content keeps every pixel it laid out.

**Todo was not a kind.** It carried no behaviour of its own: the same parse, the
same card, the same clipboard, the same export as Markdown, and the checklist
rows were already Markdown's. A kind that only renames a file is a second
spelling of one thing, so `.todo` and `.tasks` classify as Markdown and
`TextKind` lost the variant. Nothing on screen changed; the name in `pin list`
did.

**A lost pin comes back only as far as the threshold, and it animates.** The
first release limiter shoved a pin completely onto its output, which threw away
the drop position and made a pin dragged past a corner leap inward. Overlap is
the product of the two axes' overlaps, so the shortest way to the quarter-share
is a one-axis move or, when neither axis alone has room, the square root on
both; `settle_position` picks the cheapest and places the pin on that axis only.
The move is a tween (`SETTLE_BACK`, 0.18 s ease-out) whose zero is written on
every frame of the drag — an animation id that is new snaps to its target, so
the release must have a start value waiting. Checked in a nested session by
dragging a picture pin past the top-left corner, where the visible edge grew 20
→ 45 → 65 → 80 → 93 → 99 → 104 px before resting. The quarter-share threshold
itself was replaced by a fixed handhold (below).

**A pin's zoom is a paint scale, not a bigger box.** The canvas multiplied the
content's size by the zoom, so a text pin reflowed at the old font size inside a
wider card and a picture was stretched by the box. The zoom is now
`ContainerParams::scale` on the card — about the node's own top-left, with the
ring and the shadow taking the same scale — and the canvas lays every card out
at its own size. Two things had to become real for that to be worth anything:
`Tree::scaled_visual_rect` now composes declared scales (its old fold also
applied nested presence scales outermost-last, which is the opposite of paint),
and the text pass rasterizes glyphs at the declared scale — until now
`TextPrim::zoom` was only ever applied as a stretch.
`a_declared_scale_paints_and_hits_from_the_node_origin` fails if either the
paint composition or the hit-box composition is dropped (checked both ways). In
a nested session: a picture at 7.4× shows exactly the four colours its source
has on a scanline — no blend at any stripe edge, so the sampler is nearest — and
at 0.077× those exact colours are gone, because the sampler averages; a markdown
pin at 3× has crisp glyph edges with its lines unchanged; the 9 px active border
of the visible-border config sits outside the card at three times its width with
the corner curves still concentric.

**The big swatch flashed the card white on every pick.** The block was keyed by
the colour it showed, so a pick replaced the node, and the new one entered with
`Presence::fade` while scaling up from 45 %. An opaque block fading in over the
card's surface shows that surface: sampled in a nested session with the swap
stretched to 1.5 s, the block read `(107, 85, 166)` purple → `(202, 224, 204)`
near-white → green, so the picked colour never appeared. A departure alone would
not have fixed it — two semi-transparent layers over white still show white
where both are half painted — and holding the old node would have left it on
screen after the pin closed, since the card declares no exit. The block is now
one node whose fill `Cx::animate_colour` carries, all four channels on one
clock; the same sample reads purple → `(93, 93, 142)` → green with no light
frame in between, and two picks 60 ms apart land on the second colour.

**A vector raster is made in the frame that asks for it.** The design expected
the old raster to be stretched for a frame or two while a new one is made
somewhere else. There is nowhere else: an `App` builds and its surface presents
in the same turn, and nothing wakes a surface when a worker finishes, so
`Vector::image` rasterizes inside the build and that frame paints the result.
The hysteresis is what bounds the cost — a wheel notch is 1.1×, a step is
`2^(1/8) = 1.09`, so a sweep rebuilds every second notch rather than every one.
Measured in release on this machine: a 240×160 document with text takes 0.5 ms
at 480×320, 6.8 ms at 1920×1280 and 21 ms at 3547×2365, and the first document
with a `<text` in a process also pays a 600 ms scan of the system's fonts — a
document without one skips the scan and the 21 ms sizes are what the zoom limit
actually allows. The surface is double buffered, so a slow build shows a late
frame, not a torn one. Caught by
`a_zoom_keeps_the_raster_until_it_crosses_a_rung`, watched red with the rebuild
removed (the entry kept its old size and its old `Arc`).

**A pin added to a canvas that was already up never appeared.**
`MessageCtx::add_image` — and `MessageCtx::request_frame` itself — found the
surface and set the loop's `pending_draw`, but never asked *the surface* for a
frame; that flag only keeps the loop from sleeping, and a surface with no damage
and no deadline has nothing to draw. The first pin of a batch opened its canvas
and drew, so a single drop looked right, while every later pin, every `pin add`
onto a live canvas and every `pin show`/`hide` stayed invisible: `pin list`
showed it, the board held it, the screen did not. Both doors now call
`SurfaceHandle::request_frame`. Caught by
`a_texture_reaches_the_surface_that_survived_and_makes_it_build` in
`crates/ui-platform/tests/host_loop.rs`, watched red with the request removed
(`requested` read 0 where it reads 2).

**A custom `AVIOContext`'s buffer is not the caller's to free by the pointer it
allocated.** The first `cs_decode` kept the `av_malloc`'d read buffer and freed
it after `avio_context_free`. libavformat replaces a custom input buffer with
one sized for the demuxer and frees the original, so that free was a double free
of memory the allocator had already taken back — and the buffer the context
ended up with leaked, because `avio_context_free` releases the context, not its
buffer. The shim now frees `io->buffer` (the one in use) and then the context.
Caught by an address-sanitizer harness around `cs_decode` on a PNG: a double
free of the 65536-byte region, then a leak of the same size once that free was
removed; both clean now.

**A memory source that cannot seek fails every ISO-BMFF picture.** The first
`cs_decode` read from the bytes with a read callback only. AVIF and HEIF keep
their index (`meta`) after the picture, so the demuxer seeks to the end, finds
it, then seeks back to the sample — and without a seek callback the position
stays at the end, the read comes back short, and the file is reported as
partial: a real 2560×1440 AVIF failed with "no picture in the bytes" while
ffmpeg's own command line read it. The reader now answers `AVSEEK_SIZE`,
`SEEK_SET`, `SEEK_CUR` and `SEEK_END` within the bytes it was given. Caught by
`a_picture_whose_index_sits_at_the_end_decodes_from_memory`, whose fixture is
larger than the demuxer's read buffer and has its boxes ordered the same way —
watched red as that very message with the callback removed.

**The program cannot hand back a raw picture without also saying how big it
is.** The design's fallback was `-f rawvideo -pix_fmt rgba -`, a pixel stream
with no header: the reader would have to ask the program for the size in a
second run, or infer it from the byte count, which is ambiguous for any frame
that is not a known shape. The fallback writes PNG instead (`-f image2pipe -c:v
png`), which carries its own width, height and layout; the app reads it with the
`png` crate it already writes with. The path is 8-bit sRGB, which rawvideo would
have been as well.

**Density is not part of a decode.** The shim answers with pixels and colour,
and the program's PNG loses the `pHYs` chunk on the way through — so a pin that
took its scale from the decoder would place every 2× screenshot at half its
intended size, and lose it entirely under the fallback. The pin reads the scale
from the bytes it already holds, before either decoder runs. Checked with `pin
list` after pinning a 2× file: the size stays in the points the capture wrote.

**`sws_scale` writes past a tightly packed destination.** The shim allocated
exactly `width × height × 4` bytes for the converted picture. libswscale's SIMD
converters emit a whole block of 16 pixels per row whatever is left of it, so
the last row of a narrow picture runs past the end of the buffer: 60 bytes at
one pixel wide, 56 at two, nothing from sixteen on — and the next chunk's header
is gone. A 2×2 AVIF written by this program's own encoder then aborted in
`avformat_close_input` (`free(): invalid next size`) while freeing the demuxer's
private data, nowhere near our own code. A canary harness against the installed
libswscale measured the overrun across widths 1–20; the `RGBA64LE` path is
scalar here and overran nothing. The destination now has 128 bytes of tail
padding that the picture copy never reads. ffmpeg's own frame buffers need none:
`av_frame_get_buffer` left 1344 spare bytes on a 2×2 `BGRA` frame, so only our
`malloc` was tight. Caught by
`a_picture_whose_index_sits_at_the_end_decodes_from_memory`, which encodes that
2×2 AVIF and aborts without the padding.

**The selection's MIME list survives its own `selection` event.** Wayland sends
`data_offer`, then the offer's `offer` events with each MIME, and only then
`selection`, which names the same object that `data_offer` created. The first
reader cleared its MIME list on every `selection`, so the list it had just
received was wiped a moment later and `pin add --clipboard` saw an empty
selection while `wl-paste -l` listed six types. The clear now happens only when
the offer object actually changes. Verified against the nested compositor:
`offers ["text/uri-list", …]` where it used to read `[]`.

**A daemon with no surface never read its own connection.** The clipboard host's
queue was only dispatched, never read: in a process whose only window is a pin
canvas that may not exist yet, nothing calls `prepare_read`, so the
`data_offer`/`offer` events sat in the socket. `ClipboardHost::service` now
reads what already arrived before dispatching, the same shape `Outputs::list`
uses; another queue mid-read leaves it to the next turn.

**A content without pixels did not redraw an open canvas.** `place` requested a
frame only when the new pin brought an `ImageEntry`, so a text pin (or any card
with no texture) added to an already-open canvas stayed invisible until
unrelated input forced a rebuild — `pin add --clipboard` of a text selection
showed it. The texture-less branch now calls `request_frame`; the clipboard path
is what first added a texture-less pin to a canvas that already existed.

**`Transfer` lives next to the uri-list parser, not in the platform crate.** The
design table put it in `ui-platform`, but `Files` needs the `text/uri-list`
parser, which lives in `crabsnap-core`, and the portable platform crate must not
depend on the app. The split is: `ui-platform` defines `Selection` and the
`SelectionReader` trait (raw MIME and bytes), `crabsnap-core` owns
`Transfer::from_selection` (paths or one blob), and the daemon joins them. The
MIME remains a hint: the classifier reads the bytes either way.

**A file handed over as plain text was pinned as text.** Some applications offer
a copied file only as `text/plain` whose whole payload is its `file:` URI; `pin
add --clipboard` read that as one blob and pinned a text card showing the URI,
where the user had copied a picture. `Transfer::from_selection` now takes a text
payload of nothing but `file:` lines as a file list, and any other text — a
sentence mentioning a path, a mixture, an `https` URI — stays one blob. Caught
by `plain_text_that_is_only_file_uris_is_a_file_list`.

**`pin add --clipboard` blamed the files when it meant the daemon.** With no
daemon behind the socket, `request` returns `None` and `pin add` falls back to a
local pin; that fallback was handed an empty file list whenever the source was
the clipboard, so the command answered "nothing could be pinned" while the
clipboard held a picture. A source with no local form now says what is missing:
"no daemon is running: the clipboard must be read where pins live". A selection
whose owner closes the pipe without writing is reported as empty instead of
becoming a blank text card. Verified by hand with the daemon stopped, then
started, on the same selection.

**The seat `data_device` read is not wired.** The design named it the fallback
when `ext_data_control_v1` is absent, but an offer follows keyboard focus, and a
daemon holds no focused surface; taking the keyboard from the user to read a
clipboard is the behaviour the data-control channel exists to avoid. Writing
still falls back to the seat source, so a compositor without data-control keeps
its copies; reading one reports "this compositor offers no clipboard to read".
The Windows and macOS backends arrive with those platforms.

**Zoom limits came from the screen and could sit below the pin's own scale.**
The maximum was the smaller of a fixed ceiling and the content's fit on its
output. A 3840×2160 picture pasted on a 2560×1440 output fits at 0.67, below the
1.0 the pin starts at, so the first wheel notch clamped the pin *down* to 0.67
and no later notch could grow it back. The range now comes from the content's
logical size — its long side drawn between 32 and 16384 points, inside the
shared 0.05–32 envelope, with the envelope as the fallback for a content with no
measurable size — and the output does not enter it, so a pin may be larger than
the screen it sits on. Caught by
`a_contents_zoom_range_comes_from_its_own_size`.

**The pull-back threshold was a share of the pin.** It kept a quarter of the
pin's area over the output, so a full-screen pin dragged nearly off was thrown
back most of the way across the screen, and the share said nothing about how the
visible corner was shaped. The corner kept is now a fixed `KEEP_AREA` of 48²
square logical points: the side is the reference for both axes, an axis already
showing that side is left alone, and only the shorter one is brought in, so a
pin that kept one long edge does not drag a bar further across. Both axes must
show the side, so a very wide pin cannot rest on a few points of height even
though its visible area is large. Points are the unit the compositor scales, so
the handhold is the same physical size at 1×, 1.5× or 2×. Caught by
`a_release_keeps_a_fixed_corner_on_the_output`, which states the long-edge and
wide-pin cases; watched red by forcing the "already shows enough" branch off,
where the long axis moved too.

**Marks landed near the surface origin, and the text field went with them.**
`Space::snap` already converts a *surface* point into picture pixels, and two
call sites converted first: the pen's anchor drag passed
`space.snap(space.to_image(pointer))`, and the text tool passed
`space.snap(from)` where `from` was `space.to_image(origin)`. Both therefore
computed `(image − rect.origin) / scale` — the picture coordinate read as a
surface one. On the overlay's 1024×576 selection at (768, 288) a press drew its
mark nearly a thousand pixels up and to the left of the picture, and the text
field was placed from that mark, on the surface's own origin rather than under
the pointer: the field took the keyboard and the typing went into a box the user
could not see, which is what "the text tool cannot type" looked like. The call
sites now hand `snap` the surface point and nothing else, and a nested-session
run of the text tool put the field's rect at (968, 438) — the pressed pixel,
`768 + 200` by `288 + 150` — with the mark under it. No test pins the pair: the
`Space` conversion itself is covered by `the_space_maps_both_ways`, and what
separates the two readings is the call site, which the type does not yet
distinguish.

**The pin editor's loupe was in the tree and painted nothing.** The session's
chrome added the magnifier with `Container::from_decl`, which then took only a
*childless* declaration and kept children in its own slot; the loupe is a
finished subtree — a disc, a zoom plate and two readings — so every node below
its root was dropped. `from_decl` asserted that in a debug build, and the app
ships `--release`, where the assert is compiled out, so the frame was valid and
empty: a probe of the disc's rect read `None` in a pin's editor while the
overlay, which splices through `child_decl`, drew the same disc from the same
`loupe::build`. The session splices the subtree too, and `from_decl` now adopts a
declaration's children rather than refusing them, so the trap cannot reach a
later caller. What catches it now:
`from_decl_adopts_the_children_of_the_declaration_it_wraps`, and a nested-session
screenshot of the editor's disc.

**The wheel only zoomed while the secondary button was held.** `Session::wheel`
asked `Loupe::held`, which is the button's visit, so the bar's switch — the one
that sets `toggled` — left the disc up and the wheel dead. It now asks
`visible(false)`, the same question the chrome asks before it paints a disc, so
what the wheel turns is always what is on screen. The two states stay separate:
a held button is still a visit that ends on release, and the switch is still a
setting that survives it.

**Escape spent itself on the tool and the mark before it left.** The session's
layering was "a popover, a text field, node editing, a drag, the selection, the
armed tool, and only then the session", so leaving an editor cost one press per
layer the user had not noticed was up — three on the way out of a session that
held a marked mark and an armed tool. The key now takes only what is unfinished;
the armed tool and the mark selection have chords of their own, `cursor` (`V`)
and `clear` (`Ctrl+D`), so a session holding neither closes at once. What
catches it now: `escape_leaves_the_tool_and_the_selection_to_their_own_chords`
and `escape_and_history_walk_their_own_layers`.

**The box stood off the shape's own points.** The bounds added half a stroke
around a line and, when it carried a head, a whole head length — `width * 5` —
to both of the box's extents, though a head is drawn back along the segment and
inside the points' own box. On the default 3 px stroke an arrow's box reached
16.5 image px past its tip and grew as much in height, which put its grips where
no part of the mark was. The box is the envelope of the control points alone
now, for a line, an arrow and a pen; a rectangle and an ellipse kept their half
stroke for a while longer, since their ink straddles the outline the frame is
drawn on, until the drift described two paragraphs below took it out too. A flat
envelope, such as a horizontal line's, is squared about its centre to 6 px,
because its four grips would otherwise coincide and its height could not be
dragged at all. What catches it now: `a_box_runs_through_the_control_points`,
which fails on the head allowance for an arrow and on the minimum for a flat
line.

**A resize could not turn a mark round.** The target was a `Rect`, whose size is
positive by construction, so a grip dragged past the side opposite it was
clamped there: a line drawn from top-left to bottom-right stayed pointing that
way however its box was dragged, and the endpoint away from the hand travelled
with it instead of holding still. The target is a `Sides` — four coordinates,
each keeping what its grip gave it — and the mapping takes the sign of both
extents, so a crossed box mirrors the mark about the side that did not move.
`rebased` also scaled the stroke with the box every time, shrinking a 3 px line
to 1.5 px on a short drag; the width now keeps its size unless `Ctrl` is held.
What catches it now: `a_grip_dragged_past_its_opposite_mirrors_the_line` and
`a_resize_rebases_the_mark_and_escape_restores_it`, which pins the width both
ways and the `Shift` proportion.

**A stroke's half width in the box made the opposite corner drift.** The
paragraph above kept half a stroke in a rectangle's bounds, because its ink
straddles the outline; `rebased` then mapped that grown box onto the dragged
sides, so the shape's own corner did not stay where the grip opposite it was
held. The error is half a stroke times one minus the scale: on a 3 px stroke
dragging a 30 px side out to 15 px moved the fixed corner 0.75 px, and a
square's corner slid diagonally on every drag. The box of a rectangle or an
ellipse is the rect itself now, ink or no ink: the frame is drawn on the
geometry, as the handles already were, and the stroke straddles it. What
catches it now: the corner assertion in
`a_resize_rebases_the_mark_and_escape_restores_it`, which compares the mapped
bounds against the dragged box exactly rather than within a stroke; watched red
with the inflation put back, where the same drag reported 24.25 for a 25.0
side.

**The text field was a text leaf, and a leaf cannot be an input.** The field's
node was the `ui_widgets::text::Text` the mark draws, with `text_input` asked
of it. That setter writes into container parameters and does nothing on a text
node, so the field held no buffer: the engine found no input owner, no
`Change::Text` could be produced, and every keystroke fell past the field. A
text leaf also takes no press (`HitBehavior::Ignore` without `Select`) and is
no focus stop unless it paints, so the request for the keyboard had nothing to
land on either; what the user saw was a mark with handles that took no typing.
The field is a container now, holding the buffer and declaring
`focus_stop(true)` because its only paint is the text below it, with the text
as its one view under `field.with("text")` — the shape `LineEdit`, the MD3
field and the multi-block editors already have. It hugs a caret slot's width at
least, since an empty mark's buffer measures nothing and an empty rect is not a
stop. What catches it now: a nested-session run of the text tool, where typing
`abc` put three glyphs and a caret in the mark. No unit test pins it: the
engine's field tests all build a container input, so none of them ever entered
the leaf, and a test for this would have to drive the app's whole chrome.

**We believed creating a complex mark enters an exclusive mode that owns the
pointer until Escape.** A finished freehand stroke had to be dismissed before
the next mark, and a closed Bezier path kept an edit session with nothing left
to edit. The pen family now holds freehand and Bezier children, and creation
ends at its commit: release, save, first anchor, Enter or Finish Path; Escape or
Discard Path drops an open preview, Edit Points reopens node editing, Escape
leaves it, and an armed tool stays armed. What catches it now:
`a_freehand_stroke_finishes_on_release`,
`a_closed_bezier_path_leaves_its_session`,
`an_open_bezier_path_is_one_step_of_history` and
`escape_leaves_the_tool_and_the_selection_to_their_own_chords`.

## Status

Implemented today: image pins from a capture or any file ffmpeg can read, SVG
pins, drag, wheel zoom, the three-row menu, export through the codecs, and the
annotation editor over a picture: shape, line, freehand, Bezier and text tools,
marks in the picture's own pixels, its history, and its parameters and colour
picker on the floating bar. A pin pinned from an annotated selection keeps
those marks in its own document. `pin
add` takes any number of `--file` paths: the daemon classifies each head before
decoding it (`--as` overrules the verdict), places the opened ones as a cascade
on one canvas, and answers with every failure named by path while the rest still
pin. `pin list` prints each pin's kind, title, size, position, source and
visibility. `pin add --clipboard` does the same from the selection: a file list
pins one per file, an image or a text blob pins once, and the command answers as
soon as the offer is known to carry something a pin can show.

The read-only text family is in: plain text, Markdown, code, LaTeX and GIMP
palettes each pin as a light card — squircle corners, a shadow, and an outline
drawn outside the card — with the shared Copy/Actual size/Close rows. Text kinds
lay out once at a maximum width and are *scaled* when the pin is zoomed, so the
glyphs are shaped at the drawn size and stay sharp instead of a wider card
reflowing them at the old size; take `text/plain` (plus `text/markdown` for
Markdown) on the clipboard, and write their source back. A code pin highlights
the languages `ui-highlight` knows and stays monospaced otherwise; LaTeX goes
through `ui_widgets::Math`; `.todo` and `.tasks` are Markdown. A palette pin
draws the colour on show beside the swatches it came from and that colour's hex,
`rgb()` and `hsl()`; a swatch click moves the selection (an animated swap), the
menu copies the selected colour in any of the three forms, the clipboard carries
the `#RRGGBB` list and the export writes a `.gpl`. `pin add --color CODE` opens
a one-colour palette from a CSS code, hex or `rgb()` or `hsl()`; `pin add
--palette-from PATH` quantizes a picture into the eight colours it is mostly
made of. A picture is the exception: it is not a card, keeps square corners and
is never rounded. Text pins have no texture, so the canvas treats a pin's image
entry as optional and draws only the node. An SVG pin is the other side of that:
its raster follows the drawn size, so the build hands the engine a new picture
when the wheel crosses a rung of the eighth-step ladder, and copy and export
rasterize at the drawn size too. A bitmap is whatever the machine's ffmpeg can
read, and it keeps the colour description the stream carried. A bitmap, every
text kind — Markdown, plain, code and LaTeX — and the palette have their
editors. The palette's session edits the colour on show through a code field,
generates the MD3 roles from it, picks a pixel from another pin's picture or
from the screen under the press, and adds or removes swatches; copy and export
read the edited list.

`apps/crabsnap/src/pin.rs` holds the content object: `Pin` no longer knows
pixels, only `Box<dyn PinContent>`, and the contents draw their own node and
answer copy, export and their own interactions. The trait is the subset the
kinds use — `kind`, `title`, `size`, `image`, `view`, `rounded`, `clipboard`,
`export`, and the default-empty `menu`, `act`, `take_copy`, `owns`, `change`,
`colour_at`, `picking`, `picked` — because editing and capabilities have nothing
to say until a kind differs in them; each arrives with the kind that first needs
it. `image`, `clipboard` and `export` are told the size the pin draws at: only a
kind that renders for the screen has a use for it, and the canvas is the one
place that knows the zoom. `open` takes `(path, dots, as_kind, ffmpeg)` for the
same reason: the decoder is passed in, not looked up, so opening a file has one
source of "what this machine can show". Drag, wheel zoom, copy, save and close
go through the trait exactly as before, and a drag starts from any node on a
card. A released pin that lost its output is tweened back until a fixed handhold
stays over it, the zoom range comes from the content's own size, and the border
and shadow follow the pointer's visit on one blended clock, so a hover arrives
instead of switching.

Planned, in this order:

1. `media` and `uri_list` helpers, `pin add` taking many paths, one pin per
   item, protocol v7 (**done**; v8 added the colour source). The pin still holds
   a bitmap and nothing on screen looks different.
2. `PinContent` for the bitmap (**done**, no visual change); the engine work —
   static scale, per-entry sampling, the sampling row — is B5 (**done**).
3. `usvg`/`resvg` for SVG and its raster cache (**done**), then `cs_decode`
   (**done**), so a pin reads every format the writer offers, with the colour
   description the file carried.
4. Text, Markdown, code, LaTeX and palette pins, render-only (**done**, with the
   card's look, per-kind menu rows and the palette's selection); the engine work
   for scaling a laid-out pin is B5 (**done**).
5. Editing: annotation (**done**), Markdown (**done**), plain text (**done**),
   code (**done**), LaTeX (**done**), palette (**done**: role generation at the
   standard contrast level, a picture's colours as its source, and a pixel
   picked from another pin or from the screen).
6. Clipboard acquisition (**done** on Wayland through `ext_data_control_v1`):
   `pin add --clipboard` reads the selection on the loop, turns a uri-list into
   one pin per file and any other payload into one pin, and answers before the
   bytes exist. The Windows and macOS transfer backends arrive with those
   platforms; the seat `data_device` read is not wired (it needs focus).

## See also

- [The Screenshot App](./screenshot-app.md): the canvas, drag and zoom a pin
  starts from.
- [Pin Badges](./pin-badges.md): the chrome a pin wears — zoom, HDR and the file
  it came from.
- [Selection Readouts and the Loupe](./magnifier.md): the magnifier a held
  secondary button raises while a bitmap pin is being annotated.
- [Command Line, Daemon and Settings](./cli.md): the daemon, its channel and the
  configuration these commands read.
- [In-Place Markdown Editing](../engine/markdown-editor.md): the source-and-
  decorations model the Markdown pin edits with.
- [Colour](../engine/colour.md): `ContentColour`, which every bitmap pin
  carries.
