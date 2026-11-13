# liquid_glass_widgets — How Its Visual Effects Are Actually Implemented

Source studied: `/home/ollie/git/widget-ref/liquid_glass_widgets/` at commit `5d28597` ("chore(release): prepare v1.15.0").
Method: read the shader files and the engine render objects directly. Every fact cites `file:line`.
Doc/README claims are **not** treated as fact; where code and comments disagree, the code is reported and the mismatch is flagged.
Paths below are relative to the package root. Sentences kept short for translation.

---

## 1. Backdrop acquisition

**Fact: the default path obtains backdrop pixels via a `BackdropFilterLayer` whose `filter` is the render fragment shader — the engine binds the captured backdrop to the shader's sampler.** — `lib/src/engine/liquid_glass_layer.dart:1003-1004` (`final shaderLayer = (_shaderHandle.layer ??= BackdropFilterLayer())..filter = ImageFilter.shader(renderShader!);`) — verified by reading `paintLiquidGlass`; the same idiom is used for blur (805), frost (895), weight (909).

**Fact: a second acquisition path exists — a pre-captured `ui.Image` from `RenderRepaintBoundary.toImageSync(pixelRatio: dpr)`, bound directly to shader sampler slot 0, bypassing the compositor pass.** — `lib/widgets/shared/glass_effect.dart:373` (`toImageSync(pixelRatio: dpr)`), `lib/src/engine/rendering/liquid_glass_render_object.dart:843` (`..setImageSampler(0, capture)`); the branch that picks capture mode is `liquid_glass_layer.dart:981-998`.

**Fact: capture mode exists because live `BackdropFilterLayer` reads were nondeterministic on Impeller.** — `lib/src/engine/liquid_glass_layer.dart:196-206` (widget doc), `glass_effect.dart:507-511` — comment cites compositor ordering bug #99. Reasoning: comment states intent; code confirms the bypass.

**Fact: the web capture path is async `toImage()` with a `_isCapturingAsync` guard (1-frame lag).** — `glass_effect.dart:385-410`.

**Fact: capture cadence is gated on interaction.** The ticker runs only while `interactionIntensity > 0.01 && key != null && blur > 0`; while interacting it recaptures every frame; captures also restart on size/position change. — `glass_effect.dart:236-243`, `:271-287`. Verified by reading `_updateTicker`/`_handleTick`.

**Fact: capture tickers halt on `inactive`/`paused`/`hidden` and restart one frame after `resumed`.** — `glass_effect.dart:214-234` — stated reason: GPU texture churn racing `surfaceChanged` on the raster thread (Vulkan ANR).

**Fact: the `standard` (non-premium) quality tiers use captured images too.** `LightweightLiquidGlass` captures via **async** `boundary.toImage(pixelRatio: 1.0)` — `lib/widgets/shared/lightweight_liquid_glass.dart:328`.

**FACT/CODE MISMATCH: `lightweight_liquid_glass.dart:318-321` claims `toImageSync` "races the GPU and returns an all-black image on the first frame", so async is "critical" for it.** But `glass_effect.dart:299-301` claims `toImageSync` is "fully synchronous, stays in GPU memory, zero CPU←GPU readback" and runs it every frame during drags. One of these two comments is stale; the code in each file follows its own local comment. Do not read either comment as an engine fact.

**Fact: `captureOriginInScreenSpace` is the *global, screen-space, logical-pixel* origin of the `RepaintBoundary` that produced `captureImage`.** — `lib/src/engine/rendering/liquid_glass_render_object.dart:219-224` + field at `:233-238`.

**Fact: it is used to derive `uCaptureOffset`, in *physical* pixels: `captureOffset = (captureOriginInScreenSpace − thisRenderOriginLogical) * dpr`.** — `liquid_glass_render_object.dart:735-737`. This is the offset from the shader's render-surface origin to the capture image's origin.

**Fact: inside the shader, `uCaptureOffset` maps `FlutterFragCoord()` into capture-image space: `vec2 screenUV = (fragCoord + uCaptureOffset) * invTexSize;`.** — `shaders/liquid_glass_render.frag:323-325`.

**Fact: dual-mode contract of `uCaptureOffset`.** BackdropFilter mode: it is `vec2(0)` and fragCoord is pass/screen space, `uSize` = pass size. Capture mode: fragCoord is RepaintBoundary-surface space, `uSize` = captured image size, offset bridges the two origins. — `liquid_glass_render.frag:78-95` (comment block), Dart side: `liquid_glass_render_object.dart:549-551` writes `Offset.zero` in the live path, `:736-737` writes the real offset in the capture path; slot numbers match (`layer.dart:26` comment "Slots 26-27").

**Fact: the screen-space → local-space reconciliation for geometry uses *pass-relative* coordinates.** `enclosingBackdropPassRect()` walks up to the nearest enclosing `RenderBackdropFilter` or own-layer glass and returns that pass's global logical rect (clamped to the root surface). Because on Impeller `FlutterFragCoord()` inside a nested shader is relative to that pass texture, `uSize = passPhysical.size`, `uGeometryOffset = activeBounds.topLeft * dpr − passPhysical.topLeft`, `uTouchPosition = _touchPosition * dpr − passPhysical.topLeft`. — `liquid_glass_render_object.dart:110-160` (walker), `:620-683` (`writeScreenUniforms`); pass rect published at `liquid_glass_layer.dart:1002`, cleared on capture path at `:996`. Verified by reading both functions.

**Fact: geometry UV is computed from fragCoord alone (no capture offset): `geometryUV = (fragCoord − uGeometryOffset) / uGeometrySize`, clamped to [0,1].** — `liquid_glass_render.frag:335-348`. The clamp prevents Repeat-mode wraparound artifacts during stretch overshoot, and lets the alpha early-out discard fragments outside the pill.

**Fact: on the capture path the geometry uniforms are expressed *relative to the capture origin* instead:** `geometryOffsetInCapture = (activeBounds.topLeft − captureOriginInScreenSpace) * dpr` — `liquid_glass_render_object.dart:750-753`.

**Fact: ancestor *scale* breaks the capture mapping (capture is baked unscaled), so the layer freezes its transform + capture origin at the last unscaled value.** `matteTransform` / `captureOriginInScreenSpace` getters return frozen snapshots while `_hasScale` is true; scale detected as both axes < 0.9999 while a push-back scope is active. — `liquid_glass_layer.dart:624-700`, threshold constant `lib/src/renderer/liquid_glass_self_scale_scope.dart:53`; rationale comment `liquid_glass_layer.dart:966-980`.

**Fact: a widget moved without repaint (retained boundary, scroll) has its screen-space uniforms rewritten at composite time, not by a repaint.** — `liquid_glass_layer.dart:719-741` (`_updateAtComposite`), `liquid_glass_render_object.dart:370-397` (`_updateScaleState` called from paint and from composite; `onTransformChanged` deliberately avoided — comment explains first-frame miss).

---

## 2. Blur

**Fact: plain blur is the engine's own gaussian.** `ImageFilter.blur(sigmaX: blurSigma, sigmaY: blurSigma, tileMode: TileMode.mirror)` as the filter of a `BackdropFilterLayer`. — `lib/src/engine/liquid_glass_layer.dart:794-812`. The comment at `:782-783` states the inline shader blur was abandoned ("pixelated with text").

**Fact: frost is one more engine blur, clipped to alternating pixel rows.** `ImageFilter.blur(tileMode: mirror, sigma = effectiveFrost)` on a `BackdropFilterLayer` clipped to `frostRows` (odd pass-relative rows) inside the shape clip; the render shader then mixes cloud and sharp rows itself. — `liquid_glass_layer.dart:846-947`; row geometry `liquid_glass_render_object.dart:1113-1188` (`FrostRows` class: one rect per odd physical row, built once per size/scale/dpr, shifted by phase while moving); shader read `liquid_glass_render.frag:234-307` (`frostAt`).

**Fact: the progressive blur is a separable one-axis gaussian, run twice (X then Y) via `ImageFilter.compose` of two `ImageFilter.shader`s on one `BackdropFilterLayer`.** — `lib/widgets/effects/progressive_blur.dart:400-403` (`outer: vShader, inner: hShader`), `:240-250` (comment on why a disk is not used: O(σ²), streaks or noise).

**Tap count / stride rule** — `shaders/progressive_blur.frag`:
- **Fact: `const int kHalf = 24;` (line 42) → 49 taps per pass** (1 centre at `:92` + 24 symmetric pairs in the loop `:96-109`). Header `:8` documents the reduction from 97 taps.
- **Fact: stride = `max(1.0, 3.0 * sigma / float(kHalf))` device px** (line 70) — tap span fixed at ±3σ; bilinear filtering smooths between taps (`:34-36`).
- **Fact: early-out passthrough when `sigma < 0.5`** (`:63-66`).

**IIR weight recurrence** — `progressive_blur.frag:88-109`:
- **Fact: `inv2s2 = 1/(2σ²)`; `g1 = exp(−stride²·inv2s2)`; `g2 = g1·g1`; per tap `w *= g1; g1 *= g2`.** After k steps `w = exp(−(k·stride)²·inv2s2)`. Two multiplies per tap replace per-tap `exp()`. Proof in comment at `:75-87`; code at `:97-98` follows it exactly. Verified.

**Out-of-bounds handling (the critical part)** — `progressive_blur.frag:99-109`:
- **Fact: in-bounds taps are accumulated; out-of-bounds taps get zero weight; the fetch itself is still executed at a clamped UV.** `ip = step(0.0, sp.x)*step(sp.x,1.0)*step(0.0,sp.y)*step(sp.y,1.0)` (`:103`); `acc += texture(uTexture, clamp(sp, 0.0, 1.0)) * (w * ip)` (`:105-106`). Comment `:102`: "Discard out-of-bounds taps (clamping would smear the edge pixel)." — i.e. clamping the *coordinate* is done only to keep the fetch legal; the *value* is discarded by `ip`.
- **Fact: normalization adapts via a dynamic weight sum**: `wsum += w*ip + w*im` (`:107`), centre contributes 1.0 (`:93`), final divide `fragColor = acc / wsum` (`:110`). So edge pixels renormalize over only the in-bounds support instead of smearing the border colour.

**Gradient sigma formula:**
- **Fact: `sigma = uMaxSigma * pow(1.0 − g, uFalloff)`** (`:53`), where `g` is the fragment's normalized position measured from the strong edge: `rn = clamp((FlutterFragCoord().xy − uRegionOriginPx) / max(uRegionSizePx, 1.0), 0, 1)` (`:46-47`); `g = rn.y / 1−rn.y / rn.x / 1−rn.x` for `uDirection` 0/1/2/3 (strong top/bottom/left/right) (`:49-52`).
- **Fact: the gradient is normalized over the *widget's* device-pixel rect (`uRegionOriginPx`, `uRegionSizePx`, Dart indices 6-9) because the bound texture is the whole screen.** — `progressive_blur.frag:17-19, 28-29`; Dart fills them at paint time: `progressive_blur.dart:455-466` (`progressiveBlurUniforms`, maxSigma×dpr at index 2) and `:560-565` (`origin = localToGlobal(Offset.zero)` — backdrop-layer space).

**FACT/CODE MISMATCH: the Dart doc calls the shader "an importance-sampled gaussian"** (`lib/widgets/effects/progressive_blur.dart:49`; same phrase in `docs/PROGRESSIVE_BLUR.md`). The code is a plain stride-3σ/sampled gaussian with bilinear smoothing between taps (`progressive_blur.frag:34-41, 70`) — no importance sampling; the shader's own header says importance sampling was the thing *avoided* (`:11-15`). Trust the shader.

**Fact: the frost "ghost" is a small in-shader gaussian, not an engine pass.** 9 columns × 1 px + 5 even rows × 2 px = 45 taps, luminance-weighted (`blurWeight`), over the sharp rows; special-cased to 6 taps at σ ≤ 0.3; capped at σ = 2.4 physical px, wider blurs run as their own engine pass. — `liquid_glass_render.frag:224-307` (`frostAt`; taps `:255-296`, early-outs `:243-247`, `:267-292`), cap `liquid_glass_render_object.dart:63` (`frostGhostMaxSigma = 2.4`), decision `liquid_glass_layer.dart:791-794`, uniform write `liquid_glass_render_object.dart:586`.

**Fact: `ImageFilter.compose(outer: blur, inner: colorMatrix)` is used for the frost weight when a backdrop group is shared.** — `liquid_glass_layer.dart:888-893`. Weight = luma-scaled alpha (`alpha = base + slope·luma`, Rec.709 coefficients) via `ColorFilter.matrix` — `:866-877`.

**Fact: GLES ≤3.45 bound-textures are y-flipped; sampling is corrected under `LGR_GLES_FLIP_SAMPLE_Y`.** — `progressive_blur.frag:55-61`; macro + rationale `shaders/gles_compat.glsl:40-58` (flip removed since Flutter 3.46; gating on `IMPELLER_OPENGLES_UNFLIPPED_DEPRECATED`, not `IMPELLER_TARGET_OPENGLES`).

---

## 3. Geometry / refraction — pass structure

**Fact: the premium pipeline is multi-pass: a geometry-matte pass, 0-3 backdrop passes, then the render shader pass.** — `liquid_glass_layer.dart:771` (Pass 0 shadows), `:781` (Pass 1a blur), `:846` (Pass 1b frost), `:949` (Pass 2 render shader); shadow pass implementation `:1057-1153`.

**Fact: geometry pass = per-shape SDF shader rasterized synchronously.** `RenderLiquidGlassBlendGroup` binds `shaders/liquid_glass_geometry_blended.frag` (asset key `ShaderKeys.blendedGeometry` — `lib/src/engine/shaders.dart:21-22`); it records a full-bleed `drawRect` with that shader into a `Picture` (`lib/src/engine/render_liquid_glass_geometry.dart:338-377`) which is rasterized with `Picture.toImageSync` (`:381`, and layer-side `liquid_glass_render_object.dart:936-940`). Matte bounds snap to device pixels (`render_liquid_glass_geometry.dart:244-250`).

**Fact: shape parameters are uploaded as physical-pixel floats, 7 per shape: type, center.xy, size.xy, topR, bottomR.** — `lib/src/engine/liquid_glass_blend_group.dart:471-494` (`updateGeometryShaderShapes`), layout documented `shaders/sdf.glsl:56-63`; stride 7 confirmed at `:203`. `numShapes`/`dpr` go to `uShapeSettings` — `shaders/liquid_glass_geometry_blended.frag:35`; `blend` scaled by dpr — `blend_group.dart:453-459`.

**Fact: the geometry texture encodes (V1 encoding):**
| Ch | Content | Range |
|---|---|---|
| R | surface normal.x | [−1,1] → [0,1] |
| G | surface normal.y | [−1,1] → [0,1] |
| B | height (glass depth at this texel) | normalized by thickness |
| A | coverage alpha from SDF anti-aliasing | [0,1] |

— `shaders/displacement_encoding.glsl:17-21` + `encodeGeometryData` `:28-38`; write at `shaders/liquid_glass_geometry_blended.frag:149`; read in render at `liquid_glass_render.frag:374-380` (`decodeNormalXY`, `z = sqrt(1 − dot(xy,xy))`). Verified in both files.

**Fact: "V1" = the change from storing refraction-displacement to storing the true SDF-gradient normal.** Header `shaders/liquid_glass_geometry_blended.frag:7-9`; rationale `shaders/displacement_encoding.glsl:8-14` — `normalize(displacement)` diverges from real surface orientation in blend-group neck zones. Render-side comment `liquid_glass_render.frag:366-372`.
**FACT/CODE MISMATCH: the same header claims the normal comes "via dFdx/dFdy".** The code computes it with explicit finite differences: 5 full `sceneSDF` evaluations around the fragment, step `uDpr/3.0`, `dx = (sdPX − sdMX) / (2·stepDist)` — `liquid_glass_geometry_blended.frag:71-81`. No `dFdx`/`dFdy` call exists anywhere in `shaders/` (grep). The mechanism (central differences on the SDF) is real; only the wording is stale.

**Fact: the dome profile gives height and normal from the normalized distance.** `n_cos = (thickness + sdN)/thickness`, `n_sin = sqrt(1 − n_cos²)`, `normal = normalize(vec3(dx·n_cos, dy·n_cos, n_sin))`, `height = sqrt(thickness² − x²)` (clamped to thickness inside the flat face) — `liquid_glass_geometry_blended.frag:117-149`. Fragments outside the silhouette keep the outer half of the AA ramp with height 0 (`:131-142`).

**Fact: pseudo-SDF correction.** `sdN = sd / gradMag` when `gradMag > 0.1` — converts the superellipse field to a metric distance so AA and lighting behave — `liquid_glass_geometry_blended.frag:83-88`.

**Fact: refraction = Snell `refract()` through the stored normal, per fragment.** `incident = (0,0,−1)`, `invN = 1/n`, `refractLen = (height + 8·thickness)/|refract.z|`, `displacement = baseRefract.xy · refractLen · uRefractScale` — `liquid_glass_render.frag:390-406`; `uRefractScale` is Dart-set to constant 1.0 (`liquid_glass_render_object.dart:513`) — the comment there admits normalization is handled by physical-geometry scaling instead.
- Paraxial alternative (`uLensModel=1`): `displacement = −normalXY·|normalXY|·baseHeight·(n−1)` — `:378-388`. Comment documents measured rim ratios (0.6 start / 0.76 fold / 1.9:1 inverted band) from native screenshots (`:381-387`).
- Rim reach clamp: displacement length capped to half the matte's shorter side — `:408-422` (prevents the rim sampling outside the glass and splitting text into rainbow noise).

**Fact: chromatic aberration = per-channel displacement scaling of the same vector.** `redOffset = displacement·(1 + 0.5·strength)`, `blueOffset = displacement·(1 − 0.5·strength)`, three bilinear samples recombined — `liquid_glass_render.frag:560-575`. Skipped below 0.01 (`:552-557`).

**Fact: rim/fresnel is computed from `normalZ` × an edge factor from height, plus an optional ambient rim ring.** `rimBase = (1 − normalZ)·edgeFactor`, `fresnel = rimBase·0.12·uEdgeConfig.y + ring·0.45` — `liquid_glass_render.frag:866-869`; `edgeFactor = 1 − smoothstep(0, edgeThreshold, normalizedHeight)` with DPR-scaled threshold `:692-702`; ring window scaled by `uEdgeConfig.z = dpr/3` (`:860-865`; Dart `liquid_glass_render_object.dart:519-527`, `layer.dart:531`). Directional specular lobe `totalInfluence^1.5 · lightIntensity · 3` with Reinhard soft-clamp `x/(1+x)` — `:770-786`.

**Fact: meniscus = physically-shaped absorption along the rim.** Hemisphere profile `r_rim = 1 − normalizedHeight`, `rimThickness = 1 − sqrt(1 − r_rim²)`; light-modulated scale `dirScale = mix(1.4, 0.6, litness)`; `finalColor *= 1 − sqrt(rimThickness)·uEdgeConfig.w·dirScale` — `liquid_glass_render.frag:727-741`. The comment block `:704-725` explains each factor and states that chromatic weighting at the rim emerges automatically from `normalXY` magnitude.

**Fact: frost is "computed from" the interleaved backdrop rows: cloud = odd-row samples, ghost = luminance-weighted gaussian over even rows, then `mix(max/min(ghost, cloud − clamp), cloud, frostOpacity)`** — `liquid_glass_render.frag:234-307`; Dart-side opacity incl. 2-pt ramp `liquid_glass_render_object.dart:509-515`.

**Fact: body colour is a luminosity-preserving tint** (`glassColor.rgb + (backdropLuma − glassLuma)`, chroma-gated mix) — `shaders/render.glsl:66-99`; content-adaptive strength `mix(1.2, 0.8, backdropLuma)` modulates saturation and tint weight — `liquid_glass_render.frag:617-644`. Luma weights Rec.709 `render.glsl:22`.

**Fact: backdrop samples are manually bilinear-filtered (4 nearest fetches + mixes)** because Impeller binds the implicit `BackdropFilterLayer` sampler Nearest — `liquid_glass_render.frag:153-215`; tracked Flutter issues #139887/#188365 cited at `:159-161`. The *geometry* sampler is deliberately excluded (`:173-177`). The geometry sampler is explicitly bound with `FilterQuality.medium` — `liquid_glass_render_object.dart:590-595`, `:844`.

**Fact: premultiplied backdrop is un-premultiplied before chromatic math** — `liquid_glass_render.frag:578-588`.

**Fact: touch specular glint lives in the render shader.** `x⁶` lobe of `dot(rimNormal, touchDir)` × distance falloff radius `max(70·dpr, uSize.y·1.5)`, scaled by `uTouchIntensity`, Reinhard-compressed — `liquid_glass_render.frag:798-834`; uniforms written `liquid_glass_render_object.dart:673-682`.

**Fact: capture mode is a direct `canvas.drawRect` with the shader paint — no `BackdropFilterLayer`.** — `liquid_glass_render_object.dart:859-866` (`..drawRect(clipRect.shift(offset), Paint()..shader = renderShader!)`), blur+contents ordering `:849-856, 868`.

**Doc cross-check:** `docs/ARCHITECTURE.md:38-40` describes geometry→render two-stage + progressive blur; matches code. Its quality-tier list is stale — see §8.

---

## 4. Edge and corner geometry

**Fact: rrect SDF** — `sdfRRect(p,b,r) = min(max(q.x,q.y),0) + length(max(q,0)) − r`, with `r` clamped to the half-minor — `shaders/sdf.glsl:68-73`.

**Fact: squircle SDF = analytic Lamé curve (Ln norm), matching Flutter's `RoundedSuperellipseBorder`.** `squircleShape`: corner term `M·(‖m/M‖_n)` with `fastPow = exp2(n·log2(x))` — `sdf.glsl:103-111`; exponent derived from the *clamped* zone: `zone = min(r·1.528, boxShort)`, `n = −1/log2(clamp(1 − 0.29289·r/zone, 0.5, 0.9999))` — `sdf.glsl:131-138`. Consequence documented at `:121-130`: full pill clamp → n=2 (pure circle), so lighting matches a Flutter clip already forced circular. Verified the formula: r≥zone ⇒ base=0.70713 ⇒ n=2 exactly.

**Fact: asymmetric top/bottom corner radii are supported** (iOS sheet bottom corners following device chassis): per-shape slot `[base+6]` (`sdf.glsl:63`), Dart fills it (`blend_group.dart:487-493`; rationale `ShapeGeometry.rawBottomCornerRadius` `render_liquid_glass_geometry.dart:572-589`); consumed by `sdfRRectAsym`/`sdfSquircleAsym` which mix the two evaluations along `smoothstep(−2,2,p.y)` — `sdf.glsl:76-89, 141-166`. Comment `:79-88` explains why a per-quadrant branch would produce garbage normals (gradient discontinuity at centerline); symmetric case is bit-identical to the plain call (`:87-88`, `:94-99`).

**Fact: smoothUnion is a branchless polynomial blend**: `e = max(k − |d1−d2|, 0); min(d1,d2) − e²/4/k` — `sdf.glsl:171-175`; k=0 degenerates to exact `min` (`:170`).

**Fact: the blend across a scene of n shapes is symmetric bidirectional: `mix(fwd, bwd, 0.5)` where fwd = left-fold in index order, bwd = left-fold reversed.** — `sdf.glsl:241-260` (design note) and `sceneSDF` blocks `:263-...`; concrete code `:277-284` (n=2 both folds), e.g. n=16 `:472-490`. **The ADR-style claim in `sdf.glsl:9-11` ("Replaced for-loop sceneSDF with symmetric bidirectional blend") and `:35-40` ("backward pass correctness") is verified in code.** `smoothUnion` is commutative for n=2, so pairs see no change (`:257-258`).

**Fact: sceneSDF is fully unrolled for Windows/SkSL/glslang, whose GLSL ES-1.0 rule forces constant array indices.** Each shape has a named accessor with literal offsets: `#define SDF_SHAPE_N(BASE)` reading `uShapeData[BASE]…[BASE+6]` (`:193-201`), instantiated `sdf0()…sdf7()` at `:206-213` and `sdf8()…sdf15()` at `:215-222`; `MAX_SHAPES 16` (`:54`). The failure mode is quoted at `:26-33` ("error: index expression must be constant"). **Verified as described** — and see §8 for the one correction (the doc's #ifdef claim doesn't exist; the loop is simply gone).
- **Fact: a compile-time 8-shape cap exists for OpenGL ES/ANGLE** (`LGR_OPENGLES_CAP_SHAPES`): shapes 9-16 compiled out, early return after n=8 — `sdf.glsl:214-223, 330-335`; defined for `IMPELLER_TARGET_OPENGLES` in `shaders/gles_compat.glsl:60-65` (Intel Arc ANGLE / PowerVR AST-node explosion reason). Dart `maxShapesPerLayer = 16` still enforces 16 (`blend_group.dart:41-42`, throw at `:472-477`) — so on GLES paths 9-16 silently evaluate to `min` of the first 8 folds (code observation; no Dart-side clamp found).

**Fact: anti-aliasing is shader-side in the matte.** AA uses the *normalized* distance: `foregroundAlpha = smoothstep(smoothing·0.5, −smoothing·0.5, sdN)`, `smoothing = 1.5·max(1, uDpr)` px — 1.5 logical px on all densities — `liquid_glass_geometry_blended.frag:90-107`. Rationale comments `:84-94` (raw pseudo-SDF crosses zero too fast → stair-stepping; DPR-scaled window survives 4% press resampling).
- **Fact: the "native edge" variant narrows the AA window to half a logical px (floor 1 physical px) and offsets the field by `max(1, uDpr/3)`, because the iOS-27-style outline/frost must reach an opaque pixel at the boundary; the matte bounds grow to pay for it.** — `liquid_glass_geometry_blended.frag:97-106`; `uNativeEdge` set when `rimShade>0 || frost>0` — `blend_group.dart:456-463`; bounds inflation `(layerBounds ?? Rect.zero).inflate(blend·.25 + 3/devicePixelRatio)` — `blend_group.dart:528-530`.
- **Fact: Dart-side AA is secondary.** `clipBehavior` defaults `Clip.hardEdge` on the shape (`liquid_glass.dart:47, 62, 81`; the doc comment at `:118-123` still says "Defaults to `Clip.none`" — stale, constructor shows hardEdge). Blur/frost layers clip via `ClipPathLayer` of the union of shape paths (`liquid_glass_layer.dart:814-826, 897-946`); render layer clips a rect (`:1006-1019`). Snap-to-physical-pixel helper — `lib/src/engine/snap_rect_to_pixels.dart:17-33`, used for matte bounds (`render_object.dart:461-470, 925`) and uniform rects (`:625-631`).
- **Fact: hairline edge sampling** — inside the hairline band the shader samples *outside* the shape (untouched by frost): `outward = normalize(normalXY)·2.5·dprScale; displacement = mix(displacement, outward, hairline)` — `liquid_glass_render.frag:433-456`; hairline darkening as linear burn with cos²·1.75 falloff along the light axis `:896-906`.

---

## 5. Adaptive quality

**Fact: three runtime tiers exist: `standard`, `premium`, `minimal`.** — `lib/types/glass_quality.dart:23,44,77`. (See mismatch note below about docs.)

**Fact: Phase 1 static probe forces `minimal` when `ImageFilter.isShaderFilterSupported == false`, and caps to `standard` on web/Windows/Linux before any timing measurement.** — `lib/utils/glass_quality_adapter.dart:539-561`.

**Fact: Phase 2 warm-up: collects 180 raster durations (skipping the first 90 frames), takes P75; `P75 < 20 ms → premium ceiling`, `20–28 ms → standard`, `> 28 ms → minimal`.** — constants `:264-277` (`skipInitialFrames=90`, `warmupFrames=180`), decision `:576-601`, configurable thresholds defaulting to `20.0/28.0` (`:112-113`). `docs/ADAPTIVE_QUALITY.md:11-16` prints the same table and flags the middle band as "provisional — no real-device data" and the premium band "based on 1 report" — i.e. the *doc itself* says the thresholds are uncalibrated; treat numbers as defaults, not validated facts.

**Fact: Phase 3 hysteresis: P95 over non-overlapping 120-frame windows; downgrade after 2 consecutive windows with `P95 > targetFrameMs × 1.5`; upgrade after 10 consecutive windows with `P95 < targetFrameMs × 0.6` (only if `allowStepUp`); 8 s cooldown between changes; a neutral window decays the under-budget counter instead of resetting it.** — windows `:280` (`windowSize=120`), counters `:364-365`, thresholds `:655-690` (`p95Ms > budget*1.5` / `< budget*0.6`), counters `:692-699`, cooldown `:322` + `:750-756`, neutral-decay `:678-689` (comment explains the old one-way-ratchet bug). Degradation-vs-recovery 5× asymmetry: `docs/ADAPTIVE_QUALITY`/`glass_adaptive_scope.dart:26-30`; matches 2 vs 10 windows.

**Fact: what gets turned off FIRST with `frostStep: true` is the frost, before any tier change.** `_tryStepDown`: if premium and frost enabled → `_setFrostEnabled(false)` and return; only the *next* step-down leaves premium. On the way up, standard → premium-without-frost → premium-with-frost. — `glass_quality_adapter.dart:701-745` (`frostStep && premium && _frostEnabled` branch at `:707`; re-enable path `:726-743`; comment "Back to premium without the frost first" `:742`).

**Fact: a denied frost degrades TO a plain blur at a quarter of the frost sigma.** `GlassFrostBudget.blurPerFrost = 0.25`; `apply()` returns `settings.copyWith(frost: 0, blur: max(settings.blur, settings.frost * 0.25))`. — `lib/src/renderer/glass_frost_budget.dart:29-33, 44-55`. Header comment `:4-9`: frost costs one backdrop pass per surface (+1 more for `frostWeight`) and the widest shader reads.

**Fact: the frost budget reaches premium glass through an InheritedWidget; `frostEnabledOf` defaults true with no scope.** — `glass_frost_budget.dart:36-41`; wired by `GlassAdaptiveScope` (`lib/widgets/shared/glass_adaptive_scope.dart:106-113` field, `:238` diagnostic equality).

**Fact: session-level caching of the settled tier** — Phase 2 result stored in a static (`_sessionSettledQuality`, `glass_quality_adapter.dart:216-232`); on remount the adapter skips Phase 2 and applies cache (`:474-491`); `initialQuality` param lets apps persist across launches (`:100-107`); reset on app resume re-benchmarks (`:496-520`).

**FACT/CODE MISMATCH (docs):** `docs/ARCHITECTURE.md:63-65` lists a fourth tier `GlassQuality.economy`; **no `economy` member exists** in `lib/types/glass_quality.dart` and grep finds no use in `lib/`. `docs/PLATFORM_SUPPORT.md:94-99` P75 table matches code defaults. `docs/ADAPTIVE_QUALITY.md:8-9` itself says thresholds are based on ~1 report — unvalidated by the package's own admission.

**Fact: what the *runtime* actually turns off first, in cost order (from code structure, not docs):** frost rows (one full backdrop pass + up to 45 shader taps) → `frostWeight` second pass → blur pass (merged into the frost ghost when σ ≤ 2.4 px) → chromatic/refraction shader (premium→standard) → everything shader (minimal = BackdropFilter only). Supported by: ghost merge `liquid_glass_layer.dart:786-794`; weight second layer `:905-915`; tier semantics `glass_quality.dart` doc blocks. Quantified: frost σ→blur×0.25; blur cutoff 2.4·dpr px.

---

## 6. Animation

**Fact: morph spring — open `SpringDescription(mass 1.0, stiffness 120, damping 16)` (ω₀≈11, ζ≈0.73, underdamped), close `(mass 1.0, stiffness 200, damping 21)` (ζ≈0.74, ≈240 ms).** — `lib/utils/liquid_morph_physics.dart:194-215`; comments `:185-192, 206-211`. (Doc `docs/LIQUID_MORPH_ENGINE.md:100` says "closeVelocityHint −2.5" — code says −3.2, `:222`; another small doc drift, listed under mismatches.)
- Speed presets preserve ζ≈0.73: slow (60/11.3), fast (300/25.1 close, 200/20.5 open), instant (500/32.4); reduced-motion forces instant. — `lib/utils/glass_morph_controller.dart:305-337`; spring chosen by `MorphSpeed` + `_isClosing` at `:310-336`, applied via `SpringSimulation(…, snapToEnd: true, tolerance: Tolerance(distance 0.001, velocity 0.05))` — `:340-355` (comment documents that the default tolerance kept it ticking ~0.5 s past visible rest).

**Fact: the morph position curve is a custom back-out J-curve with amplitude 2.5** (`(t−1)·t·((A+1)t+A)+1`), **size curve `Curves.linearToEaseOut` for small morphs, `smootherstep t³(6t²−15t+10)` for scaleDelta > 3, and `pow(t,1.4)` ease-in contraction on close.** — `liquid_morph_physics.dart:224-231, 448-476, 529-538` (`_BackOutCurve` `:556-570`).
- **Fact: adaptive large-travel damping:** back-out amplitude scaled `2.5·100/|dy|` clamped [0.7, 2.5] when |dy| > 100 px; blend attenuated `3/scaleDelta` (floor 0.2) for scaleDelta > 3; trigger-catch push capped via 80 px travel normalization. — `:236-259, 264-277, 424-434, 481-497` (`computeAdaptiveBackOutAmplitude` `:293-305`, `computeBlendAttenuation` `:318-323`).
- **Fact: anchor blob shrinks over the first 40 % of open** (`anchorScale = 1 − value/0.4`); on close it reforms from 0.85→0.45 so the returning droplet can bridge; **metaball blend = |pathT − sizeT|·150 clamped to 28** (open) or `easeOut(1 − value/0.6)·28·attenuation` (close re-merge). — `:246-277, 456-509`. `_maxBlend 28` matches the shader's SDF blend units (`blend` uniform fed to geometry shader, `blend_group.dart:456`).
- **Fact: consumer mapping: Blob A ghost + Blob B droplet driven from `computeState()` each frame** — `lib/widgets/overlays/shared/glass_menu_internal.dart:1443, 1909, 2309` (submenu morph curve = `easeOutCubic`); `glass_popover_internal.dart:993` (`easeInExpo` size). Morph `hasHandedOff` latch on zero-cross — `glass_morph_controller.dart:361-372`; documented `docs/LIQUID_MORPH_ENGINE.md:133-151`.

**Fact: press/stretch springs (LiquidStretch).** With native pressGrowth (17 pt longest side, `:203`): press `GlassSpring.snappy(duration 250 ms, extraBounce 0.1)`, release `GlassSpring.bouncy(duration 280 ms, extraBounce 0.15)` — `lib/src/engine/stretch.dart:256-265`. Without it, critically damped `smooth(300 ms)` (`:255`). Anchor-drag: while released `bouncy(extraBounce: settings.bounciness)`; during press `interactive(≈150 ms, bounce 0.14)` or `smooth(150 ms)` when bounciness 0 — `:271-281`. Defaults intensity 0.5, squash 0.3, translationDamping 0.15, bounciness 0.15 (`:86-92`); measured native envelope comment `:245-252`; `nativeTremor` preset (0.1/0.1/0.1/0) `:127-132`.
- **Fact: `GlassSpring` presets are duration+bounce based**: bouncy/snappy/smooth/interactive all `withDurationAndBounce` defaults 500 ms bounce .3/.15/0/.14 at 150 ms for interactive. — `lib/utils/glass_spring.dart:30-68`.

**Fact: touch glow + touch specular.** `GlassGlowLayer` offset spring = `smooth(1 s)`, alpha/radius = `smooth()`; on drag they hot-swap to `interactive()` — `lib/src/engine/glass_glow.dart:281-300, 383-391, 431-438`. The intensity flows `ValueNotifier → _TouchSpecularBridge → render object.setTouchSpecular → uTouchIntensity` (no setState) — `liquid_glass_layer.dart:296-341` + `liquid_glass_render_object.dart:243-252`.

**Fact: popover blur ramp.** `blurRampDuration = 260 ms`, `blurRampCurve = Curves.easeOut`; effective blur = `settings.blur × curve(t)` over the opening morph (rationale: gaussian backdrop blur cost ∝ area×σ during the heaviest frames) — `lib/widgets/overlays/glass_popover.dart:127-128`, doc `docs/POPOVER_BLUR_RAMP.md:8-11, 27-33`.

**Fact: other style-defining curves (representative, read in lib/):** sheet modal spring `mass 1, stiffness 220, damping 30` — `lib/widgets/overlays/shared/glass_modal_sheet_state.dart:247` and `glass_modal_sheet_morph.dart:1085`; search-bar pill spring `stiffness 350, damping 30` — `lib/src/widgets/surfaces/tab_bar_searchable_layout.dart:135`; modal route curve `Curves.easeOutQuart` 500 ms — `glass_modal_sheet.dart:399, 579`; indicator settle on pinned nav uses `SpringSimulation(GlassSpring.bouncy(extraBounce: 0.1), 0, 1, 0)` — `lib/widgets/surfaces/shared/glass_nav_pinned_host.dart:231-236`; tab-bar drag release `snappy(350 ms)` — `lib/src/widgets/interactive/scrollable_segment_content.dart:244`. `ProgressiveBlur.maxSigma` is intended to be driven from scroll offset — `docs/PROGRESSIVE_BLUR.md:88-95` (API contract; default 18 `:70-73`).

---

## 7. Caching / invalidation

**Retained GPU objects (all in the layer/render-object, found by reading):**
- geometry matte `ui.Image _geometryImage` — `lib/src/engine/rendering/liquid_glass_render_object.dart:276-300`.
- per-shape `GeometryCache` (Picture before first raster; rendered cache returns itself) — `lib/src/engine/render_liquid_glass_geometry.dart:218-236, 425-437` (comment "rendered caches return themselves"; a `mightNeedUpdate` paint only re-renders the recorded picture once, later rebuilds blit — `:222-231`).
- baked shadow image — `liquid_glass_layer.dart:545-553` fields, `:1022-1046` bake-on-second-same-frame, `:1057-1092` raster at dpr.
- captured backdrop image — `glass_effect.dart:157` + `_lastCaptureSize/_lastCapturePosition` `:158-159`.
- composed filters — `liquid_glass_layer.dart:482-494` (`_cachedBlur/_cachedFrost/_cachedWeight/_cachedSharedFrost`), `lightweight_liquid_glass.dart:712-760` (`_getBlurFilter`).
- frost-row paths — `liquid_glass_render_object.dart:71-75` (`FrostRows` built once per size+scale, shifted by phase; test counter `debugFrostRowsBuilds` `:77-79`).

**Fact: the matte is rebuilt only when the *local* shape changed, not on position/scale moves.** Rasterization records the SDF **without** `matteTransform`, so screen moves/scale only rewrite uniforms at paint time; rebuild trigger = `needsGeometryUpdate || _geometryImage == null || link._dirty || !_sameTransformsAsMatte()` — `liquid_glass_render_object.dart:456-459` (transform list `:301-307`, compare `:348-360`). A group that moves together with the layer needs nothing — `liquid_glass_blend_group.dart:212-222` + `matteMatchesShapeTransforms` `liquid_glass_render_object.dart:322-345`.
- `needsGeometryUpdate` set in `layout()` — `liquid_glass_render_object.dart:341-345`.
- settings that force matte rebuild: `effectiveThickness`, `refractiveIndex`, or (rimShade>0 ‖ frost>0) flipping — `render_liquid_glass_geometry.dart:487-499` (`blend` excluded; it rebuilds through its own setter `blend_group.dart:186-193` — comment `:490-492` documents this).
- DPR change → forced rebuild — `render_liquid_glass_geometry.dart:101-109`.

**Fact: animating shapes rasterize the matte at reduced resolution, then settle.** Consecutive-frame rebuilds are `capped`; cap = matte pixels ≤ 1,048,576 (`_kAnimatingMattePixelBudget`, `liquid_glass_render_object.dart:293-297`), ratio `dpr·sqrt(budget/pixels)` — `:966-976`; one extra post-frame paint is scheduled to restore full res — `:978-992`. Comment `:960-965`: a sheet-sized matte ≈ 8 MB per frame at 3×/120 Hz otherwise.
- Resolution of matte rebuilds is *synchronous* (`toImageSync`) — chosen to kill 1-frame jitter during size animations — `:461-464`, `:913-916`.

**Fact: how blur is not recomputed every frame.** (a) filters cached by sigma (`layer.dart:483-484, 795-802`); (b) the blur/frost `BackdropFilterLayer`s are retained `LayerHandle`s — the engine re-runs them only when the subtree repaints or the pass is dirty; (c) a move without repaint goes through `_updateAtComposite` which rewrites only uniforms + re-adds clips (`layer.dart:711-741`); (d) resting surfaces reuse the matte + baked shadow (`layer.dart:545-553, 1023-1046`); (e) capture ticker self-suspends after 3 stable frames — `lightweight_liquid_glass.dart:255-269, 333-344`. **Honest caveat:** (b)/(c) keep layers *retained*, but on Impeller every retained `BackdropFilterLayer` still renders its pass each frame the scene composites — the package itself says so (`glass_backdrop_group.dart:7-13`: "every BackdropFilter ends the render pass and copies the whole screen"; hence the shared-key group + `backdropKey` wiring `layer.dart:812-815`). The real per-frame savings are the matte/shadow/rows/filter caches and the ghost merge (§5), plus skipping the blur pass when frost absorbs it.

**Stale-pixel comments (explicit, in code):**
- **Fact: the geometry-texture lag rationale.** "Since the asynchronous texture lags 1 frame behind, rendering the old texture using the new origin visually shifted entire group of buttons" → mapping locked to `_geometryLocalBounds` (the bounds the texture was built for), not current paint bounds — `liquid_glass_render_object.dart:614-631`.
- **Fact: local-space rasterization gives "zero async lag"** because screen position is derived synchronously at paint — `:276-279`.
- **Fact: web async capture has an accepted "1-frame delivery lag during a drag"** — `glass_effect.dart:375-377`; sync native path "zero CPU←GPU readback" — `:299-301` (see §1 mismatch).
- **Fact: stale-transform flags reset on re-attach** so a surface's first matte is full-res — `:330-336`; **stale capture-pass rect cleared** `layer.dart:994-996`.
- **Fact: 1-frame async `toImage` ghosting during modal expansion is why `_updateGeometrySync` exists** — `:913-916`.

---

## 8. Platform fallbacks

**Fact: capability gate for all shader paths is `ui.ImageFilter.isShaderFilterSupported`.** Used at: `lib/src/engine/liquid_glass_layer.dart:239` (layer passthrough, no premium), `render_liquid_glass_geometry.dart`/`blend_group.dart:85` (blend group passthrough), `liquid_glass.dart:196` (shape renders child without glass), `glass_effect.dart:97`, `adaptive_glass.dart:127`, `progressive_blur.dart:173, 386`, `glass_quality_adapter.dart:543` (→ minimal), `liquid_glass_settings.dart` (`effective*` getters exist but the gate is external).
- **Fact: web/Windows/Linux are capped to `standard` by the static probe** (`kIsWeb ‖ windows ‖ linux`) — `glass_quality_adapter.dart:549-561`.
- **Fact: Skia/web `ProgressiveBlur` falls back to one uniform `BackdropFilter` at `maxSigma·0.6`** — `lib/widgets/effects/progressive_blur.dart:169-182`.
- **Fact: `platformViewBackdrop: true` forces the live-BackdropFilter fallback regardless of tier** (premium silently renders minimal) — documented `docs/PLATFORM_SUPPORT.md:283-296`, wired in `adaptive_glass.dart:334-336` (`:112-127`); the shader-side companion is passthrough mode: body dropped, rim only, alpha = sampled coverage — `liquid_glass_render.frag:908-940` + `uPlatformViewMode` write `liquid_glass_render_object.dart:530-540`.

**Fact: GLES shape cap** — `LGR_OPENGLES_CAP_SHAPES` cuts the scene to 8 shapes (rationale Intel Arc ANGLE / PowerVR GE8320 AST blowup) — `shaders/gles_compat.glsl:60-65`, `shaders/sdf.glsl:214-223, 330-335`.

**Fact: pre-3.46 GLES Y-flip compatibility is a compile-time decision in one header** — `gles_compat.glsl:40-58` (Flutter PR #186556 shipped in 3.46; `IMPELLER_OPENGLES_UNFLIPPED_DEPRECATED` presence = don't flip); sampling sites: `progressive_blur.frag:55-61`, `liquid_glass_render.frag:329-340`, displacement y-negation at `:459-469` (comment explains the flip inverts refraction if left uncompensated).

**SkSL/glslang constant-index constraint — verify against docs claims (`docs/PLATFORM_SUPPORT.md:183-196`, the "six rules"):**
1. *No dynamic array indices* — **VERIFIED as implemented**: `SDF_SHAPE_N(BASE)` macro with literal `BASE+0…BASE+6` offsets, 16 named functions `sdf0()…sdf15()` at `sdf.glsl:193-222`; the runtime `getShapeSDFFromArray()` is gone (`sdf.glsl:31-33`); `#version 460 core` still passes this strictness per the docs' validation script.
2. *No `dFdx/dFdy` on scalar float, guarded with `#ifdef IMPELLER_TARGET_METAL` in `liquid_glass_geometry_blended.frag`* — **FALSE as written.** Grep over `shaders/` finds zero `dFdx`/`dFdy` code; the `#ifdef IMPELLER_TARGET_METAL` guard does not exist (only mentioned in a rationale comment `sdf.glsl:253-259`). The *actual* technique is manual central differences: `sceneSDF` at ±stepDist, `stepDist = uDpr/3.0` — `liquid_glass_geometry_blended.frag:71-81`. Same technique, stale guard story. (Rule's *intent* — derivative-of-float portability — is satisfied, but not by that mechanism.)
3. *No `min(int,int)`* — observed: `sceneSDF` returns float-only arithmetic; no int mins in the hot path.
4. *No non-constant global initializers* — **VERIFIED**: unpack done inside `main()` with the rule cited inline — `liquid_glass_geometry_blended.frag:50-60`, `liquid_glass_render.frag:311-319` ("Same fix as 0.7.10 geometry shader").
5. *Loop bounds compile-time constants* — **VERIFIED via unrolling**: `sceneSDF` has no for-loops (`sdf.glsl:263-490`); progressive blur uses `const int kHalf = 24` bounds (`progressive_blur.frag:42, 96`).
6. *No array-by-value parameters* — **VERIFIED**: `uShapeData` is a file-global read by macro (`sdf.glsl:66-70`, comment `:51-53` and `liquid_glass_geometry_blended.frag:42-43`: "rejected by glslang on Windows/Vulkan").

**FACT/CODE MISMATCH: `liquid_glass_render.frag:12` claims "Uniform layout migrated to explicit layout(location) slots for Impeller."** Only `uSize`…(two) `layout(location)` lines exist in that file (`grep -c layout(location) == 2`, the rest are plain `uniform`); slot numbers are documented in comments (`:66-143`) and enforced by Dart write order (`setFloatUniforms(initialIndex: …)` — `liquid_glass_render_object.dart:508-596`). The geometry shader does use explicit locations (`liquid_glass_geometry_blended.frag:31-46`). Trust the comment tables + Dart indices, not the "migrated" one-liner.

**Doc-vs-code stale items collected (trust code):**
- `docs/ARCHITECTURE.md:65` `GlassQuality.economy` — **not in code** (§5).
- `docs/ARCHITECTURE.md:50` "Impeller Picture Caching" is accurate but incomplete: it also says resting premium glass reads live backdrop — matches `layer.dart:999-1021`.
- `liquid_glass.dart:118-123` `clipBehavior` doc "Defaults to `Clip.none`" vs constructor default `Clip.hardEdge` (`:47`).
- `docs/LIQUID_MORPH_ENGINE.md:100` closeVelocityHint −2.5 vs code −3.2 (`liquid_morph_physics.dart:222`).
- `shaders/liquid_glass_geometry_blended.frag:11` / `displacement_encoding.glsl:24` "dFdx/dFdy" vs manual finite differences (§3).
- `lib/widgets/effects/progressive_blur.dart:49` + `docs/PROGRESSIVE_BLUR.md` "importance-sampled" vs stride-3σ sampling (§2).
- `lightweight_liquid_glass.dart:318-321` vs `glass_effect.dart:299-301` — contradictory `toImageSync` race claims (§1).
- `docs/PLATFORM_SUPPORT.md:205-210` (iOS "not scrollable") is *policy* not mechanism; code does support scrolling via pass-rect rewrites (`render_object.dart:110-160`), but premium scroll capture is indeed not scroll-position-aware on the capture path (capture is by GlobalKey boundary; comment `:219-224` doesn't claim otherwise) — reported as guidance.

**Not fully clear / honest uncertainty:**
- Which exact engine tier `standard` maps to for *which* widgets (AdaptiveGlass vs LightweightLiquidGlass use slightly different shader selection; the mapping is spread over `adaptive_glass.dart:234-336`, `glass_effect.dart:454-541`). I read the fallback branches but did not exhaustively trace every widget's tier→shader table.
- `interactive_indicator.frag` (used by `GlassEffect` standard refraction, `glass_effect.dart:129-131`) — I read its sampler/uv comments (`:51, 77, 93`) but not the full 21 KB body; its bilinear-on-NN rationale matches `textureBilinear` in the render shader. Claims about it are limited to lines cited.
- Whether `LGR_OPENGLES_CAP_SHAPES` ever triggers on Metal/Vulkan: grep shows it's defined only under `IMPELLER_TARGET_OPENGLES` (`gles_compat.glsl:60-64`); Dart still allows 16 shapes on those paths (no platform-specific clamp found — potential silent truncation, observed not asserted).
- Impeller pass-retention semantics for `backdropKey` sharing are asserted from the package's comments (`glass_backdrop_group.dart:7-24`); I did not read the engine.

---

## What this implies for a platform-independent blur/effects API

Contract-level observations; design for a CPU/GPU-portable effects API, not for Flutter:

1. **Acquisition must be an explicit input, not ambient.** The library needed *two* coordinate systems (live compositor pass rect vs captured image origin) and a `uCaptureOffset` to reconcile them — `liquid_glass_render.frag:78-95`, `render_object.dart:712, 737`. A portable API should take an explicit source rect (in one declared space) + an explicit output clip, and never assume "the backdrop is the screen."
2. **Blur must declare its own working band and spread mode.** Every blur here fails loudly at bounds if the API hides this: progressive blur zeroes out-of-bounds taps and renormalizes by dynamic weight sum (`progressive_blur.frag:99-110`); engine blur is told to `TileMode.mirror` (`layer.dart:798, 861`); shadow blur uses `decal` (`layer.dart:1132`); render sampler is clamped explicitly (`render.frag:348`). Therefore: `spread = decal | clamp | mirror | wrap`, plus an explicit input rect, is required; normalization rule must be part of the contract (drop-and-renormalize vs smear-edge).
3. **Separability is the portability answer to quality-vs-cost.** O(σ) two-pass separable (progressive blur) and O(1) in-shader ghost capped at σ≤2.4px with a 45→6 tap degradation path (`layer.dart:791-794`, `render.frag:251-296`) are both expressible as "per-pass working band + tap budget" — an API should expose `radius, stride/quality` or `taps` explicitly rather than a magic sigma.
4. **Geometry must be precomputed into a texture with a documented channel contract.** The V1 layout (`R=nx, G=ny, B=height, A=coverage`, `displacement_encoding.glsl:17-21`) decouples a once-per-shape-change SDF pass from a per-frame lighting pass — that split is the platform-agnostic structure (CPU rasterizers included). It also shows the trap: storing derived quantities (displacement) instead of the primary field (normal) breaks lighting in blend necks (`:23-26`). Encode the field; derive per-frame.
5. **Distance fields need a metric guarantee before AA or lighting can use them.** Two hard lessons: normalize pseudo-SDF by gradient magnitude (`geometry_blended.frag:84-88`) and derive normals by central differences with a DPR-scaled step (`:67-74`) — otherwise edges stair-step and normals band. An API should own the field, the step, and the AA width (here 1.5 logical px, `:95`) as separate explicit knobs; coverage (`A`) must survive resampling of the matte (`render_object.dart:995-1006` filterQuality note; `geometry_blended.frag:131-142` keeps the outer half-ramp).
6. **Blending of shapes = associative fold; direction matters.** The symmetric bidirectional `mix(fwd, bwd, 0.5)` exists because a single left-fold biases the neck (`sdf.glsl:35-40, 241-260`). Any portable blend API should define the fold order (or mandate symmetric averaging) rather than "min with a smooth k."
7. **Uniform-bounded parameters are a portability design point.** 7 floats/shape with literal indices, MAX_SHAPES 16 (uniform-buffer limit), GLES compile-time 8-shape cap (`sdf.glsl:54, 193-222`; `gles_compat.glsl:60-65`): keep effect parameters in fixed-stride uniform blocks and *unroll at the API boundary* (spec per-shape list, platform decides unroll) — and document silent truncation (Dart allows 16 on GLES; shader folds to 8).
8. **Coordinate spaces need to be named in the contract.** This code juggles screen-logical, pass-relative physical, surface-local, and capture-image spaces; bugs and frozen-UV hacks all came from mixing them (`layer.dart:624-700`, `render_object.dart:614-683`). A portable API should define exactly one canonical space (device-pixel, y-down) and provide explicit transform inputs; any "freeze on ancestor scale" behavior becomes `source_transform` override + invalidation callback.
9. **Cost control belongs in the API contract.** Tier table by measurable budgets (P75 warmup, P95 window, explicit degrade order frost→blur×0.25→shader-off: `glass_quality_adapter.dart:576-601`, `glass_frost_budget.dart:29-33`), explicit cache invalidation keys (shape-local raster, transform list, thickness/n/edge-flag rebuild: `render_object.dart:456-459`, `render_liquid_glass_geometry.dart:487-499`), retained-pass dedup (shared backdrop read: `glass_backdrop_group.dart:7-24`), and an explicit settle/quality budget for animating frames (1 Mpx matte cap: `render_object.dart:293-297, 966-976`).
10. **Y-up/Y-down and premultiply are platform facts, not details.** The entire `gles_compat.glsl` header + flip/displacement-negation gates (§8) show that sampling-convention differences must be absorbed by the *API layer*, with versioned detection; shaders should receive a single pre-normalized texture convention. Premultiplied inputs must be declared, because chromatic math corrupts if not un-premultiplied (`render.frag:578-588`).
