# Transitions

A transition declares a target and the engine interpolates towards it. The app
never writes an interpolated value back into its state, and a control never
tweens itself.

## What can be declared

| Declaration | Animates | Reaches layout? |
| --- | --- | --- |
| `transition_size(spec)` | Any axis of a container: a fixed one toward its declared extent, a `hug` one toward its last measurement | Yes: siblings reflow every frame |
| `enter(Presence::fade(spec, from))` | Opacity, and optionally an offset or scale | No: paint and hit only |
| `exit(Presence::fade(spec, to))` | The same values, in reverse, while the engine holds the node | No |
| `Presence::scaled(scale)` | A uniform scale about the node's centre | Hit uses the same transform |
| `Presence::scaled_over(scale, spec)` | Scale on its own clock, separate from the fade | No |

Only `transition_size` affects layout. Presence offsets and scale change what
is painted and what is hit, so a moving element does not reflow its siblings.

## Enter

Start values are written in the declaration, as CSS `@starting-style` does.
On the frame a node appears, presence is 0 and the node paints its declared
start values. Each appearance gets a fresh animation: the id is forgotten when
the node reappears, so a reopen animates again.

Entering values are interpolated to the declaration, not multiplied into it.
An opaque slide-in stays opaque; only the offset moves.

## Exit

When a declaration disappears and carries an `exit`, the engine holds the
subtree instead of reclaiming it:

1. The node is marked held. It leaves the live tree, layout, hit index, and
   focus search.
2. `detach_held` removes it from its parent's `order`, paint order, and id
   table after the tree is rebuilt. Missing the paint list paints the subtree
   twice; a translucent scrim showed alpha 53 instead of 117.
3. Presence travels to 0. The subtree paints above live content.
4. At 0 the subtree is released and siblings reflow.

## Exit space

What happens to the slot the node held is `exit_space`, declared beside the
exit:

| Policy | The slot |
| --- | --- |
| `OutOfFlow` (default) | Gone on the first exit frame: siblings reflow at once, only the paint waits |
| `KeepSpace` | Kept at the size the node last had; siblings reflow when the exit lands |
| `Collapse` | Shrinks with the exit, so siblings travel with the node as it leaves |

The last two keep the held root in its parent's order, so it keeps taking part
in layout while it leaves; both still strip it from paint order and the id
table, or the subtree paints twice. `Collapse` writes the shrunk extent into
the node's size each frame, which only an axis declared at a fixed pixel
extent can do: a hug or fill axis has no size of its own to write, so it does
not shrink.

Exit animation state is keyed by `NodeId`, because a node is held at most once
and cannot inherit a previous exit's settled value.

## Content-size axes

A `hug` axis has no declared extent to travel to: its size *is* what the content
measured. The engine reads that measurement from the node's last pass
(`hug_extent`), clamps it by the declared bounds exactly as layout's hug does,
and pins the axis there with `AxisSize::hug_pinned` for the leg. The sizing
stays `Fit`, so a parent still reads the node as one that fits its content
rather than as one holding a fixed size it may not shrink.

The measurement trails the declaration by one pass: on the frame a line is
added or dropped, layout still sees the previous extent, and the leg starts on
the next frame from that same value. Nothing jumps — the node holds its old
size for that one frame while the new content is measured inside it — and the
siblings travel with the leg instead of after it.

A content change the engine cannot see never moves the target: an app that
toggles state outside a control must announce it (`invalidate_decls`), or the
declaration cache serves the old subtree and the extent stays put.

## Two clocks

`Presence::scaled_over(1.05, spec)` gives scale its own `AnimSpec`. WinUI's
dialog is the reason: opacity reaches 1 in 83 ms, while scale takes 250 ms in
and 167 ms out. Sharing one value makes one of the two channels wrong, and the
result reads as "it appeared instantly".

## One leg clock

Every consumer of an `AnimSpec` asks `AnimSpec::progress_at(start, now)` how far its leg
has come, and `AnimSpec::eased_at` for that progress through the spec's curve. Three facts
live in the first: a zero-duration leg is a write that lands once its delay is over, a
clock behind the leg's start holds `0` rather than rewinding, and a clock with no usable
value lands the leg. The engine's own entries, a ripple's grow and fade, a menu's entrance
and an automatic scrollbar's fade read them; copies composed by hand is how those drifted,
one clamping the duration and one ignoring the delay.

Blending the two ends goes through `geom::lerp`, which is exact at both ends: a settled leg
reports the target itself rather than a value one ULP off it.

## Values and colours

A control that changes a value it owns declares the target and paints what comes back:
`Cx::animate(id, target, spec)` for a scalar and `Cx::animate_colour(id, target, spec)` for
a fill. Both travel from the value on screen, so an interrupted leg retargets rather than
restarting, and both record the read, so a cached declaration is rebuilt while the leg runs.
A colour's four channels share one clock and cannot desync.

This is the shape a *value* change takes. A node that disappears and comes back is a
replacement, and a replacement of an opaque element has nothing behind it to show: see the
white flash recorded in [Pins](../apps/pins.md#corrections).

## Keyframes

`anim::weighted(t, stops, from, lerp)` is the one segment evaluator: each stop is
`(end weight, curve, value)` and the segment runs from the previous stop's value. The
engine's keyframed entries and Fluent's Lottie trim table go through it. MD3's
`progress::interval` is not a copy: it is flat before and after its window, which a stop
list cannot express.

The values a presence node paints at a given progress — alpha, shift, scale — come from
`Presence::values_at`, never from arithmetic at the paint site.

## Durations come from the published pairing

Do not choose a duration by feel. MD3 pairs emphasized decelerate with 400 ms
for entering the screen and emphasized accelerate with 200 ms for leaving it.
The decelerate curve is steep at the start: at 200 ms, 75% of the distance is
covered in the first 50 ms, so the entry looks like no animation at all.

## Corrections

**A spring parked below one f32 ulp and kept the loop awake.** At page-scale
coordinates the smallest representable step was wider than the settle
tolerance, so the integrator stopped moving while `live()` stayed true;
dropped springs were also never advanced, so one left behind spun the frame
loop at full rate with nothing to draw. Springs integrate in f64 now, the
scheduler advances every spring each frame, settled entries stay as the next
animation's start, and activity is a fact of the current frame rather than a
standing flag.

**A scrim appeared at alpha 53 instead of 117.** `detach_held` detached the
held root from the parent's children and the id table but left it in the
parent's paint order. The subtree was painted once through the parent and once
from the held queue; opaque content hid it, a translucent scrim composited
itself twice. Every sibling list, including the paint list, is now stripped;
pinned by `an_exit_starts_from_the_pixels_its_subtree_rested_on`.

**MD3 entry used the exit duration.** Both directions used 200 ms. With
emphasized decelerate that made the surface look like it snapped open. Entry is
now 0.4 s and exit 0.2 s, matching the published pairing.

**The Fluent dialog was already settled on its first frame.** The control
tweened its own opacity with `cx.animate`, which has no start value on a node
that did not exist on the previous frame. The gallery compensated by mounting
the dialog closed for one frame; the WinUI gallery did not, so the bug appeared
only there. The dialog now declares `enter` and `exit` (83 ms linear fade on the
smoke layer, scale 1.05 over 250/167 ms on the surface), and the `open`
parameter and the mounting dance are gone. Pinned by
`a_dialog_declares_its_way_in_and_out_instead_of_arriving_at_rest`.

## Verification

| Test | Pins |
| --- | --- |
| `a_size_transition_reaches_its_target_through_the_layout` | Half-way value, and retarget starts from the current value |
| `an_entering_node_fades_in_from_its_declared_opacity` | Start opacity is painted, not skipped |
| `an_entering_node_is_painted_from_its_start_offset` | Offset moves paint and hit, not layout |
| `an_entering_node_scales_about_its_centre_and_hits_where_it_is_painted` | Scale origin and hit agreement |
| `a_scale_on_its_own_clock_outlives_the_fade_that_rides_the_presence` | Two channels, two clocks |
| `a_leaving_node_keeps_painting_until_its_exit_lands` | Held node keeps painting, then releases |
| `an_exit_starts_from_the_pixels_its_subtree_rested_on` | No double composite on the first exit frame |
| `a_kept_slot_holds_its_space_until_the_exit_lands` | `KeepSpace` keeps the row below in place, then reflows |
| `a_collapsing_row_takes_its_siblings_with_it` | `Collapse` moves the row below with the exit, not after it |
| `a_hug_axis_walks_to_the_extent_its_content_measured` | A hug card starts from its old extent and lands on the measured one, both ways |
| `a_modal_scrim_fades_instead_of_appearing_dark` | The landing frame is submitted |
| `an_auto_scrollbar_holds_after_the_wheel_then_fades_out_and_lets_the_loop_idle` | The engine-owned bar fade rides the leg clock, lands exactly, and stops asking for frames |

## Traps

| Change | Symptom |
| --- | --- |
| Tween opacity inside a control | First frame of a new node is its end state |
| Share one clock for fade and scale | One channel finishes first; the other reads as instant |
| Remove the node from `by_parent` but not from paint order | Translucent subtrees composite twice |
| Let a presence offset reach layout | Siblings reflow during a slide that should only move paint |
| Key the exit animation by logical id | A reopened node inherits the last exit's settled value |
| Pick a duration without the easing curve | A steep curve makes a "medium" duration look instant |
| Ask `Collapse` to shrink a hug or fill axis | Nothing shrinks: only a fixed pixel extent has a size to write |
| Toggle content from outside a control without `invalidate_decls` | The declaration cache serves the old subtree, so a hug target never changes |

## Status

Implemented: size transitions on fixed and `hug` axes, enter with
opacity/offset/scale, exit with engine-side holding and the three slot
policies, per-channel scale clock, and the automatic scrollbar's own fade.

## See also

- [Frame Pipeline](./frame-pipeline.md): where transitions resolve in the frame.
- [Style Objects](./style-objects.md): where motion curves are defined.
- [Controls](../controls/control-sets.md): which systems use which durations.
