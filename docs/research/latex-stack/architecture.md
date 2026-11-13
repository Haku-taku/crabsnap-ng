# Target architecture: an owned math stack

A design for a standalone Rust math stack: LaTeX in, deterministic geometry and a
display list out, with structural editing as a first-class output. It exists so
one engine can serve both display and editing, and so every construct has a
source span and a stable identity. The survey behind it is in `survey.md`.

Status: proposal. Nothing here is implemented; numbers are order-of-magnitude
estimates, not measurements of this design.

## Goals

1. **One engine for display and editing.** The tree that lays out a formula is
   the tree an editor moves a caret through. No two-layout drift.
2. **Every construct is addressable.** Byte spans on syntax nodes and glyph
   clusters; stable node ids; named slots; serializable caret paths.
3. **Deterministic geometry.** Same input, font and size produce identical
   boxes on every platform. No browser layout, no runtime text measurement.
4. **Font-agnostic.** Any OpenType font with a MATH table; metrics read from the
   font, with documented fallbacks.
5. **Embeddable.** No I/O, no globals, no host types. Font bytes and a shaping
   callback come from the host; a flat display list goes back.

## Non-goals (first version)

- TeX programming beyond a small macro subset (`\def`/`\newcommand` with `#n`,
  no `\edef`, no delimited parameter text, no catcodes).
- Line breaking inside formulas; page layout; floats.
- MathML import/export; colour; tags and references; `\includegraphics`.
- Full command coverage. The long tail is a maintenance programme, not a phase.

## Shape

Three crates in a workspace, no dependency on any host project:

```
math-core    model tree, spans, resolve, layout, geometry, display list, traits
math-syntax  lexer, macro expander, parser, LaTeX writer      (uses math-core)
math-font    OpenType MATH/hmtx reader, metrics cache         (uses math-core)
```

A host composes all three: it supplies font bytes, a `TextShaper`, a size and a
display mode, and receives a `Layout` it can paint. Time and platform capability
remain the host's.

## The pipeline

```
&str
  │  lexer: tokens with spans; macro expander over token lists with provenance
  ▼
model tree  ── editable: named slots, stable ids, empty-slot objects
  │  resolve: atom class, style level, cramped, stretch requests
  ▼
layout tree ── boxes with width/height/depth; MATH-driven dimensions
  │  index: node id → box, slot → caret rect, point → caret path
  ▼
Layout { display list, geometry, diagnostics }
```

The model tree is the parse tree. Unlike KaTeX's parse nodes it must survive
edits and carry spans; unlike pulldown-latex's events it must have structure.
The layout tree is separate and rebuilt per layout, but every layout node keeps
the `NodeId` it came from, so the two are joined by id, not by re-matching.

## Model

```rust
pub struct Document {
    source: String,               // the text the spans point into
    nodes: Arena<Node>,           // id-stable arena
    root: NodeId,
    diagnostics: Vec<Diagnostic>,
}

pub struct Node {
    kind: NodeKind,
    span: Span,                   // byte range in source
    origin: Origin,               // Source | Macro { def, call } | Synthetic
    slots: Slots,                 // named child rows, at most four per node
}

pub enum NodeKind {
    Row, Fraction, Scripts, Radical, Delimited, LargeOp, Accent,
    Symbol { codepoint, variant }, Text, Space, Table, Error,
    Composition,                  // live IME preedit; replaced on commit
}
```

Decisions:

- **Slots hold Rows.** `Fraction { num, den }`, `Scripts { base, sup, sub }`,
  `Radical { body, degree }`, `Delimited { open, body, close }`. A slot is
  always present; an empty slot is a zero-width row with its own span at the
  caret position. That makes "empty numerator" a real object instead of an
  absence, which is what gives editors an empty-slot rect.
- **Ids are stable per document.** Insertions allocate; deletions free. A full
  reparse rebuilds ids and is only for external source changes.
- **Spans are mandatory.** A node created by a macro expansion keeps the call
  site span and its `Origin` records the definition; an editor can therefore
  treat expanded content as read-only and edit the call instead.
- **Structure is the truth, LaTeX is an export.** The writer serialises the
  model; a source edit reparses. Structural edits never rewrite source text
  directly.
- **A composition is a node, not an overlay.** The IME preedit is the only
  node that exists in the document while being neither undoable nor spanned;
  its contract is below.

## Resolve

A single pass assigns what layout needs, in reading order:

- `AtomClass` (Ord, Op, Bin, Rel, Open, Close, Punct, Inner), with bin
  cancellation at the boundaries of each row;
- `Style` (D/T/S/SS and cramped) by the transition table, with display style at
  the root only and `\displaystyle` overrides;
- stretch requests for delimiters, radicals, horizontal accents and arrows:
  `Stretch { axis, target: RelativeTo(BoxId) | Min(f32), shortfall: f32 }`,
  resolved after the target is measured, never inline.

Spacing is not a node. The layout pass inserts glue between adjacent atoms using
the 8×8 table and the tight variant for script styles. Keeping glue out of the
model means an editor never sees a spacing node between bytes that have no
spaces in the source.

## Layout

Boxes carry `width`, `height` (above baseline), `depth` and a kind:

```rust
enum BoxKind {
    HList(Vec<BoxId>), VList(Vec<BoxId>), Glyph { .. }, Rule, TextRun(ShapedRun),
    Fraction { num, rule, den }, Scripts { base, sup, sub }, Radical { .. },
    Delimited { .. }, Limits { .. }, Table { rows, cols },
}
```

- One vlist primitive serves scripts, fractions, radicals and limits, as in
  KaTeX's `makeVList` and MathLive's `VBox`; nothing derives a baseline from CSS
  or from text metrics.
- Dimensions come from the font: MATH constants where they exist, TeX-like
  defaults where a font omits them. The minimal constant set and the rules that
  consume it are in `raw/algorithm.md` §1–2.
- Stretchy construction follows the MATH three-step protocol: ready-made
  variants, then assembly with overlap reduced toward `minConnectorOverlap`,
  then additional extenders.
- Units: layout is done at the requested size in host logical units. Font
  metrics are read in em and scaled once, so changing size re-lays out but never
  changes relative geometry.

## Geometry for editing

```rust
pub struct CaretPath { steps: Vec<Step> }   // node, slot, index; serializable
pub struct Geometry { .. }

impl Geometry {
    fn rect_of(&self, id: NodeId) -> Option<Rect>;
    fn caret(&self, path: &CaretPath) -> Option<CaretRect>;
    fn hit_test(&self, point: Point) -> Option<CaretPath>;
    fn selection(&self, anchor: &CaretPath, focus: &CaretPath) -> Vec<Rect>;
}
```

- `hit_test` descends the layout tree, ignoring boxes that do not contain the
  point, and applies a midpoint bias at the innermost box. Table cells pick a
  row first, then a column.
- Every empty slot produces a caret rect even when no glyph exists; the last
  caret in a row sits after the final box, not inside it.
- Selection rects are per line of the box tree, unioned per branch; a drag never
  consults rendered pixels.
- The display list may be flattened for painting; the geometry index is built
  before flattening, from ids.

### IME inside a slot (decided)

A preedit is a temporary `Composition` node in the model: it participates in
layout, and commit replaces it with the parsed result in one edit. While it is
alive it is not undoable, carries no span and does not count as a dirty edit.
`TextShaper` shapes the preedit (font fallback and combining marks are the
host's); `Geometry::caret()` must return a stable anchor for the candidate
window. A commit that crosses slots lands in the slot containing the commit
point, and the result enters the model as math, never as text mode.

Reasons recorded with the decision: a view-only overlay would paint over the
neighbouring symbols and give the candidate window nowhere to anchor; a hidden
input would hide what the user is typing.

## Parser, macros, recovery

- Tokens carry `Span` and a `noexpand` flag. Macro expansion substitutes token
  lists (KaTeX's model), not strings, so spans survive; every expanded token
  carries the call-site span and the definition it came from.
- Budgets are hard and explicit: expansion depth, expansion count, total bytes.
  A formula that exceeds them yields a diagnostic and an error node, not a hang.
- The parser never aborts. An unknown command becomes an `Error` node holding
  its raw source; a missing argument becomes a marked empty slot. Diagnostics
  carry spans, so a host can underline the exact bytes.
- The LaTeX writer is the inverse of the parser for the supported subset. A
  corpus test asserts `parse(write(parse(s)))` has the same structure and spans
  for every supported construct.

### Editing expanded macros (decided)

Arguments are editable; expansion is not. An argument keeps its own span and
behaves like any other node. A node produced inside a macro body is read-only,
and the user may run an explicit "expand here" that replaces the call with the
body's text. Expanded nodes share the call-site span with
`Origin::Macro { def, call }`, so several nodes may point at one range.

The span invariant therefore weakens to: every byte is either covered by a leaf
span or marked as expansion of a call that covers it. The definition may live
outside the document (a preamble or template), which is why editing the body is
not merely a UX choice but often physically unwritable.

## Fonts and text

- `math-font` reads `head`, `hmtx`, `cmap`, MATH (constants, italics correction,
  accent attachment, MathKern, variants, assemblies) with `ttf-parser` as the
  SFNT layer. Writing a second SFNT parser is not where the value is.
- Math glyph advances are tabular, from `hmtx`; italic correction from MATH.
  Script styles use the MATH scale percentages, never a hard-coded 0.7.
- `\text` and `\operatorname` go through a host-provided `TextShaper`. The core
  defines the trait and a trivial fallback that treats the string as a run of
  glyphs from the same face.
- Font absence or a missing MATH table is a typed error at engine construction,
  not per formula. A missing glyph is a diagnostic with the span that asked for
  it.

## Host contract

```rust
pub trait MathFont { /* advance, italic, ink, constants, stretch, glyph lookup */ }
pub trait TextShaper { fn shape(&self, text: &str, style: TextStyle) -> ShapedRun; }

impl Engine {
    fn new(font: Arc<dyn MathFont>, shaper: Arc<dyn TextShaper>) -> Result<Self, Error>;
    fn parse(&self, source: &str) -> Document;
    fn layout(&self, doc: &Document, size: f32, display: bool) -> Layout;
    fn edit(&self, doc: &mut Document, op: EditOp) -> Dirty;   // dirty subtree root
}
```

`Layout` owns the display list (`GlyphRun`, `Rule`, `Background`), the geometry
index and diagnostics. Painting, rasterisation and input are the host's.

## Hosts and migration

The stack knows nothing about any host's widget, text pass or paint pipeline. A
host adapts by converting `Layout`'s display list into its own item types; that
is a mechanical mapping (`GlyphRun`/`Rule`/`Background`), and it is where the
current host's flat `Formula` would be produced during migration.

The existing pipeline (LaTeX → MathML → formulary) can therefore be replaced in
one host with a flag: parse with `math-syntax`, lay out with `math-core`, emit
the same display-list items. The old path stays until the corpus gate passes,
then is deleted — the two-layout rule forbids shipping both.

## Performance and incrementality

- Metrics are cached per `(face, glyph)`; shaped `\text` runs per `(text,
  style, size)`.
- `layout` is subtree-local: an edit returns the highest dirty node, and only
  that subtree and its ancestors are re-laid out. Sibling boxes are reused by
  id.
- Formulas are small (tens of nodes); a full reparse on external changes is
  acceptable. Keystroke edits go through the model, never through source text.
- Budgets: a simple formula should lay out in tens of microseconds; the target
  is 10× faster than the 6–8 ms measured for mathtex, whose cost is dominated by
  its engine.

## Validation

- **Differential geometry.** A corpus of formulas is laid out by this stack and
  by formulary on the same font bytes; widths, heights and glyph positions are
  compared with a stated tolerance. Divergences are either bugs or documented
  rule choices (cramped handling, spacing suppression).
- **Conformance list.** KaTeX's supported-function table (MIT) is the coverage
  checklist; a formula that parses in KaTeX but not here is a tracked gap.
- **Invariants.** Every caret path resolves; every child box lies within its
  parent except documented overhangs; spans are ordered and inside the source;
  every byte of the source is covered by a leaf span, a diagnostic, or a macro
  expansion whose call site covers it.
- **No panics.** A fuzz target over arbitrary strings and truncations; every
  prefix of a valid formula must produce a layout.

## Demos and test environments

The core is validated without a browser; demos exist to prove the two other
things a browser can: that the display list is host-agnostic, and that the
editing geometry works under real input.

| Environment | What it proves | What it cannot prove |
| --- | --- | --- |
| Native Rust, headless (CI) | rules, geometry, spans, fuzz, differential metrics | painting, interaction |
| SVG emitted from the display list, rasterised with resvg (CI) | exact, deterministic pixels; golden regression | browser text behaviour |
| WASM + canvas 2D, painted by our own rasteriser | the native rendering contract; parity with native output | browser layout equivalence |
| WASM + DOM, absolutely positioned runs | the same geometry drives a CSS painter; selection and accessibility use real text | exact glyph fidelity for variant glyphs |
| Browser page with KaTeX/Temml/formulary side by side | substitutability, measured against references | — |

### Why the CSS mode is a painter, not a layout engine

The survey's "browser lays out" family (KaTeX's HTML, MathJax CHTML, Temml)
delegates horizontal advances to the browser's text layout. Reproducing that in
our demo would reintroduce the nondeterminism the design removes. In the CSS
mode the browser must only paint: every glyph run is a positioned element whose
origin, size and baseline come from our layout tree, and the page applies no
layout of its own.

One fidelity limit must be stated: CSS text is addressed by codepoint, not by
glyph id, so MATH size variants and assembled extenders cannot be selected. The
CSS painter therefore
uses codepoints where they exist and falls back to inline SVG `<path>` for
variant and assembly glyphs. If that fallback is skipped, the mode is a layout
check, not a fidelity check.

### What "replacing both classes" means

The two reference classes differ in where geometry is computed, not in what a
formula looks like. The honest comparison is therefore output-level, in one
page:

- render the same corpus with KaTeX (JS), formulary (native or WASM) and this
  stack, at the same size and face where the font can be shared;
- compare widths, ascent/descent and glyph positions within a stated tolerance;
- overlay the three bitmaps and inspect the diffs by hand for the constructs
  where rules are chosen differently (cramped shifts, spacing suppression,
  delimiter assembly).

The canvas and CSS demos are host adapters for the same `Layout`; neither is a
second engine. Two demos that disagree are a bug in an adapter, not a layout
difference — which is exactly the property the display list is meant to have.

### Harness choices

- Core tests: plain `cargo test`, no display, no font fixtures beyond the
  corpus faces.
- WASM: `wasm32-unknown-unknown` + `wasm-bindgen`; font bytes and the text
  shaper are passed in from JS. A cross-target test dumps the same corpus's
  boxes natively and in WASM and requires identical values (f32 arithmetic is
  deterministic; avoid SIMD paths that reorder sums).
- Browser automation: Playwright with Chromium and Firefox, screenshot diffing
  for the two demos; `Input.imeSetComposition` covers the composition path, but
  a real OS IME check stays manual because candidate-window behaviour is not
  reproducible in automation.
- Performance numbers come from native `criterion`, not the browser; the demos
  only check the interactive budget (a keystroke re-layout inside one frame).

## Milestones

| Phase | Deliverable | Exit criterion |
| --- | --- | --- |
| M0 | font crate, box model, spacing, glyph runs | `a^2+b^2=c^2` matches hand-computed advances |
| M1 | scripts, fractions, radicals, delimiters, large ops | the 16-formula corpus lays out without panics; deltas vs formulary recorded |
| M2 | lexer/parser with spans, recovery, writer | every corpus formula round-trips; no parse aborts |
| M3 | macros, `\text`, accents, font commands | KaTeX conformance list ≥ 80% parse |
| M4 | editing model: ids, caret paths, hit test, selection | caret invariants green; empty slots return rects |
| M5 | tables/aligned, long tail, incremental layout | differential deltas documented; per-edit budget met |

Order of magnitude: M0–M1 about five weeks, M2–M3 about five, M4 about three,
M5 ongoing. Estimates carry the usual uncertainty of first-of-kind work; the
survey's measured sizes (4.5k–28k LOC for the comparison stacks) bound the scope
rather than predict the schedule.

## Rejected alternatives

- **Keep MathML as the IR.** Loses spans at a stage we do not control, adds two
  conversions, and keeps geometry inside MathML semantics rather than TeX rules.
- **Fork formulary.** No spans, no node identity; both would be inventions on a
  foreign codebase, and the fork drifts from upstream.
- **Adopt mathtex.** Correct and complete, but a 3.7 MB format tied to the
  compiled engine, TeX Live at bake time, and 58k generated `unsafe` LOC. Not an
  embeddable dependency for a UI library.
- **Port KaTeX.** Hardcoded metrics, browser-dependent horizontal layout, no
  spans, no editing.
- **Depend on Typst.** Not a library; the math layer is entangled with content
  eval and its frame trees.

## Open decisions

1. **Units.** Confirmed as host logical units at a given size. Revisit only if a
   zoom path needs size-independent cached boxes.
2. **Undo.** Formula-sized snapshots per edit are likely cheaper than inverse
   operations; measure at M4. A snapshot taken while a composition is live must
   exclude the `Composition` node, and the commit is the single undoable step;
   otherwise undo restores half a preedit.
3. **Licensing.** Test corpora and conformance lists need an explicit provenance
   note per source (KaTeX MIT, Typst Apache-2.0, arXiv samples).
4. **Incremental parse.** Full reparse is fine for keystrokes; only external
   source edits may need re-lexing. Measure before designing.

Settled in this revision: IME composition (a model node, see "Geometry for
editing") and macro editing (arguments editable, bodies read-only, see "Parser,
macros, recovery"). Both were open questions before M4 and now constrain the
model and geometry contracts.
