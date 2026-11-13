# Documentation Conventions

Editorial structure: what gets documented, where it goes, and how a page is shaped. Voice and
per-type patterns are in `../.rules`; agent mechanics are in `../AGENTS.md`. Both sit outside
`src/`, so they are not book pages and must be referenced in code formatting, never linked.

## What gets documented

### Document

- A new or changed engine contract: what the engine guarantees, and the trap it prevents.
- A new control, or a control reaching a new completion state.
- A decision that replaces an earlier approach, written as a correction in the owning design doc.
- A bug whose root cause was not local to the line that looked wrong: a correction in the owning
  design doc.
- Verification recipes: commands, environment variables, and the order they must run in.
- Terminology that a contributor would otherwise have to infer from the code.

### Skip

- Refactors with no behavior change.
- Test-only changes, unless the test is itself the evidence for a contract.
- Internal renames.
- Formatting and typo fixes.
- Anything the compiler or the layering test enforces mechanically. Point at the test instead.

## Doc types

| Type | Question it answers | Lives | Lifetime |
| --- | --- | --- | --- |
| Design | How does this subsystem work, why this way, and what did it get wrong? | `engine/`, `controls/` | Revised as the system changes |
| Corrections | What did we believe, what happened instead, and what catches it now? | a section inside the owning design doc | Appended, grouped by contract area |
| Reference | What are the exact values, gaps, and sources? | `reference/` | Regenerated or appended |
| Progress | What is done and what is next? | `controls/` | Rewritten, never appended indefinitely |

There is no ADR directory, design-error log, lesson collection, or incident page. A mistake, and the
decision that replaced it, are written into the design document of the subsystem whose contract they
changed. Anything that cannot be told that way is a rule in root `AGENTS.md` or a line in the
workflow page. The reason is retrieval: the person touching a subsystem reads its document before
changing it, and nobody reads a table of hundreds of rows that mixes unrelated subsystems.

## Page versus section

Create a page when:

- The topic has its own contract, its own API, or its own failure modes.
- Contributors would search for it by name.
- It needs its own verification section.

Add a section to an existing page when:

- The fact belongs to a contract that already has a page.
- It is a status row, an option row, or one more gap in a table.
- It is a correction or a rejected approach: these are always sections, never pages.
- Splitting the page would leave two pages describing one invariant.

## Page anatomy

1. **Title** — the question the page answers, not the name of a file.
2. **Opening paragraph** — what the page covers, in one or two sentences. No history, no preamble.
3. **Body** — the pattern for the doc type, in the order the reader needs it.
4. **See also** — related pages, one sentence each on what the reader gets there.

Every page states its status in the body when part of the contract is missing. A page that
describes planned behavior in the present tense is wrong even if the plan is right.

## Anchors and headings

- Use `##` for sections and `###` for subsections. Two levels are enough; a third level usually
  means the page is two pages.
- Add an explicit anchor only to a heading that is a link target:
  `## Frame demand {#frame-demand}`. Ids are lowercase with hyphens and unique per page.
- Do not number headings. Numbers make insertion a renumbering job and break external links.

## Cross-linking

- Relative paths with the `.md` extension, from the linking page.
- Link to a section by anchor, not by repeating its content.
- Every page is reachable from `SUMMARY.md` and from at least one prose page.
- A design document links to the subsystem pages it depends on; it does not repeat their contracts.

## Keeping documents true

- A document that contradicts the code is a bug in one of them. Fix the document or delete it; do
  not leave a stale page with a warning on top.
- When a design changes, revise the Contract and append a paragraph to Corrections. The paragraph
  records what was believed and what changed; do not rewrite history so the old text looks right.
- Once a fix is pinned by a test, the correction keeps the test name, not the debugging route.

## Quality checklist

Before a documentation change is complete:

- [ ] The page answers one question, stated in the title.
- [ ] The opening paragraph says what the page covers and what it does not.
- [ ] Every claim about the legacy project or an external framework is cited or `[unverified]`.
- [ ] Every measurement states its conditions.
- [ ] Corrections are prose paragraphs, each naming what catches the regression now.
- [ ] Verification names the test, not "tests pass".
- [ ] Status is stated when part of the contract is missing.
- [ ] `SUMMARY.md` has an entry for the page, and no entry points at a missing file.
- [ ] Every code fence has a language.
- [ ] No link points into `old/`.
- [ ] No page duplicates a contract owned by another page; it links instead.

## Gold standard examples

See `../.doc-examples/` for the design document template, including its `## Corrections` section.
Use it as the starting shape, not as prose to copy.
