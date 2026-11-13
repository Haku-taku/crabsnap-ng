# Split Button

MD3's split button: a filled button and a menu button sharing one height and
two inner corners that widen while the pointer is on them. This page covers
the box, the corner morph, and why the panel is drawn from the filled
button's palette.

## The box

| Size | Height | Leading space | Trailing space | Arrow | Inner corner (rest / pressed) |
| --- | --- | --- | --- | --- | --- |
| xsmall | 32 | 12 / 10 | 13 | 22 | 4 / 8 |
| small | 40 | 16 / 12 | 13 | 22 | 4 / 12 |
| medium | 56 | 24 / 24 | 15 | 26 | 4 / 12 |
| large | 96 | 48 / 48 | 29 | 38 | 8 / 20 |
| xlarge | 136 | 64 / 64 | 43 | 50 | 12 / 20 |

The table is `md.comp.split-button` in `tokens/buttons/split-button.json`;
every width is a sum of the label or arrow and its two paddings, and the two
halves are separated by `between-space` (2 dp). The export's `circular` shape
means the outer corners are half the height, so the halves meet in a straight
edge with only their inner corners rounded. It writes the outer radius as 50%
(`50` in the token), which is why the halves stay pills at every size.

The inner corners are where the two halves meet: the leading button's end
corners and the trailing button's start corners. At rest they are the rest
radius above; while either half is hovered or pressed they widen to the
pressed radius, over 100 ms, so the halves read as one rounded block while
the pointer is on them. When the menu is open the trailing button becomes a
pill — all four of its corners are half the height — and its inner corner is
the same full radius.

## Colours

The split button has no colour table of its own. It paints the filled button:
`container.color` and the icon and label inks come from `md.comp.button`
with the `filled` style and the plain (non-toggle) base, which is
`primary`/`onPrimary`. The disabled pair is that table's too: `onSurface` at
0.1 for the container and 0.38 for the content.

The leading half may carry an icon before its label. The split export has no
icon size and no icon-label gap — its paddings are the label's — so both come
from `md.comp.button` for the same size (24 dp and 8 dp at medium), together
with the icon's ink. The paddings stay the export's, so a split button with an
icon is wider than one without by the icon and its gap.

Each half is its own focus stop and its own press target, with the state
layer of the filled button (0.08 hover, 0.1 focus or press). A press reports
which half it landed on: `Change::Selected(Some(id))` carries the leading or
trailing half's id, and the app decides what each means.

## Deviations

**Only the filled variant.** Compose also ships tonal, outlined and elevated
split buttons. The export's split-button subtree carries geometry only, so
the colour family is not an axis here; the other variants would be a table of
their own.

**The corner morph is one radius per half.** Compose animates the four inner
corners independently and uses its motion scheme's default effects. Here one
radius drives both of a half's inner corners, and both halves ride one clock:
100 ms ease-out, the same one the icon button's morph uses.

## See also

- [Floating Action Button](./fab.md): the other compact action surface, and
  the table that does carry its own colours.
- [Icon Button](./icon-button.md): where the shared 100 ms corner morph and
  its shape-scale radii come from.
