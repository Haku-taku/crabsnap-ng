# Menu

MD3's menu surface: a short list of commands with an optional leading mark,
supporting text and a trailing shortcut or icon. This page covers the box the
export describes and the state model a row carries.

The engine already paints one menu: a secondary click collects text rows and
draws them through [`MenuStyle`], which MD3 supplies as `menu_style`. That
menu is the engine's, and stays text-only. `Md3Menu` is the surface an app
places itself, for a menu button, a split button's panel, or a command list
inside a popover.

## The box

| Part | Value |
| --- | --- |
| Surface | `surfaceContainerLow`, elevation 3 |
| Container radius | 4 dp |
| Container padding | 2 dp |
| Row gap | 2 dp |
| Row height | 44 dp minimum, plus 8 dp above and below |
| Row padding | 16 dp leading, 16 dp trailing |
| Mark to text gap | 12 dp |
| Leading and trailing mark | 20 dp |
| Label | 14 sp / 20 sp, weight 500 |
| Supporting text | 12 sp / 16 sp, weight 400 |
| Both texts | `onSurface`; supporting text and marks `onSurfaceVariant` |

The table is `md.comp.menus` in `tokens/menus.json`. Its geometry is written
once, outside the `standard`/`vibrant` split, and this build paints the
`standard` section only. The export names the container's and the rows' shape
`rounded_corners` without a number; 4 dp is Flutter's `_MenuDefaultsM3` menu
border, the authority this build reads MD3 visuals from.

A row is 44 dp when it holds a label and grows when a supporting line or a
trailing shortcut needs the room: the export's height is a minimum, and its
8 dp top and bottom spaces are the row's own padding. The container's 2 dp
padding and the rows' 2 dp gap are the export's `group.padding` and root
`gap`.

Every row's fill is `rounded_corners`, the same shape name the container
carries. The export also names a first-child and a last-child inner corner
with that same value, so this build rounds every row by 4 dp and does not
read those two states.

## States

| State | Row fill | Ink | Layer |
| --- | --- | --- | --- |
| Rest | none | `onSurface` | — |
| Hovered | none | `onSurface` | `onSurface` 8% |
| Focused | none | `onSurface` | `onSurface` 10% |
| Pressed | none | `onSurface` | `onSurface` 10% |
| Selected | `tertiaryContainer` | `onTertiaryContainer` | 8% hover, 10% focus or press of `onTertiaryContainer` |
| Disabled | as its selection | `onSurface` at 38% | none |
| Disabled and selected | `tertiaryContainer` at 38% | `onTertiaryContainer` at 38% | none |

A selected row keeps the same state layers as an unselected one, only in the
selected ink. The focus ring is `secondary`, 3 dp; the export insets it by
3 dp and the decl draws it on the row's own edge, as the button does.

The 8% hover and 10% focus or press fade over the shared 100 ms reaction
clock.

## Sections and dividers

A section label uses `section-label-text` (`onSurfaceVariant`) at the
`label-small` type style, 24 dp tall with the row's 16 dp leading padding.
A divider is `md.comp.divider`'s 1 dp `outlineVariant`, inset 16 dp at both
ends. Neither carries geometry in the menu export, so both come from the M3
spec's list rules.

## Reporting

One activation reports `Change::Selected(Some(item))` with the row's id. The
app owns which row is selected, sets it back as `Md3MenuItem::selected`, and
decides whether a press closes the menu; the widget holds no selection of
its own. A disabled row reports nothing.

## Deviations

**Text-only engine menu.** `menu_style` predates this table and cannot show a
mark or a supporting line, because the engine's `MenuItem` has only a label,
a shortcut and an enabled flag. The widget's richer rows do not replace it.

**The engine menu's row margins.** A row's hover shape sits `row_inset` in from
the container's sides; MD3 supplies the row's own `top_space`, 8 dp, so the
highlight keeps the same margin on all four sides instead of a tighter one at
the sides. The label keeps its `leading_space` from the container's edge, which
puts it 8 dp inside the shape, mirroring the chord's `trailing_space`.

**The engine menu's material.** `MenuStyle.frost` blurs what is painted behind
the surface. MD3 sets it only when `Md3Options.menu_blur` is above zero and the
pack's `device` reports it can sample, and then tints the surface at 80% so the
picture behind stays legible through the blur. A backend without backdrop blur
gets the opaque `surfaceContainerLow`.

**No width cap.** The export carries no menu width, and M3's 280 dp cap is
not applied: a long label makes a wider menu unless the app constrains it.
The surface hugs its content with a 112 dp minimum, the M3 floor.

**One radius for every row.** The first-child and last-child inner corners
are the same `rounded_corners` as every other row, so the widget gives all of
them 4 dp rather than tracking which row touches a container corner.

**No scroll.** A menu taller than its space is the caller's problem: wrap the
surface in a scroll container, or shorten the list. The export describes no
scrollbar or section pinning.

## See also

- [Design Tokens](./../reference/tokens.md): how `md.comp.menus` becomes
  `menus_spec`.
- [Split Button](./split-button.md): the other control that opens a menu and
  reports the half that was pressed.
- [Icon Button](./icon-button.md): where the shared 100 ms reaction clock and
  the row state layers come from.

[`MenuStyle`]: ../engine/popovers.md#menus-are-the-exception
