# GlassTabBar.bottom — implementation spec (from `liquid_glass_widgets`)

Source: `/home/ollie/git/widget-ref/liquid_glass_widgets` (`lib/` prefix). Every number is copied;
**derived** values give the formula. Path tags:

| TB `src/widgets/surfaces/tab_bar_bottom_layout.dart` · TI `…/tab_bar_bottom_internal.dart` · LU `…/tab_bar_layout_utils.dart` · PH `utils/draggable_indicator_physics.dart` · DR `src/widgets/surfaces/tab_bar_drag_gesture_mixin.dart` · GTB `widgets/surfaces/glass_tab_bar.dart` · EX `widgets/surfaces/shared/tab_bar_extra_button.dart` · AGI `widgets/shared/animated_glass_indicator.dart` · GE `widgets/shared/glass_effect.dart` · ALGL `widgets/shared/adaptive_liquid_glass_layer.dart` · LGS `src/engine/liquid_glass_settings.dart` · ST `src/engine/stretch.dart` · GS `utils/glass_spring.dart` · GD `constants/glass_defaults.dart` · GSH `constants/glass_shadow.dart` · GTD `theme/glass_theme_data.dart` · TYPES `widgets/surfaces/shared/tab_bar_types.dart` · SCL `widgets/surfaces/shared/tab_bar_searchable_controller.dart` |
|---|

Flutter framework refs: curves `flutter/lib/src/animation/curves.dart`; spring conversion
`flutter/lib/src/physics/spring_simulation.dart:70-82`; gesture constants `gestures/constants.dart`.

## 1. Public API and defaults

`GlassTabBar.bottom` (GTB:187-245) → `TabBarBottomLayout` (TB:39-95).

| Param | Default | Cite | Param | Default | Cite |
|---|---|---|---|---|---|
| `barHeight` | 64 | GTB:201, TB:51 | `barBorderRadius` | `GlassDefaults.capsuleRadius` = 9999 | GTB:202, GTB:1002, GD:87 |
| `horizontalPadding` / `verticalPadding` | 20 / 20 | GTB:199-200 | `spacing` (extra-button gap only) | 8 | GTB:198, TB:48 |
| `tabPadding` | `symmetric(horizontal: 4)` | GTB:203, TB:53 | `iconLabelSpacing` | 4 | GTB:204 |
| `iconSize` | 24 | GTB:219 | `labelFontSize` | 11 | GTB:220 |
| `textStyle` | null → size 11, `w600` selected / `w500` unselected, color = icon color | TI:223-228 | `blendAmount` / `enableBlend` | 10 / true | GTB:205-207 |
| `showIndicator` | true | GTB:209 | `indicatorColor` | null → white 10% dark, black 10% light | TI:574-577 |
| `indicatorPinchStrength` | 0.4 | GTB:212, TB:61 | `indicatorExpansion` (per side at t=1) | horizontal 12, vertical 8 | GTB:234-235, TB:83-84 |
| `indicatorBorderRadius` | null → `barRadius - 4`; capsule sentinel passes through | TI:588-594 | `indicatorSettings` | null → `baseIndicatorSettings` (§5) | GTB:211 |
| `selected/unselectedIconColor` | null → dynamic label color | TB:242-245 | `selected/unselectedLabelColor` | null | GTB:215-216 |
| `selected/unselectedLabelStyle` | null; merged last over base | TI:229-238 | `magnification` | 1.15 | GTB:228, TB:77 |
| `innerBlur` | 0.0 | GTB:229 | `maskingQuality` | `high` | GTB:230, TB:79 |
| `glowDuration` / `glowBlurRadius` / `glowSpreadRadius` / `glowOpacity` (per-tab icon glow) | 300 ms / 32 / 8 / 0.6 | GTB:222-225 | `tabWidth` | null → fills available width | GTB:232, LU:92 |
| `interactionBehavior` / `pressScale` | `full` / 1.04 | GTB:238-240 | `interactionGlowRadius` | null → native: radius 1.6, blur 16 | LU:19-23, 52-55 |
| `quality` / `backgroundQuality` | null → `premium` (both) | TB:214-223 | `adaptiveBrightness` / `platformViewBackdrop` | false / false | GTB:241-242 |
| `bottomAccessorySpacing` | 6.0 | GTB:196 | `blendAmount` when `enableBlend=false` | 0 | TB:295 |

`GlassTab` (GTB:1850-1900): `icon`, `activeIcon`, `label`, `semanticLabel`, `glowColor`,
`thickness`; icon or label required (GTB:1861-1864). `GlassTabBarExtraButton` (EX:50-63):
`size 64` (EX:55), `placement right` (EX:56), `position beforeSearch` (EX:57), enabled true.

**No `AnimationController` exists.** Motion is four spring channels (§3) plus: tab glow transform +
opacity, 300 ms `Curves.easeOutCirc` = cubic(0.075, 0.82, 0.165, 1.0) (TI:279-290;
curves.dart:1715); bottom accessory, 300 ms `Curves.easeOutCubic` = cubic(0.215, 0.61, 0.355, 1.0)
(TB:465-468; curves.dart:1671); press scale, `smooth` 300 ms spring (ST:244-252).

## 2. Layout maths

Footprint: `height = barHeight + 2*verticalPadding = 104` (GTB:1254-1256), width fills parent.
`AdaptiveLiquidGlassLayer` wraps `Padding(h20,v20)` then `LayoutBuilder` (TB:289-301). Let `W` =
`constraints.maxWidth` inside that padding (TB:313).

```
extraBtnW = extra ? extra.size + spacing : 0          // TB:310-312; 64+8 = 72
maxTabW   = W - extraBtnW                             // TB:313
tabPillW  = tabWidth == null ? max(0, maxTabW)
                             : clamp(tabWidth*n, 0, max(0, maxTabW))   // LU:83-94
tabLeft   = extra on left ? extraBtnW : 0             // TB:319, 353-355
```

Width does **not** derive from item count unless `tabWidth` is set; the pill fills `W`. Extra
button box = `size × barHeight`, pinned left/right 0 (TB:326-336). Cells (TB:361-428): `Row` of
`Expanded` inside `Container(padding: tabPadding)`:

```
trackW = tabPillW - 8      cellW = trackW / n      cell_i.x = 4 + i*cellW
cellHeight = barHeight     // no vertical tabPadding
```

Item content: `SizedBox.expand → FittedBox(scaleDown, center) → Column(min, center, spacing =
iconLabelSpacing)`; icon `iconSize`, label one line ellipsized (TI:258-345).

Indicator geometry (`AnimatedGlassIndicator`: `Positioned.fill → Padding(all 4) → Stack →
Positioned.fromRelativeRect` inside `FractionallySizedBox(widthFactor: 1/n, alignment:
Alignment(tabXAlign, 0))`, AGI:571-604):

```
a = (tabXAlign + 1)/2      innerW = tabPillW - 8      innerH = barHeight - 8
cw = innerW/n              x_rest = 4 + a*(innerW - cw)        // == (innerW-cw)*a inside padding
x(t) = x_rest - 12t        w(t) = cw + 24t
y(t) = 4 - 8t              h(t) = innerH + 16t                  // t = thickness, AGI:347-356
r_ind = indicatorBorderRadius ?? (barRadius >= 9999 ? 9999 : clamp(barRadius-4, 0, 9999))  // TI:587-594
```

Mask (`JellyClipper`, `MaskingQuality.high`; TYPES:111-155):

```
tabW = (size.width-8)/n; avail = size.width-8-tabW; rawLeft = 4 + a*avail
left = clamp(rawLeft, -inf, 4+avail);  right = clamp(rawLeft+tabW, 4+tabW, inf)
rect = LTRB(left-12t, 4-8t, right+12t, barHeight-4+8t)
maskRadius = min(2*r_ind, shortestSide/2 - 0.1)      // 2x the lens radius (TI:951, 972, 999, 1019)
```

`tabXAlign(i) = clamp(-1 + 2i/(n-1), -1, 1)` (PH:162-165), seeded at mount (DR:127); `n == 1`
gives 0/0 → NaN (**derived**, unhandled). Magnification: `scale = lerp(1, 1.15, t)` for tabs with
`i ∈ [floor(a*n)-1, ceil(a*n)+1]` only; others are empty `Expanded` slots (TB:491-527).
本仓**刻意不移植**这条放大（内容不随透镜缩放），但保留透镜罩在标签之上的折射，并把
玻璃罩住的那部分标签染成强调色：见 `docs/decisions/glass-lens-and-tab-accent.md`。

**Searchable variant** (GTB:451-520): same tab pill, plus search capsule `searchBarHeight = 50`
(GTB:469); extra button defaults to `beforeSearch`. From SCL:337-403:

```
targetH = searching ? searchBarHeight : barHeight
extraTargetW = searching ? min(size, targetH) : size
extraFullL/R = extra on that side ? size + spacing : 0
pillReserve  = showPill ? targetH + spacing : 0          // 64+8 = 72 when idle
maxTabW      = totalW - pillReserve - extraFullL - extraFullR
targetTabW   = !searching ? resolveTabPillWidth(tabWidth, n, maxTabW)
                          : (collapsedTabWidth ?? targetH)
```

So the searchable tab pill never spans the full bar.

## 3. Animated indicator

`SpringDescription.withDurationAndBounce(T, bounce)`: `m = 1`, `k = 4π²/T²`,
`ζ = bounce>0 ? 1-bounce : 1/(1+bounce)`, `c = ζ·2√k` (spring_simulation.dart:70-82).

| Channel | Target | Active spring | Released spring | Cite |
|---|---|---|---|---|
| `tabXAlign` | snapped index | `interactive` 150 ms, bounce 0.14 | `snappy` 350 ms, bounce 0.15 (or `springDescription`) | TI:634-641, GS:40-69 |
| `thickness` | 1 if `visible && (down \|\| dragging \|\| abs(value-target)>0.05)`, else 0 | `snappy` 300 ms, bounce 0.15 | same | TI:649-664 |
| `barSwayOffset` | ±0.75 fast drag, else 0 | `smooth` 250 ms, bounce 0 | same | TI:599-602, DR:113-120 |
| press | `interactionScale` 1.04 down, 1.0 up | `smooth` 300 ms | same | ST:244-252 |

**Derived** constants: interactive k=1754.60 ζ=0.86 c=72.05; snappy 350 k=322.27 ζ=0.85 c=30.52;
snappy 300 k=438.65 ζ=0.85 c=35.60; smooth 250 k=631.65 ζ=1.00 c=50.27; smooth 300 k=438.65
ζ=1.00 c=41.89. Reduce Motion snaps instead of animating (GS:364-369, 492-494).

`thickness` drives: glass fade `f = clamp(t,0,1)` → `settings.visibility` (AGI:391, 408-410;
`effectiveThickness = thickness*f` LGS:343, `effectiveRefractiveIndex = 1+(ri-1)*f` LGS:843-844);
pinch `(1-(1-f)²)·0.4` (AGI:407-410); rect expansion; resting-pill opacity `clamp(1 - t/0.15, 0, 1)`
(AGI:373); `interactionIntensity` (AGI:416); magnification. The lens widget mounts only at
`thickness > 0.01` and paints only at `fade > 0.05` (AGI:490-505); `t < 0.01 && !visible && high`
short-circuits to a plain non-indicator layout (TI:667-687).

Jelly (PH:100-139), shared by lens and mask:

```
if speed == 0 || !finite → identity + translate(0.0001, 0)                 // PH:105-112
d  = clamp(speed/velocityScale, 0, 1) * maxDistortion                      // PH:118-119
sx = (1 - |dx|*d*0.5) * (1 + |dy|*d*0.3)                                   // PH:122-131
sy = (1 - |dy|*d*0.5) * (1 + |dx|*d*0.3)
```

`velocity` is the position spring's velocity in **alignment units/s**, passed as `Offset(v, 0)`
(TI:690-695, AGI:559-563). `velocityScale = 10`; `maxDistortion = 0.8` premium, `0.35`
standard/minimal (AGI:562). Horizontal-only: `sx = 1-0.5d`, `sy = 1+0.3d`. Applied around the
inflated rect centre: lens via `Transform(alignment: center)` (AGI:554-567), mask via
translate·multiply·translate (TYPES:157-164).

## 4. Gestures

Raw `Listener` + `GestureDetector`, under `LiquidStretch` under `Transform.translate(sway, 0)`
(TI:599-633).

- **Down** (Listener, before arena): `tabIsDown = true`, `_gestureActive = true` (DR:269-276).
- **Tap** → `index = clamp(floor(clamp(localDx/width,0,1) * n), 0, n-1)` (PH:251-266). Default
  `selectsOnRelease = platformViewBackdrop = false`, so `notifyTabChanged` fires on `onTapDown`
  (DR:441-456); the pill moves because `didUpdateWidget` sets `tabXAlign` (DR:202-206, TI:561-563).
  Flutter delivers `onTapDown` at arena resolution or `kPressTimeout` 100 ms.
- **Drag start** (DR:307-313): `tabIsDragging = true`, `tabXAlign = alignmentFromGlobal(p)`; starts
  after `kTouchSlop = 18` px (`DragStartBehavior.start`).
- **Drag update** (DR:323-336); pointer → alignment (PH:187-222, `mirrorForRtl: false`):

```
rel = clamp(localDx/width, 0, 1);  p = 1/(2n);  range = 1 - 1/n
normalized = (rel - p)/range
rubber(v): v<0 → -clamp(-v*0.4, 0, 0.3);  v>1 → 1+clamp((v-1)*0.4, 0, 0.3)   // PH:45-64
align = 2*rubber(normalized) - 1
```

  Sway `delta = d.delta.dx`: if `|delta| > 4` → `clamp(delta*0.08, ±0.75)`, else 0 (DR:113-120,
  329-334) — per-frame delta, not time-normalised.
- **Release** (DR:372-404): `relX = (tabXAlign+1)/2`; `index = clamp(round(relX*(n-1)), 0, n-1)`;
  `vel = pixelsPerSecond.dx / trackWidth` (bar width, not cell); target = `index ± 1` when
  `|vel| > 0.5` and the neighbour exists, else `index`. Then `tabXAlign =
  computeTabAlignment(target)`, `notifyTabChanged(target)`, and `reconcileWithHost` springs back
  post-frame if the host declines (DR:194-199).
- **Cancel** (DR:407-431): nearest index, no velocity. **Stuck recovery**: global route force-snaps
  on an outside touch (DR:144-170); `recoverIfGestureStuck` re-resolves from the lift point and
  bumps `gestureEpoch` (DR:512-531).

Selection change is **not** an opacity cross-fade (in `high` mode) — two full icon layers are
clipped (TI:941-1032): unselected style with `JellyClipper(inverse: true)` +
`Clip.antiAliasWithSaveLayer` (visible outside the pill), selected style with the forward clipper
(visible inside). Both follow the same jelly rect. The selected layer is built with
`intensity = thickness`, so magnification grows during drag. `MaskingQuality.off` draws all
unselected icons plus one selected overlay at the target alignment with `intensity = 1.0`
(TI:849-867). Per-tab glow (only if `tab.glowColor != null`): scale 0.4 + `rotateZ(-π)` → identity,
opacity 0→1, 300 ms `easeOutCirc`, shadow alpha 0.6, blur 32, spread 8 (TI:272-307).

## 5. Glass material recipe

**Bar surface** — `kBottomBarGlassDefaults` (TI:47-57), identical in light and dark (TB:256,
289-295): `glassColor 0x3DFFFFFF` (white ≈24%), `thickness 30`, `blur 3`,
`chromaticAberration 0.3`, `lightIntensity 0.6`, `refractiveIndex 1.59`, `saturation 0.7`,
`ambientStrength 1`, `lightAngle 0.75π`; every other field at the `LiquidGlassSettings` ctor
default (visibility 1, ambientRim 0, fresnelStrength 1, glowIntensity 0.75, shadowElevation 1,
lensModel spherical, …) (LGS:99-134).

Wrapper: `AdaptiveLiquidGlassLayer(clipExpansion h20/v15, settings, quality premium, blendAmount
10 [0 if !enableBlend], platformViewBackdrop)` (TB:289-295). No `shape` is forwarded, so the layer
uses `LiquidRoundedSuperellipse(borderRadius: 0)` (ALGL:50); the visible pill is the inner
`AdaptiveGlass.grouped(shape: LiquidRoundedRectangle(barBorderRadius))` (TI:557-558, 676-686,
806-812, 897-903), joined to the parent's `LiquidGlassBlendGroup(blend: 10)` when premium + Impeller
(ALGL:163-196). Light-mode shadow = `GlassShadow` defaults: black 0x0F (≈6%) blur 8 offset (0,2) +
black 0x05 (≈2%) blur 2 offset (0,1) (GSH:26-42); disabled in dark (ALGL:166-169).

**Indicator lens** — a separate `GlassEffect`, not an `AdaptiveGlass`, kept out of the bar's
backdrop group (`GlassBackdropGroup(enabled:false) → LiquidGlass.withOwnLayer`, GE:521-534). Base
settings (AGI:183-202): transparent white `glassColor`, `thickness 20`, `refractiveIndex 1.10`,
`lightIntensity 2.0` (GD:25), `chromaticAberration 0`, `lightAngle 0.75π`, `blur 0`. Caller
`indicatorSettings` merge only fields differing from ctor defaults; blur forced 0 (AGI:225-308).
Runtime: `visibility = f`, `pinch = (1-(1-f)²)·0.4` (AGI:408-410).

| GlassEffect arg | Premium (default) | Standard/minimal | Cite |
|---|---|---|---|
| `rimThickness` | 0.8 | 0.5 → ×0.35 = 0.175 | AGI:433-436, GE:551-553 |
| `ambientRim` | 0.1 | 0.08 → ×0.7 = 0.056 | AGI:442-444, GE:554-556 |
| `baseAlphaMultiplier` | 0.2 | 0.08 | AGI:445-446 |
| `edgeAlphaMultiplier` | 0.4 | 0.15 → ×0.7 = 0.105 | AGI:447-448, GE:557-559 |
| `rimSmoothing` | 1.5 | 1.5 | GE:42 |
| `interactionIntensity` | `thickness` | same | AGI:416 |
| `clipExpansion` | h20/v15 | h20/v15 | AGI:322-325 |
| extra normalisation | — | thickness ×0.4, lightIntensity ×0.6, ambientStrength ×0.25, glowIntensity ×0.5 | GE:572-579 |

Light/dark (the glass constants do not change):

| Element | Light | Dark | Cite |
|---|---|---|---|
| resting solid pill | black 10% `0x1A000000` | white 10% `0x1AFFFFFF` | TI:574-577 |
| default icon + label | `CupertinoColors.label` #000000 | #FFFFFF | TI:73-93; Flutter `cupertino/colors.dart:382-388` |
| interaction sheen | `0x1AFFFFFF` (10% white) | `0x12FFFFFF` (7% white) | LU:26-29 |
| bar shadow | GlassShadow defaults | none | GSH:26-42, ALGL:166-169 |
| selected-tab glow | `tab.glowColor` @ 0.6, blur 32, spread 8 | same | TI:296-300 |
| sheen geometry | radius 1.6 × shortest side, blur 16, spread 0, opacity 1, `BlendMode.plus` | same | LU:19-23, TB:386-389; `glass_glow.dart:740-790`; explicit `interactionGlowRadius` uses theme primary `0x3DFFFFFF` / `0x2AFFFFFF` (GTD:605-625) |

The lens paints **above** the icon layer and sheen (it refracts them); the solid resting pill
paints **below** the icons in `high` mode (TI:906-1034, 1039-1060), above them in `off` mode
(TI:828-847).

## 6. Port notes

Paint order, bottom → top (`high`): (1) bar shadow (light) / outer layer backdrop capture;
(2) `GlassGlow` sheen, painted after its child; (2a) bar glass `AdaptiveGlass.grouped` (pill shape);
(2b) solid resting pill (`paintBackground`, no glass); (2c) icon layer (inverse-clipped
unselected, forward-clipped selected); (3) moving glass lens; (4) extra `GlassButton`, and the
bottom accessory above the pill (`AnimatedSize`, 300 ms `easeOutCubic`, bottomCenter).

Flutter-specific, replace rather than copy:

- `Opacity` / `AnimatedOpacity` are compositing layers; multiply paint alpha instead.
- `Transform.scale` defaults to `Alignment.center` (press ST:334-340, magnification TB:504-505);
  `LiquidStretch` uses 1.00001 instead of 1.0 to avoid a layer drop (ST:335-339).
- `FractionallySizedBox(alignment:)` positions by `(parent-child)*(a+1)/2` over the whole-track
  4 px inset — reproduce explicitly (§2).
- The two icon layers need `ClipPath` + save-layer; `shouldReclip` ignores changes < 0.001
  (TYPES:177-195).
- Capsule radii rely on Flutter clamping radius to half the shortest side; clamp explicitly.
- Gesture slop 18 px, press timeout 100 ms and `DragStartBehavior.start` are Flutter defaults;
  make them parameters.

Disagreements / quirks (both sides cited):

- `barBorderRadius` default 9999 publicly (GTB:202) but 32 inside both layouts (TB:52, 97;
  `tab_bar_searchable_layout.dart:131`); the public constructor always forwards 9999.
- `TabIndicator` defaults `indicatorExpansion all(8)` / pinch 1.0 (TI:476-477), but the parent
  always passes h12/v8 and 0.4 (TB:61, 83-84, 373).
- Mask radius is `2*r_ind` (TI:951 etc.) vs lens radius `r_ind` (AGI:369, 412).
- `MaskingQuality.off`'s selected overlay uses the full pill width (TI:854-867) while the row
  beneath is inset 4 px, so the two grids differ by up to 4 px; `high` does not.
- `computeTargetIndex` (PH:302-344) is unused here; snapping is reimplemented in DR:372-404.
  Only the segmented control calls it (`src/widgets/interactive/segmented_control_internal.dart:212`).
- `TabIndicatorState.buildShadowOverlay` (TI:740-761) is never called.
