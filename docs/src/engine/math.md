# Formula Layout

A formula is laid out by a library and drawn by the engine: LaTeX in, a flat
display list of glyph ids and rules out, with the baseline metrics inline
content needs. This page records the decision, the contract, and the
measurements the implementation reproduces.

## Pipeline

`$…$` / `$$…$$` from the document model is LaTeX, and the chain is:

1. **math-syntax** (this project) parses LaTeX into a model tree: rows,
   fractions, radicals, scripts, tables, text leaves and operators, each with
   the byte span it came from. A command it does not know becomes an `Error`
   node in place, so the surrounding structure survives.
2. **math-core** lays that tree out against the font's OpenType MATH table
   through **math-font**'s `Face`.
3. The result is `Layout { width, ascent, descent, items }`: `Glyph { glyph,
   x, baseline, size, color }`, `Rule`, `Background` and `Line`. Positions are
   final and in the units of the requested font size; list order is painter's
   order; `ascent`/`descent` are the inline-integration contract.

MathML is no longer in the middle. The model carries node identity and source
spans, which is what structural editing needs; MathML carries neither. The
trade is that accessibility no longer gets a MathML by-product from layout —
that becomes a separate exporter if a host needs it.

## The seam

`ui-math` is portable and does no I/O. It owns the three layout crates and
exposes our types, so nothing above it sees `math_core::Layout`:

- `MathFace::new(bytes: &[u8], index: u32) -> Result<MathFace, Error>` parses the
  face — and therefore the MATH table — once; the caller keeps the bytes alive.
  `face.layout(source, size, display) -> Result<Formula, Error>` runs the chain.
  `Formula` and `MathItem` live in `ui_core::math`, the engine's vocabulary, and
  `ui-math` re-exports them: a node can carry a display list without depending on
  this crate.
- `MathFace::editor(source, size, display) -> MathEditor` owns a structural
  editing session over the same model tree: keys, clicks, IME, undo, and the
  caret, selection and empty-slot rectangles. Those rectangles come back in the
  formula's own space (y from the baseline), so a control paints them beside
  the display list; `MathEditor::formula` returns the display list with the
  same error marks a read-only formula carries. The face is passed to each
  call that edits, because the host owns the parsed bytes.
- `probe(bytes: &[u8], index: u32) -> bool` — has a MATH table — cheap enough to
  scan a font list, because the face parse reads the table directory only.
- `Error` keeps its public variants: `Latex` now carries a parse or layout
  diagnostic, `Font` a face without a MATH table. `Mathml` and `Unsupported`
  are retained for callers that match them; no stage produces either now that
  MathML is gone and every display item has a converter.

## Drawing the display list

`GlyphPrim` is post-atlas — an atlas slot plus a destination rect — and only
the text pass produces it, because only the pass owns the raster. So a formula
gets a leaf of its own: `NodeKind::Math` with `NodeData::Math { formula, family,
ink }`. Layout sizes it from `width` and `ascent + descent`, with no reflow —
min and max content coincide, and the baseline is `ascent` from the top.

Paint copies the display list into a `MathPrim` (the seventh primitive kind),
which the pass resolves: for each glyph at `(x, y)` it blits the face at
`baseline + y` and size `size × scale`, emitting a `GlyphPrim`; rules and
backgrounds become plain `RectPrim`s pushed first, so they sit under the
glyphs, and `Line` becomes an eighth primitive kind, a stroked segment drawn
after the glyphs because a cancellation mark sits over the body it crosses. A
primitive the pass cannot resolve stays in the list, as text does.
The zoom in force where the node was painted rides along, as it does for text.

Two facts make the glyph ids trustworthy. The face is resolved once per family
by the pass and **admitted to the rasterizer** — the MATH table is read for
layout and the outlines for drawing from the same bytes — and the pass hands
that face out through `TextPass::layout_math`, so the only producer of a
formula is the component that will paint it. The face's bytes are leaked once
per family, because `MathFace` borrows them and a face lives as long as the
process.

That split keeps `ui-core` free of math-core and of the LaTeX parser: the
display list is vocabulary, the raster stays in the text pass, and a second
producer (an SVG icon set, a chart) could use the same node without touching
the math crate.

Text inside a formula — `\text`, operator names, the editing preedit — is
measured by a `math_core::TextShaper` the pass installs on every editor
(`FormulaShaper`). It shapes the formula's own face first and then the faces the
provider reports for the run's script, so `\text{中文}` measures with the CJK
face the rest of the page uses; a run no single face covers is split per
grapheme cluster. Shaping happens at a reference em and comes back in em units,
because a formula's size is its own. The face each glyph came from rides to
`MathItem::Glyph::face` as a slot, and the pass blits with that face: a glyph id
means nothing in a font that did not produce it.

## The control

`ui_widgets::Math` is the formula's basic control: `Math::new(id, source, size, ink)`,
with `.display()` and `.family(name)` builders, built like `Text` through
`resolve_in(&cx)`. A build runs before layout, so the control asks the engine
context for the display list: `Cx::math_family()` names the host's default face
and `Cx::math_layout(family, source, size, display)` returns the formula. The
engine context carries both behind `ui_core::MathLayout`, attached by the view
from the same text pass that paints, so the control never touches a provider.

Without a math face the control draws its source as monospace text instead of
an empty box: a formula that could not be laid out should look like its LaTeX,
not like nothing.

A design system names the formula rather than drawing it: `Md3Math`, `FluentMath`
and `GlassMath` are `new(id, source, pack)` plus the same `.size`, `.ink`,
`.family` and `.display` builders, and they delegate layout, paint and
`object_in` to this control. Each takes its size and ink from its pack — MD3
body-large in `onSurface`, Fluent and Glass `TextRole::Body` in their primary
text ink — because that is the part a theme owns; the face and the laid-out
glyphs stay the host's. A pack's test asserts that mapping, so a role name the
pack does not know fails there rather than as a magenta formula on a screen.

Which family the host's default is belongs to the pass: it scans installed
family names for one that says "math" and probes the font's MATH table
(`TextPass::default_math_family`), once. Formulas are cached by family, source,
size and style, because a still page asks for the same one every frame.

## Editing by structure {#editing-by-structure}

`Math::editable()` makes the control its own editor: a press places the caret
inside the structure, a character or an editing key changes the model, and the
next frame paints the caret, the selection and an outline around every empty
slot from the same layout that draws the glyphs. The engine owns the editor —
`TextPass` keeps one per node id across frames, reached through
`ui_core::MathEdit` on the context — while the app keeps owning the source,
which returns as `Change::Text` after every edit, exactly as for a text input.

A standalone formula takes focus and the keyboard with it: Backspace, Delete,
the arrows, Home, End and Space map to `ui_core::EditKey`, a committed character
becomes a `Character` key, and a composition goes through
`set_preedit`/`cancel_preedit`. Tab stays the engine's so focus can leave, and
Escape falls through when the editor has nothing to cancel. A primary press
clicks at the pressed point in formula space, so typing continues where the user
pointed. While it holds the keyboard the formula also answers undo, redo, select
all, copy and cut, because its model is its buffer. An editor not attached for a
frame is dropped, which bounds the history a page keeps.

Inside the formula node the painter's order is the selection wash, then the
empty-slot outlines, then the caret: the caret is a 1.5pt `Rule` in full ink,
the slot borders a hairline at 35% of it. The caret paints only where the caret
is: the focused control when it stands alone, and inside a document the formula
whose bytes hold the document's caret.

### One caret in a document {#one-caret}

`Math::in_text(input, range)` makes the formula a view of bytes of another
input's buffer, and there the document owns the caret. The node is not
focusable; it is an input view of `range`, so a press maps through the formula's
own hit test to a byte of the buffer, and keys, text, paste and composition are
routed to the structural editor while the selection stays inside those bytes.
An edit exports the formula's source; the engine splices that export over the
range with `apply_edit`, and the caret becomes the editor's selection mapped
back into buffer offsets. A key that only moves the caret changes no model, so
nothing is spliced and the document keeps the spelling it had. The document's undo, redo, select all, copy, cut and history are
therefore the only ones: `Ctrl+Z` inside an embedded formula steps the
document's history, and a selection dragged from the prose through the formula
copies the buffer's bytes, fences included.

The two sides meet through byte offsets of the formula's own source:
`selection_offsets` and `select_offsets` on `MathEdit`, `offset_at` for a point,
and `formula_at(owner, offset)` so the engine can ask which formula holds a
buffer byte. A stop maps to the start of the child after it or past the last
child, so offsets are monotonic in reading order; a text leaf maps inside its
`\text{…}` body, through the escapes the writer added; and a host offset between
two stops snaps to the nearest one, the deepest on a tie. The control mirrors the
document's selection into its editor on every build — clamped to its own ends,
so a drag that starts outside and ends inside paints the covered part — and
`AppLoop::dispatch` repeats that for the formula it just entered, because events
arrive in bursts between frames. The input method reads the structural caret as
the place for its composition window. Offsets name the bytes the document
holds: while the model still matches what was parsed, those are the payload's
own bytes, because the editor keeps the input it was given and each node's span
in it; the first edit that changes the model switches the seam to the export,
which is exactly the text the engine splices in. A host offset at the payload's
end is therefore the formula's last stop even when the writer's export is
longer — it terminates commands with a space and braces scripts — and
`\int_0^1 x^2 \, dx` keeps its last two characters selectable. A key the editor
declines falls back to the document: at a formula's first or last stop,
`Backspace` and `Delete` remove the byte beside its range and a horizontal arrow
moves the caret out, so a formula's edge is not a place where keystrokes vanish.
`Up` and `Down` leave the block at the same level once the structure has no line
left, and a step *into* it is answered with its own hit test. Mirroring the
selection back moves nothing when the caret already stands on the byte it names:
several stops export one byte, and re-deriving would pick another of them.

The Markdown control uses this for a display block, whose range is the payload
between its fences, so the app keeps one string and never learns a block id
([Editing Markdown](./markdown-editor.md#corrections)). Each fence line is a view
of the same buffer in its own right, so the caret stands on the `$$` bytes and a
selection covers them; only the payload's bytes reach the editor.

The payload's two ends are boundaries with those views, and a boundary belongs
to one view: the later one. Its first byte is the formula's, because the payload
follows the opening fence; its last byte is the closing fence's when that fence
shares the payload's line, and the formula's when a line break separates them.
`in_text` therefore takes a `caret_ends` flag from the host, which reads that
byte in the document, so a formula written on one line never paints a bar where
the fence already stands.

Two limits are accepted. A selection that starts inside a formula and ends
outside it is the document's, so replacing it splices raw bytes and the model
resyncs from the app's text: bytes outside a formula have no structural meaning.
And the export normalizes whitespace, so a hand-typed payload maps approximately
until its first edit writes the export back.

## A refused source

`Cx::math_layout` returns a `MathError`, not a missing formula. `NoFace` is a
host installation problem: the control draws the source as monospace text.
`Failed` means the chain could not produce a display list at all. A source the
parser refuses is neither: it lays out, with one `MathErrorMark` per refused
run, and the structure around each mark — the fraction, the radical, the script
— is the one the author is editing.

`MathFace::layout_recovering` parses the source as it is. The parser keeps
the structure around a command it refuses and records both an `Error` node and
a diagnostic; the layout walks that tree, so `\frac{1}{1+\frc{1}{n}}` is one
fraction with a marked run in its denominator. `ui-math` turns each non-
`Incomplete` diagnostic into a `MathErrorMark`, finding the smallest laid-out
node whose span covers it. An `Incomplete` diagnostic — an open group, a
missing argument — is the next keystroke's input, so it gets no mark; the
structure above it still lays out.

The mark carries the parser's diagnostic message, not a byte excerpt: the
model knows the offending node's span, and the diagnostic already names why it
was refused. `ui-math` tags the mark rect with the caller's `MathErrorStyle`
when there is one, and the formula's own ink when there is not. The style is
therefore part of the layout key.

`Math::marked_decl` wraps the formula in a relative box and places an empty,
hover-sensing target over each mark; the parser's reason hangs from the target
in a panel while the pointer is over it. The target is padded past the run and
may cross the formula's edge: it is a hit area, not a node boundary.

Two limits are accepted. A refusal that discards the parser's buffered atoms —
a duplicate superscript, a stray closing brace — leaves whatever tree recovery
built, which is not the one the author meant. A source that defines macros skips
the command naming, because a command cannot be judged without the definitions
before it, so its marks are empty boxes.

TeX recovers by skipping the offending token, which is what this follows.
MathJax's `merror` is the same shape — one box in place, reached through its
`formatError` hook — though a hard error replaces the whole expression
(<https://docs.mathjax.org/en/latest/options/input/tex.html>). KaTeX replaces too:
with `throwOnError: false` it renders the invalid expression as its source in
`errorColor` and puts the message in the element's title
(<https://katex.org/docs/options>). The repair pass is what neither has: an
incomplete group, the normal state while typing, becomes an empty slot instead
of an error.

## Inline formulas ride on atoms

An atom was a box layout reserved and the painter filled with a placeholder.
`AtomBox::object` now carries what belongs inside it — `ui_core::InlineObject`,
whose first case is `Math { formula, family, ink }` — and the text pass paints
it at the atom's rect. An image or an icon inside a sentence becomes another
`InlineObject` case and touches neither layout nor the text pass's geometry.

The object supplies the box: `AtomBox::object(object)` sets the atom's size from
`object.size()` and its baseline from `object.baseline()`, so the line breaks
and the line's baseline are computed from the very extents the painter will
draw. `ui_richtext::to_text_block` takes an object callback beside its look
callback: a mark it turns into an object leaves one U+FFFC in the runs with an
atom at that byte, so the source range still covers the formula and an edit
slices the source. A callback that returns `None` keeps the text, which is why
the mark's look (its source, monospaced and tinted) remains the fallback when
the host has no math face.

`ui_widgets::Math::object_in(&cx)` is the control's inline door: it returns the
same `InlineObject` `resolve_in` would turn into a node, so a sentence and a
display formula reach one layout entry point. The formula's ink is the
control's, fixed when the object is built; a run's ink does not recolour a
formula inside it.

## Fonts

- Nothing is bundled (repository rule). The family is named, then resolved
  through the provider; `fontique` 0.12 has no `Math` generic and our
  `GenericFamily` has four cases, so a `Math` case is the clean way to let the
  machine answer, and it does not exist yet.
- Measured on this machine, 2026-11-05: `fc-match 'DejaVu Math TeX Gyre'` →
  `/usr/share/fonts/wps-office/DejaVuMathTeXGyre.ttf`, `probe` true;
  `fc-match math` → Cambria Math (`/usr/share/fonts/TTF/cambria.ttc`), present
  but proprietary; STIX Two Math is not installed.
- The gallery names DejaVu Math TeX Gyre, falls back to any installed family
  whose name contains "Math", and draws the formula's source text when none is
  found. Cambria Math is never a dependency.

## Substitutability

`math-diff` in the mathstack repository lays a corpus of fractions, radicals,
scripts, delimiters, operators, matrices and text with this stack and with
formulary 0.3.0, then compares width, ascent and descent case by case. Its
`--gate` run on DejaVu Math TeX Gyre passes: every delta is either zero or one
of the documented rule choices (script placement and display limits differ
between TeX Appendix G and MathML Core). That check is what makes the
replacement safe: the display list changed producer without changing the
contract the engine paints.

The details that matter for the engine — a fraction is taller than a letter,
scripts are set smaller, a bar spans the fraction — are pinned by `ui-math`'s
own test against a real system face rather than by the corpus floats.

## Dependency cost

`ui-math` now depends on the in-repository `math-core`, `math-syntax`,
`math-font` and `math-editor` crates (path dependencies until they are
published). The only new transitive crate is `ttf-parser` 0.25, which reads the
SFNT and MATH tables;
formulary's `read-fonts`/`skrifa`/`harfrust` copies and pulldown-latex's
MathML writer are gone. The workspace lockfile no longer names either crate.

## Corrections

**A key the structural editor declined vanished at a formula's edge.** The
route consumed every key the editor answered `false` for, so `Backspace` at a
formula's first stop did nothing: repeated presses emptied the payload and then
stalled, leaving the bytes before the formula — its fences included —
unreachable. A declined key now falls through to the document, which deletes
the byte beside the formula's range or moves the caret out of it. Pinned by
`markdown_math`'s backspace-at-the-first-stop test.

**A formula's text could only be drawn in the formula's face.** Every editor was
built without a host shaper, so `\text` fell back to one glyph per character from
the math face: no kerning, no ligatures, and a missing-glyph diagnostic for any
character the face lacks. `\text{中文}` therefore laid out at zero width with two
"no glyph" marks instead of drawing, and its carets were per byte rather than per
cluster. The pass now installs `FormulaShaper` — the formula's face, then the
script's fallback faces, split per grapheme where needed — and each glyph names
the face slot it was shaped in, which the paint path resolves. Pinned by
`math_edit`'s `a_text_container_measures_in_the_documents_own_text_stack` (red:
zero width and two diagnostics) and `markdown_math`'s
`a_mark_and_its_base_are_one_caret_stop_inside_text` (red: a step did not cross
the grapheme).

**The caret could not leave a formula with the arrow keys.** Two halves of the
same mistake, one on each side. The editor's horizontal and vertical steps
clamped their target and reported a change anyway, so the document never saw the
key at the edge; and the document's vertical step only knew text views, so it
skipped the formula's box and landed in the paragraph past it. The editor now
declines a step that does not move (`math-editor`'s
`mirroring_the_reported_caret_does_not_undo_a_step` also pins the mirroring rule
below), the document measures a formula view by its
box, and `markdown_math`'s `the_arrows_walk_the_caret_out_of_a_display_formula`
walks the caret out to the left, back in from below and out downward again.

**Mirroring the selection back re-derived the caret every frame.** The document
hands the editor the caret it reported after every key, and the editor derived a
path from that byte. Several stops export one byte, so the derivation could pick
the one the user was not on; worse, a collapsed request also derived an anchor,
leaving two paths on one byte that the next arrow collapsed instead of stepping
through. The caret keeps its path when the offset is unchanged and a collapsed
request stores no anchor, which is what lets a step between equal-exporting stops
survive the echo.

**Offsets named the export while the document still held the source.** The
editor's spans were normalized to the export at construction, and the host's
payload is the text the user typed — which an untouched export does not
reproduce: the writer terminates a command with a space and braces a script, so
`\int_0^1 x^2 \, dx` exports four bytes longer than it was written. Every host
offset was therefore treated as an export offset, and an offset at the payload's
end named a stop *inside* the formula: its last two characters (`dx`) could not
be selected, a press at its right edge landed short, and the first arrow key
spliced the export into the document — rewriting the author's formula — just to
make the two texts agree. The editor now keeps the input it was given and each
node's span in it, and names that text while the model still matches the parse;
the first edit that changes the model switches the seam to the export, which is
the text the engine splices in. mathstack pins both with
`offsets_name_the_text_the_host_holds` and
`the_hosts_last_byte_reaches_the_formulas_last_characters`; the document test
`an_arrow_inside_the_formula_steps_by_structure` asserts a boundary in the
source's own bytes (`start + 6`, not the export's `start + 7`).

**A stop and its enclosing boundary can export one byte.** `\binom{a}{b}` ends
at byte 12 three times over: the denominator's own end, the end of the binom node
inside the row, and the row's end after it. Two of the three are drawn at the
same point. A vertical step between two of them changes the editor's path and
nothing else, so the document sees no byte change and the press buys no
document movement; the next press leaves the block. That is inherent in a host
whose caret is a byte — it cannot tell two stops apart, so the editor cannot
report the step as progress. The mirror rule above keeps such a step from being
undone, which is what the host needs; the remaining cost is the single press.
Standalone, the same step is a real move between two lines of the formula.

**The MathML middle stage was removed.** The first chain was pulldown-latex →
MathML → formulary. It gave accessibility a MathML by-product, but MathML
discards node identity and byte spans, so carets, slot geometry and structural
edits could not be derived from what was painted. math-syntax now parses LaTeX
straight into a model and math-core lays that model out; `ui-math` converts the
four display item kinds, including `Line`, into the engine's vocabulary.

**The app was going to resolve the face and the pass was going to trust the
family name.** That assumes two resolutions of the same family land on the same
face — true for an explicit name through one provider, false the moment a stack
or a fallback is involved — and it still left the pass to admit the face to the
rasterizer. Building the paint path showed the first attempt dropping every
glyph (the face existed in the provider but not in the glyph cache). The pass
now owns the math face: `TextPass::layout_math` lays out and `math_items`
paints with the same entry, so the two cannot disagree. The cost is that a
formula cannot be produced without a pass, so the control reaches it through the
engine context (`MathLayout`) rather than owning a provider of its own.

**"A refused source falls back to its text" assumed the old writer would report
the refusal.** pulldown-latex's MathML writer recovered silently, so a source
with a typo still drew a half formula and no fallback ran. The mathstack parser
makes the failure explicit: it keeps the surrounding tree, inserts an `Error`
node, and records a diagnostic with a byte span. `layout` treats any diagnostic
as fatal; `layout_recovering` lays the tree out and marks every non-
`Incomplete` diagnostic. An unclosed group is the normal state while typing, so
it gets no mark; the structure above it still draws.

**A refused source was boxed as one tail, then split into runs, before it was
laid out structurally.** The first two stop drawing at the first error, and a
run split cannot carry a fraction across the cut: typing
`\frac{1}{1+\frc{1}{n}}` in the gallery's editable field drew `\frac{1}{1+` as
one formula and the rest as another, with the frame the author was working in
gone. The model now keeps the fraction and an `Error` node inside its
denominator. `a_mistyped_command_keeps_the_structure_it_sits_in` pins the bar
spanning the formula and the mark below the baseline;
`an_unclosed_group_still_lays_out_the_structure_above_it` pins the no-mark
incomplete case; the gallery self-test types the formula into the editable field
and hovers the mark.

**The app was going to place a child node at each atom's rect.** A text node's
content is shaped inside the pass and has no node children, so a child would
have to be positioned from the flow lines after layout — a second layout pass
the retained tree does not expose, and one that could disagree with the line the
text was broken into. The atom carries the object instead, and the pass paints
it while it paints the line: one baseline source, one break decision. Inline
images will take the same door.

**The edited formula kept the size it was first attached at.** The registry
entry copied the requested size and display into itself before comparing them,
so the `set_size`/`set_display` arms were unreachable: a control that changed
size went on laying out at the old one — 10.64 wide at both 20 and 40 points.
The comparison now reads the stored value, which is written back only after the
editor accepts the change. `a_size_change_reaches_the_edited_formula` goes red
when the early assignment is restored.

**A standalone source field was the first editor, and it was removed.**
`MathField` put a one-line engine text input over a live preview, with
`Md3MathField`, `FluentMathField` and `GlassMathField` adding each pack's field
chrome. Once the structural editor gave the caret a home inside the formula,
two editing widgets meant two stories to keep true and three more field
surfaces to theme. The control, the three pack wrappers and the gallery's live
rows are gone; each pack's error style moved into its own `math` module. The
Markdown block's source-plus-popover mode will be built inside the block
instead, as a mode of that block rather than a second widget, so one control
owns formula editing. The workspace build and the gallery self-test are what
catch a stale reference.

**The embedded formula kept a caret and a history of its own.** A display block
in Markdown was a focusable control whose editor owned the caret, the selection
and the undo stack, while the engine kept a second selection in the document and
pinned it to the formula's end. Two bars could name different places, a drag
could not reach into the formula, and `Ctrl+Z` answered the formula's stack
instead of the document's. The document is now the only caret and the only
history: the node is not focusable but an input view of the block's bytes, the
engine routes keys and text to the structure and splices the export through
`apply_edit`, and the control mirrors the document selection to paint. This
needed byte offsets on the last layer first — `offset_of`, `caret_at_offset`,
`selection_offsets` and `offset_at` in `math-editor`, and `formula_at` on
`MathEdit` — because a caret path and a buffer offset name the same place only
through the writer's own export. The `markdown_math` tests listed under Status
catch a return to a second buffer; `an_edit_inside_an_embedded_formula_joins_the_documents_history`
is red when the structural edit bypasses the document's history.

## Traps

| Change | Symptom |
| --- | --- |
| Treat the display list as text runs | A stretchy radical or a fraction bar has no character to shape |
| Lay a formula out against a face the rasterizer never admitted | Every glyph drops and the prim stays unresolved |
| Check glyph ink against `ascent`/`descent` | Rasterized ink overhangs the reported extents by a pixel; assert on the baseline |
| Build a `MathFont` per frame | The MATH table is parsed again on every frame |
| Read the font file per formula | File I/O inside a frame; hold the bytes in the provider |
| Assume a glyph id means the same in another face | Wrong glyphs — ids are per font, not per character |
| Ignore `mirrored` or item order | Right-to-left radicals draw backwards; backgrounds cover their content |
| Push a prim only in `list_from_tree` | The view builds its own list in `paint_self`; the node lays out and draws nothing |
| Expect a one-letter fraction to be wider than the letter | Its scripts are set near 0.7×, so its bar is narrower than the letter at full size |
| Size an atom apart from its object | Line breaks measure one box and the painter draws at another; `AtomBox::object` sets both |
| Recolour an inline formula through its run's ink | The object carries the control's ink; the U+FFFC run's style never reaches it |
| Forget that math-core item y is measured from the formula's top | Every glyph is drawn one ascent too low; `ui-math` subtracts `layout.ascent` when building `Formula` |
| Treat an `Incomplete` diagnostic as an error mark | Every keystroke that opens a group flashes a box while typing; only non-incomplete kinds are marked |
| Assume a diagnostic span equals an error node's span | The mark falls back to a larger enclosing box; pick the smallest laid-out node containing the span |
| Turn a caret path into a byte offset by adding the node's span | A text leaf's export escapes `{` as `\{` and `\` as `\textbackslash{}`, so every offset after it drifts; map through the exported body |
| Assume a formula's export equals the payload the app holds | The writer normalizes spacing (a command gains a space), so offsets are exact only once an edit writes the export back |
| Keep a formula's own undo stack when it lives in a document | `Ctrl+Z` steps the wrong history and the document's copy drifts; record the spliced export with `apply_edit` |
| Match a mark id with an integer literal of the wrong width | `Id::with` hashes the salt's bytes; `0usize` and `0i32` derive different ids |

## Status

Implemented: the `ui-math` crate — `MathFace::new` (parse the face once) and
`layout` (LaTeX → model → `Formula`), `probe` for family scans, and conversion
of every math-core display item, including `Line` for `\cancel`. The engine
side is the `Math` node, its measurement and baseline, the `MathPrim` and its
resolution in the text pass, and `TextPass::layout_math` as the face owner.

Five `ui-math` tests pin it: a real system math family, where a fraction is
taller than a letter, sets its scripts smaller and draws one bar across its
width; a `\cancel` whose stroked diagonal survives conversion; the bundled
Inter face, refused with `Error::Font`; the typo that keeps its fraction and
gets one mark; and an unclosed group that lays out with no mark. Engine tests
pin the formula node's extents and baseline, its resolution into glyphs and a
stroked line, and the control's fallback to source text when the host has no
math face. The paint test needs a
system math font and skips when the build has no system fonts, as the
neighbouring shaping tests do.

Implemented on top of that: `ui_widgets::Math`, the `MathLayout` door on the
engine context, the pass's default-face scan and formula cache, and the
gallery's Markdown page, whose display formula is set by the control — the
sample's `\int_0^1 x^2 \, dx` renders with a real integral, limits and
superscript.

Inline formulas are implemented through atoms: `InlineObject::Math`,
`AtomBox::object`, the pass's object paint, `Math::object_in`, and the object
callback of `to_text_block`, which the gallery's Markdown page answers for
`InlineMath` at the paragraph's own size. Two more tests pin it: a marked run
becomes one U+FFFC with an atom at its offset (and its text when no object is
made), and an atom carrying a real formula paints glyphs where the placeholder
block would otherwise be.

The per-system controls are implemented: `Md3Math`, `FluentMath` and
`GlassMath` wrap the shared one, and each pack's test pins its formula to that
pack's body size and primary text ink (six assertions over three packs: the ink
and the size each, seen red by substituting a transparent ink and a size one
point larger).

The gallery's math page gives each column a structural editor and a read-only
formula whose source the parser refuses, so the marked run and its hover reason
are visible beside the editable cards. Its self-test clicks the editor, types a
character, and asserts that the app's source gained it and that the control still
draws a formula; it then hovers the refused card's mark and asserts the panel
mounted below it.

A refused source keeps its structure. `layout_recovering` closes what an
incomplete source leaves open and runs the parser through its errors, so
`\frac{1}{1+\frc{1}{n}}` lays out as a fraction with a marked run in its
denominator; `MathErrorStyle` colours that run, and a hover target over it hangs
the parser's reason in a panel. The formula cache stores the error marks with
the display list, so an editor's invalid source is not reparsed every frame.
Two tests pin it: the typo keeps a bar spanning its formula and a mark below the
baseline, and an unclosed group lays out with no mark. The gallery self-test
types ` + \frac{1}{\frc} + y` into the editable field, then asserts the formula
node, exactly one mark inside it below the baseline, and the panel below the
mark; the whole self-test passes.

Structural editing is implemented for the display control: `Math::editable()`,
the `MathEdit` door on the engine context, the pass's per-id editor registry and
its routing of keys, text and preedit, and the caret, selection and slot paint in
the node's own display list. An embedded formula is part of its document's text
surface instead of a control of its own: its bytes are input views, a press maps
through its hit test, its structure takes the keys, and the export is spliced
back through `apply_edit`, so the document keeps one caret and one history
(one-caret). The Markdown control uses that for display blocks in its
`Structural` mode, while `Source` mode shows the payload and floats a formula
preview for a block as well as for an inline atom. Tests in
`ui-widgets/tests/math_edit.rs` pin the standalone control without a window — a
press at the formula's left edge takes the keyboard, a typed character reaches
the app ahead of the seed, a size change reaches the layout — and
`ui-widgets/tests/markdown_math.rs` pins the document path: a press into a
fraction, an arrow that steps by structure, a drag from the prose into the
formula, the selection wash the formula paints, the composition window's place,
the shared undo stack, and copying the buffer through a formula. Each goes red
when its wiring line is removed.

Not implemented: inline images (the same `InlineObject` door, no case yet) and
a structural editor for an inline formula, which has an atom rather than a node
to focus. The standalone source field was removed once the structural editor
landed, because two editing widgets meant two places to fix. The spike that chose
the engine is recorded in [Editable Math](./editable-math.md). Mirrored glyphs
are not implemented either, but no LaTeX input reaches them; a correction records
the evidence.

## See also

- [Rich Text Stack](../reference/rich-text-survey.md): why these libraries.
- [Block Document](./document-model.md): where `Math` and `InlineMath` come from.
- [Inline Content](./layout-inline-content.md): atoms, which carry inline formulas.
- [Editable Math](./editable-math.md): what an editor needs, which crates exist,
  and the spike that decides the layout engine.
