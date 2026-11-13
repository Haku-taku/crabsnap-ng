# Documentation

The book lives in `docs/src/` and is built by mdBook. It is the map of the
workspace: how a subsystem works, what it guarantees, and which paths were
already found to be wrong. Code comments answer "why this line"; a page here
answers "what do I need before I touch this".

## Where a fact goes

| Content | Home |
| --- | --- |
| A trap that changes how you edit | root `AGENTS.md` |
| How a subsystem works, and why | that subsystem's page under `engine/` |
| An approach that failed | `## Corrections` on the owning page |
| A command, knob, or test recipe | `development/` |
| A control's status | `controls/completion.md` |
| An external spec or crate | `reference/` |

A page that is not in `SUMMARY.md` is unreachable. Add the entry in the same
change that creates the file, and delete it with the file.

## The rules

Chapters are written in English, in prose. Each subsystem page should open
with the decision a reader has to make, name the contract, and then list traps
as a two-column table. History is a `## Corrections` section of one paragraph
per mistake: what we believed, what happened, what catches it now. There is no
ADR directory and no separate lesson log.

Mechanics and voice rules live outside the book:

- `docs/AGENTS.md` — `SUMMARY.md` format, placement, evidence discipline.
- `docs/.rules` — voice, page anatomy, terminology, status vocabulary.
- `docs/.conventions/CONVENTIONS.md` — what is worth documenting at all.

## Preview and check

```sh
mdbook serve docs
```

Before committing a page: build the book, follow every link you touched, and
confirm the page names the invariant its traps protect. A page that
contradicts the code is a bug in one of them, not a known issue.

## See also

- [Workflow](./workflow.md): the edit, build and verify loop.
- [Comments](../conventions/comments.md): what belongs in code instead of here.
