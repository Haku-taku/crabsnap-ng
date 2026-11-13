# Canvases

A canvas is a render target the app declares, painted from primitives in its
own pixels and sampled by the frame like a picture. It exists for content whose
pixel grid is not the surface's: an annotated bitmap edited at 1:1 under a
zoomed pin, a document rasterized once for export. One target serves the
editor, the card and the export, and the surface tree keeps only a textured
quad.

## Problem

A pin draws an image with `params.scale` and samples it per draw. Annotations on
that image cannot follow the same route. Marks written as ordinary scene
primitives are re-rasterized at surface scale on every frame, so a zoom softens
or hardens every edge and a glyph's size follows the surface rather than the
document. A laid-out subtree inside the pin fails the other way: the engine's
scale multiplies the node's paint but not its layout, so growing the box leaves
glyphs at their old size.

Surface coordinates are also the wrong home for a mark. The pointer arrives in
surface points; the mark has to be stored in image pixels or it moves when the
pin is dragged or zoomed. The conversion needs a Space, and the drawing code
must not see it.

Finally, rebuilding the marks every frame costs what the document has, not what
changed, and a mark list hashed into the scene damages the whole pin node.

## Comparison

Qt's `QGraphicsItem::ItemCoordinateCache` is the model closest to this one: a
fixed-resolution raster in item coordinates, drawn through the view transform,
so zoom magnifies the raster instead of re-rendering it (`qgraphicsitem.cpp`,
`_q_paintIntoCache`). Its `DeviceCoordinateCache` re-rasterizes at device scale
on every scale change, which Qt accepts because the cache is only a speed
optimization: entries live in the global `QPixmapCache` (10 MiB by default) and
may be dropped, so `paint()` never depends on a hit.

Flutter's raster cache keys a display list by its unique id and the current
matrix, with translation zeroed (`flow/raster_cache_key.h`), caches only after
three visible frames, and evicts every entry not seen in the last frame
(`flow/raster_cache.cc`). Its surfaces are always N32 premultiplied 8-bit even
when the destination is wide-gamut; Impeller disables the cache entirely.

Chromium tiles content at 256 px on the CPU and 512 px is its untiled threshold
(`cc/layers/tile_size_calculator.cc`), gives each tile a one-texel apron, maps a
damage rect to tile indices including the apron, and holds a 64 MiB default
memory policy with a 3000 px interest area. Partial raster into an existing
texture is off by default because redrawing a sub-rect is only safe when the
content's filters and blending are idempotent.

GTK4 keeps a retained render-node tree per widget and derives damage by
diffing two trees (`gsk/gskrenderer.c`); a plain `GskTextureNode` always filters
linearly, and `GskTextureScaleNode` is what exposes nearest or mipmapped
sampling per node. Skia's picture-image cache key is only issued for sRGB
content under an identity or translation matrix (`SkImage_Picture.cpp`), inside
a 32 MiB resource cache.

| Model | What its cache keys on | What we take |
| --- | --- | --- |
| Qt item cache | item, fixed logical size | content-resolution target, zoom samples it |
| Flutter raster cache | display list id + matrix, translation removed | the app's version is the id; zoom leaves the key |
| Chromium tiling | tile index, contents scale | damage maps to tiles once one target is too large |
| GTK node tree | subtree re-record | not applicable: a canvas has no tree |

## Contract

```rust
pub enum ImageSource {
    Pixels(std::sync::Arc<Vec<u8>>),
    Canvas(CanvasId),
}

pub struct CanvasId(pub u64);

pub struct Canvas {
    pub id: CanvasId,
    pub size: (u32, u32),
    pub version: u64,
    pub list: DrawList,
}

Cx::canvas(&self, canvas: Canvas)
```

- A canvas has no layout, no hit index and no events. Its list is in canvas
  pixels, origin at the canvas's top-left, and it is drawn with the same
  primitive types as the frame.
- `version` is the whole cache key, together with `size`. The app bumps it in
  the edit that changes the picture; a restyle counts, because a restyle moves
  no bounds but changes pixels. Every node that samples the canvas is damaged
  through the fingerprint, which hashes `(canvas id, version)`, so an unchanged
  version paints nothing even when the list is rebuilt with different contents.
- `ImageEntry.source` names the canvas where a picture's bytes used to be. The
  node then samples it with its own `Sampling` and `ImageFit`, so the same
  target serves an editor reading it nearest and a card reading it smooth.
- `DrawList::canvas_owner()` is the push owner for every primitive in a canvas
  list: a node id no arena hands out, so a stray owner lookup cannot land on a
  real node and drag its clip or style into the canvas.
- Text in a canvas list is resolved at scale 1.0, once, in the build that
  changes the version. A canvas is rasterized at its own size; the surface's
  scale applies when it is sampled, not when it is built.
- The engine never reads a canvas back. The CPU presenter holds the bytes (an
  `Arc`, so sampling copies nothing) and the GPU presenter keeps a texture; a
  canvas entry that reaches a sampler with no target behind it draws nothing
  rather than a stale buffer.
- A canvas absent from a frame loses its target, and the engine drops its
  recorded version with it: a canvas that comes back is repainted instead of
  sampling bytes nothing holds any more.
- Deliberately absent: canvases do not nest (a canvas list samples pictures,
  not other canvases), do not animate on their own, and are not repainted when
  the app mutates the list behind an unchanged version.

The frame path is: build collects `Canvas` declarations, `resolve_canvases`
compares `(version, size)` against the targets it holds, a changed canvas has
its text resolved, and the presenter paints the changed ones before compositing
the frame. The CPU presenter renders at scale 1 into `Pixels` and converts to
straight RGBA; the GPU pass owns a `LayerTarget` per canvas and draws into it
directly, so no frame-sized copy is made.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `crates/ui-core/src/draw.rs` | `Canvas`, `CanvasId`, `ImageSource`, `canvas_owner` |
| `crates/ui-core/src/cx.rs` | `Cx::canvas` collects the frame's declarations |
| `crates/ui-platform/src/view.rs` | version cache, text resolution, CPU painting, damage hashing |
| `crates/ui-platform/src/present.rs` | `CanvasFrame`, the per-frame contract with a presenter |
| `crates/ui-paint/src/lib.rs` | `encode_rgba_straight`, sampling a canvas entry |
| `crates/ui-gpu-scene/src/renderer.rs` | one target per canvas, painted at scale 1 before the frame |
| `crates/ui-platform-wayland/src/session.rs` | maps the engine's canvases to the presenter's uploads |

## Corrections

The first CPU painter reused the presenter's buffer bytes as the picture's
bytes. `Pixels` is premultiplied BGRA while `PixelStorage::Sdr8` reads three
bytes as R, G, B, so a red column sampled back as blue: the test read
`[0, 0, 255, 255]` where its red had been stored as those same bytes. The root
fix is a conversion that owns the contract:
`encode_rgba_straight` unpremultiplies and reorders in one pass, and its name
says which order it produces. `canvas_pixels_reach_the_node_at_the_canvas_scale`
fails if the order or the premultiplication drifts.

An engine effect layer was the first candidate for the target. It is keyed by
bounds and damage and painted in place at surface scale, and it lives inside
one node's paint, so it cannot be sampled by another node and cannot survive a
zoom without re-rasterizing. A second tree with its own layout and paint was
the second candidate; paint is coupled to the main tree's node walk, so a
second tree means a second painter and a second set of primitives. A
surface-sized canvas for the whole pin was the third: it loses the image's
pixel grid, which is what annotation coordinates are stored in. The declared
canvas replaces all three.

The first GPU pass freed image textures inside each call to the upload sync.
With a canvas list synced before the frame's own list, every sync evicted the
other's pictures, so a canvas and the frame sharing a source image uploaded it
twice per frame. The fix moves eviction to the end of the frame and keys it on
the union of keys every list named, which is what `frame_keys` holds. The
example's `[16b]` and `[7]`..`[9]` checks fail if the union or the per-picture
reuse regresses.

Per-mark ink caches were rejected for the same reason: one version-keyed target
subsumes them, and it has one key to get right instead of one per mark.

The GPU pass stamped a canvas target's last use only when it repainted it, and
freed every target whose stamp was not this frame's. A canvas that went one
frame unchanged was therefore freed right after being sampled, and the next
frame failed with "sampled but was never painted". Sampling counts as use, and
the unchanged branch stamps the target too. `[21]` in the GPU example samples
one unchanged canvas on two frames in a row and fails on the second without the
stamp; the same bug ended the app's session the first time an annotated pin
presented a still frame.

## Verification

- `view::tests::contracts::a_canvas_repaints_only_when_its_version_changes`
  builds a canvas, changes the list without the version, and changes the
  version without the geometry: the pixels must follow the version alone.
- `view::tests::contracts::canvas_pixels_reach_the_node_at_the_canvas_scale`
  stretches a 2×2 canvas over an 8×8 box with nearest sampling and checks each
  half: canvas pixels land on the node at canvas scale with no offset, and in
  the right channel order.
- `view::tests::contracts::a_canvas_that_leaves_and_returns_is_repainted`
  drops the declaration for one frame and brings it back with the same version:
  the returning canvas must be painted from its list, because its bytes went
  with the frame that stopped naming it.
- `crates/ui-paint-gpu/examples/gpu_image_scene.rs` checks `[16]`..`[21]`: a
  canvas reaches the frame, painting it uploads no picture, an unchanged
  version keeps its target and a new one replaces it, the filter is chosen per
  draw — nearest keeps the texel seam hard while smooth blends across it — and
  a still canvas survives every frame that samples it, not only the one after
  its paint.

The example needs a render node, so it runs outside the unit suite:

```sh
cargo run --release -p ui-paint-gpu --features egl-probes --example gpu_image_scene
```

## Status

Implemented: the declaration, the id/version cache, text resolution, CPU and
GPU painting, sampling through `ImageEntry`, and damage keyed by the canvas
version.

The GPU pass owns one `LayerTarget` per canvas, painted at scale 1 into a
target the size of the canvas before the frame's own target is touched. Its
texture is sampled by the frame with the filter of each draw, so nearest and
smooth need no second target. Image uploads are freed against the union of the
main list and every canvas list, which is what lets a canvas sample a picture
the frame itself never names.

The annotated bitmap is the first client: a pin whose document holds a mark
draws picture and marks into one canvas at the image's pixels, and flattens the
same list through the CPU rasterizer for a copy or an export, so the screen and
the file cannot disagree. A pin with no marks keeps the decoded pixels, which
costs no second buffer.

Partial: a canvas list may hold primitives and clips, not layer effects or
backdrops; one that declares either is refused with the reason. Sampling is
chosen per `ImageEntry`, so one canvas read two ways declares two entries. A
flattened export is SDR: marks over an HDR picture are refused until the export
has a linear target.

Planned: a tile split with a one-texel apron and a memory budget once a single
target exceeds the texture limit.

## See also

- [Effects](./effects.md) — the layer cache a canvas deliberately is not.
- [Presentation Targets](./present-targets.md) — where a presenter's targets
  live and what it may keep between frames.
- [Colour and HDR](./colour.md) — the storage and transfer functions a canvas
  target has to match.
- [Pins](../apps/pins.md) — the annotated bitmap that motivates canvases.
