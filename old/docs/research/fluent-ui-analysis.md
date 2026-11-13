# Fluent UI (Flutter) — Evidence-Based Design-System Inventory

Target: `/home/ollie/git/widget-ref/fluent_ui/` (pubspec `fluent_ui` v4.17.0, ~80MB, Dart; 126 `.dart`
files in `lib/`). All paths below are relative to that repo root.
Method: every claim cites a `file:line` I opened and read this session. Where code and comments
disagree, the code is reported. Anything I could not confirm is flagged **[unclear]**.

Key structural fact up front: `lib/src/styles/color_resources.dart:1-2` says the file is
"GENERATED FILE, DO NOT EDIT … generates the values from … Common_themeresources_any.xaml", so the
token set is a direct port of WinUI's *theme resources* dictionary, not the older UWP
`SystemControl*` names. The `SystemControlFillColor*` / `SystemControlHighlight*` naming does **not**
appear anywhere in `lib/` (grep for `SystemControl` returns nothing). The modern
`TextFillColor*` / `ControlFillColor*` / `Card*` / `Smoke*` / `Layer*` / `Mica*` naming is what this
library actually uses.

---

## 1. COLOR SYSTEM

**Three layers.** (a) raw palette `Colors` + accent-shade classes, (b) the theme-resource dictionary,
(c) the aggregate `FluentThemeData`. `lib/fluent_ui.dart` re-exports all three.

### 1a. The resource dictionary — the "theme resources" equivalent

`ResourceDictionary` is a plain immutable Dart class (a `Diagnosticable`), **not** a Flutter
inherited widget or a string-keyed map — it is 83 named `Color` fields. — verified by counting
`final Color` fields in `color_resources.dart:17-101` (`awk 'NR>=17 && NR<=187' | grep -c "final Color"`
→ **83**).

The actual field names, grouped as declared (`color_resources.dart:18-100`):

- **Text** (`:18-27`): `textFillColorPrimary / Secondary / Tertiary / Disabled / Inverse`,
  `accentTextFillColorDisabled`, `textOnAccentFillColorSelectedText / Primary / Secondary / Disabled`.
- **Control fill** (`:28-51`): `controlFillColorDefault / Secondary / Tertiary / Quarternary /
  Disabled / Transparent / InputActive`, `controlStrongFillColorDefault / Disabled`,
  `controlSolidFillColorDefault`, `subtleFillColorTransparent / Secondary / Tertiary / Disabled`,
  `controlAltFillColorTransparent / Secondary / Tertiary / Quarternary / Disabled`,
  `controlOnImageFillColorDefault / Secondary / Tertiary / Disabled`, `accentFillColorDisabled`.
- **Stroke** (`:52-68`): `controlStrokeColorDefault / Secondary`, `controlStrokeColorOnAccentDefault
  / Secondary / Tertiary / Disabled`, `controlStrokeColorForStrongFillWhenOnImage`,
  `cardStrokeColorDefault`, `cardStrokeColorDefaultSolid`, `controlStrongStrokeColorDefault /
  Disabled`, `surfaceStrokeColorDefault / Flyout / Inverse`, `dividerStrokeColorDefault`,
  `focusStrokeColorOuter`, `focusStrokeColorInner`.
- **Background / layer / material** (`:69-88`): `cardBackgroundFillColorDefault / Secondary /
  Tertiary`, `smokeFillColorDefault`, `layerFillColorDefault`, `layerFillColorAlt`,
  `layerOnAcrylicFillColorDefault`, `layerOnAccentAcrylicFillColorDefault`,
  `layerOnMicaBaseAltFillColorDefault / Secondary / Tertiary / Transparent`,
  `solidBackgroundFillColorBase / Secondary / Tertiary / Quarternary / Quinary / Senary /
  Transparent / BaseAlt`.
- **System / semantic** (`:89-100`): `systemFillColorSuccess / Caution / Critical / Neutral /
  SolidNeutral / AttentionBackground / SuccessBackground / CautionBackground /
  CriticalBackground / NeutralBackground / SolidAttentionBackground / SolidNeutralBackground`.

Note `Quarternary` (WinUI's spelling) is preserved verbatim in the field names
(`color_resources.dart:31,45,84`).

### 1b. Dark/light organization

Dark and light are two `const` constructors with hardcoded per-field defaults, not a lookup table:
- `ResourceDictionary.dark(...)` `color_resources.dart:189+` — e.g. `textFillColorPrimary = Color(0xFFffffff)` (`:190`).
- `ResourceDictionary.light(...)` `color_resources.dart:276+` — e.g. `textFillColorPrimary = Color(0xe4000000)` (`:277`).

Selected values confirming ARGB-alpha tokens (not solid colors) are used for the translucent fills:
- dark `solidBackgroundFillColorBase = Color(0xFF202020)` `color_resources.dart:253`
- light `solidBackgroundFillColorBase = Color(0xFFf3f3f3)` `color_resources.dart:340`
- light `cardBackgroundFillColorDefault = Color(0xb3ffffff)` `:328`; dark `0x0dffffff` `:241`
- light `smokeFillColorDefault = Color(0x4d000000)` `:331`; dark `0x4d000000` `:244`
- focus pair light `focusStrokeColorOuter = 0xe4000000` / `Inner = 0xb3ffffff` `:326-327`; dark
  inverted `Outer = 0xFFffffff` / `Inner = 0xb3000000` `:239-240`.

`ResourceDictionary.lerp(a,b,t)` exists and lerps every one of the 83 colors —
`color_resources.dart:362+` (e.g. `textFillColorPrimary: Color.lerp(...)` `:368`). It has
`debugFillProperties` (`:787`) but **no `copyWith` and no `operator==`/`hashCode`** — verified
`grep -n "copyWith\|operator ==\|hashCode" lib/src/styles/color_resources.dart` returns nothing. This
matters for theme-switch identity (see §6).

### 1c. How accent is expanded into shades — derivation IS present

Accent colors are swatches keyed by **string**, not numbers.
`class AccentColor extends ColorSwatch<String>` `color.dart:278`, with a `primary` key
(default `'normal'`, `color.dart:291`) and a `Map<String,Color> swatch`.

Two shade models coexist:
- **Named 7-stop ramp** for the 8 built-in accents (`Colors.yellow/orange/red/magenta/purple/blue/
  teal/green`, each an `AccentColor.swatch({...})`, e.g. `blue` `color.dart:168-176`). Getters:
  `darkest, darker, dark, normal, light, lighter, lightest` (`color.dart:295-321`). When a swatch
  omits a stop, it is **derived** by alpha: e.g. `dark = swatch['dark'] ?? normal.withValues(alpha:0.9)`
  (`:305`), `light = swatch['light'] ?? normal.withValues(alpha:0.9)` (`:313`),
  `darker = swatch['darker'] ?? dark.withValues(alpha:0.8)` (`:300`). (These fallbacks change alpha,
  not hue — a divergence from WinUI's perceptual ramps; see §7.)
- **Numbered ramp** only for `Colors.grey`: `ShadedColor grey` `color.dart:81-107`, shades 10–220
  (`220: 0xFF11100F` … `10: 0xFFFAF9F8`).

**The actual derivation function** for arbitrary colors: `ColorExtension.toAccentColor(...)`
`color.dart:394` builds a full swatch by lerping the input toward black (dark stops) and white
(light stops) with tunable factors `darkestFactor=0.38, darkerFactor=0.30, darkFactor=0.15,
lightFactor=0.15, lighterFactor=0.30, lightestFactor=0.38` (`:395-400`). So: `normal` = input, and
dark/light sides are `lerpWith(Colors.black/white, factor)`.

**primary/secondary/tertiary "brushes"** (WinUI's `AccentFillColorDefaultBrush` family) are methods on
`AccentColor`:
- `defaultBrushFor(Brightness)` `color.dart:347` → `light: dark` / `dark: lighter` (`:348-350`).
- `secondaryBrushFor` `color.dart:358` → `defaultBrushFor(brightness).withValues(alpha: 0.9)`.
- `tertiaryBrushFor` `color.dart:366` → `...withValues(alpha: 0.8)`.
Each carries a doc-comment link to WinUI `Common_themeresources_any.xaml#L163-L166` (`:344-345,355-356,363-364`).
So there is **no quaternary accent-brush**; only default/secondary/tertiary accent brushes are derived
— but the *fill* resource tier does define a `Quarternary` slot.

### 1d. The aggregate theme

`FluentThemeData` `theme.dart:261` holds palette entry points NOT in the resource dict:
`accentColor: AccentColor` `:274` (default `Colors.blue` `:477`), `activeColor` `:279`, `inactiveColor` `:284`,
`inactiveBackgroundColor`, `shadowColor`, `scaffoldBackgroundColor` `:293`, `menuColor`, `cardColor`,
`selectionColor`, plus the 83-token `resources: ResourceDictionary` field `:419`.
Defaults wire theme fields *into* resources: `scaffoldBackgroundColor ??= resources.layerOnAcrylicFillColorDefault`
`:484`; `micaBackgroundColor ??= resources.solidBackgroundFillColorBase` `:493`;
`cardColor ??= resources.cardBackgroundFillColorDefault` `:495`; `selectionColor ??= accentColor.normal`
`:496`. Brightness is one `Brightness brightness` field `:366` driving `isLight` in the factory
(`:465`), which selects `ResourceDictionary.light()` vs `.dark()` (`:472-474`).

So the closest thing to WinUI's "theme resources dictionary" is `ResourceDictionary` itself — a flat
83-field immutable object swapped wholesale (`theme.dart:472`), with no per-key override API.

`Typography` is the same shape: 8 fixed `TextStyle?` fields `display / titleLarge / title / subtitle /
bodyLarge / bodyStrong / body / caption` (`typography.dart:42-78`), built by
`Typography.fromBrightness(...)` `typography.dart:102`.

---

## 2. MATERIALS (acrylic / mica / noise)

### The four TintColor-family knobs (`theme.dart:290-330`)

`FluentThemeData` declares exactly the WinUI `AcrylicBrush` quartet as theme fields:
- `acrylicBackgroundColor` (TintColor) `theme.dart:299` — doc "corresponds to WinUI
  `AcrylicBrush.TintColor`" `:295-298`; default `light: 0xFFFCFCFC / dark: 0xFF2C2C2C` `:485-487`.
- `acrylicTintOpacity` (TintOpacity) `:304` — default `light: 0.0 / dark: 0.15` `:488`.
- `acrylicLuminosityOpacity` (TintLuminosityOpacity) `:311` — default `light: 0.85 / dark: 0.96` `:489`.
- `acrylicFallbackColor` (FallbackColor) `:316` — default `light: 0xFFF9F9F9 / dark: 0xFF2C2C2C` `:490-492`.
- `micaBackgroundColor` `:319` (default = a solid resource, not a blur).
All four are lerped on theme change (`theme.dart:654-673`).

### Is acrylic REAL? — Yes: genuine blur + real noise texture

`Acrylic` widget `acrylic.dart:61` is backed by `_AcrylicState` `acrylic.dart:145`. The blur is a real
`ImageFilter.blur(sigmaX: blurAmount, sigmaY: blurAmount)` cached in `_cachedBlurFilter`
`acrylic.dart:187-191`, default `kBlurAmount = 30` (`acrylic.dart:8`). The paint path
`_AcrylicGuts.build` `acrylic.dart:491` composes a `Stack`:
1. `BackdropFilter(filter: inherited.state.blurFilter, child: SizedBox.expand())`
   `acrylic.dart:524-527` — the actual blurred backdrop, clipped by `ShapeBorderClipper` (`:519`).
   It samples **Flutter content behind the widget**, nothing else.
2. `CustomPaint(painter: _AcrylicPainter(tintColor, luminosityColor))` `acrylic.dart:530-535`.
   `_AcrylicPainter.paint` does the WinUI two-stage: `canvas.drawColor(luminosityColor,
   BlendMode.luminosity)` then `canvas.drawColor(tintColor, alpha>=1 ? src : color)`
   `acrylic.dart:577-581`.
3. `const Positioned.fill(child: _AcrylicNoise())` `acrylic.dart:538`.

**Noise overlay is REAL and uses a texture asset, not a shader.** `_AcrylicNoise` `acrylic.dart:546`
renders `Opacity(opacity: 0.02, child: DecoratedBox(decoration: … image:
AssetImage('assets/AcrylicNoise.png', package:'fluent_ui'), repeat: ImageRepeat.repeat))`
`acrylic.dart:551-561`. The PNG exists: `ls assets/ → AcrylicNoise.png 3241 bytes`, and it is
registered in `pubspec.yaml` (`assets: - assets/AcrylicNoise.png`). So the "tiled 2% noise layer" claim
in the class doc (`acrylic.dart:25` — a comment; the pixels are `:552-560`) is backed by a real asset.

**WinUI luminosity math is ported, not faked.** `AcrylicHelper` `acrylic.dart:592`:
`getLuminosityColor` clamps HSV value to `[0.125, 0.965]` and maps opacity through `[0.15, 1.03]`
(`acrylic.dart:626-639`); `getTintOpacityModifier` reproduces the mid-point-0.50 / whiteMax-0.45 /
midPointMax-0.90 / blackMax-0.85 suppression curve (`acrylic.dart:646-678`). When
`luminosityAlpha == null`, automatic mode is used (tint modifier `acrylic.dart:603-604`;

### Fallback when blur is unavailable — REAL, via an opt-out widget

`DisableAcrylic` `acrylic.dart:689` is an `InheritedWidget`. `_AcrylicGuts.build` checks
`DisableAcrylic.of(context)` `acrylic.dart:495`; when present it returns early:
`ClipPath → DecoratedBox(color: properties.fallbackColor)` with **no BackdropFilter and no noise**
`acrylic.dart:497-506`. This is the "effects disabled / reduced transparency" path.
Consumers honor it: `FlyoutContent.useAcrylic` `flyout_content.dart:62,128`;
`menu_flyout.dart:127,142`; `auto_suggest_box.dart:718`; `pickers.dart:387,477`;
`navigation_view/view.dart:1178-1189`. Note `FlyoutContent` still layers a solid `menuColor` container
inside the acrylic (`flyout_content.dart:112-121`,
`menuColor.withValues(alpha: kMenuColorOpacity)` where `kMenuColorOpacity = 0.65` `acrylic.dart:17`).
`AnimatedAcrylic` `acrylic.dart:245` tweens tint/tintAlpha/luminosityAlpha/blurAmount/elevation with
`ColorTween`/`Tween<double?>`/`_ShapeBorderTween` (`:310-397`), retaining cached blur between frames.

### Which widgets actually use acrylic

`grep -rn "Acrylic" lib/src/controls`: `FlyoutContent` `flyout_content.dart:112`, `TeachingTip`
`teaching_tip.dart:264`, `AutoSuggestBox` overlay `auto_suggest_box.dart:1060`, `ComboBox` popup
`combo_box.dart:485`, `Picker`/date-time popups `pickers.dart:445`, `selection_controls.dart:596`
(text-selection toolbar), `NavigationView` pane `view.dart:1178`. Buttons (hover/press) do **not** use
acrylic. `MenuFlyout` gets acrylic only through the shared `FlyoutContent`.

### Mica is TOKENS + SOLID PAINT only — no real blur

`Mica` `mica.dart:36` `build` reads `theme.micaBackgroundColor` `mica.dart:88` and paints a plain
`DecoratedBox(decoration: BoxDecoration(color: boxColor …))` `mica.dart:89-96` (+ a `PhysicalModel`
when `elevation>0` `mica.dart:97-105`). Its doc is explicit: "Native WinUI Mica uses an OS
system-backdrop controller … a pure Flutter widget cannot sample that wallpaper" `mica.dart:9-10`
(doc comment; the code backs it — `mica.dart:88-96` is a plain color fill, no filter).
The `layerOnMicaBaseAlt*` / `solidBackgroundFillColor*` resources exist (§1b) but `Mica` composites
none of them and does no blur: `micaBackgroundColor` defaults to one solid color
(`theme.dart:493`). `layerOnMicaBaseAlt*` is consumed elsewhere — by `TabView` tabs, not by `Mica`
(`tab_view/tab.dart:399-405`), as flat fill colors. **REAL: acrylic. COLOUR-TOKEN-ONLY / degraded: mica.**

---

## 3. MOTION

### Duration tokens (naming check)

`FluentThemeData` declares four durations — the field is **`mediumAnimationDuration`, not
`normalAnimationDuration`** (`grep -r normalAnimationDuration lib → no matches`):
- `fasterAnimationDuration` `theme.dart:333` default **83ms** `:468` (doc: micro-interactions).
- `fastAnimationDuration` `theme.dart:338` default **167ms** `:469`.
- `mediumAnimationDuration` `theme.dart:343` default **250ms** `:470`.
- `slowAnimationDuration` `theme.dart:348` default **358ms** `:471`.
- `animationCurve: Curve` `theme.dart:353` default `standardCurve`.
`const standardCurve = Curves.easeInOut` `theme.dart:209`. All four are lerped in
`FluentThemeData.lerp` via `lerpDuration` `theme.dart:677-695`; the curve is a hard swap at `t<0.5`
`theme.dart:709`.

### Mapping to the WinUI motion spec

This library's numbers are the **Flutter/Material ramp** (83/167/250/358), not WinUI's documented
150/250/350 + `cubic-bezier(0.1, 0.9, 0.2, 1)` family:
- `faster 83ms` — no WinUI equivalent (WinUI's fastest control transition is ~150ms).
- `fast 167ms` — ≈ WinUI `ControlFastReveal`/`SmokeSlideIn` 150ms → used for flyouts, dialogs, pane,
  nav-body page swaps.
- `medium 250ms` — ≈ WinUI `NormalThemeTransition` 250ms → used for Expander.
- `slow 358ms` — ≈ WinUI `Slow` 350ms → used by the sticky nav indicator.
**[deviation, verified]**: no `Curve` here equals WinUI's `cubic-bezier(.1,.9,.2,1)`. The only
`Curves.fastOutSlowIn` usage is `ToggleSwitchThemeData` `toggle_switch.dart:488`. Curve usage across
the library is ad-hoc: `grep -o "Curves\.[a-zA-Z]*" | uniq -c` →
`linear×9, easeOut×6, easeInOut×5, easeIn×4, ease×4, easeInCirc×2, fastOutSlowIn×1, elasticInOut×1`.
So motion is **not centralized** — most controls pick their own curve and often ignore
`theme.animationCurve`.

### Per-control entrance/transition numbers actually in the code

- **Flyout** `flyout.dart`: default duration `theme.fastAnimationDuration` `:817`; default **curve
  `Curves.linear`** `:803` (inconsistent with the easeInOut theme default);
  `reverseTransitionDuration` is declared `Duration.zero` on `showFlyout` (`:786`, default arg `:802`) then
  reconciled to `transitionDuration` at `:818`. Default transition is a directional
  `SlideTransition` from `Offset(0, 0.25)` (top placements) / `Offset(0, -0.15)` (bottom/default)
  `flyout.dart:846-866`, wrapped in `FadeTransition(CurvedAnimation(Curves.ease))` `:1210`.
- **ContentDialog** `showDialog` `content_dialog.dart:242-275`: duration
  `theme.fastAnimationDuration ?? 300ms` `:272-275`; barrier `Color(0x8A000000)` `:251`;
  `FluentDialogRoute._defaultTransitionBuilder` (`content_dialog.dart:350`) = `FadeTransition(Curves.easeOut)`
  (`:357`) + `ScaleTransition` over `Tween<double>(begin: 1, end: 0.85)` with `Curves.easeOut` (`:358-363`).
- **NavigationView pane** resize `pane.dart:1153-1156` and `:1275-1277`: `AnimatedContainer` using
  `theme.animationDuration ?? Duration.zero` and `theme.animationCurve ?? Curves.linear` (i.e. **no
  animation unless the pane theme sets one**). Nav-view body page swap `body.dart:142` defaults to
  `theme.fastAnimationDuration`.
- **StickyNavigationIndicator** `indicators.dart:192`: three `AnimationController`s (shrink / grow /
  expand) `indicators.dart:264-278` initialized at **150ms** each (`Duration(milliseconds: 150)` `:266,:270,:275`);
  then `didChangeDependencies` sets `theme.slowAnimationDuration` `:290` while `didUpdateWidget` uses
  `theme.fastAnimationDuration` `:302` — three different defaults for the same animation
  (**[inconsistency, verified]**). Curve default `Curves.easeInOut` cached at `:251`, overridden from
  `theme.animationCurve` `:288`.
- **Expander** `expander.dart`: an `AnimationController` (`:281`) is driven with
  `widget.animationDuration ?? _theme.mediumAnimationDuration` and
  `widget.animationCurve ?? _theme.animationCurve` (`:297-308`); the collapse is a `SizeTransition` with
  `CurvedAnimation(curve: Interval(0, 0.5, …))` (`:433-439`), the chevron a
  `Tween<double>(0 → 0.5 turns)` (`:399-402`), plus an `AnimatedSlide` on
  `theme.fastAnimationDuration` + `Curves.easeInCirc` (`:411-413`).
- **ComboBox** popup: hardcoded `_kComboBoxMenuDuration = 300ms` `combo_box.dart:12` and
  `_kComboBoxSearchDuration = 500ms` `:13`, with staged Material-style `Interval(0,.25)` /
  `Interval(.25,.5)` curves `combo_box.dart:297-302`. Its "reveal" is a clip rect that expands from the
  selected item `combo_box.dart:529-549` (`Rect getClip`) — not a Reveal effect.
- **Tooltip** `tooltip.dart:215-224`: forward `Duration(200ms)` / `Curves.easeOut`, reverse
  `Duration(75ms)`; defaults wait 1s, show 1500ms `:557-558`.
- **ToggleSwitch** `toggle_switch.dart:487-488`: `fasterAnimationDuration` +
  `Curves.fastOutSlowIn`; applied at `:239-240`, `:312-314`.
- **AutoSuggestBox** overlay: `theme.fastAnimationDuration` + `Curves.easeInOut`
  `auto_suggest_box.dart:1006-1007`, item fade 125ms `auto_suggest_box.dart:1135`.
- **Slider** value label `showLabelDuration = 400ms` `slider.dart:285`.
- **TabView** close animation `Duration(milliseconds: 100)` `tab_view.dart:281`; scroll
  `theme.fastAnimationDuration` `:302`; `closeDelayDuration` 1s `:85`.
- **ProgressBar** indeterminate `Duration(seconds: 3)` `progress_indicators.dart:111`;
  **ProgressRing** indeterminate `Duration(seconds: 2)` `:449` with a `TweenSequence` on start/sweep
  angle `:434-441`.
- **Page transitions** `page_transitions.dart`: `EntrancePageTransition` = slide (`startFrom = 0.25`
  default `page_transitions.dart:17`) + fade `:65-79`; `DrillInPageTransition` = fade + `ScaleTransition(0.88 → 1)`
  `:105-110`; `HorizontalSlidePageTransition` = `Offset(±0.65 → 0)` slide `:155-163`;
  `SuppressPageTransition` returns the child unchanged `:168-178`. None read a theme duration — the
  caller's route supplies it.
- **Theme cross-fade** `AnimatedFluentTheme` `theme.dart:138`, default duration
  `kThemeAnimationDuration` (Flutter's 200ms, re-exported via `fluent_ui.dart:13`)
  `theme.dart:148`; `AnimatedFluentTheme` is an `ImplicitlyAnimatedWidget` (`theme.dart:138`), its
  `forEachTween` in `_AnimatedFluentThemeState` `theme.dart:171-176`.

**Bottom line on motion**: durations exist as 4 real tokens and are genuinely used for buttons
(`base.dart:219,232`), flyouts, dialogs, pane items. Curves are *not* a Fluent-spec abstraction — the
single `standardCurve = Curves.easeInOut` is easily ignored by per-widget literals.

---

## 4. CONTROL INVENTORY

Enumerated from `lib/src/controls/**` (top-level widget classes).

**Buttons** (`buttons/`, `inputs/`): `BaseButton` `base.dart:31`, `Button` `button.dart:78`,
`FilledButton` `filled_button.dart:54`, `HyperlinkButton` `hyperlink_button.dart:41`,
`IconButton` `icon_button.dart:78`, `OutlinedButton` `outlined_button.dart:46`, `SplitButton`
`split_button.dart:65`, `DropDownButton` `dropdown_button.dart:50`, `ToggleButton`
`toggle_button.dart:10`, `ToggleSwitch` `toggle_switch.dart:80`. **CommandBar** group
`commandbar.dart`: `CommandBar` `:147`, `CommandBarItem` `:633`, `CommandBarBuilderItem` `:659`,
`CommandBarCard` `:11`, `CommandBarButton` `:766`, `CommandBarSeparator` `:914`.

**Inputs / Form** (`inputs/`, `form/`): `Checkbox` `checkbox.dart:81`, `RadioButton`
`radio_button.dart:94`, `Slider` `slider.dart:72`, `RatingControl` `rating.dart:44` + `RatingIcon`
`:458`, `TextBox` `text_box.dart:140`, `PasswordBox` `password_box.dart:50`, `NumberBox`
`number_box.dart:85` (+ internal `_SpinButton` `:953`), `TextFormBox` `text_form_box.dart:57`,
`ComboBox` `combo_box.dart:565` + `ComboBoxItem` `:114` + `EditableComboBox`
`editable_combo_box.dart:25`, `AutoSuggestBox` `auto_suggest_box.dart:171`, `FormRow`
`form_row.dart:15`, text-selection controls `selection_controls.dart:252`.

**Pickers** (`pickers/`): `DatePicker` `date_picker.dart:86`, `TimePicker` `time_picker.dart:91`,
`CalendarView` `calendar_view.dart:91`, `CalendarDatePicker` `calendar_date_picker.dart:16`,
`ColorPicker` `color_picker/color_picker.dart:95` (+ `ColorBoxSpectrum` `color_spectrum.dart:173`,
`ColorRingSpectrum` `:14`, `ColorState` `color_state.dart:54`), `Picker` base `pickers.dart:338`,
`PickerDialog` `:505`.

**Surfaces / containers** (`surfaces/`): `Card` `card.dart:46`, `Expander` `expander.dart:96`,
`InfoBar` `info_bar.dart:208`, `ListTile` `list_tile.dart:46`, `Acrylic` / `AnimatedAcrylic`
`acrylic.dart:61` / `:245`, `Mica` `mica.dart:36`, `ProgressBar` `progress_indicators.dart:47`,
`ProgressRing` `:361`. Layout: `ScaffoldPage` `page.dart:65`, `PageHeader` `:261`,
`DynamicOverflow` `dynamic_overflow.dart:40`, `HorizontalScrollView`
`horizontal_scroll_view.dart:14`.
**`ScrollView` and `SettingsCard` are NOT present** — `grep -rn "SettingsCard|SettingsExpander" lib` →
empty; no dedicated `ScrollView` control (the task's "ScrollView?" resolves to Flutter's own scroll
views plus `HorizontalScrollView`).

**Navigation** (`navigation/`): `NavigationView` `navigation_view/view.dart`, `NavigationPane`
`pane.dart:97` (+ `NavigationPaneSize` `:561`), `PaneItem` `pane_items.dart:127`,
`PaneItemSeparator` `:670`, `PaneItemHeader` `:718`, `PaneItemAction` `:789`, `PaneItemExpander`
`:830`, `PaneItemWidgetAdapter` `:1283`; indicators `NavigationIndicator` (abstract)
`indicators.dart:27`, `EndNavigationIndicator` `:110`, `StickyNavigationIndicator` `:192`;
`TitleBar` `title_bar.dart:21`, `PaneToggleButton` `:685`, `PaneBackButton` `:748`;
`TabView` `tab_view/tab_view.dart:62`, `Tab` `tab/tab.dart:197`, `TabData` `:36`;
`BreadcrumbBar` `breadcrumb_bar.dart:109` + `BreadcrumbItem` `:49`; `TreeView`
`tree_view.dart:873` + `TreeViewItem` `:121` + `TreeViewController` `:610`.

**Overlays** (`flyouts/`): `Flyout` `flyout_content_manager.dart:9`, `FlyoutContent`
`flyout_content.dart:13`, `FlyoutListTile` `:150`, `FlyoutTarget` `flyout.dart:1234`,
`FlyoutController` `flyout.dart:649`, placement delegate `_FlyoutPositionDelegate` `flyout.dart:451`
(**no public `FlyoutPositioned` class exists in this port** — `grep -rn "FlyoutPositioned" lib/` returns 0;
placement is done by that private delegate plus an inline `transitionBuilder` lambda).
`MenuFlyout` `menu_flyout.dart:25`, `MenuFlyoutItem` `:259`, `ToggleMenuFlyoutItem` `:409`,
`RadioMenuFlyoutItem` `:451`, `MenuFlyoutSeparator` `:381`, `MenuFlyoutSubItem` `:539`,
`MenuBar` `menu_bar.dart:62` + `MenuBarItem` `:8`, `ContentDialog` `content_dialog.dart:62`,
`TeachingTip` `teaching_tip.dart:133` + `TeachingTipBorder` `:413`, `Tooltip` `tooltip.dart:68`.

**Progress / utils**: `Divider` `divider.dart:43`, `InfoBadge` `info_badge.dart:24` (with
`InfoBadgeSeverity` = `InfoBarSeverity` typedef `:4`, and `.attention/.informational/.success/.critical`
ctors `:37-58`), `InfoLabel` `info_label.dart:13`, `Scrollbar` `scrollbar.dart:8`,
`RoundedRectangleGradientBorder` `rounded_rectangle_gradient_border.dart:12`, `WindowsIcon`
`windows_icon.dart:16`, `CustomIcons`/`ChevronDown` `custom_icons.dart:11`.
**"RingSizing" from the task prompt does not exist**; the ring is `ProgressRing` with
`_kMinProgressRingIndicatorSize = 36` `progress_indicators.dart:6`.
**Pivot / FlipView / SplitView are absent** — `grep -i "pivot|flipview|splitview" lib` returns no class
matches (`SplitButton` exists; `SplitView` does not).

### Visual-state model — representative controls

State is a `Set<WidgetState>` (Flutter `WidgetState`: `hovered / pressed / focused / disabled /
selected …`). `HoverButton` is the interaction engine: `states` returns `{disabled}` when not enabled,
else the union of `{pressed, hovered, focused}` from internal booleans `hover_button.dart:339-350`;
`focused` is fed by `FocusableActionDetector.onShowFocusHighlight` `hover_button.dart:401-403`. A
convenience resolver `WidgetStateExtension.forStates(...)` encodes the priority
disabled → pressed → hovered → focused → none `hover_button.dart:470-490`. `WidgetStateProperty`
appears 214 times; 28 of the 72 files under `lib/src/controls` use it.

- **Button (standard)** `button.dart:98-113` → `ButtonThemeData.buttonColor(context, states)`
  `buttons/theme.dart:312-325`: pressed `controlFillColorTertiary`, hovered
  `controlFillColorSecondary`, disabled `controlFillColorDisabled`, none `controlFillColorDefault`.
  Foreground `buttonForegroundColor` `:332-343`: pressed `textFillColorSecondary`, disabled
  `textFillColorDisabled`, else `textFillColorPrimary`. Shape `shapeBorder` `:346-370`: pressed/disabled
  → flat `RoundedRectangleBorder(side: controlStrokeColorDefault, radius 4)`; **default/hover →
  `RoundedRectangleGradientBorder`** with `LinearGradient(begin: center, end: Alignment(0,3),
  colors: [controlStrokeColorSecondary, controlStrokeColorDefault], stops: [0.3, 1.0])` — the Fluent
  "lighter on top" 1px border. Painted through `AnimatedContainer(duration:
  theme.fasterAnimationDuration, curve: theme.animationCurve)` `base.dart:217-222` plus
  `AnimatedDefaultTextStyle(duration: theme.fastAnimationDuration)` `base.dart:230-233`, and focus is
  `FocusBorder(focused: states.isFocused)` `base.dart:245`.
- **FilledButton (accent)** `filled_button.dart:100-124`: disabled `resources.accentFillColorDisabled`
  `:102`, pressed `accentColor.tertiaryBrushFor(theme.brightness)` `:104`, hovered
  `secondaryBrushFor` `:106`, none `defaultBrushFor` `:108`. Foreground pressed
  `textOnAccentFillColorSecondary`, else `textOnAccentFillColorPrimary`, disabled
  `textOnAccentFillColorDisabled` `:113-124`. Shape: on pressed/disabled a transparent-side
  `RoundedRectangleBorder`, otherwise a gradient border over
  `[controlStrokeColorOnAccentSecondary → controlStrokeColorOnAccentDefault]` stops `[0.33,1.0]`
  (`filled_button.dart:126-150`).
- **Checkbox** `checkbox.dart:81` — three-state (`final bool? checked` `:97`; `null` = indeterminate,
  rendered by `_ThirdStateDash` `:230`). `CheckboxThemeData` resolves `checkedDecoration` /
  `uncheckedDecoration` / `thirdstateDecoration` / `checkedIconColor` / `uncheckedIconColor` as
  `WidgetStateProperty`s `:321-336`. Unchecked fill disabled `controlAltFillColorDisabled`, pressed
  `controlAltFillColorQuarternary`, hovered `controlAltFillColorTertiary`, none
  `controlAltFillColorSecondary`; border `controlStrongStrokeColorDefault` /
  `…Disabled` — `checkbox.dart:382-396`. Checked fill = `ButtonThemeData.checkedInputColor`
  `buttons/theme.dart:375-382`, which delegates to `FilledButton.backgroundColor` (accent ramp).
- **TextBox** `text_box.dart` — pressed **or** focused → `resources.controlFillColorInputActive`
  `:1431-1432` (WinUI's "input active" white token; verified `else if (states.isPressed || states.isFocused)`
  at `:1431`), hover uses `controlFillColorSecondary` `:1434`; `FocusBorder` is drawn on
  `states.isFocused` `:1549`.
- **InfoBar** `info_bar.dart:208` — its "state" axis is **severity**, not interaction:
  `enum InfoBarSeverity` `info_bar.dart:105`. `InfoBarThemeData` exposes `decoration`/`iconColor`/`icon` as
  `InfoBarSeverityCheck<T>` function types (`typedef … = T Function(InfoBarSeverity)`
  `:539`), and the defaults pick `systemFillColorAttentionBackground` /
  `…CautionBackground` / `…SuccessBackground` / `…CriticalBackground` for fills (`:591-601`) and
  `systemFillColorCaution/Success/Critical` for icon color (`:622-631`), with a `cardStrokeColorDefault`
  border (`:606`). Severity is consumed by `InfoBar.build` at `info_bar.dart:374,405,416`.
- **NavigationView / PaneItem** — **selected** state overlays hover/press:
  `navigation_view/theme.dart` holds `tileColor: WidgetStateProperty<Color?>?` `:106`, plus
  `selectedTextStyle` / `unselectedTextStyle` / `selectedTopTextStyle` / `unselectedTopTextStyle` /
  `selectedIconColor` / `unselectedIconColor` as `WidgetStateProperty`s `:127-142`
  → resolved in `fromResources` (`navigation_view/theme.dart:177`) against
  `textFillColorPrimary/Secondary/Tertiary/Disabled` (`:193-228`).
  defaults to `accentColor.defaultBrushFor(brightness)` (`theme.dart:516`) and drives the
  `StickyNavigationIndicator` bar (`indicatorSize = 3.0` `indicators.dart:201`; ctor defaults `:199-201`;
  `leftPadding = 10.0` doc says "matches WinUI3 specifications" `indicators.dart:213-215` (doc).
  `NavigationPaneThemeData.fromResources` is called from the theme factory rather than being a `const`
  default (`theme.dart:512-518`).

---

## 5. FOCUS + KEYBOARD

### Focus rectangle — two-layer solid ring; Reveal glow is scaffolded but OFF

`FocusBorder` `focus.dart:10` draws **two concentric solid `BorderSide`s — not dashed, not a light
gradient**: `_buildBorder` nests `DecoratedBox(style.buildPrimaryDecoration(focused))` around
`DecoratedBox(style.buildSecondaryDecoration(focused))` `focus.dart:69-82`. Defaults come from
`FocusThemeData.standard(theme)` `focus.dart:216-228`:
- `primaryBorder = BorderSide(width: 2, color: theme.resources.focusStrokeColorOuter)` `:219-222`
- `secondaryBorder = BorderSide(color: theme.resources.focusStrokeColorInner)` `:223` (WinUI's dark+
  light dual ring; note the values are *inverted* between light and dark — §1b)
- `borderRadius: BorderRadius.circular(6)` `:218`
- `glowColor = accentColor.withValues(alpha: 0.15)` `:224`, **`glowFactor: 0`** `:225`,
  `renderOutside: true` `:226`.
The 4-direction halo (the closest thing to a Reveal focus glow) is implemented in
`buildPrimaryDecoration` `focus.dart:267-300` (four `BoxShadow`s at `Offset(±1,±1)`,
`spreadRadius: glowFactor`, `blurRadius: glowFactor * 2.5`) but is **inert because the default
`glowFactor` is 0** — the guard is `focused && glowFactor != 0 && glowColor != null` `focus.dart:273`.
Rendering strategy is selectable: `useStackApproach` default `true` overlays the ring via `Stack` +
`PositionedDirectional(start/end/top/bottom: -borderWidth)` `focus.dart:95-112`; with `false` it
insets a transparent placeholder border `focus.dart:113-115`. `renderOutside` controls
`Clip.none` vs `Clip.hardEdge` `focus.dart:96-97`.
Resolution order is documented and implemented as
`FocusThemeData.standard(theme).merge(theme.focusTheme).merge(inheritedTheme?.data)`
`focus.dart:160-167` — i.e. WinUI defaults → app theme → local `FocusTheme`.
19 files call `FocusBorder(`: all button families, checkbox, radio, slider, toggle_switch,
split_button, tree_view, tab, list_tile, expander, pane_items, pickers, date/time pickers,
flyout_content, menu_bar, selection_controls.

### Tab / directional movement

Tab order is Flutter's default `FocusTraversalPolicy`, but navigable regions are grouped with
`FocusTraversalGroup`: nav-view pane `pane.dart:921,1188,1199,1343,1355`, nav-view body (one group per
page, kept alive) `body.dart:96,100,125`, `Tab` content `tab/tab.dart:189`, `TreeView`
`tree_view.dart:1174`, `RatingControl`, `AutoSuggestBox`.
Directional (arrow-key) traversal is handled **locally, not globally**: the wheel `Picker` registers a
`DirectionalFocusIntent` callback that calls `FocusTraversalGroup.of(context).travel(TraversalDirection.up/
down/left/right)` `pickers.dart:207-218`; `TreeViewItem` binds `SingleActivator` arrow / Home / End
shortcuts `tree_view.dart:1366-1404`; `RatingControl` maps `LogicalKeySet` arrows to a custom
`_AdjustSliderIntent` `rating.dart:194-200,415`. There is **no custom `FocusTraversalPolicy` subclass**
and **no `FocusTrap` widget anywhere** (`grep FocusTrap → empty`) — modality is approximated by
`FocusScope(autofocus: true)` + an `Actions { DismissIntent: … }` handler on dialogs
`content_dialog.dart:337-341`, flyouts `flyout.dart:1201-1205`, and `MenuFlyout`.

### Keyboard-vs-mouse focus distinction ("focus visual")

**Yes, but implicitly delegated.** `HoverButton` wraps its child in `FocusableActionDetector` and sets
`_shouldShowFocus` only from `onShowFocusHighlight` `hover_button.dart:398-404`; that callback is
driven by Flutter's `FocusManager.highlightMode` (manifest-focus only when keyboard-driven). Only that
flag contributes `WidgetState.focused` `hover_button.dart:345`, and only `WidgetState.focused` renders
the ring (`base.dart:245` etc.). So hover or mouse click alone does not paint a focus border.
Keyboard activation is separate: `ActivateIntent` / `ButtonActivateIntent` → `handleActionTap()` sets
`_pressing = true`, fires `onFocusTap` + `onPressed`, then waits
`Future.delayed(theme.fastAnimationDuration)` before clearing `hover_button.dart:287-295` — a
keyboard-press visual state. `Slider` `slider.dart:410` and `RatingControl` `rating.dart:310` use the
same `onShowFocusHighlight` mechanism. There is **no first-class Fluent `FocusVisualPrimary/Secondary`
brush abstraction** beyond the two `BorderSide`s and no per-input-source config exposed.

---

## 6. THEME SWITCHING AT RUNTIME

**Mechanism = InheritedWidget dependency + optional lerp tween.**
`FluentTheme.of(context)` → `context.dependOnInheritedWidgetOfExactType<_FluentTheme>()!.data`
`theme.dart:61-63`; `_FluentTheme extends InheritedTheme` `theme.dart:91` with
`updateShouldNotify => oldWidget.data != data` `theme.dart:97-98`. `FluentTheme.build` also injects
`IconTheme(data.iconTheme)` and `DefaultTextStyle(data.typography.body!)` for the subtree
`theme.dart:80-86`. `maybeOf` exists for optional lookup `theme.dart:69-72`. So a new `FluentThemeData`
instance triggers a top-down dependent rebuild — CupertinoApp/Material-like, no custom notifier.

`FluentApp` wires it as `AnimatedFluentTheme(curve: themeData.animationCurve, data: themeData)`
`fluent_app.dart:500-503`, inside its `WidgetsApp` `builder`. `AnimatedFluentTheme`
`theme.dart:138` is an `ImplicitlyAnimatedWidget` whose state `forEachTween` maintains a
`FluentThemeDataTween` `theme.dart:171-176`, and `FluentThemeDataTween.lerp(t) => FluentThemeData.lerp`
`theme.dart:121`. So switching `ThemeMode` / replacing the theme **cross-animates**: colors lerp via
`Color.lerp`, `ResourceDictionary.lerp(a.resources, b.resources, t)` `theme.dart:639`, durations via
`lerpDuration` `theme.dart:677-695`; discrete fields swap at `t<0.5`
(`brightness` `theme.dart:637`, `visualDensity` `:638`, `animationCurve` `:697`,
`cursorOpacityAnimates` `:698`). `FluentThemeData.light()` / `.dark()` are shorthand ctors
`theme.dart:615` / `:626`. `theme(context)` resolves `ThemeMode.system|light|dark` against
`MediaQuery.platformBrightnessOf(context)` `fluent_app.dart:448-465`. `FluentApp` also mirrors the
theme into a Material `m.AnimatedTheme`/`ColorScheme.fromSwatch` so embedded Material widgets follow
along `fluent_app.dart:472-499`.

**Correctness caveat (verified, real limitation):** `FluentThemeData`, `ResourceDictionary`,
`Typography`, and `ButtonThemeData` declare **no `operator==`/`hashCode`** — `grep -c "operator =="` →
0 in each of `theme.dart`, `color_resources.dart`, `typography.dart`, `buttons/theme.dart`. So
`updateShouldNotify` compares by object identity: any newly-allocated theme instance notifies every
dependent, even if all 83 tokens are identical. Value-equality-based diffing is impossible. Also
`ButtonThemeData.lerp` returns `const ButtonThemeData()` and discards both inputs
`buttons/theme.dart:256-261` — button styles **pop** rather than animate during a theme transition.

**Per-widget overrides — three levels:**
1. Widget constructor args win over theme, e.g. `Acrylic` resolves
   `widget.tint ?? theme.acrylicBackgroundColor`, `widget.tintAlpha ?? theme.acrylicTintOpacity`
   `acrylic.dart:177-178` (in `_updateProperties` `acrylic.dart:173`); `Mica(backgroundColor:)` `mica.dart:88`;
   `Card(backgroundColor:, borderColor:)` `card.dart:122,125`; `FlyoutContent(useAcrylic: false)` `flyout_content.dart:128`;
   `Expander(animationDuration:)` `expander.dart:300`; `StickyNavigationIndicator(duration:, curve:)`
   `indicators.dart:288,290`.
2. Local component theme widgets (`InheritedTheme`) merge over the global one — `ButtonTheme`,
   `CheckboxTheme`, `RadioButtonTheme`, `ToggleSwitchTheme`, `ToggleButtonTheme`, `SliderTheme`,
   `InfoBarTheme`, `DividerTheme`, `FocusTheme`, `ScrollbarTheme`, `TooltipTheme`,
   `ContentDialogTheme`, `NavigationPaneTheme` — all fields on `FluentThemeData` `theme.dart:374-413`,
   each merged in `copyWith` via `this.X.merge(newX)` `theme.dart:814-831`.
3. Explicit `style:` objects with `WidgetStateProperty` fields; buttons resolve
   `widgetValue ?? themeValue ?? defaultValue` `base.dart:155-159`.

**Swappable "resource dictionary"?** Partly. `FluentThemeData.resources` `theme.dart:419` is injectable
via the factory param `:461` and `copyWith(resources:)` `:784`, so an app can hand in
`ResourceDictionary.raw(...)` to retokenize all 83 slots without touching widget code (37 control files
reference `theme.resources.*`, 177 such references). But it is **not key-addressable** (no
`resources['CardBackgroundFillColorDefault']`), **not extensible** with new named tokens, has no
`copyWith`, no value equality, and there is no merge/fallback chain — so it is "replace the whole
palette object", not WinUI's per-key `ResourceDictionary`.

---

## 7. GAPS vs REAL WinUI 3

1. **RevealHighlight / pointer-following light — NOT implemented.** No `Reveal` API exists
   (`grep -i "revealhighlight|class Reveal" lib` → 0 matches). The only "reveal" hits are
   `PasswordRevealMode` (the show-password eye toggle, `password_box.dart:10-33`) and
   `_ComboBoxRevealClipper` (`combo_box.dart:515`), which is a rectangular clip tweening from the
   selected item's bounds (`combo_box.dart:529-549`, `Rect getClip` at `:529`) — an entrance clip,
   not WinUI Reveal.
   pointer position to paint a border/corner light. The gradient top-border in
   `ButtonThemeData.shapeBorder` `buttons/theme.dart:346-370` is a *static* approximation of the Reveal
   look.
2. **Mica has no real backdrop, and no wallpaper/composition layering.** `Mica` = one opaque color
   `mica.dart:88-94`; the doc concedes wallpaper sampling is impossible `mica.dart:9-10` (a comment, but the
   code backs it: `Mica.build` is only `DecoratedBox(color:)` `:89-94`, no filter). The
   `layerOnMicaBaseAltFillColor*` tokens are declared (`color_resources.dart:77-80`) and **`Mica` consumes
   none of them**: `grep layerOnMicaBaseAlt lib/src/controls` hits only `TabView` tabs
   (`tab/tab.dart:399-405`), which use them as flat fill colors, not as a backdrop layer stack.
3. **Acrylic cannot cross windows.** By design it blurs only in-app Flutter content
   `acrylic.dart:21-22` (doc) — and the code agrees: the only backdrop source is the in-app
   `BackdropFilter` `acrylic.dart:524-527`. No `DesktopAcrylicBackdrop` / `SystemBackdrop` equivalent.
4. **Accent generation is alpha/hue-naive.** `toAccentColor` lerps toward black and white
   (`color.dart:394-412`), and missing named stops fall back to `.withValues(alpha: …)`
   (`color.dart:295-321`) — so `dark`/`light` of a swatch-less accent differ only in transparency, not
   in perceived lightness. WinUI generates accents perceptually. Accent brush tiers stop at tertiary
   (`color.dart:347-366`); no quaternary accent brush.
5. **No spacing / corner-radius / control-size token scale.** No `SpacingTokens`, `SizeToken`, or
   `kSpacing` abstraction exists (grep empty). Radii are hardcoded per control: 4 for buttons and
   combos (`buttons/theme.dart:354`, `combo_box.dart:38`), 6 for focus rings (`focus.dart:218`), 8 for
   flyouts (`flyout_content.dart:96`), 4 for `Card` (`card.dart:55`) — 52 occurrences of
   `circular(4|6|8)` across `lib/src` (verified: `grep -oE "circular\((4|6|8)\)" | wc -l` → 52: 31×4, 14×6, 7×8).
   pair, and no `SizeChanged`/adaptive-view-state API.
6. **Compact sizing thresholds are width-only and partial.** `NavigationView` auto mode does implement
   breakpoints: `kCompactModeThresholdWidth = 640` and `kExpandedModeThresholdWidth = 1008`
   (`pane.dart:10,13`, consumed at `view.dart:690-693`), pane widths fixed at
   `kCompactNavigationPaneWidth = 50` / `kOpenNavigationPaneWidth = 320` (`pane.dart:4,7`), plus a
   640px page-header compact breakpoint `page.dart:240`. But there is no per-control **compact density**:
   density comes solely from Flutter `VisualDensity.adaptivePlatformDensity`
   (`theme.dart:467`, applied as padding deltas `base.dart:201-206`) — no `Compact` control variants,
   no WinUI `Compact2X`/`Standard2X` equivalences, and `FlyoutContent`'s minimum width is "Eyeballed
   value from Windows Home 11" (`flyout_content.dart:4-5`, `minWidth: 118`).
7. **Motion easing is not the WinUI family and is inconsistently applied.** Only
   `standardCurve = Curves.easeInOut` `theme.dart:209`; no
   `cubic-bezier(0.1,0.9,0.2,1)` / `EaseOutEX`/`FastOutSlowIn`-per-intent mapping. Flyout defaults to
   `Curves.linear` `flyout.dart:803`; nav pane animation defaults to `Duration.zero`/`Curves.linear`
   `pane.dart:1155-1156`; combobox hardcodes 300/500ms `combo_box.dart:12-13`; indicator durations
   conflict across lifecycle methods `indicators.dart:264-278,290,302`. No
   `ConnectedAnimation`, no `Controller`, no `Composition`-level staggered collections.
8. **No ContentTransitions abstraction.** `ContentDialog`'s scale-fade and flyout slides are private
   transition builders `content_dialog.dart:350-366`, `flyout.dart:846-866`; the four
   `*PageTransition` widgets are route-level and take no theme duration `page_transitions.dart:9-179`.
9. **No XAML-style resource lookup, `x:Bind`, templates, or theme dictionaries** — everything is Dart
   constructors, so a "swap visual style without touching widget code" is limited to
   `FluentThemeData` + the 13 component theme-data objects (§6); custom control templating (WinUI
   `ControlTemplate`) has no equivalent — e.g. `Card` hardcodes a `Container` and exposes only 4 knobs
   `card.dart:46-127` (build at `:115-127`).
10. **Missing WinUI controls/classes**: `Pivot` (→ `TabView`), `SplitView`, `FlipView`,
    `SettingsCard`/`SettingsExpander`, `ScrollView`/`ScrollViewer` wrapper, `ItemsView`/`CollectionView`,
    `RadialGauge`, `NumberBox` expression parity is Flutter-side only (`math_expressions` dep,
    `pubspec.yaml`), `TeachingTip` has no light-dismiss/tail-placement parity (only placement + a
    `ScaleTransition(Curves.ease)` `teaching_tip.dart:233-236`), no `AppTitleBar` OS integration
    (`TitleBar` is an in-content header `title_bar.dart:21`), no `MenuBar` accelerators beyond basics.
11. **`ResourceDictionary` cannot be partially themed** (no `copyWith`) — the only way to change one
    token is to construct a whole new dictionary; and the theme factory does not re-derive
    `menuColor`/`cardColor` etc. from a user-supplied `resources` in `copyWith`
    (`copyWith` passes values through with `?? this.X` `theme.dart:770-800`), so overriding
    `resources` does not cascade into `scaffoldBackgroundColor`/`micaBackgroundColor`/`cardColor` the way
    the initial factory does (`theme.dart:484-495`). **This is a real behavioral asymmetry.**
12. **Light-mode acrylic tint is defaulted to zero** — `acrylicTintOpacity ??= isLight ? 0.0 : 0.15`
    `theme.dart:488`, so the tint stage is suppressed in light theme by default and only luminosity
    (0.85) shapes the material. Whether that matches real WinUI 2.x/3.x light acrylic is **[unclear —
    needs reference values]**, but it is asymmetric with dark and worth noting.

---

## What this implies for a style abstraction API

**Token taxonomy.** Model the palette as the WinUI port does — a flat, *name-addressable* resource map
(~83 slots) split into Text / ControlFill / Stroke / Background-Layer / System-semantic families,
with per-tier `Default|Secondary|Tertiary|Quarternary|Disabled|Transparent` axes rather than
ad-hoc names — but fix what this library got wrong: give the dictionary **value equality + `copyWith` +
per-key override/merge**, so a theme swap can be diffed cheaply and a single token can be retargeted.
Keep dark/light as *parameterizations of the same key set* (here they are two parallel `const` ctors
with duplicated keys — a drift hazard), and make accent derivation a **separate perceptual generator**
(default/secondary/tertiary/… brushes) instead of `withValues(alpha: …)`, which is what
`AccentColor.defaultBrushFor/secondaryBrushFor/tertiaryBrushFor` amount to (`color.dart:347-366`).
Separate "raw palette" (`Colors`, `ShadedColor`, `AccentColor`) from "semantic resources" — that split
is worth keeping; it is what makes the port auditable.

**State model.** The reusable pattern here is `Set<WidgetState>` + `WidgetStateProperty.resolveWith` +
small static resolvers (`buttonColor`, `buttonForegroundColor`, `shapeBorder`, `uncheckedInputColor`,
`checkedInputColor`) that map state→token (`buttons/theme.dart:312-397`). Codify it as: for each
control *role*, a resolver returning resource slots for
{none, hover, pressed, focused, disabled, selected, checked/indeterminate}. Two additions the port
lacks: (a) an explicit **focus-visual** concept (outer + inner stroke as tokens, and a Reveal glow with
a real default intensity rather than `glowFactor: 0` `focus.dart:225`), and (b) a **severity/status
axis** as a first-class dimension — `InfoBar` needed a function-of-severity escape hatch
(`InfoBarSeverityCheck` `info_bar.dart:539`) because interaction states alone can't express it.
Precedence must be widget-instance → local-theme → app-theme → system-default, which is exactly the
`widgetValue ?? themeValue ?? defaultValue` chain the buttons implement (`base.dart:151-158`); make that
generic instead of hand-written per control.

**Motion model.** Expose *duration + curve pairs per intent*, not four floats and one
`easeInOut`: separate `micro`, `control-fast`, `theme-normal`, `slower`, `decorative`, each with its
WinUI bezier, and require every control to consume them (the port violates this with
`Curves.linear` flyouts, 300/500ms combobox, 100ms tab close, 400ms slider label, and the indicator's
three contradictory lifecycle defaults). Also make entrance transitions declarable/composable
(slide + fade + scale with a direction parameter) rather than duplicated per widget — the existing
`EntrancePageTransition` / `DrillInPageTransition` / `HorizontalSlidePageTransition`
(`page_transitions.dart:9-179`) plus the flyout's slide and `FluentDialogRoute`'s scale-fade are
five hand-written variants of the same idea. If themes are animated, the lerp contract must cover every
component theme (`ButtonThemeData.lerp` currently returns a constant and drops both inputs,
`buttons/theme.dart:256-261`).

**Material degradation.** Treat a material as a *quality ladder resolved at build time*, which is the
single best idea to copy from this codebase: `Acrylic` resolves tint → luminosity → blur → noise →
`fallbackColor`, and degrades to the opaque fallback with no filter and no texture when
`DisableAcrylic` is in scope (`acrylic.dart:497-506`, check at `:495`, stack layers `:518-540`), with the noise as a real
tiled 2%-opacity asset (`acrylic.dart:551-561`). An abstraction should (1) name the capability
inputs — effects enabled, reduced transparency, low power, and *"can I sample anything outside my own
scene?"* — because this port proves the answer is no for mica (`mica.dart:8-10` doc, code `mica.dart:89-94`;
`acrylic.dart:21-22` doc, code `acrylic.dart:524-527`), and (2) require every material token consumer to declare its own
`FallbackColor` tier rather than silently relying on translucency. Where the port is weak and worth
improving: the material parameters live in two places (theme-level `acrylicTintOpacity` /
`acrylicLuminosityOpacity` / `acrylicFallbackColor` `theme.dart:299-316` *and* widget-level
`AcrylicProperties` `acrylic.dart:408-474`) with no shared named "brush" object, so acrylic-vs-mica-vs-card
consistency is by convention only. A real API should have one material-resource type, one degradation
policy, and one place to swap them.
