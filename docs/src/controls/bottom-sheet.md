# Bottom Sheet

MD3's modal bottom sheet: the surface, the drag handle, the scrim behind it, and
what a drag settles on. The docked form is not drawn; the section below says
why.

## The box

| Part | Value | Token |
| --- | --- | --- |
| Container | `surfaceContainerLow` | `sheet.bottom.docked.container.color` |
| Corner | 28 dp, top only | `container.shape` (`rounded_corners`) |
| Elevation | 1 dp | `docked.modal.container.elevation` |
| Drag handle | 32 × 4 dp, `onSurfaceVariant` | `docked.drag-handle.*` |

The table is `md.comp.sheet.bottom.docked` in
`tokens/sheets/bottom-sheets.json`. Its `minimized` shape is excluded: that is
the box while the sheet is hidden, and a hidden sheet here is a node the app
removed, not a shape it paints. The export's focus ring sits at
`md.comp.sheet.bottom`, above the `docked` subtree, and belongs to the controls
inside the sheet, so this table does not carry it.

The handle sits in a 48 dp band: 22 dp above, 4 dp of its own, 22 dp below
(`DragHandleVerticalPadding` in `SheetDefaults.kt`). The surface takes at most
640 dp of width (`BottomSheetDefaults.SheetMaxWidth`) and sits against the
window's bottom edge, centred.

## The modal form

The sheet is a dialog with one anchor. A barrier covers the window at 32% black
(`ScrimTokens.ContainerOpacity`, the same dim the modal navigation drawer uses),
two children fill it: a grower and the surface, so the surface rests on the
bottom edge at its content's height. A press on the barrier reports
`Change::Cancelled`.

A drag on the surface follows the finger downward. On release the sheet settles
at the nearer of its two anchors — expanded at 0, hidden at the surface height —
so more than half the height travelled dismisses it; a shorter drag springs
back. While the sheet is dragged the spring is parked and the pointer's travel
is the position; the release re-seeds the spring where the finger left it, so
the sheet does not snap back for a frame.

The sheet enters by springing up from below its own height. The first frame
after the app declares it has no measured height yet, so that frame is drawn
transparent and the entrance starts on the second; at 60 fps that is one frame
the eye cannot see. The exit is a `Presence` slide of the last measured height,
which is why the app removes the sheet and lets the engine play it out.

## Deviations

**One expanded anchor, no partial detent.** Compose's sheet can rest at a
partially expanded height; the export carries no detent sizes, and the default
`rememberModalBottomSheetState` is used here as Hidden/Expanded only, so a
partially expanded form is not drawn.

**Settle is positional only.** Compose also closes on a fast downward fling
(125 dp/s). This engine's drag reports no release velocity, so a fast short
drag springs back.

**The standard form is not drawn.** The export's `standard` elevation is the
same 1 dp as the modal one, and a docked sheet is the same surface without a
scrim; an app that wants one builds it from a card. Only the modal form has a
gallery sample and a self-test.

## See also

- [Date Picker](./date-picker.md): the other modal surface, and the scrim it
  shares with the dialog.
- [Design Tokens](../reference/tokens.md): the scrim is a role plus an opacity,
  not a colour of its own.
