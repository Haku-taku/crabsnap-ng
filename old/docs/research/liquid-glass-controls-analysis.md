# liquid_glass_widgets 控件层剖析：switch / slider / morph / 融合

> 来源：`/home/ollie/git/widget-ref/liquid_glass_widgets/`（v1.15.0，commit `5d28597`）。
> 方法：直接读控件源码与 shader；每条事实标注 `file:line`。注释里的说法不作为事实，
> 与代码冲突处单独标出。
>
> 配套文档：`docs/research/liquid-glass-analysis.md` 讲**引擎渲染层**（背景捕获、pass 结构、
> 降级），本文讲**控件层**——尺寸、动画节奏、jelly 物理、光照参数、morph 与融合。

---

## 0. 为什么我们现在的效果不对（先给结论）

对照参考实现，我们重写的 switch/slider 在五个地方偏离，且都不是"调参能救"的：

| # | 我们的做法 | 参考的做法 | 后果 |
|---|---|---|---|
| 1 | thumb 是 24×24 的**圆** | switch 是 `22 × 35.2` 的**横向胶囊**（宽 = 高×1.6）；slider 是 `24 × 39`（宽 = 高×2.6） | 形状语言完全不同，iOS 开关的"滑片"感消失 |
| 2 | 弹簧驱动一切（pos/jig/press 三组） | switch 是**两条 380 ms 曲线**（位置 easeInOutCubic + thickness 脉冲）；slider 才用弹簧，且弹簧只喂**速度**给 jelly 矩阵 | 节奏不对：参考的"跳"是缓入缓出 + 一次性鼓起-收回，弹簧会拖尾、回弹过头 |
| 3 | 按住时朝中间膨胀 6dp | 按住（thickness 0.45）时：纵向上下各扩 `t×10`、横向加 `t×16`、整体 `scale` 先压扁 8% 再因液态回涨 10%（净 1.02） | 我们的膨胀方向与幅度都不对 |
| 4 | 白色 pill 保留 22% 不透明度 + 放大 rim | 白色 pill 按 `1 - t×1.2` 淡到 **0**，阴影 α 同步归零；玻璃感由折射+边缘光提供 | 我们的 thumb 始终"半实心"，没有真正液态 |
| 5 | Glass 材质用 premium 配方直接喂引擎 | 交互控件默认走 **standard**：`thickness×0.4`、`lightIntensity×0.6`、`ambientRim×0.7`、`edgeAlpha×0.7`、`rimThickness×0.35` | 玻璃过重/过亮，小尺寸上像塑料 |

另外引擎侧还缺三样参考有的东西：**方向光**（135°）、**触摸高光**、**多形状 SDF 融合（morph）**。

---

## 1. 尺寸与布局

### 1.1 GlassSwitch（默认 58×26）

`lib/widgets/interactive/glass_switch.dart:78-79`（`width = 58.0`, `height = 26.0`）

```dart
thumbSize   = height - 4.0                      // 22    :500
thumbWidth  = thumbSize * 1.6                   // 35.2  :501
travel      = width - thumbWidth - 4.0          // 18.8  :353-354
thumbOffset = 2.0 + travel * position           // 左内缩 2 :603
```

- thumb 是**胶囊**：`borderRadius = thumbHeight/2`（`:672`），不是圆。
- 轨道圆角 `height/2`（`:581`），轨道就是整块 58×26。
- 拖动行程只有 **18.8 dp**——因为滑块本身占掉了 35.2，这正是 iOS 开关"十字拖拽"手感的一部分。
- 拖拽判断用「位移 / travel」映射（`:431-434`），不是按行宽。

### 1.2 GlassSlider（默认 trackHeight 4 / thumbRadius 15）

`lib/widgets/interactive/glass_slider.dart:122-123`

```dart
thumbWidth  = thumbRadius * 2.6     // 39   :663
thumbHeight = thumbRadius * 1.6     // 24   :664
boxHeight   = thumbRadius * 2 + 16  // 46   :537
trackWidth  = maxWidth - 2*thumbRadius            // :485
thumbLeft   = thumbRadius + trackWidth*v - thumbRadius  // :486-487, :637
```

- 滑块同样是**横向胶囊**（39×24），不是圆。
- 轨道只有 4dp 高，**无 rim、无 shader**，就是半透明条（`:549-560`）。
- 活动段颜色：浅色 `black@80%`、深色 `white@80%`；未活动段 `black/white@20%`（`:473-480`）。
  这是**库的默认值**（参考自己的 demo 会覆盖成系统蓝 `0xFF007AFF`，`quality_comparison_demo.dart:877`）。
  新仓库取 iOS 27 的系统强调色：`systemBlue` = 浅色 `#0088FF`、深色 `#0091FF`
  （HIG 表 `ui-design/data/hig_data.rs:9`；未活动段保持参考的中性值）。

---

## 2. 动画与物理

### 2.1 switch：两条 380 ms 曲线，不是弹簧

`glass_switch.dart:209-237`

| 控制器 | 时长 | 曲线 | 用途 |
|---|---|---|---|
| `_positionController` | 380 ms | `Curves.easeInOutCubic` | 0↔1 的滑块位置 |
| `_thicknessController` | 380 ms | `TweenSequence`：0→1（**45%**，easeInOutCubic）+ 1→0（**55%**，easeInOutQuad） | 一次"鼓起再落定" |

thickness 的驱动时机（这是手感的全部来源）。表里的数值是**控制器进度**，不是厚度本身：
`_thicknessAnimation` 把进度经 `TweenSequence` 映射成厚度，进度 0.45 正好是序列第一段的终点，
对应厚度 1.0——所以"按住"看起来是满膨胀，而控制器只走到 0.45：

| 事件 | 控制器目标（映射后厚度） | 时长 / 曲线 | 代码 |
|---|---|---|---|
| 按下（tap down） | 0.45 | 120 ms easeOut | `:360-371` |
| 确认拖拽 | 0.45（补到该值） | 80 ms | `:390-414` |
| 点击抬起 | 从当前值走完整序列（0→1→0） | 380 ms | `:373-381` |
| 松手（拖拽结束） | 从当前值继续到 0 | 380 ms | `:478-479` |
| 外部 value 变化（非拖拽） | 先冲到 1，时长 `380 × 剩余行程`，**easeIn** | 与位置同步落地 | `:267-296` |

几何全部由 thickness `t` 推出（`:599-623`）：

```dart
vExpand      = t * 10.0                 // 上下各扩
leadStretch  = t * 16.0                 // 横向总增量
totalWidth   = thumbWidth + leadStretch
totalHeight  = thumbHeight + 2*vExpand
scale        = (1 - t*0.08) * (1 + t*0.1)   // 先压扁 8%，再因液态回涨 10%
thumbLeft    = thumbOffset - anchorOffset
top          = 2.0 - vExpand
```

**锚点决定拉伸方向**（`:605-612`）：

```dart
anchorOffset = _isDragging
    ? leadStretch / 2                    // 拖动：对称拉伸
    : (_isMovingForward ? 0.0 : leadStretch);  // 跳跃：前缘不动，向后长
```

thumb 内部白色物质的位置也随之偏移 `anchorOffset`（`:730-733`），所以视觉上"物质跟着胶囊头走"。

### 2.2 slider：弹簧只驱动 jelly 矩阵

`glass_slider.dart:303-350`

| 控制器 | 参数 | 作用 |
|---|---|---|
| `_scaleController` | 150 ms，`easeOutBack`（反向 `easeInBack`），1.0→**1.35** | 按住时拇指膨胀，带回弹 |
| `_thicknessController` | 300 ms，`easeOut` 正向 / `easeIn` 反向，0→1 | 白色物质淡出/淡入，拖动期间保持 1 |
| `_jellyController` | `GlassSpring.snappy(350ms)` 的单值弹簧，目标 = 归一化位置 | **只用它的 velocity** |

jelly 矩阵（`lib/utils/draggable_indicator_physics.dart:100-139`，slider 传 `maxDistortion 0.6, velocityScale 2`）：

```dart
distortion = clamp(|v| / velocityScale, 0, 1) * maxDistortion
squashX    = 1 - |dirX| * distortion * 0.5
squashY    = 1 - |dirY| * distortion * 0.5
stretchX   = 1 + |dirY| * distortion * 0.3
stretchY   = 1 + |dirX| * distortion * 0.3
scaleX     = squashX * stretchX
scaleY     = squashY * stretchY
```

即：**沿运动方向压扁、垂直方向拉长**，强度由弹簧速度给出——速度大则形变大，停下即回圆。
矩阵以 `Alignment.center` 应用（`:639-641`），所以不改变 thumb 中心。

> 注意：slider 的 thumb 位置**不经弹簧**，直接是 `thumbRadius + trackWidth * value`（`:486-487`）。
> 弹簧只跟踪同一个目标、只贡献速度；位置本身是精确跟手的。

### 2.3 弹簧预设（全局共用）

`lib/utils/glass_spring.dart:26-70`，全部走 `SpringDescription.withDurationAndBounce`：

| 预设 | 时长 | bounce | 用途 |
|---|---|---|---|
| `bouncy` | 500 ms | 0.3 | 强调动作 |
| `snappy` | 500 ms（slider 传 350） | 0.15 | slider jelly |
| `smooth` | 500 ms（glow 用 1 s / 100 ms） | 0 | glow 跟随与淡出 |
| `interactive` | 150 ms | 0.14 | glow 按下 |

### 2.4 触摸光晕（dynamic lighting 的控件侧）

`lib/src/engine/glass_glow.dart:286-300, 341-470`

- 三个弹簧：位置（拖动 100 ms `smooth`，释放 1 s）、alpha（`smooth`）、半径（按下 1→1，释放 1.2）。
- 按下瞬间位置**直接吸附**到触点（不弹簧），避免光晕从原点滑入（`:385-390`）。
- 拖动中有 **3 px 死区**，且位置直接 set（`:432-437`），避免电容噪声抖动。
- 绘制：`BlendMode.plus` 的径向渐变圆，半径 = `glowRadius × 短边 + spread × 短边`，
  可选 `MaskFilter.blur`；裁剪到玻璃形状（`:773-800`）。
- 光晕同时把 `(位置, 强度)` 传给祖先 `LiquidGlassRenderObject.setTouchSpecular`，
  shader 里做边缘高光（见 §4.3）。

---

## 3. 玻璃材质参数（switch / slider 实际用的那一档）

两个控件都 `quality` 默认 `GlassQuality.standard`（`glass_switch.dart:141-142`、
`glass_slider.dart:237-241`），走的不是 premium 的 `liquid_glass_render.frag`，而是
`GlassEffect` → `_InteractiveIndicatorEffect` → `shaders/interactive_indicator.frag`
（`lib/widgets/shared/glass_effect.dart:561-640`）。

### 3.1 控件传入的原始值

| 参数 | switch（浅/深） | slider（浅/深） | 位置 |
|---|---|---|---|
| `glassColor` | `rgba(.88,.88,.90,.12)` / `white@.08` | 同左 | switch `:695-699`，slider `:741-745` |
| `refractiveIndex` | 1.22 / 1.12 | 1.4 / 1.3 | `:700`，`:746` |
| `thickness` | 14 / 10 | 17 / 13 | `:701`，`:747` |
| `lightIntensity` | **0**（standard） / 2.0（premium） | 0 / 1.8 | `:702-704`，`:748-750` |
| `blur` | **0** | **0** | `:705`，`:751` |
| `lightAngle` | `0.75π` = 135° | 同 | `:706`，`:752` |
| `rimThickness` | 0.7 / 0.5 | 0.7 / 0.5 | `:708-710`，`:754-756` |
| `ambientRim` | 0.15 / 0.08 | 0.15 / 0.08 | `:711-713`，`:757-759` |
| `baseAlphaMultiplier` | **0.04 / 0.0** | 0.12 / 0.08 | `:714-716`，`:760-762` |
| `edgeAlphaMultiplier` | 0.28 / 0.15 | 0.28 / 0.15 | `:717-719`，`:763-765` |
| `interactionIntensity` | = thickness `t` | = thickness `t` | `:721`，`:767` |

### 3.2 standard 归一（必须照抄，否则小尺寸上过重）

`glass_effect.dart:551-580`：

```dart
thickness        *= 0.4     // switch: 14 → 5.6；slider: 17 → 6.8
lightIntensity   *= 0.6     // 0 仍是 0
ambientRim       *= 0.7     // 0.15 → 0.105
edgeAlpha        *= 0.7     // 0.28 → 0.196
rimThickness     *= 0.35    // 0.7 → 0.245
```

### 3.3 白色物质与阴影

```dart
materialOpacity = clamp(1.0 - t * 1.2, 0.0, 1.0)          // switch :666, slider :685
thumbShadow α   = (1.0 - t) * materialOpacity              // slider :687
switch 阴影     = 0x33000000, α 0.2*(1-t), offset(0,2), blur 0   // :673-681
slider 阴影     = black@0.15, blur 8, offset(0,2)          // :280-286
```

要点：**t 到 0.83 时白色物质已经完全消失**，之后只剩玻璃。我们保留 22% 保底是错的。

---

## 4. 渲染与光照（`interactive_indicator.frag`）

文件：`shaders/interactive_indicator.frag`（440 行）。坐标全部是**逻辑像素**。

### 4.1 形状与法线

- SDF：圆角矩形（`sdfRRect` 同款），`cornerRadius = totalHeight/2` 即胶囊（`:158-160`）。
- 法线：clamp 到内缩骨架上的最近点，`normalize(p - closest)`（`:175-183`）。
- AA：`smoothing = 1/uScale.x`（`:163`）。

### 4.2 折射 / 色差 / 边缘

```glsl
edgeZone       = 14.0                                     // 逻辑 px
edgeInfluence  = smoothstep(edgeZone, 0, distFromEdge)^2
bendStrength   = 0.9 * (0.5 + interactionIntensity * 0.7) // 0.45 → 1.17
edgeOffset     = normal * edgeInfluence * bendStrength * size.y * 0.35
chromaticShift = normal * edgeInfluence * chromaticAberration * 0.12
```

（`:205-247`）注意：折射幅度按**控件高度**缩放（`size.y * 0.35`），不是固定像素——
所以 22dp 的 switch thumb 边缘位移约 7.7dp，39dp 宽的 slider 约 8.4dp。

### 4.3 方向光（da 光）

`lightDirection = [cos(angle), -sin(angle)]`，`angle = 0.75π` → 光从左上方来。

```glsl
key  = pow(max(dot(n, L), 0), 8)  * lightIntensity * 0.5   // :279-282
kick = pow(max(dot(n,-L), 0), 12) * lightIntensity * 0.5   // :286-290
rimBrightness = ambientRim + key + kick                     // :293
bevelGradient = -n.y * 0.18                                 // :326-328
rimColor      = white * (rimBrightness + bevelGradient*borderMask) * ambientRim*10
finalColor   += rimColor * borderMask * 1.5                 // :346-349
```

`bevelGradient` 模拟 3D 倒角：上缘亮、下缘暗（`kBevelStrength = 0.18`，`:326`）。

### 4.4 菲涅尔 / 内提亮 / 着色

```glsl
fresnel      = (radialDist^2) * 0.25                       // :301
finalColor  += white * fresnel * glowIntensity             // :352
finalColor  += vec3(ambientStrength)                       // :355
finalColor   = mix(finalColor, finalColor + glassColor.rgb*0.2, glassColor.a)  // :358
```

### 4.5 苹果酒边缘吸收（meniscus）

```glsl
lensThickness  = sqrt(1 - (distFromEdge/edgeZone)^2)       // 半球剖面
litness        = dot(normal, L)
dirScale       = mix(1.4, 0.6, litness*0.5+0.5)             // 背光侧更暗
absorption     = 1 - lensThickness * edgeAbsorption * dirScale
finalColor    *= max(0, absorption)
```

（`:396-407`）iOS 26 标定值 `edgeAbsorption ≈ 0.15`（`:62`）。

### 4.6 透明度合成

```glsl
standardBaseAlpha = baseAlphaMultiplier * mix(0.6, 1.0, interactionIntensity)
baseAlpha = hasBackground ? 0.70 : standardBaseAlpha
edgeAlpha = hasBackground ? 0.95 : edgeAlphaMultiplier * mix(0.6, 1.0, t)
glassAlpha = mix(baseAlpha, edgeAlpha, edgeInfluence)
ringOpacity = borderMask * 0.9 * clamp(ambientRim*10, 0, 1)
glassAlpha = max(glassAlpha, ringOpacity)
```

（`:419-436`）**边缘比中心更不透明**，这是小尺寸玻璃"有实体感"的关键：
标准路径下 switch 的 baseAlpha = 0.04×0.6≈0.024，而 edgeAlpha = 0.28×0.6×0.7≈0.118，
再加 ring。

### 4.7 touch specular（premium 路径，1.5.0）

`liquid_glass_render.frag:772-831`：

```glsl
touchRadius = max(70*dpr, size.y*1.5)          // 物理 px
distFactor  = smoothstep(touchRadius, 0, |frag - touch|)
rimTouchDot = max(dot(normal, normalize(frag - touch)), 0)
tSpec       = rimTouchDot^6 * distFactor
tBrightness = tSpec * touchIntensity * lightIntensity * 2.5 / (1 + ...)
```

也就是说：手指附近的那段边缘会被点亮，强度由 GlowLayer 的 alpha 弹簧给出。

---

## 5. 模糊

两条路，互不相同：

1. **普通模糊**：`ImageFilter.blur(sigma)` 作为 `BackdropFilterLayer.filter`
   （`lib/src/engine/liquid_glass_layer.dart:794-812`）。switch/slider 的 `blur = 0`，
   所以 thumb 本身不模糊背景——**它是纯折射，不是磨砂**。
2. **渐进模糊**：`shaders/progressive_blur.frag`（49 taps，`kHalf=24`），
   `sigma = maxSigma * (1-g)^falloff`，`g` 沿方向（上下左右四选一），
   权重用 IIR 乘性递推 `w *= g1; g1 *= g2` 代替每 tap 的 `exp()`；
   分横纵两趟经 `ImageFilter.compose` 组合（`:44-111`）。用于弹层边缘的渐变模糊。

---

## 6. morph（从触发物到菜单/弹层的液滴）

`docs/LIQUID_MORPH_ENGINE.md` + `lib/utils/liquid_morph_physics.dart`（包版本 1.15.0，行号以它为准）。

- 两条弹簧，同一个欠阻尼比：开 `120/16`（`:194`）、关 `200/21`（`:206`）；
  关闭起点注入 `closeVelocityHint = -3.2`（`:217`）放大回弹。
  > 旧版本此处记作 −2.5，已按 1.15.0 源码核实为 −3.2。
- 位置曲线：`_BackOutCurve(amplitude 2.5)`（`:230`），只在**打开**时用

  ```dart
  double transform(double t) { return (t -= 1.0) * t * ((2.5 + 1.0) * t + 2.5) + 1.0; }
  ```

- 状态量（每帧，`compute()` `:360-515`）：
  - `pathT`：开 = backOut(clamp) + 关闭欠冲；关 = clamp + 关闭欠冲（无反向起跳）
  - `sizeT`：开 = `linearToEaseOut`（`scaleDelta > 3` 且 adaptive 时改 smootherstep）；
    关 = `clamp^1.4`（先快速收成液珠）
  - `anchorScale`：开 `1 - t/0.4`；关 `1 - (t-0.45)/0.4`（先长回来，好接住液滴）
  - `blend`：开 `clamp(|pathT-sizeT|*150*atten, 0, 28)`；
    关按**接近程度**拉起 `easeOut(1-t/0.6)*28`（近距再融合，`_closeProximityThreshold`）
  - `containerScale`：`raw>1` 轻微胀（×0.10），`raw<0` 明显挤（×0.55）
  - `pushDx/pushDy`：仅在关闭欠冲（`raw<0`）时推 Blob A，幅度按 `_maxPushTravelPx = 80` 归一
  - 大尺度自适应：`|dy|>100` 时回弹幅度按 `100/|dy|` 衰减（下限 0.7）；
    `scaleDelta>3` 时融合强度按 `3/scaleDelta` 衰减（下限 0.2）

**几何**（`SheetMorphGeometry.blobRect` `:639-712`）：Blob B 的**中心由 `sizeT` 驱动**，
`pathT-sizeT` 的偏差夹在 ±8 px 内（`maxAnchorDrift`），所以液滴是"从触发物里长出来"，
不是"飞出去"；尺寸同样 lerp(trigger, destination, sizeT)，欠冲时按触发物自身挤压。

两个 blob 各自是一个 `LiquidGlass` 形状，**颈由 SDF 融合自动生成**，没有显式几何。

---

## 7. 融合（blend group）——morph 的底层机制

`lib/src/engine/liquid_glass_blend_group.dart`：

- `LiquidGlassBlendGroup(blend: 20.0)`——`blend` ≈ 两个形状开始融合的逻辑像素距离（`:33-43`）。
- 每个形状向 group 注册 `(type, center, size, rTop, rBottom)`，最多 **16 个**（`:49`），
  写入 `uShapeData`（每形状 7 个 float，`shaders/sdf.glsl:42-49`）。
- 几何 shader 对整组求 `sceneSDF`，并把表面法线编码进离屏纹理
  （`liquid_glass_geometry_blended.frag:62-149`）：
  - 法线来自 SDF 梯度（中心差分，步长 `dpr/3`），不是位移
  - 穹顶剖面：`n_cos = clamp((thickness + sd)/thickness, 0, 1)`，`n_sin = sqrt(1-n_cos²)`
  - 高度：`sqrt(thickness² - (thickness+sd)²)`
- 融合公式（`sdf.glsl:172-177`）：

  ```glsl
  float smoothUnion(float d1, float d2, float k) {
      float e = max(k - abs(d1 - d2), 0.0);
      return min(d1, d2) - e * e * 0.25 / max(k, 1e-5);
  }
  ```

- 多形状用**双向折叠再平均**（`fwd` 左→右、`bwd` 右→左，`mix(fwd,bwd,0.5)`），
  消除折叠顺序带来的不对称（`sdf.glsl:31-39, 259-...`）。
- 融合区里法线由合并后的 SDF 梯度给出，所以颈部自带正确折射与光照——
  这就是"融合处有液桥"的来源。

---

## 8. 与我们引擎的映射

| 参考概念 | 我们已有的 | 差距 |
|---|---|---|
| SDF 穹顶 + Snell + reach clamp + 色差 + 亮度保持 tint + 菲涅尔 + ambient ring | `ui-paint-gpu/src/shader.rs` 的 `GLASS_FRAGMENT`（几乎同款） | 算法够用；缺 **方向光**、**bevel 梯度**、**触摸高光**、**meniscus 边缘吸收** |
| `BaseAlpha / EdgeAlpha` 边缘更实 | 无 | 玻璃 alpha 固定由 tint.a 给 |
| `thickness×0.4` 等 standard 归一 | 无 | 我们直接把 premium 值喂进去，小控件过重 |
| lightAngle 135°、key/kick lobe、bevelGradient | 无 | 可先在控件层用 `rim` 色近似，根治要 shader |
| touch specular（手指点亮边缘） | 无 | 需要引擎知道触摸点与强度 |
| 多形状 smoothUnion + geometry pass | `Glass.blend/blend_count/blend_k` + premium 趟的双向折叠 | 上限 4 个形状（参考 16）；不另开几何纹理，改由场梯度求法线 |
| morph 控制器（backOut + 欠阻尼弹簧） | `ui-widgets-glass/src/morph.rs` 全量移植 `compute()` 与 `blobRect()` | 弹层那套 fill 交叉淡入与 `handedBack` 闩锁未移植（演示不需要） |
| progressive blur | 普通高斯（`BackdropBlur`） | 渐变模糊未做 |

---

## 9. 建议的实施顺序

**阶段 A（控件层，不改引擎 shader，收益最大）——已完成**

1. 尺寸/形状：switch 58×26 → thumb 35.2×22 胶囊（照参考）。slider 取参考自己的默认值
   **39×24、轨道 4dp、盒子 46dp**（`thumbRadius` 15 ⇒ `2.6r × 1.6r`、`2r + 16`，
   `glass_slider.dart:122-124, 665-667`）；曾经从 WWDC 画面临时换算出的 33×22 / 5.6 已废弃——
   用画外推值取代参考源码值是错的，见 `docs/lessons/design-errors.md`。
   按下膨胀用参考的 `1.0 → 1.35`（`glass_slider.dart:319-322`）与药丸圆角 `r = h/2`（`:667`）。
2. 动画模型：switch 换成「380 ms 位置曲线 + thickness 脉冲序列」；slider 换成
   「scale 1.35 / 150 ms easeOutBack + thickness 300 ms + jelly 矩阵（速度驱动）」。
3. 几何映射：`vExpand = 10t`、`leadStretch = 16t`、`scale = (1-.08t)(1+.1t)`、
   拖动对称锚点 / 跳跃方向锚点。
4. 白色物质 `1 - 1.2t`（无保底）、阴影 α 同步；轨道渐变 + 光晕阴影；活动色。
   **物质矩形 = 外壳矩形**（参考把 39×24 的物质居中放进缩放后的壳，屏幕上读成「两块」，已改）；
   膨胀与果冻绕当前值所在的**中心**，不照抄参考 `left: thumbPosition − thumbRadius` 的左缘锚定。
5. 参数归一：写进 glass 包的 recipe 时套用 standard 的 ×0.4/×0.7/×0.35。

**阶段 B（引擎：光照）——已完成**

6. ✅ `Glass` 换成参考的整套交互指示器参数（`rim_thickness`、`rim_smoothing`、`ambient_rim` 强度、
   `ambient_strength`、`base_alpha`、`edge_alpha`、`light_direction`、`light_intensity`、
   `saturation`、`glow_intensity`、`interaction`、`chromatic`、`edge_absorption`、`edge_zone`），
   `GLASS_FRAGMENT` 按 `interactive_indicator.frag:113-439` 重写（边缘法线位移 + 色散 +
   合成倒角 + 半月板吸收），旧的穹顶/斯涅尔模型删除。
7. ⬜ 触摸高光（`GlassGlow`）：仍是独立的一层径向光，见 `docs/engine-gaps.md` #28。

**阶段 B2（引擎：premium 透镜）——已完成**

7b. ✅ 参考的两档材质分岔：`GlassQuality.standard` 是各平台的近似，
   `GlassQuality.premium`（`glass_effect.dart:475`，仅 Impeller）才是 iOS 26 原样。
   我们两档都要：`draw::Glass.quality` 选程序，`PREMIUM_GLASS_FRAGMENT` 按
   `liquid_glass_render.frag` 移植（法线/高度 → `refract()` 位移 → 色散 → 保亮度着色 →
   自适应饱和 → 半月板 → 环光 → 菲涅尔）。关键换算与陷阱见 `docs/engine-gaps.md` #30：
   高度是 `cos_i·thickness` 不是 `u·thickness`；边缘带按编码高度取阈值；
   `thickness`/`ambientRim` 以 3 倍屏像素计量，传 `值 × dpr / 3`。

**阶段 C（引擎：融合 / morph）——已完成**

8. ✅ 融合不做独立 geometry pass：形状随材质走（`Glass.blend` + `blend_k`），
   premium 趟里对整组求 smoothUnion（双向折叠平均），覆盖度与法线都取自并集场。
   见 `docs/engine-gaps.md` #26（含 4 形状上限的理由）。
9. ✅ morph 控制器落在控件层（`ui-widgets-glass/src/morph.rs`）：`compute()` 照抄物理，
   `blob_rect()`/`anchor_rect()` 照抄 `SheetMorphGeometry`；关闭要换弹簧并注入 −3.2 初速度，
   为此引擎补了 `spring_hinted()` / `spring_value()`（`ui-core/src/anim.rs`）。

---

## 10. 明确不做 / 存疑

- **不照搬 Flutter 的 `toImageSync` 捕获**：我们的 GPU 渲染器已经能在 pass 内读背景。
- ~~不照搬 `InteractiveIndicator` 的 0.35×height 折射幅度~~ **已推翻**：标准路径跑的
  就是这个指示器着色器，它的折射就是 `normal × influence² × 0.35 × height`；
  自研的"穹顶 + 斯涅尔"模型与参考完全是两种材质，已整体替换（`docs/design-errors` 有记录）。
- 参考在有截图背景时把本体不透明度钉成 0.70、边缘 0.95（`interactive_indicator.frag:419-426`），
  `baseAlphaMultiplier`/`edgeAlphaMultiplier` 只在无背景的合成路径生效；我们的背景层始终存在，
  因此走 0.70/0.95 这一支。
- 参考里 `lightweight_glass.dart:318-321` 与 `glass_effect.dart:299-301` 关于
  `toImageSync` 的说法互相矛盾（见 `liquid-glass-analysis.md`），本文不采信任何一方。
