# Completion Status

A control is done when six things exist together: its token table, its
module, unit tests, a gallery page, a self-test group, and a green
workspace. Five of six is a control that looks finished and is not.

## The rule

| Evidence | Proves |
| --- | --- |
| Generated or hand-written spec table | The values came from the authority, not from taste |
| Module with `Params` and `Widget` | The control can be built and reports changes |
| Unit tests | Geometry, state reporting and the branch that matters |
| Gallery page with build-time variants | A human can look at every state side by side |
| Self-test group | Synthetic input moves the real tree as intended |
| `cargo test --workspace` and zero warnings | Nothing else broke |

A row is marked done only when all six hold. "The crate has the type" is not
completion; a control with no sample and no self-test has no evidence behind it.

## What the self-test proves

The self-test navigates every gallery page, drives controls with synthetic
pointer and keyboard input, and asserts geometry and ink facts against the
real tree. It fails loudly with exit code 1; on success it prints
`selftest: PASS` and keeps running for `GALLERY_SECONDS`.

What it does not do: compare pixels. A control can pass every assertion and
still look wrong (overlapping text, a clipped shadow). Screenshot review in
a nested session remains a manual step, and a change to a control's look is
not verified until someone has looked.

## Where status is recorded

One row per control in the completion table, grouped by set. Each row names
the residual gaps it still carries, so "done" and "has a known hole" are
distinguishable. A gap that needs engine work is recorded against the engine,
not repeated in every control that waits on it.

The table is only as good as its last audit. When a control is upgraded,
update the row in the same change; when a row claims done and has no sample,
treat the row as wrong and fix it.

## Residual gaps at the last audit

- MD3: the wavy progress ring, blocked on the engine's polygon fill.
- Fluent: `ListView`/`ListViewItem`, the icon-and-label button lockup,
  text-box validation and multiline states, combo-box transitions and
  editing, and the `NavigationView` top mode.
- Glass: menu morph and sub-menus, card glow and width limits, popover arrow
  and scale, list-tile editing states, text-field multiline, and toast
  auto-dismiss.
- Inventory drift: modules that no gallery page or self-test uses, and
  controls with no unit test file. Both are invisible in a status table that
  counts modules.

## Corrections

**Status rows outlived the code.** Several rows were marked done because the
module existed, while the gallery had no sample and the self-test no group for
it; a later audit found divider and badge in exactly that state. The
completion rule above now names the sample and the group as evidence, because
the module alone was the thing that lied.

**Old status documents pointed at gaps that had already closed.** They still
said the gallery's system pages were blank, that search had no expanded view,
and that the nav rail had no expressive variant, all of which had landed. A
status document that is not re-read against the code becomes a list of things
not to trust; this page records the rule and the current gaps instead of
accumulating a history of them.

## See also

- [Parity Plan](./parity.md): the remaining work, grouped by cause.
- [Testing](../development/testing.md): what the test layers cover.
- [Graphical Tests](../development/graphical-tests.md): how to look at a
  control.
