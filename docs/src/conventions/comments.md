# Comments

A comment exists to explain **why** the code is shaped the way it is. If the
code already says it, the comment is noise that will go stale.

## Budget

A comment block (consecutive `//` or `///`) is at most two lines. A comment
that needs three lines is describing a design, and designs live in
[Engine](../engine/frame-pipeline.md) or
[Controls](../controls/control-sets.md), not in source.

The block is prose, not a list, table, banner, ASCII diagram, or stack trace.
Protocol excerpts belong in the design page that cites them.

## What a comment is for

| Purpose | Example |
| --- | --- |
| A non-obvious constraint | `// Must run before commit: the compositor drops late damage.` |
| An invariant the reader could break silently | `// Sum == 1.0 by construction; do not renormalize.` |
| An external protocol or API trap | `// zwp_text_input_v3 buffers until "done"; apply nothing before it.` |
| A deliberate deviation that would look like a bug | `// Affine on purpose: two scale centres compose to a translation.` |

## What never goes in a comment

- Restating the code, the type name, or the signature.
- Change history: dates, "changed to X", "previously used Y", who fixed what.
- Bug narratives, ticket numbers, or links to a review.
- Commented-out code. Delete it; the history is in the patch.
- A `TODO` with no issue or no owner.
- Decorative separators longer than one line.
- A second copy of a design page. Leave one line pointing at the page instead.

## When the explanation does not fit

Choose one, in this order:

1. **A test.** If the invariant can be stated as a named assertion, write the
   test; its name carries the explanation.
2. **The subsystem's design page.** If it is a contract, an invariant, or a
   correction, it belongs in `docs/src/engine/` or `docs/src/controls/`, and
   the code keeps at most a one-line pointer.
3. **Nothing.** If neither applies, the comment is probably restating the code.

There is no separate note or decision file to fall back on. A correction with
no owner page is a sign the owning page is missing.

## User-visible strings

Never inline. Every user-visible string goes through Fluent i18n, including
error text that ends up in a control. See
[Text and Input](../engine/text-and-input.md) for the catalog side.
