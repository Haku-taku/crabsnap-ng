# 布局引擎架构与行内排版调研

只读调研：先对比五套实现的尺寸协商契约，再给建议架构与迁移路线。缩写：`FLU` = `/home/ollie/git/widget-ref/flutter`（commit `950bf4e`；所引 `packages/flutter/lib/src/` 文件取自 GitHub 同 commit，行号为该文件行号）；`GPUI` = `~/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/crates/gpui/src`；`EGUI` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src`；`TAFFY` = 同目录 `taffy-0.15.0/src`；CSS 引 `w3.org/TR/CSS22/visudet.html`、`visuren.html`、`css-flexbox-1`、`css-sizing-3`、`css-overflow-3` 下载快照行号（正文以章节号为权威）；本项目现状引 `ui/crates/...`。

## 1. 五套实现怎么协商尺寸

| 框架 | 向下传 | 向上传 | 递归形态 | 内容尺寸来源 |
|---|---|---|---|---|
| Flutter | `BoxConstraints`(min/max × w/h) | `Size` | `layout()` 内自递归，子先完成 | `compute{Min,Max}Intrinsic{Width,Height}` |
| Qt | item 三元组 | `sizeHint/min/max Size` | 布局对象收集 item 解方程 | `sizeHint()` + `heightForWidth()` |
| GTK4 | `orientation` + `for_size` | `(minimum, natural, baseline)` | manager 逐子 measure 再 allocate | `measure` 虚函数 |
| GPUI | taffy `Style` + `AvailableSpace` | taffy `Layout`(size/location/baselines) | 每帧重建造树，一次求解 | `request_measured_layout` 闭包 |
| egui | `max_rect`（可用区） | `min_rect`（占用包围盒） | 单趟游标推进 | 控件自报 `desired_size` |

### 1.1 Flutter：约束进、尺寸出，约束只能收紧

每个 RenderBox 收到 `BoxConstraints`，布局完子节点后**挑一个满足约束的 `Size`**（`FLU rendering/box.dart:65-73`）；定位与尺寸是两个独立动作，位置变化不必然重布（`:69-71`）。四条不变量：

- **子必须服从父约束**（"The child is required to obey the given constraints"，`object.dart:2797-2798`）。违约不是"被裁掉"而是 debug 断言：`debugAssertDoesMeetConstraints` 检查是否设过 size（`box.dart:2566-2585`）、size 是否有限（`:2593`）。
- **约束只能收紧不能放宽**：`enforce(parent)` 把自身 min/max 各自 `clamp` 进父区间（`:222-229`）。父说"最多 200"时子的 `minWidth:400` 被压到 200——冲突由父裁决，不是子在下游堆一个更大的下限。
- **tight 轴**：`hasTightWidth => minWidth >= maxWidth`（`:370-373`）、`tighten({width})` 造单值轴（`:234`）；子看到 tight 轴就无需再问内容尺寸。
- **intrinsic 与 dry**：`getMinIntrinsicWidth → computeMinIntrinsicWidth`（`:1651-1666`、`:1771`）、`computeDryLayout(constraints)` 是"不许有副作用的投机尺寸"（`:2025-2076`）。文档明说 intrinsic 尺寸**不要求与 `performLayout` 的结果一致**（`:1706-1730`）——所以它只用于协商、从不用于定位。这个 dry/wet 二分正是"父要问子、又不想真的布局子"的干净解。

### 1.2 Qt：三元组 + heightForWidth 特例

`QLayoutItem` 全纯虚：`sizeHint / minimumSize / maximumSize / expandingDirections / setGeometry`（`/usr/include/qt6/QtWidgets/qlayoutitem.h:30-34`）+ `hasHeightForWidth / heightForWidth / minimumHeightForWidth`（`:37-38`）。`QSizePolicy::Policy` 用**位标志**组合 `Fixed / Minimum / Maximum / Preferred / MinimumExpanding / Expanding / Ignored`（`qsizepolicy.h:30-37`）——"能不能缩"与"能不能长"是两个独立的位；我们的 `AxisSize` 只有一个 `min`，`can_shrink()` = `min < hint`（`ui/crates/ui-core/src/layout.rs:67-69`），而它**全仓库无调用者**。短板也明确：`sizeHint` 不吃可用空间，任何"依赖宽度的高度"都得走 `heightForWidth` 旁路。

### 1.3 GTK4：两阶段，且 baseline 是一等公民

`GtkLayoutManagerClass` 只有两个布局虚函数：`measure(manager, widget, orientation, for_size, &minimum, &natural, &minimum_baseline, &natural_baseline)`（`gtklayoutmanager.h:63-70`）与 `allocate(..., width, height, baseline)`（`:72-76`）。三点直接相关：(1) `for_size` 把"沿一轴问另一轴"写进接口而非特例，配合 `get_request_mode`（`:60`）声明 width-for-height / height-for-width；(2) **baseline 同时出现在 measure 的输出与 allocate 的输入**，是布局契约的一部分，不由绘制端事后猜；(3) 尺寸是 `(minimum, natural)` 双值，与 CSS 的 `min-content`/`max-content` 同构（`css-sizing-3` §4），不是我们的三值单轴。`GtkWidgetClass.measure` 文档写明 "A widget will never be allocated less than its minimum size"（`gtkwidget.h:164-166`）。

### 1.4 GPUI：taffy 是外置求解器，文本仍要自己喂

每帧用 `Style` 造 taffy 节点（`GPUI taffy.rs:62-107`），叶子挂 `MeasureFn` 闭包（`:25`、`new_leaf_with_context` `:104`），求解走 `compute_layout_with_measure`（`:236`），`AvailableSpace` 的 `Definite/MinContent/MaxContent` 在 logical↔device 像素间双向换算（`:223-228`、`:255-261`）。它 `disable_rounding()`（`:45`）后自己补了两阶段像素对齐（求解前取整 authored 长度、求解后对齐边），理由是 flex 分配、百分比、居中、文本测量都会产生小数（`:283-312`）。能力边界：`Style::display` 只有 `Block|Flex|Grid|None`（`style.rs:1131-1141`），**没有 inline**。文本走 `request_measured_layout`（`elements/text.rs:654`），闭包内把 `available_space.width` 折成 `wrap_width`（`:657-666`）。它踩过的坑要抄进我们的设计：**被截断过的测量结果会污染 intrinsic 尺寸**——"a truncated layout answers an unconstrained probe with the truncated size, which poisons intrinsic sizing with whatever width some earlier measure pass happened to use"（`:686-696`）；解法是缓存命中要求 `wrap_width` 相等且未截断（`:697-701`）。

### 1.5 egui：不做尺寸协商，只报期望尺寸

`Ui::allocate_space(desired_size)`（`EGUI ui.rs:1188`）没有 min/max 区间：控件自报期望，`Placer::next_space` 给 frame（`placer.rs:113-125`）、`justify_and_align` 决定落点（`:137`）、`advance_after_rects` 推游标（`:165`）。父从不压缩子，**溢出就溢出**：`Ui` 只把 `min_rect` 与 `max_rect` 一起撑大来容纳（`ui.rs:691-698`）。因为它不知道内容尺寸，自动尺寸的容器要多跑一趟：`Area` 无状态时先跑 `sizing_pass`，整棵子树 `invisible()+sizing_pass()`（`containers/area.rs:444-487`、`:625-627`），grid 在该趟把列宽当 0（`grid.rs:193-196`），拿到 `content_ui.min_size()`（`:668`）后再真画，并 `request_discard` 盖住首帧抖动（`grid.rs:465-472`）。**这条路对我们是关着的**：`Widget::decl` 拿不到测量器（`docs/engine-gaps.md` §12）。

## 2. 建议架构

### 2.1 结论形状

取 **Flutter 的契约**（约束 = (min,max)×两轴、输出 = Size、约束只收紧、违约用断言而不是裁剪）+ **CSS/taffy 的尺寸词汇**（`min-content`/`max-content` 双值 + `Avail::Definite/MinContent/MaxContent`）+ **GTK 的 baseline 一等公民地位**。不引入 taffy（§2.7）。行内排版由本项目自有的单次断行趟负责——即使 taffy 自己也把 line box 留给外部："inline formatting contexts (line boxes) which are laid out externally to Taffy"（`TAFFY compute/block.rs:33`）。

### 2.2 格式化上下文：属性还是节点类型

CSS 把两件事分开：**外侧值**（本盒如何参与父上下文：inline-level / block-level / atomic inline / out-of-flow，`visuren.html:339-368` §9.2.2）与**内侧值**（本盒为子节点建立什么上下文：IFC / BFC / flex / grid，§9.4.2）。我们的 `NodeKind` 把两者混了，且混错的方向很具体：`Column/Row/Stack` 是内侧上下文，`Box/Text` 是叶子负载，而 `NodeKind::Overlay` 其实是**外侧值**（out-of-flow）——`Tree::floats()` 的判据就是 `n.kind == NodeKind::Overlay || is_modal(nid)`（`ui/crates/ui-core/src/tree.rs:1285-1288`），与它内部怎么排子节点无关。

- `NodeKind` **保留**，收窄为"内侧上下文"：`Column | Row | Stack | Block | Inline`。它是身份的一部分，`Tree::apply` 靠它决定"换 kind 就退役整棵子树"（`tree.rs:1050-1053`）。
- **新增独立字段** `outer: OuterFlow { Flow, Atomic, OutOfFlow }`，默认 `Flow`。新字段不触发 `kind_changed`，因此不动既有节点的身份与缓存；`floats()` 改读 `outer`。选择依据是改动面：kind 的分支只在引擎 4 处（`layout_engine.rs:360,368,445,450`）加 `view.rs:547-552` 一处 match，但**改变已有节点的 kind 会退役其子树**，迁移期每改一次就是一次身份抖动；加字段只动读取处。
- 行内**内容**（文本 run + 原子行内盒）既不属于 kind 也不属于 `outer`，它是 `Inline` 上下文里一次 line-breaking 的产物（§3）。

### 2.3 测量：两趟，还是双尺寸单趟

答案：**一趟递归、两种模式**。Flutter 与 taffy 在此殊途同归——"自上而下的约束 + 自下而上的尺寸"是同一趟递归的前后两半（`box.dart:63-66`："walks down the render tree passing constraints, then walks back up passing concrete geometry"），而 intrinsic / min-content 查询是**另一个入口**：按需、可缓存、dry（`:1651`）。

现状：`Constraints` 只有 `max`，`soft_max` 从未被读（全仓库仅 `layout.rs:130,137,142`，都在构造里）；`measure_node` 只吃 `available: Size`（`layout_engine.rs:275`）。**没有 min 约束就无法表达"这行至少要多宽"，也就无法表达"不收缩"**（§2.4）。建议：

```rust
pub enum Avail { Definite(f32), MinContent, MaxContent }
pub struct Constraints { pub min: Size, pub max: Size, pub avail: Size<Avail> }
/// 一次 measure 的输出：定尺 + 两个内容尺寸 + 基线，一起缓存。
pub struct MeasureResult { pub size: Size, pub min_content: Size, pub max_content: Size, pub baseline: Option<f32> }
```

`measure(node, constraints) -> MeasureResult`：某轴 tight 时只算那一轴；`min_content`/`max_content` 在同一趟顺手算出并缓存。Flutter 对文本正是如此——把 placeholder 的宽度换成 min/max-content、高度给 dummy 0，再重跑 `computeMin/MaxIntrinsicWidth`（`paragraph.dart:804-831`）。父约束与子内容不一致时，定尺取 `max(child.min_content, parent.min).min(parent.max)`，冲突记进 `LayoutStats`（debug 断言、release 只溢出）。分工：**收紧发生在传约束那步（enforce），钳制发生在出尺寸那步（constrain）**。

### 2.4 收缩语义：压扁必须变成溢出

CSS 的规则是**盒子不会为迁就父容器而变小**；父变小 → 内容溢出 → `overflow` 决定裁还是滚（`css-overflow-3`，`scroll container`）。唯一例外是规范明文授予的：flex item 主轴的 **automatic minimum size**（`css-flexbox-1` §4.5，锚点 `min-size-auto`）——`min-width:auto` 让它的主轴下限取自身 min-content 而非 0。CSS2.1 §10.4 的 min/max-width 算法 likewise 只在**自身声明**的区间内钳制，从不参照父的可用宽度（`visudet.html:673` 起）。

现状相反，且被当成特性写着：`distribute()` 的注释"亏空压向各自下限且不低于下限"（`layout_engine.rs:86-88`），实现取 `axis(sc).min` 作地板（`:140-143`），而**多数控件的 `min` 是 0**（`AxisSize::hug(0.0, …)` 53 处），于是 `out[i] -= take * (room / compressible)`（`:161`）把固定尺寸的子节点一起压小。`docs/lessons/design-errors.md` 已两次记到：「布局装不下就会溢出」（内容被**压缩塞进**盒子，永远不溢出，滚动容器因此无事可滚）；「内容比容器高一点…」（`distribute()` 连**固定尺寸**子节点一起变小，遮罩等于挤压后的矩形）。架构层保证：

1. `distribute()` 的地板从"声明的 `min`"换成**测得的 `min_content`**（§2.3）——即把 CSS 给 flex item 的自动下限推广为所有盒子的默认。
2. 地板之和超过可用空间时**不再分配**：每个孩子拿自己的 `min_content`，容器返回 `LayoutStats{overflow}` 并向上传；盒子自身尺寸**由父给的约束决定**，不由内容决定。
3. debug 断言 `child.size >= child.min_content`，违反即报"引擎压缩了一个不该压缩的盒子"并打印是哪条祖先链给了 tight 约束。这是本节的**可执行检查**，对标 `debugAssertDoesMeetConstraints`（`box.dart:2566`）。
4. `hug` 与 `fill` 现在**构造完全相同**（都产出 `max: INFINITY`，`layout.rs:39-55`），这是压扁得以蔓延的语义漏洞：紧贴内容的盒子在架构上没有任何东西阻止它被撑大或压小。`hug` 必须落到"定尺 = shrink-to-fit"，`fill` 落到"定尺 = 拉伸到父给的 max"。

### 2.5 裁切契约

现状：`mask = 父.mask ∩ 自己的 rect`（`layout_engine.rs:424-431`），且 `accumulate_offsets` 让 mask 随 offset 平移（`:215`）。两条各自合理，合起来就是"文本下端被裁"与"窗口小时被裁而不是滚动"的直接来源：**裁剪矩形被子的矩形决定**，于是墨迹超出矩形（图标字形、阴影、`ink_margin` 声明的外扩，`tree.rs:385`）必被切；同时 offset 移动 clip，滚出去的内容 clip 也跟着走。建议：

- **裁剪是容器的属性，不是子的属性**：`clip(node)` = 最近一个声明裁剪的祖先的 padding-box ∩ 该祖先自身的 clip，**与子的 rect 无关**。子的 `ink_margin` 只在 `clip == Visible` 时才允许影响脏区。
- 三态 `Clip::{Visible, Hidden, Scroll}`（`css-overflow-3`）：`Visible` 不产生 clip；`Hidden` 产生 clip 但不滚动、不参与 intrinsic；`Scroll` 产生 clip **并建立独立格式化上下文**。`ContainerParams::scrollable`（`tree.rs:211`）现在是一刀切的唯一开关，要拆成 `clip` + `scroll_axes`。
- **滚动容器不改变自己的 intrinsic 尺寸**：min/max-content 按视口算，内容尺寸走 `scroll_extent` 单独上报（taffy 用 `scrollable_overflow_rect` 表达，`TAFFY tree/layout.rs:475-478`）。现有实现相反：`scrollable` 时把 `main_avail` 改成"内容总量"（`layout_engine.rs:482-491`），那是绕开压缩的权宜之计，§2.4 的地板改成 min-content 后应删。
- **offset / scroll 不移动 clip**：`offset` 只影响绘制与命中（`tree.rs:361-367` 的契约是对的）；`:215` 那句 `mask.translate(own)` 改成"clip 在布局期按绝对坐标算好，offset 只在绘制端乘"。`corners`（`tree.rs:253`）参与 clip 的形状，现状已对。

### 2.6 失效与缓存

现状：**`dirty` 是装饰**。`layout_engine.rs` 全文没有一处读 `n.dirty`；`Tree::layout_is_dirty()`（`tree.rs:1340`）无外部调用者；每帧无条件整树 `solve`（`ui/crates/ui-platform/src/view.rs:966`）。`apply()` 只在 `data`/`size_class` 变化时 `n.dirty = true`（`tree.rs:1091-1097`），`set_layout_clean()` 每帧无条件清（`:1331-1338`）。真正省下工作的是 `TextPass` 的测量缓存（`text_pass.rs:714-730`，key 见 `:1239-1252`）——那是文本层的，不是布局层的。Flutter 的两条机制可直接照搬语义：

- **约束相等就跳过**：`if (!_needsLayout && constraints == _constraints) return;`（`object.dart:2871-2885`）。`Constraints` 已 `PartialEq`（`layout.rs:125`），缺的是**在节点上存上一次吃进的约束**（现在没存）。
- **relayout boundary**：`_isRelayoutBoundary = !parentUsesSize || sizedByParent || constraints.isTight || parent == null`（`:2870`）——父不读子尺寸、或子被 tight 约束时，脏不必再往上冒。这条决定"标脏标到哪里为止"。

建议：`Node` 增 `last_constraints: Option<Constraints>` 与 `measured: Option<Rc<MeasureResult>>`；`dirty: bool` 换成 `generation: u32` + `mark_upward(boundary)`。`platform_boundary.rs` 抓不住这件事，用差分测试抓："改一个叶子 → 被测量节点集合 == 期望子树"（AGENTS.md §7.4 的属性/差分层）。

### 2.7 是否引入 taffy：不引入

依据不是边界测试——taffy 的依赖只有 `slotmap` + 可选的 `smallvec/cssparser/serde/document-features`（`TAFFY Cargo.toml:135-160`），不含 OS 特定 API，加进 `ui-core` 不会让 `platform_boundary.rs` 的 `FORBIDDEN_IN_PORTABLE`（`:48`）或两条依赖检查变红。三条真理由：

1. **行内不在它的能力范围**（`block.rs:33` 明说 line box 由外部布局），而我们的难点恰在行内 + baseline（§3）。引入它我们仍要自己写行内趟，两套趟必须就每个文本盒达成一致——等于把 `design-errors.md` 里"布局与绘制用了两个测量器"的坑复制到布局与布局之间。
2. **双树双缓存**。taffy 自带 arena（`TaffyTree<NodeContext>`，`taffy.rs:32`），GPUI 因此每帧重建节点（`:62-107`）。我们的 `Tree::apply` 已在做身份复用（`tree.rs:1030` 起），第二棵树意味着第二套失效逻辑，两套缓存的一致性比一套缺失更难查。
3. **词汇是超集且我们不想要全部**：百分比、`fr`、`aspect_ratio`、`box_sizing`、margin collapsing、`position::absolute` 都在 `compute/leaf.rs:46-90` 一个函数里；GPUI 为此补了两阶段像素对齐（`taffy.rs:283-320`）。

保留 `LayoutEngine` trait 作为出口（`layout.rs:163-174` 已有）。将来若真要完整的 grid track sizing，用 taffy 的**低层 API**（`compute_grid_layout` 单节点函数）当算法库而不是当引擎——那才是它适合被嵌入的形态。

## 3. 行内排版与 TextPass 的对接

### 3.1 一次断行能否同时处理文本 run 与原子行内盒

**能，但必须把原子塞进待断行的字符串里**，不能在字符串外拼宽度。Flutter：`WidgetSpan` 继承 `PlaceholderSpan`（`widgets/widget_span.dart:71-86`），段落在文本流里插入 `U+FFFC OBJECT REPLACEMENT CHARACTER`（`rendering/paragraph.dart:387-388`），先对每个子节点用 **`BoxConstraints(maxWidth: maxWidth)`** 量出 `PlaceholderDimensions`（`:195-203`，单 child 的换算见 `:147-175`），喂给 `TextPainter.setPlaceholderDimensions`，最后用断行结果的 `inlinePlaceholderBoxes` 定位回去（`:972-983`；`positionInlineChildren` 见 `:220-231`，其文档明确某些 child 会因省略而不被定位，`:212-216`）。intrinsic 查询走同一流程，只是 child 尺寸换成 min/max-content（`:804-831`）。

我们的 `ui_text::wrap_indented<F>(text, max_width, max_lines, indent, width_of)`（`ui/crates/ui-text/src/linebreak.rs:189`）的 `width_of(&str, Range<usize>) -> f32` 是**天然注入点**：断行器只问"这段字节多宽"。最小改动方案：`TextBlock` 的 run 序列允许"原子 run"（内容为单个 `U+FFFC`，携带 `InlineAtom`），`width_of` 命中它就返回预量好的宽度，其余不动；`Line.range` 仍是字节区间。三个前提必须先定：

- **量原子的约束只能给块宽，不能给剩余行宽**——行宽依赖原子宽、原子宽依赖行宽，Flutter 用"整段 maxWidth"斩断这个环（`paragraph.dart:200`）。要写成显式限制，否则一定会有人把它"优化"成按行宽量，于是不收敛。
- `U+FFFC` 在 UTF-8 里占 **3 字节 / 1 个 UTF-16 码元**，且不得出现在普通文本 run 中；字节区间 ↔ 码元区间的换算集中一处（插入符、选区、IME 都按码元说话）。
- **省略掉的原子不得被定位**：位置来源永远是存活行表（`max_lines` 截断后），不是子的顺序（`paragraph.dart:212-216`）。

### 3.2 baseline 从哪来

CSS2.1 §10.8.1 给了公式：对每字形取字体 `A`/`D`，`L = line-height − (A+D)`，半 leading 上下均分，故 `A' = A + L/2`、`D' = D + L/2`；**行框高度恰等于 `line-height`**；空的内联盒用 strut（零宽、取"首个可用字体"的隐形字形）；OpenType 建议用 `sTypoAscender/sTypoDescender`，缺失时退 `hhea`（`visudet.html:1401-1457`）。`TextPass::line_height` 与 `ascent` 已是该公式的"跨 span 取最大"版本（`text_pass.rs:767-793`），但**行基线目前是纯解析式**：`line_baseline = origin.y + ascent + i*(line_h + line_gap)`（`text_pass.rs:501-503`），没有"行内最高项决定基线"这一步。

原子盒的基线也来自规范：**`inline-block` 的基线是它最后一个正常流行框的基线；若没有流入的行框，或 `overflow` 计算值不是 `visible`，基线就是它的下外边距边**（CSS2.1 §10.8，`visudet.html:1648`）。这条决定了很多观感：带裁剪的图标按钮里，图标与文字"对齐到底"而不是对齐基线，是规范而非 bug。Flutter 把它做成接口：`computeDistanceToActualBaseline(TextBaseline)`（`box.dart:2477`；文本实现 `paragraph.dart:943-953`），`RenderFlex` 的 `CrossAxisAlignment.baseline` 用 `_AscentDescent` 收集每项 ascent/descent 决定行高（`flex.dart:53-98`、`:316-346`）。

建议：`MeasureResult` 带 `baseline: Option<f32>`（GTK 也这么带，`gtklayoutmanager.h:69-70`；taffy 的 `Baselines{first,last}` 同理，`TAFFY tree/layout.rs:201-213`）。原子的基线按上面那条 CSS 规则由**子自己的 measure 结果**给出，不由行内趟猜。

### 3.3 `vertical-align` 最小集

值语义定义在 CSS2.1 §10.8 的属性表（`visudet.html:1556`，`vertical-align: baseline | sub | super | top | text-top | middle | bottom`）与其对行框的作用（§10.6.1，`:969` 起；`line-height` 对行框的作用见 §10.8 正文 `:1357`、`:1380`）。只做四个，全部可验收：`Baseline`（默认，与行框基线对齐；文本里插图标、`inline-block`）、`Top` / `Bottom`（与行框顶/底对齐；徽标、状态点）、`Middle`（与父字体 x-height 中线对齐；图标配单行按钮文字）。不做 `text-top`/`text-bottom`（需要额外度量通道）与长度值（`vertical-align: 3px`），显式记为"暂不支持"。Flutter 的对应枚举 `PlaceholderAlignment{top,bottom,middle,aboveBaseline,belowBaseline,baseline}` 且 `baseline` 必须与 `TextBaseline` 同时给（`widget_span.dart:81-86`），可佐证这个集合划分够用。

### 3.4 `inline-block` 的收缩宽度

shrink-to-fit 公式：`min(max(preferred minimum width, available width), preferred width)`（`visudet.html:473`，§10.3.5；§10.3.9 让 `'Inline-block', non-replaced elements` 沿用浮动的这条，`:655-663`）。`preferred width` = max-content，`preferred minimum` = min-content——这正是 §2.3 那两个缓存值的唯一消费者。**所以必须先有双尺寸，行内趟才落得了地**；否则此处只能填 `available`，于是"一段带 `inline-block` 的文字在窄父里该换行还是该溢出"没有答案。

### 3.5 与 `max_lines` / 省略号的配合

`wrap_indented` 现在就是"截断 + 末行加省略号"，`Line.ellipsis` 为真时由绘制端追加 `ELLIPSIS`（`ui-text/src/linebreak.rs:171-231`）。行内趟再加两条：

- `max_lines` 数的是**行框**不是文本行：一个把行高顶起来的 `inline-block` 不额外占行；但被省略掉的原子**必须**从 `Prepared.lines` 里消失，不能只是不画（否则命中与选区会算到不存在的位置）。
- **被省略号截过的测量结果不得作为 intrinsic 答案返回**——GPUI 的"poisons intrinsic sizing"（`elements/text.rs:686-696`）是现成事故报告。我们的 `measure_key` 已把 `max_lines` 纳入 key（`text_pass.rs:1244`），但缓存返回 `Measured{size, lines}` 而非双尺寸；加 min/max-content 时必须把"截断过的结果"与"探测用的结果"分成两个条目。

### 3.6 接口草图

```rust
// ui-core：块里可以出现原子
pub struct TextRun { pub text: String, pub style: TextStyle,
    /// Some() 即原子行内盒；`text` 必须是单个 U+FFFC。
    pub atom: Option<InlineAtom> }
pub struct InlineAtom { pub node: NodeId, pub valign: VerticalAlign }  // 参与常规布局树的子节点
pub struct AtomicBox { pub size: Size, pub baseline: Option<f32> }     // 引擎供给文本趟

// ui-platform/src/text_pass.rs
impl TextPass {
    /// 行框断行：原子的宽度与基线由调用者**预先量好**（约束 = 块宽）。结果缓存。
    pub fn layout_inline(&mut self, block: &TextBlock, atoms: &[AtomicBox],
                         avail: Size<Avail>, max_lines: Option<u32>) -> Rc<InlineLayout>;
}
pub struct InlineLayout { pub size: Size,
    pub min_content: Size, pub max_content: Size,     // §3.4 的 shrink-to-fit 输入
    pub first_baseline: f32, pub last_baseline: f32,  // 作为原子时上报 last
    pub truncated: bool,                              // true 时尺寸不得当 intrinsic 用
    pub lines: Vec<InlineLine> }
pub struct InlineLine { pub baseline: f32, pub height: f32, pub items: Vec<Placed> }
pub enum Placed { Text { range: std::ops::Range<usize>, x: f32 }, Atom { slot: usize, x: f32, top: f32 } }
```

一个 `Inline` 容器的处理顺序（对应 §3.1 的三个前提）：`layout_inline(avail = MaxContent)` 探内容 → 对每个 `atoms[i].node` 用 `Constraints{min:0, max:Definite(块宽)}` 递归 `measure` 得 `AtomicBox` → `layout_inline(avail = Definite(w))` 得真实行框 → 按 `Placed::Atom{x, top}` 定位子节点（**定位在子已按块宽定尺之后**）。绘制端不再自己按 `wrap_width` 断行，而是消费同一个 `Rc<InlineLayout>`（现状是把数字传下去：`tree.rs:388`、`view.rs:1261` 与 `:3146`）。

## 4. 迁移路线

### 4.1 字段归宿

| 现有 | 归宿 | 说明 |
|---|---|---|
| `NodeKind::{Column,Row,Stack}` | 保留（内侧上下文） | 语义不变 |
| `NodeKind::Overlay` | 保留 + 重新解释 | 等价 `outer=OutOfFlow`；`floats()` 改读 `outer` |
| `NodeKind::{Box,Text}` | 保留 | kind 只表示"无子节点上下文"，负载由 `NodeData` 决定 |
| `NodeKind::{Block,Inline}` | **新增** | 新变体不触碰 `kind_changed`（`tree.rs:1050`） |
| `Node.outer: OuterFlow` | **新增字段** | 默认 `Flow`，不改身份与缓存 |
| `SizeClass.width/height` | 改名 `Sizes`（`min/preferred/max`） | 现 `AxisSize{min,hint,max}` |
| `AxisSize::hug` / `::fill` | **拆语义**（当前构造相同，`layout.rs:39-55`） | hug→shrink-to-fit；fill→拉伸到 max |
| `SizeClass.grow_x/grow_y` | 改名 `Flexing{basis, grow}` | `Grow` 四态保留，基准显式化 |
| `Constraints.max` | **补 `min`** | 必须：§2.4 的地板靠它 |
| `Constraints.soft_max` | **删除** | 从未被读（仅 `layout.rs:130/137/142`） |
| `Constraints.avail: Size<Avail>` | **新增** | intrinsic 探测入口 |
| `MeasureResult{…}` | **新增** | 挂 `Node.measured: Option<Rc<_>>` |
| `ContainerParams.gap/padding` | 保留 | — |
| `ContainerParams.cross_align/cross_pos` | 改枚举 `Align` / `Justify` | 0..1 分数无法表达 `Baseline` |
| `ContainerParams.scrollable` | 拆 `clip: Clip` + `scroll_axes` | §2.5 |
| `ContainerParams.scroll_x/scroll_y` | 保留，语义不变 | 仍是视图值，不参与 intrinsic |
| `Node.mask` | 保留，算法改写 | 不再 ∩ 子矩形、不随 offset 平移 |
| `Node.wrap_width` | 替换 `Node.inline: Option<Rc<InlineLayout>>` | 绘制消费对象而非数字 |
| `Node.dirty: bool` | 替换 `generation` + `last_constraints` | §2.6 |
| `Tree::layout_dirty` | 保留（帧标志） | 新增 relayout boundary 判定 |
| `measure_node/place_node` | 合并 `measure(node, constraints)` + `place(node, origin)` | 定尺来自约束，不在分配里改尺寸 |
| `distribute()` 亏空压缩分支 | **删除**（换溢出上报） | §2.4 |
| `TextMeasure` trait | 替换为 §3.6 的 `layout_inline` | `MonospaceMeasure` 留在测试里 |

### 4.2 阶段划分

顺序不可换：P4 依赖 P3，P3 依赖 P1，P1 依赖 P2 的 clip 定义（否则溢出时的可见性没有答案）。每阶段验收都是可执行的。

- **P0 契约与检查先行**：`Constraints` 加 `min`、`MeasureResult` 三值，只加断言不改行为。验收：新断言在现有画廊上跑出"引擎压缩了固定盒子"的报告清单（**预期非空**——那两个已知缺陷），分层测试仍绿。先看它红。
- **P1 取消压缩**：删 `distribute()` 亏空分支，地板换 min-content，溢出向上传。验收：P0 清单逐条消失；新增 `a_fixed_child_is_never_shrunk_below_its_content` 先红后绿；`layout_engine.rs:482-491` 的权宜代码可删。
- **P2 clip 契约**：`Clip{Visible,Hidden,Scroll}`，clip 属于容器、不随 offset 平移，滚动容器的 intrinsic 按视口。验收："文本下端被裁"的嵌套会话复现用例转绿；`ink_margin` 非零的节点墨迹不再被自身矩形切。
- **P3 双尺寸与 shrink-to-fit**：`Avail` 三态全链路，`hug`/`fill` 拆语义。验收：属性测试 `max_content ≥ min_content` 且 `size ∈ [min_content, max]`；差分测试"增量 == 全量"。
- **P4 行内**：`InlineAtom` + `U+FFFC` + `layout_inline` + `vertical-align` 四值 + 基线上抛。验收：图标嵌在中文段落里按基线对齐；带裁剪的原子按底边对齐；`max_lines` 截掉的原子不参与命中与选区。
- **P5 增量**：`last_constraints` + relayout boundary + 测量缓存分条目。验收：差分测试"改一个叶子 → 被测节点集合 == 期望子树"；release gallery 的 `layout` 帧时间不高于 P4 基线。
- **P6（可选）显示属性化 / grid**：为 wrap 扩 `NodeKind`，或接 taffy 的低层 `compute_grid_layout`。验收：`platform_boundary` 三条仍绿。
