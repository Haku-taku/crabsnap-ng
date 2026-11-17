# Control Sets

The workspace ships three control sets and one set of shared primitives. A
control belongs to exactly one family, and that family's specification
decides what the control may look like. The first question about a missing
control is therefore not "how do I draw it" but "which family defines it, and
does the shared layer already have the mechanics".

## The three sets

| Set | Crate | Authority | Scope |
| --- | --- | --- | --- |
| Material 3 | `ui-widgets-md3` | The official token export plus Flutter `material` | Navigation, sheets, pickers, search, chips, progress, text fields |
| Fluent / WinUI 3 | `ui-widgets-fluent` | A pinned microsoft-ui-xaml corpus and its `ThemeResource` tables | Buttons, inputs, dialogs, flyouts, navigation, info bars, tooltips |
| Liquid Glass | `ui-widgets-glass` | Apple HIG plus the two local glass implementations | Frosted surfaces, sheets, action sheets, toasts, Cupertino-style controls |

The authoritative module list is the `pub use` block in each crate's
`lib.rs`. It is shorter than the vendor's component catalogue: parity is
scoped to the controls that correspond across systems, not to every component
a vendor ships.

## Shared primitives

`ui-widgets` holds `Button`, `List`, `Card`, `Panel`, `Scroll`, `Icon`,
`Text` and the `Styled` adapter. These are stateless and contain no
colour, duration or radius literal; they read the common plane of the
pack they are given.

Use them before writing a fourth button:

- A family control that only re-skins a shared primitive wraps it and
  supplies tokens.
- Mechanics that any family would need (hit areas, scroll, focus rings,
  popups) belong in the shared layer or the engine, not copied into a kit.
- A family-specific control keeps its own module and spec table; do not
  add family branches to a shared control.

`Styled` completes a control's disabled look. A control that paints its own disabled colours
reports `handles_disabled` and is left alone; every other control is faded as one group to
38%. That flag is folded from the declaration the build just produced plus the inertness its
containers declared while building it, never from the committed routing, which describes the
previous frame. A caller-drawn declaration placed through `Pressable` opts in with
`painted_disabled()` when it resolves its own disabled colours.

## What each crate also contains

Structural modules sit beside the controls and are not controls themselves:
`scaffold`, `spec` (generated token tables), `pack`/`style` (the family's
style snapshot), `layer`/`material`/`paint` (shared visual recipes), and
small helpers such as glyph sets or menu styles. A new file in these
directories is not a new control; it does not need a gallery page.

## Gallery and self-test

`ui-gallery` has 45 pages, including one page per system (`MD3`, `Fluent`,
`Glass`) that shows that set's token vocabulary and a Markdown page that places
a parsed document. Every control gets a
gallery sample with its meaningful variants at build time, and a self-test
group that drives it with synthetic input. A control with no page cannot be
checked; a control with no self-test group is not verified.

## Adding a control

1. Confirm the family's specification defines the control and read its states.
2. Add or regenerate the token table ([Design Tokens](../reference/tokens.md)).
3. Write the module in the family crate: a `Params` struct, a
   `decl`/`resolve` build, and a `Widget` impl. Read the checklist in
   [UI Checklist](./checklist.md) first.
4. Add unit tests for geometry and state reporting, a gallery page, and a
   self-test group.
5. Record the status in the completion table.

## Traps

| Change | Symptom |
| --- | --- |
| Add a control to a family the spec does not define | It drifts from the vendor's geometry and no reference settles arguments |
| Copy a shared primitive into a kit to tweak one colour | Two hit areas, two focus behaviours, one bug fixed twice |
| Put family branches in a shared control | The shared crate grows a dependency on every kit |
| Count structural modules as controls | Status tables and parity plans fill with files nobody can use |

## See also

- [Completion Status](./completion.md): what "done" means per control.
- [Parity Plan](./parity.md): which controls remain.
- [Architecture](../architecture.md): why the sets are separate crates.
