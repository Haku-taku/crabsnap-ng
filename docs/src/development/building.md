# Building

Run cargo from `ui/`. The workspace root is virtual: `cargo build` at the
repository root does not find a manifest.

## Commands

| Purpose | Command |
| --- | --- |
| Build | `cargo build` |
| Full tests | `cargo test --workspace --features ui-text-fontique/system` |
| Zero warnings | `cargo build --workspace --tests --examples` |
| Lint | `cargo clippy --workspace --all-targets` |
| Layering | `cargo test -p ui-platform --test platform_boundary` |
| Look or speed | `cargo build --release` and `--release` on every run |
| Frame profile | `cargo build --release --example gallery --features profile` |

## Toolchain

`rust-toolchain.toml` pins a dated nightly. On the nightly channel Cargo passes
`-Zembed-metadata=no` by default, so a `.rlib` no longer stores a second copy of
the metadata that pipelined compilation already writes to its `.rmeta`. On this
workspace that cut the artifacts directory by 16% in `dev` and 34% in `release`
(`-p ui-platform`, 2026-11-17, with `build.build-dir` enabled). `rust-version =
"1.99"` still states the language floor; bump the pinned date deliberately,
because an unpinned `nightly` moves under you.

## Profiles

`Cargo.toml` sets these deliberately:

```toml
[profile.dev]
debug = false
opt-level = 1

[profile.test]
debug-assertions = true
```

- Debug builds carry no debug info and optimise lightly: `cargo build` is for
  fast iteration, not for judgement.
- Any number about speed, frame time, or memory comes from `--release`. Debug
  numbers are not comparable, not even to each other.
- `profile.test` keeps debug assertions on. A test that only fails with them
  is telling you something about an assumption, not about a release bug.

## Features

| Feature | Turns on | When you need it |
| --- | --- | --- |
| `ui-text-fontique/system` | The OS font stack (fontconfig, DirectWrite, CoreText) | Any test or example that shapes real text |
| `ui-platform-wayland/system-fonts` | The same, through the Wayland backend | Examples that draw text in a window |
| `ui-platform-wayland/gpu` | `ui-paint-gpu` (the GL device), `ui-gpu-egl` and the dma-buf presenter | The GPU path; off for CPU-only work |
| `ui-platform/profile` | `hotpath` timing in every frame phase | Frame profiling only |
| `ui-platform-wayland/profile` | The same in the present path, plus `ui-paint-gpu/profile` | Present-path profiling only |
| `ui-paint-gpu/profile` | `hotpath` timing inside the renderer's passes | Reached through the wayland crate |
| `ui-paint-gpu/egl-probes` | The GL probes under `examples/`, which need a real EGL context | Running `gpu_scene`, `gpu_image_scene` and their siblings on unix |
| `ui-gallery/profile` | `ui-platform/profile`, plus the gallery's own frame report | `--example gallery --features profile` |

`ui-text-fontique` has `default = []`. The system font stack is opt-in because
a test binary should not depend on the machine's fonts unless it is a test
about fonts.

> **Wrong path:** running the suite without `ui-text-fontique/system` makes
> text shaping return no clusters. Text indices become 0, and selection, caret,
> and IME tests fail or, worse, pass by comparing 0 to 0.

## What builds where

- `crates/*` are workspace members; `crates/assets` is excluded because it
  holds fonts, not a crate.
- Examples live beside the crate they exercise (`crates/<crate>/examples/`).
  Keep one only if a design document or a script names it, or if it asserts an
  invariant that needs a GPU, a compositor, or a pair of backends to reach
  ([Where tests live](../conventions/tests.md)). Anything else is demonstration
  drift: archive it to `old/examples-cleaned-<date>.tar.gz` and delete it —
  there is no version control to undo the deletion. The two gallery front-ends
  are `gallery` (all control sets) and `winui_gallery` (Fluent only).
- Platform backends are only reachable through a feature. A build without
  `gpu` still compiles the portable crates and the CPU presenter.
- A backend whose platform is absent builds as an empty library: `ui-gpu-egl` and
  `ui-platform-wayland` gate their bodies with `#![cfg(unix)]` and declare their bindings in
  a `[target.'cfg(unix)'.dependencies]` table, the way `ui-gpu-d3d11` and
  `ui-platform-win32` do for Windows. Both halves are needed; either one alone leaves the
  workspace unbuildable on the other OS.
- An app names `ui-host`, never a backend: its target tables choose one and two `pub use`
  lines re-export it, so an app's manifest and source mention no OS.
- `ui-demo` is a configuration layer over `ui-host`: the faces an app carries, the wallpaper,
  and the scripted-input knobs. Opening the window, choosing the presenter and pacing the loop
  belong to the host, so every demo front-end runs wherever `ui-host` has a backend.

## Clippy

`cargo clippy --workspace --all-targets` is the lint command. Existing lint
debt has a recorded baseline; do not add to it. Do not add `#[allow]` to
silence a lint that is right about the design: fix the design, or record the
exception with the baseline.
