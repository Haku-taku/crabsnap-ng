# Pin Badges: What a Pin Says About Itself

A badge is a small pill a pin wears: the zoom it is drawn at, whether its
picture is HDR, which file it came from. This page is the badge system's
contract — its model, anchors, visibility rules and built-ins — and the record
of what the first implementation got wrong.

## Problem

A pin states nothing beyond its pixels. Three facts a user needs are
invisible:

- A zoomed pin does not say how far it was zoomed. The wheel changes the paint
  scale and the position, and neither is readable.
- An HDR picture looks like any other until it is compared against the same file
  on a display that shows the difference. `ContentColour` knows the transfer
  function and peak luminance; nothing shows them.
- A pin pinned from a file or from a selection does not say where it came from.
  A pasted file loses its path once the selection that named it ends.

A badge is the place for exactly this kind of fact. It is chrome the canvas
draws, not something a content's layout produces.

## Comparison

`Decl::overlay` with `OverlayPlacement` (`ui/crates/ui-core/src/layout.rs:338`)
already anchors a panel to a node, unfolds it on a side, aligns it and flips it
at the viewport edge. For badges it is the wrong tool twice over.
`place_overlay` anchors on the node's *layout* rect, which a pin's zoom does not
change, so a zoomed pin would place its badge against the unzoomed box. And an
overlay is lifted above its siblings in paint order, so a lower pin's badge
would draw over a higher pin.

The first implementation reused `ui_widgets_md3::Md3Badge` for the pill. It is
the wrong body: an MD3 badge is a spec component with one colour (the error
role) and no material, and chrome over a picture needs both as knobs. The pill
is drawn here instead, from the same token numbers (`badge_spec(true)`) — see
the correction below.

Badges are chrome for the pointer's visit: they arrive when the pointer is on
the pin and leave with it, so a wall of pins stays pictures and nothing else.
Inside a visit, the ordinary fact is always visible and its detail arrives with
the reveal key. The zoom chip is the one badge with a clock of its own, because
it reports an action rather than a standing fact (see below).

## Contract

```rust
/// One pill of chrome a pin wears.
pub(crate) struct Badge {
    /// Stable within the pin: the node id and the memo key.
    pub id: &'static str,
    pub label: String,
    /// Where on the pin it sits.
    pub anchor: Anchor,
    /// What happens when that anchor leaves the output or meets the pointer.
    pub policy: Policy,
    /// When it shows at all.
    pub show: Show,
    /// The pill's fill and the ink on it.
    pub tone: Tone,
    /// Whether the fill sits on a blurred copy of what is behind it.
    pub material: Material,
    /// How solid the chip is; a badge on its way out carries its fade here.
    pub opacity: f32,
}

/// The eight positions a badge takes, read against the pin's drawn rect.
pub(crate) enum Anchor {
    NorthWest, North, NorthEast, East,
    SouthEast, South, SouthWest, West,
}

pub(crate) enum Policy {
    /// Always at its anchor, even when the pin has carried it off the output.
    Forced,
    /// Its anchor when that stays on the output and clear of the pointer; then
    /// a mirror, then a fold across the pin's own edge; nothing when none does.
    Preferred,
}

pub(crate) enum Show {
    /// Always visible: the ordinary fact.
    Always,
    /// While the reveal key is up. Paired with `AdvancedOnly`, the two swap.
    NormalOnly,
    /// Only while the reveal key is held: the detail behind an ordinary badge.
    AdvancedOnly,
}

pub(crate) enum Tone {
    /// The pack's raised surface: chrome that must not shout. The default.
    Surface,
    /// The pack's error pair, the MD3 badge's own colour.
    Error,
    /// The declarer's pair: a fill, and an ink that reads on it.
    Custom { fill: Color, ink: Color },
}

pub(crate) enum Material {
    /// The tone over a blurred backdrop. The default.
    Frosted,
    /// The tone's colour, flat.
    Opaque,
}
```

Invariants:

- A badge is drawn at a fixed size in logical points, whatever the zoom. Chrome
  is read at screen size; a pill that grew with the pin would be unreadable
  when small and absurd when large. The pin's border ring is the one piece of
  chrome that does scale, because it belongs to the card's edge.
- A badge normally sits **inside** the pin's drawn rect, `MARGIN` (6 points)
  from its edges. A pin dragged half off the output must not drag its badges
  onto the desktop where the output clamp would pull them away from the pin.
  When no place inside the pin clears the pointer, the block folds across the
  pin's own edge instead: it then stays on the output, `MARGIN` outside the
  picture, beside the corner it belongs to.
- A badge is inert: it declares `Sense::PASS_THROUGH`, so hit testing never
  stops on it. The declaration is load-bearing — a node with no `Sense` is
  opaque to the hit test, and an opaque chip took the pin's own hover away (see
  the correction below). A press on a badge belongs to the card under it, so it
  still drags, still opens the menu.
- A badge exists only while the pointer is on the pin. The whole set shares the
  card's hover transition (`SKIN_SWAP`), so the border, the shadow and the
  chips arrive and leave together, and a pin with no pointer on it wears
  nothing.
- Badges keep the pin's stacking slot: the canvas emits a pin's card, its ring,
  then its badges, before the next pin. Two pins never interleave their chrome.
- The fill is translucent over a blurred backdrop by default, so a chip never
  punches an opaque hole in the picture it names. Where the pack says the
  backend cannot sample (`frost_allowed`), the tone is drawn opaque instead.
- A chip that has faded out (`opacity <= 0.01`) is not declared at all: an
  invisible node in a stack still takes a slot.

### Anchors

The canvas lays the chips of one *block* — one declared anchor and one policy —
out with `layout(blocks, pin_rect, output_rect, pointer)`. It answers two
questions, in this order.

**Where.** `place` puts the block at its anchor: the corresponding corner or
edge of the block sits `MARGIN` inside the pin's drawn rect, and the block grows
from the anchor toward the pin's centre.

```text
NorthWest  ┌───────────────┐  NorthEast
           │ [HDR]         │
West       │               │  East
           │         [125%]│
SouthWest  └───────────────┘  SouthEast
```

The policy then decides whether the block stays there. `Forced` keeps it, even
when the pin has carried it off the output. `Preferred` walks seven candidates
in order — the declared anchor, its mirrors on x, on y and on both, then the
declared anchor folded across the vertical edge, across the horizontal one and
across both — and takes the first whose block lies inside the output and whose
chips miss the pointer. When none does, the declared corner is clamped into the
output and kept unless the pointer stands in it; a block with no place left is
not drawn.

The pointer is the stronger constraint, which is the reverse of the first rule.
A chip under the cursor is not merely hidden: it takes the pin's hover, the
visit fades, and the chip that took the hover leaves with it — a blink. Folding
is what lets the pointer win without paying for it: the chip stays beside the
visible corner instead of over the pixels there. A block that can neither clear
the pointer nor stand on the output is absent until the pointer moves on, since
either placement would fight the other rule.

**Who shares a corner.** Two declared anchors can resolve to one anchor — a pin
larger than its output mirrors several of its corners onto the one that is
still on screen. Blocks that resolved to one anchor form one stack, in
declaration order, and are placed as one block. Placing them independently
would hide one chip under another; the merged stack grows the way a
same-anchor stack does.

### Order and count

A kind may declare any number of badges: `PinContent::badges` returns a list,
and the bitmap's HDR pair is the first two. The blocks appear in one fixed
order, nearest the anchor first:

1. the built-ins (zoom),
2. the origin badge,
3. the content's own badges, in its declaration order.

A kind that wants its summary above its detail declares the summary first.

### Several badges at one anchor

Chips that share an anchor form a stack that grows from the anchor toward the
pin's centre, along the edge the anchor sits on:

| Anchors | The stack grows |
| --- | --- |
| NorthWest, North, NorthEast | down |
| West | right |
| East | left |
| SouthWest, South, SouthEast | up |

The first chip sits at the anchor; each later one clears the previous chip by
`GAP` (4 points). Sizes come from the previous build (`cx.rect_of`), so a stack
settles one build after it changes, the way a pin's own size does; the first
build a chip appears in places it from an estimate of its label.

The stack is cut at the pin's inner box: a chip that would fall outside the
pin's drawn rect is not drawn, and neither is anything after it at that anchor.
A chip whose place does not exist is worse than a missing chip, and at the
sizes where this happens the pin itself is barely visible. A folded stack is
cut at the output instead, because it stands outside the picture on purpose and
the picture cannot be its limit.

### Visibility: the visit, the reveal key and the fade

Three gates, multiplied into the chip's `opacity`:

1. **The pointer.** The pin's hover progress (`visit`) is every badge's base
   opacity. Hover is `hovered_within` the card, not `hovered()`, so a kind's own
   controls count as part of the pin. The same progress drives the skin, so the
   chips cannot lag the border they belong with.
2. **The zoom visit.** Only the zoom chip has one; its own opacity lands with a
   wheel step and departs after it.
3. **The reveal key.** `Show` picks among the badges already in the visit.

A chip whose product falls to `0.01` or below is not declared, so an invisible
chip never takes a stack slot.

`Show` is read against one held modifier, `REVEAL = Alt`, a constant until the
config gains a key:

| `Show` | Alt up | Alt held |
| --- | --- | --- |
| `Always` | visible | visible |
| `NormalOnly` | visible | hidden |
| `AdvancedOnly` | hidden | visible |

Kinds use both patterns: the HDR pair appends its detail, and the origin chip
swaps its short form for its full one. The canvas does not guess which.

**Engine dependency.** A modifier change repaints only when the build read the
modifiers: `Recorder` records `Dep::Modifiers`, the view keeps the flag, and
`Event::Modifiers` sets `requested` when it is set. The pin canvas has the
keyboard while the pointer is over it (`KeyboardMode::FollowPointer`), so Alt
over a pin reaches it; a client holding an exclusive keyboard takes that away,
and the badge stays hidden until it lets go.

## Built-in badges

The canvas supplies these; every pin has them, no content declares them.

| Badge | Shows | Anchor | Policy | `Show` | Tone | Material |
| --- | --- | --- | --- | --- | --- | --- |
| zoom | the drawn scale in whole percent, while it is not 1 | SouthEast | Preferred | `Always` | `Surface` | `Frosted` |

The zoom badge's label is `{:.0}%` — whole percent, because a step of ×2.18 %
does not need a fraction and a badge is read at a glance. The lattice puts an
octave exactly on a count, so 200 %, 400 % and 50 % are labels the wheel can
produce rather than values it passes near.

### The zoom chip's visit

The zoom chip is a visitor, not furniture. A wheel step lands it at full
(`VISIT_LAND`) and the departure (`VISIT_DEPART`) holds it for 1.1 s after the
last step, then fades it over 0.3 s: a chip that stayed would sit over the
corner the user is zooming into, and the wheel's own position is what the chip
would be covering. Its opacity is one engine animation, so the engine's clock is
the only timer — no build watches the wall clock, and the frames the fade needs
are the fade's own. It is still multiplied by the hover visit: a pointer that
leaves the pin takes the chip with it, and brings it back if the visit has not
expired.

The step has to land rather than ramp. The build after a wheel step declares the
departure again, and a retarget samples the value the arrival reached, which a
one-frame ramp leaves at zero. Landing is also what the gesture is: a notch is
discrete, and the chip has nothing to say about the travel between two notches.

`badge::zoom(zoom, opacity)` declares nothing at all while the visit has faded
out, so a pin left at 2× wears no chip once the user has stopped.

## Content badges

`PinContent::badges(&self, catalog) -> Vec<Badge>`, default empty. A kind that
knows a fact about its own material declares it here.

### HDR (bitmap)

`Bitmap` holds the picture's `ContentColour`. A picture is HDR when its transfer
function can express more than SDR — `TransferFunction::is_hdr()`
(`ui/crates/ui-core/src/colour.rs:98`) — not when `max_nits` happens to be set:
the display's own 203-nit reference travels with a capture and says nothing
about the content.

| Badge | Label | Anchor | Policy | `Show` | Tone |
| --- | --- | --- | --- | --- | --- |
| hdr | `pin-badge-hdr` → "HDR" | NorthWest | Preferred | `Always` | `Error` |
| hdr-detail | `PQ · BT.2020 · 1000 nit` | NorthWest | Preferred | `AdvancedOnly` | `Error` |

The detail is data, built by a pure formatter: the transfer's name, the
primaries' name, and the peak when the file states one. It is capped at 48
characters with a middle ellipsis, because a badge is a chip, not a banner.

The labels `HDR` and `Clipboard` are user-visible strings and live in the
catalogs (`pin-badge-*`); transfer and primaries names (`PQ`, `BT.2020`) are the
standards' own names and are not translated.

## Origin

Every pin carries a provenance, not only the ones pasted from a clipboard:

```rust
/// Where a pin's material came from.
pub enum PinOrigin {
    /// A capture this app took: the output it came from.
    Capture { output: Option<String> },
    /// A file the daemon opened, named as the command or file list gave it.
    File { path: String },
    /// A selection read from the clipboard.
    Selection { mime: String },
    /// A colour the user typed.
    Colour { code: String },
}
```

`Pin::new` takes a `PinOrigin`; the creation sites already know it. The
overlay's pin is `Capture`, `open_all` gives each item `File`, a clipboard
selection is `Selection`, and `open_colour` is `Colour`. `PinInfo.origin`
(protocol v10) is the same field, and `pin list` prints it, so the field a badge
reads and the field a script reads are one fact.

The origin badges:

| Origin | Ordinary (`NormalOnly`) | Advanced (`AdvancedOnly`) |
| --- | --- | --- |
| `File` | the file name | the full path |
| `Selection` | `pin-badge-clipboard` → "Clipboard" | the MIME |
| `Capture` | — | the output name |
| `Colour` | — | — |

They swap rather than stack: a file's path ends in its own name, so the short
form carries nothing the long one lacks, and one chip at a corner is less to
read than two. A capture the user just took does not need a badge saying it is a
screenshot; its output's name is technical and arrives with Alt. A colour pin
already shows its code in the card. Labels are capped at 48 characters with a
middle ellipsis (`abbreviate`), so a deep path does not become a banner.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `apps/crabsnap/src/pin/badge.rs` | `Badge`, `Anchor`, `Policy`, `Show`, `Tone`, `Material`, the zoom and origin badges, the zoom visit's fade, `layout`, `pill`, `abbreviate` |
| `apps/crabsnap/src/pin.rs` | `PinContent::badges`, the origin field on `Pin` |
| `apps/crabsnap/src/pin/content.rs` | the bitmap's HDR pair and its detail formatter |
| `apps/crabsnap/src/main.rs` | the canvas collects the badges, notes the wheel's visit, measures, places and paints them |
| `apps/crabsnap/src/pins.rs` | one `PinOrigin` per pin at placement |
| `apps/crabsnap-core/src/protocol.rs` | `PinOrigin`, `PinInfo::origin`, protocol v10 |
| `ui/crates/ui-core/src/memo.rs`, `ui-platform/src/view.rs` | modifier-driven repaint |
| `apps/crabsnap/locales/{en,zh-CN}/crabsnap.ftl` | `pin-badge-*` |

The pill resolves its colours through the canvas's `Env`, the same scope a pin's
own controls resolve in, so badges follow the system theme rather than the pin's
look. The look is a per-badge decision (`Tone`, `Material`), because only the
kind knows whether its fact is a warning or a quiet annotation.

## Verification

Pure logic, unit-tested in `apps/crabsnap/src/pin/badge.rs`:

- `a_badge_prefers_the_anchor_that_stays_on_the_output` — a pin hanging off the
  left flips its NorthWest badge to NorthEast; a forced one stays; a pin wholly
  on the output keeps its declared anchor; nothing fits, so the block clamps.
- `a_chip_steps_off_the_pointer` — a pointer over the chip sends it to the
  mirror; a pointer elsewhere changes nothing; a forced chip stays.
- `a_chip_with_no_mirror_left_folds_across_its_corner_or_is_not_drawn` — a pin
  whose only visible corner is its declared anchor folds the chip outside that
  corner to clear the pointer; a chip that is still under the pointer there is
  not declared at all.
- `blocks_that_mirror_onto_one_corner_form_one_stack` — three declared corners
  of a pin larger than the output resolve to one, and stack in declaration
  order instead of hiding each other.
- `badges_stack_toward_the_pin_centre_and_stop_at_its_edge` — two chips on
  NorthWest stack down; West and East grow sideways; a chip that no longer fits
  is dropped together with the ones after it.
- `a_chip_is_tinted_glass_unless_it_asks_for_flat_paint` — the default is a
  translucent tint of the tone over a blur; a flat chip is opaque and samples
  nothing; a custom tone carries its own ink.
- `an_hdr_picture_says_so_and_a_plain_one_does_not` — a PQ picture declares the
  error-toned pair, an sRGB one declares none even with a peak set, and the
  detail names the curve, the primaries and the peak.
- `an_origin_names_the_file_it_came_from` — a file names itself and keeps the
  path behind the reveal key, a selection names itself and its MIME, a capture
  states only its output name, a colour says nothing.
- `the_zoom_badge_appears_only_away_from_native` — 1.0 declares nothing, 1.1
  reads "110%", 0.077 reads "8%", and a faded-out opacity declares nothing.
- `a_modifier_change_repaints_when_the_build_read_it` (ui-platform) — the engine
  asks for a frame on a modifier change only after a build read them.

The six-line `Show::visible` match carries no test of its own: it is a table,
and what can go wrong is the canvas passing the wrong flag, which the check
below covers.

Through the engine, headlessly in `apps/crabsnap/src/main.rs`:

- `a_wheel_step_shows_the_zoom_chip_at_full_and_the_departure_lets_it_go` —
  a real canvas and pin on a 1280×720 output, a synthetic wheel event and
  scripted 60 Hz builds: the chip is in the tree at opacity 1.0 on every step,
  and gone after 100 frames.

Through the engine, in a nested session:

- A PQ-tagged PNG and an sRGB one side by side: with the pointer on the first
  it wears a red `HDR` chip at its north-west corner, the second none; holding
  Alt appends `PQ · BT.2020`, with no other input in between, and releasing it
  removes it.
- A pointer that leaves a pin: every chip goes with the border and the shadow,
  and a pin left alone wears nothing.
- A file pinned with `pin add -f`: a `sdr.png` chip at the south-west corner,
  swapped for the full path while Alt is held.
- A pin scaled to 2×: a `200%` chip at the south-east corner, gone after the
  visit, and moved to the south-west corner — stacked above the file name —
  when the pointer is left on the corner it wants.

## Status

Implemented. The zoom, origin and HDR badges, the hover visit, the reveal key,
the pointer rule and the mirrored-corner merge are in; `PinOrigin` rides
protocol v10; the canvas draws `Surface`/`Frosted` chips unless a kind asks
otherwise. The zoom visit's wheel path is covered headlessly, because the
nested session cannot inject a wheel; the earlier nested check at 2× drove the
zoom from a probe, which kept the visit up every frame and so never exercised
the departure. Not yet verified in a nested session: a preferred chip flipping
because the *pin* was dragged off the output (the mirror path is unit-tested and
was seen for the zoom chip at 3×), a folded chip (unit-tested only; a nested
run needs a pin large enough that one corner is all the output shows), and a
content kind declaring `Tone::Custom` or `Material::Opaque` (both are covered
by the pill test, and no built-in uses either).

## Corrections

**The pill started as `Md3Badge`.** The design reused the MD3 component for the
badge body, on the argument that a second pill would drift. The first
implementation showed what the reuse cost: an MD3 badge is painted in the error
role with no material, and the app's chrome needs a quiet default, a per-kind
colour, and the frosted backdrop its other floating surfaces use. The pill is
now drawn in `badge.rs` from `badge_spec(true)` — the same height, padding and
type — so the numbers still come from one place while the surface is the
app's. `a_chip_is_tinted_glass_unless_it_asks_for_flat_paint` pins the material
and tone; a visual check covers what a unit test cannot.

**The zoom chip stayed forever.** The first paint drew the chip at south-east
for as long as the pin was zoomed, which put it over the corner the user was
zooming into — the one place the wheel's anchor wants to show. It is now a
visit: one engine animation, arriving instantly and departing after a delay,
with the fade's own frames as the only clock. Verified in a nested session at
2×: present immediately, gone after about 1.4 seconds.

**The zoom chip arrived on a ramp that never played.** The visit was one
animation declared as `ZOOM_ARRIVE` (0.12 s, and only on the wheel's own frame)
and `ZOOM_DEPART` on every other. The next build retargeted the arrival back
down before its curve had run, and the departure's delay then froze the value
where the ramp had started: a headless canvas measured 0.0000 on the wheel
frame and 0.0027 more per notch, so the chip only crossed the 0.01 declaration
gate after several notches and then vanished at the end of the delay. It read as
"the badge shows when I zoom out far enough" — the number of notches decided it,
not the direction. A step now writes the visit to 1 in a zero-duration leg and
the departure holds it from there, which is the design the doc already described
("arriving instantly"). The test named under Verification fails at its first
`expect` with the ramp put back.

**Two blocks could occupy one corner.** Placing each declared block on its own
let a pin larger than its output mirror its south-west and south-east corners
onto the north-west one, where the two chips drew on top of each other. Seen at
3× on a 640×360 picture over a 2560×1440 output: the file name hid the zoom
percent. `layout` now resolves every block first and stacks blocks that landed
on one anchor, in declaration order; `blocks_that_mirror_onto_one_corner_form`
`_one_stack` pins it.

**Nothing kept the chips off the pointer.** The first paint ignored the
pointer, so a chip could sit exactly where the user was pointing — the zoom
chip over the corner being inspected, the file name over the pixels a user is
about to drag. `Preferred` now treats the pointer as a second obstacle, weaker
than the output: a block that can clear neither keeps to the output. Seen in a
nested session: the zoom chip left the south-east corner for the south-west one
when the pointer was parked on it, and stacked above the file name there.

**The chips claimed to be always visible.** The design's rule was that the
ordinary fact stays on screen and only the detail waits for Alt, with Snipaste's
hover toolbar named as the approach it rejected. On a screen of pins that is
chrome over every picture at once, and the badge's own facts — a scale, a file
name — are only wanted while the user is working on that pin. The rule is now
the pointer's visit: chips fade in with the card's skin and leave with it, and
`Show` chooses among them inside the visit. The zoom chip keeps its own clock on
top, because a scale is about an action rather than a standing property. Seen in
a nested session: a pin left alone wears nothing, and its chips arrive when the
pointer does.

**A chip could blink by taking the pin's own hover.** On a pin whose only
visible corner was its declared anchor, the chips had one place to stand: the
corner the pointer was on. A pill that declares no `Sense` is opaque to the hit
test, so a pointer standing on the chip left `hovered_within` the card and the
visit fell; the chip faded out, the pointer came back over the card, the visit
rose, the chip returned — a blink the user reported as a badge that flickers
when the pointer comes near. Two rules changed. A pill now declares
`Sense::PASS_THROUGH`, so it never takes the pin's hover; and `Preferred` gained
the fold candidates, which stand the block outside the pin's own edge beside the
corner the pointer is at, so a pin showing one corner still gets a readable
badge. If even the fold would sit under the pointer, the block is not drawn and
the badge arrives when the pointer leaves.
`a_chip_with_no_mirror_left_folds_across_its_corner_or_is_not_drawn` states the
chain; the blink itself was seen in a nested session on a pin zoomed past the
output.

## See also

- [Pins: Content, Sources and Editing](./pins.md): the pin, its content kinds
  and the canvas a badge is drawn on.
- [The Screenshot App](./screenshot-app.md): where a capture becomes a pin.
- [Colour and HDR](../engine/colour.md): the `ContentColour` the HDR badge
  reads.
- [Popovers and Tooltips](../engine/popovers.md): `OverlayPlacement`, and why
  badges are not one.
- [Transitions](../engine/transitions.md): the animation store the zoom visit
  rides.
