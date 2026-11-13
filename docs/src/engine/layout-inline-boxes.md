# Inline Boxes

A widget laid out inside a line of text: how the paragraph measures it, where it
sits, and what this model deliberately leaves out. Stage 1 is implemented — a
holder child is measured, placed and painted; interaction and intrinsic sizing
are designed only.

## Problem

A sentence can already carry a picture. An atom reserves a box and an
`InlineObject` paints it: a formula, the glyph an HTML entity stands for. It
cannot carry a *widget* — something with its own layout and state. A markdown
task list wants a checkbox at the start of its line; a mention or an inline code
chip wants a rounded box with padding around a run of text; a toolbar wants a
button inside a sentence.

Three things are missing, one per pass:

| Pass | Today |
| --- | --- |
| Measure | `NodeData::Text` calls `TextMeasure::layout` and never looks at its children (`ui-core/src/layout_engine.rs:1311`); an atom keeps whatever size its builder wrote |
| Place | `place_node` reads container parameters only (`layout_engine.rs:1615`), so a child of a `Text` node never gets a rect |
| Paint | The text pass paints `InlineObject::Math` and `::Glyphs`; nothing paints a node at an atom's box |

The third gap is what makes the first two necessary rather than cosmetic: the
text pass cannot paint a widget, and should not learn how.

## Comparison

**CSS and Blink** model a widget in a line as an *atomic inline*: an
inline-block or a replaced element is "a single opaque box" that "cannot split
across lines" (CSS 2.1 §9.2.2; CSS Display 3, "atomic inline"). Its width is
shrink-to-fit against the containing block — "min(max(preferred minimum width,
available width), preferred width)" (CSS 2.1 §10.3.5) — and the available width
is the containing block's, not the room left on the line. It gets a soft wrap
opportunity before and after it, even next to a no-break space (CSS Text 3 §5.5;
Blink `line_breaker.cc`), so it moves to the next line when it does not fit and
overflows instead of splitting when it is wider than the whole line (CSS 2.1
§9.4.2).

Its baseline is "the baseline of its last line box", or the bottom margin edge
when it has no in-flow line box or its overflow is not visible (CSS 2.1 §10.8.1);
a replaced element has no content-derived baseline at all, and CSS Inline 3 §A.3
tells the UA to synthesise one at its line-under edge. Blink splits the work the
way this engine does: line breaking measures the items into `InlineItemResult`s
and line box construction orders and positions them (LayoutNG README). The caret
stops at a replaced element's edges — Blink's `EditingIgnoresContent` and
`HTMLImageElement::CanContainRangeEndPoint()` — while an inline-block may contain
one.

**Flutter, Android and Qt** answer "measure first" three ways. Flutter lays out
every `WidgetSpan` child as a real `RenderBox` with `maxWidth` set to the
paragraph's width, never the room left on its line, pushes the resulting
`PlaceholderDimensions` into the painter, and paints and hit-tests the child as
an ordinary render object (`paragraph.dart`, `text_painter.dart`). Android's
`ReplacementSpan` measures itself: `getSize` returns a width and fills a
`FontMetricsInt`, and `TextLine` substitutes the span with U+FFFC and U+FEFF so
the unit cannot break. Qt's `QTextObjectInterface` returns only an
`intrinsicSize`, with no baseline at all; `QTextLayout` breaks before the object
when it does not fit and gives an oversized one a line of its own
(`qtextlayout.cpp`). All three put one object replacement character in the text,
as this engine already does.

The baseline and interaction contracts differ with who owns the layout. Flutter
asks the child for a real baseline, owns the layout, and gets focus, hit-testing
and semantics for free; Android and Qt delegate drawing to the span or object and
lose touch, IME and copy inside it — Qt's documentation says "Copy and Paste
operations ignore custom text objects", and a `WidgetSpan`'s content is not
selectable either. This design follows Flutter: a real node, painted and
hit-tested by the ordinary walker, with the caret stopping at the atom's edges as
in all three.

One deliberate difference follows from what this engine reports:

| CSS | Here |
| --- | --- |
| An inline-block's baseline is its **last** line box | The holder's baseline is the **first** one the measure pass reports; a one-line chip agrees, a wrapping holder sits higher than CSS would put it |

The fallback matches CSS. A holder that reports no baseline — a container does
not derive one — sits by its bottom edge, the synthesised baseline a replaced
element gets (CSS 2.1 §10.8.1; CSS Inline 3 §A.3).

Measurement and placement are separated in both systems for the same reason: the
box's size must be known before the line breaks, and its x position only after.

Sources: [CSS 2.1 §10.8.1](https://www.w3.org/TR/CSS21/visudet.html#inline-box-height),
[§10.3.5](https://www.w3.org/TR/CSS21/visudet.html#float-width),
[§9.2.2](https://www.w3.org/TR/CSS21/visuren.html#inline-level),
[CSS Display 3](https://www.w3.org/TR/css-display-3/#atomic-inline),
[CSS Text 3 §5.5](https://www.w3.org/TR/css-text-3/),
[CSS Inline 3 §A.3](https://www.w3.org/TR/css-inline-3/),
[LayoutNG README](https://source.chromium.org/chromium/chromium/src/+/main:third_party/blink/renderer/core/layout/inline/README.md),
[Blink `line_breaker.cc`](https://source.chromium.org/chromium/chromium/src/+/main:third_party/blink/renderer/core/layout/inline/line_breaker.cc),
[Blink `editing_utilities.h`](https://source.chromium.org/chromium/chromium/src/+/main:third_party/blink/renderer/core/editing/editing_utilities.h),
[Flutter `paragraph.dart`](https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/rendering/paragraph.dart),
[`text_painter.dart`](https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/painting/text_painter.dart),
[`placeholder_span.dart`](https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/painting/placeholder_span.dart),
[Android `TextLine.java`](https://github.com/aosp-mirror/platform_frameworks_base/blob/master/core/java/android/text/TextLine.java),
[`ReplacementSpan.java`](https://github.com/aosp-mirror/platform_frameworks_base/blob/master/core/java/android/text/style/ReplacementSpan.java),
[Qt `QTextObjectInterface`](https://doc.qt.io/qt-6/qtextobjectinterface.html),
[Qt `qtextlayout.cpp`](https://github.com/qt/qtbase/blob/dev/src/gui/text/qtextlayout.cpp).

## Contract

### The binding

An inline box is a child of the text node, named by an atom:

```rust
pub struct AtomBox {
    // at, len, size, baseline, align, object — as today
    /// A node laid out inside this box, instead of an object the text pass paints.
    pub holder: Option<Id>,
}

impl AtomBox {
    /// Builder: this atom holds `id`, a child of the text node, instead of a painted object.
    pub fn holder(mut self, id: impl Into<Id>) -> Self;
}
```

The child is declared as any other child, on the `Text` decl:

```rust
let mut text = Decl::container(id, NodeKind::Text);
text.data = NodeData::Text { block, .. };
text.child(chip)
```

The atom names the child by `Id`, not `NodeId`: a decl is built before the tree
exists.

### Invariants

| # | Invariant | What breaks it |
| --- | --- | --- |
| I1 | An atom carries a `holder` or an `object`, never both; neither means the faint placeholder | Two painters at one byte, or a widget plus a placeholder under it |
| I2 | A holder atom paints nothing in the text pass | The placeholder box shows through the widget |
| I3 | A holder is a descendant of the text node whose atom names it | The node has no rect to inherit, and `mark_dirty_up` cannot invalidate the wrap |
| I4 | The atom's box is the holder's border box; margins are not modelled inside a line | A margin silently does nothing (use padding) |
| I5 | The atom's baseline is the holder's reported baseline, else its height | A box with no baseline sits by its bottom edge, the way CSS synthesises one |
| I6 | The holder is measured at the text block's wrap width, so its width is shrink-to-fit | Measured against the room left on its line instead: the width changes with where it sits |
| I7 | Measure, place, paint and hit-testing read the line origin from one function | A widget that paints where the caret does not think it is |
| I8 | A holder atom spans one U+FFFC in stage 1, so no cut can split it | The cluster cut splits a box that replaced several bytes |

A binding that does not resolve — the id is gone, or names a node outside this
text node — leaves the atom at its declared size and paints nothing. A stale
binding must not break the paragraph.

I3's invalidation already exists: `mark_dirty_up` (`ui-core/src/tree.rs:1469`)
marks every ancestor, so a change under a holder reaches the text node. I8 holds
because the wrap's cluster cut never splits a cluster
(`ui-text/src/linebreak.rs:283`), and a holder spans one `U+FFFC`, which is one
cluster.

The sentence that ties them together: **the paragraph sees an opaque box of a
fixed size.** It wraps around it, aligns it on a baseline, and gives it a rect.
What is inside the box is laid out by the ordinary engine — one level down, with
its own children, its own scrolling, its own state. This is the CSS
atomic-inline model (`inline-block` and replaced elements), not a general inline
formatting context.

### Geometry

Today the line's origin is two expressions that only the painter knows —
`line_x` (`ui-platform/src/text_pass.rs:978`) and `line_top` (`:1032`). Nothing
else can turn an `InlineAtom` into a surface rect, which is why placement cannot
put a node there. Stage 1 moves both into `ui_core`:

```rust
impl InlineLayout {
    /// Top-left corner of line `index` inside a placed text node.
    pub fn line_origin(
        &self,
        index: usize,
        origin: Point,      // the node's content origin
        box_height: f32,    // the node's box height, as paint uses it today
        wrap_width: f32,    // `Node::wrap_width`
        indent: f32,
        align: TextAlign,
    ) -> Point;
}
```

`InlineAtom::rect(line_origin)` then gives the holder's rect to placement, to the
painter and to the hit region, all from the same call. The move kept the
painter's centring exactly: the block is centred in the node's box height while
`text_origin` is the content origin, so unequal top and bottom padding still
drift the block by `(pad.top - pad.bottom) / 2`. Changing that is a separate
correction with a test of its own; the shared function is where it would land.

### What it does not do

- **Non-atomic inline boxes.** A box that shares its line with text and also
  wraps across several lines needs line-box fragmentation: the box is cut into
  per-line fragments, each with the correct padding and background. That is a
  second layout mechanism, and no component in this repo needs it yet.
- **Margins inside a line.** Use padding; a margin would have to take part in
  the wrap width and the baseline.
- **Inline-flex**, floats and `vertical-align: text-top/bottom`. The four
  existing `VerticalAlign` values are what an atom has.
- **Caret inside a holder, and selection ink over it.** The paragraph selects
  the atom's bytes, which is what a copy yields. A widget with editable content
  is a nested editor, not an inline box.
- **Figures.** A holder measured at the paragraph's width can reflow; a figure
  that must keep an aspect ratio is the image block's job.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui-core/src/text.rs` | `AtomBox::holder`, the line-origin function, `aligned_x` moved from the platform layer |
| `ui-core/src/layout_engine.rs` | `NodeData::Text` measure: measure holders, stamp each atom's size and baseline, then wrap; `place_node`: place holders at their atom rects; baseline of a text node unaffected |
| `ui-platform/src/text_pass.rs` | Skip the placeholder for a holder atom; call the shared line origin for paint and for every geometry query |
| `ui-widgets/src/text.rs` | A builder that takes a child and an object character, so a widget does not hand-write `AtomBox` |
| `ui-richtext` + `ui-widgets/src/markdown.rs` | A task list carries the box's state, but draws it in the item's marker cell, not through a holder — see the correction |

The measurer stays pure: `TextMeasure::layout(block, wrap)` still sees a block
whose atoms already have their sizes. The engine measures the holders before it
calls the measurer, which keeps `MonospaceMeasure` and the font-backed pass
honest and keeps the tree out of the text layer.

## Verification

| Test | What it catches |
| --- | --- |
| `an_inline_holder_gives_its_atom_the_measured_size` | An atom left at its builder's size while the child is 30×20: the text runs under the widget |
| `an_inline_holder_is_placed_where_its_atom_lands` | A holder left at the parent origin, or placed before an alignment that moves the line |
| `a_holder_without_a_baseline_sits_on_its_bottom_edge` | A box aligned by its top against the text baseline |
| `a_holder_wider_than_the_line_takes_a_line_of_its_own` | A wrap that splits or overlaps an oversized widget |
| `a_holder_atom_paints_no_placeholder` | The faint box drawn under a real widget |
| `a_centred_line_puts_its_holder_at_the_aligned_origin` | A line origin read from the left edge instead of the alignment |
| `a_narrow_parent_never_squeezes_a_holder_below_its_own_width` | A minimum content taken from the words alone, leaving the widget clipped by the parent |
| `a_holder_that_grows_rewraps_its_paragraph` | A text node answering its measure from the cache after a holder grew: the old wrap with a stale atom |
| Gallery `inline holder` | The whole path on real fonts: the widget's rect inside its line band, no glyph painted over it, the text after it starting at its right edge, a press on it taking focus and a press beside it clearing focus |

The layout tests run on `MonospaceMeasure`: no GPU, no compositor, no system
fonts; the placeholder test and the gallery check need a face. A future fix for
the padding drift will need its own test, which does not exist yet.

## Status

Stage 1 is implemented; stages 2 and 3 are designed only.

1. **Measured holders — done.** `AtomBox::holder` and `InlineAtom::holder`, the
   measure and placement branches in `layout_engine.rs`, `InlineLayout::line_origin`,
   and the placeholder skip in the text pass. A holder that breaks the rules
   (an id that does not name a child) keeps the atom's declared size.
2. **Interactive holders — click and focus done.** A press on a holder focuses
   it, and a press on the words beside it clears focus instead; the gallery
   `inline holder` check asserts both. The press rule already treats a
   `Sense::CLICK` node between the press and the input as the control's own
   (`press_belongs_to_input`, with its own test), so the paragraph never reads
   that press as a caret. Keyboard activation inside a paragraph, and
   scroll-into-view ignoring the box, are not built.
3. **Intrinsic sizing — done, and it needed no new code.** A holder needs no
   separate min-content plumbing: its atom is an unbreakable box of the stamped
   size on every line, so the box is already inside both the wrap and the minimum
   content the measurer reports, and a parent narrower than the widget cannot
   squeeze the paragraph below it. The measure cache needs nothing either: a size
   change arrives as a decl change on the holder or below it, `mark_dirty_up`
   marks every ancestor, and the dirty text node measures and re-stamps. Two
   layout tests pin both halves (see Verification); each was watched failing
   against the change that would break it. The consumer this stage was planned
   around was built elsewhere: see the correction below.

Open questions:

- A holder whose size comes from state the decl does not carry would leave the
  paragraph clean: the measure cache answers from `dirty` and the constraints
  alone, and only a decl change marks the chain. Today every node's size is in
  its decl, so the case does not arise; a state-driven one would have to declare
  the size it renders at.
- Whether `AtomBox::holder` should force `len` to one `U+FFFC`, or accept a
  source range the way an editor's object does. Stage 1 authors one `U+FFFC` and
  the builder does not enforce it.

## Corrections

**The task list was planned as this path's first consumer, and never became one.** The item's
`[ ]` bytes belong to its marker, which no paragraph frame holds, so a holder inside
the item's first paragraph would have named a widget over bytes that are not the
paragraph's text; the box would also have pushed that item's text right of every
sibling's column. It is drawn in the marker cell where the marker's ink was, and the
`[x]` bytes still show when the caret enters the item (see
[Markdown Editor](./markdown-editor.md)). The consequence is that the holder path's
only consumer is the gallery sample. The gallery's `inline holder` check and the
layout tests are what keep the path honest until a real one exists.

**The placement test first proved itself.** It derived the expected rect from
the same `line_origin` call placement uses, so a wrong origin formula would have
passed on both sides; it only pinned that a rect existed at all. The absolute x
in `a_centred_line_puts_its_holder_at_the_aligned_origin` and the explicit box
size in `an_inline_holder_is_placed_where_its_atom_lands` are what can disagree
with the code. Both were watched failing with placement turned off: the chip
stayed at the text node's origin.

**The third stage was described as work that was already done.** This page said a
holder's min-content in a hug width was unbuilt, and that a holder growing past a
cached measure would leave the paragraph clean. Neither was true. The stamped atom
is an unbreakable box on every line, so its width is already inside both the wrap
and the minimum content (the measurer floors on it, and the engineering reason is
the same one that keeps a control from being clipped); and a size change always
arrives as a decl change, which `mark_dirty_up` carries to the text node, so the
node measures and re-stamps. What was missing was a check, not a mechanism. Two
layout tests now state both, each watched failing against the one change that
would break it: the atom floor removed from `MonospaceMeasure::intrinsic` turned
the narrow-parent test red, and a text node answering across `dirty` turned the
grow test red.

## See also

- [Inline Content](./layout-inline-content.md): runs, atoms, line boxes and
  baselines — the paragraph side of this contract.
- [Layout](./layout.md): the two passes and where a leaf's measure fits.
- [Block Document](./document-model.md): how a markdown block becomes a node,
  which is what a task-list checkbox would attach to.
