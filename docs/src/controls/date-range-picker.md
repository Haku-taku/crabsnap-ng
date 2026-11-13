# Date Range Picker

The MD3 calendar that chooses two days: the header that names both, the month
list it scrolls through, the band that joins the endpoints, and the rule that
decides what a press means. Only the modal form is drawn.

## The box

| Part | Value | Token |
| --- | --- | --- |
| Surface | 360 × 524, corner 28 | the modal picker's box |
| Header | 68 dp | `range-selection.header.container.height` |
| Title | 14/20, `onSurfaceVariant` | `modal.header.supporting-text` |
| Headline | 22/28, `onSurfaceVariant` | `range-selection.header.headline` |
| Subhead | 14/20 w500, `onSurfaceVariant` | `range-selection.month.subhead` |
| Band | 40 dp, `secondaryContainer` | `range-selection.active-indicator.container` |
| In-range label | `onSecondaryContainer` | `range-selection.date.label-text` |
| In-range layer | `onPrimaryContainer` 0.08 / 0.1 | `range-selection.date.state-layer` |

The table is `md.comp.date-picker.modal.range-selection` in
`tokens/date-pickers.json`, generated as `range_selection_spec`. It carries
only what the range form adds: the band, the in-range label, the subhead and
the header. Everything else — the day cell, its disc, today's outline, the
weekday row — is the docked picker's tree and is read from `date_picker_spec`,
as the modal form already does.

The header is 68 dp because the export's 128 subtracts 60 for the two buttons
Compose's default range header does not draw (`HeaderHeightOffset` in
`DateRangePicker.kt`). Its arithmetic is 8 + 20 (the title) + 28 (the headline
line) + 12 (the gap Compose puts under it). The title takes the modal picker's
supporting-text style: the range table names no title of its own.

The surface takes the modal picker's `surfaceContainerHigh` and 28 dp corner.
The range table's container carries `elevation 0` and `rounded_corners` but no
colour, so the box borrows the one it is shown in; its own elevation adds no
shadow.

## The modal form

The surface is a dialog over the 54% scrim the single picker's dialog uses
(`showDialog`'s `black54`), two growers centre it, and a press on the barrier
reports `DateRangeChange::Dismissed`. The picker is declared only while it is
open, so its entrance is the control's own `Presence`: no warm-up frame and no
app clock. The widget id names the scrim and the surface takes `<id>/surface`,
which is how a press outside the surface is told apart from one inside it.

## The month list

The body is a 48 dp weekday row over a scroller 336 × 408 dp (524 − 68 − 48).
Its content is `months` blocks of 336 dp: a 48 dp subhead (20 + 20 + 8, with
24 dp of left inset) and six 48 dp weeks. Seven 48 dp cells fill 336 exactly,
so the row needs no gaps.

The list opens on the app's `shown` month, which is its first row, so no
initial offset is needed. The window is a count of months, not the app's year
range: `RowWindow::of` keeps the months whose rows meet the viewport plus one
viewport of overscan, and spacers above and below carry the rest of the
height. The engine owns the offset, as for every `SCROLL_OWNED` container.

A month's band is painted per row: 40 dp tall, 4 dp below the row's top. On
the first row it starts at the middle of the start cell when the start falls
in this month, and at the row's left edge otherwise; on the last row it ends
at the middle of the end cell, and at the right edge otherwise; rows between
fill the width. The endpoint's own 40 dp disc covers the half band under it,
which is what rounds the ends. This is `drawRangeBackground`
(`DateRangePicker.kt:1039`) with the 4 dp grid spacing the export's flexible
row adds and this fixed row does not.

## Selection

`next_range(start, end, day)` is Compose's `updateDateSelection`:

- with no start, or with both dates set, the press starts a new range;
- with a start and no end, a day at or after the start ends the range, and an
  earlier day restarts it at the pressed day.

The app holds both dates. The widget reports
`DateRangeChange::Selected { start, end }` with the result of that rule, so the
rule is a pure function of what the app passed in and the day that was pressed,
and the gallery and a self-test can drive it without a clock.

## Deviations

**A module of its own.** Compose ships the range form as its own composable
over shared calendar internals, and so does this pack: the range picker draws a
scrolling month list where the single picker draws one fixed 6×7 grid, so the
two share the day cell, the modal surface and the labels, not a body.

**One form, 360 wide.** Compose's range picker is a full-bleed dialog that
fills the window. This build keeps the modal date picker's 360 × 524 surface,
so the two calendars share a box and the same entrance and scrim.

**Title and headline indent 24, not 64.** Compose reserves 64 dp for the close
button and the mode toggle its header can carry. Neither is drawn here, and
the modal picker's header indents 24; the range form matches it.

**The window is a month count.** Compose lists every month of the year range.
Here `months` names how many months follow `shown`, and the app moves `shown`
to show another stretch. A list that started at the year's January would have
needed a scroll seed to open on the selected month.

**No display-mode toggle and no input mode.** The range picker draws one
picking surface. A text mode for two dates would be a second control over two
fields, not a state of this one.

**No year selection and no keyboard grid.** There is no year list to jump
years; a cell takes a focus ring but the arrow keys do not walk the grid.

## See also

- [Date Picker](./date-picker.md): the single-date calendar this form borrows
  its day cell and its modal surface from.
- [Design Tokens](../reference/tokens.md): what a table that carries a state
  like `in-range` looks like once generated.
