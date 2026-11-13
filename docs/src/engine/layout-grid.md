# Grid

A grid is a track template plus placement. Track sizing and placement are
separate: tracks are resolved from the items' content contributions, then each
item is placed in its cell and aligned inside it.

## Tracks

| Track | Sizes to |
| --- | --- |
| `Fixed(v)` | Exactly `v` |
| `Auto` | The largest max-content of the items on the track |
| `Fr(w)` | A share of what fixed and auto tracks leave over |
| `MinContent` / `MaxContent` | The largest min-content / max-content of the items |
| `MinMax(a, b)` | Content contribution clamped into `[a, b]`; an `Fr` bound takes a share of what the other tracks leave |
| `Repeat(n, track)` | The inner track repeated `n` times |

`Repeat::Count(n)` is a fixed count. `AutoFill` fits as many tracks as the
space allows and keeps the empty ones; `AutoFit` does the same and then
collapses tracks no item landed in.

`TrackBound` is separate from `Len` because `fr` is meaningful only inside
`minmax()`.

## Placement

An item either declares a `GridArea` (0-based row and column plus spans) or
is auto-placed:

| Flow | Order |
| --- | --- |
| `Row` | Row by row, left to right |
| `Column` | Column by column, top to bottom |
| `RowDense` | Row flow, backfilling the first free cell |
| `ColumnDense` | Column flow, backfilling the first free cell |

Items outside the template grow the grid: `place_items` extends the column and
row counts to cover every declared area.

Inside its cell, `Align` is `Stretch` (default), `Start`, `Center`, or `End`.
A definite declared size wins over `Stretch` on that axis.

## Traps

| Change | Symptom |
| --- | --- |
| Treat `AutoFit` as `AutoFill` | Empty tracks keep their gap, so the visible columns drift left |
| Ignore an item outside the template | It overlaps the last cell instead of extending the grid |
| Compute `fr` before fixed and auto tracks | The shares are wrong whenever a fixed track exists |
| Use `RowDense` for content whose order matters | Items appear out of reading order; dense backfills holes |
| Clamp an `fr` track's content floor to zero | Text overflows its cell instead of the track growing |
| Rely on `AutoFit` for row tracks | The collapse is re-expanded by the stale row count after placement |
| Expect `Auto` tracks to fit a width below their max-content | The grid paints wider than the box holding it; CSS overflows too, so scale columns yourself |

## Known gap

`AutoFit` collapses empty columns, but the row count is not updated after the
collapse, so collapsed rows are re-expanded as `Auto` tracks. Columns are safe;
rows are not. Fix the row count before treating row `AutoFit` as supported.

## Corrections

**An item kept the lines it wrapped at the grid's width.** Every item was
measured once against the grid's content box and then handed a cell of some
other width, so a paragraph in a `Fixed(100)` column of a 400-wide grid laid out
one 144 px line and then had the row sized to one line's height: the text
overflowed its cell and the row below it started 12 px too high. Measure now
sizes the columns first, re-measures any item whose own width exceeds its cell
at the cell's width, and only then sizes the rows; the intrinsic widths are put
back so that placement, which re-runs the same track sizing, cannot narrow the
column the item was told to fit into. Pinned by
`a_grid_item_wraps_at_the_width_of_its_cell`.

**Placement read the raw size class and capped a stretched text at one line.**
`layout_size_class` moves a text's height hint into its minimum, so measure
treats the line box as a floor; `place_grid` read `size_class` unnormalized, so
`place_in_cell` saw the 1.4 em hint again as a maximum and clamped a text
stretched into a taller cell back to a single line. Measure and placement now
resolve the class the same way, as the flow branch already did. The same test
fails with either half reverted, which is how the two defects were separated.

**A flexible track in an unbounded scroller became infinite.** The `definite`
flag was read from the node's `Sizing` alone, so a `fill` grid inside a
scrolling column divided an infinite space: every `Fr` track was infinite and
the infinite rects reached damage and paint. The flag now also requires the
space the grid can occupy to be finite; pinned by
`a_flexible_track_inside_an_unbounded_scroller_settles_to_its_content`.

**A definite item only raised its track's floor.** The item's contribution was
`max(intrinsic, declared)`, so a `fixed(20)` item with wide text widened its
`auto` track to the text. A definite preferred size *is* the contribution, as
in CSS, and the content is not consulted; pinned by
`a_definite_item_sizes_its_auto_track_instead_of_its_content`.

**Stretch ignored the item's own bounds.** `place_in_cell` filled the cell and
consulted only a definite size, so a `hug(0, 100)` item became as wide as the
cell. A stretched or measured item is now clamped by its own minimum and
maximum, as it is in a row; pinned by
`a_stretched_grid_item_stops_at_its_own_maximum`.

**A column-flow span taller than the template panicked.** The column-flow
branch advanced to the next column instead of growing rows, and the occupancy
probe then indexed past its last row. A span taller than the template now
creates implicit rows, as CSS does; pinned by
`a_column_flow_span_taller_than_the_template_creates_implicit_rows`.

## See also

- [Layout](./layout.md): constraints and the two passes.
- [Flexbox](./layout-flexbox.md): the other distribution algorithm.
- [Box Model](./layout-box-model.md): what sits inside a cell.
