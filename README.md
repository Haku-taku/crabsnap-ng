# crabsnap-ng

A UI engine and three control sets, written as a Rust workspace. `ui/` contains the engine
(layout, text, painting, platform backends), the MD3, Fluent/WinUI 3 and Liquid Glass controls,
the gallery examples, and the GPU/wayland backends. The legacy project at `../crabsnap` is
reference material, not a dependency.

## Build and run

Run cargo from `ui/`:

```sh
cargo build
cargo test --workspace --features ui-text-fontique/system
cargo run --release --example gallery
```

The full command table, including the nested-session recipe for graphical checks, is in the
book's [Getting Started](docs/src/getting-started.md) and [Building](docs/src/development/building.md).

## Documentation

The engineering book lives in `docs/src/` (mdBook); `docs/src/SUMMARY.md` is the map.

```sh
mdbook serve docs
```

Start with [Architecture](docs/src/architecture.md) for the crate layers, then the engine page
for the subsystem you are about to change. Each subsystem page ends with `## Corrections`:
what we believed, what the code does, and what catches the mistake now.

## Rules

- [`AGENTS.md`](AGENTS.md) — repository traps only: build, tests, process operations, engine
  gotchas. Read it before editing.
- [`docs/AGENTS.md`](docs/AGENTS.md) and [`docs/.rules`](docs/.rules) — how to change the
  documentation.

Everything in the legacy project is unverified input. Trust it only after reproducing it here,
matching an external authority, or deriving it from first principles.
