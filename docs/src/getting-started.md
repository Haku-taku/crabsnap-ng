# Getting Started

`ui/` is a Rust workspace: a UI engine (layout, text, painting, platform
backends) plus three control sets — MD3, Fluent (WinUI 3), and Liquid Glass.
The engine is a rewrite. The legacy project at `/home/ollie/git/crabsnap` is
reference material, not a dependency, and its claims are unverified.

This page takes you from a checkout to a running gallery and a green test run.
It does not explain the engine; read [Architecture](./architecture.md) for
that.

## Prerequisites

- The toolchain in `rust-toolchain.toml` (edition 2024,
  `rust-version = "1.99"`).
- A Wayland session for anything that opens a window. Unit tests need neither
  a display nor a compositor.
- `crates/assets` is excluded from the workspace: it holds fonts, not a crate.

## Build and test

Run cargo from `ui/`:

```sh
cargo build
cargo test --workspace --features ui-text-fontique/system
```

The feature is not optional once text is involved. Without it the font provider
yields no clusters, `index_at` returns 0 for every position, and a text test
passes for the wrong reason.

Zero warnings is the bar:

```sh
cargo build --workspace --tests --examples
```

## Run the gallery

| Target | Command | What it shows |
| --- | --- | --- |
| `gallery` | `cargo run --release --example gallery` | all three control sets, one page per control |
| `winui_gallery` | `cargo run --release --example winui_gallery -p ui-widgets-fluent` | the Fluent set in a WinUI-like shell |

`GALLERY_PAGE=<tag>` opens one page directly; `GALLERY_SECONDS` sets the
lifetime (default 300). Use `--release` for anything judged by eye or by
speed.

All three run on any backend `ui-host` provides, Wayland or Win32.

> **Warning:** Do not run a graphical test in your own session. It takes the
> compositor you are working in. Use the
> [nested session](./development/graphical-tests.md) instead.

## First five minutes

1. `cargo test --workspace --features ui-text-fontique/system` — the full suite.
2. `cargo test -p ui-platform --test platform_boundary` — the layering contract.
3. `cargo build --workspace --tests --examples` — must print no warnings.
4. Read [Workflow](./development/workflow.md),
   [Comments](./conventions/comments.md), and
   [Tests](./conventions/tests.md). They change how you edit, not just what
   you know.
5. Before touching a subsystem, read its design page under
   [Engine](./engine/frame-pipeline.md) or
   [Controls](./controls/control-sets.md), including its `## Corrections`
   section.

## Where to go next

| Question | Page |
| --- | --- |
| How is the workspace layered? | [Architecture](./architecture.md), [Crate Map](./crate-map.md) |
| How do I add a platform backend? | [Platform Boundary](./platform-boundary.md) |
| How does one frame work? | [Frame Pipeline](./engine/frame-pipeline.md) |
| How do I test a visual change? | [Graphical Tests](./development/graphical-tests.md) |
| What is known to be missing? | [Engine Gaps](./reference/engine-gaps.md) |
| What terms mean what? | [Glossary](./glossary.md) |
