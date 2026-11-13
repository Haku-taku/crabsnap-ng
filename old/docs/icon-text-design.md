# 图标与文本模块完整设计

> 状态：**已实现**（2026-10-29）。实施与本文有四处偏差，见 §5.1；下面的正文保留为
> 设计原稿，偏差处已就地标注。本文给出文本子系统改造后的**完整形状**——数据类型、数据流、
> 缓存键、字体注册、图标入口——而不只是「要加哪几个字段」。
>
> 事实来源：本仓库源码（带 `file:line`）+ 对 `MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf`
> 的 `fontTools` 实测（见 §2.2）。凡是推断都标了「推断」。

---

## 0. 需求

用户报告：**常规图标无法正常使用**，判定为引擎缺口。要求给出完整模块设计，
以便评审「改完之后的整体设计」是否自洽。

同时确认：变形图标（FAB 关闭 ↔ 箭头等）**保留手绘**（官方 `_AnimatedIcon` 也是手绘路径），
但**线端必须是直角**，我们目前画成了圆头。

---

## 1. 现状：五层，以及每层的形状

```
控件          ui_widgets::text::Text / leaf()      （size / weight / ink）
   ↓ Decl
引擎          ui_core::text::TextStyle { size, weight, italic }        （text.rs:32-40）
              TextBlock { spans: [TextRun { range, style }] }
              TextPrim { block, origin, max_width, ink }               （draw.rs:258-269）
   ↓
排版/绘制     ui_platform::TextPass                                     （text_pass.rs:20-52）
              stack: Vec<FaceId>            全局一份
              styled: HashMap<(u16,bool), Vec<FaceId>>
              script_face: HashMap<(Script,u16,bool), FaceId>
              shapers: HashMap<FaceId, Shaper>
              cache: GlyphCache { entries: HashMap<(FaceId,u32,u32,u8), Slot> }  （glyph.rs:356）
   ↓
字体          ui_text::FontStack / FontRequest { families, generic, weight, italic, script }
              ui_text_fontique::FontiqueProvider（系统枚举） / Shaper（swash）
```

---

## 2. 缺口清单

### 2.1 六条硬缺口（带证据）

| # | 缺口 | 证据 | 后果 |
|---|---|---|---|
| **G1** | **没有字体注册入口** | `FontiqueProvider` 只有 `new_system()` / `new_isolated()`（`ui-text-fontique/src/lib.rs:122,132`），全 crate 无 `register`/`add_font` | 内置字体（Material Symbols 打包版）无法被解析；只能碰运气依赖系统装没装 |
| **G2** | **`TextStyle` 没有字体族** | `TextStyle { size, weight, italic }`（`ui-core/src/text.rs:32-40`）；`FontRequest.families` 只在 `TextPass::build` 里被读一次（`text_pass.rs:69-120`） | 「这段用图标字体、那段用正文」做不到。整个窗口只有一份栈 |
| **G3** | **只设 `wght` 一个轴** | `shape.rs:218-220` 只 `variations([(b"wght", w)])`；`ParsedFace` 只记 `weight: Option<f32>`（`glyph.rs:69-71`） | Material Symbols 的 `FILL`（空心↔实心）、`GRAD`（强调）、`opsz`（光学尺寸）全部不可用 |
| **G4** | **图集键不含轴** | `entries: HashMap<(FaceId, u32, u32, u8), Slot>` = (字面, 字形, 量化像素, 亚像素相位)（`glyph.rs:356`） | 即使加上了 `FILL`，`FILL=0` 与 `FILL=1` 会共用同一个槽位，后光栅的覆盖前者 |
| **G5** | **无 OpenType 特性开关** | `Shaper` 只调 `builder.script().size().direction()`，没有 `features()` | 依赖 `rlig` 的图标连字、依赖 `tnum` 的表格数字都不可控 |
| **G6** | **`script_face` 是全局缓存，会被图标字面污染** | `HashMap<(Script, u16, bool), FaceId>`（`text_pass.rs:44`），命中后**优先于**正常栈（`text_pass.rs:539-546`） | 一旦某段拉丁文本用图标字面解析成功，之后所有拉丁文本都会先试图标字面。AGENTS.md §1.5 已记同类事故（「按 (script,weight,italic) 缓存会让粗体复用常规字面」） |

### 2.2 关于 Material Symbols 的实测事实

对 `/usr/share/fonts/ttf-material-symbols-variable/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf`
用 `fontTools` 读取（本机实装，`fc-list` 可见 Outlined/Rounded/Sharp 三族）：

| 事实 | 数值 |
|---|---|
| 表 | `GSUB` `HVAR` `OS/2` `STAT` `avar` `cmap` `fvar` `glyf` `gvar` `hmtx` … （**无** `GPOS`） |
| cmap 条目 | 4405；其中 PUA（U+E000..U+F8FF）**4200** 条 |
| ASCII 条目 | 66 条，且映射到**真实字形**：`period`、`digit_zero`、`a`… |
| GSUB 特性 | 只有 `rclt` 与 `rlig` |
| 连字条目 | **4299** 条（例如 `a+n+d+r+o+i+d+underscore+c+e+l+l+…`） |

三条推论：

1. **连字路径是完整的**：每个图标名都有连字，不只是多词名。（此前的调研摘要声称「708 条、
   仅多词」，**实测不成立**——这正是 AGENTS.md §1.1 要求的复核。）
2. **PUA 码点是更稳的路径**：一个码点 = 一个字，不依赖特性选择，宽度恒为一个 em。
3. **`gid != 0` 不能当覆盖率判据**：ASCII 全部映射到真实占位字形，所以「字体里有这个字形」
   对图标字体永远为真，用它选字面会让图标字体吞掉所有拉丁文本（G6 的放大版）。

---

## 3. 目标设计

### 3.1 模块图（改后）

```
ui-core/src/text.rs
  Family            ← 新增：Named(&'static str) | Generic(GenericFamily)
  Variations        ← 新增：optical_size / fill / grade（重量不在这里，见 §5.1）
  Features          ← 新增：定长 (tag, value) 表（不是位图，见 §5.1）
  TextStyle { family, size, weight, italic, variations, features }
        ↑ 仍是 Copy：Family 是 &'static str，其余是数值

ui-text/src/
  provider.rs       FontProvider 增加 register(name, bytes) / has_family(name)
  fallback.rs       FontStack 的链按 (Family, weight, italic, script) 缓存
  registry.rs       ← 新增：内置字体清单（名字 → include_bytes!）

ui-text-fontique/src/
  lib.rs            FontiqueProvider::register(name, bytes)   ← 新增
  shape.rs          Shaper::new_with_axes(data, index, Variations)
  glyph.rs          ParsedFace 记完整 Variations；轴位置驻留成 FaceId（见 §5.1）

ui-platform/src/text_pass.rs
  TextPass 的缓存键从 (script,weight,italic) 改成 (Family, weight, italic, axes 指纹)
  script_face 限定在「族内的脚本回退」，不再跨族污染

ui-widgets/src/
  text.rs           Text / leaf() 带 Family
  icon.rs           ← 新增：Icon（名字 → PUA 码点 → Text）
```

### 3.2 `Family`

```rust
/// Which family a run asks for. `&'static str` keeps `TextStyle` copyable and the cache
/// key cheap; a family is a compile-time constant, never user data.
/// 一段文字要哪个族。`&'static str` 让 `TextStyle` 可 Copy、缓存键廉价；
/// 族是编译期常量，绝不是用户数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// A named family, resolved by the provider (system or registered).
    Named(&'static str),
    /// The platform's generic family.
    Generic(GenericFamily),
}
```

`TextStyle::family` 默认 `Family::Generic(SystemUi)`，因此现有代码零改动。

### 3.3 `Variations` 与 `Features`

```rust
/// Variable-font axes. `weight` is first-class because every face has it; the rest are opt-in.
/// 可变字体轴。`weight` 是一等字段（每个字面都有），其余按需。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Variations {
    pub weight: Weight,
    /// `opsz`: optical size in points; `None` means "follow the font size".
    pub optical_size: Option<f32>,
    /// `FILL`: 0 outlined, 1 filled.
    pub fill: Option<f32>,
    /// `GRAD`: emphasis, typically -50..200.
    pub grade: Option<f32>,
}

/// OpenType features this run turns on. `0` means the shaper's defaults.
/// 本段开启的 OpenType 特性。`0` 表示整形器的默认。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Features(u32);
impl Features {
    pub const RLIG: Self = Self(1 << 0);
    pub const LIGA: Self = Self(1 << 1);
    pub const CALT: Self = Self(1 << 2);
    pub const TNUM: Self = Self(1 << 3);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}
```

`TextStyle.weight` 保留（它是 `Variations.weight` 的别名），避免破坏 500+ 处调用点；
`axes()` 返回合并后的 `Variations`。

> **实施偏差（§5.1）**：`weight` 没有进 `Variations`——它决定选哪个**字面**，
> 而另外三个轴是同一字面的位置；把它并进去会让「粗体是另一个 face」和「粗体是一个轴值」
> 两种语义混在一个类型里。`Features` 也不是位图，而是定长的 `(tag, u32)` 表：
> 位图只认得四个我们写死的特性，表能表达任意 tag，且保持 `Copy`。

### 3.4 图集键

```rust
/// One rasterized glyph. The axis fingerprint is part of the key: the same glyph at two
/// `FILL` values is two different bitmaps.
/// 一个已光栅字形。轴指纹是键的一部分：同一字形在两个 `FILL` 值下是两张不同的位图。
struct GlyphKey {
    face: FaceId,
    gid: u32,
    px_q: u32,       // 量化像素
    phase: u8,       // 亚像素相位
    axes: u32,       // Variations 的指纹（量化后打包）
}
```

`axes` 指纹把每个轴量化到 1/64 后打包成一个 `u32`；未设置的轴用字面默认值。
`FILL` 这类连续轴因此最多 65 个槽位，而实际只会用到 0 与 1 两端（动画中间值另计，见 §3.7）。

> **实施偏差（§5.1）**：没有给图集键加 `axes` 维度。轴位置在 provider 里驻留成
> **新的 `FaceId`**（`instance_axes`，量化到 1/64），下游的字形缓存、整形器缓存与图集
> 键全部沿用「字面」这一维——这正是既有「重量阶梯也是一个 FaceId」的延伸，改动面最小。
> 代价是 `FaceId` 的数量随用到的轴位置增长；量化把它压到有限个。

### 3.5 字体注册

```rust
impl FontiqueProvider {
    /// Register bytes under `name`. Registered families win over system ones with the same
    /// name, so a bundled icon font cannot be shadowed by a stale system copy.
    /// 把字节注册到 `name` 下。注册族优先于同名系统族，内置图标字体因此不会被陈旧系统副本遮蔽。
    pub fn register(&mut self, name: &str, bytes: Arc<[u8]>) -> bool;
}
```

`ui_text::registry` 维护内置清单：

```rust
/// Fonts compiled into the binary. Empty today; the icon font is the first candidate.
/// 编进二进制的字体。今天为空；图标字体是第一个候选。
pub const BUNDLED: &[(&str, &[u8])] = &[];
```

**本机的验证路径不需要打包**：三族 Material Symbols 已随系统安装，`Family::Named("Material
Symbols Outlined")` 现在就能解析。打包只是为了让「没有装这个字体的机器」也能出图标——
这也是 G1 必须补的原因，但**它不阻塞**图标功能的实现。

### 3.6 图标

```rust
/// One Material Symbols icon. The name maps to a PUA codepoint at build time.
/// 一个 Material Symbols 图标。名字在构建期映射到 PUA 码点。
pub struct Icon {
    id: Id,
    glyph: char,          // PUA 码点
    size: f32,            // 24 默认
    ink: Color,
    fill: f32,            // 0 空心 / 1 实心
    weight: Weight,       // 图标线宽，默认 400
    grade: f32,
}
```

渲染就是一段单字符文本：

```rust
Text::new(id, self.glyph.to_string(), self.size, self.ink)
    .family(Family::Named(self.set.family()))   // Outlined / Rounded / Sharp
    .weight(self.weight)
    .variations(Variations::NONE.fill(self.fill).optical_size(self.size).grade(self.grade))
```

实施时命名是 `IconSet { Outlined, Rounded, Sharp }`，另有 `Icon::from_char` 供码点表以外的
字形使用；名字查不到时**在构造处 panic**（Flutter 用编译期常量达到同一效果）。
`opsz` 跟随字号——Google Fonts 的 CSS 也是这么发的。盒子的宽度就是字形的推进宽度
（一个 em），高度是一行；放置由使用者负责。

**为什么走 PUA 而不是连字**：

| 判据 | PUA 码点 | 连字（`rlig`） |
|---|---|---|
| 依赖整形特性 | 否 | 是（`rlig`+`rclt`，实测字体只声明这两个） |
| 宽度 | 恒 1 em | 需要整形后才知道，断行会参与 |
| 名字拼错 | 构造期 panic（码点表是生成的；Flutter 靠 `static const` 做到编译期） | 运行期显示成字面文本 |
| 可测量性 | 一个字形 | 一串字形 |

连字保留为**调试手段**（在示例里打一段名字看有没有字），不作为 API。

### 3.7 图标与动效

`FILL` 是连续轴，因此「空心 ↔ 实心」的过渡是一条轴插值，不需要两张字形：
`ctx.animate(id.with(param::PROGRESS), on, spec)` 驱动 `fill`，图集按 §3.4 的指纹各存一张。
这是 MD3 官方（`material-symbols` 的 `FILL` 轴）与 Compose 的 `IconToggleButton` 的做法。

**但这只覆盖同一图标内的形状变化**。跨图标的变形（关闭 ↔ 箭头、播放 ↔ 暂停）官方用
`_AnimatedIcon` + 手绘路径，我们也手绘——见 §4。

### 3.8 线端：直角，不是圆头

官方图标字形是**填充轮廓**，笔画端点是平切的（butt cap）。我们的手绘笔画必须一致：

| 位置 | 现状 | 目标 |
|---|---|---|
| `ui-widgets-md3/src/checkbox.rs` 的勾与横杠 | 已改 `Corners::ZERO` | ✅ |
| `ui-widgets-md3/src/toggle.rs` 的滑块字形（`glyph_bars`） | `bars_along` 出方块，端点为直角 | ✅ 已符合 |
| `ui-gallery` 的 `cross_glyph` / `ink_glyph` | `Corners::all(0.6)` / `all(6.0)` | 占位图形，不是图标；保留圆角无妨，但**新写的图标替身一律 `Corners::ZERO`** |

规则写进 §5 的验收清单：**手绘图标笔画一律方头**。

---

## 4. 变形图标：为什么保留手绘

官方 Flutter 的 `_AnimatedIcon` 用 `AnimatedIconData` 存**两套路径 + 一条插值曲线**，
在 `CustomPaint` 里按进度重绘；Material Web 的 `md-icon-button` 用 CSS `transition` 换两个
`<svg>` 的 `d`。两者都不是「用字体插值」。

理由：两个不同图标的字形没有共同拓扑，字体层无法在它们之间插值。
（`FILL` 轴能插值，是因为那是**同一个**图标的两端。）

所以我们手绘，但必须与官方观感一致：

- 笔画端点为直角（§3.8）；
- 线段用「沿路径排方块」的方式画（`glyph.rs::bars_along`），步长为半个条厚；
- 变形分两段：离开的字形在前半程收缩，到达的在后半程长出（`checkbox.rs` 的 `GLYPH_OUT`/`GLYPH_IN`），
  与 Flutter 的 `_CheckboxPainter` 分段一致。

---

## 5. 迁移步骤与验证

| 步 | 改动 | 验证 | 状态 |
|---|---|---|---|
| 1 | `Family` / `Variations` / `Features` 进 `ui-core::text`，`TextStyle` 加字段（有默认值） | 既有测试全绿（`TextStyle::new(size)` 语义不变） | ✅ |
| 2 | `FontProvider::register` + `FontiqueProvider::register` | `a_registered_family_resolves_on_an_isolated_provider`——在 `new_isolated()` 上注册系统字体的字节，`resolve` 必须按新名字命中 | ✅ |
| 3 | `Shaper::new_with_axes`；`ParsedFace` 记完整 `Variations` | `the_fill_axis_changes_the_rasterized_coverage`——同一码点 `FILL=0` 与 `FILL=1` 的覆盖率必须不同 | ✅ |
| 4 | 轴位置驻留成 `FaceId`（原计划是给图集键加轴指纹，见 §5.1） | `an_axis_position_gets_its_own_face_handle`——同位置同句柄、异位置异句柄 | ✅ |
| 5 | `TextPass` 按 `Family` 解析，`script_face` 限定族内 | `an_icon_run_does_not_poison_the_latin_stack`——先画图标、再画拉丁，后者的整形结果必须与只画拉丁时相同（G6 的回归） | ✅ |
| 6 | `ui-widgets::icon::Icon` + 码点表生成脚本 | 画廊 `icons` 页：Outlined/Rounded/Sharp × FILL 0/1 × 尺寸 20/24/40 + 一个 `FILL` 动画样本 | ✅ |
| 7 | 手绘笔画端点统一为直角 | 像素断言：勾/横杠/滑块字形的端点像素覆盖率必须是 1 或 0，没有半覆盖的圆角 | ✅（此前已完成） |

**测试纪律**：第 3、4、5 步的测试都必须在**破坏实现后变红**再提交（AGENTS.md §7.6）。
第 5 步尤其重要——`script_face` 的污染是静默的，只有对照两组字面才能发现。

### 5.1 实施与本文的四处偏差

1. **重量留在 `TextStyle.weight`，不进 `Variations`。** 重量选字面（family 里那么多
   FontInfo），`opsz`/`FILL`/`GRAD` 是同一字面内的位置。混进一个类型会让「请求 700 必须
   落到 Bold 字面」这个既有不变量失去表达。`Variations` 只有三个轴。
2. **轴位置驻留成 `FaceId`，不给图集键加维度。** `FontiqueProvider::instance_axes` 把
   量化后的轴位置映射到一个新槽位；字重阶梯本来就是这么做的。字形缓存、整形器缓存、
   图集键因此都不变。代价是 `FaceId` 数量随用到的位置增长，1/64 量化把它压到有界。
3. **`Features` 是定长 `(tag, u32)` 表，不是位图。** 位图只认写死的四个特性；表能表达
   任意 OpenType tag（Flutter/GPUI 也都是列表），同时保持 `Copy`。容量 4，超出按写入顺序丢弃。
4. **`Icon` 不居中，盒子就是字形的推进宽度。** 文本节点的矩形是它自己的宽度，而
   `TextAlign::Center` 是在**换行宽度**里居中；在 Row 里两者不相等，字形会被排到节点矩形
   之外并被遮罩裁掉（`docs/lessons/design-errors.md`）。垂直居中也没有引擎支持，
   所以 `Icon` 只声明一个恰好等于字形的盒子，放置交给使用者。可点击的图标同样要包一层
   `Widget`：文本节点在命中测试里一律被忽略，`child_decl` 的 `Leaf` 也不上报 `Change`。

另外两条实施期发现（不属于原设计的四条缺口，但同批修了）：

- **行高按 run 自己的字面算**：`line_height`/`ascent` 原来读 `self.stack`（pass 的默认栈），
  图标 run 的行框因此由拉丁字面决定；现在按每个 span 的族取度量并取最大值。
- **画廊布局改用真实度量**：`AppLoop::new` 用 `MonospaceMeasure`（0.6 em/字符、1.3 倍行高），
  绘制却用真实字体；图标是 1 em 宽的字形，布局出的盒子只有 0.6 em。画廊改用
  `AppLoop::with_stack(&[])`，布局与绘制共用同一个 `TextPass`。

---

## 6. 不做的事

- **不做文本里的图标内联**（`Icon` 作为 `TextBlock` 的一个 span）。需要时再加：
  `Family` 已经是 span 级字段，内联只是把 `Icon` 拆成一次 `TextRun`。
- **不做连字 API**。PUA 码点覆盖 4161 个图标（4299 条连字里另有 138 个新图标尚未分配
  码点，生成脚本会跳过并报数）；连字只作为调试手段。
- **不做字体子集化**。4200 个图标全量约 1 MB；打包时再议。
- **不改断行器**。图标是单字符，`linebreak` 无需改动。
- **不改亚像素相位**。图标在 24dp 整数尺寸下不需要亚像素，而 `SUBPIXEL_PHASES` 已由现有逻辑处理。
