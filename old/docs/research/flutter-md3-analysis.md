# Flutter Material 3 + Liquid Glass — evidence inventory for a style abstraction API

Mined 2026-10-27 by subagent `md3-miner`. Read-only against both targets; only this output file was written.
Line numbers were re-verified against the sources in a final citation pass.

## Provenance and what is actually readable

Two checkouts were mined.

| id | root | state |
|---|---|---|
| **A** | `/home/ollie/git/widget-ref/flutter/` | official Flutter monorepo, **sparse checkout** (non-cone), 11 MB on disk |
| **B** | `/home/ollie/git/widget-ref/flutter_liquid_glass/` | standalone package, 132 KB, 3 commits, v1.0.0+1 |

A's sparse rules were read from `.git/info/sparse-checkout`: `packages/flutter/lib/src/painting`, `…/material`, `…/animation`, `engine/src/flutter/impeller/renderer/backend/vulkan`. `git config core.sparseCheckout` = `true`, `core.sparseCheckoutCone` = `false`. `git ls-files` reports 16 264 tracked paths, so the tracked list is *not* evidence a file is readable.

Confirmed **absent from disk** (each checked with `ls`, exit non-zero):

- `packages/flutter/lib/src/widgets/` — so `BackdropFilter`, `ShaderMask`, `WidgetState`, `WidgetStatesController` sources are unreadable.
- `packages/flutter/lib/src/rendering/`, `…/physics/`, `…/cupertino/`, `…/gestures/`.
- `dev/` — so `dev/tools/gen_defaults/**` (the token-DB generator and its JSON: `elevation.json`, `motion.json`, `state.json`, `color_light.json`) is unreadable even though 60+ of those paths appear in `git ls-files`.
- `packages/flutter/examples/` — so the `dynamic_content_color.0.dart` example is unreadable.
- `engine/src/flutter/lib/ui/`, `…/flow/`, and any `.glsl`/`.frag` shader-lib outside `packages/flutter/lib/src/material/shaders/` (`find . -name "*.glsl"` outside that dir returned nothing).

On-disk inventory that *was* readable: `material/` 184 files, `painting/` 55, `animation/` 8, `material/shaders/{ink_sparkle.frag, stretch_effect.frag}`.

Every claim below cites a file:line inside that readable set. Claims I could not ground in code are marked **[unclear]** or **[inferred]** and say why.

---

## 1. MD3 color system

### 1.1 The role list is 45 live roles + 3 deprecated

- `ColorScheme` is documented as "A set of 45 colors based on the [Material spec]" — `material/color_scheme.dart:66` — (read doc comment; cross-checked against the field enumeration below: 45 non-deprecated + 3 deprecated).
- The default ctor takes 50 `this.` parameters — `material/color_scheme.dart:160-255` — (counted `this.` occurrences in the `const ColorScheme({` signature: 50).
- Three are deprecated shims, all deprecated "after v3.18.0-0.1.pre", and each redirects in its getter: `background` → `_background ?? surface` (`material/color_scheme.dart:1142`), `onBackground` → `_onBackground ?? onSurface` (`:1155`), `surfaceVariant` → `_surfaceVariant ?? surface` (`:1027`).
- Live roles, in declaration order (`material/color_scheme.dart:833-1133`), each line read:
  - primary group: `primary` (`:833`), `onPrimary` (`:840`), `primaryContainer` (`:845`), `onPrimaryContainer` (`:851`), `primaryFixed` (`:859`), `primaryFixedDim` (`:865`), `onPrimaryFixed` (`:870`), `onPrimaryFixedVariant` (`:876`)
  - secondary group: `secondary` (`:882`), `onSecondary` (`:889`), `secondaryContainer` (`:894`), `onSecondaryContainer` (`:900`), `secondaryFixed` (`:908`), `secondaryFixedDim` (`:914`), `onSecondaryFixed` (`:919`), `onSecondaryFixedVariant` (`:925`)
  - tertiary group: `tertiary` (`:931`), `onTertiary` (`:936`), `tertiaryContainer` (`:948`), `onTertiaryContainer` (`:954`), `tertiaryFixed` (`:962`), `tertiaryFixedDim` (`:968`), `onTertiaryFixed` (`:973`), `onTertiaryFixedVariant` (`:979`)
  - error group: `error` (`:985`), `onError` (`:992`), `errorContainer` (`:997`), `onErrorContainer` (`:1003`)
  - surface group: `surface` (`:1010`), `onSurface` (`:1017`), `surfaceDim` (`:1032`), `surfaceBright` (`:1037`), **the five container roles** `surfaceContainerLowest` (`:1043`), `surfaceContainerLow` (`:1049`), `surfaceContainer` (`:1057`), `surfaceContainerHigh` (`:1067`), `surfaceContainerHighest` (`:1073`), `onSurfaceVariant` (`:1081`)
  - outline/misc: `outline` (`:1087`), `outlineVariant` (`:1093`), `shadow` (`:1099`), `scrim` (`:1104`), `inverseSurface` (`:1111`), `onInverseSurface` (`:1120`), `inversePrimary` (`:1126`), `surfaceTint` (`:1133`)
- Surface containers are named by **ordinal, not `t0..t5`**: Lowest/Low/(base)/High/Highest. Getter docs define them purely comparatively — "least emphasis" (`material/color_scheme.dart:1041`), "less emphasis than [surfaceContainer]" (`:1047`), "recommended … distinct area within the surface" (`:1053-1056`), "more emphasis than [surfaceContainer] but less emphasis than …Highest" (`:1067`), "darkest tone … most emphasis" (`:1073`) — (read all five getters).
- `surfaceDim`/`surfaceBright` are absolute, not relative: "always darkest in the dark or light theme" (`material/color_scheme.dart:1031`), "always the lightest" (`:1036`).
- `surfaceTint` is documented as "A color used as an overlay on a surface color to indicate a component's elevation" — `material/color_scheme.dart:1132` — and defaults to `primary`: `_surfaceTint ?? primary` (`:1133`). Unless overridden, the tint color *is* the primary role.
- `shadow` and `scrim` default to opaque black, not to a scheme color: `_shadow ?? const Color(0xff000000)` (`material/color_scheme.dart:1099`), `_scrim ?? const Color(0xff000000)` (`:1104`).
- `-Fixed` roles are brightness-invariant: "they stay the same color between light and dark themes", and `-Dim` "provide a stronger, more emphasized color with the same fixed behavior" — `material/color_scheme.dart:90-94` — (read doc).
- Every accent role has an `on*` partner with a documented pairing target: contrast ratio "at least 4.5:1" — `material/color_scheme.dart:837-838` (`primary`/`onPrimary`), a phrase that recurs 13 times across the role docs, plus the class-level statement at `:111`. The 4.5:1 number is a *documentation* constraint; enforcement lives in the external generator (1.2).
- Container roles are explicitly framed as **replacing** the overlay model: "Material 3 also introduces tone-based surfaces and surface containers. They replace the old opacity-based model which applied a tinted overlay on top of surfaces based on their elevation" — `material/color_scheme.dart:101-105` — and repeated at `surfaceContainer` (`:1054-1056`).

### 1.2 Yes — MD3 derives the whole scheme from one seed, at runtime

- `ColorScheme.fromSeed({required Color seedColor, Brightness brightness = light, DynamicSchemeVariant dynamicSchemeVariant = tonalSpot, double contrastLevel = 0.0, …all roles as optionals})` — `material/color_scheme.dart:268-336` — (read signature).
- Its body builds a scheme then applies per-role `override ?? generated`: `_buildDynamicScheme(...)` at `material/color_scheme.dart:337-341`, then `primary: primary ?? Color(MaterialDynamicColors.primary.getArgb(scheme))` at `:343`. All 45 roles follow that shape (`:343-428`), so partial override is first-class.
- `MaterialDynamicColors` is **not** defined anywhere in A's readable tree (`grep -rn "class MaterialDynamicColors"` over `packages/flutter/lib/src/` → no matches). Same for `class Hct`, `class DynamicSchemeBuilder`, `class TonalPalette`, `class DynamicScheme`. All come from the external package — `material/color_scheme.dart:14` `import 'package:material_color_utilities/material_color_utilities.dart';` — (verified `ls ~/.pub-cache`: package not on disk, so its source could not be read).
- The doc names that package as the tonal-palette engine: "[https://pub.dev/packages/material_color_utilities], the package used to generate the tonal palettes needed for the scheme" — `material/color_scheme.dart:266` — (read).
- Seed → `Hct` conversion, verbatim: `final Hct sourceColor = Hct.fromInt(seedColor.value);` — `material/color_scheme.dart:1980` — (read `_buildDynamicScheme`, `:1969-2025`).
- `_buildDynamicScheme` dispatches to nine concrete scheme classes: `SchemeTonalSpot`, `SchemeFidelity`, `SchemeContent`, `SchemeMonochrome`, `SchemeNeutral`, `SchemeVibrant`, `SchemeExpressive`, `SchemeRainbow`, `SchemeFruitSalad` — `material/color_scheme.dart:1981-2024` — (read the `switch`). All nine take `sourceColorHct`, `isDark`, `contrastLevel`. **No tone table is inlined in the framework**; the tone→ARGB arithmetic is entirely in the dependency.
- `DynamicSchemeVariant` is the framework's own enum of nine algorithms, documented by chroma behavior — `material/color_scheme.dart:30-61` —: `tonalSpot` "low chroma" (`:30-31`), `fidelity` "match seed color, even if … high chroma" (`:34`), `monochrome` "All colors are grayscale, no chroma" (`:37`), `neutral` "Close to grayscale, a hint of chroma" (`:40`), `vibrant` "primary palette's chroma is at maximum" (`:42-45`), `expressive` "medium chroma … hue is different from the seed color" (`:47-48`), `content` "[primaryContainer] is the seed color, adjusted … tertiary is analogue" (`:50-53`), `rainbow`/`fruitSalad` "the seed color's hue does not appear in the theme" (`:57`, `:60`).
- `contrastLevel` is validated in code, not just documented: `assert(contrastLevel >= -1.0 && contrastLevel <= 1.0, …)` — `material/color_scheme.dart:1974-1978` — (read). Doc mapping: "0.0 is the default (normal); -1.0 is the lowest; 1.0 is the highest … medium and high contrast correspond to 0.5 and 1.0" — `material/color_scheme.dart:258-262`.
- **Image → theme also exists (content color)**: `static Future<ColorScheme> fromImageProvider({...})` — `material/color_scheme.dart:1717` — (read signature). Pipeline, all read:
  1. scale down: `const maxDimension = 112.0` (`:1903`), resolved via `imageProvider.resolve(ImageConfiguration(size: Size(112,112)))` (`:1904-1906`), repainted through a `PictureRecorder`/`Canvas` into `picture.toImage(...)` (`:1930-1941`);
  2. quantize: `QuantizerCelebi().quantize(imageBytes.buffer.asUint32List(), 128, returnInputPixelToClusterPixel: true)` (`:1893-1897`) — 128 clusters;
  3. score: `Score.score(colorToCount, desired: 1)` → `baseColor` (`:1791-1792`);
  4. feed that into the same `_buildDynamicScheme` (`:1794-1799`). Byte order fixed by hand in `_getArgbFromAbgr` (`:1958-1966`).
  `QuantizerCelebi`, `Score`, `QuantizerResult` are external-package types too (`:14`).
- Theme wiring proves runtime derivation is the normal path: `if (colorSchemeSeed != null || useMaterial3) { if (colorSchemeSeed != null) { colorScheme = ColorScheme.fromSeed(seedColor: colorSchemeSeed, brightness: effectiveBrightness); } colorScheme ??= isDark ? _colorSchemeDarkM3 : _colorSchemeLightM3; }` — `material/theme_data.dart:437-444` — (read). `useMaterial3 ??= true` (`:413`), field `final bool useMaterial3` (`:1146`) — M3 is the default.
- Even without a seed, the M3 fallback is a *generated constant* scheme, not `fromSwatch`: `_colorSchemeLightM3` / `_colorSchemeDarkM3` — `material/theme_data.dart:3380` / `:3434` — inside a `// BEGIN GENERATED TOKEN PROPERTIES - ColorScheme` block (`:3372`). `ColorScheme.fromSwatch` is now only the M2 path and is marked for phased removal — `material/color_scheme.dart:791-796`.
- Baseline M3 light values, verbatim from `material/theme_data.dart:3380-3431`: `primary: 0xFF6750A4`, `onPrimary: 0xFFFFFFFF`, `primaryContainer: 0xFFEADDFF`, `onPrimaryContainer: 0xFF4F378B`, `secondaryContainer: 0xFFE8DEF8`, `surface: 0xFFFEF7FF`, `surfaceContainerLowest: 0xFFFFFFFF`, `surfaceContainerLow: 0xFFF7F2FA`, `surfaceContainer: 0xFFF3EDF7`, `surfaceContainerHigh: 0xFFECE6F0`, `surfaceContainerHighest: 0xFFE6E0E9`, `surfaceDim: 0xFFDED8E1`, `surfaceBright: 0xFFFEF7FF`, `onSurface: 0xFF1D1B20`, `onSurfaceVariant: 0xFF49454F`, `outline: 0xFF79747E`, `outlineVariant: 0xFFCAC4D0`, `inverseSurface: 0xFF322F35`, `inversePrimary: 0xFFD0BCFF`, `surfaceTint: 0xFF6750A4`. That last entry carries the comment "The surfaceTint color is set to the same color as the primary" (`:3430`).
- **[unclear] the actual tone numbers (e.g. primary = tone 40, container = tone 90).** Not present in A: `grep -rn "tone"` in `color_scheme.dart` only hits prose ("mapped to light or dark tones", `material/color_scheme.dart:21-23`) and comparative getter docs. The tone table lives in `material_color_utilities` (not on disk) and in `dev/tools/gen_defaults/data/color_light.json` (not checked out). So a claim like "surfaceContainerLow = neutral tone 98" **cannot be cited from this repo state**.

### 1.3 `material/colors.dart` structure — legacy, and deliberately separate

- File is 1956 lines. Three layers, all read:
  1. `class MaterialColor extends ColorSwatch<int>` — `material/colors.dart:104` — "There are 10 valid indices: 50, 100, 200, ..., 900. The value of this color should the same the value of index 500 and [shade500]" (`:21-22`, verbatim including its typo).
  2. `class MaterialAccentColor extends ColorSwatch<int>` — `material/colors.dart:156` — indices 100/200/400/700 (`:149`).
  3. `abstract final class Colors` — `material/colors.dart:281` — the flat palette.
- Counts (verified with `grep -c`): 19 `static const MaterialColor`, 19 `MaterialAccentColor`, 17 plain `static const Color`. The 17 flat names are `transparent` + the grayscale ramp: `transparent 0x00000000` (`:283`), `black 0xFF000000` (`:295`), `black87 0xDD000000` (`:310`), `black54 0x8A000000` (`:326`), `black45 0x73000000` (`:336`), `black38 0x61000000` (`:350`), `black26 0x42000000` (`:365`), `black12 0x1F000000` (`:377`), `white 0xFFFFFFFF` (`:396`), `white70 0xB3FFFFFF` (`:412`), `white60` (`:428`), `white54` (`:440`), `white38` (`:456`), `white30` (`:469`), `white24` (`:482`), `white12` (`:495`), `white10` (`:507`).
- Swatch ramp starts at `red` (`material/colors.dart:536`) with `redAccent` (`:577`), then pink (`:612`), purple (`:691`), deepPurple (`:770`), indigo (`:849`), blue (`:930`), lightBlue (`:1009`), cyan (`:1090`), teal (`:1169`), green (`:1251`), lightGreen (`:1333`), lime (`:1412`), yellow (`:1491`), amber (`:1570`), orange (`:1651`)… each with an `*Accent` sibling.
- The file frames itself as legacy: "The [ColorScheme] is preferred for representing colors in applications that are configured for Material 3" — `material/colors.dart:26-28` — and it is where the role concept is defined framework-wide: `{@template flutter.material.colors.colorRoles} … Each property in the [ColorScheme] class represents one color role …{@endtemplate}` — `material/colors.dart:32-36` — (read). `ColorScheme` re-injects that block via `{@macro flutter.material.colors.colorRoles}` — `material/color_scheme.dart:73`.
- `ColorSwatch<T>` itself lives in painting: `class ColorSwatch<T> extends Color` — `painting/colors.dart:420` — (read). That file is the readable color-math home: `HSVColor`, `_getHue`, `_lerpHue`, `_colorFromHue` (`painting/colors.dart:14-52`, `:58+`). Note what that is: HSL/HSV utilities for interpolation, **not** HCT/OKLCH tone generation. There is no HCT implementation in `painting/`.

---

## 2. MD3 elevation and the state layer

### 2.1 Two elevation systems coexist; M3 is migrating off the tint

- **Shadow table** `kElevationToShadow` — `material/shadows.dart:40` — aliases the literal `_elevationToShadow` at `:46`. Documented keys: "The following elevations have defined shadows: 1, 2, 3, 4, 6, 8, 9, 12, 16, 24" (`:18`); "Each entry has three shadows which must be combined" (`:20-21`).
- Three fixed alpha constants drive the decomposition — `material/shadows.dart:43-45`: `_kKeyUmbraOpacity = Color(0x33000000) // alpha = 0.2`, `_kKeyPenumbraOpacity = Color(0x24000000) // alpha = 0.14`, `_kAmbientShadowOpacity = Color(0x1F000000) // alpha = 0.12`.
- dp → geometry, read line-by-line from `material/shadows.dart:46-196` (offsets are `(0, y)`; format `offset / blur / spread`):

| dp | umbra | penumbra | ambient |
|---|---|---|---|
| 0 | empty list (`:48`) | — | — |
| 1 | (0,2) 1 −1 | (0,1) 1 0 | (0,1) 3 0 |
| 2 | (0,3) 1 −2 | (0,2) 2 0 | (0,1) 5 0 |
| 3 | (0,3) 3 −2 | (0,3) 4 0 | (0,1) 8 0 |
| 4 | (0,2) 4 −1 | (0,4) 5 0 | (0,1) 10 0 |
| 6 | (0,3) 5 −1 | (0,6) 10 0 | (0,1) 18 0 |
| 8 | (0,5) 5 −3 | (0,8) 10 +1 | (0,3) 14 +2 |
| 9 | (0,5) 6 −3 | (0,9) 12 +1 | (0,3) 16 +2 |
| 12 | (0,7) 8 −4 | (0,12) 17 +2 | (0,5) 22 +4 |
| 16 | (0,8) 10 −5 | (0,16) 24 +2 | (0,6) 30 +5 |
| 24 | (0,11) 15 −7 | (0,24) 38 +3 | (0,9) 46 +8 |

- The shadow table is M2-era: its header cites `https://material.io/design/environment/elevation.html` — `material/shadows.dart:11` — the pre-M3 spec.
- **Surface tint table (M3)** — `material/elevation_overlay.dart:172-177`, inside a generated block (`BEGIN GENERATED TOKEN PROPERTIES - SurfaceTint`, `:159`, naming `dev/tools/gen_defaults/bin/gen_defaults.dart`). dp → opacity, verbatim from those lines:

| level | dp | tint opacity | line |
|---|---|---|---|
| 0 | 0.0 | 0.0 | `:172` |
| 1 | 1.0 | 0.05 | `:173` |
| 2 | 3.0 | 0.08 | `:174` |
| 3 | 6.0 | 0.11 | `:175` |
| 4 | 8.0 | 0.12 | `:176` |
| 5 | 12.0 | 0.14 | `:177` |

- The lookup **interpolates** off-table and **clamps** outside 0..12 — documented at `material/elevation_overlay.dart:41-44` ("interpolating between values as needed. If the elevation is outside the range of values in the table it will clamp"), implemented in `_surfaceTintOpacityForElevation` (`:45-66`): walk, early-return on exact match or end-of-list, then linear blend of the bracketing pair.
- Application is a single alpha blend over the base color, guarded by "not null and not transparent" — `material/elevation_overlay.dart:31-39` (`Color.alphaBlend(surfaceTint.withOpacity(opacityForElevation), color)` at `:33-36`).
- The M2 dark-theme overlay is still present and documented as dead for M3: "If using Material Design 3, this type of color overlay is no longer used. Instead a 'surface tint' overlay is used" — `material/elevation_overlay.dart:72-73` — (same sentence repeated at `:113`, `:129`). Its formula: `opacity = (4.5 * math.log(elevation + 1) + 2) / 100.0` — `material/elevation_overlay.dart:146` — commented "This formula matches the values in the spec: material.io/design/color/dark-theme.html#properties" (`:144-145`).
- Gate for which system runs, in `Material`: `theme.useMaterial3 ? ElevationOverlay.applySurfaceTint(backgroundColor!, widget.surfaceTintColor, widget.elevation) : ElevationOverlay.applyOverlay(context, backgroundColor!, widget.elevation)` — `material/material.dart:512-518` — (read). Also `material/bottom_app_bar.dart:210-211`.
- `Material.elevation` docs confirm it drives both: "This controls the size of the shadow below the material and the opacity of the elevation overlay color if it is applied" — `material/material.dart:225-234` — and `surfaceTintColor`: "If [ThemeData.useMaterial3] is true and [surfaceTintColor] is not null and not [Colors.transparent], then it will be used to overlay the base [color] with an opacity based on the [elevation]" — `material/material.dart:298-300`.
- **Quantified: the tint is being retired in favor of container roles.** In generated M3 default blocks, `surfaceTintColor` is set to `Colors.transparent` 40 times across `material/` (`grep -rn "surfaceTintColor => Colors.transparent\|MaterialStatePropertyAll<Color>(Colors.transparent)"` → 40). Container roles are read 252 times: `surfaceContainerHighest` 63, `surfaceContainerLow` 46, `surfaceContainer` 43, `surfaceContainerHigh` 42, `surfaceContainerLowest` 34, `surfaceDim` 32, `surfaceBright` 32 (`grep -rho … | sort | uniq -c`).
  - Concrete, all read: ElevatedButton bg `surfaceContainerLow` (`material/elevated_button.dart:530-538`) with `surfaceTintColor = transparent` (`:567-569`); Card `surfaceContainerLow` (`material/card.dart:313`); FilledCard `surfaceContainerHighest`, elevation 0 (`material/card.dart:336-348`); OutlinedCard `surface` + `outlineVariant` side, elevation 0 (`material/card.dart:371-383`); NavigationBar `surfaceContainer` (`material/navigation_bar.dart:1440`) with `surfaceTintColor = transparent` (`:1446`).
  - Read as a design signal: **new M3 component defaults use static tone-derived containers; dp-driven tint is legacy plumbing that only a few widgets (`Material` canvas path, `BottomAppBar`) still exercise.**

### 2.2 State layer model

- States live in a `Set<WidgetState>`; `material/material_state.dart` is a compat shim: `typedef MaterialStateProperty<T> = WidgetStateProperty<T>` (`material/material_state.dart:543`), docs point at "[WidgetState], a general non-Material version that can be used interchangeably" (`:32-33`). `WidgetState`'s definition is in `packages/flutter/lib/src/widgets/widget_state.dart` — tracked (`git ls-files`) but **absent from disk**, so the numeric masks were not read.
- Usage counts over `material/` (`grep -rho "WidgetState\.[a-zA-Z]*" | sort | uniq -c`): `disabled` 208, `selected` 206, `focused` 201, `hovered` 188, `pressed` 131, `error` 25, `dragged` 16, `any` 11, `scrolledUnder` 8. So the working vocabulary is hover, focus, pressed, dragged, selected, disabled, error, scrolledUnder, any (the one stray `WidgetState.` hit is a doc artifact).
- `dragged` is real: `if (_dragging) WidgetState.dragged` — `material/slider.dart:854` — consumed in generated defaults at `material/slider.dart:2226`, `:2315`; documented at `material/slider.dart:507`.
- `selected` drives on/off components: `const selectedState = <WidgetState>{WidgetState.selected}` — `material/navigation_bar.dart:427` — and picks `onSecondaryContainer` vs `onSurfaceVariant` (`material/navigation_bar.dart:1453-1457`).
- `error` composes with hover/focus/press: `material/checkbox.dart:998-1032` re-tests pressed/hovered/focused inside the `error` branch, then again inside the `selected` branch — state-set priority via early `return`s, not additive blending.

**The opacity constants, verified against code** (generated M3 `overlayColor` resolvers):

- ElevatedButton — pressed `primary.withOpacity(0.1)`, hovered `…0.08`, focused `…0.1`, else null — `material/elevated_button.dart:548-559`.
- OutlinedButton — identical numbers on `primary` — `material/outlined_button.dart:490-501`.
- FilledButton — same opacities but on **`onPrimary`** — `material/filled_button.dart:566-577`.
- Checkbox — layer color flips with selection and error: error branch `error 0.1/0.08/0.1`; selected branch `onSurface 0.1` pressed but `primary 0.08/0.1` hover/focus; unselected `primary 0.1` pressed, `onSurface 0.08` hover, `onSurface 0.1` focus — `material/checkbox.dart:998-1032`.
- Net: **pressed 0.10 / hovered 0.08 / focused 0.10** is the MD3 state-layer triple; the *carrier* is chosen per context (primary on low-emphasis surfaces, onPrimary on filled surfaces, onSurface when neutral/unselected, error when in error). The prompt's guess "0.08 hover, 0.12 focus" is **wrong for M3**: 0.12 is the disabled-container alpha; focus is 0.10.
- The 0.12/0.38 pair is disabled, universal across the button family: disabled background `onSurface.withOpacity(0.12)` (`material/elevated_button.dart:530-534`, `material/filled_button.dart:548-552`), disabled foreground/icon `onSurface.withOpacity(0.38)` (`material/elevated_button.dart:539-543`, iconColor `:603-619`; `material/filled_button.dart:557-561`; `material/outlined_button.dart:481-485`), disabled OutlinedButton side `BorderSide(color: onSurface.withOpacity(0.12))` (`material/outlined_button.dart:554-558`).
- **Where 0.12/0.04 does come from — legacy M2 theme defaults, still live:** `focusColor ??= isDark ? Colors.white.withOpacity(0.12) : Colors.black.withOpacity(0.12)` and `hoverColor ??= … 0.04` — `material/theme_data.dart:467-468` — (read). Same pair in the legacy button theme: focus `getTextColor(button).withOpacity(0.12)` (`material/button_theme.dart:541`), hover `…0.04` (`:554`). Docs that say "hover 0.04 / focus 0.12" describe **M2**; do not confuse with the M3 generated defaults.
- Resolution order in `InkResponse`: `widget.overlayColor?.resolve(statesForThatType) ?? widget.<type>Color ?? theme.<type>Color` — `material/ink_well.dart:1357-1368` — state-aware property wins, then widget flat color, then theme. Each highlightable state resolves **separately with non-highlightable states preserved**: `const highlightableStates = <WidgetState>{focused, hovered, pressed}` then `final pressed = {...nonHighlightableStates, WidgetState.pressed}` etc. — `material/ink_well.dart:1343-1355`. That is how `disabled` still modulates a hover layer.
- Pressed is documented as no longer a fill: "The pressed state triggers a ripple (ink splash), per the current Material Design spec. A separate highlight is no longer used." — `material/ink_well.dart:1359-1361`. Buttons hard-code `highlightColor: Colors.transparent` and pass `overlayColor` to `InkWell` — `material/button_style_button.dart:567`.
- The ink host is `InkHighlight`, created lazily per state with the resolved color; disabled ⇒ `.withAlpha(0)` — `material/ink_well.dart:1033-1055`. Splash color: `widget.overlayColor?.resolve(statesController.value) ?? widget.splashColor ?? Theme.of(context).splashColor` — `material/ink_well.dart:1091-1094` (creation) and `:1373-1376` (live update).
- FAB shows the older per-color spelling, still generated: `splashColor`/`focusColor` = `onPrimaryContainer.withOpacity(0.1)`, `hoverColor` = `…0.08` — `material/floating_action_button.dart:811-813` — same triple, different API shape.

**Elevation as a state-mapped property.** Buttons put dp in `WidgetStateProperty<double>`; `Material` animates it implicitly.

- ElevatedButton: disabled 0.0, pressed 1.0, hovered 3.0, focused 1.0, default 1.0 — `material/elevated_button.dart:571-586`. Legacy M2 style used flat `elevation: 2` (`:405`) and a relative table survives as a doc comment: `disabled: 0, pressed: elevation + 6, hovered: elevation + 2, focused: elevation + 2` (`:234-237`), i.e. M2 was *relative* dp bumps; M3 hard-codes absolute levels.
- FilledButton: disabled/pressed/focused/default 0.0, hovered 1.0 — `material/filled_button.dart:589-604`.
- OutlinedButton: flat `MaterialStatePropertyAll<double>(0.0)` — `material/outlined_button.dart:513`.
- FAB theme data: `elevation: 6.0, focusElevation: 6.0, hoverElevation: 8.0, highlightElevation: 6.0` — `material/floating_action_button.dart:778-781`.

---

## 3. MD3 motion

### 3.1 Easing tokens — (0.2, 0, 0, 1) is real and named `standard`

- Generated block `BEGIN GENERATED TOKEN PROPERTIES - Motion` — `material/motion.dart:7` — declares `abstract final class Durations` (`:21`) and `abstract final class Easing` (`:161`), both from the `gen_defaults` pipeline.
- Curves, verbatim with lines (all read):
  - `emphasizedAccelerate = Cubic(0.3, 0.0, 0.8, 0.15)` — `material/motion.dart:168`
  - `emphasizedDecelerate = Cubic(0.05, 0.7, 0.1, 1.0)` — `material/motion.dart:176`
  - `linear = Cubic(0.0, 0.0, 1.0, 1.0)` — `material/motion.dart:184`
  - **`standard = Cubic(0.2, 0.0, 0.0, 1.0)`** — `material/motion.dart:192` — matches the M3 spec value named in the prompt
  - `standardAccelerate = Cubic(0.3, 0.0, 1.0, 1.0)` — `material/motion.dart:200`
  - `standardDecelerate = Cubic(0.0, 0.0, 0.0, 1.0)` — `material/motion.dart:208`
  - `legacyDecelerate = Cubic(0.0, 0.0, 0.2, 1.0)` — `material/motion.dart:216`
  - `legacyAccelerate = Cubic(0.4, 0.0, 1.0, 1.0)` — `material/motion.dart:224`
  - `legacy = Cubic(0.4, 0.0, 0.2, 1.0)` — `material/motion.dart:232` — equals `Curves.fastOutSlowIn` (`animation/curves.dart:1865`)
- Duration ladder, 16 `static const Duration` values in 4 bands of 4 — `material/motion.dart:21-159` —: short1 50, short2 100, short3 150, short4 200, medium1 250, medium2 300, medium3 350, medium4 400, long1 450, long2 500, long3 550, long4 600, extralong1 700, extralong2 800, extralong3 900, extralong4 1000 ms.
- M2 globals survive as deprecated exports: `standardEasing = Curves.fastOutSlowIn` (`material/curves.dart:22`), `accelerateEasing = Cubic(0.4, 0.0, 1.0, 1.0)` (`:36`), `decelerateEasing = Cubic(0.0, 0.0, 0.2, 1.0)` (`:51`), each tagged "deprecated after v3.18.0-0.1.pre".

### 3.2 Durations actually used by widgets

- Theme change / button state animation: `kThemeChangeDuration = Duration(milliseconds: 200)` — `material/constants.dart:39` — set as `animationDuration` in every generated button default ctor (`material/elevated_button.dart:517`, `material/filled_button.dart:535`, `material/outlined_button.dart:464`). 200 ms == `Durations.short4`.
- `kTabScrollDuration` 300 ms (`material/constants.dart:51`), `kRadialReactionDuration` 100 ms (`:45`), `kRadialReactionRadius` 20.0 (`:42`), `kRadialReactionAlpha` 0x1F (`:48`), `kMinInteractiveDimension` 48.0 (`:27`).
- InkSparkle ripple: `_animationDuration = Duration(milliseconds: 617)`, `_targetRadiusMultiplier = 2.3`, `_noiseDensity = 2.1`, `_rotateRight = math.pi * 0.0078125` — `material/ink_sparkle.dart:181-185` — (read). Envelope = weighted `TweenSequence`s: radius `Curves.fastOutSlowIn` weight 75 then constant 25 (`:134-137`); alpha ramp 13 / hold 27 / fade 60 (`:150-156`); sparkle alpha 13 / 27 / 50 (`:158-164`); center eases to widget center 50/50 (`:141-148`).
- The emphasized curve is a **piecewise five-point cubic**, not one `Cubic`: `ThreePointCubic easeInOutCubicEmphasized = ThreePointCubic(Offset(0.05,0), Offset(0.133333,0.06), Offset(0.166666,0.4), Offset(0.208333,0.82), Offset(0.25,1))` — `animation/curves.dart:1779-1785` — (class `ThreePointCubic` at `:462`, self-splitting at `:502`/`:510`).
- `Curves` is a flat `abstract final class` of ~40 named constants — `animation/curves.dart:1467+` — e.g. `fastOutSlowIn = Cubic(0.4, 0.0, 0.2, 1.0)` (`:1865`), `easeOutCubic = Cubic(0.215, 0.61, 0.355, 1.0)` (`:1671`), `fastLinearToSlowEaseIn = Cubic(0.18, 1.0, 0.04, 1.0)` (`:1493`), `slowMiddle = Cubic(0.15, 0.85, 0.85, 0.15)` (`:1871`).

### 3.3 Page transitions — what is really in the file

`material/page_transitions_theme.dart` (1305 lines). Public builders (`grep -n "^class "`): `FadeForwardsPageTransitionsBuilder` (`:451`), `ZoomPageTransitionsBuilder` (`:588`), `PageTransitionsTheme` (`:754`). Plus `PredictiveBackPageTransitionsBuilder` (`material/predictive_back_page_transitions_builder.dart:46`) and `PredictiveBackFullscreenPageTransitionsBuilder` (`:126`).

- **Shared-axis and fade-through builders are NOT in the framework.** `grep -rn "SharedAxis\|FadeThrough" material/` → zero class definitions; `OpenUpwards`/`Slide` appear only as doc cross-references (`material/page_transitions_theme.dart:441`, `:580`, `:745`). `git ls-files | grep -i "shared_axis\|fade_through\|open_upwards"` → only `dev/integration_tests/new_gallery/lib/demos/reference/motion_demo_fade_through_transition.dart` (not checked out). **[unclear] M3 shared-axis / fade-through / container-transform motion cannot be cited from this checkout.**
- Defaults: android → `PredictiveBackPageTransitionsBuilder()`, iOS/macOS → `CupertinoPageTransitionsBuilder()`, windows/linux → `ZoomPageTransitionsBuilder()` — `material/page_transitions_theme.dart:762-770`.
- **FadeForwards** (non-gesture navigation falls back to it — `material/predictive_back_page_transitions_builder.dart:96-98`):
  - `kTransitionMilliseconds = 450` (`material/page_transitions_theme.dart:470`), `transitionDuration` (`:473`).
  - The number is self-declared as approximate: "Eyeballed on a physical Pixel 9 running Android 16. This does not match the actual value used by native Android, which is 800ms, because native Android is using Material 3 Expressive springs that are not currently supported by Flutter. So for now at least, this is an approximation." — `material/page_transitions_theme.dart:464-469`.
  - `_transitionCurve = Curves.easeInOutCubicEmphasized` (`:486`), "Used by all of the sliding transition animations".
  - Neighbor page translates ±0.25 width (`Offset(-0.25, 0.0)`, `:489-498`).
  - New page fades in over `Interval(0.0, 0.75)`, old fades out over `Interval(0.0, 0.25)` — `material/page_transitions_theme.dart:501-510` — fade completes at 337 ms of 450.
- **Zoom** (Android 10/Q): curve sequence "similar to the 'fastOutExtraSlowIn' curve used in the native transition" — `material/page_transitions_theme.dart:41-57` — seg 1: 0.0→0.4 `Cubic(0.05, 0.0, 0.133333, 0.06)` weight 0.166666; seg 2: 0.4→1.0 `Cubic(0.208333, 0.82, 0.25, 1.0)` weight 0.833334. Those are the same control points as `Curves.easeInOutCubicEmphasized` (`animation/curves.dart:1779-1785`), hand-segmented.
  - Intervals: enter alpha `Interval(0.125, 0.250)` (`:178`), enter scale `_scaleCurveSequence` (`:183`, `:188`), exit alpha `Interval(0.2075, 0.4175)` (`:193`), exit uses `Interval(0.0825, 0.2075)` (`:288`).
  - Snapshotting is on by default (`allowSnapshotting = true`, `allowEnterRouteSnapshotting = true`, `:592-594`); docs warn inner animations "appear frozen" (`:608-612`); benchmark escape hatch `bool.fromEnvironment('flutter.benchmarks.force_disable_snapshot')` (`:640`).
  - `_FadeForwardsPageTransition` (`:364`) carries the ±0.25 `Offset(0.25, 0.0)` tweens (`:381-390`).
- **Override API**: `AnimationStyle {curve, duration, reverseCurve, reverseDuration}` + `static const AnimationStyle noAnimation = AnimationStyle(duration: Duration.zero, reverseDuration: Duration.zero)` — `animation/animation_style.dart:30-40` — (read). Its doc lists exactly who honors it: Expansible, ExpansionTile, MaterialApp, PopupMenuButton, `ScaffoldMessengerState.showSnackBar`, `showBottomSheet`, `showModalBottomSheet` (`animation/animation_style.dart:13-21`). Per-call motion overrides exist for a curated set, not for every widget.

### 3.4 Spring model

- Springs are reachable but **not readable here**: `animation/animation_controller.dart:20` `export 'package:flutter/physics.dart' show Simulation, SpringDescription;`, and `packages/flutter/lib/src/physics/` is **not checked out** — `SpringDescription`/`SpringSimulation` bodies could not be read.
- The one concrete spring in the readable set: `SpringDescription.withDampingRatio(mass: 1.0, stiffness: 500.0)` — `animation/animation_controller.dart:40-43` — plus `_kFlingTolerance = Tolerance(velocity: double.infinity, distance: 0.01)` (`:45`).
- `AnimationController.fling({velocity = 1.0, SpringDescription? springDescription, …})` builds `SpringSimulation(springDescription, value, target, velocity * scale)..tolerance = _kFlingTolerance` — `animation/animation_controller.dart:773-787` — and asserts against oscillation: "An underdamped spring results in oscillation rather than a fling." (`:788-796`). Doc: default is `SpringType.criticallyDamped` (`:762-765`).
- Accessibility is centralized: `AnimationBehavior` enum (`animation/animation_controller.dart:52-60`) lets a scrollable keep physics under `AccessibilityFeatures.disableAnimations` (`:47-51`); when disabled, `fling` multiplies velocity by an "arbitrary" 200.0 (`:783-784`).
- Scroll physics also reach springs: `ScrollSpringSimulation(spring, metrics.pixels, target, velocity, tolerance: tolerance)` — `material/carousel.dart:1550`.
- **[inferred]** M3 Expressive's spring motion is absent, and the framework says so out loud (`material/page_transitions_theme.dart:464-469`). Any "M3 motion = curve+duration tokens" model must note that Flutter's newest transition is a hand-tuned cubic approximating a native spring.

---

## 4. BackdropFilter, blur, TileMode, and fragment shaders in the framework

### 4.1 What is and is not readable

`BackdropFilter` the widget lives in `packages/flutter/lib/src/widgets/` — **absent** (`ls` → no directory). Engine side: `engine/src/flutter/flow/layers/backdrop_filter_layer.{h,cc}` are tracked but the on-disk engine subtree contains only `impeller/renderer/backend/vulkan` (139 files); `ls engine/src/flutter/flow` → no such directory.

**Therefore: I could not read the layer class `BackdropFilter` creates, nor any TileMode/edge documentation for blur.** The task asked for exactly that; it is out of reach in this checkout. Everything below is what the readable tree *does* show.

- The only framework blur call site in the readable set: `BackdropFilter(filter: ui.ImageFilter.blur(sigmaX: blurAmount, sigmaY: blurAmount), child: const ColoredBox(color: Colors.transparent))` — `material/flexible_space_bar.dart:283-286`.
- Its sigma is derived, not authored: `blurAmount = (constraints.maxHeight - settings.maxExtent) / 10` — `material/flexible_space_bar.dart:280` — 1 sigma per 10 px of over-stretch, and only when `constraints.maxHeight > settings.maxExtent` and `StretchMode.blurBackground` is set (`:278-279`). The enum is documented "The background will blur using a [ui.ImageFilter.blur] effect." (`:37-38`).
- The transparent-child idiom is the point: a `BackdropFilter` needs a child to size the effect; a transparent box contributes nothing visual. Package B uses the same trick with `child: Container()` (§5).
- `Clip.antiAliasWithSaveLayer` is the only layer primitive `painting/` shows: `canvas.saveLayer(bounds, Paint())` — `painting/clip.dart:29-31` — with the `bounds`-is-the-saveLayer-bounds phrasing repeated at `:43`, `:56`, `:69-70`, `:88`.
- Layer classes visible anywhere in the readable tree: `LayerHandle<OpacityLayer>` in chips — `material/chip.dart:2194`, `:2218` (`saveLayer`), `:2230`. No `BackdropFilterLayer`/`ImageFilterLayer` reference exists in `painting/`+`material/`+`animation/` (`grep -rn "BackdropFilter"` → the single flexible-space-bar hit).
- `ImageFilter` and `FragmentProgram` are **not** re-exported by painting: the `export 'dart:ui'` list — `painting/basic_types.dart:14-59` — carries `BlendMode, BlurStyle, Canvas, Clip, Color, ColorFilter, FilterQuality, ImageShader, MaskFilter, Paint, Path, RRect, Shader, Shadow, TileMode, VertexMode`, … and neither `ImageFilter` nor `FragmentProgram` (verified: `grep -n "ImageFilter\|FragmentProgram" painting/basic_types.dart` → exit 1). Call sites must `import 'dart:ui' as ui` and write `ui.ImageFilter.blur(...)` — exactly `material/flexible_space_bar.dart:284`, and `material/color_scheme.dart:10` for `ui.Image`. The file documents the intent: "Intentionally not exported: Image, instantiateImageCodec, decodeImageFromList: We use ui.* to make it very explicit that these are low-level image APIs." — `painting/basic_types.dart:63-66`.
- TileMode: the only edge-behavior docs readable are **shader tile modes for gradients**: `TileMode.clamp` = "The edge colors are extended to fill the remaining area", `repeated`, `mirror`, `decal` = "Only the gradient is drawn, leaving the remaining area transparent" — `painting/gradient.dart:1065-1068` — (read). All three gradient classes default `tileMode = TileMode.clamp` (`:406`, `:685`, `:1013`).
  **So the familiar sentence "blur near bounds tiles with clamp and therefore smears edge color outward" is NOT documented in this checkout.** `grep -rn "TileMode"` across `painting/`+`material/` returns only the export and those gradient lines. Treat that claim as dart:ui/engine knowledge, not repo evidence.
- The other blur primitive in reach: `MaskFilter.blur(BlurStyle, sigma)` on a `Paint` — exported at `painting/basic_types.dart:31`, used by `BoxShadow`'s `blurStyle` (`painting/box_shadow.dart:42`, `:50-54`, `:64`) and heavily by package B. That is *geometry* blur, not *backdrop* blur — an important distinction for any abstraction: `BlurStyle.normal/outer/inner/solid` selects how a shape's own coverage is blurred (`material/shadows.dart:26-32` shows the `outer` conversion idiom for `kElevationToShadow`).
- `ShaderMask` and `RuntimeEffect` are **not referenced anywhere** in the readable tree (verified `grep -rn "ShaderMask\|RuntimeEffect"` over `painting/`+`material/`+`animation/` → nothing).

### 4.2 Fragment-shader integration — the one real pattern: `InkSparkle`

- Loading is `fromAsset` + async + static cache: `ui.FragmentProgram.fromAsset('shaders/ink_sparkle.frag').then((program) { _program = program; }); _initCalled = true;` — `material/ink_sparkle.dart:421-427` — inside `static void initializeShader()` (`:421`). Triggered from the feature constructor — `material/ink_sparkle.dart:123-124` — with comment "InkSparkle will not be painted until the async compilation completes." (`:123`).
- **Graceful degradation is the load-bearing detail**: `paintFeature` returns early when the program is null — "Skipping paintFeature because the shader it relies on is not ready to be used." — `material/ink_sparkle.dart:239-245` — so the first tap after startup simply shows no ripple instead of blocking a frame.
- The shader instance is created once and reused: `if (!_fragmentShaderInitialized) { _fragmentShader = _InkSparkleFactory._program!.fragmentShader(); _fragmentShaderInitialized = true; }` — `material/ink_sparkle.dart:247-250` — and disposed with `if (_fragmentShaderInitialized) { _fragmentShader.dispose(); }` (`:229-231`).
- Uniforms are a **positional float array**; Dart documents the packing in comments — `material/ink_sparkle.dart:288-350` — (read): `setFloat(0..3)` rgba each ÷ 255; `setFloat(4..7)` the packed `u_composite_1` = alpha, sparkleAlpha, 1.0 (blur), radiusScale; `setFloat(8..9)` center.xy; `10` max radius; `11..12` `1.0/_width`, `1.0/_height`; `13..14` `noiseDensity/_width`, `noiseDensity/_height`; `15` `noisePhase/1000.0`; `16..21` three circles from the turbulence seed; `22..27` cos/sin of three rotation angles.
- The GLSL declares the mirror layout with explicit locations and vec4 packing: `location 0 uniform vec4 u_color`, `location 1 uniform vec4 u_composite_1` (`// u_alpha, u_sparkle_alpha, u_blur, u_radius_scale`), `2 vec2 u_center`, `3 float u_max_radius`, `4 vec2 u_resolution_scale`, `5 vec2 u_noise_scale`, `6 float u_noise_phase`, `7-9 vec2 u_circle1..3`, `10-12 vec2 u_rotation1..3` — `material/shaders/ink_sparkle.frag:13-26` — and re-exposes the packed vec4 as named floats at `:36-39`.
- Shader header requirements, read: `#version 320 es` (`material/shaders/ink_sparkle.frag:1`), `precision highp float;` (`:7`), `#include <flutter/runtime_effect.glsl>` (`:10`) — Flutter's runtime-effect shim, the only engine shader-lib dependency visible anywhere here; `layout(location = 0) out vec4 fragColor;` (`:28`).
- Draw pattern: canvas save → `_transformCanvas` → shape clip (`material/ink_sparkle.dart:253-262`) → `_updateFragmentShader()` → `Paint()..shader = _fragmentShader` → `canvas.drawRect(_clipCallback(), paint)` or `canvas.drawPaint(paint)` → restore (`:266-270`). Uniforms mutate on the live shader object each frame; no per-frame program rebuild.
- Provenance: "All double values for uniforms come from the Android 12 ripple implementation" (`RippleShader.java`/`RippleDrawable.java`/`RippleAnimationSession.java`) — `material/ink_sparkle.dart:278-282` — and the second shader's header cites an AOSP `StretchEffect.cpp` commit — `material/shaders/stretch_effect.frag:7-8`.
- Constant math is deliberately moved to Dart for perf: "The Android 12 version has these values calculated in the GLSL. They are constant for every pixel in the animation, so the Flutter implementation computes these animation values in software in order to simplify the shader implementation and provide better performance on most devices." — `material/ink_sparkle.dart:189-194` — (read).
- The two shaders use **different uniform styles**: `ink_sparkle.frag` uses `layout(location = N) uniform` (`:13-26`); `stretch_effect.frag` uses bare named uniforms `uniform vec2 u_size; uniform sampler2D u_texture; uniform float u_max_stretch_intensity; uniform float u_overscroll_x; uniform float u_overscroll_y; uniform float u_interpolation_strength;` (`material/shaders/stretch_effect.frag:11-24`) — (read).
- Only `stretch_effect.frag` samples a texture (`sampler2D u_texture`) — that is the readable precedent for **injecting backdrop pixels as a texture uniform** instead of using `BackdropFilter`: the overscroll/stretch effect rasterizes content and binds it. It is the closest in-framework proof that a displacement-style effect needs explicit capture, not a backdrop filter.
- Shader *asset registration*: `packages/flutter/pubspec.yaml` is not on disk (`ls packages/flutter/` → only `lib`), so **the `flutter: shaders:` manifest entry mapping `shaders/ink_sparkle.frag` could not be read.** The only readable evidence of the convention is the literal path `'shaders/ink_sparkle.frag'` — `material/ink_sparkle.dart:423` — matching the on-disk directory `material/shaders/`.
- `painting/shader_warm_up.dart` is the Skia-era compile-latency hatch: "When Skia first sees a certain type of draw operation on the GPU, it needs to compile the corresponding shader. The compilation can be slow (20ms-200ms). Having that time as startup latency is often better than having jank in the middle of an animation." (`:16-21`), run from `PaintingBinding.initInstances`, app-overridable via `PaintingBinding.shaderWarmUp` (`:23-28`), and "can't be pre-computed during the Flutter engine compilation as the engine is device-agnostic" (`:43-45`) — (read).
- **[unclear]** Impeller's `RuntimeEffect`/`FragmentProgram` pipeline, blur tiling at bounds, and the layer-class graph: the checked-out engine subtree is Vulkan-backend-only, no `.glsl` anywhere in `engine/` (`find` confirmed).

---

## 5. Package B — `flutter_liquid_glass`: pure widget composition, no shader

132 KB, 20 source files, 3 commits (`git log --oneline`: "Initial commit", "Initial commit of the Flutter Liquid Glass package…", "Release version 1.0.0+1"). `pubspec.yaml:1` `name: flutter_liquid_glass`; dependency is only `flutter` (`pubspec.yaml:13-14`); and `pubspec.yaml:22-23` says `flutter:` / `# No assets required` — verified: `find . -name "*.frag" -o -name "*.glsl" -o -name "*.vert" -o -name "*.png"` → **zero results**. There is no shader in this package.

### 5.1 Technique

- Backdrop pixels come from exactly one mechanism: `BackdropFilter(filter: ui.ImageFilter.blur(sigmaX: config.blurAmount, sigmaY: config.blurAmount), child: Container())` — `lib/src/effects/liquid_glass_effect.dart:31-38` — (read). No texture capture, no `RepaintBoundary.toImage`, no `Layer`, no shader read of pixels.
- Everything else is stacked painters inside `ClipRRect(borderRadius: config.borderRadius) > Stack` — `lib/src/effects/liquid_glass_effect.dart:26-93` — (read the build tree). Layer order: blur (`:31-38`), frost = a *second* `BackdropFilter` at `sigma = blurAmount * (1 + frostIntensity)` plus a white gradient (`:41-61`), `CustomPaint(LiquidGlassPainter)` (`:64-73`), content (`:76`), `IgnorePointer(CustomPaint(SpecularHighlightPainter))` (`:79-90`).
- No displacement map, no SDF, no refraction of actual pixels. "Refraction" is painted geometry: two 10 px quadrilateral strips along the top and left edges, white at `alpha = refractionIntensity * 0.1`, `MaskFilter.blur(BlurStyle.normal, 10 * glassThickness)` — `lib/src/painters/liquid_glass_painter.dart:72-97` — (path vertices at `:79-84` and `:89-94`). Backdrop content is not bent; a translucent blurred strip sits on top of it.
- "Chromatic aberration" = two offset circles: red at `center + Offset(-2, 0)`, blue at `center + Offset(2, 0)`, both `alpha 0.05`, `MaskFilter.blur(normal, 10)`, `BlendMode.screen` — `lib/src/painters/specular_highlight_painter.dart:168-196` — (read). Purely additive; no per-channel resampling.
- "Morphing" = a hand-plotted blob polygon: 36 vertices, `for (var i = 0; i < 360; i += 10)`, radius perturbed by `math.sin(angle * 3) * distortionAmount * 10` — `lib/src/painters/liquid_glass_painter.dart:99-124`. Closest thing to a distance technique in the package, and it is a polar polygon, not an SDF.
- Gradients do the "lighting": primary specular = `ui.Gradient.radial(center, shortestSide * 0.3, [white α .2/.1/transparent], [0,.5,1])` + `MaskFilter.blur(normal, 20)` — `lib/src/painters/specular_highlight_painter.dart:44-71` — and dynamic light = `RadialGradient(radius: 1.5, …)` painted with `BlendMode.overlay` over the whole rect — `lib/src/effects/dynamic_light_effect.dart:134-152`.
- Second blur primitive in use: `MaskFilter.blur(BlurStyle.normal, r)` — `lib/src/painters/liquid_glass_painter.dart:75-76`, `lib/src/effects/liquid_glass_effect.dart:114`, `:130`, `lib/src/effects/dynamic_light_effect.dart:158`, `:182`. So the entire visual vocabulary is: `BackdropFilter` (2×), `ClipRRect`, gradient shaders, `MaskFilter.blur`, `BlendMode.overlay/screen`, plain fills/strokes. No shader programs, no `FragmentProgram` anywhere (`grep -rn "FragmentProgram\|.frag" lib` → no hits).

### 5.2 Parameters — and which are decorative

`LiquidGlassConfig` declares 22 fields with defaults — `lib/src/models/liquid_glass_config.dart:6-29` — (read ctor): `baseColor?`, `opacity=0.1`, `blurAmount=10.0`, `borderRadius=Radius.circular(20)`, `enableSpecularHighlight=true`, `refractionIntensity=0.5`, `adaptToContent=true`, `shadows?`, `border?`, `gradient?`, `animationDuration=300ms`, `animationCurve=Curves.easeInOutCubic`, `enableParallax=true`, `parallaxIntensity=0.1`, `enableCustomShader=false`, `distortionAmount=0.05`, `enableDynamicLight=true`, `lightPosition=Offset(0.5,0.3)`, `enableMorphing=true`, `glassThickness=1.0`, `enableChromaticAberration=false`, `frostIntensity=0.0`. Note there is **no dispersion/tint parameter**: what a reader might call "tint" is `baseColor × opacity` (`lib/src/painters/liquid_glass_painter.dart:26-31`), and "border" is a `Border` whose *top* color/width alone are used (`:57-63`).

Presets are ctor sugar over the same fields — `lib/src/models/liquid_glass_config.dart:32-76`: `simple()`, `dark()` (black, opacity .2, blur 15, refraction .7), `vibrant()` (opacity .15, blur 20, refraction .8, aberration on, distortion .08), `subtle()` (.05 / 5 / specular off / .2 / parallax off), `frosted()` (.3 / blur 25 / frost .8 / refraction .3).

**Dead / unwired parameters** (each verified by `grep -rn` over `lib/` + `example/` for read sites, excluding declarations, `copyWith`, and docs):

- `enableCustomShader` — **no read site at all**; appears only at `lib/src/models/liquid_glass_config.dart:21`, `:158`, `:183`. The README's performance advice to "disable `enableCustomShader`" refers to a no-op flag.
- `parallaxIntensity` — **no read site** (`:20`, `:157`, `:182` only). The sole parallax consumer, `lib/src/widgets/liquid_glass_container.dart:151-156`, checks `enableParallax` and merely records `_localPosition`; no parallax offset is ever applied to any layer.
- `shadows` — **never consumed.** `config.shadows` appears nowhere in paint code; `LiquidGlassCard` *writes* one (`lib/src/widgets/liquid_glass_card.dart:34-42`) and nothing reads it. The card's `elevation` parameter therefore has **no visual effect**.
- `enableChromaticAberration` — read only at `lib/src/painters/specular_highlight_painter.dart:39` — in the file that nothing imports (see next bullet). So aberration never renders.
- **Two source files are dead code.** `lib/src/painters/specular_highlight_painter.dart` (207 lines) is imported by nobody (`grep -rn "specular_highlight_painter" .` → zero hits), while `lib/src/effects/liquid_glass_effect.dart:98-142` defines a *second, different* class with the same name and uses that one (`:82`). Consequence: edge highlights ("glass thickness"), chromatic aberration, the animated highlight radius, and `animationProgress` are all unreachable. Likewise `DynamicLightEffect`/`DynamicLightPainter` (`lib/src/effects/dynamic_light_effect.dart`, exported at `lib/liquid_glass.dart:3`) is never instantiated by any widget — `grep -rn "DynamicLightEffect" lib example` → self-references only. `enableDynamicLight` is read only inside the inline specular painter (`lib/src/effects/liquid_glass_effect.dart:127`).
- Genuinely live: `opacity`, `blurAmount`, `borderRadius`, `baseColor`, `gradient`, `border`, `refractionIntensity`, `glassThickness`, `adaptToContent` (`+ hoverAnimation`), `enableMorphing`, `distortionAmount`, `frostIntensity`, `enableSpecularHighlight`, `lightPosition`, `isPressed/isHovered/localPosition/hoverAnimation` — verified at `lib/src/painters/liquid_glass_painter.dart:28,34-37,43-45,52,57-61,67-68,74-76` and `lib/src/effects/liquid_glass_effect.dart:31,41-56,79`.
- README is also wrong about the package name: instructions say `liquid_glass_ui` and import `package:liquid_glass_ui/…` (README.md), but `pubspec.yaml:1` is `name: flutter_liquid_glass`. Code over docs — verified.

### 5.3 Interaction and state

- One composition root: `LiquidGlassContainer` (210 lines). Three `AnimationController`s — press, hover (both `config.animationDuration`), parallax (fixed 100 ms, and otherwise unused) — `lib/src/widgets/liquid_glass_container.dart:82-95`; press drives `Tween(begin: 1, end: 0.95)` (`:97-105`) applied via `Transform.scale` (`:183-184`). Hover drives a 0→1 ramp passed into painters (`:195`). "Pressed" = 5% shrink; there is no elevation, no ripple, no state layer.
- State detection is hand-rolled: `MouseRegion(onEnter/onExit/onHover)` + `GestureDetector(onTapDown/onTapUp/onTapCancel/onTap/onLongPress)` + `HapticFeedback.lightImpact()` on tap-down — `lib/src/widgets/liquid_glass_container.dart:126-149`, `:161-176`. There is **no `Focus`**, so no focus state; no `Semantics`; no `WidgetStateProperty`-style state→value mapping; no theme/`ColorScheme` integration except `Theme.of(context).primaryColor` in the nav bar's selected pill (`lib/src/widgets/liquid_glass_navigation_bar.dart:73-74`) and a hardcoded `Colors.grey` for unselected (`:84`, `:94`).
- `LiquidGlassEffect` is stateless and receives state as plain scalars: `isPressed`, `isHovered`, `localPosition`, `hoverAnimation` — `lib/src/effects/liquid_glass_effect.dart:12-22`. That is the entire "state layer" story: discrete booleans forwarded to painters, versus Flutter's `Set<WidgetState>` + resolvers.
- Buttons/cards/nav are thin wrappers over `LiquidGlassContainer`: `LiquidGlassButton` — `lib/src/widgets/liquid_glass_button.dart:6` (+ `.icon` factory `:27`); `LiquidGlassCard` — `lib/src/widgets/liquid_glass_card.dart:44`; `LiquidGlassNavigationBar` — `lib/src/widgets/liquid_glass_navigation_bar.dart:33`. `example/lib/main.dart` exercises the widgets; nothing in it reaches a shader either.

### 5.4 Architectural contrast with `liquid_glass_widgets`
Scope: `/home/ollie/git/widget-ref/liquid_glass_widgets/` was **not in my assigned targets and I did not read it**. So the right column below is the pipeline *as described in the task brief*, not code I verified — left column is fully verified.
**Pointer, not my evidence:** a teammate mined that repo into `docs/research/liquid-glass-analysis.md` and reports it as a multi-pass premium pipeline (geometry-matte pass → 0-3 backdrop passes → render shader pass, `liquid_glass_layer.dart:771/781/846/949` per that doc), with the geometry pass rasterizing a per-shape SDF shader via `Picture.toImageSync` and displacement replaced by an encoded surface normal (`shaders/displacement_encoding.glsl:17-21`). That confirms the framing's *shape* independently — but per AGENTS.md §1.2 it is another agent's claim, unverified by me; re-read its lines before merging into the design.

| | B (verified) | "geometry-pass + render-pass" framing |
|---|---|---|
| backdrop access | `BackdropFilter` inside a clipped Stack — GPU read of whatever the compositor already has (`lib/src/effects/liquid_glass_effect.dart:31-38`) | explicit capture into a texture sampled by the effect shader (cf. `sampler2D u_texture`, `material/shaders/stretch_effect.frag:12` — A's only readable texture-injection precedent) |
| render passes | 1 pass, `CustomPaint` + stacked layers, no offscreen target | ≥2 passes (geometry → mask/distance data → render) |
| shader code | none (`pubspec.yaml:22-23`, `find` → 0 assets) | authored `.frag` per effect |
| distortion | painted strips/polygons; **pixels never move** (`liquid_glass_painter.dart:72-97`) | real UV displacement from a generated map |
| shape model | `BorderRadius` → `RRect`, plus a 36-pt polar blob (`liquid_glass_painter.dart:99-124`) | SDF/implicit geometry, analytic mask driving displacement |
| cost model | blur sigma grows with *aesthetic layer count* (two `BackdropFilter`s when `frostIntensity > 0`) | fixed shader cost + capture cost, independent of layer count |
| state plumbing | booleans per widget, hand-rolled (`liquid_glass_effect.dart:12-15`) | uniforms fed from animation values |

Decision-relevant sentence: **B buys its whole look from blur + alpha + gradient composition, so it cannot refract; refraction, edge falloff, and real chromatic split require reading backdrop pixels inside a shader, which means replacing B's `BackdropFilter` with an explicit capture (texture uniform).** A's `stretch_effect.frag` is the in-framework proof of that pattern (`sampler2D u_texture` + `u_overscroll_x/y` + `u_interpolation_strength`, `material/shaders/stretch_effect.frag:11-24`).

---

## 6. Widget coverage in `material/` and the shared button pattern

### 6.1 Taxonomy of the 184 files

Families present (names from `grep -h "^class [A-Z]"` over `material/*.dart` plus the directory listing; every one is in the checked-out set):

- **Buttons**: `ElevatedButton`, `FilledButton`, `FilledTonalButton`, `OutlinedButton`, `TextButton`, `IconButton` + `FilledIconButton`/`FilledTonalIconButton`/`OutlinedIconButton`, `FloatingActionButton` (+ `.small`/`.large`/`.extended`), `SegmentedButton`, `ToggleButtons`, `ButtonBar`, `MaterialButton` (legacy), `BackButton`/`CloseButton`/`DrawerButton`, `DropdownMenu`, `CheckboxMenuButton`, toolbar action buttons.
- **Chips**: `Chip` (base + `ChipAttributes`/`DeletableChipAttributes`), `ActionChip`, `AssistChip`, `InputChip`, `FilterChip`, `ChoiceChip`, `SuggestionChip`.
- **Cards & surfaces**: `Card`, `FilledCard`, `OutlinedCard`, `GridTile`/`GridTileBar`, `CircleAvatar`, `Banner`, `Badge`, `Divider`.
- **Dialogs & sheets**: `Dialog`, `AlertDialog`, `SimpleDialog`/`SimpleDialogOption`, `DatePickerDialog`, `DateRangePickerDialog`, `TimePickerDialog`, `AboutDialog`, `DialogRoute`, `BottomSheet`, `showModalBottomSheet`/`PersistentBottomSheetController`, `SnackBar`/`SnackBarAction`/`ScaffoldMessenger`, `Tooltip`.
- **Navigation**: `NavigationBar`+`NavigationDestination`+`NavigationIndicator`, `NavigationRail`+`NavigationRailDestination`, `NavigationDrawer`+`NavigationDrawerDestination`, `BottomNavigationBar`, `Drawer`/`DrawerController`/`DrawerHeader`/`UserAccountsDrawerHeader`, `Tabs`/`TabBar`(secondary)/`TabController`/`tab_indicator`, `AppBar`/`SliverAppBar`(medium/large)/`BottomAppBar`/`FlexibleSpaceBar`, `PageTransitionsTheme`.
- **Inputs & pickers**: `TextField`, `TextFormField`, `InputDecorator`, `InputBorder` family, `Autocomplete`, `DropdownButton`(+`FormField`), `CalendarDatePicker`, `InputDatePickerFormField`, `SearchBar`/`SearchAnchor`/`SearchView`, `Checkbox`(+`ListTile`), `Radio`(+`ListTile`), `Switch`(+`ListTile`), `Slider`, `RangeSlider`, `Stepper`, `ExpansionTile`, `ExpansionPanel`, `ReorderableList`, `SelectionArea`, text-selection controls/toolbars, `Magnifier`.
- **Feedback / progress**: `CircularProgressIndicator`, `LinearProgressIndicator`, `RefreshIndicator`, `InkWell`/`InkResponse`/`Ink`/`InkDecoration`/`InkHighlight`/`InkRipple`/`InkSplash`/`InkSparkle`, `NoSplash`, `Material`, `MergeableMaterial`, `ExpandIcon`, `AnimatedIcon` (+ `animated_icons/data/*.g.dart` keyframe sets).
- **Menus**: `MenuAnchor`, `MenuBar`, `MenuButton`, `PopupMenu`/`PopupMenuButton`, `MenuItemButton`, `SubMenuButton`, `CheckboxMenuItem`, `RadioMenuItem`, `CheckedPopupMenuItem`, `menu_style.dart`.
- **Data**: `DataTable`, `DataCell`/`DataColumn`/`DataRow`, `PaginatedDataTable`.
- **Layout & app**: `Scaffold`, `CarouselView`(+`CarouselController`, `CarouselScrollPhysics`), `ListTile`, `app.dart`/`MaterialApp`, `theme.dart`/`theme_data.dart`/`Typography`/`TextTheme`, `debug.dart`, `Adaptation`.
- **A companion `*Theme`/`*ThemeData` pair per family** (40+ pairs in the class dump; `InheritedTheme` or `InheritedWidget`): AppBar, Badge, BottomAppBar, BottomNavigationBar, BottomSheet, ButtonBar, Button, Card, CarouselView, Checkbox, Chip, DatePicker, Dialog, Divider, Drawer, DropdownMenu, ExpansionTile, FloatingActionButton, IconButton, ListTile, Menu, NavigationBar, NavigationDrawer, NavigationRail, ProgressIndicator, Radio, SearchBar/SearchView, SegmentedButton, Slider, SnackBar, Switch, TabBar, TextButton, TextSelection, TimePicker, ToggleButtons, Tooltip.
- **gen_defaults-generated M3 default blocks live in 45 files** (`grep -rc "BEGIN GENERATED TOKEN PROPERTIES" material/*.dart | grep -v ':0' | wc -l` → 45). Blocks named: ActionChip, AppBar, Badge, Banner, BottomAppBar, BottomSheet, Card/FilledCard/OutlinedCard, Checkbox, Chip, ChoiceChip, DatePicker, Dialog/DialogFullscreen, Divider, Drawer, ElevatedButton, **SurfaceTint** (`elevation_overlay.dart:159`), ExpansionTile, FilledButton/FilledTonalButton, FilterChip, **FAB**, IconButton(×4), InputChip, InputDecorator, LisTile (typo in source, `navigation_drawer.dart:717`), Menu, **Motion**, NavigationBar, NavigationDrawer, NavigationRail, OutlinedButton, PopupMenu, ProgressIndicator, Radio, RangeSlider, SearchBar/SearchView, SegmentedButton, Slider, Snackbar, Switch, Tabs, TextButton, TextField, **ColorScheme** (`theme_data.dart:3372`), TimePicker, Typography. → **the M3 numbers quoted in this report are machine-generated from a token DB that is not itself checked out.**

### 6.2 The shared MD3 button pattern

One class does the work; the three widgets are thin subclasses.

- `ButtonStyle` holds **16 `WidgetStateProperty` slots** — `material/button_style.dart:199-331` — read: `textStyle` (`:199`), `backgroundColor` (`:202`), `foregroundColor` (`:210`), `overlayColor` (`:214`), `shadowColor` (`:222`), `surfaceTintColor` (`:227`), `elevation` (`:230`), `padding` (`:252`), `minimumSize` (`:271`), `fixedSize` (`:285`), `maximumSize` (`:293`), `iconColor` (`:296`), `iconSize` (`:299`), `side` (`:321`), `shape` (`:327`), `mouseCursor` (`:331`). Plus non-state: `visualDensity`, `tapTargetSize`, `animationDuration`, `enableFeedback`, `alignment`, `splashFactory`, `backgroundBuilder`, `foregroundBuilder`.
- Layer stack, from `ButtonStyleButton.build` — `material/button_style_button.dart:578-612` — (read):
  `Semantics(container, button, enabled) > _InputPadding(minSize) > ConstrainedBox(effectiveConstraints) > Material(elevation: resolvedElevation, textStyle, shape: resolvedShape.copyWith(side: resolvedSide), color: resolvedBackgroundColor, shadowColor: resolvedShadowColor, surfaceTintColor: resolvedSurfaceTintColor, type: color == null ? transparency : button, animationDuration, clipBehavior, borderOnForeground: false) > result(= AnimatedTheme > InkWell)` — so:
  - **elevation + shape + shadow + tint belong to `Material`**; the button only resolves states → values.
  - **state layer + ripple belong to `InkWell`** — `material/button_style_button.dart:555-570`: `overlayColor` (the one resolver built at `:437-440`), `highlightColor: Colors.transparent` (`:567`), `customBorder: resolvedShape!.copyWith(side: resolvedSide)` (`:568`) so ink conforms to the shape.
  - `_InputPadding` grows the tap target, not the visual: "increases the size of the button and the button's 'tap target', but not its material or its ink splashes" — `material/button_style_button.dart:630-635`; padded min size = `kMinInteractiveDimension + densityAdjustment` = 48 dp (`:578-584`, `material/constants.dart:27`).
  - Horizontal padding is density-protected: `final double dx = math.max(0, densityAdjustment.dx);` — "Per the Material Design team: don't allow the VisualDensity adjustment to reduce the width of the left/right padding" — `material/button_style_button.dart:497-502` (read).
  - Resolution is a 3-level fallback inside `resolve`/`effectiveValue` — `material/button_style_button.dart:389-394` (read; `effectiveIconColor` at `:397-400` shows the chain explicitly): widget style → theme style → `_XxxDefaultsM3(context)`; the M2 path instead calls `styleFrom(...)` (`material/elevated_button.dart:394-418`).
- **What Elevated / Filled / Outlined share** — read side-by-side:

| | Elevated | Filled | Outlined |
|---|---|---|---|
| textStyle | `textTheme.labelLarge` (`:526`) | same (`:544`) | same (`:473`) |
| backgroundColor | `surfaceContainerLow`; disabled `onSurface@0.12` (`:530-538`) | `primary`; disabled `onSurface@0.12` (`:548-556`) | `Colors.transparent` (`:477-478`) |
| foregroundColor | `primary` / disabled `onSurface@0.38` (`:539-546`) | `onPrimary` / `onSurface@0.38` (`:557-564`) | `primary` / `onSurface@0.38` (`:481-488`) |
| overlayColor | primary **0.1 press / 0.08 hover / 0.1 focus** (`:548-559`) | onPrimary 0.1 / 0.08 / 0.1 (`:566-577`) | primary 0.1 / 0.08 / 0.1 (`:490-501`) |
| shadowColor | `_colors.shadow` (`:563-564`) | `_colors.shadow` (`:581-582`) | `Colors.transparent` (`:505-506`) |
| surfaceTintColor | `Colors.transparent` (`:567-569`) | `Colors.transparent` (`:585-587`) | `Colors.transparent` (`:509-511`) |
| elevation dp | disabled 0 / press 1 / hover 3 / focus 1 / def 1 (`:571-586`) | disabled 0 / hover 1 / else 0 (`:589-604`) | flat `0.0` (`:513-514`) |
| shape | `StadiumBorder()` (`:628-629`) | `StadiumBorder()` (`:646-647`) | `StadiumBorder()` (`:566-567`) |
| side | none | none | resolve: disabled `onSurface@0.12`, focused `primary`… (`:554-563`) |
| minimumSize | `Size(64.0, 40.0)` (`:593-594`) | same (`:611`) | same (`:521`) |
| iconSize | `18.0` (`:599-600`) | same (`:617`) | same (`:527`) |
| animationDuration | `kThemeChangeDuration` = 200 ms (`:517`, `constants.dart:39`) | same (`:535`) | same (`:464`) |
| padding | `_scaledPadding(context)` (`:589-590`) | same (`:607`) | same (`:517`) |

  (button files: `material/elevated_button.dart`, `material/filled_button.dart`, `material/outlined_button.dart`, line numbers per column above.)
  — i.e. **the shared MD3 style = `labelLarge` + `StadiumBorder` + 64×40 min + 18 px icon + 200 ms state animation + the identical 0.10/0.08/0.10 state layer; only three axes vary: fill role, dp level, outline side.** All three explicitly opt **out** of elevation tint (`transparent`) and take fill from static surface/primary roles.
- Padding scaling is density/text-scale aware and identical across the family: `padding1x = theme.useMaterial3 ? 24.0 : 16.0`, `effectiveTextScale = MediaQuery.textScalerOf(context).scale(labelLarge.fontSize ?? 14.0) / 14.0`, then `ButtonStyleButton.scaledPadding(p, p/2, p/4, effectiveTextScale)` — `material/elevated_button.dart:457-470` — (read). The `.icon` factories halve the start padding (documented "[ButtonStyle.padding] is reduced from 24 to 16" — `material/elevated_button.dart:390-391`, `material/filled_button.dart:430-431`).
- One non-obvious invariant preserved in the shared build: if an opaque button's background turns translucent while its elevation drops to 0, the framework **defers the color change** until the elevation animation completes — "Material implicitly animates its elevation but not its color. SKIA renders non-zero elevations as a shadow colored fill behind the Material's background." — `material/button_style_button.dart:507-529` — (read). A style abstraction that hides "elevation animates, color does not" will re-grow this bug.

---

## What this implies for a style abstraction API

### MD3's abstractions, as the code actually instantiates them

1. **Role, not color.** The unit of reuse is a named semantic slot — 45 of them — and the framework says so: "Each property in the [ColorScheme] class represents one color role" (`material/colors.dart:34`). Components never say "purple 40"; they say `primaryContainer`. Role-set migration happens at the getter level (`_surfaceVariant ?? surface`, `material/color_scheme.dart:1027`) without breaking call sites — an abstraction API should make deprecation exactly that cheap.
2. **Tone is the generator's business, not the consumer's.** Pipeline: `Hct.fromInt(seed)` (`material/color_scheme.dart:1980`) → scheme class (`:1981-2024`) → `MaterialDynamicColors.<role>.getArgb(scheme)` (`:343+`). Consumers see finished ARGB only; no tone table is inlined in the widget layer, and the table lives in a separate package. A style API should expose **role resolution** with tone/palette math behind a pluggable generator — Flutter's own seam is `(variant, contrastLevel)` + per-role overrides (`material/color_scheme.dart:268-272`).
3. **Brightness is one flag; `Fixed` roles are the exception hatch.** "they stay the same color between light and dark themes" (`material/color_scheme.dart:92`). Any abstraction needs slots that *refuse* to rescale, or contrast-sensitive chips/badges break.
4. **Two elevation models exist and M3 is deleting one.** dp→shadow (`material/shadows.dart:46-196`, M2, three-layer decomposition, fixed alphas 0.2/0.14/0.12), dp→tint (`material/elevation_overlay.dart:172-177`, 6 breakpoints + interpolation + clamp, `:41-66`), and the destination: **static tone containers** (`surfaceContainer*` read 252×, 40 generated `surfaceTintColor = transparent`). Endpoint in code: *elevation is a shadow concern; depth-in-color is a precomputed role concern.* A style API should model depth as one record `{shadow set, tint opacity, container role}` and let the theme choose which members are populated — not hard-code "dp means tint".
5. **State layer = (carrier, opacity) over an orthogonal state set.** `Set<WidgetState>` with hover/focus/press/drag/selected/disabled/error/scrolledUnder (measured usage §2.2), resolved per-highlight-state with the non-highlightable states carried through (`material/ink_well.dart:1343-1355`), fallback property → widget → theme (`:1357-1368`). M3 numbers: **0.10 press / 0.08 hover / 0.10 focus**, carrier flips per context (primary/onPrimary/onSurface/error — see checkbox). Disabled is its own ladder: fill `@0.12`, content `@0.38`, elevation 0. So expose a **layer primitive = role × alpha × state predicate**, not nine per-state color properties: Flutter still ships the legacy per-state `Color` props and had to force `highlightColor: Colors.transparent` (`material/button_style_button.dart:567`) to keep them from interfering.
6. **Motion tokens are named, quantized, per-call overridable.** 9 easings (`material/motion.dart:161-232`) + 16 duration buckets (`:21-159`), generated. `standard = Cubic(0.2,0,0,1)` is *the token* (`:192`); the emphasized family requires a **piecewise** representation (`ThreePointCubic`, `animation/curves.dart:1779-1785`) — one cubic cannot express it. `AnimationStyle{curve,duration,reverseCurve,reverseDuration}` + `noAnimation` (`animation/animation_style.dart:30-40`) is the escape hatch, wired to only 7 documented call sites. Since the framework admits native motion is now spring-driven and unsupported (`material/page_transitions_theme.dart:464-469`), the abstraction must hold *either* a curve *or* a spring description, and honor `disableAnimations` (`animation/animation_controller.dart:47-60`) plus a fling-spring default (`:40-43`).
7. **Generated-from-token-DB is the maintenance model.** 45 `BEGIN GENERATED TOKEN PROPERTIES` regions inside shipped files, values machine-extracted, generator + JSON DB outside the package (`dev/tools/gen_defaults/bin/gen_defaults.dart`, cited in every block header). A style API should ship the same split: token DB in, defaults class out, hand-written component code never holds literals.
8. **Effects are two different things, and only one is declarable.** `BackdropFilter` + `MaskFilter` + `Gradient` + `BlendMode` are declarative canvas primitives reachable from widgets (`material/flexible_space_bar.dart:283-286`); a `.frag` program is *asset + positional uniform block + async-compile + skip-frame fallback* (`material/ink_sparkle.dart:421-427`, `:239-245`, `:288-350`, `material/shaders/ink_sparkle.frag:13-26`). B proves the gap: its README sells "refraction / chromatic aberration" while its code paints blurred strips (`lib/src/painters/liquid_glass_painter.dart:72-97`) because it has no pixel access. A style abstraction that cannot express a uniform block is capped at B's fidelity. Flutter even keeps the seam visible: `ImageFilter`/`FragmentProgram` are deliberately **not** re-exported by painting (`painting/basic_types.dart:14-59`, `:63-66`) — the low-level/high-level boundary is a real API decision worth copying.

### vs Fluent's model

Honest scope: `fluent_ui` sits at `/home/ollie/git/widget-ref/fluent_ui/` but was not a target this turn and I did not read it. The Fluent column is stated at design-intent level and is **flagged as unverified here**; whoever merges this into the design must re-mine it per AGENTS.md §1.2.

- Fluent's center of gravity is **resource brushes** — a themed key→brush table resolved at draw time with implicit rebinding. MD3's is a **role enum resolved at build time** into plain `Color`s (`final Color`/`Color get`, `material/color_scheme.dart:833-1133`). MD3 has no brush indirection; state variation is a separate resolver type (`WidgetStateProperty`), not a property of the paint object.
- Fluent buckets motion into a handful of **duration tiers** plus a few easing presets; MD3 exposes 16 ms values in 4 bands + 9 named easings — finer, and the *component default* picks the tier, not the app. Both agree on "buckets, not free numbers"; MD3 additionally separates forward/reverse (`animation/animation_style.dart:30-36`).
- Fluent themes are authored resource dictionaries with no equivalent seed→palette generator; MD3 derives the entire scheme from one seed at runtime, and even from an image (`material/color_scheme.dart:1717-1800`). A unified contract should not lose that capability just because Fluent lacks it.
- Elevation: MD3 = numeric depth with token tables; Fluent = static layered brushes with a small shadow vocabulary. The contract must tolerate "depth is precomputed into a fill role" (M3's actual destination) as well as "depth = shadow" and "depth = tint".

### What a unified contract must expose

1. **A role slot registry, not a fixed struct.** MD3 ships 45 live + 3 deprecated; Fluent ships dozens of brush keys. Slots must be addable, deprecable, redirectable — Flutter's deprecated-getter-with-fallback (`material/color_scheme.dart:1027`, `:1142`, `:1155`) is the cheapest mechanism that verifiably works.
2. **Two orthogonal resolution axes:** variant (brightness/density/contrast/context) and interaction (state set). Fusing them (nine per-state colors) cannot express checkbox's selected-vs-unselected hover carrier flip (`material/checkbox.dart:998-1032`).
3. **State layer as `role × alpha` with the alpha ladder tokenized** (0.08 hover / 0.10 press-focus / 0.12 disabled-fill / 0.38 disabled-content, all verified above), *plus* an escape to literal colors for systems without layers. The layer must stay a separate compositing step (`Color.alphaBlend`, `material/elevation_overlay.dart:33-36`), not a pre-merged background, or hover+focus cannot stack.
4. **Depth as a 3-member optional record** `{shadowLayers, tintOpacity, containerRole}`, per-component mechanism choice. Accept heterogeneous tables: MD3's shadow map is sparse (11 keys, `material/shadows.dart:46-196`) and its tint map has 6 breakpoints with interpolation + clamping (`material/elevation_overlay.dart:41-66`).
5. **Motion = curve *or* spring**, forward/reverse pairs, a `noAnimation`/reduce-motion kill switch, piecewise-capable curve representation (`animation/curves.dart:1779-1785`), and a fine bucket ladder (MD3's 16 steps) so components can pick `short4` vs `medium4` without redefining the scale.
6. **A generator seam** so "seed → full role set" is a plugin: `variant` (9 algorithms) + `contrastLevel` (−1..1, asserted `material/color_scheme.dart:1974-1978`) + per-role override passthrough; a theme is then `(seed, variant, contrast) → scheme`, a literal scheme, or an image-derived scheme (`:1717`).
7. **Effects split into declarative layers vs shader-bound leaves.** The 80% case is a stack of named layers — backdrop-blur, tint/gradient, border, highlight, shadow — which is all B needs and all a theme can safely animate. The last mile (real refraction/displacement/chromatic split) needs an effect node with `{asset, uniform layout, pixel-access mode}`, an async-compile + skip-frame degradation contract (`material/ink_sparkle.dart:239-245` is the model), and a documented uniform-packing contract (`ink_sparkle.frag:13-26` ↔ `setFloat(0..27)`). Pixel access mode must distinguish `BackdropFilter`-style compositing from texture capture (`stretch_effect.frag:12`) — B shows these are not interchangeable.
8. **Tokens generated, never hand-literalled, and every declared knob wired.** 45 generated regions prove the model scales. B is the counter-proof: 22 config knobs with at least 4 unreachable (`enableCustomShader`, `parallaxIntensity`, `shadows`, and the whole orphaned `specular_highlight_painter.dart`/`dynamic_light_effect.dart` pair) because nothing mechanically enforced that a declared parameter is consumed. The contract needs a check for "declared but unread" — a style token nobody reads is a bug generator, and this repo's own AGENTS.md §1.4 lists "fields that were never read" as a recurring failure class.

### Honest gaps in this inventory

- Tone numbers per role: **not obtainable** here (external package + `dev/tools/gen_defaults/data/*.json` not checked out). §1.2.
- `BackdropFilter`'s layer class, and any blur edge/TileMode documentation: **not obtainable** — `widgets/` and engine `flow/` are outside the sparse set. §4.1. Zero `TileMode` sentences about blur exist in the readable tree; the only ones are gradient-specific.
- M3 shared-axis / fade-through / container-transform transitions: **not in the framework**; only doc cross-references to `OpenUpwards…` exist. Their numbers may live in the unread `new_gallery` demo.
- `SpringDescription`/`SpringSimulation` internals: `physics/` not checked out; only `animation/animation_controller.dart:40-43` and `:773-796` are readable.
- `liquid_glass_widgets`: on disk at `widget-ref/liquid_glass_widgets/` but **outside my assigned targets; not read by me**. §5.4's right column is a framing, not evidence. A sibling report (`docs/research/liquid-glass-analysis.md`) does cover it; that report is not mine and was not verified here.
- `fluent_ui`: present but not read by me; the Fluent half of the comparison is intent-level only and must be re-verified (`docs/research/fluent-ui-analysis.md` exists, also not mine).
