# Layout

Layout is a CSS subset with a fixed shape: every node declares how each axis
sizes itself, the solver measures bottom-up, then places top-down. There is one
solve per frame and no global retry; a container that would squeeze a child
re-measures that child locally with a tighter main-axis cap.

## Vocabulary

| Type | Meaning |
| --- | --- |
| `Len` | `Auto`, `Px`, or `Pct` of the containing block on the same axis. An unresolvable percentage acts as `Auto` |
| `Sizing` | `Definite(Len)`, `Fit` (hug content), `Stretch` (fill), `Auto` (decided by the container) |
| `AxisSize` | `min`, `hint`, `max`, and the `Sizing` that produced them. The numbers alone cannot distinguish hug from fill |
| `AxisSizes` | The context-free form: `Sizing` plus explicit bounds; min starts at 0, max unbounded |
| `SizeClass` | Both axes plus `grow_x`/`grow_y` |
| `Grow` | `No`, `Yes` (take the spare space), `Flex(w)` (share by weight), `Split(w)` (grow from a zero basis) |
| `Constraints` | `min` and `max`. The type documents `min` as a hard floor, but the solver reads only `max` today; `Constraints::tight` is unused, so a nonzero `min` is silently ignored |

`Grow::Split` exists for `flex: 1 1 0`: equal weights give equal sizes
regardless of content. `Yes` and `Flex` grow on top of the content instead,
which makes equal weights unequal.

## The two passes

1. **Measure, bottom-up.** Each node reports its demand for the constraints
   it was given. A fill axis measures like a hug one: the space it is meant to
   take is granted at placement, by the parent's cross-axis rule or by `Grow`
   on the main axis, so the solver treats `Stretch` and `Auto` alike.
2. **Place, top-down.** Each container positions its children from the sizes
   it settled on.

There is no global second pass. The one exception is a line that would squeeze
a child: the container re-measures that child with a tighter main-axis cap
before placing it. A container that needs a child's size to choose its
algorithm must decide during measure from the demand alone. `AxisSwitch` is the
model: a row becomes a stack when a child is taller than `max_cross`,
determined while measuring.

The root fills the available space unless it declares a fixed size or an
explicit grow policy. Without that, nothing inside could ever receive spare
space.

## Constraints at build time

Build runs before layout, so a widget's `decl` cannot be handed the space its
parent is about to give it. `Cx::constraints_of(id)` is the working substitute:
the constraints layout measured that node under last frame — the slot the parent
offers, which a hugged child learns even though it took less. A control that
must know its width at build (a text field's wrap width, a grid's column count)
reads it there and falls back to its declared size on the first frame; a resize
reaches it one frame later, the same lag `rect_of` has. An exact answer would
require building a subtree from inside layout, which no control needs yet.

## Text leaves

A text leaf's height hint is one line box, so it is a floor for one line and
never a ceiling on the block. `layout_size_class` moves that hint into `min`
for text, which leaves the derived maximum open. Treating it as a cap would
clip every paragraph to its first line.

Measurement is injected through `TextMeasure`; `MonospaceMeasure` is the
deterministic implementation for tests. `LayoutStats::remeasured` counts nodes
actually measured, which is the number to watch when layout gets slow.

## Positioning

| Scheme | Behaviour |
| --- | --- |
| `Static` | In flow; insets ignored |
| `Relative` | Keeps its slot; insets shift paint and hit only |
| `Absolute` | Out of flow; placed against its containing block, insets resolved per side |
| `Fixed` | Out of flow; containing block is always the root box |

`Placement::overlay` places a node from an anchor instead of insets and lets
it escape every ancestor clip. `Placement::lift` paints at the top layer
without an anchor but keeps its layout slot. Only a positioned node has an
effective `z`; static children count as 0.

## Stack children

A `Stack` has no line, so there is nothing for a child to grow into. A child declared
`AxisSize::fill` on an axis therefore takes the stack's content box on that axis, the answer CSS
gives `position: absolute; inset: 0`; its declared `min` and `max` still clamp that. A `hug` or
fixed child keeps its measured size and is placed by `stack_align`.

The rule exists because a filling child used to measure its content — zero for an empty box —
and a full-bleed layer (a frozen frame, a scrim, a backdrop) silently drew nothing. Pinned by
`a_stack_stretches_a_fill_child_to_its_content_box`.

## Overflow

`Clip { x, y }` is per axis and opt-in: `Visible` (default) contributes no
clip, `Hidden` cuts at the box, `Scroll` also lets the wheel move content. A
scroll container's extent is recorded for the engine-owned scroll pass; see
[Scroll Containers](./scroll.md).

## Traps

| Change | Symptom |
| --- | --- |
| Treat a text height hint as a maximum | Paragraphs clip to one line |
| Compute content-box sizes | Padding and border shrink the content instead of the box growing |
| Resolve a percentage against an indefinite parent | It must read as `Auto`; treating it as 0 collapses the box |
| Add a rule that needs a global second pass | None exists; only the squeezed-child local re-measure will run, so the result is wrong |
| Use `Grow::Yes` for equal flex weights | Equal weights produce unequal sizes; use `Split` |
| Expect `z` on a static node | It has none; position the node or use `lift` |

## Corrections

**A relative node's percentage insets used the wrong box.** `left: 50%` on a
relative node resolved against the nearest *positioned* ancestor, which is the
rule for absolute and fixed boxes. CSS gives a relative node its parent's
content box, because it keeps its flow slot; a 200-wide parent inside a
positioned panel shifted the child by half the panel instead. The two rules are
now separate, and a static node no longer pays for the ancestor walk it does
not use; pinned by `a_relative_percentage_resolves_against_its_parent`.

**The offset fold compounded on a subtree the declaration cache skipped.**
`accumulate_offsets` folded each node's displacement into `Node::offset` and
read that same field on the next pass. A subtree whose declaration object did
not change is skipped whole, folded value included, so the next pass added the
ancestor's displacement again: the card stayed put while everything inside it
drifted by the card's offset, and only when a sibling dirtied the tree often
enough to force a second layout — a palette pin beside a Markdown pin, whose
rows rebuild every frame. The node now keeps the declaration's own displacement
in `local_offset`, and the fold starts there, so the result depends on the tree
and not on how many passes it has seen. Pinned by
`a_cached_subtree_under_an_offset_keeps_its_place_when_layout_runs_again`.

## Subpages

- [Box Model](./layout-box-model.md): border box, padding, margins,
  percentages, positioning insets.
- [Flexbox](./layout-flexbox.md): the supported subset and its growth math.
- [Grid](./layout-grid.md): tracks, spans, auto-placement.
- [Inline Content](./layout-inline-content.md): baselines, line boxes, and
  atoms.

## Status

Implemented: the constraint model, both passes, single-line flex (grow,
shrink, freeze, gap), grid tracks (fixed, auto, `fr`, `minmax`, content
keywords, `repeat`, dense flow), inline text leaves, positioning and overlays,
opt-in clipping, scroll extents, and a per-node measure cache keyed by dirty
state and constraints.

Not modelled: `Sticky`, `box-sizing` (sizes are always border boxes), flex wrap
and line formation, `justify-content`/`align-items`/`align-content`, baseline
alignment between flex or grid items, named grid lines and areas, and relayout
boundaries (`mark_dirty_up` walks to the root). Cross alignment is a scalar on
the line, which a child overrides with `align_self`; the flexbox and grid pages
list the deliberate omissions with their reasons.

## See also

- [Frame Pipeline](./frame-pipeline.md): where layout sits in the frame.
- [Style Objects](./style-objects.md): how a control's size class is produced.
- [Text and Input](./text-and-input.md): what `TextMeasure` reports.
