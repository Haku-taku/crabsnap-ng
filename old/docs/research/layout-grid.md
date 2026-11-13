# CSS Grid 调研：完整算法与可裁剪子集

调研日期 2026-10-30。只读，未改任何代码。

引用前缀：`T = /home/ollie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/taffy-0.15.0/src/compute/grid`，
`U = /home/ollie/git/crabsnap-ng/ui/crates`。**章节号**：任务里的 §12.3–12.8 / §8 是 CSS Grid
**Level 2** 的编号；`w3.org/TR/css-grid-1/`（现行 WD）同一批内容是 §11.3–11.8 / §8，taffy 源码注释
用的就是 11.x。两套号指同一算法，本文写 12.x 并注 taffy 行号。

## 1. 为什么现有引擎放不下网格

`U/ui-core/src/layout_engine.rs:223` 的 `solve` 是**单趟**：`measure_node` 自下而上求需求 →
`place_node` 自上而下定位，注释写明 "one pass each, no retry"。交叉轴取 `widest/tallest`
（`layout_engine.rs:358-365`），主轴用 `distribute()`（`:89`）。这条链上没有任何一步让**兄弟节点互相耦合尺寸**，
而网格的定义就是"同一轨的所有条目共享一个尺寸"。三个缺口都不是加一个 `NodeKind::Grid` 能补的：

| 缺口 | 现状 | 后果 |
|---|---|---|
| 无 intrinsic sizing | `TextMeasure::measure(block, max_width)`（`layout_engine.rs:19`）只有"给定宽求高"；`min_content\|max_content\|intrinsic` 在 `ui-core`/`ui-text` **零命中** | `auto`/`min-content`/`max-content`/`fit-content()` 做不出来 |
| 约束只有上界 | `Constraints { max, soft_max }`（`layout.rs:126`） | 求解需要的 `AvailableSpace::{MinContent,MaxContent,Definite}` 三态表达不出来 |
| 无"只请求一轴尺寸" | taffy 靠 `RunMode::ComputeSize` + `RequestedAxis` 短路（`T/mod.rs:264-272`、`:442`） | 跨轴循环（§3.6）做不了 |

## 2. 轨道定义：解析与数据形状

`<track-size>` 六类（§7.2.1）按三种角色（fixed / intrinsic / flexible，§12.2）归组：

| 写法 | min 位 | max 位 | 要点 |
|---|---|---|---|
| `<length>` / `<percentage>` | 定值 | 定值 | 唯一不看内容的一类；百分比父尺寸不定时**当 auto** |
| `auto` | 最大**最小贡献**（≈min-content，见 §5 末条） | 最大 max-content 贡献 | 作 max 时 ≡ `max-content` **+ 允许被 align-content 拉伸** |
| `min-content` / `max-content` | 对应贡献 | 同 | 作 min 时条目可溢出轨 |
| `fit-content(L)` | `auto` min | `max(auto min, min(L, max-content))` | 只出现在 max 位 |
| `<flex>` | `minmax` 外时隐含 `minmax(auto, Nfr)` | 定 flex factor | **作 min 非法**；`Σfactor < 1` 时只占该比例 |

`minmax(min,max)`：max < min 时 max 被 min 顶住（"essentially yielding minmax(min, min)"）。
taffy 用 `CompactLength` 标签位把两类压成 8 字节 newtype：`MaxTrackSizingFunction(CompactLength)`
`T/../style/grid.rs:732`、`MinTrackSizingFunction:1181`（`style/mod.rs:1798` 断言 size==8）。
**建议照抄"min/max 两套"的形状而不是单个 `enum TrackSize`**：算法里两个半边被分开问
（`definite_value` / `is_intrinsic` / `is_fr` / `is_fit_content` / `behaves_as_auto`），
单枚举让每个判断变成三向 match。

- `repeat(n, …)`：定次数只是展开，零成本。
- `repeat(auto-fill|auto-fit, …)`：轨道**数**必须在放置之前算出（`T/explicit_grid.rs:27`，
  策略枚举 `:16`：有定值/最大值→不溢出的最大次数；只有定值最小→越过它的最小次数；否则 1）。
  每条轨按"max 定值则用 max，否则用 min，两者都定值时 max 被 min 顶住"折算。硬约束：
  **全表只能有一个 auto-repeat，且模板其余部分必须全含定值分量**，否则整个模板作废（`:82-91`）。
  `auto-fit` 与 `auto-fill` 的唯一差别在放置**之后**：空轨塌成 0 且两侧 gutter 一起塌
  （`explicit_grid.rs:315`；auto-repeat 位于表尾时的回退塌缩 `:339-347`，首末 gutter `:365`）。
- **命名线/命名区域：不实现 CSS 语法，但实现它的能力。** taffy 为这块写了 `T/types/named.rs`
  （998 行，占 grid 模块 13%：`NamedLineResolver` + `LinePositions` + `GridAreasMap`），
  只为把字符串映回线号。本仓库已有同一语义的更强机制：`SlotName`（`U/ui-core/src/slot.rs:12`）
  + `Decl::slot`/`slot_with`（`tree.rs:856/870`，按 id **替换**而非追加）。区域应表达成
  "命名槽 → 轨道区间"，由控件声明槽，而不是让 app 写 `"a a b"` 字符串表。

## 3. 轨道尺寸求解（12.3–12.8）

### 3.1 骨架

`T/track_sizing.rs:257` `track_sizing_algorithm` 一个函数对应规范一整节：

```
initialize_track_sizes        :275 (定义 :415)   // 12.4
resolve_fit_content_limit     :280               // fit-content 的 limit 一次定死
resolve_item_baselines        :285 (定义 :452)   // 12.5 step 1，仅 inline 轴
— 快路径：所有轨 min 定值且 base==growth_limit → return（:293-305）
resolve_intrinsic_track_sizes :321 (定义 :548)   // 12.5
maximise_tracks               :334 (定义 :1220)  // 12.6
expand_flexible_tracks        :352 (定义 :1245)  // 12.7
stretch_auto_tracks           :368 (定义 :1415)  // 12.8，仅该轴 align-content: stretch
```

### 3.2 12.4 初始化

`GridTrack { base_size, growth_limit, … }`（`T/types/grid_track.rs:21`）。min 定值 → `base_size=定值`，
intrinsic → `0`；max 定值 → `growth_limit=定值`，intrinsic **或 flexible** → `+∞`；
最后 `growth_limit < base_size` 则抬平（`:415-450`）。**gutter 是同一 Vec 里的定值空轨道**
（`explicit_grid.rs:250` 先 push gutter；`grid_track.rs:14` `GridTrackKind::Gutter`），
轨道与 gutter 交替，故 `i%2==0` 是 gutter（`alignment.rs:60`）、条目区间要 `start+1..end`
（`grid_item.rs:202` `track_range_excluding_lines`）。

### 3.3 12.5 intrinsic 解析（最难的一段）

三种贡献（`T/types/grid_item.rs`，都靠 `tree.measure_child_size` 递归测子项）：
`min_content_contribution:537`、`max_content_contribution:590`、`minimum_contribution:702`。
后者是自动最小尺寸（§6.6）：**条目不是滚动容器、且跨的轨里至少一个 min 是 `auto`、且（只跨一轨
或跨的轨都不是 fr）→ 用内容推出的最小值，否则是 0**（`:744-790`，把规范四句话写成六个
`.or_else()`）。不变量 `minimum ≤ min-content ≤ max-content`。

迭代顺序按条目跨度升序（`:571-585`：跨度 1 走优化分支；跨 fr 轨的**全部**混成一批发到最后），
每批内六个相位（`:715-993`）：① intrinsic minimums→base；② min/max-content 的 min→base；
③ max-content 约束下 `auto`/max-content 的 min→base（用 limited max-content）；④ max-content 的
min→base；⑤ intrinsic maximums→growth_limit；⑥ max-content 的 max→growth_limit。
收尾把仍是 `+∞` 的 growth_limit 落成 base_size（`:995`）——**这是 12.6 不误伤 fr 轨的前提**。

跨多轨分配：`distribute_item_space_to_base_size`（`:1013`，inner `:1064`）/
`..._to_growth_limit`（`:1156`），共享内核 `distribute_space_up_to_limits`（`:1447`）：
`space = max(0, 贡献 − Σ跨过的轨当前尺寸)`，按权重分、撞 limit 冻结、剩余按 flex factor 重分，
循环到剩余 < `THRESHOLD`。"超出 limit 之后再给谁"的三段规则（`:1105-1140`）是规范原文直译。
**计划增量 `base_size_planned_increase`/`growth_limit_planned_increase` 必须存在**
（`:386`/`:396` flush）：同批条目互不可见，就地累加会让结果依赖条目顺序。

### 3.4 12.6 / 12.7 / 12.8

- **12.6 Maximize**：正 free space 平均分给所有轨，撞 growth_limit 冻结。max-content 约束下 free
  space 视为 `+∞`（直接 `base=growth_limit`），min-content 下视为 0。
- **12.7 Expand flexible**：先算 used flex fraction。定值空间 → `find_size_of_fr(所有轨, available)`；
  min-content 约束 → 0；**空间不定** → 取「各 fr 轨的 `base/flex_factor`（factor≤1 时取 base）」与
  「各跨 fr 条目的 `find_size_of_fr(它跨的轨, 它的 max-content)`」的最大，再按容器 min/max-size 兜回
  （`:1290-1330`）。`find_size_of_fr`（`:1350`）是**带重启的循环**：`flex_factor_sum` 按 `max(Σ,1)` 算；
  若 `factor × hypothetical_fr < 某轨 base_size`，把那些轨当 inflexible **从头重算**；
  迭代上限 `tracks.len()+1`（`:1367`，NaN 会破坏"每轮至少冻结一轨"的收敛证明，taffy 专门加了界）。
  最后 `base_size = max(base_size, flex_factor * fr_size)`。
- **12.8 Stretch auto**：只扩 max 为 `auto` 的轨，平分**容器自己给出的**剩余空间。注意 `:340-357`：
  `inner_node_size` 为 `None` 时要把 available 映射成 `MaxContent`，否则 fr 会去抢外部空间。

### 3.5 taffy 标注的 QUIRK / 未支持

- `:622-660`、`:720-770`：规范说 min/max-content 约束下用 limited min-content 代替 minimum
  contribution，但**浏览器只在条目不是滚动容器时这么做**（`overflow:hidden` 也算滚动容器），
  taffy 跟浏览器。`[未证实]`——这是 taffy 注释的断言，我没有第二来源。
- `:744-762`、`:868-885`：跨 fr 条目按 `min(Σfr, 1)` 缩放贡献；规范要按比率混合"flex 比例"与
  "平均"，Chrome 只用受影响轨的 flex 和（`:1023-1030` 注释）。`:888-899`：`max-content` 的 min
  优先于 `auto` 的 min 拿空间——规范未写（写的是平均分配），Chrome/Firefox 都这么做。
- baseline：条目自对齐的 `baseline` 被当 `start`（`alignment.rs:482` TODO）；块轴 baseline 未支持
  （`mod.rs:465`、`:632`）。

### 3.6 跨轴循环（12.1）

列先解、行后解；测列时**假设**"max 定值的行取该值、其余行无限"，并把另一轴 `align-content` 造成的
gutter 增量预先算出挂到每条 gutter 轨上（`track_sizing.rs:166` `compute_alignment_gutter_adjustment`，
写入 `:310-319`）。容器尺寸确定后**最多各重跑一次**（`mod.rs:500-635`），触发条件很具体：
有百分比轨且父尺寸不定，或任何跨 intrinsic 轨条目的 min-content 贡献变了。

## 4. 放置算法（§8.5）

`T/placement.rs:35` `place_grid_items`，四步与规范一一对应：

1. 两轴都定 → `place_definite_grid_item`（`:142`）。
2. 次轴定、主轴不定 → `place_definite_secondary_axis_item`（`:155`）：从主轴起点或游标逐轨试探，
   `line_area_collision_jump`（`types/cell_occupancy.rs:301`）返回"跳过冲突后的下一位置"。
3. 确定隐式列数：taffy 不做——用可扩张的 `CellOccupancyMatrix` + `compute_grid_size_estimate`
   （`implicit_grid.rs:18`）预先估出三区轨道数 `TrackCounts{negative_implicit, explicit,
   positive_implicit}`。
4. 其余全 auto → `place_indefinitely_positioned_item`（`:192`）：主轴走到尾就复位、次轴 +1。

**游标不回退 vs dense 回退**，代码只差一处（`placement.rs:133-136`）：
`grid_position = if dense { grid_start_position } else { (primary_span.end, secondary_span.start) }`
——dense 每个条目都回网格起点找洞，sparse 单调前进、空洞永久作废。

`sparse` 下步骤 2 用的游标是**按次轴每条轨分别记的**（`cell_occupancy.rs:368`
`auto_placement_cursor`），语义是"跳过本行已被前面条目占掉的位置"，与步骤 4 的全局游标是两回事，
极易混。§8.3.1 的冲突归一（起线比止线更靠尾→交换；相等→丢止线；两个 span→留起线那个）逐条标在
`implicit_grid.rs:90` 注释 A/B/C/D；越界条目夹回 `[MIN_OZ_LINE, MAX_OZ_LINE]`（`placement.rs:304`，
`MAX_GRID_TRACKS = 10_000`，`types/coordinates.rs:13`）。**坐标建议**：内部一律用 taffy 的 OriginZero
（显式网格左/上边界为线 0，向左上为负，`types/coordinates.rs:60-68`，`i16`），CSS 的 1-based 含负线号
只在解析边界转换（`:49`）；L0 也用它，L1 的 auto-flow 与负隐式轨会直接复用。

## 5. 对齐、gap、条目尺寸、绝对定位

- 容器 `justify-/align-content` → `T/alignment.rs:23` `align_tracks`，复用与 flex 共用的
  `compute/common/alignment.rs:70` `compute_alignment_offset` + `:28` `apply_alignment_fallback`
  （单条目或溢出时 `stretch`→`flex-start`、`space-*`→`center` 并隐式 `safe`）。
- 条目 `justify-/align-self` 回落容器 `justify-/align-items`，再回落 `normal`；`normal` 在条目上
  等价 stretch，**但条目有定尺寸/尺寸关键字/是替换元素时退为 start**（`alignment.rs:167-190`）。
  auto margin 优先于对齐并吃掉全部 free space（`align_item_within_area:454-475`）。`place-*` 只是
  两值简写，纯解析层。
- `gap`：taffy 把 gutter 当轨道（§3.2），于是 `align-content: space-between` 会**改变跨条目可用空间**，
  才需要 §3.6 的 gutter 调整量。L0 不把 gap 当轨道，这块自动不存在——这是不建议 L0 照抄
  gutter-as-track 的主要理由。
- 条目侧输入全在 `T/types/grid_item.rs:25-97`（size/min/max/aspect_ratio/padding/border/margin/
  overflow/box_sizing）。两个反直觉点：① **两个轴的 margin 百分比都按网格区宽度解析**
  （`alignment.rs:199-204` 注释 "This is not a bug"）；② `overflow: auto/scroll/hidden` 让自动最小
  尺寸变 0（`grid_item.rs:749`），这正是"网格子项把容器撑爆"最常见的解药。
- 绝对定位：`position:absolute` 子项**不参与放置、不产生隐式轨**（`mod.rs:334-338` 跳过），但
  containing block 可以是网格区：`resolve_static_position_grid_area`（`mod.rs:955`）经
  `resolve_absolute_grid_area`（`:1185`）把线号变矩形，规则是"起线取其后那条轨的**起始边**、
  止线取其前那条轨的**结束边**"（`:950-953` + `:1133`），线号越界当 `auto`（`:1098-1120`）。
  我们的 `NodeKind::Overlay`（`tree.rs:27`）现在只会落在内容原点；把它补成"相对网格区"是最小形态。

## 6. 轻量网格 vs CSS Grid

| 能力 | Flutter `GridView` | Qt `QGridLayout` | CSS Grid |
|---|---|---|---|
| 模型 | Sliver 的**单元重复**，不是共享轨道 | 行列表格 | 显式+隐式轨道网络 |
| 轨道尺寸 | `SliverGridDelegateWithFixedCrossAxisCount{crossAxisCount, childAspectRatio, mainAxisExtent}`，全是"算一个单元矩形" | `setColumnStretch(i,int)` + `setColumnMinimumWidth(i,px)`（`/usr/include/qt6/QtWidgets/qgridlayout.h:43-49`）；三档 min/preferred/max 来自 `QSizePolicy`，**无 min/max-content 概念** | 六类函数 + §12.5 六相位 |
| 跨格 | 无 | 有 `rowSpan/columnSpan`（`:67`）；求解在私有 `qgridlayoutengine_p.h`：`QGridLayoutBox{min,preferred,max}` + `MultiCellMap` + `distributeMultiCells`/`stealBox` | 有，且区分三种贡献 |
| auto 放置/dense · 塌缩/auto-fill | 无 | `setDefaultPositioning` 只是顺序填充；两者均无 | §8.5 全套；有 |

Qt 的 `QGridLayout` 就是"L0 那一档"的工业参照：行列 + 单元格 + stretch 整数 + 最小像素 +
per-cell alignment，恰好等于我们最可能需要的第一档。Flutter 干脆没有二维布局，`GridView` 是滚动
列表的排布策略（`layout-inventory.md` A-1 那行的分类成立）。

## 7. 与 flex 重叠：Grid 独有、且仓库内真实存在的需求

`distribute()` 已把单轴 flex 做到位。Grid 只在"多个兄弟共享一条不可分割的轴"或"两轴同时耦合"时
才有增量：

1. **画廊的三栏页**（`U/ui-gallery/examples/gallery.rs:4499` `shell`，各页 `bodyrow`）现在是
   `Row(nav, Row(col_md3, col_fluent, col_glass))`，列宽靠 `COL_W = 380`（`:36`）硬编码、行高靠
   `COL_TALL_H = 800`（`:45`）常量对齐。想要"三种风格的同名控件**逐行对齐**"就必须让每页的行进入
   同一套行轨。flex 不行：三个独立 `Column` 的行高彼此不可见。
2. **日期选择器 7×6**（`U/ui-widgets-md3/src/date_picker.rs:977` `grid_decl`）现在是 `Column of Row`，
   每格 `SizeClass::fixed(cell, cell)`（`:780`）。真实需求是"列宽由最宽单元格决定且各列等宽、月/年
   切换时变 3×4、跨月填充格不改变列宽"。flex 不行：交叉轴 `widest` 只作用在**单行**内，
   行与行的列宽会各自漂移。
3. **仪表盘卡片墙**：`repeat(auto-fill, minmax(200px, 1fr))` + 一张 `span 2 × span 2` 的大卡要与
   右侧小卡**底边对齐**。flex 不行：换行点由内容宽度决定而非列数，大卡也够不到两行的深度。

反向判据（继续用 flex）：条目数运行时未知且单轴、无对齐耦合（`chip.rs`、`button_group.rs`）、
纯叠放（`Stack`/`Overlay`）。

## 8. 三档实现建议

| | 能表达 | 代价 | 测试怎么写 |
|---|---|---|---|
| **L0** | 显式 `cols/rows`（≤8）；轨道 = `Fixed(f32) \| Auto \| Fr(f32)`；`gap(row,col)`；条目 = 显式 OriginZero 线区间 + `span`；自对齐 `start/center/end/stretch`；容器 `align-content` 到 `start/center/end/stretch`（不含 `space-*`）；隐式轨 = `Auto` | `Auto` 必须**降级定义**为"跨该轨条目**测得尺寸**的最大值"（复用 `measure_node` 返回值，即现 Row/Column 的交叉轴语义），否则整档做不出来。新增 `grid.rs`（求解+放置）约 300–450 行。**前提**：容器主轴尺寸已知（`Constraints::tight` 或父给），fr 才有解 | 单元层、自建世界，全用 `NodeKind::Box + SizeClass::fixed`，零文本依赖。钉住：① `1fr 1fr` 在 100 宽 + gap 20 下得 `(40,40)`，gap **只算一次**；② 跨两轨条目不把自身尺寸重复计入两次剩余；③ `auto` 轨 == 跨它条目的最大测得尺寸；④ sparse 游标不回退（4 项、第 3 项 `span 2`，第 4 项落下一行开头而非洞）。每条都要"先看它红" |
| **L1** | + `minmax(min,max)`、`min-content`/`max-content`/`fit-content(L)`、`repeat(n)` 与 `repeat(auto-fill/auto-fit,…)`（含空轨塌缩）、`grid-auto-flow: row/column/dense`、`space-between/around/evenly` | 先补三块地基：(a) intrinsic 测量通道（`TextMeasure` 要能报 min/max-content：`MonospaceMeasure` 的 `text_w` 就是 max-content，min-content 需最长不可断词）；(b) `AvailableSpace` 三态 + 只求一轴的 `RunMode`；(c) §3.6 跨轴循环与"最多重跑一次"。估 1200–1800 行（taffy 对应：`track_sizing.rs` 1507 + `placement.rs` 638 + `explicit_grid.rs` 833） | 属性/差分最有价值："增量==全量"式不变量、交换兄弟顺序不改结果。以及**与 taffy 对拍**：`taffy` 进 `ui-core` 的 **dev-dependencies** 不触发 `platform_boundary.rs:134`（该检查只读 `[dependencies]`，且 taffy 非平台 crate），表驱动跑同一批轨道定义比尺寸。**这条引入需 Lead 决策**。视觉/人工留 `examples/` |
| **L2** | + 命名线与 `grid-template-areas`、baseline 组、百分比轨的二次解析、`position:absolute` 按网格区定位、RTL、`overflow` 与自动最小尺寸的交互 | 参考量级：taffy 整个 grid 目录 7771 行、52 个 `#[test]`，其中命名那块单独 998 行 | 只有 baseline、auto-fit 塌缩、跨轴重跑三类值得写单测；其余按 `AGENTS.md` §7.2 属"为覆盖率而写" |

**落点建议：L0 先行；L1 内优先 `minmax` + `auto-flow dense`**（`fit-content()` 顺带就有——它就是
"max 上加一个 limit"）。命名区域按 §2：用 `SlotName` 承接，不实现 CSS 语法。
**接线**：轨道表放 `ContainerParams`（`tree.rs:186`，与 `gap/padding/cross_align` 同层，
`PartialEq` 自动进 `Decl` 比较）；条目区间放**子项自己的 `Decl`/`Node`**（新增
`grid_area: Option<GridArea>`，`GridArea { col: Line<i16>, row: Line<i16> }`），**不要**放容器的
平行数组——那会让 `Rc<Decl>` 子树缓存（`docs/notes/decl-cache.md`）与 `slot_with` 的按 id 替换失效。

## 9. 最容易做错的三处（按踩坑概率排序）

1. **`growth_limit` 的无穷↔有限转换与 `infinitely_growable`。** 12.5 第 5 相位刚把某轨 growth_limit
   从 `+∞` 落成有限值时，第 6 相位必须仍把它当无穷（`track_sizing.rs:396-414`、`:970`/`:991` 的
   `flush_planned_growth_limit_increases(.., true/false)`）；规范里 Peter Salas 的两轨例子说明搞错的
   表征是"第一条轨被跨轨条目连带撑大"。以及收尾 `:995`"仍是无穷就置为 base_size"——漏掉它 12.6
   会把 fr 轨也 maximise 一遍。
2. **fr 的两处钳制。** `Σflex_factor < 1` 要按 1 算（`1fr` 不填满容器，§7.2.1 的 Note）；
   `find_size_of_fr` 的**重启语义**——`factor × hypothetical_fr < base_size` 的轨要当 inflexible
   重算而不是就地夹（`:1350-1410`），`minmax(100px, 1fr)` 在窄容器里的结果全由这步决定。
   另外 12.6/12.7/12.8 的**空间来源**不同：后两步只能用容器自己给的空间（`:340-357` 的
   `axis_available_space_for_expansion` 映射），用错就出现 fr 抢 intrinsic 的地。
3. **跨轴循环与 planned increase 的批处理。** (a) 测列时必须按"definite max 的行取该值、其余无限"
   假设行尺寸并计入另一轴 align-content 的 gutter 增量（§3.6），直接用"当前行尺寸"会得到依赖求解
   顺序的结果；(b) 同批条目必须写 `*_planned_increase` 再统一 flush（`:386`/`:396`），就地累加会让
   "交换声明顺序"改变布局；(c) `distribute_space_up_to_limits` 的 `THRESHOLD` 有两套（`:1094` 的
   `1e-6` 与 `:1457` 的 `0.01`），统一成一个会引入浮点死循环或亚像素抖动。
