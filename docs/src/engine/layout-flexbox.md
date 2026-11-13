# Flexbox

A flex line resolves free space with the CSS Flexbox §9.7 algorithm, implemented
in `ui-core/src/flex.rs`. Do not reimplement distribution in a container:
the freeze loop, the sub-unit grow factor rule, and shrink weighting are the
three places a hand-written version gets it wrong.

## Inputs

One item contributes:

| Field | Meaning |
| --- | --- |
| `basis` | Flex base size before min/max clamping |
| `grow` | Share of positive free space |
| `shrink` | Weight when the line overflows |
| `min`, `max` | Clamped bounds, with the content floor already folded into `min` |
| `hypothetical` | `clamp(basis, min, max)`: what the item wants before free space is shared |

The container supplies its inner main size and the gap between adjacent items.
Gap is reserved before distribution, not charged to the first or last item.

## The algorithm

`solve_line` returns one main size per item:

1. If the hypothetical total (plus gaps) already equals the available space,
   return the hypothetical sizes.
2. Freeze items whose factor is 0, and items whose basis is already past
   their hypothetical.
3. Loop: distribute the remaining free space among unfrozen items by factor,
   freeze any that reach a bound, repeat. The loop freezes at least one item
   per round, so it ends within `n` rounds.
4. When the sum of unfrozen factors is below 1, they claim only that fraction
   of the original free space. This is the rule that surprises hand-written
   implementations.

## What this engine adds

- `Grow::Split(w)` grows from a zero basis, which is `flex: 1 1 0`.
  `Grow::Yes` and `Grow::Flex(w)` grow on top of the content, so equal
  weights give unequal sizes.
- `AxisSwitch::max_cross` turns a row into a stack while measuring: when
  it has one child, when the row's main content exceeds the available space,
  or when a child's cross size exceeds the threshold. It is a layout decision
  made in the measure pass, not a wrap step. A stacked child reads
  `margin_alt` if it declared one.
- Cross-axis alignment is a scalar (`cross_align`, `cross_pos`), not
  `justify-content` and `align-items`. A child overrides it for itself with
  `align_self` (`Align`: `Stretch`, `Start`, `Center`, `End`), which is the
  cross axis on a `Column` as well as on a `Row`; `justify_self` stays a grid
  property, because a flow item's main slot is its own main size and leaves no
  slack (CSS ignores `justify-self` in flex for the same reason).
- `cross_baseline` replaces that scalar with one rule: every child's baseline
  sits on the line's, so text of different sizes and a mark beside it share a
  line. A child that reports no baseline is placed by its bottom margin edge,
  which is the baseline CSS synthesises for a box.

## Not modelled

Deliberate: `order` and `row-reverse` (paint and focus stay in declaration
order), inline-flex, content-box sizing (sizes are border boxes), and in-flow
auto margins. Missing, not decided: `wrap` and line formation,
`justify-content`, `align-items`, `align-content`, and `justify-self` on a flow
item.

Baseline alignment exists for a `Row`'s children only, and only where a child
reports a baseline: a text or math leaf does, a container does not, so a nested
wrapper is placed by its bottom edge rather than by the baseline of the text
inside it.

A percentage declared on a flex item's main axis is not used as its basis:
placement reads `Px` only, so the item is placed at content size. Do not rely
on percentage flex bases.

## Corrections

**An item could not align itself on the cross axis.** `align_self` and
`justify_self` were stored on the grid item and read only by grid placement, so
a `Column` or `Row` child could not override the line's `cross_align`/
`cross_pos`. A flow child now reads `align_self` first: `Stretch` fills the
line, the others keep the child's own cross size and move it, and the explicit
request wins over `cross_baseline` and the line's `cross_pos`. `justify_self`
remains grid-only, as CSS leaves `justify-self` without effect on a flex item.
Pinned by `a_flow_item_aligns_itself_across_the_line`, watched red with the
child's alignment ignored (a centred item read x 0 where 40 belongs).


**The line was said to be solved twice per container per frame, and it is
not.** An audit reported the measure pass's `squeezed_limits` and the placement
pass as the same solve run twice. Counted over two solves of a squeezed row,
`line_items` ran four times on the first (two for the row: one to find the caps,
one to place; one for each childless box) and three on the second — the measure
pass answers only when a line is squeezed *and* is being remeasured, and its
input is the pre-reflow size, while placement solves the post-reflow sizes. The
two answer different questions; carrying one into the other would be wrong, not
faster. The only per-frame duplicate found was the stacked-axis lookup repeated
for every child, which is now read once.

**A scrolling line froze its zero-basis growers at min-content.** The line's
available main size was the sum of the items' bases, and a `Split` grower's
basis is zero, so a scrolling row handed its line less space than its content
needed: the solver shrank every `Split` item to its minimum while the row
itself was still sized by that content. The line now sums what each item wants
(its basis, or its content for a zero-basis grower); pinned by
`a_scrolling_row_keeps_a_zero_basis_child_at_its_content_width`.

**Cross-axis alignment was written on the child.** `cross_align` and
`cross_pos` were set on item declarations, where placement never read them;
both are properties of the line. The parent now owns them: `cross_align` is
the fraction of spare cross space the item occupies, and `cross_pos` places
what it leaves over. A value on the child is inert.

**A baseline row was still as tall as its tallest child.** Placement alone cannot
answer this: moving a 12sp text down to meet a 24sp one pushes its descender
past the line's box, so the line needs both the rise above the shared baseline
and the fall below it. Measure now unions those two, and the union is the line's
min- and max-content height as well — without the max-content half, the hug
height clamp took the tallest child back and cropped the descender. Both halves
are pinned by `a_baseline_row_shares_one_line_and_grows_to_both_sides_of_it`,
which reads 26 where the union gives 30 when the measure half is dropped, and
(10, 20, 24) where the placement half is.

**Stretch erased a deficit.** The cross-axis formula clamped the surplus with
`.max(0)`, so an item taller than its line kept its content height and
overflowed instead of being squeezed. The ratio now applies to negative spare
space as well, matching `align-self: stretch`, and a stretched scroll child
shrinks to its line before anything overflows.

## Traps

| Change | Symptom |
| --- | --- |
| Distribute free space without freezing at bounds | A flexible item overshoots its `max`; the next item is starved |
| Ignore the "factors below 1" rule | Items with `grow: 0.5` take all the space instead of half |
| Use content size as the basis for a `Split` item | Equal weights become unequal |
| Charge gap to one item | The last item is smaller by one gap |
| Map `Grow::No` to shrink 0 | Hug text refuses to shrink to min-content and overflows; every grow mode shrinks with factor 1 |
| Expect wrap or line formation | There is none; `AxisSwitch` stacks the row instead |
| Size a baseline line by its tallest child | The deepest descender is cropped; the line is the rise plus the fall |

## See also

- [Layout](./layout.md): the two passes and the constraint vocabulary.
- [Grid](./layout-grid.md): track sizing, the other distribution algorithm.
- [Box Model](./layout-box-model.md): padding and margin around an item.
