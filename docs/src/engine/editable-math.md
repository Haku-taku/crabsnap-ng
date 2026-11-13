> **Superseded in part (2026-11-07).** The geometry gap described below was
> closed by the mathstack replacement: `math-core` owns the model, node spans
> and caret/empty-slot geometry, and `ui-math` now lays formulas out through it.
> The surveys and the mathtex measurements remain as the research that led to
> that decision; see [Formula Layout](./math.md) for the current pipeline.

# How should a formula be edited?

Inline and display formulas are read-only today. Editing them in place needs a
structure, a caret inside it and geometry for both — this page records what the
current pipeline has, what exists already, and which boundary to research next.

## What editing needs

The F6 request has two tiers: edit LaTeX directly against the rendered result,
and click a structure's slot (fraction, root, script, delimiter) to enter it,
with empty slots marked so the next keystroke has an obvious home. Both need the
same five things:

- a structural tree with named slots, not a flat list of glyphs;
- caret paths that address a slot and survive serialization;
- geometry for carets and empty slots in the painted formula's own units, and
  the inverse — a click position mapped back to a caret;
- source spans, so each node is a byte range of the LaTeX and an edit is a
  splice rather than a re-serialization guess;
- an input layer: natural typing (`/`, `^`, `_`, `(`), command completion,
  structure-aware backspace, undo, clipboard.

## What this pipeline already has

The source of truth is the LaTeX string (`ui_math::MathFace::layout`), and
pulldown-latex exposes a public **event stream** beside the MathML renderer it
already uses: `Event::Visual(Visual::Fraction | Visual::Root | Visual::SquareRoot)`,
`Event::Script { ty, position }` and `Event::Begin`/`Event::End` groups
(`pulldown-latex 0.8.0`, `src/event.rs`). Structure is therefore reachable
without parsing MathML.

Geometry is the gap. formulary's `Layout` is a flat item list — glyphs, rules and
backgrounds with positions — and an item carries no identity, so "which node is
this glyph in" cannot be answered from what we paint. Every editor below solves
this by owning its layout; a consumer of somebody else's layout cannot.

## What exists

| Project | Model | Geometry | Status |
| --- | --- | --- | --- |
| `mathtex-editor-core` / `-session` 0.4.1 | editable tree, `CaretPath`, `SpanMap` from nodes to LaTeX bytes, host boxes (`\hostbox{N}`) | host typesets; the editor returns caret, selection and empty-slot rects | Apache-2.0 / MIT; headless core, keymap, undo, clipboard; typeset by `mathtex` |
| `ratex-gpui` 0.4.x | MathQuill-style `Row`/`Atom` tree | own `geometry` module maps tree + layout to caret and slot rects | MIT-family; the editor core is GUI-free, the view is gpui-only |
| MathLive | MathJSON AST, MathML/LaTeX export | owns its layout; `getElementInfo`, `offsetFromPoint` | MIT, active, 2187 stars (2026-11) |
| MathQuill | `Cmd` tree with inner fields | owns its layout | MPL-2.0, 2870 stars, long dormant, "resuming active development" in its README |
| KaTeX, MathJax | render only | — | not editors |

Two Rust projects matter here. `mathtex-editor-core` is a headless editor: the
host sends commands, reads values, and **typesets itself**; the editor supplies
the caret, selection and empty-slot rectangles to paint on top. Its README also
lets host widgets live inside a formula — the same contract our atoms have. The
price is that the host's typesetter must report geometry per node, which
formulary cannot.

## The boundary decision

There are only two honest ways to close the geometry gap:

1. **Adopt a typesetter that reports node boxes** and drive a structural editor
   from it. `mathtex` (the XeTeX engine in Rust behind `mathtex-editor`) is the
   candidate; it would replace formulary for math layout, at least while a
   formula is editable, and possibly everywhere.
2. **Write our own layout tree** over the pulldown-latex event stream and the
   MATH table. That duplicates formulary, and two layouts drift — the drag we
   refused for display lists would return.

Two engines (formulary for static, mathtex for editing) is the third option and
the worst-looking one: the formula would change shape the moment it is focused.
It is only acceptable if mathtex's static output matches, which is unlikely
enough that the first spike should measure it.

## What Desmos does

Desmos' equation editor is MathQuill, which Desmos has funded for years; its
own engineering blog describes the editor half and the SNHU accessibility guide
names the library outright. Reading it settles where the geometry can come from.

MathQuill holds a tree, not a glyph list: `MathCommand` nodes (fraction, root,
script) own `MathBlock` inner fields, and an empty field carries the `mq-empty`
class, which CSS fills at 20 % black (`math.less:92`); the dashed outline that
`\dot`'s inner box gets (`mq-empty-box`) is the only dashed box in the
library. A LaTeX parser (`src/services/latex.ts`) builds that tree from the
source and `latex()` writes it back, so the editor owns the importer that
`mathtex-editor-core` lacks. Keys are translated into "build this structure,
put the caret in that field": `/` opens a fraction with the caret in the
numerator, `^` opens a script, and backspace removes a whole structure from its
end.

The caret and the empty slots are the browser's, not a typesetter's: the tree
templates into HTML spans, each node remembers its element
(`mathquill-command-id`), and hit testing reads `offset().left` and
`outerWidth()` per element. MathQuill never lays out mathematics itself — the
fraction bar and the raised script are CSS boxes. A hidden textarea takes the
real keyboard, IME and mobile input while the visible layer only renders.

Two lessons, and they point in different directions. The interaction contract is
language-independent and worth copying field for field. The geometry does not
transfer: an HTML editor is measured by its host, and a canvas engine has no
host to ask. That is why this page's two options stand.

One cost falls, though. Desmos keeps LaTeX as the only boundary between the
editor and everything else: MathQuill emits LaTeX per edit and a worker parses
and compiles it. Our editor already uses that split, and `pulldown-latex`
already parses LaTeX into an event stream, so the "LaTeX importer" priced above
is a mapping from that stream into a document model, not a parser written from
scratch.

## The spike that decides

Before choosing, measure these on one corpus (fractions, roots, scripts,
delimiters, limits and integrals, matrices, text, operator spacing):

1. Whether `mathtex`'s static output matches `formulary`'s closely enough that
   a formula does not change shape when it gains a caret.
2. How visible the baked-face spacing drift is on that corpus, since a runtime
   face choice needs one format per face.
3. The size of the `pulldown-latex` event → `mathtex-editor-core` document
   mapping: how many event kinds, and which constructs have no counterpart.

The work runs in a throwaway crate outside the repository; nothing is checked in
until the numbers say which option is cheaper.

## What the spike measured

A throwaway crate outside the repository (`mathtex 0.2.0`, `mathtex-editor-core
0.4.1`, release build, this development machine, the system's DejaVu Math TeX
Gyre and a scheme-small TeX Live 2026 tree) answered the four questions.

| Question | Answer |
| --- | --- |
| Build and assets | A format is 4.63 MB and bakes in 7.5 s from a TeX Live tree (scheme-small is 1.2 GB of `texmf-dist`); typesetting then needs no TeX install, only the font files |
| A system MATH face | Yes: `FontLoader` resolves by name and the host chooses the key, so our text pass can hand it the same face bytes it paints with |
| Glyph ids and positions | `Placed::Glyph { font, glyph_id, x, y, source }` and `Placed::Rule { x, y, width, height, source }` in TeX points, plus `surface { width, height, baseline }`; the ids are the face's own (DejaVu gid 453 is `u1D44E`), which is exactly what `blit_glyph(face, glyph_id, …)` takes |
| Editor geometry | `Editor::render(&source, &fragment)` returns the caret, selection and empty-slot rects in points with y down, and `hit_test` maps a point back to a `CaretPath`; on a fraction it reported the caret and both placeholders, and the hit test resolved |

Typeset plus render took 7.7 ms per edit on average over 20 edits, so a
keystroke can afford it.

Two costs the four questions did not cover showed up as well.

**The format bakes the math face's spacing constants.** Serving a different
face through the same format relayouts with that face's advances but keeps the
constants of the face it was baked with (`a^2 + b^2 = c^2`, inline, points):

| Baked face | Served face | Width |
| --- | --- | --- |
| DejaVu Math | DejaVu Math | 64.611 |
| DejaVu Math | Latin Modern Math | 55.017 |
| Latin Modern Math | Latin Modern Math | 53.310 |
| Latin Modern Math | DejaVu Math | 62.451 |

So a formula follows the served face's glyphs and advances, but spacing is
about 3 % off unless the format was baked for that face — and rebaking needs
TeX Live, which an end user of a formula control does not have. Correct
runtime font choice therefore means one baked format per face, and the
repository would carry that binary, tied to the exact engine build that baked it.

**There is no LaTeX importer.** `mathtex-editor-core` documents are JSON and
LaTeX is export-only (`Editor::source`); no crate in the family parses LaTeX
into the model. Our formulas enter as LaTeX strings (Markdown, presets, the
`Math` control's own API), so structural editing needs a LaTeX → document
importer written by us, whatever lays the formula out.

## Where that leaves F6

Tier ① does not depend on any of this: editing the source against a live
render uses the pipeline we have.

## Tier ①, as built and then withdrawn

`ui_widgets::MathField` was a single-line engine text input over the same `Math`
control that draws a read-only formula: the app owned the buffer, an edit arrived
as `Change::Text` on the field's id, and the preview was re-declared from the new
source on the next frame. The engine's text input supplied the caret, selection,
IME, clipboard and text menu; the per-pack wrappers supplied the field's surface,
roles and body size. The gallery's math page gave each column one and its
self-test typed into it.

The field is gone. Tier ② landed first, and keeping both would have meant two
editing stories and three more field surfaces to theme; the source mode returns
as a mode of the Markdown block rather than as a widget of its own. The reason
the preview was the same `Math` control still holds for that mode: an invalid
source does not blank it,
the layout repairs what is incomplete and lays the source out through its
errors, so a mistyped command is a boxed run inside the fraction it belongs to,
with the reason on hover. In an editor this is the common state, not an
exception, and it is why recovery works per run rather than per whole source
([Formulas](./math.md#a-refused-source)).

What tier ① does not give is a caret *inside* a structure: the caret lives in the
source line, so entering a fraction's numerator means placing a text caret
between `{` and `}`, and an empty slot has no mark of its own. That is
tier ②, and it is what the mathtex question above is about.

Tier ② wants a structural model, and mathtex was the only surveyed candidate
whose caret geometry and glyph output fit our text pass. Adopting it would have
meant checking a 4.63 MB engine-tied format into the repository (a binary asset
and a TeX Live bake step for every engine bump), accepting the 3 % spacing drift
for faces other than the baked one, and writing the importer.

## Tier ②, as built

The structural model came from mathstack instead. `math-syntax` parses LaTeX
into a tree with node ids and byte spans, and `math-editor` holds the caret,
selection, empty slots, undo, IME and clipboard over that tree, taking its
geometry from the same layout that paints the formula. `ui-math` exposes the
session as `MathEditor`; the engine keeps one per focused formula behind
`ui_core::MathEdit`, and `Math::editable()` is the control that drives it
([Editing by structure](./math.md#editing-by-structure)). No binary format, no
bake step and no importer are needed, because the parser is part of the same
project as the layout.

## Corrections

**Tier ① shipped as its own widget and was withdrawn.** `MathField` plus three
pack wrappers shipped first, with the gallery self-test typing into it. That
made the tier order wrong: the structural editor arrived while the source field
still owned the gallery's editing demo, and two widgets for one job diverge. The
field and its wrappers are removed; the gallery drives the structural card, and
the source mode will live in the Markdown block as one of its modes. A stale
reference now fails the workspace build.

**Tier ② was scoped as a choice between mathtex and staying open.** The surveys
looked for a crate that owned both a LaTeX importer and caret geometry, found
only mathtex, and weighed its costs as a repository policy question. Writing the
LaTeX parser for mathstack removed the premise: with `math-syntax` and
`math-editor` in the same repository as the layout, the structure, the spans and
the geometry are the ones being painted, at no binary cost. The mathtex
measurements stay as the reason that path was not taken.

**Empty slots are filled, not dashed.** The page described an empty structural
slot as a dashed box, repeated from prose about visual formula editors and never
checked against the library. Reading MathQuill 0.10.1 (`math.ts:665`,
`math.less:92`) shows the empty field takes a 20 % black fill on blur; a dashed
outline exists only on `\dot`'s inner box. The wording is corrected throughout,
and [the spike](./math-editor-spike.md) carries the source lines.

## See also

- [Formulas](./math.md): the read-only pipeline this would extend.
- [Inline Content](./layout-inline-content.md): atoms, the box contract a
  formula editor's host boxes would reuse.
