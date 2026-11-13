<!--
  TEMPLATE: Design document

  Use for a subsystem contract that will be revised as the system changes.
  The order matters: problem before comparison, contract before implementation,
  corrections before verification. A reader should be able to stop after the
  contract and still know what the engine guarantees.

  Corrections replace ADRs, lesson logs, and incident pages: every mistake and
  every rejected approach belongs to the subsystem whose contract it changed.

  Replace every angle-bracket placeholder. Delete sections that do not apply.
-->

# <Subsystem>: <what it guarantees>

<One or two sentences: what this page covers and what it deliberately leaves to another page.>

## Problem

<What cannot be expressed today. Include the failing call site or the declaration that has no
effect, with `path:line`.>

## Comparison

| Framework | How it solves this | Source |
| --- | --- | --- |
| <name> | <mechanism, one clause> | <source path, version, or spec URL> |

<What the comparison rules out or forces. Name the invariant the frameworks agree on, not a
feature list.>

## Contract

```rust
<the declaration or call that expresses the behavior>
```

- <Invariant 1: what the engine guarantees, stated so a test can pin it.>
- <Invariant 2.>
- Deliberately not covered: <what callers must do themselves, and why.>

## Implementation sites

| Location | Responsibility |
| --- | --- |
| `crates/<crate>/src/<file>.rs` | <what this file owns> |

## Corrections

<One paragraph per correction, in the order it was found: what we believed, what was observed
instead, what changed, and what catches it now. Name the test that pins it. Update the Contract
above in the same edit when a correction changed it. Add `###` sub-headings by contract area once
this section passes a screen; never turn it into a table.>

## Verification

| Test | What it pins | What breaks it |
| --- | --- | --- |
| `<test_name>` | <the invariant in the test's own words> | <the line or call whose removal turns it red> |

<Where the test lives, and whether a visual check is also needed.>

## Status

- Implemented: <what exists today>
- Partial: <what works for part of the contract, and which part is missing>
- Planned: <designed, not built>

## See also

- [<Related page>](./<path>.md): <what the reader gets there>.
