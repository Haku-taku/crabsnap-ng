# Testing

Tests are the executable half of the design documents. A test that no document
refers to, and that no named invariant describes, is probably covered
elsewhere or not worth keeping.

## Run the suite

```sh
cargo test --workspace --features ui-text-fontique/system
```

Add a name filter to run one test or one module:

```sh
cargo test --workspace --features ui-text-fontique/system typing_reaches
```

Without the feature, text shaping returns no clusters and text indices read
as 0. Tests that touch text then fail, or pass by comparing 0 to 0.

Where a test lives, and what makes one worth keeping, is in
[Tests](../conventions/tests.md). A unit test must not need a display, a
compositor, or the real clock; if it does, it belongs in an example or in the
self-test.

## Determinism

- Time is injected. The engine has a scripted clock for this; never sleep to
  advance a frame.
- No unseeded randomness, no filesystem, no network, no environment variables
  in a unit test.
- Build the smallest world the assertion needs. A shared mutable fixture is a
  hidden second test.

## The self-test is the integration suite

`GALLERY_SELFTEST=1` walks every gallery page and asserts behavior that unit
tests cannot see: geometry, hit testing, animation midpoints, selection pixels,
dialog state. It prints one line per check and ends with `PASS`.

- It keeps running for `GALLERY_SECONDS` after printing PASS. "The test ended"
  is not "the process exited"; see
  [Process Operations](./process-operations.md).
- It needs a [nested session](./graphical-tests.md). `GALLERY_DUMP` and the
  self-test must run separately: the self-test switches pages, so a dump would
  capture the last frame of the wrong page.

## Before you add a test

Read [Tests](../conventions/tests.md). The short version: name the invariant,
name the change that would break it, and say why no existing test catches
that. Watch it fail before it passes.

## Reading a failure

- `0 passed` is a filter that matched nothing, not a suite that succeeded.
- A text test that fails only without the feature is a build-configuration
  failure, not a behavior failure.
- A test that fails only in release is usually an assumption about overflow,
  timing, or float precision; it is not noise.
- A visual assertion that fails intermittently is a real race until proven
  otherwise. Run it in the nested session before blaming the self-test.
