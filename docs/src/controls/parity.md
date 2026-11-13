# Parity Plan

Parity means a control's counterpart in another system is recognisably the
same control: the same states, the same geometry rules, the same motion
vocabulary, the same behaviour under pointer and keyboard. It does not mean
every vendor component becomes a module — only the ones that correspond
across systems.

## Four layers

| Layer | Holds | A gap that lives here |
| --- | --- | --- |
| Reference | The XAML or token export for one system | A value the export does not carry |
| Pack | Roles, metrics, motion, material | A role the system cannot express |
| Spec | Per-control envelopes and generated tables | A state the spec table has no column for |
| Control | The module that builds the tree | A behaviour nobody implemented |

Put a fix in the lowest layer that can express it. Hardcoding a colour
in a control because the pack lacks a role makes every later control
repeat the literal.

## Gap categories

- **Engine capability.** The control could be written, but the engine
  exposes nothing to build it on. Recorded once in
  [Engine Gaps](../reference/engine-gaps.md); every waiting control points
  there instead of growing its own workaround.
- **Control omission.** The capability exists and the module is missing.
  This is ordinary work.
- **Advanced variant.** A mode of an existing control (top navigation,
  editable combo). Land it as a state of the same control, not as a second
  module. A form whose body is a different tree is a control of its own
  instead: the date range picker scrolls a month list where the date picker
  draws one fixed grid.
- **Reference deviation.** The vendor does something the pack or the engine
  cannot reproduce. Name the deviation and its reason on the
  control's page, so the next reader does not "fix" it.
- **Out of scope.** A component with no counterpart in the other systems.
  Leave it out and say so once, rather than carrying it in the plan forever.

## Remaining work, grouped

**Fluent.** `ListView`/`ListViewItem` (blocked on a template), the
icon-and-label button lockup, text-box validation and multiline states,
combo-box transitions and editing, `NavigationView` top mode, menu-flyout
sub-items and theme shadow, and the info bar's action button.

**MD3.** The wavy progress ring, which waits on a faithful port of Compose's
rounded star polygon (`RoundedPolygon` corner rounding and smoothing); the
engine can stroke the sampled path already ([Progress Indicators](./progress.md)).

**Glass.** Menu morph and sub-menus, card glow and width limits, popover
arrow and scale, list-tile editing states, text-field multiline, and toast
auto-dismiss. Haptics and accessibility remain unmodelled.

**Engine, blocking controls.** Parent constraints
([Engine Gaps](../reference/engine-gaps.md)), which keeps the glass tab bar on a
host-supplied width; text tracking, which keeps Cupertino labels one notch
loose; multiline field height; and non-uniform transform for the lens and morph.

## Closing one item

1. Decide the layer, then change that layer once.
2. If an engine gap blocks it, record the gap and the workaround; do not
   silently recompute geometry in the control and call it done.
3. Add the state as a mode of the existing control; a second module
   duplicates every future fix.
4. Extend the gallery sample and the self-test group in the same change;
   parity without evidence is a claim.
5. Update the completion row.

## Corrections

**The ledger counted engine gaps that had already closed.** Focus visuals
outside the box, tri-state `Toggled`, and declared enter/exit transitions
were all listed as missing while the engine had them, so work was planned
against a state that no longer existed. Engine gaps are now verified against
the code before a control is blocked on one.

**Coverage totals drifted upward without new modules.** A mapping table
marked components done as their first variant landed, and its totals stopped
matching the rows. Status is counted from the completion table and the
gallery, not from a separate summary number.

## See also

- [Completion Status](./completion.md): the evidence each item needs.
- [Control Sets](./control-sets.md): which family owns a counterpart.
- [Engine Gaps](../reference/engine-gaps.md): the capabilities that unblock
  several items.
