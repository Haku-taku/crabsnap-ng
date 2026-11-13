# The Legacy App, Read as a Migration Source

What the previous CrabSnap does, module by module, and which of its contracts this
workspace already owns. Paths are relative to `/home/ollie/git/crabsnap`; each claim was read
from the cited line, and a claim about *why* something is shaped that way is marked
`[unverified]` because the source cannot answer it.

The migration target is not this program's file layout. It is the set of behaviours a
screenshot and pin tool must have, rebuilt on the engine here.

## One binary, three roles

`crabsnap` is one executable that is a CLI, a daemon and a surface owner at once
(`src/main.rs:7`, `src/app/cli/mod.rs:179`). The daemon is not a separate program: the same
binary re-enters `src/platform/linux/daemon/server.rs:137`, and `--no-daemon` turns the client
into the surface owner directly (`src/app/cli/mod.rs:144`). Requests travel as JSON lines over
`$XDG_RUNTIME_DIR/crabsnap-$WAYLAND_DISPLAY.sock` (`src/platform/linux/daemon/discovery.rs:35`).

Everything interactive ends in one loop: nonblocking `accept` on the socket plus
`AppState::pump_input`, whose only wait is the Wayland pump (`server.rs:229`,
`src/app/state/mod.rs:530`). Global hotkeys are installed only by a standalone daemon, never by
a temporary one (`server.rs:137`).

The settings window is the exception: `eframe::run_native` takes the main thread and its own
winit loop in a separate process (`src/app/cli/commands/settings/mod.rs:52`), which is why
`eframe` is linked into every command (`Cargo.toml:88`).

## Capture: one fixed chain of five rungs

`CaptureBackend` is stateless, `ImageSource` is the stateful half, and the order is literal —
`ext-copy → wlr-screencopy → kwin → portal → shm` (`src/platform/linux/capture/mod.rs:94`).
Availability is probed separately, and `backends()` discards its probe argument without
filtering (`:103`); `select()` takes the first available (`:108`).

| Rung | What it gives | What it costs |
| --- | --- | --- |
| `ext-image-copy-capture-v1` | Per-output and per-toplevel capture, shm or dma-buf | A ready event polled by roundtrip plus 2 ms sleeps, about a second worst case (`ext_copy.rs:220`) |
| `zwlr_screencopy_manager_v1` | Output capture, one frame per copy, `copy_with_damage` for live | Output-only (`wlr_screencopy.rs:47`) |
| KWin `ScreenShot2` | Output capture over a private D-Bus interface and a pipe fd | KWin only; premultiplied BGRA (`kwin.rs:56`) |
| `org.freedesktop.portal.Screenshot` | Whole-screen PNG behind a dialog | Interactive, slow (`portal.rs:75`) |
| `wl_shm` placeholder | Nothing | `available()` is true and `open()` is `Unimplemented` (`shm_source.rs:21`) |

Two consequences the new design must fix rather than inherit:

- Because the fifth rung always claims availability, `select()` never returns `None`, so
  `live::available()` is always true and capability checks mean nothing (`src/live/mod.rs:212`).
- Every captured frame rebuilds a probe that opens two blocking session-bus connections
  (`src/app/state/frames.rs:54`, `capture/probe.rs:46`), so "is a portal there" is answered on
  the frame path.

Window targets are resolved through compositor IPC — `hyprctl`, sway or niri by
`(app_id, title)` (`capture/window.rs:80`) — because the capture protocol carries no focus or
geometry. Multi-output composition is plain geometry plus paste on a max-DPI base scale
(`capture/all.rs:44`).

## Selection, annotate, deliver

The chain after a hotkey is one state machine, and each step hands the next a fully baked
document rather than a live protocol object:

1. `capture_and_select` parks the previous session, freezes a frame, ensures one overlay
   surface, pushes a `FreezeLayer`, then `begin_selection` (`src/app/state/mod.rs:2798`).
2. Selection geometry is pure `kurbo` arithmetic with eight handles and clamping
   (`src/selection.rs:206`); the overlay's state enum is
   `Selecting / Annotating / DraggingToolbar / DraggingSetting / DraggingScrollbar / Picking`
   (`src/ui/panel/overlay/mod.rs:148`).
3. Confirm bakes pixels, a clean base and the annotation document (`mod.rs:1612`); live mode
   re-captures after hiding the overlay (`src/app/state/pins.rs:25`).
4. Delivery always pins and optionally writes a file or the clipboard
   (`src/app/state/deliver.rs:14`). Clipboard *write* is external `wl-copy` only
   (`src/clipboard.rs:494`), even though reading uses `zwlr_data_control_v1`.

## Pins: `layer/` is mechanism, `layers/` is content

The two directories are not duplicates. `src/layer/` owns the stack, the surface, present
slots and the compositor backends (`src/layer/mod.rs:16`); `src/layers/` owns the concrete
`freeze`, `sticker` and `debug_overlay` instances and depends on the first one-way
(`src/layers/mod.rs:15`). A pin layer rasterises every pin of one output into one canvas, and
moving or zooming rewrites only origin and scale (`src/layers/sticker/layer/mod.rs:1`). Pin
interaction is pure: drag moves the origin, the wheel zooms about the centre, scale is clamped
to 0.05–32 (`src/layers/sticker/stack.rs:8`).

The debt here is ownership, not naming: `layer/host.rs` still runs a full surface lifecycle
while `SurfaceStack` claims to be its only owner, and the two expose overlapping size and input
methods (`src/layer/host.rs:165`, `src/layer/surface_stack.rs:498`).

## Long shot: stitch by row probes

Alignment compares each new frame with the previous one, on 8-bit luma subsampled 4×
horizontally (`src/stitch/mod.rs:153`). Fixed chrome is excluded by per-row activity bands and
a median fraction, plus `ignore_top` (`:246`). `best_shift` searches offsets exhaustively with
early abandon, and accepts only with a zero-advance rejection, `MISMATCH_COST = 25` and a
`RIVAL_MARGIN = 3` against the second-best offset (`:978`); probe length steps down 1/2/4 and
the longest decides (`:740`). `place` appends rows below or prepends above and rejects upward
matches farther than one viewport as ambiguous (`:550`).

Limits are explicit: 30 000 rows, 6 000 frames, 120 s, a 96-row strip
(`src/longshot.rs:81`, `src/stitch/mod.rs:68`). Grab and scroll are injected closures and time
is a virtual clock (`src/longshot.rs:109`, `:184`), so the whole algorithm is testable without
a compositor — the one subsystem in the old tree shaped that way from the start.

## The annotation document

Undo is centralised, inverse-and-replay, and belongs to the document, not to each tool:
`EditOp { Add, Remove, Move, Resize, ToolState, Nodes, Reorder, Restyle, TextEdit, Retune }`
with a stashed edit body and a dirty rect (`src/annotate/document/mod.rs:23`, `:133`). Tools
only describe what they would do; the manager commits (`src/annotate/manager/mod.rs:1764`).
The eraser is the interesting case: it stores no mark, only probe points, and the manager
deletes what they cover (`tools/eraser.rs:30`).

Tools draw through a scene IR, not through the UI engine: `preview(&mut Scene)` and
`Annotation::to_scene` (`src/annotate/tool.rs:395`). That is the seam the migration can keep —
`model` has no engine imports at all and could move unchanged (`src/model/frame.rs:10`),
while `annotate` needs a scene-command consumer, a text engine and one stray chrome helper
(`tools/bezier.rs:881`).

## The bespoke engine, against this one

The old engine is immediate-mode declarations with retained per-`Id` state
(`src/ui/engine/widget/mod.rs:1266`), a custom cursor-stack layout with `SizeHint`/`SizePolicy`
(`src/ui/base/layout.rs:391`), one exclusive focus slot (`:2138`), and a three-state
`Dirty { Empty, Rects, Full }` (`src/ui/engine/damage.rs:36`).

| Contract | Legacy | Here |
| --- | --- | --- |
| Declarations | Re-declared per frame, state keyed by `Id` | Same idea, `Decl`/`Element` (`crates/ui-core/src/tree.rs`) |
| Layout | Cursor stack plus explicit hints | CSS subset: flexbox, grid, inline content (`engine/layout.md`) |
| Damage | Three-state, per widget | Per-node damage and a hashed scene (`engine/frame-pipeline.md`) |
| Animation | `AnimStore` per `Id`, tween and spring | Declared `Presence` and transitions (`engine/transitions.md`) |
| Hit test | Six parallel implementations | One `hit.rs::rebuild` |
| Text | parley directly, edit state machine beside it | Shaping, bidi, grapheme and editable text in `ui-core` (`engine/text-and-input.md`) |
| Scroll | `Response.scrolled` always 0; scrolling bespoke per panel | Scroll containers (`engine/scroll.md`) |
| Theme | File-local colour constants; egui-only `Palette` | Design tokens and three control sets (`reference/tokens.md`) |
| IME | In the platform layer, batched by annotate | `ImeState` through the frame target (`engine/text-and-input.md`) |

What the old engine never had and the new one does: a scroll container, an IME path, a token
layer, popup placement, and a hit test that is one implementation. What the migration must
still bring across: the tool bridge and the magnifier. The snap zones came with the selection's
floating toolbar (`apps/crabsnap/src/toolbar.rs`), and the eight resize handles with the
overlay's grips.

## Rendering substrate

One currency: `Frame { ScRgbImage }`, linear f16 scRGB, with colour description as a property
(`src/model/frame.rs:15`). Byte transport has three formats — `Sdr8`, `Fp16`, `Fp16Straight` —
converted in one funnel, `encode_for_output` (`src/core/color/output.rs:140`). GPU is desktop
GL over EGL on a GBM device into 4× MSAA FBOs (`src/gl/egl.rs:37`); HDR presentation is an
`ABGR16161616F` dma-buf plane (`src/layer/present/dmabuf/backend.rs:38`).

Three independent fallback decisions exist (layer compositor, one-shot scene, interactive
presentation), each with its own availability rule (`src/layer/backends/mod.rs:32`,
`src/render/mod.rs:749`, `src/layer/present/target.rs:159`). The one-shot annotation path
flattens HDR to SDR8 before compositing (`src/render/mod.rs:450`), so the "HDR everywhere"
claim is only true on the interactive path.

Optional pieces are dlopen'd at runtime: FFmpeg stills via a C shim, PipeWire audio, EGL
itself (`src/ffmpeg/still.c:79`). GBM is the exception — linked at load time when compiled in
(`src/gl/gbm.rs:28`). Building without FFmpeg headers removes the format, not the feature flag
(`build.rs:59`).

## Settings

Three parsers write the same TOML file with no locking: the main `Settings`
(`src/settings.rs:20`), the settings crate's `[ui]`/`[shortcuts]`
(`crates/settings/src/store.rs:36`), and a hand-spliced `[annotate.*]` writer
(`src/settings.rs:248`). The shared schema is real; the ownership is not. Several keys are
written and never read — tonemap mode, reference white, record backend, audio fields
(`crates/settings/src/store.rs:163`).

There is a live bug in that seam: the settings window stores the display name `"H.264"`
(`store.rs:291`), and the record parser accepts only `"h264"` and hard-errors otherwise
(`src/record/plan.rs:30`).

## Debt not to carry across

- Six hit-test implementations, three engine aggregates, seven `AnimStore`s in one struct.
- Oversized files: engine widget 2 665 lines, overlay 1 798, annotation manager 2 261.
- A build-time capability decision (EGL/GBM headers) that the docs describe as runtime
  probing.
- `#[allow(dead_code)]` fields on the very type that claims to own the surface lifecycle.
- 28 diagnostic examples (4 621 lines) standing in for integration tests.

## Mapping

| Legacy | Target here |
| --- | --- |
| `model/` (frame, pixels, codec) | New app crate; colour funnel stays as written |
| `annotate/document` + tools | New app crate, on `ui-core` scene commands |
| `selection.rs`, `select.rs` | App crate; pure geometry, no port |
| `stitch/`, `longshot` | App crate; keep the injected grab, scroll and clock |
| `layer/` stack and present | `ui-platform-wayland` present targets plus one app-level layer registry |
| `layers/sticker` | App crate: pin state plus `Decl` output |
| `ui/` engine, widgets, overlay | `ui-core`, `ui-platform`, `ui-widgets-md3` |
| `platform/linux/capture` | App crate, platform-bound, on [`WaylandLink`](../engine/window.md#application-protocols-share-the-connection) |
| `hotkey/`, `inject`, `portal` | App crate, platform-bound |
| `settings` + `crates/settings` | One schema, one writer |
| `record/`, `ocr`, `translate` | Peripheral services, optional features |

## See also

- [Window Backend](../engine/window.md): the connection an application protocol layer binds on.
- [Architecture](../architecture.md): the tier rules the new app crate has to obey.
- [Engine Gaps](../reference/engine-gaps.md): what the engine still lacks for this app.
