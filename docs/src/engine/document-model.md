# Block Document

The tree a Markdown source becomes before layout: blocks that nest, and inline
attributes that do not. `ui-richtext` owns it; `TextBlock` remains the leaf's
layout input, and layout never sees the source format.

## Blocks

A block is a `kind`, the source byte range it occupies, and a body: nested
blocks, or the pieces of the leaf's own text. The kinds that nest are quotes,
lists, list items and footnote definitions; the leaves are paragraph, heading,
code block, rule and display math.

A leaf's span is its text alone: a heading's span starts after the `#`, and a
paragraph's ends before its newline. A container's span is the whole construct,
markers and newlines included, because an editing command deletes a list item
with its marker. The parser takes each from a different source — the text
events for a leaf, the block event's range for a container — so the two can
never be confused at the point where they are filled.

A leaf also keeps `full`: the whole construct as the parser reported it,
markers and trailing newline included. Layout reads `span`; an in-place editor
edits `full` and hides what `span` leaves out
([In-Place Markdown Editing](./markdown-editor.md)). A tight list item has no
paragraph event, so its synthesised leaf takes `full` from the marks inside it,
widened to their delimiters.

## Pieces

A leaf holds `Piece`s, not one string: a run of decoded text and the source
bytes it came from. The two differ wherever the source spelled something as an
entity or an escape, so `AT&amp;T` is 9 source bytes and 5 characters of text.
Layout needs the text and cannot decode it — CommonMark's entity table is the
parser's business — while an edit, a mark and a caret index the source.

That is why the text is stored rather than re-sliced. Slicing the source would
lay out `&amp;` literally; storing only the text would lose the bytes a mark
covers. The pair is the parse result, not a second source of truth: an edit
re-parses, it never patches one copy of the other.

Inline content is text today. An inline formula is an object beside the pieces
rather than a `Piece`, which is where `BlockBody::Inline` grows next. An image
keeps its alt text as pieces and carries `MarkKind::Image` over `![alt](src)`;
a paragraph that is nothing but one image also becomes `BlockKind::Image`, so
the view can draw a picture where the model still holds every byte.

A table nests three deep: the table holds rows, a row holds cells, and a cell is
a leaf. GFM has no column or row spans and its cells hold inline content only,
so no cell needs to be a container of blocks. The table's `BlockKind` carries
the delimiter row's alignment, one per column, because that is the only thing a
table says that its children cannot; the pipes, the padding and the delimiter
row belong to the table's `full`, not to any cell's `span`.

## Parsing

`ui_richtext::parse` turns Markdown into this model with pulldown-cmark's
`into_offset_iter`, one stack of open blocks and one of open marks. Each text
event becomes one piece, so text an entity split across events needs no
merging: the pieces are adjacent and the runs built from them join.

Four event-stream facts shape the code. A tight list item has no paragraph
event, so the item's inline bytes are collected and the missing paragraph is
synthesised at close, keeping the model's rule that inline content lives in a
leaf. An item that holds nothing emits no event at all, so an item with no
children gets an empty paragraph at its content start — past its bullet —
because an editor needs a line and a caret position there. A display formula
arrives inside a paragraph, so a paragraph whose only
content is `$$…$$` is rebuilt as a math block. An emphasis, strong, strike or
link event spans its delimiters, so a mark's range is assembled from the first
and last content event it contains — the alternative, trimming a known
delimiter, fails on `_x_`, `***x***` and nested marks. The start event's range
is the whole construct, so it becomes the mark's `full` without arithmetic.

Byte spans point into the original source, so an insertion shifts them exactly
as it shifts the source, and a caret offset, a mark and a block all compare
without a line/column conversion. Text the parser decodes (`&amp;` becomes `&`)
is laid out decoded while edits still slice the source.

## Marks

An inline attribute — emphasis, strong, inline code, strikethrough, a link, a
footnote reference — is a `Mark`: a content range, the whole construct in `full`, and a kind, held in
one flat list on the document, not attached to a block. A link may therefore start in one paragraph and end in
the next, which a tree of nested style nodes cannot express; Flutter's nested
`TextSpan` pays for the opposite choice with a semantic tree polluted by
style-only nodes.

Each block asks for its own slice with `marks_in(block.span)`, which returns the
overlapping marks clipped to the query. Clipping is the whole mechanism: a
block turns its slice into runs without knowing which other blocks share the
mark, and overlapping marks on one range compose (strong inside emphasis is two
marks, not two levels).

## Runs

`ui_richtext::to_text_block` builds one leaf's `TextBlock`: it cuts each piece at
every mark boundary and hands each resulting run to a caller-supplied `look`,
which may set ink, weight, decoration or anything else a run carries. The
document states semantics, never looks — emphasis is italic only because a
theme says so, and a link's ink belongs to the component.

A piece whose decoded text is shorter than its source stays whole, because byte
arithmetic no longer maps a source offset inside it. A delimiter cannot sit
inside an entity, so no mark boundary lands there in practice. Runs are not
merged back together afterwards: the converter sees an opaque `look` and cannot
tell two runs apart.

Two Markdown decisions live in the import: a soft break becomes a space and a
hard break a newline, so a paragraph flows instead of breaking at every newline
in the source.

## Corrections

**A footnote reference was about to lose its bytes.** Turning on
`ENABLE_FOOTNOTES` makes `[^1]` arrive as `FootnoteReference` instead of a text
event, so a parser that only opened the definition's container would drop the
reference from the body: the event matched the catch-all arm and the bytes
vanished. The parser keeps the bytes in a piece and marks them
`MarkKind::FootnoteReference`, so the source form writes `[^1]` and the scene
draws the label as a raised object; the definition itself is a
`BlockKind::FootnoteDefinition` container whose marker keeps its own cell and
whose paragraph is the note. Checked by `a_footnote_reference_is_a_mark_over_its_bytes`
(the mark covers `[^1]` and carries the label), `a_footnote_definition_holds_its_paragraph`
(the body still reads `Body[^1].` and the definition holds the note),
`a_footnote_definition_keeps_its_marker_and_note_on_their_lines` (the scene
keeps `[^1]: the note` on one row, the marker in a cell and the note as the
row's content) and `a_footnote_reference_draws_its_label_until_the_caret_reaches_it`
(rendered is `ObjectKind::Footnote`, source is the bytes).

**Front matter vanished from the editor.** The row was a bare `RowKind::FrontMatter`
drawn as a zero-height box with no `Form`, so it was hidden in every mode — a
document opened for editing lost its metadata block entirely, with no way to
reach or change it, while the status called it "kept whole for editing". Every
other opaque leaf (`Code`, `Math`, `Image`, `Rule`) carries a `Form` and shows
its source when focused. `RowKind::FrontMatter { form }` now does the same, and
the widget shows the source whenever the control is editable or the row is in
source form; only a reader keeps the zero-height box. Checked by
`a_focused_front_matter_row_shows_its_source` (focused is `Source`, unfocused is
`Rendered`).

**An HTML block's bytes were dropped, not passed through.** The status said HTML
was "passed through" as text, but a block-level `<div>…</div>` arrives as a bare
`Event::Html` with no paragraph around it. `note` pushed pieces onto the open
block, and with none open the text went nowhere: a document starting with an
HTML block parsed to zero blocks, and one inside a quote landed on the quote's
unread piece list. The parser now opens a paragraph for an HTML block when the
open block is not an inline leaf, and closes it again, so the bytes land in a
block at the top level and as a container's child inside a quote. Checked by
`an_html_block_keeps_its_bytes`: `<div>hi</div>` is one block whose text is the
HTML, and the same inside `> ` is the quote's child.

**Front matter was a block kind with no producer.** `BlockKind::FrontMatter`
existed, and the scene, editor and widget all handled it, but the parser never
enabled `ENABLE_YAML_STYLE_METADATA_BLOCKS`, so a leading `---\ntitle: x\n---`
parsed as a thematic break followed by a paragraph and the variant was dead.
The status line said front matter was not implemented while three layers
rendered it. The option is on now and `Tag::MetadataBlock` opens the block; its
span is the whole construct, fences included, because the editor draws it as one
fence whose payload is the bytes between the delimiters. Checked by
`a_metadata_block_is_front_matter`: the block's span is `---\ntitle: x\n---`,
and the paragraph after it stays a paragraph.

**A leaf was not going to store its text at all.** The first model kept only a
span, on the argument that two copies of the text can disagree. Building the run
conversion showed the span is not enough: the source spells `&amp;` and layout
must draw `&`, and decoding belongs to the parser that owns CommonMark's entity
table. The leaf now stores pieces, each pairing decoded text with its source
range, and the "two copies" risk turned out to belong to an editable model, not
to a parse result that is thrown away and rebuilt on every edit.

## Status

Implemented: the model, its nesting, mark clipping per block, a Markdown parser
for paragraphs, headings, quotes, both list kinds, code blocks, rules, display
and inline math, GFM tables (one header row, the delimiter row's alignment per
column, cells that hold inline content), YAML front matter (a leading `---`
metadata block, opaque to layout and kept whole for editing), footnote
definitions (a `[^label]:` container whose marker keeps its line and whose
blocks are the note), footnote references (a `[^label]` mark the reader draws as
a raised label), the emphasis, strong, strike, inline-code and link marks,
an entity mark over each complete `&...;` the decoder knows (never inside code),
and the conversion of a leaf into a `TextBlock` with the caller's look per run.

The `ui-widgets` `Markdown` control is the reader: it maps paragraph, heading,
quote, list, code block, rule, math and front matter to nodes, gives each mark
its look, highlights a Rust fence through [ui-highlight](./highlighting.md), and
takes its document from the caller. The gallery's Markdown page wraps it in a
card and reads its sample from the catalog so a CJK locale shows a CJK document;
the look of a construct lives in the control, not the page.

Not implemented: any HTML block or inline HTML beyond passing the bytes
through.

## Traps

| Change | Symptom |
| --- | --- |
| Keep only the text, or only the span | Layout cannot decode an entity, or a mark cannot find its bytes |
| Attach a mark to the block it starts in | A link cannot reach the next paragraph |
| Count characters instead of bytes | Offsets drift at the first non-ASCII character |
| Give a style its own nested node | Style-only nodes appear in the semantic tree |
| Take a leaf's span from its block event | The trailing newline and the `#` reach layout as text |
| Take a cell's span from its cell event | The cell's padding spaces reach layout, and every column measures wider |
| Take a mark's span from its delimiter event | The markers get the decoration the text should have |
| Read a container's span as its content | It carries the blank lines after the container and the next sibling's marker, so a blank line after a list counts as a list line |

## See also

- [Rich Text Stack](../reference/rich-text-survey.md): why this shape, and which
  parser, highlighter and math engine will feed it.
- [Inline Content](./layout-inline-content.md): what a leaf becomes for layout —
  runs, ink, decorations and atoms.
- [Formula Layout](./math.md): what a `Math` block becomes on screen.
