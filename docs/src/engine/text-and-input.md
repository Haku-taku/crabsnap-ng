# Text and Input

Text runs through four layers: the contract (`ui-text`), a provider
(`ui-text-fontique`), the engine's text pass (`TextPass`), and the control
that owns a buffer. Each layer has one job, and the mistakes that matter are
about offset units and face choice.

## Offsets are bytes

Caret, selection anchor, preedit cursor, IME surrounding text, and
`DeleteSurrounding` are all byte offsets into the buffer. Every one of them
must be snapped to an extended grapheme cluster before use:

- `grapheme::snap` floors an offset, `ceil` raises it.
- Every edit step (backspace, delete, word movement, caret movement) moves by
  clusters.
- IME `DeleteSurrounding` bytes are widened with `snap`/`ceil`.

Treating a byte offset as a character index panics on slicing, or places the
caret inside a character. Skipping the snap lets a backspace delete half of `e`
+ combining acute, or one component of a ZWJ sequence.

## One buffer, several views

An input's buffer belongs to its owner node, and normally one text node draws
it. A document input declares several: each text node keeps its own layout, and
`TextBlock::input_base` names the buffer offset its first drawn byte has. The
engine then answers a press, a step and a deletion in buffer offsets
(`input_views` in `ui/crates/ui-platform/src/view.rs`), so a selection can cross
the views and an edit is still one whole-buffer change, with no per-view
mapping in the app.

The owner is a container that draws no text of its own: it declares
`Sense::FOCUSABLE` and `text_input(buffer)`, and one of its children is the view
at base 0. Its height must be `AxisSize::hug(0.0, 0.0)`. A fit axis' hint is a
cap on a container — only a text leaf reads its hint as a line-box floor — so a
line-box hint holds a three-line field to one line while the view overflows it.

A view must draw exactly `buffer[base..base + len]`; the bytes between two views
— the line break ending each frame — are drawn by no view, and a step that lands
on one is moved to the nearest drawn byte. One caret position belongs to one
view: at a boundary, where a view's last byte ends and another's first begins,
the later view owns it, and only past the last drawn byte does the view ending
there keep its own end. `view_of` answers that, and a widget that draws its own
caret must draw it on the same rule or it paints two. Painting a caret or a
highlight is the widget's job: it maps the buffer offset into its own text and
passes the local offset, because a `Caret` and a `SelectionPrim` name positions
in the text node that draws them.

A row can also be several views **side by side**, like a list's marker cell and
the text after it. Vertical motion therefore crosses lines, not views: a step
takes the line nearest in the direction of travel, and on that line the view
holding the goal column — the x of the insertion point, not of the caret's box.
The views of one row are that row's **columns**, and a step between two columns
keeps the caret's place in the marker instead of its x, counted back from the
column's end. A marker is written back to front from its separator — the space
and dot before the text are the same bytes in every item — so the offset from
the end sits after the dot of `1.` and of `2.`, while no x can: a digit is not a
fixed advance.

A view may be empty: a block whose source has no text still takes a line and a
caret position, so `len` can be zero and the view draws one empty run. That is
how an empty list item or an empty fence is reachable at all. A view whose runs
are all hidden draws nothing either, and a press on it has no byte to point at:
the engine puts the caret after that view instead of inside bytes nothing shows.

A block may also hold a `Structure`: a byte range in that block that shares one value with
sibling ranges in other blocks of the same document. Every member is ordinary text — a caret
stands in it and an edit reaches it like any other byte — and `shown` marks the one member the
source spells out. `ui_core::text_input::edit_structural` answers `None` for any edit that touches no
spelled member, and for one that writes anything but spaces. Otherwise it loads the new value
onto every member of the group inside the same `apply_edit`: a member at least as deep as the
spelled member's old value moves by the same delta, a shallower one keeps its own spaces, and a
later member shifts with the edit. One `Change::Text`, one undo step, so a value a block writes
once cannot come to read two ways. A member may be zero-length: the group then still exists, and
a space typed at its position is the group's. A Markdown code block uses this for the
indentation CommonMark strips from it — the fence line spells it, every content line and the
closing fence hold a member, and a line that writes no container marker starts its member at the
line's first byte, so the column the container reserves there is the block's too. The fence
line's bytes are drawn, in a view of their own left of the card's surface; the other lines'
copies stay caret positions the card stands for: their runs are
`Visibility::Covered` and take a zero `Room`, so the card carries their column instead, and
`TextBlock::hidden_ranges` reports only `Hidden` bytes, so a step lands on them rather than
across them.

`Decl::text_input_enter_newline` makes a plain Enter write `\n` into such an
input instead of activating its node; a single-line field never sets it, and a
multiline field without it keeps Enter as activation and Shift+Enter as the
break.

A selection that covers a line break shows it. A break draws no glyph, so the pass gives
it a fragment of its own (`SelectionFragment::line_break`): a narrow block past the line's
last cluster, as tall as the line, filled like the selected text. Inside one view the break
after each line is found by its own bytes; between two views no byte of it is in either, so
the view it follows names it by ending its selection range one break past its own text, and
the pass reads a range that reaches past the text as that break alone. That reading is
one-directional: a view names the break only when the selection takes the break whole, so a
selection lying beyond the view adds nothing to its end, and one break never marks every
view's end at once. A selection that stops at the text leaves the break unmarked.

## Selection across blocks

A paragraph is one text node; a page is many. A read-only selection is two ends, each a block
and a byte in it, and the blocks it covers are derived rather than stored: the walk's order is
the order hit testing and painting already agree on, so the blocks between the two ends are
exactly the ones the drag travelled over. `Cx::selection_for` answers for every covered block —
the two ends with their own byte ranges, the blocks between them whole — so each widget paints
its own wash and knows nothing of the rest of the selection.

The head follows the selectable block under the pointer. Over anything else it stays where it
was, so a drag through the gap between two paragraphs keeps the last paragraph's bytes instead
of emptying the selection; a press outside the text still drops the whole selection.

Copy joins the covered blocks with a newline, one line per block, and cut is refused as it is
for any read-only text. Select-all covers every selectable block in the window, which is what
Ctrl+A means when no input holds the caret.

Shift extends a selection from its anchor: a shift press (in an input or on a
selectable block) moves the head and keeps the anchor, in another block
included, and the drag that follows it extends from that anchor too.

## Undo and redo

History belongs to the input owner, keyed by its `Id`: focus moving away does
not lose it, and two inputs never share a stack. A step stores the whole buffer
and selection it returns to, not an inverse edit. Each owner keeps at most 200
groups, dropping the oldest.

The built-in ids are `text.undo` (Ctrl+Z) and `text.redo` (Ctrl+Shift+Z, and
Ctrl+Y); an app overrides either through its id like any other shortcut.

An **undo group** is the unit one undo takes back:

- One grapheme cluster typed at a collapsed selection joins the open run while
  no caret or selection move happens between keystrokes and the pause since the
  last one is at most a second. Any arrow step,
  Home/End, click, selection drag, select-all, or focus leaving the input ends
  the run. The pause is the second half of the same rule: a typist who stops has
  finished a thought, and undo must take back that burst rather than everything
  typed since the field was touched. The stamp comes from the live clock, since
  a keystroke arrives between frames.
- Every other edit opens its own group and ends any run: Backspace, Delete,
  `DeleteSurrounding`, cut, paste, Enter's line break, an insertion of more than
  one cluster, and an insertion that replaces a selection.
- An edit that leaves the buffer byte-identical opens no group at all, so
  Backspace at the buffer's start is not an empty undo step.

Undo restores a group's start and redo its end; each clamps the restored
selection to the restored buffer and writes through the same path as a typed
edit, so the app receives `Change::Text` with the whole buffer. Any new edit
clears the redo stack. With no editing input both chords return `None` and fall
through to the app; with an empty stack they return `false` and change nothing.

## Edits the app asks for

The engine is not the only writer of a buffer. A control that rewrites text its app owns — a
document's list break, an indent, a completion — asks through `Cx::edit_input(owner, range,
text, caret)`, and the engine merges the request once the view returns
(`ui/crates/ui-platform/src/view.rs`, `apply_input_edits`). The edit goes through
`apply_edit`, so it lands in that input's undo history, the caret is where the request says,
the app receives `Change::Text`, and the frame that shows it is asked for. The request names
an owner and only that owner's focused input takes it: a control that lost focus between
asking and the merge does not take the keyboard back.

One request is not about the caret at all. A control drawn inside a document input may
rewrite a byte the caret does not own — a task list's box flips its own marker — and asks
through `Cx::edit_input_detached(owner, range, text)`. It applies to the input it names
whether or not that input has focus, and it moves no caret: an input no caret lives in
takes the bytes and one undo step, and one whose caret lives elsewhere has that caret
shifted by what the edit did, never placed. Like any other edit it asks for the frame that
shows it, and the step belongs to that input's own history.

An edit can leave a selection, not only a caret: `Cx::edit_input_selecting` takes an
`(anchor, caret)` pair and `apply_input_edits` builds a `TextSelection` from it, clamped to
the buffer the edit produced, so a control that rewrites several lines can keep them
selected. `Cx::edit_input` is the same request with a collapsed pair, and either way it is
one `Change` in the history, so one undo takes the whole edit back.

Writing the clipboard is the same shape and the same reason: `Cx::copy_text` — or
`Cx::copy_bytes` for an image or any other opaque payload — asks the engine, which drains the
request after the build into a `ClipboardCommand`, and the host applies it. A control's copy
button therefore needs no app wiring, and the platform stays the only thing that touches the
clipboard.

A command carries `ClipboardContent`: text under the platform's text MIME types, or bytes under
one MIME type. A backend offers exactly the types the payload can be read as, so an image is not
also announced as text.

Wayland's clipboard is pull-based: the source must stay alive and answer `send`, and destroying
it takes the selection with it. An app whose copying surface is short-lived — a capture tool's
overlay exists for one action — must not hand the copy to that window. `ClipboardHost` owns a
data device and a queue of its own on the same connection for the life of the process, and the
window hands it the serial of its freshest input event. Paste has no host: an offer follows
keyboard focus, so it stays with the focused window. The host still has to *receive* offers —
the compositor sends them to every device on the seat — and destroys each one it will never
paste from.

A node's own chord bindings (`Decl::shortcut`) are how a control reaches this door: the
engine reports `Change::Shortcut` to the app and leaves the one-frame fact in the build
context, so the control reads `Cx::shortcut()` in its next build and answers with an edit.
Any discrete event already asks for a frame (`dispatch` marks the demand), so the control
needs no cooperation from the app to be rebuilt.

The fact carries the editing state the chord was pressed in — the focused input's buffer and
selection (`Cx::input_value`, `Cx::shortcut_selection`). Keys are dispatched as they arrive
and one frame answers the whole burst, so by build time the live selection may belong to a
later key: an edit computed from it replaced text the user selected after the chord.

Facts queue rather than overwrite: a frame answers the one at the front, asks for another
frame while the queue is not empty, and each applied edit is replayed on the queued facts'
own buffers and selections, so the second Enter of a burst breaks the line the first one
left behind. Pinned by `a_burst_of_shortcuts_is_answered_one_frame_each`.

## Focus requests

Focus moves on a press, and an engine-owned control steers it with
`Change::Focus`. Neither helps a canvas that opens a field from a press: the
field did not exist when the press landed. Such a build asks through
`Cx::request_focus(id)`, and the engine applies it once the declaration is in
the tree and layout has given the node a rect — before that, no node is a focus
stop, because a stop is something the mask can show. A node that draws no ink of
its own, such as a document column whose rows are the whole picture, declares
`focus_stop(true)`; without it the node is not a stop and the request is
dropped. A text or formula node is a stop already: its ink comes from its node
data rather than from a shape, so there is no paint to ask. The caret is synced the way
a press syncs it, so the field takes typing with the next event. A request names
one node and is not a stack: a second request in the same build moves focus
again. An id that cannot take focus is ignored rather than kept for later, since
the node may never come back.

`view::tests::contracts::a_build_can_ask_for_the_keyboard_focus` presses a
canvas, opens a field from that build, and types into it on the frame after: it
fails on the focus assertion when the applied request is removed.

## Faces and fallback

- `FaceId` identifies a **face instance**, not a family: variable-font axes
  and weight are interned into it, so downstream caches need no extra key
  dimension.
- A named family is an exact query; a miss returns `None` instead of
  substituting. The engine then appends the generic family and the base stack,
  so a typo renders in the wrong family rather than failing visibly.
- The provider's script fallback chain uses family default faces: a bold or
  italic request is lost on a fallback face.
- Emoji faces are found by family name containing `emoji`, not by a capability
  query.
- Named, generic, and fallback faces come from `fontique`; without the
  `system` feature there is no OS font stack at all.
- The base stack is the families an app passed to `with_stack`, then the
  platform's UI face (`GenericFamily::SystemUi`), then the plain sans — so the
  default style, which names `SystemUi`, renders in the desktop's own face.
- `SystemUi` is fontique's own UI slot, not its sans: Segoe UI on Windows
  (`backend/dwrite.rs:37`), the system font on macOS (`backend/coretext.rs:32`),
  the configured default on a fontconfig desktop (`backend/fontconfig.rs:841`).

## Shaping and line breaking

- Shaping is swash; outlines and metrics are skrifa; coverage rasterisation is
  zeno.
- `ShapedGlyph::cluster` is the byte start of the shaping cluster, not a
  grapheme. Do not compare it to cluster offsets.
- Variable axes must be set on the builder before shaping, or advances come
  from the default instance.
- Bidi levels come from `unicode-bidi`; a run carries the script its shaper
  needs, so a Common character (an emoji, a digit) never renames the script of
  the text around it. RTL runs are reversed by the caller **by cluster** —
  swash leaves order alone deliberately, and reversing glyphs inside a cluster
  moves a mark away from its base. swash mirrors bracket characters itself
  when the direction is right-to-left. This is also UAX #9 L3: a mark stays
  after the base it attaches to because clusters never separate them, and the
  shaper puts a mark in its base's cluster.
- No cut inside a grapheme: `TextBlock::flatten` snaps a run boundary up to the
  cluster's end, so the mark takes its base's style, and the painter snaps a
  selection edge the same way. A mark shaped on its own is detached in any
  direction, and in an RTL run it is the mark-before-base case L3 exists to
  prevent.
- The base direction is the **paragraph's**, not the line's: `bidi::paragraph_direction`
  reads it from the first strong character of the block (UAX #9 P2/P3) and every
  itemize call — measurement, shaping, boundaries — passes it in. A line that
  opens with digits or punctuation has no strong character of its own, and
  inferring per line would give its neutrals the wrong level, moving a trailing
  `!` to the other end of the number. `InlineLayout::rtl` carries the answer to
  the layer that never sees the text.
- Alignment is relative: `TextAlign::Start` and `End` swap in an RTL block, and
  `Justify` counts as `Start`, because its last line is the one it does not
  stretch. `line_origin` applies `align_for_direction` once, so placement, paint
  and hit testing keep one answer.
- `TextStyle::letter_spacing` is CSS `letter-spacing` in pixels: one extra
  advance after every cluster, the last one included, added to the shaped
  advances themselves. Measurement, paint and caret boundaries therefore share
  it; a paint-only offset would wrap and place carets by different widths.
- Line breaking is UAX #14, greedy. At a soft wrap, trailing whitespace hangs:
  it is out of the line's range and ink width while glyph placement still
  consumes the full advance. A line a hard break ends keeps the spaces before
  that break, because a caret stands inside them.
- `break_lines_indented` and `wrap_indented` take two left insets: `indent`
  narrows the first line and `hanging` every later one. Both are subtracted
  from the wrap width, so a line offset by an inset cannot overflow by it;
  [Inline Content](./layout-inline-content.md) owns the geometry. The inset is
  on the **reading** edge, so an RTL paragraph takes it from its right:
  `line_origin` mirrors the inset and the marker a hanging indent clears stands
  at the right of the item.
- Mixed emoji/text runs split at grapheme boundaries when presentation differs.
  One face for the whole run is wrong: an emoji face treats digits as empty
  keycap components and draws nothing.

## Resolving primitives

Paint emits a `Text` primitive per block; the pass shapes and places each one into glyphs,
selection rects and rules, and that is what the painter draws.

- A primitive no clip can show is dropped before shaping. A document taller than its viewport
  resolves only the rows the clip admits: 10,000 rows in one scroller (release test build,
  800×600) went from 139 ms to 22 ms on a still frame and from 144 ms to 17 ms on a scroll
  frame, with 369,000 placed glyphs becoming 136.
- The proof is the block's box inflated by one em, which covers bearings, italics and symbols
  centred in their box, transformed by the primitive's own zoom. Text with no laid-out box is
  never dropped, so an estimate cannot hide a visible line.
- A formula is proved the same way, with half its height of slack.
- Only text and maths are culled. A rect, an image or a shadow outside the clip still reaches
  the list, and paint and the damage diff still walk it.

## Atlas

One 1024² coverage page is allocated eagerly; the colour page is created on
first use. Glyphs are packed by a shelf allocator. Growth doubles width and
height and preserves rows; only when growth fails is the page zeroed and its
index entries dropped.

The cache key is `(FaceId, glyph id, rounded size, x phase, y phase)`. Glyphs
with no pixels are cached as `None`. Resolved primitives carry an atlas stamp —
the eviction counter, and nothing else: rasterizing a glyph or widening the page
leaves every slot where it was, so only a cleared page can make a cached
rectangle point at a tile that no longer holds it.

Every primitive kind rasterizes into this one page — a formula's glyphs go
through the same cache — so the pass checks the rasterization counters once at
the end of the whole pass, and the renderer re-uploads the page when the pass's
version moved. A check that only ran inside text shaping missed a formula whose
coverage was the last thing the pass added.

A shaped run's missing glyphs are collected before its per-glyph lookups and
rasterized together. Past 64 misses the batch is spread over up to eight
threads; below that, starting threads costs more than the work. Colour faces
keep the serial path because their swash scaler carries per-face state. Only
rasterization is parallel: slot allocation and index insertion stay in run
order, so a batched run fills the page byte for byte like a serial one
(`a_parallel_glyph_batch_fills_the_atlas_like_a_serial_one`).

Measured on 2026-11-10, release build, 16 px on the system sans face (MiSans
VF on this machine), 24 cores, one run of fresh `(glyph, phase)` pairs: 64
pairs take 611 µs serially and 403 µs batched, 512 take 5.9 ms against
1.3 ms, and 2048 take 42.2 ms against 8.1 ms. At 32 the batch does not engage
and the two timings are within noise (381 µs against 444 µs).

```sh
cargo test --release -p ui-text-fontique --features system -- --ignored --nocapture
```

## Subpixel placement

A glyph's position is quantized to phases in **device** space, and a phase is a translation
of the outline: the tile is rasterized from a baseline shifted by it, so the placement stays
on whole pixels and the tile carries the fraction. Horizontal phases are quarters (four
tiles); vertical phases are halves (two), because a block of text moves down a screen far
more often than it changes width, and a whole-pixel grid there makes it step while every
other primitive glides.

What the phases do not take, they leave as a fractional destination: at most an eighth of a
pixel horizontally and a quarter vertically, which the renderer interpolates by sampling the
atlas with `LINEAR`. A 1:1 tile still samples exact texels, because fragment centres land on
texel centres, so a line resting on the device grid is unchanged; each tile carries a pixel of
empty padding, so the blend at its edge reaches only transparency. Both counts are per glyph
and per size, so a face full of motion fills the atlas faster; the vertical phase is a
multiplier of two, not four, for that reason.

Subpixel phases are skipped for CJK scripts horizontally: thousands of characters times four
phases would dominate the atlas, and Han glyphs are near-square so horizontal jitter is less
visible. The vertical phase applies to them as it does to any other script.

## IME

- The engine owns the preedit. The application's buffer holds committed text
  only.
- Composition emits no `Change::Text`; a commit inserts once and clears the
  preedit.
- The app sees `Event::Preedit(Option<Preedit>)`,
  `Event::DeleteSurrounding` (bytes), and `Event::Text` for the commit.
- `ImeState` is enabled only when a caret rectangle was computed this frame.
  Otherwise the candidate window opens at (0, 0).
- Surrounding text is re-sent only on enable or when it changes. Sending it
  per keystroke makes the input method restart composition.
- On Wayland the backend buffers preedit, commit, and delete until `done`, then
  emits `DeleteSurrounding`, exactly one `Preedit`, then `Text`.
- On Windows a focused non-secret field is a TSF document that publishes
  `surrounding` as its text and holds the preedit as a splice at the caret; an
  edit the input method makes to that document becomes the delete and the commit
  above. Every other state uses IMM32, which carries the preedit and the commit
  and nothing else.
- A secret input sends no surrounding text and states `content_type` as
  `hidden_text | sensitive_data` with purpose `password`, so the method neither
  learns the value nor offers suggestions. Its previous neighbourhood is cleared
  with an empty statement rather than left registered.
- While a composition is live the engine holds the selection it began with, and
  `Cx::reveal_range_for` answers with that instead of the live one. Structure
  reveal (which delimiters a document draws) is cut from it, so no byte under
  the preedit is swapped between rendered and source — several input methods
  cancel a composition when their surroundings change shape. The caret and
  selection the control draws still follow the input; the hold ends when the
  preedit does.
- A field names what it is for with `text_input_purpose`; the time picker's
  digits say `ContentPurpose::Time` and reach the protocol as purpose `time`.
  A secret buffer states the hidden form whatever purpose it named. The
  protocol has no search purpose, so a search field sends `normal`.

## Hidden fields

`TextBlock` has no notion of secrecy; the control masks what it draws. The mask
replaces each character with a mark of the same UTF-8 length — `*`, `·`, `•`, and
two dots for a four-byte character — so every character boundary keeps its byte
offset. Carets, selections, the preedit splice and the engine's edits index the
same bytes in the drawn line as in the buffer, and no layer needs a second table.

`ContainerParams::text_input_hidden` is the engine's half of the same
declaration: `copy_selection` refuses to copy or cut, the text menu disables both
rows, and the input method is told what the buffer is. The app still receives the
real bytes in `Change::Text`; it is the one that declared them secret, and it may
copy them deliberately if the product wants that.

One deliberate deviation: a four-byte character draws two dots, since no common
font has a four-byte mark, so the mark count for an emoji is one higher than the
buffer's character count.

## Key repeat

Wayland has no repeat: the seat states a rate and a delay once
(`wl_keyboard.repeat_info`) and sends nothing more while a key stays down.
The window keeps a table of the keys held down and when each is next due
(`Repeats`), and replays a key when its period comes round, so a repeat
reaches the app through the same queue and the same routing as the press did.

A repeat is the press asked again, not the first press replayed: its events
come from the same translation the real press used, under the modifiers held
now. A letter typed with `Shift` down repeats that way until `Shift` is let
go, and a key pressed beside it — a modifier included — starts its own period
without disturbing the first. Only a press that produced no event is kept out
of the table: a modifier changes what the keys beside it mean and repeats
nothing on its own.

`take_events` replays after draining the socket, and `repeat_deadline` joins
the session's `next_deadline`, so an idle loop wakes for a repeat with nothing
arriving from the compositor. A turn that was busy for several periods replays
once rather than catching up: a burst would type the same character many times
for one press of the loop.

A rate of zero means the seat wants no repeat, and losing keyboard focus drops
every held key — their releases can no longer arrive, so they must not keep
firing. Repeat is also what makes a held cursor key walk: each replay is an
ordinary event, resolved like any other, so the modifiers held at that moment
decide what it means.

## Localisation

- Fluent is the only implementation, in `ui-i18n`. English is the source of
  truth: `build.rs` generates the key table from `locales/en`.
- Lookup walks an explicit locale chain
  (`zh-Hans-CN → zh-Hans → zh-CN → zh → en`); it never relies on
  bundle-internal fallback.
- Static messages are preformatted per locale. Parameterised messages format on
  demand.
- A missing key or a formatting error logs once and returns `⟨id⟩`. Calling
  `text()` on a parameterised message gives that literal; use the `_with` form.
- Placeables are emitted without bidi isolation marks (`use_isolating(false)`).
- A number placeable is written from ICU4X's CLDR data through Fluent's
  formatter hook: the locale's digits, separators and grouping positions, not
  `f64`'s own text. A plural selector still reads the number itself, not the
  text it formatted to. A locale with no data is logged once and falls back to
  Fluent's own rendering.
- A date has no Fluent placeable in this bundle version; `Catalog::date` (and
  `Cx::date`) formats a civil date with the chain's first locale at CLDR's
  medium length ("Nov 9, 2026", "2026年11月9日"), and the app passes the text.
  The calendar is fixed to Gregorian, so the text names the year the Gregorian
  picker shows even where the locale's own calendar differs.
- The application chooses the locale. The library never reads `LANG` or
  `LC_ALL`.

## Corrections

**Pressing `Shift` mid-hold froze a held cursor key.** The window kept one repeating key,
replaced on every `pressed` event by a snapshot of the events that press produced. A modifier
produces no event and names no key, so `Shift` pressed while `A` was held replaced the letter's
repeat with an empty one: the pointer stopped at once and stayed stopped after `Shift` was let
go, because the released key no longer matched the held one. Measured in the nested session with
the overlay on the host loop: 800 → 796 → 787 while `A` was held, then 683 → 683 → 683 after
`Shift` went down. The table is now per key, and a repeat re-derives its events under the
modifiers held now instead of replaying the first press, so taking `Shift` down or letting it go
changes what the key means, as for a client that never saw a repeat.
`holding_another_key_leaves_a_repeat_running` pins the table; the toggle itself was checked by
hand in the nested session.

**The clipboard host panicked on any session that had a selection.** `HostState` implemented
`Dispatch<wl_data_device::WlDataDevice>` as an empty function, but `data_offer` is the only
event on that interface that creates an object; without an `event_created_child!` entry
wayland-client panics the moment one arrives. The daemon never showed it — it holds no focused
surface — and the overlay only starts with a selection the same process just set. Opening the
settings window while anything owned the clipboard (`wl-copy hello`, then the window) killed the
process at startup: `Missing event_created_child specialization for event opcode 0 of
wl_data_device`. The window's own device already had the specialization; the host now has the
same one and keeps the offer it will never paste from, replacing and destroying it as the
compositor sends new ones. What catches it now: running any windowed app while a selection
exists — by hand in a real session, since the specialization needs a live connection and no
unit test can hold one.

**Flattening a block rescanned it once per run.** `TextBlock::flatten` moved each run
boundary out to a cluster edge by calling `ceil`, and `ceil` walks the clusters from byte
zero, so one block's span table cost runs × length. The per-frame damage pass
(`crates/ui-platform/src/view.rs:7271`) and `Prim::bounds` both built that table for every
text on the page only to count characters. A selection drag over the demo document put
162,096 of a frame's 162,366 cluster steps in this one function, listing 53 MB of text for
one 287 ms frame; the trace came from per-call-site counters, since the sampling profiler
on this machine cannot unwind Rust user stacks. Cuts arrive in run order, so one walk over
the clusters now answers all of them, and the two length-only callers read the runs
directly. `a_run_boundary_inside_a_cluster_snaps_to_its_edge` pins the rule the rewrite
had to keep: a mark whose run boundary falls inside a cluster stays with its base.

**A newly rasterized glyph reshaped the whole page.** The atlas stamp folded in the
pass's texture version and the colour page's version, so any rasterization or growth
invalidated every resolved text on screen. The Atlas section already said the opposite:
growth preserves rows and only a zeroing drops entries. A selection drag reveals markup
construct by construct, and each revealed delimiter is a glyph the atlas may not hold at
that phase; every such frame reshaped the whole page, which is what made a drag on a
document page drop to a few frames a second. The stamp is the eviction counter now,
checked by `a_new_glyph_does_not_invalidate_the_shaped_text_cache`: a primitive resolved
again after a glyph rasterized elsewhere stays a hit.

**A selection fragment shaped every line of its block.** `selection_fragments_with` took
each line's boundaries before asking whether the range reached that line, so a highlighted
code card shaped all its lines once per span carrying a background — 30 spans over 10 lines
for one fence — on every frame that painted the selection. The loop now skips a line the
range never reaches and shapes one only when it contributes a fragment or a break mark.
First resolve of a 30-run, 10-line fence fell from 2.95 ms to 1.67 ms, and a fully selected
one from 766 µs to 214 µs (release test build).

**An RTL paragraph left its insets on the left edge.** `line_origin` applied
`align_for_direction` so `Start` became the right edge, then still added
`indent`/`hanging` to the left of it: the two cancelled out, so a wrapped RTL
list item aligned right but never cleared its marker, and the marker stood at
the wrong end of the item. The inset a marker opens is on the reading edge, so
an RTL paragraph takes it from its right — the room still shrinks by the inset,
but its start is the box edge, and the hanging indent clears the marker where
the marker is. Checked by `an_rtl_block_takes_its_insets_from_the_right_edge`:
a first line of 30 px with a 10 px indent starts 160 px into a 200 px box, and
a wrapped line of 50 px under a 30 px hanging inset starts at 120 px.

**A read-only selection had no modifier of its own.** Shift on a selectable
block (and inside an input) reset the selection to a single collapsed point,
because the press always started a fresh selection. A shift press now keeps the
anchor and moves the head; the drag that follows extends from that anchor too,
so crossing a paragraph boundary with Shift held selects both blocks rather
than starting over. The two-block press is a press on a wrapped paragraph, so
the selectable block is found geometrically when the topmost consumer is its
container — the drag path keeps its topmost-consumer rule, which is what lets a
drag over a gap keep the last block's bytes. Checked by
`a_shift_press_extends_the_selection_it_finds` (two blocks, anchor's tail kept)
and `a_shift_press_in_an_input_extends_its_selection` (field, anchor kept).

**The colour gap was named after the format no font here has.** The status
listed SVG-in-OpenType as the missing colour source, but none of the bundled
fonts and none of the 488 system fonts carry an `SVG ` table, and swash 0.2 has
no source for one: it would need a whole SVG renderer for a format nothing on
the machine uses. What the colour path does render is COLR v0 layered outlines
and CBDT/sbix bitmaps. The gap with a real font behind it is COLR v1 paint
graphs — Segoe UI Emoji here is COLR version 1 with 3365 base glyphs and zero
v0 layers, so swash's v0 lookup finds nothing and the glyph falls back to its
monochrome outline. That is a renderer of its own, not a small addition; the
status now names it instead of SVG.

**A date is not three fields in a row.** The gallery's readout built
`"{day} {month} {year}"` by hand — an order no locale asks for — and
fluent-bundle 0.16 registers only `NUMBER()` as a builtin, so a message cannot
format a date either. `Catalog::date` now asks ICU4X for the locale's medium
pattern. It uses the fixed Gregorian calendar rather than the locale's, for two
reasons: the picker's grid is Gregorian, so a Buddhist-era year beside it would
contradict it, and the fixed-calendar formatter links one calendar's data
instead of all of them (about 0.4 MiB of CLDR patterns, against 1.1 MiB for the
locale-calendar formatter). Checked by
`a_date_is_written_in_its_locales_order`: en "Nov 9, 2026", zh-CN
"2026年11月9日", and 2026-02-30 has no text at all.

**Localised numbers needed CLDR data, not a separator table.** The status listed
locale-aware numbers as missing, and the obvious fix — formatting to a string
and passing that as the argument — would have removed the number Fluent selects
plural branches on. Replacing the `NUMBER()` builtin fails too: `add_function`
refuses a name that is taken, and a formatted string can no longer answer
`[one]`. The seam is `FluentBundle::set_formatter`, which runs before any
`FluentValue` is written; `ui-i18n` formats numbers there with `icu_decimal`'s
CLDR data, so `{ $count }` is localised without the messages changing and the
value stays a number. ICU's payloads are not `Sync` until `icu_provider/sync`
is enabled, which the bundle's memoizer map requires. Checked by
`a_number_is_written_the_way_its_locale_writes_numbers`: en "1,000 rows",
de "1.000 rows", bn "১,০০০ rows", de "1.234,5 kg", where the same catalog
without the hook reads "1000 rows".

**The atlas-upload check lived inside text resolution.** The pass bumped its
version when the glyph cache's counters moved, but only at the end of shaping a
text primitive. A frame that rasterized a formula's glyph — a character typed
into an editable formula — had no text primitive afterwards, so the page changed
in memory while the renderer kept the old one: the character drew as an empty
slot until some other text glyph rasterized and bumped the version. The check
now also runs once at the end of the pass, after every primitive kind, and
`a_formula_rasterized_after_the_last_text_uploads_the_atlas` goes red without
it.

**An RTL line reordered itself at every style change and selection edge.**
`shape_line` walked the line in logical order and shaped each span, and each
selection piece, on its own, placing them left to right. One Arabic word split
across two runs therefore swapped halves, and selecting a middle word reversed
it while selected. Bidi is now itemised for the whole line, pieces are cut at
run boundaries as well as span and selection ones, and the pieces are placed in
the line's visual order, an RTL run's pieces in reverse; pinned by
`an_rtl_line_keeps_its_order_across_a_style_change`.

**The base direction was inferred per line.** Every `itemize` call passed `None`, so
each line took the direction of its own first strong character. Inside a
right-to-left paragraph, a line opening with a number therefore read as
left-to-right: its digits stayed left-to-right either way, but the neutral
punctuation after them moved to the other end — `123!` drew as `!123`. The
direction is now the block's, read once from the first strong character of the
flattened paragraph and passed to measurement, shaping and boundaries alike,
with `InlineLayout::rtl` carrying it to line placement; alignment follows through
`align_for_direction`, so `Start` is the right edge in an RTL block. Caught by
`a_line_without_strong_characters_follows_its_paragraph` in
`crates/ui-platform/src/text_pass/tests.rs` and
`a_right_to_left_block_starts_its_lines_at_the_right_edge` in `ui-core`'s
`text.rs`.

**An empty line at a block edge measured one pixel.** A line opened by a hard
break overlaps no span (`span.start < end && span.end > start` is false for an
empty range), so its metrics were zero and `line_metrics`' one-pixel floor
stood in for a line box: the caret was 2×1 and the IME candidate window
anchored to it. An empty line now takes the box of the span it sits at; pinned
by `a_line_after_a_hard_break_is_as_tall_as_the_text`.

**A block with no glyphs drew no caret.** Two paint gates dropped it: `shape`
returned early when the prepared block had no spans, discarding a caret it had
already computed, and `resolve_into` treated an empty glyph list as an
unresolved prim and pushed it to the fallback path, which paints neither caret
nor selection. A markdown document is mostly blank rows with a caret and
nothing else, so they read as uneditable. The caret is now computed from the
line's metrics before the span check, and a block with nothing to ink resolves
its own prim instead of falling back; pinned by
`a_block_without_glyphs_still_draws_its_caret`, watched red with each half
reverted.

The first version of that rule read "no atoms and no visible non-empty run",
which only covered a block with no bytes at all. A window can hold nothing but
spaces — a marker cell's indent, or the whitespace a line ends with — and spaces
ink nothing, so the prim fell back: a click or a `Right` step into that window
put the caret at a byte whose prim painted nothing. "Non-empty" now means "not
blank", so a run of spaces resolves its own prim and every byte in it owns a
caret x; pinned by the same test's second half, which now reads three carets
across `"  "` and requires them to stand strictly increasing.

**Two views drew one caret, so a caret stood twice.** `InputView::covers` counts
a view's end, and `view_of` took the first view covering a position. A markdown
row is several views: each marker cell draws `"- "` or `"1. "` and the body
starts where the cell ends, so the position between them lay in both. The
markdown widget drew a caret in every view whose range contained the offset, and
its cell took the end: a press at the end of `"9. "` in the gallery's ordered
sample put a caret in the cell and one in the body, and the cell's line box is
18.8 tall where the body's is 20.9, so the two stood at different heights on one
column. The same
boundary also spliced an IME preedit into both. The engine now gives a shared
boundary to the later view, and the widget's cell window owns its caret
positions up to but not including its own end; pinned by
`the_text_after_a_boundary_owns_the_caret_there`, which meets two views at one
offset and reads the engine's answer for each, and by the gallery's `one caret`
flag, which clicks the second item's marker end and counts the caret-bearing
views in the whole input: exactly one, the body's, at its first byte. With the
cell taking its own end the same read finds two.

**The same rule, declared instead of derived, broke again at a formula's
fence.** A window said by hand whether it owned its last position, and the
answer was right when it was written: a line's window ends where its break
starts. A second view on that line makes it wrong. `$$x^2$$` puts the payload's
first byte where the opening fence's text ends and its last byte where the
closing fence begins, so the fence drew a caret at its end and the formula its
bar at the same position — one caret, two bars, on two visual lines. A window
now owns its end only when the document writes no drawn byte there: the end is
followed by a line break, or it is the end of the buffer. The formula applies
the same rule from its side — `Math::caret_ends` hands the payload's last byte
to the fence that starts there — and its first byte is its own, because the
payload is the later of the two views. Pinned by
`a_formula_sharing_a_fence_line_draws_one_caret_at_each_boundary`.

**An empty line's selected break painted nothing.** A block whose every run is empty still
owns a line box, and the pass kept the caret on it — but the prim assembly returned before
the selection was asked for, so a selection that took the blank line's break drew no fragment
while its caret showed. The selection and the composing underline are now painted on that
line too. Pinned by `an_empty_lines_selected_break_gets_a_block`, which resolves an empty
block with its break selected and reads the block it fills.

**A vertical step inside one view aimed at a point on a line's edge.** A view can draw many
lines — a display formula's source, a code card's body, a table's source — and the step
inside it asked `index_at` for the point `caret.y + line height`. That point is the next
line's top edge, which belongs to either line, and one ulp in its favour sent the answer back
to the caret's own line: `stayed` then read true, `crossing` took over, and the step left the
view. A table's source state could not reach its middle rows, and a step down from the
paragraph above entered the table's first row and then jumped out of it. The step now walks
the layout's own lines (`TextPass::line_step`), one line per press, and only the edge of a
view's line list hands over to `crossing`. Pinned by
`a_tables_source_state_steps_through_every_row`, which steps down into a table and up out of
it and requires every row to be visited; with the point step it read the first row and then
the paragraph after the table.

**A vertical step entered the next line's marker instead of its text.** A row is
several views — a marker cell and the text after it — but `crossing` took the
first view of the line it stepped into, and the cell's bytes then took no
advance while the caret was away, so `index_at` found no boundary in them and
the caret landed at the line's start. In the gallery's `- a` / `- b` /
`- c`, Down from offset 2 (right after `- `) reached 4, the next line's `-`,
then 8; Up from 8 reached 6, so the two directions disagreed by a marker. A step
now takes the line and then the view holding the goal column, and the goal is
the insertion point's x — the caret's box is centred on it, and reusing the
box's left edge put the column half a caret inside the cell. Pinned by the
gallery's `vertical` flag, which reads 2 → 6 → 10 with Up back to 6; on the old
crossing it read 2 → 4 → 8.

**A break segment crossing a style change was measured in one style.** The
breaker hands over a whole UAX #14 segment, and `measure_range` read the style
at the segment's first byte, so the wrapped width and `Line::width` of mixed
text were those of its first style. The width is now summed over slices cut at
span boundaries; pinned by
`a_segment_measured_across_a_style_change_counts_both_sizes`.

**Script was inferred from one character for a whole run.** `Script::dominant`
took the first non-ASCII character, and every unlisted range — emoji included —
read as Latin, so `"😀中文"` shaped the Han text with a Latin face and `"😀مرحبا"`
lost Arabic joining. Itemisation now splits at script changes, resolving
Common and Inherited characters from their neighbours, and each run carries its
script; pinned by `a_script_change_splits_a_run_even_at_one_bidi_level` and
`characters_without_a_script_take_the_script_around_them`.

**An RTL run reversed its glyphs, not its clusters.** Reversing the flat glyph
list moved the mark of `مَ` one base-advance away from where the font placed
it. Clusters now swap places while the glyphs inside one keep logical order;
the bracket re-mapping that duplicated swash's own mirroring went with it;
pinned by `an_rtl_run_reverses_clusters_without_disturbing_a_cluster`.

**An ellipsized line swallowed the lines after it.** The truncation walked
clusters from the last kept line to the end of the buffer, so `"aaaa\nbbbb"`
at one line drew `aaaa
bbbb…` with the newline shaped as ink. The cut stops at
the last kept line's end; pinned by `a_truncated_line_stops_at_its_hard_break`.

**A skin tone made an emoji sequence text.** `EmojiPresentationAndModifierAndEmojiComponent`
is `Emoji_Modifier=YES` — the skin tone — but it was passed as a modifier
*base*, and the DFA checks base first, so `✌🏽` fell back to two text glyphs.
Pinned by `a_skin_tone_forms_one_emoji_sequence`.

**A changed atom was served from the cache.** The measured-block key hashed
runs, styles and limits but not the atoms, so growing an inline box kept the
old wrap and placed the box at its old measured position. Atoms are part of the
key; pinned by `a_measured_block_key_notices_its_atoms`.

**A trailing no-break space was trimmed out of the line.** Trailing white was
removed with `char::is_whitespace`, which includes U+00A0; CSS hangs a space but
never a no-break space, and the byte was dropped from `Line::range` too, so no
caret could sit after it. Only space, tab and ideographic space hang now;
pinned by `a_no_break_space_is_ink_not_hanging_white`.

**A stale caret split a character when composition arrived.** The preedit
splice clamped the caret to the buffer's length but not to a cluster boundary,
so a buffer that shrank under a held offset could slice `é` in half and panic.
Both the widget and the text pass now splice through
`ui_core::text_input::composed`, which snaps the caret; pinned by
`a_composition_at_a_stale_caret_splices_on_a_boundary`.

**A keycap sequence lost its base.** Splitting a run by code point put the
variation selector, ZWJ, or keycap mark on a different face, so only a bare
digit remained. Runs now split at grapheme boundaries; the boundary is pinned
by `an_offset_inside_a_cluster_snaps_to_its_start_and_steps_past_it` in
`ui-core/src/grapheme.rs`, and the mixed-presentation split by
`a_digit_after_a_keycap_keeps_its_own_glyph`.

**A digit after a keycap disappeared.** Shaping the whole run on one emoji
face made a plain digit resolve to an empty keycap component, which draws
nothing. Clusters with different presentation now use different faces; pinned
by `a_digit_after_a_keycap_keeps_its_own_glyph`.

**Fast typing lost characters.** Several text events can arrive in one batch,
and each read the tree before the previous edit had been applied, so the
second character overwrote the first. An edit is now written back into the
tree before the next event is dispatched.

**Clicks landed on the wrong character.** `boundaries()` returned offsets
local to each shaped line while caret and selection compared them with
whole-block offsets, so every line after the first was shifted by the length
of the text before it. Each line's start is added before the boundaries leave
the text pass.

**IME stayed dead on focus.** `enable` and the surrounding/state data shared a
commit, and the session reset that `enable` performs discarded the rest. The
enable is committed alone and the state follows in a second commit; surrounding
text is still sent only when it changes.

**An empty field had no caret rectangle.** A text block with no spans measured
zero lines, so the caret rect was `None` and IME refused to enable. An empty
block now measures one line box, which gives the caret a place to sit.

**Undo stores snapshots, not inverse edits.** An inverse edit is compact, but a
typing run merged into one group has no single inverse; the group would need a
log of the run anyway, and redo would have to re-execute it against a buffer
that may have changed shape. Each group therefore stores the buffer and
selection it returns to, which costs one copy of the buffer per group (bounded
at 200 per owner) and makes undo and redo plain stack moves. Pinned by
`typing_merges_into_one_undo_group_and_redo_restores_it`,
`a_deletion_is_its_own_undo_group`, and
`a_caret_move_between_keystrokes_starts_a_new_undo_group`.

**A chord's edit was computed from the live selection, not the one it was pressed at.** The
shortcut fact reached the control's build a frame after the key, and events are dispatched as
they arrive: Enter followed by Shift+Down within one frame made the break replace the selected
line, deleting text the user had selected after pressing Enter. The fact is now captured with
the chord — the focused input's buffer and selection at that moment — and routed with it
(`Routing::shortcut_focus`); pinned by the gallery's `burst` flag, which read
`"- burst tail\n\n"` with the live selection back.

**A revealed run reused the lines measured while it was hidden.** Hidden bytes take no
advance, so revealing a construct's delimiters changes where the paragraph wraps. The
measured layout was keyed by the runs' text and style alone, so the shown state drew the
hidden state's line breaks and its longer line ran past the block's width. A run's
visibility is part of that key now; pinned by
`a_revealed_run_wraps_again_instead_of_overflowing_the_width`.

**A step into a sibling marker landed before its dot, and a rendered marker had no
geometry to map at all.** The cell's bytes were hidden whenever the caret was away, so
they took no advance: every offset in the marker shared one x, and an x step could only
tie back to the cell's first byte. The bytes are `Covered` now — unpainted, but laid out
with their own advance — so a press inside a rendered marker names the byte under it. The
x column still could not carry a caret from `1.| ` to `2.| `: the digits have different
advances, so the place after one dot fell before the next line's dot, one byte short at
offset 6 of `2. b`. A step between two columns now keeps the caret's place counted back
from the cell's end — a marker is written back to front from its separator, so the count
keeps the dot under the caret however many digits precede it — and it reaches 7. In
`9. a` / `10. b` it reaches 8, after the dot of `10.`, where the x column landed between
the digits. Pinned by the gallery's `marker step` flag.

**UAX #9 L3 was listed as missing while the real hole was a cut inside a grapheme.** The
Status line said L3 was unimplemented because reversal-by-cluster was believed to be the
L3 fix only for intact clusters. Two facts settle the intended half: swash puts a mark in
its base's cluster (probed with Arial: `مَر` shapes as one cluster of two glyphs), and
`reverse_clusters` reverses clusters, never glyphs inside one — so a mark already follows
its base. What could still separate them was the piece split: `TextBlock::flatten` turned
runs into spans at any byte, and the painter cut a selection's ink at its raw edges, so a
boundary inside `e\u{301}` shaped the acute on its own — three glyphs for `éx` instead of
two, spans `[0..1, 1..4]` instead of `[0..3, 3..4]`. Run boundaries now snap up to the
cluster's end, and the selection cut snaps the same way. Pinned by
`a_cut_inside_a_cluster_puts_the_mark_with_its_base`, which compares an inside cut with
one at the cluster's edge for both a style run and a selection edge.

**Vertical placement had no phase at all.** Every horizontal origin took a quarter-pixel
phase with its own tile, and the baseline was rounded to a whole device pixel. The engine
keeps every other primitive — fills, images, shadows, the pin zoom, a settle tween — at a
float position, so a moving block of text stepped in whole pixels while its own card glided,
and the report was that text "does not move smoothly" inside a scrolling or dragged surface.
Measured on the palette card at the same position, before and after: the code rows sat at
their laid-out fractional baselines instead of the rounded ones — the rows moved −0.23,
−0.43 and +0.29 px, each its own fraction, in a capture where nothing else moved. The
baseline now takes the nearest half pixel into the tile and leaves a quarter to the sampler
(see [Subpixel placement](#subpixel-placement)): two phases, not four, because each phase is
another tile per glyph and per size, and a quarter-pixel shift is invisible while a
half-pixel step is not. The decoration rules lost their own snapping for the same reason —
a rule on the pixel grid under letters off it slides against them as the text moves (the
underline invariant in `decorations_follow_the_face_and_stay_inside_their_run` caught it).
Pinned by `a_baseline_between_pixels_reaches_the_destination` (watched red with the baseline
rounded again), `place_run_keeps_the_subpixel_phase_on_both_axes`, and
`vertical_split_lands_on_the_nearest_half_and_leaves_a_quarter_at_most`.

**A phase moved the anchors but not the control points.** `ContourPen` added its horizontal
shift to `move_to`, `line_to`, and each contour's anchor, but `quad_to` and `curve_to` built
their control points from the unshifted coordinates. A TrueType glyph is mostly quadratics, so
every phase variant was a *bent* copy rather than a translated one — the bulge lagged the
anchors by up to the phase itself, and the shape changed slightly each time a run crossed a
quarter pixel. `a_phase_translates_the_outline_instead_of_bending_it` states the invariant: a
whole pixel of phase must produce the same tile one pixel to the right, for `o`, `S`, `e` and
`g`. Watched red on the old pen, where `o`'s tile came out 15 px wide instead of 14. Every
point, control points included, now goes through `ContourPen::place`.

## Traps

| Change | Symptom |
| --- | --- |
| Treat offsets as characters | Panics on slicing; the caret lands mid-character |
| Skip grapheme snapping on IME input | Half-deleted emoji or combining marks |
| Shape a mixed run with one face | Missing digits, or tofu in place of emoji |
| Use `FontiqueProvider::metrics` for layout | A synthetic 0.8 em line box and wrong wrapping |
| Reverse a run's glyphs rather than its clusters | A mark lands one base-advance off |
| Cut a styled run or a selection inside a grapheme | The mark shapes on its own: detached in LTR, and before its base in RTL |
| Infer the base direction per line | A digit line inside an RTL paragraph moves its trailing punctuation to the other end |
| Mask with a fixed bullet string | Every offset after the first character slides, so the caret lands in the wrong place |
| Draw the marks and stop | The secret leaves through copy, the text menu and the input method's surrounding text |
| Give a copy to the window that is about to close | The selection dies with its source: the clipboard is empty the moment the overlay goes |
| Reuse atlas rectangles across an eviction | Ghost or blank glyphs |
| Allocate atlas slots inside the rasterizing threads | Slot order stops matching the serial path, and an eviction can clear entries another thread still holds |
| Drop the parallel batch threshold below 64 | Thread startup costs more than the rasterization; the measured crossover is in the Atlas section |
| Bump the atlas version only where text is shaped | A just-typed formula character stays blank until an unrelated glyph is rasterized |
| Derive a selection's blocks from the block the drag began in | A drag into the next paragraph paints only the first one's bytes |
| Resolve every text primitive regardless of the clip | A long document shapes and places every row it holds on every frame |
| Join covered blocks with no separator | Two paragraphs arrive as one run-on line |
| Re-send surrounding text per preedit update | Composition restarts every keystroke |
| Send the whole buffer as surrounding text | Over the protocol's 4000 bytes libwayland aborts the connection and the window disappears |
| Put `text_input` on a text node | No buffer and no caret: input parameters live on a container, so the text must be its child |
| Call `text()` on a parameterised message | `⟨id⟩` on screen |
| Replace the `NUMBER()` builtin to localise numbers | `add_function` refuses a taken name, and a string result can no longer select `[one]`; use the bundle's formatter hook |
| Build a date as `{day} {month} {year}` | The field order and the month's form are the locale's, not the caller's |
| Give a search field the protocol purpose `search` | text-input-v3 has no such value; its purposes are normal, alpha, digits, number, phone, url, email, name, password, pin, date, time, datetime, terminal |

**Displaying a large document killed the window.** The engine handed the whole buffer to
`zwp_text_input_v3.set_surrounding_text`. The protocol caps that text at 4000 bytes — one
Wayland message is capped, and the spec says so outright — so `demo.md` (9929 bytes) made
libwayland abort with `Protocol error 1 on wl_display@1: message length 9952 exceeds 4096`
and the connection died with it. Chromium and Firefox have the same failure recorded
(`set_surrounding_text` at ≥4096 bytes), and GTK answers it by sending the cursor and as much
of the selection as fits, with text on both sides. `present::surrounding_window` now does
that: half the budget each way around the caret, reaching back for the anchor when it fits,
snapped to character boundaries and never longer than the limit; an anchor outside the window
clamps to its edge. Caught by
`the_demo_documents_ime_surrounding_text_fits_one_request` in
`crates/ui-widgets/tests/demo_doc.rs`, which read 9929 bytes before the change.

## Status

Implemented: font contracts, the fontique provider, shaping and line breaking,
the paragraph's bidi base direction with relative alignment, UAX #9 L3 (a cut
never separates a mark from its base, and reversal is by cluster),
per-cluster emoji presentation, the glyph atlas with per-run batch
rasterization over up to eight threads, IME on Wayland and on Windows
(the TSF document a focused field hands an input method, so its surrounding
text and a delete beside it are served there), with the
input's content purpose and hints (a secret buffer, a time field), hidden
fields (a byte-preserving mask, no copy or cut, no
surrounding text), a read-only selection that crosses blocks (derived spans,
copy one line per block, page-wide select-all), undo and redo over per-owner
snapshot history (grouped by caret motion and by a one-second typing pause),
app-requested edits through `Cx::edit_input`, app-requested focus through
`Cx::request_focus`, reveal held where a composition started
(`Cx::reveal_range_for`), culling of text and maths that no clip can show,
client-side key repeat from the seat's rate and delay, and Fluent with locale
parity tests.

Not implemented: IME on macOS or X11; on Windows a secret field's neighbourhood,
because a secret field keeps the IMM32 path and IMM32 has no surrounding text (a
focused non-secret field is a TSF document and serves both); content purposes
beyond a secret buffer and a time field; COLR v1 paint graphs and SVG-in-OpenType colour; and typing or editing a
read-only selection (it is selection and copy only).

## See also

- [Frame Pipeline](./frame-pipeline.md): where the text pass runs.
- [Layout](./layout.md): how text measurement feeds hug axes.
- [Comments](../conventions/comments.md): why user-visible strings never appear
  inline.
