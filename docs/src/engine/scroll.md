# Scroll Containers

A scroll container has exactly one owner of its offset. The engine owns it for
any node that declares `Sense::SCROLL_OWNED`; the container decides axes, size
and scrollbar; the application only reads the result. A design that keeps a
second offset in app state drifts from the painted content, so answer the
ownership question before writing scroll code.

## Who owns the offset

| Declaration | Wheel behaviour |
| --- | --- |
| `Sense::SCROLL_OWNED` | The engine accumulates, clamps and paints; the app sees no `Value` |
| `Sense::SCROLL` | The engine reports `Change::Value(-dy)`; the app holds the offset |
| `Clip { y: Overflow::Scroll }` | Makes the node a wheel target; ownership still comes from a sense bit |

The wheel lands on whatever is on top — a row, a label — never on the viewport
that would move, so routing starts from the topmost consumer and walks up.
`wheel_chain` gives the step to the nearest `SCROLL_OWNED` ancestor on the axes
its clip scrolls, and hands what that container could not take to the next
ancestor: an inner list at its end must not swallow the gesture the page behind
it still answers. A `SCROLL` container is a boundary — it takes the `Value`
itself, so nothing behind it moves. With no engine scroller anywhere on the
path, `app_scroll_target` names the nearest `SCROLL` node, or a clip-scrolling
one that declares no sense bit, and the app gets `Change::Value(-dy)`. A node
that asked for neither is not a scroll target and hears nothing.

A wheel carries no phases, so a burst is locked to the scroller it started on
for as long as the pointer stays put. The content moves under a resting
pointer, the node under it changes, and the gesture must not jump to whatever
lands there; a press, or any pointer move, ends the lock.

A drag chains by axis rather than by range: the pan holds the scrollers between
the press and the gesture's owner, and each takes only the axes it scrolls. An
inner container that owns an axis keeps the drag to its own end, where only that
container's rubber band absorbs the rest — the gesture never hands a spent axis
to the parent (see [Dragging, flinging, bouncing](#dragging-flinging-bouncing)).

- `Cx::scroll_of(id)` returns `ScrollGeom`: extent, viewport, offset, both
  axes, `active`.
- `Cx::set_scroll(id, x, y)` jumps after `view`; `Cx::scroll_to` eases under
  the spring.
- `Cx::bring_into_view(id)` moves every ancestor scroller the least distance
  that shows the node, innermost first and on both axes; the engine runs it
  after layout, and focus follows the same path.
- `AppLoop::scroll_of` and `scroll_offset` read the stored offset outside a
  build.

A plain wheel goes to the axis its device reported; shift turns it sideways, the
convention a mouse expects. A step spent on the container under the pointer is
not passed on, and a container with nothing sideways hands the horizontal step
to the ancestor that has it.

A container that scrolls exactly one direction takes the upright wheel on that
axis, so a horizontal strip is reachable with no modifier at all. This is a
deliberate trade-off: while the pointer is over such a strip the page behind it
does not scroll, because the strip captures the wheel the way a carousel does —
move off the strip to reach the page. When the device does report its own x, a
diagonal gesture splits: the strip takes x and the page still takes y.

## Geometry is one frame old

Before `view`, the engine walks the retained tree and hands the build one
`ScrollGeom` per `SCROLL_OWNED` node: extent from the previous layout, offset
from the stored table, `active` when a wheel or gesture moved it since the
last frame. `scroll_of` records `Dep::Scroll`, so a cached declaration
rebuilds when geometry or activity changes.

Reading this during `view` sees last frame's layout. That is the right anchor
for a scrollbar, whose shapes live in the container's own space and must not
move with the content.

The offset half is this frame's as soon as the build writes it: a `set_scroll`
earlier in the same `view` is folded into what `scroll_of` reports afterwards.
A list that moves the scroller and then asks which rows are visible would
otherwise window the rows of where the scroller stood last frame and paint blank
where the new rows belong, with nothing left to ask for the frame that fixes it.
An animated `scroll_to` is not folded in: the frame still paints where the spring
starts.

## Writing the offset back

The order is: app scroll requests, `apply`, re-apply the engine's stored
offsets, size transitions, presence, layout. The re-apply exists because the
app's declaration cannot reset a wheel position it never set.

The stored offset is exact and the painted one is snapped. `scroll_x`/`scroll_y`
receive the stored offset rounded to the grid text is rasterized on — the device
scale times the renderer's supersample — while the table keeps the wheel's exact
sum, so a trackpad's sub-pixel steps accumulate instead of rounding themselves
away and the content moves a whole raster pixel once they cross half of one. Text
takes a vertical subpixel phase; a fractional origin would give it a new
rasterization on every step while the rows and fills around it glide, which reads
as the text jittering. Every desktop scrolls by whole device pixels for the same
reason. `on_text_grid` is the one place that rounding happens, and only for
offsets the engine owns: an app-declared `scroll_y` is the app's.

Layout clamps the offset into `[0, extent - viewport]` so the wheel cannot
move content past its end. The exception is `params.scroll_elastic`, which the
engine sets only on frames where it is itself stretching the container;
clamping there would make the rubber band and the spring return invisible.

The viewport here is the content box, not the border box: padding is not
something the content travels through. One helper, `scroll_range`, answers the
range for the wheel, a request, the bar and a pan, so the six call sites cannot
drift apart again.

Immediately after layout, the engine reconciles the stored offset table against
the extents layout just wrote: an offset the content no longer reaches is
pulled back, and the entry of a node that is gone is dropped. Scrollers a pan or
a spring is stretching are exempt, for the same reason layout is.

The same pass compares each scroller's content box and viewport with the ones the
build read before layout, and a frame that changed either asks for the next one.
A windowed list declared the rows the old box reached, so this frame is showing
the rows of the box it used to be. A short directory replaced by a long one is
the case that found this: a `Scroll::vertical` hugs its content, so the viewport
was the short list's own height, and the long list stopped a few rows above its
end until an unrelated frame arrived. The clamped offset above is the same fact
seen through the offset: a range only moves when the box does.

Layout itself closes the same gap inside the pass. The content origin is
computed before the children are placed, so it can only use the previous
frame's extent; once the new extent is known, `settle_scroll_extent` compares
the offset it used with the one the new range allows and, when they differ,
moves the whole subtree back before anything is painted. The container's
`scroll_x`/`scroll_y` are corrected in the same step, so the parameters never
report a position the content cannot have.

## The scrollbar

`Scroll::decorate` computes one bar per scrolling axis from `scroll_of` every
frame and stores nothing:

```
visible = viewport / extent
size    = max(thumb_min, viewport * visible)
travel  = viewport - size
start   = offset / (extent - viewport) * travel
```

Each axis decides on its own: a container scrolling both ways draws a bar down
its right edge and another along its bottom edge, and an axis whose content fits
draws none under `Auto`. With no overflow, `Auto` and `Never` draw nothing;
`Always` is literal and shows a full-length thumb with nowhere to travel.
`Thin` floats a 4 dp thumb over the content; `Solid` is 8 dp and takes that
width out of the content box as a right or bottom inset, so the track never
covers a row. The default track is transparent.

An `Auto` bar is declared for as long as the content overflows, and the engine
decides whether it is showing. It fades in (0.12 s) while the pointer is
within the container, holds for 0.6 s after the last wheel step or thumb drag
and fades out over 0.3 s. The state lives in the engine because the bar carries
none of its own: a widget cannot animate a bar it stops declaring, and the
activity to read is the engine's `scrolled_since`. While a fade is in flight or
a hold is pending the frame reports `animating`, so the loop keeps presenting
until the bar lands; a bar resting under a stationary pointer is not awake, or
hovering a list would pin the window at the frame rate.

The painted bar is also the hit area: `ContainerParams::scrollbar` carries an
optional bar per axis — `Scrollbar::vertical`, `Scrollbar::horizontal` — each
with its thumb, track and fills in container space, and a primary press inside
either track becomes a thumb drag along that axis before anything else sees it
— no `pressed` state, no ink, no focus move. A press outside the thumb jumps it
to the pointer first, and either way the press ends any coast on that scroller.
The fade is part of that contract: a bar that has faded out takes no press, so
the thumb does not blink under the pointer when the content is touched.

The engine paints the bar, after the content and after the press ink, rather
than the container drawing it as one of its shapes. A bar is an affordance for
the whole viewport and its rects stay live press targets, so an opaque child —
a page fill, a card, a drawer surface — must not cover it.

The default thumb is translucent black, which is invisible on a dark surface.
A control supplies its own colour; `List` uses the pack's `OnSurface` at 40%.

## Dragging, flinging, bouncing

All three live in the engine, so every `SCROLL_OWNED` container gets them with
no control code.

- A press inside a scroller is only a candidate. It becomes a pan after
  `past_threshold`, so a click on a row still clicks.
- The walk up from the press stops at a `DRAG` or `SELECT` node: a slider or
  a selectable label keeps the gesture. A plain `CLICK` does not stop it, so
  dragging across a button scrolls.
- The pan holds a chain of scrollers, innermost first: each link takes the
  movement its own range allows on the axes it scrolls and passes the rest up.
  A vertical drag over a horizontal strip therefore moves the page behind it,
  and a horizontal one stops at the strip — without the chain the strip took
  the gesture and had no vertical range to spend it on. The same walk rules
  bound the chain: a `DRAG`/`SELECT` node or an app-owned `SCROLL` container
  ends it.
- While panning the raw offset follows the pointer 1:1 and is damped past
  either end with iOS' rubber band. Only the outermost link that scrolls an
  axis stretches on it, so a page and the strip inside it bounce
  independently — x on the strip, y on the page. The damped offset is what
  paints.
- On release, each link that moved coasts on its own speed with exponential
  decay (stop below 30 px/s); an overscrolled one is handed to a spring
  (k=280, c=30) seeded at the current offset. A link that took none of the
  movement is left still, so a vertical drag does not wake the strip's coast.
- Speed is measured per frame from the offset delta, not per event: events
  carry no timestamp. `frame_step` is capped at 50 ms, so a stall cannot fling
  the content away.
- A wheel, an app `set_scroll`, a scrollbar press, or any press inside the
  scroller ends the motion — including a press on a slider or a selectable
  label, which owns the gesture but still stops the content moving under the
  finger. A non-empty `scroll_motion` reports the frame as animating, which
  keeps the loop submitting frames.

## Choosing the container

- `Scroll::fixed(id, w, h)` — content at its own size, viewport clips (Qt's
  `widgetResizable=false`). `Scroll::vertical(id, max_h)` hugs its content
  up to a ceiling, which is right for a menu or a capped code block and wrong
  for a panel: `Scroll::fill(id)` takes the height the parent offers and
  scrolls vertically only, `Scroll::both(id)` fills the parent and scrolls both
  axes; `Scroll::fixed_both(id, w, h)` fixes the viewport instead, for content
  that must overflow a box the parent does not size.
- A region-size scroller fills; a content-size scroller hugs. A list or page
  that hugs its content inside a fixed panel leaves the clip, the scrollbar
  and the hit target at the content's bottom, so a drag or a pointer below the
  last row lands outside the viewport the panel shows.
- `Scroll::child` takes an evaluated `Decl`, so content is paint-only. Content
  that must report interaction (a nav drawer's `Change::Selected`) stays an
  `Element`: put it in a `Container` with `.scrollable()` and
  `.sense(Sense::SCROLL_OWNED)`, or use `Scroll::element_in`.
- A carousel with logical pages (snap targets,
  `max_scroll = page × (count - 1)`) is not a scroll container: its offset is
  page state, not geometric overflow. It stays in the control.

## Traps

| Change | Symptom |
| --- | --- |
| Keep a second offset in app state | Content and app disagree after one frame |
| Route the wheel by the raw hit target | Nothing moves: the wheel landed on a label |
| Expect this frame's extent from `scroll_of` | The bar lags one frame behind a resize |
| Write `params.scroll_y` from the app | The next apply restores the engine's stored offset |
| Clamp while the engine is stretching | No rubber band or bounce: overscroll never paints |
| Put a live control inside `Scroll::child` | Its `change()` never fires; clicks do nothing |
| Use `SCROLL` and expect the engine to keep the offset | The app gets `Value` deltas and owns the state |
| Add a threshold for "the coast ended" | The motion ends while the spring still moves, or never ends |
| Put page content straight into a fixed-height column | A document taller than the window runs past it, and the caret reveal has no viewport to move |
| Give a panel's scroller a hug size | A short list's clip, bar and wheel area stop at its last row, inside a panel with room to spare |
| Choose a row window from the raw offset | A rubber-band frame looks past every row and the list goes blank until the spring returns |
| Move the scroller, then choose the window from the offset read before it | The list declares the rows of its old position and paints blank where the new ones belong |
| Change what a windowed list holds without giving it the frame after | The window covers the old box, so the tail of the list is blank until unrelated input arrives |
| Keep measured row heights in the widget, not in the app | Every frame starts from the estimate, so the spacers never agree with the rows they stand for |
| Predict an unmeasured row at the one-line floor | A stretch never laid out scrolls at the shortest row's rate: five times too fast for wrapped rows |
| Snap on every wheel event instead of at the end of the burst | A trackpad's dozens of small steps each turn a page |
| Put a sideways strip's children in a `Column` | The pages stack down the strip and the horizontal overflow is zero, so it neither scrolls nor pages |
| Skip the root's own clip when painting | A scroller returned as the whole app draws every row outside its box |

## Corrections

**A windowed list could declare the rows of its old position.** The picker reveals a row by
asking `set_scroll` during `view`, then chooses which rows to declare from `scroll_of`. The
request was merged only after `view` returned, so that read still answered last frame's offset:
the frame declared the rows around where the list stood and painted nothing where the revealed
row belonged, and nothing asked for a frame that would choose the window again, so the gap
stayed until unrelated input rebuilt it. A `set_scroll` the build has already made is now what
the rest of that build reads back from `scroll_of`. Pinned by
`a_revealed_row_is_declared_by_the_frame_that_reveals_it` in `crates/ui-platform/src/view/tests.rs`.

**A list stopped above its end after the content grew.** Clicking a place whose directory was
longer than the one on screen listed it short: `C:\`'s last entry was missing from the list
until the pointer moved and the place's hover faded. The list hugs its content, so the viewport
the receiving frame read from `scroll_of` was the *old, short* directory's height; the rows
past it were replaced by the window's bottom spacer, and no fact asked for the frame that would
window again — the listing's completion had already bought the one frame that polled it. The
engine now compares every scroller's content box and viewport with the ones the build read, and
a frame that changed either asks for the next. That also covers the clamped-offset case, since
a range only moves when the box does. Pinned by
`a_list_whose_viewport_grows_owes_the_frame_that_rewindows_it` and
`a_shrunken_list_owes_the_frame_that_rewindows_it`.

**The wheel's sideways convention changed twice.** The handler first gave an upright wheel to
whichever container scrolled only x, so a code card with wrapping off swallowed every step and the
page behind it moved only by its scrollbar. The fix made a plain wheel strictly vertical and ctrl
the sideways one — which cured the swallowed page but made a horizontal strip need a modifier, on
the wrong key: a mouse user expects shift. Shift is now the sideways modifier, and a container
that scrolls exactly one direction takes the upright wheel on that axis on purpose — reaching the
strip needs no modifier, and reaching the page means moving the pointer off it. A device that
reports its own x keeps the diagonal split. Pinned by `a_shift_wheel_turns_the_wheel_sideways` and
`a_plain_wheel_over_a_sideways_strip_scrolls_it_and_not_the_page` in
`crates/ui-platform/src/view_scroll_test.rs`, and end to end by the gallery's `code unbroken`
self-test line, which now scrolls its unwrapped strip with a plain wheel.

**A paging arrow could not scroll its list.** `ScrollBy` walked up from the
requester, and an arrow beside a list is its sibling, so the walk found no
scroller and the request was dropped silently. The change now carries an
optional owner id and resolves through the same `scroll_owner` path as an
ancestry request; an arrow names the list it pages.

**A migrated nav drawer stopped navigating.** Moving the drawer into
`Scroll::child` wrapped an already-evaluated `Decl`, so the element adapter no
longer saw the engine's clicked state and `Change::Selected` never fired; the
self-test fell from 35/35 to 1/35. The drawer stayed an `Element` in a
`Container` with `SCROLL_OWNED`, and `Scroll` later gained `element_in` for the
same case.

**The rubber band was invisible.** Layout clamped every offset into the
content range, so the overscroll a pan had just produced was erased before
paint and the spring returned from an edge it never appeared to leave.
`scroll_elastic` now marks exactly the frames the engine stretches, and layout
skips the clamp only there.

**The list's scrollbar vanished on dark rows.** The shared default of
translucent black is right on a light surface and invisible on a dark one.
`List` takes its thumb colour from the pack's `OnSurface` role instead of
inheriting the default.

**A wheel over page chrome was reported as a semantic change.** With no scroller
on the path, routing fell back to the raw hit target, so a wheel over a label or
an empty area called `App::changed(hit, Value(-dy))` — and every such call threw
away the declaration cache, at the rate a trackpad delivers wheel events. The
fallback now names only a node that asked to scroll (`SCROLL`, or a scrolling
clip with no sense bit) and reports nothing otherwise. Caught by
`a_wheel_reports_a_value_only_to_the_node_that_asked_for_it` in
`crates/ui-platform/src/view/tests.rs`.

**The two scrollbar modes were documented and not implemented.** `decorate`
returned early whenever the content fitted, so `Always` — documented as drawn
even then — never appeared on a scroller with nothing to scroll, and `Solid`
only widened the thumb while claiming to reserve its track, so it still covered
the last 8 dp of every row. `Always` now draws a full-length thumb when nothing
overflows, and `Solid` takes its width out of the content box as a right inset.
Caught by `an_always_bar_shows_a_full_track_when_the_content_fits` and
`a_solid_bar_reserves_its_own_track` in `crates/ui-widgets/src/scroll.rs`.

**The scrollbar declaration assumed one bar on the vertical axis.** A container
that scrolls both ways could only declare a vertical bar, and a horizontal-only
strip could declare none at all — the sideways range had no handle. The type was
not the only place the assumption lived: the press and drag code read `pos.y`
and wrote `offset.y` directly, so a horizontal bar would have appeared and then
dragged the wrong offset. `Scrollbar` now holds one optional bar per axis,
`Scrollbar::bars()` is the single iteration order paint and hit-testing share,
and a drag stores the axis it started on. Caught by
`the_horizontal_thumb_drags_the_x_offset_only` in
`crates/ui-platform/src/view/tests.rs` and
`a_two_axis_scroller_draws_one_bar_per_axis` in `crates/ui-widgets/src/scroll.rs`.

**Focus could land on a row that was on screen in a panel that was not.** Keeping
a node visible moved only the nearest vertical scroller and stopped, so a row
inside a list that already showed it stayed put while the list itself sat far
down the page — the focus ring and the caret were off screen. The walk now
adjusts every `SCROLL_OWNED` ancestor, on each axis its clip scrolls, and
carries the displacement of each move into the next one because the rects still
belong to the layout that ran. Caught by
`keeping_a_row_visible_moves_every_scroller_above_it` in
`crates/ui-platform/src/view_scroll_test.rs`.

**A list that got shorter showed one empty frame.** The content origin is
computed from the extent of the previous pass, because the current one only
exists after the children are placed — so a list scrolled to its end and then
filtered shorter was laid out shifted by an offset that no longer existed, and
the frame that shrank it drew nothing where the content should have been. The
placement now compares the offset it used with the one the new extent allows
and moves the subtree back before paint, which is Flutter's
`applyContentDimensions` correction. The assertion in
`a_shorter_content_pulls_the_stored_offset_back_into_range` reads the content's
origin in the frame it shrinks.

**An inner list at its end froze the page behind it.** Routing picked the
nearest scrollable ancestor and stopped there: a card holding a list that had
reached its end swallowed every wheel step, and the page did not move while the
pointer was over the list. The step is now split — each scroller takes what its
range allows and passes the rest up — and a wheel burst is locked to the
scroller it began on, because the content moving under a resting pointer
changes the hit target mid-gesture. Caught by
`a_wheel_at_the_end_of_an_inner_scroller_moves_the_one_behind_it` in
`crates/ui-platform/src/view_scroll_test.rs`.

**A scrolling stack had no content extent.** `place_node` returned from the
`Stack` branch before the extent write at the end of the flow path, so a
`SCROLL_OWNED` stack reported a zero extent: the wheel clamped to zero, no bar
appeared, and `bring_into_view` could not move it. A stack has no line to
solve, so its extent is the union of the boxes it lays down; it is now computed
in that branch. Caught by
`a_scrolling_stack_reports_the_union_of_its_layers_as_its_extent` in
`crates/ui-core/src/layout_engine/tests.rs`.

**A shrunken list kept its old offset.** The offset table was written by wheels
and requests and never read back against the content. Filtering a list from 300
to 50 while scrolled to its end left 200 stored: layout drew the content clamped
to the new range, but the bar was computed from the stale value and fell outside
its track, `scroll_of` reported a position the content did not have, and the
first wheel step moved nothing. Layout now reconciles the table after every
solve — exempting the scrollers a pan or a spring is stretching, and dropping
entries whose node is gone instead of letting the next node with that id inherit
them. Caught by `a_shorter_content_pulls_the_stored_offset_back_into_range` in
`crates/ui-platform/src/view_scroll_test.rs`.

**"Not measured yet" was spelled as zero.** The layout clamp skipped whenever
the extent was zero, because a scroller's first frame has no content size yet —
so an empty scroller was never clamped at all. The extent is now `Option<Size>`,
`None` until the first placement, and a zero extent means empty.

**A `scroll_to` past the end slid out and snapped back.** The request was stored
as given, the spring carried the offset past the end, and the next layout pulled
it back — a rebound nobody asked for. Requests are clamped at the point they
land, against the same `scroll_range` a wheel uses.

**The coast ignored presses that were not pans.** Ending inertia was keyed on
`pan_candidate`, which is `None` above a slider or a selectable label, so
holding one of those while the content coasted left it sliding away under the
finger. A press now ends the motion of the nearest `SCROLL_OWNED` ancestor
whatever owns the gesture; a scrollbar press does the same, and before this it
had its position overwritten by the coast every frame.

**A wheel that changed the hovered row left the highlight behind.** The frame
recomputed the hover owner after painting and damaged the screen, but did not
request another frame, so the highlight stayed on the node that had scrolled
away until some other event arrived. A hover change now reports frame demand.

**Drag-to-scroll was first ruled out.** The design declared that a desktop
mouse needs no drag panning, no bounce and no inertia. The requirement for all
three reversed that: they now live in the engine's input layer, keyed on
`SCROLL_OWNED`, so a container gets them for free.

**A focused input only ever revealed its block.** Focus revealed the node's
rect, so moving the caret down a document taller than the viewport scrolled
nothing: the block already covered the whole viewport, and a wrapped paragraph
taller than it was only ever shown at its first or last line. The caret's line
box is the rectangle that matters, so `bring_into_view` now hands its rect to
`bring_rect_into_view`, which carries the same per-scroller walk, and an input's
caret goes through it with one line of context above and below. A press keeps
the plain minimum: the user pointed at a place, so the content must not shift
under the finger. It runs only when the caret or the buffer changed — a hand
scroll with a still caret stays where the hand left it — and the buffer is
compared by digest because the app may rewrite the declared text without an
edit. The digest is per frame while an input is edited, which is O(document)
work; a buffer revision counter would make it O(1) if a very large document ever
shows it in a profile. Only `SCROLL_OWNED` containers move, as everywhere else.

**Focusing an input still dragged the document's edge into view.** The caret reveal had
replaced the block reveal for an input's own line, but focus moving to the input still called
`bring_into_view` on its node — and that node is the whole buffer. A press on a line already
on screen therefore revealed the document's top first, and the caret reveal then measured
against the offset that move had stored, leaving the pressed line at the viewport's edge: a
scroll on a click whose content was fully visible. A text input's focus now reveals nothing
by its node (`is_text_input`) and forgets the last reveal instead, so the caret's own line
alone moves the viewport, from the offset the user left it at. Pinned by
`a_press_that_focuses_the_document_keeps_a_visible_line_still` (red: the offset moved from
200 to 160.8 on a press at a visible line).

**An unmeasured row was predicted at the one-line floor, so a stretch never laid out scrolled
five times too fast.** The estimate was documented as the floor of a one-line row, which is
the shortest a row can be: on a list whose rows wrap to five lines, 300 points of travel
covered 14 rows in a stretch that had never been laid out — and the frame the stretch was
measured, the rate dropped to the two rows those heights actually allow, so the list jumped
where the estimate ended. Rows are now predicted from the average of the rows layout has
already given a height to, floored at the declared one-line height; the same travel covers 2
rows. Caught by `a_stretch_never_measured_scrolls_at_the_measured_rate` in
`crates/ui-widgets/tests/virtual_list.rs` (red: 14 rows, the assertion allows 4).

**Paint never pushed a clip for the root, so a scroller returned as the whole app drew outside
its box.** `declares` skipped `is_root`, on the assumption that the root is a synthetic frame
wrapper; it is the app's own declaration, so a `SCROLL_OWNED` root pushed no clip at all and
every row scrolled past the viewport was still painted where layout put it — visible whenever
the scroller is smaller than the window. Paint now honours a root's clip like any other
node's; the rect it pushes is the root's mask, which layout had already intersected. Caught
by `a_row_below_the_viewport_paints_no_primitive` in
`crates/ui-platform/src/view_scroll_test.rs`.

**The painted offset carried the wheel's exact fraction.** The stored offset was written
straight into `scroll_x`/`scroll_y`, so on a fractional display scale every step moved the
content by part of a device pixel. Text takes a vertical subpixel phase, and that phase
changed with every step: the glyphs re-rasterized each frame while the rows and fills around
them moved smoothly, which reads as the text jittering — the complaint was a virtual list
whose words shook while scrolling. The painted offset is now snapped to the grid text is
rasterized on by `on_text_grid`, and the stored one stays exact. Caught by
`an_engine_owned_offset_paints_on_the_text_grid` in
`crates/ui-platform/src/view_scroll_test.rs` (red: a stored 10.5 at scale 1.5 painted at
15.75 device pixels instead of 16).

**A panel's scroller hugged its content, so a drag was cut where the content ended.** The
picker's list, grid and sidebar used `Scroll::vertical`, which sizes itself to its content up
to a ceiling: in a folder of one row the viewport — and with it the clip — was 32 units tall
inside a 463-unit panel. A row dragged below the last one left the viewport's box and was
clipped at the content's own bottom, in a place the panel had room for; the scrollbar also
covered only the first third of the panel and a reveal scrolled a list that already fit. The
scroll container's size must follow the *parent's* offer, not its content: `Scroll::fill(id)`
is the vertical constructor with `AxisSize::fill` on both axes and `Grow::Yes` down the main
axis, and `Scroll::both` now grows on both axes for the same reason. Hugging stays correct
where the ceiling is the point — a menu, a code block capped at N lines. Caught by
`a_short_list_fills_the_panel_below_its_header` in `crates/ui-picker/src/view.rs` (red: 32
units against 463 available).

## Long content

Nothing is windowed: layout walks every node, and every node still costs its declaration, its
placement and its hit entry, so a long document still costs its length per frame. What paint
does cull is everything a clip can never show: shapes are dropped as they are emitted (see
[Culling is the complement](#culling-is-the-complement)), and the text pass drops text and
maths that miss the clip stack, which is most of the cost of a document that is mostly text.

Measured in a release test build at 800×600, a column of text nodes in one `SCROLL_OWNED`
container:

| Paragraphs | Still frame | Scroll frame |
| --- | --- | --- |
| 1,000 | 2.3 ms | 1.3 ms |
| 10,000 | 22 ms | 17 ms |

Before the resolve pass culled to the clip stack, the same 10,000-row column cost 139 ms on a
still frame and 144 ms while scrolling, and placed 369,000 glyphs instead of 136. The same
change took the gallery's markdown page over a 550 KB document from about 420 ms per frame to
about 113 ms, measured in a release build in the nested session. The rest of the frame is the
declaration rebuild, the layout walk and the hit index, which still grow with the document.
One earlier blowup is recorded rather than measured again: a 3,000-row scroll frame spent
1.9 s in the damage accumulator before it stopped merging regions quadratically (see
[Frame Pipeline](./frame-pipeline.md#damage)).

### Windowing a list

Mainstream frameworks window *at the widget layer* rather than cull at the paint layer:
Flutter's `SliverList` lays out only the children the viewport reaches and asks its delegate
for the rest, GPUI's `List` draws an index window with overdraw, Qt's item views ask a model
for the rows a viewport shows, and egui's `ScrollArea::show_rows` lays out a row range and
spaces the rest.

`RowWindow::of` is that window for a declaration-built list. It reads `Cx::scroll_of` — a
`Dep::Scroll` read, so a memoised declaration rebuilds when the viewport moves — and takes the
row count, an overscan and a height per row. It answers the rows to declare and the height of
the rows above and below them, and the caller declares those rows between two spacers. Ten
thousand rows of alternating height cost 20,002 nodes and 38–60 ms per frame declared whole,
against 35 nodes and under a millisecond through a window (release test build, 360×300).

`List` is the first consumer: it windows its rows through `resolve_in`, so the gallery's
thousand-row sample declares the eight rows its viewport shows. Its context-free `resolve`
still declares every row, because it has no geometry to choose a window from.

### Measured heights

The heights the spacers need are not always known before layout: a row whose text wraps is as
tall as its lines. `RowHeights` holds both halves: a prediction for every row, and the height
layout gave each row that has been declared. `RowHeights::window` first reads the boxes the
last window declared out of `Cx::rect_of` and stores them, then asks `RowWindow::of` for the
next window with the truer heights. The table belongs to the app, because it has to outlive
the frame; `List::resolve_in` takes it as an argument and reads `row_height` as the floor of a
one-line row rather than its exact height.

The prediction for a row never laid out is the average of the rows that have been, floored at
that one-line height — Flutter's sliver estimates the same way, and so does `SliverList`'s
`estimateMaxScrollOffset`. A fixed floor is always the shortest row: on a list whose rows
wrap to five lines, a stretch that had never been laid out scrolled past 14 rows per 300
points of travel, five times the real rate, and changed to the real rate the frame the
stretch was measured. With the average the same travel covers 2 rows, which is what rows
~118 points tall allow (measured on the `WrappingList` fixture in
`crates/ui-widgets/tests/virtual_list.rs`). The prediction is never below the declared floor,
so a list of uniform one-line rows predicts exactly what it did before.

A row keeps the height it was measured at for as long as it stays out of the window, so the
content does not shift when rows leave it: they left with the height they had. The estimate
still shows in two places. A stretch whose rows genuinely differ from the average scrolls at
the average's rate until it is measured, then snaps to its own — reading down and back up is
exact because those rows were measured on the way down. And the extent, with it the scrollbar
thumb, follows the prediction until the rows are measured. A width change invalidates every
measurement, since the same text wraps differently: start a fresh table rather than trusting
the old one.

Before the first layout there is no viewport to intersect, so the window assumes 800 logical
units of height. The frame that lays the scroller out has changed its box, which is what makes
the engine ask for the frame that windows from the real one.

### Snap paging

A snapping scroller comes to rest on whole pages: `Decl::snap([x, y])`, or `Scroll::snap(page)`
on the widget, gives each scrolling axis a page size (zero leaves the axis free). The offset
still moves freely *during* a gesture; the page is chosen when the gesture ends, and the strip
eases to it under the same spring `scroll_to(animate)` uses.

Two gestures end differently. A released finger is a request to go where the hand was heading:
the fling's landing is `offset + v / k` for the engine's decay `k`, and the page nearest that
landing wins, so a drag of two fifths of a page springs back and one of three fifths turns. A
wheel is a request to change pages instead: any travel in a direction turns at least the next
page that way, because a notch of fifty pixels on a four-hundred-pixel page must still be a
notch. A wheel burst is therefore tracked from the offset it began at, and the page it lands on
is the one the burst travelled into, at least one page ahead of where it started.

A wheel has no end event, so the burst ends by going quiet: `SNAP_IDLE` (120 ms) without a
wheel. That deadline is the one place the engine asks to be woken for something other than the
caret (`next_wake`), because after the last wheel notch the loop has no other reason to build
the frame that settles the strip.

`Scroll::horizontal` is the strip this is for: a fixed-size `Row` that scrolls sideways and
takes the upright wheel, so a mouse pages it without a modifier. The gallery's generic page
carries a four-page sample.

### Culling is the complement

Windowing cannot help one text block larger than the viewport: an editor holding a 500 KB
buffer is a single primitive. The text pass therefore tracks the clip stack and drops a text
or formula primitive whose box, plus one em of slack, misses every clip in force; the rows
of a long document that sit below the viewport no longer shape or place. On the same 10,000
row fixture a still frame fell from 139 ms to 22 ms and a scroll frame from 144 ms to 17 ms,
and the placed glyphs from 369,000 to 136.

Shapes are culled earlier, as `push_shapes` emits them: the draw list keeps the clip in force
(intersected down the stack and already transformed, so a presence scale is priced in) and a
shape whose painted bounds — shadow blur and sub-shape offsets included — miss it is never
pushed. That also takes the primitive out of the damage fingerprint, the batch plan and the
backends' vertex builds. The rows of an unwindowed 5,000-row column therefore emit 5
rectangles instead of 5,000; in a release test build at a 200×100 viewport the whole frame
went from 13.8 ms to 11.9 ms still and 10.2 ms to 9.3 ms while scrolling. The rest of the
frame is the declaration rebuild, the layout walk and the hit index, which is why the gain is
small next to the text pass's.

It does not replace windowing. A culled document still pays for its declaration, its layout
walk and its hit index, while a windowed one has no rows there at all.

## Status

Implemented: engine-owned offsets on both axes, readable geometry with
dependency tracking, `set_scroll`/`scroll_to`/`bring_into_view`, the caret of a
focused input revealed line by line (`bring_rect_into_view`), wheel routing by
device axis with shift for the sideways one and single-direction containers
taking the upright wheel, scrollbar geometry on both axes with engine-owned
fading and thumb dragging, a fill-height vertical constructor (`Scroll::fill`)
beside the hugging one, pan chained through nested scrollers with a rubber
band, inertia with spring return, a row window (`RowWindow`) that declares only
the rows a viewport shows, measured row heights (`RowHeights`) so the window also
works for rows a layout sizes to their wrapped content (`List` uses both), snap
paging on either axis, chosen at the end of a wheel burst or a released drag, and
painted offsets snapped to the text raster grid so a glide keeps every glyph on one
rasterization.

Not implemented: a document still costs its declaration rebuild, its layout walk
and a hit entry per node per frame — culling and windowing only remove the
primitives (see [Long content](#long-content)).

## See also

- [Pointer Model](./pointer-model.md): how a gesture is assigned before a pan
  starts.
- [Frame Pipeline](./frame-pipeline.md): where the write-back and layout sit.
- [Incremental Declarations](./declarations.md): how `Dep::Scroll` invalidates
  a build.
