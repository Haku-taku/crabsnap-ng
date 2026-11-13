# Profiling

Two questions, two tools: where a frame spends its time, and why a window takes
forever to close.

## Frame phases

`hotpath` is an optional dependency. Without the feature it contributes zero
bytes to the build.

```sh
cargo build --release --example gallery --features profile
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=slider GALLERY_SECONDS=6 ./ui/target/release/examples/gallery
scripts/nested-session.sh stop
```

On exit the profile prints a table by label: calls, average, p95, total.
Release is not optional; debug numbers measure the absence of optimisation,
not the engine.

Reading the table:

1. **Calls first.** A phase that runs once per frame showing thousands of
   calls is inside a loop it should not be in.
2. **Labels nest.** `text ⊃ linebreak ⊃ slice ⊃ shape`. When a child dominates
   its parent, drill down rather than reading the parent's total.
3. **`paint` only collects primitives.** Rasterisation happens in the GPU
   process. A small `paint` is not proof that drawing is cheap.

The present path is labelled too, in the backend crates: `atlas`, `gpu_draw`,
`gpu_upload`, `gpu_band`, `gpu_layers`, `gpu_backdrop`, `gpu_blur`,
`gpu_composite`, `gpu_lens`, `gpu_fence` and `commit`. All are opt-in
(`--features profile` on the gallery); the macro is `ui_core::timed!`.

To measure throughput rather than per-refresh cost, set
`GALLERY_UNCAPPED=1`. It removes the one-frame-per-vblank gate and leaves frame
demand alone. Skipped frames are neither shown nor released by the compositor,
so this mode is for stress numbers only, never for judging appearance.

## Present timing

`present` has two branches: explicit sync through
`wp_linux_drm_syncobj_manager_v1`, and a `glFinish` fallback for compositors
that do not advertise it. The nested session does not advertise it. Confirm
with `wayland-info`, and state which branch a number came from.

## Exit stalls

Every step between closing the window and process exit is instrumented
(`ui_platform::exit_probe`). Timing starts at `toplevel-close`; a watchdog
reprints the current stage every second and lists each thread's `comm` and
`wchan`.

```sh
EXITDBG=1 ./ui/target/release/examples/gallery
scripts/exit-check.sh        # close the window; prints the timeline
scripts/exit-stacks.sh       # when stuck, dumps real thread stacks
scripts/freeze-test.sh       # witness window plus test window; reports stalls
scripts/compositor-watch.sh  # second terminal: compositor reply latency
```

These switches bisect the teardown path. All are off in normal builds.

| Switch | Skips | Rules out |
| --- | --- | --- |
| `EXIT_NO_DETACH=1` | `attach(null)+commit` before closing | A surface still holding a dma-buf |
| `EXIT_KEEP_BUFFERS=1` | Destroying GBM buffers and dma-bufs | A driver-level wait on buffer destruction |
| `EXIT_KEEP_EGL=1` | `eglTerminate` | Tearing down the EGL display blocking shared GPU |
| `EXIT_KEEP_GL=1` | The whole EGL context teardown | Pinning the stall to one EGL call |
| `EXIT_FINISH=1` | Adds a `glFinish` before teardown | Queued GPU work the compositor is waiting behind; the `gpu: finish` time is the backlog |

`EXITDBG=1` also calls `prctl(PR_SET_PTRACER, ANY)`, because
`yama.ptrace_scope=1` refuses `eu-stack`. Startup noise (configure, scale,
colour management) only prints with `WAYLAND_TRACE=1`.

## Memory

`GALLERY_ALLOC=1` reports allocation totals, `GALLERY_ALLOC_SITES=1` adds call
sites, `GALLERY_ALLOC_SIZES=1` adds a size histogram. Use them to find
per-frame allocation, not to optimise a single frame.

## Reporting a measurement

State the build profile, the page, the session (nested or host), the machine
load if it was unusual, and which present branch was measured. A number
without those conditions cannot be compared with anything, including a later
run of the same code.
