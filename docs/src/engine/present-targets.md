# Presentation Targets

The app declares content. The engine decides what needs presenting and hands
a backend either pixels or a scene. Cursor, IME, and the submit decision belong
to the engine; an app that performs them has written a second copy of the frame
rules.

## Two contracts

Both live in `ui-platform`.

| Trait | Shape | Implemented by |
| --- | --- | --- |
| `PresentTarget` | The engine renders into `pixels_mut()`, then calls `present(damage)` | `OffscreenTarget`, the CPU window target |
| `SceneTarget` | The engine hands over `DrawList`, `TextPass`, and `Damage` | The GPU presenter in `ui-platform-wayland` |

Both expose `set_cursor`, `set_ime` and `set_input_region`; a target with no
shape to set keeps the default no-op. `SceneTarget` adds `commit_pending()`:
the surface owes a commit that is not about pixels, such as a pending
input-method state or input region.

`AppLoop::frame_scene` returns `SceneFrameOutcome { outcome, presented }`;
`presented == None` means this frame had nothing worth carrying to the
compositor.

## The submit rule

There is exactly one place that decides:

```
presented = damage is non-empty
         || still animating
         || IME state changed
         || target.commit_pending()
```

The last term cannot be dropped. Without it the compositor keeps a stale caret
rectangle or an input region the tree no longer claims, and the frame that
would clear either may never come.

## Who owns the loop

`Driver::step` holds the state and the app only receives a `Turn`:

- It ingests events into the `AppLoop`.
- It decides whether to draw: `take_dispatched()`, a silent host change,
  `is_animating()`, or a size change.
- If not, it sleeps until `next_wake()` or the host's next event. If yes, it
  sets the scale, hands back the pointer, and lets the app build.

`Driver::pace` waits for the next frame window after presenting — a Wayland frame
callback, or on Windows the swap chain's frame-latency object — so the next turn
lands in a new window. `uncapped` is a switch on the driver, not a branch in the
app.

> **Wrong path:** letting an application decide "this frame does not need
> presenting". That is a second copy of the rule above, and the first one to
> drift loses a caret or a fade.

## Host and session

`FrameHost` is implemented once per window backend: size, scale, pointer,
`can_draw`, `wait`/`wait_for_frame`, close state, events, and silent changes.

`ui-platform-wayland::Session<A>` is the only handle a host needs:

```rust
pub struct Session<A: App> {
    gpu: Option<GpuWindow>,   // field order is teardown order
    driver: Driver,
    lp: AppLoop<A>,
    win: WaylandWindow,
}
```

`open` creates the window and picks the presenter; if the GPU is unavailable
it returns an error unless the CPU presenter was requested. `tick` runs one
turn: ingest, decide, wait, build, present, pace. The field order guarantees
teardown: GPU objects while the surface still exists, then the loop, then the
window.

`split()` exists for hosts that genuinely need cross-layer access (clipboard
routing, scripted input). It is not a general escape hatch.

The EGL context, its GBM device and its display belong to the GPU, not to a window:
`GpuWindow` takes a per-render-node context from a thread-local registry, so a process with an
overlay and several pin canvases opens one. The renderer stays per window — its destination and
its uploaded atlases encode one surface's colour, SDR or HDR, and an HDR canvas must not read
the SDR atlas a sibling uploaded.

## Corrections

**Every window created its own EGL context.** A context, its GBM device and its display are a
GPU resource, but the presenter opened one per window, so a daemon with four pin canvases held
four of each. Opening one per render node costs a second window 1.2 ms less in a nested Hyprland
session (9.5–9.7 ms → 8.4–8.7 ms, release build, second window of a process), and one EGL
display and GBM device fewer per surface. The first context in a process still costs 60–185 ms:
that is driver initialization, and sharing cannot move it.

**The GPU path bypassed the target contract.** The gallery called
`sync_atlas`, `present`, `set_cursor`, and `set_ime` itself, and decided
whether to submit. The same frame rule existed twice, in the engine for CPU and
in the app for GPU, and only the app's copy checked `ime_pending()`. Moving the
GPU path onto `SceneTarget` removed the second copy;
`CRABSNAP_TARGET_CHECK=1` over 1814 frames reported `differing=0`, so the move
changed no pixel.

**The submit rule was incomplete.** A frame with no damage and no animation
could still owe a commit for IME state. `commit_pending()` is the fourth term,
and removing it turns
`a_scene_frame_presents_only_when_it_has_something_to_carry` red.

## Traps

| Change | Symptom |
| --- | --- |
| Drop `commit_pending()` from the rule | A stale caret or preedit rectangle stays on screen |
| Decide "needs present" in the app | Two rules drift; one of them loses a fade |
| Call `set_cursor`/`set_ime` from the app | The engine's state and the backend's disagree |
| Reorder `Session` fields | Teardown touches the surface after the objects that depend on it |
| Present with an empty damage list every frame | Wasted commits; the compositor never settles |

## See also

- [Frame Pipeline](./frame-pipeline.md): what a frame carries and when it is
  built.
- [Rendering Backends](../rendering-backends.md): what a `SceneTarget`
  implementation must guarantee.
- [Graphical Tests](../development/graphical-tests.md): the presenter the
  gallery uses in each mode.
