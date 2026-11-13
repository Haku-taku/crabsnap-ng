# Side Sheet

MD3's side sheet: a panel anchored to one side of the window, docked as part
of the page or floating over it with a scrim. This page covers the box, the
two forms, and what the export does not say.

## The box

| Part | Value | Token |
| --- | --- | --- |
| Container | 256 dp wide, full height | `sheet.side.docked.container.width` |
| Fill | `surface` (standard), `surfaceContainerLow` (modal) | `container.color` |
| Elevation | 0 (standard), 1 dp (modal) | `container.elevation` |
| Corner | 16 dp, inner edge (modal only) | `container.shape` (`rounded_corners`) |
| Divider | 1 dp `outline`, inner edge | `divider.color` |
| Headline | `title-large` 22/28, `onSurfaceVariant` | `headline.*` |
| Action label | `primary`, layer `primary` 0.08/0.1 | `action.*` |

The table is `md.comp.sheet.side` in `tokens/sheets/side-sheets.json`. Every
value hangs under the export's `docked` segment, so that segment names the
docked side sheet rather than an appearance; the two appearances are the
`standard` and `modal` forms. The `detached` form carries only a shape name
and is not drawn here.

The width is the export's 256 dp, fixed. The M3 spec gives a 400 dp maximum
instead of a width, so a 256 dp panel is one legal size: the token is what
this pack can read, and an app that needs a wider panel has no axis for it
yet.

The surface pads 24 dp along its inner edge and 24 dp at the top. The
headline sits 12 dp above the content the app supplies. When the sheet has
actions they form a row at the bottom, 16 dp above the surface's edge and
24 dp below it, left-aligned; the actions are text buttons, so their layer
and label colours come from `md.comp.button`, not from the `action` entries
in this table. The table's focus ring belongs to those controls too, which
is why the sheet itself is not a focus stop.

## The two forms

A standard sheet is page content: the control builds the surface alone, and
the app places it beside the main pane. It does not dim, does not catch
presses outside itself, and does not close on its own.

A modal sheet is a dialog with one edge anchor. A barrier covers the window
at 32% black (the same scrim the modal drawer and the bottom sheet use), and
the surface is aligned to its own edge, full height. A press on the barrier
reports `Change::Cancelled`; a press on an action reports
`Change::Selected(Some(id))`. The sheet slides in from its own edge and fades
the barrier in with it, both through `Presence`, so the app removes the sheet
and the engine plays the exit.

The divider is drawn for both forms on the inner edge, one device-independent
pixel wide, because the export carries its colour but not its width.

## Deviations

**The corner comes from the spec, not the export.** The export names
`rounded_corners` without a radius. The M3 side sheet page states a 16 dp
corner for the modal form, and MDC resolves the docked sheet's shape to
`corner.large`, also 16 dp; the standard form is square because it is docked
into the page.

**No detached form.** The detached sheet is the same panel inset 16 dp from
every edge. The export gives it a shape name only, and no sample needs it.

**No drag or flick to dismiss.** MDC's side sheet follows a horizontal drag
and a release velocity. This engine's drags report no velocity, and the modal
form already closes on the scrim; the panel stays put while dragged.

**The action row is 80 dp, not 72.** The spec's 72 dp counts a 32 dp action
inside 16 dp above and 24 dp below; this pack's text button is 40 dp tall, so
the row is 8 dp taller.

**No close or back affordance.** The spec's header may hold a close icon and
a back icon. Both are ordinary icon buttons the app can put in the content
it supplies; the control does not reserve a slot for them.

## See also

- [Bottom Sheet](./bottom-sheet.md): the other edge-anchored modal surface,
  and the scrim it shares with this one.
- [Icon Button](./icon-button.md): what a close affordance in the header is
  built from.
- [Design Tokens](../reference/tokens.md): how a segment that names no state
  is declared.
