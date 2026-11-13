# Colour and HDR

A pixel buffer's bytes never travel without a description of what they
mean. Storage format and working domain are separate decisions: SDR stays in
8-bit sRGB and blends in the encoded domain, while HDR stores linear light
and blends there. Treating "HDR support" as one branch in one function
is the mistake this model exists to prevent.

The same separation applies to content. A capture, a pin or an imported
image carries the description it was made with; the display has its own.
Neither is rewritten to fit the other.

## Vocabulary

| Type | Meaning |
| --- | --- |
| `TransferFunction` | `Srgb`, `Linear`, `Pq` (ST 2084, absolute), `Hlg` |
| `Encoding` | Transfer function plus premultiplication; `is_linear()` |
| `ColorSpace` | `Srgb`, `DisplayP3`, `Bt2020` primaries |
| `BufferFormat` | `Bgra8Srgb`, `Rgb10a2`, `Rgba16Float` |
| `OutputDescription` | Colour space, transfer, peak and black luminance, from the compositor |
| `SurfaceColour` | What one surface presents and blends in: `Sdr` or `Hdr` |
| `ContentColour` | The description an image or capture was authored with |
| `ToneMapping` | The curve that maps content above the display's peak |

`Encoding::SDR_PREMULTIPLIED` is what every SDR path produces;
`LINEAR_PREMULTIPLIED` is the HDR working domain. Premultiplication is in
the type because it cannot be inferred from bytes.

## Two descriptions

`SurfaceColour` belongs to a surface and comes from the output it is on.
`ContentColour` belongs to pixels and never changes after they are made.
Conversion happens when content is drawn, never when it is stored.

A texture built for one domain is a cache, not the content. Discarding it and
rebuilding it from the original pixels is always allowed, and is what a
display change does.

## Choosing a buffer format

`BufferFormat::choose` decides from the transfer function, not the peak:
if the output is not PQ or HLG, it returns 8-bit sRGB. Every HDR output
takes f16.

Colour depth is not what rules 10-bit out: `ABGR2101010` spends its last two
bits on alpha, and interface content carries its antialiasing and shadow
ramps in alpha, so a soft shadow collapses into four steps. Eight bits of
alpha is the floor for a UI buffer; 10-bit RGB only suits opaque video.

8-bit is not enough for HDR because dark gradients band before the peak is
reached. The same rule explains why the choice is not "HDR display means
10-bit": an 8-bit `wl_shm` surface on an HDR monitor still cannot carry
values above 1.0.

DRM fourccs include alpha — `ABGR2101010` and `ABGR16161616F`, never
`XRGB…` — because the compositor blends the result.

## Encoding has one home

`ui_core::colour` owns the transfer functions, the gamut matrices and the
single PQ output path:

```rust
encode_for_pq_output(linear, reference_nits, min_nits)
```

The display's SDR white and black level come from the compositor, never from
a constant; the shader transcribes this function rather than re-deriving it.
If encoding lived in several places, a change to primaries or tone mapping
would have as many copies as there are output paths, and two disagreeing
copies show up as a colour cast on one path only.

## How a linear destination is encoded

A linear destination also states which encoding its bytes carry.
`LinearEncoding::Pq` is PQ over BT.2020, which is what a Wayland colour manager
asks for; `LinearEncoding::ScRgb` is linear light in sRGB primaries whose 1.0 is
`SCRGB_WHITE_NITS` (80), which is what a DXGI float swap chain carries on an
advanced-colour display. The two differ in gamut and in where white sits, so the
renderer keeps the choice and the output pass has one variant per encoding. The
scRGB variant scales the scene's reference white into 80-nit units and writes the
light unencoded; PQ leaves that to `encode_for_pq_output`.

## Blending domain

The SDR path blends encoded values. This is deliberate, and the difference is
not cosmetic: 50% white over opaque black gives 128 in the encoded domain
and 187 when decoded, blended, and re-encoded. Switching SDR to linear
blending would brighten every translucent surface, so it is an appearance
change, not a bug fix.

HDR stores linear values in the buffer, so composition needs no decode; the
same measurement means the two paths are two data streams, not one function
with a flag. `composite_over_in` remains as a diagnostic that quantifies the
gap.

The domain follows the surface. Both domains use premultiplied alpha and the
same blend equation, so one shader source serves both: only the passes that
cross the encoding boundary — the present pass and the capture pass — are
compiled as two variants, selected by `#define`.

## Surface colour {#surface-colour}

One value per surface, produced by the platform layer and consumed by the
presenter:

```rust
enum SurfaceColour {
    Sdr,
    Hdr(HdrSurface), // reference/min/max nits, gamut matrix, buffer format
}
```

`Hdr` is derived from the output's own image description and declared on the
surface; a constructed description claims a 10000-nit peak and is mapped
down by the compositor. A surface that cannot declare HDR stays `Sdr` and
keeps rendering.

The value is not a mode and not a build feature. Two surfaces in one process
can hold different values, and the CPU presenter is always `Sdr`.

### Runtime switching

| Trigger | Source |
| --- | --- |
| The surface moved to another output | `wl_surface.enter` / `leave` |
| The compositor changed what the surface should show | `preferred_changed` on the surface feedback |
| The output's luminances or primaries changed | the output description's events |
| The declaration was rejected or the format is unsupported | the declaration's `failed` event, the dma-buf format list |

A switch is one transaction: recompute `SurfaceColour`; rebuild the target
pool when the buffer format changed; change the renderer's domain; invalidate
every cache keyed by the domain (image textures, the colour atlas, layer and
scratch targets); repaint the whole surface.

One `wp_color_management_surface_v1` is created per surface and reused for
every later declaration: a second `get_surface` for the same `wl_surface` is a
protocol error that kills the connection. Reverting to SDR is
`unset_image_description`, not dropping the object.

Failure at any step keeps the previous value and the previous frame, and logs
the reason once.

## Content colour {#content-colour}

```rust
struct ContentColour {
    storage: PixelStorage, // Sdr8(RGBA8) | Packed10(one 32-bit word) | Float16(scRGB)
    primaries: Chromaticities,
    transfer: TransferFunction,
    max_nits: f32, // zero when nobody stated one
}
```

Every capture and every pinned image carries one. Capture stores the native
pixels and their description; converting to sRGB 8-bit is an export step, not
an ingest step. A texture is built from the content for one domain and one
tone-mapping choice, and is discarded when either changes.

There is no white level in the type. PQ codes are absolute in nits; the sRGB
and gamma codes of SDR content mean "the desktop's white", and the
destination is that same desktop, so the destination's reference is the only
anchor either direction needs.

`Float16` is the one storage that is already in light: scRGB half floats, where
1.0 is `SCRGB_WHITE_NITS` (80 nits) and a highlight above white is simply a
value above 1.0. Both are absolute, so the packing itself carries the anchor
and no field is needed. A float texel skips the transfer decode — `decode`
clamps at 1.0, which is precisely the light such a frame exists to keep.

`ContentColour::placement(destination)` derives the transform once per
texture: a gamut matrix to the working domain's sRGB primaries with Bradford
adaptation, a gain from content light to destination-white units, and the tone
curve when the content claims more than the destination holds. Placeholders
for content whose description could not be read keep the codes and read them
as sRGB, which is the least a compositor's numbers can be trusted for.

Placement by pair:

| Content | Surface | Treatment |
| --- | --- | --- |
| SDR | SDR | unchanged: the codes are handed over and never leave eight bits |
| SDR | HDR | SDR white at the surface's reference nits; no curve |
| HDR | HDR, peak sufficient | gamut conversion only |
| HDR | HDR, lower peak | tone curve to that peak |
| HDR | SDR | tone curve to the SDR peak |

Only the first row is the identity, and it is the one every existing SDR
picture takes. A picture is never rewritten: `Image` and `Cropped` in the app
carry the native bytes and the description, and each renderer builds its own
texture from them.

## Tone mapping {#tone-mapping}

Applied in this order: decode to absolute luminance, convert primaries, place
in the working domain, then apply the curve as a ratio on the luminance so
hue survives. The engine's own interface is authored SDR and never passes
through the curve; on an HDR surface it is only placed at the reference
white. A destination whose peak covers the content's gets no curve at all.

| Strategy | Character | Source |
| --- | --- | --- |
| `none`, `clip` | hard clip to the peak; pixel-exact for comparison | — |
| `bt2390` | 1:1 below the knee, Hermite roll-off above it, applied as a ratio on RGB so hue survives | ITU-R BT.2390 §5.4 |
| `bt2446a` | broadcast luminance mapping with chroma compensation | ITU-R BT.2446 |
| `spline` | single-pivot polynomial; mpv's gpu-next default `[unverified]` | libplacebo via mpv `--tone-mapping` |
| `reinhard` | global `L/(1+L)`, cheap, flattens highlights | Reinhard et al. 2002 |
| `hable` | filmic S-curve from Uncharted 2 | Hable, GDC 2010 |
| `aces` | filmic fit; lifts midtones and shifts hue | Narkowicz 2015 |
| `agx` | Blender's default; resists ACES's hue shift | Blender 4.0 |
| `pbr_neutral` | near-identity below the shoulder, desaturates highlights to white | KhronosGroup/ToneMapping |

The settings shape, with only keys that have a consumer:

```toml
[hdr]
tone_mapping = "bt2390"
target_peak = "auto"  # or nits
sdr_white = "auto"    # or nits, for SDR content on an HDR surface
```

The default for HDR to SDR is `bt2390`: it is the standard curve, has no look
of its own, and needs only the two peaks. `pbr_neutral` is the alternative
for colour-critical work, and `clip` is the control.

`Clip` and `Bt2390` are implemented; the rest of the table is the shape the
setting will offer. `Bt2390` normalises both peaks to the source's, puts the
knee at `1.5 · target/source − 0.5` and runs a cubic Hermite from the knee to
the target peak, with the same slope as the straight part at the knee and
zero slope at the top.

Which peak bounds the curve is the content's *measured* light, not
`ContentColour::max_nits`: the latter is the display's capability, so a float
capture of an SDR desktop states 1000 nits while holding 240. `with_peak` swaps
the measured value in for the placement. Measuring means one pass over the
pixels, which an app runs off the frame: a pin queues it on the job worker and
a file measures only when it is written. A file written from a picture above
the destination's white now gets `bt2390`; below it the curve is skipped and the
rendition is a plain conversion. Display always uses the destination's curve.

A file destination can also keep the light instead of rolling it. An HDR picture is written as
PQ codes over BT.2020, whose peak is the container's 10000 nits, so the curve has nothing left
to do; the app picks that destination when the measured peak is above the display's own white,
and a desktop whose brightest pixel *is* that white stays in sRGB codes, where white means the
viewer's white. Reading such a file back is the same placement run backwards, into the scRGB
light a capture of an HDR screen arrives as.

## Where the description comes from

- The Wayland backend reads the output's image description through
  `wp_color_management_v1` when the compositor offers it, and adapts white
  points with Bradford by default.
- `GpuWindow::open` asks the window for an HDR surface and follows the answer
  (`WaylandWindow::declare_hdr_surface`): when the output's peak is above SDR
  and its transfer is PQ, it declares the output's own description, presents
  f16, and encodes to PQ at that output's reference white and gamut.
  `SurfaceColour::Sdr` opts a surface out; a compositor without colour
  management leaves every surface SDR.
- The CPU path produces `SDR_PREMULTIPLIED` and the same encoding point.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui-core/src/colour.rs` | transfer functions, gamut matrices, format choice, `ContentColour`, `Placement`, tone curves |
| `ui-platform-wayland/src/window.rs` | reads output descriptions, declares the surface, builds `HdrSurface` |
| `ui-platform-wayland/src/gpu_window.rs` | turns `SurfaceColour` into a destination, a target pool and renderer state |
| `ui-gpu-scene/src/renderer.rs` | working domain, target formats, per-destination texture builds, present pass |
| `ui-paint/src/lib.rs` | the CPU painter's samples, through the same `Placement` |
| `apps/crabsnap/src/color.rs` | reads the compositor's description into a `ContentColour` |
| `apps/crabsnap/src/capture.rs` | keeps the native frame and its description; the SDR rendition for a file |
| `apps/crabsnap/src/pins.rs` | a pin hands its texture the pixel description it was cut from |

## Capture

A screenshot is a picture of the interface, not of the buffer: `GpuRenderer::capture`
reads the persistent frame before the present pass, so an HDR or 10-bit session
writes the same SDR image an SDR session does. The presented buffer carries the
output transfer — an HDR one holds PQ codes — and a float attachment cannot be read
back as bytes on every driver, so reading it gives codes or zeros instead of a
picture.

A supersampled frame is downscaled by the same resolve pass that presents it, and an
f16 frame is resolved into an 8-bit scratch first, so a capture is the same picture
whatever the internal precision or sample count.

A grab of the screen keeps the pixels and description the compositor delivered:
a `Xbgr2101010` frame stays a buffer of 10-bit words, with `X`'s undefined
alpha bits made opaque, which is a repack and not a colour operation. Its SDR
rendition is produced when a file asks for one (`Cropped::rendition`) and
never replaces the original. An advanced-colour desktop arrives as scRGB half
floats instead, eight bytes a texel, and travels the same way: the bytes are
copied row by row and the description says `Float16`, so the light above white
reaches the pin and only the destination decides what to do with it.

The frame also carries the display's SDR white, and a picture carries it on as
a `Light` through every crop and bake, together with whatever peak has been
measured from the pixels. A crop of part of the frame starts a new `Light`: its
peak is its own, and the whole-frame crop shares the measurement because it is
the same pixels. Windows reports the white it maps SDR to; a Wayland output
description states a peak and no white, so an absolute capture there is measured
against the model's own 203 nits.

## Traps

| Change | Symptom |
| --- | --- |
| Keep the format next to the buffer instead of in the type | A buffer is interpreted as sRGB when it holds PQ, or vice versa |
| Choose the format from peak luminance alone | HDR content on an 8-bit buffer bands in dark gradients |
| Pick 10-bit for HDR because its colour depth suffices | `ABGR2101010` has 2 bits of alpha: shadows and antialiased edges band |
| Blend SDR after decoding to linear | Every translucent surface jumps brighter |
| Blend HDR in the encoded domain | Blur and alpha compositing pick the wrong brightness |
| Hardcode 80 or 100 nits as SDR white | The same UI looks different on a display with another reference level |
| Add a second encode step for an overlay or screenshot | A cast appears on exactly that path |
| Use `XRGB` fourccs | The compositor composites without your alpha |
| Capture the presented buffer | An HDR screenshot is black, or holds PQ codes instead of a picture |
| Allocate a target with a type its internal format rejects | The allocation fails silently and the target keeps its old size or stays empty |
| Settle a control on `Color::lerp`'s `t = 1` | The resting colour is one float off the token; settled values take `Color::lerp_exact` |
| Convert content to the display's description when it is imported | A pin shown on an SDR output never returns to HDR when the output switches |
| Carry a white level in `ContentColour` and scale PQ by it | Absolute luminance divided twice, so every HDR pin on an SDR surface comes out dark |
| Read a `Xbgr2101010` word as BGRA byte order | Red and blue swap only in 10-bit content, which looks like a gamut bug |
| Squeeze a float desktop into eight bits at ingest | Every highlight above white lands on 1.0, so the pin looks the same on an HDR surface |
| Decode scRGB floats with the sRGB transfer | The decode clamps at 1.0 and throws away the only light the packing was for |
| Run a tone curve when the destination's peak already covers the content | Every highlight dims a little for no reason |
| Keep one global HDR mode, or read it from the environment | Two surfaces on different outputs cannot both be right |
| Destroy the surface feedback after reading it | The compositor's change of mind is never heard |
| Call `get_surface` again when re-resolving | Protocol error "CM surface already exists" and the connection dies |
| Tone map interface colours | The same control changes appearance with the display's peak |

## Corrections

**10-bit was chosen for HDR from the peak luminance.** `choose` returned `Rgb10a2` up
to 1000 nits, reasoning from colour depth alone; and the surface on an HDR output was
left SDR unless a caller opted in by environment variable, so the compositor mapped
every surface it did not describe. A pin on the 203-nit output showed a shadow that
was not soft at all: `ABGR2101010` carries two bits of alpha, and the ramp lives
entirely in alpha. HDR now always presents f16, `GpuWindow::open` declares the
output's own description by itself, and `hdr_picks_float_buffers_not_ten_bit` states
the format rule.
**An f16 target was never allocated.** `resize_target` passed `UNSIGNED_BYTE` with
every internal format, and a half-float target rejects bytes with
`INVALID_OPERATION`: the call failed, the texture kept whatever storage it had, and
the struct recorded the new size regardless. `CRABSNAP_FRAME_F16=1` therefore drew
nothing — a fresh texture has no storage at all — and a resize while an f16 frame
was in force left the frame smaller than its recorded size (measured: `LayerTarget`
128x128, texture 64x64). The type now follows the internal format. Caught by check
[6] of `gpu_hdr_scene`, which captures the same hard edge at one and two samples per
pixel and finds 6144 differing bytes with the old type.

**Capture read the presented buffer.** `dump_ppm` read the surface the compositor
was handed, which on an HDR session holds PQ codes and on an f16 surface could not
be read as bytes at all: a screenshot of the same scene came out all zeros (max 0,
mean 0.00 against 245 and 36.15 in SDR). A capture now reads the frame before the
present pass, so it does not depend on the output transfer, the buffer format or the
sample count. Caught by `gpu_hdr_scene` [3]–[5] (a capture must equal the sRGB
input, also from an f16 and a supersampled frame) and by dumping one scene through
`window_gpu` in SDR and HDR: 0 differing pixels of 3.69 M.

**HDR composited in the encoded 8-bit domain.** The first HDR path drew everything into the
same RGBA8 sRGB frame and converted to PQ in the present pass, so nothing above SDR white
could exist and every translucent blend, blur and shadow mixed encoded values. The surface
then declared the output's description while its pixels were still SDR. The working domain
now follows `SurfaceColour`: an HDR surface draws into linear f16, the colour, gradient,
image, atlas and material inputs convert at their boundary, and only the present and
capture passes cross the encoding. `gpu_hdr_scene` [4] states that an HDR frame is f16;
`gpu_scene` holds the SDR path to the CPU rasteriser so it cannot drift.

**The working domain was a field nothing read.** `GpuRenderer` carried a
`working: Encoding` set by `set_working`, and this page said the renderer holds
its working encoding. No shader read it: blending, blurs and alpha compositing
all run on encoded values, and the transfer to linear light happens once, in
the PQ output pass. The field and its setter are removed rather than left as a
promise the renderer does not keep. A real working domain decodes on the way
into every target and encodes on the way out — a different pipeline, not a
flag — and it is the open half of the trap above. It returns as part of
[Surface colour](#surface-colour): the domain is chosen together with the
target format, by the surface, and every cache carries it in its key.

**Captured pixels were converted to sRGB 8-bit at ingest.** The first capture
path turned a 10-bit PQ frame into eight-bit sRGB the moment it arrived, using
the output's description, so the colour was right on screen and there was
nothing left to lose: that frame could never be pinned as HDR, and a pin made
from it on this 203-nit output stayed SDR even after the display changed.
Capture now keeps the compositor's words and the description, and every
destination builds its own rendition. The pinned HDR capture therefore
survives a move to an SDR surface and back, because nothing was replaced. The
SDR rendition a file gets is the same conversion the old ingest performed, so
saved shots are unchanged; `Cropped::rendition` is that single step now.

**The BT.2390 curve was normalised to the source peak at both ends.** The
first implementation put the knee in the source-normalised scale but mapped
the top back through the source peak, which would have left a 1000-nit
picture's peak at 1000 nits, and shaped the roll-off with a quintic of zero
slope at the knee, which kinks away from the 1:1 segment below it. The curve
now scales the output by the destination peak and uses the cubic Hermite of
BT.2390 with the knee's own slope, as libplacebo does
(`src/tone_mapping.c`, `bt2390`). Caught by
`the_bt2390_curve_reaches_both_peaks_and_never_folds_back`.

**Content carried a white level it had no use for.** `ContentColour` first
held a reference white, and placement scaled PQ by it. PQ codes are already
absolute, so that divided the luminance a second time and made every HDR
picture dark on an SDR surface (203 nits came out at 0.15 sRGB instead of
1.0). The field is gone: the destination's reference is the only anchor, and
`a_pq_picture_on_an_sdr_surface_keeps_its_white_and_mid_grey` states it.

**Linear blending was assumed to be a one-LSB difference.** The estimate said
the choice of blending domain would be invisible in SDR. A blend-domain test
measured 128 against 187 for 50% white over black, a 59-level difference. The
conclusion changed with it: SDR keeps encoded blending for visual
continuity, and "switch the blend domain" is not a runtime toggle but a
different data stream.

**Pixel format was treated as an implementation detail.** The first design
let each backend carry its own format constants. Format is semantics and the
platform is a mapping: the same f16 buffer is a DRM fourcc on Wayland, an
`MTLPixelFormat` on macOS, and a DXGI format on Windows. The enum now lives
in `ui-core`, and backends map to it.

**Modifier support was probed with zero.** A probe asked whether a format was
advertised rather than whether the allocated `(fourcc, modifier)` pair was
accepted; NVIDIA returns a tiled modifier, so the answer was wrong for the
pair actually used. Probe the allocated combination, and treat
`implicit-only` as a different failure from "not advertised".

**A float desktop had no storage, so it was encoded back into eight bits.** The
Windows advanced-colour capture hands the desktop over as scRGB half floats,
and `PixelStorage` knew only `Sdr8` and `Packed10`: ingest divided by the
reference white and encoded, so a 2.5 value (200 nits) and an 8.0 (640 nits)
both arrived at or above 1.0 and the pin drew the same picture on an HDR
surface as an SDR one. The storage gained `Float16` with its own texel size
(eight bytes), `ContentColour::scrgb` names its absolute anchor, and `texel()`
skips the transfer decode for it — the decode clamps at 1.0, which is exactly
the light the packing exists to keep. Caught by
`a_float_frame_keeps_the_light_above_white` and
`a_float_picture_keeps_its_light_above_the_reference_white`.

**A picture's peak was whatever the display could do.** `max_nits` comes from
the output and travelled with a capture, so tone mapping normalised to it. On a
1000-nit display a desktop holding 240 would be mapped as if it held 1000:
BT.2390 places its knee at a fraction of the *source* peak, so the same white
lands on a different code depending on which number is passed, and a range is
compressed for light that is not in the picture. `with_peak` replaces the field
with a peak measured from the pixels before a placement is derived; an
unmeasured picture keeps the stated peak, the widest bound it can honestly
have. Caught by `a_measured_picture_rolls_its_highlights_into_the_file_white`,
which collapses to two 255s when the file's destination returns to `Clip`.

## Verification

| Check | What it catches |
| --- | --- |
| `hdr_picks_float_buffers_not_ten_bit` | an HDR surface allocated with 2-bit alpha |
| `gpu_scene` | the GPU and CPU rasterisers disagreeing on a primitive; the SDR path drifting |
| `gpu_hdr_scene` [1]–[2] | the PQ output transform disagreeing with `encode_for_pq_output` |
| `gpu_hdr_scene` [3], [5] | a capture that depends on the output transfer, the frame format or the sample count |
| `gpu_hdr_scene` [4] | an HDR frame that is not f16 |
| `gpu_hdr_scene` [6] | an f16 target allocated with the wrong element type |
| `a_pq_picture_on_an_sdr_surface_keeps_its_white_and_mid_grey` | content converted to absolute light twice, or the curve running where the peak already fits |
| `only_srgb_codes_on_an_sdr_surface_are_handed_over_untouched` | the identity branch widening to content it would round trip, or vanishing |
| `the_bt2390_curve_reaches_both_peaks_and_never_folds_back` | a curve that misses its endpoints or folds |
| `a_ten_bit_frame_keeps_its_words_and_gains_opaque_alpha` | a 10-bit frame repacked into eight bits, or an `X` surface left transparent |
| `a_float_frame_keeps_the_light_above_white` | a float desktop reduced to four bytes a texel at ingest |
| `a_float_picture_keeps_its_light_above_the_reference_white` | a float texel put through the transfer decode, so 640 nits arrives as 203 |
| `a_new_buffer_behind_a_known_id_is_uploaded_again` | a texture kept after its description or destination changed |
| Dual-domain scene (planned) | encoded and linear domains producing different geometry or coverage |
| Declare / unset / declare on the real compositor (run by hand, probe removed) | a second `get_surface`, or an unset that does not commit |

The content path was checked against the compositor's own numbers, on this
2560x1440 PQ output (reference 203 nits, peak 417, measured primaries). A
capture keeps its 10-bit words; the SDR rendition of ten sampled pixels
matched an independent implementation of the same steps (PQ decode, measured
primaries to sRGB, sRGB encode) to within two codes of 255, one everywhere the
channel is not at the bottom of the curve. An
SDR probe image pinned on the HDR surface produced the expected PQ codes to
within one code of 1023 at every step of a nine-step grey ramp, which is the
whole relative-to-absolute path: sRGB decode, place at the reference white,
PQ encode.

The switch transaction was checked at the platform seam on the real compositor:
declare, unset, declare again, with a commit between each. All three were accepted,
and the second declaration produced the same description. Moving a live surface
between an SDR and an HDR output was not observed: this machine has one output and
the compositor offers no runtime HDR toggle. The presenter's rebuild-and-repaint
half rests on review, and is the first thing to watch on a two-output setup.

## Status

Implemented: transfer functions and gamut matrices, the buffer format choice
and DRM fourccs, the single PQ output path, the compositor's output
description and colour-management surface on Wayland with automatic HDR
surface creation, the working domain that follows it, and re-resolution when the
surface's output, the compositor's preference or the panel's description changes:
a changed buffer format rebuilds the target pool, forces the atlases and image
textures to upload again, and repaints the whole surface. An SDR surface
draws into 8-bit sRGB and blends there; an HDR surface draws into linear f16,
with solid colours, gradient stops, images, the colour atlas and the material
uniforms converted at the boundary, and only the present and capture passes
compiled as two shader variants. Captures read the frame before the output
transform.

`ContentColour` travels with every picture: a capture keeps its native 10-bit
words or its scRGB half floats and the compositor's description, a crop of it
is a smaller copy of the same picture, a pin hands those bytes and that
description to the surface, and each renderer builds a texture for its
destination with `Clip` or `Bt2390`.
Both painters honour it: the GPU one builds a texture per (colour, destination)
and rebuilds when either changes, and the CPU one converts its samples the
same way.

Partial: the switch path has not been seen end to end on hardware with one
output. Tone mapping is chosen per path, not configured: display uses
`bt2390`, a file uses `clip`.

Planned: `[hdr]` tone-mapping settings with export renditions, and the export
design that decides what an HDR file is; GPU-resident content sources
(dma-buf import); a colour-management path on platforms other than Wayland;
a working domain that carries the surface's own primaries instead of sRGB,
which currently clips wide-gamut content at the sRGB triangle.

## See also

- [Rendering Backends](../rendering-backends.md): where the buffer is
  allocated and presented.
- [Presentation Targets](./present-targets.md): the submit rule above
  the buffer.
- [Text and Input](./text-and-input.md): glyph coverage and the colour page.
- [Command Line, Daemon and Settings](../apps/cli.md): where the `[hdr]`
  section will be documented.
