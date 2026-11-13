# UI Checklist

Run this list before calling a control finished. Every item has caught a real
defect; none is ceremonial.

## Structure

- [ ] The control is stateless. Nothing survives between frames except what
      `Params` carries.
- [ ] A live control is placed through `Cx::element` (or a container), never by
      stuffing its `decl` into a `Decl`; otherwise `change()` is silently never
      called.
- [ ] The root node carries the control's own id. Internal shells do not reuse
      an app action id, and a parent and child never share an id.
- [ ] Part ids name roles (`"label"`, `"state-layer"`), not declaration indices.
- [ ] `Id::with` keys are the same type everywhere: an `i32` and a `usize`
      produce different ids for the same value.
- [ ] A grouped control reads its position from the parent; the parent computes
      corners. A child never infers its siblings.
- [ ] The control sits in the family scaffold (nav, header, main, overlay)
      rather than drawing its own page chrome.

## Values

- [ ] Colours, radii, paddings and durations come from the pack or the
      generated spec table. No literal in a shared control; a family literal
      must carry its reference and token name.
- [ ] A missing role goes through `colour_or`/`metric_or` with a stated
      fallback, not a silent default.
- [ ] User-visible strings go through `Cx::t`/`t_args`; no inline text,
      including labels on generated rows.
- [ ] Time comes from `Cx::now`; animation goes through the node's
      animate/spring channels.

## Behaviour

- [ ] Every interactive part declares the `Sense` bits it uses, and an inner
      clickable part does not swallow the control's own press.
- [ ] Keyboard activation reaches the same state change as a click; the control
      does not rely on a press point that a key does not provide.
- [ ] Disabled state blocks both pointer and keyboard activation, and still
      paints.
- [ ] A focusable part is a real focus stop even when it paints nothing at rest
      (`.focus_stop(true)`), or the keyboard skips it.
- [ ] Opening, closing and dismissal use the declared enter/exit path; a
      control that appears instantly has no `Presence`.

## Evidence

- [ ] Unit tests cover geometry and reported changes, not only construction.
- [ ] The gallery page shows the meaningful variants at build time (no scripted
      clicks needed).
- [ ] A self-test group drives the control and asserts geometry or ink.
- [ ] A screenshot was reviewed in a nested session for the states the self-test
      cannot judge.
- [ ] The completion row is updated in the same change.

## Traps

| Shortcut | What breaks |
| --- | --- |
| Build the control in `view()` and wrap it in a plain `Decl` | Clicks do nothing; `change()` never runs |
| Draw the state layer as a child above the label | The label is dimmed or covered; state layers paint below content |
| Hardcode a padding from the reference screenshot | It disagrees with the token table the next control uses |
| Reuse the parent's id for the visual shell | The tree merges the two nodes; one keeps the other's interaction state |
| Rely on the pointer press to select a row for keyboard users | Enter/Space reach no item |
| Skip the gallery page because the self-test passes | Assertions cannot see overlap, clipping, or wrong contrast |

## See also

- [Completion Status](./completion.md): the evidence a control needs.
- [Control Sets](./control-sets.md): where the control belongs.
- [Graphical Tests](../development/graphical-tests.md): the nested-session
  recipe.
