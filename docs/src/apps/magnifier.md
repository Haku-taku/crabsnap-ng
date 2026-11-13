# Selection Readouts and the Loupe

The region overlay says nothing about the rectangle being dragged, and nothing
in the application magnifies a pixel. This page is the contract for both: the
two pills that state a selection, and the loupe that shows the picture under the
pointer.

## Problem

A drag leaves the user counting pixels by eye. The saved file's size is the
capture's own pixels, while the overlay's geometry is in surface points, and at
2× those differ by a factor of two — so the number the user guesses while
dragging is not the number the file carries. Nothing states where the rectangle
starts either, which is what a second capture of the same area needs.

Placing a mark on a one-pixel edge, reading a colour, or checking that a border
landed on the intended row is impossible at the overlay's own scale. The legacy
project answers both needs: a size pill beside the selection
(`src/ui/chrome/mod.rs:494-533` there) and an eight-times loupe with coordinate
and colour pills (`src/ui/chrome/mod.rs:791-940` there).

## Comparison

| Source | Readout | Magnifier | What it establishes |
| --- | --- | --- | --- |
| Legacy project | `W × H` pill outside the selection, avoiding the toolbar | 8× disc, right button held, coordinate and colour pills, selection edges drawn inside the disc | Both features belong to selection chrome, not to a mode |
| Legacy project | Size in the selection's own logical units | Samples the base picture plus its annotations | What the loupe reads |

The legacy readout deliberately omits the origin (`src/ui/chrome/mod.rs:494-495`
there). This application shows it because two captures of one area must line up
exactly, and the origin is the only number that lets them.

## Contract

### The pill

Readouts are dark pills: a fill of `#141414` at 88 % alpha, white ink at 13
points, 6 points of horizontal and 3 of vertical padding, a 4 point radius. The
fill is dark rather than the app's MD3 surface because it sits on an arbitrary
picture: a light chip disappears on a bright screenshot, and a frosted one
blurs the pixels it is meant to help read.

### The selection readout

Two pills in one row: the selection's origin as `x, y`, and its size as
`w × h`. Both count **image pixels** — the surface point multiplied by the
frame's device ratio — because that is what `Save` writes and what a file
viewer reports. The row shows whenever a selection exists, including while it is
being dragged, resized or moved.

Placement is a fallback chain, in this order:

1. above the selection's top-left corner, right-aligned to it;
2. right of the top-left corner, level with the top edge;
3. below the selection's bottom-left corner, right-aligned to it;
4. inside the selection, 6 points in from its top-left corner.

Each candidate is clamped into the surface. A candidate is skipped when it
overlaps the selection, or the action bar when `cx.rect_of(bar::id())` reports
one: the bar is drawn above the overlay's chrome, so a pill beneath it is
invisible. The fourth candidate is the answer for a selection that fills the
surface.

### The loupe

The disc is 120 points across. Its zoom is an integer in `2..=16`, 8 by
default, and one wheel notch moves it to the next zoom while the loupe is
visible. The disc covers about `120 / zoom` image pixels of the picture, so a
higher zoom shows less area, not a larger disc.

The disc's texture is `120 × surface device ratio` texels, and each image pixel
is drawn as a block of exactly `zoom` texels, sampled nearest. The window need
not hold a whole number of them: it is an odd count,
`2 · ceil((W − zoom) / (2 · zoom)) + 1` cells of a disc `W` texels across — one
cell for every whole pixel the disc's width leaves beside the aimed one, rounded
up so no rim texel reads a cell the window did not fetch. Odd keeps the aimed
pixel in the middle cell, whose middle is the disc's centre, so the crosshair at
that centre always names the pixel it shows; the guide's edges are mapped in the
picture's own pixels by the same rule, which is why a guide edge lands on the
boundary it separates. A caller therefore hands the guide a rectangle already on
that grid: the overlay passes the integer crop box, never the fractional
selection the pointer is dragging.

A source pixel outside the picture repeats the edge pixel rather than leaving a
hole: the pointer at the surface edge must still stand at the centre of the
disc. Two neighbouring zooms can hold the same number of cells — 8× and 9× both
hold 15 on a 1× surface — but their blocks differ by a texel, so the pictures
never agree.

The disc is round by alpha: output pixel `i` takes coverage
`clamp(r - dist + 0.5, 0, 1)` from the disc centre, in output pixels. There is
no circular clip anywhere in the paint list, and this is the same rule the
legacy sampler uses (`src/ui/chrome/magnifier.rs:116-131` there).

The loupe is visible in three cases:

- the toolbar's loupe switch is left on;
- the secondary button is held, anywhere on the overlay or on the editing pin;
- a selection drag is running in the region overlay, so the edges it is placing
  can be seen up close.

Drawing a mark never raises the disc: in the pin editor the left drag is the
mark, and the legacy rule is the same (`magnifier_wants` there excludes
`pin_edit` drags). A switch already on keeps the disc up through a drag, which
is the user's own answer to "show me while I draw".

The switch carries three colours, because held and on are different states and a
user pressing the secondary button has to see which one is showing: off; amber
while the secondary button holds a disc up that a tap has not switched on; and
the bar's own selected tone once a tap has left it on. Releasing the secondary
button while the switch is off returns the button to off with the disc.

The disc sits at the pointer's bottom-right by `1.1 × radius`, flips to the
opposite side on an axis that would leave the surface, then clamps.

The zoom is not one of the readings: it is a white plate over the disc's top,
with black text chosen by the same contrast rule. It keeps no vertical padding
of its own — the line box around the digits is air enough. Its centre is
corrected for the circle: the disc's edge falls a sagitta
`R - sqrt(R² - (w/2)²)` below its apex over the plate's half-width, so the plate
drops by that sagitta and the edge crosses it at the plate's own middle.
Centring the plate on the apex leaves the whole arc below its middle, which
reads as a plate lifted outward. A disc at a screen edge moves the plate onto
the disc rather than letting it hang off the surface.

The disc is a box rounded to half its side — a circle — and the plate a rounded
pill, and each casts the same thin shadow, `ShadowSpec::black(4.0, 0.35, 1.0)`;
the plate's falls on the disc as on anything else, with no exception for the
edge it crosses. The disc also keeps a white 1.5 point line on its own edge,
because a shadow fades into a dark picture where a line does not. The plate has
no stroke of its own: its white body is the edge, and the shadow separates it.

The two readings stack below the loupe — above it when the surface has no room,
or when the action bar floats over the place they would take — as one column of
equal-width pills, so they read as a single panel; "above" is measured from the
plate, so a reading that steps up never covers the zoom. The top pill is the
aimed pixel's image coordinates. The bottom one is the colour:
its body *is* that colour, opaque, and its ink is black or white, whichever the
WCAG relative luminance of the body makes more legible. A white `#RRGGBB` on a
pale swatch would vanish, which is why the text follows the colour instead of
the other way round. The colour is read from the same SDR rendition the disc
shows, so body and pixels agree.

The disc's pixels are the picture with its document, rasterized through the same
list a file gets: every mark the document holds shows in the disc, and a stroke
in flight is a preview in that document from the frame it has a shape, so the
disc follows the hand rather than waiting for the release. The guide rectangle
is separate from that ink: it names the box a mark occupies, which a one-pixel
line or an unfilled rectangle would otherwise leave to guesswork.

Inside the disc, the guide rectangle is drawn: the selection in the overlay, and
the mark being drawn or the selected mark in the pin editor. Its four edges are
mapped into disc coordinates and clipped analytically to the circle at the
source; the paint list only clips to axis-aligned rectangles, and giving both
backends a path clip for one static disc is not a trade worth making. Each edge
is drawn twice, black at 2.4 points under white at 1.2, because one colour
disappears over some picture.

The crosshair is two white line segments through the disc's centre, `side / 2.5`
long, and the disc's edge is a white 1.5 point ellipse drawn over the texture,
on the same circle the texture's alpha fades at.

### The engine seam

The secondary button is the loupe's trigger, and the engine had no way to report
it: a right press focused the node and opened its menu, and the app saw neither.
`Sense::SECONDARY` opts a node into the press:

```rust
pub const SECONDARY: Sense = Sense(1 << 10);
```

The press is reported to the nearest ancestor declaring it, as
`Change::Secondary { down: true }`, and the release as `down: false`. A release
within 300 ms of the press is still a **tap**: the engine then opens the node's
menu exactly as an immediate right click did, so existing menus (a mark's Edit
and Delete rows) keep working. A longer hold suppresses the menu, which is what
makes hold-to-aim and tap-for-menu coexist on one button.

Nodes without `SECONDARY` in their ancestry keep the old behaviour: the menu
opens on the press.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui/crates/ui-core/src/sense.rs` | `SECONDARY` |
| `ui/crates/ui-platform/src/element.rs` | `Container::from_decl` adopts the children of the declaration it wraps |
| `ui/crates/ui-core/src/change.rs` | `Change::Secondary` |
| `ui/crates/ui-platform/src/view.rs` | Press/release routing, the tap threshold |
| `apps/crabsnap/src/pill.rs` | The dark pill and a row of them |
| `apps/crabsnap/src/readout.rs` | Labels and the placement fallback chain |
| `apps/crabsnap/src/loupe.rs` | Zoom range, sampling, disc texture, guide, node, the on/held switch |
| `apps/crabsnap/src/toolbar.rs` | The switch's three colours and its message |
| `apps/crabsnap/src/main.rs` | Overlay wiring: readout, right-hold, wheel, drag visibility |
| `apps/crabsnap/src/pin/edit.rs` | Pin-editor wiring: right-hold, wheel, guide |

## Verification

| Test | What it would catch |
| --- | --- |
| `a_secondary_press_reaches_the_node_that_asked_and_a_tap_still_opens_its_menu` | A press reported nowhere, or a menu that no longer opens |
| `a_held_secondary_press_reports_the_button_and_withholds_the_menu` | Every right press reaching the app and the menu both |
| `the_readout_prefers_a_place_that_covers_neither_the_selection_nor_the_bar` | A fallback chain that stops at the first candidate, or ignores the bar |
| `the_disc_flips_at_the_edges_and_stays_on_the_surface` | A disc that hangs off the surface at a corner |
| `the_disc_centre_is_the_aimed_pixel_and_the_rim_fades` | A sampling offset, or a circle that is a square |
| `a_wheel_notch_moves_the_zoom_to_the_next_window_and_stops_at_the_limits` | A zoom that runs past the range, or a bank that drops the remainder |
| `no_two_zoom_steps_show_the_same_picture` | Two neighbouring zooms drawing one picture, as 13× and 14× did |
| `the_window_holds_every_cell_the_disc_shows_and_no_more` | A window rounded down, where the rim reads a cell it never fetched |
| `a_source_pixel_is_one_block_of_zoom_texels` | A zoom whose blocks are uneven, or an aim that is not the block under the crosshair |
| `the_crosshair_is_the_disc_centre_and_shows_the_aimed_pixel` | A crosshair off the disc's centre, or a centre that shows a neighbour |
| `the_guide_edges_sit_on_the_block_boundaries_the_disc_draws` | A guide mapped by cells while the sampling maps by pixels |
| `the_swatch_ink_keeps_its_contrast` | A colour pill whose text disappears on its own swatch |
| `the_zoom_plate_sits_where_the_rim_meets_its_middle` | A plate centred on the rim's apex, which reads as lifted outward |
| `the_readings_step_above_the_disc_for_the_bar` | A reading hidden under the action bar, or one that steps up onto the plate |
| `from_decl_adopts_the_children_of_the_declaration_it_wraps` | A readout or loupe silently losing every node under it |

## Corrections

**The legacy readout counts points; this one counts pixels.** The legacy size
pill reports the selection's logical extent (`sel.width()` there), while the
file it saves carries the capture's pixels: at 2× the two differ by two.
Both readouts here count image pixels, because that is the number the user
compares against the saved file and against another tool. The placement rule is
kept from the legacy project.

**The disc sampled the picture alone and showed no marks.** The first version
read the raw pixels, on the argument that the marks are already drawn as vector
chrome over the disc's area; the user's report — "why can the loupe not see the
annotations?" — is the answer to that argument, since a stroke being placed with
the loupe up is exactly what one is looking at. The disc now reads a raster of
the composed list, the picture and its marks through the same `painted` the
canvas and the export use, and `bake` samples that. Only the disc's window is
rasterized, a handful of cells, because the window is what the zoom and the disc
side decide. The window carries the composed list's own limitation: the CPU
rasterizer draws a text mark as one bar per character cell
(`ui/crates/ui-paint/src/lib.rs:1142-1172`), so text appears in the disc as the
bars a flattened export would write, not as glyphs. What catches a regression is
that `bake` and `window` derive the window from one function,
`cells_beside` — a disc that reads a window it did not fetch would repeat
its edge instead of magnifying the aim.

**The legacy zoom is fixed; this one answers the wheel.** The legacy sampler
bakes `ZOOM = 8` into the disc size (`src/ui/chrome/magnifier.rs:21-23` there).
The disc here is a fixed 120 points and the zoom changes the area it covers, so
the wheel changes what is visible without moving the disc or the pointer.
Integer zoom keeps each source pixel a whole block of output pixels, which is
what a pixel loupe is for; a fractional factor would blur the grid it exists to
show.

**The window is not a division of the disc.** The first ladder rounded the
window to `DISC * dots / zoom`, so on a 120 texel disc 13× and 14× both landed
on a nine pixel window and showed the same picture, as did 15× and 16× — the
user reported the pairs as indistinguishable. The next version made the window
the disc's texel side divided by the zoom and offered only the divisors of that
side: every block was whole, but half the range was gone and on a 1× surface the
plate could state 15× at most. The premise was wrong, as the user then pointed
out: the window need not hold a whole number of pixels. Sized from what the
disc's width leaves beside the aimed block, it is odd, every integer zoom still
draws blocks of exactly `zoom` texels, and 13× and 14× differ by one texel of
block width. `no_two_zoom_steps_show_the_same_picture` compares the baked
textures of every zoom on 1×, 1.25×, 1.5× and 2× surfaces — red under the
rounded window, where 14× repeated 13×;
`the_window_holds_every_cell_the_disc_shows_and_no_more` is red when the cell
count is rounded down and the rim reads a cell the window never fetched.

**A row handed to the overlay had no children.** The first implementation
wrapped the measured row in `Container::from_decl`, and the readout measured
0 × 0 in the running application while `pill::measure` returned 147.4 × 24.2:
a container element rebuilds its declaration's children from its own element
children, so the finished subtree arrived with none. `child_decl` is the door
for a finished subtree. The 0 × 0 was first papered over with a fixed size,
which hid the loss and left the pills' height disagreeing with placement by a
point.

**`from_decl` refused the subtree instead of keeping it.** The guard that
followed — a `debug_assert!` that the wrapped declaration is childless, and a
doc note to use `child_decl` — made the loss loud in a debug build and silent in
release, where the assert compiles out. That is how a node came to sit in the
tree and paint nothing. The API was the defect, not its caller: it took a
finished declaration and dropped every child of it. It now takes the children
out of the declaration and adopts them as leaf children, so the result is total
for any declaration, and the loupe's editor path lost the comment that warned
about it. `from_decl_adopts_the_children_of_the_declaration_it_wraps` is the
regression check; it was red against the guard, rebuilding a declaration with
one child and finding none.

**A row's height is its line box, not its ink.** `cx.text_size` reports what
the measurer drew (17.24 points for 13 point text), while layout raises every
text node to its line hint (`layout_size_class` in
`crates/ui-core/src/layout_engine.rs:320`), giving 18.2. Placement that used
the ink height put the row a point into the selection it was avoiding.
`pill::measure` now measures the declaration the pill itself builds and takes
the larger of the ink and the line hint; the two numbers agree to the bit, so
the row hugs without a declared size.

**Blocks must be centred on the aim, not rounded onto it.** The first sampling
mapping chose a cell by `round(offset / zoom)` from the window's middle cell.
At zoom 3, where the window's middle falls between cells, that made the aimed
pixel a two-texel block and its neighbours uneven. Addressing the aimed cell's
middle in texel space — `floor((du + zoom / 2) / zoom)` — gives every source
pixel exactly `zoom` texels, which is what the block test states.

The crosshair moved three times. It began on the disc's centre, which is a cell
boundary whenever the window has an even number of cells. The next version moved
it half a cell right in *every* case, taking the aimed cell's position inside
the window for its position on the disc: at the default 8× the window had 15
cells, so the crosshair sat 4 points right and low of the pixel it named, and
the user saw it as crooked. Moving it again by the cell's measured offset was
still wrong whenever the window held an even number of cells, which was most
zooms.

The crosshair is not the thing to move, and neither is the window: an odd window
makes the aimed pixel its middle cell, so centring that cell on the disc's
centre is the same as putting the crosshair there, and it never moves. The
guide map uses the same rule, so edges draw where the pixels they separate are
shown. `the_crosshair_is_the_disc_centre_and_shows_the_aimed_pixel` walks every
zoom, asserting the aimed block is `zoom` texels wide and covers the centre; a
4× screenshot put the aimed cell's four corners exactly on the disc's centre row
and column.

**The readings were three pills in a row; they are a column under a rim plate.**
The first version put `8×`, the coordinates and the colour side by side, and the
colour pill carried white text on the overlay's dark body. Three pills are as
wide as the disc and more, and a white `#RRGGBB` told the user what colour was
read without showing it. The zoom now belongs to the rim it is a property of,
and the colour pill carries the pixel itself as its body, with its ink chosen
by contrast. The two readings share one column and one width, so they are read
as a panel rather than as three unrelated numbers.

**The zoom plate is the rim's white outline widening, not a chip on it.** It
began as a dark pill of its own with white text, and the user read it as a chip
laid over the border; the body is now `RIM_LIGHT`, the very white of the rim's
outline, with black ink from `ink_on`. The next reading of the request was
wrong in the other direction: the plate was stood wholly outside on the rim's
inner edge, which kept the picture clear but read as a tab bolted on rather than
as part of the border. It straddles the rim again. The alignment needed a
correction rather than a choice of side: the rim under the plate is an arc, so
placing the plate's centre on the apex leaves the whole arc below its middle and
the user saw it as pushed outward. `place_plate` now drops it by the sagitta
`R - sqrt(R² - (w/2)²)`, which puts the rim's entry points at the plate's
middle. A 1× screenshot of the 28 point plate showed it 699–715 rows tall with
its middle at 707, and the rim's white line 1.7 rows below the apex at its
sides. `the_zoom_plate_sits_where_the_rim_meets_its_middle` states the
invariant. Its own padding is zero: on a plate this short the line box already
leaves air around the digits, and every point it keeps cuts into the rim.

**The band was tried on the plate, then on the silhouette, then dropped.**
The dark band was the disc's ellipse and nothing else: where the plate stood out
of the circle its white body began with no edge, and the plate read as a chip
resting on a ring. The first attempt gave the plate a band on its own outline,
drawn with the plate's body and so over it; the user saw a band on the plate's
upper half only — which is the geometry, since below its crossing the plate is
interior to the union — and far too wide, because a stroke straddles its path
and the plate has no white core to split it, where the disc showed a dark line
either side of its white. The second attempt painted one band for both shapes in
a node under both faces, so each face covered its inner half; that is correct,
and it also halved the disc's rim and left the two edges at different weights.
The band is gone. Each shape casts a thin shadow instead; the disc keeps its
white 1.5 point line, the plate has none, and the plate's shadow needs no
treatment where it crosses the disc. The band's geometry (`plate_edge`) and its
test went with it. Nothing pins the shadows but the eye: a screenshot of the
plate's lower edge over the texture shows its falloff, 126 → 185 over five rows.

**The readings went under the action bar.** The bar floats in its own layer
above the overlay, so the column placed below the disc vanished beneath it, and
the colour reading — the one thing the loupe exists to report — was what got
lost. `place_readout` now takes the bar as an obstacle, like the selection
readout does, and takes the loupe's own extent (disc plus plate) as its anchor,
so a column that steps up clears the zoom plate instead of covering it.
`the_readings_step_above_the_disc_for_the_bar` states the step; the plate's
footing stays with `the_zoom_plate_sits_where_the_rim_meets_its_middle`.

**The disc's guide sat at quarter-pixel offsets.** The guide is mapped in
picture pixels, so its edges land on the boundaries between the blocks the disc
draws — but the overlay handed it the selection rectangle, which is fractional
on purpose. Dragging an edge put the border at 0.25 or 0.5 of a pixel, and the
disc drew the line inside a block instead of between two, at an offset that
changed as the hand moved. The overlay now derives the guide from the same
integers the crop takes (`Image::crop_of`), so the border hugs the content it
encloses. The unit test states the mapping; the call site was the bug, so what
holds there is that the guide and `Space` are built from one `crop_of` result
and cannot round differently.

**The overlay's disc covered the bar; the pin's went under it.** Both surfaces
draw the disc in their chrome tree, but the overlay appended it after the
toolbar and the pin editor before it, so a resize near a docked bar hid the
buttons on one surface and not the other. The order is the contract: the disc
reports, the bar acts, and the control a finger is aiming at stays on top. The
overlay now appends the disc before the toolbar, where the pin editor already
had it. A screenshot of a bottom-edge drag with the bar docked beneath shows the
pill whole and the disc's arc cut across its top.

**The wheel stopped wherever a mark's chrome was.** A wheel step arrives as
`Change::Value` for the node under the pointer. Over bare picture that node is
the overlay's root, which is the overlay's own disc; over a mark, a handle or
the bar it is a node the annotation session owns, so the change went to
`Session::change` and its fallthrough, `Session::wheel` — a disc the *pin*
editor draws and the overlay never raises, since it passes `loupe: false`. The
step was swallowed there and reached nothing. A disc is a property of the
surface, not of the node under the pointer: the overlay now takes a step the
session declines for its own disc, in the one place both owners are decided.
What catches it now: a nested-session run with the switch on, where the wheel
took the disc from 8× to 2× over bare picture and from 2× to 9× with the
pointer over a text mark and its handles.

## Status

Implemented on 2026-11-11. The engine reports the secondary button; the region
overlay shows the origin and size pills and the loupe during a drag or a
right-hold; the pin editor shows the loupe during a right-hold. The bar's loupe
switch leaves it on for a session and shows through its colour when a right-hold
is what raised it. Wheel zoom, the zoom plate and the two stacked readings work
in both places.

Seen in the nested session: the two pills sitting above a dragged selection's
corner with the legacy fill and radius, and the disc, its white edge and
crosshair, the guide edge, the white `8×` plate with its shadow on the disc, and
the coordinate-over-colour column following the pointer. On a blue pixel the
swatch showed `#007DCB` with black ink, its body matched the aimed pixel
exactly, and both readings measured the same width. With the disc over the
action bar the column stepped above the plate rather than disappearing under
the bar. The pin editor's path shares the same builder and was read, not
driven: the nested session has no pointer-button injection, so its right-hold
was not exercised by hand.

The loupe shows the picture in the display's SDR rendition, so a highlight above
the SDR peak reads clipped there; the saved file rolls it off into white with
BT.2390 instead, so above white the disc and the file are not the same picture.
The disc is baked on every frame it is visible, at 120 times the surface's
device ratio in texels: 120 × 120 on a 1× surface, 240 × 240 at 2×. Its cost is
part of the overlay's own frame time rather than measured separately.
