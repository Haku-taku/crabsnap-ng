# Pointer Model

The pointer produces one normalized stream of presses, moves, releases, wheel
notches, and clicks. Controls never hit-test themselves and never parse raw
events; they declare what they are sensitive to and receive semantic reports.

## Sensitivity

A node declares a `Sense` bitset:

| Flag | Means |
| --- | --- |
| `HOVER` | The pointer is over this node |
| `CLICK` | A press that never became a drag is delivered as an activation |
| `DRAG` | The node may originate a drag |
| `DROP` | The node accepts a dropped item |
| `FOCUSABLE` | The node can hold keyboard focus |
| `SCROLL` | Wheel events route here, not past it |
| `SCROLL_OWNED` | The engine owns the offset and accumulates the wheel into it |
| `SELECT` | Characters can be selected with the pointer; opt-in |
| `CAPTURE_KEYS` | Raw chords arrive here instead of the shortcut table |
| `SECONDARY` | The secondary button's press and release are reported here |
| `MIDDLE` | The middle button's press and release are reported here |
| `PASS_THROUGH` | Hit testing passes through this node |

`SELECT` is opt-in on purpose. A label that defaulted to it would swallow the
press of the control it sits in, and a text field's own text would start a
read-only selection instead of taking focus.

## Hit testing

`HitIndex` is rebuilt every frame from the frame's rects. Each target is
`Opaque` (consumes input) or `PassThrough`.

- `topmost_consumer(p)` returns the topmost `Opaque` target: the one press and
  hover belong to.
- `stack_at(p)` returns every target at `p`, top to bottom, stopping at the
  first opaque one. Menus and popovers use it to decide whether a press is
  inside or outside.

Every node kind except text is `Opaque` by default, whether or not it declares any
sensitivity. A container that only paints — a full-surface scrim, a dimming layer, an
adornment — therefore *takes* the press from the node beneath it, and the app sees its own
id in `cx.pressed()` instead of the one that senses the gesture. Such a node declares
`PASS_THROUGH`, which keeps it in the index for painting and stacking while letting the
search continue.

Hit testing uses the same transform as painting, so a scaled or offset presence
is hit where it is drawn. The index reads `Tree::scaled_visual_rect`, which
composes every scale between the root and the node in paint order: a presence
scale turns about the node's centre, a declared `ContainerParams::scale` about
its own top-left. `Tree::paints_scaled` is the cheap test that says the index
has to ask for that rect at all; a tree with no scale anywhere keeps using the
laid-out one.

## The secondary button

A right press focuses the node under the pointer and opens its menu, and until
recently that was all it did: an app could not see the button at all. A tool
whose secondary button means something else — the screenshot loupe is one —
declares `Sense::SECONDARY` on the node that should hear it. The nearest
ancestor declaring it is then told `Change::Secondary { down: true }` on the
press and `{ down: false }` on the release, and the menu waits.

The wait is what lets one button carry two meanings. A release within
`SECONDARY_TAP` (300 ms) is a tap: the engine opens the menu the press would
have opened, at the press point. A longer hold opens no menu. This mirrors the
legacy overlay's right-tap/right-hold split (`RIGHT_TAP_MS` there), and it is
why a mark's context menu still opens with a quick right click while a held
right press aims the loupe.

A node without `SECONDARY` in its ancestry keeps the immediate behaviour: the
menu opens on the press, before the release.

## The middle button

The engine has no gesture of its own for the middle button: a middle press
starts at the topmost consumer under the pointer and walks up to the nearest
node declaring `Sense::MIDDLE`, which hears `Change::Middle { down: true }`.
The release goes to the node that heard the press, wherever the pointer has
moved by then, so a node that slides out from under the pointer still hears it.

Because the walk starts at the hit and only looks for `MIDDLE`, a pressed
control does not take the middle press from an ancestor that declares it; the
two buttons have separate owners. The node that heard the press is the press
owner for `cx.local()` while the button is held, so a control that moves
something with it answers in its own space. Motion during the hold keeps the
loop drawing: `pointer_affects_frame` counts a held middle button like any
other gesture.

## Gestures

- A press captures the consumer for the rest of the gesture.
- Movement past `DRAG_THRESHOLD` (4 px on either axis, not the diagonal) turns
  the press into a drag. Below it, the release is an activation.
- Exactly one owner drives each gesture kind: `pressed` for the captured node,
  `pan` for an engine-owned scroll container, `select_drag` for a text
  selection. The text selection deliberately does not keep `pressed`, which is
  why it has its own owner. Its head is the selectable block under the pointer,
  so one drag can cover several paragraphs; see
  [Text and input](./text-and-input.md).
- The engine computes the ancestor paths for clicks, activations, and drops at
  the start of the frame, so a control's root can answer `clicked_within` even
  when a descendant was hit.

## Frame demand

Continuous pointer samples are absorbed unless they can change a pixel.
`pointer_affects_frame` returns true when:

- any gesture owner is active (`pressed`, `pan`, `select_drag`);
- a control read the raw pointer this frame (`pointer_read`);
- the hover owner changed;
- the resolved cursor shape changed.

> **Wrong path:** a gesture that changes pixels without changing hover — a
> text selection — needs an owner in that list. Without one, every move during
> the gesture is dropped and the result appears only on release.

## Coordinates {#coordinates}

Events carry device pixels. `pointer_local` converts the current pointer into the
captured node's space; the node is the one from the press, so a drag keeps answering in
that space even after the pointer leaves the node's rect.

A drag has three measurements, and the wrong one is how an object drifts. `cx.local()` is the
pointer in the pressed node's own space. `cx.drag_step()` is the movement since the previous
frame, summed over the events one frame swallowed. `cx.press_origin()` is where the press
landed, in surface coordinates and fixed for the whole gesture. Move the object by
`position at press + (pointer - press_origin)`: an absolute measurement, so a frame that brings
no new input cannot move anything and nothing accumulates.

Never move the surface that receives the drag. The compositor measures a pointer sample against
the surface *as it was when the event was sent*; a client that moves its own surface and then
reads the next sample against the new placement has added its own move to the next frame's
measurement, and the object accelerates away while the hand is still. Put the draggable objects
in one fixed surface — a canvas the size of the output — and move the nodes inside it.

## The surface's input shape

A layer surface publishes, as its input region, exactly the areas its tree consumes: the union
of every `Opaque` target's `bounds ∩ mask` (`HitIndex::consumer_areas`). That union is, point for
point, where `topmost_consumer` names a node — a `PassThrough` target never ends the search and an
ignored one is not in the index — so the region the compositor applies and the routing inside the
surface cannot disagree. A tree whose root is `PassThrough` and that holds nothing opaque
publishes no rectangles at all: an empty region, a surface the compositor forwards every click
through.

The engine's context menu is the one thing drawn without a node, so `collect_input_areas` adds its
panel to that union by hand. The menu opens at the click and may reach past the node that declared
it; without the panel, a row outside the union would be a painted row the compositor hands to
whatever is under the surface.

The region is pending surface state, so it rides a commit. The frame that computed it draws and
commits; when nothing changed, `commit_pending` reports the pending region and the engine commits
anyway.

Only a layer surface gets one. A toplevel's shape is its window manager's business, and the
protocol default — the whole surface — is what a decorated window wants. A layer surface is
placed by the app, has no titlebar making up for the space around it, and its shadow margin is
painted rather than interactive.

## Keyboard is not the input region {#keyboard-is-not-the-input-region}

The input region decides where the pointer goes, and only that. Keyboard focus is a seat
property: the compositor gives it to one surface and leaves it there until it re-evaluates. A
layer surface that covers the output but claims small patches therefore keeps the keyboard over
the whole screen even though clicks between the patches land below it.

The client's only lever is the layer's keyboard interactivity. Lowering it to `None` and
committing is what makes the compositor clear the focus and hand it back to the window below;
there is no pointer-side request that does the same. Raising it needs a transition the compositor
notices, and Hyprland notices only one: a focused layer going from `Exclusive` to `OnDemand`. So
`KeyboardMode::FollowPointer` lowers to `None` whenever the pointer leaves, and on the way back
sends `Exclusive` first (only when the surface has no keyboard focus) and `OnDemand` second, each
with its own commit — the intermediate value has to be the committed one to be seen.

"The pointer is on what we consume" is the same fact the input region is built from — the
topmost consumer under the pointer — plus two states the region cannot express: a live gesture
(a drag keeps its capture past the node's edge) and the engine menu, which has no node. The
engine applies it only for `FollowPointer`; every other mode is the app's one-time choice. It
travels with a frame, so a target that must act on the pointer leaving cannot wait for it: a
still pointer builds no frame, and the departure would never be reported. A backend that sees
the leave itself acts there, as Win32 does with `WM_MOUSELEAVE`.

Checked against the Hyprland source in `/home/ollie/git/hyprland/Hyprland` (v0.56.0-224,
`src/desktop/view/LayerSurface.cpp`, `onCommit`): the `WASLASTFOCUS && →NONE` branch clears the
surface focus and calls `refocusLastWindow`, and the `WAS_KEYBOARD_EXCLUSIVE && →ON_DEMAND`
branch calls `simulateMouseMovement`. The running session is 0.57.1 [unverified: the checkout is
one release behind]. The wire check is `WAYLAND_DEBUG=1`, where
`zwlr_layer_surface_v1.set_keyboard_interactivity` must read `NONE` as the pointer leaves a pin
and `EXCLUSIVE` then `ON_DEMAND` as it returns.

## Traps

| Change | Symptom |
| --- | --- |
| Turn on `SELECT` for a plain label | The label eats the press of the button it sits inside |
| Clear `pressed` without registering another gesture owner | Moves during the gesture are absorbed |
| Hit-test with untransformed rects | Scaled content is hit away from where it is painted |
| Treat the drag threshold as a diagonal distance | A mostly horizontal drag never starts |
| Read raw events in a control | The engine's routing, focus, and capture rules are bypassed |
| Put a full-surface decorative child over the sensing root | The press lands on the decoration: `cx.pressed()` never names the root, so the drag never starts |
| Start a drag from a button held before the surface appeared | The hotkey's own press is read as the drag's origin; catch the press at its edge instead |
| Leave a painted margin on an `Opaque` root | The layer surface's input region includes the margin: clicks, hover and keyboard focus land in shading that means nothing under it |
| Expect the input region to bound the keyboard | A full-output layer keeps typing wherever its pointer region is empty, until it asks for `None` |

## See also

- [Cursor](./cursor.md): how the shape is resolved from the same hit, and how
  an app steps the pointer from the keyboard.
- [Frame Pipeline](./frame-pipeline.md): frame demand and when one-frame facts
  are consumed.
- [Scroll Containers](./scroll.md): `SCROLL_OWNED` and the pan gesture.

## Corrections

**A pin's shadow swallowed clicks.** Every layer surface claimed its whole rectangle as input, so
the 20 px margin a pin reserves for its shadow and hover ring took presses and hover meant for
the desktop under it. The engine now derives the region from the hit index and
publishes it on layer surfaces only; `consumer_areas_are_exactly_where_a_consumer_wins` checks
the union against `topmost_consumer` point by point, including under a pass-through decoration.
Verified end to end in a nested session: `WAYLAND_DEBUG` shows one `set_input_region` per surface,
carrying `add(20, 20, 1224, 676)` for a 1224x676 pin; with the pointer parked in the shadow the
hover ring's top edge samples `(8, 24, 91)`, where the binary without regions lit it — the same
`(153, 160, 187)` both show once the pointer is on the picture.

**A menu reaching past its owner was not clickable.** The pin declares its menu on the picture
node, and the panel at the click reaches below the picture whenever the rows are taller than the
node. The input region was the hit index's union, which has no entry for a menu — the engine draws
it outside the tree — so the last row lay outside the region and the compositor handed its click
to the layer under the surface. `collect_input_areas` now adds the panel. Caught by
`a_menu_reaching_past_its_node_stays_inside_the_input_region` in
`ui/crates/ui-platform/src/view/tests.rs`: a 200×200 `PassThrough` page with a one-row menu opened
at `(50, 190)` publishes only `(0, 0, 200, 200)` without the panel, and a region containing
`(150, 236)` with it.

**The screenshot overlay could not be dragged at all.** Its root senses `DRAG` and draws the
frozen frame; a full-surface child painted the dimming bands and the selection border. That
child is a `Container`, hence `Opaque`, so it took every press, and the app's
`cx.pressed() == Some(root)` was never true. The child now declares `PASS_THROUGH`. Caught at the
time by a scripted probe in `apps/crabsnap` (since removed) that drove a press, four moves and a
release through `AppLoop::dispatch` and printed the rectangle the app ended up with: `settled
None` before the change, `Rect(768, 288, 1024, 576)` after.

**A node that heard a middle press was given no local pointer.** `pointer_local` took its node
from `pressed` alone, so a middle press reached the app with `cx.local() == None`: the button
was reported, but not where it had landed, and the screenshot app's middle-drag of the selection
never started. A held middle press is now a press for that resolution
(`info.pressed.or(self.middle)`), so the node in the `MIDDLE` ancestry sees the pointer in its
own space for as long as the button is down. Caught by `the_middle_button_drags_a_settled_selection`
in `apps/crabsnap/src/main.rs`, which settles a selection, presses the middle button inside it,
and moves.

**A full-output canvas kept the keyboard after the pointer left it.** The pin canvas asked for
`OnDemand` once, at map time, on the reading that the compositor withdraws the keyboard when the
pointer leaves what the surface claims. It does not: focus is a seat property that survives
pointer motion, and the input region says nothing about it, so typing anywhere on the screen went
to the canvas. `KeyboardMode::FollowPointer` makes the request a per-frame fact instead: `None`
when the pointer leaves, `OnDemand` when it arrives. What catches it now is the engine's
`set_pointer_inside` fact (the same topmost-consumer test the input region uses) and the
`WAYLAND_DEBUG` wire check in the section above. The legacy project claims the same compositor
behaviour (`docs/notes/pin-keyboard-follows-hover.md` there), unverified here; the Hyprland
source in this checkout matches it, and the running compositor is one release ahead of that
checkout.
