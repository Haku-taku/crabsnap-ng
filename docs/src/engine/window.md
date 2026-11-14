# Window Backend

A window is a surface, a rotating set of buffers, and a configure loop. The
compositor owns the size, the buffer format and the frame rhythm; the
application owns what to draw once those are known. Code that treats the
requested size as the real one, or presents whenever it likes, works on one
compositor and fails on the next.

## Size comes from configure

A tiling compositor ignores the requested size and answers with its own.
Hyprland gave `configure 1272x1397` for a `420x240` request, and setting min
and max size did not change that. This is correct behaviour, not a bug:
the application adapts.

- The first configure must be awaited before allocating buffers;
  `configured` gates the loop.
- Every configure is acknowledged, then buffers are reallocated, layout
  rebuilt and the frame redrawn.
- A layer-shell surface is the escape hatch for a fixed-size overlay, not a
  way to force a toplevel size.

## Two buffers, two pools

`wl_shm` double buffering needs two buffers that do not share memory:
`create_pool` plus `create_buffer(offset)` looks like slicing one pool, but
offset overlap is not prevented. Give each slot its own pool and rotate
them after the compositor releases the front one.

The GPU path does the equivalent with a target pool and waits on the
release point before rewriting a buffer; see
[Rendering Backends](../rendering-backends.md).

## Frame pacing

`can_draw()` reports whether the compositor's next frame window is open — on
Wayland because its frame callback arrived, on Windows because the surface's
render thread finished the frame it was handed (its `Present(1)` returns at the
compositor's blank) — and `wait_for_frame()` blocks until it is, with a timeout
so a compositor that never replies cannot hang the application. One commit
produces at most one callback, so a loop that draws without waiting builds frames
nobody displays.

This is also why the engine asks the window instead of guessing a refresh
rate: the frame window is opened by the compositor, never computed by the
client.

## Fractional scale

Three pieces are needed together:

| Piece | Role |
| --- | --- |
| `wp_viewporter` | Maps the scaled buffer back onto the logical surface size |
| `wp_fractional_scale_v1` | Reports `preferred_scale` |
| `set_buffer_scale(1)` | Must stay 1 under fractional scaling |

`preferred_scale` is in 1/120 units: 192 means 1.60×. It arrives after the
window exists, so a resize must be handled at runtime, not read once at
construction. Buffer size is logical size times scale; layout stays in
logical units and the renderer scales primitives.

## Surfaces: windows and layers

A host asks the engine for a surface with one description, `ui_platform::window::SurfaceSpec`:

```rust
SurfaceSpec::window(app_id, title, size)
SurfaceSpec::layer(app_id, namespace, LayerSpec { .. })
```

`SurfaceRole::Window` is a surface the platform's window manager places, decorates and stacks.
`SurfaceRole::Layer` is one the engine places: no decoration, no window-manager layout, and a
stack level of its own. The vocabulary is `zwlr_layer_shell_v1`'s, which is also the one
`gtk4-layer-shell` gives GTK apps: a level, four anchor booleans, a size, a margin, an exclusive
zone and a keyboard mode.

Only Wayland has the role as a protocol; a platform without layer surfaces takes the same spec
and approximates it — an undecorated window placed from `anchor`, `size` and `margin`, kept at
the level the spec names, with `keyboard` folded into activation rules (`WS_EX_TOPMOST`,
`WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE` and `WM_MOUSEACTIVATE` on Windows; `NSWindow.level` on
macOS; the equivalent of winit's `WindowLevel` on X11). `exclusive` has no counterpart
anywhere, and a keyboard mode that platform cannot express degrades to the closest one it can.
That mapping is written on the types themselves; the engine never branches on it. Where a
platform needs more than one window to honour the role — a painted surface and a shaped one on
Windows — the extra window is the backend's own business and no app sees it.

The engine's own place in this is narrow: it hands the spec to the backend and receives a
`PresentTarget`. Whether a surface is a layer is visible to the frame contract as
`SurfaceKind::LayerSurface`, which is what decides that the surface owns an input region
([Pointer model](./pointer-model.md#the-surfaces-input-shape)).

A window may also declare a floor: `SurfaceSpec::window(..).with_min_size((width, height))`
names the smallest client area, in logical units, a user may drag the window to. It is a
client size because that is what layout sees; a backend adds whatever its own sizing rules
count (the Win32 window converts it through `AdjustWindowRectExForDpi`). A layer ignores it,
since the spec's anchors, size and margin place a layer and no user resizes one. The floor
reaches the user, not the engine: a program that asks for a smaller size still gets it, and
the floor never clips a frame or constrains layout — it only keeps a window from being
dragged narrower than its content can lay out. Wayland forwards it to
`xdg_toplevel::set_min_size`; the Win32 window answers `WM_GETMINMAXINFO` so the shell's
sizing loop stops there, and applies the same floor in the border drag it performs itself,
which never reaches that loop. `resize_probe` checks both routes: the shell's answer equals
the declared window rectangle, and an inward drag of 240 by 160 pixels leaves the client area
at the size it started from.

## Layer placement and the keyboard

A layer surface says where it goes through four booleans, a size, and a margin. With
`Anchor { top: true, left: true, .. }` the margin *is* the position: `margin: Insets { left:
x, top: y, .. }` puts the surface exactly at `(x, y)`, which is how a floating image lands on
the rectangle it was cut from. Margins are signed: a negative one places the surface outside
the output (verified with `hyprctl layers` — a left/top margin of `-20` put the surface at
`-20 -20`), which is how a surface keeps a shadow's padding off-screen while its content sits
on the edge.

`LayerSpec.exclusive` replaces the protocol's signed exclusive zone, where `-1` means "not part
of the layout at all". That is what a floating image or a transient overlay wants; a panel
reserves space with `Reserve(px)`.

`LayerSpec.keyboard` is a mode, not a flag:

| Mode | Meaning |
| --- | --- |
| `None` | The surface never hears the keyboard |
| `OnDemand` | It takes the keyboard when the user clicks it |
| `Exclusive` | It holds the keyboard while mapped |
| `FollowPointer` | It holds the keyboard only while the pointer is on what it consumes, or a gesture it owns is live |

A capture overlay wants `Exclusive` — its Escape must work at once. A surface that covers the
output but claims only patches of it wants `FollowPointer`: between its patches the pointer and
the keyboard both belong to the desktop, and it should not have to be dismissed to give typing
back. `OnDemand` is a one-time grant with no way to ask for less afterwards, which is why it is
not enough for that shape; see [Pointer Model](./pointer-model.md#keyboard-is-not-the-input-region).

Placement is not fixed at open time. `set_layer_margin` moves a surface against its anchors, and
`set_layer_size` asks for a new one; both are surface state, so they ride a commit. A size is a
request: the compositor answers with a configure, and the frames after it are laid out and
buffered at the new size. The request is remembered as well as the configured size, because a
call repeated every frame before the configure arrives would otherwise commit the same state
each time.

## Keyboard

The keymap arrives as a file descriptor, not a string. `Keymap::new_from_fd`
parses it with the trailing NUL excluded; reading it as UTF-8 and trimming
or keeping the NUL fails both ways (parse error, or `NulError` from
`CString`). The fd is the protocol's answer to the trailing byte problem.

Event translation is evdev to keysym through `xkbcommon`; repeat and
modifiers come from the same state.

## Wheel

A notch arrives as two events in one `wl_pointer.frame`: `axis_value120`, which
counts in 1/120 of a detent, and the `axis` event the protocol couples it to,
which restates the same motion as a length in surface coordinates. Their order
is the compositor's, so `Wheel` holds both per axis until the frame ends and
then takes the detent: one notch scrolls once whatever order it arrived in. A
detent moves 40 px (Chromium's `kPixelsPerLineStep`), and a high-resolution
wheel's fraction of one moves its fraction of that, not a whole line. A
two-finger scroll has no detents and sends `axis` alone; that is a length
already and passes through unchanged.

The engine therefore never depends on the compositor's own pixels-per-line,
which differ between compositors and mice. `axis_stop` is ignored: the engine's
wheel lock keys on the pointer staying put, not on a gesture end.

## Application protocols share the connection

The engine lends the window's connection and registry to platform-bound application code
through `WaylandLink` (`crates/ui-platform-wayland/src/link.rs`). The application binds the
protocols the engine does not speak on a queue of its own, with its own `Dispatch` state, so
its events never reach `WindowState` and no portable crate ever names Wayland:

```rust
let link = session.link();
let mut queue = link.new_queue::<CaptureState>();
let capture: ZwlrScreencopyManagerV1 = link.globals().bind(&queue.handle(), 3..=3, ())?;
// after each `session.tick()`:
queue.dispatch_pending(&mut capture_state)?;
```

A second connection compiles but is wrong: a queue waits on the fd of the connection it came
from, so events on another connection never wake this loop, and the compositor orders
requests only within one connection. One connection also means one set of serials — a
keyboard grab, a popup's positioner, a dma-buf transfer — which is what interactive capture
needs.

Three rules hold the seam:

- The window reads the socket; the application only dispatches. `dispatch_pending` delivers
  what the last window turn read, and a request on the application's queue is flushed by the
  next frame.
- The registry is the one enumerated at connect time. A global that appears later is not
  seen; code that needs hotplug binds `wl_registry` itself on its own queue.
- Do not call `blocking_dispatch` or `roundtrip` from inside the frame loop: it waits on the
  shared connection and holds the window's own events until it returns.

A tool that must see the screen before it shows anything opens the connection alone:

```rust
let link = WaylandLink::connect()?;   // no surface, no window
```

The registry snapshot is taken here and stays fixed; surfaces opened from this link later
bind from it. Opening a window first would put the tool inside its own capture.

### More than one surface

`WaylandLink::surface` opens a surface on the existing connection from a `SurfaceSpec`, whose
role decides between a toplevel and a layer surface; `Session::open_on` does the same for a
whole session. Each surface allocates its
own `EventQueue`, binds its own proxies and dispatches only its own events, so two windows on
one connection cannot deliver each other's input.

One limit follows from that: the *first* configure of a new surface waits in
`EventQueue::roundtrip` for that surface alone, so a surface opened while another has events
pending leaves those pending until the other dispatches. Dispatch every surface each turn.

Cross-thread code opens its own connection (`Connection::connect_to_env`) instead, because an
`EventQueue` is not `Send`; the legacy project's debug overlay does exactly that.

Verified by `crates/ui-platform-wayland/examples/link_app_protocol.rs` in a live session: it
bound a second `wl_output` on a queue of its own and received `name DP-6`, the output's
description, `scale 1` and `done`, while the window dispatched its own queue throughout.

Verified by `crates/ui-platform-wayland/examples/two_windows.rs` in a live session: a 320×180
toplevel opened directly and a 420×180 one opened from its link both reached their requested
logical sizes and stayed mapped, each through its own queue.

## Traps

| Change | Symptom |
| --- | --- |
| Allocate buffers before the first configure | Allocation against a guessed size; the first frame is the wrong size |
| Treat a tiling compositor's resize as fatal | The window closes after one frame |
| Share one shm pool between both buffers | Torn frames: the compositor shows memory being rewritten |
| Present without waiting for the frame callback | Frames are built and never shown; CPU burns for nothing |
| Read `preferred_scale` once at startup | Wrong buffer size after the user changes scale |
| Trim the keymap's trailing NUL before parsing | `Failed to parse input xkb string` |
| Use `set_buffer_scale(scale)` with fractional scaling | Double scaling; the surface is larger than its viewport |
| Let two windows share one thread's EGL state | The last context created or released is current; the other window's GL calls run against it and fail |
| Delete GL objects in a window's `Drop` without making its context current | Ids are per context: the delete names the *other* window's object, which vanishes silently while every buffer, size and modifier still checks out |
| Give a floating layer `Exclusive` keyboard interactivity | It swallows every keystroke on the desktop for as long as it is mapped |

## Corrections

**Dropping one window deleted another window's GL objects.** The pin's ids collide with the
overlay's, because both are the first objects a fresh context names. `GpuWindow::drop` deleted
them while the *pin's* context was current — the pin's targets, programs and buffers were gone
the moment the overlay closed, and every later frame drew into deleted ids. GL raised errors
nobody read, the dma-buf kept its cleared transparent content, the compositor composited that
faithfully, and the pin was invisible while its scene, layout, buffer size and modifier were
all correct. `GpuWindow::drop`, `finish` and `dump_ppm` now make their own context current
first, and `EglContext::drop` releases the thread's binding only when the binding is its own.
Caught by sampling the pin's padding in a nested session: with the overlay still open the
probe's block was on screen, and the moment it closed the same pixels were the bare desktop.

**The second window on a connection could not upload its glyph atlas.** An EGL current
context is thread state, not a property of the context object: the capture overlay created its
context, then the pin window created its own, then the overlay was dropped and its `Drop`
released the current context entirely — and the pin's first frame ran GL against no context at
all (`GPU: GL: Unable to create Texture object`). `EglContext::make_current` now exists and
`GpuWindow::sync_atlas` / `present` call it first, so every piece of GL work states which
context it belongs to. Caught at the time by a scripted probe in `apps/crabsnap` (since
removed), which drove a drag, a save, a resize and a pin click and printed what each surface
produced.

**A layer surface could not be placed anywhere but the output's edges.** The layer description
had anchors and a size but no margin, so a pin could only be centred. `margin` now goes to
`zwlr_layer_surface_v1.set_margin`, and `hyprctl layers` reports the pin at `768 188 1224 676`
— the exact selection rectangle.

**A layer surface reported itself as a window to the frame contract.** `PresentTarget::kind`
answered `SurfaceKind::Window` from every `WaylandWindow`, layers included; nothing consumed it
at the time, so the input-region rule could not have been keyed on it. It now answers from the
surface's role. No test pins this: the only callers are the presenter tests, and an assertion
that one constructor reports `LayerSurface` would restate the constructor rather than test a
rule. What would catch a regression is a caller that branches on the kind — the input region
does not, because the backend checks its own layer state.

**One notch scrolled twice on Hyprland.** The pairing assumed `axis_value120`
came first and dropped the `axis` after it; Hyprland sends the restatement
first, so a 120 detent (40 px) and its 15 px `axis` were both counted and one
notch moved 55 px — read off a pin's wheel log as `value 15` then `value 40` per
notch, five zoom steps instead of four, with a half-step of change left over
that made the next notch down move one step less, so the wheel could not return
to 100 %. Scroll is decided at `wl_pointer.frame` now, the unit the protocol
groups a notch into: a frame's events are held per axis and the detent wins
where both are present. Caught by `one_detent_scrolls_once_in_either_order` in
`crates/ui-platform-wayland/src/wheel.rs`, red with the restatement preferred
(15 px instead of 40).

**A wheel notch scrolled by whatever the compositor put in `axis`.** The backend
passed the raw value through as pixels, so the distance depended on the
compositor and on the mouse, and `axis_value120` — the only event that defines
what a notch is — was not read at all. The notch is now counted from
`axis_value120` at 40 px per detent, and the coupled `axis` event that follows
it is dropped; taking both would have scrolled every notch twice. Caught by
`one_detent_scrolls_once` in `crates/ui-platform-wayland/src/wheel.rs`, which
cannot run without a compositor only because the state machine was pulled out of
the dispatch impl to make it testable.

**The window closed after one frame and the loop was blamed.** The loop
skipped redraws on an idle branch, which looked like the cause. Printing the
event sequence showed a configure for a different size, a buffer allocation
that failed, and a loop that treated the error as fatal. Print the event stream
before changing the loop.

**Both buffers pointed at one allocation.** The first presenter created one
pool and called `create_buffer(offset = 0)` for each frame, so the compositor
could still be scanning memory that was being overwritten. Two slots now
own two pools; the release event rotates them.

**A layout field existed but nothing read it.** A card was 94 px wide in a
real window and correct offscreen, because `cross_align` was written by the
container and never consumed by placement. The field had documentation and a
setter; the wiring line was missing. This is the same failure the engine's rule
names: landed is not wired.

## Status

Implemented: toplevel and layer-shell surfaces, shm and dma-buf presentation,
explicit sync when the compositor offers it, configure-driven resize,
fractional scaling, keyboard and IME through Wayland protocols, colour
management where available, and lending the connection to application
protocols through `WaylandLink`. On Windows, a decorated window and an
approximated layer are behind the same spec: a topmost popup that takes clicks
only where its frame consumes them, gives up the monitor edge an auto-hidden
taskbar hides behind unless the spec asks to cover it, and hands the keyboard
back when a pointer-following surface is left; see
[Rendering Backends](../rendering-backends.md).

Not implemented: macOS and X11 windows; multi-seat input; tablet and touch
protocols beyond the pointer path.

## See also

- [Rendering Backends](../rendering-backends.md): buffer pools and explicit
  sync.
- [Colour and HDR](./colour.md): the output description this backend reads.
- [Frame Pipeline](./frame-pipeline.md): what runs after the callback arrives.
