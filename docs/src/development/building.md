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
| `ui-gallery/profile` | All of the above, plus the gallery's own frame report | `--example gallery --features profile` |

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

## Clippy

`cargo clippy --workspace --all-targets` is the lint command. Existing lint
debt has a recorded baseline; do not add to it. Do not add `#[allow]` to
silence a lint that is right about the design: fix the design, or record the
exception with the baseline.
