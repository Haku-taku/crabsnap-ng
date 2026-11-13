# Rich Text Stack

What to build a Markdown viewer, a rich-text editor, a code block and a formula
on: which crates own parsing and layout, and which parts we have to own
ourselves. The conclusion is what the controls will be built against; the
alternatives are kept here so the next reader does not re-run the survey.

## What the engine already offers

Text is a paragraph: `TextBlock` holds styled `TextRun`s, an atom table, the
alignment, the line cap and the line gap (`crates/ui-core/src/text.rs`). Layout
wraps the block once into `InlineLayout`, and paint, hit testing and the caret
all read that result, so a caret cannot drift from a painted line.

Inline objects already work: an `AtomBox` occupies a U+FFFC in the run text and
carries its own size and baseline; the breaker gives it its own advance and
`VerticalAlign` places it against the baseline. That is the hook a formula, an
image or an inline widget needs, and it is the reason math does not have to
become a paragraph of its own.

What a document needs and the engine does not have:

| Missing | Why it blocks a document |
| --- | --- |
| Document pipeline | A Markdown import and a marks-to-runs conversion exist, but nothing lays a block tree out or paints it |
| Long-document scrolling | The scroll container has no virtualisation or anchor, so a 10 000-line document lays out and paints whole |

A run now carries its own ink, background, underline and strikethrough
(`TextRun::inked`, `on_background`, `underlined`, `struck_through`), so a link
is a run and an inline code span is one rect per line under its own glyphs, with
the rule placed from the face's `post`/`OS/2` metrics
([Inline Content](../engine/layout-inline-content.md)). A code block is better
served already: a run carries four OpenType feature switches and they reach the
shaper, which is what turning `liga` and `calt` off takes.

## Markdown parser

Versions, dates and licences were read from the crates.io API on 2026-11-05.
Speed is comrak's own corpus benchmark (progit, parse plus render), where
pulldown-cmark takes 55 ms and comrak 97 ms.

| | pulldown-cmark | comrak | markdown (wooorm) |
| --- | --- | --- | --- |
| Version, date | 0.13.6, 2026-10-30 | 0.59.0, 2026-10-26 | 1.0.0, 2025-04-23 |
| Licence | MIT | BSD-2-Clause | MIT |
| API | pull events, no tree | arena AST | mdast tree |
| Byte spans | `Range<usize>` on every event | line:column, end inclusive | byte offsets on every node |
| Speed | 55 ms | 97 ms (1.76×) | vendor claim only |
| Binary | 440 KiB | 2.8 MiB | 1.0 MiB |
| Maintenance | 6 authors in 30 commits, org-owned | one author wrote 24 of 30, 5 breaking releases in 5 weeks | no commit since 2025-04 |

**Decision: pulldown-cmark 0.13.6 also for the editor, with our own IR.**

The deciding property is not speed or extensions but the byte range on every
event: `into_offset_iter()` yields `(Event, Range<usize>)`, so a caret byte
offset, a run's source range and a token all compare without a line/column
conversion. comrak has the stronger compliance evidence (652/652 CommonMark and
670/670 GFM badges versus pulldown's vendored 0.31.2 suite) and a ready AST, but
its spans are 1-based line:column with an inclusive end, it is 1.76× slower, its
size is 6× larger, and its default features pull a C dependency. `markdown`
has the nicest editing tree — byte offsets on definition nodes too, `no_std`,
zero unsafe — but it has had no commit for eighteen months and 99 open issues;
starting a new control on it would be a bet on a dormant crate.

pulldown-cmark's gaps are small and known: GFM bare-URL and e-mail autolinks are
absent, `==highlight==` exists on `main` but not in 0.13.6, and a link
reference definition has no span of its own (rustdoc carries the same FIXME), so
an editor needs a small scanner for `[label]: url`. Text events arrive *decoded*
(`&amp;` becomes `&`, `\*` becomes `*`) while their range still points at the
source: layout the decoded text, slice the original for edits.

What we write on top, in one crate:

- A `Document → Block → Inline` IR with a byte span on every node; the event
  stream builds it with a stack machine. A leaf keeps one piece per text event —
  the decoded text and its source range — so `TextMergeStream` turned out to be
  unnecessary: adjacent pieces join when runs are built. Blocks for table,
  front matter and image are still unmodelled.
- An extension policy: math and HTML become opaque nodes, front matter an
  opaque block, and a post-pass adds GFM autolink literals if we decide we
  want them.
- A mapping from inline semantics to paragraph runs, with images and math as
  atoms, so layout never sees Markdown.

The legacy project's notes list pulldown-cmark 0.13.4 for a card renderer
(`old/docs/crate-map.md`; unverified, and only a demo rather than a control).
It agrees with the choice above but is not why it was made.

## Document model

Every framework surveyed splits the document in two: a tree of blocks, and
inside each text block a **flat sequence with attributes on ranges**. Flutter
is the exception that nests styles as spans, and it pays for it: a style-only
node pollutes the semantic tree and an attribute that crosses a block cannot
be expressed.

| Framework | Blocks | Inline | Style attach point |
| --- | --- | --- | --- |
| Qt | `QTextFrame`/`QTextBlock` | `QTextFragment` | one `QTextCharFormat` per fragment |
| Flutter | widget tree, not text | nested `TextSpan` | span nodes, merged down |
| GPUI/Zed | editor layers above a rope | runs per byte length | `HighlightStyle` overlays |
| Apple | paragraph attributes | `AttributedString` runs | attribute ranges |
| ProseMirror | node tree | flat text with marks | mark set per text node |
| mdast, pandoc | `Block` | `Inline` tree | semantic inline nodes |

Objects inside a line use the same pattern everywhere: a placeholder
character (U+FFFC) that layout asks for a size and a baseline, and that paint
draws as a box. Qt calls it `QTextObjectInterface`, Apple an
`NSTextAttachment`, Flutter a placeholder dimension, Zed a measured
`LineFragment::Element`. Our `AtomBox` already has exactly this contract.

**Decision: ProseMirror's shape, Qt's layout primitives, Zed's overlays.**

- A `Document` of blocks: container blocks (quote, list item, table cell)
  hold blocks; leaf blocks (paragraph, heading, code block, display math)
  hold inline content. Marks are attribute sets on runs, not nodes, so a link
  or a comment may span blocks; the tree is derived only when exporting.
- Attributes are stored *explicitly* per run and resolved at layout
  (document default → block style → run attribute → overlay), because there
  is no inheritance in a flat sequence; Qt's `merge` is the primitive.
- Editing state sits outside the view: `{ doc, selection: {anchor, head},
  typing attributes, undo }`, with undo as inverse transactions. Caret is a
  collapsed selection, as in Qt, ProseMirror, Flutter and Zed.
- Syntax highlighting, search hits and diagnostics are overlays over display
  text, never document mutations — Zed's rule, and the reason a code block
  needs no document type of its own.

The mapping onto this engine is short: `TextBlock` is already a leaf block's
inline content (runs plus atoms, with per-run ink, background and decorations),
and `InlineLayout` is what the view consumes. The block tree and the flat mark
list now exist in `ui-richtext` ([Block Document](../engine/document-model.md));
what is missing is everything that fills them. Position units stay UTF-8 byte
offsets throughout, which matches pulldown-cmark's event ranges and Zed's run
lengths; unist and AppKit count UTF-16 units, so an import from Markdown must
convert at the boundary.

## Formula

| Option | Version, licence | What it gives | Risk |
| --- | --- | --- | --- |
| formulary | 0.3.0, MIT/Apache-2.0 | MathML Core → glyph ids, positions, rules, ascent/descent | one author, two months old |
| RaTeX | ratex-layout 0.1.15, MIT | LaTeX → box tree → display list, KaTeX-faithful | 0.1.x; KaTeX fonts and metrics baked in |
| typst | 0.15.1, Apache-2.0 | a complete typesetter, best quality | no math-only API; 6.7 MB assets; second shaping stack |

Rejected: `latex2mathml` (dead since 2020), `mitex` (stale), `tex2math`
(LGPL-3.0), `katex-rs` (markup, not layout), `waterui-math` and `latex-rust`
(architecture or output is tied to their host framework), `mathra` (its own
SVG only). `pulldown-latex` 0.8.0 and `math-core` 0.9.0 are LaTeX → MathML
converters, kept as inputs rather than as layout engines.

**Decision: formulary behind our own `MathLayout` seam.** It takes font bytes
and returns positioned glyph boxes plus rules and ascent/descent, which our
swash scaler already accepts, and its input is MathML — a format we can also
accept from documents and expose for accessibility. It parses the MATH table
itself through read-fonts; swash 0.2.10 does not read that table (checked in
the crate source). The table was expected to be free because read-fonts 0.46 is
already in our lockfile through skrifa 0.49 in `ui-text-fontique` — it is not:
formulary pins read-fonts 0.43 and skrifa 0.46, which cannot unify with ours, so
a second copy of each enters the tree. [Formula
Layout](../engine/math.md) records that measurement and the rest of the design.

The seam is the exit: formulary is young, and if it stalls the layout module
behind `MathLayout` is replaced by RaTeX (LaTeX-native, KaTeX quality) without
touching the editor or the scene adapter. Typst is not a runtime dependency
but a reference oracle: golden images for a formula corpus are generated with
`typst` offline and compared against our layout.

What we own under this plan: the seam and scene adapter (glyph runs at
absolute positions, rules), inline integration as an atom with
width/ascent/descent, font fallback for glyphs the math font lacks, a small
macro prelude for the LaTeX input, and — for the later editable formula — a
`Row`/`Atom` slot tree with a cursor path, serialised to LaTeX for layout.

### Math fonts

| Font | Size | Licence | Variants |
| --- | --- | --- | --- |
| TeX Gyre Pagella Math | 601 KB | GUST | 95 vertical, 86 horizontal |
| Latin Modern Math | 734 KB | GUST (LPPL-based) | 94 / 86 |
| STIX Two Math | 839 KB | SIL OFL 1.1 | 118 / 47 |
| New Computer Modern Math | 1.4 MB | GUST | 163 / 86 |
| Cambria Math | system `Cambria.ttc` | proprietary | the spec's reference font |

Any of the four free fonts may be redistributed unmodified; OFL and GUST both
allow embedding in rendered documents. **STIX Two Math is the recommended
family** — OFL, no LPPL naming rules, and the widest variant set at a modest
size. The repository's rule against binary assets means it is *not* bundled:
the control asks for the family by name, and adding a `Math` case to
`GenericFamily` (which maps onto fontconfig's `math` alias) is the clean way
to let the machine supply one. Neither exists today — `fontique` 0.12 has no
math generic and our `GenericFamily` has four cases — so a formula control
starts by naming the family. Cambria Math is not redistributable and is never
a dependency.

## Syntax highlighting

| Option | Version, licence | Output | Incremental | Grammar data |
| --- | --- | --- | --- | --- |
| tree-sitter + `-highlight` | 0.27.1, MIT | byte range → class index | parser yes, iterator ours | current; one C build per grammar |
| syntect (+ two-face) | 5.3.0, MIT; 0.5.2, MIT/Apache | (style, text, range) | line-state cache | pinned old (`fa6b862` at v5.3.0) |
| giallo | 0.6.0, EUPL-1.2 | TextMate tokens | no | 220+ grammars, 1.1 MiB |
| lumis | 0.19.1, MIT | formatter-first | no | 122 languages, 162 MiB for all |
| arborium | 2.18.2, MIT/Apache | HTML and ANSI | no | ~70 grammars |
| inkjet | 0.11.1, archived 2025-09 | formatter only | no | 76 languages, 23 M lines of C |

**Decision: tree-sitter with a curated, feature-gated grammar set, and our own
class → style theme.**

The output is the reason: `HighlightEvent::Source { start, end }` plus a class
index is exactly a run span, in the UTF-8 bytes our text model already uses,
and the class list is supplied by the host — so the theme map is ours, which is
the requirement. It is also the only candidate with real incremental parsing,
which the editable code block needs. Its grammars are current; syntect's
released set is pinned to an old `sublimehq/Packages` commit (`fa6b862` at the
v5.3.0 tag, checked on the tag's tree), and that staleness is a product
problem, not a taste: Zola replaced syntect with giallo in 0.22.0.

The costs, stated rather than hidden:

- Every grammar is C compiled by `cc`. Ship ten to fifteen languages as
  optional features (Rust, C/C++, Python, JS/TS, JSON, TOML, YAML, Bash, Go,
  HTML/CSS), never the whole catalogue.
- Grammar crates are licensed individually; most are MIT, some Apache, and a
  few are GPL-3.0 (tree-sitter-nu, -jq, -angular). Each is audited when added,
  and a copyleft grammar goes behind its own opt-in feature — the licence
  follows the upstream, not the crate name.
- `HighlightConfiguration::new` compiles queries at startup. One config per
  language, built lazily and shared.
- `tree-sitter-highlight` walks the whole document. The incremental iterator —
  `Tree::edit`, reparse, query only the changed ranges, splice the spans — is
  ours to write; the reference is its own MIT source, not Helix (MPL-2.0) or
  Zed (GPL-3.0).

Themes are a `TokenClass → Style` map with a default light and dark pair. A
Helix TOML loader is the cheapest way to borrow an ecosystem, because its keys
are tree-sitter capture names and it ships 218 themes; `.tmTheme` only makes
sense if we ever adopt syntect, and it would need a scope translation table.

Fallback: `syntect` with `two-face` is the lowest-integration path for a
read-only block with `.tmTheme` compatibility, but only after vendoring a
current `sublimehq/Packages` dump, so it trades a grammar-generation step for
the editing story.

## Status

A survey with both models landed and the pipeline half built: a run carries ink,
background and both decorations, `ui-richtext` holds the block tree with
document-level marks and fills it from Markdown, and a leaf converts into a
`TextBlock` under a caller-supplied look. `ui-highlight` implements the
tree-sitter decision for Rust, and the Markdown page colours its fence from the
token classes. The formula chain (pulldown-latex → formulary → display list) has
been verified in a spike but no crate wraps it yet; the incremental reparse, the
rest of the grammar set, and the layout and view of a block tree are still
missing, as is long-document scrolling. The crate choices above are
the ones the controls will be built against.
The versions and dates are the state on 2026-11-05; re-check them before
adding a dependency rather than trusting this page alone.

## See also

- [Engine Gaps](./engine-gaps.md): the capability list a document control
  would extend.
- [External Sources](./external.md): where framework claims are checked.
- [Research Index](./research.md): the other surveys this one joins.
