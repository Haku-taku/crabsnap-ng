# The Screenshot App

What the application built on this engine must do, the decisions its structure
rests on, and the order the work lands in. Only the first item is implemented
today; every other section states its status.

## What the mature tools agree on

| Tool | Shape | What to take | What not to |
| --- | --- | --- | --- |
| [Flameshot](https://flameshot.org/docs/dev/) | One binary; a daemon hosts the tray, hotkeys, pins and, on X11, the clipboard; subcommands talk to it over D-Bus | Pins and captures share one process | A second IPC for things the process already holds; a D-Bus surface that Wayland compositors do not need |
| [Snipaste](https://www.snipaste.com/faq.html) | Each pinned image is its own topmost window; a tray process holds hotkeys | Pins are first-class surfaces, movable and zoomable | A window per pin on Wayland, where each is a protocol object with its own buffer |
| [Spectacle](https://docs.kde.org/stable_kf6/en/spectacle/spectacle/) | Capture first, then a preview window with an annotate mode | "Capture immediately, arrange after" | The absence of pins, which its own users ask for |
| ksnip | Tabs of open images; pins as desktop widgets | Reopening a capture for more annotation | — |
| ShareX | Capture, then a pipeline of destinations | Post-capture actions as data, not code paths | Windows-only assumptions |

The converge: capture one frozen frame, let the user mark it up, then either
save, copy, or pin it. The divergence is only where pins live.

## Decisions

**D1 — One service, one connection, many surfaces.** One process owns the
connection and every surface on it: the overlay, each pin canvas, and the
settings window. What the first version of this decision got
wrong is where that process sits. A command line cannot hold pins, the clipboard
or hotkeys after it exits, so the connection lives in a daemon and commands are
clients of it; a command with no daemon reaches runs the same service locally.
The connection itself belongs to the framework's host, never to application code
([Event Loop](../engine/event-loop.md)); `WaylandLink` is a backend detail. The
command surface, the IPC contract and the capability rules are in [Command Line,
Daemon and Settings](./cli.md).

**D2 — The framework stays unaware.** No crate under `ui/crates/` learns what a
screenshot is. The app lives in `apps/` as two crates: `crabsnap-core`
(portable: selection geometry, the annotation document, stitching, the pin
model, the toolbar's `Decl` tree) and `crabsnap` (platform-bound: capture
backends, hotkey registration, surface roles, the loop). The only thing that
crosses back into the framework is the `WaylandLink` seam.

**D3 — Capture is a capability chain with honest availability.** Rungs, in
order: `ext-image-copy-capture-v1` (per output and per toplevel, shm or
dma-buf), `zwlr_screencopy_manager_v1` (outputs),
`org.freedesktop.portal.Screenshot` (whole screen, `interactive: false`, which a
desktop may still turn into a permission prompt). No rung may claim availability
it cannot honour — the legacy `wl_shm` placeholder that always said yes is the
mistake not to repeat (`apps/legacy-audit.md`). Probing happens once at startup
and on an explicit refresh, never on the frame path.

All three rungs are implemented in `apps/crabsnap/src/capture.rs` behind one
`Capture` type, which tries them in order and binds the first protocol the
compositor offers. The grab runs before any surface exists
(`WaylandLink::connect`), because a window of ours would otherwise appear in its
own screenshot. Absent globals return `None` rather than an error, so the chain
moves to the next rung. `CRABSNAP_CAPTURE_BACKEND=ext|screencopy|portal` pins
one rung, so the fallback can be exercised on a compositor that offers more than
one.

The ext rung waits for the session's constraints, attaches one shm buffer and
captures one frame. On an HDR output the session offers only 10:10:10:2 shm
formats, and the grab keeps those words: the frame carries the compositor's own
numbers and the output's description, and converting for a display or a file
happens later, per destination. The screencopy rung learns the format, size and
stride from the frame's `buffer` event, allocates at that stride, and flips rows
when `y_invert` is set, so both rungs return the same tight, top-down `Frame`.
Verified against one static nested session: 2560×1440 `Argb8888` from both
rungs, one checksum and byte-identical PPMs (sha256 `c25372ba…`).

The portal rung is the last resort: it asks `org.freedesktop.portal.Screenshot`
with `interactive: false` and waits for the request's `Response` within the
grab's deadline. The desktop takes the picture in its own process and returns a
`file://` URI to a PNG, which is decoded into the same BGRA frame, so this rung
captures the whole screen and cannot choose an output. A timeout closes the
request (`Request.Close`): the first live run, before that, left the desktop's
permission prompt on screen after the process exited. Verified live as far as
this machine allows: the request reached the desktop's permission dialog, a
`Response` arrived and decoded (`code 2` — the portal backend selected here
cannot capture a Hyprland session), and an unreachable session bus made the rung
unavailable. The desktop then answered at once, so the close-on-timeout path
itself was not re-observed with a prompt. A successful capture needs the
desktop's screenshot permission, which is the user's to grant; the two steps
after `code 0` are pinned by `a_file_uri_decodes_to_its_path` and
`a_portal_png_decodes_to_bgra_rows` instead.

**D4 — The frozen frame is a texture, not a widget tree.** The capture is
uploaded once through `AppLoop::register_image` and drawn by an `ImageSpec`;
selection, annotation preview and pins all composite over that one texture.
Re-encoding it per frame is what makes capture tools janky.

Implemented as `CRABSNAP_OVERLAY=1`: connect without a surface, capture, upload,
then open a layer surface the size of the output and draw the texture filling
it. Verified in a live session by sampling the screen while it ran — the overlay
covered the desktop, and a change made to the desktop underneath (window border
size) never appeared.

**D5 — Selection is geometry; the toolbar is a `Decl` tree.** `Selection` (rect,
eight handles, aspect constraint, clamping) is pure arithmetic with no engine
types. The overlay's toolbar and panel are ordinary engine declarations, so the
MD3 controls, tooltips and hover machinery apply unchanged; only the toolbar's
placement is arithmetic the app does itself, because a pill that follows a drag
and turns between two layouts must be positioned on the frame it is drawn in.

The drag itself is the engine's: the overlay's root declares `Sense::DRAG`, and
`cx.pressed()` against its own id says whether the press belongs to it, with
`cx.local()` giving the point in that node's space. Outside the selection is
four dim bands rather than one hole, because the paint list has no clip
subtraction. A drag shorter than 4 px is a click and clears the selection; a
pointer dragged past the edge clamps to the surface. Escape closes the overlay,
which needs the layer surface to hold the keyboard (`keyboard: true`).

Verified in a nested session by pixel sampling against a capture of the same
desktop: inside the selection the overlay's pixels equal the bare desktop
exactly, outside they are 0.57× (black at alpha 110), and the 1 px border lands
on the rect's origin. `shot region --demo` paints a fixed rectangle for exactly
this check.

While the selection was being built it was driven by scripted probes as well,
which pressed, moved and released through `AppLoop::dispatch` and printed the
rectangle the app ended up with (`Rect(768, 288, 1024, 576)` for a drag from
30%/20% to 70%/60% of a 2560×1440 output). They found that the first version
could not be dragged at all: the dimming child covered the drag root and, being
a container, was the opaque hit target, so `cx.pressed()` never named the root.
It now declares `PASS_THROUGH`; the engine rule is in [Pointer
Model](../engine/pointer-model.md#hit-testing). The probes were removed once the
path settled — a probe that cannot reach its surface is worse than none, and the
ones here reached through a downcast that the host's `Box<dyn App>` never
satisfied.

A button already held when the overlay appears is ignored — that is the hotkey's
press, and a drag starts only from a press the overlay itself saw begin.

The settled selection carries a floating toolbar
(`apps/crabsnap/src/toolbar.rs`): a pill of icon buttons that hangs off the
selection and can be dragged anywhere on the surface. The module knows no action
and no subject — a caller hands it the tools to show (`Tool`: an id, an icon, a
message id) and the rectangle to hang around — so a pin's annotation session
uses the same pill with its own table (`pin_edit`: confirm or cancel). Another
surface costs a table and a `Toolbar`, not a copy of the bar.

The bar is the topmost interactive layer of both surfaces: it is declared after
the session's chrome — a mark's box, its handles, the typing field — so a press
that lands on a button is the button's even where a mark lies beneath it. The
order is the marks image, the frame, the session's chrome, the disc, then the
bar.

Its members fall into three classes with a hairline after them: the cursor,
which draws nothing and is what a settled selection shows; the annotation tools;
and the loupe switch. The session's own buttons follow the hairline — Save,
Copy, Pin, an editor's Confirm and Cancel — so acting on the selection and
acting on the surface never share a run of icons.

The annotation tools are grouped by family, one slot each: shape (rectangle,
ellipse), line (line, arrow), and pen, with the freehand `Mark::Pen` and the
Bezier `Mark::Bezier` as its two children. Three family slots remain; a slot
shows its family's current child, a wheel notch over it turns to the next child,
and its menu lists the children by name for a pointer that would rather pick
than cycle. Text stands alone, and choosing the cursor leaves every tool
unarmed.

A tool with parameters carries them inside the pill, in three kinds. A number —
stroke width — steps with the wheel; an enumeration — an arrow's heads — turns
the same way; the paint is one round swatch whose inner disc is the fill and
whose outer ring is the stroke, over a checkerboard when the fill is
translucent. A tool that paints no interior shows a solid disc instead, so the
swatch never claims a fill it will not draw. The swatch opens a small palette:
rows of swatches with the stroke and fill rows labelled, a no-fill chip where a
fill applies, and an eyedropper that reads the frozen frame's texel under the
next press.

The block opens a lane of its own: the pill is a box of two lanes across the
bar, the buttons in one and the block in the other, so a horizontal bar gains a
second row under its buttons rather than a longer run of icons. The lane takes
the side facing away from what the bar hangs around — below a bar under the
rectangle, above one docked over it — and the other side when the screen holds
only that. It reads the edge the bar actually hangs from, not the configured
one: a bar near a screen edge has already flipped to the far side, and a block
opening toward the far side would open over the rectangle it just left.
The buttons keep their place either way; when the block is on the near side the
pill's origin moves instead, by the lane's thickness. One clock — the presence
animation on the block's own id — is both the block's opacity and its share of
the lane's thickness, so the two cannot disagree, and the pill clips its members
so a chip never paints past the bar while it opens or closes. The chips stand at
their final places from the first frame and the growing pill reveals them, so
they never squeeze as the lane opens. The block keeps the last tool's contents
while it fades, so a closing bar does not empty first, and an armed tool with no
parameters shows no lane at all. Inside the lane the chips run from the bar's
leading end along its axis and are centred across it. While a mark is being
drawn the pill fades out and comes back on release, so the hand is never drawing
through a row of controls.

Placement is the module's own rather than the anchored-overlay rule. A bar that
follows a drag, docks to four kinds of edge and turns between a horizontal and a
vertical layout has to know where it is on the frame it is drawn in. The default
is the bottom edge, centred, 12 px under the rectangle; a docked bar that does
not fit flips to the opposite edge and then clamps inside the surface, so it is
never lost off-screen. Only the grip at the pill's leading end owns a drag — a
press on a button is that button's.

A release docks to the snap zone the bar overlaps most, and while a drag is live
the zones of the edges it crosses are painted as thin bands, the one the bar
would take brighter (`zone_decl`). A band stands where its alignment actually
docks the bar: the start and end bands hug the ends of their edge and the centre
band is centred, so the band a drop lands on and the place it lands agree. A
release somewhere else lets the bar travel to its dock over 200 ms (`SNAP`)
instead of jumping there; a bar still under the hand follows the pointer with no
animation at all. The travel runs on the buttons lane's own origin rather than
on the pill's drawn corner, so a parameter lane opening mid-flight cannot pull
the buttons off their path, and the target is recomputed every frame, so a bar
that reaches a new edge while flying turns into its dock rather than arriving
with the old orientation.

Members are placed on offsets the module computes, because the engine paints no
rotational transform: the turn is each member's centre lerped between the two
layouts, with the pill box interpolated between the two sizes so it grows its
other axis as it turns. The path is straight because a straight path is what
keeps every member inside the box at every step; an arc swings the ends out
through the border, which the turn's own test
(`every_member_stays_inside_the_pill_through_the_whole_turn`) catches. The
vertical form is the one a bar docked to a left or right edge is drawn in, and
the state the turn interpolates towards — not a second widget.

No button carries text. Each is the framework's MD3 icon button, built through
`ui_widgets::Cx` with an `Env` of the app's catalog and `Md3Options`, and its
label is a tooltip that waits 600 ms of rest — a `Presence`-declared panel
anchored to the button and retargeted every frame, so one button's timer cannot
label another. The label's second line names the chord that runs the action,
spelled as the configuration spells it; it is absent for a tool with no action
and for an action the file cleared. An armed tool takes the tonal shape and an
idle one the standard shape: the standard shape tints only its glyph, and a tint
alone does not read as a selection at a glance. Both label and action are the app's strings in the
session's language.

`Save` crops the frozen frame and writes it — PNG unless the name or the
configuration asks for another format (see [the CLI](./cli.md)) — into the
pictures directory (`$XDG_PICTURES_DIR`, else `user-dirs.dirs`, else
`$HOME/Pictures`, with `Screenshots/` under it), named
`Screenshot-<output>-YYYYMMDD-HHMMSS.png` in local time. The output is the
compositor's own `wl_output.name`: two screens captured in the same second would
otherwise produce one name, and the name is reduced to `[A-Za-z0-9_-]` so a
compositor cannot steer it out of the directory. A capture that names no output
— the portal rung — keeps the shorter `Screenshot-YYYYMMDD-HHMMSS.png`. Local
time, because a file name is read by a person and UTC would file tonight's shots
under yesterday for half the world. `$CRABSNAP_SAVE_DIR` overrides the
directory, so a scripted run never writes into the user's own folder. `Copy`
puts the same PNG on the clipboard, under `image/png` and nothing else; the copy
goes through `ClipboardHost` rather than the overlay's window, because the
source must outlive the surface that made it.

A picture with light above the display's own white is written as a 16-bit PNG instead: BT.2020
primaries, PQ codes, `cICP` saying how to read them and `cLLI` carrying the measured peak.
Nothing is tone mapped there — PQ is absolute, so a 1000-nit highlight stays 1000 nits — and the
writer is the same built-in one, so HDR survives on a machine with no ffmpeg at all. A desktop
whose brightest pixel is its own white is an SDR picture and stays 8-bit. Reading such a file is
the same crate in reverse: a 16-bit PNG whose cICP says PQ arrives as scRGB light, exactly what a
capture of an HDR screen arrives as, so a pin of either behaves alike.

Encoding runs on the worker (`ui_platform::jobs`): `Save` and `Copy` hand the
settled crop to a job and read the result back in the next `view` or `on_turn`,
so the frame that starts a save is not held by it. The daemon's output hotkey
does the same for the whole-output save, which would otherwise stop pin canvases
and the overlay for as long as a full-size PNG takes.

A button that reports and does nothing is the "landed is not wired" failure in a
nicer costume, which is why `Copy` stayed disabled until this path existed.

The annotation session's bar hangs off the surface being drawn on — the
overlay's selection before it is anything else, or the pin being edited. After
the hairline the overlay shows its own four actions, Pin, Copy, Save and Cancel,
and Copy and Save bake the marks into the crop they write; the pin editor shows
Confirm and Cancel. `Confirm` closes the session keeping every mark, exactly as
the menu's Done row; while an open Bezier path holds the chord, Enter or the
path menu's Finish Path row saves that path and exits instead. Its chord is
`confirm`, `Enter`, which runs the same commit while no text field holds the
caret. The editor's `Cancel` closes it and takes the session's commits back: the
document rewinds to the depth it recorded when the session opened, and the redo
tail is dropped so a cancelled mark cannot come back through a redo. Escape is
the keyboard's way out: it takes only what is still unfinished — a popover, a
text field, node editing, a live drag — so a session
that consumes none of them lets the surface close, one press whatever the bar
holds. These exit rules cover the annotation surface's own Confirm, Cancel and
Escape: they do not keep a complex element's exclusive edit alive after its
commit. A freehand stroke leaves on release, text on save, and a closed Bezier
path on its first anchor; Escape or Discard Path drops an open preview, while
Escape still leaves a node mode opened by hand through Edit Points. An armed
tool is not a session. The armed tool and the mark selection each have a chord:
`cursor` (`V`) arms nothing, and `clear` (`Ctrl+D`) puts a selected mark down
before its second press drops the selection itself. Whether the marks
survive is decided on the bar, where it is visible: Save, Copy and Pin keep
them, Cancel and the editor's `Cancel` drop them.

A settled selection carries eight grips. Pressing one resizes: only that grip's
edges move, the opposite side stays put, a drag across it stops at the 4 px
minimum, and a grip cannot leave the surface. With no tool armed, a press inside
the settled rectangle slides the whole rectangle, which keeps its size and stops
at the surface edge; the pointer shows `Move` there. Pressing anywhere else starts a
new selection, which is also how the old one is discarded. Grips are painted by
the overlay's own shape list and hit-tested by the app, not by eight nodes:
which part of *my* rectangle was grabbed is selection geometry, and one gesture
owner keeps the drag rules in one place.

The rectangle is a marquee, not the canvas. The marks live on the whole frozen
frame, so an armed tool draws wherever the press lands — inside the rectangle or
outside it, which is dimmed but still annotatable — and a new selection keeps
what was drawn; the crop is cut only when a file, the clipboard or a pin asks
for it.

The selection's gesture reads the picture's own node, not only the root. The
picture fills the surface, so a press on it belongs to that node and never
reaches the root; the overlay accepts either id as "the press is on the
picture", and a press on the bar, a grip or a mark's box belongs to that
control instead.

The middle button slides that rectangle even with a tool armed, where a primary
press inside it belongs to the annotation canvas. The root declares
`Sense::MIDDLE`; a press inside the settled rectangle records the grab and the
rectangle as it was, each following frame slides it by the pointer's
displacement clamped to the surface, and the release ends it. A press that began
outside never grabs a rectangle later, and while the middle button drags, the
primary press is not read.

Every action the bar carries comes from the schema rather than from a literal:
`crabsnap_core::hotkeys` names the actions and their defaults, and the app's
`hotkeys::Plan` resolves the file into one chord table per surface. A press on a
button and its chord run the same `shortcut()`, so a rebind cannot leave the two
disagreeing. `Ctrl+D` — or whatever `clear` names — drops the selection, the
marks and the bar, and returns the overlay to choosing a rectangle. The cursor
keys step the real pointer by one pixel of the picture it is over: `1 / scale`
points on the overlay, whose capture is drawn one pixel per device pixel, and
`zoom / dots` on a pin canvas, whose picture is the pin's own. Shift takes ten
pixels instead of one: the plan leaves `Shift` free on every direction, so one
entry answers both presses, and each press banks its own count of fine steps
from the modifiers it resolved with. The build that applies them only measures
what one step is on the picture under the pointer, so the steps are banked into
one warp per frame with their sizes already settled; a direction bound to two
chords, as every default direction is, names the first in its hint. A pixel of
the picture is a pixel of the screen exactly at native zoom, so that is where
the step lands on the output's grid; `hyprctl cursorpos` shows the step in
points around a press.

**Pinning draws many pins on one canvas.** `Pin` cuts the settled selection and
closes the overlay. Every pin of one output lives on a single canvas: a layer
surface the size of that output, placed at its origin and never moved again,
holding each pin as a node. The canvas root is `PassThrough`, so the surface's
input region — derived from the hit index — is exactly the pins' rectangles and
a click between pins reaches the desktop. Each pin registers its own texture and
draws an image node at its position with a pass-through ring above it (a box
paints its fill and border before its image, so the ring cannot live on the
picture's own box). The corner radius and the shadow come from the `[pin]` keys
in the config ([Configuration](./cli.md#configuration-and-hot-reload)); the
radius is stated on the box shape itself, because that shape's curve is what the
picture is sampled through — stating it as a node parameter rounds the clip and
leaves the pixels square. The canvas follows the pointer with its keyboard: it
holds the keyboard while the pointer is on a pin or a drag is live, and asks for
none as soon as the pointer is between pins, so typing there belongs to the
window below. Escape closes the pin under the pointer ([Pointer
Model](../engine/pointer-model.md#keyboard-is-not-the-input-region)). Several
pins are one session; the surface is torn down when the last pin closes.

**Dragging a pin is one absolute measurement.** The canvas never moves, so
pointer coordinates on it are screen coordinates and a drag is `position at
press + (pointer - press_origin)`, using `Cx::press_origin()`. Nothing is
accumulated, so a frame that carries no new input cannot move anything. The
first implementation gave every pin a surface of its own and moved it by setting
a layer margin. That cannot work: the compositor measures a `wl_pointer` sample
against the surface *as it was when the event was sent*, so once the surface
moved, the app combined a stale sample with a new position and committed another
move — the pin fed its own motion back into the next frame and accelerated away
while the hand was still. A real drag showed `applied 9245` while the pointer
was at `1235` on a 2560-wide output, and the samples stayed non-physical from
there on.

**Zooming and closing.** A wheel notch over a pin zooms it by 1.1, and the
content point under the pointer stays where it is: a wheel is a request to see
more of *that* point. A pin is `position + zoom`; `zoomed` moves the position by
the ratio that actually applied after clamping, because at a limit the requested
factor and the effective one differ and moving by the request pushes the content
out from under the pointer. The limits follow the content's own size: its long
side is drawn between 32 and 16384 logical points, inside the shared 0.05–32
envelope. The output does not enter, so a picture larger than the screen can
still be zoomed in and a pin may hang off the edge of its output. Nothing is
asked of the compositor — the canvas is already that large.

**The right-click menu is the engine's.** The picture node declares three rows
with `Decl::menu`, labelled through the catalog's `pin-menu-*` keys; a secondary
click opens the engine's panel and the pick arrives as `Change::MenuItem` at the
picture node. `Copy image` builds the same PNG as the bar's Copy and hands it to
the same `ClipboardHost`, `Save image` writes through `save_shot`, and `Close`
sets the pin's own flag, which `done()` reads. Nothing about the menu is app
geometry: placement, clamping, hover and the entrance all belong to
`AppLoop::menu`.

The rows print the chords the plan resolved — `Ctrl+C` on Copy, `Ctrl+S` on
Save, and `Enter` on Done, which is `confirm`, the same action the editor's
check mark runs. A row carries the chord as a `KeyChord` as well as a spelling,
so pressing it picks that row; an action the file cleared prints nothing and
still runs from the pointer.

Those chords also work with the menu shut: `copy` and `save` are bound on the
pin surface, where they act on the pin the keyboard aims at. A text field that
holds the caret keeps the text's own commands — while a caret is open the
engine answers `Ctrl+C`, `Ctrl+Z` and their kin with its built-in editing
actions before any app binding — so annotating a pin's text never copies the
picture instead.

The look is the app's: the canvas answers
`App::menu_style` with MD3's style resolved in the scope its controls build
under, and `toolbar::env` sets `Md3Options.menu_blur` to 16 points, which the
engine turns into a frosted surface tinted at 80% — the picture under the menu
stays visible, blurred.

Two engine facts came out of wiring it. The menu has no node, so it was outside
the input region derived from the hit index and a row reaching past the picture
was a painted row the compositor handed to the desktop ([Pointer
Model](../engine/pointer-model.md#corrections)). And the rows are hit-tested
where they are drawn during the 167 ms entrance, so a probe that presses at once
picks a neighbour ([Popovers](../engine/popovers.md)). Verified in a nested
session while the probe existed: each row's action ran (`copy`, a 1224x676 PNG
on the clipboard; `save`, a file that reads back as a 1224x676 RGBA PNG with
244,116 distinct colours; `close`, the pin gone), and stopping the probe on the
open panel left a screenshot of the dark rounded panel over the picture with
`复制图片 / 保存图片 / 关闭` in the session's own language, the rows at rest.

**A pin sits exactly where its position says.** On the canvas there is no
padding and no margin to translate: the picture's corner is its position,
because the surface starts at the output's origin. The drag clamps the position
onto the output's top-left; no padding arithmetic exists to get wrong.

**Points, pixels, and the grid a pin lands on.** The engine lays out in logical
points and the capture arrives in device pixels; the two are related by the
surface's own scale, read from the frame (`Cx::scale()`) rather than assumed
from the output. A selection is converted to picture pixels exactly once, in
`Overlay::cropped`, so the pointer, the bar and the marks stay in points and
only the crop sees pixels. A pin is placed in points: its origin is the crop's
own corner divided by that scale, so the corner the rectangle was rounded to is
where the pixels go. A pin draws one content pixel per device pixel —
`display_size` divides by the canvas's own scale — and at native zoom `draw_at`
snaps the origin to the device grid. Both halves matter: the sampler maps the
destination box onto the texture's edges, so a fractional origin, or a
destination that is not the texel count, blends every texel with its neighbour.
A zoomed pin keeps the pointer's own sub-pixel position, because a non-integer
scale resamples whatever the origin.

**Every picture remembers what a point was worth.** A frame carries the output's
scale, a crop carries it on, and the file states it in the field its container
defines: `pHYs` in PNG, the JFIF APP0 density in JPEG, `biXPelsPerMeter` in BMP,
tags 282/283 in TIFF. A logical point is 96 dpi by the web's own convention, so
a 2× capture says 192 — exactly the scale it was taken at, in a field every
viewer reads. WebP, AVIF and JXL would need an EXIF or metadata item for it, and
GIF and QOI have no density field at all, so those five are written without.
`pin add` reads `pHYs` back into the same field. Layout never consults it: a pin
is always one content pixel per device pixel; the density is what another
application needs to show the file at the size the screen gave it.

The whole path is verified by the same scripted run: after the drag it reads the
save button's rect, clicks its centre, and prints the file it made —
`/tmp/crabsnap-shots/Screenshot-WAYLAND-1-20261109-172932.png 1024x576` for the
1024x576 selection above, read back as a 1024x576 RGBA PNG with 180,801 distinct
colours. It then grabs the north-east grip and pulls it by (200, -100): the
settled rectangle becomes `Rect(768, 188, 1224, 676)`, which is exactly those
two edges moving. A press at its centre dragged by (-100, -60) moves it to
`Rect(668, 128, 1224, 676)` — the size unchanged — and the pin it asks for
afterwards is cut at that moved rectangle. Finally it hovers a grip, an edge and
the middle and prints the frame's resolved cursor — `ResizeDiagonalUpRight`,
`ResizeVertical`, `Move`.

The pin was driven by scripted probes too while it was being built, against the
canvas session rather than a surface of the pin's own: a press at the picture's
centre, travel in four steps, a hold of five frames before release. The hold was
the measurement that mattered — any frame that adds motion without input shows
up as the position leaving where the press and the travel put it — and it never
moved after the canvas replaced the window-per-pin design (see
[Corrections](#corrections)). Nothing in those runs asked the compositor for a
new size or position, so the wire carries no `set_size` or `set_margin` after
the canvas is mapped. They were removed with the rest of the scripted probes.

**D6 — Annotation is a document of inverse edits.** Keep the legacy model:
`EditOp` with an undo/redo stack centralised in the document, tools describing
intent and the manager committing (`apps/legacy-audit.md`, `annotate/document`).
Tools emit engine draw primitives; they do not call chrome.

The tool set has three families: shapes share one slot, lines and arrows
another, and the pen family holds the freehand `Mark::Pen` and the Bezier
`Mark::Bezier` in the third. Text stands alone. A Bezier path is one mark and
one history step; `Document::draw` rasterizes it into an `ImageEntry` and draws
it as an `ImagePrim`, so the canvas, the loupe, a saved file and a pin read one
list.

The document is `apps/crabsnap/src/document.rs`: `Document` holds marks in paint
order and the two history stacks; an `Edit` carries the mark before and after
the change, so undo is redo with the two swapped and a move or restyle needs no
new history code. A mark's geometry is in the *frozen image's pixels*;
`Document::shapes(scale, offset)` is the only place that maps them onto a
surface, which keeps a mark on the content it was drawn over at another scale,
and scales its stroke width with it. The eraser stores no mark: it commits a
removal, and undo puts the mark back at its recorded index rather than on top.

Marks are created by an `edit::Session` (`apps/crabsnap/src/pin/edit.rs`), and
both surfaces run one: the pin's editor and the region overlay each hold a
session beside their document. The session owns the armed tool, the selected
mark, a drag in flight, the tool's parameters and the loupe; what differs is one
flag — a pin has a band to move the pin by, an overlay selection has none, so
there `frame` is off and a press inside the selection is the tool's.

The overlay annotating a frame is what makes a pin optional. A mark is drawn on
the frozen frame's own pixels — the whole screen rather than the selection on
it — and `Pin` hands the document to the pin (`Bitmap::with_marks`) inside a
grid translated to the crop's corner instead of rasterizing it, so the pin can
reopen its editor on the same marks. A new selection keeps the marks and only
changes what leaves with them. Copy and Save bake through
`pin::content::baked_in` with the crop's corner as the offset, and a pin
previews through the same `painted` list, so no two paths can disagree about a
pixel.

A mark's pixel grid is the frozen frame's. `Space::new` places the drawing box
over the whole surface and states the picture's pixel size beside it, so a mark
lands on the pixel it was drawn over wherever on the screen it is drawn;
nothing else rounds a coordinate. The selection rectangle stays fractional for
the pointer and for the chrome, and `Image::crop_of(rect, scale)` yields the
crop's device-pixel edges only when a file, a clipboard or a pin asks for them.
A pin, whose picture *is* that crop, takes its corner as its position, so the
pixels land where the rectangle was.

The overlay's marks are one [canvas](../engine/canvas.md) over the frame, and
nothing of the picture is in it: the frozen texture is painted once, so the
inside of a selection and the outside are one picture by construction, and an
HDR frame keeps its values under the marks instead of having its selection
redrawn in the canvas's SDR list. A frame with no marks declares no canvas at
all. The marks enter a file the same way a pin paints them — the picture, then
the document at the offset the crop asks for — so the preview and the file
cannot disagree about a pixel. Both presenters paint canvases; the one list the
GPU pass refuses is a canvas that carries a layer effect, which has no canvas
pass.

A stroke in flight is a preview in the document, not a second drawing of the
same mark in the surface's chrome. Every reader of a mark — the canvas, the
loupe's window, a file — therefore sees the hand's work before the release, and
the geometry a drag shows is the geometry it commits, since one function builds
the mark for both.

A text mark is one more reader of that rule. Its frame is derived, never dragged
loose: the shaped text plus the padding, or the width a side grip pulled it out
to, whichever is larger (`TextMark::frame`). A hugged frame paints with no wrap
width at all, the freedom it was measured with; only a pulled-out one breaks
lines, at its own interior. The alignment therefore has slack only when the
frame is wider than its text, which is what the side grips are for: they set the
width and leave the type its size, while a corner maps the whole mark. A live
field draws the mark no copy of it: the canvas paints the frame, the wash and
the paragraph, and the field is a transparent keyboard stop over the same box
that adds the caret and the selection. Editing and the exported file are one
rendering because they are one list.

A selection that carries marks and is not already 8-bit sRGB is converted to the
SDR rendition a file gets (`Cropped::sdr`) before the marks go on: that rendition
is what every file format here writes anyway, so the alternative was a file
missing the marks. A bare selection leaves native, and the worker that writes it
converts once. A pin still refuses an HDR picture with marks when it exports,
because a flatten has no linear target; its measurement runs first, so the
refusal is read from the picture rather than from its tagging.

**D7 — Pins are one canvas per output.** All pins of an output live in a single
layer surface the size of that output, placed once at its origin. The earlier
design gave each pin a surface of its own and moved it by setting the layer
margin; that was rejected here after a real drag: a client that moves the
surface receiving its own pointer input can never know which placement the
compositor used to measure the next sample, so the pin's motion feeds back into
the next frame and it drifts away from the hand. A canvas removes the question —
the surface is the screen, the pointer's coordinates are screen coordinates, and
a drag is `press_origin` plus travel. The full-output surface was first rejected
because an empty input region could not be expressed; the engine now derives the
region from the hit index, so a `PassThrough` canvas with opaque pins publishes
exactly the pins' rectangles.

**D8 — Long shot is portable and injected.** The stitch algorithm moves across
almost unchanged: it already takes grab and scroll as closures and time from an
injected clock, so its tests run without a compositor.

**D9 — Settings are one schema, one writer.** A single TOML schema owned by
`crabsnap-core`'s settings types; the overlay, the settings window and the CLI
all read and write through it. The legacy three-writer split, and the `"H.264"`
versus `"h264"` bug it caused, are the reason.

**D10 — A pin is content, not a picture.** The pin keeps its placement, stacking
and zoom, and gives up owning pixels: it holds a `PinContent` — bitmap, vector,
text, Markdown, code, formula or palette — and the canvas asks it for a size, a
node, a menu and an editor. Sources arrive as paths or as a file list from a
selection, one pin per item, classified by their bytes rather than their names.
The contract and its phases are in [Pins](./pins.md).

## Surface lifecycle

```text
hotkey ─► overlay surface (one per output, frozen frame + selection)
           │ confirm
           ├─► clipboard / file
           └─► pin canvas (one per output, pins composited)
                 └─ last pin closed ─► surface torn down
```

The overlay is a layer surface with keyboard focus, created on demand and
destroyed on confirm or cancel; it is not kept mapped, because a fullscreen
always-on-top surface costs a compositor blend every frame. The pin canvas uses
the same layer role, sized to the output, anchored top-left with an empty margin
and `exclusive_zone = -1`, so its coordinates are the screen's. The settings
window is the one normal toplevel.

An action that leaves no other trace — a copy, a save — answers with the MD3
snackbar at the surface's bottom: at most 344 points wide, centred, 24 above the
edge, there for two seconds. A failure says so on the bar and keeps its reason
on the terminal. The bar's life is one engine animation: a landing puts it at
full, a delayed departure holds that value and then drops it, and the control's
own fade follows, so no surface reads a clock or keeps a timer of its own. The
arrival is the engine's spring, parked below the surface's edge when a showing
starts: the bar rises into place, overshoots it once and settles, so it enters
the way the scroll code arrives rather than appearing on the line.
Checked in a nested session on both surfaces: `Ctrl+S` and `Ctrl+C`, on a
selection and on a pin, each raised the localized line centred at x = 1280 of
2560 pixels and 25 above the bottom, and no bar pixels remained five seconds
later.

## What the engine still needs

| Need | Why | Status |
| --- | --- | --- |
| `WaylandWindow` on an existing link | D1: many surfaces, one connection | Done (`WaylandLink::surface` from a `SurfaceSpec`) |
| Several windows in one process | D1 | Done: one `Session` per surface, `Session::open_on` |
| Pointer constraint or a hidden-cursor overlay during selection | The selection must not show a live cursor over the frozen frame | Needs a protocol binding through `WaylandLink` |
| A texture upload path from captured `Pixels` | D4 | `register_image` exists; the upload format contract is unverified |
| Image clipboard write | Copy a capture without an external `wl-copy` | Done: `ClipboardContent::Bytes` under `image/png`, served by `ClipboardHost`; a surface-less copy goes through the compositor's data-control channel, which needs no input serial |
| A surface input region | A pin's transparent padding used to swallow clicks meant for the desktop under it | Done: the engine derives the region from the hit index and publishes it on layer surfaces ([Pointer model](../engine/pointer-model.md#the-surfaces-input-shape)); a `PassThrough` root with opaque pins publishes exactly the pins' rectangles, which is what the pin canvas is built on |
| The fixed end of a drag | Moving a surface with its own pointer input feeds back and drifts; an object moves inside a still surface instead | Done: `Cx::press_origin()`, the press position in surface coordinates, fixed for the gesture |
| A line and an ellipse shape | Annotation arrows and ellipse tools cannot be approximated by rects | Done: `Shape::Line` declares the scene's stroked segment, and `Shape::Ellipse` a filled and/or stroked ellipse; the CPU rasteriser and a GPU shader both derive coverage from one distance estimate, held to 3 LSB by the dual-backend scene check |
| A rendition for 10-bit captures | An HDR output's shm capture is PQ-encoded BT.2020, so truncating it to 8 bits is neither sRGB nor the desktop | Done where the pixels are needed, not at ingest: `wp_color_manager_v1` states the output's primaries, transfer function and peak, `color.rs` reads it into a `ContentColour`, and `Image`/`Cropped` keep the native words. A file gets `Cropped::rendition`: decode, convert the stated primaries into sRGB, roll what is above the file's white off with BT.2390 bounded by the picture's measured peak, encode. Both transfer functions are tabulated — 70 ms for a 2560×1440 frame against 348 ms untabulated, measured in release on this machine; a picture above the file's white also runs the curve per texel, which is not tabulated and is measured on the save worker rather than on the frame. Measured against an independent implementation of the same steps: within 1 LSB, two at the very bottom of the curve, except the most saturated primaries, whose cross-channel leakage is the gamut conversion itself |
| Surfaces that match an HDR output | An SDR surface on an HDR output is mapped by the compositor with a curve the app cannot see, so its own colours are not the ones the display receives | Done: `GpuWindow::open` asks `WaylandWindow::declare_hdr_surface`, which reads the output's luminances and transfer function, declares the output's own description, and presents f16 encoded to PQ at that output's reference white and gamut; `SurfaceColour::Sdr` opts out. A pinned chart captured from the output returns to its own colours, the same 1-LSB agreement as the tone-map row |
| Offscreen canvas on the GPU | Annotation baking and caching must not round-trip through CPU memory | Not started; render into an `OffscreenFbo`, read back only when exporting |
| Zero-copy dma-buf capture | A whole output crosses the CPU once per grab through shm | Not started; this compositor offers `zwp_linux_dmabuf_v1` v5, so the capture side is available, but the engine's image source is `Arc<Vec<u8>>` — it needs a texture source imported from a dma-buf, and saving still needs a mapped read of the same buffer |

## Work order

0. Framework host and command surface (D1). ~~The portable loop and the backend
   seam.~~ Done: `Host`, `Backend` and `SurfaceHandle` in `ui-platform`,
   `WaylandBackend` in `ui-platform-wayland`, and the toolbar path runs on
   `Host::run_until`. The command tree in [Command Line, Daemon and
   Settings](./cli.md) exists for `shot`, `info` and `completions`; the daemon,
   the pins behind it and the remaining targets are the current work. From here
   on every step is exercised through a command, not an environment variable.
1. ~~`WaylandWindow` and `Session` on a shared `WaylandLink`.~~ Done; verified
   by `examples/two_windows.rs` in a live session.
2. ~~`apps/crabsnap` skeleton: one connection, the toolbar surface.~~ Done:
   `apps/crabsnap` opens a layer-surface toolbar and generates its strings from
   its own catalog.
3. ~~Capture chain D3 with the ext-copy rung first.~~ All three rungs are in; a
   successful portal capture needs a desktop that grants it.
4. ~~Frozen-frame texture (D4).~~ Done.
5. ~~Selection overlay (D5): drag, dimming, border, Escape, action bar, eight
   grips.~~ Done; a drag inside a settled selection moves it. The origin and
   size readout and the loupe came after it: [Selection Readouts and the
   Loupe](./magnifier.md).
6. ~~Annotation document and the tool set (D6).~~ Done: the document and its
   undo/redo (`document.rs`); the tools — rectangle, ellipse, line, arrow,
   freehand pen, Bezier pen, text — with their parameters and colour picker on
   the bar; and one session the overlay and the pin share, so a selection is
   annotated before it is pinned. The tool-drive path was checked by unit tests
   and a build, and the marks' placement by a probe that printed the mark's
   bounds; no pointer could be injected into the nested session to press a tool
   button by hand.
7. ~~Pins (D7).~~ Done: one canvas per output holds every pin as a node; a pin
   is drawn where its position says, drags by `press_origin` plus travel, zooms
   about the pointer with the wheel, and its right-click menu copies, saves or
   closes it. The keyboard follows the pointer between pins, Escape closes the
   pin under it, and the surface goes when the last pin does.
8. Long shot (D8) and settings (D9).

Each step lands with its own verification: the portable pieces by unit tests,
the surfaces in a nested session, and the capture path against a real
compositor.

The GPU path is the target; the CPU presenter stays as the reference path for
tests and as the fallback for machines without a GPU context, and it does not
gate any feature.

## Status

The connection seam and the app skeleton exist:
[`WaylandLink`](../engine/window.md#application-protocols-share-the-connection)
with secondary queues and several surfaces on one link, and `apps/crabsnap`,
which draws a layer-surface toolbar from its own Fluent catalog. Both were
verified in a live session. Capture works for one output at a time, and the
frozen frame is a texture on a full-output layer surface carrying a drag
selection, its dimming, Escape, eight resize grips and a floating toolbar whose
Save writes a crop, whose Copy puts the PNG on the clipboard and whose Pin puts
a copy on the output's canvas where the selection was; a settled selection
slides by dragging inside it, and on the canvas a pin drags without drift, zooms
about the pointer and closes with Escape or its menu. The same toolbar serves
annotation on both surfaces — a selection's own session and a pin's editor —
with the cursor, the three tool families (the pen family holds freehand and
Bezier), text, the loupe switch and the tool's parameters; a mark drawn on a
selection travels into the pin as the pin's document, and copy and save bake it.
A copy or a save answers with a bar at the surface's bottom, on both surfaces,
because neither leaves anything else on screen. Long shot and the GPU canvas
pass are still design.

## Corrections

**The editor's frame painted over the picture instead of around it.** The first
frame was a border on the card, and a border paints inward: the line covered the
picture's outermost pixels, and the card's box is the pin's laid-out box rather
than the picture's drawn edges, so the frame did not line up with the selection
state's either. Both surfaces now draw it the same way — one shape on the
session's chrome, whose rectangle is the picture's own edges from the same
`Space` the marks use, grown by the stroke width so the whole line lies in the
band outside the picture. Chrome is not part of a canvas, so no export carries
the line. What catches it now: the selection state and the edit state draw one
frame by one rule, and a picture keeps every pixel it has.

**The overlay's marks were going to be one canvas with the frozen frame.** D6
said the picture and its marks are one target sized in image pixels, which is
what a pin does. An overlay cannot: the frozen frame is whatever the output
produced — PQ or HLG on an HDR screen — and a canvas's list is SDR, so drawing
the frame into it would change every pixel of a picture carrying no mark at all.
The overlay paints its marks onto a transparent canvas and draws it as a
texture over the frame: the two never have to agree about a colour space, an
unmarked frame pays for no second picture, and nothing is re-encoded. The
canvas is the whole frame's, not the crop's, since a mark may be drawn anywhere
on a screen that is the canvas. Both surfaces reach the export through
`pin::content::baked_in`, so a pin's preview and the file it writes still agree.

**The tools lived in the editor's chrome, and the bar only confirmed.** The pin
editor came first, so its tools sat in a palette on the picture and its bar
carried Confirm and Cancel, while the selection's bar had Save, Copy and Pin and
no tools at all — which is why annotating meant pinning first. The tools, their
parameters and the loupe switch moved into the bar, and the chrome kept only
what surrounds a mark: the frame, the selection box, the handles, the typing
field. Both surfaces build one `State`, and a session holds no palette of its
own to drift from it.

**A pin's surface moved in response to its own pointer input.** The first pin
implementation was a window per pin, so a drag moved the window: the app took
the press's surface-local position, added the pointer's travel to the surface's
placement and committed a new layer margin. A real drag with
`CRABSNAP_PIN_TRACE=1` showed what that does: on a 2560-wide output the
pointer's own reported x ran from 6565 to 35979, the surface's origin tracked it
at a constant offset, and the "applied" origin ended at 9245.8 while the pointer
was at 1235.1. The compositor measures each `wl_pointer` sample against the
surface position *at the time the event was sent*; the app then added that
sample to the placement it had just committed, so every frame folded the
previous frame's move into the next measurement. A first attempt discarded the
first sample after each move; that is a guard, not a fix — it leaves the same
feedback one frame late and cannot be proven correct. The root fix is D7: the
objects live in one surface that never moves. The engine gained
`Cx::press_origin()`, so a drag is an absolute `position at press + (pointer -
press_origin)` rather than a sum of moves. What catches it now: the rule in
[Pointer model](../engine/pointer-model.md#coordinates), the drag probe's five
frames held still before release (any motion there is a frame adding to the
measurement), and the absence of any surface geometry request during a drag.

**One process meant no IPC, until commands existed.** D1 first argued that
Flameshot's daemon was unnecessary because the capture process never exits. That
holds only while the tool is one long-running program: a command such as `shot
region` or `pin list` is a short-lived client, and pins, the clipboard and
hotkeys must outlive it. The reading was also wrong about the framework: the
connection was opened by application code, so every new front end would have had
to own its own copy of the platform half. The fix is a service layer that owns
the connection, a daemon that hosts it, and commands that are clients of it —
the shape in [Command Line, Daemon and Settings](./cli.md). The first version's
second mistake is recorded in [Event Loop](../engine/event-loop.md): the
application drove every surface's blocking wait itself.

**The BT.2020 matrix was written transposed.** The first version of the gamut
conversion carried the published BT.2020 → BT.709 matrix with its rows and
columns swapped. Its own test passed, because the table it compared against had
been written from the same wrong memory. Building the matrix from the primaries
exposed it immediately: a colour-space conversion's rows must each sum to one
(white is the same white in both spaces), and the transposed rows summed to
1.52, 0.44 and 1.04. The matrix is now derived rather than typed, and the test
checks the derived result against the published table *and* holds the
rows-sum-to-one property.

**The output states measured primaries, not a named set.** `wp_color_manager_v1`
can name the primaries, but this display sends coordinates instead (0.6865,
0.3086 …), which match neither BT.2020 nor P3 exactly. Trusting the nearest name
would convert by a set the panel does not have, so the matrix is built from
whatever coordinates arrive: solve the primary scales from the stated white
point, apply a Bradford adaptation when that white is not D65, then compose with
XYZ → sRGB. A named set is only the fallback for a compositor that sends nothing
else, and an ICC-only description (which needs a colour-management library to
interpret) leaves the frame repacked.

**A pin drew its frame, ring and shadow but never its picture.** The local `pin
add` composed the texture id itself: it set `PIN_TEXTURE_BASE` on the pin before
handing it to the board and registered the image under the same constant. The
board numbers every pin (`ids + 1`) and derives its texture from that number, so
the first pin drew under `PIN_TEXTURE_BASE + 1` while the surface held the image
under `PIN_TEXTURE_BASE`: the build found no texture and painted the box around
whatever was beneath it. A 64x64 red PNG in a nested session showed a
white-edged hole in the middle of the screen, and the same command drew the red
square as soon as the entry carried the id the board had assigned. The first
reading — "the upload path is broken" — was wrong, and the correct reading took
the board's numbering seriously: one place composes a texture id, and every
caller asks it (`pins::entry`) instead of spelling the base itself. What catches
it now: a local `pin add`, whose entire purpose is to show the picture.

**A pin was placed at the selection's fractional origin.** The overlay cut its
pixels out on the pixel grid (floor/ceil) but told the pin to sit at the raw
pointer rectangle's origin. That is wrong twice: the position could be up to a
pixel away from the pixels it held, and the fraction survived into the quad,
where the sampler maps the box onto the texture's edges and therefore blended
every texel with its neighbour. A 1-px checkerboard pinned at integer
coordinates comes back as exact 255/0 alternation; the same picture at a
half-pixel origin does not. The pin now takes the crop's own corner, `draw_at`
snaps to the device grid at native zoom, and the point/pixel conversion happens
once at the boundary — `Overlay::cropped` and `Pin::dots` — instead of being
assumed away because this output's scale happens to be 1. What catches it now:
`a_native_pin_lands_on_the_device_grid`, and the checkerboard check.

**The parameters lived outside the pill, then on the buttons' own line.** The
row was first declared beside the bar and positioned from the pill's rect, so
its geometry and the bar's were two independent answers to the same question:
the row appeared and vanished without the bar changing size, and the bar's own
fade — the animator that hides it while a mark is being drawn — could not reach
it. Moving the block into the pill's layout fixed that but put it in the
buttons' run, where a horizontal bar grew longer and the chips stood where the
next icon would. The bar is now two lanes across its axis: the buttons in one,
the block in the other, so arming a tool makes the bar thicker and never longer,
and the block fades as one container. The user's report is what separated the
two mistakes — a block inside the pill but on the buttons' line still reads as
"appended to the buttons", not as a second row. What catches it now:
`the_block_takes_a_lane_of_its_own`, which fails if any button's band crosses
the strip's, and
`every_member_stays_inside_the_pill_through_the_whole_turn`, which now runs with
the lane on either side, closed and open. The lane's side first came from the
configured edge, so a bar that had flipped to the far edge — a selection running
to the screen's bottom — opened its block back over the selection; the bar reads
the edge it actually hangs from now, which
`the_block_opens_away_from_the_area_even_when_the_bar_flips` pins.

**The annotation's boxes took the presses meant for the bar.** The engine paints
and hit-tests one tree in declaration order, so a sibling declared later owns
the pixel it covers. The overlay built the session's chrome — each mark's drag
box, the eight handles, the typing field — after the bar, which put every one of
them above it: with the bar inside the selection, a press on a button lying over
a mark started a mark drag, and the handle under a docked bar took the press
meant for the bar. The chrome is now declared before the disc and the bar, which
is the order the pin editor already had (picture, chrome, badges, bar), so both
surfaces answer one rule. What catches it now: only the declaration order in
`main.rs`'s two `view`s, each carrying a note saying why the bar is last; no
test pins the z-order of two sibling declarations.

**The snackbar hung at the page's left edge instead of its centre.** The control
described its `overlay` slot as "horizontally centred by the auto sides" and
gave the node `left: auto; right: auto; bottom: 24`. The layout engine leaves an
out-of-flow box with neither horizontal side declared at its static origin, so
every floating bar drew flush left: the app's first notice sat at x = 0 rather
than the centre of the surface. The slot now declares both sides as 0 and
centres with a pair of auto margins, the one shape `solve_oof_axis` splits
evenly, so any caller of the control gets the placement the doc promised. What
catches it now: `a_floating_bar_hangs_centred_above_the_page_bottom` in the md3
tests reads the laid-out box of both the shown and the faded declaration.

**Escape on the overlay took the tool and the mark before the capture.** The
key walked the session's layers and then its own, so an armed tool, or a mark,
made leaving the surface take two or three presses — the chord meant "leave"
read as "step back". It now takes only what is unfinished (a popover, a text
field, a live drag) and cancels at once; the cursor tool has `V`, and `clear`
puts a selected mark down before its second `Ctrl+D` drops the selection, so
the two presses are the stated cost of one chord doing both. The session's own
contract and its tests live in [the pin editor](./pins.md).

**The overlay's canvas painted the picture again inside the selection.** The
marks canvas had grown to carry the frozen crop as well, so laying down the
first mark changed what the inside of the rectangle *was*: the crop is rounded
out to whole picture pixels and drawn into a rectangle that keeps its fractional
edges, and a canvas's list is SDR, so an annotated selection could sit a
fraction of a pixel off the frame around it and lose the frame's colour
rendition. The canvas now holds the marks alone — transparent where the frame
shows through — so the frozen texture is painted once and the two cannot
disagree. The crop also moved to where it belongs, the export: `Copy`, `Save`
and `Pin` cut it, the document lives on the whole frozen frame, an armed tool
draws anywhere on that frame, and a new selection keeps the marks instead of
replacing the document. What catches it now:
`an_offset_bake_moves_the_marks_into_the_crop`,
`a_translated_document_moves_its_marks_and_its_history`, and a nested-session
frame whose mark crosses the selection's edge, saved and compared with the
rectangle.

**The stroke being drawn was drawn twice, and the loupe only knew one copy.**
The session painted a live drag as chrome shapes in surface points, so the mark
existed nowhere but the screen until the release put it in the document: the
loupe rasterizes its window from the document, so a disc held up while drawing
showed the picture under the hand and none of the ink, and the export would
have missed it too. The drag now inserts a preview mark into the document and
updates it each frame, one `drawing_mark` builds the geometry for both the
preview and the commit, and the chrome copy is gone. What catches it now: a
nested session with the loupe switched on, a rectangle and a pen stroke
screenshotted mid-drag with the ink inside the disc, and undo/redo of the
committed mark through `Ctrl+Z` and `Ctrl+Shift+Z`.

**The selection's gesture listened for the root alone.** It read `cx.pressed() ==
root`, which worked while the picture's node covered only the settled rectangle:
outside it the root was the topmost consumer, so a press there began a new
selection. Once the picture's node spanned the surface it became the topmost
consumer everywhere, and every press went to it, so a drag neither began,
slid nor resized anything — a virtual pointer across the frame left the old
rectangle untouched. The gesture now accepts the picture's node as the
rectangle's own hand, and a press the bar, a grip or a mark's box consumed stays
with that control. What catches it now: a nested-session drag of a new
rectangle outside the old one, a slide from inside it, and a resize by one of
its grips.

**A parameter chip took the press and answered nothing.** The chips were built
as `ui_platform::Container`s. A container draws, takes the press and reports no
change — that is its contract — so arming the text tool showed seven chips and
none of them opened a picker; only the MD3 icon buttons, which build their own
change, answered. The chips are now one `Chip` element that reports `Activated`
on `WidgetState::activation()`, the press the engine delivers to a control that
speaks, and carries a declaration cache key built from its own declaration so
the bar's cached entries stay honest. What catches it now: a nested session
right-clicking the align chip, whose menu opens and whose row turns the text.

**Changing a text mark's colour replaced what was being typed.** Restyling
applied the bar's template mark wholesale, and the template's text is empty by
definition: the field kept its buffer while the document's mark — the one the
canvas paints — became blank, so the words vanished on the next frame. The mark
now takes only the template's dressing (`TextMark::dressed_as`): size, padding,
corner, alignment and the three inks, while the words, the place and a
pulled-out width stay the mark's own. What catches it now: typing a word, then
changing its outline colour, keeps the word and gives it the halo.

**The typing field painted a second copy of its mark.** The field drew the
frame, the wash and the paragraph itself while the canvas skipped that mark, so
editing and the saved picture were two independent renderings of one thing.
They drifted, and a hugged frame broke a line the measurement had kept whole:
paint was offered the measured width, and shaping rounding a hair past it folded
the last letter onto a second line with no newline typed. The document is now
the only painter. A keystroke is an ordinary preview (`Document::preview`, which
moves the revision, so the canvas rebakes), and the field is a transparent
keyboard stop that adds the caret and the selection and nothing else. What
catches it now: typing a word and seeing no fold, a second line entered with
Shift+Enter, and the field's box sitting on the canvas's frame pixel for pixel.

**A text frame's corner was clamped to its padding.** The first rule kept the
radius inside the padding so the arc could not cut into the glyphs' box, which
with the default four-pixel padding allowed a radius of at most four: the wheel
turned 4 into 12 and nothing moved on screen. A radius past the padding is a
look the bar offers on purpose — padding is how a user gives the corner room
back — so the only real bound is half the frame's short side, where the two arcs
would meet and the box would stop being a rounded rectangle. What catches it
now: `the_frame_hugs_its_text_and_an_explicit_width_only_widens_it` holds the
half-short-side clamp, and a radius of 12 on a 36-pixel-tall frame draws as a
strongly rounded box.

**We believed the bar's Confirm, Cancel and Escape own every exclusive edit
until the surface closes.** A freehand release, a saved text field and a press
closing a Bezier path's first anchor each left an exclusive edit open, so the
next press needed Escape before it could draw. The pen family now holds freehand
and Bezier children, and creation ends at its commit: release, save, first
anchor, Enter or Finish Path; Escape or Discard Path drops an open preview,
Edit Points reopens node editing, Escape leaves it, and the armed tool stays
armed. What catches it now:
`a_freehand_stroke_finishes_on_release`,
`a_closed_bezier_path_leaves_its_session`,
`an_open_bezier_path_is_one_step_of_history`,
`a_box_runs_through_the_control_points` and
`escape_leaves_the_tool_and_the_selection_to_their_own_chords`.

**A file clipped the light the display could show.** Every export was written
with `ToneMapping::Clip`: anything above the file's SDR white landed on 1.0, so
a desktop's white and a specular highlight in the same capture came out as one
flat code and the file lost the light the capture had kept for it. A file now
rolls off with BT.2390 against its own white, bounded by the peak measured from
the picture rather than by the display's stated peak, which is a capability and
not the content's range. Saving a capture with no pin measures it once, on the
worker; a pin's peak is measured when it appears, so no frame waits for either.
Caught by `a_measured_picture_rolls_its_highlights_into_the_file_white`.

## See also

- [Selection Readouts and the Loupe](./magnifier.md): the pills a selection
  wears and the disc that magnifies the picture under the pointer.
- [The Legacy App](./legacy-audit.md): where each behaviour and each trap comes
  from.
- [Engine Gaps](../reference/engine-gaps.md): the framework-side list this
  design adds to.
