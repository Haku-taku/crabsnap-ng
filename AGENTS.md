# AGENTS.md

Rules for working in this repository. Every agent session reads this file, so it holds traps only:
things a careful engineer would still get wrong. Maps (architecture, history, reference material)
live in `docs/src/`; this file points at them instead of repeating them.

## Rust coding guidelines

- Prioritize correctness and clarity. Speed and memory are secondary unless the task is about
  performance.
- Comments explain *why*, never *what*. Do not restate the code, keep a changelog, or draw tables
  and banners. A comment block is at most two lines.
- Prefer implementing functionality in existing files. Add a file for a new logical unit, not for a
  few helpers.
- Do only what the task asks. Do not add features, abstractions, or refactors nobody requested.
- Avoid `unwrap()`/`expect()` outside tests and examples; propagate with `?`, and use `match` where
  the caller needs a specific fallback. Indexing that can panic is the same bug in disguise.
- Never discard an error silently. Propagate it with `?`, log it, or write one clause saying why
  dropping the result is correct.
- Full words for identifiers; no `q`, `cfg_`, `tmp` spellings.
- Do not add new `mod.rs` files; use `src/some_module.rs`. The existing ones are legacy. New crates
  name their library root in `Cargo.toml` (`[lib] path = "..."`) rather than defaulting to `lib.rs`.
- User-visible strings go through Fluent i18n; never inline literals in controls.
- No compile-time `cfg` to choose between crates or backends. Capabilities are detected at runtime.
- After editing, read the change back. A tool call that reports success says the write happened, not
  that the content is what you meant.

## Tests

- Before adding a test, answer three questions: which invariant does its name state, what change
  makes it fail, and why no existing test catches that. "It guards against regressions someday" is
  not an answer; ask the user when unsure.
- Test the major branch geometry (fallback chains), a complex algorithm against expected output, and
  regressions of *major* bugs. Do not test every method, snapshot trivial output, assert a constant
  against itself, or duplicate a check at two layers.
- Watch each new assertion fail for the right reason before it passes. A test that has never been
  red has not been shown to test anything. Never weaken an assertion to make a refactor pass: decide
  whether the refactor or the assertion is wrong, and record which.
- Inject time and input. Never sleep to advance a frame and never read the real clock; drive the
  scripted clock. A unit test must not need a GPU, a Wayland compositor, or a display.
- Tests that depend on text shaping need `--features ui-text-fontique/system`. Without it shaping
  yields no clusters and text indices silently read as 0.
- The suite is a budget, not an accumulation: a test that pins one situation's look belongs in the
  token table or the design doc, and a fix is recorded in the owning doc's `## Corrections` before
  a test pins its branch. Full rules: `docs/src/conventions/tests.md`.

## Build

Run cargo from `ui/`.

| Purpose | Command |
| --- | --- |
| Build | `cargo build` |
| Full tests | `cargo test --workspace --features ui-text-fontique/system` |
| Zero warnings | `cargo build --workspace --tests --examples` |
| Lint | `cargo clippy --workspace --all-targets` |
| Layering | `cargo test -p ui-platform --test platform_boundary` |
| Look / speed | always `--release`; debug only for fast compile loops |
| Frame profile | `cargo build --release --example gallery --features profile` |

- While iterating, use `cargo check`, and test only the crate you changed
  (`cargo test --workspace --features ui-text-fontique/system -p <crate>`; the feature lives on
  the workspace, not on a single package). The full workspace test run and `cargo build` belong to
  the final verification, so a round of edits does not pay for linking the whole tree.
- Zero warnings is the bar, not a goal. `0 passed` is a green run over an empty set, not evidence.
- Graphical tests run only inside a nested session: the main session's compositor may be in use.

```sh
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=slider GALLERY_SECONDS=300 ./ui/target/release/examples/gallery &
scripts/nested-session.sh shot /tmp/a.png
scripts/nested-session.sh stop
```

- `GALLERY_SELFTEST=1` prints PASS but keeps running for `GALLERY_SECONDS`; do not assume it exits.
  `GALLERY_DUMP` and the self-test must run separately: the self-test switches pages.

## Process operations

- Never find or kill a process you just started with `pgrep -f`/`pkill -f`: the pattern also matches
  the shell and wrapper that issued the command, so the wrong pid dies and the failure looks like
  something else.
- Take the pid from `$!` at launch. When a wrapper may have forked, walk
  `/proc/<pid>/task/<pid>/children`, checking `/proc/<pid>/comm` at each step.
- Kill by explicit pid after reading `ps -eo pid,args`. Re-check `/proc/<pid>/comm` before killing a
  pid read from a file; pids are reused.
- If a pattern is unavoidable, bracket one letter (`'examples/galler[y]'`) and cover every launch
  path.

## Engine traps

- `AppLoop::dispatch` returns "routing changed", not "a frame is needed". Frame demand is
  `take_dispatched()`. A gesture that changes no pixels immediately but must still run every frame
  (a selection drag) belongs in `pointer_affects_frame`.
- Timestamps for an event that starts an animation come from `FrameClock::live()`. The engine clock
  advances only when a frame is built, so `frame_now` during idle is the distant past.
- Animation is declared, not tweened in app code. A widget-side tween has no start value on a node
  that did not exist last frame, so it draws its end state immediately. Use `Presence`,
  `transition_size`, and the per-channel `scaled_over` clock.
- A change is only real when the wiring line exists: adding a field is not applying it. When a probe
  does not fire, either the code did not run or the probe sits in the wrong function.

## Trust

- Everything in the legacy project (`/home/ollie/git/crabsnap`) is unverified input: notes, plans,
  and comments are anecdotes; source is in use but may be wrong. Trust it only after reproducing it
  here, matching an external authority, or deriving it from first principles. Mark anything else
  `[unverified]` with the risk.
- Prefer mature frameworks (GPUI, egui, Qt, GTK, Flutter, Skia, parley) over the legacy
  implementation. Read their source for the problems and invariants they solve, not their layout.
- Before changing a subsystem, read its design document, including the `## Corrections` section.

## Layering

- Boundaries are enforced by `ui/crates/ui-platform/tests/platform_boundary.rs`, not by review.
  Portable crates must not contain `std::os::unix`/`windows`, `cfg(target_os)`, `libc::`,
  `OwnedFd`, or `std::process`, and must not depend on platform crates.
- Every crate is classified `PORTABLE` or `PLATFORM_BOUND`; a new platform backend is added to the
  latter with a written reason. Never widen `PORTABLE`.
- Dependencies point from lower layers to higher ones only. Facade crates contain `pub use` only.

## Documentation

- Documentation lives in `docs/src/` (mdBook). Writing rules: `docs/.rules`. Structure:
  `docs/.conventions/CONVENTIONS.md`. Agent-facing rules: `docs/AGENTS.md`.
- Design documents are verified like code: state how each claim was checked. History lives in the
  design document's `## Corrections` section, one paragraph per mistake or rejected approach, each
  naming what catches it now. There is no separate decision log or lesson list.

## Rules hygiene

This file is read by every agent session. Keep it high-signal.

- Editing or clarifying an existing rule is welcome. A new rule must be non-obvious, repeatedly
  encountered, and specific enough to act on.
- Do not add architecture descriptions (module layout, data flow, key types) here. They go stale and
  belong in `docs/src/`.
- Do not edit this file in the middle of unrelated work. Put the proposed rule in your report and
  let the reviewer decide.
