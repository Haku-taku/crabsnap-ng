# Fonts

The engine never knows that Windows uses Segoe UI, macOS uses SF Pro, or
Linux asks fontconfig for `sans-serif`. It states a request — a family, a
weight, a script — and a provider answers with a face. The boundary sits
between the request and the face, not between a font file and a string of text.

## The request

`FontRequest` carries `families` (an ordered list of exact names), `generic`
(`SansSerif`, `Serif`, `Monospace`), `weight`, `italic`, and `script`. A
named family is an exact query; a miss returns `None` instead of
substituting. A generic family is the platform's default, whatever the
platform decides that is.

The distinction matters: fontconfig's `fc-match Arial` returns the system's
substitute, while fontique's `family_by_name("Arial")` returns not-found
when Arial is absent. We use exact lookup for names and platform defaults for
generics, so a typo renders in the fallback face and is visible, rather
than silently becoming another family.

## Who chooses the face

The engine names no font. `TextPass::new` resolves the platform's own choice for
`GenericFamily::SystemUi` and ships an empty preference list, so what a fresh app
draws with is whatever the platform calls its UI face.

A host narrows that choice, in two separate steps:

| Step | Call | What it does |
| --- | --- | --- |
| Supply bytes | `AppLoop::register_font(name, bytes)` | Admits a family the engine ships no copy of |
| Prefer a family | `AppLoop::set_font_stack(&["…"])` | Tries these before, not instead of, the platform default |

`AppLoop::font_families` returns the installation's family names plus every
registered one, which is what a font picker lists; `AppLoop::font_stack` reads
back the preference. A name that is not installed or registered resolves to
nothing and the platform default stays in force, so a stale saved preference
cannot leave text unresolved.

Both steps may run at any time, and both drop what was derived from the old
choice: the per-family face cache, the per-script face cache, every measured
wrap, every shaped span, and — through the view — every cached declaration. A
font change is therefore visible on the next frame, not after the app restarts.

The resolution order for one run is the family it names, the generic family it
asks for, the preferred families, the platform default, then the base stack.
`set_font_stack` moves only the third slot: an icon run that names its family
still wins, and `Monospace` runs still get the platform's monospace before the
preference.

## The provider

`FontProvider` in `ui-text` is the only platform-facing text trait. Its
methods fall into four groups:

| Group | Methods |
| --- | --- |
| Enumeration | `name`, `family_names` |
| Resolution | `resolve`, `fallback_chain`, `has_glyph` |
| Metrics | `metrics` (at size 1.0), `advance` |
| Faces | `face_variation`, `face_index`, `face_axes`, `instance` |
| Registration | `register_family`, `face_data` |

The fallback chain is per script and genuinely platform-specific: fontconfig
applies configuration rules, DirectWrite uses the system fallback map,
CoreText asks `CTFontCreateForString`. A cross-platform guess cannot replace it.

`instance(face, axes)` interns a variable-font instance and returns a
`FaceId`. Because the axes are part of the face identity, every cache keyed on
`FaceId` stays correct without another dimension. `face_index` must travel with
the bytes: a collection or variable file holds several faces, and parsing
byte zero always yields the first.

`register_family(name, bytes)` admits a bundled font and wins over a
system family of the same name. A name whose bytes were already admitted is a no-op: the
database is shared per thread and keeps every admission for the process, so the next surface
handing over the same file must not add a second. Bytes that differ get an internal family
name of their own, because fontique can append a family but not drop one. `face_data` hands
the raw bytes to the rasterizer.

One provider exists, `FontiqueProvider`, over fontique's per-platform
backends. There are no per-platform provider crates.

One database serves every surface. Which fonts exist, and which faces back them, is a fact
about the machine, so `ui_text_fontique::shared_system()` builds the provider once per thread
and every `TextPass` on that thread shares it. What a pass keeps for itself is everything
derived for one surface: the glyph atlas, the shaped spans, the measurements, the preferred
stack. Layout and paint must agree on one face choice, which is why those stay per pass; the
face database they choose from does not need a second copy.

Families are parsed on demand. fontique's scan answers names cheaply — 15 ms for 180 families
on this machine — while `family()` parses that family's font files, about 0.36 ms per face.
The provider builds only the name map up front and admits a family's faces the first time
something resolves it, asks for its fallback or shapes with it, so a program that draws with
three families pays for three. `family_names` therefore lists the installation without
parsing a single font, and a family whose files are missing is remembered as empty rather
than retried.

## Metrics and advances

`metrics` returns representative face metrics at size 1.0; the provider does not parse
every face's `head`/`hhea` just to answer. `advance` is the face's own width: it shapes the
text with the same shaper layout uses, so kerning, ligatures and the weight instance are
included and a bold face measures as its designer widened it. Neither is the layout
authority:

- Line layout asks the shaped run for advances, not the provider: wrapping also needs the
  style, the direction and the per-face split of a mixed run.
- Line boxes come from the face's ascent and descent; a provider's synthetic
  line height produces the wrong wrapping.

An instance's advances have one more trap. `HVAR` is the table a shaper reads for
weight-varying widths, but a variable font may instead carry them in `gvar` phantom points,
and swash 0.2 does not read those. The shaper adds the difference between the instance's
advance and the default one, taken from skrifa, to every shaped glyph — kerning and mark
offsets stay as shaping produced them. Without that, MiSans VF at `wght=700` drew bold
outlines on regular advances and the letters collided.

## Platform quirks worth knowing

CoreText's font collection hides some faces, including SFNS.ttf, which is the
system UI font; fontique patches around that. This is the argument for a
per-platform provider rather than a cross-platform enumeration: the
platform's own view of its fonts is incomplete in ways only the platform knows.

## Traps

| Change | Symptom |
| --- | --- |
| Use `metrics` for layout | A synthetic 0.8 em line box and wrong wrapping |
| Take a shaped advance as a face's default advance | A variable font without `HVAR` keeps its regular widths and bold letters overlap |
| Substitute a missing named family silently | A typo renders in the wrong face and nobody notices |
| Assume a named family goes through fontconfig aliases | Exact queries return nothing where `fc-match` returns a substitute |
| Parse face 0 of a collection | Every style of a `.ttc` or variable file looks identical |
| Set a family without its bytes | The name resolves to nothing and the platform default answers, silently |
| Treat `has_glyph` as "will render" | An icon font maps ASCII to placeholder glyphs and reports coverage |

## Corrections

**Every text pass scanned the system font stack.** `TextPass::new` built its own provider, and
building one scans and parses the whole stack: 420–480 ms per pass (release build, 160 installed
families). The daemon paid that again for every pin canvas it opened, which was most of the delay
between clicking pin and seeing the pin. Which fonts exist is a property of the machine, not of a
surface: the provider is now built once per thread and shared, and a second pass costs about
0.2 ms.

**Every family was parsed before the first frame.** Sharing the provider left one scan on the
first surface's path, and 211 of that scan's 230 ms were `family().fonts()` over every installed
family — 592 faces parsed to draw text in one of them (release build, 180 families). The
provider now admits a family when something asks for it: building it costs 18–23 ms, listing
180 family names 14 µs, and each family actually used about 0.9 ms. What remains is fontique's
own scan, which a picker or a cold start pays once per process.

**The stack was designed as three provider crates.** The plan was one crate
per platform, selected by feature. fontique already owns the per-platform
backends, so a single provider over it kept the boundary and removed the
feature selection. The platform details stayed in fontique, where they are
tested.

**Coverage answered from whichever cmap subtable came last.** A font can list
several Unicode subtables, and the probe kept the final record: when that was a
format 4 table, every character above the BMP read as uncovered even though an
earlier format 12 table held it, so an emoji or CJK fallback could pick the
wrong face. The probe now keeps the most capable subtable (format 12 over 4)
and memoises its answer per face and character, since fallback asks it once per
character per candidate face.

**`ui-text` carried a second text layout.** `StackMeasure` re-implemented the
line breaker, per-line metrics and atom placement over the provider's
approximate `advance` and `metrics` — the very values this page calls a trap —
while the application lays text out through `TextPass`. The duplicate and its
only consumer (`examples/font_stack.rs`'s layout section) are gone; layout
comes from `TextPass`, and the example shows only the fallback API.

**fontconfig's substitution was expected from the exact lookup.**
`fc-match Arial` returns an installed substitute;
`family_by_name("Arial")` returns not-found. Both are correct for their
question, and the code briefly assumed they agreed. Named queries are exact,
generic queries ask the platform.

**A font choice was a constructor argument.** The stack could only be given to
`with_stack` when the loop was built, so a settings page could not reach a
running app, and `register` refreshed the face caches while leaving already
laid-out nodes at their old metrics — the ordering trap this page used to
state. The choice is now runtime state: `TextPass::set_families` and
`AppLoop::set_font_stack` re-resolve the stack and drop every derived answer,
`register` does the same, and both mark the declarations stale so the next
frame lays the tree out again. `a_family_set_at_runtime_reaches_layout` pins
it: it switches between two already-loaded faces and fails on the cached
answer if any of those drops is missing.

**Family enumeration was suspected of listing only the faces in use.** It does
not: `from_collection` admits every face of every family at construction, so
`family_names` already answers with the installation. Replacing it with a walk
over fontique's collection changed nothing observable, and the test written for
the suspicion passed against the old code, so both were dropped.

**A variable font's bold text kept its regular advances.** The default family on the
development machine is MiSans VF, which carries advance variation in `gvar` phantom points
and has no `HVAR`. swash's shaper reads `HVAR` only, so every weight shaped at the default
advance — `H` measured 719/1000 em at both `wght=400` and `wght=700` — while the rasterizer,
which does apply the instance coordinates, drew the bold outlines. The pen and the outlines
disagreed by 11% and bold letters collided. The shaper now adds skrifa's instance-minus-default
advance to each shaped glyph (`an_instance_advance_follows_the_outlines_it_is_drawn_with`;
the red run read `shaped 28.76, outlines 32.72`). `advance` is shaped for the same reason:
0.6 em per character had made every face and weight measure alike
(`an_advance_is_the_shaped_width_of_the_face_it_names`, red at `432 against 389.3`).

**Every app rendered in Arial on Windows.** This page said the engine names no font and a fresh
pass draws in whatever the platform calls its UI face, and the provider did not do that:
`GenericFamily::SystemUi` — the family `TextStyle::new` names for every run that names none — was
folded into fontique's sans slot, and the base stack behind the default style was resolved from
the plain sans besides. Measured through the provider on this machine, `SystemUi`, `SansSerif`
and the base stack's first face all named `Arial`. `SystemUi` now reaches fontique's own UI slot
and the base stack takes that face ahead of the plain sans; the same measurement then reported
`Segoe UI` for the UI slot and for the stack, `Arial` only for the sans. Pinned by
`the_default_style_shapes_in_the_platforms_ui_face`, watched red with the base stack back on the
plain sans — face 14 where the UI slot's face 0 belonged.

**A bundled font was admitted again by every surface, so each window kept its own copy.** The
provider is shared per thread, but `register` went straight to fontique, which appends another
face — and another reference to the whole blob — to a family it already knows, keeping both for
the process. A picker that registers its interface and icon fonts on every open therefore grew
about 11.6 MB per window: `MaterialSymbolsOutlined.ttf` is 10.7 MB and `InterVariable.ttf` 0.9 MB.
Three open/close cycles of `picker_demo`, each closed and left idle past the picture caches'
window, measured 41.0 → 73.4 → 87.4 → 100.0 MB. Registration is now idempotent: the same bytes
under a known name are a no-op, and different bytes get an internal family name so a genuine
replacement still takes effect. `registering_a_family_again_keeps_one` pins the first half and
was watched red at two faces where one belonged.

## Status

Implemented: one fontique-backed provider for family enumeration, exact and
generic resolution, per-script fallback, variable-font instancing (outlines, advances
and weight-arranged widths agreeing), bundled registration, raw face bytes, a shaped
`advance`, and a host-chosen family preference that may change while the app runs.

Not implemented: font matching on Windows and macOS (the information is there; no
window backend exercises it yet), and per-glyph fallback inside a cluster beyond the
script-level chain.

## See also

- [Text and Input](./text-and-input.md): shaping, clusters and the atlas
  built on these faces.
- [Icons](./icons.md): a glyph source over the same provider.
- [External Sources](../reference/external.md): fontique and parley as
  references.
