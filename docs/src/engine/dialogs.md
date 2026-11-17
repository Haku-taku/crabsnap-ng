# Dialog Surfaces

A dialog is a window that belongs to another window: it stacks above it, holds its
input when modal, and closes with it. This page covers the contract that relation is
expressed by, the two platform implementations, and the host-side rules that make
modality hold even where the platform only hints at it.

## Problem

`SurfaceSpec` described a `Window` or a `Layer`, and `Host::open` opened every surface
as an independent top-level. An application that wanted a file picker, a settings
panel, or a confirmation therefore had three bad choices: paint it inside its own
window (losing the platform's stacking and task switcher behaviour), open a second
unrelated window (which the compositor tiles, hides behind other windows, and which
never goes away with the window that opened it), or refuse the interaction.

Nothing in the API said "this window belongs to that one". The host could not block
one surface's input because it never learned that a surface was held; nothing closed
a secondary window when its owner closed.

## Comparison

| Platform | Ownership | Modality | Notes |
| --- | --- | --- | --- |
| Wayland | `xdg_toplevel.set_parent` (stable) | `xdg-dialog` v1 (staging), `set_modal` | stacking is stable; only modality is a hint, and the client filters the parent's events |
| Win32 | `CreateWindowExW` with the owner in `hWndParent` | `EnableWindow(parent, FALSE)` | an owned window is destroyed with its owner |
| macOS | `NSWindow.beginSheet(_:completionHandler:)` | the sheet holds its parent | not implemented here |

The quotes below are from the local protocol files:
`/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml` and
`/usr/share/wayland-protocols/staging/xdg-dialog/xdg-dialog-v1.xml`.

`xdg-shell` says of `set_parent`: "This surface should be stacked above the parent
surface and all other ancestor surfaces", and names dialogs, toolboxes and other
auxiliary surfaces as what the parent is for. Of modality, `xdg-dialog-v1` says:
"Clients must implement the logic to filter events in the parent toplevel on their
own." A compositor may deliver the parent's events unfiltered; the hint affects
presentation, not delivery.

That is the shape of the whole capability: the platform gives stacking and, where it
exists, a modality hint, and the client is responsible for the input filter in every
case.

## Contract

A dialog is a surface with a role of its own:

```rust
SurfaceSpec::dialog(app_id, title, size)      // modal by default
SurfaceSpec::dialog(..).with_modal(false)     // modeless
SurfaceRole::Dialog { size, modal }
```

It is opened through the surface it belongs to, never as a top-level surface of its
own. Three entry points exist because callers sit at different layers:

| Caller | Entry point |
| --- | --- |
| message handler between turns | `MessageCtx::open_dialog(parent, spec, app, options)` |
| the loop's owner | `Host::open_dialog(parent, spec, app, options)` |
| a backend | `SurfaceHandle::open_dialog(spec, presentation)` on the parent's handle |

Both host-level calls check `SurfaceSpec::dialog_modal()` first: a spec that is not a
dialog is refused with `"a dialog surface is opened with SurfaceSpec::dialog"`, so a
window cannot be smuggled in through the dialog path. The parent's handle decides
whether the platform can express the relation; a presentation that cannot returns
`"this presentation has no dialog surfaces"`. The refusal is expected, not fatal: the
caller falls back to presenting the same content in its own window.

A dialog is a surface of its own with an engine and an app of its own, the same rule
as every surface `Host::open` creates. Nothing is shared between the parent and the
dialog but what the application itself shares — the dialog's app writes its result
where the parent's app reads it. There is no completion callback and no borrowed
engine: a dialog's tree would otherwise have to be merged into its parent's focus
ring, and its app lifecycle into the parent's.

A panel borrows the role without the second engine. `Presentation::Window` asks the
surface that holds the panel for a modeless window of its own
(`SurfaceHandle::open_panel_window`): the platform opens the owned, transient window a
dialog is, and the host presents the panel's subtree into it through
`AppLoop::present_panel`, so the owner's tree, app and focus stay in charge and the
window closes when the panel leaves the tree. Modality is pinned to `false`, because a
panel that only reports must not block the dialog it describes. The platform still owns
where the window stands; how big it is belongs to the panel, and the user is not offered
a drag that the content would not fill. See
[Popovers](./popovers.md#where-a-panel-is-painted).

What the contract deliberately does not do:

- It does not resize or reposition an open dialog. The platform decides where a
  transient child goes; the opening size is a request, exactly as for a window.
- It does not inherit modality. A modal dialog blocks the surface that opened it, not
  that surface's dialogs, and a dialog of a dialog is an ordinary link to its own
  parent.
- It does not give the dialog a title-bar policy. It is an ordinary framed window on
  Win32 and a toplevel with a parent on Wayland; a backend that decorates differently
  is a later change to that backend, not a field on the spec.

## Host rules

Two rules live in the host, because neither platform can guarantee them.

**A modal dialog holds its parent's input.** `mark_modality` runs at the start of
every turn over the live surfaces: it collects the parent of each live modal dialog
and calls `AppLoop::set_input_blocked` on it. A blocked engine still builds frames —
the parent stays painted behind the dialog and keeps animating — but `dispatch` drops
every event and the surface's pointer is not read, so no hover, focus or press
changes. The set is recomputed each turn, so closing one of two modal dialogs leaves
the parent blocked by the other, and a modeless dialog blocks nothing.

**A dialog closes with its parent.** `retain_live` first drops retired and
self-closed surfaces, then repeatedly drops dialogs whose parent is no longer live
until nothing more is orphaned. The loop matters: a dialog of a dialog whose parent
was itself orphaned must go in the same pass. The platform may also destroy the
dialog's native window on its own — Win32 does when the owner is destroyed — and the
host sees that as a closed handle, which is the same path.

**A surface leaving is its opener's news.** A dialog that answers stores the answer in
the app that opened it and then closes; nothing in the parent's tree changed, so the
parent's last build predates the answer and the loop has no reason of its own to draw
again. `retain_live` therefore reports whether it retired anything, and the turn forces a
frame on every remaining surface when it did. Without it, the picker's answer arrived and
the window that asked stayed blank until the next pointer event — the state was right and
the screen never said so.

Platform hints still go out where they exist. On Wayland a dialog is a toplevel with
`set_parent`, marked through `xdg_dialog_v1` and `set_modal` when the compositor
advertises `xdg_wm_dialog_v1`; on Win32 the dialog is an owned window and a modal one
disables its owner. Those are for placement, decoration and activation policy. The
input filter above is what makes modality a rule rather than a wish.

One consequence to state plainly: a blocked surface keeps its focus ring and its
hover state as painted, because the engine is not told to clear them — it never sees
the focused window lose activation in its own model. The ring is the last state the
app drew.

A layer surface cannot own a dialog on Wayland: `xdg_toplevel.set_parent` needs a
toplevel parent, and a layer has none. `create_dialog` returns that as a protocol
error, which the host reports as the presentation's refusal, and the caller falls
back to the in-window modal.

## Implementation sites

| Path | Responsibility |
| --- | --- |
| `ui-platform/src/window.rs:73` | `SurfaceSpec::dialog`, `with_modal`, `dialog_modal`, `SurfaceRole::Dialog` |
| `ui-platform/src/host.rs:164` | `SurfaceHandle::open_dialog`, refused by default |
| `ui-platform/src/host.rs:372` | `MessageCtx::open_dialog`: role check, parent lookup, `DialogLink` |
| `ui-platform/src/host.rs:1104` | `Host::open_dialog`, the same contract between turns |
| `ui-platform/src/host.rs:652` | `mark_modality`: the parent of every live modal dialog is blocked |
| `ui-platform/src/host.rs:671` | `retain_live`: orphaned dialogs are dropped until no chain is left |
| `ui-platform/src/view.rs:5812` | `AppLoop::set_input_blocked`; `dispatch` returns early and the turn reads no pointer |
| `ui-platform-wayland/src/window.rs:1377` | `create_dialog`: `set_parent`, optional `xdg_dialog_v1` and `set_modal` |
| `ui-platform-wayland/src/session.rs:821` | `open_dialog`: the parent's presenter kind, GPU or CPU |
| `ui-platform-win32/src/window.rs:233` | `new_dialog`: owned window, `EnableWindow`, re-enable in `Drop` |
| `ui-platform-win32/src/session.rs:696` | `open_dialog`: wraps the window, adds its frame pace to the wait set |

## Verification

The host rules are pinned by three tests in
`crates/ui-platform/tests/host_loop.rs`, each watched failing with its branch
disabled:

- `a_modal_dialog_blocks_its_parent_until_it_closes` — a fake surface records whether
  each press it dispatched was taken. The parent accepts a press before the dialog
  opens, reports dropped presses while it is up, and accepts again after it closes;
  the dialog's handle is dropped. Red with `mark_modality` neutralised: every press
  read `true`.
- `a_dialog_closes_with_the_surface_that_opened_it` — the parent closes itself on its
  second step while the dialog would run forever; `surface_count()` must reach zero.
  Red with the orphan pass removed: "the dialog outlived its parent".
- `a_presentation_without_dialog_surfaces_refuses` — the scripted backend's default
  `open_dialog` answers `"no dialog surfaces"`, which is the fallback trigger the
  picker and every other caller rely on.

Wayland is verified by a real run. `examples/dialog_probe.rs` in
`ui-platform-wayland` opens a 640×400 parent, then a modal 420×260 dialog, and drives
both through `Host`. In the nested Hyprland session the probe printed
`xdg_wm_dialog_v1: advertised`; both surfaces stayed live and produced 174 frames
over 6 s. The screenshot shows the dialog above the parent, centred, and
`hyprctl -j activewindow` reported it floating at `[1070, 590]` at `[420, 260]`.
With `DIALOG_PROBE_CLOSE_PARENT=2` the parent closed after two seconds and the run
ended with `0 of 2 surface(s) open`, which is the lifecycle rule holding against a
real compositor. `DIALOG_PROBE_MODAL=0` ran the modeless path to completion as well.

Win32 is written but not compiled here. This machine has no Windows target and the
crate is empty off Windows, so `new_dialog`, the owner style, `EnableWindow` and the
`Drop` re-enable have not been through a type check. Reading the `windows` crate's
generated source is the only check available, and it already paid once: the first
version imported `EnableWindow` from `WindowsAndMessaging`, where 0.62 does not declare
it — the function lives in `Input::KeyboardAndMouse`. What would verify the rest: a
`cargo check -p ui-platform-win32` on a Windows host, then a probe that opens a modal
dialog, clicks the parent, closes the dialog, and prints whether the parent took the
press — the same shape as the Wayland probe.

## Corrections

**Wayland was said to have no dialog protocol.** The first version of the file picker
page stated that XDG had no dialog and that the in-window modal was the only option.
`xdg_toplevel.set_parent` has been stable since `xdg-shell` 1, and `xdg-dialog-v1` is
in staging with bindings already present in the `wayland-protocols` crate this
workspace builds with. The framework, not the protocol, lacked the capability. The
picker page now says so, and the capability was built instead of worked around. The
probe's `xdg_wm_dialog_v1` line and this page's citations to the two XML files are
what keep the distinction visible.

**Modality was going to be the platform's job alone.** The first implementation
called `set_modal` / `EnableWindow` and left the parent's engine untouched, which
makes a modal dialog advisory on Wayland: the spec explicitly requires the client to
filter the parent's events, and a compositor may deliver them unfiltered. The host
now blocks the parent's engine itself and the platform hint stays as a hint.
`a_modal_dialog_blocks_its_parent_until_it_closes` is red when the host filter is
neutralised, and that test runs without a compositor, which is the point.

**A dialog survived its parent.** `retain_live` dropped only surfaces that closed
themselves or failed a step; a dialog's native window is not closed when its owner
is, so the dialog stayed in the list with no one left to answer it. The orphan pass
exists now, and it runs to a fixed point because a dialog of a dialog is orphaned one
level at a time. Caught by `a_dialog_closes_with_the_surface_that_opened_it`.

**The dialog was first considered as a panel of the parent's tree.** Painting a
dialog from the parent's engine with `Decl::modal` was on the table, because the
engine already resolves modals, moves focus into them and keeps the keyboard there.
It was rejected: a popup-style subtree has no surface of its own, so it cannot be
stacked by the compositor, cannot survive its parent's size, and its modality would
depend on the very events the parent must ignore. A dialog surface is a second window,
and the engine's modal path stays available inside it for anything the dialog must
hide from itself.

## Status

Implemented on Wayland (run): the role and spec, host-side open and refusal, modality
and lifecycle, `set_parent`, and `xdg-dialog` where the compositor carries it.
Unverified on Win32: the owned window and `EnableWindow` are written, not compiled
here. Not implemented: macOS sheets, X11 ownership, per-dialog decoration. Text input
inside a dialog works because the dialog is an ordinary window with its own engine,
not because this capability handles it.

## See also

- [Window Backend](./window.md): surfaces, roles and the frame loop a dialog runs on.
- [Presentation Surfaces](./window.md#presentation-surfaces): why a dialog is not a
  popup, and what a popup is instead.
- [File Picker](./file-picker.md): the first caller, and its in-window fallback.
- [File Choosers](./file-choosers.md): the platform's dialog, which is not a dialog
  surface the host steps and holds its parent's input directly.
- [The Event Loop](./event-loop.md): turns, waits and the frame clock behind the
  host rules above.
