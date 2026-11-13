# Documentation Agent Guidelines

Rules for agents that add, edit, or delete pages under `docs/`. Voice and page patterns live in
`docs/.rules`; editorial structure lives in `docs/.conventions/CONVENTIONS.md`.

## Documentation system

The book is built with [mdBook](https://rust-lang.github.io/mdBook/). `docs/src/SUMMARY.md` is the
table of contents, `docs/book.toml` the configuration. There is no preprocessor and no frontmatter
pipeline: what you write is what mdBook renders.

To preview:

```sh
cargo install mdbook
mdbook serve docs
```

### SUMMARY.md format

- Chapters are links: `[Title](./path/to/file.md)`.
- Indent two spaces per nesting level.
- Separate sections with `---`.
- A planned but unwritten chapter is a draft: `[Title]()` with empty parentheses. Do not create an
  empty file for a draft; mdBook skips it.

A page that is not in `SUMMARY.md` is unreachable. Add the chapter entry in the same change that
creates the file, and delete the entry when a page is removed.

### Where content belongs

| Content | Location |
| --- | --- |
| A rule that prevents a specific mistake | root `AGENTS.md` |
| How a subsystem works, why it is shaped that way, and what it got wrong before | that subsystem's design doc under `docs/src/engine/` or `docs/src/controls/` |
| An approach we tried and rejected | the `## Corrections` section of the owning design doc |
| A workflow command or test recipe | `docs/src/development/` |
| A process or tooling trap | `docs/src/development/process-operations.md` |
| An external specification, crate, or cloned source | `docs/src/reference/external.md` |
| A claim imported from the legacy project that is not verified | mark `[unverified]` inline where it is used, and write it into the owning design doc's Corrections once it affects a design |

## History lives with the subsystem

There is no design-error log, no ADR directory, and no incident page. A mistake belongs to the
contract it changed, so it is written into that subsystem's design doc, in the `## Corrections`
section, as prose:

- what we believed, or what the first implementation did;
- what was observed instead, with the input, pixel, or measurement;
- what changed, and why that is the root fix rather than a guard;
- what catches the regression now (a test name, a boundary check, a visual check).

A decision that replaces an earlier approach is a correction with a longer story: it goes in the
same section, not in a separate record. Cross-cutting policy that has no owning subsystem is one
line in root `AGENTS.md`, not a history page.

This is deliberate. A single table of hundreds of rows cannot be found by the person touching the
code, and it interleaves subsystems that never interact. A reader of the layout document should
learn the layout mistakes; a reader of the transition document should not have to scroll past them.

## Writing rules

- Every page starts with a single `#` title naming the question it answers, followed by one or two
  sentences saying what the page covers. No other preamble.
- Prefer updating an existing page over creating a parallel one. Two pages that describe the same
  contract will disagree within a month.
- Keep one idea per paragraph and paragraphs to two or three sentences.
- Use `##` for sections, `###` for subsections. If a page needs `####`, it is probably two pages.
- Use a table for enumerable facts (options, files, status) and prose for reasoning.
- Put a `## See also` section at the end when related pages exist, with a sentence per link saying
  what the reader gets there.

### Section anchors

Use an explicit anchor only for a heading that is a link target:

```markdown
## Frame demand {#frame-demand}
```

Anchor ids are lowercase with hyphens, unique per page. Do not add anchors to every heading.

### Code and callouts

- Every code fence carries a language: `rust`, `sh`, `toml`, `json`, `text`.
- Shell commands use `sh`, in one command per block.
- Callouts are blockquotes with a bold label: `> **Note:**`, `> **Tip:**`, `> **Warning:**`.
- File references in prose use inline code and include the line when it points at one:
  `` `crates/ui-platform/src/view.rs:1142` ``.

### Links

- Relative links with the `.md` extension: `[Transitions](./engine/transitions.md)`.
- Link to a section by its anchor: `[Frame demand](./engine/frame-pipeline.md#frame-demand)`.
- Every page must be reachable from at least one other page. A draft chapter may be linked; the
  link stays unresolved until the page is written, which is visible in the book and acceptable
  during migration. Do not link to a page that is neither written nor listed in `SUMMARY.md`.
- Do not link into `old/`. It is a staging area that will be removed.

## Evidence

- A statement about the legacy project is written "the legacy project claims X (`path:line`),
  unverified" or "verified here: X". Never "the legacy project proves/records/concludes".
- A statement about an external framework names the version and the source (source path, spec URL,
  or crate documentation).
- A design claim names how it was checked: a test name, an external source, or a derivation.
  Anything else is `[unverified]` with the risk stated.
- Measurements carry their conditions: build profile, sample size, machine, and whether the numbers
  came from a nested session (which throttles frames and lacks explicit sync).

## Change classification

Needs a documentation change:

- A new or changed engine capability, contract, or public type.
- A new control, or a control reaching a new completion state.
- A decision that replaces a previous approach, written as a `## Corrections` entry.
- A trap that cost more than one debugging session: a rule in root `AGENTS.md`, or a correction if
  it changed a contract.
- A change to a command, feature flag, or test recipe.

No documentation change:

- Refactors with no behavior change.
- Test-only changes.
- Renames of private items.
- Formatting and typo fixes in code.

## Safety

- Do not delete a page or section that still describes the current system. Update it.
- Do not silently rename or drop a path that other files reference. Search first:
  `grep -rn 'docs/src/' --include='*.md' --include='*.rs' .`
- Do not record speculative designs as implemented. State status explicitly.
- Do not add binary assets (screenshots, fonts, captures) to the repository. Reference them from
  outside it, or paste the measurement instead.
- Do not document code comments or restate the code in prose. Document contracts and reasons.
- Do not open a page for a single incident. If it cannot be told as part of a subsystem, it is a
  rule in root `AGENTS.md` or a line in the workflow page.

## Reporting a documentation change

When a change touches docs, report:

- The pages added, changed, or removed, and why.
- Any claim that could not be verified, and what would verify it.
- Any `SUMMARY.md` entry left as a draft because the content is not written yet.
