# Ownership and Copies

A frame builds declarations, nodes, primitives and GPU work on one thread,
and almost every value in that pipeline has one owner. This page states when
a second owner is legitimate, which handle it gets, and how to change shared
data without copying it back. Layout, text and paint contracts stay with
their own pages; this one only decides what is shared and how.

## One loop thread

`Host::run` steps every surface in place (`crates/ui-platform/src/host.rs`).
A surface's tree, text pass, style cache and GPU renderer never leave the
thread that runs the loop, and the EGL context is thread state kept in a
thread-local registry ([Presentation Targets](../engine/present-targets.md)).
Engine values therefore need no thread safety of their own.

Four threads exist beside the loop, each owning one blocking source:

| Thread | Source | What crosses the boundary |
| --- | --- | --- |
| `xdg-appearance` | XDG portal colour scheme | `Arc<Mutex<ThemePreference>>` |
| portal hotkey reader | GlobalShortcuts signals | a shared cell plus a wake fd |
| `portal-screenshot` | a blocking portal reply | owned reply through an `mpsc` channel |
| exit watchdog | teardown stages, debug builds only | log lines |

None of them holds a `Decl`, `Node`, `Style` or primitive. A worker that needs
one receives owned data or an `Arc` and returns owned data; a `Rc` never
crosses that boundary.

**Rule.** `Rc` on the loop thread. `Arc` only where a side thread, an external
protocol or a long-lived static holds the handle.

## Work that leaves the loop

The loop waits between frames, so a worker does not take time from a frame.
Work leaves the loop only when it is long, owns its input, and is not needed
to draw the current frame.

| Work | Today | Why it can leave |
| --- | --- | --- |
| Saving a shot (PNG or ffmpeg) | `jobs.submit` from the overlay's Save and the daemon's output hotkey (`apps/crabsnap/src/main.rs:417`, `apps/crabsnap/src/daemon.rs:444`) | One-shot; the pixels are already an `Arc` the worker can hold |
| Clipboard image | `copy_image` reads and re-encodes on the worker; the request is answered later from `on_turn` (`apps/crabsnap/src/daemon.rs:288`) | Owned bytes; the selection outlives the reply |
| Portal screenshot decode | the capture path, once | Owned bytes; the overlay waits for it, so this trades latency, not responsiveness |
| Video encode (future) | none | A bounded queue and dropped frames, never a stalled loop |

**Shape.** One worker and an `mpsc` (`ui_platform::jobs`): `Jobs` is cloned to
whatever submits, `submit` returns a `Job<T>` carrying one result, and the owner
polls `take()` from its `view` or `on_turn`. A completion bumps a counter and
calls the backend's `Waker` — Wayland's is the same eventfd the portal hotkey
thread uses — and `Host::turn` forces one frame on every surface when the
counter moves, because a finished job is not an event a surface listens for.
`outstanding()` keeps the process alive until running work lands. A generation
per request (a newer one dropping stale results) belongs here when something
repeats; the three callers today are one-shot. A thread and a channel are
enough; no async runtime is involved.

Work needed to draw the current frame does not leave it: glyph rasterization
(the text must appear now), shaping, layout, damage, and the crop a pin is
waiting for. Glyph rasterization can still use other cores inside the frame;
that is parallelism, not a job.

## Handles by data kind

| Value | Handle | Status | Why it is shared |
| --- | --- | --- | --- |
| Paragraph (`TextBlock`) | `Rc<TextBlock>` | Now | Layout, shaping and paint read one block; an edit replaces it |
| Formula | `Rc<Formula>`, face name `Rc<str>` | Now | Same, and the resolve cache must recognize an unchanged formula |
| `Style` | `Rc<Style>` from `StyleCache` | Now | One resolve serves every control at the same scope step |
| Editable buffer (`Decl::text_input`) | `Arc<str>` | Now | The app owns the value and replaces it on each edit |
| Parsed document (`Markdown`) | `Arc<Document>`, `Arc<Prepared>` | Now | The page keeps one parse and hands the same tree to each frame's build |
| Image pixels | `Arc<Vec<u8>>` | Now | The picture's identity; `ImageEntry` and `Cropped` already share it |
| Whole-frame crop | the same `Arc` | Now | A full-frame rectangle has the same rows and stride |
| Clipboard payload | `Arc<Vec<u8>>` | Now | The selection outlives the request that set it, and every paste reads the same bytes |
| Per-frame scratch (`Vec` of routes, damage, copies) | `&mut Vec`, cleared | Partial | Capacity should outlive the frame |

A sub-rectangle crop still copies: its rows are not contiguous in the
source, and the destination is tight. A handle does not change that.

Small strings are their own case. Below roughly one cache line, an inline
string (`SmolStr` or `compact_str`) beats both `String` and `Arc<str>`: no
allocation and no refcount. The project has no such dependency yet; measure
before adding one.

## Identity is not equality

`Rc::ptr_eq` proves "same value" only when the payload is immutable and every
change replaces the handle. `DeclCache` already relies on that
([Incremental Declarations](../engine/declarations.md)).

The reverse is a trap: a changed payload may still arrive behind a new handle
only because the app rebuilt it. Code that decides whether to re-shape, or
whether pixels changed, must compare content or a revision counter. Sharing a
handle is not evidence that the text stayed the same.

egui records the cost of the other direction: reusing last frame's
tessellation by comparing shapes was rejected because the comparison alone
takes about 50% of tessellation time (`egui-0.36.2/src/context.rs:2867`).
Reuse is driven by identity or a dirty flag, never by a deep comparison.

Equality itself may take the shortcut in the safe direction: the same handle
proves the same content (the payload never changes in place), while different
handles fall back to comparing content — a miss, never a wrong hit.
`TextPrim::same_as` does this for a paragraph and its wrap layout, which is
what a text cache lookup compares on every frame.

## Change by copy-on-write

- Edit with `Rc::make_mut`. A block the edit did not touch keeps its handle.
- A reparse shares what its blocks carry: `Document::source` and `Piece::text`
  are `Arc<str>`, so an unchanged block's bytes are never copied. Rebuilding
  the block rows themselves still clones their spans and vectors: they are a
  small struct per block, and making the reparse structural (a green tree)
  costs more than it saves until a document is large enough to measure.
- `Arc::make_mut` only for a type that is already behind `Arc`. Do not move a
  value to `Arc` just to edit it.
- `Cow` fits a read-mostly value returned from a cache. On a write-heavy path
  `to_mut` copies on the first write of every frame.

## Reuse scratch buffers

Per-frame collections clear and reuse; they are never rebuilt per frame.
cosmic-text's `ShapeBuffer` and `BufferLine::reclaim_text` are the pattern:
the buffer belongs to the shaping context and survives the frame. `clone_from`
does the same for a destination that must be overwritten.

A scratch buffer belongs to the frame, the `TextPass` or the renderer, never to a
global.

Measured, in this order: on the gallery's markdown page (`GALLERY_ALLOC_SITES=1`,
release) the largest per-frame allocation source is rebuilding the page's
declaration tree (`Markdown::resolve_in`), not a scratch buffer. Cache a built
tree under its inputs — document revision, width, selection, options — before
micro-reusing buffers inside it; the two are not alternatives, but the order
matters.

## Measure allocations, not clones

The acceptance metric for an ownership change is allocations per frame on a
named page, in a release build, in a nested or host session:

```sh
GALLERY_ALLOC=1 GALLERY_PAGE=slider ./ui/target/release/examples/gallery
```

`GALLERY_ALLOC_SITES=1` adds the top caller chains and `GALLERY_ALLOC_SIZES=1`
a size histogram; `[hb]` prints the counts per heartbeat. State the page and
build before and after, as [Profiling](../development/profiling.md) requires.

A clone count is not the metric. Cloning a small `Copy` value costs less than
the refcount traffic and the extra indirection a handle would add.

Rejected on measurement: `TextRun::text` was moved from `String` to `Arc<str>`
so a paragraph could share its source pieces. Every per-frame construction
site then turned an owned `String` into a handle, which allocates and copies,
and the markdown page went from 3098 allocations and 1555 kB to 3240 and
1561 kB. A string carried inside a per-frame value stays `String`; only a
payload that already exists once and is handed out many times takes a handle.

Not taken: the ffmpeg shim's encoded file. `cs_encode` mallocs the whole
output and Rust copies it out before `cs_free`, so saving a JPEG, WebP or AVIF
pays one copy of the finished file. `Vec::from_raw_parts` over that buffer
would be unsound — the Rust allocator is not the C one — so the only zero-copy
route hands libavformat's AVIO write and seek callbacks to Rust: two FFI
boundaries with their own error and unwind rules, on a shim whose value is a
narrow surface whose layout cannot be wrong. The copy is once per save,
bounded by the file, and never inside a frame, so the `Arc<str>` rule above
decides it: a handle bought for a one-shot copy costs more than it saves.

Baseline, 2026-11-10: 526 `.clone()` calls in library code, counted over
`ui/crates` and `apps` with examples, tests and build scripts excluded. Nine
removable copies found during the audit are gone; what remains either has a
real second owner or belongs to the per-frame scratch row above.

## Anti-patterns

| Pattern | Why it loses |
| --- | --- |
| `Rc`/`Arc` around a `Copy` value | Refcount traffic costs more than the copy avoided |
| `Arc` on a path that never leaves the loop thread | Atomic increments where `Rc` would do; GPUI marks its foreground executor `!Send` instead (`crates/gpui/src/executor.rs:32`) |
| `Cow` on a write-heavy buffer | The first write of every frame copies |
| Deep equality to decide reuse | Costs as much as the work it might save; use identity or a dirty bit |
| A handle whose receiver immediately clones the inner value | The copy moved, it did not go away |

## Where the rules come from

| Source | Invariant taken |
| --- | --- |
| vello `peniko::Blob` | An image is an `Arc`-backed blob; encoders and scenes move the handle, never the pixels |
| Bevy `Handle<T>` / `Assets<T>` | Assets have one home; `RenderAsset::take_gpu_data` moves heavy data between worlds instead of cloning it |
| egui `Arc<Galley>` | A laid-out paragraph is shared until it is edited; `WidgetText` accepts the shared form instead of re-laying it |
| GPUI `SharedString` | `SharedString(SmolStr)` (`crates/gpui_shared_string/gpui_shared_string.rs:15`): cloning a carried string is a refcount, short ones do not allocate |
| rowan / rust-analyzer | Green nodes are immutable and shared across reparses; red cursors are built on demand |
| cosmic-text `ShapeBuffer` | Shaping scratch belongs to the context and is cleared, not reallocated (`cosmic-text-0.19.0/src/shape.rs:88`) |
| Qt Quick threaded render loop | Even with a render thread, the GUI thread's scene data is single-threaded and crosses a sync phase |
| Flutter task runners | The UI thread builds a layer tree; the raster thread consumes it; the boundary is an owned tree |
| Rust Performance Book | Profile before removing a clone; `clone_from` reuses capacity; `Rc`/`Arc` add allocations for rarely shared values |
| `bytes::Bytes` | A byte buffer is cheap to clone and slice; sub-ranges share the allocation |

## Status

Implemented: `Rc<Decl>` in the declaration cache, shared `Arc` image payloads
(`ImageEntry`, `Cropped`), `Cow` returns in damage and primitive projection,
`Rc<TextBlock>`/`Rc<Formula>` through `Decl` → `NodeData` → `TextPrim`/`MathPrim` →
the resolve cache, `Rc<Style>` from `StyleCache`, `Arc<str>` for an input's declared buffer, and
the job service (`ui_platform::jobs`) for saves and clipboard encoding.

Measured on the gallery's markdown page under a continuous sway drag (release,
`GALLERY_HEARTBEAT=1 GALLERY_ALLOC=1`, allocations per built frame): 4347
allocations and 1754 kB before the paragraph and style handles, 3101 and
1561 kB after. Giving the reading card a `Document` handle and its own element
cache key brought it to 2373 and 1366 kB: the card's whole declaration subtree
is reused while its recorded facts hold. The layout passes then stopped copying
each child list (`children_of(...).to_vec()`) and walk it by `child_at` index,
which took the page to 2258 and 1363 kB. The editing card cannot be reused: a
declaration that owns an input's bytes is live state (see
[Incremental Declarations](../engine/declarations.md)). The shell's navigation
drawer was then found to be about half of what was left: it rebuilt 22 slots
and their text blocks every frame because it answered no cache key. With one
(the resolved inks, labels and sizes fingerprinted), the page measured 1574 and
989 kB.

A clipboard image is one `Arc<Vec<u8>>` from the app's encode through the
engine request to the Wayland source that serves it, so a paste hands the peer
the same bytes instead of a copy per request. A protocol message is one
allocation too: the encoder writes the payload into the message's own buffer
and patches the header length afterwards, where it used to build the payload
and then copy it behind the header.

Planned: per-frame scratch reuse, and the structural part of rich-text reparse
(a green tree, deferred until a document is large enough to measure).

## Sources

- [peniko `Blob`](https://docs.rs/peniko/latest/peniko/struct.Blob.html)
- [Bevy `RenderAsset`](https://github.com/bevyengine/bevy/blob/main/crates/bevy_render/src/render_asset.rs)
- [Bevy pipelined rendering](https://github.com/bevyengine/bevy/blob/main/crates/bevy_render/src/pipelined_rendering.rs)
- [egui `WidgetText`](https://docs.rs/egui/latest/egui/enum.WidgetText.html)
- [rowan](https://docs.rs/rowan/latest/rowan/) and rust-analyzer's
  [syntax book](https://rust-analyzer.github.io/book/contributing/syntax.html)
- [cosmic-text](https://github.com/pop-os/cosmic-text)
- [Qt Quick threaded render loop](https://github.com/qt/qtbase/blob/dev/src/quick/scenegraph/qsgthreadedrenderloop.cpp)
- [Flutter architectural overview](https://docs.flutter.dev/resources/architectural-overview)
- [Rust Performance Book: heap allocations](https://nnethercote.github.io/perf-book/heap-allocations.html)
- [`bytes`](https://docs.rs/bytes)
- [`triomphe`](https://docs.rs/triomphe)

Local copies of egui 0.36.2, cosmic-text 0.19 and GPUI 0.2.2 are listed in
[External Sources](../reference/external.md).

## See also

- [Frame Pipeline](../engine/frame-pipeline.md): where these values are built
  and dropped.
- [Incremental Declarations](../engine/declarations.md): the `Rc` fast path
  and what a cache entry may assume.
- [Style Objects](../engine/style-objects.md): the cache key and the current
  clone on a hit.
- [Profiling](../development/profiling.md): allocation counters and how to
  report a measurement.
