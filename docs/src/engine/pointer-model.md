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

A node without `SECONDARY` in its ancestry and no drag payload keeps the immediate
behaviour: the menu opens on the press, before the release. A payload makes the press a
possible drag even without `SECONDARY`, so its menu waits for the release and the
`SECONDARY_TAP` rule decides — the same wait a declaring node gets.

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

### Click counts {#clicks}

A click carries how many presses of its button it was part of. A press counts as the
continuation of the previous one when it arrives within the system's double-click time
and rectangle (`GetDoubleClickTime` and `SM_CXDOUBLECLK`/`SM_CYDOUBLECLK`, scaled to
logical units) of the previous release of the same button; the count is per button, so
a right click does not extend a left burst. A third press in the same burst reads 3,
and a press that travels past `DRAG_THRESHOLD` ends the burst instead of extending it.

The count reaches a control three ways, all naming the frame's click:

| Surface | What it says |
| --- | --- |
| `Change::Secondary { down, clicks }` / `Change::Middle { down, clicks }` | the catch-all kinds' burst length, on the press and the release |
| `WidgetState::clicks` / `cx.clicks_within(id)` | the same number for a primary click, and for any ancestor of the clicked node |
| `FrameInfo::clicks` | the burst length of this frame's click, for an app that draws from `view` directly |

`Dep::ClicksWithin` is what makes a control rebuild on the second press and not only on
the first. Neither `Activated` nor `Secondary` is split into single- and double-click
variants: the control decides what a count means, which is why a file row opens on 2 and
a text field selects a word on 2 while both keep their single-click behavior.

## Drag and drop {#drag-and-drop}

A drag can carry a payload, and the payload can cross the surface boundary: out to the
desktop, in from another application, or between two nodes of one surface. The gesture
machinery above is unchanged — a press that travels past the threshold is still the one
`DRAG` owner's drag — and the payload rides on the declaration instead of the event stream.

### Declaring

| Builder | On the node | Also sets |
| --- | --- | --- |
| `Decl::drag_data(data)` | the payload a drag from this node carries | `Sense::DRAG` |
| `Decl::drag_out()` | hand the gesture to the operating system instead of keeping it in-app | — |
| `Decl::accept_drop(types)` | the MIME patterns this node takes | `Sense::DROP` |

`Container` forwards all three, and `Decl` is what every element already builds, so no
element needs to know drag and drop exists.

The payload is a `DragData`: a list of `DragItem { mime, data }` representations, the
effects it may be put to (`DropEffects`), and the one a target performs without a modifier
(`DropEffect`, `Copy` by default). With a modifier held, `DragData::negotiate` answers:
Ctrl copies, Shift moves, Ctrl+Shift links, clamped to the permitted set — the Wayland
table, which Windows matches apart from Command. Patterns compare ASCII-insensitively, and
`*`, `*/*`, and a family such as `text/*` are patterns; anything else is an exact type. A
`DROP` node that never called `accept_drop` — a plain `sense(Sense::DROP)` — takes any
drag, which is what gesture-only dragging did before payloads existed.

The secondary button drags too: a press on a node with a payload arms the gesture, the
release that crossed the threshold is a drop rather than a menu tap, and
`DropReport::button` carries which button started it. A target reads `Secondary` as "ask
where this should go" — Explorer's right-drag menu, drawn by the app at the release point.

### Inside the surface

A source without `drag_out` keeps the drag internal. The engine tracks the pointer, walks
up from the hit to the nearest node that declares `DROP` and accepts one of the offered
types, and reports:

- the target: `Change::DragEntered { types }` when it becomes the target, `DragExited` when
  it stops being one, and `DroppedOn(DropReport)` when the release lands on it;
- the source: `DragStarted`, `Moved`, `Dropped { target, outcome }` or `Cancelled`.

A `DropReport` carries the decoded payload, the negotiated effect, the modifiers and button
held at the release, the point, the side that performs the effect (`Performer::Target` for
an in-app drop), and whether the drag came from another application (`foreign`).

`cx.drag_over_within(id)` (and `WidgetState::drag_over`) is true while the live drag's
target is the node or inside it, which is what a highlight reads; `cx.drag_types()` is what
the drag offers. Both record a cache dependency (`Dep::DragOverWithin`, `Dep::DragTypes`),
so a declaration that read one is rebuilt when the target or the offered types change.

The source hears the target's entry before its own `DragStarted`, so a highlight that reads
the source's state sees it already settled. A drop ends the hover without a `DragExited`:
`DroppedOn` is the exit.

### To and from the operating system

`drag_out()` makes the threshold crossing hand the gesture to the host instead, on the same
frame. Only the source is tracked in-app from then on: the host reports the ending as
`Event::Dnd(DndEvent::Finished { outcome })`, which becomes `Change::Dropped { target,
outcome }` — `target` names one of this app's own targets when the payload came back to it —
or `Cancelled`. Because the host owns the pointer for the whole gesture, the release never
reaches this loop; the engine settles the press (and its ink) when `Finished` arrives, and
ignores every pointer event in between, because those belong to the gesture and a release
let through would click the node the drag started from.

The two platforms split a move differently, and `DropOutcome { action, performed_by }`
names the half the source still owes. On Win32 a target signals an optimized move by
clearing the source's path list (`SetData` with a null medium), so `performed_by` is
`Target` and the source does nothing; without that signal the target only copied, and the
source removes the originals. On Wayland the source always performs a move after
`dnd_finished`. An in-app drop is `Performer::Target`.

An incoming drag arrives as `Event::Dnd` phases: `Entered { pos, types, effects }`,
`Over { pos }` (many), then either `Dropped { pos, data, button, performer }` or `Left`.
The payload is revealed only at the drop. `AppLoop::drop_effect()` is the effect the
resolved target negotiated, which each host publishes to the operating system so the
pointer shows it.

| Platform | Incoming | Outgoing |
| --- | --- | --- |
| Win32 | OLE `IDropTarget` on the window thread; every clipboard format the table can decode is read back at the drop; `IDropTargetHelper` paints the source's image and drop description | `DoDragDrop` on the window thread, its modal loop pumping the surface so frames keep arriving; the payload is exported in the formats `clipboard::item_formats` knows, plus `Preferred DropEffect` for the source's preference, and the source node's own drawing is rendered off-screen and registered with `IDragSourceHelper` so a target can draw it under the pointer |
| Wayland | `wl_data_device.enter`/`motion`/`drop`/`leave`; every announced MIME is read down its own pipe after the drop, bounded by type count, size, and a two-second timeout | `wl_data_source.start_drag` with every action the payload permits, `dnd_finished`/`cancelled` as the outcome |

Two losses are the protocols', not the engine's. Wayland has no link action, so a `Link`
payload is offered as `Copy`; and a Win32 payload whose types have no Windows form (anything
but text, files, PNG, or a DIB) cannot leave the process, and the gesture is cancelled.
Wayland has no such restriction.

On Win32 the drop target answers `DROPEFFECT_COPY` before the first frame has resolved a
target — OLE asks during `DragEnter` — and the window replaces it with the engine's effect on
every subsequent frame. On Wayland the destination accepts the first offered type at
`enter` for the same reason: a version-3 drag nobody accepts is cancelled outright, and the
engine's own verdict needs a frame that has not been built yet.

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

The union has no exception: the engine's context menu is a declared panel, so its rows are nodes
in the index like any control's. A menu that reaches past the node that declared it is still in
the region, because it is still part of the tree.

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
topmost consumer under the pointer — plus one state the region cannot express: a live gesture,
where a drag keeps its capture past the node's edge. The
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
| Time a double click in the app | The burst is the engine's, per button, on the desktop's own time and rectangle; a private timer disagrees with the system and with every other control |

## See also

- [Cursor](./cursor.md): how the shape is resolved from the same hit, and how
  an app steps the pointer from the keyboard.
- [Frame Pipeline](./frame-pipeline.md): frame demand and when one-frame facts
  are consumed.
- [Scroll Containers](./scroll.md): `SCROLL_OWNED` and the pan gesture.

## Corrections

**A right press on a payload started no drag.** Arming the secondary gesture sat inside the
`SECONDARY`-owner branch, so only a node that asked to hear the button could leave with the
pointer; a row carrying files dropped its drag on the floor and, with no declared menu,
nothing happened at all. The arming now asks the hit node for a drag owner on its own, and
`SecondaryPress` records `node: None` when nobody reports the press, so the tap rule still
holds the menu back. Caught by
`a_secondary_press_on_a_payload_starts_a_right_drag` in
`ui/crates/ui-platform/src/view/tests.rs`, which shows the tap taking no native drag and
the threshold crossing taking one.

**A drag entering a target asked for no frame.** `set_drag_target` returned only the app's
answer to `DragEntered`, so an app that keeps no state — every app whose highlight reads
`cx.drag_over_within` — never got the frame that would paint it, and the host kept the
previous `drop_effect()` in front of the pointer. A target change is now a frame by
itself. Caught by `a_drag_entering_a_target_asks_for_a_frame_by_itself`, driven with an app
whose `changed` always answers `false`.

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

**The input-region special case was the symptom, not the fix.** Adding the painted menu rectangle
to the union made that one click work, but the menu still had no node: no hover, no press, no
`Change::Activated`, no popup surface. The menu is a declared panel now, so its rows are in the
index and in the region like every other node, and `collect_input_areas` is back to the hit
index's union alone. The old test's premise is gone with the special case; the invariant — a row
past the owning node still takes the press — is `a_menu_reaching_past_its_node_still_takes_the_press_there`
in `ui/crates/ui-platform/src/view/tests.rs`, which presses the row and expects the pick.

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

**A drag-out left its source pressed.** The operating system takes pointer capture for a
native drag, so the `PointerReleased` this loop waits for never arrives; the source kept
`pressed`, its ink, and its splat after the gesture had ended. `DndEvent::Finished` now
settles the press where the engine keeps it, and a host that refuses `begin_drag` (no payload
format it can carry) reports the same ending, so the loop never believes the button is still
down. The engine's four DnD contract tests cover the payload paths; the press settling and the
two platform hosts' live gestures are not yet run end to end.

**A drag the engine had not judged yet needed an answer before its first frame.** OLE asks
`IDropTarget::DragEnter` for a `DROPEFFECT` immediately, and a version-3 Wayland drag nobody
accepts is cancelled outright. Both hosts start with a permissive answer — `DROPEFFECT_COPY`
on Win32, the first offered MIME on Wayland — and the engine replaces it on every frame after
the first build (`AppLoop::drop_effect`) so the pointer shows the target's real verdict.

**DoDragDrop is modal, and it belongs to the thread that owns the pointer.** The first
version ran it on a worker STA thread so the window could keep painting. Measured, that does
not work: the worker has no window the OS feeds, so OLE's loop never observes the release —
`QueryContinueDrag` was never called and `DoDragDrop` never returned, while the button went
up. It now runs on the window's thread inside the turn that started the gesture, and the
app shows the frame it had until the gesture ends; a drag image under the pointer carries
the element through it. Two consequences had to be handled with it: the pointer events the
window still sees during the gesture are the OS's and are dropped (otherwise the release
clicks the source after the drag), and a drop that comes back to this app's own window is
reported to the source as `Dropped(Some(target))`, because that landing is the one the loop
did see. The image itself is the screen under the source node, captured into a 32-bit DIB —
the window is GPU-composited, so nothing can ask it to render into a DC — and registered
through `IDragSourceHelper`; a target only ever paints it through `IDropTargetHelper`, which
the Win32 backend now instantiates too, and which OLE's helper needs `SetData` to accept
(the image and its drag context are handed to the data object after the payload was).

**One drag state served both directions.** An in-app drag knows its payload from the press; a
native drag learns it only at the release. Two sessions would have duplicated target
resolution, cache dependencies, and the hover highlight, so `DragSession` holds both and its
`types_decide` says when the offered types decide a target: a started gesture with no payload
still lets any `DROP` node take it. The Wayland host adds its own reading of the same rule —
`wl_data_device.enter` must be answered before the engine's first frame can decide.

**The drag image was a crop of the screen, not of the element.** The first image was the
source's window rectangle read back with `BitBlt`, so everything behind and beside the node
travelled with it: a card dragged out of a panel carried the panel's background in its rounded
corners, and a sibling painted behind it showed through. The engine now cuts the image from the
frame's `DrawList` instead. `DrawList::owners` names the node behind every primitive, and
`DrawList::subtree` copies the ones inside the source, in their existing clips and layers, so no
primitive is rewritten; the UI's CPU renderer draws that list into an off-screen buffer of the
surface's size at the surface scale, and the box those primitives actually painted is cropped
out. The host converts the straight-alpha BGRA into the premultiplied DIB `SHDRAGIMAGE`
requires. A node that painted nothing has no box to crop and falls back to the host's capture.
Two things cannot survive the separation and are left out rather than approximated: a
`Backdrop` samples the surface the drag is departing, and a `Canvas`'s picture is only filled
on the CPU present path, so a node wrapping one shows everything but the canvas. Verified live
with a drag whose ghost corner pixels read the panel behind the drag rather than the page the
source sat on — the image is transparent there, while a screen crop could only be opaque — and
pinned by `a_drag_image_is_the_element_and_never_what_sat_behind_it`, which paints a red card
over a blue sibling and rejects blue from every pixel of the image. A text drag narrows the
same crop to the selected lines; see
[Text and Input](./text-and-input.md#selection-across-blocks).

**DoDragDrop held the thread, so the surface stopped.** The gesture has to run on the window's
thread ([above](#drag-and-drop)), and the loop that runs it does not return until the drop:
the surface kept presenting the frame it had, so a drop target's highlight never appeared and
an animation stopped dead until the button came up. OLE's loop only dispatches the thread's
messages, so the surface can be driven from inside it. The Win32 host arms a 8 ms timer on the
source window before `DoDragDrop` and kills it after; `WM_TIMER` calls back into the engine
through a thread-local (the closure's lifetime is erased for the duration, and the slot is
disarmed before the borrow ends). One tick takes the Dnd phases the drop target queued,
rebuilds and presents their frame, and does nothing at all when the queue is empty and no
animation is running, so a still drag costs no painting. Qt reaches the same place by calling
`processEvents` from `QueryContinueDrag`; a timer needs no OLE subclass, which also keeps the
incoming targets' signatures untouched. Verified live: a screenshot taken mid-drag shows the
drop panel already highlighted and its hint text painted, before the release. The title-bar
move gets the same pump on the Win32 window: the caption press is deferred to the host loop
rather than started inside message dispatch, which starts the system's move — snapping
included — with a timer armed, so an animation keeps running while the window is dragged.
`SC_SIZE` from the system menu and the system menu itself still block, because neither
route goes through the loop that can arm a pump.

**A double click existed for text only, timed inside the view.** The selection code carried
its own 0.4 s and 4 px repeat rule, so no other control could see a second press: the picker
documented that the engine reports no double-click and required select-then-accept, and a
control that wanted "press twice to run" had to count in app state. The burst now belongs to
the engine and is kept per button, measured against the desktop's double-click time and
rectangle (Win32's `GetDoubleClickTime` and `SM_CXDOUBLECLK`, DPI-scaled; a scripted clock in
tests), and a drag or a press outside either starts a new burst. It reaches controls as
`WidgetState::clicks`/`cx.clicks_within`, `Change::Secondary`/`Middle { clicks }` and
`FrameInfo::clicks`; text word selection now reads the same count instead of a private one.
Caught by `a_press_joins_its_burst_only_within_the_desktops_gap_and_rectangle` in
`crates/ui-platform/src/view/tests.rs` (red: a second press inside the rectangle was two
separate clicks) and the picker's
`a_double_click_opens_a_file_while_one_click_only_selects_it`.
