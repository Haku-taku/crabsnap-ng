# Style 类完整设计（重构稿）

> 状态：**已实施**（2026-10-29；§10 第 0–8 步全部落地）。
> 本文给出 `ui-style::Style` 重构后的完整形状：**两个平面**、语义层与风格层的契约、
> 解析 / 缓存 / 过渡、家族选项与迁移步骤。§1、§2 是**重构前**的现状盘点与参考框架
> （调研已完成），其中出现的 `Md3Style`/`Md3Params`/`StyleId` 等是当时的名字；
> 附录保留 27 条缺陷清单——每条都能对应到 §10 的某一步。

## 1. 现状（重构前）：一个类承担了四件事

`ui-style::Style`（`crates/ui-style/src/pack.rs:42-64`）现在是九字段扁平结构体：

```rust
pub struct Style {
    pub name: &'static str,
    pub roles: RoleDict,              // HashMap<&'static str, Color> + 别名表
    pub palette: Option<PaletteHandle>, // Arc<dyn ui_design::Palette>
    pub motion: MotionTable,          // 4 个固定槽位
    pub metrics: MetricsSet,          // 3 个固定槽位
    pub type_table: TypeTable,        // 5 个固定槽位
    pub material: Option<Material>,
    pub device: Capability,
}
```

它同时是四样东西：

| 它想当的 | 实际状态 |
|---|---|
| 设计体系的令牌表 | `ui-design` 已经有 `StyleSystem`（Palette + Dimensions + Geometry + Typography + Motion）、`CommonColor` 22 个跨体系角色、`Resolution<T>` 带原因。`ui-style` 没用它，另起了一套自由字符串 |
| 一次主题过渡的结果 | `lerp_style` 只混颜色，其余令牌取 `to`（`pack.rs:161-170`），所以过渡开始时圆角/字号/动效会瞬跳 |
| 作用域参数 | `Env` 链只做整份 `params` 的 downcast，没有逐字段合并（`env.rs:86-95`）；`StyleId` 存了但从不参与解析 |
| 逐控件的几何来源 | MD3/Fluent/Glass 的设计系统控件**一个都不读** `metrics`/`motion`（读它们的是通用 `ui-widgets` 控件）；MD3 的卡片 12、对话框 28、芯片 8 全是常量（`card.rs:23`、`dialog.rs:20`、`chip.rs:28`） |

### 1.1 盘点出的具体缺陷（摘要）

完整清单见 §9 附录；影响设计的核心是六条：

1. **没有构造器**：`Style` 无 `Default`、无 `with_*`，只能九字段 struct literal 构造。加一个字段就打断所有构造点
   （`ui-widgets/src/button.rs:142-150`、`list.rs:86-95`、`surface.rs:231-239`）。
2. **传进来的 `Style` 被丢弃**：`Md3Style::new(_pack)` / `with_params(_pack, ..)` 把参数忽略，
   `pack_for` 固定指向 crate 内部（`md3/style.rs:60-71`）。调用方无法注入自定义包。
3. **`ink()` 混了两个命名空间且丢了原因**（`pack.rs:79-90`）：先查手工角色、再查 palette，
   两者都是字符串；`Resolution::Unmapped` 与 `Absent(Reason)` 都在 `.value()?` 处变成 `None`，
   `provenance()` 完全丢弃。缺失令牌在 release 下只剩品红像素（`md3/toggle.rs:160-164`）。
4. **`PartialEq` 不是值相等**：`PaletteHandle` 用 `Arc::ptr_eq`（`pack.rs:34-38`），
   而 `pack_for` 每次都新建 `Arc`，于是 `pack_for(Light) != pack_for(Light)`。
5. **每控件每帧重建**：`pack_in` 在每个控件构建时调用（`md3/style.rs:79-82` 等 11 处），
   每次重建 49 个角色的 HashMap，无缓存、无同帧共享。
6. **`Style` 不是 `Send + Sync`**（`Palette` trait 无约束，`ui-design/src/capabilities.rs:21`），
   因此不能作为 scope params 放进 `Env`（后者要求 `Send + Sync`，`env.rs:41`）。

---

## 2. 参考框架的「样式对象」形态

| | egui 0.36.2 | GPUI 0.2.2 | Flutter |
|---|---|---|---|
| 形态 | 扁平嵌套结构体：`Style { text_styles: BTreeMap<TextStyle, FontId>, spacing, interaction, visuals, animation_time }`（`style.rs:244`） | 扁平 CSS 风格结构体约 50 字段 + 自动生成的 `StyleRefinement`（Option 字段）（`style.rs:180`） | 巨型 `ThemeData` + 子主题对象 + `Map<Object, ThemeExtension>` 泛型扩展槽（`theme_data.dart:220,986`） |
| 颜色查找 | 强类型字段/枚举，**没有字符串查找** | 强类型字段（`Hsla`、`Fill`）；主题色是 `ThemeColors` 的具名字段 | 强类型 `ColorScheme` 字段 + 按类型键的扩展 |
| 局部覆盖 | `Ui` 持 `Arc<Style>`，`style_mut()` 写时复制；`StyleModifier` 是 `Fn(&mut Style)` | 元素存 `StyleRefinement`，`refine()` **逐字段**覆盖 | `ThemeData.copyWith`；子树 `Theme` InheritedWidget |
| 主题切换 | `set_theme` 在 dark/light 两份 `Style` 间**整体切换**，不插值 | 替换全局 `Arc<Theme>` | `ThemeData.lerp` + `_lerpThemeExtensions` |
| 引擎是否认识具体设计系统 | 不认识；核心只有通用 `Visuals` | 不认识；语义颜色在独立 `theme` crate | 不认识；`widgets/` 只认 `InheritedWidget`，`material/` 才有 `ThemeData` |

**三条结论**：

1. **令牌一律强类型**。三个框架都没有「按字符串查颜色」的 API。
2. **覆盖是逐字段的**，不是整份替换。egui 用闭包改，GPUI 用 Option 字段 refine，Flutter 用 `copyWith`。
3. **引擎核心不认识具体设计系统**。我们反过来：`ui-style::Style` 里嵌了 `ui_design::Palette` 与 `Appearance`（`pack.rs:26,176-179`）。

---

## 3. 设计命题

> **`Style` 是「某设计体系在某个上下文下，于本帧解析出的令牌快照」。**
> 它不可变、按值相等、可共享、可缓存；它不认识任何具体控件，也不承载逐控件几何。

五条推论：

- **解析与使用分离**：`Style` 由 `(SystemId, Context, 过渡进度)` 解析出来，一帧只解析一次；
- **两个平面**：跨体系语义走封闭枚举（`CommonColor` / `CommonMetric` / `TypeRole` / `MotionName`），
  体系独有的走原生名（保留 `Resolution` 与 `Source`）——两者都必须在快照里，见 §4；
- **令牌强类型**：通用控件只读封闭枚举，永远不写字符串；
- **逐控件几何不进 `Style`**：它属于各包自己的生成令牌表（MD3 已有 `spec/*_tokens.rs`）；
- **作用域只装上下文**：`Env` 里放 `Context` 与过渡进度；**家族构建旋钮**是另一回事，见 §8.3。

---

## 4. 两个平面与体系收窄

### 4.1 结构

```rust
/// The cross-system plane: one system's answers to the closed common lists, resolved once.
/// 跨体系平面：某体系对封闭公共清单的一份答案，解析一次。
#[derive(Clone)]                       // PartialEq 手写：Arc 不参与比较
pub struct CommonPlane {
    system: SystemId,
    context: Context,
    /// The system itself: the entry to its own vocabulary, and to generation. Shared.
    /// 体系本身：既是其自有词汇的入口，也是生成能力的来源。共享持有。
    source: Arc<dyn StyleSystem>,
    /// One entry per `CommonColor`, resolved at construction. A refusal stays a refusal.
    /// 每个 `CommonColor` 一条，构造时解析。拒绝仍是拒绝。
    colours: [Resolution<Resolved<Color>>; CommonColor::COUNT],
    metrics: [Resolution<Resolved<f32>>; CommonMetric::COUNT],
    caps: StyleCaps,
}

pub struct Style {
    /// `None` only for a hand-written pack that names no design system (`legacy_pack`).
    /// 只有不指名体系的手写包（`legacy_pack`）为 `None`。
    common: Option<CommonPlane>,
    // 以下字段是过渡期的家族平面，由第 3、7 步逐步删除：
    name, roles, motion, metrics, type_table, material, palette, device,
}
```

`Style` 的目标形状是「`common` + 体系句柄 + 材质 + 设备能力」四件事；今天仍是八字段，
因为家族控件还在读 `roles` / `metrics` / `type_table`。**先让公共平面立起来并逐帧解析一次，
再逐个把读取方搬过去**——反过来做会先拆掉控件唯一的颜色来源。

`Option` 只为止住 `legacy_pack`：它故意不属于任何体系。等它退场，`common` 变回非可选。

### 4.2 为什么必须两个平面（不能只有"翻译"）

语义层把通用令牌翻译成家族令牌是**单向投影**：22 个公共角色对 MD3 的 49 个角色，
`onPrimaryFixedVariant` / `surfaceTint` / `inversePrimary` 都没有公共对应物，任何
common → native 的映射都造不出它们。所以：

| 谁 | 读哪个平面 | 入口 |
|---|---|---|
| 通用控件（`ui-widgets::Button`） | 公共平面 | `style.colour(CommonColor::Accent)` / `style.metric(CommonMetric::Radius)` |
| 家族控件（`Md3Chip`） | 家族平面 | `Md3Tokens::of(&style)?` → `tokens.role(Md3Role::OnPrimaryContainer)` |
| 两者都要的跨族语义 | 公共平面 | 同上；家族实现里的 `common_colour` 已决定它对应哪个原生令牌 |

**翻译知识只存在于 trait 实现里**（§5），且只在 `Style::resolve` 期跑一次。家族控件不重新翻译，
它直接拿自己那一平面。

### 4.3 体系收窄：`Md3Tokens::of(&Style)`

```rust
// ui-widgets-md3/src/tokens.rs
/// MD3's own vocabulary, narrowed from a snapshot. Fails when the snapshot is another system.
/// 从快照收窄出的 MD3 词汇。快照属于别的体系时失败。
pub struct Md3Tokens<'a> { scheme: &'a Md3Scheme }

impl<'a> Md3Tokens<'a> {
    pub fn of(style: &'a Style) -> Result<Self, Resolution<Color>>;
    pub fn role(&self, role: Md3Role) -> Color;
}
```

- 失败返回 `Resolution::Unmapped`——它的语义就是「该令牌属于另一套体系」（`value.rs:44-46`），
  不需要新类型，也不需要 panic；
- `Md3Role` 的 49 个变体由**一张表**同时生成变体名与字符串名（宏），并另加一条测试断言
  它和 `ui-design/data/md3_data.rs` 的 `ROLES` 逐名相等——于是「令牌名拼错」从运行期品红
  变成编译错误或测试红（附录 #3）。注意 `gen_spec.py` 生成的**不是**它：那个脚本处理的是
  `md.comp.*` 组件令牌，与方案的 49 个角色名是两套词汇；
- 家族内部的字符串（`md3/pack.rs:21` 的 `scale_radius("largeIncreased")`）留在家族内，不越界。

### 4.4 为什么是数组，且为什么带 `Source`

`CommonColor` / `CommonMetric` / `TypeRole` 都是封闭枚举，数组让「漏填一个」变成构造期错误
而不是运行期品红。`Source` 是 `Copy` 的 `&'static str` 枚举（`source.rs:8-20`），数组仍然是
`Copy` 的廉价结构。

**带 `Source` 是硬要求**：语义层会生成与补全缺失令牌（§5.4 的第 2、3 级），若快照只存颜色，
`Source::Spec`（厂商值）、`Source::Derived`（由该体系规则算出）、`Source::Convention`（跨体系约定）
在类型上无法区分。今天 HIG 的 `common()` 里已有 20 个手算值在冒充厂商令牌，就是这个缺陷的实例。

### 4.5 为什么保留 `Arc<dyn StyleSystem>`

体系独有的原生令牌无法枚举进公共数组，必须留一条按名字取值的路；生成能力（MD3 的
`palettes.tone()`、Fluent 的 `brand_ramp_from_seed`、HIG 的 `accent + neutral`）也只能从体系本身调用。
`Arc` 让 `Style` 克隆廉价，也让 `Send + Sync` 可行（`StyleSystem` 加约束，§10 第 0 步）。

---

## 5. 语义层与风格层的契约

**一个 trait，两套词汇，五个属性。** 这是"任何风格实现它就能被语义层动态切换"的唯一条件。

### 5.1 形状

```rust
pub trait StyleSystem:
    Palette + WithDimensions + Geometry + Typography + Motion + Send + Sync
{
    fn id(&self) -> SystemId;
    fn context(&self) -> Context;
    fn caps(&self) -> StyleCaps;

    // 通用词汇：语义层问，体系答「我的哪个令牌承担这个语义」
    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>>;
    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>>;

    // 家族词汇：按本体系自己的名字取值（Palette::native_colour）
}
```

`common_type` / `common_motion` / `common_material` 在 `Style` 需要它们时再加——
按「有调用方才加 API」的纪律，不在这一批预先造出来。

### 5.2 命名：属性 + 词汇

| 属性 | 通用（语义） | 家族（原生） |
|---|---|---|
| 颜色 | `common_colour(CommonColor)` | `native_colour(&str)` |
| 几何 | `common_metric(CommonMetric)` | `native_metric(&str)`（有调用方才加） |
| 文字 | `common_type(TypeRole)` | `native_type(&str)`（同上） |
| 动效 | `common_motion(MotionName)` | `native_motion(&str)`（同上） |
| 材质 | `common_material()` | 无（材质不可命名） |

**`common()` 这个名字作废**：它只回答颜色，加了几何之后名不副实；一个方法也无法同时返回四种类型。
同理 `type_style(role)` 作废——`typography` 命名的是整条字阶（`TypeRamp`）而不是单个角色的值，
方法名统一用 `common_type`，类型名保留 `TypeStyle` / `TypeRole`（与 `TypeRamp` / `TypeStep` 同族）。

### 5.3 `CommonMetric`

```rust
/// A metric every design system can be asked to fill, or to refuse explicitly.
/// 每个设计体系都能被要求填上（或明确拒绝）的度量。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommonMetric {
    /// The default corner radius for a generic control.
    /// 通用控件的默认圆角。
    Radius,
    /// Stroke / hairline / divider width.
    /// 描边 / 细线 / 分隔线宽度。
    StrokeWidth,
}
```

三档清单（判据：① 有通用控件要问它；② 三套体系答案确实不同；③ 控件自己能决定）：

| 档 | 变体 | MD3 | Fluent | HIG |
|---|---|---|---|---|
| **现在就要** | `Radius` | 12（shape medium） | 4（borderRadiusMedium） | 12（medium） |
| | `StrokeWidth` | 1（outline） | 1（strokeWidthThin） | 0.5（发丝线约定） |
| **接线后才加** | `HitTarget` | 48 | 40 | 44 / 28（macOS） |
| | `ControlHeight` | 40 | 32 | 44 / 28 |
| | `FocusRingWidth` | 3 | 2 | 3 |

`Radius` 的取值规则是**「该体系自己刻度里的 `medium` 档，连同那条刻度的 `Source`」**，不是三个手挑的数：
`each_system_answers_the_common_radius_from_its_own_scale` 会同时断言值与来源，改成一个字面量即变红。
`StrokeWidth` 之所以进清单，是因为三者确实不同（HIG 的发丝线比整点细），
`the_common_stroke_width_is_not_one_shared_constant` 钉住这条差别。

「接线后才加」的三个与附录 #24 的死 API 不同：它们是**该用但还没接**，等命中层 / 密度 /
焦点环真正读取时再加，删除判据不同。

**永不进**（归属写清楚）：`Padding`（`Insets` 而非标量；`MetricsSet.control.padding` 仍是唯一来源，
是否升为 `common_padding()` 由第 6 步定）、`Spacing`（布局层）、
`IconSize`（控件级，且图标是回调）、`Elevation`（是效果，属 `MaterialRecipe`）、
`DividerThickness`（= `StrokeWidth`）、`CornerShape`（是形状不是度量，与 `MaterialRecipe` 并列）、
`RadiusScale` 整条刻度（家族内部；`CommonMetric::Radius` 只是默认值一个数）。

**同心规则**（MD3 `outer − padding = inner` 与 HIG `outer = inner + padding` 是同一条，
见 `docs/reference/md3-scales.md:29-30`）不是枚举项，是 `CommonMetric` 旁边的自由函数
`inner_radius(outer, padding)`。

### 5.4 四层优先级：缺一个角色时怎么答

| 级 | 来源 | 谁负责 | 标记 |
|---|---|---|---|
| 1 | 显式映射（ADAPTER 表 → 原生角色） | 体系的 `common_*` | `Source::Spec` |
| 2 | **体系内生成**（MD3 调色板 + tone 规则 / HIG accent + neutral / Fluent brand ramp） | 体系的 `common_*`，第 1 级之后的下一分支 | `Source::Derived` |
| 3 | **跨体系代数兜底**（如 `SurfaceSelected = blend(SurfaceHover, Accent, .12)`） | `CommonPlane::resolve` 的第二趟，仅当 1、2 都没给出值 | `Source::Convention` |
| 4 | 明确没有 | `Absent(Reason::NotAConcept)` 或 `Absent(PlatformResolved(..))` | — |

**第 3 级今天没有调用方**：MD3 映射全部 22 个角色，Fluent 与 HIG 只缺 `Scrim`——而 scrim 走的是
第 4 级（`PlatformResolved` / `NotAConcept`），不是"算不出来"。按「有调用方才加」的纪律，
第 3 级暂不实现；平面存的是 `Resolution`，将来加它不必改类型。

三条约束：

- **第 2 级不是每个体系都有**。Fluent 的非 brand 角色必须**跳过**第 2 级——它没有生成算法，
  硬造的值比"我们约定一个"更糟：它伪装成厂商值；
- **第 1、2 级都写在体系里**，因为只有它知道生成是否合法；第 3 级写在平面里，因为那是跨体系约定；
- **每级都写 `Source`**，控件与画廊据此判断"这个值是厂商给的还是我们算的"（§4.4）。

### 5.5 `Resolved<T>`

```rust
/// A value together with where it came from.
/// 值，连同它的来源。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved<T> {
    pub value: T,
    pub source: Source,
}
```

`Style::colour` / `metric` / `type_style` 都返回它。缺 `Source` 的快照分不清厂商值与派生值，
这是附录 #6（`ink()` 丢 `Resolution`）的同一条病。

---

## 6. API

```rust
impl CommonPlane {
    /// Resolve every common token once, per scope rather than per widget.
    /// 一次解析全部公共令牌，按作用域而非按控件。
    pub fn resolve(system: Arc<dyn StyleSystem>) -> Self;

    pub fn system(&self) -> SystemId;
    pub fn context(&self) -> Context;
    pub fn caps(&self) -> StyleCaps;
    pub fn source(&self) -> &Arc<dyn StyleSystem>;   // 家族平面的入口

    /// The common vocabulary. `Resolution` is kept, not flattened: a refusal is an answer.
    /// 公共词汇。保留 `Resolution` 而不压平：拒绝也是答案。
    pub fn colour(&self, role: CommonColor) -> Resolution<Resolved<Color>>;
    pub fn metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>>;

    /// Themes blend; the endpoints return the endpoints, not a 0% blend.
    /// 主题可混合；端点原样返回，而不是 0% 混合。
    pub fn lerp(from: &Self, to: &Self, t: f32) -> Self;
}
```

`Style` 上先只加转发访问器（`system()` / `caps()` / `colour()` / `metric()`），
等公共平面的读取方都到位，再把 `common` 变私有、把家族字段删掉。

尚未实现：`type_style()` / `motion()` / `material()` / `native()`。

**`ink()` 仍在（生产代码 14 处、测试 11 处）**，它是 §6.1 唯一没落地的部分：家族控件今天
仍以字符串查自己的令牌（`spec_ink(&pack, spec.container.color, ..)`），换成 `native()` 需要
先有三包各自的收窄类型（第 2 步留下的 `Md3Tokens::of(&Style)`），否则只是把字符串查表
换个名字。顺序是「收窄 → `native()` → 删 `ink()`」，不是一次重命名。

### 6.1 删除 `ink()`

`ink(&'static str) -> Option<Color>` 同时承担「公共角色」与「原生令牌」两件事，并把 `Resolution`
压成 `Option`。拆成 `colour()`（封闭、必有、带来源）与 `native()`（开放、可缺、带原因）之后，
调用方第一次能表达「我要的是语义角色」还是「我要的是这套体系的原生令牌」。

**控件侧的写法对比**（MD3 复选框）：

```rust
// 现在：两次字符串查表，失败给品红，来源全丢
let fill = spec_ink(pack, on.container.color, on.container.opacity);

// 之后：家族平面收窄一次，之后全强类型
let tokens = Md3Tokens::of(style).expect("md3 scope");
let fill = tokens.role(Md3Role::OnPrimaryContainer).with_alpha(on.container.opacity);
```

`Style::missing(name)` 是**唯一**的兜底出口：release 下用 `Once` 打一次日志（名字 + 体系），
debug 下 `debug_assert!(false)`。品红保留——它是有效的视觉报警——但不再散落在四个 crate 里。

### 6.2 类型阶

```rust
/// The closed set of semantic text roles; MD3's 15 styles are the superset.
/// 语义文字角色的封闭集合；MD3 的 15 档是它的超集。
pub enum TypeRole {
    DisplayLarge, DisplayMedium, DisplaySmall,
    HeadlineLarge, HeadlineMedium, HeadlineSmall,
    TitleLarge, TitleMedium, TitleSmall,
    BodyLarge, BodyMedium, BodySmall,
    LabelLarge, LabelMedium, LabelSmall,
}
```

`TypeStyle { size, weight, italic, line_height, letter_spacing }`——后两项今天没有，是 MD3 字阶的
真实字段（`typescale.rs` 里的常量表只记了字号与字重）。三个体系各自映射到这 15 档，缺档由体系
决定最近邻，**在 `resolve` 时一次算定**。

### 6.3 材质

```rust
/// A surface recipe: flat colour, or a blur + tint + noise stack.
/// 表面配方：纯色，或「模糊 + 着色 + 噪点」堆栈。
pub enum MaterialRecipe {
    Solid(Color),
    Frost { tint: Color, fallback: Color, sigma: f32 },
    Acrylic { tint: Color, fallback: Color, sigma: f32, noise: f32 },
    Glass { tint: Color, rim: f32, refraction: f32, ambient: f32 },
}
```

`ui_style::Material` 与 Fluent/Glass 各自的 `material.rs` 合并为这一条。

---

## 7. 过渡：把「插值」与「切换」写成规则

`lerp_style` 现在按两类令牌行事：

| 类别 | 令牌 | 过渡行为 |
|---|---|---|
| **可插值** | 全部 `CommonColor`、`CommonMetric` 的长度、`MaterialRecipe` 的颜色分量 | 按 `t` 线性混合 |
| **必须切换** | `MotionSet`、`StyleCaps`、`MaterialRecipe` 的 sigma/rim/refraction、`Source` 的标签、`MetricsSet`、字阶、材质 | 在 `t = 0.5` **整体切换** |

理由：时长与曲线插值会产生「半途的动效」，语义上不存在；sigma/rim 这类光学参数插值会让磨砂
在过渡中变成既不像 A 也不像 B 的东西。颜色与长度是唯一「中间态有意义」的维度——这与
Flutter `ThemeData.lerp` 的做法一致。

`lerp` 在 `t == 0.0` / `1.0` 时**返回端点的克隆**而不是重新计算，保证端点严格相等
（此前 `t=0` 时颜色来自 `from`、其余来自 `to`，加上一个混合过的调色板句柄，
端点不等于任何一个包；`a_transition_at_its_endpoints_is_the_pack_itself` 钉住）。

`CommonPlane::lerp` 早就是这个形态（端点克隆 + 内部混合），家族侧这次与它对齐；
`non_colour_tokens_switch_at_the_midpoint_while_colours_keep_moving` 钉住切换点。

---

## 8. 作用域、缓存与家族选项

### 8.0 一个构造入口

```rust
/// The one place a `Style` is assembled, so its two planes always name the same system.
/// 组装 `Style` 的唯一去处，使它的两个平面始终指名同一个体系。
pub struct StyleBuilder { /* system, roles, motion, metrics, type_table, material, device */ }

impl StyleBuilder {
    pub fn new(system: Arc<dyn StyleSystem>) -> Self;
    pub fn role(self, role: &'static str, colour: Color) -> Self;
    pub fn role_from(self, role: &'static str, native: &str) -> Self;      // 体系不认识 → 构建失败
    pub fn role_common(self, role: &'static str, common: CommonColor) -> Self;
    pub fn metrics_from_scale(self, step: &str) -> Self;                   // 三档层级由一条刻度推出
    pub fn build(self) -> Style;                                           // 同一个 Arc 交给两个平面
}
```

`build()` 是唯一一处同时写 `common` 与 `palette` 的代码，所以"两个平面来自同一个体系"
不再是三个包各自要记得的纪律，而是结构上做不到别的。`role_from` 把原先散在三个包里
的 `unwrap_or_else(|| panic!(...))` 收成一条：体系不认识被告知要用的令牌即构建失败，
不会变成透明像素（`a_token_the_system_does_not_know_fails_the_build` 钉住）。

`new` 原先还收一个 `name: &'static str`，只用于 panic 文案——那是 `SystemId` 的第二次抄写，
第 8 步连同 `Style.name` 一起删掉，文案改由 `system.id().name()` 给出。

三个包因此只剩自己的内容——MD3 的 49 个角色名表、Fluent 的三条合成洗色、Glass 的八个
Cupertino 角色——加上一行 `metrics_from_scale("<radius step>")`。

### 8.1 一帧只解析一次

```rust
/// A bounded memo table inside each style kit. Keyed by every input that can change the
/// result, so an entry never goes stale and no frame boundary is needed.
/// 每个风格包内部的有界记忆表。键含所有能改变结果的输入，因此条目不会过期，也不需要帧边界。
pub struct StyleCache { entries: Mutex<Vec<((Appearance, Option<Appearance>, u8), Style)>> }

impl StyleCache {
    pub fn get(&self, scope: StyleScope, resolver: fn(Appearance) -> Style) -> Style;
}
```

`t` 量化到 1/256（浮点抖动不穿透）；条目超过 64 条整表丢弃——过渡会扫过整个 `t` 区间，
不设上限就会无限长，而重建很便宜。键用 `scope.settled()` 之后的值，所以「已走完的过渡」
与「根本没有过渡」落在同一条目上。

**实测**（`--release`，MD3，2000 次取平均）：未缓存 `pack_of` **209.6µs/次**，
命中缓存 **1.5µs/次**（Clone + 键扫描），**139×**。一页三十个控件每帧从 6.3ms 降到 45µs——
这是原设计里"每控件每帧重建 1–2 个 pack"（附录 #13）的真实代价。

缓存放在**风格包**里而不是 `AppLoop` 里：键已经包含全部输入，按帧清空反而会丢掉跨帧命中，
而 `AppLoop` 还要为此认识 `Style`。

### 8.2 作用域只装上下文

```rust
/// What a subtree overrides. Context only — tokens are derived, never stored.
/// 子树覆盖什么。只含上下文——令牌是推导出来的，从不存储。
#[derive(Clone, Copy, PartialEq)]
pub struct StyleScope {
    pub appearance: Appearance,
    pub theme_from: Option<Appearance>,
    pub theme_t: f32,
}
```

`Env` 链保留，但多了一个 `scope: Option<StyleScope>` 槽位（`Env::with_scope` 填，
`Env::with` 留空），`scope()` 取链上**最近一个明说了上下文的层**——留空即继承父层，
这就是 §8.2 原先说的"逐字段合并"，只是合并粒度取整份上下文而不是逐字段。
之所以不塞进那层不透明的 params：`find::<P>()` 按类型匹配，上下文与家族旋钮是两个类型，
塞两层 `Env` 就得让两边都写对，而一个具名槽位让"没说过"与"说了"可区分。

`Style::resolve(scope, resolver)` 是**直接**解析，`StyleCache::get(scope, resolver)` 是
**带记忆**的同一件事：两者都收整份上下文而非三个散字段，`StyleScope::settled()` 先把
「过渡已结束 / `from == to`」归一成 `theme_from: None`，于是缓存键与解析读的是同一个
已归一的值。三个包的 `pack_in` 都只剩两行。第 3 步曾把入口放在 `StyleScope::resolve`，
第 8 步归位到 `Style` 上——解析的产物是 `Style`，上下文只是它的入参。

`Env` 原先还带一个 `StyleId` 字段（`StyleId::MD3` / `::of("acme")`），第 8 步删掉：
全仓没有任何读取方，**选中哪套体系由「调用哪个 kit 对象」决定**，不在链上。留着它等于
宣称作用域能选体系，而实际不能。

**未做**：`disabled` 仍留在家族参数里（它是控件构建期读的，不是包解析用的）；
把它挪进来属于第 7 步（构造器不再收参数）。

### 8.3 家族构建旋钮：`*Options`，不是 `*Params`

`Md3Options`（原名 `Md3Params`）里混着两类东西，必须拆开：

| 字段 | 类别 | 去哪 |
|---|---|---|
| `appearance` / `theme_from` / `theme_t` | 解析上下文 | ✅ `StyleScope`（§8.2，已落地） |
| `outline_width` / `has_icon` / `disabled` | **家族构建旋钮**（`has_icon` 决定 24 vs 16 的滑块规则，注释写明是构建期旋钮） | 家族自己的 `Md3Options` |

```rust
/// MD3 knobs the semantic spec cannot express; chosen when the control is built.
/// 语义规格表达不了的 MD3 旋钮；在构建控件时选定。
pub struct Md3Options { pub outline_width: f32, pub has_icon: bool, pub disabled: bool }

kit.slider(&SliderSpec::new(id, v, ""), Md3Options::default(), cx)
```

上下文已搬走，类型名也已在第 8 步改完：`Md3Options` / `FluentOptions` / `HigOptions`。
改名与 `Md3Style` → `Md3Kit` 同批做，避免同一批文件改两遍。

同一批里还删掉了三个 kit 的 `new(pack: Style)` 参数：它叫 `_pack`、从未被读，
画廊却认真地传了 `pack_of(appearance_of(dark))` 进去——一个**假装**在选体系的构造器。
现在是 `Md3Kit::new()`（回落上下文 = 标准浅色）与 `Md3Kit::with_scope(home, options)`。

旋钮**不进** `StyleScope`（否则通用作用域长出家族字段），**也不能删**（画廊的图标变体依赖它）。

### 8.4 控件工厂拿到什么

原提案是给 `StyleKit`（当时叫 `WidgetFactory`）加 `fn style(&self, cx) -> &Style`，理由是"app 只能写字面量"。
核过之后**不做**：画廊那几处字面量（`page_title` 的 `20.0` 与 `236,240,250`、`muted()`）
是**画廊外壳自己的配色**，不是任何一套体系的令牌——它本来就该有自己的样子。真正需要读令牌的
app 代码今天不存在；等它出现再加，届时形状也不一定是"返回 `&Style`"（懒解析 vs 已解析）。

### 8.5 家族控件的构造：具名参数链，按参数个数排序的优先级

`Md3Button::new_with(params, id, label, style, size, toggle, icon, cx, pack)` 这类签名的问题
不在长度本身，而在**相邻同型**：`style`/`size`/`toggle` 三个枚举挨着，`ChipParams` 的
`lead`/`trail` 更是两个一模一样的 `Option<fn(Id, Color) -> Decl>`——互换编译器一声不吭。
`Md3Chip::new_with(params, id, label, kind, variant, selected, lead, trail, cx, pack)` 是对的
顺序，而画廊当时就写成 `style.chip(id, label, kind, variant, selected, mark, mark, ..)`：
两个标记同一个字形，谁也没发现。

规则：**参数 ≥6 或存在相邻同型参数时，改用具名参数结构 + 链式 setter**（`ListItemParams`
是范本）。已经改的：`ButtonParams`、`ChipParams`。其余控件参数 ≤5 且类型互不相同，不为了
统一而统一。

---

## 9. 命名

| 概念 | 今天出现过的名字 | 定案 |
|---|---|---|
| 一套设计体系 | `ui_design::StyleSystem`、`StyleId`、`Style.name` | **`SystemId`**（保持封闭，见 §11）；`StyleId` 与 `Style.name` 已删（✅ 第 8 步） |
| 已解析的令牌快照 | `Style`、`pack`、`Pack`、`palette` | **`Style`**；变量仍叫 `pack`（见下） |
| 通用词汇查询 | `ink()`、`common()`、`roles.get(RoleName(..))` | **`colour()` / `metric()`**（快照）/ **`common_colour()` / `common_metric()`**（trait） |
| 家族词汇查询 | `colour(&str)`、`roles.get` | **`native_colour(&str)`**（✅ 已改）/ **`native()`**（快照，未做） |
| 解析入口 | `pack_for`、`pack_for_transition`、`pack_in`、`with_params` | **`Style::resolve`** + **`StyleCache::get`**（✅ 第 8 步）；`with_params` 已换成 **`with_scope`**（✅） |
| 作用域 | `Env`、`Md3Params`、`FluentParams`、`GlassParams` | **`StyleScope`**（✅ 上下文）+ **`Md3Options` / `FluentOptions` / `HigOptions`**（✅ 第 8 步） |
| 控件工厂 | `WidgetFactory`、`Md3Style`、`FluentStyle`、`GlassStyle` | **`StyleKit`**（trait）+ **`Md3Kit` / `FluentKit` / `HigKit`**（✅ 第 8 步） |

`Style` 这个名字是对的——它是快照。真正错的是把**体系**、**作用域**、**工厂**都叫成了 `Style*`。

**"变量一律叫 `style`"没有执行，改判为保留 `pack`**。落地时看到的是三件互相打架的事：
① 每个包仍有一个 `pack()` 构造器（`let style = pack()` 读起来像构造器叫错了名）；
② 模块就叫 `pack`，`crate::pack::pack()` 是真实路径；③ 728 处、69 个文件的纯局部改名，
收益只是用词统一，风险却是任何一次误替换（`pack::` 前缀、`pack()` 调用）都只能靠人眼发现。
定案：**类型与入口叫 `Style`，装它的局部变量叫 `pack`（=「一份包」）**，二者是类型与值的关系，
不是一个概念的两种拼法。

---

## 10. 迁移步骤

| 步 | 改动 | 验证 |
|---|---|---|
| 0 | ✅ `common()` → `common_colour()`；新增 `Resolved<T>`、`CommonMetric` + `common_metric()`，三套体系各实现；画廊颜色列开始印 `Source` 标签 | 新增 `each_system_answers_the_common_radius_from_its_own_scale`、`the_common_stroke_width_is_not_one_shared_constant`（两条都验证过破坏→红→恢复→绿）；全仓 690 条全绿 |
| 1 | ✅ `CommonPlane`（`ui-style/src/common_plane.rs`）：22 色 + 2 度量一次解析，`Resolution` 与 `Source` 随值；`Style.common` 可选（`legacy_pack` 为 `None`）；`StyleSystem: Send + Sync`；`Style::colour/metric/system/caps` 转发；三包各自只建一次体系 `Arc`（原先 palette 与公共平面各建一份） | 新增四条，均验证过破坏→红：`the_snapshot_returns_the_systems_own_answer_and_source`、`a_refusal_survives_the_snapshot`、`a_transition_at_its_endpoints_equals_the_endpoint_snapshots`、`two_separately_resolved_snapshots_of_one_system_are_equal`；全仓 694 条全绿 |
| 2 | ✅ `Palette::colour` → `native_colour`，`Theme::color`/`metric` → `common_colour`/`common_metric`；`Palette: Send + Sync`，于是 `Style` 真的可 `Send + Sync`（附录 #26，由 `a_style_can_travel_inside_a_scope` 钉住）；**未做**：三包各写 `Md3Tokens::of(&Style)` 收窄 + `Md3Role` 枚举 | 重命名由编译器逐个指出调用方；收窄待做时的验收：`narrowing_another_systems_snapshot_is_unmapped` |
| 3 | ✅ `StyleBuilder`（`ui-style/src/pack.rs`）统一三包构造，两个平面出自同一个 `Arc`；三个包各自的 `metrics()`/`scale_radius()`/`scale_corner()` 六份重复折叠成一行 `metrics_from_scale`；`StyleScope`（`ui-style/src/env.rs`）接管 `appearance`/`theme_from`/`theme_t`，`Env` 新增 `scope` 槽位与 `with_scope`/`scope()`，三包改从作用域读上下文。**未做**：`*Params` → `*Options` 的改名（纯机械，归第 8 步） | 新增 `a_built_style_answers_both_vocabularies_from_its_one_system`、`a_token_the_system_does_not_know_fails_the_build`（均验证过破坏→红）；`an_md3_toggle_reads_the_blended_pack` 在把 `pack_in` 改成忽略作用域后变红，恢复后转绿；全仓 699 条全绿 |
| 4 | ✅ `StyleCache`（`ui-style/src/cache.rs`）进三个风格包，6 处 `pack_for_transition` 改为查缓存；键 = (外观, 过渡起点, 量化后的 t)，上限 64 条 | 新增 `a_repeated_context_resolves_only_once`、`another_appearance_is_a_different_entry`（均验证过破坏→红）；实测 209.6µs → 1.5µs；画廊 `GALLERY_SELFTEST=1` 全项 PASS（含外观切换） |
| 5 | ✅ `lerp_style` 端点返回克隆（两个平面都做到了）；`MetricsSet`/字阶/材质/动效按 §7 在 `t = 0.5` 切换 | 新增 `a_transition_at_its_endpoints_is_the_pack_itself`、`non_colour_tokens_switch_at_the_midpoint_while_colours_keep_moving`（均验证过破坏→红）；全仓 701 条全绿 |
| 6 | ✅ 通用 `Button`/`List`/`Card` 改读公共平面，**回落顺序是「包自己的角色 → 公共平面 → 常量」**，所以 `legacy_pack`（`common: None`）行为逐字节不变；`Style::colour_or`/`metric_or` 收掉 `.value().map(..).unwrap_or(..)` 的噪声。**`MetricsSet` 不收缩**：三档都有读方（`control` ← Button/List，`surface` ← Card，`overlay` ← Panel），无死数据 | 新增 `a_generic_button_under_a_kit_pack_paints_the_kits_accent`、`a_generic_list_under_a_kit_pack_reads_the_surface_roles`、`a_generic_card_under_a_kit_pack_reads_the_raised_surface`（三条都验证过破坏→红）；`crates/ui-widgets/tests/parity.rs` 不动仍绿；全仓 704 条全绿 |
| 7 | ✅ 参数构造器链落地在两个**位置参数最多**的控件上：`ButtonParams`（9 参，其中 `style`/`size`/`toggle` 三个枚举相邻）与 `ChipParams`（10 参，`lead`/`trail` **同型相邻**，互换即静默错位——画廊原先就写作 `mark, mark`）。两者都照 `ListItemParams` 的样子做具名链，`new_with` 顺手降为 `pub(crate)`。其余控件（Card/Dialog/Toggle/Checkbox/Radio/Slider/Divider/TabBar/FabMenu）参数 ≤5 且类型互不相同，暂不动 | 编译器逐处指出调用方；画廊按钮/芯片页像素不变；全仓 704 条全绿 |
| 8 | ✅ 重命名（§9）：`Md3Params`/`FluentParams`/`GlassParams` → `*Options`（`HigOptions` 随 `HigKit`）；`Md3Style`/`FluentStyle`/`GlassStyle` → `Md3Kit`/`FluentKit`/`HigKit`；`WidgetFactory` → `StyleKit`；删 `StyleId`（`Env` 不再带体系名）与 `Style.name`（`StyleBuilder::new` 也不再收）；`pack_for` → `pack::pack_of`，`pack_for_transition`/`StyleScope::resolve` → `Style::resolve`，`StyleCache::style_for` → `get(scope, resolver)`；三个 kit 的 `new(_pack)` → `new()`。**未做**：`pack` 局部变量改名（见 §9 末段） | 每类改名前后 `grep -wo` 计数逐一相符（`Md3Params` 211→`Md3Options` 211、`Md3Style` 46→`Md3Kit` 46、`WidgetFactory` 27→`StyleKit` 27、`pack_for` 34→`pack_of` 36）；删掉的 `StyleId` 35 处与 `Style.name` 归零，编译器零残留；`a_token_the_system_does_not_know_fails_the_build` 的 panic 文案随 `SystemId::name()` 改为 `MD3 has no native token …` 后仍命中；`cargo build --workspace --tests --examples` 零 warning；全仓 704 条全绿（与第 7 步同数）；画廊 `GALLERY_SELFTEST=1` PASS、按钮页截图与改名前的排版一致 |

第 6 步之前不能删 `Style.metrics`：`MetricsSet` 是通用 `ui-widgets` 控件今天唯一的几何来源，
必须先让它们改读 `CommonMetric` / 自己的常量，才能删（附录 #24）。

第 6 步的落地结论：

- **真正修掉的缺陷**：通用 `Button`/`List`/`Card` 只认自己那套名字（`buttonFill`、
  `scrollerFill`、`cardFill`），而三套真实体系的包里根本没有这些名字——所以
  `Button::resolve(state, &md3_pack, ..)` 今天画的是**全透明**（填充与文字都是）。
  改成「先问包自己的角色，再问公共平面」后，套件包终于能画通用控件，手写包一字不变。
- **`Card` 的 `onSurface` 是家族词汇泄漏，但不是替换而是补链**：`legacy_pack` 设了
  `onSurface`，直接换成 `CommonColor::OnSurface` 会让它在链路里消失；补成
  「`title_ink` → `onSurface` 角色 → `OnSurfaceRaised` → 白」才既不动旧行为又接上平面。
- **`Panel` 仍然例外**（下条）：它的墨色来自材质亮度，不是体系表面角色。

**`Panel` 的白字不是 bug，不能改成 `CommonColor::OnSurface`**：磨砂/玻璃面板的墨色取决于
  **材质亮度**，不是体系的表面角色。玻璃包里 `colour(OnSurface)` 会给出 HIG 的深色墨
  （neutral 12），落在深色磨砂上直接不可读——也就是说，改成"更语义"的写法反而更错。
  材质墨需要一个自己的来源（材质配方携带 `ink`），不在 `CommonColor` 里。

---

## 11. 明确不做的

- **不做 `ThemeExtension` 式的泛型扩展 map**。Flutter 用它是因为 `ThemeData` 是全局单例；
  我们的作用域已经是树上的值，扩展可以做成 `Env` 的另一种层。
- **不让 `Style` 承载控件实例**。重构前 `Md3Style` 既是工厂又是参数包；现在 `Style` 只是数据，
  工厂是 `Md3Kit`（`StyleKit` 的实现）。
- **不做热重载式的样式热替换**。没有需求，且它会把 `Style` 变回可变状态。
- **不让 `Style` 泛型化**（`Style<S: StyleSystem>`）。画廊把三套实现并排放在一个数组里
  （`gallery.rs:641`），`Env` 里也要异构存放；泛型最后一定要么 enum 要么 trait object，
  等于把擦除推迟到更痛的地方。保持**一个擦除的 `Style` + 每个体系一个 `of()` 收窄**。
- **暂不开放 `SystemId`**（不加 `Custom(&'static str)`）。第三套体系之外没有第四套，
  而 `dimensions_of(id, ctx)` 按 id 查静态表，开放后它只能撒谎或改成 `Option` 并波及 5 处调用方。
  等真有外部体系时再改，届时同时决定它怎么给出自己的维度表。

---

## 附录：盘点出的完整缺陷清单（27 条）

| # | 缺陷 | 证据 |
|---|---|---|
| 1 | 无构造器，九字段 literal | `pack.rs:42-64`；`ui-widgets/src/button.rs:142-150` |
| 2 | 传入的 `Style` 被丢弃 | `md3/style.rs:60-71`、`fluent/style.rs:49-61`、`glass/style.rs:50-61` |
| 3 | `StyleId` 死机制，作用域「风格」与参数类型可矛盾 | `env.rs:86-95`；`md3/tests/scope.rs:57-70` |
| 4 | `ScaffoldSpec.env` 死字段 | `ui-widgets/src/scaffold.rs:80-86`；三个 `*Scaffold::new` 不读 |
| 5 | `Env` 未挂到 `Decl`，引擎的 env 折叠闲置 | `styled.rs:28-38`；`ui-core/src/tree.rs:430-439` |
| 6 | `ink()` 混两个命名空间、丢 `Resolution` | `pack.rs:79-90` |
| 7 | MD3 palette 兜底对 camelCase 无效 | `md3/pack.rs:62-112` vs `ui-design/data/md3_data.rs:12-15` |
| 8 | `PaletteHandle` 用 `Arc::ptr_eq`，`Style` 非值相等 | `pack.rs:34-38` |
| 9 | `metrics`/`motion` 对设计系统控件无效 | `md3/pack.rs:166-167` 只在测试里读 |
| 10 | `material` 三包全 `None`，材质两套平行系统 | `md3/pack.rs:150`、`fluent/pack.rs:118`、`glass/pack.rs:107` |
| 11 | `device` 硬编码且 `frost_allowed` 忽略它 | `pack.rs:69-71`；各包写 `GPU_BASELINE` |
| 12 | 过渡只混颜色 | `pack.rs:161-170` |
| 13 | 过渡每控件每帧解析 1–2 个 pack | `md3/style.rs:79-82` 等 11 处 |
| 14 | 作用域整份替换，不能逐字段覆盖 | `env.rs:86-95` |
| 15 | 语义规格不完整（`SliderSpec` 只有 id/value/label） | `style.rs:8-24` |
| 16 | `WidgetFactory` 只有三个方法，无令牌访问器 | `ui-widgets/src/style.rs:18-30` |
| 17 | `TypeTable` 5 档且 MD3 完全不用 | `types.rs:45-58`；`typescale.rs:25-39` |
| 18 | `MetricsSet` 三档且 surface == control | `types.rs:90-129`；三包同写 |
| 19 | `ui-design` 已有语义层，`ui-style` 另起炉灶 | `ui-design/src/common.rs:7-80` vs `roles.rs` |
| 20 | 引擎样式层依赖设计系统词汇 | `crates/ui-style/Cargo.toml:7-9` |
| 21 | `Style` 不记录 appearance/context | `pack.rs:42-64` |
| 22 | `pack_for_transition` 用 `fn` 指针，不能带状态 | `pack.rs:175-180` |
| 23 | 通用控件与三包的角色契约不兼容 | `button.rs:11-24` vs `md3/pack.rs:62-112` |
| 24 | `ui-widgets` 自己违反「不得出现字面量」 | `styled.rs:52`、`button.rs:114` |
| 25 | `RoleDict` 的别名/合并是死代码 | `roles.rs:51-54,74-83` |
| 26 | `Style` 非 `Send + Sync` | `ui-design/src/capabilities.rs:21` |
| 27 | 主题过渡状态是 thread-local 全局量 | `gallery.rs:318-330` |
