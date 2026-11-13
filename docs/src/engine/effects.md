# Effects

A blur, a colour matrix or a backdrop filter reads pixels outside the
rectangle it writes. That fact is the whole contract: every consumer of an
effect — layer padding, damage, a cached target, a capture region — needs to
know how far the read reaches, and the effect has to report it rather than let
each caller guess.

## Kernel geometry

A gaussian kernel carries two different radii:

| Value | Meaning |
| --- | --- |
| `radius` | The truncation actually applied |
| `guaranteed_reach()` | The pass chain's support, or `ceil(3σ)` if that is wider |
| `reach()` | What the effect's output depends on, in pixels |
| `band()` | The internal working band, which grows with the number of passes |

The guaranteed bound is the larger of the two honest answers: a truncated
gaussian's output can depend on a pixel `radius` away whatever `3σ` says, and a
box chain reaches one radius per pass. `ceil(3σ)` alone is where a gaussian's
tail becomes negligible, not a bound on a kernel truncated wider than that, and
reporting it as one would tell a consumer to pad less than the kernel reads.

`Kernel::gaussian(radius, sigma)` truncates at the requested radius and
normalises the weights, so a cheap blur is visibly cheaper instead of
silently darker: a kernel that loses its tail stops summing to one, which reads
as "the picture dimmed".

`band()` is separate from `reach()` because a separable or box blur has
several passes, and each extra pass reads one more radius. Only the result's
dependence matters for damage; the band matters for how much intermediate
work a backend must do.

A kernel is authored in logical dp, so a target with more pixels per dp needs a
wider one. `ui_core::effect::on_grid` is the single definition of that
conversion; both the CPU and the GPU renderer call it, because two conversions
drift and the symptom is a blur that halves at every scale.

## Chain reach

`EffectReach::of(effects)` sums both:

- `guaranteed` — the analytic bound, `ceil(3σ)` summed over the chain.
- `actual` — the band the chain really needs.

Keeping both stops an approximation from being reported as if it were the
truth. A backend that under-reports reach clips a blur's tail or leaves a ghost
at a layer edge; one that over-reports does extra work at every frame.

Only `actual` has consumers today: it pads a layer and a focus ring, so the box
a layer occupies already contains everything its chain writes. `guaranteed` is
for a consumer that must never clip a tail, and no damage path consults it — the
band grows over the padded box instead.

## Effect algebra

`ColorMatrix` is a 4×4 transform on premultiplied `[r, g, b, a]`. Opacity
scales every channel, alpha included, because the data is premultiplied. A
colour operation that touches only RGB on premultiplied data changes the
meaning of the colour.

Effects compose in declaration order. The reach of a chain grows by
accumulation, not by taking the maximum, because each stage can read the
previous stage's output.

## Damage and the band

A GPU frame clears and redraws one band of the surface and keeps the rest of its
pixels. Two kinds of effect read pixels they do not own, so a change near them
moves pixels the damage rect never named: a backdrop captures the frame around
its box, and a layer chain reads its layer around every pixel it writes. Both
grow the band over the whole reader (`cover_readers` in
`crates/ui-paint-gpu/src/renderer.rs`), and a layer the band then contains is
drawn in full, so a kernel never reads a hole a cull left in its layer.

The growth is the whole reader, not the reach around the damage. Grow-by-reach
has no fixed point — the union adds pixels, their reach adds more — and a frost
inside an effect layer changes that layer's source, so the two rules would have
to settle against each other. Repainting the whole reader is also the rule the
engine applies to a backdrop (`crates/ui-platform/src/view.rs:5478`): any change
within a kernel's reach alters its output, and the whole box is the change.

A layer whose bounds the band stays clear of is skipped: not cleared, not drawn,
not composited, because no pixel of it was damaged. `RenderStats::band_skips`
counts those batches, so a frame that moves a cursor outside a blurred panel now
costs the panel nothing.

## Caching an effect chain

An effect result may be cached across frames only at the **output** of the
chain. Caching between passes would hand the next pass values that have
already been quantised once, while a recomputed frame would not; a cache hit
and a recompute must be bit-identical.

Grid alignment — rounding the reused region to tile boundaries — is an
addressing optimisation, not a correctness requirement. Correctness only
demands recomputing `damage ⊕ reach`; tiles make that region cheap to
address.

The CPU renderer keeps each layer's chain output for the next frame
(`CpuRenderer::layer_effects`). The key is the chain as it runs on this grid,
the device rect the chain was given, and a fingerprint of the source bytes in
that rect — and those three are the whole input, because both sampling passes
clamp to the rect they are handed. Equal key and equal rect mean the reuse is
the very pixels a recompute produces, so no damage bookkeeping is needed: the
bytes decide. A chain with no kernel is one in-place pass, and hashing its input
would cost about as much as running it, so those are neither stored nor hashed.
The counters are `RenderStats::effect_hits` and `effect_misses`.

The GPU renderer reuses a layer's finished pixels instead of hashing them.
A layer declared `offscreen`, or one whose chain has a kernel and whose bounds
cover at least `EFFECT_CACHE_MIN_SHARE` (a quarter) of the frame, keeps a
target under the node's id, and the renderer reuses it while the frame's damage
stays clear of the layer's device bounds (`cached_is_clean` in
`crates/ui-paint-gpu/src/renderer.rs`). The share is a cost rule, not a
correctness one: a cached layer pays a full-frame clear and copy on every frame
it changes, while a kernel pass pays about its own area times the taps, so a
press glare of a few thousand pixels is cheaper to blur again than to store,
and a frosted panel wins. The chain is part of the clean test from the other
side too: `compute_damage` compares each layer's bounds and effect vector and
claims the layer when either changed, so an animated radius, a retuned blur or
a running fade is recomputed rather than composited from a stale target. The
stored pixels are the chain's output, effects included, because the cache
stores what `end_layer` produced.

An opacity-only chain is not cached on either renderer: it is one in-place
pass, and a target plus a copy would cost more than the pass it saves. The GPU
counters are `RenderStats::cached_skips` and `band_skips`.

## Traps

| Change | Symptom |
| --- | --- |
| Derive padding from the kernel's chosen radius | A truncated blur is treated as if it reached the analytic bound |
| Report `band` as `reach` | Damage is too large every frame, or a cache hit keeps stale pixels |
| Report `ceil(3σ)` as the guaranteed reach | A kernel truncated wider reads past the bound a consumer padded by |
| Sample the effect's output twice in one frame | Half-transparent layers double-composite and get darker |
| Cache an intermediate pass | Hit and miss paths differ in the last bits |
| Cache every kernel layer, not just the large ones | A press glare pays a full-frame copy to save a few thousand pixels |
| Cull a batch inside a cached layer | The cache stores the hole; the next reuse paints it |
| Take layer bounds from the layout rects | A scaled or blurred child is cropped at the group edge |
| Normalise a truncated kernel by its original sum | The result darkens; mass is not conserved |
| Treat the reuse region as a tile-aligned grid requirement | Correct output is rejected because a region is not aligned |
| Band a layer kernel by the damage alone | The blurred tail past the damage keeps the previous frame's pixels |

## Corrections

**`guaranteed_reach` was `ceil(3σ)`, and the implementation could read past it.**
The field was described as the analytic bound, which is true of an infinite
gaussian and false of a kernel truncated wider than three sigma: `Effect::blur(16,
1.5)` samples 16 pixels out while the "bound" said 5, so a consumer padding a
layer by it would clip the taps it paid for. A box chain's support is one radius
per pass, which for a small sigma can also exceed `ceil(3σ)`. The field is now
`max(band, ceil(3σ))` — the support, raised to the analytic figure when that is
larger — and nothing read it yet, so no rendering changed. Caught twice: the
kernel test states the wide-tap case, and
`an_effect_reaches_no_further_than_it_reports` spreads a solid block through each
kernel on the CPU pass and fails when the passes read past the band they declare
(four box passes instead of three is enough).

**Layer bounds were the union of the layout rects.** `subtree_ink` unioned
`ink_rect()` over the subtree, which is where each node *sits*, before any
presence transform and without the reach of a descendant that is a layer of its
own. A group holding an entering child (`scale > 1`) or a blurred child cropped
it: the ring and the blur tail were cut at the group's edge. The walk now reads
`Tree::scaled_ink_rect`, which applies the composed presence scales the way
paint and hit already did, and inflates each node by its own chain's reach.
A focused node's adorner is unioned in as well: its margin is written negative
to reach outside the control, so the ring is not the control's rect. Caught by
`a_group_layer_covers_a_scaled_child`,
`a_group_layer_covers_a_child_layers_reach` and
`a_group_layer_covers_the_focus_ring` in
`crates/ui-platform/src/view/tests.rs`.

**A cached offscreen layer was stored holding only the damaged band.** A frame
redraws only the damage band and culled every batch outside it, but a dirty
layer is cleared first and stored whole: the culled batches left their pixels
empty in the cache, and the next frame that reused it — or any frame that
redrew the whole surface — composited those holes back onto the screen. Batches
inside a layer that carries a cache key now skip both the cull and the band
scissor, so what is stored is the layer in full, as Flutter re-records an
invalidated `RepaintBoundary`. Caught by `a_cached_layer_is_redrawn_whole` in
`crates/ui-paint-gpu/src/renderer.rs`; no built-in control uses
`Decl::offscreen`, so the public API was the only way to reach it.

**A cached layer never saw damage that landed inside it on a scaled window.**
The offscreen cache compared the frame's damage — logical units — against the
layer's bounds after they had been multiplied by the output scale, so at scale
2 the two rectangles never intersected and a cached subtree kept the pixels of
its first frame while the rest of the screen updated. Damage is now converted
into the target's device pixels before the check, the same convention
`device_scissor` already used; pinned by
`a_scaled_layer_sees_damage_inside_its_device_bounds`. The built-in controls do
not use `Decl::offscreen`, so this was reachable only through the public API.

**`guaranteed` was documented as what damage expands by, and nothing read it.**
The chain-reach paragraph said so and the status line claimed "damage from the
reported reach", but a frame containing a layer effect simply disabled banded
rendering, which is a safe upper bound and expanded nothing. Both now state the
measured behaviour: `actual` pads layers, `guaranteed` has no consumer, and the
band grows over the padded layer rather than by a reach.

**A layer effect forced a full frame.** Banded rendering bailed out whenever any
layer carried an effect, so a blurred panel beside a blinking caret repainted
the whole window every frame. The bail was honest about what it did not know:
the band could neither prove an effect layer untouched nor be trusted to
recompute one, because a redraw draws only the band and a kernel reads past it.
The band now grows over every layer whose chain reads neighbours
(`cover_readers`), a layer inside the grown band is drawn whole, and a layer the
band stays clear of is skipped entirely. Measured on
`ui-paint-gpu/examples/gpu_effect_band_scene.rs`: a frame whose damage misses the
layer is partial with `band_skips > 0`, a frame whose damage lands inside it is
partial too, and both match a full redraw pixel for pixel. Caught by check [7]
there — without the growth 346 pixels at the band's edge keep the previous
frame's tail — and by [3]/[6] when the skip is disabled or made unconditional.

**A backdrop inside a group sampled nothing.** Backdrop effects were skipped
while a layer was open, because the group's own target held the pixels behind
it. A backdrop now samples the parent target for the layer depth in force,
with the blur band written into the layer's scratch, so a frosted tooltip or
glass surface that fades as a group still frosts what is behind it.

**Padding was a value the algorithm computed.** `pad` encoded one
implementation's current radius into engine metadata, so changing the algorithm
changed the contract. It is now `EffectReach { guaranteed, actual }`, which
separates the bound the engine must honour from the work the backend
happens to do.

**An edge policy sat in engine metadata.** `extends` described how a
particular filter handled pixels past its source. Edge behaviour belongs to the
source description (what is outside the canvas), not to the effect; the field
was removed.

**A sampling cache key omitted the colour domain.** The key held size and
sampling mode, so the same content sampled in a different domain could hit an
entry produced in another one. The domain is part of the key now. This was the
one real bug the semantics-versus-means review found, as opposed to a design
smell.

**The GPU effect cache was documented as blocked on a signal it already had.**
The page said the engine's damage was frame-wide and that a layer could not
learn nothing inside it had changed. Damage has been per-rect since the ink
fingerprints landed; `cached_is_clean` already tests it against a layer's
device bounds, and `compute_damage` already claims a layer whose effect vector
or opacity changed. What was missing was paint handing an effect layer a cache
key at all: only `offscreen` nodes got one, so a blur layer was recomputed
every frame even when nothing moved. A kernel chain now gets one once it covers
enough of the frame to pay for the copy, and the old "needs a signal first"
sentence no longer suggests a second, fingerprint-based scheme the renderer
does not need.

**A lens capture reused one target for every glass surface.** One slot meant
the two glass cards on the Lists page — 356×202 and 356×67 — re-specified that
texture on every call, and a `tex_image_2d` over a texture the queue still
sampled stalls the driver: `gpu_lens_alloc` measured 7.9 ms per capture,
`present` 22.3 ms per frame, 26.4 ms of CPU per frame while a drag scrolled the
page. Captures now come from a pool keyed by size (four entries, the least
recently used re-formed past the cap), so a card that keeps its size pays
nothing. Measured in a nested session after the change: `gpu_lens_alloc` below
the table's cut, `present` 2.6 ms, 4.5 ms of CPU per frame; both cards still
render their frosted content. No GPU test can pin this, so the `gpu_lens_alloc`
label is what makes a recurrence visible.

## Status

Implemented: gaussian and box kernels, colour matrices, reach and band
reporting, chain accumulation, layer padding from the reported reach, backdrop
capture with an explicit extent and a per-size capture pool, damage that
invalidates a cached layer in the target's own pixel space, a per-effect check
that the pixels an effect actually spreads over stay inside the bound it
reports, banded frames for layers with effects, and a cross-frame cache of the
chain's output in both renderers.

Not implemented: a GPU cache target sized to the layer instead of the viewport.
A cached GPU layer holds a full-frame target, so its cost does not shrink with
its own area; the CPU renderer stores only the effect's region.

## See also

- [Rendering Backends](../rendering-backends.md): how the passes are scheduled.
- [Colour and HDR](./colour.md): the domain an effect computes in.
- [Presentation Targets](./present-targets.md): how damage reaches the
  compositor.
