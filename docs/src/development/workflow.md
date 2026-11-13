# Workflow

The loop for any change: read the owning design, edit with tools that fail
loudly, build, test red before green, read the change back, and only then
judge it by eye. Each step below exists because skipping it cost hours at
least once.

## 1. Read before you edit

Read the design page of the subsystem you are about to touch, including its
`## Corrections` section. The correction you are about to repeat is usually
written there, next to the test that now prevents it.

## 2. Edit with tools that fail loudly

Use file editing tools. Do not assemble files with `cat > file <<EOF`,
`python -c` rewrites, or `sed -i`: quoting, backticks, escapes, and failed
subprocesses make those fail silently, and each failure mode looks different
from the next. An editing tool that fails says so and shows the file.

Copying an existing Markdown block with `sed -n 'A,Bp'` is the one exception.
Diff the result byte for byte after the move.

## 3. Build until there are no warnings

```sh
cargo build --workspace --tests --examples
```

A warning is a change you did not finish. Do not leave it for the next session.

## 4. Watch a new assertion fail first

Temporarily break the code the test is supposed to protect, watch the test go
red, restore the code, watch it go green. A test that has never been red has
not been shown to test anything.

Never weaken an assertion to make a refactor pass. Decide whether the refactor
or the assertion is wrong, and write down which.

## 5. Read the change back

After an edit, confirm it landed:

```sh
grep -c '<expected text>' <file>
```

A tool call that reports success says the write happened, not that the content
is what you meant. `0 passed` is a green run over an empty set, not evidence.

## 6. Judge visuals in a nested session

Only the [nested session](./graphical-tests.md) produces a screenshot that
means anything: your own compositor is in use, and a window may map after the
scripted input has already fired.

## Three failure modes to check before debugging

1. **Landed is not wired.** A new file that no `mod` declares, a field that
   nothing reads, a command that was never added to a dispatch table. After
   any move, add, or rename, ask where the wiring line is.
2. **A plausible mechanism is not a fact.** Run it if it can be run; say
   "I infer" if it cannot.
3. **A probe that does not fire has two causes.** The code did not run, or the
   probe sits in the wrong function. `grep -c '<function>'` before you trust
   the probe.

## Separate the promise from the means

A review of any design asks two questions of every field or parameter:

1. **Does the engine read this to make a correctness decision?** If not, it is
   not engine metadata. An edge policy or a filter's preferred padding belongs
   to the implementation that chose it.
2. **Is this a guarantee, or this implementation's current choice?** If the
   latter, keep it separate from the guarantee. A radius that happens to be
   `ceil(3σ)` today is not a contract for tomorrow's kernel.

And for every assertion of the form "these must be equal", ask: if the two
sides use different mechanisms, can it still hold? If not, replace equality
with an invariant that both mechanisms owe — area conservation instead of
identical edge pixels, monotone coverage instead of identical alpha.

The questions are cheap. In the review that produced this list they found two
fields that had frozen an algorithm's choices into engine metadata, and one
cache key that omitted the colour domain it sampled in.

## Where the rules live

| Topic | Page |
| --- | --- |
| Commands, profiles, features | [Building](./building.md) |
| Test layers and the suite | [Testing](./testing.md) |
| Screenshots and scripted input | [Graphical Tests](./graphical-tests.md) |
| Frame timing and exit stalls | [Profiling](./profiling.md) |
| Starting and killing your own processes | [Process Operations](./process-operations.md) |
| Comment budget | [Comments](../conventions/comments.md) |
| What a test must prove | [Tests](../conventions/tests.md) |
