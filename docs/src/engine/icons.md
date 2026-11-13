# Icons

An icon is text. It is one glyph from a named family, shaped with variable
axes, measured and painted by the same text pass as a label. That is not a
shortcut around an icon system; it is what makes an icon inherit the font
stack, the atlas and the scaling rules without a second pipeline.

## The source

`IconSource` has two variants:

| Variant | Carries |
| --- | --- |
| `Glyph` | `family`, `glyph` (a `char`), `fill`, `weight`, `grade` |
| `Custom` | `fn(Id, Color) -> Decl`: an app-drawn mark |

`IconSource::glyph(name, set)` maps a Material Symbols name to its
private-use codepoint through a generated table. `set` picks `Outlined`,
`Rounded` or `Sharp`; the name lookup is the only part that knows about
Material Symbols, since `Glyph` itself takes any `Family`.

A name that is not in the table panics. That is deliberate during
development: a silently substituted icon is harder to notice than a crash in a
test.

## Why codepoints, not ligature names

Material Symbols also exposes every icon as a ligature (`add`,
`arrow_back`). The font was measured rather than assumed: 4299 ligature
entries, not the "708, multi-word only" an earlier survey claimed.
Ligatures are complete, but they depend on `rlig`/`rclt` being enabled and on
the shaper keeping the name unbroken.

A private-use codepoint is the stabler path: one codepoint, one glyph, a
fixed one-em advance, no feature selection. The generated table is the
mapping, and a test checks a few entries against the font's own cmap, because a
drifted table draws the wrong icon without any compile error.

## Variable axes

`FILL`, `wght` and `GRAD` are set on the face before shaping:

- `FILL` runs 0 (outlined) to 1 (filled); fractional values are an animation.
- `wght` is stroke width, not a face choice: 700 here does not select a
  Bold face.
- `GRAD` is emphasis, typically -50 to 200.

A face instance is interned with its axes, so `FILL=0` and `FILL=1` are two
`FaceId`s and the glyph cache needs no extra key dimension. Without that,
the two fill states would share one atlas slot and the later rasterisation
would overwrite the earlier.

The provider's `instance_axes` merges requested axes onto the face's own;
`Variations::NONE` returns the face unchanged.

## The slot

The glyph is laid out in a `size`-square em box and centred there by its advance.
Material Symbols, Fluent System Icons and SF Symbols all advance one em, so for them
the centring changes nothing. A set that draws its narrow marks on a smaller canvas
would otherwise sit against the slot's left edge: Segoe Fluent Icons advances 0.6 em
for some glyphs.

## Colour icons

A colour glyph reaches the same atlas emoji use. COLR v0 outlines and CBDT/sbix
bitmaps rasterise to premultiplied RGBA, and the painter ignores the slot's ink, so
the icon keeps the set's own palette. COLR v1 paint graphs are not rendered: swash
0.2 resolves version 0 only, so a v1-only set draws its version 0 base or nothing.
Checked by registering a COLR face as a family and drawing one glyph through
`IconSource`: the prim came out `GlyphKind::Color`, and the colour atlas held 544
distinct colours for it.

## Registration

`FontiqueProvider::register(name, data)` admits a bundled font under its
family name. Without it, a packaged icon font cannot resolve at all and the
application depends on the system having the font installed. Register once at
startup, before the first frame; the provider enumerates the system stack
separately.

An app that opens its surfaces through `Host` does not hold a loop to register
on: it declares the faces in `OpenOptions::fonts`, and the backend registers
them on the surface's loop before the first frame. `crabsnap` ships the
Material Symbols variable face that way, under `IconSet::Outlined`'s family
name, because no Windows machine has it installed.

## When not to use a glyph

Morphing icons — a FAB's plus rotating into a close mark — stay hand-drawn.
The vendor's own animated icon is drawn as paths, and interpolating two glyphs
is not a font operation. Draw with square line caps: the reference uses them,
and rounded caps read as a different icon at small sizes.

## Traps

| Change | Symptom |
| --- | --- |
| Choose a face by "the font has this glyph" | Material Symbols maps ASCII to real placeholder glyphs, so it passes and then swallows all Latin text |
| Key the glyph cache on family instead of the axis-instanced face | Filled and outlined icons overwrite each other in the atlas |
| Enable a ligature path without checking features | Icon names render as literal text when `rlig` is off |
| Split a multi-codepoint sequence across faces | Half an icon: a component is shaped by the wrong font |
| Rely on a system-installed icon font | Missing icons on a machine without it; register the bundled face |
| Draw a morphing icon as two glyphs | No interpolation; the two states pop |
| Draw a colour glyph from the coverage atlas | The icon is one flat tint of the slot's ink instead of the set's colours |
| Expect COLR v1 paint graphs from swash | The glyph shows its version 0 base, or nothing |
| Leave an icon at its pen origin and assume a one-em advance | A set with 0.6 em narrow marks draws them against the slot's left edge |

## Corrections

**Icons measured too tall and sank below the baseline.** An icon was a text
node with the usual 1.4 em line box, while symbol fonts fill exactly one em,
so the box was taller than the glyph and the extra height pushed it down.
`icon_extent(size)` returns the square em box, and an icon declares that fixed
size instead of a line box.

**Two items were listed as unimplemented after they already worked.** The Status
line claimed colour (multi-layer) icon fonts and per-icon advances were missing.
Drawing an icon through a registered COLR face produced a `GlyphKind::Color` prim
with 544 distinct colours in the colour atlas: the text pass had rasterised colour
glyphs all along, and no branch in the icon path treats one differently. The advance
half was real, in the other direction: an icon was drawn at its pen origin, so a
glyph advancing 0.6 em landed 3 px from the centre of a 32 px slot (Segoe Fluent
Icons). The slot now centres the line's advance, which leaves a one-em glyph where
the pen put it. Morphing stays out of scope, but as the design decision above rather
than a missing feature. Caught by
`an_icon_is_centred_by_its_advance_and_a_full_em_glyph_does_not_move` — watched red
at centre 3 of 16 before the alignment was added — and by
`an_icon_in_a_colour_font_paints_the_fonts_own_colours`, which skips on a machine
with no colour face.

**The ligature survey was wrong.** A summary said Material Symbols had 708
ligatures and only for multi-word names; measuring the font found 4299
entries, one per icon. The conclusion survived (codepoints are still the
stabler path), but the claim had to be re-derived before it could be used.

**"The glyph exists" was used as a coverage test.** Material Symbols maps
ASCII to real placeholder glyphs, so `gid != 0` is true for Latin text.
Combined with a global `(script, weight, italic)` face cache, one Latin run
resolved to the icon face and every later Latin run tried it first. Coverage
is now decided by script and family, never by glyph presence.

**A fill state shared an atlas slot.** The cache key was family, glyph, size
and subpixel phase. Adding the `FILL` axis without putting it in the face
identity made filled and outlined icons the same entry; the axis is now part of
`FaceId`.

## Status

Implemented: glyph sources with `FILL`/`wght`/`GRAD`, custom marks, SF
Symbols names, bundled font registration, axis-instanced faces, the shared
text atlas, colour icon fonts (COLR v0 and bitmaps) and per-icon advances.

Not implemented: COLR v1 paint graphs, and animated glyph-to-glyph morphs,
which are rejected by design rather than pending — see
[When not to use a glyph](#when-not-to-use-a-glyph).

## See also

- [Text and Input](./text-and-input.md): shaping, faces and the atlas.
- [Style Objects](./style-objects.md): where an icon's ink comes from.
- [Design Tokens](../reference/tokens.md): icon size slots in the
  generated tables.
