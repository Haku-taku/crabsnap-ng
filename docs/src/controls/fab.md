# Floating Action Button

MD3's FAB in its four sizes and five colour styles: the box, the icon, the
elevations that say hover and press, and the lowered form. The extended FAB
and the FAB menu are their own controls.

## The box

| Size | Box | Icon | Corner |
| --- | --- | --- | --- |
| small | 40 | 24 | 12 |
| standard | 56 | 24 | 16 |
| medium | 80 | 28 | 20 |
| large | 96 | 36 | 28 |

Width and height come from `container.width`/`container.height` and the mark
from `icon.size` in `md.comp.fab`; the box is a square at every size, so all
three are read per size. The export names only `rounded_corners`, so the four
corners are the shape scale's medium, large, large-increased and extra-large
values, matching Compose's `smallShape`, `shape`, `mediumShape` and
`largeShape`.

The surface paints `container.color`, the mark `icon.color`, and the shadow
`container.shadow-color` through the elevation. Hover raises the elevation
from 6 to 8; focused and pressed keep 6. Each of those states also paints a
state layer of `state-layer.color` at 0.08 (hover) or 0.1 (focus, press).

## The colour styles

The export spells one style per colour family, plus a `-container` family for
the classic FAB:

| Style | Container | Icon |
| --- | --- | --- |
| `primary-container` | `primaryContainer` | `onPrimaryContainer` |
| `primary` | `primary` | `onPrimary` |
| `secondary` | `secondary` | `onSecondary` |
| `tertiary` | `tertiary` | `onTertiary` |
| `surface` | `surfaceContainerHigh` | `primary` |

`primary-container` is the default FAB, the one Compose builds when no colour
is named. The other four are the expressive styles, whose containers are the
solid colour roles. `surface` is the only style whose state layer is
`primary` rather than its on-colour.

The lowered form drops the shadow to 1 and lifts it to 3 on hover, staying at
1 when focused or pressed. The export writes the lowered values under the
`surface` style only, whose container also becomes `surfaceContainerLow`, so
the other styles have nothing to lower; the axis is still there because the
table carries it for the style that does.

## Focus and disabled

The focus ring is 3 dp of `secondary`, 2 dp outside the box, drawn only for
`focus_visible`. The export carries the ring under `surface` and `branded`
only; every style uses the same secondary ring here.

The export has no disabled values. A disabled FAB paints `onSurface` at 0.12
as its container and at 0.38 for the mark, the pair MD3 uses for a disabled
container and its content, and it drops the shadow.

## Deviations

**The branded style is not drawn.** Its container is a multi-stop gradient of
brand colours the export names as a role the pack cannot resolve, and its
indicator badge is a second subtree; both are out of scope for this control.

**No indicator badge.** The export's `indicator` entries sit under `branded`
only, so a badge on a FAB would need a shape this control does not own.

**The extended FAB is a separate control.** `tokens/buttons/extended-fab.json`
carries its own table with a label and a wider container; it is not a size of
this one.

## See also

- [Icon Button](./icon-button.md): the other mark-only control, and the state
  layer both read per style.
- [Design Tokens](../reference/tokens.md): how the export's two spellings of
  hover and focus are folded into one axis.
