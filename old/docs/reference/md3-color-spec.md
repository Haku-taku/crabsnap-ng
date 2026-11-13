# MD3 色彩规范通读（2026-10-28）

来源与许可：

| 内容 | 出处 | 许可 |
|---|---|---|
| 分层结构、key colors 的文字描述、tone 定律、三档对比、surface/neutral 变更 | `m3.material.io` `styles/color/system/*`、`styles/color/roles`（updated 2026-07-23 / 07-27），经 `md3-docs` MCP 读取 | 文档，引用 |
| 全部数字（hue/chroma 规则、tone、ContrastCurve、ToneDeltaPair、解析算法） | `material-foundation/material-color-utilities`（本地 vendored：`/home/ollie/git/noctalia/third_party/material_color_utilities/cpp`） | Apache-2.0 |

**本项目立场**：数字是规范数据，照此建立**我们自己的数据表**；算法按下文描述**自己实现**，
不引入 MCU 依赖、不拷贝其代码。

角色数据文件：[`md3-color-roles.json`](md3-color-roles.json)（49 个角色 + key colors + tone 定律 + 提取规则）。

---

## 1. 分层结构

```
来源色 ARGB
   └─ 模型层  CAM16(hue, chroma) + CIELAB L*（= tone）  ── 统称 HCT
        └─ 调色板层  TonalPalette(hue, chroma)；get(tone) = IntFromHcl(hue, chroma, tone)
             └─ key colors  5 条调色板（+ 静态 error）
                  └─ 角色层  49 个 DynamicColor：palette + tone + ContrastCurve + ToneDeltaPair + background
                       └─ 解析  tone 依对比级别被调整 → palette.get(tone) → ARGB
```

## 2. 模型层：CAM16 × L\*

`Cam` 的字段（正变换的全部中间量）：

```
hue, chroma, j, q, m, s, jstar, astar, bstar
```

- **tone = L\***（CIELAB D65 明度），不是 CAM16 的 J 或 Q。这是 HCT 的核心取舍：
  tone 与相对亮度 Y 直接挂钩，因此与对比度直接挂钩。
- 观看条件（默认）：白点 D65、适应亮度、背景 L\*（**低于 30 的会被抬到 30**）、
  surround、是否折扣光源；系数 `f = 0.8 + surround/10`；RGB↔LMS 用固定 3×3 矩阵。
- 正变换：`CamFromInt(ARGB)`；逆变换有两条路径（见 §3 的求解）。

**tone 定律**（源码注释即设计目的，可直接当验收依据）：

> tone 相差 **40 ⇒ 对比度 ≥ 3.0**（实测：tone 0..60 全部成立）；相差 **50 ⇒ ≥ 4.5** 只在**未到顶亮度**
> 时成立——tone 50 对白是 **4.48**。这是经验法则不是定理，`ui-color-hct` 的测试把两者都钉住了。

对比度数学：`RatioOfYs(y1,y2) = (lighter + 5) / (darker + 5)`；`Y = YFromLstar(tone)`、
`LstarFromY(y)` 互逆；容差 `CONTRAST_RATIO_EPSILON = 0.04`、`LUMINANCE_GAMUT_MAP_TOLERANCE = 0.4`。

## 3. 反解：HCL（hue, chroma, L\*）→ ARGB

`SolveToInt(hue, chroma, lstar)`：

1. `chroma < 1e-4` 或 `lstar` 接近 0/100 → 直接返回该 L\* 的灰（`IntFromLstar`）。
2. 目标亮度 `y = YFromLstar(lstar)`。
3. **`FindResultByJ`**：以 CAM16 的 J 为迭代变量——由 (hue, chroma, J) 反推 CAM16 → LMS →
   线性 RGB，计算该 RGB 的 `fnj`（即实际 Y）；
   - `fnj ≤ 0` → 失败；
   - 第 4 轮或 `|fnj − y| < 0.002` → 若各通道 ≤ 100.01 则取色，否则失败；
   - 否则按 `j ← j − (fnj − y) · j / (2·fnj)` 校正（**牛顿式一维收敛**）。
4. 失败则 **`BisectToLimit`**：在 sRGB **色域边界**上搜满足该 hue 与 Y 的颜色——先用
   `BisectToSegment` 求边界线段，再沿变化轴在预置亮度平面表 `kCriticalPlanes` 上二分 8 轮。
   （早先记作「二分降彩度」不准确：**机制在亮度平面上搜**，彩度只是被动结果。）

这解释了官方文档那句「某些色相在某些明度下无法达到目标彩度，于是彩度会被降低」——
**调色板允许 tone 处的彩度低于请求值**，这不是 bug 而是模型的一部分。

## 4. 调色板层

```
TonalPalette(hue, chroma)                    // 角色实际使用的构造
  get(tone) = IntFromHcl(hue, chroma, tone)  // 只变 tone，hue/chroma 固定
  keyColor  = KeyColor(hue, requestedChroma).create()
```

`KeyColor::create()`：以 **T50 为枢轴**（T50 平均可用彩度最大）在 0..100 上二分，
步长 1、容差 ε=0.01，找「可用最大彩度 ≥ 请求彩度」的 tone，构造一个代表色。
它只用于展示/参照，**角色的取色不经过它**。

## 5. key colors（变体决定数值）

**TonalSpot = Material 3 默认变体**：

| 调色板 | hue | chroma |
|---|---|---|
| primary | 来源 hue | **36** |
| secondary | 来源 hue | **16** |
| tertiary | 来源 hue **+ 60** | **24** |
| neutral | 来源 hue | **6** |
| neutral variant | 来源 hue | **8** |
| error | **固定 hue 25** | **固定 chroma 84**（静态色） |

与官方文档的两处更新一致：neutral chroma「4 → **6**」、浅色默认 surface「tone 99 → **98**」。

对照其他变体（说明「变体只改 hue/chroma，角色层不动」）：

| 变体 | primary C | secondary C | tertiary | neutral C | neutralVariant C |
|---|---|---|---|---|---|
| TonalSpot | 36 | 16 | hue+60, 24 | 6 | 8 |
| Rainbow | 来源色 | 0 | hue+60, 24 | 0 | 0 |

## 6. 角色层：`DynamicColor` 的八个字段

```cpp
DynamicColor(name, palette(scheme), tone(scheme), isBackground,
             background(scheme)?, secondBackground(scheme)?,
             ContrastCurve?{low,normal,medium,high},
             toneDeltaPair(scheme)?{roleA, roleB, delta, polarity, stayTogether})
```

`polarity ∈ { darker, lighter, nearer, farther }`。

解析入口：`argb(scheme) = palette(scheme).get(tone_resolved(scheme))`。

## 7. ContrastCurve：三档对比的取值

`contrast_level ∈ [−1, 1]`，分段线性：

| level | 返回值 |
|---|---|
| ≤ −1 | `low` |
| (−1, 0) | `lerp(low, normal, level+1)` |
| [0, 0.5) | `lerp(normal, medium, level/0.5)` |
| [0.5, 1) | `lerp(medium, high, (level−0.5)/0.5)` |
| ≥ 1 | `high` |

即官方文档的 **standard = 0（normal 档）/ medium = 0.5 / high = 1**；负值是「低于标准」。

角色表里的曲线实例（元组顺序 low, normal, medium, high）：

| 角色 | 曲线 |
|---|---|
| primary / tertiary / error / inverse_primary | `3.0, 4.5, 7.0, 7.0` |
| on_primary / inverse_on_surface | `4.5, 7.0, 11.0, 21.0` |
| on_primary_container | `3.0, 4.5, 7.0, 11.0` |
| primary_container | `1.0, 1.0, 3.0, 4.5` |
| outline | `1.5, 3.0, 4.5, 7.0` |
| outline_variant | `1.0, 1.0, 3.0, 4.5` |

## 8. tone 解析算法（`GetTone`）

前置：`decreasingContrast = contrast_level < 0`。

### 情形 1：带 ToneDeltaPair（成对前景）

1. `nearer / farther` 由 polarity 与主题决定：
   `nearer = a` 当 `polarity==nearer`，或 `lighter && !dark`，或 `darker && dark`；否则交换。
2. `expansionDir = dark ? +1 : −1`（深色主题向更亮方向拉开，浅色向更暗方向拉开）。
3. **第 1 轮**：各自取满足自身曲线所需的最小调整
   `tone_i = 已满足 ? 原 tone : ForegroundTone(bgTone, required_i)`；
   `decreasingContrast` 时**强制**取「刚好满足」的 tone。
4. 若 `(fartherTone − nearerTone) · expansionDir ≥ delta` → 满足，结束。
5. **第 2 轮**：`fartherTone = clamp(nearerTone + delta·expansionDir, 0, 100)`；再判一次。
6. **第 3 轮**：`nearerTone = clamp(fartherTone − delta·expansionDir, 0, 100)`。
7. **避开尴尬区 50..59**：
   - nearer 落入 50..59：整体外推（expansionDir>0 时 nearer=60，否则 nearer=49），farther 随之修正；
   - farther 落入 50..59：`stayTogether` 则两个一起修正（同上），否则只动 farther（→60 或 →49）。
8. 返回本角色对应的那个 tone。

实例：`ToneDeltaPair(primaryContainer, primary, 10.0, nearer, stayTogether=false)`。

### 情形 2：无配对，只对自己负责

1. `answer = tone(scheme)`；**没有 background 的角色不调整**，直接返回。
2. `desired = contrastCurve.get(contrast_level)`；已满足则**不改善**（文档语义：不过调）。
3. 不满足 → `answer = ForegroundTone(bgTone, desired)`。
4. `decreasingContrast` → 强制取最小满足值。
5. **背景色**（`isBackground`）且落在 50..59 → 若 `RatioOfTones(49, bgTone) ≥ desired` 取 49，否则取 60。

### 情形 3：双背景（`secondBackground`，用于 on_\* 落在两个容器之间）

1. `upper = max(bg1, bg2)`、`lower = min(bg1, bg2)`；对两者都满足则直接返回。
2. `lightOption = Lighter(upper, desired)`（不可达为 −1）、`darkOption = Darker(lower, desired)`。
3. `prefersLight = TonePrefersLightForeground(bg1) || TonePrefersLightForeground(bg2)`：
   倾向亮前景时 → `lightOption < 0 ? 100 : lightOption`；
   否则若只有单候选 → 用它；最后 `darkOption < 0 ? 0 : darkOption`。

### `ForegroundTone(bgTone, ratio)`

```
lighterTone = LighterUnsafe(bg, ratio);  darkerTone = DarkerUnsafe(bg, ratio)
lighterRatio / darkerRatio = 实际达到的对比
preferLighter = round(bgTone) < 60
negligible = |lighterRatio − darkerRatio| < 0.1 且两者都 < ratio
倾向亮：lighterRatio ≥ ratio 或 lighterRatio ≥ darkerRatio 或 negligible → lighter，否则 darker
倾向暗：darkerRatio ≥ ratio 或 darkerRatio ≥ lighterRatio → darker，否则 lighter
```

### 前景倾向的三个常数

| 名称 | 定义 |
|---|---|
| `TonePrefersLightForeground(tone)` | `round(tone) < 60` |
| `ToneAllowsLightForeground(tone)` | `round(tone) ≤ 49` |
| `EnableLightForeground(tone)` | 倾向亮但不允许 → 返回 **49.0**，否则原值 |

> **50–59 是「尴尬区」**：太暗不足以放亮前景（需 ≤49），又太亮不足以放暗前景（<60 才倾向亮）。
> 整套算法里多处专门避开它，这是 MD3 实现的关键工程细节。

## 9. 已提取的直接 tone（28 个角色，其余 21 个按对比级别取）

| 角色 | palette | light | dark |
|---|---|---|---|
| primary | primary | **40** | 80 |
| on_primary | primary | **100** | 20 |
| primary_container | primary | 90 | 30 |
| on_primary_container | primary | 30 | 90 |
| tertiary | tertiary | 40 | 80 |
| error | error | 40 | 80 |
| inverse_primary | primary | 80 | 40 |
| inverse_on_surface | neutral | 95 | 20 |
| surface / background | neutral | **98** | **6** |
| on_background | neutral | 10 | 90 |
| outline | neutralVariant | 50 | 60 |
| outline_variant | neutralVariant | 80 | 30 |

`surface_dim` / `surface_bright` / `surface_container*` 五件套按对比级别取 tone
（例如 surface_dim 的 light 侧是 `ContrastCurve(87, 87, 80, 75)`），已在 JSON 的 `toneExpr` 保留原文。

与官方文档的逐条对应：`primary=primary40`、`onPrimary=primary100`、light surface=98、
neutral chroma=6、三档对比已 token 化——**全部复现**。

## 10. 我们的接口设计（无第三方依赖）

```rust
// ui-color-hct
pub struct Cam16 { hue, chroma, j, q, m, s, jstar, astar, bstar: f64 }
pub fn cam_from_argb(argb: u32) -> Cam16
pub fn argb_from_hcl(hue, chroma, lstar: f64) -> u32      // §3：牛顿式 J 迭代 + 二分降彩度
pub struct TonalPalette { hue, chroma: f64 }
impl TonalPalette { pub fn tone(&self, t: f64) -> u32 }
// ui-color-md3
pub struct KeyColors { primary, secondary, tertiary, neutral, neutral_variant }  // §5 数据
pub struct ContrastCurve { low, normal, medium, high }                            // §7
pub struct Role { name, palette, light_tone, dark_tone, curve: Option<_>,
                  background: Option<RoleId>, second_background: Option<RoleId>,
                  delta_pair: Option<DeltaPair>, is_background: bool }
pub struct DeltaPair { a: RoleId, b: RoleId, delta: f64, polarity: Polarity, stay_together: bool }
pub fn resolve(role: &Role, scheme: &Scheme) -> u32     // §8 三情形
pub struct Scheme { variant, is_dark, contrast_level: f64 /* −1..1 */, palettes }
```

**数据与代码分离**：§5/§7/§9 的表来自规范，落成 `md3-color-roles.json` → 编译期生成 Rust 常量；
算法是我们自己的实现。

## 11. 验收集（不需要任何他人 hex 即可自证）

1. **tone 定律**：差 ≥ 40 ⇒ CR ≥ 3.0（全域）；差 ≥ 50 ⇒ ≥ 4.5（顶亮度除外，见 §2 与 §13）。
2. **文档复现**：`primary = primary40`、`onPrimary = primary100`、light surface = neutral98、
   neutral chroma = 6、dark surface = neutral6。
3. **曲线单调**：`contrast_level` 从 −1→1 时，所有含曲线的角色其 `normal/medium/high` 要求递增，
   且实际 CR ≥ 曲线要求。
4. **尴尬区**：任何解析结果都不落在 tone ∈ [50, 60) 的「前景不可用」区间（背景类角色除外，
   它们按 §8 情形 2 第 5 步被推到 49 或 60）。
5. **金标准比对**（第二优先级）：Material Theme Builder 导出的 baseline JSON，与我们的输出比 hex；
   已知缺陷：其 palette 导出缺 tone（issue #322），故只比 scheme 层。

## 12. 尚未读通的部分

| 项 | 状态 |
|---|---|
| `cam16` 的完整常数 | ✅ 已实现并用**公开默认常数自证**：n/aw/nbb/c/fl/fl_root/z 全部对上（`viewing.rs` 测试）|
| `BisectToLimit` 的细节 | ✅ 已读：边界线段 + `kCriticalPlanes` 亮度平面二分 8 轮（见 §3） |
| `blend`（`cam16_mix`、`tone_delta_safe`、`harmonize`） | 未读；动态色**和谐化**需要它 |
| `dynamic_scheme` 里 `contrast_level` 的平台/变体默认 | 未读 |
| 其余 7 个变体的 key color 数值 | 只读了 TonalSpot 与 Rainbow |
| `overrides`（2021/2025 两套角色表的差异、fixed/fixed-dim 的 ToneDeltaPair） | 未逐项核对 |

这些在动手实现时逐条补齐，并回填本文档。

## 13. 实现后的核对与修正（`ui-color-hct`，2026-10-28）

自己实现的 CAM16/HCT 跑通后，有三件事被**实测推翻或修正**：

1. **tone 定律是经验法则**：Δ40 ⇒ ≥ 3.0 在 tone 0..60 全域成立；Δ50 ⇒ ≥ 4.5 **在顶亮度失效**
   （tone 50 对白只有 **4.48**）。因此验收测试按「全域断言 Δ40」+「分段断言 Δ50 并钉住天花板」写。
2. **M3 baseline 的 `#6750A4` 不是 TonalSpot 的产物**：实测 CAM16 为
   `hue 298.98 / chroma 47.86 / tone 40.08`。tone 与文档的 `primary = primary40` **完全吻合**（40.08），
   但彩度是 ~48 而非 TonalSpot 的 36 → **文档 baseline 与「从 seed 生成的方案」是两回事**，
   不可拿 baseline hex 去验收 TonalSpot 生成器（我们一开始就踩了，测试因此变红）。
3. **中性色有彩度地板 ≈ 3**：在本模型的观看条件下，白与灰的 CAM16 彩度不为 0
   （白实测 ≈ 2.9）。这不是 bug，而是白点不在 CAT02 中性轴上的后果；
   调色板与角色层因此**不能拿「彩度 < 1」当中性判据**，测试改用「远低于饱和色」的相对判据。

与参考实现的**有意偏离**（已写进代码注释）：越色域时 MCU 在**亮度平面边界**上搜色，
我们改为沿同一 hue/Y **对彩度二分**（前提：可达彩度沿该方向单调）。意图相同
（取该 hue 在该 tone 的最大可达彩度），实现更短；若将来发现二者结果偏离，回到 MCU 的做法。
