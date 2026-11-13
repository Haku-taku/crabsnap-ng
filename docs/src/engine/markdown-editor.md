# How should Markdown be edited in place?

The document is Markdown source, and the editor renders it where it is written:
syntax markers hide, the caret moves through the rendered structure, and the bytes
on disk stay the bytes the user typed. This page records the model that makes
that possible and the order it is built in.

## What the model was missing

The first Markdown page drew a document it could not edit. `Block::span` was the
content only, so the `**` around a bold run belonged to no block;
`to_text_block` dropped the source ranges it started from; a `TextRun` either
drew its text or was not there at all. The projection and the hidden-run form
below were added to close exactly those three gaps.

## How mature editors do it

There are two families, and they differ in what is stored.

| Family | Stored value | Hides syntax by | Markdown round trip |
| --- | --- | --- | --- |
| Source and decorations | the Markdown source | replacing ranges at paint time | exact, it is the source |
| Rich tree | a rendered document tree | there is no syntax to hide | lossy, Markdown is an export |

Obsidian's Live Preview and Zettlr take the first route through CodeMirror 6.
`Decoration.replace` hides a source range without removing it, `EditorView`
reveals a construct when the selection intersects it, and `atomicRanges` makes
arrow keys and backspace treat a hidden range as one unit
([Decoration](https://codemirror.net/docs/ref/#view.Decoration),
[the decorations example](https://codemirror.net/examples/decoration/)). The
document keeps one coordinate space, so every position is a source offset
whatever is hidden.

ProseMirror, Lexical and Muya store a rich tree. ProseMirror's author states the
consequence on the project's forum: "The conversion to and from Markdown is
currently lossy — it loses information such as the precise indentation, line
breaks, list marker characters used"
([discussion](https://discuss.prosemirror.net/t/possible-to-collaborate-in-different-docformat-of-the-same-document/63)).
Our document must round-trip byte for byte, which rules the family out.

Typora is the effect to match, not the architecture to copy. Its editor is a
rendered block DOM with Markdown as the save format (the build is Electron and
the Markdown is regenerated on save) `[unverified]` — judged from behaviour, not
from its source — so its editing guarantees are weaker than ours need to be.

## The contract: source-faithful runs

`Block::full` and `Mark::full` carry what the parser already knows and the model
discarded: the whole construct, delimiters included. `Block::span` and
`Mark::range` stay the content, so layout is unchanged.

`ui_richtext::project` cuts a leaf block's `full` range, minus its trailing line
break, into runs. The rules:

- every byte of the frame is in exactly one run, in order;
- a run's text is its own source bytes, verbatim — never a decoded form such as
  an entity's character;
- a run is content or the delimiter of a construct, and a delimiter belongs to
  the innermost construct that owns it;
- `shown(selection, focused)` marks each run drawn or hidden. Hidden runs keep
  their bytes, so a caret offset is a source offset.

Reveal is per construct, not per delimiter: while the selection touches a
construct's span, both of its delimiters are drawn, and while it does not,
neither is. A caret at either `**` of `**bold**` is a caret inside the
emphasis, so showing only the star it meets hides the other one and brings it
back as the caret moves between the ends — two states for one construct, which
the user reads as the text jumping.

A block marker belongs to the bytes its own line writes, not to the block: a
heading's `# ` and a quote's `> ` draw while the caret touches those bytes and
hide while it stands elsewhere in the block. With one input over the whole
source, focus no longer says which block is being edited, so a rule per block
would un-hide every heading at once, and a quote's whole prefix column the
moment the caret entered the quote at all. A fence is the one construct that
spans lines, so it watches its whole span and draws while the caret is anywhere
inside the block it delimits.

The engine hides a run rather than dropping it (`TextRun::hidden`). A hidden
span measures zero width, so the wrap breaks where the visible text would; the
shaper emits no glyphs and no advance for it, but still emits its byte
boundaries at the current pen, so every caret offset keeps naming its source
byte. A line's bytes are stored on the line rather than derived from its runs,
because an empty line a hard break opened has no runs but still has a position
that the caret must land on.

A marker cell does not remove its bytes from the flow, it covers them
(`Visibility::Covered`): they paint nothing but keep their advance, so the
marker has one geometry in both forms. Only syntax the rendered text does not
spell at all — a delimiter — is `Hidden`.

A row's cells open a column. The row is one text node, and each cell's room
travels inside that node's runs and atoms, so its first line starts at the
node's own left edge and `TextBlock::indent` stays zero. The cells' total is the
node's `TextBlock::hanging`: every later line of a wrapped item measures in the
width left after it and starts clear of the marker, instead of sliding under it.
A row whose cells spell no byte at all — a blank line inside a list — carries
their room as `indent` instead, because no run or atom does; its caret then
starts where its neighbours' text does.

A hidden run is atomic to the caret, as CodeMirror's `atomicRanges` is: an arrow
key or a deletion whose end lands inside a hidden run moves to the run's edge in
the direction of travel instead, so one press crosses the whole delimiter and
never leaves the caret where nothing is drawn. A deletion that would take part
of a run takes all of it — deleting one `*` of a hidden `**` would change the
Markdown rather than the text the user sees. The engine asks a block for its
hidden runs (`TextBlock::hidden_ranges`) and the editor answers from the same
projection that set the flags, so motion and the drawn state cannot disagree.

A press past the drawn text resolves to the first of two boundaries at the same
x, which is the near edge of a trailing hidden run: clicking at the end of
`**bold**` puts the caret at 6, so typing keeps the new text inside the emphasis
instead of after its closing stars.

Every block draws a line, an empty one included. CommonMark keeps a construct
with no content as a block — `1. ` is an item whose paragraph is empty, and a
fence line with nothing under it is a code block of no text — and each of them
is a position the caret has to reach, as it is in every Markdown editor. So the
builder gives a childless item a paragraph at its content start, `project`
accepts a frame that holds no run, and the widget gives such a frame one empty
run to give it a line box.

The editor applies a mark's look from the document rather than through
`to_text_block`: a link's ink and underline, an inline formula's ink, the code
fill, and the delimiters it hides are all the control's, so a mark reads the
same inside the editor as on the read-only page. A fenced block's highlighting
is not the document's, because a grammar and its colours belong to the page:
`code_style` takes the language, the block's content and the base style, and
returns runs whose text must concatenate to that content byte for byte. The
editor splices them where the content runs were, so the caret stays a source
offset.

The whole editor is **one input**, whose buffer is the document source, and
each leaf block is a *view* into it: a text node that draws the block's frame
and declares where that frame starts (`TextBlock::input_base`). A caret, a
selection and an edit are therefore in source offsets, exactly as in CodeMirror,
and a selection crossing several blocks is one range, not a special case.
`Backspace`, `Delete` and `Paste` go through the engine's ordinary buffer edit,
so a deleted line break merges two blocks on the next parse.

The engine finds the views by walking the input owner's subtree for text nodes
that declare a base (`ui/crates/ui-platform/src/view.rs`, `input_views`). A
press names a view and the engine reports that view's base plus the local
offset; motion lands in the dragged block's coordinates and is moved to the
nearest drawn byte when a step falls on a line break no view draws
(`snap_to_drawn`). Up and Down hold their column across both wrapped lines and
block boundaries, walking into the neighbouring view when a block has no line
left (`goal_x`). That crossing takes the first view in the direction of travel
whose line does not overlap the caret's, siblings before anything outside the
input, and never asks the neighbour to align with the column: a short line in a
narrow nested block would otherwise drop the step.

A display formula is a view of its payload, though it draws no text line: a
vertical step measures it by its box, and a step into it lands on the stop the
formula's own hit test puts nearest the column, read from the edge the step came
through. A horizontal step at the payload's first or last stop is one the
structure declines, so it is the document's, like every other declined key.

A list item's marker is bytes no leaf frame holds, so `ui_richtext::marker`
names them: the item's start up to its first child's frame, cut at the line
break. The scene gives every container marker a **cell** of its own
(`ui_richtext::Cell`) holding those bytes as a view, which is what lets the
caret step onto a `- ` or a `1. ` and delete it like any other byte. Its caret
positions stop at its last byte: the position after the marker belongs to the
text that begins there, so one caret stands between the two views, and typing at
a marker's end grows the text, not the marker. While the
selection is away from the marker the cell covers the bytes and draws the label
they stand for — a bullet, or the number as written — and its room is the same
in both forms, so the text beside a marker never moves when the marker appears.
The bytes keep their own geometry under the label too, so a caret can stand
inside a rendered marker and a step maps its column to the byte under it.

A cell's width belongs to the construct that writes it, not to the line: every
item of one list, and every line of one quote, takes the widest of its written
markers or the width its stand-in needs, whichever is larger
(`Markdown::cell_rooms`). A line that continues the construct without writing
the marker — a wrapped paragraph's second line, a lazy quote continuation, a
blank line the construct still has a block below — stands in a spacer cell of
that width, so the text keeps one column and a caret on that blank line starts
there too. A blank line after the construct's last block is no line of it and
takes no room, and a `>`-only line is a quote line because it writes the marker.
Which construct a line of its own stands in is decided by bytes, not by the
marker: the line's first byte must lie inside the construct's `full` range, so an
earlier quote of the same document — it spells `>` at the same column — never
claims it.
A cell declares its content's alignment — a bullet at its first byte, a number
at its dot — and both forms honour it: a `nowrap` line never breaks, but it
still sits in the box the cell gives it, so a `9.` and a `10.` end on one dot.
Whitespace the source spells at a cell's end is the cell's own room, not part of
the label: the room is the wider of the source's own advance and the label plus
the gap the source spells, which `Markdown::kept_space` measures as the
difference between the source's advance and its trimmed advance. The bytes and
the label therefore end at the same edge and the text beside them starts at one
column in both forms, including while the caret is in the marker. Indentation the source spells before a nested
construct names the column its container already reserves, so those bytes join
the continuation cell ahead of them instead of opening a cell of their own: a
nested bullet stands at the parent item's content column, and a code block, quote
or paragraph under the item keeps that column. Only a marker that follows a
written cell indents past its construct, which is the one column nothing has
opened yet. The marker's bytes and its label share
the body's line box: both carry the same half-leading and line gap, so a bullet
or a number sits on the item's first line.

A continuation line's own marker begins past the columns its containers reserve.
A setext heading is underlined on a line the item's indentation opens, so those
spaces join the cell whose column they spell and the underline is the line's own
syntax: hidden while the caret is away, drawn when it arrives. A leading run of
spaces alone is indentation, not syntax.

A GFM task item is the same marker with a box drawn in its place. The parser
records the box an item opens with (`BlockKind::ListItem { task }`), and while
the caret is away a second cell after the bullet cell draws it: a checked box
filled in the link ink with a glyph, an unchecked one outlined in the muted ink.
The `[x]` bytes stay that cell's view, so a caret that enters the item gets the
source back, and the item's text keeps the list's one column. A press on the box
flips that byte through `Cx::edit_input_detached`, so it works wherever the caret
is and leaves it there; the press is the box's own, so the cell's own view never
reads it as a caret placement.

A quote writes `> ` on every line, so each line carries a marker cell, one per
quote it stands in. The bar is what the cell renders as: it is placed against
the cell's box and stretches over the row's whole height, so a quoted line that
wraps keeps one unbroken bar, and a nested quote sets a second cell inside the
first. The bytes and the bar take the same cell, so revealing the marker moves
nothing beside it. A press anywhere in the cell puts the caret at the marker's
first byte, which is how a press on the bar enters the quote. A line with fewer
markers than the one above it continues the inner paragraph, which is
CommonMark's lazy continuation: the nesting is real, and the lazier line is the
inner block's text — it takes spacer cells, so it lines up with the inner
text.

The app applies an edit by replacing its copy of the source with the reported
buffer and reparsing next frame; it never mirrors the tree. A plain Enter in a
document input writes a line break (`Decl::text_input_enter_newline`) instead of
activating the node, and the newline is the block split the next parse sees.

## Read or edit, one control

`ui_widgets::Markdown` draws the document whether or not `editable` is set, and the flag
changes no block's kind, place or look. The same placement, the same mark look and the
same page-supplied highlighter run in both modes, so a document reads exactly as it will
read while being edited. What the flag adds is entirely the input: the column becomes a
focusable text input over the whole source, each leaf block declares its `input_base`, a
press lands the caret, and the projection reveals the syntax under it. While it is off no
node declares a buffer, a caret or a selection, and a press on a block does nothing.

The gallery draws both cards from this one control: the reading card is `editable: false`
and the editing card is `editable: true`, over one sample and one palette. The page keeps
only what is genuinely its own — the colours and the grammar behind `code_style`.

Two details come from the switch. A card's layout tag is the card's role, never the mode:
a page with two editable documents would otherwise give both cards one column id and the
tree would keep only one of them. And reading still builds its runs through
`to_text_block` while editing builds them through the projection: reading decodes an
entity, which the projection cannot do without moving the bytes after it. An inline
formula is the other way round — both modes draw it as a box, an atom spanning the
formula's source bytes so no offset moves.

## The control's own keys

`Enter`, `Tab` and `Shift+Tab` are the document's, not the engine's: the control declares
them (`BREAK`, `INDENT`, `OUTDENT`) on its column, so `Enter` writes a break through
`ui_richtext::line_break` rather than the engine's plain newline. On a list item's line the
break repeats the marker — an ordered item counts up — and on one whose content is empty it
removes the marker instead, which ends the list; a quote's markers repeat the same way, so a
break inside `> > inner` continues both levels. A quote's break writes the blank marker line a
paragraph break needs (`\n> \n> `), because the editor draws blocks and a soft break would
join the next line to this paragraph; the quote draws that bare line as a row of its own.

`Tab` indents the caret's line by the width that nests it under the item above — that item's
own marker, trailing space included — and `Shift+Tab` takes one step back. Two spaces is the
floor, for a paragraph.

Each command is applied as **one engine edit** through `Cx::edit_input`: the build asks for a
replacement with the caret where the user continues, and the engine writes it with the same
`apply_edit` a keystroke goes through. The edit is computed against the state the chord was
pressed in — the engine snapshots the focused input's buffer and selection when the chord fires
and routes them with it — because one frame answers a whole burst, and the live selection may
already belong to a later key. So the app's copy, the undo history and the drawn line
all take one step — without that door the control could only change the text the app rebuilds,
and neither the caret nor the history would follow.

## While an input method composes

The preedit is drawn where the caret stands, but the bytes under it must not
change look: several input methods cancel a composition when the text it is
replacing is swapped out. Reveal is therefore frozen for the life of a
composition. `Cx::reveal_range_for` reports the selection the composition began
with instead of the live one, and the scene is cut from that, while the caret
and selection the control draws still follow the input. A press or an arrow key
during composition moves the caret without flipping a delimiter to source. The
hold ends with the preedit, and the reveal follows the caret again; the commit's
own edit is an ordinary one and reveals what it lands in.

## An object in the source

An inline formula is the first place the editor draws something that is not text. Its
content run keeps its bytes and hides them, and an `AtomBox` spanning exactly those bytes
is placed at the run's start: the text pass draws the box, advances the pen past its width
and keeps both edges as caret positions, so the bytes after the formula stay where the
source puts them. `AtomBox::len` is what makes the span explicit — a reader's atom stands
for one U+FFFC, an editor's for the whole `$…$` content.

The box is drawn only while the caret is away from the mark. Touching the mark — anywhere
in its `full` range, `$`s included — drops the atom and shows the source, so the formula can
be edited like any other bytes. Two engine facts make this safe: the run is hidden, so
motion treats it as one atomic step rather than stopping inside it, and the atom is read
before the hidden branch in the paint pass, because an atom's bytes are hidden by design
and the hidden branch moves no pen.

A display formula is the same object one level up, and in structural edit mode it keeps its
typesetting while the caret is in it. The block's payload is an editable math view over the
bytes between the `$$`s, and replacing that view with source text would take the structure
away exactly while the user needs it; the control draws the two fence lines around the
typeset payload instead. They follow the rule every fence construct already uses — the
caret or selection touching the block reveals it — and each fence is a view over its own
`$$` bytes: drawn while the block is revealed, and standing with no height and no ink while
it is not, so a press or a drag past the payload reaches those bytes before anything has
revealed the block. The caret stands on them, a selection covers them, and a key typed
there edits the document. The payload view covers the payload's bytes only, so a structural edit
rewrites those and leaves the fences and the breaks around them alone. A key the structural
editor declines, such as `Backspace` at the payload's first stop, goes back to the document,
which deletes the byte beside the view: no keystroke is swallowed at a formula's edge.

## Scale

The control redraws its whole document every frame, so anything linear in the document in
that path is paid sixty times a second for nothing. Two of them were: the gallery page
parsed both cards in the page body, and the control cut a projection per leaf block.

Measured on a 246 KB document of 2000 paragraphs (`ui-richtext`, release): a parse takes
2.7 ms and projecting every leaf 6.7 ms, so two cards cost about 19 ms per frame — more
than a frame's budget. The app now parses once per source change and cuts
`ui_richtext::Prepared` beside the parse; `Markdown::prepared` takes those frames, and a
frame only positions runs.

Neither step is linear in the document any more. `Document::reparse` finds the edited bytes
from the common prefix and suffix, widens them to the top-level blocks that cover them plus
one guard block on each side, reparses that window alone and shifts everything after it by
the length change. A window that does not land on old block boundaries, or a mark crossing
it, falls back to a full parse. `Reparse` also names the splice, so `Prepared::update`
keeps the left frames as they are, shifts the right ones without cutting their runs again,
and projects only the window's leaves. Measured again on the same 246 KB document
(release, `#[ignore]` benchmark): parse 1.18 ms against reparse 0.44 ms, projection
3.62 ms against update 0.80 ms — a keystroke costs 1.24 ms instead of 4.80. What is left
linear is copying: `reparse` builds a fresh `Document` from `&self`, and `Prepared` owns
each run's text, so both walk the document to copy it. A consuming or in-place reparse, or
runs sharing their text, would take that too.

## What the editor deliberately does not do yet

- An image draws as a picture only when it is the whole of a paragraph. Inline, repeated
  or nested in a list item the reader shows its alt text with the syntax hidden around it;
  the editor shows the whole construct once the caret is inside it. No view places a
  picture inline.
- A table is edited as its source, never by cell: a press on the grid names the byte under
  the pointer inside that cell, so there is no `Tab` between cells and no command that adds a
  row or a column. The cell texts are ordinary runs, so typing in the source state edits them
  like any other text.
- An inline formula is an atom, not a node, so a press can name an edge of its
  bytes but nothing inside it; its LaTeX is reachable from the keyboard or
  Source mode only. A display formula is a node and takes a press anywhere,
  because its editor maps points to its own source bytes.
- A selection that begins inside a formula and ends outside it is the document's,
  so replacing it splices raw bytes and the model resyncs from the app's text:
  bytes outside a formula have no structural meaning.
- The parts of a formula's export the writer normalizes (spacing after a
  command) mean a hand-typed payload maps approximately until its first edit
  writes the export back.

Backspace at the document's first byte is a no-op, which is what one buffer wants:
every other block boundary is an ordinary byte the edit can delete.

## Fenced code cards

A fence away from the caret draws a card: a head with the language and copy, the code without
its fences, and a foot with the line count. The corrections below record why each band is
shaped the way it is. Three independent options shape the body (`CodeOptions`, mirrored on
the page's three switches in the gallery):

- `line_numbers` — a column of source-line numbers at the body's left, on a darker band that
  runs the body's whole height from the card's own edge, so the card's inset never rims it.
- `wrap` — on, lines break at the card's width; off, a line is laid out whole and the body
  scrolls sideways under it (`Sense::SCROLL_OWNED`, a plain wheel as on any single-direction
  container, shift+wheel, a trackpad's own x delta, or a thin bar that fades in over the last
  line while the pointer or a wheel is on the strip).
- `max_height` — `CodeHeight::Lines(n)` (n code line boxes) or `Px`; past it the body
  scrolls vertically in the engine's own scroller, and shorter code hugs.

A fence's own indentation belongs to its block, not to its container: CommonMark removes up to
that many columns from every content line. The block's lines each carry their part of that value
as ordinary text: `TextBlock::structures` names, per line, the byte range that holds it
(`Structure { group, range, shown }`). The value begins after the markers that line writes, and a
line that writes none — an item's continuation line, the fence's own line under it — has the
column its container reserves as part of the value there. So every line's value is the same run
of spaces, the fence line's (`shown: true`) and the copies' (`shown: false`) alike, and an edit
to one is an edit to all. `scene::code_value` computes it.

The fence line's leading bytes are drawn where they stand. The card gives up that column — in the
source form its left edge moves right by the block's own width and its right edge stays — so the
bytes stand in the room it leaves, drawn as text: a caret stands in them, a selection highlights
them, and the member the source spells is edited there. In the card's own form the same view takes
no room and draws nothing, so a press on the block's spacing still enters the block. The other
lines' copies take no advance of their own: they stay in the body and foot as
`Visibility::Covered` with a zero `Room`, which keeps them caret positions and bytes an edit can
name without letting them push the code sideways. Only inline syntax the rendered form does not
spell is `Hidden`, so a step and a deletion cross it rather than stand in it.

A container's cell draws the bytes of a line that writes its marker, so those bytes get no view
of the block's own: a fence that opens an item's line has the item's padding on its left, and
those bytes are the item's. A fence after a quote's `>` has a cell that runs to the content's
column, and the cell's view carries the member the source spells. A line that writes no marker
has nothing on it but the block's own indentation, so the block's view draws it from the row's
left edge and the card starts after the column the source writes.

An edit inside the spelled member that writes only spaces loads the new value onto every member
of the group in `edit_structural`, within the same `apply_edit`: one `Change::Text`, one undo
step. A member at least as deep as the old value moves by the same delta; a shallower one keeps
the spaces it has, because the parser strips all of them and giving it more would put a space
the reader would see into the code. A block with no indentation yet carries the group the same
way: the fence line's member is empty, so the first space typed there is the block's and every
line takes it. The content lines' members are ordinary text too — a deletion inside one edits
that line alone.

A marker the source writes is never part of the group, and neither is the item padding after it.
A fence that opens a list item's first line has no value of its own: the spaces after the marker
are the item's padding, and a typed space there grows the padding or, at four, pushes the fence
into indented code. Such a line therefore spells no member, and edits in those spaces stay plain.

Home stops at the line's first non-space and, pressed again, at its first byte, as in other
editors; the leading spaces are ordinary text, so a caret can stand anywhere among them.

An indented code block is not a fence and draws no head or foot: CommonMark gives it no fence,
so its first and last lines are code even when they read like a fence
(`BlockKind::CodeBlock { fenced: false }`). Its text draws as code, with the card's body alone.

The closing fence is found by its bytes rather than by its line. The
break before it can be deleted — the fence then ends the last code line and the block is
unclosed — and the card still draws that fence on a line of its own, the body stopping where
the fence begins. When the fence does start a line, the foot owns that whole line, the
containers' prefix and the block's indentation included, so a quoted closing fence's bytes are a
view like any other and its share of the value follows the fence line. Every byte stays where
the source put it, and the line the fence stands on is the layout's, not a byte of text.

The numbers are painted by the text pass, the only place that knows where the shaper broke a
line. That pass numbers source lines only: a line a wrap opened carries no number, and every
line starts at the edge the gutter leaves it. The column itself is a second text node laid
over the body's left edge: the code's own bytes with every run `Covered`, the same width and
style, so its line boxes fall where the body's do however a line was broken. It paints
nothing but the numbers, and it travels with the code when the cap scrolls.

The band is the body's own surface, not a layer over the code: it spans the body from the head's
rule to the card's bottom edge — the foot included — and starts at the card's left edge, so the
card's inset never leaves a rim of the code's fill around it. The band is as wide as the widest
number plus half the number gap on each side, and a one-pixel rule ends it: the digit stands the
same room from the card's edge and from the rule. The band's tint stops at the rule, so the rule is
where the gutter's background gives way to the code's own fill; the code's first column follows the
rule, one number gap in from the widest digit. The sideways strip's clip therefore starts past the
rule: a scrolled line is never drawn under the numbers, and the band only has to paint its own
surface, not hide what slides beneath it. Band, rule and numbers are three nodes; the band and the
rule are absolute children of the body, the twin is laid at the row's edge with the code's column
as its indent.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui/crates/ui-richtext/src/document.rs` | `Block::full`, `Mark::full`, and their clipping |
| `ui/crates/ui-richtext/src/markdown.rs` | fills `full` from the parser's start and end events |
| `ui/crates/ui-richtext/src/editor.rs` | the projection, constructs and reveal, and `Prepared`, the frames cut beside a parse |
| `ui/crates/ui-richtext/src/scene.rs` | the rows, and the marker `Cell` each container stands in |
| `ui/crates/ui-richtext/src/ops.rs` | the line break that continues a list or quote, and the indent step |
| `ui/crates/ui-core/src/text.rs` | the run visibility that hides or covers a run without dropping its bytes; a line's own byte range; `TextBlock::hidden_ranges`, `line_numbers`, `structures`; `input_base`; an atom's `len` |
| `ui/crates/ui-core/src/text_input.rs` | the plain insert, delete and range helpers, and `edit_structural`, which carries a spelled block value to the members of its group in one edit |
| `ui/crates/ui-core/src/layout_engine.rs` | the box a `nowrap` text is measured in, which is also the box its lines align inside |
| `ui/crates/ui-core/src/tree.rs` | `text_input_enter_newline`; a node's own chord bindings |
| `ui/crates/ui-core/src/cx.rs` | `InputEdit`: the edit a build asks an input to make |
| `ui/crates/ui-platform/src/text_pass.rs` | measures hidden runs at zero and covered ones in full, shapes no glyph for either, keeps their boundaries |
| `ui/crates/ui-platform/src/view.rs` | `input_views`, `input_structures`, atomic motion and deletion, motion across views, `goal_x`, `apply_input_edits` |
| `ui/crates/ui-widgets/src/markdown.rs` | block look, the views, the tail line, the IME splice, the control's own keys, the code card's gutter, wrap, height cap, fence dedent and closing-fence detection |
| `ui/crates/ui-gallery/examples/gallery/pages/markdown.rs` | both cards from the one control, and the palette it needs |
| `ui/crates/ui-gallery/examples/gallery/app.rs` | the parsed trees and their frames, rebuilt only when a source changes |
| `ui/crates/ui-gallery/examples/gallery/selftest/markdown.rs` | covered marker, reveal on focus, a column step, an edit reaching the app, undo, both cards' geometry, the control's keys |

## Phases

0. **Model** — `full` on blocks and marks, the projection, the reveal rule, and
   tests pinning the byte invariant.
1. **Engine** — a hidden run form through `TextRun`, `Span` and the text pass;
   multi-line paste for a multi-line input; line ranges for blank lines. Done.
2. **Block editor** — paragraphs, headings and code; click to place the caret,
   typing, reveal by focus and selection, the list and quote chrome; a gallery
   card and self-test. Done.
3. **Inline constructs** — the five inline marks, atomic motion and backspace
   over hidden delimiters, reveal by selection. Done.
4. **One input over the source** — views instead of per-block inputs, selection
   across blocks, Up/Down across blocks with a held column, plain Enter as a
   line break, `$…$` and entity objects, IME freeze, incremental reparse. Done.

## Corrections

The fence lines were drawn but could not be edited. They were plain text, not views, so
`input_views` never named them: Left at the payload's first stop jumped over the opening
`$$` into the block before, and a document ending with the closing fence could not be
selected to its end — its last two characters were out of reach. Each fence is now a
`text_frame` view over its own bytes, the same door a thematic break's source already used,
so every byte the block draws is one the caret can stand on and the document edits. Pinned
by `markdown_math`'s `a_display_formulas_fences_are_caret_stops_of_the_document` (red: the
closing `$$` never appeared in the caret's walk, and the selection stopped at the payload's
end).

That view still only existed once the block was revealed, and reveal follows the selection:
a drag towards the fence spent its first motion on the payload, because no view stood where
the fence would be, and only a second motion — after the reveal — could land on it. The
fence now always stands as a view: painted with its line while revealed, and zero-height
and unpainted while not. A press there answers with the view's end, so the caret lands on
the fence's last byte and the block reveals in the same frame. A tie between two views at
one distance now goes to the later one, as a caret on a boundary already does, which is
what lets the fence below a formula take the drag. Pinned by
`markdown_math`'s `one_drag_reaches_the_fence_without_visiting_the_formula_first` (red: the
selection stopped at the payload's end).

A thematic break drew a line and nothing else, so it had no view at all: a press fell
through to the nearest neighbouring text, and no arrow could put the caret on the rule's
bytes — the one stretch of the source an editor could neither click nor select. Its
container now carries a hidden run over the rule's bytes with `input_base` set, so a press
on the line lands after those bytes, the block reveals and the source is drawn. Pinned by
`markdown_math`'s `a_press_on_a_thematic_rule_puts_the_caret_in_its_bytes` (red: the rule
was not a view at all).

A display formula's view covered the whitespace around its payload. The parser's block span
runs between the `$$`s and keeps the breaks they enclose, while the editor's source was the
trimmed payload; a click at the formula's end therefore landed two bytes early, and the
first keystroke spliced the export over the breaks, folding the block onto one line. The
view now takes the payload's own bytes, so its range and its source name the same text.

Backspace stopped at a display formula's first stop. The structural route consumed every key
the editor declined, so nothing could delete the bytes before the formula: repeated
Backspace emptied the payload and then did nothing. A declined key now falls through to the
document, which deletes the byte beside the view or moves the caret out of it.

The arrow keys could not leave a display formula. Vertical steps asked every neighbouring
view for a text line, and a formula draws none, so the crossing skipped the block and landed
in the paragraph past it; a horizontal step at the payload's edge was reported by the
structure as a change it never made, so the document never saw the key. The formula now
answers a vertical step with its own hit test and a boundary step declines, and
`markdown_math`'s `the_arrows_walk_the_caret_out_of_a_display_formula` walks the caret out
to the left, down and up again.

The first plan folded markers by deleting them from the text the input carried,
which puts the caret in a second coordinate space and needs a mapping table that
every edit and IME composition must pass through. Reading CodeMirror 6's
decoration model showed that a hidden range still occupies its bytes; the
projection now keeps them, and the invariant test
`every_byte_of_the_frame_is_one_run_of_its_own_source` pins the result.

Blocks were first the unit of editing: one input per leaf block, with the app
splicing each `Change::Text` into the source through a recomputed frame. Two
things forced one input instead. A selection could not leave a block, so the
control was several pieces of content to the user, and every edit moved the
block ids the app had to look up. The one-input design dissolves both: offsets
are source offsets, and the app replaces its whole source. The engine reports a
press, a step and a deletion in buffer space for any input that declares views,
so `a_selection_spans_the_blocks_of_one_input` pins the behaviour rather than a
widget workaround.

Reveal compared the selection with construct spans in document offsets while
the caret it was given was local to the block's frame. In a document whose
first block did not start at zero the two spaces drifted apart, and a caret
could light up a hidden run some distance to its left — reported as "the caret
reveals a hidden block to the left". The fix is the coordinate space and
nothing else: the editor passes document offsets, because a construct span is
in that space and the projection has no other. The first attempt also narrowed
reveal from the construct to the delimiter the caret touched, reading the
offset bug as a span bug; with the offsets right, that narrowing hides the far
delimiter of `**bold**` while the caret is inside it, which the user then
reported. `the_construct_under_the_caret_shows_both_delimiters` and
`a_selection_spans_the_blocks_of_one_input` pin both halves.

An empty block drew nothing. The source `aaaaa```rust` on one line, then code,
then a closing fence, showed the paragraph and nothing at all of the fence: the
parser reports the paragraph and then an empty code block, and the projection
refused a frame with no runs, so the block had neither a line nor a view to put
a caret in. `cmark` and `cmark-gfm` render that input as one paragraph followed
by an empty `<pre><code></code></pre>`, because a fence must begin a line — the
parse was right and the drawing was the defect. The projection drew no line for
a frame with no run, so that block had neither a line nor a view to put a caret
in. Now an empty frame projects, a childless item gets a paragraph of its own,
and the widget gives an empty frame one empty run; a line holding only `1. `
is reachable too. `a_block_with_no_content_keeps_a_place_for_its_text` pins both
sources, and the self-test clicks the empty item.

The item's marker was placed with a row gap equal to the space the source writes
after it. That held while the marker was the marker; once it gave way to a label,
the two states differed in width — `• ` is wider than `- ` — and entering an
item moved the text beside it. Reserving the wider of the two fixed that half and
broke the other: every bullet item's content then started a bullet's width to the
right of where the source puts it, reported as the list's text no longer being
aligned. The cell is now the marker's own room, its trailing space included, and
both the marker and the label draw inside it, so the text starts where the source
puts it and does not move when the caret arrives. The self-test's `steady` flag
reads both states' row height and the text's x.

A quote's lazy continuation line kept its cell and its room but lost its bar.
The bar was drawn with the cell's rendered stand-in, and a cell that spells no
byte takes the branch that reserves width and draws nothing, so a line with no
`>` of its own went unmarked — the quote read as opening and closing on its
marked lines. A rendered quote cell now draws its bar whether or not it writes a
marker; the cell's room was right all along, so the text beside it did not move.
Pinned by `markdown_math`'s `a_quote_draws_its_bar_beside_a_lazy_continuation_line`
(red: no bar node stood at the lazy line's y).

The bar stayed beside a revealed `> `, so editing a quote line drew the marker
twice: a bar and the bytes it stands for. The bar now stands down while the caret
is in the quote and keeps its width, so nothing shifts; each line's prefix is a
view that draws on its own line only. The report that "quotes do not nest" came
from the same area: nested quotes did parse and did draw two bars, but the
read-only card joined the inner paragraph's lines, which read as the second block
being text of the first.

A fence nested in a list item kept the item's indentation on every content line but the
first: `Block::span` starts after the first line's indentation, because the parser's range can
only drop a prefix, and the card drew the span's bytes as they stood. The code read
`cargo build --release` flush and `   cargo test` three columns in — reported as a marker's
indentation reaching lines it does not belong to, and as a code block that appears indented.
Every line now hides up to the fence's own indentation (`dedent`), the bytes staying in the
block; `a_nested_fences_indentation_reaches_no_content_line` pins the flush body (red:
`"...\n   cargo test --all-features"` where the two flush lines belong).

The closing fence was taken to be the block's last line, whatever it held. Deleting the break
before it — the user's report: Backspace at the start of the closing fence line — leaves the
fence at the end of the last code line and the block unclosed, and the card read that code
line as the fence, so the body painted it and the foot painted it again, each holding the
caret for the same bytes: one line twice and two carets, which a step left made visible. The
fence is now recognised by its own bytes (a run at least as long as the opening one, at the
line's start or its end), and the body stops where it begins, so the fence keeps a row of the
layout's without a byte moving. `a_closing_fence_at_the_end_of_a_line_is_drawn_as_its_own_line`
pins one copy of the line, one caret and the fence on its own row (red: two copies, two
carets, and no fence row).

The read-only card was then made to keep the source's line breaks, so that a line
ending with a fence marker and the line after it stood apart as they do in the
editor. That acted on the request's premise rather than on the specification. A
fence must begin a line, so CommonMark parses those two lines as one paragraph,
and a soft break inside a paragraph is a space. The import is a space again
(`a_soft_break_is_a_space`), and the empty code block that follows such a
paragraph still takes its line. The editor never needed the change: it draws
source bytes, so it shows the lines the source has.

Phase 4 kept a list bullet outside every view, on the argument that chrome the
layout owns is not editable text. The consequence was that a marker could not be
reached at all: no caret could sit on a `- `, so it could not be deleted or
rewritten, and a list read as a fixed decoration beside the text. The marker's
bytes are source like any other, so they are a view of their own; what stays
outside the input is only the label drawn while the caret is away from the item.

The reading view was a renderer of its own, in the gallery page. It repeated the heading
scale, the quote bar, the list cell and the mark look, so the two copies drifted the moment
either changed: the read-only card joined a nested quote's lazy line, and it drew no markers
at all — right until the same document was read beside its editor. One control now draws
both, with `editable` choosing the input and the reveal and nothing else; the page keeps only
its palette and its grammar. Reading still builds runs through `to_text_block` and editing
through the projection, because a decoded entity or a formula atom needs a byte range that
still counts — the object-run work above is what closes that last gap.

The card's layout tag was written as "editing when editable, document otherwise". With the
mode flipped on the reading card the two cards reached the same id, the tree kept one of
them, and the page's column check read `document None` — a card that had silently vanished.
The tag is the card's role now, never its mode.

An inline formula's box drew in its place but took no room, so the words after it started
at the formula's own x and overlapped it. The paint pass read the hidden-run branch first,
and the atom's bytes are hidden by design, so its piece was skipped before the atom could
move the pen; the atom moved first for the whole line, not for one block. The atom is read
before the hidden branch now, and the gallery's `formula box` and `formula source` flags
pin that the box exists away from the caret and gives way to the source on it.

The chord's edit was computed from the live selection, which is not the selection the key
saw: one frame answers a whole burst, so Enter followed by Shift+Down in the same frame
replaced the line the arrows had just selected with a single break — a whole item vanished
from the source. The engine now snapshots the focused input, its selection and its buffer
when a chord fires and routes them with it (`Routing::shortcut_focus`), and the control
computes its edit from that snapshot (`Cx::shortcut_selection`); buffer and selection are one
state, so a later key cannot change what the earlier one asked for. The gallery's `burst` flag
pins it: with the live selection back, `"- burst tail\n- after\n"` becomes
`"- burst tail\n\n"`.

A quote's break first wrote `\n> `, which continues the paragraph on the next line rather
than ending it — and the editor draws blocks, so the line joined this one and the break looked
like nothing happened. A paragraph break inside a quote needs its blank quote line, so the
break writes `\n> \n> `. That line holds only markers and had no view at all, so the caret
could not stand on it and the new line drew nothing; the quote now places a tail row for a
bare marker line at its end, measured from the block's own source. A second break on the line
the first opened closes the quote, rather than leaving a marker line around nothing.

The list cell measured one item at a time, so content started at a different x wherever
markers differed in width — after a number gained a digit, or beside a nested list — reported
as the list's content not being left-aligned. One cell now serves the whole list: the widest
marker's room, measured once. The self-test's `stepped` flag reads a `9.` / `10.` list, whose
second item's text otherwise starts a digit further right.

Up/Down at a view's edge demanded that the next line lie in the same view, and where none did
the caret left the control for whatever focus navigation found — several lines below it,
outside the parent block. A step that stays on its own line now crosses to the neighbouring
view in the direction of travel, taking the first whose line does not overlap the caret's, and
positions the caret there by the held column: siblings first, with no x alignment required.
Marker cells and quote prefixes share the caret's line and are skipped, so a step moves one
visual line. The self-test's `crossed` flag reads the caret after one Down from an item: it
must stay in the input and inside the list.

A quote's bar and its `> ` each took leading room, so a quoted line started a bar's width
plus the prefix's width in from the quote's edge. They say the same thing and never show at
once, so they now share one gutter: as wide as the widest prefix in the quote, with the bar
drawn inside its first `QUOTE_BAR` and the prefix text in the same cell. Content keeps one x
whether the caret is on the line or away, and the line no longer moves when it arrives. The
state model later made a line's prefix runs of its own text, so the two no longer share a cell:
the bar keeps its column, the drawn bytes take their width beside it, and the line now steps
aside by its marker's width when the marker appears.

The quote's bar was one rounded rectangle per row, so a quote of several lines drew a row of
lozenges: each rectangle rounded both its own ends, and the joins read as breaks in the bar. The
room after a nested quote's bar was that bar's padding, and a box's background reaches across its
padding, so the two bars touched with no gap between them. A row's bar now rounds only where the
quote's run of rows begins or ends and stays square where the next row's bar takes over, and the
room after it is a margin, which no background paints. The joins are a visual property with no
assertion of their own: the check is a screenshot of a three-line quote with a marker-only line
and a nested line, which shows one unbroken bar, the nested bar inset beside it, and each row's
text one bar's width in from its own bar. The self-test's `nested` and `bar` flags pin only the
state half — a bar drawn away from the caret and standing down on its marker.

Blank lines between two blocks were drawn by nothing: the block gap stood for the whole run,
so a document with three blank lines looked exactly like one with a single blank line and the
caret could not stand between them. The gap stands for the first blank line only; every
further line of the run gets a line box of its own (`blank_rows`, fed by
`ui_richtext::blank::blank_runs`), and those boxes are views of the source, so the caret
reaches each of them. The count stays the source's: `blank_run_at` answers the run and the
line's place in it from the bytes, whatever the drawing chose to show. A list item's blank
runs reach the same helper through its children.

A quote's blank lines are a different shape: each is a line of its own holding `>` and
nothing else, which the parser drops as a paragraph break, so it is no block and
`blank_runs` does not see it — it trims to `>` which is not whitespace. Those bytes showed
nowhere, and the caret could not stand on them or delete them. `marker_lines` now names
every `>`-only line in the gap before each child and after the last one, and each gets a row
of `[marker cell | empty line]` with both halves as views, the marker hidden until the
caret's line arrives like any other prefix. Consecutive marker lines share one column with
no gap between them, so the run reads as blank lines and not as paragraph spacing. The
self-test's `quoted` flag reads two such rows for `> a\n>\n>\n> b`, presses the second
line's body and expects the caret at the byte after its `>` with the marker now drawn; it
fails on every count when `marker_lines` answers nothing.

A fenced block away from the caret now reads as a card: a head naming the language (or
the catalog's `widgets-code-plain-text` when the fence names none) with a copy button, the
code without its fences, and a foot naming how many lines the code holds
(`widgets-code-line-count`, plural). The head and the foot are each a stack of their two
states — the language and copy button, or the opening fence; the count, or the closing
fence — so each takes the taller one's height and entering or leaving the block moves
nothing; the read-only card is always in the rendered state. A fixed gap separates the
head's rule from the first code line, and the foot's band is the count's own box plus its
insets. The copy button is bare at rest: its pill is an out-of-flow layer the engine fades
in under the pointer and out again after it, so the head reads as the language alone and
never blinks. The count is a note, not a badge — it owns no surface of its own. The copy
button copies the code, never the fences, through `Cx::copy_text`. A press on a control
drawn inside an input belongs to that control, not to the caret —
without that rule, clicking copy dropped the caret into the code and the card folded away
before the click completed (`a_press_on_a_control_inside_an_input_leaves_the_caret_alone`).

The card's own notes measured zero wide. The card built its language, its copy label and
its line count as hand-written `nowrap` text leaves with a `hug(0, 0)` size, and a
`nowrap` leaf reports `min(measured, hint - padding)`: a zero hint is a zero cap, so the
text painted but its box took no room, and "Copy" and the count ran past the card's right
edge. The notes now come from the text control, which is where that rule is stated and
which leaves the hint open (`max_width: INFINITY`) unless a caller asks for a clipped
line. The self-test's `card` flag reads the count's width as well as its text, which is
the fact the broken leaf could not produce.

The count chip hung off the card's bottom edge. The foot was a row of fixed height, taken
from one code line, and the chip centred in that band was taller than it, so its box and
the lower half of its label fell outside the card. A height taken from a child the row does
not measure cannot hold that child; the foot is now a stack that hugs its children, the
chip's row and the closing fence's line, and the taller one sets the band. The self-test's
`card` flag now requires the chip's box to end within the card's rect; over the fixed band
it fails on `rect.origin.y + rect.size.h`.

The card's chrome disappeared in the light appearance. The head's icon, its language label,
the copy label and the count all took the page's secondary ink, and that ink was one grey
pinned by hand, `150,158,176`: legible on the dark card, about 2.4:1 on the light one. The
page now takes that ink from the same MD3 pack its surfaces are painted with — the pack's
`onSurfaceVariant` — so the two move together when the appearance changes, instead of one
palette meeting two backgrounds. The same reading removed the noise the chrome had: the
pill behind copy was a permanent fill and is now a layer that fades in under the pointer
(100 ms, the pack's reaction), and the count lost its chip, which had made a footnote the
loudest mark on the card. Checked by screenshots of both appearances (`GALLERY_DARK=0|1`,
nested session); the fade was checked by building the same constant at 1 s and sampling the
pill's fill mid-transition, 0 → 30% → full. A page that pins a secondary ink by hand escapes
this; the pack lookup is what keeps the two in step.

A display formula written on one line drew its caret twice. `$$x^2$$` puts the
payload's first byte at the end of the opening `$$` and its last at the start of
the closing one; the widget gave a window its last caret position by
declaration, and the formula took both of its ends inclusively, so one position
was painted by two views — a bar at the end of the `$$` line and another at the
formula's first stop, one caret visible in two places. A window now owns its
last position only when the document writes no drawn byte there, which is why
the opening fence yields the payload's first byte, and the formula yields its
last byte to the fence that starts there (`Math::caret_ends`). Pinned by
`a_formula_sharing_a_fence_line_draws_one_caret_at_each_boundary`, which steps
Right across the block and counts every caret bar in the tree; with either half
back, the two boundary offsets read two bars.

A break in an ordered item left the items below repeating its number — `1. a` split gave
`2.` and then the old `2. b` again. `ui_richtext::renumber` now rewrites the markers of the
same-level items after the caret, from the number the new item takes; an item ended by a
break on its empty line shifts the items after it down onto its number. The bytes between
come along in the same edit, so one undo step still restores the list, and a nested list
keeps its own numbering. Pinned by `a_break_renumbers_the_ordered_items_after_it`.

An inline formula had one way to be edited: touch it and read TeX. The two modes now
differ by what the press acts on. `MathEditing::InPlace` (the default then; now
`Structural`, see below) leaves the typeset
box in place and lets the engine's atom hit test answer the nearest boundary, so a press on
the left half puts the caret at the `$` and one on the right half at the content's end, and
the reveal that was already there takes over. `MathEditing::Source` reveals the source and
floats a preview of the typeset formula under the caret's line. The preview is rebuilt from
the committed document every frame, so a live IME composition cannot rewrite it, and it is
an overlay, so opening it moves no block.

That is the v1 of editing on the rendering: mainstream editors (Typora, Obsidian, VS Code)
all show source plus a preview, and only MathQuill and MathLive put a caret on the rendered
formula, through a glyph tree of their own. KaTeX exposes no glyph-to-source map, and our
atoms carry no glyph spans yet, so a press can name an edge of the formula but not an inner
byte. The preview's anchor has the same hole from the other side: an atom has no node, so
the overlay hangs from the leaf's caret x and its block's bottom, and can overlap the block
below until atoms carry their own rects. The selftest's `math mode` flag reads all of it —
a press on the box reveals the source at its opening byte, Source mode floats a preview
whose text is the formula's, and no preview exists with the caret away or in InPlace mode —
and was watched false by dropping the mode from the preview's condition.

`Tab` used to answer only a collapsed caret: with a selection the indent answered nothing
at all. It now reaches every line the selection covers, with the step taken once from the
first line so the lines stay aligned, and a line the selection only touches with its final
byte is not one of them. The harder half was the selection itself: an edit could state only
where the caret lands, so indenting three lines collapsed the selection and `Shift+Tab`
took back one line. `Edit` now carries `selection: Option<(anchor, caret)>` and
`InputEdit` the same, `apply_input_edits` sets a range instead of a caret when it is
present, and the control passes the direction it saw so a backwards selection stays
backwards. It is still one engine edit, so one undo restores all the lines.

An entity could not be decoded because a run's text had to be its own source bytes: `&amp;`
is five bytes and draws one, and every caret offset after it would shift. The answer is the
one an inline formula already had — an atom that takes the bytes' place. The parser now
marks each complete entity whose written form the table decodes to pulldown's own reading of
it (`ui_richtext::entity::decode`, named plus decimal and hex), and never inside a code span
or a fenced block; the projection, seeing the mark, hides those five bytes and puts an
`InlineObject::Glyphs` of the decoded character in their place, measured and shaped like the
run around it. The atom spans the source bytes, so nothing after it moves, Right steps over
the whole entity and Delete takes all five. The caret inside the entity reveals the written
form, as any construct does. The table is the common subset, not HTML5's two thousand names:
an entity it does not know stays source in the editor and is still decoded by the reading
path, which takes pulldown's text.

Every edit used to reparse and re-project the whole document, and the parse was only the
smaller half: on a 246 KB file a keystroke cost 1.18 ms of parsing and 3.62 ms of
projection. Reparsing the edited window alone brought the parse to 0.44 ms, and the
keystroke only fell from 4.80 ms to 3.95, because the projection still cut every leaf. The
projection had to become incremental too, and that needed the parse to say what it
replaced: `Reparse` carries the old byte window, the length delta and the new blocks, so
`Prepared::update` can shift the frames after the window instead of cutting them again.
Both are pinned by one differential test — twelve edits, each asserting the incremental
document and prepared frames equal a full `parse`'s — which fails on the first edit when
the right-hand shift is left out.

An image paragraph is the third thing to take an atom's shape, and the first that paints
something the text stack cannot measure by itself: a texture. The page answers
`ImageSource` — a function from the written destination to a texture id and its pixel
size — and the control draws the paragraph as one picture, scaled to the column width, or
as its source when the page has no answer. The caret's block being the picture means the
picture's own height is the block's height, so the source state takes that height too and
leaves the rest blank: entering the block must not push every block below it down. Only a
paragraph that is nothing but one image becomes a picture; anything mixed keeps the alt
text as its drawn run, and that boundary is v1's, not the model's.

A table was left out of the plan because "cell spans and header rows are undecided". Reading
GFM's table extension showed the question does not apply to it: there are no column or row
spans, and a cell holds inline content only, so neither a spanning cell nor a nested block
needs a shape the model lacks. The undecided part belongs to HTML tables, which this parser
does not accept. The table is now three nested kinds with the delimiter row's alignment on
the table itself, pinned by
`a_gfm_table_is_one_block_with_aligned_columns_and_rows_of_cells`.

`project` answered a frame per leaf and nothing for a container, so a table had no frame
and could not show its source while the caret was inside it. The table is the one container
that projects: `project_table` takes every cell's `span` as content and everything between
them — pipes, padding, the delimiter row and the line breaks — as one hidden construct,
which keeps the byte invariant and makes the reveal the whole table.
`a_table_frame_covers_every_byte_and_shows_it_in_the_source_state` asserts the runs are
contiguous, cover the frame exactly, read as `abbcdd` with the construct hidden and as the
source with it shown.

The grid was first drawn with `Auto` column tracks, on the reading that auto-sizing means
"as wide as the content". The engine gives an `Auto` track its max-content width and never
takes any of it back for the space available, so a long cell pushed the grid past its card,
invisibly: the block's own width is fixed, and only the cells painted outside it. The view
now measures each column, scales the columns to the width it was given and emits `Fixed`
tracks that sum to it. The `wide` half of the `table` self-test sets a cell of 80 characters
and fails when the scaling is dropped, because the last cell's box then ends past the
table's.

A task item spelled its flag twice. A hidden marker's label was built from the whole
marker instead of from the bytes the label's own part covers, so a task item's stand-in read
`• [x] ` beside the box the marker's object part drew — reported as "the checkbox and `[x]`
both show". The label reads its own slice now: the box stands for exactly the bytes it
replaces, and the source state is the only form those bytes take. `marker_part` is where a
label is made, so no call site can hand it a wider slice; the self-test's `tasks` flag reads
the label's and the box's atoms.

A box was then placed on another card's row, or left at the origin. The measurer shares one
measured layout between text blocks whose runs and atom geometry match, and that layout
carried each atom's holder id; the reading card's task row and the editing card's task row
are byte-identical, so the second placed the first's children at its own coordinates and
never placed its own. A measured layout is geometry and nothing else: `InlineAtom` no longer
carries the holder, and placement and painting read the child out of the node's own block.
The cell was also the box and nothing after it, where the source writes a space, so the
item's text began against the box; the holder keeps that space now. `toggled` clicks the box
and reads the flipped byte.

Markers were first given room inside the row's text: a list item's marker was an atom the
body reserved, and a quote's `> ` a syntax run with the bar drawn beside it. Moving the forms
onto one state model took the marker out of the body's text and into runs of its own, and the
shared room went with it — the quote's bar and its bytes each took a column, so a line
stepped aside by its marker's width when the marker appeared, and a list's labels measured
themselves one item at a time. The room is now a **cell** of the scene (`ui_richtext::Cell`),
one per container marker, whose width the construct shares across its lines and whose two
forms — the marker's bytes, or the stand-in they render as — take the same room by
construction. The self-test's `quote_steady` and `steady` flags read the text's x before and
after a marker is revealed, `aligned` reads one column for a list's items plus one more cell
for the list nested inside it, and `stepped` reads a `9.` and a `10.` whose dots line up.

A cell had no indentation of its own, so a nested item's marker — which the parser records
after the spaces that nest it — matched no cell: the line handed its whole prefix to a
cell marked unwritten, the item drew no number, and the two items' text started at different
columns. A marker now matches the line's bytes from the cursor onward rather than from the
column exactly, and the indentation before it is a cell of its own
(`a_nested_item_stands_at_its_parents_content_column`).

A blank line inside a list or a quote then had no cell at all. The scene gave cells to the
lines that wrote a marker and to wrapped continuations, but not to empty ones, so the row
began at the card's edge and its caret — once one was drawn — stood at the left margin,
left of the construct's own text. A blank row now takes one unwritten cell per container
its byte position sits in (`containers_at`, `spacer_cells`), the same room its siblings
reserve (`a_blank_line_inside_a_list_keeps_its_column`).

That first version asked whether the position lay in the block's span, which the parser
carries past the construct's last block through the blank lines that follow: the blank line
after a list — and the empty line at the end of a document that ends with one — took the
list's room, so their carets started at the marker's column although no list was there. The
span cannot answer this, because it also holds a sibling's marker: in `> - a` / `>` / `> b`
the quote's `>` on the blank line ends up inside the list's span, and a byte test would keep
the list's room on a line only the quote owns. Containment now asks whether the construct
holds a block that begins below the line (`content_after`), and a quote stays a quote on a
line where it writes `>` (`containers_at`, which now also hands each container's written
marker to `gap_rows` in the same walk). Only a blank line with a block below it keeps the
room; `a_blank_line_after_a_list_belongs_to_no_container` reads a list, a quote, and the
quoted list, and was red on the span rule with `cells` where none belong.

Typora was taken as the architecture to copy because it is the effect the
request names. Its editor is a rendered block tree that regenerates Markdown on
save `[unverified]`, which cannot promise an exact file. The architecture is
CodeMirror 6's source-with-decorations model instead, and Typora remains the
visual reference.

A row that renders its marker also gave it a second inset. Once a row became one
text node with the cells' rooms inside its runs and atoms, the node took an
`indent` equal to the cells' room whenever it had no visible parts — the rule a
blank line needs — but a rendered marker line has no parts either: its bytes are
hidden behind the stand-in, while the atom already carries the room. Pressing
Enter at the end of a bullet therefore opened a line whose `- ` stood one marker
column right of the bullets above it, because that line was shifted by the room
its own atom draws. The room now travels on the atom and nowhere else: a row
takes `indent` only when no cell writes a byte at all, and the body column the
cells open becomes `block.hanging`, so every later line of a wrapped item
measures and starts clear of the marker instead of under it. The gallery
self-test's `row geometry` reads a marker line's first line at x 0.00 beside
`- a`'s 0.00 and a wrapped item's second line at 10.54, the atom's own room; with
the old rule the two read 10.54 and 0.00.

A rendered marker's bytes were `Hidden`, so they took no room and every offset
inside the marker shared one x: an x step into a sibling item could only tie back
to the cell's first byte, and a press inside a label could not name the byte
under it. A cell now uses `Visibility::Covered` — unpainted, but laid out in
full — and that is the rule both forms follow: paint changes, geometry does not.
Pinned by the gallery's `marker step`; with the bytes `Hidden` again, Down from
`1.| ` reaches 6, the byte before the dot.

A step between two markers still used the pixel column, and digits do not share
an advance: with `Covered` in place, Down from `1.| ` reached 6 of `2. b`, and
from `9.| ` it landed between the digits of `10.`. A row's views are its
columns, and a step between two columns now keeps the caret's place counted back
from the cell's end — where a marker writes its separator and dot — so `1.| ` →
`2.| ` and `9.| ` → `10.| ` both land after the dot. Pinned by the same
`marker step` flag, which reads both documents.

Cell alignment was declared but never drawn, and the self-test could not see it.
A cell's label and its bytes are `nowrap` nodes, and layout stamped an infinite
`wrap_width` for them because a single-line editor must not break; the painter
uses that field as the alignment box, and `aligned_offset` returns zero for a
non-finite width. Every marker therefore started at the cell's left edge, and
`9.` and `10.` did not line up on their dots. The self-test hid it because
`text_ink_bounds` recovered the offset from the node's rect and the block's
alignment, repeating the layout's claim instead of reading the drawing. Layout
now stamps the content box's width even for a `nowrap` node — the line is
unbreakable, not boxless — so the alignment has a box, and
`a_nowrap_line_aligns_inside_its_box` pins it. The gallery's `stepped` flag now
reads the dot's glyph rect from the draw list and was red (`stepped false`)
before the fix.

The separator was a no-break space inside the label, because a measured line end
hangs a plain trailing space. That gave the label its gap but left the source
form — the raw `1. ` bytes, whose trailing space is trimmed — with its dot one
space further right, so focusing a marker moved it. The gap is now the cell's
trailing room: `Markdown::kept_space` measures the source's trailing whitespace
with an ink probe and the cell keeps it as padding, while `label_of` returns only
the marker's ink. Both forms end on the same edge and the text beside them starts
at one column; a task's box and a quote's bar keep their own gap in `stand_in`,
so that space is counted once.

That room was given only to a bullet's or a number's cell, so a nested item's
indent cell measured zero: the breaker then trimmed its two spaces off the line,
and the cell's source side therefore reported no advance. The indentation still
looked right only because the nested list starts at its parent's content column,
and the cells' bytes were caret-reachable while occupying no room. `kept_space`
now answers for every cell whose stand-in does not carry its own gap, so the
indent keeps its own advance: in the release gallery (nested session) a two-space
indent took 7.88 px and moved the nested marker's view from x 398.54 to x 406.42.
A caret in the cell stands at the cell's own edge. The probe that measured that
gap is gone: a hard-broken line now keeps its spaces (see
[Inline Content](./layout-inline-content.md)), so the advances answer for it.

**A rendered marker lost the space the source spells after it.** While a line's
end hung its whitespace, `kept_space` learned the gap from an ink probe around a
no-break space. Once a hard-broken line kept its spaces, measuring `1. ` already
counted the space, the probe answered zero, and the label was drawn against the
cell's right edge: the gallery's editing card read `1.ordered items`. The room is
now `advance(source).max(stand_in + kept_space)`, with `kept_space` the difference
between the source's advance and its trimmed advance, so each form asks for what
it needs and the separator is counted once. Measured by the widget itself at the
reading card's size: `1. ` advances 14.25, the label `1.` 10.31, the gap 3.94.

**The two formula modes were both about text; one of them is now about the
formula.** `MathEditing::InPlace` left the typeset box on an inline atom and let
the press pick an edge of the source, which is all an atom can offer. The
structural editor made a display formula its own control, so the variant is now
`Structural`: a rendered block is a focusable formula whose press lands the
caret inside the structure, and its edits rewrite that block's byte range in the
document before the app hears one `Change::Text` of the whole buffer. The widget
reaches this through `Math::in_text(owner, range)`; the control reports the
splice on the document's id, so the app never learns a block id. Inline formulas
keep the old behaviour, because an atom has no node to focus. Source mode gained
the display-block half of its preview: the panel now hangs from the block's row
when the caret is inside the block, not only from an inline atom's caret x.
`a_display_formula_edits_in_place_and_the_document_stays_one_buffer` and
`a_formula_inside_a_document_rewrites_its_own_bytes` pin the splice, and the
former goes red when the block is not made editable or the owner range is
dropped; `source_mode_floats_a_preview_beside_the_display_formula` pins the
block preview.

**A formula inside the document had a second, independent caret.** Pressing a
display formula focuses its editor, but the engine still resolved the focus's
text-input owner to the document and kept an editing selection there; the
Markdown painted that caret whether or not it owned the keyboard, so two carets
moved independently and `Ctrl+Z` answered the document's history, not the
formula's. The first fix pinned the document's caret to the formula's end and
left the formula's own history holding the keyboard — one visible bar, but still
two buffers and two stacks, and a selection could not reach into the formula at
all. The document is now the only caret and the only history: the node is not
focusable, it is an input view of the block's bytes, keys and text are routed to
the structure and spliced back through `apply_edit`, and the control mirrors the
document's selection for painting. `an_arrow_inside_the_formula_steps_by_structure`,
`a_press_inside_the_fraction_names_a_boundary_inside_its_bytes`,
`a_drag_from_the_prose_into_the_formula_selects_across_it`,
`the_formula_paints_the_part_of_the_document_selection_inside_it` and
`an_edit_inside_an_embedded_formula_joins_the_documents_history` catch a return
to a second buffer; each fails when its wiring line is removed. The last one
types in the prose, then in the formula, and undoes twice: a formula-local
history answers only the first undo.

**A fenced block's head was shorter than the button it holds, and its surface was a
band laid over the card.** The head was one code line tall and centred the copy
button inside that fixed band, so the button and its padding overflowed it. The head
is a stack now: one child is the icon, the language and the copy button, the other
the opening fence that the editing state draws, and layout sizes the stack to the
taller of the two. Only one child shows — the other is transparent and inert — so
entering the block still moves nothing, and the head takes the height the button asks
for instead of a line's. The card is two rectangles joined edge to edge, the head's
tint above and the code's fill below, with a rule between them; the head is a
surface of its own rather than a strip over a larger one. The foot owns no surface:
it is the room the count or the closing fence needs at the bottom of the code's
rectangle. The self-test's `card` flag reads the language text, the fence text and
the chrome's opacity in both states.

**The code gutter scrolled with the code, then measured its twin in a 24-pixel box.** The
numbers were first painted inside the body's own text node, so a sideways scroll carried them
out of their column and nothing painted a band behind them. The column then became a node of
its own, but the twin text inside it was laid out at the column's width: the shaper broke the
code into 806 lines and the card grew to 13,852 points, which the gallery drew as a grey slab.
The column is now an out-of-flow overlay — an opaque band and a twin measured against the
card's full width — and both are siblings of the sideways strip, inside the cap's scroller.
The self-test's `code numbers` line reads the twin's `line_numbers` and the body's height;
over the narrow column the height assertion fails with the 806-line wrap.

**A `>`-only line borrowed its cells from every quote above it.** The cells of a line no block
owns were found by walking each container that started above the line and keeping it when the
line spelled a quote marker at the cursor or the container still had a block below. A quote that
ended many lines above spells `>` at that column too, so it matched first and took the cell that
belonged to the quote holding the line; the real quote then had no marker left and became a lazy
cell one column to the right. A document with an earlier quote and a nested one therefore drew
two bars on each blank `>` line and three on a blank `>>` line, each borrowed cell a one-row run
with rounded ends, so the run of bars broke and showed a seam — the report of twice the bars and
gaps between rows. Containment now asks the construct's own bytes: the line's first byte must lie
in the block's `full` range, and the "still has a block below" rule stays for lines that write no
marker (a list's trailing span can reach the next block's line). Text rows were never affected;
they take their cells from their own ancestors.
`a_blank_line_inside_a_quote_draws_only_its_own_bars` pins one bar per level per line and square
joints (red: `[1, 1, 1, 1, 1, 2, 2, 3, 2, 3, 1, 3]` where one bar per level belongs).

**The gutter band was an inset layer over the code, so the card's fill rimmed it and the scroll
clip reached under it.** The band was a child of the code's row, inset by the card's side padding
and its gap under the head, and only as tall as the code lines: above, left and below it the body
showed its own fill, so the gutter read as a slightly darker rectangle floating in a differently
coloured frame. The sideways strip also spanned the whole inner width and was clipped there, so a
scrolled line slid under the band and only the band's opacity hid it. The band is now the body's
surface — from the card's edge to the code's first column, over the body's whole height, foot
included — and the code starts past the gutter, so the strip's clip begins at the rule. The numbers
keep their own box on the row's edge; the background and the number text are separate nodes. That
first layout still left the band's room uneven — the card's inset on the left, a quarter of the
number gap on the right — and its tint ran a few pixels past the rule into the code's fill. A
follow-up report asked for equal room around the widest number and for the rule itself to be the
colour boundary: the band is now the widest number plus half the gap on each side, the rule stands
on the band's edge, and the code's first column follows it one gap from the widest digit. Checked
in a nested session's screenshot of the gallery (`GALLERY_MARKDOWN_CODE=numbers`): the widest
number's ink stands as far from the card's edge as from the rule, no rim of the body's fill around
the band, and one unbroken rule down to the card's bottom corner; the selftest's `code unbroken`
line also reads that the strip starts at or past that rule.

**A nested item's indentation was counted twice.** `line_cells` turned the source's spaces before a
nested marker into an `Indent` cell of their own, after the `Ordered` continuation cell that already
reserved the parent's content column. The row therefore opened one cell too many, and the nested
marker stood that indentation past the column the parent's content starts at: in
`1. **准备环境**\n   - 安装 Rust`, the bullet sat 31.9 px in where 14.8 + the bullet's room was
right. The same row's quote bar and code card took their x from the accumulated cells, so
`   > 提示…` drew its bar over the first character and its card started visibly right of the item's
text. The indentation bytes now fold into the preceding unwritten cell, extending its source range to
cover them; only a marker after a written cell still opens a room of its own. Caught by
`a_nested_item_stands_at_its_parents_content_column` in `crates/ui-richtext/src/scene.rs` and
`a_nested_construct_stands_at_its_parents_content_column` in `crates/ui-widgets/tests/indent_columns.rs`,
which measures the nested item, the code card and the quote bar against the parent's column; with the
fold removed the nested item reads 35.2 against an expected 24.3.

**A block's indentation vanished from view and stayed deletable.** The reported document's fence
line began with three spaces that the card did not draw at all: pulldown-cmark puts `full.start`
after a fence line's indentation (measured on the report's block: `span=35..56`, `full=24..62`),
so those bytes sat before every view and no caret could reach them, while each content line's
indentation lived in a `dedent` run that no selection could touch. A deletion at the boundary
therefore edited bytes the frame did not show: Backspace at a content line's start merged it
into the fence line, and a deletion on the fence's indentation shortened the fence alone. The
card now gives those bytes to the frame: `code_head` draws the fence line from its own line
start, `card_lines` splits every content line into the containers' prefix and the block's own
spaces, and each line's share of the value is a `Structure` member over ordinary visible text,
carried to the group by `edit_structural`. Pinned by
`the_spelled_indentation_carries_to_every_line` (with the members removed, the fence line alone
loses its space) and `a_code_lines_indentation_is_plain_text` in
`crates/ui-widgets/tests/indent_edits.rs`.

**A block's indentation could not be created, and an indented block grew fences.** The report:
adding or removing the leading spaces of a code block's first line was not handled at all, and
several spaces typed on a flush fence line broke the block apart. A flush block carried no
members, so the first typed space was an ordinary insertion only the fence line took, and the
fourth pushed the fence past CommonMark's three columns, which made the parser read `   ```rust`
as an indented code block, the next line as a paragraph and the closing fence as another empty
block — one block became three. `code_fences` also recognised a fence by its bytes alone, so an
indented block's first line — a literal ```` ```rust ````, since CommonMark gives indented
blocks no fence — was drawn as a head and its last line as a foot. A fence line at zero
indentation now spells an empty member of the group, so the first space typed there is the
block's and every line takes it; `BlockKind::CodeBlock` carries `fenced` and `code_fences`
answers nothing without it. Pinned by `the_first_space_typed_on_a_fence_is_the_blocks` (red with
the empty member removed: only the fence line gained the space),
`home_stands_where_the_lines_text_begins` and `an_indented_block_has_no_fences` (red without the
`fenced` check: a row painted ```` ``` ````).

**Hidden linked copies made the source's own bytes uneditable.** The first shape of the fix
above gave every line a `Hidden` run linked to the fence's spaces: zero-width copies where a
block had no indentation, and `edit_indented` refused deletions inside a copy and moved
insertions past it. That inverted the editor: the source's bytes were drawn once and copied
invisibly, so the same value had one editable place and several unreachable ones, a caret could
not be put where the bytes were, and the group was an engine mechanism pretending the missing
bytes did not exist. An in-place editor shows the source it edits, so the value is now spelled
on every line as ordinary text and only the *carry* is special: `Structure` marks the members,
`shown` marks the one line the source spells, and `edit_structural` rewrites the group in one
edit. The bytes under the card's stand-ins became `Covered` for the same reason — a press and a
drag reach them — and `hidden_ranges` reports only `Hidden`, which is now inline syntax and
nothing else. `TextRun::link` and `TextBlock::indentation` are gone. Caught by
`a_code_lines_indentation_is_plain_text` (a deletion inside a content line's own member edits
that line alone; with the links back it would be refused) and by the two tests above, which fail
if a member's bytes are `Hidden` and so unreachable.

**The block's indentation was read through the card's code.** With the leading bytes as ordinary
text, the body gave each line's prefix its source advance: the code stood indented inside the card
by the block's own columns, and a fenced block inside a list drew the item's continuation spaces
inside its code area, as if they were code. The bytes are no longer allowed to push the text: they
keep their place as `Covered` runs of zero `Room`, and the card itself starts after the block's own
indentation in the source form, with the same right edge as before. A `  ```rust` block therefore
shows the two columns before the card's surface and its fence and code flush at the card's edge;
the code's column and wrap width are unchanged, so entering the block moves the card, not the
code. Pinned by `the_spelled_indentation_carries_to_every_line` (the rendered card paints the code
flush) and by the source form's geometry: the head's fence and the body's code share one left edge.

**A fence opening a list item's line has no indentation of its own.** The card treated the
spaces between the item's marker and the fence as the block's value, so a space typed there was
carried to every content line — but the parser reads those spaces as the item's padding: the
fence stayed at the item's content column and the code gained a space the reader could see.
The card now asks the row's cells what they draw: the bytes an item's cell spells are the item's
padding, so a fence that opens the item's line has no member there and edits stay plain, while a
quote's cell reaches the content's column and carries the block's spelled member itself. Caught
by
`a_fence_after_a_marker_owns_no_indentation`: with the line spelling a member, the content lines
gain a space and the code grows one the reader can see.

**A rule's indentation belonged to no row.** A thematic break's row spanned only its marker
bytes, so the spaces before `   ***` fell between rows: no caret could enter them and no drag
could take them. The row now spans its whole source line, at both ends, so the bytes before and
after the marker are ordinary text of that row.

**The source's indentation was not drawn where it stands.** With the leading bytes zero-room, a
`  ```rust` block showed its two columns as a blank band inside the card's head: the bytes were
positions a caret could take but nothing drew, so a reader could not tell the indentation from
the card's own padding, and a selection over it highlighted nothing. The fence line's leading
bytes are now a view of their own, drawn left of the card's surface, and the card gives up their
column: in the source form its left edge moves right by the block's own width and its right edge
stays. The code moves with the card — the earlier correction claimed the opposite and was wrong,
because bytes that stand before the text and take room must push it, unless they are drawn
*inside* the card, which is the shape that started this. The code still reads flush at the card's
padding. The bytes no cell claims are the block's; a container's cell draws its column itself, so
those bytes get no separate view. Two consequences had to be mended with it: a quote's cell runs
to the content's column, so the block's member is attached to the cell's own view, and the foot
now owns the closing fence's whole line rather than starting at the fence, so a quoted closing
fence's bytes are reachable and follow the value. Pinned by
`the_blocks_spacing_stands_left_of_the_card` (red with the leading view removed: no view starts
at the line's first byte), `a_quoted_fences_indentation_is_the_blocks` (red without the cell's
member: the content line stays behind) and `home_stands_where_the_lines_text_begins`, whose press
now lands in the indentation's own view.

**A continuation line's indentation was nobody's.** The value was read as the spaces after the
line's container prefix, and a list item's continuation line writes no marker there: the reserved
column the parser strips was counted as prefix and left out of the value. On
`2. **启动服务**\n\n   ```bash\n   RUST=1`, the three spaces before the fence therefore became a
zero-room view at x=27.4 rather than the row's left, and a deletion at the fence took a space from
that one line while the content lines kept theirs — the block's value in the source no longer
matched the width the parser strips. The value now starts where the line's markers end, and a line
that writes none starts at its first byte, so the reserved column is the block's there: the same
run of spaces on every line, drawn from the row's left edge, edited as one. Pinned by
`home_stands_where_the_lines_text_begins`, which deletes at the fence and expects every line — the
item's continuation column included — to lose a space, and by
`the_blocks_spacing_stands_left_of_the_card`, which places the view left of the fence.

**A setext underline was drawn as the line's text.** A continuation line's leading run was taken
whole as the line's prefix, so `- x\n  -\n` — a heading inside the item, underlined — put `  -`
in the item's cell as indentation and left the row with no text: the editor painted the
underline as literal source at the heading's scale, beside a heading it had already drawn. The
run begins with the columns the containers reserve, so it now splits there: the spaces join the
cell whose column they spell, and the first byte past them begins the line's own syntax, which
the projection hides while the caret is away and draws when it arrives, as it does an ATX `# `.
A run of spaces alone stays indentation, so a wrapped paragraph's second line keeps the column
it spells. Pinned by `a_setext_underline_is_syntax_past_the_containers_column` (red before the
split: the item's cell held `4..7` and the row's parts were empty).

**A block marker was documented as drawing from anywhere in its block.** The contract said the
bytes outside the content's extent delimit the whole block, so any caret inside it draws them, and
named a `Construct::edge` split that the code does not have. `Scope::Marker` watches the bytes the
line writes instead: `a_marker_shows_only_while_the_caret_touches_it` reads `## Head` with the
caret in the title and on another line, and `a_fence_shows_as_one_unit_while_the_caret_is_inside_it`
is the one construct that does span lines. The paragraph now states that split.

## Verification

| Test | What it catches |
| --- | --- |
| `every_byte_of_the_frame_is_one_run_of_its_own_source` | a run that skips, duplicates or decodes bytes, which would shift every caret |
| `the_construct_under_the_caret_shows_both_delimiters` | reveal leaking to a construct the caret is not in, or one delimiter hiding while the caret is in it |
| `nested_delimiters_belong_to_the_innermost_construct` | an outer construct revealing an inner one's markers |
| `a_tight_list_item_keeps_the_bullet_out_of_its_frame` | the list bullet or a trailing delimiter falling outside the frame |
| `a_marker_shows_only_while_the_caret_touches_it` | a heading's `# ` staying visible while the block is not edited |
| `a_marker_is_the_source_before_the_first_child_on_its_line` | a list marker that takes in its child's indentation, or starts at the wrong byte |
| `a_block_marker_shows_on_its_own_line_only` | a quote's prefix column drawn whole, or a prefix hidden on the caret's own line |
| `a_fence_shows_as_one_unit_while_the_caret_is_inside_it` | a fence hidden while the caret is in the block it delimits |
| `a_quote_nests_and_a_lazier_line_continues_the_inner_paragraph` | a nested quote flattened into its parent, or a lazy line read as a new sibling |
| `a_nested_item_stands_at_its_parents_content_column` | a marker matched at the line's first byte only, so a nested item's number goes undrawn and its indentation sits inside the marker |
| `a_setext_underline_is_syntax_past_the_containers_column` | a setext underline taken for the item's indentation, so the row draws `-` as the line's text |
| `a_blank_line_inside_a_list_keeps_its_column` | a blank row inside a list given no cell, so its caret starts at the card's edge |
| `a_blank_line_after_a_list_belongs_to_no_container` | a blank row after a construct's last block taking its room, so its caret starts at the marker's column; also a quoted list's `>` line losing the quote's cell |
| `a_blank_line_inside_a_quote_draws_only_its_own_bars` | a `>`-only line taking an earlier quote's cell, so it draws two bars for one level, shifts the real bar a column right, and rounds off where the run should continue |
| `a_soft_break_is_a_space` | a paragraph breaking at every line ending in the source |
| `a_break_continues_the_list_with_the_marker_of_its_own_line` | a break that drops the marker it stands in, or one that writes the wrong marker |
| `a_break_on_an_empty_item_ends_the_list` | an empty item continued into a second empty item instead of ending the list |
| `a_break_renumbers_the_ordered_items_after_it` | a number repeated instead of counted up |
| `a_break_writes_one_line_whatever_the_construct` | a quote's break continuing its paragraph instead of opening a line, or a nested quote flattened by it |
| `a_break_on_an_empty_quote_line_closes_the_quote` | a second break leaving the blank quote line behind, holding an empty quote open around nothing |
| `a_tab_nests_by_the_marker_above_and_shift_tab_takes_it_back` | an indent that cannot nest under an ordered item, or an outdent that eats more than one step |
| `a_prepared_document_answers_what_the_projection_answers` | a prepared frame cache that misses a nested block, or answers one block's frames for another's |
| `a_block_with_no_content_keeps_a_place_for_its_text` | an empty item or an empty code block losing its line, and the caret its way into it |
| `a_composition_freezes_the_selection_reveal_is_cut_from` | reveal following a caret that moved during a composition, so the bytes under the preedit change shape |
| `a_selection_spans_the_blocks_of_one_input` | a press, drag or step that stays inside its own block's coordinates |
| `the_empty_last_line_takes_the_caret` | the empty line a trailing break leaves drawing no position at all |
| `a_gfm_table_is_one_block_with_aligned_columns_and_rows_of_cells` | a table read as paragraphs, its alignment vector lost, or its cells given block bodies |
| `a_table_frame_covers_every_byte_and_shows_it_in_the_source_state` | a pipe, padding byte or the delimiter row falling outside the table's frame, or the source state not showing them |
| `a_gutter_numbers_source_lines_not_wrapped_ones` | a gutter that numbers a wrapped line as if it were a source line, or a number painted outside its column |
| `a_plain_wheel_over_a_sideways_strip_scrolls_it_and_not_the_page` | an upright wheel over a sideways strip fed to the page behind it, or a plain wheel refused by the strip |
| `an_unwrapped_strip_hands_the_engine_a_horizontal_bar` | a cut-off line with no bar, so the reader cannot see the line continues or drag to its end |
| `a_nested_fences_indentation_reaches_no_content_line` | a fence's own indentation drawn inside the code, from the second content line on |
| `the_spelled_indentation_carries_to_every_line` | a block value edited on the fence line without reaching the other lines, or a carry that changes the code's own columns |
| `a_fence_after_a_marker_owns_no_indentation` | the item's padding read as the block's value, so the code gains a space the reader can see |
| `the_blocks_spacing_stands_left_of_the_card` | the fence line's indentation read as part of the fence or as a gap inside the card, and a card that moves its right edge when it makes room for it |
| `home_stands_where_the_lines_text_begins` | a deletion in the fence's indentation that leaves a continuation line's reserved column behind, or a Home that stops short of the line's first byte |
| `a_quoted_fences_indentation_is_the_blocks` | a quoted fence's spelled member lost to its cell, so the fence's spaces move alone and a quote's closing fence stops following the value |
| `a_closing_fence_at_the_end_of_a_line_is_drawn_as_its_own_line` | a closing fence that shares the last code line read as code, so that line is painted twice and two views hold its caret |
| the self-test's `code numbers …` / `code cap scroll` / `code unbroken …` lines | a gutter node that does not paint numbers, a cap with no scroller, a sideways strip that cannot take the wheel, or one whose clip reaches left of the gutter's rule |

Each was watched red before it passed: revealing a construct unconditionally
fails `the_construct_under_the_caret_shows_both_delimiters` with
`"**bold** and *it*"` where a caret away from both expects `"bold and it"`,
revealing only the opening delimiter fails it with `"bold and it"` at the
caret's own `**`, removing the selection test from reveal fails the two reveal
tests, and stopping the builder from giving a childless item its paragraph
makes `a_block_with_no_content_keeps_a_place_for_its_text` panic on the missing
place, while taking the projection's frame from `block.span` instead of `full`
fails that test's fence half with `""` where `"```"` belongs. Restoring the old
containment rule for a synthesized paragraph fails the list-item test, and
dropping the last run fails all five on the byte invariant. Removing the line
cut from `marker` fails its test with `"- \n  "` where `"- "` belongs. Making a
fence show only the fence line the caret is on fails
`a_fence_shows_as_one_unit_while_the_caret_is_inside_it` with `"let x = 1;\n"` where
the fences belong, and revealing a block marker
unconditionally fails `a_block_marker_shows_on_its_own_line_only` with `"a\n> b"`
at a caret on the first line. Drawing the body's bytes without a dedent fails
`a_nested_fences_indentation_reaches_no_content_line` with
`"cargo build --release\n   cargo test --all-features"`, and taking the block's last line for
a closing fence fails `a_closing_fence_at_the_end_of_a_line_is_drawn_as_its_own_line` with
two copies of `"let b = 2;   ```"` and two caret views. Walking every quote that starts above
a gap line, without asking whether its bytes still reach the line, fails
`a_blank_line_inside_a_quote_draws_only_its_own_bars` with twelve bar rows where ten lines
belong and `[1, 1, 1, 1, 1, 2, 2, 3, 2, 3, 1, 3]` bars per row.

The gallery's self-test drives the marker the way the request described it: it
reads that the cell's bytes are hidden while the caret is away (the label stands
in their place), presses the cell and reads them drawn as `"- "`, presses Home
and Delete, and checks the source no longer holds `- Nesting works:`. It also
reads the row's height and the text's x before and after that press (`steady`),
which is the jump the marker's two states must not produce. One list's items
must share a column, and a list nested in an item one cell further in, so
`aligned` reads the two bullets' x and the two ordered items' x; `stepped` reads
a `9.` / `10.` pair, whose dots must end at one x and whose text must start at
one column, and it takes each dot's x from the glyph the draw list holds, so a
cell that declares an alignment without drawing it fails. `one caret` presses
the end of the second item's marker and counts the caret-bearing views in the
input: one, the body's, at its first byte, with the cell drawing none. `vertical` starts at three sibling items and reads
Down from one item's text start to the next's (offsets 2 → 6 → 10) and Up back
to 6, which a step into the marker cell failed at 4. `marker step` presses into
an item's text, steps Left into the marker, then Down and Up, and reads 2 → 7 →
2; it repeats on `9.` / `10.` and reads 2 → 8 → 2. The x column read both Down
steps one byte short, before the dot, because a digit is not a fixed advance. The
same run presses into a nested ordered item and a quote, presses
Down from a list item into the next block, and presses Enter inside one, each of
which must report an edit. For the quote it reads that the cell's bytes draw
nothing while the caret is away and `"> "` once the caret presses the bar, that
the quoted text's x does not move (`quote_steady`), and that the bar's fill goes
transparent while the bytes stand (`prefix away`, `shown`, `bar away`, `gone`).
It clicks the
code block and reads both fences drawn (`fences`), and appends a nested quote to
the source, clicks the inner paragraph and types into it (`nested quote`,
`edited`). Each of the new flags was watched false with its rule disabled: a
label off the body's first-line leading makes `steady` false, reveal that hides a
fence with the caret inside it and a bar that never stands down make `fences`
and `bar gone` false,
and a scene that matches a nested item's marker at the line's first byte only
makes `aligned` and `stepped` false — the sample's two nested ordered items then
start their text at 416.7 and 418.8, which is the failure the scene test above
pins at the byte level. `one caret` was watched false on the build before the
ownership rule, and `vertical` was watched false on the step that took the
line's first view, reading `2 → 4 → 8`. The blank-row tests were
watched red from the other side: with `spacer_cells` answering no cell, the
scene test reads zero cells on the blank row, and with the text pass's two paint
gates back in place the caret test reads no rect at all.

The views test was watched red three ways: making `input_views` answer `None`
leaves the press at the buffer's end (5
instead of 4), making `crossing_view` answer `None` keeps Down in the first
block (1 instead of 4), and dropping the Enter declaration leaves `"aXd"`
instead of `"aX\nd"`. With views off, `the_empty_last_line_takes_the_caret`
also fails: Up does nothing and the caret stays at 3 instead of leaving the
empty line for 0.

The engine half is pinned by two more. `a_hidden_run_takes_no_room_and_leaves_every_offset_where_it_was`
measures the block, the caret and the glyphs of `[**][bold][**]`: measuring or
drawing a hidden run fails it (91.4 against 48.5 wide, or 8 glyphs against 4).
`an_empty_line_from_a_hard_break_keeps_its_offset` reads the ranges of `a\n\nb`
and the caret's line: deriving a line's bytes from its runs collapses the
middle line to `0..0` and the test fails on `[0..1, 0..0, 3..4]`. The multiline
paste branch is pinned by `a_paste_keeps_line_breaks_only_in_a_multiline_input`,
which fails with `"aba b"` when the flattening is unconditional.

`a_hidden_run_is_crossed_in_one_step_and_deleted_whole` drives an input whose
runs are `[**][bold][**]` and reads the reported caret: one Right press must
cross the closing run, four Left presses must walk the visible bytes, and Delete
at the opening run's start must remove both stars. Making `past_hidden` an
identity fails it with the caret at 7 instead of 8; making `over_hidden` an
identity fails it with `"*bold**"` instead of `"bold**"`. Each branch was
disabled separately to confirm both reds.

The gallery's `markdown_edit` self-test drives the control end to end: it reads
the editor's buffer and checks it is the whole source, checks that `# ` is a
hidden run while nothing is being edited, clicks the heading, checks the marker
is drawn, types, and checks the app's document grew by exactly those two bytes.
It then clicks the paragraph below — a block that starts after offset zero — and
types, checking the byte landed in that paragraph. After the marker and nested
checks above, it appends a line of `1. ` to the source, waits for the reparse,
and clicks the empty item's line and types into it. It fails with `whole false`
when the input stops being one buffer, with `source edited false` when the app
drops the reported buffer, with `later block edited false` when a press or a step
is measured in the wrong block's offsets, with `empty item edited false` when a
block with no text draws no line to click, and with any of the marker flags
false when the marker is drawn where the caret is not, or cannot be reached.

The four operation tests were each watched red. The empty-item branch answered with the
prefix's own offsets while the line starts elsewhere, so `a_break_on_an_empty_item_ends_the_list`
failed with `"first\n- \n"` — it had eaten the document's opening marker. The quote prefix read
one level only, so the nested case ended as `"> inner"` where `"> > inner"` belongs. Fixing the
offsets and looping over the markers in `prefix` made both pass for the first time.

The gallery's `break`, `indent` and `outdent` flags were watched false with the control's
command disabled, and the reading pair with the two drifts it exists to catch: a size of 15 in
the reading card alone turns `heading` and `paragraph` false, and building the reading card with
`editable` on turns `plain` false and `buffer` `Some(...)`. `formula box` was watched false with
the box built only while the mark is revealed, and `formula source` false with the reveal test
removed — the box then stays over the source the caret is editing.

`a_prepared_document_answers_what_the_projection_answers` was watched red by cutting only the
top-level blocks: it failed on the first list item with `None` where its one-run projection
belongs.

The three flags this round added were each watched false with their rule disabled: `burst`
with the control reading the live selection again (`"- burst tail\n\n"`, the second item gone),
`aligned` with the cell measured per item, and `crossed` with the crossing step answering
`None`, which leaves the caret outside the input.

A text row became one node with its markers as atoms (`line_row`), and the self-test's
marker flags still named the per-cell nodes that rows no longer have: `one caret` asked for a
marker cell's text node and read nothing at all, and `vertical` clicked the row's left edge,
which now names the first byte of a marker the stand-in covers. `one caret` counts the bars
this frame and `vertical` places the caret with Home and two Right steps; both read true, and
`vertical` is false when the crossing step returns `None`. `stepped`, `marker step`,
`aligned`, `marker label`, `tasks` and `quoted` still read the old shape — positions inside a
marker, a label node, a cell's own box — and stay false until the rule they name is decided:
a marker's bytes are atomic to the caret in a text row and its stand-in is an atom, so a
caret cannot stand after the dot of `9. ` there.

A table's source state drew no content. The projected view read each run's form as its own
and hid a rendered one, but a content run has no delimiters to hide: its form is the form of
the construct it is the payload of, which tells an object that the mark is an object, not that
its bytes are invisible. A table's cells are content inside a fence whose pipes are its
syntax, so entering the table hid every cell and left the delimiter row alone. Only a syntax
run follows its construct's form now. The `table` self-test's `entered` reads the three rows'
bytes, the way the projection test does; with the old rule it read the delimiter row and no
cell.
A table's grid cells all named the table's first byte. A press and a vertical step therefore
landed on the table's first line wherever they came from: stepping up into the table from the
paragraph below put the caret on the header and the next step left the table, and no step
could reach a middle row. Each cell now names its own bytes (`cell.span.start`), so a press
names the byte under the pointer and a step into the grid lands on the row it crossed. Pinned
by `a_tables_source_state_steps_through_every_row`, which steps down into a table and back up
out of it and requires every row; the gallery's `table` flag still reads its grid, its source
state and the geometry after the round trip.


## Status

Phases 0 to 4 are implemented (2026-11-11; the object runs and the IME freeze
followed the rest): the projection and construct reveal, hidden runs through the whole
text stack, atomic motion and deletion, one input over the whole source with
blocks as views, selection and Up/Down across blocks and into the next sibling view, a held
column, a plain Enter as a line break that continues a list item and opens a quote paragraph,
the edit a chord asks for computed from the state the chord saw, the tail line an empty last
line needs, one cell for a whole list, a line for every
empty block, a line for every `>`-only blank line of a quote, list markers as views that
show as written where the caret is,
quote prefixes that show on their line while the bar stands down, fences that
show for the block they delimit, the marks' look and a page-supplied
highlighter, one control that reads and edits with `editable` as its only switch,
the control's own `Enter`/`Tab`/`Shift+Tab` written as one engine edit each, a `Tab`
over a selection stepping every line it covers and leaving the selection on them,
undo and redo from the input's history, an inline formula drawn as its box while
the caret is away and as its source on it, the two formula editing modes (a
display formula edited by structure in place, or its source with a live preview
on a floating panel), entities drawn as the glyph they spell rather than their source, a GFM
table read as a grid while the caret is away and as its source while it is
inside, a GFM task item drawn as its box while the caret is away and toggled by a press on
that box, and the parsed tree and its frames
cached per source so a frame positions runs instead of cutting them and updated
incrementally, so a keystroke re-cuts only the edited window. The gallery
draws both cards from that control.

## See also

- [Block Document](./document-model.md): the tree and the spans this extends.
- [Text and Input](./text-and-input.md): the engine's text input a block uses.
- [Inline Content](./layout-inline-content.md): runs, atoms and the text pass.
- [Editable Math](./editable-math.md): the formula field this editor will embed.
