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
SurfaceSpec::dialog(app_id, title, size)   // opened through its parent's surface
SurfaceSpec::layer(app_id, namespace, LayerSpec { .. })
```

`SurfaceRole::Window` is a surface the platform's window manager places, decorates and stacks.
`SurfaceRole::Layer` is one the engine places: no decoration, no window-manager layout, and a
stack level of its own. The vocabulary is `zwlr_layer_shell_v1`'s, which is also the one
`gtk4-layer-shell` gives GTK apps: a level, four anchor booleans, a size, a margin, an exclusive
zone and a keyboard mode. `SurfaceRole::Dialog` is the third: a secondary window opened through
the surface it belongs to, stacked above it and closed with it. Its modality, lifecycle and the
host's input filter are [Dialog Surfaces](./dialogs.md)' contract. A popup is not a role: it is
a `PopupSpec` a surface hands out through `SurfaceHandle::open_popup` when a popover declares
`Presentation::Platform` ([below](#presentation-surfaces)). A panel declared with
`Presentation::Window` reuses the dialog role as a modeless window of the surface it belongs to:
same ownership and stacking, no `EnableWindow` and no modality hint
([Presentation surfaces](#presentation-surfaces)).

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

## Presentation surfaces {#presentation-surfaces}

> **Status:** Implemented on Wayland and Win32; run on Wayland. The engine/presenter
> split is in place: `Host` owns an `Engine` per surface and a backend never
> holds an app. A platform panel is opened through `SurfaceHandle::open_popup`,
> shown by `AppLoop::present_panel`, and reported back as dismissed through
> `AppLoop::dismiss_panel` when its surface goes away. Both backends route a
> popup's input back into the shared tree and present the panel in a window of
> its own: Wayland through `xdg_popup` with a grab and `xdg_positioner`, Win32
> through an owned `WS_POPUP` with `SetCapture`. Wayland's CPU presenter has no
> scene target to hand a panel to, so it refuses and the panel paints in its
> parent; Win32 has no CPU presenter at all.
>
> One Wayland panel has run: the gallery's menu-flyout page declares its Fluent
> flyout with `Presentation::Platform` when `GALLERY_HOST=1`, and a nested
> Hyprland session opened it as an `xdg_popup` — trace evidence is a positioner
> sized 178×216 and anchored to the trigger's rect, `xdg_surface.get_popup`, and
> then an attach of 178×216 on the popup's own `wl_surface`. Still unrun: a panel
> ended by the platform's grab or `PopupDone`, a `reconfigure`, and a panel nested
> in another panel. `picker_demo` uses the Win32 popup path for its menus and the
> modeless-window path for its Properties and failure windows, which is the visual
> check for both; the Wayland counterparts stay compile-verified only.

The engine (`AppLoop` today) owns the application, its declaration tree, the
clock, the focus and the hit index. A presenter owns one native surface, its
buffers and its input queue. `Host` drives both: it steps the engine once a
turn, opens and closes the surfaces the panels ask for, and each surface shows
the part of the frame it owns in its own coordinates.

The split is what makes a popup an engine decision instead of an application
one. A popover's subtree is laid out in the parent tree; the engine chooses
between painting it into the parent's surface and presenting it in a surface of
its own. The application declares a panel and never names a surface.

Nothing in the application's API changes: `Host::open` still opens one surface
for one app. What changes is behind it — the engine exists once per group and
presents through one or more `Presenter`s:

```rust
pub trait Presenter {
    /// Read what arrived and translate it into events local to this surface.
    fn pump(&mut self) -> Vec<Event>;
    /// Show `scope` of `frame`: the whole tree for a root, one subtree for a panel.
    fn present(&mut self, scope: DrawScope, frame: &Frame) -> Result<bool, String>;
    /// Move or resize an open panel.
    fn reconfigure(&mut self, spec: PopupSpec) -> Result<(), String>;
    fn next_deadline(&self) -> Option<Instant>;
    fn is_closed(&self) -> bool;
}
```

The seam that exists today is `SurfaceHandle`, which is the same split without a
single `present` call: `step(&mut Engine)` and `take_copies(&mut Engine)` are
what a surface implements, `open_popup` and `reconfigure` are how a parent
surface hands out panels, and a panel's surface presents through
`AppLoop::present_panel` from its own `step`. The engine paints the root and its
panels once per build into separate lists; `pump` and a per-scope `present`
fold into `step` until a presenter outlives its subtree.

`Presentation::Parent` is the default: the panel is painted into its parent's
surface. `Presentation::Platform` and `Presentation::Window` ask for a surface
of their own:

```rust
OverlayPlacement {
    presentation: Presentation::Platform, // a popup, placed by the engine
    ..
}
OverlayPlacement {
    presentation: Presentation::Window,   // with `Decl::window_title(..)` on the panel
    ..
}
```

A `Platform` panel is a popup: the platform holds it at the rect the engine
placed against the output, and a press outside dismisses it. A `Window` panel is
a modeless window owned by the surface the panel is declared in — the dialog
role, without modality. The platform decides a modeless window's place, so the
panel's rect supplies the client size and `Decl::window_title` supplies its
name; the engine still needs the rect's origin to map the surface's input back
into the tree, which is why a window panel is laid out exactly as a popup is.

The engine reconciles the request against the presenters it has open. A new
declaration opens one, a vanished declaration closes it, and a moved anchor
repositions it. A popup takes the engine's rect. A window takes only its size: a
backend pins the window to that size — Win32 a dialog frame without a sizing
border, Wayland the toplevel's minimum and maximum both — so content that grows
or shrinks resizes the window, and the user is offered no resize that would
leave a blank strip beside the panel. Closing is immediate: a platform panel has
no exit animation until a presenter can outlive its subtree, which the binding
table does not do yet.

### Sizing and placement

The detached subtree is measured in the parent tree before its surface exists,
so the popup's size is that subtree's layout rect, with whatever shadow paints
outside the rect included. No second layout pass follows: the surface takes the
content size.

A presenter is told which part of the frame it owns by which list it is handed.
The root's list paints the tree but skips every subtree bound to a panel; a
panel's list is that subtree rebased on the panel's top-left
(`AppLoop::present_panel`).
Every coordinate in `PopupSpec` is the parent
presentation's, so a nested panel's anchor is its root-tree rect minus the
outer panel's subtree origin.

`PopupSpec` is what the engine asks of the platform:

```rust
pub struct PopupSpec {
    /// The laid-out panel: the surface's size, and the tree position it stands for.
    ///
    /// Logical units, from the detached subtree's layout rect plus its outset.
    pub rect: Rect,
    /// The anchor rectangle, in the parent presentation's coordinates.
    pub anchor: Rect,
    /// The preferred unfolding side and alignment.
    pub side: Side,
    pub align: OverlayAlign,
    pub offset: f32,
    pub constraint: Constraint,
}
```

`rect` carries both facts a platform needs and cannot derive: the size is the
content's, and the origin is what the surface's own coordinates mean in the
tree. A platform that routes the popup's input back into the tree shifts every
position by it, which is why the origin travels with the size instead of the
size travelling alone.

A platform popup is never clamped into a viewport. The engine knows the parent
surface, not the output: a window may straddle two monitors, so only the
platform knows what "on screen" means. The platform applies the constraint
adjustment; flip-and-clamp stays the rule for a panel painted in the surface it
belongs to.

### The binding table {#binding-table}

The host reconciles after the frame that laid the panels out. A declaration
with no binding asks its parent's surface for one (`SurfaceHandle::open_popup`,
which receives the panel's node so the new surface can present that subtree); a
bound panel whose request changed is reconfigured, and a platform that cannot
move a popup closes and reopens it instead; a panel that left the tree drops
its binding. A platform that refuses loses nothing: the engine is told to paint
the panel in its parent from the next frame.

The frame that first declares a panel cannot know the host's answer yet, so the
panel is held back for that frame — painted in neither place — and the build
asks for the frame that follows. Painting it in the window for one frame and
moving it to its own surface in the next is the flash that rule exists to
prevent. A panel whose surface the platform took back stays bound-but-gone
until the app undeclares it: reopening it while the declaration still stands
would ignore the dismissal.

`AppLoop::present_panel` hands a panel's list to its surface. The list is
rebased on the panel's top-left and carries no damage diff: that surface shows
nothing else, so every frame it receives is the whole panel. The parent's own
damage is diffed before the panels paint, so panel ink neither claims nor loses
damage in the parent.

### Constraint adjustment

`Constraint` is `xdg_positioner`'s set, because it names the four answers a
window server can give to an anchor that does not fit.

| Field | Behaviour |
| --- | --- |
| none | Leave the request alone; the output edge cuts the panel |
| `flip_x`, `flip_y` | Mirror to the anchor's other side |
| `slide_x`, `slide_y` | Move along the axis until it fits, covering part of the anchor |
| `resize_x`, `resize_y` | Ask for a smaller surface |

The default is `flip_x | flip_y | slide_x | slide_y`: flip where that keeps the
anchor visible, slide where it does not. `resize` stays off, because the size is
the subtree's layout result and a platform that changed it would cut content the
engine laid out. A control overrides the default through its placement.

The worked case: an anchor at the bottom of the output, a 300 px panel, and
120 px below the anchor. `flip_y` puts the panel above the anchor, `slide_y`
moves it up 180 px and covers part of the anchor, and no adjustment leaves
180 px off screen.

### Wayland

`xdg_positioner` carries `size`, `anchor_rect`, `anchor`, `gravity`, `offset`
and `constraint_adjustment`. `xdg_popup` takes the parent `xdg_surface`, and
`grab` takes the seat and the serial of the input that opened it. The serial is
the connection's, not a window's: the compositor accepts the last event read on
the connection, and the click that opened a menu usually is it.

The positioner's `anchor` and `gravity` are not `side` and `align`. The anchor is
a corner or edge of the anchor rectangle; gravity is the direction the popup
grows from it. "Below, starting at the edge" is therefore anchor `bottom_left`
with gravity `bottom_right`, and each of the twelve combinations is fixed by the
side and alignment pair. The offset is the gap alone — `(0, gap)` below,
`(0, -gap)` above, `(gap, 0)` to the right, `(-gap, 0)` to the left.

A positioner fixes a popup's size when it is mapped; `reposition` (protocol v3)
moves it and cannot resize it. A panel whose subtree changes size is therefore
closed and reopened against a new positioner, in the same frame, before a frame
at the old size is drawn; a popup below v3 is reopened for a move too. The
reopen reuses the connection's last serial; a compositor that refuses that grab
leaves the panel without one, and the engine's own outside-press test still
dismisses it, because a press outside then arrives on the parent surface.

A popup declared inside another popup uses that popup's surface as its parent,
which keeps the topmost panel the one holding the grab. Its request is rebased
on the outer panel first: a compositor places a nested popup against the surface
it hangs from, not against the window under it.

`popup_done` — a click outside, Escape, or the parent going away — closes the
popup's window. The host sees the closed handle on its next step and reports it
through `AppLoop::dismiss_panel`, which is the same path a press outside the
panel takes; it does not travel as an `Event`. The panel stays bound but unpainted
until the app undeclares it, so the dismissal is not ignored by a reopen.

### Windows

A popup is an owned window: `WS_POPUP` with the parent `HWND` as owner, plus
`WS_EX_TOOLWINDOW` and `WS_EX_NOACTIVATE`. An owned window is destroyed with its
owner and stays above it, but it does not follow it. Each window carries the list
of popups it owns; its `WM_MOVE` and `WM_SIZE` place them again, because the
engine cannot: it knows the anchor in the parent's coordinates, not on the screen.
A panel that opens a panel adds to its own list, so the chain keeps following the
window at the bottom of it.

A grab is `SetCapture`, taken when the popup opens and kept past the press's
release, so every mouse message — including one outside the panel — arrives at
the popup. That outside press is dispatched into the tree, where the engine's own
outside-press test dismisses the panel; `WM_CAPTURECHANGED` covers the cases the
grab did not survive, such as another window taking the capture, and closes the
panel then. A capture taken by a popup this one owns is the nested case and does
not close it.

Nothing activates: the window is `WS_EX_NOACTIVATE` and answers `MA_NOACTIVATE`,
so the keyboard stays with the owner and the engine routes keys into the panel's
focus scope. That is why a panel needs no input method of its own.

Windows has no positioner, so the backend names the output instead: the monitor's
work area in the surface's client coordinates goes to the engine
(`AppLoop::set_output_bounds`), which places every platform panel with the same
rule a positioner would apply — flip first, slide after, never resize. The
window then opens at `PopupSpec::rect` unchanged. Moving it after layout would
leave the tree's rectangles where the user is not looking, and the grab routes
every press outside the panel into that tree.

### Input

A popup's pointer events arrive in its own surface's coordinates. The presenter
that owns the popup adds the panel's tree origin and dispatches them into the
shared engine, so hit testing, focus scope and dismissal keep one tree and one
hit index. Keyboard events are not shifted; they belong to the same tree already.
No presenter needs to know where it is on the screen.

A press outside the panel reaches the tree because the grab delivers it on the
popup's window with coordinates outside the panel, not because the root surface
saw it. That is why the panel's own outside-press test needs nothing extra.

Platform focus stays the backend's. A Wayland popup with a grab hears the
keyboard itself; a Win32 menu activates and takes focus. Both hand the events
to the same engine, whose focus scope already names the topmost panel, so the
application never sees a second focus model.

Not implemented: hover across surfaces. A leave on one popup and an enter on
another would clear and rebuild the tree's hover in the wrong order, so joining
them up needs the host to own the pointer's presentation; today each surface
reports what it saw.

Not implemented: text input in a panel. `text-input` on Wayland and the IME
context on Win32 are per-window, and neither backend creates one for a popup
surface yet, so text entry works in the root window only.

### Implementation sites

| Path | Responsibility |
| --- | --- |
| `ui-core/src/layout.rs` | `OverlayPlacement::presentation`; a platform panel is measured as content and not clamped |
| `ui-platform/src/window.rs` | `PopupSpec`, `Constraint` |
| `ui-platform/src/view.rs` | the root's list skips bound panels; each panel painted rebased into its own list; `present_panel`, `frame_number`, `dismiss_panel`, `frame_due` |
| `ui-platform/src/host.rs` | one engine per group, the binding table (open, reconfigure, dismiss, fall back), nested rebase |
| `ui-platform-wayland/src/window.rs` | `create_popup`, `reposition_popup`: `xdg_popup` and `xdg_positioner`, the grab serial, the side/align-to-anchor/gravity mapping |
| `ui-platform-wayland/src/session.rs` | presents one surface; holds no `AppLoop`; `PopupPresenter` shows a panel, shifts its input, repositions it, and refuses on the CPU presenter |
| `ui-platform-win32/src/window.rs` | `new_popup` and `move_popup`: the owned window, `SetCapture`, the placement carried on the owner's move, the monitor's work area the engine places panels against |
| `ui-platform-win32/src/session.rs` | presents one surface; holds no `AppLoop`; `PopupPresenter` shows a panel, shifts its input, and moves it |

### Verification

`place_popup` is pinned by six tests in `crates/ui-platform/src/window.rs`:
flip before slide, slide when flipping is refused, a panel larger than the
bounds pinned to its origin, `NONE` leaving the request alone, an aligned panel
flipping its alignment at the right edge, and a centered panel sliding because
it has no other alignment. Each was watched failing with its branch disabled.
The rule is now `ui_core::layout::place_anchored`; `place_popup` maps a
`Constraint` onto it, and the layout runs it for a platform panel with the
output the host named. `a_platform_panel_unfolds_inside_the_output` pins that
the layout applies it.

The split is pinned by five tests. `a_platform_panel_keeps_its_preferred_position_and_is_recorded`
in `crates/ui-core/src/layout_engine/tests.rs` drives a panel past a small
viewport and reads the recorded request. `a_platform_panel_paints_into_its_own_list_and_not_the_parent`
in `crates/ui-platform/src/view/tests.rs` checks the rebased list, the
held-back first frame and the parent fallback. In
`crates/ui-platform/tests/host_loop.rs`, `a_panel_gets_a_surface_of_its_own_or_falls_back_to_its_parent`,
`a_closed_panel_is_reported_as_dismissed_and_not_reopened` and
`a_panel_inside_a_panel_is_placed_against_the_outer_surface` check the binding
end to end against a fake surface: the panel goes to its own surface and leaves
the parent's list, a surface closing itself reaches the app as a dismissal and
is not opened again, and a nested panel's request is measured from the outer
surface. Each was watched failing with its branch disabled — the nested one with
the rebase removed, whose request came through at `(0, 460)` instead of
`(0, 60)`.

The Wayland backend has no test: it needs a compositor. It is verified only by
compiling for `x86_64-unknown-linux-gnu` with and without `gpu`, which catches
the API misuse — a request sent with the wrong argument, a protocol object at
the wrong version — but not a wrong positioner or an input shift that is off.

Two checks are planned with a real backend:

- A gallery page whose window is 200 px tall with a 400 px menu fails today by
  clipping the panel at the window edge; the panel must open as its own surface
  and outlive the edge.
- A submenu inside a platform menu must close only the submenu, with the outer
  panel's grab intact.

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

**A Win32 popup was slid away from the tree's rectangle, and the grab made the tree
believe the moved positions.** The tree placed a menu's flyout beside its row; `popup_rect`
then ran `place_popup` against the monitor's work area again. A flyout 823 px tall with less
room below the menu was slid up by 746 px: the window painted the panel at the new place,
while the tree's rectangles stayed beside the row. A popup keeps the mouse capture, so every
move outside its window still arrives at its presenter, and the presenter shifts it by the
panel's tree origin — a position the user was not looking at. Moving back onto the parent menu
reported a point 746 px below it, so no row left its hover and the flyout never closed;
pressing near the flyout's left edge mapped outside every level and the whole menu dismissed.
The tree now owns placement: the host names the monitor's work area as the output, the layout
applies flip-then-slide, and `popup_rect` opens the window at `spec.rect` unchanged. Caught by
`a_platform_panel_unfolds_inside_the_output` plus the recorded probe log of the run that showed
`origin (768.1, 643.8)` receiving `PointerMoved (-60, 746.4)` while the cursor stood on the
parent row.

**A panel's window also acted on the parent's input.** Running the first platform panel showed
the menu's hover lighting and going out as the pointer moved, with the page's nav row lighting
instead. Both surfaces bind their own `wl_seat`, and a seat delivers pointer and keyboard events
to every proxy bound on the connection, so the popup's presenter dispatched each event shifted
by its own origin while the parent's dispatched the same event unshifted — into the same tree.
`WindowState::surface` now gates `enter`/`leave` on both devices and the events that name no
surface (`motion`, `button`, `key`, `modifiers`) ride `pointer_over`/`keyboard_focus`, which only
a matching `enter` sets. Caught by moving the pointer over the popup in a nested session: the
parent's nav item lit up at the popup-local coordinates `(62, 92)`.

**An acrylic panel painted nothing.** `MenuFlyoutPresenterBackground` is
`DesktopAcrylicTransparentBrush` (`#00000000`); in a window the acrylic pass *is* the background.
On a surface of the panel's own there is nothing behind it, so the pass shaded transparent black
and the menu showed only its glyphs. A material now names what to paint where no backdrop can be
sampled — WinUI's `AcrylicFallbackColor` reaches `Acrylic::fallback`, and `paint_panels` draws
that fill instead of recording a backdrop entry, because a panel list has no window behind it.
Glass names no fallback and keeps its declared fill. Caught by looking at the same popup: the
rounded panel and its border appeared the moment the fill was painted, and the hover row stopped
being the only thing on screen.

**A pointer that crossed into a panel erased the tree's hover.** A row lit for a frame and
went out as the pointer moved, and no row was lit once it stopped. Every build recomputed the
hover from the frame's pointer argument, and the window's argument is its own surface's pointer
— `None` while the pointer is on the panel — so each frame the window built cleared the hover
the panel's motion had just set, and the last of those frames is the one left on screen.
`build_step` now keeps the tree's own pointer when the surface supplies none: one tree, one
pointer, whether the surface under it is the window or a panel. Caught by warping the pointer
onto the popup and screenshotting: the row went out a frame after it lit.

**A surface that still held the last pointer position hid the pointer that arrived.** After
the fix above, crossing onto the popup still built the frame without hover: the window's
`pointer_over` was still true on the enter batch, so `SurfaceHandle::pointer()` reported the
place the pointer last stood on the window, and the hint the panel's motion had just made won
over the crossing frame's own build. The group now reads its panels' queues before the build,
asks the panels first for the pointer (their intake is this turn's), then the window — whose
queue its own step reads later, so its answer is stale — and hands the build a hint the
surface's own sample may still override. Caught by warping the pointer out of the popup and
back in: the row lit only after the pointer stopped until the panel's answer was asked first.

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

**A platform dismissal was going to arrive as an `Event`.** The first design had `popup_done`
become `Event::Dismissed` for the panel's subtree, so the app would read it from the event stream.
An event is what a device produced and the tree routes through hit testing; the compositor taking
a popup back happens to a surface, outside any hit test, and injecting it would have meant
inventing a coordinate and an event kind no input source sends. The host reports it with
`AppLoop::dismiss_panel` instead, the same `dismissed` slot an outside press fills, so the app has
one dismissal path and `cx.dismissed()` answers for both. Pinned by
`a_closed_panel_is_reported_as_dismissed_and_not_reopened` in
`crates/ui-platform/tests/host_loop.rs`.

**A panel was going to take focus on Win32.** The first design activated a menu-shaped popup
and kept only a tooltip `WS_EX_NOACTIVATE`, because "the panel owns the keyboard" reads as a
platform focus requirement. That creates a second focus model: the shell's activation would
compete with the engine's focus scope, and closing a panel that had activated would have to
hand the foreground back to the window it covered. Every panel is `WS_EX_NOACTIVATE` now and
answers `MA_NOACTIVATE`. The owner keeps the keyboard, the engine routes the keys into the
popup's scope, and nothing but pixels and the pointer crosses the window boundary. Text input
inside a panel is the price: neither backend gives a panel an input method yet.

**The Win32 floor was read at the system DPI.** `tracked_minimum` called `GetSystemMetrics` for
`SM_CXMINTRACK`/`SM_CYMINTRACK`, and in a per-monitor-aware process that call answers for the
system DPI, not the window's monitor. On a scaled display the floor it returned was smaller than
the shell's own, so the window could be dragged under the frame's minimum and the declared
client floor lost a few logical units per step. The same function now reads
`GetSystemMetricsForDpi` with `GetDpiForWindow`. Derived from Microsoft's documented behaviour of
the two calls; the unscaled case is unchanged, which is what `resize_probe` still checks.

## Status

Implemented: toplevel and layer-shell surfaces, shm and dma-buf presentation,
explicit sync when the compositor offers it, configure-driven resize,
fractional scaling, keyboard and IME through Wayland protocols, colour
management where available, and lending the connection to application
protocols through `WaylandLink`. Dialog surfaces are [their own page](./dialogs.md):
a toplevel with a parent, plus `xdg-dialog` where the compositor carries it.
On Windows, a decorated window and a layer
approximated as a topmost popup are behind the same spec: the popup takes
clicks only where its frame consumes them, gives up the monitor edge an
auto-hidden taskbar hides behind unless the spec asks to cover it, and hands
the keyboard back when a pointer-following surface is left; see
[Rendering Backends](../rendering-backends.md).

Partial: the engine/presenter split — `Host` owns an `Engine`
(`AppLoop<Box<dyn App>>` per surface, one `SurfaceHandle` per presentation) and a
backend holds no app. `PopupSpec` and `Constraint` are unit tested, and the
constraint maths a platform without a positioner needs is now the layout's
`place_anchored`, which the engine runs itself.

Platform popup surfaces on Wayland (GPU presenter only; the CPU one falls back
to its parent) have run: the gallery's menu flyout opens as an `xdg_popup` at
its trigger and closes on an outside click. Win32's owned, non-activating
window with the mouse captured is compile-verified only. Nested panels and a
popup that repositions are unrun on both.

Planned: the `Host` split into separate presenters that a panel surface
outliving its subtree needs.

Not implemented: text input in a popup surface; macOS and X11 windows;
multi-seat input; tablet and touch protocols beyond the pointer path. A pointer
that moves between surfaces of one group is followed — the tree keeps one
pointer — and the frame it crosses builds with it: the host reads the panels'
queues before it builds the group's frame, resolves the one pointer the
surfaces share, and hands it to the build as a hint the surface's own sample
may override. Pinned by `a_pointer_crossing_between_surfaces_reaches_the_frame_it_builds`
in `crates/ui-platform/tests/host_loop.rs`.

## See also

- [Rendering Backends](../rendering-backends.md): buffer pools and explicit
  sync.
- [Dialog Surfaces](./dialogs.md): a secondary window, its modality and its
  lifecycle.
- [Colour and HDR](./colour.md): the output description this backend reads.
- [Frame Pipeline](./frame-pipeline.md): what runs after the callback arrives.
