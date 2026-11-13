# Is mathtex the way to edit a formula in place?

Tier ② needs a structural tree, geometry for carets and empty slots, and a
click that maps back to a caret. This page records the MathQuill interaction
contract, a measured comparison of `mathtex` and `formulary` on one corpus,
and the cost of mapping our LaTeX event stream into `mathtex-editor-core`.

## Summary

**Recommendation: freeze tier ② for now, and take the mathtex route only if
structural editing becomes a product requirement.** The measurement rules out
the middle option: a formula typeset by `formulary` and then edited through
`mathtex` changes width by up to 45 % on the first keystroke. The two honest
routes are both substantial, for different reasons. Adopting mathtex means
replacing `formulary` everywhere, carrying a 3.73 MB format tied to the engine
build, and re-baselining every formula's metrics. Writing our own layout means
duplicating formulary's 1,923-line layout and adding the LaTeX byte spans that
`pulldown-latex` does not emit.

| Question | Result |
| --- | --- |
| Does mathtex match formulary on static output? | No. Widths differ by -22 % to +45 % across the corpus; `\sin` and `\lim` even change glyph count |
| Can a formula be edited without changing shape? | Only if one engine typesets both states |
| Does the editor stack work? | Yes. Format load, caret, empty-slot rects and hit testing all ran; 6.4 to 7.5 ms per edit uncached |
| Is the LaTeX importer small? | The tree mapping is tractable (23 direct and 20 convertible of 48 variants; a 7-formula prototype validates), but the event stream has no byte offsets |
| What does a format cost? | 3.73 MB, 7.3 s to bake from an 80 MB minimal TeX Live tree; runtime needs the format plus font and TFM metrics |

All measurements are release builds on this development machine on 2026-11-06,
with `mathtex-engine 0.2.0`, `mathtex-editor-core 0.4.1`, `formulary 0.3.0`
and `pulldown-latex 0.8.0`. No GPU and no nested session were involved.

## MathQuill's contract

The contract below comes from MathQuill 0.10.1, commit
`1cbc1c4128ec87db3733bc99965522f7d62cde75` (2025-06-02), read from a shallow
clone. It is behaviour, not geometry: MathQuill measures the browser DOM, and
that layer does not transfer to a canvas engine.

### Tree and inner fields

A `MathCommand` owns one or more `MathBlock` slots; a slot is a linked list
with left and right end pointers, not a named field. The command's `DOMView`
assigns meaning by position, so a fraction's block 0 is the numerator and
block 1 the denominator (`src/commands/math/commands.ts:771`). Slots are
created and adopted in `MathCommand.createBlocks`
(`src/commands/math.ts:162`), and the caret enters the first empty slot,
left to right, else the right end child (`placeCursor`,
`src/commands/math.ts:172`).

An empty slot is not a dashed box in MathQuill's own CSS. On blur an empty
block receives `mq-empty` (`src/commands/math.ts:665`); the class paints a
20 % black background (`src/css/math.less:92`) and an `::after` with a hidden
`c` keeps the block from collapsing (`src/css/editable.less:53`). The class
`mq-empty-box` exists only for the inner box of `\dot`
(`src/commands/math/commands.ts:201`). Desmos may restyle the fill, but the
library does not draw a dashed outline. This corrects the "dashed box" wording
in [Editable Math](./editable-math.md).

### Keys become structures

| Input | Structure | Caret lands | LaTeX emitted |
| --- | --- | --- | --- |
| `/` | `Fraction`, wraps the leftward operand | numerator | `\frac{...}{...}` |
| `^` | `SupSub` superscript; welds onto an adjacent subscript | superscript | `^{...}` |
| `_` | `SupSub` subscript | subscript | `_{...}` |
| `(` | one-sided `Bracket` with a ghost close | inside the bracket | `\left(...\right)` |
| `)` | closes a matching one-sided bracket | after the bracket | `\left(...\right)` |
| `\|` | `Bracket` for absolute value | inside | `\left\|...\right\|` |
| `\sqrt` | `SquareRoot`, one slot | radicand | `\sqrt{...}` |
| `\nthroot` | `NthRoot`, index and radicand slots | index | `\sqrt[n]{...}` |
| `\frac` | `Fraction` | numerator | `\frac{}{}` |
| `\int` | integral `SummationNotation`, two limit slots | lower limit | `\int _{}^{}` |
| `\sum` | `SummationNotation`, two limit slots | lower limit | `\sum _{}^{}` |
| Tab / Esc | no structure; leaves the current block right | next sibling block, else one level out | unchanged |
| Shift-Tab | leaves the current block left | previous sibling, else one level out | unchanged |
| Space | types a control space, not a tab | after `\ ` | `\ ` |

The `(` row is the important one: typing a parenthesis synthesises
`\left`/`\right` (`bindCharBracketPair`,
`src/commands/math/commands.ts:1432`), and a one-sided bracket keeps its ghost
partner until the closing key arrives (`Bracket.createLeftOf`,
`src/commands/math/commands.ts:1209`). Space does not behave like Tab by
default; `spaceBehavesLikeTab` is unset in the option defaults
(`src/publicapi.ts`). The `/` wrap skips back to a binary operator, text,
summation, `\ ` or comma, so `a+b/c` puts only `b` in the numerator
(`LiveFraction.createLeftOf`, `src/commands/math/commands.ts:888`).

The optional root index is parsed from `\sqrt[3]{x}` on import, but live typing
uses `\nthroot` or `\cbrt`; typing `[` after `\sqrt` enters a bracket command
instead. That live path is source-derived, not run in a browser `[unverified]`;
risk: a toolbar or command may be required to enter the index.

### Caret, selection and deletion

A cursor is a point with a parent block and left/right sibling references, not
a character index (`Cursor extends Point`, `src/cursor.ts:21`). Its DOM span is
inserted before the right sibling. Moving left or right calls `moveTowards`
on the sibling, which descends into a command's end block, or `moveOutOf` at a
block edge (`src/commands/math.ts:185`, `src/commands/math.ts:553`). Up and
down use the command's `upInto`/`downInto` links, which is why they cross a
fraction's numerator and denominator and a script's base and superscript.

Backspace is `deleteDir(L)` and Delete is `deleteDir(R)`
(`src/services/keystroke.ts:352`). At an empty command the whole command is
removed; otherwise the cursor moves toward the direction's end. At a block
edge `deleteOutOf` calls `unwrapGramp`, which dissolves the parent structure
and splices its non-empty slots into the grandparent
(`src/cursor.ts:177`). Deleting from the denominator of a fraction can
therefore turn it back into a row without a separate "unformat" command.

Selection is a tree range bounded by the cursor and an anticursor that stores
one ancestor per nesting level (`src/cursor.ts:229`). It can cross structure
boundaries; copying serialises the selected nodes' LaTeX
(`setTextareaSelection`, `src/services/textarea.ts:53`).

### LaTeX in and out

The tree is the canonical model; LaTeX is an export format. `MathCommand.latex`
writes its control sequence and one braced slot per block, with an empty slot
as `{ }`, a braced space (`src/commands/math.ts:282`). `MathBlock.latex` joins
children with no separator (`src/commands/math.ts:485`). Sub- and superscripts
write `_{...}^{...}` (`src/commands/math/commands.ts:451`), a summation writes
`\sum _{}^{}` (`:667`), and a bracket writes `\left...\right` (`:1151`).
`cleanLatex` removes a space after a control word when no letter follows
(`src/services/latex.ts:106`).

Import is all-or-nothing. `latexMathParser` rejects an unknown command
(`src/services/latex.ts:67`); `writeLatex` silently inserts nothing when the
parse fails, and `renderLatexMath` clears the root when it fails
(`src/services/latex.ts:325`). There is no per-run recovery like
`layout_recovering`. The importer also recognises only commands that exist in
`LatexCmds`: a bare `\left(1\right)` is structural, while a bare `(1)` takes
the symbol path and is not turned into a bracket. Source reading suggests the
bare form is refused outright because the parser commits to the symbol command
before the fallback; not executed `[unverified]`, and the risk is a rejected
paste of textbook LaTeX.

The round trip is canonicalising, not identical: typing `a/b` exports
`\frac{a}{b}`, typing parentheses exports `\left(...\right)`, spaces become
`\ `, and explicit braces are added. Importing `a/b` keeps the slash because
only the typing path consults `CharCmds` (`src/commands/math.ts:604`).

### The hidden textarea

A one-pixel textarea is absolutely positioned, clipped and scaled to zero
(`src/css/textarea.less:4`). It takes focus, keydown, paste and IME; the
visible tree only renders. A typed character is read from `textarea.value`
on the next tick (`typedText`, `src/services/saneKeyboardEvents.util.ts:267`;
`Controller.typedText`, `src/services/textarea.ts:133`). Selection is copied
into the textarea to reach the platform clipboard.

There is no `compositionstart`, `compositionupdate` or `isComposing` handling
anywhere in `src/`. IME preedit is therefore not drawn inside the formula; only
the operating system's candidate window shows it, and the committed text
arrives through the textarea. Our engine draws preedit inline, so copying the
textarea boundary verbatim would be a regression for CJK input.

### What our engine already has

| Contract item | Our engine | Gap for tier ② |
| --- | --- | --- |
| Structural tree with slots | no; source is a LaTeX string | a document model |
| Named slots and empty-slot rects | no; tier ① has one source caret | editor-owned geometry |
| Caret inside a structure | no; caret is a byte in the source line | a caret path |
| Click maps to caret | no for formula internals | inverse geometry |
| Node to source bytes | no; MathML and formulary items carry no spans | a span map |
| LaTeX import | pulldown-latex event stream, no offsets | mapping plus spans |
| LaTeX export | not needed today | clean export from the tree |
| Text input and IME | yes, byte-buffer input with inline preedit | key transforms into structures |
| Caret and selection | yes, linear byte positions | tree paths and tree ranges |
| Undo and redo | yes, text undo groups | document snapshots |
| Clipboard | yes, text | JSON and LaTeX payloads |
| Inline host objects | yes, `InlineObject` atoms | host boxes, if wanted |

## Measurements

### Assets and latency

A format was baked from a TeX Live 2026 tree installed into `/tmp` with
`scheme-infraonly` plus `latex-bin`, `amsmath`, `unicode-math` and `lm-math`
(the dependencies were resolved by `tlmgr`).

| Item | Measurement |
| --- | --- |
| Format, Latin Modern Math | 3,728,801 bytes |
| Format, DejaVu Math TeX Gyre | 3,728,495 bytes |
| Bake time | 7.36 s and 7.32 s |
| Bake tree (`texmf-dist`) | 80 MB |
| Runtime requirement | the format and the font files, including TFM metrics the format references; no TeX install |
| Typeset, cache off | 8.59 ms per formula over 1,400 typesets of 7 formulas |
| Typeset plus editor render | 7.47 ms per edit over 500 edits of one fraction; 6.38 ms with one empty slot |

The earlier spike recorded a 4.63 MB format and a 1.2 GB `scheme-small` tree.
The bake tree is now measured at 80 MB because only the needed packages are
installed. The format size differs from the earlier figure with a different
engine build and tree; what changes it was not isolated `[unverified]`, and a
format should be measured per shipped engine build.

The format is still tied to the exact engine build that baked it. A format
downloaded from the mathtex demo, built by a different commit, was refused with
`format image layout version 4, this build reads 7`, which is the trap the
crate README warns about.

### Static output: mathtex versus formulary

Sixteen formulas, 10 pt, release build. `mathtex` was driven by
`mathtex-engine 0.2.0`; `formulary` received the MathML written by
`pulldown-latex 0.8.0`. Width and height are in points; height is
`ascent + descent`. The structural rows (fractions, roots, scripts,
delimiters, sum, integral, matrix, nested fraction, cube root and `\lim`) ran
in display mode; `\text`, `a+b=c`, `f(x)`, `\sin x` and the Greek row ran
inline.

| Formula | mathtex LM | formulary LM | Δ width | mathtex DJ | formulary DJ | Δ width |
| --- | --- | --- | --- | --- | --- | --- |
| `\frac{a}{b}` | 7.690 | 5.290 | +45.4 % | 9.330 | 6.930 | +34.6 % |
| `\sqrt{x}` | 14.050 | 14.050 | 0.0 % | 13.850 | 13.850 | 0.0 % |
| `a^2 + b^2 = c^2` | 53.310 | 53.099 | +0.4 % | 64.611 | 64.310 | +0.5 % |
| `x_1^2` | 10.203 | 10.263 | -0.6 % | 13.320 | 13.220 | +0.8 % |
| `\left( \frac{a}{b} \right)` | 19.630 | 18.550 | +5.8 % | 20.030 | 18.330 | +9.3 % |
| `\sum_{i=0}^{n} i` | 19.557 | 22.929 | -14.7 % | 21.017 | 27.128 | -22.5 % |
| `\int_0^1 x^2 \, dx` | 39.170 | 40.716 | -3.8 % | 43.674 | 45.140 | -3.2 % |
| `\begin{pmatrix} a & b \\ c & d \end{pmatrix}` | 35.450 | 41.210 | -14.0 % | 35.390 | 42.170 | -16.1 % |
| `\text{hello world}` | 48.090 | 48.360 | -0.6 % | 48.090 | 56.860 | -15.4 % |
| `a+b=c` | 39.861 | 39.470 | +1.0 % | 45.831 | 45.830 | 0.0 % |
| `f(x)` | 19.300 | 18.400 | +4.9 % | 22.930 | 22.930 | 0.0 % |
| `\sin x` | 19.667 | 19.667 | 0.0 % | 21.007 | 23.487 | -10.6 % |
| `\frac{1}{1+\frac{1}{n}}` | 26.967 | 22.166 | +21.7 % | 29.485 | 24.684 | +19.4 % |
| `\alpha + \beta = \gamma` | 43.691 | 42.800 | +2.1 % | 47.301 | 47.300 | 0.0 % |
| `\sqrt[3]{x}` | 14.675 | 14.675 | 0.0 % | 14.255 | 14.255 | 0.0 % |
| `\lim_{x \to 0} \frac{\sin x}{x}` | 39.253 | 40.742 | -3.7 % | 45.146 | 49.670 | -9.1 % |

Heights diverge too. `\left( \frac{a}{b} \right)` is 18.431 pt in mathtex and
20.920 pt in formulary (LM); `\sum` is 28.014 against 25.615; `\sin x` is
6.680 against 10.000. Glyph counts differ for `\text` (10 against 11), `\sin`
(4 against 5) and `\lim` (11 against 13). Sorted glyph positions, normalised by
width, differ most on `\frac` (nearest-neighbour distance up to 0.156 of the
width, LM), `\sum` (0.124) and `\lim` (0.085).

The fraction gap has a specific cause. The difference is exactly 2.400 pt on
both faces, and `latex.ltx` sets `\nulldelimiterspace` to 1.2 pt
(`latex.ltx:534` in the TeX Live 2026 tree). TeX reserves one null delimiter
space on each side of a generalized fraction; formulary's MathML `mfrac` does
not. This is a semantic difference between the engines, not a font or rounding
difference.

The `\text` row is not like for like: mathtex sets text in the document's text
font (Latin Modern Roman), while formulary sets it in the math face. That
explains both the height and the DejaVu width difference; it does not soften
the fraction, sum, matrix or delimiter results.

### Face drift

The format bakes the math face's spacing constants. Serving another face
through the same format keeps those constants, measured on
`a^2 + b^2 = c^2`, inline, points:

| Baked face | Served face | Width |
| --- | --- | --- |
| DejaVu Math | DejaVu Math | 64.611 |
| DejaVu Math | Latin Modern Math | 55.017 |
| Latin Modern Math | Latin Modern Math | 53.310 |
| Latin Modern Math | DejaVu Math | 62.451 |

The drift is 3.2 % and 3.3 % from the correctly baked width. These four numbers
reproduce the earlier spike exactly. A runtime face choice therefore needs one
format per face, and a user-installed face that was never baked gets the drift.

### Mapping surface

`pulldown-latex` exposes 48 enum variants reachable from `Event`: the 8
`Event` envelope variants, `Content` (9), `Visual` (4), `Grouping` (15),
`ScriptType` (3), `ScriptPosition` (3), `StateChange` (3) and
`EnvironmentFlow` (3). "Direct" means one `NodeDoc` with no loss; "convert"
means a node exists but information is dropped or a wrapper is needed; "none"
means no field in `mathtex-editor-core` can hold it.

| Source enum | Variants | Direct | Convert | None |
| --- | --- | --- | --- | --- |
| Event envelope | 8 | 6 | 1 | 1 |
| Content | 9 | 7 | 2 | 0 |
| Visual | 4 | 3 | 0 | 1 |
| Grouping | 15 | 3 | 12 | 0 |
| ScriptType | 3 | 3 | 0 | 0 |
| ScriptPosition | 3 | 1 | 2 | 0 |
| StateChange | 3 | 0 | 1 | 2 |
| EnvironmentFlow | 3 | 0 | 2 | 1 |
| **Total** | **48** | **23** | **20** | **5** |

The no-counterpart shapes are `Event::Space`, `Visual::Negation`,
`StateChange::Color`, `StateChange::Style` and
`EnvironmentFlow::StartLines`. `StateChange::Font` is the one convertible state
change; its 16 font variants split 8 that have a `Variant` and 8 that do not
(bold italic, bold script, sans-serif bold italic, and so on). The largest loss
is grouping: array column alignment, `rcases`, equation numbering and
`\\`-environment outer structure have no `NodeDoc` field, so 12 of 15
groupings become a lossy `Matrix`, a `Delim`, or a flattened row.

A prototype converter over the event stream compiles and runs. It maps
fractions, square and indexed roots, sub- and superscripts, plain content and
`\left...\right`. Seven test formulas passed `Document::validate`; six
round-tripped to the same LaTeX, and the indexed root normalised to the
equivalent `\sqrt[{3}]{x}`:

```text
\frac{a}{b}              -> {"type":"frac", ...}            -> \frac{a}{b}
x_1^2                    -> {"type":"script", ...}          -> x_1^2
\sqrt[3]{x}              -> {"type":"sqrt","index":[...]}   -> \sqrt[{3}]{x}
\frac{1}{1+\frac{1}{n}}  -> nested frac                     -> same source
```

The prototype does not yet fold large-operator scripts into `BigOp`, wrap
`\text` in `Styled`, handle matrices, accents, colors, styles or spacing. It is
evidence that the event stream is mappable, not an importer.

The missing piece is larger than the mapping. `pulldown-latex` events carry no
byte offsets; its parser reports no position for a refused token. The editor
needs a span map from every node and caret to a byte range in the LaTeX, which
is exactly what `mathtex`'s IR provides through `Placed::Glyph { source, .. }`
and `SourceRange`. A converter over pulldown-latex would have to invent that
layer, either by a second parse with positions or by structural order. That
work is not counted in the 48 variants.

## The routes, updated

### Route A: adopt mathtex everywhere

Cost measured here: 3.73 MB format per face, 7.3 s bake, an 80 MB bake tree
for CI, 6.4 to 7.5 ms per edit on a fraction, 8.6 ms average typeset for seven
formulas, and a 3.1 MB generated engine crate in the dependency tree. The
editor gives caret, selection, empty-slot rects and `hit_test` directly
(`mathtex-editor-core 0.4.1`, `src/geometry.rs:61` and
`src/editor.rs:280`). An empty numerator produced one placeholder rect, and
`hit_test` at a point inside it resolved to the numerator's `CaretPath`.

Risks: every formula in the repository is re-baselined because mathtex and
formulary disagree; the format is a binary asset tied to the engine build,
which is a repository policy decision; a runtime face choice needs one format
per face or accepts the 3.2 % drift; and the importer is ours to write.

### Route B: write our own layout tree

The mapping cost is now known and moderate. The layout cost is not: formulary
is 4,532 lines, of which `src/layout.rs` alone is 1,923 lines, and it already
implements the MATH-table rules this route would have to match. The event
stream also lacks spans, so Route B needs a LaTeX parser that keeps byte
offsets, duplicating `pulldown-latex` as well. A cheaper variant is a fork of
formulary that emits node boxes, but it still has no LaTeX spans, still drifts
from upstream, and the fork has to be maintained. This is the route the
repository's "two layouts drift" rule warns about.

### Route C: freeze tier ②

Tier ① already ships: a text field over the live formula, with the engine's
caret, selection, IME, undo and clipboard. Freezing costs nothing and keeps the
formula pipeline single-engine. The feature gap is real: no caret inside a
fraction, no empty-slot box, no click into a slot.

### The mixed-engine variant is disproven

Typesetting with formulary while focused and mathtex while editing would
change the formula's width by up to 45 % on the first keystroke, even on the
same face. It is not a viable transition state.

## Open questions

1. Is structural editing a product requirement, or does tier ① cover the use
   case? The answer decides between Route C and Route A.
2. If Route A: is mathtex acceptable as the only math typesetter, including a
   re-baseline of every existing formula's metrics and tests?
3. If Route A: may the repository carry a 3.73 MB format tied to the engine
   build, baked in CI from TeX Live, or must the format be generated at
   install time? A user-installed math face needs one format per face, or the
   3.2 % spacing drift is accepted.
4. If Route A: is a mathtex-based inline formula still allowed to use the
   engine's text pass for glyphs, or does mathtex become the raster source too?
5. If Route B: is a maintained fork of formulary with traced node boxes
   acceptable, and who owns the LaTeX span parser?
6. Is inline IME preedit required in a formula editor? MathQuill has none; our
   engine draws it, and a structural editor must decide which is the contract.
7. Are matrices and alignment environments in scope for a first version? They
   are the largest mapping loss and the largest shape disagreement.

## How each claim was checked

| Claim | Check |
| --- | --- |
| MathQuill behaviour | Source at commit `1cbc1c4`, file and line cited; no browser run |
| Static widths and heights | 16 formulas, release, 10 pt, same face bytes, `mathtex-engine 0.2.0` and `formulary 0.3.0` |
| Fraction difference | Delta equals 2 x 1.2 pt, checked against `latex.ltx:534` |
| Face drift | Four bake/serve combinations on one formula, reproducing the earlier numbers |
| Latency and editor geometry | 1,400 uncached typesets and 500 render passes, release; placeholder count and one `hit_test` result printed |
| Mapping counts | Enum variants counted in `pulldown-latex 0.8.0/src/event.rs`; prototype compiled and ran |
| Format and tree cost | Files measured on disk; bake timed; revision mismatch reproduced with a downloaded demo format |

## See also

- [Editable Math](./editable-math.md): the earlier spike, the boundary
  decision, and tier ① as built.
- [Formula Layout](./math.md): the formulary pipeline this work would replace.
- [Inline Content](./layout-inline-content.md): atoms and the box contract a
  host box would reuse.
