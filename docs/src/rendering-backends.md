# Rendering Backends

One engine builds one `DrawList`; two paths turn it into pixels. The CPU path
rasterises into memory, the GPU path renders into a dma-buf that the compositor
scans out directly — or, on Windows, into a DXGI swap chain's back buffer.

## Shared by both

- The frame's `build_step`: view and declarations, layout, paint,
  `TextPass` resolution, damage, hit.
- `DrawList`, resolved primitives, clips, layers, and the glyph atlas.
- One `TextPass` instance for layout and painting. Two instances keep separate
  shaping caches and wrap the same paragraph differently.

## GPU path

The path splits at a `Device` trait. `ui-gpu-scene` decides what to draw —
batches, the layer stack, damage bands, pass order — and names no graphics API.
`ui-paint-gpu` implements that trait on GL: shaders, vertex arrays, texture
storage, framebuffers. `ui-gpu-egl` owns the EGL context and the dma-buf target;
`ui-gpu-d3d11` is the same trait on D3D11, described below. The Wayland chain
is:

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

### The Windows device

`ui-gpu-d3d11` implements the same `Device` trait on D3D11 and is empty off
Windows: a device that cannot exist without COM and DXGI has nothing to fall
back to. Both stages' constant buffer is generated from one table with explicit
`packoffset`, and the byte offsets the device writes into come back from the
same function, so the shader's layout and the upload cannot describe different
buffers. Instance layouts and primitive semantics stay the scene's; only the API
calls are the backend's.

Four API facts shape the code:

- A sampler array takes a literal index only, so a unit chosen by a uniform
  becomes a two-way `if` (`sample_unit`) rather than `u_samplers[unit]`.
- A scalar array takes one constant register per element, so the tap weights are
  `float4[9]` read through `tap_weight`, not `float[33]`.
- `ClearRenderTargetView` ignores the scissor, so a damage band clears through
  `ID3D11DeviceContext1::ClearView` and falls back to the plain clear on a
  context without it.
- Device rows grow downward and D3D's viewport grows upward, so the flip lives
  once, in `device_to_ndc`.

`cargo run -p ui-gpu-d3d11 --example d3d11_scene` compiles all eighteen programs
through d3dcompiler and reads back one 160×120 scene holding a rounded rect, an
ellipse, a line, a sector and a blurred layer over an opaque backdrop, checking
eight invariants: shape edges land where the primitive says, off-shape pixels
keep the backdrop, and the layer composite keeps its orientation. It does not
compare against the GL backend; that needs a Linux run.

### The Windows window

`ui-platform-win32` is the D3D11 device's window: `CreateWindowExW` under
per-monitor DPI awareness, an `IDXGISwapChain1` from `CreateSwapChainForHwnd`
(`B8G8R8A8_UNORM`, two buffers, `DXGI_SWAP_EFFECT_FLIP_DISCARD`), and
`Present(1, 0)` for vsync pacing. It implements `SceneTarget`, so the app loop
drives it exactly as it drives the Wayland GPU window; the opening size in a
`SurfaceSpec` is logical and becomes a physical client area through
`AdjustWindowRectExForDpi`.

The painter never draws into the back buffer. `ui-gpu-scene` keeps a frame
target of its own and, at the end of a frame, blits only the damage band into
the target it was handed; the presenter hands it a second persistent `Rgba8`
target, then runs `D3d11Device::present_copy`, one fullscreen-triangle pass
that samples all of that target into the back buffer. Two reasons for the
extra target: a flip-discard back buffer holds no previous frame, which the
band's untouched pixels depend on, and a plain `CopySubresourceRegion` does not
convert RGBA to BGRA, while the destination view's format orders the channels
during the sample. A resize releases the adopted back buffer, calls
`ResizeBuffers`, adopts buffer 0 again and resizes the frame target.

D3D11 hides a flip chain's rotation — `GetBuffer(0)` is always the buffer to
draw into, and `GetBuffer(1)` returns `DXGI_ERROR_INVALID_CALL` — so the
presenter holds one back-buffer target. A capture therefore reads buffer 0
inside `present_scene`, after the copy and before `Present`: after the flip,
that index holds the next, still-blank storage.

`cargo run -p ui-platform-win32 --example window_scene --release` draws a scene
of three primaries, a rounded bar animated from `cx.now()` and a line of text;
`WINDOW_CHECK=1` reads the presented buffer back at the opening extent, resizes
the window to 720×480, and reads it back again. Verified on Windows 11 at 1.25
device pixels per logical unit (1200×800 and 720×480 client areas): all four
probes passed at both extents. `WINDOW_DUMP=path` writes the frame as a PPM.

### The Windows window as a frame host

The window is also the driver's `FrameHost`, so the engine decides when to turn
and no app pumps messages itself. A Win32 window has no frame callback:
`wait_for_frame` calls `DwmFlush`, which returns once the composition pass that
will show the presented frame has been submitted — the pacing point a
`wl_surface` frame callback marks — and sleeps 8 ms instead when composition is
off and there is nothing to flush. `can_draw` is always true, because unlike a
Wayland surface there is no configure round trip that can withhold a buffer.

Input leaves the window procedure as `ui_core::Event`: cursor motion plus a
`TrackMouseEvent`-armed leave, the three buttons with `SetCapture` as the
implicit grab a compositor would give, `WM_MOUSEWHEEL` in notches of 48 logical
units, named keys and command chords from `WM_KEYDOWN`, text from `WM_CHAR`, and
`Modifiers` whenever `GetKeyState` disagrees with the last report. Positions are
divided by the window's DPI so input and layout share logical units. Focus,
extent and scale changes carry no event of their own and are reported as a
silent change, which still owes a frame. `WM_PAINT` is validated at once and
reported the same way, because the frame loop is what paints. The clipboard
carries `CF_UNICODETEXT`; the app colour scheme comes from
`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`,
read on every poll.

`ui-platform-win32::Session` owns window, presenter and driver in that teardown
order and offers the same `open`/`tick`/`finish` shape as the Wayland session.

An app is written against `ui_platform::session::HostSession`, the portable
contract both sessions implement, and against the two DTOs beside it,
`SessionOptions` and `Tick`: a backend may carry any number of extra methods, but
what app code calls is the trait. `ui-host` re-exports those three from
`ui-platform` and only the backend itself — `Session`, `ThemeWatch` — from the
target tables, so the gallery's manifest and source mention no OS and its run
loop is generic over `H: HostSession<Gallery>`. Nothing restates a DTO on the
platform side: two copies of one struct are two contracts that drift apart.

What a host can answer and another cannot stays a runtime question, and the trait
is where the question is asked — a refresh rate, a frame-callback count, a
dma-buf count, a process CPU clock. A host without the number returns `None`
and the report leaves the field out instead of printing a zero; a Win32 session
answers `true` to `gpu_present` because D3D11 is its only path, and `None` to
`dmabuf_stats` because a flip-model swap chain holds no dma-buf. Measured on
Windows 11 at 1.25 device pixels
per logical unit: the gallery draws its three control sets, a posted
`WM_LBUTTONDOWN`/`WM_LBUTTONUP` on the MD3 switch flipped it and built the next
frame, an animated page ran at 184 frames per second against a 275 Hz display,
and an idle page settles to four to six turns a second.

A frame cannot be drawn *during* a modal sizing drag. `DefWindowProcW` runs the
sizing loop on this thread, so the app loop is blocked inside it until the drag
ends; DWM stretches the last frame across the growing window, and the first
crisp frame is built when the button comes up. A re-entrant draw or a non-modal
sizing path is what would lift that, and neither is implemented.

Not implemented on this path: IME composition, non-text clipboard formats,
layer surfaces, which `Win32Window::new` refuses with `UnsupportedRole`, and HDR.

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
| Index a sampler array by a uniform, as GLSL does | `X3512: sampler array index must be a literal expression` |
| Declare a convolution's weights as `float weights[33]` | One register per element: the next member's `packoffset` collides |
| Pass a mip-chained target to `CopyResource` | The copy fails, silently, and the readback is zeros |
| Clear a damage band with `ClearRenderTargetView` | The whole frame is cleared and the band's promise is gone |
| Leave `SV_POSITION` out of the pixel shader's struct | Every input reads its neighbouring register |
| Draw the painter straight into a flip-model back buffer | The damage bands assume a previous frame the discarded buffer never held |
| `CopySubresourceRegion` a frame into a `B8G8R8A8` view | Channels are copied, not converted: red and blue swap |
| Read a flip chain's buffer 0 after `Present` | After the flip it is the next, blank storage |
| `ResizeBuffers` while a target still holds the back buffer | The call fails on the live reference |
| Give a `GENERATE_MIPS` texture only one bind flag | `CreateTexture2D` returns `E_INVALIDARG` |
| Store the window state behind a null check in the procedure | `WM_NCCREATE` is the message that hands it over, so it takes the same early return: no input, no redraw |
| Wait for messages with `INFINITE` | An app's own deadline is only noticed between turns, and the run overshoots it |
| Let a `WM_PAINT` reach `DefWindowProcW` unpainted | It is reposted until the thread starves; validate it and let the frame loop paint |

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

**The D3D11 readback returned zeros from a frame that had rendered.** `read_pixels`
copied the target with `CopyResource` into a one-level staging texture, and the
targets carry a full mip chain so that `set_sampling(.., mips)` works on any of
them; D3D11 wants both sides of `CopyResource` to agree on dimensions, format
*and* mip count, so the call failed and the example read the staging buffer's
zeros. The device's own copy has no error return to check, which made the
failure look like a broken clear or a broken draw: a probe that cleared a target
red and blitted it into a second one showed both working before the readback
did. The readback now copies subresource 0 with `CopySubresourceRegion`, which
does not care how many levels the source has. Caught by `[1]` of `d3d11_scene`,
which failed with an all-zero readback; the example is what makes the silent
path visible at all.

**Every D3D11 varying arrived as zero.** The pixel shader's input struct did not
declare `SV_POSITION`, so its inputs were numbered from register 0 while the
vertex shader's outputs start at register 1, and the rasteriser fed each input
from the neighbouring register instead of renumbering the pair. The symptoms
pointed elsewhere: every shape drew as a full quad in its instance colour, in the
right place, and a vertex shader that wrote a constant into a varying still lost
it. It took a fragment shader that returned `v_local` and printed the same value
at every pixel to show the inputs were not the ones the vertex stage wrote. `PsIn` now opens with `SV_POSITION` like
`VsOut`, so the two signatures describe one register layout. Caught by `[2]` and
`[7]` of `d3d11_scene` — a rounded corner read as filled and the sector's arc
read as empty — which are the two checks that only pass when a *shape*, not a
rectangle, reaches the target.

**No D3D11 texture bound both `SHADER_RESOURCE` and `RENDER_TARGET`, so the
glyph atlas could not be created.** The device's texture helper added the
render-target binding only for textures that were targets, while every texture
carried `D3D11_RESOURCE_MISC_GENERATE_MIPS`, because `set_sampling(.., mips)`
may ask for a chain on any of them. D3D11 refuses that misc flag unless both
bindings are present, so `CreateTexture2D` returned `E_INVALIDARG` — reported
as `atlas upload: texture: 参数错误` on the first frame that drew text.
`d3d11_scene` draws shapes only, so no example had exercised the path. Every
texture now binds both, which is also what the chain needs: `GenerateMips`
reads a shader resource view and writes a render target view of one resource.
Caught by `window_scene`'s first frame, the first thing in the tree to draw
text through the device.

**The Win32 window took no input and never redrew.** `window_proc` read
`GWLP_USERDATA`, and when it was null it returned to `DefWindowProcW`.
`WM_NCCREATE` is the message that carries the state pointer in
`CREATESTRUCTW.lpCreateParams`, so it took that same early return and the
pointer was never stored: every message after it went to the default procedure,
which is why the window appeared, painted once, and then only changed when DWM
stretched a frame across a resize. `WM_NCCREATE` is now answered before the
lookup, and the extra strong count taken for the procedure is released in
`WM_NCDESTROY`, which runs even when a later step of creation fails. Caught by
`cargo run -p ui-platform-win32 --example input_probe`, which posts a move, a
click, a notch, a character and a key and requires all seven translations, and
by posting a real click at the gallery's switch, which flipped it and built the
next frame.

**The Win32 message wait had no deadline.** `wait_until(None)` passed `INFINITE`
to `MsgWaitForMultipleObjectsEx`, so a run with `GALLERY_SECONDS=2` took 8.6 s:
the gallery's lifetime, like a scripted probe's next step, can only be noticed
between turns, and the loop was asleep until unrelated input woke it. The wait
is now the caller's timeout capped at one second, with 250 ms when there is
none — the same bound the Wayland backend carries.

## Status

Implemented: both presenters, dma-buf zero-copy, explicit sync, HDR targets,
offscreen layers, text on both paths, every `DrawList` primitive on both paths,
fractional scaling, a D3D11 device that renders the whole `DrawList` on
Windows, and a Win32 window that presents it through a DXGI flip-model swap
chain, feeds pointer, keyboard and clipboard events back to the loop, and hosts
the gallery. Both window backends implement the portable `HostSession` trait, so
an app's loop is written once and names no backend; the Win32 implementation is
built and exercised on this machine, the Wayland one is checked by reading each
delegated signature until it is next built on Linux.

Not implemented: IME, non-text clipboard formats, layer surfaces and HDR on the
Windows window, redraw during a modal resize drag; Vulkan. The Windows and
Vulkan notes in `old/` are surveys, not code.

## See also

- [Platform Boundary](./platform-boundary.md): how a backend is registered.
- [Frame Pipeline](./engine/frame-pipeline.md): what the `DrawList` contains.
- [Profiling](./development/profiling.md): measuring present, and why the
  nested session lies.
