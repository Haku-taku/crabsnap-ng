# Icon Button

MD3's button with only a mark: the four styles, the five sizes, the three
widths, what the shape does while it is pressed, and how a toggle differs from
a plain button.

## The box

| Part | Value | Token |
| --- | --- | --- |
| Container | style colour, per-size height | `container.color`, `container.height` |
| Width | icon + leading + trailing | root `leading-space`, `trailing-space` |
| Icon | 20–40 dp per size | `icon.size` |
| Outline | `outlineVariant`, 1–3 dp | `outline.*` (outlined only) |
| Focus ring | `secondary`, 3 dp, 2 dp outside | `focus.indicator.*` |
| State layer | icon colour at 0.08 / 0.1 | `state-layer.*` |

The table is `md.comp.icon-button` in `tokens/buttons/icon-buttons.json`,
generated as `icon_button_spec`. Its four styles are the style axis:

| Style | Container | Icon |
| --- | --- | --- |
| `standard` | none | `onSurfaceVariant` |
| `filled` | `primary` | `onPrimary` |
| `tonal` | `secondaryContainer` | `onSecondaryContainer` |
| `outlined` | `outlineVariant` stroke | `onSurfaceVariant` |

Sizes fix the container's height and the icon's size; the width is the icon
plus the two spaces, so a size and a width name the whole box:

| Size | Height / icon | Default | Narrow | Wide |
| --- | --- | --- | --- | --- |
| xsmall | 32 / 20 | 32 | 28 | 40 |
| small | 40 / 24 | 40 | 32 | 52 |
| medium | 56 / 24 | 56 | 48 | 72 |
| large | 96 / 32 | 96 | 64 | 128 |
| xlarge | 136 / 40 | 136 | 104 | 184 |

Every row is read from the export: `container.height` and `icon.size` per
size, and `leading-space`/`trailing-space` per size and width. The widths are
the sums, not constants.

## Plain and toggle

`selected` is `Option<bool>` because Compose ships two controls, not one:
`IconButton` has no selection, and `IconToggleButton` has two appearances. The
export's base values — the ones with no `selected` or `unselected` state — are
the plain button in all four styles, so `None` reads them, `Some(false)` reads
the unselected values and `Some(true)` the selected ones:

| Style | Selected |
| --- | --- |
| `standard` | icon `primary` |
| `filled` | container `primary`, icon `onPrimary` |
| `tonal` | container `secondary`, icon `onSecondary` |
| `outlined` | container `inverseSurface`, icon `inverseOnSurface` |

Toggling changes only the layer's colour, not its strength. The export omits
the opacity under `selected`/`unselected`, so the strength is read from the
same style's plain interaction state (`('filled','hovered')` and its siblings);
reading it from the selected query would paint the layer fully opaque, which
is the generated default for a missing value.

## Shape

At rest the container is a circle, and a plain button stays one in every state.
A toggle rests on a 12 dp square (`corner.medium`) whether it is checked or
not, so its hover layer, the fill it gains when checked and the box it sits in
are one shape; only a press narrows it, to 8 dp (`corner.small`). Compose
states the radii in its icon-button tokens: `ContainerShapeRound` is
`CornerFull`, `PressedContainerShape` is `CornerSmall`, and
`SelectedContainerShapeRound` is `CornerMedium`. The export names only
`rounded_corners` and carries no radii, so the numbers come from the shape
scale those names point at. The corner travels between them in 100 ms, as
Compose's `rememberAnimatedShape` does; the token's motion curve forbids a
bounce.

One frame paints that shape three times — the container, the state layer over
it and the ripple under a press — and all three read the same rect and curve.
The container and the layer are both the node's own box with this frame's
corner, and the node declares the same corner as its clip, which is what the
ripple is cut to. The ripple therefore follows the morph instead of keeping
the corner the press began with.

The focus ring sits 2 dp outside the container with a 3 dp stroke, both from
`focus.indicator`; it is drawn only for `focus_visible`, so a mouse press does
not leave a ring behind.

## Deviations

**Corner radii from the shape scale.** The export says `rounded_corners`
without a number; 8 and 12 dp are the shape scale's small and medium corners,
the values the Compose token names resolve to.

**No vibrant style.** Compose's expressive icon button has a vibrant colour
set (`iconButtonVibrantColors`) that the export does not carry; the four
exported styles are the ones drawn.

**No square xsmall variant.** The tokens name both `ContainerShapeRound` and
`ContainerShapeSquare` for the extra-small size; the round one is the default
here, and the width axis already covers a wider target.

**A toggle keeps its square while unchecked.** Compose's `ContainerShapeRound`
gives an unchecked toggle the circle; here it takes the checked square in both
states, so the highlight a pointer sees never changes shape under the click.

## Corrections

**A toggle's hover layer was a disc while its checked fill was a square.** The
state layer read the resting shape, so hovering an unchecked toggle drew a
circle where checking it drew the 12 dp square: the highlight changed shape
under the click, and a bar whose buttons are all toggles showed discs next to
the one square. `corner_target` now follows the selection axis rather than the
selected value — a toggle's hover layer, checked fill and container are one
shape, and only the press narrows it. What catches it now:
`a_toggle_holds_one_square_and_a_press_narrows_it`, whose unchecked case fails
against the old rule.

**The three paints of one button each carried a shape of their own.** The
container was a box shape, the hover layer a plain rect holding the same
numbers, and the ripple a rect target captured when the press began: three
constructions of one idea, so a morph moved the first two and left the splash
in the corner the button no longer had. All three now come from one `corners`
value, the container and the layer are both box shapes, and the ripple is cut
to the node's own clip curve, which is that same value each frame. What
catches it now: `the_fill_the_layer_and_the_ripple_are_one_shape`, which fails
if the layer stops being the box or the ripple stops targeting the node.

## See also

- [State Layers and Ripples](./ink.md): the press and hover paint every style
  of this button shares with the other controls.
- [Design Tokens](../reference/tokens.md): how a control with three axes and
  four interaction flags is generated.
