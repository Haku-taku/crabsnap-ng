# Rendering Backends

One engine builds one `DrawList`; two paths turn it into pixels. The CPU path
rasterises into memory, the GPU path renders into a dma-buf that the compositor
scans out directly.

## Shared by both

- The frame's `build_step`: view and declarations, layout, paint,
  `TextPass` resolution, damage, hit.
- `DrawList`, resolved primitives, clips, layers, and the glyph atlas.
- One `TextPass` instance for layout and painting. Two instances keep separate
  shaping caches and wrap the same paragraph differently.

## GPU path

Three crates split the path. `ui-gpu-scene` decides what to draw — batches, the
layer stack, damage bands, pass order — and talks to a `Device` trait, naming no
graphics API. `ui-paint-gpu` implements that trait on GL: shaders, vertex
arrays, texture storage, framebuffers. `ui-gpu-egl` owns the EGL context and the
dma-buf target. The chain is:

1. `gbm_bo_create(..., GBM_FORMAT_ARGB8888, GBM_BO_USE_RENDERING)`.
2. Export fd, stride, offset, modifier; import as `EGLImage` with
   `EGL_LINUX_DMA_BUF_EXT`.
3. Bind the image to a texture, render into an FBO.
4. Wrap the same fd in a `wl_buffer` through `zwp_linux_dmabuf_v1` and commit.

Two targets rotate (`TARGETS = 2`). Each carries its recent bands forward, so
a reused buffer does not show a frame of stale content. A resize rebuilds the
pool.

### Explicit sync

With `wp_linux_drm_syncobj_manager_v1`, two DRM timelines carry an acquire
point (exported by EGL and imported as a fence) and a release point (signalled
by the compositor). Both ride the same `wl_surface.commit`. Points must
strictly increase. Before reusing a buffer the old release point is waited on
for 50 ms, then drawing proceeds anyway rather than blocking the UI.

Without the protocol, `glFinish` runs before attach and commit. The nested test
session does not advertise the protocol, so present timings measured there come
from the fallback.

### Invariants

- Premultiplied alpha everywhere; blending is `ONE, ONE_MINUS_SRC_ALPHA`.
- No Y flip for dma-buf targets: GL row 0 is the top of the image.
- `read_pixels` returns straight RGBA; convert to premultiplied BGRA before
  comparing with `ui_paint::Pixels`.
- Required uniforms fail at startup. An unbound sampler does not error; it
  ghosts.
- Partial bands are disabled under supersampling or HDR: the full frame is drawn.
- A band grows over every reader it touches (a frost's box, a layer whose chain
  reads neighbours), and a layer it stays clear of is skipped whole.
- A capture reads the frame before the output pass, so it is an SDR picture;
  `read_pixels` on the surface returns the bytes the compositor got.
- A size change adds full damage. Without it, bands outside the reported
  region keep old pixels.

## CPU path

`ui-paint` uses zeno for coverage and reads the same glyph atlas. It repaints
the whole surface and reports `Damage::Full`. It is the fallback on machines
without dma-buf or a usable GPU, and the reference for every primitive in the
`DrawList`.

A texture and a shadow are where the two paths agree closest, because their
picture is a function of a distance rather than of edge coverage. A texture is
stretched bilinearly over premultiplied texels, sampling at `u * src - 0.5` so
texel centres sit half a texel in. A texture whose entry asks for
`Sampling::Pixelated` takes the nearest texel on both paths instead — the GPU
sets `GL_NEAREST` on that texture object, the CPU reads the texel the sample
lands in — and because the filter belongs to the texture, the GPU cache is keyed
by picture and sampling and one file drawn both ways holds two textures. A shadow is the silhouette's signed distance
through the normal CDF, with `blur` as two sigma. `ui-paint` evaluates the same
`sd_round_box` the shaders do — it is what feathers every fill too — so
`gpu_image_scene` and `gpu_shadow_scene` compare the two renders and find no
difference at their probes.

A shadow stops at the surface it belongs to. `ShadowPrim.bound` names that box
and which side of its edge the cast lives on: an outer cast
(`ShadowSide::Outer`) paints only beyond the edge, an inset one
(`ShadowSide::Inset`) only within it, each feathered over one pixel like a
fill's own edge. That is how CSS clips a `box-shadow` inside the border box and
an inset one outside the padding box (CSS Backgrounds 3 §7.1). A cast with no
bound paints freely, as the glass rim does. An inset cast's falloff is the same
CDF mirrored, so the surface edge carries half the peak on both sides.
`Shape::Box` bounds every shadow at its own box and paints an inset one after
its fill, above the background.

The glyph atlases are sampled with `LINEAR` on both sides of that rule: a glyph's destination
carries the fraction its subpixel phases did not take (up to a quarter pixel vertically), and a
1:1 tile still returns exact texels because fragment centres land on texel centres. The CPU
rasteriser rounds a glyph's destination instead and drops that fraction, so its text steps
where the GPU's glides; the tiles themselves are already padded, so a blend at a tile's edge
reaches only transparency.

There is no automatic GPU-to-CPU fallback: if `zwp_linux_dmabuf_v1` is absent,
session creation fails with `NoDmaBuf` unless the CPU presenter was requested
(`GALLERY_CPU=1` in the gallery).

## What the two paths must not share

Anti-aliasing is implementation-defined. The CPU rasteriser computes analytic
coverage per pixel; the GPU feathers an SDF or a distance to the edge. Their
edge pixels differ by a few levels, and demanding equality would force one
backend to imitate the other's coverage rather than its result.

The shared contract is therefore the plan, not the pixels:

| Shared | Separate |
| --- | --- |
| Declarations, tree, layout, hit, events, animation, damage | Coverage at an edge |
| Primitives and batch order | The rasterisation mechanism |
| Effect parameters and semantics | The filter implementation |
| The `PresentTarget` contract | Buffer allocation and format details |

The invariants that replace pixel equality are measurable: an opaque 100×100
square covers the same area on both paths (within 0.5%), alpha along a slanted
edge is monotone, and compositing never overflows or goes negative. The GPU
examples assert these against the CPU renderer for the primitives the CPU
draws; a new primitive must extend both the CPU rasteriser and those checks,
because two backends that are never compared drift within months — the image and
shadow scenes each found defects on their first comparison run.

## Why not the obvious alternatives

- **wgpu**: no dma-buf export under our control, so the zero-copy chain
  breaks. The rejection was made against a specific version; re-check before
  reopening it.
- **Sharing triangles between CPU and GPU**: loses SDF coverage and
  instancing, and needs a second triangulation path.
- **Two independent renderers**: they drift. The shared contract is the
  `DrawList`, not an implementation.

## Traps

| Change | Symptom |
| --- | --- |
| One shm pool or offset 0 for both slots | Compositor samples memory being rewritten: tearing, stale rows |
| Report only the diff while the CPU path repaints everything | Kept pixels in unreported areas |
| Resize without full damage | Stale bands, or garbage at the edges |
| Flip the projection Y for dma-buf | The whole frame is upside down |
| Compare `read_pixels` with `Pixels` directly | A near-100% difference from alpha convention |
| Use only `drmSyncobjImportSyncFile` for a release point | The point never advances; reuse never unblocks |
| Skip `glFinish` when explicit sync is absent | A black window with a correct readback |
| Set uniforms before `use_program` | A sticky `GL_INVALID_OPERATION` blamed on a later call |
| Render on a `renderD*` that does not drive the connected output | Import and release succeed; the output is black |
| Keep cached `wl_buffer`s across reallocation | A reused inode shows the previous allocation's content |
| Draw a new primitive on the GPU only | The CPU presenter silently omits it, and nothing counts the omission |
| Filter straight-alpha texels | A transparent texel's colour tints the edge |
| Map the destination box onto texel indices | Every texel boundary sits half a texel early |
| Mix a logical rect with a device-pixel band | The blur covers a fraction of the box at fractional scaling |
| Square every corner the intersection matches no clip | A scrolled rounded card leaks through its own corner |

## Corrections

**The CPU path drew no texture at all.** `draw_batch` counted every `Image` into
`stats.skipped_primitives`, so under `GALLERY_CPU=1` a wallpaper page was blank
while the same `DrawList` rendered on the GPU — the primitive was implemented
twice only in name. `draw_image` now converts the destination box to device
pixels, feathers it through the same `coverage_rounded_rect` the fills use, and
samples `prim.src` bilinearly with edge clamping. The sample interpolates
premultiplied channels and unpremultiplies the result, because a straight-alpha
lerp lets a transparent texel's colour reach the edge, which the GPU sampler does
not do. Caught by `a_texture_is_sampled_the_way_the_sampler_does` in
`crates/ui-paint/src/tests.rs`, which was watched red three ways: with the draw
removed, with nearest sampling, and with straight-alpha lerp.

**A shadow had no CPU implementation either, and the shape existed twice.**
`Shadow` shared `Image`'s skip arm, so `GALLERY_CPU=1` drew no elevation at all.
Filling the gap exposed the second half: `coverage_rounded_rect` tested each
corner separately while the shaders evaluated `sd_round_box`, two derivations of
one silhouette that a falloff would have had to guess between. The renderer now
has one `sd_round_box` and the fill's coverage is its `(0.5 - d)` feather — the
shader's `AA` constant is 0.5, so the fill pixels are unchanged — and `draw_shadow`
puts that same distance through the same Abramowitz & Stegun CDF the shader
carries, scaled into device pixels. Caught by
`a_shadow_fades_from_its_edge_like_a_gaussian_cast` and
`a_shadow_covers_the_device_pixels_of_its_caster` in `crates/ui-paint/src/tests.rs`,
whose expected alphas (102 and 115 at the edge, 27 one sigma out, 218 on a ring)
are the CDF evaluated by hand; each half was watched red separately, with sigma
unscaled and with the ring's band dropped. `gpu_shadow_scene` compares the two
renders at six probes and finds them identical. The widget/literal parity test
also failed, which is how the gap showed: its hand-written scene had never
expressed the pack's overlay elevation, and a skipped primitive hid the omission.
The literal now states `.elevation(3)`, so that test covers a shadow too.

**`gpu_shadow_scene` had been failing on two stale thresholds.** The scene
demanded a cast of 0.10 six pixels outside the edge, which was right when `blur`
was read as sigma; since `blur` became two sigma (CSS and Skia) the correct value
there is `0.5 · CDF(-1.1) = 0.068`, and the last probe sat 11.5 pixels out, where
the tail is still 0.005 rather than the asserted 0.002. Both probes now state
the model: the edge pixel carries half the peak, and the zero is measured at
three sigma.

**The cross-backend check found two more image defects.** `gpu_image_scene` now
rasterises each scene on both paths and compares region means. The first run
disagreed by 157 levels on a seam between a transparent and an opaque texel,
because the GPU uploaded straight-alpha texels and let `GL_LINEAR` filter them —
a transparent texel bled its colour into the edge, against the premultiplied
invariant the frame's blend equation assumes. The upload now premultiplies and
`image.frag.glsl` applies one coverage factor. The same run showed an opaque
scene off by 2.2: the CPU mapped the destination box straight onto texel
indices, putting every texel boundary half a texel early. Both paths now sample
at `u * src - 0.5`; the measured gaps fell to 0.02 and 0.5. Caught by
`[12]`–`[15]` in `crates/ui-paint-gpu/examples/gpu_image_scene.rs`.

**A rounded clip lost every corner when another clip cut it.** `plan_batches`
intersected the clip stack's rects and then looked for a clip whose rect equalled
the intersection; when none did — a card scrolled halfway out of its viewport —
it fell back to square corners, so a child's square fill bled past the card's
visible curve. It also squared the card when a *square* clip merely coincided
with the rounded one, whose region contains the square's. The batch now takes
its corner shape from the innermost rounded clip and keeps each corner only
where that clip still owns it: a corner the intersection cut through is
straight, which is what the cut is. Caught by
`a_rounded_clip_keeps_the_corners_a_square_clip_did_not_cut` in
`crates/ui-core/src/draw.rs`. Two *rounded* clips intersecting still resolve to
the innermost one's arcs; a complete answer needs the clip stack in the
backends instead of one `ClipEntry` per batch.

**The CPU frost ignored the box's shape and its scale.** `Band::write_into`
copied a rectangle, so under `GALLERY_CPU=1` a rounded panel's blur showed
square corners outside the arc; and the band was built from the logical rect on
a device-pixel buffer, so at 2x it covered the top-left quarter of the panel
with a blur half as wide. The frost now converts its area, sigma and corner
shape into device pixels and feathers the write through the same
`coverage_rounded_rect` the fills use. Caught by
`a_rounded_frost_keeps_the_pixels_outside_its_corners` and
`a_scaled_frost_covers_the_boxes_device_pixels` in `crates/ui-paint/src/tests.rs`.
The group-effect half of the same defect was the next one: `composite` passed
the authored kernels straight to `apply_effects`, so a `.blur(4)` on a 2x grid
blurred 4 device pixels. Both backends now convert through
`ui_core::effect::on_grid`, which moved out of the GPU crate so there is one
definition; pinned by `a_group_blur_widens_with_the_render_grid` in
`crates/ui-paint/src/tests.rs`.

**A shadow painted through its own surface, and no inset form existed.**
Neither `draw_shadow` nor `shadow.frag.glsl` cut the cast at the caster's edge,
so a `Shape::Box` shadow carried its peak alpha across the whole box and only an
opaque fill hid it. Any translucent surface showed the cast as a rim inside its
own edge: the gallery's markdown code card tints its body at 6 %, and on the
markdown page (release build, wayland-2, 2x) its fill read 179 inside the card,
ramping to 196 at the card's edge, against 251.7 on the page outside. CSS clips
an outer `box-shadow` inside the border box and an inset one outside the padding
box (CSS Backgrounds 3 §7.1), and Skia culls the geometry behind an opaque
occluder unless told the occluder is transparent
(`kTransparentOccluder_ShadowFlag`, `SkShadowUtils::DrawShadow`). `ShadowPrim`
now carries `bound: Option<ShadowBound>`, a box plus a `ShadowSide`; both
renderers zero the cast on the wrong side of that edge over one pixel of
feather. `Shape::Box` bounds every shadow at its own box, and `ShadowSpec.inset`
selects the mirrored falloff painted after the fill. The same row measured again
after the change holds 218.3 flat to the card's edge, with the shadow ramping
only outside it (213.7 at the edge, 232 six logical pixels out). Caught by
`a_shadow_stops_at_its_surfaces_edge` in `crates/ui-paint/src/tests.rs`, watched
red with the cut removed (an outer cast read 153 inside the surface) and with
the inset falloff unmirrored (197 where the inner edge reads 58);
`gpu_shadow_scene` now renders a translucent surface and an inset one and
compares both paths at eleven probes.

**A node's own clip cut its own shadow.** `paint_node` pushed the node's
declared clip before `paint_self`, so a clipped container — a scrolled card, a
popover surface — lost its own `box-shadow` at its padding box, and a leaf with
a clip lost it to the overflow branch as well. In CSS a clip applies to the
box's content (CSS 2.1 §11.1.1), and a `box-shadow` is not content: it is drawn
outside the border box (CSS Backgrounds 3 §7.1). Every ancestor's clip is also
already on the stack when a node paints here: the mask only narrows where a node
declared a clip, and that node pushed it. The clip now goes on after
`paint_self` and before the children, and the branch that re-pushed an ancestor
rect for the node's own ink is gone with it — redundant where an ancestor owned
the clip, harmful where the mask reflected the node's own. Caught by
`a_nodes_own_clip_cuts_its_children_but_not_its_own_shadow` in
`crates/ui-platform/tests/clipping.rs`, watched red with the push moved back
before `paint_self` (the page half a pixel outside the box read 255 instead of
153); `a_rounded_parent_clips_its_child_to_the_curve` pins the child side. Press
ink, the scrollbar and the focus adorner still paint inside the clip.

**The CPU path cannot be the reference for a second backend.** The Windows port
began from the assumption that `ui-paint`'s output defines correct pixels. It
does not: the CPU path skips canvas compositing, layer effects and backdrop
materials, so a backend that agreed with it would have baked those gaps in. The
GPU path is the reference instead, and it had to be made portable before a
second API could implement it: the painter moved from `ui-paint-gpu` into
`ui-gpu-scene`, which reaches the driver only through the `Device` trait, and
`ProgramId`, `TextureId` and `TargetId` are opaque handles a backend decodes its
own way. `ui-paint-gpu` is now just the GL implementation of that trait.
Verified as far as this machine allows: `ui-gpu-scene` builds on its own and its
unit tests pass, which covers the batch and scissor geometry and not the device.
The GL backend, the Wayland wiring and every example still need a Linux run
before this paragraph can claim more.

## Status

Implemented: both presenters, dma-buf zero-copy, explicit sync, HDR targets,
offscreen layers, text on both paths, every `DrawList` primitive on both paths,
fractional scaling.

Not implemented: any non-Wayland platform, and Vulkan. The Windows and Vulkan
notes in `old/` are surveys, not code.

## See also

- [Platform Boundary](./platform-boundary.md): how a backend is registered.
- [Frame Pipeline](./engine/frame-pipeline.md): what the `DrawList` contains.
- [Profiling](./development/profiling.md): measuring present, and why the
  nested session lies.
