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
   A panel presented by the platform skips both: the platform places it when
   the window has a surface for it, and both rules apply again when it has none.
4. Reset the clip chain at the overlay, so an ancestor's `Clip::Hidden` cannot
   cut it.

By default the panel is painted into its parent's surface, in the same paint
list and the same hit index as the content it covers. A panel that must leave
the window asks for a surface of its own.

## Where a panel is painted

| `Presentation` | Surface | Shape |
| --- | --- | --- |
| `Parent` | the parent's | A panel that fits: an in-window menu, a combo list |
| `Platform` | a popup the platform places | A panel that may leave the window: a menu, a flyout |
| `Window` | a window the platform holds | A panel that reports: properties, a failure list |

`Presentation::Parent` is the default. `Platform` and `Window` hand the
subtree to a surface of its own; the engine still lays it out in the parent
tree, paints only that subtree into the new surface, and routes its input back
into the parent tree. The application declares the same `Decl::overlay` either
way and never names a surface.

The two ask for different things. A `Platform` panel is a popup: the platform
holds it where the tree says, with the grab that makes a press outside dismiss
it. A `Window` panel is a window: it carries the platform's frame and takes the
keyboard like any window, so the panel's rect supplies only its client size and
where a modeless window stands is the window manager's answer.
`Decl::window_title` names it in its title bar and the task switcher; a popup
never uses that name. Its size is its content's and nobody else's: the panel's
rect is the client size, a change of content resizes the window, and the user is
offered no resize handle, because a window dragged larger than the panel would
show a blank strip beside it. The panel's own drawing therefore has to fill its
rect; a card, a border or a shadow inside a framed window would draw a second
frame.

A surface stands on whole pixels: the platform places its window on them, and glyph tiles are
authored on the tree's grid. The panel's rect is therefore grown out to that grid — the origin
down to a grid line and the far edge up to the next — so the window holds every pixel the panel
painted. A panel anchored at a pointer's pixel centre sits between whole logical units, and its
text must not; its size is also content-authored and fractional (text advances are not rounded),
so rounding the extent in either direction would shave the panel's border.

A panel never disappears because a surface could not be opened. A host that
presents panels says so when it builds the loop
(`AppLoop::set_panel_surfaces`); until it does, the request is read as `Parent`,
the panel is recorded for nobody, and the window places and paints it itself.
A host that then refuses one surface is answered the same way, per panel, and
a refused `Window` is first offered as a popup: a backend without window
surfaces still shows a reading panel, just without a frame of its own.

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
  `popup_done`, a Win32 capture the system released. The host reports that with
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
product decision, not an engine fact. A tooltip usually keeps `Presentation::Parent`:
a popup surface would take the pointer capture from the node it describes and end
the hover that keeps it up. The picker's name tooltip is the working example — it
appears after 600 ms over a row whose label `Text::truncated_in` reports cut.

## The menu is a panel

The context menu is not a second painter. `AppLoop` declares it as one more child of the app's
root — `Decl::overlay { anchor, presentation: Platform }` — so it is placed, clipped, painted and
hit-tested by the ordinary machinery: a level is a panel with the id `Menu::panel_id(depth)`, a row
is a node with `Menu::row_id(depth, index)`, and a flyout is the next panel declared inside the one
that opened it. The engine keeps the model (items, hover, the open child) because the keys are its
own; everything visual comes from `MenuStyle` and every pixel from the declared rows. The panel
carries no `Sense`: focus stays where the app put it, the menu's keys are resolved in
`menu_event` before the general chain ([Shortcuts](./shortcuts.md#a-menu-rows-chord)), and its light
dismiss is judged against the open levels' rectangles rather than `popup_root`.

The panel travels half its height up from below the pointer over 167 ms when it opens. A click
during the entrance lands on the row under the pointer *then*, not the row at its resting place;
anything scripted has to run frames until the entrance lands before it presses.

A header that opens a flyout draws `SUBMENU_MARK` (`›`) at its trailing edge, and both
`level_width` and `menu_row` reserve the mark's width, so a long label cannot push it out of the
panel. The cascade itself was never waiting on the renderer: `Menu::open_row` is reached by a
hover, a click, `Enter` and `Right`, and a child level is declared inside its parent's panel. A
child level records the row that heads it (`Menu::header`, set by `open_child`), and its panel
anchors at `Menu::row_id(depth - 1, header)` on `Side::Right`; a flyout with no header would fall
back to the root's point, which is only right for the root level.

A row marked `compact` stands in a *strip* instead of on a line of its own, and a run of
consecutive compact rows is declared as one `Row` of equal cells, each an icon over its label.
Every cell keeps the level's `Menu::row_id`, so hover, the pointer press, `Menu::enter` and a row's
chord reach it through the same path as any row; what a cell gives up is the trailing chord text
and a disabled row's reason line. Cells share the panel's width equally, which takes
`Grow::Split(1)`: `Grow::Yes` grows each cell from its own label's width, so "Cut" and "Rename"
would end up different widths. `level_width` reserves the widest cell times the run's length, and
`level_height` counts a run as one cell row, which is also the height the entrance starts from.
The strip has no special keyboard mode — `Up`/`Down` walk into it and across it like any rows —
and an app that wants Win11's shape puts the run first, above a divider.

## Corrections

**A panel anchored between pixels painted its text between texels.** A pointer names a pixel
by its centre, so a menu anchored at a press sits at a whole number plus a half logical unit.
The surface was painted by cutting the panel out of the tree with a translation of exactly its
laid-out origin, which shapes follow — their rectangles are transformed as they are pushed.
Glyphs are not: their tiles had already been rasterized at the tree's absolute phase, and a
half-pixel translation put every tile between two texels, so each label was sampled across its
neighbours and read soft. The same menu at an integer anchor was crisp, which is why the blur
tracked position; the panel's entrance animation also slides by fractions, which is why it was
first blamed on the animation and on the surface being resampled. The origin is now snapped to
the grid glyphs are authored on, in the paint rebase and in the popup spec the platform places
the window and shifts the panel's input by, so all three name the same pixel. The extent needed
the same treatment: a panel's width comes from its content, and text advances are not rounded, so
the window's client rounded the far edge down and shaved a sliver off the panel's border. The
rect is now grown out to the grid — origin down, far edge up — and the window receives that rect.
Pinned by `a_menu_anchored_between_pixels_gets_a_surface_on_the_pixel_grid` in
`crates/ui-platform/src/view/tests.rs`; before the origin snap it failed with `a glyph at 17.5
device px is between texels`, and with the origin rounded to the nearer grid line instead of
taken down it failed with `a prim at Rect { origin: Point { x: -0.5, y: -0.5 }, .. } falls off
the .. surface`.

**A flyout's surface stood somewhere the tree did not name.** The menu's flyout is a
platform panel placed beside its row, but the Win32 backend ran the constraint against the
monitor's work area a second time. A flyout with more rows than there was room for was slid
up to fit, and its window painted the panel at that new place while the engine kept the row
beside the menu. A popup holds the mouse capture, so the panel hears every move and press
outside its window; shifted by the panel's tree origin, those became positions far from
what the user saw. A press near the flyout's edge landed outside every level and closed the
menu, and a move back onto the parent row left the flyout open with no hover anywhere. The
tree is now what constrains a platform panel, against the output its host names
(`AppLoop::set_output_bounds`), so the surface stands where the rect says; see
[Window and Presentation](./window.md#corrections). Pinned by
`a_platform_panel_unfolds_inside_the_output`.

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
`crates/ui-platform/tests/host_loop.rs` pin the binding; the placement rule is
pinned in `crates/ui-platform/src/window.rs`, and
`a_platform_panel_unfolds_inside_the_output` pins that the layout applies it to
the output the host named. What none of them shows is the panel on
screen: the gallery page whose window is 200 px tall with a 400 px menu is
still unwritten, and the Wayland surface has not been run under a compositor.

**A `Platform` panel with no host to present it was painted nowhere.** The engine
held a declared platform panel back for the frame the host decides in, and asked
for another frame while it waited. Under a bare `AppLoop` — the harness of every
unit test, and any embedding that presents the tree itself — no host ever
decided, so the panel was never painted and the loop never stopped demanding
frames. The window's capability is the engine's input now: a host says
`set_panel_surfaces(true)` when it builds the loop, and until then
`place_overlay` reads the request as `Parent`, which places, paints and
hit-tests the panel in the window from its first frame. The last section of
`a_platform_panel_paints_into_its_own_list_and_not_the_parent` in
`crates/ui-platform/src/view/tests.rs` is the no-host branch.

**The context menu was painted outside the tree.** It kept its own rectangle, its own row
offsets, its own hit test at the drawn position, a `DrawList` appended after the tree's, and a
hand-written addition to the surface's input region. That is why it could not use a platform
popup, why a flyout could not be a nested surface, and why every look change had to be taught
twice. The menu is now a panel of the ordinary kind: a level is `Decl::overlay` with
`Presentation::Platform`, a row is a node, a flyout is the next panel declared inside the one
that opened it, and a row's pointer click reaches `pick_menu_item` through the same
`Change::Activated` a control's click uses. The engine keeps only the model — items, hover, the
open child — because the keys are resolved before the general chain, and the light dismiss is
judged against the open levels' rectangles instead of `popup_root`, since a panel that took the
focusable sense would move the page's focus into a panel with no stops.
`a_menu_reaching_past_its_node_still_takes_the_press_there` and
`a_press_outside_the_menu_dismisses_it_and_reaches_nothing` in
`crates/ui-platform/src/view/tests.rs` pin the branches the old painter answered by hand; the
keyboard and accelerator tests did not change, because that path was already the model's.

**A menu row's text measured nothing.** `menu_text` built its node with a plain container's
size class, and a nowrap line is capped by that class's width hint: the hint was zero, so every
label, chord, disabled reason and check mark laid out at zero width. The label column then took
the row's whole width and the chord was placed against the trailing edge with nothing left of it
to draw — the menu showed a blank gap before a clipped shortcut, which is how the picker's row
menu read. Text nodes now declare the size class a line of text has (a hug with an unbounded
width and a line-height floor), the chord carries `shortcut_gap` as its own leading margin, and
`level_width` includes the row's side insets, which the row's margins had been taking out of the
label's room. The panel's width is also measured as each frame declares it rather than once at
`Menu::open`, so a font that loads while the menu is up cannot leave the panel sized to the old
metrics. Pinned by `a_menu_row_measures_its_label_and_chord` in
`crates/ui-platform/src/view/tests.rs`.

**A submenu header drew no sign that it opened a cascade.** The model opened a flyout on hover,
`Enter`, `Right` and a click from the day the menu became a panel, but the row declared only its
label, mark column and chord, so a header read as a command that did nothing — the picker's shell
merge stacked its levels behind rows with no visible difference from "Properties". The header now
declares `SUBMENU_MARK` at its trailing edge, and `level_width` and the row's label room both
subtract the mark, so a long label cannot clip it. Pinned by
`a_submenu_header_keeps_its_cascade_mark_inside_the_panel` in `crates/ui-platform/src/view/tests.rs`:
the label must end before the mark's own room.

**A flyout unfolded from the menu's own click point.** `menu_level_decl` read `level.child_of` to
find the row a flyout hangs off, but `level` there is the flyout itself, whose `child_of` only says
whether *its* child is open; the arm never matched, and the flyout anchored at
`Anchor::Point(level.at)`, the root's click. Every cascade landed with its top-left on the root
panel's top-left, covering the level it came from, which is what the picker's shell merge showed.
The row that heads a level is now the level's own `header`, recorded by `Menu::open_child` and
read once by the renderer, so the child panel anchors at `Menu::row_id(depth - 1, header)`.
Pinned by `a_flyout_hangs_off_the_row_that_heads_it` in `crates/ui-platform/src/view/tests.rs`:
the flyout must start beside its header row and level with it.

**The quick-action strip's cells came out unequal.** The first strip declared every cell with
`Grow::Yes`, which grows a child from its own content, so the cell holding "Cut" started narrower
than the one holding "Rename" and both grew by the same slack. Equal shares are a zero-basis grow,
`Grow::Split(1)`, which is CSS `flex: 1 1 0`; pinned by
`a_menu_strip_lays_its_quick_actions_side_by_side` in `crates/ui-platform/src/view/tests.rs`, whose
width assertion is what failed on the first run and what the split now holds.

**One group's shared pointer was taken from the first panel that had one.** A window and the
popups it hangs build one frame from one pointer, and `group_pointer` asked the panels in the
order they opened and took the first sample. An open flyout takes the mouse capture from the
panel it hangs off, so the outer panel stops hearing moves and keeps the sample from the moment
the flyout opened; asking it first froze the shared pointer there. The parent's other rows never
hovered, and the flyout — still "under" that stale point — never closed. The panels are asked
deepest-first, which is the surface that holds the grab: only it hears `WM_MOUSEMOVE`, and its
sample is lifted into the tree's coordinates (`PopupPresenter::pointer`). Pinned by
`the_deeper_panel_owns_the_shared_pointer` in `crates/ui-platform/tests/host_loop.rs`, which
gives two nested panels different pointers and reads the frame's; the picker's row menu on
Windows is the case that exposed it — the first header hovered opened its flyout, and moving
back over the parent left the flyout up with no row highlighted.

**The context menu kept its last highlight after the pointer left.** `Menu::pointer_over(None)`
returned without touching the hover, because the model also carries the keyboard's moves and
an open flyout needs its header lit. That kept a pointer's row lit too: the menu stood with
no pointer over it and one row washed, so the next click looked like it would run that row.
The departure now clears the row it left behind, while a level the pointer never entered
keeps the keyboard's row and a level with an open flyout keeps its header.
`a_pointer_leaving_a_level_drops_its_hover_but_never_a_keyboards` pins the three cases.

**A shell menu read that answered after the wait was thrown away.** The picker held the row menu
back 200 ms for the shell's rows, then opened without them and dropped the ticket, so an answer
arriving later was ignored and only the next press merged anything. The slow read is the first
one: a file type no handler has loaded yet pays for the shell's extension setup. The read now
stays pending after the menu opens, and its rows replace the open menu's rows in place —
`Cx::update_menu` carries a `MenuUpdate` the frame applies through `Menu::replace_items`, which
keeps the panel, its hover rules and its entrance. The late answer reaches a build because the
Win32 worker wakes the owner window with `WM_NULL` when its job finishes, and `take_rows` answers
only the ticket the read asked about. Pinned by
`a_shell_read_that_answers_late_joins_the_open_menu` in `crates/ui-picker/src/picker.rs`: a
read that is silent through the wait opens the menu without a "Shell Menu" row, and the answer
that follows must hand the open menu a `MenuUpdate` that carries it.

**A menu closed the moment its own panel opened.** The Win32 popup read every
`WM_CAPTURECHANGED` whose gainer was not itself or one of its own popups as a takeover and
closed. A press delivered to a background thread leaves that thread's capture in the system's
hands: the gesture that asked for it ends it, and the change arrives with a null `lParam` — no
window took the pointer, so nobody is dismissing anything. The log showed exactly that on a
right-click on a pin: the panel opened at (1281,721)-(1400,957) holding the grab, and a change
with no gainer followed in the same turn, so the menu was closed before it finished its
entrance. A null gainer (and an own popup, which is the nested case already handled) now counts
as kept. Separately, the menu's panels are declared by the engine, not the app, so
`AppLoop::dismiss_panel` reported the dismissal under an id no app declares and the menu could
stay up over a level whose surface was gone; the call now hands the id to `menu_dismissed` too,
which closes the taken-back level. The
engine half is pinned by `a_menu_panel_the_platform_took_back_closes_the_menu` in
`crates/ui-platform/src/view_menu_test.rs`; the null-gainer rule needs a real window and a held
grab, so it rests on this record and the manual check that a right-click's menu stays up.

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
| Show a reading panel as a popup | A `WS_EX_NOACTIVATE` popup takes no keyboard, and a press outside dismisses a report the user still needs |
| Make a window panel block its dialog | A report the user reads while working must not stop the work it describes |
| Hold a window panel at its tree rect | The window manager owns where a modeless window stands; forcing the rect fights the user's move |
| Paint a context menu outside the tree | Its rows need a hand-written hit test, pick path and input region, and no popup surface can hold it |
| Give a nowrap text node a container's size class | The zero width hint caps it at nothing; siblings are pushed out and clipped |
| Measure a panel's width once, when it opens | A font loaded while it is up leaves every row against the old metrics |
| Give a strip cell `Grow::Yes` | Cells grow from their own labels, so the quick-action row is uneven |
| Draw a submenu header without its mark | The cascade reads as a command that does nothing |
| Read a level's own `child_of` as its anchor | The flyout unfolds from the root's click and covers the level |
| Take the group's pointer from the first panel that has one | The panel under the grab keeps the pre-flyout sample; hover freezes and the flyout stays open |
| Drop a menu read that answers after its wait | The first press on a slow file type shows no merged rows; the second one does |

## Status

Implemented: placement with flip and clamp, clip escape, paint and hit
priority, dismissal, keyboard scope, tooltips, combo panels, and a panel
opened from inside another panel (dismissal, keyboard scope and focus return
all key on the topmost one). The engine's context menu is one of these panels:
its root level anchors to `Anchor::Point` at the click, each flyout is a
nested panel anchored to the row that heads it, cascades nest to any depth, and
a run of `compact` rows is the icon-over-label quick-action strip.

`Presentation::Platform`: the engine opens, positions and closes a surface for a
panel, and a nested panel hangs off the outer popup. On Wayland that is
`xdg_popup` with a grab; the GPU presenter shows the panel, and the CPU
presenter refuses so the panel paints in its parent. On Win32 that is an owned
non-activating `WS_POPUP` holding the mouse capture. The Win32 path is what
`picker_demo`'s footer and crumb menus run; the Wayland path is
compile-verified only. See
[Presentation surfaces](./window.md#presentation-surfaces).

`Presentation::Window`: the engine asks the surface's own window for a modeless
window owned by it, sized to the panel and titled by `Decl::window_title`. On
Win32 that is an owned dialog window without `EnableWindow`, so the dialog
behind it keeps taking input; on Wayland a modeless `xdg_toplevel` whose parent
is the owner, which is what makes the compositor float it instead of tiling it.
A size change is applied in place — Win32 resizes the window, Wayland moves the
toplevel's minimum and maximum with it. `picker_demo` uses the Win32 path for its
Properties and failure windows, which is the visual check; Wayland is
compile-verified only.

Not implemented: arrow tails and other decorations belong to the style layer.
Exit animation across a platform surface: a presenter closes with its subtree.

## See also

- [Layout](./layout.md): positioned boxes and the containing block.
- [Pointer Model](./pointer-model.md): who a press belongs to.
- [Transitions](./transitions.md): the 167 ms entrance a tooltip uses.
