# Date Picker

The MD3 calendar in its docked and modal forms: what each box holds and what
the modal form adds.

## Forms

| Form | Box | Header | Body |
| --- | --- | --- | --- |
| Docked | 360 × 456 | 64 dp, the title | 40 dp month row, weekdays, six 48 dp weeks |
| Modal | 360 × 524 | 120 dp, title and headline | 56 dp month row, weekdays, six 48 dp weeks |

The container and header numbers come from two tables: `date_picker_spec` for
the docked form and `date_picker_modal_spec` for the modal's container and
header. The shared body keeps reading the docked table, because the modal
export repeats those values; only its day surface is 40 dp, which this picker
already paints as the 40 dp disc inside each 48 dp cell.

Both boxes are full to the token:

- 456 = 64 header + 40 row + 8 + 48 weekdays + 288 grid + 8
- 524 = 120 header + 56 row + 48 weekdays + 288 grid + 12

The modal header is 120 dp because it carries two lines: the app's title in the
supporting-text style (14/20) at 16 dp from the top, and the chosen date in the
headline style (32/40) at 68 dp, above a `md.comp.divider` hairline. Compose
spaces the two with `SpaceBetween`; a fixed offset gives the same pixels
without a main-axis justify the engine does not have.

Both lines are the app's strings, as `CalendarLabels` and `DatePickerParams`
already are. The second is never empty: before a date is chosen it reads the
catalog's `md3-date-picker-no-selection`, the role Compose's default headline
takes when it has no date to format (`DatePicker.kt`, `DatePickerHeadline`). The
three header numbers are pinned by `the_modal_form_reads_the_modal_table`; the
centred surface and the header's 120 dp by the gallery self-test's modal check.

## The modal form

A modal picker is a dialog. A barrier covers the window at the dialog scrim,
two equal growers centre the surface, the surface fades and scales in with the
dialog's pair, and a press on the barrier is reported as
`DatePickerChange::Dismissed`. The app owns the open state and the
confirm/cancel actions; the picker draws neither.

The widget is declared only while it is open, so its entrance is the control's
own `Presence`: no warm-up frame and no app clock. While modal, the widget id
names the scrim and the surface takes `<id>/surface`, which is how a press
outside the surface is told apart from one inside it.

The surface keeps MD3's extra-large corner, 28 dp, in both forms: the export
names `rounded_corners` and leaves the radius to the shape scale.

## Deviations

**The month row keeps the docked colours.** The modal export carries no
menu-button tokens: Compose replaced the two menu buttons with a year button
and two arrow icon buttons, which take their colours from the button and
icon-button tables. This build keeps the docked row and its tokens rather than
paint half a row from a table that does not describe it.

**Outside-month days stay visible.** The docked export dims them; the modal
export has no such state, and Compose's modal grid leaves those cells blank.
This build dims them in both forms, so a month does not lose its edges when the
same calendar is shown in a dialog.

**The headline takes the engine's leading.** The token's 40 dp line height is
not applied: the engine lays a 32 dp line out at its default leading (44.8 dp
in the self-test). The divider is placed from the header's own bottom, so the
band still sums to 120 dp; only the glyph sits a few dp lower than Compose's.

**Year selection is not drawn.** The modal export's `year-selection` subtree is
excluded from its table. The range form is a picker of its own, with its own
table and page; see [Date Range Picker](./date-range-picker.md).

## See also

- [Design Tokens](../reference/tokens.md): how an excluded subtree keeps a
  table honest.
- [Date Range Picker](./date-range-picker.md): the two-date form, which borrows
  this page's day cell and modal surface.
- [Progress Indicators](./progress.md): the other control whose forms differ
  by a token axis rather than a second module.
