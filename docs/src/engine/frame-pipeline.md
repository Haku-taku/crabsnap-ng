# Frame Pipeline

One frame takes the app's declaration, reconciles it with the retained tree,
settles geometry, resolves text, computes damage, and hands a draw list to a
backend. Everything else in the engine is a detail of one of those steps.

## The order

`AppLoop::build_step` runs the phases below. The order is load-bearing: each
step reads what the previous one wrote.

| # | Phase | What it does |
| --- | --- | --- |
| 0 | `advance_animations` | Moves every animation clock before the view runs, so the app reads settled values |
| 1 | `view` → `root_decl` | The app returns this frame's live control tree; it becomes a `Decl` |
| 2 | `apply` | Diffs the declaration against the retained tree |
| 2b | scroll offsets | Re-applies engine-owned scroll positions the declaration cannot reset |
| 2c | `resolve_size_transitions` | Turns declared fixed sizes into this frame's travelling values, before layout |
| 2d | `resolve_presence` | Drives enter and exit presence, including held subtrees |
| 3 | `layout` | Solves constraints; siblings reflow around an animated size |
| 3 | scroll reconcile | Pulls every stored offset back inside the extent layout just wrote |
| 3a | bring into view | Stores the offset; the next frame's scroll pass applies it |
| 3b | `collect_changes` | Live controls report semantic `Change`s once geometry is settled |
| 4 | `paint_tree` | Emits primitives into the `DrawList`; this is collection, not rasterisation |
| 5 | `hit.rebuild` | Rebuilds the hit index from this frame's rects |
| 6 | text pass | Shapes, wraps, and resolves glyphs into the list |
| 7 | `compute_damage` | Fingerprints resolved primitives and backdrops per owning node |
| 8 | present | Hands the list to a `PresentTarget` or `SceneTarget` |

Two consequences:

- **Paint is not drawing.** `paint` only appends primitives. The CPU rasteriser
  and the GPU renderer do the work later, outside the engine loop; a small
  `paint` time does not mean drawing is cheap.
- **Text resolves after paint and before damage**, because a changed label must
  dirty its node even when its geometry did not move.

## Frame demand

A frame is built only when something can change. The decision is separate from
event routing.

- `dispatch` answers whether one event needs a repaint, and records that same
  answer in `dispatched`. A loop therefore reads `take_dispatched()` alone; the
  return value is for a caller that dispatches a single event and does not own
  the frame decision.
- `Driver::step` builds when `take_dispatched()`, a silent host change,
  `is_animating()`, or a size change says so. When idle, it waits until
  `next_wake()`: the caret blink's next phase, or the host's own events.
- `pointer_affects_frame` absorbs continuous pointer moves that cannot change
  a pixel. It returns true for a changed hover target, a changed cursor shape,
  a control that read the raw pointer, and while any gesture owner
  is active: `pressed`, `pan`, `select_drag`.

> **Wrong path:** treating `dispatch`'s return as the frame decision. The flag
> is what a loop reads. A gesture that needs every frame belongs in
> `pointer_affects_frame`, not in a forced `true`.

## Clocks

- `FrameClock::now()` is the last ticked frame. During dispatch and idle it
  is the past.
- `FrameClock::live()` reads the wall clock without advancing the engine
  clock. Every timestamp that starts an animation from an event uses it: menus
  opening, ink splats starting, a pan ending.
- `advance_by(dt)` drives the scripted clock in tests.
- `frame_step` is clamped to 50 ms, so a stalled app cannot fling an animation
  past its target.

## Animation is declared

- `AnimStore` travels an `f32` from an explicit start to a declared target
  per `Id`. A retarget starts from the current value, never from the previous
  endpoint.
- `Presence` declares enter and exit values for opacity, offset, and scale.
  Exiting nodes are held by the engine, detached from their parent, painted
  above live content, and released when presence reaches 0.
- `Presence::scaled_over` gives the scale its own clock. WinUI's dialog fades
  in 83 ms but scales for 250 ms; one clock cannot express both.
- A widget that tweens on its own has no start value on a node that did not
  exist last frame, so it draws its end state immediately. Use the
  declaration.
- Not every scale is an animation. `ContainerParams::scale` is a static
  declaration of the same kind of transform, about the node's own top-left
  rather than its centre, and it composes inside the presence one. Text is
  rasterized at the static scale (`TextPrim::shape_scale`) and only stretched by
  a presence tween: re-rasterizing every frame of an animation would fill the
  glyph atlas with one size per frame.

## Damage

`DamageAccum` collects the area one frame changed; the presenter repaints exactly that.

- Each rect is clipped to the frame's own bounds. Nothing outside them is painted, and a
  scroll of a long list changes every row it holds — most of them past the viewport.
- Overlapping rects are merged once, at the end of the frame, rather than on every add.
  Merging is quadratic in the region count, and one scroll step produces thousands.
- Past `DAMAGE_RECTS_MAX` (64) regions the set collapses to the box around it: exact
  regions pay for themselves on a caret or a hover ring, not on a frame that moves
  everything.

## Corrections

**Two answers to "does this event need a frame".** `dispatch` set `dispatched`
from the absorb decision and returned a separately computed answer, and
`Driver::step` read only the flag. An absorbed pointer sample that still
changed routing would have been delivered with no frame behind it. `dispatch`
now folds its answer into `dispatched` through a private `dispatch_event`, so
the loop can no longer miss one; the return value stays for single-event
callers.

**A change applied after `view` was not shown until something else happened.**
`App::changed` runs after the build, and the loop counted only events and
animation as frame demand, so the frame that would display the new state was
skipped until the next input arrived. A change applied mid-frame now requests
the frame that shows it, and a quiet frame still requests nothing.

**A selection drag updated only when released.** The text-selection branch
deliberately clears `pressed`, and `pointer_affects_frame` listed only
`pressed` and `pan` as gesture owners, so every move during a drag was
absorbed as "changes nothing". The cursor also changed shape without a frame,
because cursor shape only travels with a frame. Adding `select_drag` and a
comparison of `cursor_shape_at(p)` against the last sent shape fixed both;
pinned by `a_selection_drag_asks_for_every_frame_it_traverses` and
`selectable_text_shows_the_text_cursor_without_the_app_naming_one`.

**A context menu appeared fully open.** `open_menu` used `frame_now` as the
entrance start. Idle time does not advance the engine clock, so the 167 ms
entrance had finished before the first frame was built. Event timestamps now
come from `FrameClock::live()`; pinned by
`a_menu_opened_after_an_idle_gap_starts_its_entrance_at_the_click`, which fails
with `got 1` on the old code.

**The frame that finished a fade was never submitted.** A group layer is not
a primitive. When presence reached 1 and the layer disappeared, neither its
disappearance nor `is_animating` marked damage, so the frame stayed at 0.97
opacity until an unrelated update. A layer present last frame and absent this
frame now damages its bounds; pinned by
`a_modal_scrim_fades_instead_of_appearing_dark`.

**A texture id was treated as the picture.** `ImageEntry.data` was documented as
bytes and nothing else, so the GPU cached its upload by `(id, width, height)`
and the damage fingerprint hashed the texture slot. An app that fills a canvas
or decodes a video keeps its id and hands over a new buffer at the same size:
the GPU kept the first picture for the life of the run, and since no primitive
moved, damage reported nothing. The `Arc` is now the picture's identity — the
renderer re-uploads when it changes and holds the `Arc` so its address cannot be
reused by a later allocation, and the fingerprint hashes that address. Caught by
`a_new_buffer_behind_a_known_id_is_uploaded_again` in
`crates/ui-paint-gpu/src/renderer.rs` and
`new_pixels_behind_a_known_image_id_damage_the_node` in
`crates/ui-platform/src/view/tests.rs`.

**A panel that only frosted was invisible to damage.** `prim_fingerprints`
walks `Op::Prim`, and a backdrop is an op of its own, so a node whose only
painted output is `.backdrop_blur(σ)` owned no fingerprint: moving it,
retuning its sigma, or removing it left the old blur on screen, and content
changing inside its kernel never repainted it. The painter now records each
frame's `BackdropEntry` per node id, and `compute_damage` diffs that map — old
and new rects on a change, the old rect on removal — while an unchanged frost
joins the probes that inherit damage from inside their reach. The built-in
glass panels had worked around the first half by also declaring a transparent
background, and that workaround is what kept the gap hidden. Caught by
`a_moved_frost_dirties_where_it_was_and_where_it_is` and
`a_frost_repaints_when_something_changes_within_its_reach` in
`crates/ui-platform/src/view/tests.rs`.

**A scroll frame of a long document spent 1.9 s in damage.** Every node of the document
contributes an ink rect per frame, and `DamageAccum::add` merged each arrival against the
whole list and then ran a pairwise coalesce over it, so the cost grew with the square of the
region count and, through the repeated coalesce, in practice with its cube. A wheel step
over 3,000 paragraphs spent 1.88 s inside that loop, while the same frame with nothing
moving spent 63 µs. Damage is now clipped to the frame as it is added, merged once in
`finish`, and collapsed to one box past 64 regions: the same scroll frame costs 4.5 ms
in the damage phase (33 µs of it merging) and 38 ms end to end, against 1.92 s before. Measured in a release test build at
800×600 with 3,000 paragraph nodes in one scroller; pinned by
`damage_stays_inside_the_frame_and_covers_every_change` in `crates/ui-core/src/damage.rs`,
which fails on the unclipped, unbounded accumulator with `damage left the frame`.

Two further corrections belong to the transition contract rather than to the
frame loop: a held subtree painted twice, and the MD3 duration pairing. They
are recorded in [Transitions](./transitions.md#corrections).

## Traps

| Change | Symptom |
| --- | --- |
| Force a frame from `dispatch`'s return value | Frames build for events that change nothing; real missing demand stays hidden |
| Read `frame_now` for an event timestamp | Entrances start in the past; the first frame is already settled |
| Clear `clicked`/`activated` outside `build_step` | A burst of events erases a click no frame has read |
| Add a full-screen damage fallback | Hides the damage bugs it was meant to cover; the screen repaints every frame |
| Merge damage on every add | A scroll over a long list spends seconds in a quadratic coalesce |
| Add damage without clipping it to the frame | One rect per row of the document, nearly all painting nothing |
| Rely on a fill to make a frost visible to damage | A backdrop-only panel never repaints; its old blur stays on screen |
| Key a texture cache by id and size | New pixels behind a known id never reach the GPU |
| Tween inside a control | The first frame of a new node shows the end state |
| Skip `advance_animations` before `view` | The app builds against last frame's values, one frame behind every animation |
| Read `Cx::now()` in a build that draws nothing time-dependent | The recorded dependency changes every frame, so an idle page rebuilds at frame rate |

## See also

- [Transitions](./transitions.md): enter, exit, size, and the presence contract.
- [Incremental Declarations](./declarations.md): the cache between `view` and
  `apply`.
- [Rendering Backends](../rendering-backends.md): what happens after phase 8.
- [Ownership and Copies](../conventions/ownership.md): which values are shared
  by handle and which are copied.
- [Workflow](../development/workflow.md): how to verify a change to any phase.
