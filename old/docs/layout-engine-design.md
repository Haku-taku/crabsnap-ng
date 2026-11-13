# 布局引擎设计

> **状态**：设计定稿，待实现。实现按 §8 的阶段推进，每阶段独立可验收。
>
> 依据：`docs/research/layout-box-model.md`、`layout-flexbox.md`、`layout-grid.md`、
> `layout-architecture.md`（下称「盒模型」「flex」「grid」「架构」）。规范条款与
> 参考实现行号都在那四份里，本文只引用结论，不重复论证。
>
> 本文的每条「现状」都在 `ui-core` 源码里核对过，行号可查。

## 0. 目标与非目标

**目标**：一套能表达 CSS 块 / 行内 / flex / grid 子集与主流 UI 引擎对齐模型的布局引擎，
用它替换 `layout.rs` + `layout_engine.rs` 的现有实现，并且**不破坏**现有控件作者的 API 形状
（`SizeClass::hug/fixed/fill`、`Grow`、`ContainerParams`）。

**要解决的现实症状**（用户实测）：

| 症状 | 本文负责的部分 |
|---|---|
| 窗口变小时控件被压缩变形，而不是溢出被裁 | §4.2 不压缩不变量 |
| 某些窗口尺寸下文本下端被切掉 | §4.7 行盒高度、§4.8 裁切归属 |
| `Text::max_width` 无效 | §6.1 尺寸词汇的迁移 |
| 滚动容器没有真正的溢出内容 | §4.2 + §4.8 |

**非目标**（§7 有理由）：float、margin collapsing、sticky 的布局期实现、RTL/书写模式、
`aspect-ratio`、嵌套 stacking context、负 z-index。

## 1. 现状：五个已核实的事实

这五条是重写的理由，不是抱怨。它们共同说明现有引擎缺的不是某个特性，而是**契约**。

1. **约束只有上界**。`Constraints { max, soft_max }`（`layout.rs:126-131`），`soft_max`
   全仓库无人读取（`grep soft_max` 只命中定义与两个构造器）。父节点无法说「至少这么宽」，
   于是任何「填满」语义都只能靠子节点自己猜。
2. **`hug` 与 `fill` 字节等价**。`AxisSize::hug`（`layout.rs:39-45`）与 `::fill`
   （`layout.rs:49-55`）都返回 `{ min, hint, max: INFINITY }`。「适应内容」与「跟随父」
   在类型层面无法区分，只有 `grow_x/grow_y` 间接补救，而 grow 只对 flex 容器生效。
3. **压缩是默认行为，且下限是声明值**。`distribute()`（`layout_engine.rs:89-164`）在
   `spare < 0` 时把缺口按可压空间分摊，下限取 `axis(sc).min.max(0.0)`（`:142`）——
   通常是 0。所以窗口变小 → 控件被压扁，而不是溢出。增长路径（`:118-137`）只按权重分，
   **从不查 `max`**，所以「最多 320」这类上界在 grow 时失效。
4. **每个节点都是 `overflow:hidden`**。`place_node` 里 `mask = parent_mask ∩ rect`
   （`layout_engine.rs:424`）无条件求交，且 `mask` 随 `Decl::offset` 平移（`:215`）。
   `visible` 无法表达；同时滚动容器没有「内容尺寸 / 视口尺寸」两套值。
5. **增量布局是装饰**。`Node::dirty` 从未被 `layout_engine` 读取；`solve()` 每次全量。

## 2. 架构选择

### 2.1 契约：约束进，尺寸出（Flutter 形状）

一次测量调用是纯函数：

```
measure(node, Constraints { min: Size, max: Size }) -> MeasureResult
MeasureResult { size, min_content, max_content, baseline }
```

**两条不变量**（debug 断言，release 下仍按 clamp 收口）：

- **I1（下界是硬的，上界是软的）**：记
  `floor = max(declared_min, min(min_content, max(declared_min, declared_max)))`，则
  `used >= floor - EPS` 恒成立；`used <= max(max, floor) + EPS` 也恒成立。
  换句话说，引擎只能**因为内容或显式尺寸**超过可用空间，不能凭空超。
  `min_content` 要先用自身声明尺寸夹住，否则 `fixed(72)` 的输入格会被 148 高的内容撑到 148。
- **I2（只有父收紧）**：子节点只能把约束收紧传下去，不能放宽。

> **为什么上界不能写成 `used <= max`**：CSS 允许显式尺寸与内容撑破包含块
> （`width: 192px` 放进 168 的内容盒 = 溢出，不是压成 168），我们的「不压缩」正是要这个。
> 把 I1 写成硬上界会与 §4.2 直接矛盾——P0 实现时 4 条 md3 测试因此变红
> （`fixed(192)` 进 168、24 图标进 20 槽、hug 列 148 高进 72 盒），而那些正是要治的症状。
> 记入 `docs/lessons/design-errors.md`。
> **P0 只落地下界断言**（`used >= floor`）；上界要等 P1 有了 `min_content` 才可断言，
> 在那之前它是恒真式。P0 另加 I2 自洽断言（传下来的约束必须 `min <= max`）。

这两条一立，「父说至少多宽」就有了表达，`hug`/`fill`/`fixed` 也不再需要靠魔法字段区分。
`Constraints::tight/loose` 保留现有签名语义（tight = min==max），新增
`Constraints { min, max }` 后 `loose` = `min: ZERO`，`tight(s)` = `min: max: s`。

### 2.2 词汇：CSS 尺寸三值

沿用盒模型 §2.4 的结论：**内容贡献**（min/max-content，测量得到）、**作者指定**
（`width`/`min`/`max`）、**可用空间**（扣完 margin 与 padding）必须分成三类输入，
不能塞进 `(min, hint, max)`。

### 2.3 不 vendor taffy

理由（架构 §4）：taffy 没有 inline 布局（我们要文本原子），它自己建树（与我们 `Tree` 冲突），
它的词汇是超集（`Style` 300+ 字段，含我们明确放弃的 float/block 流）。保留 `LayoutEngine`
trait 作为出口；将来若需要它的 grid 算法，只把 `compute_grid_layout` 当算法库调用，
不接管测量循环。

## 3. 数据模型

### 3.1 尺寸词汇

```rust
pub enum Len { Auto, Px(f32), Pct(f32) }   // Pct 相对包含块同轴尺寸；不确定时按 Auto

/// 一个轴要怎么定尺寸。上下文无关，上下文只决定 Auto 的含义。
pub enum Sizing {
    /// 精确值（fixed）。
    Definite(Len),
    /// 收缩到内容：clamp(min_content, avail, max_content)（CSS fit-content / hug）。
    Fit,
    /// 占满父给的可用空间（CSS stretch / fill）。
    Stretch,
    /// 由容器算法决定（flex item 主轴、grid item、abspos 一轴 auto）。
    Auto,
}

pub struct AxisSizes {
    pub size: Sizing,
    /// 初始 0 / 无上界（CSS min-width:0, max-width:none）。
    pub min: Len,
    pub max: Len,
}
```

三个构造器成为语法糖，**语义不同**，这是修事实 2：

| 构造器 | 结果 |
|---|---|
| `fixed(v)` | `size = Definite(Px(v))`，`min = Px(v)`，`max = Px(v)` |
| `hug(min, preferred)` | `size = Fit`，`min = Px(min)`；`preferred` 只在 `preferred > min` 时写成 `max = Px(preferred)`（向后兼容旧语义），否则 `max = Auto` |
| `fill(min, preferred)` | `size = Stretch`，`min = Px(min)`，**`max = Auto`**；`preferred` 只作 flex 基准，不是上限 |

`fill` 的 `preferred` 不进 `max`，这与 `hug` 不同：`fill` 要的是空间本身，「最多到某个舒适值」
是 `hug` 的语义。P1 曾把两者统一成「hint → max」，导致 `fill(0, 100)` 在 800 宽的父容器里
停在 100，并迫使一条既有测试从 `fill(0,10)` 改成 `fill(0,0)`；已改回。

`preferred` 的兼容处理是必要的：57 处 `hug(0.0, X)` 里，`X` 的既有含义是「舒适宽度」
（`AxisSize::hint` 的文档原话），在 flex 里是 basis 的上界。把它当 `max` 会让
`hug(0.0, 120.0)` 的文本永远不超过 120——**这正是 `Text::max_width` 想要的**（见 §6.1），
但对容器是行为变化。因此：

- `preferred == 0.0`（绝大多数调用：`hug(0.0, 0.0)`）→ `max = Auto`，行为不变；
- `preferred > 0.0` 且是 `hug` → `max = Px(preferred)`，**逐点审查**（§8 P1 的清单）；
- `fill` 一律 `max = Auto`，见 §3.1。

### 3.2 盒模型：每个节点都有

`box_model` 已是 `Node` 的一等字段，`Decl::padding()`/`margin()` 写入它（`spawn` 时折入）：

```rust
pub struct BoxModel {
    pub margin: Insets,     // 允许负值，只影响摆放
    pub margin_alt: Option<Insets>, // 流父容器改沿交叉轴堆叠时用的那份
    pub padding: Insets,
}
```

**没有 `border` 与 `box_sizing`**（审计 F2，`docs/research/engine-audit.md`）：原先设计的
「`border` 占布局空间」与引擎实际的做法相反——边框由 `Decl::border()` 画成盒内描边
（`Shape::Box { border }`，`docs/engine-gaps.md:570` 的缺口因此才能切出来），不占空间；
声明尺寸是**边框盒**（有意偏离 CSS 的 `content-box`），这个选择本身已由
`a_declared_size_is_a_border_box` 钉住。两个字段都没有生产者也没有读取者，故删除。

已知偏离（未修）：`Text` 叶子自己的 padding 被测量与绘制共同忽略——`Decl::padding()`
对它是空的。修法要同时动 `ui-text` 的换行宽度与 `ui-platform` 的文本原点/插入符映射，
见审计表第 2 项。

`Decl::padding()` 签名不变，只改写入目标。**`rect` 始终是 border box**，margin 只影响摆放。

### 3.3 flex 因子

```rust
pub struct Flexing {
    pub basis: Len,      // Auto = 内容基准
    pub grow: f32,
    pub shrink: f32,     // 默认 1（CSS 默认），与今日 `Grow::No` 的「不压」不同，见 §6.2
}
```

`Grow` 保留为糖：`No → (auto, 0, 0)`、`Yes → (auto, 1, 1)`、`Flex(w) → (auto, w, 1)`、
`Split(w) → (0, w, 1)`。**`Grow::No` 映射到 `shrink: 0`**，保持今日「声明了就不被压」的
既有作者预期；要 CSS 默认压缩得显式写 `Flexing`。

`basis: Auto` 取 **`max(max_content, hint)`**，不是纯 `max_content`：`hint` 是作者的「舒适尺寸」
（`hug(0, X)` 的 X、文本的 1.4em 行高），丢掉它会让 hug 容器塌到内容宽。容器的 measure
主轴估计必须用**同一条规则**，否则 measure 与 place 不一致，子节点会溢出父容器再被裁
（carousel 的 `whole` 就是这样暴露的）。

### 3.4 对齐、裁切、定位、流外

```rust
pub enum Align { Start, Center, End, Stretch }
pub enum Justify { Start, Center, End, SpaceBetween, SpaceAround, SpaceEvenly }

pub struct Clip { pub x: Overflow, pub y: Overflow }
pub enum Overflow { Visible, Hidden, Scroll }   // 无 clip/auto：我们没有程序滚动 API 的区分

pub enum Position { Static, Relative, Absolute, Fixed }  // Sticky 只作包含块标记，P6 再说
pub struct Placement { pub position: Position, pub inset: Insets, pub z: i32 }

/// 子节点在容器流中的角色。
pub enum OuterFlow { Flow, Atomic, OutOfFlow }
```

`Placement` 另有两个「脱离声明处裁剪」的标记：`overlay`（锚定浮层，脱离流）与 `lift`
（保留槽位、只把绘制提到顶层——拖拽中的元素在原位不画、不裁，画在整棵树之上）。

`ContainerParams::cross_align`(0..1) / `cross_pos`(0..1) 降级为糖：
`cross_align == 1.0 → Align::Stretch`，否则按 `cross_pos` 映射 Start/Center/End（容差 0.01）。
新增 `align_items: Align`、`justify: Justify`、`align_self: Option<Align>`（子节点覆盖）。

### 3.5 容器类型

`NodeKind` 只增不改：`Column`/`Row` 保持（= `flex-direction: column/row`），
`Stack` = 单子项 overlay，`Overlay` = 层叠，`Box` = 叶子，`Text` = 行内容器。
新增 `NodeKind::Grid`（§5）、`NodeKind::Inline`（§6，P4 才启用）。

`ContainerParams` 新增：

```rust
pub wrap: bool,                       // flex-wrap
pub tracks: Option<TrackList>,        // grid 列/行模板（§5）
```

grid item 的 `grid_area` 放在**子节点 `Decl`** 上（`Decl::grid_area(row, col, row_span, col_span)`），
不做容器侧平行数组——那会打断 `Rc<Decl>` 缓存与 `slot_with`。

## 4. 算法

### 4.1 一趟测量的结构

`solve()` 保持今天的分工（`measure_node` 定尺寸、`place_node` 写 rect 与 mask），但
`measure_node` 改为：

```
measure_node(nid, Constraints) -> Size
  1. box = node.box_model; 扣 margin 与 padding 得 content 约束（§4.2）
  2. 若 size 在某轴是 Definite → 该轴 tight
  3. 按 NodeKind 分派：
       Column/Row → flex（§4.4）
       Grid       → grid（§5）
       Text       → 行内（§6）
       Stack/Box  → 单子项 / 叶子
  4. 得到的 content 尺寸经 §4.2 的 clamp 收口，加回 padding = border box
  5. 记 size，写 min_content/max_content/baseline（见 §4.3）
```

`solve()` 增加两个后处理，顺序固定：**先 in-flow 全树摆放，再解 OOF**（§4.6）。

### 4.2 不压缩不变量（本设计的核心）

```
fit = clamp(min_content, avail, max_content)     // Fit 与 Auto 的候选值
used = max(declared_min, min(tentative, declared_max))   // min 胜出；max<min → min
```

- `tentative` 来自 `Definite` / `Stretch`（= avail）/ `Fit` / 容器算法。
- **`Avail` 三态**：`Definite(f32)`、`MinContent`、`MaxContent`。intrinsic 探测用后两者。
- **压缩下限 = `min_content`，不是 `declared_min`**。文本的 `min_content` = 最长不可断片段，
  控件的 `min_content` 由它自己的测量给出（通常是图标 + padding）。窗口小于内容时，
  子节点**溢出**父容器，父容器报告溢出（§4.8），而不是把子节点压扁。
- debug 断言：按 §2.1 的 I1 双边界形式（`floor = max(min_content, declared_min)`，
  `used ∈ [floor, max(max, floor)]`），违反时带节点 id 与两个数值。
  这是「控件形状被破坏」的防回归。

`scrollable` 从「主轴不压缩」变成 `Overflow::Scroll` 的糖：滚动容器自身 `min_content = 0`
（盒模型 §3.2），子节点按 `min_content` 排布，超出部分由 §4.8 裁。

### 4.3 intrinsic 测量与缓存

- `MeasureResult` 带 `min_content` / `max_content` / `baseline`。
- 容器算 intrinsic 时递归：主轴 `max_content = Σ 子 max_content + gap`，
  `min_content = max(子 min_content) + gap`（flex 行/列）；cross 轴同理按 `align_items`。
  行内见 §6。
- 缓存：`Node.last_constraints: Option<Constraints>` + `Node.last_size`。
  `measure_node` 开头若 `!dirty && last_constraints == constraints` 直接返回 `last_size`。
  这是让事实 5 失效落地的唯一改动；`dirty` 的置位已经存在（`Tree::apply` 改节点时），
  只需在读侧兑现。**布局盒边界**：`Sizing::Stretch` 或 `Overflow::Scroll` 的节点是
  relayout boundary，其子树的 `last_constraints` 不随祖先变化失效。

### 4.4 flex：八阶段

按 flex §3 落地，具体为（`Row`/`Column` 都走这套，只换主轴）：

1. 生成 flex item（跳过 `OutOfFlow`；`Atomic` 作为不可断原子）。
2. 主轴基准：`basis` 定则用之，`Auto` 用 max_content（`Grow::Split` 用 0）。
3. 收集 intrinsic 贡献（min_content/max_content）。
4. 若 `wrap`，按 hypothetical main size 分行——**用外框尺寸分行**（flex §5）。
5. 每行解主轴：free = avail − Σ basis；`free > 0` 按 grow 分；`free < 0` 按
   **scaled shrink factor**（`shrink × basis`）收缩，收到 `min_content` 为止，
   **在冻结循环里迭代**（flex §9.7），不是一次加权。
6. 主轴 `justify` 分配剩余（auto margin 先吸收）。
7. 交叉轴：每行高度 = max 子外框；`align_items: Stretch` 且子 cross 尺寸为 Auto 时
   子撑满行（auto margin 取消 stretch）。**拉伸可增可减**：行的 cross size 是目标，
   内容比行大时同样收缩到行（CSS Flexbox §9.4 的 used outer cross size），再由
   子节点自身的 min/max 夹住；`cross_align` 是 0..1 的比例，亏空时按同一比例收缩。
8. baseline：行 cross 起点 = `max_baseline − own_baseline + own_outer_cross`（flex §7）。

**`min-width:auto`**：仅主轴、仅非滚动容器，等于 min_content；滚动容器为 0（盒模型 §2.2）。

### 4.5 grid：L0 → L1

数据：

```rust
pub struct TrackList { pub cols: Vec<Track>, pub rows: Vec<Track>, pub auto_flow: AutoFlow }
pub enum TrackBound { Len(Len), Fr(f32) }        // 轨道专用：fr 只在轨道里有意义
pub enum Track {                                  // L0
    Fixed(f32), Auto, Fr(f32),
    MinMax(TrackBound, TrackBound), MinContent, MaxContent,   // L1
    Repeat(Repeat, Box<Track>),                    // L1；Repeat::{Count, AutoFill, AutoFit}
}
```

- **L0（P3）**：固定 / auto / fr + `gap` + span + 显式 `grid_area`。
- **L1（P3b）**：轨道基尺寸取 min-content 贡献、增长上限取 max-content；`minmax` 在内容
  分摊之后夹进 `[min,max]`；`repeat(auto-fill)` 计数 `floor((avail+gap)/(track+gap))`，
  `auto-fit` 放置后塌缩空轨；`dense` 每个自动项目从原点重扫。
- **`minmax(200px, 1fr)` 是必须能写的写法**：它需要 `TrackBound::Fr`，所以轨道边界不能用
  通用 `Len`（`Len` 还被 `AxisSizes` 与 `inset` 用，那里 fr 无意义）。
- **定尺寸项目对轨道的贡献是它的尺寸**，不是 0：读 `min/max_content` 时要用
  `max(measured, 声明的确定尺寸)`，否则无模板网格里 `fixed(20)` 项目的行高为 0。
- 项目默认拉伸到单元格（`align_self/justify_self: Stretch`），可用 `Align` 覆盖。
- 隐式轨道：显式区域超出模板时补 auto 轨道。

### 4.6 定位与 OOF

- `position: Relative`：保留槽位，按 `inset` 平移绘制/命中矩形（不改兄弟）。
  现有 `Decl::offset` 就是它，保留，但**不再影响 mask**（事实 4）。
- `position: Absolute`：脱离流。包含块 = 最近 `position != Static` 祖先的 **padding box**
  （盒模型 §4.2）。两阶段：in-flow 全树摆完 → 自根向下为每个 OOF 节点解 IMCB 方程
  （盒模型 §4.3）。`width:auto` 在双 inset 确定时 stretch-fit，否则 fit-content；
  abspos 的自动最小尺寸**恒为 0**。
- `position: Fixed`：包含块固定为根节点的 border box，其余同 absolute。
- `z-index` 只对 `position != Static` 生效，同一 stacking context 内按整数排序，
  同值时退回声明顺序。模态的「最后绘制 + 焦点作用域」不变。

### 4.7 行内与文本

文本节点 = 行内格式化上下文，P4 落地。要素（架构 §5、盒模型 §1.3）：

- 行盒高度 = `max(line_height, 字形实际 ascent+descent)`，**不是 font size**。
  这直接修「文本下端被切」——今天 `Text` 的行高来自字体度量，行盒不再额外截断。
- **高度由换行结果决定，行盒 hint 不是整块的上限**：`Text` 声明的 `hug(size, 1.4em)` 里，
  1.4em 是每行行盒，一段 7 行的文本就该有 7 个行盒高。文本的高度轴不看
  `min_content.h`/`max_content.h`——那两个值对应的是**别的**换行宽（最窄与不换行），
  只在决定换行的宽度轴上才有意义。P1 早期把两者混用，导致每个段落被钉成 1 行高、
  同列兄弟节点重叠。
- **原子**：图标等以 U+FFFC（OBJECT REPLACEMENT CHARACTER）嵌进文本流，测量时按
  **块宽**（不是当前行剩余宽）算尺寸；被截断的测量值绝不作为 intrinsic 返回
  （这是 `Text::max_width` 之后第二个「测量污染」陷阱）。
- `nowrap` 保留（`tree.rs` 已加），语义 = `white-space: nowrap`：不换行，宽度 = 内容宽，
  受 `max` 夹住。
- `vertical-align` 最小集：`Baseline`（默认）/ `Top` / `Bottom` / `Middle`。
- 布局结果存 `Rc<InlineLayout>` 挂在节点上，画笔消费它，不再读 `node.wrap_width`。

**落地形态（P4a）**：`InlineLayout { lines, width, height, baseline }` 挂在 `Node.inline`，
`TextPrim.layout` 带同一份；`TextMeasure::layout` 是唯一必需方法，`measure` 由它推导，
两个数从此不可能分叉。绘制端不再自己换行——此前两端连**喂进去的宽度**都不同
（布局用 `wrap_width`、绘制用视觉宽），只是碰巧多数情况下一致。首行基线同时写进
`Node.baseline` 与 `MeasureResult.baseline`（此前恒为 0）。

**落地形态（P4b）**：`AtomBox { at, size, baseline, align }` 挂在 `TextBlock.atoms`，
`at` 必须落在 U+FFFC 上（`TextBlock::atom(..)` 构造器保证）。原子按**自身尺寸**参与断行与
行盒，U+FFFC 本身不计宽；`vertical-align` 经 `extents()` 索要基线上线空间、经 `top()` 决定
行盒内 y，三个测量器共用同一套行盒算术。结果 `InlineAtom { at, x, y, size, align }` 落在
`InlineLine.atoms`，绘制端消费它（现阶段画低透明度占位块，真节点等 P5 的定位能力）。
滚动容器主轴传给子节点的上界是 `INFINITY`，横向滚动条里的文本不再按视口宽换行（P2 遗留）。

### 4.8 裁切

- 裁切**属于容器**，取 `Clip`；`Overflow::Visible` 不裁。
- 裁切矩形 = 容器 **padding box**，处在**视觉空间**：随容器自身的位移（`offset` / relative
  inset）一起移动，但**不随滚动偏移移动**（滚动移动的是内容，不是视口）。P2 与 P5 看似相反的
  两条要求其实是这两件事，`scroll_offset_moves_content_not_the_clip_rect` 与
  `a_relative_offset_moves_the_hit_test_with_the_paint` 同时成立。
- 祖先裁切矩形逐层求交，但**只在 `Overflow != Visible` 时参与**——`visible` 的祖先不贡献
  任何裁剪。这是事实 4 的正面表述。
- `Overflow::Scroll`：容器自身 `min_content = 0`，内容按 `min_content` 排，超出的部分
  记入 `Node.scroll_extent`（新字段，供 app 画滚动条），视口尺寸 = 容器 content box。
- 文本/图标不因「字形超出 rect」被裁：裁切只发生在声明了裁切的祖先。
- **圆角不隐含裁切**：CSS 的 `border-radius` 只裁元素自己的背景与边框，不裁溢出的后代。
  要把子节点切进曲线的容器必须显式声明 `Clip::Hidden`；曲线本身由批处理取该层 mask 的
  `corners` 提供。P2 之前 `paint_node` 把「有子节点 + 圆角非零」当裁剪条件，正是窄窗下
  「卡片边缘把文本切了」的来源。
- **圆角属于未裁切的盒子**：`ClipEntry.shape` 记录圆角所属的矩形，`rect` 只是外层裁切后的
  可见区域。被父或窗口切到的圆角节点，切口保持方角，只有节点自己的角是圆的；渲染器按
  `shape` 算 SDF、按 `rect` 设 scissor。对裁切后的矩形直接套节点圆角会让四个角都变圆。
- **同矩形的方角裁剪不覆盖祖先圆角**：未声明 `Clip` 的节点若因超出 mask 而需要裁切，且
  裁剪矩形与祖先已压入的相同，就不再重复压入——它的形状由祖先决定。否则一个方角子填充
  会把圆角容器的曲线压成方角（`art` 覆盖 `carousel item` 的 28dp 圆角）。

### 4.9 失效

`Node.dirty` 已有置位路径；本设计增加读侧（§4.3）。`Tree::apply` 后：
`dirty` 子树 = 该节点及其祖先链到最近的 relayout boundary。全量 `solve()` 仍可强制
（`Tree::mark_all_dirty()`），自检与测试用它。

## 5. grid 数据落点（补充说明）

`TrackList` 放 `ContainerParams::tracks`（`Option<Box<TrackList>>`，避免 `ContainerParams`
变大）；`grid_area` 放子 `Decl`。理由：`ContainerParams` 是容器自身的属性，`grid_area`
是「我在这张网格里占哪格」，属于子节点；放容器侧需要平行数组，会与 `Rc<Decl>` 共享
和 `slot_with` 重排冲突。

## 6. 作者 API 迁移

### 6.1 尺寸

| 今天 | 新语义 | 影响 |
|---|---|---|
| `fixed(w, h)` | `Definite` + min/max | 无变化 |
| `hug(0.0, 0.0)` | `Fit`，无上界 | 无变化（57 处里的多数） |
| `hug(0.0, X)` | `Fit`，`max = X` | **行为收紧**，逐点审查 |
| `fill(0.0, X)` | `Stretch`，`max = Auto`（X 只作基线与舒适值） | 从「几乎不生效」变成真拉伸 |
| `Text::max_width(w)` | `max = Px(w)` | **修复**（今天它完全无效） |
| `Grow::No/Yes/Flex/Split` | `Flexing` | 糖，见 §3.3 |

`AxisSize` / `SizeClass` 保留为对外构造入口（126 处字面量不重写），内部立即转成
`AxisSizes`。`SizeClass { width: AxisSize, .. }` 字面量继续能编译。

### 6.2 容器

| 今天 | 新 |
|---|---|
| `params.padding` | `node.box_model.padding`（3 处读取点） |
| `cross_align: 0.0..1.0` | `align_items: Align`（糖） |
| `cross_pos: 0.0..1.0` | `justify`（糖） |
| `scrollable: true` | `clip.main = Scroll`（糖） |
| `Decl::offset(x, y)` | `Position::Relative` + `inset`（糖，语义不变） |
| `NodeKind::Stack` 的单子项 | 不变 |

**`distribute()` 删除**，由 `flex::solve_line` 取代；`can_shrink()` 删除（只有 1 个调用点，
在新模型里没有对应概念）。

## 7. 放弃清单

| 放弃 | 理由 |
|---|---|
| float / clear | 唯一独有能力是图文环绕，我们无此 API；Flutter/Qt/GTK 无，GPUI 未接 taffy 的 |
| margin collapsing | 只服务块级文档流；flex/grid/abspos 不折叠；成本是 BFC/clearance/line box 三套状态 |
| sticky 的布局期偏移 | 滚动偏移本就是 app 每帧传入的视图值；taffy 自己也不做偏移 |
| `overflow: clip` / `scrollbar-gutter` | 无程序滚动 API；滚动条是 overlay，不占位 |
| 一轴 visible 一轴 scroll 的 CSS 传播规则 | 两轴独立更直观；有意偏离，写进类型文档 |
| RTL / 书写模式 / 逻辑属性 | 全仓库 LTR、水平书写 |
| `aspect-ratio` | 图片 fit 已由 `ImageFit` 覆盖 |
| 负 z-index / 嵌套 stacking context | 需要时用显式「层」字段补 |
| 百分比高度（父高不定时） | 解析为 Auto，不报错；原因写进 `Len::Pct` 文档 |
| `calc()` / `min-content` 作者关键字 | 内部保留 intrinsic 通道，作者 API 先不暴露 |

## 8. 阶段计划

每阶段独立可交付、可验收；后一阶段依赖前一阶段的类型。**每阶段先写会红的断言，再实现。**

### P0 — 契约（半天）

改 `Constraints { min, max }`；`SizeClass`/`AxisSize` 转 `AxisSizes` 但保留构造器；
`BoxModel` 上 `Node`（不加行为，只接线）；`measure_node` 加 I2 自洽断言与 I1 下界断言。
**验收**：全量测试绿；`hug`/`fill` 的构造结果不再字节相同（新单测钉住）；
`cargo test -p ui-widgets-md3` 的 4 条曾经变红的用例保持绿（它们本身就是合法溢出的样本）。

### P1 — 不压缩 + clamp（1–2 天）

删 `distribute()`，`flex::solve_line` 取代；压缩下限 = `min_content`；grow 路径查 `max`；
`Preferred` 迁移清单逐点审查（§6.1）。**验收**（先红）：
`a_child_never_shrinks_below_its_min_content`、`grow_never_exceeds_the_declared_max`。
gallery 缩小窗口：控件保持形状、溢出到窗口外（截图对比）。

### P2 — 裁切归属（1 天）

`Clip` 上容器；`mask` 只在 `Overflow != Visible` 时求交；**滚动偏移**不再平移 mask
（P5 补上「容器自身的位移要平移 mask」，两者不是同一件事，见 §4.8）。
**验收**（先红）：`a_visible_container_does_not_clip_its_child`、
`scroll_offset_moves_content_not_the_clip_rect`；文本下端不再被切（截图）。

### P3 — grid L0（2 天）

`NodeKind::Grid` + `TrackList` + `grid_area`；auto 轨道 = 最大测量尺寸；截图页。
**验收**：`grid_tracks_sum_to_the_content_box`、`a_spanning_item_sizes_its_auto_tracks`。
P3b（L1）：`minmax`/`repeat`/`auto-flow dense`。

### P4 — 行内（3–4 天）

`InlineLayout`、U+FFFC 原子、行盒高度、`vertical-align` 最小集、`Text::max_width` 生效。
**验收**：`a_line_box_is_at_least_as_tall_as_its_glyphs`、`max_width_caps_the_measured_width`。

### P5 — 定位（2 天）

`Placement`、两阶段 OOF、包含块链、`z`。**验收**：`an_absolute_child_uses_the_padding_box`、
`fixed_is_relative_to_the_root`。

### P6 — 增量与收尾（1–2 天）

`dirty` 读侧 + `last_constraints` 缓存；删除孤儿字段（`soft_max`、`can_shrink`、
`cross_align/cross_pos` 的比例存储若已无读者）；性能：gallery 帧剖析对比 P0 前后。

总计约 **11–14 个工作日**，每阶段结束跑全量测试 + gallery 自检，并同步
`docs/lessons/design-errors.md`（新增的取舍与踩坑）。

## 9. 测试策略

按 AGENTS §7：每个不变量一条测试，名字即规格，只喂确定输入。

- **契约**：I1/I2 各一条，用最小树。
- **差分**：`incremental == full`（打完 `dirty` 后局部重算与全量重算 rect 逐节点相等），
  带种子，P6 交付。
- **属性**：`max_content >= min_content`、`used ∈ [min_content, ∞)`，用几条真实控件树。
- **视觉**：窗口缩小 / 文本长行 / grid 三种情形进 `examples/`，靠截图判断，不写单测。

## 10. 风险

| 风险 | 应对 |
|---|---|
| P1 改变既有控件观感（`hug(0,X)` 收紧） | 逐点审查清单 + gallery 全页截图对比；不确定的点先保持旧语义 |
| `min_content` 算不准导致误溢出 | 先只对 Text 与已声明尺寸的控件精确，容器用保守估计；断言只在 debug |
| 增量缓存引入陈旧 rect | P6 之前不启用缓存；启用后差分测试守门 |
| API 迁移量大 | `AxisSize`/`SizeClass`/`Grow` 保留为糖，126 处字面量不重写 |
