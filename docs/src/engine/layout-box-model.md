# Box Model

Every declared size is a **border box**. There is no `box-sizing` field, and
a border paints inside that box as a stroke, so it takes no layout space.

## Edges

| Edge | Field | Notes |
| --- | --- | --- |
| Margin | `BoxModel::margin` | Outside the border box. May be negative. Shifts placement only |
| Alternate margin | `BoxModel::margin_alt` | Used when a flow parent stacks on its cross axis instead of laying out a row |
| Padding | `BoxModel::padding` | Inside the border box; shrinks the content area |

A container's content box is the declared size minus padding. Siblings are
placed using the border box plus margins. Because the border is painted, not
reserved, a 1 px border never changes geometry.

## Lengths

`Len` is one of:

- `Px(v)` — an absolute value.
- `Pct(p)` — a percentage of the containing block **on the same axis**.
  `left`/`right` resolve against the containing block width; `top`/`bottom`
  against its height.
- `Auto` — not declared; the solver derives it.

A percentage against an indefinite parent acts as `Auto`. It is not an error
and it is not zero. Resolving it as zero collapses the box; resolving it as the
available space makes a fixed-height child depend on its parent's content.

## Positioning

| Scheme | Containing block | In flow |
| --- | --- | --- |
| `Static` | — | Yes; insets ignored |
| `Relative` | Its own static position | Yes; insets shift paint and hit only |
| `Absolute` | The nearest positioned ancestor | No |
| `Fixed` | The root box | No |

`Placement::inset` is four `Len`s. An `Auto` side is derived from the opposite
side or from the static position. When both opposite insets are fixed and the
size is `Auto`, the box is stretch-fit: its size is whatever is left after the
insets and margins.

`z` exists only on a positioned node; static children count as 0.

Two related flags sit on `Placement`:

- `overlay` places a node from an anchor and lets it escape every ancestor
  clip. See [Popovers and Tooltips](./popovers.md).
- `lift` paints at the top layer without an anchor, but the node keeps its
  layout slot. Use it for a visual emphasis that must not change geometry.

## Traps

| Change | Symptom |
| --- | --- |
| Subtract a border from the content size | The box shrinks by the stroke width; the border was never layout space |
| Add `box-sizing` handling | There is no such field; two sizing models would diverge silently |
| Treat `margin` as spacing that cannot be negative | Overlaps and optical alignment become impossible; negatives only shift placement |
| Use `margin` where `margin_alt` is needed | When a row stacks, the stacked child keeps the row's margin and the gap looks wrong |
| Resolve a percentage against the parent's content when it is indefinite | The box collapses to zero |
| Rely on `z` for a static child | It has no effect; position the node or use `lift` |

## See also

- [Layout](./layout.md): the two passes and the sizing vocabulary.
- [Popovers and Tooltips](./popovers.md): `overlay` and the anchor contract.
- [Grid](./layout-grid.md): how a box is aligned inside its cell.
