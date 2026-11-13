# Cursor

The cursor has two halves: the shape the surface shows, resolved from the same
hit a press would use, and the pointer's position, which an app can step from
the keyboard. This page covers both.

## Resolution order

`cursor_shape_at(p)` settles the shape in this order:

1. **An open menu over the point.** A selectable row gives `PointingHand`; the
   rest of the menu surface gives `Arrow`. The menu is painted above the tree
   and owns the pointer over itself.
2. **The topmost opaque hit target**, then each ancestor in turn. The nearest
   node that declares a cursor wins.
3. **Selectable text.** A text node with `Sense::SELECT` gives `Text` without
   any declaration; the engine gave it selectability, so it gives it the
   I-beam.
4. **`Arrow`.**

An ancestor walk is what makes a container's cursor cover its children: a row
can declare `PointingHand` once instead of every label inside it.

## When it changes

The shape is part of a frame, not an independent channel. `build_step` records
the resolved shape in `cursor_sent`; `pointer_affects_frame` compares the shape
at the new position against it. A move that changes only the shape still
requests a frame, because otherwise the pointer would sit on a paragraph
showing the previous control's hand.

## Moving the pointer {#moving-the-pointer}

No Wayland core request moves the pointer, so a keyboard-driven move is a step
the platform may refuse. `ui_platform::cursor` holds the portable half: the
`PointerWarp` trait a backend implements, and `Cursor::step`, which applies a
relative step and reports where the pointer landed.

| Stage | Call |
| --- | --- |
| The build asks for a step, in logical points | `Cx::nudge_pointer(dx, dy)` |
| The build banks it | `AppLoop::pointer_steps` |
| The surface drains it after the frame | `AppLoop::take_pointer_steps` |
| The backend performs it | `WaylandWindow::nudge_pointer` (`wp_pointer_warp_v1`) |

The request is a step rather than a position because an app cannot read the
pointer: only the engine sees the seat's motion. So the surface supplies where
it last saw the pointer and gets back where it now is. `stepped` clamps to
`extent - 1`; a compositor rejects a warp onto the edge itself.

The engine takes the step in logical points; how large a step is belongs to the
app. The screenshot app steps one pixel of the picture under the pointer, so
the move lands on that picture's own grid, and ten of them with Shift held —
a count the press settles from the modifiers it resolved with, before the
surface knows which picture the pointer is over.

A refusal moves nothing, and that is not an error to report: with no protocol,
no pointer focus, or no recent input serial to authorise the request, the step
answers `None` and the caller keeps the position it had. The Wayland surface
records the position it asked for at once, so several steps in one turn
accumulate instead of each starting from a stale read.

The frame that was just built read the pointer's old place, so the surface asks
for one more frame after applying steps. Without it the chrome under the pointer
would show the previous cell until unrelated input arrived.

## Traps

| Change | Symptom |
| --- | --- |
| Declare `Text` on every paragraph | It duplicates what `Sense::SELECT` already gives, and lies on text that is not selectable |
| Give a container a cursor but leave a child pass-through | The child's ancestors are still walked, so the container's shape applies; verify with a hit probe before adding per-child cursors |
| Resolve the shape without a frame | The shape lags the pointer by one event |
| Compare shapes only on hover changes | Moving between two paragraphs keeps the old shape |

## Verification

`selectable_text_shows_the_text_cursor_without_the_app_naming_one` moves the
pointer over a selectable paragraph, asserts a frame was requested through
`take_dispatched`, and reads `outcome.cursor == Text`.

`a_step_is_relative_clamped_and_reported` covers the step arithmetic with a
recording warp, including the refusing platform;
`a_pointer_request_leaves_the_build_once` checks that a build's request reaches
the surface exactly once.

## See also

- [Pointer Model](./pointer-model.md): hit testing and gesture ownership.
- [Frame Pipeline](./frame-pipeline.md): why a cursor change is frame demand.
