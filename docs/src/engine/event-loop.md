# The Event Loop

How a process waits for events, what runs inside that wait, and why the application only
registers callbacks instead of driving framework services itself.

## One loop, one reader

A process has one event loop. It is owned by the framework and it is where every wait happens:
the compositor connection, the clipboard, global hotkeys, timers, and any source the application
registers. There is exactly one reader of the compositor connection; a second reader would steal
bytes from the first, and a second blocking wait would decide alone when the whole process wakes.

The loop is not a new concept layered on the engine. It is the same shape the window already
uses — `Driver` decides whether a frame is due, `FrameHost` provides the surface state — with the
waiting lifted out of the per-surface step and into one place.

> **Wrong path:** calling a per-surface step that blocks until its surface has news. With one
> surface the mistake is invisible; with an overlay, a pin canvas per output and a settings
> window, the surface stepped first can hold the process while another has work, because all of
> them share one connection.

## Portable host, platform backend

The loop lives in the portable layer; waiting and native objects do not.

| Piece | Crate | Responsibility |
| --- | --- | --- |
| `Host` | `ui-platform` (PORTABLE) | The loop, deadline aggregation, surface slots, timers, application callbacks |
| `SurfaceHandle` | `ui-platform` (PORTABLE) | One surface as the loop sees it: a non-blocking step, its wake deadline, its closed state, a texture added after it opened |
| `Backend` | `ui-platform` (PORTABLE trait) | Open a surface, hand over platform services, wait on platform handles |
| `Backend` impl | `ui-platform-wayland` (PLATFORM_BOUND) | Connection, queues, poll, surfaces, clipboard, hotkeys, outputs |
| `Backend` impl | `ui-platform-win32` (PLATFORM_BOUND) | One message pump for every window, an event as the waker, `DwmFlush`, clipboard, outputs, languages |
| `MessageCtx` | `ui-platform` (PORTABLE) | What a channel handler or a hotkey handler may ask of the loop: the outputs, a surface it opens, a texture on it, a frame on it |

`Host` contains no protocol types, no file descriptors and no `WaylandLink`. An application
obtains one through its own platform layer (for this project, `crabsnap-platform`), which picks
and constructs the backend; the portable application code never names a platform.

A `SurfaceHandle` is implemented by the backend's surface wrapper. Its `step` never waits: it
drains what has already arrived, decides, and draws only if a frame is due and the compositor has
opened a frame window. `add_image` exists because content can arrive after a surface does: a
daemon's pin canvas is already on screen when the second pin is pasted, so that texture cannot
wait for another `open`.

A channel handler answers without owning a surface, so it is handed a `MessageCtx` instead of a
bare output slice: `open` returns a `SurfaceId`, and `add_image` reaches the surface it named.

A request may also change state the surface reads only while building its tree — a daemon closing
a pin, hiding it, swapping its look. That change lands in app state the surface never sees until
the next frame, and a surface with nothing to draw and no input waking it will not build one.
`MessageCtx::request_frame` is the door for it: the handler names the surface, the loop marks it
for a build and wakes. Opening a surface and adding a texture already imply this; `request_frame`
is how a handler says the same thing after changing only what a build would read.

## A turn

```text
loop {
    for surface in surfaces { surface.step() }   // non-blocking; may draw
    services.clipboard.serve()              // answer selection requests
    for activation in services.hotkeys.poll() { handle(activation) }
    run due timers and ready sources
    if quit requested or no surface may remain {
        if a job is still running { wait a little; continue }   // its result must not be lost
        break
    }
    if a surface was opened or textured this turn { continue }  // it has no frame yet
    if the job worker finished something { ask every surface for a frame }
    deadline = min(each surface's next wake, each timer, each source, the host's own deadline)
    backend.wait(deadline)                  // poll: connection, service fds, source handles
}
```

`backend.wait` is the only blocking call. It polls the connection once and reads it; each
surface's step then dispatches its own queue without reading again. Events for every queue sit in
the connection's buffer until that queue dispatches them, so one read serves all surfaces.

## Pacing without a blocking wait

Frame pacing is a state question, not a wait: a surface that has presented is not drawable until
its frame callback arrives (`can_draw`). The loop calls `step`, the step returns "waiting", and
the loop polls; the callback wakes the connection and the next turn draws. The old
`wait_for_frame` (which spun to a 500 ms deadline) and the per-surface `wait_until` (250 ms cap)
disappear; the loop keeps a single maximum timeout so a compositor that stops answering cannot
hang the process.

A surface that needs to be woken by itself — a caret blink, a snapping scroller, an animation in
flight or a build that read the clock — reports it through its handle's `next_deadline`, which
the loop folds into the same deadline. A turn that drew is not evidence that nothing else is
owed: on a backend where no compositor event follows a presented frame, only the deadline brings
the loop straight back. A paste transfer in flight reports the pipe's read deadline the same way.

The loop takes a turn before it waits. The first frame of a surface must not wait for an event
that may never come, and a surface opened with a pin already on it has work due immediately.

Work created *inside* a turn needs the same rule. A turn steps the surfaces first and runs the
application's callbacks last, so a surface opened by a callback — the overlay asking for a pin
canvas — has not been stepped at all, and the canvas's own setup already consumed the events
that would have woken the loop. Waiting there holds it until unrelated input arrives. The loop
therefore turns again before it waits whenever a surface was opened or handed a texture during
the turn. Turning again is not the same as drawing, though: a surface builds only when a frame
is due, so `MessageCtx::add_image` and `request_frame` each ask the surface for one. Without
that a pin added to a canvas that is already up is held by the board and never painted.

## Services

| Service | Portable form | What the backend does |
| --- | --- | --- |
| Clipboard | Copies from surfaces reach the loop; a handler with no surface asks through `MessageCtx::copy` | Own the data source and its pipes, release on replacement; the privileged data-control channel when the compositor has it, so a copy needs no input serial |
| Hotkeys | `GlobalHotkeys` trait (exists) | Portal first, compositor protocol second; background reader wakes the loop. On Windows `RegisterHotKey` against a message-only window, whose procedure records the press for `poll` |
| Outputs | `OutputInfo` list through `Backend::outputs`, refreshed every turn | A second `wl_registry` on its own queue binds every `wl_output` and tracks add/remove, so a monitor plugged in after start is seen |
| Local channel | `Channel` through `Host::listen`: whole messages, a reply path, `Reply::deferred` for a worker's answer | A `SOCK_SEQPACKET` socket on Unix; a message-mode named pipe on Windows, with an overlapped read and write per client so a parked read cannot block a reply |

A surface's clipboard commands (the engine's `cx.copy_bytes` and paste requests) are drained by
the loop, which pairs them with the serial of the surface that asked. The application never sees
a clipboard host.

## Workers

Work that owns its input and is not needed for the current frame runs on a worker thread —
saving a shot, encoding a clipboard image, decoding the portal's screenshot
([ownership](../conventions/ownership.md)). `ui_platform::jobs::Jobs` is that worker: one thread
and an `mpsc`, shared as a clone by everything that has such work.

```rust
let job: Job<Result<Vec<u8>, String>> = jobs.submit(move || cropped.png())?;   // Result: the worker may not start
// later, in the owner's view or on_turn:
if let Some(result) = job.take() { ... }                                        // the slot yields the result once
```

A job is not a thread and not a future: it is a closure plus a slot for one result. Three
things make it safe to use from the loop:

- **A completion wakes the loop.** The worker bumps a counter and calls the backend's `Waker`;
  Wayland's is the same `eventfd` the portal hotkey thread uses, so the wait ends and the next
  turn sees it. The portable side never holds a file descriptor.
- **A completion asks for a frame.** A finished job is not an event any surface listens for, so
  `Host::turn` compares the completion counter with the last value it saw and calls
  `request_frame` on every surface. Without it the result would wait for the next pointer move.
- **The process does not exit while a job runs.** The loop's exit condition checks
  `outstanding()`: closing the last window must not kill a save halfway through its file.

The counter moves on the way out of the job, not after its success: a panicking job counts as
finished, so a loop waiting to exit cannot hang on it. Submission returns the reason when the
worker is not running instead of dropping the work silently.

A request that must answer only after its work is done keeps its reply through
`Reply::deferred`: the platform duplicates the peer's descriptor into an owned reply the loop
sends later. The daemon's clipboard image does exactly this — it answers the `copy` request from
`on_turn`, after the PNG is encoded, so a client's next read sees the selection already set.

Work that must be on screen this frame does not leave the loop; other cores can still help inside
the frame (glyph rasterization).

## Closing a surface

A surface closes itself when its own state says it is finished — the pin canvas with no pins
left, the overlay after its action. The app declares that in `view` with `cx.close_window()`;
the engine records it and the handle reports closed, so the loop drops the surface in the same
turn. The app never calls into the window to close it, and never has to expose a flag for the
host to poll.

## Application API

```rust
let mut host = crabsnap_platform::host()?;          // portable value; backend hidden
let overlay = host.open(overlay_spec(), Overlay::new(...))?;
host.on_hotkey(|id, cx| { ...; Effect::None });    // actions, not platform events
host.on_source(ipc, |ctx, request| { ... });
host.on_turn(|cx, turn| { ...; turn.set_deadline(...) });
host.run()?;
```

Callbacks receive a context and queue their effects (open a surface, close one, quit); they do
not borrow the host mutably. Tests drive the same loop body through `Host::run_until(deadline)`
— they are not a second scheduler. The app's scripted probes used to reach their `Session`
through a downcast of the surface handle; that never worked for a surface opened as `Box<dyn
App>`, and the probes went with the hook, so the handle is only ever the loop's.

## What a surface asks its owner

A surface's app cannot open another surface: it owns no connection, only a build function. When
it produces work that needs one — the overlay's Pin button hands over pixels that belong on a
canvas for that output — the app pushes onto a queue it shares with whoever constructed it
(an `Rc<RefCell<Vec<Pin>>>`), and the application drains it from `on_turn`. That callback gets a
`MessageCtx` beside the loop's own context, so the same door a channel request uses opens the
canvas. The order is deliberate: surfaces step and closed ones are dropped first, so a canvas
opened for a selection replaces the overlay in the same turn.

`SurfaceId` is stable for a surface's whole life, not an index into the live list: closing one
surface must not make a held id name another. `MessageCtx::is_open` answers whether an id still
names something on screen, which is how a second hotkey press finds out that an overlay is
already up.

A key the surface exists for — an overlay's `Escape`, a pin canvas's `Escape` on the pin under
the pointer — is bound when the surface opens, through `OpenOptions::shortcuts`, because the
app's build has no way to register a binding itself. The app answers it in `App::shortcut` and
closes what it names; a canvas that closes its last pin closes its surface through
`cx.close_window()`.

An owner that hands textures to a surface it opened also holds its id afterwards, so it must
find out when that surface went away: `MessageCtx::is_open` is the question, and a canvas whose
surface closed is forgotten before the next texture is placed — otherwise a later pin would be
drawn into a surface that no longer exists.

## Status

`Host`, `SurfaceHandle`, `Backend` and `OpenOptions` exist in `ui-platform`; `WaylandBackend`
implements the backend in `ui-platform-wayland`; `Session` implements `SurfaceHandle` with a
non-blocking `step` beside the blocking `tick`. `Win32Backend` implements the backend on
Windows — surfaces, outputs, languages, the clipboard, the named-pipe channel and global
hotkeys — where only the selection reader is still the trait's default. Every loop in the application runs on it: the
toolbar's path on `Host::run_until`, the daemon's overlay and pin canvases (a hotkey press
captures on the daemon's own connection and opens the overlay as a surface of the same loop),
and the standalone `shot` and `pin` commands ([command line](../apps/cli.md)), which run a
surface of their own until it closes.

The loop's control flow is covered by `ui-platform/tests/host_loop.rs` against a fake backend:
one step per turn per surface, copies reaching the backend, deadline aggregation including the
run's own deadline, and hotkeys reaching the handler once each, with the same context a channel
request gets. Nothing there needs a compositor. The job service has its own test in
`jobs.rs` (work off the loop thread, one wake, one result) and a forced-frame test in
`host_loop.rs`; the apps submit saves, clipboard images and the output save through it.

## Migration

1. ~~Non-blocking step: split `Driver::step` into a decision that never waits and keep the
   blocking behaviour only in `Session::tick`.~~ Done: `Driver::decide` and `Session::step`.
2. ~~`Host` + `SurfaceHandle` + `Backend` traits; the Wayland backend serves existing
   `Session`s.~~ Done.
3. ~~Move the application to `Host::run`; delete its loops.~~ Done: the toolbar path, the
   daemon's overlay and pin canvases, and the standalone `shot`/`pin` commands all run on the
   host.
4. ~~Clipboard and hotkeys into the backend's services; the application stops draining them.~~
   Copies and hotkeys reach the loop through the backend, and a handler that owns no surface
   publishes through `MessageCtx::copy`: the daemon holds a capture on the clipboard after the
   command that asked has exited (`shot output --clipboard`). Paste stays with the focused
   surface's own device, because an offer follows keyboard focus.
5. ~~Outputs registry and registry hotplug.~~ Done: the link keeps a registry queue of its own,
   records name, description, position, logical size (scale and rotation applied) and refresh,
   follows `global`/`global_remove`, and the host refreshes the list before each turn so a status
   answer or a pin placement never uses a stale one.
6. ~~Capture as a registered pending queue, replacing the blocking pump.~~ Done for the two
   protocol rungs: `Capture::begin` issues the requests and hands back a `Pending` that the
   daemon polls from `on_turn`, one non-blocking pass per turn, so the loop never waits on the
   compositor and the overlay opens as soon as the frame lands. The portal rung answers over
   D-Bus and has no pollable form, so it still grabs while the caller waits — the one rung
   where a hotkey press costs a pause. Verified with a startup probe in a real session (the
   frame arrived across turns and was saved); the probe was removed afterwards.
7. ~~IPC and idle timers as application-registered sources.~~ Done: the backend's local
   channel joins the wait set, `Host::listen` hands messages to the application's handler, and
   `on_turn` plus `set_deadline` give the loop an application deadline (the daemon's idle exit).

## Traps

| Change | Symptom |
| --- | --- |
| Read the connection from a surface step | Two readers split one socket; whichever wakes loses the other's events |
| Wait inside a surface step | One surface decides when the process wakes; other surfaces starve |
| Poll without a maximum timeout | A silent compositor hangs the process with no frame callback |
| Draw without checking `can_draw` | Frames are committed outside the compositor's frame window and are discarded |
| Keep the old per-surface wait "for tests" in production paths | The two loops disagree about who owns the connection |

## Corrections

**A surface opened during a turn waited for unrelated input.** `pump` turned once and then
blocked, and the turn that opened a surface had already stepped the surfaces before its callback
ran. The new surface's setup (configure roundtrip, keymap) consumed the events that would have
woken the loop, so its first frame could arrive seconds later, on the next keystroke — 7.4 s
in a nested Hyprland session, release build, with the pin canvas on screen and never drawn. The loop now turns
again instead of waiting once a turn has left a surface with no frame, and
`ui-platform/tests/host_loop.rs` asserts the new surface's first frame happens in a turn no wait
preceded.

**A surface id was an index into the live list.** Closing any surface shifted every id above
it, so a pin canvas that had been open for hours could be handed a texture meant for a surface
that had just closed — or lose one meant for it. The daemon only began to show this once the
overlay and the canvases shared a loop. Ids are now a counter that is never reused, and
`ui-platform/tests/host_loop.rs` holds a surface's id across another's close.

**A turn callback could not open a surface.** The application's per-turn work saw the loop's
deadline and quit flag but not the door a message handler uses, so a surface's request (the
overlay's pin) had to wait for a channel message that might never come. `on_turn` now receives
the same `MessageCtx`, and the overlay's pixels reach a canvas on the next turn.

**A copy from a program with no focused surface had no serial.** Wayland only takes a
selection from a client that just received input, and the selection lives in the copying
process: a `shot output --clipboard` that copied locally would either lose the serial or take
the clipboard with it when the command exits. The first reading — "copy from the daemon, which
has surfaces" — was wrong too, because a hotkey press arrives through the portal and no surface
of the daemon has seen a pointer or key event. The root fix is the compositor's privileged
clipboard channel (`ext_data_control_v1`, `zwlr_data_control_v1` before it), which sets a
selection with no input at all; the backend binds it when the compositor offers it and falls
back to the seat device otherwise. `MessageCtx::copy` therefore takes no serial, and the daemon
serves the payload from its own queue for as long as the selection is its own. What catches it
now: `shot output --clipboard` followed by another client reading the clipboard, which is the
only thing the flag is for.

## See also

- [Window Backend](./window.md): surfaces, layer roles and what the platform owns.
- [Frame Pipeline](./frame-pipeline.md): what makes a frame due.
- [Pointer Model](./pointer-model.md): where per-surface input state comes from.
