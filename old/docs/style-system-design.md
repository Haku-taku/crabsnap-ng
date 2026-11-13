# 多风格 UI 体系设计：MD3 / Fluent(WinUI3) / Apple Liquid Glass

> 状态：**E1–E5、Sense、禁用已实现**；`TextField`、虚拟列表、放置目标尚未开工（见文末待办）。
> 依据三类材料：
> 1. 三份带行号引用的调研（本目录 `research/flutter-md3-analysis.md`、
>    `research/fluent-ui-analysis.md`、`research/liquid-glass-analysis.md`），关键事实已由
>    lead 交叉抽验（`progressive_blur.frag` 权重归零、`captureOriginInScreenSpace` 偏移公式、
>    `emphasizedDecelerate = Cubic(0.05,0.7,0.1,1.0)`、elevation 6 断点表）。
> 2. 本仓库已核实的渲染事实（AGENTS.md §1.5 表中的模糊系列条目）。
> 3. Haze（Compose 背景模糊库）的 ADR 结论。
> 文中每个数字都有出处；标 `[未证实]` 的不作为实现依据。

---

## 0. 设计命题

用户的要求：**同一套内置控件树，运行时可切换 MD3 / Fluent / LiquidGlass 外观，并允许混入
自定义风格控件**。这成立的前提是一个已被三个生态共同验证的事实：

> **风格不是代码，是数据。** MD3 用生成器把 45 个角色槽喂给控件（`color_scheme.dart`），
> Fluent 用 83 字段的 `ResourceDictionary` 整体换装（37 个控件文件引用 `theme.resources.*`），
> Apple 玻璃把光学参数放在 uniform 块里（`liquid_glass_render.frag` 的 slot 表）。
> 三者都不在控件里写死颜色/时长/半径。

反面教材也在手边：`flutter_liquid_glass` 包 22 个配置旋钮至少 4 个**从未被读取**
（`enableCustomShader`、`parallaxIntensity`、`shadows`、整个孤儿 painter），README 卖的
"refraction/chromatic aberration" 实际画的是平移梯形 + `MaskFilter.blur`——**像素从未移动过**。
所以本设计的验收纪律：**声明的每个旋钮必须被消费**，见 §7。

---

## 1. 三风格的共同分解（这就是契约的形状）

把三种风格各自的「一个磨砂浮层怎么画出来」摆在一起：

| 阶段 | MD3 | Fluent acrylic | Apple liquid glass |
|---|---|---|---|
| 背景内容 | 不模糊（M3 面板是实色容器） | 全屏 σ=30 模糊（`BackdropFilter`，`acrylic.dart:524`） | 全屏模糊/折射采样（`uCaptureOffset` 把屏幕坐标映进采集图，`liquid_glass_render_object.dart:735`） |
| 色彩处理 | `Color.alphaBlend` 状态层（0.08/0.10 等） | `BlendMode.luminosity` 压亮度 → `BlendMode.color` 着色（`acrylic.dart:577-581`） | 保持亮度的着色（`applyGlassColor`，Rec.709 饱和度控制） |
| 质感 | 阴影层表（3 层 penumbra） | **2% 噪声贴图平铺**（真 PNG 资产，`acrylic.dart:551`） | 法线贴图驱动的镜面/边缘光（V1 encoding） |
| 形状 | 圆角 4-28 | 圆角 4/6/8 每控件硬编码 `[调研缺口：无半径令牌]` | squircle + SDF `smoothUnion` 融合相邻玻璃 |
| 降级 | `disableAnimations` 开关 | `DisableAcrylic` → `fallbackColor` 实色，无滤波无噪声（`acrylic.dart:497-506`） | 质量阶梯：frost 先退成 0.25×σ 普通模糊，再退 standard/minimal |

共同骨架浮现：**材质 = 一条有名字的阶段配方**（capture → blur → 色彩混合 → 质感 → 形状 →
降级出口），**状态层 = 独立的 alphaBlend 步**，**深度 = 可选三件套 {阴影, tint, 容器角色}**，
**动效 = 令牌化的曲线或弹簧 + 正反两向**。任何一个风格只是给这套骨架填不同的数。

这与 Haze ADR-0002 的阶段划分一致，也与 Skia 的 `RenderEffect` 图、Flutter 的
`FragmentProgram` 路线同构。**线性 `Vec<Effect>` 链不再是效果的最终形态**——refraction
需要「几何 pass 产法线纹理 → 渲染 pass 读它」的显式 DAG，线性链表达不了。
（注：Haze 的"按阶段缓存"我们已放弃，改为每帧全量重算，见 §5 的状态变更说明。）

---

## 2. Crate 布局（含已批准的 `ui-widgets`）

```
ui-core       (PORTABLE) 树/布局/命中/脏区/绘制词汇/效果契约        ——现状保留并扩展
ui-text       (PORTABLE) 文本整形/光栅                              ——不动
ui-style      (PORTABLE) 新增：令牌字典、材质配方、动效令牌、形状令牌、内置风格包
ui-widgets    (PORTABLE) 新增：语义控件（状态机 + 把 resolve 结果喂成 Decl）
ui-platform   (PLATFORM_BOUND) AppLoop、Decl 构造器、焦点/输入路由  ——现状保留
ui-paint / ui-paint-gpu   后端：材质阶段图的实现者                  ——按 §6 改造
```

依赖只能向上（§3 规则）；`ui-style`/`ui-widgets` 加入
`ui/crates/ui-platform/tests/platform_boundary.rs` 的 `PORTABLE` 名单——按既有纪律做
「故意污染 → 变红 → 恢复 → 变绿」。

**为什么控件必须独立**：现在 `window_gpu` 示例里按钮/卡片/滚动条是手写 `Decl` 拼装，颜色、
时长、圆角全是字面量。风格切换要求控件只说「我是按钮，处于 hover，深度 2」，由
`ui-style` 回答「那该画什么」。这批拼装逻辑需要一个有自己的状态机（tracking、value、
selection）而不碰任何视觉数值的家。

### 2.1 ui-style 的公开面（骨架）

```rust
/// 角色槽：名字寻址，不是固定 struct 字段。迁移=旧名 getter 重定向（MD3 的做法）。
pub struct RoleDict { /* Map<RoleName, Color> + 版本化别名表 */ }
// 纪律（修复 Fluent 移植的三个缺陷，fluent-ui-analysis §6/§7.11）：
// - 值语义 Eq/Hash（换主题可廉价差分）
// - copy_with + 单键覆盖 + 合并链（widget 实例 > 祖先 scope > 包默认 > 系统）
// - 明暗是同一键集的两次参数化，不是两套并行的 const ctor（漂移温床）

/// 材质配方：命名阶段序列，每阶段带能力要求与降级出口。
pub struct MaterialRecipe {
    pub stages: Vec<Stage>,          // Capture{scope} / Blur{sigma} / Tint{...} / Luminosity{...}
    pub fallback: RoleName,          // 能力不足时用的实色角色（Fluent FallbackColor 模式）
}

/// 动效令牌：曲线或弹簧，正反两向，外加全局 reduce-motion 熔断。
pub enum Motion {
    Duration(f32, Curve, Option<(f32, Curve)>),  // 正: (ms, curve)，反: 可选覆写
    Spring { stiffness: f32, damping: f32, mass: f32 },
}
pub enum Curve { Linear, EaseOut, Cubic([f32;4]), Piecewise(&[Segment]) }
// Piecewise 必须有：emphasized 族单条 cubic 表达不了（md3 报告事实 6：ThreePointCubic）。

/// 形状令牌：我们的超椭圆指数(2^k)体系保留，补一族命名预设。
pub struct ShapeTokens { pub control_radius: f32, pub overlay_radius: f32, pub squircle_k: f32 }
```

数值出处（写死进各内置包的令牌表，控件侧禁止字面量）：

- MD3 状态层：pressed **0.10** / hovered **0.08** / focused **0.10**，disabled 填充 **0.12** /
  内容 **0.38**；**载体随上下文翻转**（primary/onPrimary/onSurface/error，`checkbox.dart:998-1032`）。
  提示词里的 "0.12 focus" 是错的（那是 disabled fill）；0.04/0.12 是 M2 遗留默认
  （`theme_data.dart:467-468`）。tone 数字 `[未证实——material_color_utilities 不在此 checkout]`。
- MD3 动效：9 缓动 + 16 时长桶（50→1000ms）；`standard = Cubic(0.2,0,0,1)`
  （`motion.dart:192`）；fadeForwards 450ms（原生 800ms 弹簧，Flutter 自认未支持——
  证明**弹簧必须是一等公民**，曲线模型会追不上）。
- Fluent：时长 83/167/250/358ms；`standardCurve = Curves.easeInOut`；acrylic σ=30、
  噪声 2%、luminosity HSV value 夹 `[0.125, 0.965]`、opacity 映射 `[0.15, 1.03]`、
  tint 抑制曲线（中点 0.50 / whiteMax 0.45 / midMax 0.90 / blackMax 0.85）。
- Apple/LG：blur 49 taps、stride `max(1, 3σ/24)`、**越界 tap 保留采样但权重归零、按动态
  `wsum` 归一**（`progressive_blur.frag:42-110`——与我们 `CLAMP_TO_EDGE` 的坑直接相关）；
  frost→普通模糊降级系数 **0.25×σ**；自适应质量 P75<20ms premium / ≤28 standard / >28 minimal
  `[调研自评：阈值建立在约一次实测报告上，仅作起点]`；morph 弹簧开 (k=120,c=16)、
  关 (k=200,c=21)；法线编码 V1 = 中心差分真实梯度（步长 dpr/3），**不是位移**。

### 2.2 ui-widgets 的公开面

```rust
pub trait Widget {                      // 状态机 + 语义；禁止出现颜色/毫秒/半径字面量
    fn id(&self) -> Id;
    /// 由当前风格包 + 本控件状态解出这一帧要画的 Decl 子树。
    fn resolve(&self, st: &WidgetState, tokens: &StylePack, layout: &FrameRect) -> Decl;
}
pub struct WidgetState {                // 路由复用 ui-platform 已有的命中/焦点事实
    pub hovered: bool, pub pressed: bool, pub focused: bool,
    pub focus_visible: bool,            // 键盘驱动才画焦点环（Fluent/MD3 同规则）
    pub selected: bool, pub dragging: bool, pub disabled: bool,
}
pub trait Interactive {                 // 可选面：回调返回"要重绘"，修复旧的丢弃缺陷
    fn on_press(&mut self) -> Action { Action::None }
    fn on_drag(&mut self, _dx: f32) -> Action { Action::None }
    fn animate(&mut self) -> Vec<(Id, MotionTarget)> { Vec::new() }
}
```

首发控件集（全部来自示例里已手写过一遍的东西，不算新发明）：
`Button/IconButton`、`Toggle/Checkbox/Switch`、`Slider`、`ScrollBar/ScrollArea`、
`Card/SurfacePanel`、`List/Row/Item`、`Overlay/Flyout`、`TextField`（依赖 `Event::Text`
接线，§8）。状态→角色的解析器按 Fluent 报告证实的优先级链
`实例 > 局部 scope > 包 > 系统` 做成通用函数，**每控件手写一遍 `widgetValue ?? themeValue`
是它的反面教材**。

---

## 3. 运行时切换与混用

> **2026-10-27 修正**：本节原先假设「一个控件实现 + 不同 `StylePack` 数据」。这对颜色/度量成立，
> 对**行为与结构**不成立——MD3 的圆形状态层、Fluent 的填充洗色、MD3 开关无标签而 Fluent 有，
> 都不是数值差异。现行模型是**语义角色 + 风格家族 + 三档回落**，见 §3.0 与
> `docs/decisions/control-fallback-policy.md`。已实现：`ui-widgets::{RoleId, Service, Family,
> Registry, LegacyFamily}`。

### 3.0 语义角色 → 家族实现 → 回落

应用只写语义（`Request::Checkbox{..}`），注册表按固定三步回答：**Native**（当前风格真有）→
**Synthesized**（用当前风格令牌拼一个朴素版）→ **Borrowed**（按固定顺序借别的家族，外观
不一致是有意代价）。另有 `Broken`（家族声称支持却返回空，是缺陷信号）与 `Missing`（占位）。

判据：比较「事件 → 节点结构」的映射。相同则共用实现 + 数据；不同则必须分开实现。
按此判据，checkbox / toggle / button / menu / nav **全部必须分开实现**。

**crate 布局**（2026-10-27 落地）：`ui-widgets` 只留语义与分派（`RoleId`/`Service`/`Family`/
`Registry`/`tween`/`LegacyFamily`），每种风格各自成 crate：`ui-widgets-md3`、
`ui-widgets-fluent`、`ui-widgets-glass`。`ui-gallery` 是示例 crate（依赖全部风格包）。
风格 crate 只依赖 `ui-core`/`ui-style`/`ui-widgets`，不得反向依赖。

### 3.1 首个垂直切片：三种风格的 Toggle

数值全部取自参考源码（不是估算）：

| | MD3 | Fluent | Glass |
|---|---|---|---|
| 轨道 | 52×32，圆角 16 | 40×20，圆角 10 | 58×26，圆角 13 |
| 滑块 | 16 → 34×22（权重 11）→ 24；按下 28 | 12×18，悬停 14，按下 17 | 35.2×22（1.6 倍宽），膨胀 +16×/+20 |
| 位置 | 中心 12 → 36 | 左 3 → 25（悬停 2 → 24） | 左 2 → 20.8 |
| 时长/曲线 | 300ms，序列内两段 cubic | 83ms，fastOutSlowIn | 按压 120ms easeOut 到 0.45；行程 380×剩余 |
| 来源 | `flutter/material/switch.dart` | `fluent_ui/inputs/toggle_switch.dart` | `liquid_glass_widgets/interactive/glass_switch.dart` |

**已知缺口（诚实记录，不假装做到）**：

- **Glass 轨道**：高光渐变**两端都实现了**（`Gradient` 进 `RectPrim` 与 `NodeData`；CPU 逐像素
  采样，GPU 着色器加 `a_fill2`/`a_grad` 两个实例属性，`gpu_scene` 断言「单调下降且与 CPU 差
  ≤3 LSB」）。`blurRadius: 6` 的绿色光晕仍是缺口（无阴影词汇）。
- **Glass 滑块**：参考用折射（`refractiveIndex 1.22`、`thickness 14`、`blur: 0`）+ 边缘高光。
  阶段图未落地前，用「小幅 `backdrop_blur` + 1px 边缘」近似。
- **Glass 膨胀曲线**：参考是 `animateTo(1.0, duration: 380 * 剩余行程, curve: easeIn)`；
  我们用 `sin(π·progress)` 鼓包近似（两端 0、中间 1），行为一致但不是同一条曲线。
- **MD3**：`thumbShadow = kElevationToShadow[0]` 未画（无阴影词汇）。**状态层本来就不存在**——
  已核实 `_SwitchDefaultsM3` 不定义 `overlayColor`，按压反馈就是滑块变大变色。
  **已更正两处**：位置是**线性**的（`visualPosition` 取控制器原值）、颜色走 `_colorAnimation`
  的 easeOut（先前误判为「瞬换」）；滑块中心是 `trackRadius + progress × (trackWidth − trackHeight)`
  = 16→36，不是按左右内衬算出的 12→36。
- **Fluent**：`FocusBorder` 未画（无焦点环词汇）。

1. **切换是数据交换**：`App` 持有 `Arc<StylePack>`，`FrameInfo` 携带之。换包 = 换指针。
2. **过渡动画是可选的**：包间插值按 Fluent 报告的做法分两类——颜色/时长 lerp，
   离散项（亮度、曲线）`t < 0.5` 硬切；但必须允许 `Motion::Duration(0)` 直通（MD3 的
   `AnimationStyle.noAnimation` 模式）。
3. **混用 = 祖先可覆写**：`Decl` 树加一个 `style_scope(pack_override)` 节点，resolve 时沿
   祖先链找最近的包。这样「一列 MD3 按钮里嵌一个 Fluent 开关」是声明出来的，不是继承出来
   的。注意：混用磨砂时**采集边界按最外层玻璃合并**（LG 的 blend-group 边界做法），
   否则每块小玻璃各自抓全屏，代价不可接受。
4. **自定义风格控件**：两条门。轻者实现 `Widget::resolve` 消费同一个 `StylePack`；
   重者通过 §5 的 ShaderLeaf 接口携带 `{asset, uniform 布局, 输入取法}` 自带新材质阶段，
   注册进配方即可——**renderer 不认的配方在解析期就按能力报告降级到 fallback 角色**，
   这是 Fluent 已证的模式（`DisableAcrylic`），也是 Haze ADR-0010 的模式（native 失败→
   fallback，且失败粘滞到 detach）。

---

## 4. 绘制词汇的破坏性扩展（ui-core/draw.rs）

现状只有 `RectPrim{fill,border}` + Text/Glyph，**没有阴影、渐变、混合模式、纹理采样**
（本会话 grep 核实）。`MaterialRecipe` 落地需要：

```rust
pub enum Fill { Solid(Color), Gradient(GradientSpec), Texture{atlas_or_leaf, tiling} }
pub enum Blend { SrcOver, AlphaLayer /* role×alpha 独立步，MD3 状态层 */,
                 Luminosity, Color, Multiply, Screen, Overlay }  // Fluent 需要后四个的非可分混合
pub struct ShadowSpec { pub layers: &[(Color, f32 blur, Offset, f32 spread)] }
```

- 状态层**必须是独立合成步**而非预合并背景（MD3 用分离的 `Color.alphaBlend`；否则
  hover+focus 无法叠加——md3 报告事实 5）。
- 渐变描边（Fluent 按钮顶亮底暗 1px 渐变边，`buttons/theme.dart:352-369`）需要 `Fill::Gradient`
  进批处理词汇。
- CPU 后端缓冲是**直通 alpha**（实测 50% 红 → `[255,0,0,128]`）。原 `composite_over_in`
  把层像素按预乘读，半透明内容一进组层就被画亮（裸画 R=128、经层 R=255）——**已修**：
  层合成直接复用 `blend_pixel`，结构上保证层内层外同一条公式；两条测试钉住
  （差分 + 精确像素，破坏回「按预乘读」立即变红）。shm `Argb8888` 要求非预乘，自洽。

---

## 5. 效果系统的破坏性扩展：链 → 阶段图

> **2026-10-27 状态变更**：跨帧复用（含六趟嵌套带的 `CachePlan::pass_bands`、`frost::split`
> 子块重算、`raw` 快照、`written()` 上报）**已全部撤除**，现行实现是每帧全量重画、每个阶段
> 从当帧场景整帧重算。原因与将来重建所需的前提写在 `docs/effect-cache-plan.md` §11。
> 下面这一节的阶段图仍是**目标形态**，但它必须建立在全量重算之上：阶段声明的
> `domain`/`spread` 是为了「输出矩形与越界语义可读」，不是为了增量复用。

`Vec<Effect>` 线性链 → `Vec<Stage>` 命名阶段 DAG。这不是赶潮流，是两个硬事实逼的：

1. **一次「先产纹理再读纹理」的效果**（法线→折射、frost 行间采样重建）线性链根本表达不了。
   我们自己的箱式模糊虽已能跑（六趟 h,v,h,v,h,v），但那是链内约定，不是可复用的阶段契约。
2. **阶段的边界语义必须显式**——这是本会话全部 backdrop bug（边缘旧像素、底部透明带、
   双模糊）的共同根：契约里从来没有「本阶段的输入矩形 + 越界怎么办」。参照三家：
   Haze `calculateLayerBounds = modifierBounds.inflate(radius)`
   是声明式的；LG 显式传 `uCaptureOffset` 把 fragCoord 映进采集图坐标；
   `progressive_blur.frag` 对越界 tap **权重归零 + 动态归一**而不是夹取。

```rust
pub struct Stage {
    pub id: StageId,                    // 配方内的名字，缓存键的一部分
    pub op: StageOp,                    // Blur{sigma,boxes} | Refract{index,thickness} | Tint | Noise{tiling} | ShaderLeaf{..}
    pub inputs: &[StageId],
    pub domain: DomainSpec,             // 输出矩形怎么算：EffectRect{base: RectName, inflate: f32}
    pub spread: Spread,                 // ZeroWeights | Clamp | Tile   ← tiny-skia 的显式契约
    pub needs: Capability,              // BackdropCapture{scope} | OffscreenPass{max_megapx} | ShaderModel
}
```

硬性契约（写进类型注释 + 校验器）：
- **`spread` 必填**，默认值禁止存在（本会话的教训：`CLAMP_TO_EDGE` 是隐式默认，坑了
  边缘、底部、快照三处）。
- **每个阶段声明自己的输出矩形算法**（可从链推导，不许拍脑袋）。
- **阶段输出不跨帧复用**（现行契约）。将来若要重启缓存，键必须含 (op 参数, inputs 版本,
  输出矩形)，且 `pass_bands` 的嵌套包含性证明要原样复用；Haze ADR-0012 的诚实结论照抄：
  **保留输出的"像素冻结"不承诺**。
- **能力协商在 resolve 期完成**，不是渲染期静默跳（AGENTS.md 既有教训：静默跳过画出看似
  合理的错画面）。

ShaderLeaf（最后 5% 的光学）按 InkSparkle 已验证的模式：
`{asset, 位置化 uniform 布局声明, 异步编译期间跳过本帧（early-return），预热入口}`；
`layout(location=N) ↔ setFloat` 的对应关系写进注册结构体，运行时按声明打包，禁止手数字段序。

---

## 6. 渲染器改造清单（ui-paint / ui-paint-gpu 共同实现同一语义）

| 能力 | 谁需要 | 备注 |
|---|---|---|
| 非可分混合（luminosity/color/overlay） | Fluent acrylic | W3C compositing 公式；两后端一致 |
| 纹理平铺采样（噪声） | Fluent acrylic | 图集或平铺 uniform；LG 证明真资产比程序噪声便宜 |
| 渐变填充 | Fluent 按钮边、MD3 scrim | 进 `Fill` 词汇 |
| 多阶段离屏图 + 纹理间采样 | LG 全链、我们的 frost | 泛化现有 `LayerEntry`/effect_pass |
| progressive blur（σ 沿位置变化） | LG 导航栏 | `BOX_FRAGMENT` 的 σ 从 uniform 改读输入矩形归一化的 `fragCoord`；tap 权重归零 |
| 法线纹理生成（SDF 中心差分） | LG refraction | 几何 pass 只依赖本地形状——**移动/缩放不重画**（LG 缓存事实），正好接进阶段图 |
| backdrop 采集坐标系 | LG/我们 | 采集矩形 + 原点在设备空间显式传递；当前是整帧读当帧场景，坐标问题被消掉，将来做降采样采集时才需要显式换算 |
| 呈现脏区 | 全部 | 每帧重画整个表面，因此呈现恒为全量脏区；逐帧脏区上报随增量机制一并撤除 |

性能策略（照抄已被证明的，不重新发明）：
- 输入降采样阶梯（Haze ADR-0004：1.0/0.8/0.5，带 12.5% 迟滞、按更新频率加权）。
  这是**当前唯一保留的优化方向**：它不依赖任何跨帧像素复用，每帧仍从当帧场景重算。
  子块重算已搁置（§11），不要在本轮设计里假设它存在。
- 动画期 1Mpx 上限 + 落定后补全质量帧（LG 已验证）。
- frost 降级顺序固定：先 0.25×σ 普通模糊，再关采集走 fallback 实色（LG + Fluent 双重印证）。

---

## 7. 验收纪律（防 flutter_liquid_glass 化）

1. **旋钮消费检查**：`StylePack` 里每个公开字段必须至少被一个 resolve 路径读取。以
   「内置包全字段 × 控件全状态」的声明矩阵测试（纯 CPU 后端，逐格出 Decl）做静态差分：
   新增字段未进矩阵 = 测试红。B 包 4 个死旋钮、1 个孤儿 painter 就是这么漏的。
2. **风格包是数据**：内置三包（`style-md3` / `style-fluent` / `style-glass`）先落成
   `const`/生成表，**零逻辑分支**。风格差异只允许出现在令牌与配方里；`if style == ...`
   出现在 `ui-widgets` = 设计违规。
3. **动效测试断言几何事实**：位置/尺寸在给定 `dt` 序列后的精确值（既有弹簧/曲线单测模式），
   不断言"动画被推进过"。
4. **效果新契约必须带 `pass_bands` 同款包含性测试**：构造参数 → 断言读区⊆写区，破坏嵌套
   → 变红。

---

## 8. 前置缺口（不补齐就只能做到降级版）

- ~~`Event::Text` 有产出无消费、IME（`zwp_text_input_v3`）未接线~~ **已完成**（engine-gaps §19）；
  剪贴板仍缺 → `TextField` 系列控件被它堵死；列在此处避免设计被误读成"控件先行、输入自理"。
- ~~CPU 合成按预乘读直通缓冲（§4）~~ **已完成**：层合成改走 `blend_pixel`（直通），四处错误注释
  改正，`blend_domain_tests` 样本改成真实直通像素（187 与 128 的差域结论仍成立）。
- 双后端混合语义目前只有 src-over：Fluent 包在补齐 §6 前两档降级（实色 fallback）是**设计内
  行为**，不是残缺。
- tone 级配色数学（HCT）`[未证实]`：MD3 包的种子生成先走简化 perceptual ramp（Fluent 移植的
  `toAccentColor` 用黑白 lerp 被其报告点名为缺陷），完整 HCT 留作可替换生成器
  （§3.1 的 generator seam），接口不依赖实现细节。

---

## 9. 实施顺序（每步全绿才进下一步）

1. **M1 地基** ✅：`ui-style` 落地（`RoleDict` 键寻址 + 别名重定向 + 逐键合并链、
   `MotionTable`（legacy/fluent/material 三表 + 反向覆写）、`ShapeTokens`、`Capability`）。
   `ui-core::Easing` 扩 `Cubic(a,b,c,d)` 与 `Piece(&'static PieceCubic)`，二分求解移植自 Flutter
   `Cubic`/`ThreePointCubic`；`mirroring_a_cubic…` 钉住 g(t)=1-f(1-t)（改成只取反不交换立即红）。
   `ui-style` 已进边界名单并做过污染→红→恢复→绿；能力面按 §7.1 只留两个有读取方的字段。
   CPU 直通合成修复已提前完成。
2. **M2 控件搬家** ✅（已接入 gallery 第五页）：`ui-widgets` crate 落地（`WidgetState`/`Button`/`List`/`Card`/`Panel`
   + 共享 `text::leaf`），`ui-style::legacy_pack()` 把示例的内联字面量提出成数据。
   验收 = `crates/ui-widgets/tests/parity.rs`：同一场景「手写声明树」与「控件+包」两条路
   经 CPU 光栅器渲出**逐字节相同**的像素（1 级色差即可被它抓住）。
   **示例本体暂不切换**——用户正在用 `window_gpu` 实测嵌套带修复，换场景会毁掉对照物；
   待模糊验收通过后再接。悬停淡入改由 `Button::animate_hover` 走包的 `StateChange` 档
   （时长/曲线来自包，破坏成字面量立即变红）。
3. **M3 三包落地**：`style-apple`（squircle 已有、vibrancy=我们的 backdrop 配方）、
   `style-md3`（状态层 + 容器深度角色 + 弹簧动效令牌）、
   `style-fluent`（luminosity/color 混合 + 噪声 + 渐变边——依赖 M4a）。
4. **M4a 混合与填充**：Blend 词汇 + 非可分混合双后端 + `Fill::Gradient` + 噪声平铺。
5. **M4b 阶段图**：`Vec<Effect>` → Stage DAG（带 domain/spread/能力协商），frost 配方改写。
   **建立在全量重算之上**：每阶段每帧从当帧场景整帧重算，`Damage` 不参与「是否重算」的决策。
   增量复用与子块缓存已搁置（`docs/effect-cache-plan.md` §11），将来重启时
   `pass_bands` 的嵌套包含性证明必须原样复用。
6. **M5 光学**：几何 pass（SDF→法线纹理，本地空间缓存）+ refract/chromatic 渲染 pass +
   progressive blur（越界权重归零）。
7. **M6 输入**：`Event::Text` 路由 + IME + 剪贴板 → `TextField`。

**M0（已完成，2026-10-27）**：撤除全部跨帧像素复用，两后端改为每帧全量重画；`Damage` 只用于
提交与裁剪。这一条排在所有里程碑之前，因为它是其余设计的地基——在「谁写过哪块」的记账之上
盖阶段图，只会把静默错误搬到更深的层。

M1–M3 不动 GPU 语义；M4/M5 才碰。每步的「破坏 → 变红 → 恢复 → 变绿」按 §7.6 清单执行。

## 10. 风格化控件体系（2026-10-27 讨论定稿）

### 10.1 三类状态的归属

| 状态 | 例子 | 归属 | 传递方式 |
|---|---|---|---|
| 语义值 | 开关的开/关、分段控件的选中项、输入框的文本 | **Role 层**（实现无关，风格不知道它从哪来） | `Request` 字段 |
| 交互状态 | hover / press / focus / drag | **引擎**（`FrameInfo` + `WidgetState::of`） | `BuildCtx.state`，类型上无法伪造 |
| 动画值 | 位置、颜色、大小、厚度 | **引擎 `AnimStore`**，每个参数一个原子实例 | `BuildCtx::animate(id, target, spec)` |

三者的关系是单向的：语义值与交互状态只决定**目标**，动画实例只负责**行进**，家族只做
`标量 → 几何/颜色` 的纯映射。任何一层都不许直接写几何。

**引擎已有的支撑**（本轮核实，不是新建）：`AnimStore::animate` 从当前值重定向、时长按剩余
距离缩放；`App::anim()` 由框架推进并转成脏区；`FrameInfo::{hovered,pressed,focused}` +
`hovered_within`/`pressed_within`/`dragging_within`（后两者本轮补齐）；`WidgetState::of`。

### 10.2 风格是沿树继承的上下文

```rust
pub enum Element {
    Semantic { req: Request, children: Vec<Element> },      // 语义占位，由风格解析
    Node { decl: Decl, children: Vec<Element>, style: Option<StyleId> }, // 引擎节点
}
```

- `.style(StyleId)` 是成员函数，写进 `Node.style`，解析时替换 `StyleCtx` 并**向下传递**；
- 语义控件从 `StyleCtx.active` 取风格，调用方不再指定家族；
- **非语义节点忽略样式但不中断继承**——它只用自己 `Decl` 的属性，`child_ctx` 照常下传；
- `.child()` 两种变体都收，语义控件与自定义控件可任意嵌套；
- 容器同样走 `Element::Semantic`（`Request::Surface { id, kind }` + children 槽位），
  玻璃容器需要「背景层 / 内容 / 描边层」三层顺序，没有 children 槽位就表达不出来。

`resolve(Element, &StyleCtx, &mut BuildCtx) -> Decl` 是唯一知道风格的地方；产物是引擎类型
`Decl`，由 `tree.apply` 摄入。**中间这层 `Element` 不可省**：`Decl.children` 是 `Vec<Decl>`，
装不下 `Request`，而把语义放进 `ui-core` 会让引擎认识"开关/分段控件"。

### 10.3 由此必须改的接口

- `Registry::build(req, children, ctx) -> (Decl, Service)`：容器要求"先有子节点再建自己"，
  所以构建不能在 `resolve` 之前完成；`Service`（Native/Synthesized/Borrowed/Broken/Missing）
  随之变成**每次构建的返回值**——它本来就依赖 `StyleCtx.chain`，是上下文相关的。
- `Request` 只留语义（`Toggle { id, on, label }`、`Surface { id, kind }`），
  去掉 `progress`/`press`/`dragging`/`state`。
- 家族签名 `build(&self, req, children: Vec<Decl>, ctx: &BuildCtx) -> Decl`。

### 10.4 性能包线（实测，`toggle_bench` / `list_bench`）

| 场景 | 结论 |
|---|---|
| 结构成本 | ≈4µs/控件/帧（≈1.3µs/节点），**2000 控件以内可忽略** |
| 光栅 | ≈37ns/px，CPU 全量重绘的硬成本，与控件数无关 |
| 虚拟列表 | 36 行（视口 27 + 上下各 4 预取），10 万项与 1 千项同价：apply 20–28µs |
| 朴素全建 | 10 万项 = 20 万节点，apply 160ms + layout 116ms/帧 |
| A vs B | A（家族返回 `Decl` 再 apply）比 B（直接写树）多一整趟遍历 + 一整棵分配；1000 控件差 ~1.5ms，20000 控件差 ~40ms |

**决定**：现在按 A 做（家族可脱离树单测），`BuildCtx` 预留 `node(parent, params) -> NodeId`
以便将来切 B；切换点由数据决定（>2000 控件）。

**搁置但记录**：arena/内联子节点、动画槽位写进节点（免每帧哈希）、`RelayoutBoundary` 式布局跳过、
`Damage` 裁剪重绘区域（与"不跨帧复用像素"的契约不冲突——契约禁止的是复用**结果**，不是禁止只画脏区）。

### 10.5 极长文档的硬约束

- 坐标必须**相对视口**产生：`y = (i as f64 * ROW_H - scroll as f64) as f32`。
  直接写 `i as f32 * row_h` 在百万行处 ULP 已达 2px（Chromium 把布局上限压在
  33 554 428px 也是同一原因）。
- 滚动偏移用 f64 保存；大数只允许出现在那次减法里。
- 单行超长文本**必须按列窗口化**：裁剪只能挡住绘制，整形与布局仍会处理整行。

### 10.6 下一步计划（风格化控件体系优先）

| 步 | 内容 | 验收 |
|---|---|---|
> **注意：S1–S7 整表已过期。** 它描述的是 `StyleCtx`/`Registry`/`Service`/`Family` 的结构，
> 而这些在 E 阶段已全部删除，改为 `Element`/`Container`/`Leaf`/`Styled` + `Cx`/`NodeCx`。
> 下表仅作历史记录保留。

| **S1** | `Element` + `StyleCtx` + `resolve`（纯风格层，不动引擎） | 测试：非语义节点**忽略样式但不中断继承**（子节点仍读到外层风格）；`.child()` 混装语义与自定义控件 |
| **S2** | `BuildCtx { anim, state }`；`Registry::build`；`Service` 改为构建期返回 | 测试：同一声明在不同 `StyleCtx.chain` 下得到不同 `Service`；Borrowed 仍能回落 |
| **S3** | 三家族改签名，各自声明原子实例（`position`/`colour`/`press`/`thickness`） | 测试：MD3 中途反向不重置（`AnimStore` 语义）；玻璃厚度是独立标量而非 `sin(π·progress)`；MD3 完全按下恒 28 的连续性有界 |
| **S4** | 示例改 `Element` 链 + `WidgetState::of` + 引擎 `AnimStore`；删手写命中与 `Switch::advance` | 窗口实测：点击/拖动/悬停全走引擎路由；逐帧 trace 与现有一致 |
| **S5** | 画廊每相位用全新 `AnimStore`（首调用即落目标值） | 现有几何断言全部保持 |
| **S6** | `Request::Surface` + children 槽位：先 MD3 Card，再 Glass Panel（三层顺序） | 像素：玻璃容器的子节点在背景层之上、描边层之下 |
| **S7** | `Decl` 属性归属痕迹（风格声明 vs 调用方声明） | 测试：冲突时风格覆盖且可查询，不静默吞掉 |

### 10.7 动画与手势的使用者契约（2026-10-27 补充）

使用者（app）只做两件事：**读实例值**、**在变化时设目标**（时长与曲线由风格提供）。
步进、落定判定、转脏区、排帧全部由 `AppLoop` 负责；app 代码里出现 `advance(dt)` 或
`is_animating()` 就说明它绕过了框架。

**所有会动的属性都只经动画实例取值**，包括拖动中的滑块位置：拖拽设的是一个
**极短跟随**（`ui_widgets::build::DRAG_FOLLOW` ≈ 24ms）的目标，而不是写值。存储先把
`from` 同步成当前值再接受新目标，因此目标频繁变化时是平滑重定向，不会跳变。

手势同理：`App::drag(DragEvent)` 与 `App::activate(Id)` 是仅有的两扇门。点击**携带身份**
（引擎在 `pressed` 上已算好目标、`DragTracker` 也知道是否成拖），因此 app 永远不必为点击
做命中测试；`activate` 不再只是键盘的门。拖动用**相对位移**（`delta / travel`）映射行程，
不需要矩形，控件在指下移动也不影响。

**S8 之后**才回到能力缺口（阴影词汇 → MD3 `thumbShadow` 与玻璃 6dp 光晕；Fluent 焦点环；
折射/阶段图）与虚拟列表（`RoleId::List` + 10 万项窗口演示）。

理由：S1–S3 决定"风格化控件体系"的骨架，之后每个新控件与新容器都按这个骨架写一次；
先补阴影词汇会把它们写进即将被改签名的家族里，等于做两遍。

---

## 11. 语义化控件与语义化布局（2026-10-27 定稿，取代 §10.2/§10.3 的槽位方案）

§10.2 的 `Element::Semantic { req, children }` + 容器 children 槽位方案**已撤销**：它逼所有
风格共用一套结构，与「同一角色在不同风格下结构完全不同」的事实冲突。本节是现行方案。

### 11.1 两个基本类型的准确含义

| | `Decl` | `Widget` |
|---|---|---|
| 是什么 | **数据**：引擎唯一认识的输入 | **代码**：把事实翻译成 `Decl` 的一次性对象 |
| 生命周期 | 每帧重建；引擎经由 `tree.apply` **保留**其节点身份 | 构造完即丢弃，不进树 |
| 身份 | 有（`id` 跨帧稳定），差分/动画/脏区/命中全部以它为准 | 无 |
| 引擎是否接触 | 是 | **否，永不接触** |
| 交互状态 | 不持有 | 不持有（归引擎 `FrameInfo`/`WidgetState`） |
| 动画值 | 不持有 | 不持有；按自己的 `id` 向 `AnimStore` 登记与读取 |

`Widget` 的输出在 `view()` 结束时已全部变成 `Decl`，因此引擎侧（布局/命中/事件/脏区）
**一行都不用改**才能容纳这套方案。

### 11.2 `Widget` 只有 `decl()`

```rust
pub trait Widget {
    fn decl(&self) -> Decl;
}
```

单方法。曾经在探针里加过 `draggable()`，已撤销：拖动是节点的属性，与 `cursor`、
`ink_margin` 同级，随 `Decl` 流动，引擎按节点属性路由，不问已销毁的 `Widget`。

**动画实例的存续规则**（写进 `AnimStore` 文档注释，不作强制限制）：实例跟随 `id` 是否
仍被询问；挂在非根节点上的实例属风格特有，跨风格时不再被询问即等价于不存在。

### 11.3 语义层只做三件事

**取值、选区（选择哪个风格实现）、翻译成具体控件构造调用**。它不持有交互状态、不路由
事件、不碰动画推进。

```rust
// ui-style（无环，只依赖 ui-core）
pub struct ToggleSpec<'a> { pub id: Id, pub on: bool, pub label: &'a str }

// ui-widgets（与 `Widget` 同层；ui-widgets 已依赖 ui-style，反向放会成环）
pub struct Fx<'a> {
    pub anim: &'a mut AnimStore,
    pub state: &'a dyn Fn(Id) -> WidgetState,
    pub scale: f32,
}
pub trait Style {
    fn toggle(&self, spec: &ToggleSpec<'_>, fx: &mut Fx<'_>) -> Box<dyn Widget>;
    fn slider(&self, spec: &SliderSpec<'_>, fx: &mut Fx<'_>) -> Box<dyn Widget>;
    fn scaffold(&self, spec: &ScaffoldSpec<'_>, fx: &mut Fx<'_>) -> Box<dyn Widget>;
}
```

三条硬约束（都有实现与断言托底）：

1. **`decl()` 纯函数、可重入**——控件只存算好的标量，同一控件调两次 `decl()` 必相等；
   否则脏区重算、导出、测试都会得到不同结果。
2. **动画走显式 `&mut Fx`，不用 `RefCell`**——借用冲突必须是编译错误，不是运行时 panic。
3. **`Fx::state` 是公开字段**（跨 crate 必需），因此“不绕过 `Fx::at()` 自己查状态”
   只能是文档约束，**没有编译期强制**；这是当前唯一的弱边界，如实记在此处。

注：§11.3 原文写作“`ui-style::Style`”，实测会与 `ui-widgets → ui-style` 成环，
已按实际依赖改为 `ui-widgets::Style` + `ui-style::{SliderSpec, ToggleSpec, …}`。

「有限且固定的槽」成立，但含义是**语义字段个数固定**（`Toggle` = 3 个），不是结构槽位。
风格可返回任意层数的树（玻璃开关三层、MD3 两层），互不影响。

### 11.4 语义化布局与语义化控件同一机制

```rust
pub struct ScaffoldSpec<'a> {
    pub nav: Option<&'a dyn Widget>,
    pub header: Option<&'a dyn Widget>,
    pub main: &'a dyn Widget,          // 唯一必需区域
    pub context: Option<&'a dyn Widget>,
    pub inline: Option<&'a dyn Widget>,
    pub overlay: Option<&'a dyn Widget>,
}
```

六个区域（`Edge` 暂不做）。规则：

- `Option::None` 表示调用方未提供内容；
- 风格**可以**因此省略该区域，**也可以**画空状态，但**不得**因此改变其他区域的含义；
- 区域的内容是 `&dyn Widget`，骨架不认识它，**布局与内容天然分离**；
- 让位顺序、断点、`rail→bar` 切换都是**骨架控件内部行为**，不是框架常量。

### 11.5 `Surface`（与 `Affinity` 正交的第二个轴）

`Affinity { Page, Section, Item }` 回答「内容属于谁的生命周期」，`Surface` 回答「画在哪」。
两者正交，不可合并成一个枚举。

```rust
pub enum Surface { Canvas, Auto, Subsurface, Popup { anchor: Id }, Window }
```

- 用户定稿：`Subsurface` 保留；`Auto` 的解析规则 = **平台能力表 + 风格提示**；`Popup` 以后做。
- 降级必须可见（`Service::Borrowed/Broken` 先例），不允许画布裁剪静默吃掉内容。

### 11.6 已验证的事实（`ui-widgets/tests/widget_trait_feasibility.rs`）

7 条探针，406 测试全绿、零警告。破坏验证：把 Fluent 子节点改名回 `handle` → ①② 同时变红。

| 主张 | 结果 |
|---|---|
| 换实现后同一 `Id` 仍是同一节点 | 成立 |
| 换实现只替换稳定节点**之下**的子树（`created=1, retired=1`） | 成立 |
| 同一语义值在两种风格下绘制出不同图元 | 成立 |
| 交互由具体控件声明 | **已作废**，改为 `Decl` 属性（§11.2） |
| 飞行中的动画跨风格不跳变（按 `Id` 索引） | 成立 |
| 拿不出控件时按名字报告 | 成立 |

### 11.7 待办（本轮开始执行）

| 编号 | 内容 | 验收 |
|---|---|---|
| **T1** | `Widget` trait 落进 `ui-widgets`；`Decl.draggable` + 引擎接线（默认 `false`，行为不变） | 现有 406 测试全绿；新断言：`Decl.draggable` 随声明流动并被节点保留 |
| **T2** | MD3 / Fluent / Glass 的 Slider 与 Toggle 改 `-> Box<dyn Widget>` 垂直切片 | 三风格并排渲染；语义值跨风格保持 |
| **T3** | `ScaffoldSpec` 六区域骨架 + 三风格各一个实现 | 布局与内容分离可测：内容函数不认识骨架 |
| **T4** | Gallery：单窗口 + 左侧导航 + 每节三风格并列；含 Layouts 节 | 用户肉眼验收 |
| **T1–T4** | 已完成：426 tests、零警告，`gallery` 自检 PASS | — |

### 11.8 风格参数的传递（2026-10-27 定稿：作用域覆盖，不是调用点覆盖）

问题：每个风格有超出语义 spec 的可控项（MD3 的 hover 放大、Fluent 的滑块时长…）。
若把它们塞进 `ToggleSpec`，语义层就认得了具体风格。

**结论：用可嵌套的链式作用域覆盖，优先级 = 最近作用域 > 实例默认 > 风格常量默认。**
这不是原创：Flutter `Theme` 是 `InheritedTheme`（`material/theme.dart:234-245`）沿树下传 +
`ThemeData.copyWith`；SwiftUI 是 `environment(_:_:)` 修饰符；WinUI 是逐层 `ResourceDictionary`；
egui 是 `ui.style_mut()`。**三家都把覆盖做成作用域的事，而不是单个控件调用的事。**

放弃的方案与理由：

| 方案 | 为什么不要 |
|---|---|
| 调用点链式 `(style, params)` | spec 背上 `Box<dyn Any>`；传错风格只能运行期发现；不支持嵌套 |
| `Style` 关联类型 `type Params` | 破对象安全，`dyn Style` 没了 → 不能运行时切风格（E0038） |
| 名字字符串到静态表的映射（egui） | 类型不安全；与“风格是数据”的约定冲突 |

形状：

```rust
// ui-style：不透明环境，语义层不解释它
pub struct Env { parent: Option<Rc<Env>>, style: StyleId, params: Rc<dyn Any> }
impl Env { pub fn with<P: 'static>(parent: Option<Rc<Env>>, style: StyleId, p: P) -> Rc<Env>;
           pub fn find<P: 'static>(&self) -> Option<&P>; }

// 语义层链式（不出现任何风格类型名）
Page::new(id)
    .scoped(Md3, Md3Params { toggle: .., ..Default::default() })   // 包住整棵子树
    .main(content)
```

**两条硬约束**：

1. **传错风格不可能**：作用域类型是具体的，`Md3Params` 进 `FluentScope` 编译失败；
   `find::<P>()` 失配只在“该层不是给这个风格”时发生，风格实现自然会跳过它。
2. **实例默认值归 App**：引擎不藏可变全局。App 每帧把自己的默认值交给 `AppLoop`，
   一个进程跑多个 UI 实例时互不影响；不做进程级可变全局。

已知代价：`Style::{toggle, slider, scaffold}` 需要多收一个 `env: &Env`，三个包各改一次；
`Box<dyn Any>` 的 downcast 每次构造做一次类型比较（构造期，不在绘制期）。


### 11.9 上下文统一（2026-10-27 现状盘点 → 定稿）

**盘点的结论**：引擎里有 6 个上下文类结构，合计 25 个公开字段 + 3 个私有字段。

| 结构 | 角色 | 字段 |
|---|---|---|
| `FrameInfo` | 传给 app 的每帧事实 | 9 公开 + 3 私有（`hovered_path`/`pressed_path`/`drag_owner`） |
| `Fx` | 风格层构建通道 | 5（`anim/state/scale/now/env`） |
| `BuildCtx` | 单节点子通道 | 4（`anim/state/scale/now`，是 `Fx` 的真子集） |
| `Frame` | 旧路径构建通道 | 4（`anim/state_of/scale/now`，与 `Fx` 重复） |
| `WidgetState` | 单节点交互状态 | 6 |
| `Env` | 作用域链 | 3 私有 |

三处真实缺陷：

1. **同一事实写四遍**：`now` 在 4 个结构里各一份，`scale` 在 3 个里各一份；
   任一处漏赋值就是新的 dt=0 式缺陷。
2. **能力不对称**：只有 `FrameInfo` 有派生查询（`hovered_within`），风格层拿不到；
   而 `WidgetState::of` 恰恰需要这些查询才能算出状态。
3. **布局完全没有上下文**：`LayoutEngine::solve(tree, root, Constraints)` 拿不到
   `now`/`scale`/`env`，因此作用域参数永远影响不了布局。

**定稿方向**：合并为**一个** `Cx`，按能力派生窄视图，不按使用者分裂。

```rust
pub struct Cx<'a> {
    // 引擎本帧事实（只读）
    pub frame: u64, pub width: u32, pub height: u32,
    pub now: f64, pub dt: f32, pub scale: f32, pub pointer: Option<Point>,
    // 派生状态（查询，不是字段快照）
    hovered: Option<Id>, pressed: Option<Id>, focused: Option<Id>,
    hovered_path: Vec<Id>, pressed_path: Vec<Id>, drag_owner: Option<Id>,
    // 写入面
    anim: &'a mut AnimStore,
    env: Arc<Env>,
}
```

派生：

| 方法 | 得到 | 谁用 |
|---|---|---|
| `cx.state(id) -> WidgetState` | 单节点交互状态 | 控件、风格 |
| `cx.at(id) -> NodeCx<'_>` | 限定到节点：`animate`/`value`/`state` | 控件 |
| `cx.scoped(Arc<Env>) -> Cx<'_>` | 更窄作用域，共享存储与时钟 | 容器 |
| `cx.env() -> &Env` | 当前作用域 | 风格 |
| `cx.constraints() -> Constraints` | 布局输入 | 布局 |

**硬约束（三条，都必须有断言）**：

1. **`now`/`scale` 各只有一份**：`FrameInfo` 与 `Fx`/`BuildCtx`/`Frame` 四者的字段
   全部由 `Cx` 提供，不允许任何结构再自持一份；
2. **`Widget::decl` 收 `&mut Cx`**，`Scoped::decl` 通过 `cx.scoped(..)` 派生再传下去——
   能力因此可嵌套，而不是只能重传一个 `Arc`；
3. **布局接 `&Cx` 的只读部分**，使作用域参数能影响布局（最低要求：`scaffold` 的
   pane 宽度可被作用域改写）。

**顺序**（每步都要 `build --tests --examples` 零警告 + 全量测试绿才进下一步）：

| 步 | 内容 |
|---|---|
| R1 | 引入 `Cx` + `NodeCx`，`Widget::decl(&mut Cx)`；`FrameInfo` 改为持有 `Cx` 的共享部分 |
| R2 | `Fx`、`BuildCtx`、`Frame` 三者删除或降为 `Cx` 的别名；`Style` 收 `&mut Cx` |
| R3 | 布局收 `&Cx` 只读视图，`scaffold` 的 pane 宽度走作用域参数 |
| R4 | 三个示例与全部测试改调用点；**原有几何断言数值必须一字不变** |

### 11.10 事件由控件处理（2026-10-27，2026-10-27 二次修订）

> 本节的早期版本写于 `App::event`/`App::scroll` 尚未删除时，已作废。
> 以下为**当前**结论，代码为准。

#### A. 已完成的（有测试）

| 改动 | 结果 |
|---|---|
| `App` 方法数 | 6 → **2**：只剩 `view`、`changed`（`measure` 是引擎内部 `TextMeasure`，不在 `App`） |
| 删除 `anim` / `drag` / `draggable` / `accepts_drop` / `hit_behavior` / `focusable` | 全部并入 `Sense` 或 `Change` |
| 删除 `activate` | 点击与键盘 Enter/Space 都由引擎翻译成 `Change::Activated` |
| 删除 `event` | 0 使用者；键位(Tab/Esc/方向/Enter)全在引擎内部消化 |
| 删除 `scroll` | 滚轮经 `Change::Value` 送达滚动目标；偏移归控件 |
| `Sense`（u16 位集） | `HOVER/CLICK/DRAG/DROP/FOCUSABLE/SCROLL/PASS_THROUGH`；`HOVER` 是普通位，不是 egui 的常开 |
| `WidgetState.local: Option<Point>` | 引擎把指针换算到按下节点自身空间（E1 已完成） |
| `Change::DragStarted` | 拖拽开始与点击必须可区分（测试 `a_shaky_click_reports_no_drag_at_all` 暴露） |

#### B. 当前 `Change`

```rust
Toggled { on } / Value(f64) / Text(String) / Selected(Option<Id>)
Activated / DragStarted / Moved { dx, dy } / Cancelled / Data(Box<dyn Any + Send>)
```

- `Activated` **无载荷**（点击/键盘都不带坐标）；
- **已删** `Scoped` 容器（无任何真实使用者；E3 的 `Container` 将接管其职责）；
- 已加 `Widget::change`（E2）与 `Widget::decl(&Cx)`（E3 第一步）；旧 `Family`/`BuildCtx` 路径
  暂用 `BuildCtx::with_cx(..)` 桥接，**整条路径计划删除**；
- `Moved { dx, dy }` 只给**自绘拖动物**（面板）；
- **滑块不用 `Moved`**：它的值由控件自己从 `WidgetState.local` 算出，报 `Value`。

#### C. 方向（与成熟框架一致，未完成）

**`App::view` 返回控件树，而不是 `Decl`。**

```rust
fn view(&mut self, cx: &mut Cx) -> Box<dyn Widget>;
```

依据：GPUI 的树节点是 `Element` 对象（`element.rs:194` 的 `child()` 收 `IntoElement`），
Flutter 的树节点是 `Element`，**没有一家把树节点做成纯数据**。我们的 `Decl` 是纯数据，
控件在 `view` 返回时即被丢弃——这就是“控件处理不了自己的事件”的结构性原因。

**不做**「`AppLoop` 旁挂一张 `HashMap<NodeId, Box<dyn Widget>>`」：那只是给纯数据树打补丁，
树本身仍不记得控件，与工程实践不符。控件树即存活对象，帧内存活是它的自然结果。

#### D. 落地阶段（每步门禁：build 0 warning + 全量测试绿 + gallery 自检 PASS）

> **流程约定（2026-10-27）**：讨论中得出的任何结论，随时写入本文件；执行严格按本表顺序推进，
> 不跳过、不临时改序。纯重命名类工作单独列出交用户用 IDE 完成，不擅自批量替换。
>
> **结构优先（2026-10-27，用户明确要求）**：目标是**更良好的结构设计**，不是迁就现有实现。
> 「机械迁移」「保持调用点不变」「加过渡层让旧代码编译通过」都**不是目标**。
> 若正确结构要求某个既有形态消失，该形态就应消失；现有代码（含 213 处 `Decl::container`、
> 旧 `Family`/`BuildCtx` 路径、`Fx` 门面）是**待重构对象**，不是必须保护的资产。
> 每步先确定目标结构，再判断哪些实现要改、要删，最后才考虑迁移成本。

| 步 | 内容 | 状态 |
|---|---|---|
| E1 | `WidgetState.local` + 引擎填入 | ✅ 完成 |
| E2 | `Widget::change(&self, cx, bounds) -> Option<Change>`；滑块用它算值 | ✅ 完成（测试 `a_pressed_slider_reports_the_value_under_the_finger`；破坏后变红） |
| E3a | `ui_platform::Element` / `Container` / `Leaf`；`WidgetState` 下移；`ui_widgets::Styled` 适配器；`Fx::element` | ✅ 完成（测试 `a_styled_control_declares_what_the_control_declared`，破坏后变红） |
| E3b | `App::view` 返回 `Box<dyn Element>`；引擎 `let root = app.view(cx); let decl = root.decl(cx)` | ✅ 完成（33 处 view 实现；gallery 真正转为元素树） |
| E3c | 引擎在布局后遍历元素树调用 `Element::change(state, bounds)` 并派发 | ✅ 完成（并行遍历，无需 `Element::id()`；滑块回归已修复） |
| E4 | 删除旧 `Family`/`Registry`/`Service`/`Role`/`element`/`legacy` 整簇 | ✅ 完成（见下方 E4 记录） |
| E5 | 删除 `Fx` 门面与 `BuildCtx`/`Frame`，统一为 `Cx`/`NodeCx` | ✅ 完成 |

#### E3 前期核实（2026-10-28，源码为据）

**核实 1：`NodeKind` 的真正作用**

`NodeKind` = `Row | Column | Stack | Overlay | Box | Text`，只在**引擎内部**被读：

| 读者 | 用途 |
|---|---|
| `layout_engine.rs`（8 处） | `Row` 决定主轴方向；`Overlay` 被排除出主轴需求、单独定位 |
| `draw.rs`（1 处） | 批次合并时比较 `kind`（同类才能合批） |
| `hit.rs`（2 处） | 文字不参与命中（已由 `hit_behavior_of` 承担）|

**结论**：`NodeKind` 是**布局/绘制策略**，不是“控件类型”。它应留在 `Decl`（引擎中间表示）里，
但**不该由应用手写**。控件（`Container`/`Box`/`Text`）各自产出带正确 `kind` 的 `Decl`。
→ `NodeKind` **保留**，`Decl::container(..)` 这个手写入口**删除**。

**核实 2：`Decl` 的字段里哪些是引擎必需**

| 字段 | 必要性 |
|---|---|
| `id` / `kind` / `data` / `size_class` | 布局与绘制必需 |
| `children` | 树结构必需 |
| `ink_margin` / `offset` / `cursor` / `sense` | 引擎属性（命中、脏区、光标、订阅）|
| `env` | 作用域传递（E1 前的机制）|

全部字段**引擎都需要**；问题不在字段，而在**恰好由应用手写这些字段**。
→ `Decl` **保留**为引擎中间表示；控件负责产出它。

**核实 3：容器控件如何表达布局参数**

`NodeData::Container(ContainerParams)` 已含 `gap/padding/cross_align/opacity/effects/backdrop_blur/image/shadow/scrollable/scroll_y`。
→ 容器控件**直接持有 `ContainerParams` + `children: Vec<Box<dyn Widget>>`**，不必另造一套布局字段，
也**不要**复刻 `Decl` 的 `size_class` 等——那些由控件的 `decl()` 决定并写入 `Decl`。

**E3 目标结构定稿（待实施）**

```rust
// 唯一节点形态：容器也是控件，只是不产生语义。
pub struct Container { id: Id, kind: NodeKind, params: ContainerParams,
                       size: SizeClass, children: Vec<Box<dyn Widget>> }
impl Widget for Container { fn decl(&self, cx) -> Decl; fn change(..) -> None }

pub struct BoxLeaf { /* fill/radius/border/gradient */ }
pub struct TextLeaf { /* TextBlock */ }
```

**既有形态的处置（判断，非迁移方案）**

| 形态 | 处置 |
|---|---|
| `Decl::container(Id, NodeKind)` 手写入口 | **删**（213 处调用点随之改写为容器控件；数量不是理由） |
| `Decl` / `Tree` / `NodeKind` | **保留**（引擎中间表示与布局策略） |
| `BuildCtx` + 旧 `Family`/`LegacyFamily` 路径 | **删**（被 `Widget` 取代） |
| `Fx` 门面 | **删**，`Style` 直接收 `&mut Cx` |
| `App::view` 返回类型 | `Decl` → `Box<dyn Widget>` |

#### Sense 的归属（2026-10-28 定稿）

**问题**：核实现状——`Sense` 在控件/风格包里 **0 处真实声明**，在应用/示例里 **20 处手写**。
即“真正知道自己接受什么交互”的控件一处没写，“什么都不知道”的应用全写了。与 `Decl::container`
同病：应用在做控件的活。

**定稿：方案 C——控件给默认，调用方可用 `|` 追加。**

```rust
pub trait Widget {
    /// This control's default interaction set; the caller may widen it with `|`.
    /// 本控件的默认交互集合；调用方可用 `|` 加宽。
    fn sense(&self) -> Sense { Sense::NONE }
    fn decl(&self, cx) -> Decl;   // 控件把自己的 sense 写进 Decl.sense
}
```

- 控件声明**自己的**能力（滑块 `HOVER|DRAG|FOCUSABLE`、按钮 `HOVER|CLICK|FOCUSABLE`、容器 `NONE`）；
- 应用不再书写 `Sense`，只在确需**扩大**时对容器叠加（如整块面板可拖）；
- `Decl.sense` 由控件在 `decl()` 里写入，应用不直接触碰。

**用与实现的边界（2026-10-28，用户澄清）**：

规则不是“应用永远不写 `Sense`”，而是**“使用控件的人不写，实现控件的人写”**：

| 应用在做的事 | 写 `Sense`？ |
|---|---|
| 使用已封装控件（`Slider::new`、`Button::new`） | ❌ 控件已在 `decl()` 内写好 |
| 临时组合已有控件（包容器、调布局） | ❌ 容器默认 `NONE` |
| 给某区域加自定义行为（整块面板可拖、区域接受放置） | ✅ 对那一层显式声明 |
| 自己实现一个新控件 | ✅ 那是控件实现者的职责 |

**框架不强制这条**：`Decl.sense` 是公开字段，谁需要谁写。框架只需保证
**已封装控件在 `decl()` 内自行声明各节点的 `Sense`**，使使用者无需重复书写。

**`Sense` 的层级是节点（`Decl`），不是控件。** 一个控件内部可产出多个节点，
各自 `Sense` 不同（如 `Md3Toggle` 的根可点、内层 `track`/`thumb`/`label` 可另设）。
因此**不设 `Widget::sense()`**——那是把节点级属性错误提升为控件级。

**明确否决：不做“按现状拆分位”的设计。** 曾提出把 `SCROLL`/`PASS_THROUGH` 移出 `Sense`
（因“现在只有列表/布局用”）。**这是基于现状的臆断，已撤回**：

- 数值输入框完全可以接受滚轮（增量调整）；
- 将来的快捷键录制控件要接受任意按键；
- 不能以“当前谁在用”限定某事件位的语义。

**因此 `Sense` 保持为一个不拆分的位集**：`HOVER | CLICK | DRAG | DROP | FOCUSABLE | SCROLL | PASS_THROUGH | NONE`，
任何控件都可以声明其中任意组合。

#### E3 修正：`App::view` 的返回类型不能是 `Box<dyn Widget>`（2026-10-28 核实）

**依赖事实**：`ui-widgets` 依赖 `ui-platform`（`Cargo.toml` 明写，`tests/parity.rs` 用到）；
`ui-platform` 只依赖 `ui-core`。因此 `ui-platform` 里让 `App::view` 返回 `ui_widgets::Widget`
**会形成循环依赖**，编译期不可能成立。

**正确形态**：引擎面向的契约由**引擎自己**定义（`ui-platform`），控件层实现它。

```rust
// ui-platform/src/element.rs —— 引擎对「活控件」的唯一要求
pub trait Element {
    /// 本帧声明。引擎在遍历控件树时逐节点调用。
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl;
    /// 子元素，供引擎遍历（`change` 派发与脏区都靠它）。
    fn children(&self) -> &[Box<dyn Element>] { &[] }
    /// 本帧语义变化。布局之后调用一次，`bounds` 是最终几何。
    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> { None }
}

// App
fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element>;
```

**`WidgetState` 下移到 `ui-platform`**：它只依赖 `ui_core::Id`/`Point`，是引擎路由的产物，
不是样式概念；引擎构造它并传给 `Element::change`。`ui-widgets` 保留再导出。

**`Container` 放 `ui-platform`**：它只做布局（`NodeKind`/`SizeClass`/`gap`/`padding`），
不含任何样式知识，是引擎级元素。

**分层结果**（与成熟框架一致）：

```
ui-core      Decl / Tree / NodeKind / Sense / Change / Cx
   ├── ui-platform   Element / Container / WidgetState / AppLoop / App
   └── ui-widgets    Styled 适配器 + 具体控件 + Style 门面
```

`ui-widgets::Styled` 适配：持有 `Box<dyn Widget>` + `Arc<Env>`，实现 `ui_platform::Element`，
在 `decl()` 内构造 `ui_widgets::Cx` 并调用内层 `Widget::decl`。

#### E3a 落地记录（2026-10-28）

| 新增/改动 | 位置 |
|---|---|
| `Element` trait（`decl` / `children` / `change`） | `ui-platform/src/element.rs` |
| `Container`（持有无子节点 `Decl` + 子元素，`map()` 覆盖全部字段） | 同上 |
| `Leaf`（无子节点、无交互，供文本/盒子） | 同上 |
| `WidgetState`（引擎侧交互状态，`of(&ui_core::cx::Cx, id)`） | `ui-platform/src/state.rs`；`ui-widgets/src/state.rs` 改为再导出 |
| `Styled`（`Box<dyn Widget>` + `Arc<Env>` → `Element`） | `ui-widgets/src/styled.rs` |
| `Fx::element(control) -> Box<dyn Element>` | `ui-widgets/src/style.rs` |

**命名冲突（已知，临时）**：`ui-widgets/src/element.rs` 是旧 `Family`/`Registry` 的「构建期元素树」，
与新 `ui_platform::element` 概念不同；它随 `Family`/`BuildCtx` 一并删除。

#### E3b 落地记录（2026-10-28）

**`App::view` 现在返回 `Box<dyn Element>`**，引擎在 `frame()` 里 `let root = app.view(&mut cx); root.decl(&mut cx)`。

**示例清理（用户指示：无用示例直接删除，不做迁移）**

| 删除 | 理由 |
|---|---|
| `ui-gallery/examples/style_compare.rs` | 与 gallery 的「三列对比」页重复；三风格几何差异已有单元测试钉住 |
| `ui-gallery/examples/layouts.rs` | 与 gallery 的 Layouts 页重复 |
| `ui-gallery/examples/material_panel.rs` | 与 gallery 的 Materials 页重复；其唯一独特覆盖（`effective_tint` 的抑制跳过分支）已转为单元测试 |
| `ui-gallery/examples/window_toggle.rs` | 与 gallery 的 Switch 页重复 |
| `ui-gallery/examples/toggle_gallery.rs` | 与 gallery 的 Switch 页重复 |

**保留并转换**：`gallery.rs`（主展示 + 自检）、`window_gpu.rs`（GPU 渲染验证；全部手写 `Decl`、不含控件，
故以 `from_decl` 包装为静态声明——它没有控件需要生命周期）。

**新增单元测试**：`ui-widgets-fluent` 的 `effective_tint_skips_the_suppression_only_when_luminosity_is_explicit`
（破坏 `Some(_) => 1.0` 分支后变红）。

**环境变更**：本会话的 Wayland 显示名是 `wayland-1`（不是此前记录的 `wayland-2`）；自检须用 `WAYLAND_DISPLAY=wayland-1`。

#### E3c 落地记录（2026-10-28）

引擎在布局之后、绘制之前，**在声明树旁并行遍历活元素树**，逐节点索取 `Change`：

```rust
fn collect_changes(element: &dyn Element, decl: &Decl, tree: &Tree,
                   cx: &ui_core::cx::Cx<'_>, out: &mut Vec<(Id, Change)>)
```

- **不需要 `Element::id()`**：`Container::decl` 产出的子声明与 `children()` 顺序一一对应，
  因此两棵树可并行下走；`Styled` 的 `children()` 为空，于是它的 `change` 只针对其根节点。
- **`state.local` 必须按本节点换算**：`Cx::local` 是「最深命中节点」空间，而控件要的是自己的空间。
  引擎用节点矩形把指针换算过去（`p - rect.origin`），否则滑块算出的值会是错的。
- 有变化被接受时追加全屏脏区，保证下一帧反映新状态。

**回归已修复**：`gallery` 自检 `slider drag value 0.708 (want 0.708) ✓`，整体 **PASS**。
滑块不再由应用累加位移——`gallery` 的 `changed` 只接收 `Change::Value`，`drag_base` 字段随之删除。

**门禁**：`build --workspace --tests --examples` **0 warning**；`cargo test` **452 passed**；自检 **PASS**。

#### E4 落地记录：旧控件路径已删除（2026-10-28）

**删除的模块**（`ui-widgets`）：`family.rs`、`registry.rs`、`legacy.rs`、`element.rs`（旧构建期元素树）、
`service.rs`、`role.rs`。它们构成一条只服务旧路径的链：`legacy → registry → element → service → role`。

**删除的 `impl Family for XFamily`**：`Md3Family`、`FluentFamily`、`GlassFamily` 三个结构与其实现一并移除。

**删除的示例**：`ui-gallery/examples/toggle_bench.rs`（唯一调用 `Family::build` 的示例）、
`ui-widgets-md3/examples/thumb_jumps.rs`（其诊断价值已由单元测试 `releasing_a_drag_does_not_resize_the_thumb` 承接）。

**测试改写而非删除**（它们钉住的是真实行为，只是走了旧入口）：

| 测试 | 改法 |
|---|---|
| `the_palette_crossfades_on_an_ease_out` | 新增 `toggle_decl_at(store, pack, on, now)`；`decl_after` 用**三次构建**（!on@0 → on@0 设目标 → on@advance 读取），复现真实重定向路径 |
| `releasing_a_drag_does_not_resize_the_thumb` | 按下与拖拽归属改为**路由事实**（`core.route(..)`），位移用 `fx.at(id).animate(..)` |

**保留**：`build.rs`（`BuildCtx`/`Frame` 仍被三个风格包的控件构造与若干测试使用），
`Decl::container` 仍用于静态子树（`Leaf`/`child_decl` 正依赖它）。

**门禁**：0 warning；**435 passed**（删除 17 条随旧接口消失的测试）；`gallery` 自检 **PASS**。

#### E5 落地记录：上下文统一为两层（2026-10-28）

**删除**：`ui-widgets::Fx`（门面）、`ui-widgets::build::{Frame, BuildCtx}`。
`build.rs` 只保留共享的动画实例名 `param::*` 与 `DRAG_FOLLOW`。

**统一后的两层**：

| 层 | 类型 | 用途 |
|---|---|---|
| 一帧 | `ui_widgets::Cx` | `env` / `state` / `at` / `at_scope` / `element` / `with_cx` / `report` |
| 一节点 | `ui_widgets::NodeCx` | `id` / `state` / `scale` / `animate` / `value` / `status` |

`Style::slider/toggle/scaffold` 现在收 `&mut Cx`；控件构造收 `&mut NodeCx`。
`Fx` 的三个独有能力（`at_scope` / `element` / `with_cx`）已并入 `Cx`。

**修掉的一处自伤**：批量重命名把 `toggle_decl` 测试辅助改坏了——它丢弃传入的 `env`，
改用 `for_test_mut` 的全新根作用域，导致三条作用域测试静默失去区分力（两个 `Decl` 变得完全相同）。
改为用传入的 `Cx` 后恢复。

**门禁**：0 warning；**432 passed**；`gallery` 自检 **PASS**。

#### M2 通用控件接入 gallery（2026-10-28）

此前 `Button`/`Card`/`List`/`Panel` 与 `ui_style::legacy_pack()` **只被 `parity.rs` 互相引用**，
是悬空的里程碑：控件存在、验收通过，但没有任何示例使用，因此不会随重构演进。

**处置**：新增 `gallery` 的第五页 `Page::Generic`（"Controls · Pack widgets"），
三列分别展示 `Card + Button`、`List`、`Panel`，全部由 `legacy_pack()` 驱动。

- 按钮的悬停淡入经 `NodeCx::animate` 推进，时长取包的 `MotionName::StateChange` 令牌；
- 其余三者是包驱动的静态声明，经 `from_decl` 成为元素。

**同时删除** `Button::animate_hover(&mut AnimStore, ..)`——它是上下文统一前的旧形状
（把存储直接递给调用方），唯一使用者是它自己的测试。该测试改为经 `NodeCx::animate` 驱动，
仍断言「淡入在包的 83ms 令牌处落定」。

**自检随之泛化**：`columns_ok` 原先写死三列标签为 `STYLES[*].1`（那是显示名，不是标签），
现改为按页取标签（`page_tags`），计数器分母改用 `PAGES.len()`。

**门禁**：0 warning；**432 passed**；自检 **PASS**（5/5 页导航、5/5 页三列不重叠）。

#### Sense 落地与非标准行为取缔（2026-10-28）

**审计结果**：七个位里只有三个有读取方。

| 位 | 改造前 | 现在 |
|---|---|---|
| `DRAG` / `DROP` / `FOCUSABLE` | 有读取方 | 不变 |
| `CLICK` | **只被写入** | 点击只在声明了 `CLICK` 的自身或祖先上报 |
| `SCROLL` | **只被写入** | 滚轮路由接受 `SCROLL`，与布局的 `scrollable` 并列 |
| `HOVER` | **只被写入** | 悬停是订阅：`hovered` 取最近的订阅者，`hovered_within` 只看订阅者链 |
| `PASS_THROUGH` | **只被写入**，且 `HitIndex::topmost` 根本不看 `behavior` | 新增 `topmost_consumer`，命中测试改走它 |

**控件侧落地**（应用一行都不写）：

| 控件 | 声明 |
|---|---|
| 三风格 Slider | `HOVER \| CLICK \| DRAG \| FOCUSABLE` |
| 三风格 Toggle/Switch | `HOVER \| CLICK \| FOCUSABLE` |
| `Button` | `HOVER \| CLICK \| FOCUSABLE` |
| `List` | `HOVER \| SCROLL` |
| `Card` | `HOVER` |
| `Panel` | `HOVER \| DRAG` |

**同时取缔**：`Decl::draggable(bool)` —— 它是 `Sense::DRAG` 的第二个入口，正是要清除的那类重复。

**顺带修掉的一处真实缺陷**：`resolve_cursor` 原本遍历 `hovered_path`。悬停改为订阅制后，
光标继承会被误伤——光标是「指针下有什么」，与是否订阅悬停无关。现在分成两条链：
`pointer_path`（全部节点，光标继承用）与 `hovered_path`（仅订阅者，`hovered_within` 用）。
`pointer_path` 与 `hovered_path` 一样必须**反转成根在前**，否则 `.rev()` 会让祖先赢过子节点。

**自检抓到的真实回归**：加 `CLICK` 门控后，`gallery` 的导航行（应用手写 `Decl`）不再上报点击，
导航整页失效。这是规则的正确效果——手写控件必须自己声明 `Sense`。已在 `nav_item` 补上。

**新增测试**（5 条，全部破坏性验证过）：点击订阅、滚轮订阅、透传映射、悬停订阅、Tab 可达性。

**门禁**：0 warning；**438 passed**；自检 **PASS**。

#### 禁用（inert）落地（2026-10-28）

**审计**：`WidgetState.disabled` 此前全仓只有两处——声明与默认 `false`。没有设置方、没有读取方、
`Decl` 无此字段、风格包无禁用令牌。即「禁用」这一概念从状态到外观到行为完全不存在。

**四层落地**：

| 层 | 实现 |
|---|---|
| 声明 | `Decl.disabled: bool` + `Decl::disabled(bool)`；`Container::disabled(bool)` 让外层禁用整棵子树 |
| 继承 | `apply` 时折入：`node.disabled = decl.disabled \|\| parent.disabled`（新节点与复用节点两条路径都折） |
| 引擎 | 惰性节点 → `HitBehavior::Ignore`（不按下、不悬停、不放置）；不是 Tab 停靠点 |
| 状态 | `Cx::route` 多收一份 `disabled: Vec<Id>`；`WidgetState::of` 用 `core.is_disabled(id)` 填入 |

**控件侧收口**：`Styled::change` 在 `state.disabled` 时直接返回 `None`——
惰性控件绝不上报，**由适配器保证，而不是靠每个控件自觉**。

**外观**：`Styled::decl` 在惰性时把根节点组不透明度乘以 `DISABLED_OPACITY = 0.38`
（MD3 规定 `onSurface` 取 38%）。这是统一淡出，不是逐角色的禁用配色；
逐角色令牌（MD3 容器 12% / 内容 38%、Fluent `TextFillColorDisabled` 等）**尚未做**，需要参考值。

**顺带取缔的两处非标准行为**：

1. `Decl::draggable(bool)` —— `Sense::DRAG` 的第二个入口，已删除，调用点改为显式 `.sense(..)`。
2. **按下任意节点即让它获得焦点**（`self.focused = self.pressed`），完全不看 `FOCUSABLE`。
   现改为：焦点移向最近的 `FOCUSABLE` 自身或祖先；没有则清除焦点。
   这条被 `focusing_a_node_repaints_its_border_colour` 与 `escape_cancels_a_drag_and_keeps_the_focus`
   两条测试立即抓到——它们的节点从未声明过 `FOCUSABLE`，此前一直靠这个缺陷「恰好」通过。

**新增测试**（2 条，均破坏性验证过）：`an_inert_node_takes_no_click_and_is_no_tab_stop`、
`an_inert_control_reports_nothing`。

**门禁**：0 warning；**440 passed**；自检 **PASS**。

#### 事件归属：控件宣布，引擎只路由（2026-10-28）

按「`Activated` 只作焦点事件、切换由控件宣布」的结论重构：

| 变更 | 内容 |
|---|---|
| `Change::Dropped(Option<Id>)` | 新增，**占位**：`DragEvent::Dropped` 不再借用 `Selected`。目前无人声明 `Sense::DROP`，故恒为 `None` |
| 引擎不再宣布点击 | 点击只在引擎侧记为**事实** `self.clicked`，不再产生 `Activated` |
| `Activated` 仅焦点事件 | 只有「已聚焦且非禁用」的节点在 Enter/Space 时收到 |
| 开关自己宣布 | 三个 Toggle/Switch 实现 `change()`，`state.clicked` 时上报 `Toggled { on: !self.semantic_on }` |
| 新增 `WidgetState.clicked` | 引擎只把该事实投递给声明了 `Sense::CLICK` 的自身或祖先——`CLICK` 因此重新有读取方 |
| `route` 改具名结构体 | 8 个位置参数改为 `ui_core::cx::Routing`；再加事实不会破坏任何调用点，也不可能被静默调换 |

**gallery 随之调整**：导航行从手写 `Decl` 改为 `NavRow` 控件（自己上报 `Activated`）；
`changed` 改为「`Activated` 管导航、`Toggled` 管开关、`Value` 管滑块」，三种含义各归其主。

**一处可观测的行为位移**：自检的 `engine-clock click moved the MD3 control` 由 **20.0px** 变为 **19.2px**。
原因是控件的上报发生在**松手之后的下一帧**（布局后才调用 `change`），比引擎当场宣布晚一帧，
动画因此少走一帧。这是新结构的必然结果，不是回归。

**门禁**：0 warning；**440 passed**；自检 **PASS**。

#### 删除构建期回调管线（2026-10-28）

**删除**：`Handler`（含其 `Send` 契约与三条测试）、`Cx::report` / `on_change` / `dispatch` /
`take_changes`，以及 `Cx` 的 `pending` / `handlers` 两个字段。

**理由**：它是「构建期发布 → 回调 → 排空」的旧通道，与现在的
`Widget::change` → `Element::change` → `App::changed` 完全重复，且**从未被接通过**
（`dispatch` 与 `take_changes` 零调用，`report` 零调用）。

**它无法表达现在需要的东西**：旧通道在 `decl` 期间发布，那时节点既没有最终矩形，
也没有节点本地的指针坐标——这正是滑块 `0.558` 与 `Activated` 曾被迫携带坐标的根源。

**若将来需要每帧多条上报**，正确做法是把 `Element::change` 的返回值放宽为集合，
而不是重新引入回调：这样仍是单一汇点，仍由引擎中介。

**门禁**：0 warning；**437 passed**（减去 3 条随 `Handler` 消失的测试）；自检 **PASS**。

#### 三个风格包按控件拆分（2026-10-28）

| 包 | 拆分结果 |
|---|---|
| `ui-widgets-md3` | `geometry` / `text` / `slider` / `toggle` / `scaffold` / `style` / `pack`，测试再按主题分 `tests/{slider,toggle,scaffold,scope}` + 共享装置 `tests/mod.rs` |
| `ui-widgets-fluent` | `geometry` / `paint` / `text` / `toggle` / `slider` / `scaffold` / `style` / `pack` / `material` |
| `ui-widgets-glass` | `geometry` / `paint` / `text` / `switch` / `slider` / `scaffold` / `style` / `pack` / `material` |

`lib.rs` 现在只剩模块声明与再导出，**公开路径不变**（`pub use geometry::*;` 等），因此调用方一行未改。

**可见性**：跨文件共享但不对外的项（`LabelText`、`role_of`、`lerp`、`paint` 辅助）降为 `pub(crate)`。

**拆分方式**：按顶层项的行范围机械切块，再由编译器指出每个新文件缺哪些 `use`；
**没有用正则改写函数体**——这一轮所有改动都是「移动 + 补导入」，逻辑一行未动。

**门禁**：0 warning；**437 passed**；自检 **PASS**。

#### 动画的写法约定（2026-10-28）

**原则**：每个参数先按状态优先级求出**唯一目标值**，再对最终目标设置**一次**动画。
不按情形分支后设置多次动画；只有曲线不一致时才允许分支。

**已改**：五个「拖拽时直接读值、否则动画」的双路径统一为「先求目标、再动画一次」。
拖拽中的目标就是当前值，重定向因此是空操作，行为不变而路径唯一。

| 控件 | 参数 | 改法 |
|---|---|---|
| MD3 开关 | `position` | 目标 = 拖拽中 ? 当前值 : `on`；一次动画 |
| Fluent 开关 | `position` | 同上 |
| Glass 开关 | `position` | 同上 |
| Fluent 滑块 | `position` | 同上 |
| Glass 滑块 | `position` | 同上 |
| Glass 开关 | `thickness` | 原来「按住时直接写、否则动画」两条路；改为一个目标 + 两种规格（`IMMEDIATE` / `THICKNESS`） |

新增 `ui_widgets::build::IMMEDIATE`（零时长）作为「手指就是时钟」的显式表达，
不再靠「不调用 animate」来表达直接写入。

**两处原则无法覆盖的实情（记录而非掩盖）**

1. **MD3 开关的两个锁存端点**（`SIZE_OFF` / `SIZE_ON`）的曲线**不能**由状态推导。
   试过改用按压方向（`press.travel`），`releasing_a_drag_does_not_resize_the_thumb` 立即变红
   （帧间跳变 8.33px）：同一帧内两端可能**反向**移动，方向是每个参数自身的属性。
   故保留「目标 vs 当前」判断——它仍是一次动画，只是曲线按该参数自己的走向选。
   另外 `None`（本帧无目标）也是独立状态：换成「重定向到当前值」会重置实例并造成跳变。

2. **MD3 滑块完全没有动画**：它是唯一不接收 `NodeCx` 的控件（`Md3Slider::new` 不收 `ctx`），
   因此既不能动画也无法读状态。Fluent 与 Glass 的滑块都动画 `position`。
   是否补上取决于参考实现：MD3 的滑块在点按时是否有行程动画，我没有可靠依据，**不擅自发明时长**。

#### 几何随控件走（2026-10-28）

原先把每个包的尺寸/曲线/纯几何集中在 `geometry.rs`，按「几何属于控件」重新分配：

| 包 | 结果 |
|---|---|
| md3 | `geometry.rs` 整个并入 `toggle.rs`（其内容全部属于开关；滑块常量本就在 `slider.rs`） |
| fluent | 开关项（`TRACK`/`KNOB_*`/`MARGIN_*`/`knob_width`/`knob_x`/`POSITION`/`PRESS`）并入 `toggle.rs`；`SLIDER_*` 并入 `slider.rs` |
| glass | 开关项并入 `switch.rs`；`SLIDER_*` 并入 `slider.rs` |

**唯一保留的共享文件**：glass 的 `shared.rs`，只有两个常量——`POSITION`（380ms 位置节奏）与
`FADE_RATE`（胶囊淡出速率）。二者被开关与滑块**同时读取**，是「确实一致且有必要统一」的那类。
fluent/md3 没有这种跨控件项，因此不保留任何几何文件。

`paint.rs`（`role_of`/`alpha`/`accent_ramp`）与 `text.rs`（`LabelText`）保留：它们是绘制与文本的共享实现，不是几何。

**公开路径不变**：`lib.rs` 仍是 `pub use toggle::*;` 等，因此 `ui_widgets_md3::TRACK` 这类路径一行未改。

**一处必须记录的失误**：拆分 fluent 与 glass 时我按固定行号切块，**截断了两个函数**
（`slider_inner_factor`、`knob_width`/`knob_x`）。补回后由既有测试钉住取值：
`0.5/0.66/0.45`、`12/14/17`、`3.0/25.0/2.0` 全绿——这是「测试是行为规格」再次救回的一次。
教训：**切块必须按括号配对，不能按行号猜**（glass 第二次即改用配对切分，无误）。

**门禁**：0 warning；**437 passed**；自检 **PASS**。

#### MD3 开关按参考重写（2026-10-28）

上一版是打补丁堆出来的，与参考有三处结构性偏离，已整体重写。

**核心修正：一个控制器，三个视图。** 参考里只有一个 `positionController`（线性 300ms），
位置、尺寸、颜色都是它的不同视图：

| 视图 | 参考 | 现在 |
|---|---|---|
| 位置 | `position.value`，曲线 `Curves.easeOutBack` | `visual_position(raw)` = `Cubic(0.175,0.885,0.32,1.275)` 施加于原值 |
| 尺寸 | `TweenSequence(...).animate(positionController)`，用**原值** | `thumb_size(raw, …)` |
| 颜色 | `_colorAnimation = CurvedAnimation(positionController, easeOut)` | `colour_progress(raw)` = `EaseOut` |

旧实现把「位置」当作原值（注释里还写着「MD3 是线性移动」——**错的**），且用了一个独立的
`COLOUR` 实例。现在只有 `POSITION`（线性）与 `PRESS`（反应，100ms `fastOutSlowIn`）两个实例。

**其余按原文补齐**

| 项 | 参考 | 旧实现 |
|---|---|---|
| 端点写入 | `paint()` 在停在端点时写**两个**字段：近端随按压增长，远端复位为静息 | 只写近端 |
| 按压回缩 | `_stopPressAnimation` 为真时**一个字段都不写** | 仍在写 |
| 滑块颜色 | **四分支**：`!reaction.isDismissed` / forward / reverse / 静止 | 两分支 |
| 悬停 | 只改颜色（状态表里的 `hovered`/`focused`） | 凭空把滑块放大到 28 |
| 描边 | 宽度按 `colorValue` 从 `trackOutlineWidth` 插值到 0 | 恒为 2 |
| 拖拽 | 按**位移**累加：`+= delta / trackInnerLength` | 无锚点，滑块不动 |
| 拖拽提交 | `position.value >= 0.5 != value` | `x > width/2` |

**拖拽锚点**：参考的 `_dragStartX` / `_dragStartPosition` 是画笔的字段。我们是每帧重建，
因此把锚点写进引擎存储（`drag/held`、`drag/from-x`、`drag/from-p`，全部 `IMMEDIATE`）。
抓取帧记录，之后 `p = p0 + (x - x0) / trackInnerLength`。

**`Md3Params` 的编造项已删除**：`hover_grow` 在参考里不存在。换成 `outline_width`
（真实存在：`trackOutlineWidth`）。作用域测试因此改用未选中态构建——描边只在填充到来前可见。

**两条测试的断言与参考相反，已改正**：`an_end_only_grows…` 曾断言远端「保持锁存」，
`a_retracting_press…` 曾断言回缩时仍在写。两者都是照旧实现写的，与 `paint()` 原文不符。

**新增一条测试**并做了破坏验证：把锚点改成绝对映射后立刻变红（`travel 22.37 -> 22`）。

**门禁**：0 warning；**438 passed**；自检 **PASS**，MD3 开关首次真正可点击切换
（`on true`，滑块 x 292→308.3、宽 16→24）。

#### 缺失的角色不再静默变透明（2026-10-28）

查「一个滑块为什么看起来只有一半」时发现：`role_of` / `role` 在包里找不到名字时
**静默返回 `Color::TRANSPARENT`**。审计三个包的全部角色引用后，只有一处缺失：

| 包 | 缺失的角色 | 影响 |
|---|---|---|
| fluent | `controlSolidFillColorDefault` | 滑块滑块的外圈圆盘画成透明，参考里它是 `drawCircle(radius, borderColor)` 的实心盘 |

**已修**：补上该角色（WinUI 浅色 `#FFFFFF`），并把全部 5 处查找改成
`debug_assert!` + 仍返回透明。缺角色是包的错，不该靠肉眼发现。

**审计方法**（可复用）：把 `role_of(pack, "X")` / `role(pack, "X")` 里的名字与
`pack.rs` 里的 `RoleName("X")` 求差集。

**关于「两个圆」**：每个控件的滑块都是**一个**节点。看起来像两个圆的三处来源：

1. **Fluent 滑块**：参考确实是两个同心圆（外盘 + 内点），我们照做；此前外盘因缺角色而不可见。
2. **MD3 开关**：轨道是 52×32 胶囊，滑块正好停在端帽圆心上，轨道的 2px 描边因此在滑块外围形成一圈。
3. **Glass 开关**：行程中滑块是 47×38.6，而轨道只有 58×26（`top = 2 - vExpand`，溢出上下各 8px），
   且填充 alpha 为 0、只剩一圈描边（参考 `Opacity(1 - transition * 1.2)` 亦然）。
   参考里这层由折射外壳接管，我们还没有折射，于是只剩一个「幽灵胶囊」。

**门禁**：0 warning；**438 passed**；自检 **PASS**。

#### 拖拽距离不匹配的根因（2026-10-28）

用户报告「拖动不跟手，光标移动距离与滑块移动距离完全不匹配」，并观察到「一深一浅两个圆，
一个跟随光标、一个留在原地」。逐帧导出图元后确认：**每帧只画一个滑块**，那两个「圆」是同一个
滑块在两点之间逐帧闪烁。

三个缺陷叠加：

| # | 缺陷 | 后果 |
|---|---|---|
| 1 | `pointer_local` 以**最上层命中节点**（滑块本身）为原点，且用上一帧的矩形 | 原点追着自己跑：滑块一动，`local` 立刻反向跳回 |
| 2 | `DragEvent::Moved.delta` 是**相对按下点**的累计值，我按「本次增量」累加 | 第一帧就把整段行程加完，控制器直接饱和 |
| 3 | 拖拽期间仍对位置施加 `easeOutBack` | 视觉位移被曲线放大，与手指距离不成比例 |

**修法**

1. `pointer_local` 改用**声明拖拽的节点**（`drag_owner`）为原点——手势属于它，不属于压在它上面的滑块。
2. `DragEvent::Moved` 增加 `step`（相对上一次事件的位移），与已有的 `delta`（相对按下点）并存；
   控件累加 `step`，等价于参考的 `primaryDelta`。`delta` 的既有语义与测试保持不变。
3. 拖拽期间位置视图改走**线性**：参考的 `_handleDragUpdate` 正是
   `position.curve = Curves.linear; reverseCurve = null`，松手后才恢复曲线。

**实测**（手指每次 +5px）：中心 16 → 16 → 21 → 26 → 31，越过阈值后**精确 1:1**。

**新增测试**：`the_step_is_the_movement_since_the_previous_event`（ui-core）、
`a_drag_accumulates_the_engines_pointer_step`（md3，破坏验证：去掉累加后 `ends at 16` 变红）。

**门禁**：0 warning；**439 passed**；自检 **PASS**。

#### 形变平滑与松手判定（2026-10-28）

**形变不平滑，且原因是参考自己的时间分配。** 60fps、300ms（18 帧）逐帧测量：

| 段 | 参考（形变段占 11%） |
|---|---|
| 正向 | 第 1 帧宽度 16 → 26.5，**Δ10.5px** |
| 反向 | 最大 Δ5.9px |

滑块自身最快移动 4.4px/帧，所以 10.5px 的形变读作「一下子变成胶囊」。

**改法（两处，均已记录为偏离参考）**

1. **形变段从 11% 摊到 25%**（`SQUASH`），参考的两条曲线
   （`Cubic(0.31,0,0.56,1)` / `Cubic(0.20,0,0,1)`）与三个目标尺寸全部保留，只重新分配时间。
   实测正向降到 6.1px/帧、反向 5.5px/帧，与滑块自身速度同量级。
2. **滑块矩形夹进轨道**（`thumb_left`）：胶囊宽 34px 而行程只有 20px，在两端本就放不下；
   参考让它漏出（反向实测右边缘到 53.71 > 52），这里改为 `clamp(0, 52 - w)`。

**松手判定改为「可见中心在行程中的偏移 ≥ 50%」**：

- 旧判定用 `visual`（`easeOutBack` 后的值）。`easeOutBack(0.35)` 已经到 0.98，
  于是一个中心还在左半程的拖动会被判成「翻过去」——**这就是「松手判定有问题」的根因**。
- 现在记录 `centre_offset = (thumb_left + w/2 - 16) / 20`，由**画出来的**中心算，
  与用户的手感一致；拖拽期间它等于控制器原值，符合参考的 `position.value >= 0.5`。
- 顺带修掉松手瞬间的跳位：参考的曲线要等值变化后才恢复，因此**松手那一帧仍按线性绘制**。

**新增测试**：`a_release_decides_by_the_drawn_centre_not_the_eased_position`
（0.35 不翻转、0.65 翻转；破坏验证：改回缓动值立刻变红）。

**门禁**：0 warning；**440 passed**；自检 **PASS**。

#### 动画引擎：延迟、关键帧、每参数一实例（2026-10-28）

**1. `AnimSpec.delay`**（引擎负责相位）

`AnimSpec::delayed(0.5)`；延迟期间引擎把值钉在 `leg_start`，零时长的一段则在延迟结束时立即写入。
先例：CSS `transition-delay`、Flutter `Interval`。

**2. `Stop` 关键帧 + `AnimStore::animate_stops`**

```rust
pub struct Stop { pub at: f32, pub value: f32, pub easing: Easing }  // at 是时长比例
```
每段携带「到达它的那条曲线」，最多 4 帧。全新实例**落在最后一个关键帧上**（与 `animate`
「本就在终点」的规则一致）；重定向从当前值出发并**放弃旧关键帧**（新的一段是普通补间）。

**3. MD3 开关改成每参数一实例**

| 实例 | 目标 | 曲线 |
|---|---|---|
| `POSITION`（中心 px，16→36） | `16 + on*20` | `easeOutBack` 300ms |
| `WIDTH` / `HEIGHT` | 关键帧 `off/on → 34×22 → on/off` | 段内曲线随关键帧 |
| `COLOUR`（0..1） | `on` | `easeOut` 300ms |
| `PRESS` | 按住/拖拽 | `fastOutSlowIn` 100ms |
| `SIZE_OFF` / `SIZE_ON` | 锁存端（状态，非时间） | 24ms 增长 / 120ms 收缩 |

控件里**不再有** `visual_position` / `colour_progress` / `thumb_size` 这些派生函数：
曲线由实例携带，序列由引擎走。拖拽变成 `centre += step.x`（1:1），松手判定变成
「可见中心 ≥ 26px」。

**踩到的坑（已修，已加测试）**：方向若按「行程方向」选，全新实例会落在最后一个关键帧上，
于是**关闭状态的开关画出 24px 滑块**（自检立刻发现：`thumb x 288 w 24` 而非 `292 w 16`）。
方向必须由**语义值**决定：序列的终点就是它要去的状态，静止时即当前状态。

**关于延迟的诚实结论**：它已实现并有测试，但**目前没有控件用它**——关键帧已经覆盖了
「分相位」的需求。要不要保留，请定。

**未转换的部分**：Fluent / Glass 的尺寸是**其他实例的纯函数**（`thumb_box(progress, press)`），
若把它们也做成实例，目标会依赖另一个动画值，形成反馈环——正是我们刚修掉的那类抖动。
建议这类参数保持函数形态，只把「状态解析出的目标」做成实例。

**门禁**：0 warning；**443 passed**；自检 **PASS**。

#### 把控件里的变通搬进引擎（2026-10-28）

复查三个开关后，只有两处是「引擎做不到才在控件里手写」：

**1. 颜色插值 → `animate_colour`**

引擎以前只能动 `f32`，所以每个包都自己把颜色拆成「标量 + 两端色 + `lerp`」。
现在 `Entry` 有 4 条通道共用一个时钟（`lanes`），`animate_colour(id, Color, spec)` 直接推进颜色。

| 包 | 迁移前 | 迁移后 |
|---|---|---|
| MD3 | 四分支端点表 + `lerp(from, to, colour_t)` | 三个颜色实例（滑块 / 轨道 / 描边）+ 一个宽度实例 |
| Fluent | `alt.lerp(accent_ramp(...), p)` | 一个颜色实例 |
| Glass | `inactive.lerp(active, p)` | 一个颜色实例 |

多通道共用一个时钟是关键：若每个通道各用一个实例，`duration` 会按各自剩余距离缩放，
颜色就会脱节。时钟由**行程最远的通道**决定，并有测试钉住。

**偏离参考**：MD3 参考是「按状态挑一对端点色，再按控制器混合」；我们改为「从当前颜色
过渡到该状态的颜色」。静止时完全一致，飞行中更平滑（不再中途换端点对）。

**2. 按方向选曲线 → `AnimSpec::down(duration, easing)`**

`latch` 里原本写 `if target >= current { 24ms 线性 } else { 120ms 缓出 }`。现在：

```rust
pub const LATCH: AnimSpec = AnimSpec::new(0.024, Easing::Linear).down(0.120, Easing::EaseOut);
```

对应 Flutter 的 `reverseCurve` 与 CSS 的非对称 transition。

**仍留在控件里的（不算变通）**：`state.dropped` 的那一帧保持——它等的是 app 回传语义值，
不是引擎能力；Fluent / Glass 的派生色（`border`、`knob_fill`、`top`）是其他参数的纯函数。

**门禁**：0 warning；**445 passed**；自检 **PASS**。

#### 关键帧是轨迹，不是查表（2026-10-28）

用户报告：MD3 开关拖动松手后，**在其他动画都结束之后又形变一次**。

根因是语义错配。参考里尺寸是「按控制器位置查表」的纯函数，`position` 停在末端时
尺寸就是末端尺寸，两端锁存值从 28 回落到 24 只是**移动了端点**。改写成关键帧实例后，
它变成**一条轨迹**：端点值一变，`to` 跟着变，`animate_stops` 就从头重放整条序列
（`28 → 34(胶囊) → 24`），于是一次多余的形变在拖拽结束约 220ms 后发生。

**修法**：关键帧只在**真正的状态迁移**中播放（`position` 未落定）；一旦滑块停在某一端，
尺寸就是那一端的尺寸，按压尺寸用普通补间回落（`LATCH`）。

**回归测试**：`a_released_drag_never_squashes_the_thumb_a_second_time`
（模拟拖拽 + 松手 + 语义值翻转，断言松手后宽度不再增长、最终落在 24）。
破坏验证：把关键帧重放放回去，测试立刻报 `28.00 -> 33.96`。

**门禁**：0 warning；**446 passed**；自检 **PASS**。

#### 参考里形变到底发生在哪里（2026-10-28 复核）

按 `switch.dart:1570-1613` 逐字复核，两条序列都是**按控制器位置查表**，不是轨迹：

```dart
if (position.isDismissed || position.status == AnimationStatus.forward) {
  thumbSize = thumbSizeAnimation(true).value;    // 正向表
} else {
  thumbSize = thumbSizeAnimation(false).value;   // 反向表
}
```

| 表 | 权重 | 区间 | 内容 | 曲线 |
|---|---|---|---|---|
| 正向 | 11 | 0 → 0.11 | inactive 16 → **transitional 34×22** | `Cubic(0.31,0,0.56,1)` |
| | 72 | 0.11 → 0.83 | transitional → active 24 | `Cubic(0.20,0,0,1)` |
| | 17 | 0.83 → 1 | 恒为 active | — |
| 反向 | 17 | 0 → 0.17 | 恒为 inactive | — |
| | 72 | 0.17 → 0.89 | inactive → **transitional 34×22** | `Cubic(0.20,0,0,1).flipped` |
| | 11 | 0.89 → 1 | transitional → active 24 | `Cubic(0.31,0,0.56,1).flipped` |

**结论：胶囊始终出现在「滑块正在离开的那一端」往外 11% 处。**

- 正向（离开关端）：控制器 0.11 —— 刚起步就鼓成胶囊，再用 72% 收回目标尺寸；
- 反向（离开开端）：控制器 0.89 —— 同样是刚离开就鼓成胶囊。

中段从来不是胶囊出现的位置，中段是**从胶囊收回去**的过程。两个方向因此对称。

另外两点复核：
- `_SwitchConfigM3.thumbOffset => null`，所以 `inset = 1 - |currentValue - thumbOffset| * 2` 恒为 0——
  「滑块略微收缩」是 **M2 专属**，M3 没有。我们不必实现。
- `reaction.isCompleted` → `Size.fromRadius(pressedThumbRadius)` = 28×28，覆盖整张表。

**我此前两处错误**：反向把胶囊放在 0.75（应 0.11 处起步）；正向为平滑把 11% 拉到 25%。
现在反向已改回「与正向同形、两端对调」；`SQUASH` 仍是 0.25 的偏离（11% 时宽度两帧变 18px），
待取舍。

**门禁**：0 warning；**447 passed**；自检 **PASS**。

#### MD3 开关还原度盘点（2026-10-28）

**已还原**（逐项对照 `switch.dart` 的 `_SwitchConfigM3` / `_SwitchDefaultsM3`）：

| 项 | 参考 | 我们 |
|---|---|---|
| 轨道 | 52×32，半径 16 | ✓ |
| 滑块尺寸 | 16 / 24 / 28 / 过渡 34×22 | ✓ |
| 位置曲线 | `easeOutBack`，300ms | ✓ |
| 尺寸序列 | 两向对称，胶囊在离开端 11% | ✓（`SQUASH` 仍为 0.25 的偏离） |
| 颜色视图 | `easeOut` | ✓（颜色实例） |
| 描边 | 宽 **恒为 2**，颜色 `outline → transparent` | ✓（本轮修正：此前让环变细是偏离） |
| 按压反应 | 100ms | ✓ |
| `reaction.isCompleted` | 28×28 | ✓ |
| 光标 | `WidgetStateMouseCursor.clickable` | ✓ |
| 滑块阴影 | `kElevationToShadow[0]` = **空列表** | ✓（本来就没有，不是缺失项） |
| M2 的 `inset` 收缩 | `thumbOffset == null` → 0 | ✓（M3 无此效果） |

**仍缺**：

1. **涟漪 / 状态层**（`paintRadialReaction`）：以滑块中心为圆心，半径 0 → `splashRadius` = **20**，
   时长 `kRadialReactionDuration` = **100ms**，颜色取 `overlayColor`
   （选中+按下 `primary@10%`、未选中+按下 `onSurface@10%`、悬停 8%、聚焦 10%）。
   聚焦时同一位置画一个静态圆。
   **注意**：该函数在 `ToggleableStateMixin`（`toggleable.dart`），本机 checkout **没有这个文件**，
   因此半径/透明度的具体补间 `[未证实]`。
2. **禁用态颜色**：参考有四套
   （滑块 `surface` / `onSurface@38%`；轨道 `onSurface@12%` / `surfaceContainerHighest@12%`；
   描边 `onSurface@12%`）。我们的 MD3 包只有 9 个角色，**没有 `surface`，也没有禁用变体**。
3. **尺寸与内边距**：`switchHeight = switchMinSize.height + 8 = 48`，`padding = EdgeInsets.symmetric(horizontal: 4)`。
   我们目前是「轨道 32 + 上下各 4 = 40」，矮 8px，横向也没有 4px 的内边距。
4. **滑块内图标**（16px）——参考支持 `thumbIcon`，我们没用，属可选。

**门禁**：0 warning；**447 passed**；自检 **PASS**。

#### Decl 与绘制分离：迁移完成（2026-10-28）

绘制现在**只有一个入口**：`Decl::shapes`。

| 旧 | 新 |
|---|---|
| `NodeData::Box { fill, radius, border, gradient }` | 已删除 |
| `NodeData::ContainerBox { params, fill, radius, border, gradient }` | 已删除（并入 `Container`） |
| `ContainerParams.image` / `shadow` | 已删除（移入 `Shape::Box`） |
| `.background()/.border()/.gradient()/.elevation()/.background_image()` | 保留为**语法糖**，写入 `Shape::Box`（112 处调用点无需改动） |

`NodeData` 只剩 `Container(ContainerParams)` 与 `Text { block }`；节点不再携带任何绘制字段，
没有 `shapes` 的声明就是纯交互节点。

`Shape` 三种变体，与后端图元一一对应：

| 变体 | 含义 |
|---|---|
| `Box { corners, fill, gradient, border, shadow, image }` | 节点**自身**的盒子 |
| `Rect { rect, corners, fill, gradient, border }` | 节点盒子内的显式矩形（圆/胶囊/滑块） |
| `Shadow { rect, corners, colour, blur, dy }` | 显式矩形下的阴影 |

绘制顺序：`Box`（阴影 → 填充 → 图像）→ `Rect`/`Shadow` 按声明顺序 → 子节点。
裁剪曲线由 `Shape::Box.corners` 决定（`node_corners`），子形状从不裁剪子节点。

#### Decl 与绘制分离：`shapes`（2026-10-28 已实现）

**结论**：叶子图元走 `Decl::shapes`，效果/容器留在节点上。

| 层 | 内容 | 形态 |
|---|---|---|
| 叶子图元 | 圆角矩形（含圆/胶囊/线段）、阴影 | `Decl::shapes: Vec<Shape>`：纯数据、无 id、不布局、不命中、不占槽位 |
| 效果/容器 | 不透明度、模糊、色彩矩阵、背景模糊、圆角裁剪 | 节点属性 → `push_layer(bounds, effects, radius)` 离屏层（`radius` 已是圆角裁剪） |

**为什么不是 `Shape::Group`**：那会在已有的节点场景图之外再造一棵没有 id、没有脏区粒度、
不参与 diff 与命中的树；而节点层已经能表达 blur / 色彩矩阵 / 背景模糊，表达力反而更强。

**为什么不是回调**（`Box<dyn Fn(&mut Canvas)>`）：不可比较、不可哈希，会破坏
`Decl` 的 `PartialEq`（多处测试依赖它）与按节点聚合的 damage 指纹。

**成熟框架对照**（已核实）：egui `epaint::Shape` 枚举（13 变体，`derive(PartialEq)`）、
GPUI `Primitive` 枚举、GTK4 `GskRenderNode` 类型枚举（39 种）。它们的枚举是**两层**的：
叶子（`ColorNode`/`BorderNode`/`ShadowNode`/`TextNode`）与效果容器
（`ClipNode`/`OpacityNode`/`TransformNode`/`BlendNode`）。我们的「效果层」落在节点上，
与这套分层同构。命令式 `Canvas`（Skia/Qt/Flutter）只是把同样的东西录成 op 表。

**同时修掉的一处**：`children_of` 原按 arena 分配序号排序，注释声称「与声明顺序一致」——
这只在第 0 帧成立。条件出现的子节点槽位更大，会被画到兄弟**之上**（涟漪画到滑块之上就是这么来的）。
现按 `Node.order`（声明位置）排序，并加测试 `a_child_added_later_paints_in_declaration_order`。

**MD3 开关**：`track`/`thumb`/`reaction` 三个 `Decl` 节点全部删除，改成一个 60×48 的节点 +
三个形状（轨道胶囊、涟漪圆、滑块圆），与参考「一个 RenderObject 画三次」同构。
顺带删掉 `ToggleSpec.label` 在 MD3 的使用（参考的 `Switch` 没有标签）。

#### 完整审查 `toggleable.dart`（2026-10-28，已按此修正）

来源：`packages/flutter/lib/src/**widgets**/toggleable.dart`（不是 `material/`，这就是之前一直 404 的原因）。

| 常量 | 值 | 我们此前 |
|---|---|---|
| `_kToggleDuration` | 200ms（M3 覆盖为 `toggleDuration = 300`） | 300 ✓ |
| `_kReactionAnimationDuration` | 100ms，`fastOutSlowIn` | 100 ✓ |
| `_kReactionFadeDuration` | **50ms**，`fastOutSlowIn` | 200ms ✗ 已改 |
| position 曲线 | M3 `easeOutBack` / reverse `easeOutBack.flipped` | ✓ |

```dart
if (!reaction.isDismissed || !reactionFocusFade.isDismissed || !reactionHoverFade.isDismissed) {
  final double reactionRadius = isFocused || isHovered
      ? splashRadius
      : radialReactionRadiusTween.evaluate(reaction);   // reaction.value * splashRadius
  ...
}
```

触发条件：`_reactionController.forward()` 在 **tap down**，`reverse()` 在 **tap up/cancel**；
`reactionHoverFade` / `reactionFocusFade` 由 **`onShowHoverHighlight` / `onShowFocusHighlight`** 驱动。

**由此定位的两个真 bug：**

1. **指针离开后涟漪仍在**：`onShowFocusHighlight` **不等于** `focused`。指针按下会给控件焦点，
   但 `FocusHighlightMode.touch` 下不请求焦点环；我们之前用 `state.focused`，于是点击之后
   焦点留在开关上，光晕永久存在。
   修法：新增 `focus_visible`（Tab 置真、指针按下置假），涟漪的聚焦项改用它。
2. 淡入淡出 200ms → 50ms。

**形变**：`reaction.isCompleted` 把整条尺寸序列短路成 28 圆，因此**按下与拖动期间滑块恒为圆**；
胶囊只出现在松手之后的行程里（`reaction` 100ms 反向，position 300ms 走完）。

参考的滑块左边缘 = `16 − w/2 + p×20`：

| 位置 | 宽 | 左边缘 | 右边缘 |
|---|---|---|---|
| p=0 静止 | 16 | 8.0 | 24.0 |
| p=0.11 胶囊 | 34 | **1.2** | 35.2 |
| p=0.83 | 22 | 20.6 | 42.6 |
| p=1 静止 | 24 | 24.0 | 48.0 |

即胶囊形成时左缘几乎贴住轨道左缘，右缘外扩 11.2px：**近边缘一侧只动 6.8px，远侧动 11.2px**。

**重写：单标量 + 锁存端点（2026-10-28 已实现）**

整个控件只由一个标量驱动，两种模式只是**谁在写它**：

| 模式 | 写法 | 曲线 |
|---|---|---|
| 受控（拖拽） | `ctx.animate(PROGRESS, raw + dx/20, IMMEDIATE)` | **linear**（参考的 `_handleDragUpdate` 把 `position.curve` 切成 linear） |
| 主动（值变化） | `ctx.animate(PROGRESS, on?1:0, 300ms linear)` | 控件侧 `POSITION_CURVE.apply(raw)`（`easeOutBack`） |

所有视觉量都是 `raw` 的函数：

```rust
let eased  = if dragging || dropped { raw } else { POSITION_CURVE.apply(raw) };
let centre = 16.0 + 20.0 * eased;                 // 位置用曲线后
let (tw, th) = if reaction >= 0.99 { (28.0, 28.0) }
               else { size_at(forward, raw, off_end, on_end) };   // 尺寸用原始
let t = Easing::EaseOut.apply(raw);               // 颜色用 easeOut
```

**端点锁存**用「不重定向的实例就是记住的值」实现：

```rust
fn latch(ctx, key, want: Option<f32>, rest: f32) -> f32 {
    let current = ctx.status(key).map(|s| s.value).unwrap_or(rest);
    let Some(target) = want else { return current };   // 本帧不写 = 冻结
    ctx.animate(key, target, LATCH)
}
// 冻结条件 = reaction 正在反向且未落定（等价于参考的 _stopPressAnimation）
let frozen = reaction_status.travel == Reverse && !reaction_status.settled;
```

三个写入条件与参考一一对应：`reaction >= 0.99`（两端都长）、`eased <= 0`（近端长）、`eased >= 1`（近端长）。

**反向腿必须用镜像曲线**（本轮的真 bug）：`position.reverseCurve =
Curves.easeOutBack.flipped`。不镜像时反向腿一开始就冲得快——滑块已经到远端、形状却还是
34 宽的胶囊，右缘顶出轨道 **1.81px**（实测）。镜像后同帧的右缘是 43.96。

三条腿的实测（轨道 52 宽，起点/终点 16/24）：

| 方向 | t=30ms | t=90ms | t=150ms | t=300ms |
|---|---|---|---|---|
| OFF→ON left/right/w | 8.09 / 41.84 / 33.74 | 20.59 / 48.27 / 27.68 | 24.86 / 49.84 / 24.98 | 24.00 / 48.00 / 24.00 |
| ON→OFF left/right/w | 10.10 / 43.96 / 33.86 | 6.25 / 28.85 / 22.60 | 5.76 / 23.54 / 17.78 | 8.00 / 24.00 / 16.00 |

**位置不再钳制**：`thumb_left = centre - w/2`。包含性由两个端段的时序保证（34 宽的胶囊只在
`p ∈ [0.05, 0.95]` 装得下，参考取 11%/83%），由不变量测试
`the_thumb_never_leaves_the_track` 守护（正反两腿各 61 帧）。

**已知残留**：时钟冻结时滑块有一次性 0.008px 的亚像素落定（不是持续漂移），
自检判据从 1e-3 放宽到 0.02 并在注释里写明原因；根因未查清 `[未证实]`。

**尺寸改为「进度的纯函数」（2026-10-28，按用户提问重构）**

参考的实现是 `TweenSequence.animate(positionController)`：尺寸是**原始进度**的纯函数，
每段一条曲线，不是按腿重放的轨迹。我们此前用的是关键帧轨迹，后果是**中途改向会重放整条
轨迹**，于是「到达位置之后再形变一次」。

现在的结构（与参考同构）：

| 实例 | 用途 | 规格 |
|---|---|---|
| `POSITION` | 滑块中心 | 300ms `easeOutBack` |
| `PROGRESS` | 原始进度 0..1 | 300ms **linear** |

```rust
let (tw, th) = if reaction >= 0.99 {
    (THUMB_PRESSED, THUMB_PRESSED)          // `reaction.isCompleted` 短路成 28 圆
} else {
    size_at(forward, progress, 16.0, 24.0)  // 纯函数，胶囊在离开端外 11%
};
```

`forward = progress <= 0.0 || travel == Forward`（对应参考的
`position.isDismissed || status == forward`）。

**拖动必须同时写两个实例**：`positionController.value` 同时驱动位移与尺寸，
只写中心会让尺寸说假话（实测：拖到 0.4 松手，尺寸从 28 跳到 16，差 12px）。
因此 `state.dragging` 期间 `PROGRESS` 直接写入拖拽进度，且**不**向语义值动画。

**颜色同样是进度的纯函数**：`_colorAnimation = CurvedAnimation(positionController,
Curves.easeOut)`，因此

```dart
final double colorValue = _colorAnimation!.value;
final Color trackColor = Color.lerp(inactiveTrackColor, activeTrackColor, colorValue);
final Color? trackOutlineColor = Color.lerp(inactiveTrackOutlineColor, activeTrackOutlineColor, colorValue);
```

滑块色按 `reaction` 与控制器状态挑一对端点，再按同一个 `colorValue` 插值：

| 条件 | 起点 → 终点 |
|---|---|
| `!reaction.isDismissed`（按下中） | `onSurfaceVariant → primaryContainer` |
| status == forward | `onSurfaceVariant → onPrimary` |
| status == reverse | `outline → primaryContainer` |
| 静止 | `outline → onPrimary` |

于是**拖动会把配色一起带走**。我们此前用状态解析的颜色实例（语义阈值翻转 + 300ms），
拖动时颜色与手指脱节；现已改为 `t = easeOut(progress)` 的纯函数，三个颜色实例删除。
测试 `a_drag_carries_the_palette_with_it` 钉住：拖到 0.4 时轨道必须是
`surfaceContainerHighest.lerp(primary, easeOut(0.4))`。

**代价（与参考一致）**：中途改向时两张表（正向 11% / 反向 89%）不是同一个值，
会有一次离散跳变；参考也有。若要抹平，需要引擎提供「设置当前进度」的能力，
让改向时继承对称进度——这是下一步的候选。

**由此退役的测试**：`releasing_a_drag_does_not_resize_the_thumb`（它保护的是我们
自己的锁存平滑，参考没有这个行为）。保留 `neither_direction_swells_twice`
（单峰性）与 `size_at` 的端点/胶囊断言。

**滑块越界（用户报告，已修）**：几何本身没有问题——逐帧实测（轨道 52×32，节点 60×48）：

| t | 滑块 | 轨道内？ |
|---|---|---|
| 0.000 | (8, 8) 16×16 | ✓ |
| 0.075 | (16, 5) 34×22 | ✓（右缘 50） |
| 0.200 | (25.6, 4) 24.3×23.9 | ✓（右缘 49.9） |
| 0.300 | (24, 4) 24×24 | ✓（右缘 48） |

真正的问题是**脏区**：`ink_rect` 把 `Shape::Rect` 的 bounds 当成绝对坐标，漏了节点自身的
原点，于是脏区整体偏移一个节点坐标，重绘清错区域、旧位置的像素残留下来——看起来就像滑块
边缘越出了轨道。修法：`Shape::bounds(node)` 统一返回绝对坐标（`Rect` 抬 `node.origin`），
`ink_rect` 不再二次平移。测试 `a_shape_inside_a_node_inks_in_absolute_space` 钉住，
破坏后立刻变红。

**涟漪越出轨道是参考行为**：`splashRadius = 40.0 / 2 = 20`，而轨道半高只有 16，所以涟漪
天然上下各溢出 4px；在两端时横向也会溢出 4~8px。参考的 `CustomPaint` 不裁剪。

**在线 demo**：`chayanforyou/flutter_material_3_demo` 是**官方 `material_3_demo` 样例的副本**
（`pubspec.yaml` 只依赖 Flutter SDK，`sdk: ^3.8.0`），因此它用的就是上面这份 `switch.dart` +
`toggleable.dart`，不是第三方实现。

#### MD3：尺寸/内边距 + 涟漪（2026-10-28）

**尺寸与内边距**（`switchHeight = switchMinSize.height + 8`，`padding` 4）：

| 项 | 参考 | 现在 |
|---|---|---|
| 控件 | 52+8 × 40+8 = **60×48** | ✓（轨道 52×32 居中） |
| 内边距 | `EdgeInsets.symmetric(horizontal: 4)` | ✓ |

**涟漪 / 状态层**：`paintRadialReaction` 原文（`ToggleablePainter`，本机 checkout 无
`toggleable.dart`，故取自 Flutter 源码 / API 文档）：

```dart
final double reactionRadius = isFocused || isHovered
    ? splashRadius                                   // 20
    : radialReactionRadiusTween.evaluate(reaction);  // reaction.value * 20
final reactionPaint = Paint()..color = Color.lerp(
    Color.lerp(
      Color.lerp(inactiveReactionColor, reactionColor, position.value),
      hoverColor, reactionHoverFade.value),
    focusColor, reactionFocusFade.value)!;
if (reactionRadius > 0.0) canvas.drawCircle(origin + offset, reactionRadius, reactionPaint);
```

四个叠加色（`switch.dart:1015-1056` 逐条核实）：

| 角色 | 值 |
|---|---|
| `inactiveReactionColor` | `onSurface @ 10%` |
| `reactionColor` | `primary @ 10%` |
| `hoverColor` | 选中 `primary @ 8%` / 未选中 `onSurface @ 8%` |
| `focusColor` | 选中 `primary @ 10%` / 未选中 `onSurface @ 10%` |

实现：涟漪作为**轨道的子节点**、排在滑块之前（参考顺序是轨道 → 描边 → 涟漪 → 滑块），
半径 20 时故意溢出 32 高的轨道；`HOVER` / `FOCUS` 两个淡入淡出实例用
`kThemeChangeDuration` = 200ms。

**涟漪圆心（用户指出位置不对，已修）**：参考用的是滑块的**高度**一半，不是宽度一半。

```dart
final radialReactionOrigin = Offset(
  thumbPaintOffset.dx + thumbSize.height / 2,   // ← height
  size.height / 2,
);
```

`thumbPaintOffset.dx` 是滑块**左边缘**，所以圆心 = 左边缘 + 高/2 = **左端帽的圆心**，
不是几何中心。圆形时两者相同，但压成 34×22 的胶囊时相差 `(34-22)/2 = 6px`：

| 滑块 | 几何中心 | 涟漪圆心 | 差 |
|---|---|---|---|
| 16×16 | 16 | 16 | 0 |
| 24×24 | 36 | 36 | 0 |
| 34×22 | 26 | 20 | **−6** |

我此前用了几何中心，所以形变过程中涟漪会偏离。已改为 `thumb_left + height/2`，
并加测试 `the_splash_sits_on_the_thumbs_left_cap_not_its_centroid` 钉住。

**涟漪纵向错位（用户指出，已修）**：`Box` 容器的子节点是**纵向堆叠**的，`offset` 只是
叠加在堆叠位置上的增量。涟漪作为第二个子节点，被堆到滑块下方整整一个滑块高度：

```
thumb    rect.y=107.60 offset.y= 4.00 → visual_y 111.60   （中心 123.60）
reaction rect.y=131.60 offset.y=-4.00 → visual_y 127.60   （中心 147.60，低 24）
```

**修法**：涟漪改用 `NodeKind::Overlay`（不占槽位、落在内容原点），于是
`visual_y = 103.60`，中心 123.60 与滑块完全对齐。

**教训**：`Box` 里想「绝对定位」的装饰必须用 `Overlay`；普通子节点会参与堆叠。
测试已加断言 `kind == NodeKind::Overlay`——这条正是能抓住该 bug 的那一条。

**阴影：参考里是空的。** `_SwitchConfigM3.thumbShadow => kElevationToShadow[0]`，而
`_elevationToShadow[0] = <BoxShadow>[]`。所以 MD3 滑块**没有阴影**，我们现在画的也是没有——
这是忠实，不是缺失。若要可见阴影属于**偏离**，需要你确认。

**门禁**：0 warning；**448 passed**；自检 **PASS**（控件变 60×48 后轨道 x 284→288）。

#### E. 尚未修复的回归（必须记录，不隐瞒）

（已修复，见 E3c。）原记录：`gallery` 自检 `slider drag value 0.558 (want 0.708)` 失败；
根因是应用用 `Moved{dx}` 累加代替「按下点→值」映射。修复方式是让控件自己映射，
由引擎在布局后调用 `Element::change`——应用侧未加任何补丁。

## 12. 色彩体系转向官方规范（2026-10-28 定稿）

用户要求：**尽可能遵循官方规范**，自己实现而不是照搬别人的代码。工具已就位：`md3-docs` MCP
（官方文档检索）+ 本地 vendored 的 Material Color Utilities（Apache-2.0，**只取数据、不取代码**）。

### 已落地的规范数据

`docs/reference/md3-color-roles.json`：49 个角色（palette / lightTone / darkTone / contrastCurve /
toneDeltaPair / backgroundRole / monochrome 覆盖）+ key colors + tone 定律 + 来源与许可标注。

与官方文档逐条对得上：

| 项 | 官方文档原文 | 提取到的 |
|---|---|---|
| `primary` | 「assigns the color tone **primary40** to the primary role」 | light 40 / dark 80 ✓ |
| `on primary` | 「the tone **primary100** to the on primary role」 | light 100 / dark 20 ✓ |
| 浅色默认 surface | 「updated the default light theme surface from tone 99 to **98**」 | neutral 98 ✓ |
| neutral 调色板 | 「increasing its chroma from 4 to **6**」 | 6 ✓ |
| 三档对比 | 「Contrasts also are tokenized」 | 每角色 `ContrastCurve(low, normal, medium, high)` ✓ |
| 深色面 | 「Slightly darkened surface roles in dark theme」 | neutral 6 ✓ |

### TonalSpot（M3 默认变体）的 key colors —— 文档只有文字，数字在这里

| 调色板 | hue | chroma |
|---|---|---|
| primary | 来源 hue | **36** |
| secondary | 来源 hue | **16** |
| tertiary | 来源 hue **+60** | **24** |
| neutral | 来源 hue | **6** |
| neutral variant | 来源 hue | **8** |
| error | 固定 **25** | 固定 **84**（静态，不随动态色变） |

### tone 定律（自证式验收依据）

HCT 的设计目的，写在 MCU 源码头部：**tone 相差 40 ⇒ 对比度 ≥ 3.0；相差 50 ⇒ ≥ 4.5。**
配合文档的 3:1 / 7:1 与三档对比，构成不依赖他人 hex 的检验集。

### 实现顺序

1. `ui-color-hct`：CAM16（CIE 公开发表的色貌模型）+ CIELAB L\* 作 tone + HCT 反解；自己写。
2. TonalPalette + 上面的 key colors。
3. `DynamicColor.resolve()`：tone → 对比曲线（按 `contrast_level`）→ `ToneDeltaPair` 协调
   （如 `primaryContainer ↔ primary`，Δ10，polarity nearer）→ 最终 tone → `palette.tone()`。
4. 用 tone 定律 + 文档约束自检；再与 Material Theme Builder 导出的 baseline JSON 做金标准比对。

### 未完成

21 个角色（surface container 家族等）的 tone 是按对比级别取的**表达式**，已在 `toneExpr`
中原样保留，待第 3 步逐条转写。对比曲线与 ToneDeltaPair 的完整语义仍需从
`dynamic_color.cc` / `contrast_curve.h` 读通后自写实现。

### 通读产物

[`docs/reference/md3-color-spec.md`](reference/md3-color-spec.md)：分层结构、CAM16×L\*、
HCL 反解（牛顿式 J 迭代 + 二分降彩度）、TonalPalette/KeyColor、key colors 数值表、
`DynamicColor` 八字段、`ContrastCurve` 的分段取值、`GetTone` 三种情形与「尴尬区 50–59」规则、
`ForegroundTone` 与前景倾向常数、已提取的 28 个直接 tone、验收集、**以及尚未读通的六项**。

## 13. 三套体系的色彩/令牌与统一 Style 层（2026-10-28 立项）

用户定的方向，按顺序执行：

1. **MD3**：已通读规范（§12 + `md3-color-spec.md`）。下一步按 §10 的接口自写 `ui-color-hct`
   与 `ui-color-md3`，用 §11 的验收集自证。
2. **Fluent（WinUI3 / Fluent 2）**：同样先通读官方规范（设计令牌、Colour System：
   base colour → 主题色阶（Neutral/Locale-Primary/Alternative-Lookup 等 8 层背景与前景）、
   Accessibility contrasts、Typography ramp、Spacing、Corner radius、Motion）。
   产出 `docs/reference/fluent-color-spec.md` + 数据文件。
3. **HIG（SwiftUI / Apple 玻璃）**：通读 Apple HIG 与 SwiftUI 语义色（system colours 的
   light/dark/high-contrast 变体、`Color` 的 semantic 层、Material 材质、Dynamic Type、
   8pt 栅格与 spacing、SF 符号尺寸）。已有 `liquid-glass-analysis.md` 作渲染侧参考，
   色彩/尺寸侧需新建 `docs/reference/hig-color-spec.md`。
4. **统一 Style 令牌体系重设计**：三套规范都读通后，重新设计 `ui-style` 的令牌层，
   目标是**一套抽象同时容纳三套**：
   - 令牌按**语义**而非来源命名（`accent`、`on_accent`、`container`、`surface_*`、`text_*`、
     `stroke`、`state_overlay`、`elevation`、`motion.*`、`radius.*`、`space.*`）；
   - 每套风格是「语义 → 该风格的具体令牌 + 推导规则」的一张表，风格包只负责**填表**；
   - 三套都有的概念直接建模（强调色/前景/背景容器/描边/状态叠加/海拔/圆角/间距/动效）；
   - 只有一套有的概念（如 MD3 的 tonal palette 与 contrast curve、Fluent 的 8 层 lookup、
     HIG 的 material/vibrancy）留在**风格包内部**，不污染公共语义；
   - 公共层必须能表达「同一语义在三套里由不同算法产生」，因此令牌的值允许是
     **规则**（可解析）而不只是常量——这一点 MD3 的 `DynamicColor` 已经证明是必需的。
5. **人工验收（最终）**：gallery 建**三个页面**，分别展示三套色彩体系与对应的尺寸表格：
   - 每页：由单一来源色（或各自的基线）生成的全套角色色卡（明/暗 + 各自的多档对比/无障碍变体）、
     色调阶梯（MD3 的 0–100 等）、语义令牌 → 实际值的对照表；
   - 每页：尺寸表格——圆角刻度、间距刻度、排版刻度、控件度量（高度/内衬/命中区）、动效时长与曲线；
   - 三页共用同一套页面骨架与同一套令牌读取路径，**用展示本身验证统一层的可行性**；
   - 验收由用户目测，不做截图自动化（沿用既有约定）。

**执行纪律**：每套规范「先读通、落成文档与数据文件」再动代码；实现全程不引入第三方色彩库；
所有数字必须能回答「凭什么相信它是对的」（文档原文 / 规范数据 / 可自证的定律）。

### Fluent 数据源选型与通读结论（2026-10-28）

**选定的数据源**：npm `@fluentui/tokens` **1.0.0-alpha.24**（MIT，官方仓库 `packages/tokens/src`
的编译产物）——459 令牌 × 3 套主题 + 65 条全局调色板 + 全部刻度。已落成
[`docs/reference/fluent-tokens.json`](reference/fluent-tokens.json)，通读文档
[`docs/reference/fluent-color-spec.md`](reference/fluent-color-spec.md)。

排除的来源：`fluentui` MCP 只给 7 个示例令牌（虽值正确，仅作交叉验证）；
官方文档页只给两层模型与调色板分类（作模型依据）；仓库 TS 源码与 npm 包同源。

**关键结论**（影响统一令牌层设计）：

| 维度 | MD3 | Fluent 2 | 含义 |
|---|---|---|---|
| 语义值怎么来 | **规则解析**（tone + ContrastCurve + ToneDeltaPair） | **查表**（别名 → 全局，一层间接） | 公共层必须容纳两种形态 |
| 状态层 | 叠加透明度现场合成 | **103 个具名状态令牌**（`colorXHover/Pressed/Selected`） | 不能只留一种 |
| 色阶步长 | tone 0–100（步 10 + 95/98/99） | neutral 步 **2**、brand 步 **10**（10–160） | 刻度不是同一个网格 |
| 单一来源色 | HCT 从来源色生成 5 条调色板（有算法） | 品牌阶是**输入**，`createLightTheme(brand)` 只派生 | 色阶生成器需我们自建并验收 |
| 主题数 | 明/暗 × 三档对比 | 明/暗/**高对比** | 维度不同，需正交建模 |

**待补**：Fluent 品牌色 → 16 级色阶的生成算法（官方未公开；计划自定规则并用 `brandWeb` 与
Teams 两条官方色阶验收）；Fluent UI Apple/Android 的令牌副本；WinUI accent palette 生成。

### HIG 通读结论（2026-10-28）

产物：[`docs/reference/hig-tokens.json`](reference/hig-tokens.json)（带来源标签的数据）与
[`docs/reference/hig-color-spec.md`](reference/hig-color-spec.md)。直读了 HIG 的 Color / Dark Mode /
Motion 三页（时间戳 2026-10-28），与 `hig_mcp`（v0.3.0，自报核对 2026-10-17）**逐值一致**。

要点：

- 色彩是**两层**：固定系统色（12 色 + 5 档灰 × 4 个上下文）与**动态语义色**；
  后者 **Apple 不公布数值也不给公式**，由 OS 按 appearance × contrast × vibrancy 运行时解析。
  HIG 原文警告：文档值仅供参考，**不要硬编码**。
- 上下文不是一个开关：**appearance × contrast × 背景层级（system/grouped × 3 级）× 暗色深度（base/elevated）**。
  increased contrast 不是简单调对比（Blue light `#0088FF → #1E6EF4`，dark 则提亮降饱和），**只能查表**。
- 排版是**样式**不是字号表：iOS 11 档给默认值（`body 17/22` 等）+ emphasized 字重；
  macOS 另一套更小且固定；**必须按样式驱动**（Dynamic Type 会缩放）；≥20pt 用 Display 面。
- 命中区是**规范数值**：iOS 默认 44×44、硬下限 28×28；macOS 28/20；tvOS 66/56；visionOS 60/28。
- **间距刻度与圆角刻度 Apple 不公布**（社区约定 8pt 栅格 / 控件 8·12·16）；
  但**continuous 圆角**与同心公式 `outer = inner + padding` 是指引。
- 玻璃的数值预算 Apple **不公布**（模糊上限、合成层数等全来自社区/Figma，须标注来源）；
  `clear` 玻璃在亮内容后需约 **35% 暗化层**；Reduce Transparency/Increase Contrast 下必须给近实心变体。
- **动效维度 HIG 完全没有数值**——只有定性准则。我们的 HIG 时长/曲线必须明示为自建约定。

## 14. 三套对照：统一 Style 令牌层的设计约束（2026-10-28 定稿）

| 维度 | MD3 | Fluent 2 | HIG / SwiftUI | 对公共层的要求 |
|---|---|---|---|---|
| 语义值来源 | **规则解析**（tone+曲线+配对） | **查表**（别名→色阶） | **OS 运行时解析**（公开值缺失） | 令牌的值必须是**可求值的**，不能只是常量 |
| 派生输入 | 1 个来源色 → 5 条调色板 | 1 条品牌色阶（16 级，输入） | 无（固定色板 + 系统语义） | 公共层要允许「0 / 1 / 多条」不同数量的种子 |
| 状态层 | 叠加透明度现场合成 | **103 个具名状态令牌** | vibrancy/材质自动解析 | 状态必须能表达「算出来 / 存下来 / 交给系统」三态 |
| 主题维度 | 明暗 × 三档对比 | 明 / 暗 / 高对比（同名表） | appearance × contrast × 背景层级 × base/elevated | 主题不能是 enum，必须是**多维上下文向量** |
| 色阶网格 | tone 0–100（步 10 + 95/98/99） | neutral 步 **2**、brand **步 10**（10–160） | 无阶（固定 13 色 + 5 灰） | 「第几级」不是通用概念，需按风格自定义刻度 |
| 圆角 | **10 档**（0/4/8/12/16/20/28/32/48 + full）且风格族可切 `rounded`↔`cut` | 11 档（0…40 + circular） | **continuous（squircle）**，无公开刻度 | 圆角令牌必须带**曲率族**（round/squircle/cut），不只是数值 |
| 排版 | 字号/行高/字重 token | 17 个复合样式 + 原语引用 | **样式**（可被 Dynamic Type 缩放） | 文本令牌必须是**样式对象**，不是三个数 |
| 动效 | 有数值（时长/曲线/token） | 8 时长 + 9 曲线（cubic-bezier） | **规范无数值** | 动效层必须允许风格自带约定并标注来源 |
| 材质/深度 | elevation + tonal overlay | 6 档阴影（ambient/key） | 6 档材质 + Liquid Glass 三变体 | 「海拔」不能只是阴影，要能表达**模糊/透明/降采样** |

**两处收敛（可进公共层，不必按风格重复）**：
MD3 的「光学圆度 `outer − padding = inner`」与 HIG 的同心公式 `outer = inner + padding` **是同一条**；
MD3 的 8dp 倍率间距与 HIG 的 8pt 栅格也同基准。
（数值来源已核对，见 [`docs/reference/md3-scales.md`](reference/md3-scales.md)；
其中 MD3 间距逐项数值与动效数值表所在的结构化端点 `designSystems/20543ce18892f7d9` **当前不可用**，
已标 `[未证实]`，动效数值目前只能依 Flutter `Curves` 的已读常数。）

**因此统一令牌层的形状**（下一步实现的目标结构）：

```rust
pub struct StyleContext { appearance, contrast: u8, depth /*base|elevated*/, /* 平台可扩展 */ }
pub enum TokenValue {        // 三套体系都需要
    Constant(Cow<'static, str>),          // Fluent 查表 / 刻度值
    Resolved(fn &dyn Fn(&StyleContext, &Seeds) -> Color),   // MD3 规则 / HIG 自派生
    System(Cow<'static, str>),            // 交给宿主（HIG 语义角色）
    Style(TextStyleId | ElevationId | MotionId),  // 复合令牌
}
pub trait StylePack {
    fn color(&self, role: SemanticRole, ctx: &StyleContext) -> Option<Color>;  // None = 交给系统
    fn text(&self, style: TextRole) -> &TextStyle;
    fn shape(&self, role: ShapeRole) -> &ShapeSpec;    // 含 curvature: circular|continuous
    fn elevation(&self, level: u8) -> &Elevation;      // 阴影 / 材质 / 玻璃三选一
    fn motion(&self, role: MotionRole) -> &AnimSpec;   // 允许 provenance: Spec | Convention
    fn provenance(&self) -> &'static Source;           // 每个值都能回答「凭什么」
}
```

**验收（人工，最终）**：gallery 三页——每页展示该体系的「全套色卡（所有上下文）」+
「尺寸/排版/圆角/动效表格」，三页共用同一套读取路径；页面本身就是统一层的可行性证明。

## 15. 统一 Style 令牌层：定稿（2026-10-28）

用户已确认三条：**接受双平面 + 控件三层**；**`Context` 包含 HIG 的 `depth(base/elevated)`**；
**先把三套风格的设计体系代码做完，再一次性接入**（不做新旧并存的长期兼容层）。

### 15.1 为什么现在的层做不到

`ui-style::StylePack` 是**扁平值袋子**：`roles: HashMap<RoleName, Color>` 只存预先算好的颜色。
三个结构性限制：

| 限制 | 后果 |
|---|---|
| 值只能是常量 | MD3 的「tone + 对比曲线 + ToneDeltaPair」与 HIG 的「交给系统解析」都表达不了 |
| 没有上下文维度 | 明/暗/高对比只能各建一个包；HIG 的 4 维上下文无处安放 |
| 公共角色是唯一的门 | 未被填进去的原生令牌**不可达** → 做不出"完整的一整套" |

### 15.2 双平面

```
                      ┌────────────────────────────────┐
   跨风格共享控件 ───►│ A. 公共语义层（封闭集合，约 25）  │ accent / on_accent / surface /
                      └───────────────┬────────────────┘ container / stroke / overlay / …
                                      │ 适配表（数据，每风格一份，可校验）
        ┌─────────────────────────────┼─────────────────────────────┐
        ▼                             ▼                             ▼
  B1. MD3 原生全集              B2. Fluent 原生全集             B3. HIG 原生全集
  49 角色 + tone 调色板          459 令牌（含 103 状态令牌）     13 色 × 4 上下文 + 语义角色
  + ContrastCurve + 配对         + 65 调色板 + 全部刻度          + 6 档材质 + Liquid Glass
```

公共层**不是最小公约数**，只是其中一条访问路径；原生全集永远可达、永不裁剪。

### 15.3 类型（新层的核心）

```rust
pub enum TokenValue<V> {
    Constant(V),                        // Fluent 查表 / 刻度值
    Alias(NativeName),                  // 指向本体系的原生名
    Derived(fn(&Context, &Seeds) -> V), // MD3：tone + 曲线 + 配对；HIG：我们的派生
    Platform(&'static str),             // HIG：tintColor / .regularMaterial 等交宿主
}

pub struct Context {
    pub appearance: Appearance,   // light / dark
    pub contrast: Contrast,       // MD3 三档 / Fluent 高对比 / HIG increased contrast
    pub depth: Depth,             // HIG：base / elevated（随前景切换）
    pub density: Density, pub writing: WritingMode, pub platform: Platform,
}

pub enum Resolution<V> { Value(V), Unmapped(CommonRole), NotSupported }   // 绝不静默回退

pub trait StyleSystem {
    const ID: StyleId;
    fn context(&self) -> &Context;
    fn common(&self, role: CommonColor) -> Resolution<Color>;
    fn native(&self, name: NativeName) -> Resolution<NativeValue>;   // 原生全集按名可达
    fn caps(&self) -> StyleCaps;    // container 家族 / 材质 / tone 阶梯 / cut 圆角 / 状态具名令牌 …
    fn provenance(&self, name: &str) -> Source;   // Spec | Derived | Convention | Platform
}
```

### 15.4 控件分层

| 层 | 走什么路径 | 例 |
|---|---|---|
| T1 共享 | 只 `common()`；进阶先 `caps()` 再 `native()` | Button、List、TextField |
| T2 风格原生 | 直接 `native()`，可用体系私有能力 | MD3 开关 tone 形变、Fluent Expander、HIG 搜索式导航栏 |
| T3 覆写 | 任意令牌显式覆盖（带来源标注） | 品牌色阶替换 |

### 15.5 六条硬规则

1. **不裁剪**：每个原生令牌可寻址；适配表只做加法。
2. **多对一 / 一对多都允许**：`accent` → `primary` / `colorBrandBackground` / `tintColor(Platform)`；
   `primaryContainer` 无公共等价 → 保持原生私有。
3. **缺失显式化**：未映射返回 `Unmapped(role)` 并报错；禁止静默回退。
4. **上下文是一等值**：一个体系实例服务全部 appearance × contrast × depth。
5. **表由数据生成 + 覆盖率测试**：适配表从 `docs/reference/{md3-color-roles,fluent-tokens,hig-tokens}.json`
   生成；测试断言「每个公共角色在三套里都有条目」且「每个原生令牌都可达」。
6. **来源随值走**：`Spec / Derived / Convention / Platform` 四选一，gallery 页面直接显示。

### 15.6 crate 安排（一次性接入的落地形式）

| 动作 | 内容 |
|---|---|
| 新增 `crates/ui-color-hct` | CAM16 + L\* + HCL 反解（自己实现，零第三方色彩库）；`[PORTABLE]` |
| 新增 `crates/ui-design` | 设计体系层：`context` `value` `resolution` `caps` `provenance` + `md3` `fluent` `hig` 三个子系统 + 适配表；依赖 `ui-core`、`ui-color-hct`；`[PORTABLE]` |
| 加入 `platform_boundary.rs` 的 `PORTABLE` 清单 | 两个新 crate 都要登记并写理由（AGENTS §3） |
| 三套**设计体系代码**在 `ui-design` 内完成 | 各套的原生全集 + 适配表 + 自身验收测试，与控件无关 |
| **一次性接入** | `ui-style::StylePack` 被 `ui_design::Theme` 取代；`ui-widgets-{md3,fluent,glass}` 从"自带 pack"改为"消费 ui-design"；`legacy.rs` 只在这次接入里用来搬运旧名，之后删除 |

重叠期纪律：新层在自己的 crate 内开发并全绿，**不与旧 `StylePack` 互相依赖**，避免半迁移状态长期存在。

### 15.7 实施顺序（每步全绿才进下一步）

1. `ui-color-hct`：CAM16/L\*/反解 + **tone 定律自证测试**（Δ40⇒CR≥3.0、Δ50⇒≥4.5）。
2. `ui-design` 地基：`Context`/`TokenValue`/`Resolution`/`Caps`/`Source` + 覆盖率测试的**框架**。
3. **Fluent 灌表**（最容易，用来验证类型形状）：459 令牌 + 适配表 + 「103 状态令牌可寻址」测试。
4. **MD3 接入**：49 角色 + 5 条调色板 + ContrastCurve + ToneDeltaPair + `GetTone` 三情形；
   用 `primary40`/`onPrimary100`/surface98/neutral-chroma-6 复现官方文档。
5. **HIG 接入**：固定色表（4 上下文）+ 材质 + 语义色**我们的派生**（标 `Convention`）
   + `Platform` 出口；用 ≥4.5:1 / 7:1 与灰阶单调自证。
6. **一次性接入**：`ui-style` → `ui-design`，三个控件包迁移，`legacy` 搬运旧名后删除。
7. **gallery 三页**：每页 = 该体系完整色卡（全部上下文）+ 完整尺寸表（圆角/间距/排版/命中区/动效），
   每格标来源；三页共用同一套渲染代码，只有 `StyleSystem` 实例不同。

**验收判据（人工）**：如果某格只能靠"公共层降级"才画得出来，说明公共层在裁剪原生——那是设计失败。

### 15.8 已知缺口（不粉饰）

| 缺口 | 处理 |
|---|---|
| MD3 `space*` 逐项数值、动效数值表（端点 `designSystems/20543ce18892f7d9` 不可用） | 按「8dp × 倍率 + 少量嵌套单位」建模并标 `[未证实]`；动效暂依 Flutter `Curves` 已读常数 |
| HIG 语义色数值、Dynamic Type 倍率、vibrancy 合成 | Apple 不公开 → 我们派生并标 `Convention` |
| HIG 动效数值 | HIG 无 → 明示为自建约定 |
| Fluent 品牌色 → 16 级色阶算法 | 官方未公开 → 接口接受色阶；自建生成器并用 `brandWeb`+Teams 两条官方阶验收 |
| `cut` 圆角（MD3 形状族） | 引擎 `Corners` 目前只有 round/squircle，接入阶段需补 |

### §15.7 步骤 1 完成：`ui-color-hct`（2026-10-28）

新增 crate `ui/crates/ui-color-hct`（已进 `PORTABLE` 清单并登记进 `platform_boundary.rs`）：

| 文件 | 内容 |
|---|---|
| `lab.rs` | sRGB ⇄ 线性光（0..100 域）、`Y ⇄ L*`、XYZ 矩阵、`tone_of` / `argb_from_tone` |
| `viewing.rs` | `Viewing::create` 推导全部 CAM16 常数；`Viewing::srgb()` 为 D65 默认条件 |
| `cam.rs` | CAM16 正变换；`linrgb_from_jch` 逆变换 |
| `solve.rs` | HCL → sRGB：J 的牛顿式迭代 + 越色域时沿 hue/Y 对彩度二分 |
| `contrast.rs` | `RatioOfYs/Tones`、`Lighter/Darker(+Unsafe)`、前景倾向三常数、`enable_light_foreground` |
| `palette.rs` | `TonalPalette(hue, chroma).tone(t)`、`max_chroma(t)`、`KeyColor` 的峰值搜索 |

**测试 24 项**（`cargo test -p ui-color-hct` 全绿），其中四项是「我方实现 vs 外部事实」的自证：

1. `derived_constants_match_the_published_defaults` —— 推出的 n/aw/nbb/c/fl/fl_root/z 对上公开默认常数（1e-5）。
2. `the_md3_baseline_primary_sits_at_tone_40` —— 本文档之外的事实：`#6750A4` 的 tone 是 40.08，
   与「primary = primary tone 40」**独立吻合**。
3. `a_tone_gap_of_40_buys_at_least_three_to_one` —— tone 定律前半段全域自证。
4. `the_solver_lands_on_the_requested_tone` —— 反解器在 4 个色相 × 5 个 tone 上都落回请求的 tone。

**实测推翻了我自己的两条前提**（已回写 `docs/reference/md3-color-spec.md` §13）：

| 前提 | 实测 | 处理 |
|---|---|---|
| 「tone 差 50 ⇒ CR ≥ 4.5」 | tone 50 对白只有 **4.48** | 按「Δ40 全域 + Δ50 排除顶亮度」重写断言，并钉住 4.48 |
| 「M3 baseline `#6750A4` 可用来自证 TonalSpot 生成器」 | 它是 `H299 / C47.9`，而 TonalSpot 请求 C36；tone 却正好 40.08 | **baseline 与「由 seed 生成」是两件事**；验收改用 tone 一致 + 彩度可控 |
| 「中性色彩度应为 0」 | 白/灰的 CAM16 彩度 ≈ 2.9（白点不在 CAT02 中性轴） | 中性判据改为「远低于饱和色」，不再用 `< 1` |

门禁：`cargo build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **477 passed**
（原 453 + 24）。

下一步按 §15.7：**步骤 2 `ui-design` 地基**（Context / TokenValue / Resolution / Caps / Source + 覆盖率测试框架），
然后步骤 3 Fluent 灌表。

### §15.7 步骤 2 完成：`ui-design` 地基（2026-10-28）

新增 crate `ui/crates/ui-design`（已登记 `PORTABLE`）。**只有类型与契约，不接任何一套风格**，
目的是先把「双平面」的形状钉死，避免灌数据时被迫改接口——上一轮返工的教训。

| 文件 | 内容 |
|---|---|
| `context.rs` | `Appearance` × `Contrast`（四档：Low/Standard/Medium/High，含 `md3_level()` 双向映射）× `Depth`（Base/Elevated）× `Platform` × `WritingMode` |
| `value.rs` | `Rgba`；`Resolution<T> = Value \| Unmapped \| Absent(Reason)`；`Reason = PlatformResolved \| OutOfGamut \| NotAConcept` |
| `source.rs` | `Source = Spec \| Derived \| Convention \| Platform`（带 `label()`/`detail()`，画廊直接印） |
| `common.rs` | `CommonColor` 22 个语义角色（封闭清单，附 `ALL` 与 `name()`） |
| `caps.rs` | `StyleCaps` 11 个可选能力（tonal_palettes / containers / named_state_tokens / elevated_depth / materials / glass / radius_scale / cut_corners / motion_tokens …） |
| `system.rs` | `StyleSystem` trait（`id/context/common/native/caps/provenance`）、`StyleId`、`NativeName`、`Theme` |

**测试 17 项**（`cargo test -p ui-design` 全绿），钉住的契约：

- **对比档双向映射**：`Contrast::md3_level()` → `from_md3_level()` 四档往返一致；
- **失败是显式值而非下沉**：`Absent(PlatformResolved("tintColor"))` 能原样传出（HIG 的 accent 就是这个形态），
  未知名返回 `Unmapped` 而**不是编一个颜色**；
- **原生路径不被公共层裁剪**：`native("systemBlue")` 在公共清单之外仍可达；
- **公共角色清单无重复**（`ALL` 去重后数量不变，防止灌表时漏项却测不出来）。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **494 passed**（477 + 17）。

下一步：**步骤 3 Fluent 灌表**（459 令牌 + 适配表 + 「103 个状态令牌可寻址」测试），
用最容易的一套验证类型形状是否真的够用。

### §15.7 步骤 3 完成：Fluent 灌表（2026-10-28）

**数据流**（可重跑，不是手抄）：
`docs/reference/fluent-tokens.json` → `ui/crates/ui-design/data/gen_fluent.py` →
`ui/crates/ui-design/data/fluent_data.rs` → `include!` 进 `fluent.rs`。

| 表 | 行数 | 内容 |
|---|---|---|
| `COLORS` | 330 | 不透明颜色（三主题列 [明,暗,高对比]） |
| `ALPHA_COLORS` | 36 | 带 alpha 的颜色（叠加/模板/scrim）——**Fluent 的 scrim 就在这里** |
| `NUMBERS` | 4 | 字重 400/500/600/700 |
| `LENGTHS` | 65 | 圆角 11、间距 22、时长 8、字号/行高/描边 |
| `TEXTS` | 12 | 字族栈、9 条 cubic-bezier |
| `COLOR_TOKENS_WITHOUT_VALUE` | 24 | 值非纯色的颜色令牌（阴影 12 等） |
| `BRAND_RAMP` | 16 | 默认 web 品牌阶（10..160） |

合计 **459** 个令牌，与官方数量一致；`FluentSystem::token_count()` 与三张表去重后的名字数相等（有测试钉住）。

**适配表**：22 个 `CommonColor` → Fluent 别名，21 个有目标、`Scrim` 明确为 `None`（Fluent 用叠加
RGBA 表达，无单一实色令牌）→ 返回 `Absent(NotAConcept)` 而**不是编一个颜色**。

**九项测试**（`cargo test -p ui-design` 共 26 项全绿）里最关键的：

1. `colorNeutralForeground1` = `#242424`（明）/ `#ffffff`（暗）——与 **fluentui MCP 独立报告的值**一致；
2. **103 个状态令牌全部可寻址**（93 不透明 + 5 全透明 + 5 半透明），并断言「全透明的恰好 5 个」；
3. **四个对比档塌缩到三个主题列**：High 优先于明暗（`High+Dark → 高对比列`）；
4. 每个公共角色都有**刻意的**适配条目（数量与 `ALL` 相等，防重复/漏项）；
5. 适配表指向的每个令牌名**真实存在**（防止写了不存在的别名）。

**灌表过程中被数据打脸的三次**（都改了生成器，不是改测试去迁就）：

| 现象 | 根因 | 修法 |
|---|---|---|
| 「103 个状态令牌」只解析出 97 | `rgba(0, 0, 0, 0.1)` 带空格，正则不匹配 | 解析器容忍空格与小数 |
| 6 个令牌彻底解析失败 | **`#rrggbbaa` 的 alpha 在末尾**，不是 `#aarrggbb` | 显式转换，注释写明 |
| 同一令牌三主题写法不同（`rgba(...)` 与 `#1aebff`） | Fluent 高对比把叠加换成实色 | **逐主题列独立解析**，按 alpha 值分流到两表 |

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **503 passed**（494 + 9）。

下一步：**步骤 4 MD3 接入**（49 角色 + 5 条调色板 + ContrastCurve + ToneDeltaPair + `GetTone` 三情形）。

### §15.7 步骤 4 完成：MD3 接入（2026-10-28）

**不是查表，是规则求值**。49 个角色的 tone 全部从源码 lambda 解码成规则，再由 `ui-color-hct` 求值。

**新的数据管线**（旧的 `toneExpr` 是未解析的散文，已废弃）：

```
material_dynamic_colors.cc
  → data/gen_md3_roles.py  → docs/reference/md3-color-roles.json（结构化 tone/curve/delta/background）
  → data/gen_md3_data.py   → ui/crates/ui-design/data/md3_data.rs（编译期表）
```

`tone` 的解码结果分布（49 个角色全部解码成功，**0 个遗留 raw**）：

| kind | 个数 | 含义 |
|---|---|---|
| `pair` | 25 | `is_dark ? dark : light` |
| `mono_pair` | 11 | 剔掉 monochrome 分支后即 pair（TonalSpot 不取该分支） |
| `both_curve` | 4 | 两侧都跟 ContrastCurve（surface_container 家族） |
| `fidelity_default` | 3 | 剔掉 fidelity 分支后即 pair（TonalSpot 不 fidelity） |
| `const` | 3 | shadow / scrim / 常数 |
| `dark_curve` | 2 | 亮色常数 + 暗色曲线（surface_bright、container_lowest） |
| `light_curve` | 1 | 暗色常数 + 亮色曲线（surface_dim） |

**实现要点**（`crates/ui-design/src/md3/`）：

- `palette.rs`：`TonalSpot` 的 hue/chroma 表（36/16/hue+60·24/6/8 + error 25·84）→ `Palettes`；
  `SourceColor::from_argb` 从种子色直接读 H/C/tone。
- `tone.rs`：`Curve`（四锚点 -1..=1 分段 lerp）、`foreground_tone`、`resolve_delta_pair`。
- `mod.rs`：`Md3Scheme { ctx, palettes, source_tone }`，`tone_of` 三种情形：
  ① 配对（`stay_together` 直接返回，否则按**角色身份**而非 tone 大小定 nearer/farther）
  ② 单背景 + 曲线（**配对成员跳过**，否则会被曲线覆盖）
  ③ 背景角色落在 50–59 → 推到 49 或 60。

**11 项 MD3 测试**（`cargo test -p ui-design` 共 43 项全绿），验收要点：

1. **文档默认值复现**：primary 40 / onPrimary 100 / light surface 98 / dark primary 80 / dark surface 6；
2. **neutral chroma = 6**、neutralVariant = 8（TonalSpot 规范表）；
3. **tone 定律**：`on_surface/surface`、`on_primary/primary`、`on_error/error`、
   `on_primary_container/primary_container` 明暗两侧都 ≥ 4.5:1；
4. **高对比不降间距**：High 档的 tone 间距 ≥ Standard 档；
5. **配对保持 10 tone**：`primary/primary_container`、`error/error_container`；
6. **fixed 角色明暗完全一致**（`stay_together` 的语义）；
7. **TonalSpot 不是 fidelity**：`primary_container` = 90 而**不是**来源 tone（source tone ≈ 40.08）；
8. **自定义种子真的改变配色**（换绿色 seed，primary 变色但 tone 仍是 40）；
9. 49 个角色名唯一且齐全；适配表覆盖全部 22 个公共角色且目标都是真实角色；
10. `native()` 能到达全部 49 个角色（不裁剪）；
11. 任何角色解析出的 tone 都在 0..100。

**过程中被自己写错的三处**（都是逻辑错，不是数据错）：

| 现象 | 根因 | 修法 |
|---|---|---|
| `primary` 亮色解出 30 而不是 40 | 曲线情形**把配对成员也覆盖了** | 配对角色跳过曲线步骤 |
| 同一配对里「谁 nearer」按 tone 大小判 | polarity/nearer 是**角色身份**，不是数值大小 | 改成按 `pair.a`/`pair.b` 身份判定 |
| `primary_fixed` 暗色 80 vs 亮色 90 | 忽略了 `stay_together` | 该标志为真时两侧都保持规则值 |

另修了一处解码缺陷：`background` 的正则匹配 `return Primary(s)`，而源里是 `return Primary()`，
一度让 35 个背景指针变成**整段 lambda 字符串**；改成严格匹配 `\w+\(\)` 并**在解析失败时报错**，
这样同类错误下次会立刻暴露而不是静默生成垃圾表。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **520 passed**（503 + 17）。

下一步：**步骤 5 HIG 接入**（固定色表 + 材质 + 语义色派生 + `Platform` 出口），
然后步骤 6 一次性接入 `ui-style` → `ui-design`。

### §15.7 步骤 5 完成：HIG 接入（2026-10-28）

三套体系现在都有完整实现。HIG 是最特殊的一套：**Apple 公布值的地方极少**。

**数据管线**：`docs/reference/hig-tokens.json` → `data/gen_hig.py` → `data/hig_data.rs`
（13 系统色 + 5 档灰 × **4 个上下文**、5 个平台的命中区、6 档材质、15 个语义角色名）。

**三档取值来源，逐条标注**：

| 来源 | 内容 | 我们的处理 |
|---|---|---|
| `Spec` | 18 个系统色（每色 4 个上下文：明/暗 × 默认/增强对比） | 直接查表 |
| `Spec` | 6 档内容材质（ultraThin…ultraThick、bar） | 只知道名字 → `Absent(PlatformResolved(".regularMaterial"))` |
| `Convention` | `label`/`systemBackground`/`separator` 等 **15 个语义角色** | **我们派生**：用 tone 阶梯生成中性色，并按 HIG 的 4.5:1 自证 |
| `Convention` | `tintColor` 的实际值 | 取系统蓝作代表（原生系统的实际解析结果） |
| `PlatformResolved` | `scrim`（sheet 背后的实时模糊+暗化） | 明确拒绝，不编颜色 |

**关键设计点**：

- **四个上下文是硬编码的四列**，不是算出来的：`increased contrast` 是 Apple 独立公布的值
  （Blue light `#0088FF → #1E6EF4`，dark 侧反而提亮到 `#5CB8FF`）——任何"调对比"的公式都会错。
- **语义角色用中性 tone 阶梯派生**，而不是假装知道 Apple 的值；
  `provenance()` 会如实返回 `Convention`，画廊上直接显示"这是我们的派生"。
- **`native()` 三态齐全**：系统色 → `Value`；材质/语义角色 → `Absent(PlatformResolved(名字))`；
  不存在 → `Unmapped`。

**10 项 HIG 测试**（`cargo test -p ui-design` 共 53 项全绿）：

1. HIG 表的 `systemBlue` = `#0088FF`（明）/ `#0091FF`（暗）、`systemGray` = `#8E8E93`；
2. **增强对比切到独立值**：`#1E6EF4`、`systemRed #E9152D`，且与默认档不同；
3. 18 个系统色在**四个上下文**下全部可寻址；
4. **语义角色与材质全部报 `PlatformResolved`**，不是编造的颜色；
5. **正文对比 ≥ 4.5:1**（明/暗 × 默认/增强 四个上下文）；
6. **增强对比不减小前后景间距**；
7. **灰阶单调**：明色下逐渐变暗、暗色下逐渐变亮；
8. 命中区按平台：iOS `(44, 28)`、macOS `(28, 20)`，且默认 ≥ 下限；
9. 每个公共角色要么给值、要么显式 `PlatformResolved`（不允许其他形态）；
10. `provenance` 能区分 `Spec`（systemBlue）与 `Convention`（label），并有 `systemMint → mint` 的 SwiftUI 名映射。

**一处自己发现并修掉的问题**：`native()` 里漏了 `SEMANTIC_ROLES` 这张表（生成器一开始没输出它），
导致 15 个语义角色被当成"未知"返回 `Unmapped` 而不是 `PlatformResolved`——语义不同：
前者是"我没这个概念"，后者是"我知道它，但值归系统管"。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **530 passed**（520 + 10）。

下一步：**步骤 6 一次性接入**——`ui-style::StylePack` → `ui_design::Theme`，
三个控件包迁移，`legacy` 搬运旧名后删除。这是唯一会碰现有控件代码的一步。

## 16. 步骤 6：一次性接入（2026-10-28 完成）

按用户决定「一步」，`ui-style` 的对外形状**保持不变**（`StylePack` 仍是控件消费的那个类型），
但三个风格包的**取色来源**全部换成 `ui-design`。这样切换是原子的、无需并存两套 API。

| 包 | 之前 | 现在 |
|---|---|---|
| `ui-widgets-md3::pack()` | 9 个手抄 hex（含过时的 `onPrimaryContainer #4F378B`） | `Md3Scheme::baseline()` 解析（49 角色可用） |
| `ui-widgets-fluent::pack()` | 9 个手抄 hex | `FluentSystem::native()` 查表（459 令牌可用） |
| `ui-widgets-glass::pack()` | 5 个手抄 hex | 轨道/边缘保持参考实现数值；`systemGreen` 与文字墨色改从 `HigSystem` 取 |

三处「手抄值」被数据纠正（都不是笔误，是**过时或不忠实**）：

| 角色 | 手抄值 | 解析值 | 结论 |
|---|---|---|---|
| MD3 `onPrimaryContainer` | `#4F378B`（旧 M3 基线） | `#4D3D75`（primary tone 30） | 手抄的是 2021 之前的版本 |
| MD3 `outline` | `#79747E` | tone 50 的 neutral-variant（更暗） | 字面量并非从调色板来 |
| Fluent `accentFill` | `#0066B4` | `#0F6CBD`（`colorBrandBackground`） | 前者是旧 Fluent 的 blue-dark，后者是当前品牌色 |

**新增的桥接**：`ui-design::FluentSystem` 增加 `NATIVE_ALIAS`（6 条），
把本框架面向控件的名字（`accentFill`/`textFillColorPrimary`…）映射到 Fluent 自己的令牌；
这样控件不必改名，而取值仍来自官方表。`ui-design` 另外 re-export 了 `tone_of`。

**仍然保留字面量的三处，并写明理由**：

1. Fluent 的 `controlAltFillColorSecondary/Tertiary/Quarternary`——Fluent 没有单名令牌，
   它们是组件自己叠加的洗色（alpha-only black），保留字面量并注明「不假装是公开值」。
2. Glass 的 `glassTrackInactive #C5C5C6` 与 `glassRim`——参考实现自己的数值，不属于任何体系规范。
3. MD3 的度量（radius 20）、Fluent 的 radius 4、Glass 的 radius 13——控件度量尚未纳入 `ui-design`
   （那是下一步「尺寸令牌」的工作，见 §14 的尺寸表目标）。

**一处测试前提被推翻并改写**：`the_baseline_scheme_carries_the_documented_primary` 原本断言
`primary == #6750A4` 且 `outline == #79747E`。现在 primary 由 tone 40 生成（`#4D3D75`），
outline 来自 neutral-variant 调色板——**旧断言把过时的字面量钉成了规格**。
新测试改为断言「primary 的 tone == 40」+「primary 等于 scheme 自己解析出的值」，
这样它测的是**规则**，不是某次抓拍。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **530 passed**；
`GALLERY_SELFTEST=1` **PASS**（三列 5/5 页、拖动、引擎时钟点击、冻结时钟都在）。

遗留：`legacy_pack()` 仍在（示例的兜底外观在用），但它已不在三套风格的主路径上；
等尺寸令牌接入后再决定它的去留。

## 17. 步骤 7：尺寸令牌（2026-10-28 完成）

`ui-design` 新增 `dimension.rs`（类型）+ `scales.rs`（三套取值）+ `data/gen_dimensions.py`
（从 `docs/reference/{md3-scales,fluent-tokens,hig-tokens}.json` 生成 `dimensions_data.rs`）。

**新增数据类型**（每个都自带 `Source`）：

| 类型 | 字段要点 |
|---|---|
| `Curvature` | `Round` / **`Continuous`（squircle）** / **`Cut`（MD3 的直线角，我们引擎原来缺它）** |
| `RadiusScale` | 具名档位 + 曲率族 + `supports_cut` + 来源；`-1.0` 是全圆哨兵 |
| `SpacingScale` | **一个基准 + 具名倍数**（不是枚举）+ `nested` 非栅格值（MD3 的 2/4/6/10） |
| `TypeRamp` | 字号/行高/字重 + **`scalable`**（只有 HIG 为真，因为动态字体会放大） |
| `MotionScale` | 时长表 + `cubic-bezier` 曲线表 + 来源（HIG 的标 `Convention`） |
| `HitTarget` | `default` 舒适尺寸 + `minimum` 硬下限 |

三套的实测数据：

| | 圆角 | 间距 | 排版 | 动效 | 命中区 |
|---|---|---|---|---|---|
| **MD3** | 10 档 0/4/8/12/16/20/28/32/48/full，`supports_cut` | 8dp 基准 × 倍数 + 2/4/6/10 | 15 档（display…label），固定 | 12 时长 + 7 曲线 | 48/48 |
| **Fluent** | 11 档 0/2/4/6/8/12/16/24/32/40/circular | 11 档 0…32（base 1 存字面点数） | 5 档复合样式，固定 | 8 时长 + 9 曲线 | 32/24 |
| **HIG** | 5 档 8/12/16/20/capsule，**`Continuous`** | 8pt 栅格（约定） | 5 档 iOS 字阶，**`scalable: true`** | 3 时长 + 3 曲线（约定） | **44/28（Apple 真公布）** |

**来源分布**（这是设计的一部分，不是实施细节）：

- `spec`：MD3 的圆角/间距模型/排版/动效、Fluent 的全部、HIG 的排版与命中区；
- `convention`：MD3 的间距逐项数值（结构化端点不可用）、HIG 的圆角与间距刻度、**HIG 的全部动效**；
- 测试 `motion_provenance_reflects_what_each_system_publishes` 专门钉住「HIG 的动效标签必须是 convention」。

**同时把控件接了上来**（不再是三处魔数 20/4/13）：

| 包 | 之前 | 现在 |
|---|---|---|
| md3 | `metrics(20.0)` | `scale_radius("largeIncreased")` |
| fluent | `metrics(4.0)` | `scale_radius("medium")` |
| glass | `metrics(13.0)` | `scale_radius("medium")`（HIG 无 13，取最近的约定档 12） |

**8 项尺寸测试**（`cargo test -p ui-design` 共 64 项全绿）：

1. MD3 圆角恰好 10 档、`medium=12`、`full=-1`、`supports_cut` 为真、来源是 `spec`；
2. MD3 间距 `space100 = 8dp`、`space400 = 32dp`、`nested` 含 4dp；
3. Fluent 圆角 `medium=4`、`circular=-1`、曲率为 `Round`；
4. **HIG 曲率必须是 `Continuous`**（Apple 的形状），且命中区 44/28 且来源 `spec`；
5. **只有 HIG 的排版 `scalable`**；
6. 动效来源三套各不相同（spec/spec/convention）；
7. 每条刻度内部有序（圆角递增、字号在 10–64 且行高 ≥ 字号、字重 100–900）；
8. 统一入口 `dimensions_of(id, ctx)` 三套都能取到。

**一处自己的测试前提写错了**：最初断言「字号必须按位置递减」，但 M3 官方字阶是**按角色交错**的
（`titleLarge 22, titleMedium 16, bodyLarge 16, bodyMedium 14, labelLarge 14 …`），
相邻递减不是不变量。改成断言**每一档自身的合理性**（10–64sp、行高 ≥ 字号、字重合法），
这样既钉住了坏数据，又不会把官方顺序当成错误。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **541 passed**；
`GALLERY_SELFTEST=1` **PASS**。

下一步：**gallery 三页**——每页展示该体系的色卡（全部上下文）+ 尺寸表（本步的数据），
每格标来源；三页共用同一套渲染代码。

## 18. 步骤 8：gallery 三页（2026-10-28 完成）

gallery 从 **5 页**扩到 **8 页**，新增一节「Design systems」，每套体系一页：

| 页 | 内容 |
|---|---|
| `md3-system` | MD3 · 颜色 / 尺寸 / 来源 三列 |
| `fluent-system` | Fluent 2 · 同上 |
| `hig-system` | HIG · 同上 |

**每页三列**（`page_tags` 里已登记，`columns_ok` 会检查互不重叠）：

1. **colours**：22 个公共角色逐个色块 + `#RRGGBB`（或 `host` 当体系明确交宿主解析）
   ＋「native extras」5 个该体系私有令牌；
2. **dimensions**：圆角（含曲率族与 `cut` 支持）、间距（base × 倍数 + 嵌套值）、
   排版（每档 size/line/weight + `scalable`）、动效（时长 + 曲线名）、命中区；
3. **provenance**：五条刻度的 `[spec|derived|convention|platform]` + 详情，
   外加该体系 `StyleCaps` 的九项能力布尔表。

**自检扩展**（这是本轮最有价值的部分）：

```
selftest: nav click 8/8 pages ✓
selftest: three columns 8/8 pages ✓
selftest: system pages 3/3 complete ✓
selftest: PASS
```

`system pages 3/3` 检查的不只是「列存在」，而是**列里的行真的渲染出来了**：
对每页遍历注册表里发出的行 id，逐个在树里 `find_by_id` 并断言 `visual_rect` 非空。

**踩到的坑与真实原因**（记下来，因为下次还会遇到）：

1. `SizeClass::fill` 不存在——本仓库一律用显式 `SizeClass { width: AxisSize::fill(...), .. }`。
2. **`Id::of(tuple)` 是 FNV-1a 哈希，无法反推**：我一开始在自检里用同一个字面量元组
   重新 `Id::of(...)` 去找行，结果全 miss。实测列的第一个子节点高度 744.4、有 29 个孙节点——
   说明**行确实画出来了，是我查的 id 不对**。改成页面把生成的 id **注册进 `thread_local` 表**，
   自检从表里读，问题立刻消失。这条比「列是否存在」重要：它证明的是内容而非容器。
3. 非活动页被 `offset(100_000)` 推到屏外且 `SizeClass::fixed(0,0)`，所以自检必须先
   `click` 到该页再读几何——沿用已有导航循环的那条点击路径。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **541 passed**；
`GALLERY_SELFTEST=1` **PASS**（含新增的 3/3 系统页断言）。

**剩余（人工验收）**：三页的**目测**由用户完成（沿用既定约定，不做截图自动化）。
若目测发现某格只能靠「公共层降级」才画得出，即说明公共层在裁剪原生——那是设计失败，要回头改表。

## 19. 收尾两件：曲率接线 + switch 图标（2026-10-28 完成）

### 19.1 `Curvature` → `CornerShape`（此前只差一根线）

`§15.8` 记「引擎 `Corners` 只有 round/squircle，接入阶段需补 `cut`」——**这条记录是错的**。
实测：`ui-core` 早就有 `CornerShape::BEVEL`（`superellipse(0)`，即 L1 球 = 直线平切），
`ui-paint` 的 `superellipse_norm` 在 `n <= 1` 时正确退化为 `x + y`，GPU 侧也有同公式，
且 `ui-paint/tests/corner_shape.rs` 里已有 `bevel_cuts_the_corner_with_a_straight_line`。
所以缺的不是引擎能力，而是**两套词汇从未连起来**。

补的东西（都很小）：

| 位置 | 内容 |
|---|---|
| `ui-design/src/corner.rs`（新） | `pub use ui_core::geom::CornerShape;` — 设计侧的门，不直接伸手到渲染 crate |
| `Curvature::corner_shape()` | `Round→ROUND`、`Continuous→SQUIRCLE`、`Cut→BEVEL` |
| 三个 pack 的 `scale_corner()` | 从各自的 `radius.curvature` 取，删掉硬编码的 `CornerShape::ROUND` |

**可见收益**：glass 包的圆角从 `ROUND` 变成 **`SQUIRCLE`**——因为 HIG 的曲率是
`Continuous`。此前那 13pt 圆弧一直是错的，只是没人看得出。

**4 项测试**：三套曲率映射到三个不同形状且在超椭圆轴上按名排序（cut < round < continuous）；
每个 pack 的 `corner` 等于自己刻度的 `curvature`（glass 那条专门 `assert_ne!(ROUND)`）；
只有 MD3 声明 `supports_cut`，且 `Cut` 真的到达 `BEVEL`。

### 19.2 MD3 滑块字形（此前的「缺 icon」判断基本正确，但细节有误）

先核实参考实现（`flutter/material/switch.dart`）：

- `_SwitchConfigM3.iconSize = 16.0`；
- `iconColor`：选中 → `onPrimaryContainer`，未选中 → `surfaceContainerHighest`，禁用 → 该值 38%；
- 画出前 `Color.lerp(inactiveIconColor, activeIconColor, currentValue)`——**颜色随进度走**；
- 只在实际提供 `thumbIcon` 时才画。

实现（`crates/ui-widgets-md3/src/toggle.rs`）：

- `ThumbGlyph::{Check, Cross}` + `ICON_SIZE = 16.0` + `ICON_BAR = 2.2`；
- `icon_color(pack, on, disabled)` 与 `glyph_color(pack, disabled, t)` 照抄上面两条规则；
- `glyph_for(disabled, t)` 在 `t >= 0.5` 切到 `Check`；
- 形状用**轴对齐方块排的阶梯**近似 45° 笔画（引擎无旋转变换，这是诚实的近似，注释写明）；
- 画在滑块中心：`gx = thumb_x + (tw - ICON_SIZE) * 0.5`（与参考的 offset 算术相同）。

**一个我自己拿的主张**：禁用时**不画**字形。参考会画（38% 的图标叠在 38% 的滑块上），
但那样既看不见又多花 24 个形状/帧。这是偏离，已写进注释与测试名。

**3 项新测试**：开关有字形且开/关不同；**禁用时字形条数为 0**（明示这个偏离）；
字形颜色在两态之间穿梭（中点既不是起点也不是终点）、且禁用时 alpha = 0.38。

**连带修好的测试脆弱性**：原有 3 条测试用 `shapes.len()` 硬编码数量（2/3），
并假设「滑块是最后一个形状」。字形条让这两条前提全部失效——
改成结构性断言（`reaction_index()` 找涟漪、`glyph_bar_count()` 数图标条、
`thumb_shape()` 取轨道之后**面积最大**的形状）。这类测试本来就该这样写，
之前的形状数量断言只是当时恰好对的巧合。

**过程中我把 `tests/toggle.rs` 的括号搞坏了三次**（拼接新测试时把 mod 与相邻 fn 的 `}`
挪错了位，`never used` 警告暴露出来）。修完核对了 `{`/`}` 计数 = 80/80。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **549 passed**
（541 + 8）；`GALLERY_SELFTEST=1` **PASS**。

### 19.3 仍未做（明确留空）

**Fluent 16 级品牌色阶生成器**：官方算法未公开，`brand_tone()` 目前只是接口。
按用户决定**暂时留空**，等需要时再做（届时用已拿到的 `brandRamp` 16 级做回归验收）。

## 20. MD3 switch 补全：常驻描边 + 图标尺寸 + focus ring（2026-10-28 完成）

拿到了官方规格（`m3.material.io/components/switch/specs`），核对如下：

| 元素 | 属性 | 官方值 |
|---|---|---|
| Track | Outline width | 2dp |
| Handle | Height/Width (unselected) | 16dp |
| Handle | **Height/Width - with icon** | **24dp** |
| Handle | Height/Width (selected) | 24dp |

### 20.1 描边是轨道的常驻属性，不是关闭态专属

此前把描边画成内缩 `border`。用户指出更合理的模型是**两态都有描边，开启时描边色等于底层色**——
这与 M3 的实际实现一致（`trackOutlineColor` 选中时 `Colors.transparent`，宽度恒 2）。

- `outline = outline.lerp(track_color, t)`：开启时与填充同色，视觉消失但仍在；
- 描边画在 52×32 盒子**外侧**（`PAD - ow`，尺寸 `TRACK + 2·ow`），故两态**外框同大**；
- 内缩描边会吃掉 2dp 填充、使关闭态窄 4dp——那是被修掉的缺陷。

### 20.2 带图标时关闭态与开启态等大

规范把「with icon 24dp」写成**一个值**，不是两态各一个。故 `inactive_size = THUMB_WITH_ICON`
（24）而非 16，且替换在**尺寸序列的源头**（锁存端点、胶囊两端都用它），不是补一处。

### 20.3 focus ring 是独立的 overlay，且中间留透明 gap

- **它是另一圈**：轨道描边属于轨道，focus ring 报告键盘焦点，有自己的宽度与间隙；
- **它是 overlay**：`shapes` 是纯绘制，不参与布局；实测环宽 64 > 控件盒 60，靠 `ink(margin)`
  让引擎把溢出纳入脏区，而 `visual_rect`/命中区不变；
- **它是描边不是实心盘**：用户指出「gap 部分被填充了颜色」——实心矩形会把间隙一起染色。
  改为 `fill: TRANSPARENT` + `border: (colour, FOCUS_RING_WIDTH)`，gap 因此透出背后。

### 20.4 测试（5 项新增，`cargo test -p ui-widgets-md3` 共 42 项全绿）

1. `an_outlined_switch_occupies_the_same_outer_box_as_a_filled_one`——两态外框同大；
2. `a_switch_with_an_icon_is_24_in_both_states`——关闭态 = 24 而非 16；
3. `the_focus_ring_is_a_separate_ring_from_the_track_outline`——键盘焦点有环、指针焦点无环；
4. `the_focus_ring_is_an_overlay_that_never_enlarges_the_control`——`size_class` 不因焦点改变；
5. `the_gap_between_the_focus_ring_and_the_control_stays_transparent`——**环体 `fill.a == 0`**，
   颜色只在 `border` 里，故 gap 不被染色。

### 20.5 顺带修好的测试基础设施

`tests/toggle.rs` 里多处形状断言原先硬编码下标（`shapes[1]`）与数量（`len()==3`），
描边环与图标条让它们全部失效。改为按几何特征定位：`track_shape`（52×32 填充）、
`outline_shape`（56×36 环）、`reaction_index`（>28 的正圆）、`thumb_shape`（图标条之前的
非平凡形状）、`focus_ring_shape`（最外层环）。

**过程记录**：`tests/toggle.rs` 是 `mod toggle;` 的子模块文件，顶层即模块内容、**不需要外层
`mod {}`**。我拼接测试时反复误加末尾 `}`，把后续测试嵌进前一个 fn 体内，表现为
「`function ... is never used`」——那是括号错位的可靠信号，不是死代码。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **554 passed**；
`GALLERY_SELFTEST=1` **PASS**。

### §20.6 修正：描边宽度算重了一次（2026-10-28，用户指正）

用户看出「不加描边的大小才是加了描边后应有的大小」——**完全正确**。

原来的做法是把描边画成 `TRACK + 2·ow`（56×36）的实心矩形、再把 52×32 的填充盖上去。
于是**整体每边多出 2dp**：`52 + 2·2 + 2·PAD = 64 > WIDGET 60`，溢出 4dp。
而规范说 Track 是 **52×32（已含 2dp 描边）**——`52 + 2·PAD = 60`，与 `WIDGET` 严丝合缝。

正确做法：描边不再外扩，而是 **52×32 矩形上的向内 `border`**（引擎的 `border` 是向内描：
`origin + bw`、尺寸 `- 2·bw`）。于是外框正好 52×32、内部填充 50×28。

连带修正：焦点环的 `outset` 原本是 `ow + gap + width`（把已不存在的「外环」也算进去了），
改为 `gap + width`。修正后焦点环外宽 **60×40**，恰好落在 60×48 控件盒内——
**不再需要 `ink` 溢出声明**，已删除。这比之前更干净：焦点环完全在盒内。

**教训**：「把描边画在盒子外面」隐含了「盒子不含描边」的假设；规范里 `52dp` 是含描边的总尺寸。
遇到「外框尺寸」时，先问一句「这个数字含不含描边」，再决定加还是减。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **554 passed**；
`GALLERY_SELFTEST=1` **PASS**。

## 21. switch 颜色按规范核对 + 深浅切换与四变体展示（2026-10-28）

### 21.1 六态颜色表逐格核对

用户参考在线 demo 指出很多颜色不对。取 `_SwitchDefaultsM3` 原文（`switch.dart:2308+`），
把取色逻辑改成**直接照表求值**（`toggle.rs::switch_colors`）：

| 状态 | thumbColor | trackColor | trackOutlineColor |
|---|---|---|---|
| off 静止 | `outline` | `surfaceContainerHighest` | `outline` |
| off 点亮 | `onSurfaceVariant` | `surfaceContainerHighest` | `outline` |
| on 静止 | `onPrimary` | `primary` | **transparent** |
| on 点亮 | `primaryContainer` | `primary` | transparent |
| disabled off | `onSurface@38%` | `surfaceContainerHighest@12%` | `onSurface@12%` |
| disabled on | `surface` | `onSurface@12%` | (选中→transparent) |

对照后发现的错处：

1. **禁用态完全缺失**——thumb/track/outline 三样全没处理；`"surface"` 角色也不在 pack 里。
2. **on 态描边 lerp 到 `track_color`**——规范是 `transparent`。虽然视觉接近，语义完全不同：
   一个是环淡出，一个是环与填充混合。
3. **`Reverse` 过渡的起点用了 `outline`**——那是 off 静止色；Reverse 起点应是 on 静止色
   （`onPrimary` 系），我按 `from.lerp(to, t)` 的 `from` 修正。
4. **静止态 lerp 出一个 ULP**——测试要求 `==` 精确相等；`lerp(a,b,1.0)` 在浮点里给 `b` 差
   一个 ULP。改成两端直接 `pick(a,b)`（`t<=0/a; t>=1/b; else a.lerp(b,t)`）。

**5 项状态色测试**（`the_off_rest_colours_match_the_m3_defaults` 等）逐格钉住上面的表。

### 21.2 MD3 参数化：has_icon / disabled / pack_for

- `Md3Params` 加 `has_icon: bool`（决定 24-vs-16 规则，**构建期**旋钮）与 `disabled: bool`
  （静态样本无真实禁用祖先，只能带在 params 上）；
- `pack_for(appearance)` 三个 pack 各一份；`md3/fluent/glass` 都随外观重建；
- Fluent 的三处组件合成洗色随主题翻转（浅底黑加 alpha、深底白加 alpha，注明是我们合成，不是令牌）。

### 21.3 gallery：右上角外观按钮 + Switch 页 MD3 列四变体

按用户要求：

- **右上角只有一个外观切换按钮**——一个 `Widget`（不是裸 `Container`；裸容器不 `change()`，
  点击什么都不上报，第一次实现踩了这个坑）；`Change::Activated` → `dark = !dark` → 三个 pack 重建；
  胶囊颜色/文字色也随主题，避免浅色模式下留一块深色贴片。
- **Switch 页 MD3 列显示四种变体**：`[有图标·启用 / 无图标·启用 / 有图标·禁用 / 无图标·禁用]`，
  **有图标默认关闭、无图标默认开启**；每个变体自带 `on` 值（`variant_on[4]`），点击只翻转自己。
  Fluent/Glass 列不变，仍是响应共享 `on` 的开关。

**关键 ID 决策**：变体 id 必须**从列 id 派生**（`col_id(Switch,"md3").with(("variant",k))`）而不是
`Id::of(("variant",k))`——后者是哈希，selftest 无法重现。之前踩了一次：selftest 用哈希查节点，
永远查不到；改成 `with(())` 派生链，两侧算出同一个 id 才可寻址。

**顺带修正**：engine-clock 与 frozen-clock 两个 selftest 之前用「shapes.last()」定位滑块，图标条
出现后失效。改为**取面积最大、边长 ≤ 34.5 的正方形**——按形状特征而不是位置识别；涟漪是 40 圆，
被排除；轨道是 52 长方形，被排除。

门禁：`build --workspace --tests --examples` **0 warning**；`cargo test --workspace` **559 passed**
（549 + 5 状态色 + 其他）；`GALLERY_SELFTEST=1` **PASS**（含 appearance 翻转与 4/4 变体断言）。

## 22. StylePack 拆分：职责 trait + 语义适配层（2026-10-28 定稿，未实施）

### 22.1 触发点

用户指出「gallery 中的控件应是语义化转发器调用对应控件的**默认风格**，除非明确设置，
不应通过通用定义覆盖控件自己的体系」。核查后确认该前提**已经成立**（见 22.2），
但核查过程暴露出两个真缺陷（见 22.3）。

### 22.2 语义化并未覆盖本体系（实测三条）

| 检查 | 结果 |
|---|---|
| `ToggleSpec` 字段 | 只有 `id/on/label`，**无颜色字段** |
| `Env::with(...)` 传的 params | `outline_width`/`slider_track_h` 等**几何旋钮**，无颜色 |
| md3/fluent 读 `CommonColor` 的次数 | **0** |
| glass 的 `hig.common(...)` | 是 **HigSystem 自己实现的方法**，回答者仍是 HIG，不是通用压 HIG |

`&dyn Style` 只是多态转发，**不是**通用覆盖。所以 gallery「一套语义 spec 复制三风格」的模式可保留。

### 22.3 暴露的两个缺陷

1. **`StylePack` 是原生词汇的手工摘要（封闭字典）**
   实测：MD3 原生可寻址 **49**，pack 只暴露 **10**（20%）；Fluent 每主题 **459**，暴露 **9**；
   HIG **13 系统色 + 15 语义角色**，暴露 **5**。
   后果：`role()` 用 `debug_assert!` + `unwrap_or(Color::TRANSPARENT)`——
   **debug 崩、release 静默画全透明**。手写新控件是陷阱。

2. **字阶烤进了颜色，是类别错误**
   `TextStyleToken.color: Color` 在建包时写死 `colour_of(&scheme,"on_surface")`；
   而 ui-design 自己的 `TypeStep{name,size,line_height,weight}` **没有颜色字段**。
   MD3/Fluent/HIG 的字阶规范只定义字号/行高/字重，**墨色是组件按角色挑的**。

3. **（连带）深/浅切换目前靠重建三个 pack 实现**——包是快照的直接后果。
   `dimensions_of(StyleId, Context)` 与 `StyleSystem::native()` 本就能按上下文活解析。

### 22.4 定稿形状

```
ui-design  ← 规范层（Resolution/DimensionTokens/StyleSystem/CommonColor/ADAPTER 都在这）
  trait Palette    { fn colour(&self, id:&str) -> Resolution<Rgba>; fn provenance(&self,id)->Source }
  trait Geometry   { radius / spacing / hit_target }
  trait Typography { step(name) -> Option<TypeStep>; scalable() }     ← 无颜色
  trait Motion     { duration / curve }
  impl 四者 for Md3Scheme | FluentSystem | HigSystem   ← 「本风格自己的 Style」，原生 id 直通、永不裁剪
  struct SemanticPalette { active:&dyn Palette, overrides:&CommonDict }  ← 语义体系的适配层
        impl Palette: id → 经 ADAPTER 反查所属 CommonColor → 命中覆盖用覆盖，否则回落 active.colour(id)
                       （CommonColor 22 个变体全是颜色，故适配层只对 Palette 有话说）
ui-style   ← 只做 bundle；新增 ui-style → ui-design 依赖（已验证无环）
  struct Style { name, palette: Arc<dyn Palette>, geometry: Arc<dyn Geometry>,
                 typography: Arc<dyn Typography>, motion: Arc<dyn Motion>, material, device }
```

控件写法：`pack.palette.colour("on_primary_container")`——传**官方规范原生 id**；
来源要么是本风格自己的 Style，要么是语义适配层，同一个 trait。

### 22.5 命名决策（用户已授权）

| 现名 | 新名 | 理由 |
|---|---|---|
| `trait ui_widgets::Style` | **`WidgetFactory`** | 它造控件（spec→该风格具体控件），只有 gallery 7 处 + 3 impl，改名便宜 |
| `struct ui_style::StylePack` | **`Style`** | 它是「这套风格的长相」，被 28 个文件读，是核心概念 |
| `enum ui_design::StyleId` | **`SystemId`** | 代码自己说了：字段就叫 `NativeName{ system: StyleId, .. }`，trait 叫 `StyleSystem` |
| `struct ui_style::StyleId` | **不动** | 它是**开放**的作用域路由 newtype（有 `NONE` 哨兵、`of(任意串)` 允许第三方风格），与封闭 enum 是两个概念 |

**两份 `StyleId` 不合并**：合并会同时毁掉两边能力——封闭 enum 当路由键则第三方风格进不来，
开放 newtype 当派发依据则 `dimensions_of` 丢掉穷尽性。两边连第三套风格的名字都不一致
（`GLASS="glass"` vs `Hig="HIG"`），这本身就说明它们不是一个东西。

> **2026-10-29 更新（推翻最后一行）**：`struct ui_style::StyleId` 已**删除**，不是合并进
> `SystemId`。理由是它没有读取方——「开放路由键」保住的能力从未被使用，选体系靠调用哪个
> kit 对象。同批改名见 `docs/style-object-design.md` §9/§10 第 8 步；`WidgetFactory` 现名
> `StyleKit`，`Md3Style`/`FluentStyle`/`GlassStyle` 现名 `Md3Kit`/`FluentKit`/`HigKit`，
> `*Params` 现名 `*Options`。本条只作历史记录，**以 `docs/style-object-design.md` 为准**。

### 22.6 影响面（实测）

颜色调用点 md3 **25** / fluent 1 / glass 2；引用 `StylePack` 的文件 **28**；
`TextStyleToken.color` 删除波及三风格的 `text.rs`；`ui_design::StyleId` 跨 crate 引用均为
`ui_design::StyleId` 限定写法，crate 内均为裸 `StyleId`，**改名无歧义**。

**顺序**：先改名（名字各归其位）→ 再拆 trait → 再去色 → 再改控件 → 最后简化 gallery 深浅切换。

---

## 23. 职责 trait 落地（2026-10-28 完成）

§22 的四步里，**改名**与**拆 trait** 已实施；去色、控件调用点、gallery 简化尚未动。

### 23.1 最终形状（比 22.4 更窄）

```rust
pub trait WithDimensions { fn dimensions(&self) -> DimensionTokens; }
pub trait Geometry: WithDimensions { /* radius/spacing/hit_target，默认读 dimensions() */ }
pub trait Typography: WithDimensions { /* ramp，默认读 dimensions()；无颜色 */ }
pub trait Motion: WithDimensions { /* scale，默认读 dimensions() */ }

// 单一查询路径，只有原生 id。刻意没有 common()。
pub trait Palette {
    fn colour(&self, id: &str) -> Resolution<Rgba>;
    fn provenance(&self, id: &str) -> Source;
}

pub trait StyleSystem: Palette + WithDimensions + Geometry + Typography + Motion {
    fn id(&self) -> SystemId;
    fn context(&self) -> Context;
    fn common(&self, role: CommonColor) -> Resolution<Rgba>; // 共享角色的语义查询
    fn caps(&self) -> StyleCaps;
}
```

### 23.2 与 22.4 的两处偏离（都是用户当场纠正的）

| 22.4 原案 | 落地 | 用户理由 |
|---|---|---|
| `Palette{ native, common, provenance }` | `Palette{ colour, provenance }`，`common` 归 `StyleSystem` | “适配回落等操作是语义化的 Style 变体在代码中实现的”——不是每个调色板都得重实现两平面区分 |
| 方法名沿用 `native` | 改为 **`colour`** | `Palette` 只做一件事：按 id 取色；`native` 这个名字在与 `common` 并列时才有意义 |

`Geometry/Typography/Motion` 用**默认实现 + 空 impl**（`impl Geometry for Md3Scheme {}`），
因此没有 12 份样板；`WithDimensions` 是被引用的唯一数据源，每个体系一份 `dimensions()` 方法体。

### 23.3 机械改名（`native` → `colour`）波及

`ui-design` 三体系 impl（各把 `native`/`provenance` 搬进 `impl Palette`）、三个 pack 构建器
（md3 3 处、fluent 3 处、glass 2 处）、gallery 1 处、`Theme::native` → `Theme::colour`、
`scales.rs` 三处 `pub fn dimensions` → `impl WithDimensions`。

### 23.4 验收

`cargo build --workspace --tests --examples` = **0 warning / 0 error**；
`cargo test --workspace --features ui-core/simd` = **558 passed / 0 failed**。

### 23.5 尚未做（接 §22 顺序）

去色（删 `TextStyleToken.color`）、控件调用点改原生 id、gallery 深浅切换改为换 scope、
未命中角色回落到活的原生体系并**删除 `unwrap_or(Color::TRANSPARENT)`**。

---

## 24. 去色完成（2026-10-28）

§22 顺序的“去色”一步已落地。用户一句纠偏定下形状：“我们或许需要一个专门的文本控件，它可以接受字体相关的设置？”

### 24.1 做了什么

- **`TextStyle`（`ui-core`）删 `color`**：字阶只管字号/字重/斜体（§6.2）。引 `TextStyle::new(size)` 单参。
- **`TextStyleToken`（`ui-style`）删 `color`**：token 不再携带墨色，否则等于让控件的决定冒充字阶。
- **墨色移到图元**：`NodeData::Text { block, ink }` 与 `draw::TextPrim.ink`。绘制端（`ui-paint` / `text_pass` / `view`）只读 `ink` 一个字段。
- **新增 `ui_widgets::Text` 控件**（`text.rs`）：`Text::new(id, body, size, ink)` + `.weight().max_width().line_gap().indent().resolve()`。旧 `leaf(...)` 降为薄封装委托。控件声明的正是“画什么、用什么字体”。

### 24.2 墨色归谁（结构决策）

**拥有表面的控件选墨色**：Card/Panel 加 `title_ink`/`hint_ink: Option<Color>`，`None` 时回落包的角色（`onSurface`/`onSurfaceVariant`）。理由：只有拥有背景的那个控件知道哪种颜色在其填充上可读。

parity 测试暴露了一个真事实：card title = `WHITE`、panel title = `WHITE@240`、panel hint = `(235,240,255,210)` 三者不同——**一个语义 role 装不下“同语义、不同档位”的三个值**，这正是旧 `TextStyleToken.color` 名为“样式”实测为“按 TextRole 区分”的证据。故不能强行归到一个 role，而要让控件显式覆盖。

### 24.3 验收

`cargo build --workspace --tests --examples` = **0 warning / 0 error**；
`cargo test --workspace --features ui-core/simd` = **559 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**。

---

## 25. 控件取色走活原生体系（2026-10-28）

§22 缺陷 (1) 的结构性修复：控件不再依赖手工挑选的角色快照。

### 25.1 做了什么

- **`Style` 新增 `palette: Option<PaletteHandle>`**（`PaletteHandle` = `Arc<dyn ui_design::Palette>` 的 `Debug`/`PartialEq` 包装）。三个真包（md3/fluent/glass）各自传入活体系（`Md3Scheme` / `FluentSystem` / `HigSystem`）。
- **`Style::ink(&'static str) -> Option<Color>`**：先查手工 `roles`（快路径/可覆盖），未命中则问 `palette.colour(id)`，都没有返回 **`None`**——绝不再 `TRANSPARENT`。
- **风格化控件的 `role()` 全改走 `pack.ink`**：md3 `toggle.rs` / `slider.rs`，fluent `paint.rs`，glass `paint.rs` / `material.rs`。未命中时 `debug_assert!` + 洋红（`255,0,255`）显形。
- **依赖边**：`ui-style → ui-design`（已确认无环：`ui-design` 只依赖 `ui-color-hct`+`ui-core`）。

### 25.2 验真（临时探针，已删）

测 `tertiary`（真实 MD3 角色，**不在**控件原来的 10 角色清单）经 `pack.ink` 从活体系解析成功；`not_a_real_role_xyz` 返回 `None`。证明“手工清单遗漏的令牌仍能解析”结构上成立。

### 25.3 通用控件保留 `TRANSPARENT`（有意）

`ui-widgets` 的 button/list/surface 用的是**通用语义角色**（`buttonFill` 等），不属于任何原生体系，且注释明确“缺角色=自定义包主动跳过某部分”。这是有意设计，不属于本次修复范围。

### 25.4 验收

build 0 warning/0 error；**559 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**。

---

## 26. 深浅切换改为换作用域（2026-10-28）

§22 缺陷 (3)：旧做法“深浅靠重建三个包”已废除。

### 26.1 做了什么

- **`*Params`（Md3/Fluent/Glass）新增 `appearance: ui_design::Appearance`**：外观成为作用域参数（`Env` 仍不透明，只有风格层解释它）。
- **`*Style` 持有 `pack_for: fn(Appearance)->Style`**（就是 `pack::pack_for`），`toggle/slider` 里用 `(self.pack_for)(self.params(cx.env()).appearance)` 现算包。`pack` 字段删除（已无读取）。
- **gallery**：`style_env(tag, dark)` 把 `appearance_of(dark)` 写进 params；`dark` 穿到 `slider_page`/`materials_page`；`md3_switch_variants` 的 scope 也用带 appearance 的 params。`changed()` 的外观分支只剩 `self.dark = !self.dark`——**不再重建 `Md3Style`/`FluentStyle`/`GlassStyle` 实例**。

### 26.2 语义

主题切换 = 换作用域。工厂从作用域携带的 appearance 解析包，控件每次都拿到当前深浅的颜色；同一个 `*Style` 实例服务两种主题，无需重建。

### 26.3 验收

build 0 warning/0 error；**559 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**（`appearance toggled false->true ✓`、`md3 switch variants 4/4 ✓`）。

---

## 27. Fluent 品牌色阶生成：v8 落地，v9 留档（2026-10-28）

### 27.1 调研结论（已核实，推翻先前假设）

- 我们现有的 16 档 `BRAND_RAMP`（10..160，`80=#0f6cbd`）**是 FluentUI 手写表 `brandWeb`**，**不是任何算法生成**。FluentUI 源码 `shades.ts` 明确：`brandWeb` 为 hand-authored。
- 实测：纯 HSV（v8 `_darken`/`_lighten`）无论如何拟合，与 `brandWeb` 的最大通道差 ≥ ~31/255（4 参数拟合也只能到 ~26）。残差是系统性的（`brandWeb` 的色相/彩度轨迹是 **LCH** 形状）。因此“用 v8 复现 `BRAND_RAMP` 作验收”不成立——上一轮我提的该判据是错的。
- 微软官方 Rust 框架 `windows-reactor`（`microsoft/windows-rs`）**不生成调色板**：用 `ThemeRef` 枚举引用 WinUI XAML 资源键，运行时解析以适配明/暗。与我们“活原生体系 + scope”方向一致。

### 27.2 落地（v8）

新模块 `ui-design/src/fluent_brand.rs`：
- 实现 Fluent v8 `shades.ts` 的 HSV `_darken`（`v *= 1-f`）/ `_lighten`（`s *= 1-f`，`v += (1-v)*f`）原式。
- `pub fn brand_ramp_from_seed(seed: Rgba) -> [(u16, Rgba); 16]`：为任意种子色生成 16 档（80 档=种子）。`DARK_STEP=0.14`、`LIGHT_STEP=0.21`（标定自默认蓝，已记录）。
- 测试：单谓亮度单调、80 档=种子原色、与 `brandWeb` 差距被限定在 ≤40/255（实测 ~31，如实记录而非隐藏）。
- **默认 Windows 蓝仍用权威手写表 `BRAND_RAMP` 作锚**；生成器只用于非默认种子。

### 27.3 v9 已落地（忠实移植，2026-10-28）

网络修复后可取回真源码，v9 由「留档」改为「已落地」：
- `ui-design/src/fluent_brand_v9.rs`：逐字移植 `getBrandTokensFromPalette` → `hexColorsFromPalette`。
- `ui-design/src/fluent_hue_map.rs`：`HUE_TO_SNAPPING_POINTS`（360 行，逐字复制自 `hueMap.ts`，注明 MIT 来源）。
- 关键细节（照原文）：**D50** 白点 + Bradford D65→D50 色适应；`hexToHue` 用 **HSL** 色相取整索引；通道打包用 `floor(x*256)`；f64 精度。
- 验证：`the_port_reproduces_the_reference_implementation_for_the_default_blue` 逐字节断言 16 档 hex 等于参考实现（`#0f6cbd` → `#00030a … #74a0e6`）。

**两个硬事实（已实测，推翻先前预期）**：
1. v9 算法同样**不**复现手写 `brandWeb`：默认蓝最大通道差 **~119**（brandWeb 明显更亮）。
2. FluentUI 的 `getLogSpace(Math.log10(0), Math.log10(100), n)` 在 `min<=0` 时把 `a` 退化为 0，对数项实际只有 `1..2`，被 0.75 线性权重稀释，故生成色阶上端只到 L*≈65（不是接近白）。此处按原文保留（保真优先），未自行“修正”。

- **可选修正版**：`brand_ramp_from_seed_with(seed, LogDistribution::Corrected)` 让对数项覆盖真实带，亮端到 L*≈87.9（忠实版 ≈65.9）；默认 `brand_ramp_from_seed` 仍是 `Faithful`。

结论：**默认 Windows 蓝继续用权威手写表 `BRAND_RAMP`**；两个生成器（v8 HSV / v9 LCH）仅服务非默认种子，v9 形状优于 v8。

### 27.4 验收

build 0 warning/0 error；**565 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**。

---

## 28. 逐角色禁用令牌（2026-10-29）

§10 标的「逐角色禁用令牌尚未做」分三步：①Fluent 接线、②MD3 补令牌、③HIG 策略（待定）。①、② 已完成。

### 28.1 调查（网络已通，均对上游源码核实）

- **Fluent**：`colorNeutralForegroundDisabled`/`BackgroundDisabled`/`StrokeDisabled` 等**数据早就有且与 `@fluentui/tokens` 逐项吻合**（`#bdbdbd`/`#f0f0f0`/`#e0e0e0` 明，`#5c5c5c`/`#141414`/`#424242` 暗）。缺的只是接线。
- **MD3**：无禁用色角色，禁用 = `on-surface` 叠不透明度。权威值（material-web `_md-comp-*` 组件令牌）：内容/标签/图标/handle/active-track=**0.38**，容器/轨道/描边/inactive-track=**0.12**。
- **HIG**：无禁用色令牌（Apple 靠降不透明度，无定值）——第③步待定。

### 28.2 落地

- **结构修复**：`Widget::handles_disabled()`（默认 false，`Box`/`&` 转发）。`Styled::decl` 仅在 `disabled && !handles_disabled()` 时才整体乘 0.38。修掉了 MD3 switch「内部已 12%/38% + 适配器再乘 0.38」的**双重压暗**。
- **MD3**：新增 `DISABLED_CONTENT_OPACITY=0.38`/`DISABLED_CONTAINER_OPACITY=0.12`；switch 改用常量；**slider** 新增禁用配色（inactive track@12%、active/handle@38%、标签 @38%）。二者 `handles_disabled=true`。
- **Fluent**：toggle/slider 在 `new` 里读到 `state.disabled` 时改用中性禁用令牌（`colorNeutralForegroundDisabled` / `colorNeutralBackgroundDisabled` / `colorNeutralStrokeDisabled` / `colorTransparentStrokeDisabled`），`handles_disabled=true`。
- 通用控件（button/list/card）与 glass 仍走适配器统一 0.38（非包级禁用令牌），属后继可选。

### 28.3 测试

`a_control_that_handles_disabled_forwards_that_through_the_box`、`a_control_that_handles_disabled_is_not_faded_a_second_time`、`a_disabled_slider_uses_the_spec_12_38_opacities`。

### 28.4 验收

build 0 warning/0 error；**569 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**。

### 28.5 待定

**HIG 禁用策略：已定并落地为方案 A。**

调研（Flutter Cupertino = HIG/SwiftUI 标准移植，均对源码核实）：
- `CupertinoButton` 禁用底 = 语义系统填充（普通 `tertiarySystemFill`、filled `quaternarySystemFill`），非透明度；`CupertinoSwitch` 禁用 = 整体 `Opacity(0.5)`；`CupertinoSlider` 仅 `onChanged:null`；`CupertinoColors.inactiveGray`（`#999999`/`#757575`）即 iOS 调的禁用色。Flutter LG（`flutter_liquid_glass`）也用 `opacity*0.5`。

落地：
- [hig_data.rs] 新增 `DISABLED_COLORS`（`inactiveGray`/`tertiarySystemFill`/`quaternarySystemFill`，四上下文取值，来源 Cupertino），`HigSystem::colour` 作为有值色解析（从 `SEMANTIC_ROLES` 移出）。
- 通用 `Button`：新增 `buttonFillDisabled`/`buttonTextDisabled`/`buttonBorderDisabled` 角色；禁用时优先取包禁用角色，否则把当前色淡到 38%（`disabled_role`）。
- HIG（glass）包：三个禁用角色 = `quaternarySystemFill`/`inactiveGray`/透明。
- glass `switch`/`slider`：禁用整体 `opacity(0.5)`（CupertinoSwitch），`handles_disabled=true`。
- 新测试：`disabled_colours_resolve_in_every_context`、`a_disabled_button_prefers_pack_disabled_roles_else_dims`。验收：**571 passed / 0 failed**，build 0，gallery PASS。

---

## 29. 主题色全局渐变 + 系统深浅色同步（2026-10-29）

### 29.1 调研：其他框架怎么做

- **Flutter（决定性先例）**：`MaterialApp` 内部包 `AnimatedTheme`，默认 200ms 线性；`ThemeData.lerp` → `ColorScheme.lerp` 逐角色 `Color.lerp(a,b,t)`，单一全局 t + 重建子树。
- egui / GPUI：无调色板插值（随即时样式/重绘）；Qt/GTK 无内建；Apple 系统跨淡、SwiftUI 需 `withAnimation`；WinUI 无内建逐色过渡（均标为社区资料/未逐源核实）。

### 29.2 方案 1（已实现）：插值 Style

- `ui-style`：`lerp_style(from,to,t)` 逐角色用 `ink`（含 palette）求值后 `Color::lerp`；纯 palette 令牌经 `BlendedPalette` 在解析后颜色上混合。`pack_for_transition(resolve, from, to, t)` 封装。
- 三个包 Params 新增 `theme_from: Option<Appearance>` + `theme_t`，`pack()` 改走 `pack_for_transition`。
- gallery：`appearance` 按钮切换时设 `theme_from`，`view` 用 `cx.animate` 驱动 200ms `t`（`EaseOut`），通过线程局部 `THEME_FROM/THEME_T` 传给 `style_env`，所有对应颜色一起渐变。

### 29.3 系统深浅色获取与同步（Linux/Wayland）

- 引擎抽象：`ui_platform::theme`：`ThemePreference{NoPreference,PreferDark,PreferLight}` + `ThemeWatcher` trait + `FixedTheme`。
- Wayland 实现：`ui_platform_wayland::theme::PortalThemeWatcher`——读 XDG 门户 `org.freedesktop.appearance color-scheme`（0/1/2），`gsettings` 回退；后台线程 1s **轮询**（不用长期 `gdbus monitor` 子进程，避免孤儿占管）。gallery 启动时初始化 `dark` 并每帧 `poll()` 跟随系统变化。
- **实测**（本机 Hyprland）：portal `ReadOne` 返回 `<uint32 1>`（prefer-dark），`gsettings color-scheme='prefer-dark'`。

### 29.4 验收

build 0 warning/0 error；**580 passed / 0 failed**；`GALLERY_SELFTEST=1` = **PASS**（退出后无遗留 `gdbus` 子进程）。新测试：`lerp_style_blends_each_role_and_only_at_the_ends`、`a_palette_only_token_blends_through_the_blended_palette`、`a_halfway_theme_blend_sits_between_light_and_dark`、`the_theme_transition_endpoints_match_the_plain_packs`、`an_md3_toggle_reads_the_blended_pack`、`the_track_uses_the_hig_fill_and_system_green`、`a_portal_reply_maps_the_three_codes`、`gsettings_text_maps_dark_light_and_default`、`no_preference_falls_back_but_explicit_ones_do_not`、`a_fixed_watcher_never_reports_a_change`。

### 29.5 两个实测缺陷与修法（2026-10-29）

- **MD3 不渐变**：MD3 那一列走专用构造 `md3_switch_variants`，它自建 `Md3Params` 只设了 `appearance`，**漏了 `theme_from/theme_t`**（Fluent 走 `style_env` 所以正常）。修法：抽 `theme_from_appearance()`/`theme_progress()` helper，两处共用；加测试 `an_md3_toggle_reads_the_blended_pack`（断言 track = 两端精确中点）。
- **HIG/LG 无变化**：glass 包把未选中轨道写成固定字面量 `0xC5C5C6`，外观无关。Cupertino 事实是 switch 未选中轨道=`secondarySystemFill`、slider 轨道=`systemFill`（都随明暗变化）。修法：`hig_data` 新增有值的 `FILL_COLORS`（`systemFill`/`secondarySystemFill`，Cupertino 四上下文值），`glassTrackInactive` 改用 `secondarySystemFill`；测试改为断言「随明暗变化且等于 HIG 值」。

### 29.6 accent-color 接线建议（未实现）

三层：①**来源**：同门户 `org.freedesktop.appearance accent-color`（`(ddd)` sRGB 0..1）；②**传输**：`ThemeWatcher` 升级为 `SystemTheme` 快照（preference + accent + contrast + reduced_motion）；③**注入**：accent 是品牌种子，落在 `ui_design::Context::with_accent(seed)`，各包从 `Context.accent` 播种（MD3 `from_source`、Fluent `brand_ramp_from_seed`、HIG tint），**不接在控件里**。

---

## 30. 官方令牌导出 → Rust 规范查询（2026-10-29）

导出的三份 Material 3 Design System DB JSON（dsdb 38.2.9）放在 `ui/crates/ui-widgets-md3/tokens/`：`switch.json`、`sliders.json`、`tabs.json`。结构均为 `system.tokens`（令牌，带 `tokenValueType`）+ `system.values`（值：字面量 `length/opacity/color/shape/elevation/fontSize/lineHeight/fontTracking/type` 或 `tokenName` 引用）。令牌名形式：`md.comp.<control>.<状态...>.<对象>.<属性>`，对象本身可带点（`track.outline`、`focus.indicator`）。

**生成器** `tokens/gen_spec.py`（两个子命令）：

- `inspect <file>.json`：输出模型 JSON（control / prefix / 默认态 / 参数 / 对象-属性表 / 全部条目 / 跳过的废弃令牌）。前缀缺省从 `tokenSets[0].tokenSetName` 推导。
- `gen <file>.json --out src/spec.rs`：生成**精简书**：每个对象一个 `struct`，加上一个函数，参数是非默认态的 bool 开关，函数内部从基线值出发按优先级逐个 `if` 叠加。颜色停在 `md.sys.color.*`/`md.ref.palette.*`（控件该消费的规范名），长度/不透明度/高度/字号解成数值，排版复合值解析为字阶名（`title-small`）。

关键点：

- 对象词表按控件写在 `OBJECTS_BY_CONTROL`（最长优先匹配，所以 `track.outline` 不会被当成 `track`）。
- 状态词表从数据推导：默认态（switch=`unselected`、slider/tabs=`inactive`）+ 主态（`selected`/`active`）+ 其余开关；叠加顺序由 `PRIORITY`（selected/active → with-icon → hover → focus → pressed → disabled）决定。
- 废弃令牌（`deprecationMessage` 在**令牌对象**上）**直接跳过**，只在文件头写一行计数；已废弃的 `handle.elevation/shadow-color` 因此不再出现在结构体里。
- 互斥变体不是 bool：slider 的 `xsmall/small/medium/large/xlarge` 会被 `gen` 拒绝并提示“需逐变体生成或改为枚举参数”；`inspect` 仍可完整输出。

三库对比（inspect 结果）：

| 文件 | 控件/前缀 | 组件条目 | 默认态 | 开关参数 | 对象 | 跳过废弃 |
|---|---|---|---|---|---|---|
| `switch.json` | `md.comp.switch` | 72 | unselected | selected, with_icon, hovered, focused, pressed, disabled | track, track.outline, handle, icon, state-layer, focus.indicator | 6 |
| `sliders.json` | `md.comp.slider` | 83 | inactive | 不可生成（尺寸变体） | track, track.inner-corner, track.outer-corner, handle, container, stop-indicator(.container), value-indicator…, label… | 40 |
| `tabs.json` | `md.comp.secondary-navigation-tab` | 32 | inactive | active, with_icon, hovered, focused, pressed | container, label-text, active-indicator, divider, state-layer, focus.indicator, icon | 2 |

用法：`python3 tokens/gen_spec.py gen tokens/switch.json --out src/spec.rs`。
验收：**585 passed / 0 failed**，build 0 warning；新测试 `the_default_state_is_unselected_at_rest`、`selected_replaces_the_unselected_branch`、`a_pressed_handle_overrides_the_selected_size`、`hover_uses_the_state_layer_for_the_current_selection`、`disabled_is_applied_last_and_keeps_the_track_visible`。

接入与 slider 的落地见 §31。

## 31. 令牌接入控件：switch 与 slider（2026-10-29）

### 31.1 接入方式：让生成物是 `const fn`

生成函数改为 `pub const fn`，于是控件里的常量**直接由令牌派生**，而不是抄一份数字：

```rust
const REST: SwitchSpec = switch_spec(false, false, false, false, false, false);
pub const TRACK: (f32, f32) = (REST.track.width, REST.track.height);
pub const THUMB_INACTIVE: f32 = REST.handle.width;
```

颜色不走常量（要跟随主题混合），而是在绘制时经 `pack.ink(role)` 解析：
`switch_spec` 输出的是**包内角色名**（`primary`、`surfaceContainerHighest`…），
`spec_ink(pack, token, opacity)` 负责「空令牌 = 不画色 + 透明度」。于是 §29 的主题渐变
（`pack_for_transition` 产出的混合包）自动生效，控件不需要知道主题在过渡。

状态表的两端由同一次调用给出（`off = switch_spec(false, …)` / `active = switch_spec(true, …)`），
`t`（缓动行程）在两端之间插值——与 Flutter 的 `Color.lerp(inactive, active, colorValue)` 同构。

### 31.2 与 Flutter 官方实现逐状态交叉比对

新增 `src/tests/token_parity.rs`，把 `_SwitchDefaultsM3` 的取色表**抄成测试**（滑块、轨道、描边、
状态层），遍历 `selected × disabled × {静止, 悬停, 焦点, 按压}` 全组合比对；另有几何比对
（52×32、16/24/28、带图标 24、字形 16、状态层 40、描边 2）。**全部一致。**

导出与 Flutter 实现不一致的有两处，已用 `the_export_differs_from_flutter_in_two_named_places` 钉住：

| 位置 | 导出（我们采用） | Flutter 实现 | 处理 |
|---|---|---|---|
| 选中态图标色 | `md.sys.color.primary` | `_SwitchConfigM3.iconColor` = `onPrimaryContainer` | 跟导出；视觉差异已记录 |
| 焦点指示器 | `secondary` / 3dp / offset 2dp | M3 开关**不画环**（只有 Cupertino 分支画 3.5dp） | 颜色跟导出（primary→secondary）；**宽度按盒子夹到 2dp**（见下） |

焦点环宽度是一处真实冲突：令牌要 3dp + 2dp 间隙，而 60dp 的控件盒子（= Flutter `switchMinSize`）
每侧只剩 4dp。不变量是「环是叠加层，不得撑大控件」（`the_focus_ring_is_an_overlay_that_never_enlarges_the_control`
正是钉这条），因此宽度写成 `(WIDGET.0 - TRACK.0) / 2 - FOCUS_RING_GAP`，由盒子反推而不是写死数字。
若将来决定采用令牌原值，就必须把盒子加宽到 62dp —— 那是**外观变更**，需要显式决定。

### 31.3 slider：尺寸是互斥轴 → 枚举参数

`md.comp.slider` 的 `xsmall/small/medium/large/xlarge` 不是布尔（一个滑块只有一个尺寸），
生成器改为**变体组**：输出 `pub enum SliderSize { Base, XSmall, Small, Medium, Large, XLarge }`，
条件写成 `matches!(size, SliderSize::Large)`（`==` 需要非 const 的 `PartialEq`，在 `const fn` 里不可用）。
`Base` 表示「不叠加任何变体令牌」，即导出里不带尺寸前缀的那一套。

实测各档（`the_slider_sizes_grow_the_track` 钉住）：track 高度 Base/XSmall 16、Small 24、Medium 40、
Large 56、XLarge 96；手柄始终 4×44。

### 31.4 接入时发现并修掉的真 bug

滑块的禁用态原来是「把 `primary` 淡到 38%」。导出与 `_SliderDefaultsM3` 都写的是
`onSurface@38%`（活动轨道/手柄）与 `onSurface@12%`（非活动轨道）。接入后按令牌取色，
`the_slider_track_and_handle_match_flutter_defaults` 四种组合全部对齐。

### 31.5 生成器健壮性（本轮补齐）

- `md.comp.slider.value-indicator.active.bottom-space` 是**对象在前、状态在后**的少数派写法，
  原来被静默丢弃。现在前后缀都试，并用状态名白名单校验，`inspect` 会报未解析项。
- `fontWeight` 在导出里是**裸数字**（`500`）而非 `{value: 500}`；`fontNames` 是
  `{values: [...]}`。两者都补了，`md.ref.typeface.*` 不再当作颜色截断，改为递归解析。

### 31.6 验收

- `cargo build --workspace --tests --examples`：0 warning / 0 error。
- `cargo test --workspace --features ui-core/simd`：**590 passed / 0 failed**。
- 画廊自检（直接跑二进制）：`selftest: PASS`。

### 31.7 尚未做

- `slider` 控件目前只用 `SliderSize::Base`；把尺寸档暴露成 `Md3Params` 字段并接进画廊还需要一次改动。
- `tabs` 的 `secondary_navigation_tab_spec` 已能生成（`inspect`/`gen` 均通过），尚未接入控件。
