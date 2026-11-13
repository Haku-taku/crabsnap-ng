# Tests

A test is a behavior claim a reader can falsify. If the claim is not visible
from the name and the body, the test is documentation debt wearing a safety
net.

## Three questions before adding one

1. Which invariant does it pin? Put it in the name.
2. What change would make it fail? If you cannot say, it is not a test.
3. Why does no existing test catch that? If one does, extend it instead.

Do not add a test whose answer to any question is "it guards against
regressions someday". When the need is real but the shape is unclear, ask.

## What a test is worth

A test earns its place by pinning a mechanism other code depends on. The
mechanisms are:

- **Architecture**: a fallback chain, an ownership rule, a state machine, a
  cache key, an ordering between phases. The failure is a wrong shape, not a
  wrong number.
- **Algorithm**: expected output for representative input, on the cases where
  the formula has a branch or a clamp.
- **Token contract**: a generated table matches its export, and a control set
  reads the role, size, or motion slot it names. The wiring is the claim, not
  the literal value.
- **A correction**: a bug that changed a contract, recorded in the owning
  design document's `## Corrections`.

The suite is curated, not accumulated. Its size is a budget: roughly two to
three hundred tests for this workspace. A new test is paid for by naming a
mechanism no current test pins, or by removing one that no longer does.

## What is not worth keeping

These are shapes, not a list. Recognise the shape and drop the test.

- **One situation's appearance.** "In state X the control is this colour, this
  radius, this padding" pins a value that lives in a generated token table and
  is read from there. The token table and the design document are where a
  reader finds it; a test freezes it where nobody looks.
- **A fix that changed data.** If the bug was a wrong constant, duration, or
  colour, the corrected value belongs in the design document and the token
  source. Write a test only when the bug came from a branch, an ordering, or
  an ownership rule the engine now follows, and write it as that rule.
- **Scaffolding.** Getters, field round-trips, "the struct holds what was put
  in it", a generated table compared with the data it was generated from, a
  one-line formula with no branch.
- **A second copy.** The same invariant asserted at a higher layer with more
  machinery. Keep the layer where the invariant is decided and delete the
  echo.

Control-set tests decay fastest: a control's look changes every time the
export is refreshed, while the rule "a control reads its role from the pack"
never does. Prefer the second.

## Bug fixes and corrections

A fix is not finished by a green test. Ask what the engine got wrong, write it
into the owning design document's `## Corrections` with what was observed, and
name there what catches it now. Then decide whether a test pins anything a
correction cannot: a branch that code can take again, an ordering, an
invariant a future refactor would silently invert. A test that only restates
the corrected value is not that pin.

## Where tests live

| Layer | Location | Covers |
| --- | --- | --- |
| Unit | `#[cfg(test)]` module in the crate | One mechanism's contract |
| Property / differential | Same crate | "Incremental == full" style invariants, seeded |
| Cross-crate contract | `crates/<focused>/tests/` | Only a contract between two crates |
| Visual | `examples/`, the gallery self-test | GPU, compositor, or human judgement |

An example that asserts an invariant is a test with a heavier harness, and the
Visual row is where those live. These are kept for exactly that reason:

| Example | Invariant |
| --- | --- |
| `ui-paint-gpu/gpu_scene` | One scene, both backends, four agreements; a violation exits non-zero |
| `ui-paint-gpu/gpu_cached_scene` | A cached subtree keeps its pixels until damage lands inside it |
| `ui-paint/effects_cpu` | The effect chain's four invariants |
| `ui-platform/text_wrap` | No glyph paints outside the text node that laid it out |
| `apploop_text` | Text reaches the screen through the real `AppLoop`, not a hand-rolled pipeline |

The remaining examples are demonstrations. One is kept only when a design
document or a script names it — see [What builds where](../development/building.md).

A large module keeps its tests beside it, not inline: `src/foo.rs` ends with
`#[cfg(test)] mod tests;` and the body lives in `src/foo/tests.rs`, split into
submodules when it outgrows a screen. One `#[cfg(test)]` module per file, and
never more: a second one means either the file is two modules or the tests
belong in the sibling file. Test helpers live in the same file as the tests
that use them and are deleted with the last one; an unused helper left behind
is a warning, and warnings are errors here.

## Atomicity

- **One behavior per test.** Three arrange/act groups are three tests in one
  coat, unless the sequence itself is the invariant (a state machine).
- **No shared mutable fixture.** Each test builds its own smallest world.
  Helper functions are fine; helpers with hidden state are not.
- **Deterministic inputs only.** No wall clock, unseeded RNG, filesystem,
  network, or environment variables. Inject time and ports as parameters. The
  engine provides a scripted clock.
- **Assert the thing, not a proxy.** Assert the returned rectangle, not that
  a function was called.
- **The name is the specification.** `pad_widens_beyond_the_dirty_rect` beats
  `test_effect_2`.

A test that makes you open three files to understand a failure is not atomic.

## Changing or deleting a test

Deleting a low-value test is a normal, approved action; say so in the change
description. Do not keep a test because it has always been there. Deleting a
test whose correction is not yet written down is a documentation gap: write
the correction first, then delete.

Never weaken an assertion to make a refactor pass. Decide whether the refactor
or the assertion is wrong, and record which.

## Review checklist

- [ ] The name states an invariant about a shared mechanism.
- [ ] There is one reason it can fail.
- [ ] It asserts wiring, a branch, or an algorithm, not one situation's look.
- [ ] If it came from a bug, the correction is in the owning design doc and the
  test pins the branch rather than the value.
- [ ] No IO, clock, RNG, or environment variables.
- [ ] It failed for the right reason before it passed (temporarily broke the
  code, restored it).
- [ ] No constant-folded tautology can satisfy it.
- [ ] It sits at the lowest layer that can express the invariant.
- [ ] No other test already covers it, and its file holds only one
  `#[cfg(test)]` module.
