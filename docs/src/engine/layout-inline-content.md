# Inline Content

Inline formatting exists only inside a text leaf. There is no inline container
node, no inline-flex, and no mixed box/inline tree: a `Text` block owns its
runs, atoms, line boxes, and baseline. A widget inside a line is designed
separately, as an atom holding a child node: see
[Inline Boxes](./layout-inline-boxes.md).

## Runs and ink

A run is `{ text, style, ink, background, underline, strikethrough }`. Both
colours are optional and belong to the run, not to `TextStyle`, because the
type ramp never chooses a colour: a link, an inline code span or an emphasised
word is the component's decision. The text node's own `ink` is the paragraph
default, and a run that carries none inherits it.

Resolution order at paint time is selection override, then the run's ink, then
the node's. Selection wins because a selected link must invert with the text
around it; a run ink wins over the node because it is the more specific claim.
A decoration is drawn in the ink its own run resolves to, except where a
selection covers it: the rule is cut at the selection's edges and the covered
part takes the selection's ink, so a highlighted half of a link does not keep
the link's colour.

A run's background is painted per visual line as one rect spanning that line's
slice of the run, under the selection and under the glyphs. A run that wraps
therefore gets several rects, and each covers only its own run's columns — the
line box height, not the glyph ink box.

Underline and strikethrough use the face's own `post` and `OS/2` metrics:
`position` is the line's top edge from the baseline with y up, so the underline
is normally negative and the strikeout positive, and the device edge is
`baseline - position × size`. Thickness and edge are snapped to whole device
pixels, because a one-pixel rule at a fractional offset spreads into two faint
rows. A font that omits either table falls back to the usual sans-serif values
rather than losing the line. Variable-font MVAR deltas are not applied to these
two, the same as for ascent and descent.

A rule style fills that band rather than moving it: `Single` is the band,
`Double` is two rules with a gap of one thickness (the pair grows away from the
glyphs, because the face reports room for one), `Dotted` is a square dot and a
gap of two, and `Wavy` is a sine sampled every two thicknesses — bounded by half
the run's width in boxes, which is the price of a squiggle without a path
rasterizer. The pattern is phased from the run's own start, so a piece cut out
by a selection continues the run's dots and waves instead of restarting them.

The shaper still splits pieces by style and script, so one paragraph can carry
a coloured word without a second primitive — which is the point: a second text
node would wrap on its own and drift out of the paragraph's line breaks.

## Visibility

A run paints its glyphs and takes their advance. An editor's syntax has two
other states. `Hidden` bytes paint nothing and take no room, so the text around
a rendered delimiter closes up while every offset still names its source byte.
`Covered` bytes paint nothing either but keep their advance and their
boundaries: a stand-in paints over their room, as a list's number paints over
the bytes `1. `. The form chooses what is painted, never where a position
sits, so a caret can stand inside a covered marker and a column maps to the
byte under it in both forms.

## Atoms

An atom is a U+FFFC placeholder inside a text block. It is measured at its
own size and laid into the line like a glyph with a fixed advance. Four
vertical alignments are modelled (baseline, top, middle, bottom); the atom's
box decides the line box height when it is taller than the text.

`AtomBox::len` says how many bytes the atom stands for. A reader's atom covers
the one U+FFFC; an editor's covers a hidden run — a formula's `$…$` content —
so the bytes stay in the text for caret offsets while their glyphs are not
drawn. The atom is read before the hidden-run branch when a line paints, or a
hidden run would swallow the box and never move the pen past it.

Atoms are how a control puts an icon inside a sentence without building an
inline layout tree. An atom may carry an `InlineObject` in `AtomBox::object`;
the text pass paints it at the atom's rect while it paints the line, so the
object and the text share one baseline and one set of line breaks. A formula is
the first object (`InlineObject::Math`); an image would be another case of the
same enum. An atom with no object stays a reserved box, drawn faintly, which is
what an unresolved embed looks like.

## Line boxes

A line's height is `max(text line box, ascent + descent)`. The text line box
comes from the font's metrics for the used size; an atom taller than that
raises the line.

Baselines are reported by text and math leaves; every other node kind reports 0,
and a container does not derive a baseline from its first child. A `Row` can
still put its children on one shared baseline (`cross_baseline`): each child
keeps its own cross size and moves until its baseline meets the line's — a mark
beside text is placed by its bottom margin edge, the baseline CSS synthesises
for a box. The line is then as tall as the rise above that baseline plus the
fall below it, which is more than its tallest child. Grid items do not align on
a baseline, and a baseline does not pass through a wrapper: a control that needs
one through a container is still best built as one text block.

## Measurement

The text pass measures the block once at its used width, and asks for intrinsic
min/max content for hug axes. The min/max heights are then overwritten with the
wrapped height, so a hug axis follows the wrapped result rather than the
unwrapped block.

At a soft wrap, trailing whitespace hangs: it is out of the line's range and out
of its ink width, while glyph placement still consumes the full advance, so a
line can look narrower than its advance sum. A line a hard break ends keeps the
spaces before that break in both its range and its width — a space in the source
owns a caret byte, and a caret inside it has to stand somewhere — and the wrap
test asks for the ink only, so those spaces never wrap the line.

`TextBlock::indent` is the first line's left inset and `TextBlock::hanging`
every later line's: a paragraph that opens with a marker wraps clear of it
instead of sliding under it. Both take their room from the wrap width — the
first line measures in `max_width - indent`, the rest in `max_width - hanging` —
so a line never overflows into the room it was offset by, and both enter
`line_origin`, the one place placement, paint and hit testing turn a line into
coordinates. The width layout reserves is the widest `line.width + left`, since a
line's inset belongs to the same box its width does.

## Justification

`TextAlign::Justify` fills every line but the last. The room a line left is
`max_width - line.width` (the same box `line_x` aligns inside, the line's own
inset included), and the text pass reopens it as an equal extra at each break
opportunity inside the line — the same UAX #14 opportunities that chose the
breaks, so a CJK line widens between characters and a Latin one between words.
A single word has no opportunity and stays as it is.

The widening happens inside `shape_line`, the one walk that both places glyphs
and records caret boundaries: pieces are cut at the gap offsets and the pen moves
by the gap after each piece. A caret, a selection and a hit test therefore read
the x the glyph was painted at instead of an unjustified approximation. An atom's
x comes from the layout, which knows nothing of gaps, so the pass shifts it by the
gaps before it.

A line holding an RTL run is left flush, even when the block asks for
justification: those runs are placed in reverse, and the gaps would have to be
mirrored to land between the right words. The last line, a line cut short by an
ellipsis, and a line closed by a forced break keep `Start` — CSS leaves the line
before a `<br>` flush too, since a few words spread across the row read worse than
a ragged edge. The forced break is found past the line's end: a wrapped line
leaves a space there, a hard-broken one the newline it stops before.

## Traps

| Change | Symptom |
| --- | --- |
| Add a `NodeKind::Inline` container | Nothing measures or places it; inline layout is not implemented |
| Mix text and boxes under one container | Each box becomes a block; the sentence breaks apart |
| Expect flex or grid baseline alignment | Baselines are not consumed outside the text leaf |
| Treat the atom's advance as zero | Following text overlaps the icon |
| Skip a hidden run before reading its atom | The box draws but takes no room, and the text after it starts underneath |
| Set an atom's size separately from its object | The line measures one box and paint fills another; the object is the box |
| Use the intrinsic height as the wrapped height | A hug block clips or leaves a gap |
| Give a coloured word its own text node | It wraps alone and drifts out of the paragraph's line breaks |
| Place a decoration from the line box or the glyph box | It drifts with the line height, or cuts the letters it should sit beside |
| Expect a container to report a baseline | A nested row is aligned by its bottom edge, not by the text inside it |
| Restart a dotted or wavy pattern at each piece | A selection cut makes the two halves disagree about where the dots sit |
| Apply justification outside `shape_line` | The wrap width, the carets and the painted glyphs disagree about where a word starts |
| Expect a justified line with an RTL run to reach both edges | Reverse-placed runs need mirrored gaps; the line stays flush |
| Give every line the first line's inset | A wrapped paragraph slides under its own marker instead of clearing it |
| Apply an inset outside the wrap width | A line offset by its inset overflows past the right edge by the same amount |
| Read a line's x without its inset | A caret, a glyph and a hit test disagree about where the line starts |
| Add letter spacing at paint time only | The wrap and the caret use the untracked width and drift from the glyphs |

## Status

Implemented: text-leaf inline layout, per-run ink, background, underline and
strikethrough, U+FFFC atoms with four alignments, line boxes that include atom
height, intrinsic min/max content, first-baseline reporting from text leaves,
atoms carrying an `InlineObject` that paint draws inside the line (a math
formula, or the character a Markdown entity stands for; see
[Formulas](./math.md) and [Markdown Editor](./markdown-editor.md)), and
baseline alignment of a `Row`'s children on one shared line, four rule styles
(single, double, dotted and wavy) on the face's own band, a rule cut at the
selection's edges so its ink follows the glyphs', and justification of every line
but the last, with carets and hit tests on the same x as the paint.

Not implemented: a general inline formatting context and inline-flex. A widget
inside a line is an atom that holds a child node, which layout measures, places
and paints like an inline-block; interaction inside it is not built — see
[Inline Boxes](./layout-inline-boxes.md). A container reports no baseline, so
baseline alignment does not reach through a wrapper or inside a grid.

## Corrections

**Justification first stretched the line before a forced break.** Only the
block's last line and an ellipsised line were exempt, so a paragraph that ended
after a `<br>` spread the two or three words before it across the whole row — the
gallery specimen showed `with the column.` pulled apart. CSS 2.1 §16.2 exempts
that line as well, and it reads better. The exemption is decided from the
whitespace past the line's end: a wrap leaves a space there, a forced break
leaves the newline. The text-pass test drives a wrapped line, the line before the
break and the last line; the gallery specimen shows all three at once.

**A line the source broke dropped the spaces it ended with.** The breaker hung
trailing whitespace off every line, so `abc   ` and a list item's trailing spaces
were outside the line's range and its width: no caret could stand in them, and a
click past the last word went no further. A space in the source owns a caret
byte, so a line a hard break ends now keeps the spaces before that break in its
range and its width; a soft wrap still hangs them, and the wrap test asks for the
ink only, so a run of spaces cannot push a line into wrapping. Pinned by
`a_hard_break_keeps_the_spaces_before_it_but_a_wrap_hangs_them`; the text pass's
caret test, whose three carets across `"  "` used to stand at one x and now stand
strictly increasing, is the same rule read through shaping.

## See also

- [Block Document](./document-model.md): the tree a leaf paragraph belongs to,
  and where inline attributes live.
- [Inline Boxes](./layout-inline-boxes.md): the design for a widget inside a
  line, which this page's atom model is the substrate for.
- [Layout](./layout.md): the two passes and the constraint model.
- [Text and Input](./text-and-input.md): shaping, clusters, and line breaking.
- [Box Model](./layout-box-model.md): padding around the text leaf.
