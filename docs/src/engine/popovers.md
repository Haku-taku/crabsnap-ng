# Popovers and Tooltips

A popover is a declaration subtree that floats above content, escapes its
ancestors' clips, takes the pointer above anything declared after it, and owns
the keyboard while open. The engine places it; the control supplies the
content.

This is the same split as selection and the context menu: the engine holds the
facts that cross the tree (where it goes, who gets the click, when it closes),
the control holds the content.

## Contract

A popover root carries an `OverlayPlacement`:

```rust
OverlayPlacement {
    anchor: Anchor,          // Node { id, side, align } or Point
    offset: f32,             // gap from the anchor
    flip: bool,              // flip to the opposite side before clamping
    match_anchor_width: bool,// combos and menus: panel width follows the anchor
    presentation: Presentation, // Parent, or a surface of its own
}
```

`Decl::overlay(..)` also sets `Position::Absolute`. Placement runs after
in-flow layout, using the anchor's resolved rectangle:

1. Resolve the anchor (`Node` id or a point supplied by the app).
2. Place on the requested side with the requested gap.
3. If it overflows, flip to the opposite side, then clamp into the viewport.
   A `Presentation::Platform` panel skips both: the platform places it.
4. Reset the clip chain at the overlay, so an ancestor's `Clip::Hidden` cannot
   cut it.

By default the panel is painted into its parent's surface, in the same paint
list and the same hit index as the content it covers. A panel that must leave
the window asks for a surface of its own.

## Where a panel is painted

| `Presentation` | Surface | Shape |
| --- | --- | --- |
| `Parent` | the parent's | A panel that fits: an in-window menu, a combo list |
| `Platform` | one the platform places | A panel that may leave the window |

`Presentation::Parent` is the default. `Presentation::Platform` hands the
subtree to a surface of its own; the engine still lays it out in the parent
tree, paints only that subtree into the new surface, and routes its input back
into the parent tree. The application declares the same `Decl::overlay` either
way and never names a surface.

A platform whose popup support is missing falls back to `Parent`, so a panel
never disappears because a surface could not be opened.

The engine's table of open platform surfaces mirrors the tree; it is not a
second source of truth. A binding exists only while its subtree does, so a
dismissed panel stays dismissed by being undeclared. The frame that first
declares a panel is painted with it held back, because the host's answer comes
after that frame; see [the binding table](./window.md#binding-table).

The platform decides the final position against its own output. The engine does
not clamp a platform panel into the parent's viewport: a menu near the bottom
edge of a short window must be allowed past it. See
[Presentation surfaces](./window.md#presentation-surfaces).

A `Point` anchor has zero size, so `Center` centres the panel on that point and
`Start`/`End` put one of its edges there. That is how a panel hangs under a
rectangle that has no node of its own — the app passes the rectangle's edge
midpoint and keeps no anchor node.

`Decl::shadow` (and `Container::shadow`) attaches the elevation to the box shape
itself, so a surface can float without a widget owning its shadow. Only the box
carries it: a node with no background casts nothing.

## Ordering

Overlays and modals are pushed to the end of the walk and of the paint order.
Hit order follows the walk, so an overlay takes the pointer before content
declared after it, regardless of declaration order.

> **Wrong path:** teaching only the hit walk about overlays. Paint still
> follows declaration order, and a panel gets covered by a sibling declared
> later. Hit and paint must share one predicate.

## Lifecycle

- **Open and close are application state.** The trigger reports `Activated`;
  the app flips its `Option<Id>` and declares the popover next frame. The
  engine keeps no list of open popovers.
- **An outside press asks the open panel to close.** The engine knows the
  panel rectangle and the press position, so it emits a payload-free
  `Change::Dismissed` to the panel's root. The app decides whether to obey.
  A platform panel can also be dismissed by its window server — a Wayland
  `popup_done`, a Win32 capture changing. The host reports that with
  `AppLoop::dismiss_panel`, which fills the same `dismissed` slot an outside
  press does, so `cx.dismissed()` and the `Change::Dismissed` a control derives
  from it answer either cause.
- **While open, the panel owns the keyboard.** `popup_root` scopes focus;
  `settle_focus` takes focus on open and hands it back on close; navigation
  keys loop inside the panel, with `Home`/`End`.
- **A panel may open another panel.** The engine keys everything on the topmost
  one — `walk()` answers it last, so a press outside it lights it down, the
  keyboard scope is its subtree, and `Home`/`End` loop inside it. Closing it
  hands focus back to the panel that opened it, or to the page when none is
  left. A second panel is a float of its own even when the app declares it
  inside the first: the walk defers a nested float the way paint already did.
  When both are platform panels, the inner surface is owned by the outer
  panel's surface, so the grab stays with the topmost one.

## Tooltips

A tooltip uses the same mechanics with `Anchor::Node`, `Sense::NONE`, no focus,
and a small offset. The hover delay is application state: the trigger watches
`cx.age()` across frames and opens after its own threshold. Delay length is a
product decision, not an engine fact.

## Menus are the exception

The context menu is engine-drawn outside the tree (`AppLoop::menu`,
`paint_menu`): it has no focusable content, and design systems only change its
`MenuStyle`. It does own keys for as long as it is open: a row that carries a
chord picks itself, and `Escape` closes the menu
([Shortcuts](./shortcuts.md#a-menu-rows-chord)). Do not extend that path for
rich content; use a popover.

The panel travels half its height toward the pointer over 167 ms when it opens, and the rows are
hit-tested where they are *drawn* at that instant (`OpenMenu::row_at`): a click during the
entrance selects the row under the pointer on screen, not the row at its resting place. A probe
that right-clicks and presses at once therefore picks a neighbour; anything scripted has to run
frames until the frame reports nothing animating before it presses.

## Corrections

**A popover was covered by a sibling declared after it.** `walk()` pushed
overlays to the end, so hit testing was right, but `paint_tree` only pushed
modals: painting fell back to declaration order and a later sibling drew over
the panel (engine-gaps #45). Both paths now use the same predicate; pinned by
`an_overlay_paints_over_content_declared_after_it`.

**A panel nested inside another was not deferred by the walk.** `walk()` deferred floats at
the top level, but `walk_subtree` — the path a deferred subtree takes — appended its children
in plain order, so a panel declared inside another came before the siblings that followed it.
Paint defers every float, so the nested panel covered a later sibling while the hit index,
built from the walk, still gave that sibling the press. The subtree walk now applies the same
predicate. Caught by `a_panel_nested_in_another_floats_last_in_the_walk` in
`crates/ui-platform/src/view/tests.rs`; the same module pins that a press inside the outer
panel dismisses only the inner one, that the inner one owns the keyboard, and that closing it
hands focus back to the outer one.

**Anchoring was tied to the pointer.** The first design anchored tooltips to
the pointer, which made layout depend on input. `Anchor::Point` takes a
position from the application instead, so layout stays a function of
declarations.

**An anchored panel was placed at its branch's origin.** `place_overlay` read
`Anchor::Node`'s rectangle before `accumulate_offsets` ran, so it knew the
anchor's flow position but not its paint offset: `.offset()` is folded in
afterwards, and descendants are usually placed by it. The screenshot app's
toolbar places every member on offsets, and each button's tooltip landed at
the surface's top-left, level with the pill instead of above the button it
named. Nobody had noticed because every existing anchor was a node the layout
flowed into place, where the two spaces coincide. Placement now lifts the
anchor and the viewport into the surface's own space by the offsets they will
carry (`pending_offset`, the same rule `accumulate_offsets` applies) and puts
the panel back into the space its own branch will add. Caught by
`an_overlay_declared_beside_the_bar_anchors_to_a_button_inside_it`: a panel
declared beside an offset shell, anchored to a button inside it, must land
under that button.

**A floating region was assumed to belong in its parent's surface.** The first
contract said a popover never needs `xdg_popup`, because the engine-drawn menu
had already shown that a floating region paints correctly inside one surface.
The menu proves only that the panel can be *painted* there: a combo panel in a
short window, a tooltip under a trigger at the output's bottom edge, and a menu
taller than its window have nowhere to draw, and no in-surface option exists
for them. `OverlayPlacement::presentation` now selects `Parent` or `Platform`,
and the engine opens, positions and closes the platform surface itself.

`a_panel_gets_a_surface_of_its_own_or_falls_back_to_its_parent`,
`a_closed_panel_is_reported_as_dismissed_and_not_reopened` and
`a_panel_inside_a_panel_is_placed_against_the_outer_surface` in
`crates/ui-platform/tests/host_loop.rs` pin the binding; `place_popup` is pinned
in `crates/ui-platform/src/window.rs`. What none of them shows is the panel on
screen: the gallery page whose window is 200 px tall with a 400 px menu is
still unwritten, and the Wayland surface has not been run under a compositor.

## Traps

| Change | Symptom |
| --- | --- |
| Push overlays only in the hit walk | The panel is painted under a later sibling |
| Resolve the anchor before layout | A stale rectangle; the panel points at the previous frame's trigger |
| Read an anchor's `rect` as its painted position | A panel anchored inside an offset branch lands at the branch's origin |
| Let an ancestor clip apply to the overlay | A popover is cut off inside a scrolled list |
| Keep the open set in the engine | Two sources of truth; a dismissed panel reappears |
| Defer only top-level floats in the walk | A panel nested in another paints above a sibling, and the press lands on the sibling |
| Keep every panel in its parent's surface | A panel near the window's edge has nowhere to draw |
| Clamp a platform panel into the parent's viewport | A menu cannot leave a window that straddles two outputs |
| Resize a platform panel to fit the output | The platform cuts content the engine laid out |

## Status

Implemented: placement with flip and clamp, clip escape, paint and hit
priority, dismissal, keyboard scope, tooltips, combo panels, and a panel
opened from inside another panel (dismissal, keyboard scope and focus return
all key on the topmost one).

`Presentation::Platform`: the engine opens, positions and closes a surface for a
panel, and a nested panel hangs off the outer popup. On Wayland that is
`xdg_popup` with a grab; the GPU presenter shows the panel, and the CPU
presenter refuses so the panel paints in its parent. On Win32 that is an owned
non-activating `WS_POPUP` holding the mouse capture. Compile-verified only —
nothing has run either path. See
[Presentation surfaces](./window.md#presentation-surfaces).

Not implemented: `Anchor::Point` has no production user yet. Arrow tails and
other decorations belong to the style layer. Exit animation across a platform
surface: a presenter closes with its subtree.

## See also

- [Layout](./layout.md): positioned boxes and the containing block.
- [Pointer Model](./pointer-model.md): who a press belongs to.
- [Transitions](./transitions.md): the 167 ms entrance a tooltip uses.
