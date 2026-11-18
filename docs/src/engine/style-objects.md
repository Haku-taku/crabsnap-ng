# Style Objects

A control never invents a colour, a duration or a radius. It reads a `Style`
snapshot that was resolved before it ran, and the pack's promises — every
common token, the typography table, the material, the motion vocabulary —
are already final. The decision in front of you is narrow: common token,
family role, or a constant owned by the control.

## What a style object is

`ui_style::Style` is immutable and `Send + Sync`. It is built only through
`StyleBuilder` and never mutated afterwards, so a widget can hold a reference
across a frame.

| Field | Holds |
| --- | --- |
| `common` | The cross-system plane: 23 colours and 2 metrics, each with a source |
| `roles` | The family's own role dictionary (`onSurface`, `glassFallback`, …) |
| `motion` | Five named `MotionToken` slots |
| `metrics` | Tier metrics: control, surface, overlay |
| `controls` | Per-control envelopes; populated by Fluent only |
| `type_table` | Five named text styles |
| `material` | The default surface material, or `None` for a solid fill |
| `device` | What the backend can do (backdrop, reduced transparency) |
| `palette` | The live native vocabulary, for roles the curated list missed |
| `selection` | The text-selection painter, or the engine's flat rect |

`common` is `None` only for the hand-written legacy pack.

Adding a field means editing every struct literal outside the builder, including
tests. Extend `StyleBuilder`; do not construct a `Style` by hand.

## Three resolutions

| When | What is decided |
| --- | --- |
| Build | Every common colour and metric, native roles, tier metrics, type styles, material, capability |
| Resolve, per scope | Theme-transition blending between two appearances |
| Build the control | State overlays, role lookups, material choice, spec to `Decl` |

Build-time work (HCT palettes, 49 roles) is expensive and scope-invariant, so it
happens once. Interaction state changes per frame. Value equality matters
because repaint and caching compare snapshots.

During a theme transition `Style::resolve` interpolates colours and
common-plane values, and non-colour tokens — motion, metrics, controls,
typography, material — switch at the midpoint. Half a duration or half a sigma
is not a value; do not add them to the blend. Endpoints return a clone.

## Reading it from a control

A control asks the environment, not a global:

```rust
let scope = cx.env().scope().unwrap_or(home);
let options = cx.env().find::<Md3Options>();
```

- `Env` is an immutable chain of opaque parameter layers. `find::<P>()` walks
  outward by type; `scope()` returns the nearest layer that states an
  appearance.
- `StyleScope` is appearance, `theme_from`, and `theme_t`; `settled()` drops a
  finished or no-op transition.
- Propagation is a pull: the app rebuilds the root `Env` each frame, and a
  subtree that needs a different appearance calls `cx.at_scope`.
- While a theme tween runs, the app invalidates declarations every frame so
  controls rebuild against the new scope.

`Decl::env` and its inheritance fold exist, but no production build writes them.
Scoping is per-build through `cx.at_scope`; do not expect a scope to arrive
from the declaration tree.

## Common plane versus family roles

The common plane is a closed list with explicit refusals: `Resolution` is
`Resolved` (with a `Source`) or `Absent(reason)` / `Unmapped`. A system that
cannot fill a role says so instead of inventing a colour.

- Read it with `colour`, `metric`, `colour_or`, `metric_or`. Use the `_or`
  form when a family pack may legitimately lack the value.
- A surface that must *hide* what it covers reads `opaque_or`, not
  `colour_or`: it resolves the role over `Surface` the way the system's own
  compositor would. WinUI expresses `SurfaceRaised` as white at five percent —
  a card tint meant to sit on a page — so taking it literally on a menu makes
  the menu a veil over the rows behind it.

Each system exports `pack()` and `pack_of(appearance)`. `pack()` is the light
pack and is only correct for a surface that is deliberately theme-free; a page
in the running app must take the frame's appearance
(`StyleScope::of(&root_env()).appearance`) and ask for `pack_of`. A control
that draws a surface never restyles what it holds, so the caller picks the
text role: `onSurface` for a title and `onSurfaceVariant` for a body, dimmed by
the disabled content opacity when the control is disabled.
- Family roles come from `roles.get(RoleName("…"))`; a native token the
  curated list forgot comes from `palette.native_colour(..)`.
- MD3 maps all 23 common colours; WinUI leaves 3 unmapped; the web Fluent
  adapter leaves the scrim unmapped; HIG synthesizes all of them and marks the
  source `Derived`.
- A generic control reads the common plane. A family control reads its own
  token table; MD3 and Glass controls read no `CommonColor` at all.

Two Fluent implementations share `SystemId::Fluent`: the family pack builds on
`WinUISystem` while the gallery's system page shows `FluentSystem`. Their
colours and capabilities can disagree; do not compare across the two.

## Where a constant belongs

| Kind of control | Reads |
| --- | --- |
| Generic (`Button`, `List`, `Panel`) | `CommonMetric` and the tier metrics |
| Family (MD3, Fluent, Glass) | That kit's spec/token table, then tier metrics for shape |
| Per-control envelope (Fluent) | `ControlMetricsTable` |

Do not read `metrics.control.radius` from a generic control: it bypasses the
system's own medium step. Ask for
`metric_or(CommonMetric::Radius, …)` instead. `ControlMetricsTable` is populated
only by Fluent; MD3 and Glass leave it empty.

## Materials

`Material` is the surface recipe: `Solid`, `Frost`, `Acrylic`, `Mica`, `Glass`.
The questions it answers are what to sample behind the surface, what tint to
lay over it, and what to draw when sampling is impossible.

- `fallback_color()` is the opaque stand-in; `backdrop()` exists only for
  Acrylic and Glass; `sigma()` is `None` for Solid and Mica.
- `frost_allowed(device)` is `backdrop && !reduced_transparency`. Capability
  can only disable the blur path; it never picks a different recipe.
- `Panel` is the consumer: fill = the pack's material or the `glassFallback`
  role; with a sigma and capability it blurs (Frost adds a tint fill, Acrylic
  and Glass stay clear); otherwise it paints `fallback_color()` with no filter.
- `Mica` is not a filter: it draws the wallpaper image plus a tinted layer,
  even when the backdrop is unavailable.

A system that publishes a material family supplies its pack's default: the
builder asks it for `regularMaterial`, converts the answer with
`Material::from_spec`, and an explicit `.material(..)` still wins. A system with
none — MD3, WinUI — leaves `None`, so its panels stay solid.
`Style::material_named(name)` resolves any other name for a surface that wants
a different thickness than the pack default.

No production builder calls `.device(..)`, so every pack still carries the
baseline capability. Family kits bypass `Material` and emit their backdrop
material directly, with no capability or fallback gate.

## Motion

`MotionName` has five slots: `StateChange`, `Quick`, `Standard`, `Slow`,
`ColourChange`. `MotionToken` is one duration and one curve.

A shorter way back is declared where it is used, not on the token:
`AnimSpec::down(duration, easing)` gives the downward leg its own timing, and
the curve is mirrored when no leg is declared. Curves the families share are
constants rather than table entries — `ui_style::motion::{EMPHASIZED,
EMPHASIZED_DECELERATE, WINUI_SPLINE, CSS_EASE_OUT, EASE_OUT_BACK}`, plus
`STANDARD`, `EMPHASIZED_ACCELERATE`, `FAST_OUT_SLOW_IN` and
`DESTINATION_SELECT` beside the MD3 controls that read them. `EMPHASIZED` is
Material's full two-part curve (Flutter's `easeInOutCubicEmphasized`), for
motion that begins and ends on screen; `EMPHASIZED_DECELERATE` is the separate
single-cubic curve an element enters on, and `EMPHASIZED_ACCELERATE` the one it
leaves on.

A pack's numbers are also data: `MotionScale` carries the same durations and
CSS control pairs for inspection. Its named lookups are
`Motion::{duration_ms, curve}` on the capability, and no control calls them
yet.

`MotionTable::material`, `::fluent` and `::winui` exist; only the Fluent
pack calls `.motion(..)`. MD3 and Glass get the legacy 150 ms table and do
not read it — their controls hardcode `AnimSpec` constants, which are the
authority for that family and not copies of the table. A new generic control
reads `pack.motion`; a family control keeps its own constants, or the two
clocks will disagree. Shared *values* still have one home: the curves in
`ui_style::motion` and the MD3 ones in `ui_widgets_md3::motion`, so a control
that needs the same curve twice neither repeats a literal nor grows a second
copy of the table.

## Caching

Each kit owns a `StyleCache` keyed by
`(appearance, theme_from, round(t * 255))` after `settled()`. On a hit it
returns a clone; at 64 entries it clears the whole table. A poisoned lock
degrades to rebuilding.

Underneath, `pack_of(appearance)` memoizes a whole pack per appearance in a
`OnceLock`, so a system builds at most two snapshots regardless of how many
scopes ask.

The cache key omits the kit, its options, the device and the system seed. That
is safe only because each kit owns its cache and `StyleScope` has one axis. A
new input that is not in `StyleScope` makes every cached entry silently stale.

## Traps

| Change | Symptom |
| --- | --- |
| Add a `Style` field and fix only the builder | Struct literals in tests and `legacy_pack` stop compiling |
| Interpolate motion or metrics across a theme change | Half a duration; the midpoint test fails |
| Read `pack.metrics.control.radius` in a generic control | The system's radius step is bypassed |
| Share one `StyleCache` between kits | Wrong colours, because the key has no kit |
| Add a scope input without extending the key | Permanently stale styles |
| Expect `Decl::env` to carry a scope | Nothing writes it; the control resolves against the root |
| Compare two packs of the same system with `==` | Palette comparison is `Arc::ptr_eq`, so rebuilds differ |
| Forget `.motion(..)` in a new pack | The 150 ms legacy table, silently |
| Add a `Material` variant and skip `Panel`'s tint arm | The new variant drops its tint to transparent |
| Read `OnSurface` for ink over a frost | Contrast that assumes an opaque surface |
| Call `pack()` in a page that follows the theme | The light pack's near-black text on the dark surface (and the reverse) |
| Give a surface-owning control a fixed content ink | The card reads in one appearance and not the other |
| Read `SurfaceRaised` on a menu or dialog | WinUI's five-percent card tint becomes a veil: the rows behind show through the surface |

## Corrections

**Style scope was documented as travelling on the declaration tree.**
`Decl::env` exists and its inheritance is folded during apply, which made the
claim look true. In practice no production build writes `Decl::env`; a control
that read the tree would find no scope at all. Scoping is per-build through
`cx.at_scope`, and the environment chain is the only carrier.

**A raised surface was read as if it were opaque.** WinUI's
`CardBackgroundFillColorDefaultBrush` is white at five percent alpha: a card
tint that the system composites over the page, not a colour. Reading
`CommonColor::SurfaceRaised` literally made the picker's Fluent dropdown, and
its dialog, a veil — the folder rows showed through the surface and the text
over them could not be read (measured: `Surface` #202020 at alpha 255 against
`SurfaceRaised` white at alpha 13). `Style::opaque_or` now resolves a covering
role over `Surface` with `Color::over` (source-over, straight channels, the
same blend the painter applies), so MD3 and Glass answer their own opaque
values unchanged and Fluent answers the composited colour, about #2B2B2B. The
fix is at the role read, not a special case in the picker: every menu or dialog
that has to hide content asks the same question.

**MD3's hover and pressed were container tokens.** The common plane's
`SurfaceHover` and `SurfacePressed` were mapped onto `surface_container_high`
and `surface_container_highest`. The picker's dialog is `SurfaceRaised`, which
MD3 also answers with `surface_container_high`, so a hovered file row was
painted in exactly the colour it already stood on and showed no highlight at
all, in both appearances; on the sidebar the same token painted an opaque
container block instead of a wash. Material states are state layers, not
containers: the answers are now `on_surface` at the spec's 8% (hover) and 10%
(pressed) opacities, carrying their source. The picker's hover fills fade that
layer by alpha (`faded(lift).over(base)`) rather than lerping from transparent,
which would drag the layer's own channels towards black on the way in.
`the_state_layers_are_translucent_and_take_their_ink_from_on_surface` fails if
either answer becomes opaque, and the picker demo shows the row highlight in
both appearances.

**The style cache was described as keyed by every input.** Its comment says
the key is "every input that can change the result". The key is appearance,
the appearance faded from, and the quantized step — no kit, options or device.
That is sound only while each kit owns its cache and the scope has one axis;
the comment states a guarantee the key does not carry.

**Material defaults arrive from the system, not from the builder.** The builder
once left `material` at `None` for every system pack, so a HIG-styled app could
only paint opaque panels; the note that "no production builder calls
`.material(..)`" was true and hid it. The system now owns the recipe
(`StyleSystem::material`) and the builder takes its `regularMaterial` as the
default, which is the only way an Apple surface and an Apple token can agree.
Apple publishes no numbers for the family, so the six values are our
derivation; the ordering contract is pinned by
`content_materials_thicken_in_the_published_order` in `ui-design`.

**The motion token carried a reverse leg nobody could fill.** `MotionToken`
held an optional `reverse: AnimSpec` that only `reverse_spec()` read, and no
table ever set it: the direction's timing is declared at the use site, via
`AnimSpec::down`, with the curve mirrored by default. Two layers named one
fact, and the unused one invited a second way to say "shorter way back" that
would have silently run a mirrored duration. The field and its reader are
gone; `MotionScale` lost its four accessors for the same reason — they had no
callers, and the capability's `scale()` is the one place to ask.

**Standard and slow were documented with the wrong Material curve.** The
emphasized curve was named `EMPHASIZED_DECELERATE`, and `material()` used it
for both the 300 ms and 400 ms slots. Material's entering curve
(`emphasizedDecelerate`, `0.05, 0.7, 0.1, 1`) is one cubic; the constant holds
the full two-part emphasized curve that motion beginning and ending on screen
uses. The slots now follow Material's pairs: `standard` at 300 ms runs the
standard cubic, `slow` at 400 ms runs the decelerate token, and the MD3
dialog's 400 ms entrance reads the same constant.

**The material state layer ran an eased curve.** `material()` put the standard
cubic on `StateChange`, but a state layer is a 100 ms linear fade
(`kRadialReactionDuration`) — the MD3 controls' own `REACTION` constant, and
the comment already said the layer fades linearly. An eased hover reads as the
wrong family motion. `state_change` now runs `Linear` at 100 ms, matching the
colour-change slot and the MD3 authority; the MD3 controls keep their own
constants as documented, and the table no longer contradicts them. Checked by
`the_material_state_change_is_linear`.

**WinUI's row states were control fills.** `SurfaceHover`, `SurfacePressed`
and `SurfaceSelected` answered `ControlFillColorSecondary` and
`ControlFillColorTertiary` — the alpha fills a control lays down as its own base
over the page, not washes over a surface that already has one. Those light
values are near-white, so over the picker's raised dialog the selected row
measured #F9F9F9 against #FBFBFB: a 2/255 step, and a user could not tell a
marked file from an unmarked one. Pressed and selected also named the same
brush, so the two states were one colour by construction. Hover and pressed now
use the subtle fill family (`SubtleFillColorSecondary`/`Tertiary`) that WinUI's
own list and navigation rows use, whose light column darkens the page.
Selection cannot come from that family: WinUI pairs its subtle selected fill
with an accent indicator pill, which the common plane has no role for, so the
answer is the accent (14%, the HIG convention's alpha) with its own
`Convention` source; the row then reads as a tinted fill in both appearances.
`a_selected_row_reads_against_the_row_it_marks` composites the fill over the
dialog and fails below an 8/255 step — it measured 0.7 under the old mapping.

## Status

Implemented: the ten-field snapshot and builder, the common plane with explicit
refusals, theme blending between appearances, per-kit caches with pack
memoization, five motion slots, the material vocabulary with a capability gate,
and per-system adapters for MD3, WinUI, web Fluent and HIG.

Not implemented: device wiring in every system pack; runtime contrast, depth and
writing; `ControlMetricsTable` outside Fluent; a single Fluent system shared by
pack and gallery. MD3 and Glass keeping their own motion constants is a decision,
not a gap — see the section above.

`motion()`, `type_style()` and `native()` accessors are not a gap. `motion`,
`type_table` and `palette` are public fields, every reader takes them directly,
and an accessor would only rename the same lookup. `ink()` is the one reader
that wraps two planes (curated role, then native palette); that shape is
planned to go away — see below.

## Planned: narrowing `ink()`

`Style::ink(&'static str) -> Option<Color>` answers from two vocabularies: the
curated role dict first, then the live system's native palette. Nothing types
the two apart, so a name from the wrong plane falls through and paints
something plausible instead of failing. Every control also reaches it through
one of two near-identical wrappers (`role` in MD3, `role_of` in Fluent), each
carrying its own magenta debug fallback.

The plan splits the planes and then removes the combined reader:

1. `Style::role(RoleName) -> Option<Color>` reads the curated dict alone and
   `Style::native(&str) -> Option<Color>` reads the palette alone. `ink` is
   re-expressed as role-then-native, so nothing changes at this step.
2. Each caller moves to the plane it means: the control wrappers and the
   theme blend read `role` (a control's names are its system's curated roles),
   while the few names that exist only natively (`colorNeutralBackground1`,
   `ButtonBackground`) read `native`.
3. `ink` has no caller left and is deleted, so a cross-plane read cannot come
   back without naming its plane.

Status: not implemented. The wrappers keep the magenta fallback and the
`debug_assert` naming the missing role, so a wrong name stays visible.

## See also

- [Architecture](../architecture.md): which crate owns a token.
- [Transitions](./transitions.md): how a resolved style reaches a frame.
- [Comments](../conventions/comments.md): user-visible strings go through
  Fluent.
