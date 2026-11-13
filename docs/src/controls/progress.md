# Progress Indicators

The MD3 bar and ring, and the expressive wave form: what each paints, which
value moves it, and the one variant this build leaves out.

## Forms

| Form | Constructor | Box | Active indicator |
| --- | --- | --- | --- |
| Linear | `progress_indicator` | `width` × 4 | a rounded bar |
| Linear wavy | `progress_indicator_wavy` | `width` × 10 | a wave |
| Ring | `progress_ring` | 40 × 40 | an arc from twelve o'clock |
| Ring wavy | not painted | 48 in the table | — |

Every value comes from `progress_indicator_spec`. The two container heights are
states of the form: `with-wave` makes the linear bar 10 dp tall so a 3 dp wave
fits, and the table's 48 dp wavy ring is generated but not drawn.

## The wave

A wavy linear indicator paints a straight track at the vertical centre and a
wave over the part the value covers. The wave's peak and wavelength are tokens
(3 and 40 dp); the indeterminate form switches to the export's 20 dp
wavelength.

Compose owns this form — Flutter's `material` has no wavy variant — and two of
its rules are kept here. The wave travels one wavelength per second, and its
amplitude falls to zero at the ends of the range: no wave at or below 10%, none
at or above 95%, full between. The amplitude change runs over 300 ms.

A shown wavy bar is therefore never still: it reads the engine clock every
frame, as the sweeping forms do, and keeps the loop awake while it is on screen.

The painted ends are the value's ends, as the classic bar's are. Compose lets a
round cap reach half a stroke past the coerced endpoint; this control draws the
cap inside the span, so a bar at 25% covers exactly a quarter of the width.

The wave is sampled every 2 dp and stroked as short joined segments, with a
round cap drawn inside each end. The engine has no path primitive; the chord
error at the steepest point is under 0.05 dp at the 40 dp wavelength and under
0.15 dp at the indeterminate form's 20 dp, neither visible at a 4 dp stroke.

The stop dot is Compose's: a 4 dp circle at the trailing end of the track, its
centre `2 + min((height - 4) / 2, 6)` dp from the right edge. The trailing
space is not in the export, so it is a named constant in the control.

## The ring's wavy form

Compose draws the wavy ring as a rounded star polygon that morphs out of a
circle: the active arc is a segment of a polygon whose vertex count follows the
wavelength, not a radial sine. The engine is not what blocks it — a stroked
path is sampled into `Shape::Line` segments, as the linear wave already is.
What blocks it is the polygon itself: `androidx.graphics.shapes` rounds every
corner with a flanking cubic plus a smoothing factor, and a first prototype
that rounded the corners with plain arcs left a 49-degree kink at each inner
(reflex) vertex of the star, which is visible at 48 dp. The ring therefore
stays classic until that geometry is ported and checked against a Compose
render. The table carries the form's 48 dp size already.

## See also

- [Design Tokens](../reference/tokens.md): how the `with-wave` and `wave`
  segments reach the generated table.
- [External Sources](../reference/external.md): the Compose files the wave
  geometry was read from.
