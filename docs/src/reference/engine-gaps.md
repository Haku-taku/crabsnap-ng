# Engine Gaps

A gap here is a capability the engine does not expose, phrased as what a
control therefore cannot do. It is not a bug list: a bug is behaviour that
contradicts a contract, a gap is a contract that does not exist yet. Controls
that need one either wait, or use a named workaround recorded at the call
site.

## Open gaps

| Gap | Status | Symptom |
| --- | --- | --- |
| Static and non-uniform transform | Partial | Only a uniform paint-time scale exists; slider-lens x/y stretch is faked per shape |
| Stack cross alignment | Partial | `cross_align`/`cross_pos` on a `Stack` are ignored; `stack_align` works |
| Multiline field height | Partial | A text field cannot grow by line count; the MD3 field is fixed at 56 dp |
| Generic backdrop capture extent | Partial | The glass lens can widen its capture; Plain and Acrylic still read the node rect |
| Idle window responsiveness | Unverifiable | A desktop may still label an idle window "(not responding)"; nothing in the repo decides it |

## The ones a control author meets

**Transform.** `ui_core::draw::Transform` is a uniform scale plus an
offset, applied to paint and to hit testing, never to layout. That covers
presence entry/exit and a slider lens that only magnifies. It does not cover
per-axis scale, rotation, or an arbitrary origin; a shape that needs those
recomputes its own geometry, which is correct but duplicates
the shape math.

**Parent constraints.** `Widget::decl(&self, cx)` receives the context, not
the space the parent will give it, because build runs before layout.
`Cx::constraints_of(id)` answers with the slot the parent offered, as measured
last frame; a control sizes itself from that, and a resize reaches it one frame
later. `Cx::text_size` and `rect_of` are the other build-time facts.

**Stack alignment.** A `Stack` places children with
`stack_align`/`stack_align_axes` (`OverlayAlign`: start, center, end).
The older `cross_align`/`cross_pos` setters still exist and still do
nothing on a stack. Fractional stretch is not expressible.

**Capture extent.** `LiquidGlass` carries `capture_size`/`capture_scale`,
and the renderer widens the read bounds and resamples the larger region into
the lens texture. `BackdropEntry` has no generic capture field, so acrylic
and plain backdrops still read exactly the node rectangle.

## How to record one

- Add the row here when a control's need reveals the gap, with the symptom
  a user would see.
- A workaround stays at the call site with a comment naming this page; do
  not hide it in a helper that looks like a normal path.
- When the capability lands, delete the row and record the correction on the
  subsystem page that now owns the contract.

## Corrections

**Parent constraints closed as a one-frame hint.** The row said a control could
not learn the space its parent offers, so the host passed a width and a resize
left it behind. `Cx::constraints_of(id)` now returns the constraints layout
measured that node under last frame: the parent's slot, not the size the node
chose inside it. Build still runs before layout, so a resize reaches the control
on the following frame — the same lag `rect_of` has; that is the honest limit of
a decl-before-solve engine, and an exact answer would need to build a subtree
from inside layout, which nothing needs yet. Pinned by
`a_control_reads_the_slot_its_parent_offers`.

**Text tracking closed.** The row said there was no letter-spacing channel. It
is `TextStyle::letter_spacing` (pixels per cluster, after the last one too, as
CSS counts it), baked into the shaped advances so the wrap, the painted glyphs
and the caret boundaries read one width; the estimator adds it per character.
The control-side step — choosing MD3/Fluent tracking values per type role — is
not part of the engine gap and remains open in the control work. Pinned by
`letter_spacing_widens_the_measured_line`.

**Flow-item self alignment closed, with one half deliberately left inert.** The
row asked for `align_self` and `justify_self` on a child of a `Column` or `Row`.
`align_self` now overrides the line's cross alignment for that item (CSS
`align-self` is the cross axis, whichever way the container runs);
`justify_self` stays a grid property because a flow item's main slot is its own
main size, so there is no slack to place it in — CSS ignores `justify-self` in
flex layout for the same reason. Pinned by
`a_flow_item_aligns_itself_across_the_line` (`layout-flexbox.md` records the
rule).

**Apple's content materials are a name table, not an engine primitive.** The row
said sidebars and overlays could only be opaque fills. The engine already had
the pieces a material needs — a backdrop blur, a translucent fill over it, and
an opaque fallback gated on the capability — so what was missing was somewhere
to read the six names from. `StyleSystem::material(name)` now answers with a
`MaterialSpec` (blur sigma, wash, fallback); HIG resolves all six from the
`materialRecipes` block of `hig-tokens.json`, `Style::material_named` turns one
into `Material::Frost`, and a pack built on a material-carrying system defaults
to `regularMaterial`. A surface with no system, or a system with no materials,
still says `None` and keeps its solid fill.

Apple publishes no numbers for these (`appleNumericBudgets: "none published"`)
and the HIG page itself ranks them by meaning, not by value, so the sigma
(10–26) and wash alpha (0.16–0.82) are our derivation: both grow with
thickness in the published order. Checked on the HIG system page over a line of
text, where the thinnest chip leaves the text legible and the thickest hides it
(nested session, light theme), and pinned by
`content_materials_thicken_in_the_published_order`.

The same change removed a generator trap: `hig_data.rs` carried hand-added
colour constants its script never emitted, so regenerating it silently deleted
them. They now live in the JSON with the material recipes, and the script
reproduces the checked-in file byte for byte.

**The glare row was mis-filed: the engine had every primitive.** The row said a
glass press could only lift uniformly, so it read as a missing engine contract.
It was not. `Cx::pointer()` gives the build-time pointer, `offset` is a
paint-time translate, a container with `clip` and `corners` gives a rounded
window, and `Effect::blur` on a subtree blurs it; the glass button now places a
soft white spot at the pointer inside that window. What was missing was a
control using them, and the row's wording sent every reader to the engine
instead. The one trap found on the way is geometric: the spot lives in the
inflated face, so its offset must subtract the face's own `x`/`y` or it trails
the finger by half the press growth — measured at 8.5 dp at full press.
Checked by the `glass button glare` selftest line: the spot is centred within
1 dp at press, follows a moved pointer, and is gone after release.

**Two rows described work that had already partly landed.** The stack row
said alignment was ignored outright, while `stack_align` had been implemented
and only the older setters were dead; the capture row said materials could
not ask for a larger region, while the glass lens already did. Both were
written from the control that hit the limit, not from the engine. The status
column now distinguishes open, partial and unverifiable, and each
partial row names the part that works.

**A gap was kept open after the capability arrived.** The transform row
once said no transform primitive existed at all; a uniform paint-time scale
had landed with presence animations. The row now says which transform is
missing, because "no transform" and "no non-uniform transform" send a
reader to different work.

## See also

- [Parity Plan](../controls/parity.md): which control waits on which gap.
- [Layout](../engine/layout.md): the constraint model a parent could pass down.
- [Presentation Targets](../engine/present-targets.md): where a capture is read.
