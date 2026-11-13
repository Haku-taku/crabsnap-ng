# CSS Flexbox 完整算法（布局引擎重写调研）

> 只读调研，未改代码。证据基线：taffy 0.15.0（`/home/ollie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/taffy-0.15.0/`）。
> 下文 `F:` = `src/compute/flexbox.rs`，`A:` = `src/compute/common/alignment.rs`。规范条款以 taffy 注释中逐字引用的
> W3C Flexbox L1 §9 原文为据，并用 WebSearch 核对了 §4.5、§5.4、§8.5 的结论；未能直连 w3.org 原文（本机网络策略拒绝）。
>
> 现有引擎的 Row/Column 是 flex 的**单行、单轴、无 min/max 收缩、无换行/基线**退化情形。完整 flex 不是把
> `distribute()` 加几个参数，而是**八个阶段、阶段间有反馈**的算法。

## 0. taffy 的能力边界（决定我们能借鉴到哪一步）

- 已实现：§9.1–9.7 全流程、`wrap`/`wrap-reverse`、`justify-content` 全部关键字、`align-items/self/content`、
  baseline、auto margin、gap、`box-sizing`、百分比、aspect-ratio、`position:relative`、绝对定位候选项。
- **未实现**：CSS `order` 属性（`F:754` 的 `order` 只是树索引，用于文档序，不排序，见 `F:2818`）；inline-flex /
  行内格式化上下文（taffy 只做块级布局，style 里没有 inline 概念）；`visibility: collapse`（`F:466` TODO）；
  垂直书写模式的 baseline（`F:1985` 明确只支持 row 的基线）；`flex-wrap: balance`/`flex-line-count` 是 L2 草案（可选）。
- **有意偏离规范**：flex basis 被地板到 padding+border（`F:1104-1105`），规范说内容盒不地板 0；taffy 注释说明这与
  Chrome/Firefox 一致。我们应跟随浏览器行为，而不是跟随规范字面。
- GPUI 不自己实现 flex：`crates/gpui/src/style.rs:1300` 把 `FlexDirection` 直接转成 `taffy::style::FlexDirection`；
  其 `align_items/justify_content/flex_wrap/flex_grow/flex_shrink` 字段（`style.rs:255-275`）是 taffy 的薄封装。
- Flutter `RenderFlex` 源码不在本机 sparse checkout（`src/` 只有 `animation/ material/ painting/`），**无法逐行核对**；
  只从 `docs/research/layout-inventory.md` 得知 Row/Column 暴露 `mainAxisAlignment/crossAxisAlignment/mainAxisSize`，
  且 `Wrap` 是独立控件。其“只有 Flexible/Expanded、无 shrink 整套算法”的说法标记 **[未证实]**。

## 1. 轴、常量与 definite 判定（所有阶段的输入）

| 内容 | taffy |
|---|---|
| direction / wrap / wrap-reverse | `F:587-592` |
| `normal` → flex 里按 `stretch` / `flex-start` 解析 | `F:605-618` |
| gap 解析（百分比对容器内尺寸） | `F:648` |
| `content_box_inset = padding + border + scrollbar gutter` | `F:633-639` |
| inner size = known outer − inset；definiteness 标志 | `F:641-647` |
| 容器主轴尺寸已知时直接采用 | `F:388-410` |

不变量：① 后续每个量都要明确是 main 还是 cross；row 的 main=width、cross=height。② padding/border 不参与 flex 分配，
gap 参与（主轴项之间、交叉轴行之间）。③ **容器主轴是否 definite** 同时决定三件事：百分比 basis 能否解析、能否换行、
intrinsic 尺寸怎么算。④ 内容盒不得为负；outer = inner + padding + border。

## 2. 基线尺寸：flex-basis、hypothetical main size、自动最小尺寸（§9.2/§4.5）

taffy `F:885-1161`。逐项：

| 规范分支 | 实现 | 不变量 |
|---|---|---|
| A. basis 是 definite 长度/百分比 | `F:967-970`；百分比仅在容器 main definite 时解析（`F:965-966`） | 百分比 basis 对**容器内主轴尺寸**，不是自身包含块 |
| basis:auto | `F:1011-1013`：主轴 `width/height` definite 就用它 | 只有 `auto` 才看 width；非 auto 时 width 被忽略 |
| basis:content / auto 且 width 不 definite | `F:1064-1094`：用 max-content 约束测量；交叉轴可用空间先减 margin、再按 min/max clamp（`F:909-924`） | 测量 basis 时**不要把容器自己的 definite main size 当约束**，否则内容永远缩成容器宽 |
| B. aspect-ratio + definite cross | `F:1054-1062` | 由 cross 反推 main；我们无内在比例节点可省 |
| flex basis 地板 padding+border | `F:1104-1105` | 浏览器行为；否则 `height:0; padding:20` 会出负内容盒 |
| hypothetical inner = clamp(basis, min, max) | `F:1150-1159` | **basis 本身不被 min/max clamp**；hypothetical 才被 clamp |
| 自动最小尺寸（min-width:auto） | `F:1121-1145` | 未显式给 min 时 = **min-content 测量值**，再被自身 main size、transferred max 夹住、被 p+b 地板；overflow 主轴非 visible 时为 0（`F:1122`） |

必须记住的四条：
1. `flex-basis:auto` 的“auto”是“先看 width，没有就看内容 max-content”，不是“0”。
2. flex base size **不受 min/max 约束**；§9.7 用它做初始分配，最后才 clamp。
3. 自动最小尺寸只作用于**主轴**、只在 overflow 主轴 visible 时存在；它是 min-content，不是 0，也不是声明的 min。
   这条漏掉的表现是按钮/文字被压成一条线；错误地作用于交叉轴的表现是高度莫名被撑住。
4. basis 与 width 同时存在时，basis 赢；`box-sizing` 只改变“声明值加不加 p+b”，不改变上述优先级。

## 3. 行构成与容器主轴尺寸（§9.3）

taffy `collect_flex_lines` `F:1181-1264`，容器 main `F:1369-1689`。

- `nowrap` 或容器 main 不 definite → 全部进一行（`F:1189-1192`）。
- definite main：贪心累加 **hypothetical outer main size + gap**（首项不加 gap）；`line_length > available && idx != 0`
  时在该项前断行（`F:1239-1259`）。第一项无论多大都单独成行；恰好填满后，后续零尺寸项仍留在本行（条件用 `>`）。
- min-content 约束 → 每项一行（`F:1224-1232`）；max-content → 一行（`F:1217-1221`）；column 且 main 不 definite 但
  available definite → 按 MaxContent 处理（`F:1205-1210`）。
- 容器 main 未知时：available definite → 最长行的 `max(basis, style min)+margin`（多行再与 available 取大，`F:1378-1438`）；
  min-content+wrap → 最长行（`F:1440-1460`）；min/max-content → 逐项算 intrinsic contribution（grow/shrink 为 0 时
  用 basis 夹、否则测量），取行最大（`F:1461-1676`）；最后用容器 min/max clamp（`F:1680-1687`）。
- 不变量：行构成用 **hypothetical outer**（含 margin），不是 basis；行是**稠密连续切片**，不能重排；换行只在主轴发生；
  容器自动主轴尺寸不是“所有项宽度之和”，flex factor 会改变每项贡献。

## 4. §9.7 Resolving Flexible Lengths（逐行，`F:1696-1883`）

1. **选 grow 还是 shrink**（`F:1704-1709`）：`gap + Σ outer hypothetical main` 与容器 inner main 比较；小于用 grow，
   大于用 shrink，相等则全部冻结直接返回（`F:1733-1735`）。
2. **初次冻结**（`F:1718-1731`）：flex factor 双 0；用 grow 且 `basis > hypothetical`；用 shrink 且 `basis < hypothetical`。
   冻结项 target = hypothetical。
3. **初始自由空间**（`F:1741-1754`）：`inner_main − (gap + Σ(frozen 的 outer target, 未 frozen 的 outer basis))`。
4. 循环：
   - a. 全冻结则退出（`F:1762`）。
   - b. 重算 remaining free space；若未冻结项的 grow 之和 < 1（shrink 同理），用 `initial_free_space * Σfactor − gap`
     再与普通 remaining 取更接近 0 者（`F:1772-1801`）。这就是 `flex-grow:.5` 只吃掉一半空间的原因。
   - c. 分配（`F:1822-1843`）：grow → `target = basis + free * g / Σg`；shrink → 权重是
     `scaled_shrink = inner_flex_basis * shrink`，`target = basis + free * scaled / Σscaled`（free 为负）。
   - d. clamp（`F:1850-1862`）：target 依次被 `resolved_minimum_main_size`、max clamp，并把内容盒地板到 0；
     `violation = clamped − target`。
   - e. 冻结（`F:1873-1879`）：总 violation > 0 只冻结 min 违反项；< 0 只冻结 max 违反项；= 0 全部冻结。
   - f. 回到 a。
5. 输出：每项 `target_size.main` / `outer_target_size.main`；**只在行内**做，不跨行。

### 为什么不能简化成“按权重分剩余空间”

- grow/shrink 的选择依据是 hypothetical 之和，不是 basis 之和；basis 被 max-width 压小的项会让整行从 grow 翻成 shrink。
- 第 2 步先冻结“已经被 min/max 夹住”的项；它们不参与分配，剩余空间只在其余项之间重分。
- shrink 的权重是 **basis 缩放后的**：100px 与 200px、shrink 都是 1，亏空按 1:2 分，不是 1:1。
- 第 4b 步允许因子之和 < 1 时只分配相应比例，剩余空间留在容器里（`flex-grow:.5` 不是 `flex-grow:1`）。
- 第 4d/4e 步是反馈：clamp 会改变总占用，可能让别的项再次越界，必须循环到全冻结；单趟分配必然在某些 min/max 组合下
  给出“总宽刚好等于容器但各项比例错”的静默错误。
- clamp 前 target 可能为负；先地板 0 再算 violation，顺序反了会让 min 违反判定错误。
- 全部比较用 inner main size；margin 只进 outer，不参与 factor 比例。

## 5. 交叉轴尺寸与 baseline（§9.4）

| 步骤 | taffy | 不变量 |
|---|---|---|
| hypothetical cross | `F:1892-1974` | 用已定的 main size 布局；cross:auto → fit-content；显式 cross 被 cross min/max clamp、被 p+b 地板 |
| 子项基线 | `F:1978-2060` | 只对 row；仅当行内 ≥2 个基线参与者才算；baseline = 子项首基线 + margin.top |
| 行 cross size | `F:2068-2122` | 单行且容器 cross definite → 行 cross = 容器 inner cross（clamp 后），不是最大子项；否则取 max：非基线项用 outer cross，基线项用 `max_baseline − own_baseline + outer_cross` |
| align-content:stretch | `F:2132-2154` | 仅当 cross definite、且 Σ行 cross + 行 gap < inner cross；差额**平均**加到每行 |
| 每项 used cross | `F:2168-2224` | align-self:stretch + cross:auto + 无 auto cross margin → `line_cross − margins`，再被 cross min/max clamp；否则用 hypothetical cross |
| 容器 cross size | `F:2458-2481` | definite → clamp；否则 Σ行 cross + 行 gap + p+b，再 clamp |

- 基线只实现 row：taffy 明确不支持垂直书写模式（`F:1985`）。column 里 `align-self:baseline` 退化为 flex-start（`F:2423-2432`）。
- stretch 之后要**重新布局子项内容**，把 used cross 当作 definite cross，百分比子项才解析得对；但 stretch 不改变 main size，
  即使有 aspect-ratio（`F:2166` 注释）。
- 非 wrap 单行的行 cross 会被容器 min/max clamp；wrap 的每行不单独 clamp（`F:2110-2120`）。
- auto cross margin 在 align-self 之前解析（见下），会抢占 stretch 的机会。

## 6. 主轴对齐（§9.5，`F:2237-2294` + `A:70-129`）

- `free = inner_main − (Σ outer target + Σ gap)`；gap 只计项与项之间。
- auto main margin：仅当 `free > 0`，所有 auto margin 均分 free，然后 `free = 0`，`justify-content` 变成 no-op；
  `free <= 0` 时 auto margin 归 0（`F:2254-2276`）。
- `justify-content` 偏移：首项单独算起点，后续每项 = gap + 分布量（`A:70-129`）：
  start/flex-start/end/flex-end/center；space-between `free/(n−1)`；space-around `free/n`（首项再加一半）；space-evenly `free/(n+1)`。
- 回退（`A:28-62`）：`n <= 1` 或 `free <= 0` 时，stretch/space-between → flex-start；space-around/evenly → center；
  `safe` 且溢出 → start。这条不实现会得到负 gap、项重叠或反向堆叠。
- 负 free 时后续间距用 `free.max(0)`（`A:115`），溢出全部堆到 main-end；不要产生负 gap。
- `row-reverse`：main-start 在右；taffy 用反向迭代 + `layout_reverse` 偏移（`F:2279-2292`），**不重排数组**。
- 偏移量与 margin 分开保存，最终位置 = 自然流位置 + offset + margin（`F:2553-2563`）。

## 7. 交叉轴对齐（§9.6，`F:2309-2501`）

- 先解析 cross auto margin（`F:2309-2355`）：两端 auto → 各 `free/2`；单端 auto → 吃掉全部 free；都没有 → 走 align-self。
- align-self（`F:2364-2445`）：start/end 是逻辑方向，flex-start/flex-end 受 wrap-reverse 影响；center = free/2；
  baseline = `max_baseline − own_baseline`（wrap-reverse 改为把整组贴底：`line_cross − max_baseline_to_bottom − baseline`）；
  stretch 本身偏移 0（尺寸已在 used cross 阶段撑满）。
- `safe` 且 free<0 → start（`F:2377-2381`）。
- 容器 cross 尺寸：definite 用 clamp 后的值；否则 Σ行 cross + gap + p+b（`F:2470-2474`）。
- `align-content`：`free = inner_cross − Σ行 cross − 行 gap`；用同一套回退/偏移；wrap-reverse 反向迭代行
  （`F:2497-2500`、`F:2721-2747`）。行偏移是累加的，不是每行独立绝对定位。
- 行内 offset_cross 与行自身 offset_cross 相加，再与 padding/margin 相加（`F:2559-2563`）。

## 8. order、reverse、绘制顺序与焦点顺序

- 规范 §5.4（2018 CR，已检索核对）：`order` 改变布局，并**像源文档重排一样改变绘制顺序**；但**不改变**语音顺序和
  顺序导航（Tab）的默认遍历顺序。误用会造成视觉序与 DOM/焦点序不一致。
- taffy 没有 `order` 属性；`FlexItem.order` 只是原始索引（`F:754`），仅用于绝对定位项的文档序（`F:2818`）和
  `Layout.order` 回填（`F:2599`）。因此 taffy 需要宿主在布局前自行按 order 重排子节点。
- `row-reverse/column-reverse` 只改变 main-start 与迭代方向；换行仍在 order-modified document order 上做。
- 我们的引擎里绘制顺序 = 声明顺序。引入 flex 后必须把三条序列分开定义：
  **布局序**（order-modified，决定几何）、**绘制序**（规范要求同布局序）、**焦点/命中序**（规范要求仍是声明序）。
- 兼容旧语义的最省事做法：不暴露 `order`；`row-reverse` 只翻转几何，数组与遍历保持声明序。这样绘制/焦点都稳定，
  但必须明确记录“视觉序 ≠ 声明序是设计选择”，否则将来加 order 时绘制与焦点会一起错。
- 命中测试若按“最后绘制者优先”，绘制序改变会连带改变重叠时的命中结果；测试需要覆盖。

## 9. inline-flex

- inline-flex 是**行内级** flex 容器，参与行内格式化；它的基线用于行内对齐，取**第一个 flex item 的基线**
  （第一行中第一个参与基线对齐的项；没有则第一项；没有基线则从边框盒合成；裁切盒不贡献基线）——§8.5，已检索核对。
- taffy 无行内布局；我们的引擎也没有：Row/Column 只是把子盒排开，文本是独立叶子。因此当前**无法也不必**实现
  “作为行内盒参与文字行”的那一半。
- 但基线的**产出**需要现在就做：列方向取视觉最先行（wrap-reverse → 末行）的首/末项，行方向取首个基线参与者；
  taffy 的容器基线收集在 `F:540-554`。将来有文字行流时可直接消费 `first_baseline`。
- 一个 flex 容器作为别人的 flex item 且 `align-self:baseline` 时，同样通过这个基线参与对齐。

## 10. min/max、box-sizing、百分比

- base size 忽略 min/max（也不因 box-sizing 地板 0）；hypothetical 与 §9.7 的 target 才 clamp。max-width 把 basis 压小后，
  grow 场景下该项在第 2 步就被冻结，拿不到增长。
- box-sizing：taffy 把 content-box 的声明尺寸加上 p+b 换成 border-box（`F:248-249`、`F:954-961`），basis/min/max 都如此。
  我们的 `AxisSize` 已是 border-box（padding 只偏移子项），因此**固定 border-box、省略 content-box**。
- 百分比：
  - basis 百分比只对**definite 的容器 inner main** 解析，否则按内容（`F:965-966`、`F:730-734`）。
  - `basis:auto` 时主轴 width 百分比同理；cross 百分比对 indefinite cross 按 auto。
  - 循环依赖的处理原则：不要拿“由本项内容算出的尺寸”去解析本项百分比；回退到 content，**不要回退到 0**（0 会压扁控件）。
  - 百分比 padding/border 对容器 inline size（宽度）解析，column 里也是宽度（`F:953-956`），不是当前轴。

## 11. 最小正确实现骨架（八阶段）

| # | 阶段 | 输入 | 输出 |
|---|---|---|---|
| 0 | 常量 | 容器 style、可用空间 | dir/wrap、padding+border、gap、inner main 是否 definite、对齐默认值 |
| 1 | 项准备 + 基线尺寸 | 子项 style、容器 inner 尺寸 | 每项 basis、hypothetical inner/outer main、resolved_min（自动最小）、margin |
| 2 | 行构成 | hypothetical outer + gap | 连续行切片（order 已定） |
| 3 | 容器主轴尺寸 | 行、available、min/max | inner/outer main（若未知） |
| 4 | §9.7 | 每行项、inner main、gap | 每项 target/outer target main |
| 5 | 交叉尺寸 | target main、可用 cross | hypothetical cross → 基线 → 行 cross → align-content stretch → used cross |
| 6 | 定位 | 行 cross、free space、对齐 | main offset（auto margin/justify）、cross offset（auto margin/align-self）、容器 cross、行 offset |
| 7 | 落位 | 所有 target 尺寸/偏移 | 子项矩形、容器基线、绘制/焦点序 |

阶段 4 必须循环；阶段 5 的 stretch 可能要求阶段 7 之前再跑一次子项内容布局（百分比子项）；
阶段 3 若由内容推导，还要回头重解析百分比 gap（`F:401-409`）。

## 12. 三个最容易实现错、且错了会静默产生错误布局的点

1. **自动最小尺寸**：忘掉 `min-width:auto = min-content` 或把它应用到交叉轴。布局仍“算得通”、总和仍等于容器，
   但按钮和文字被压扁；`min-width:0` 不生效则表现为永远溢出。必须在阶段 1 得到 resolved_min，并在阶段 4 的 clamp 里用。
2. **把 §9.7 写成单趟权重分配**：只要有一项撞到 min/max，剩余空间就必须在其余项之间重分，且 grow/shrink 选择、
   缩放 shrink 权重、factor 和 <1 三条都会改变结果。错误形态是“总尺寸对、比例错”，测试若只断言容器尺寸会全绿。
3. **换行用 basis 而不是 hypothetical outer，或忘记 margin/gap**：行会在错误位置断，且因为每项最终尺寸仍被
   §9.7 修正，容器总尺寸可能看起来正常。基线行 cross 的 `max_baseline + (outer_cross − baseline)` 同理：
   只取最大 outer cross 会在有降部字体时裁掉基线下方内容。

## 13. 可以安全省略的部分

- inline-flex 的行内参与（无 IFC）：只产出容器基线，留给未来文字行流。
- CSS `order` 属性：保持声明序即保住绘制/焦点；若将来加，必须同时改绘制序而**不改**焦点序。
- `flex-wrap:balance` / `flex-line-count`：L2 草案，控制类 UI 用不到。
- `visibility:collapse`、分片（fragmentation）、scrollbar gutter：我们有独立滚动模型。
- 垂直书写模式与 RTL：先按 LTR 水平实现，但轴抽象保留；RTL 只在需要 i18n 时补。
- aspect-ratio / 替换元素内在比例：当前只有盒与文本，无内在比例节点。
- content-box：`AxisSize` 是 border-box 语义，固定 border-box。
- 百分比尺寸：当前 `AxisSize` 是 f32，没有百分比；将来引入时按 §10 的规则，不要回退到 0。
- 绝对定位在 flex 内的 static position：我们的 Overlay/Modal 不是 CSS abspos，暂不需要；将来需要时按对齐规则算静态位置（taffy `F:2766+`）。

## 14. 我们引擎里 Column/Row 现有语义 → flex 属性

| 现有语义 | flex 等价 | 差异与注意 |
|---|---|---|
| `NodeKind::Row` / `Column` | `flex-direction: row` / `column` | 主轴/交叉轴随之互换 |
| `gap` | `gap`（row-gap = column-gap） | 现有只用于主轴；wrap 后还需行间 cross gap |
| `padding` | `padding` | 不参与 flex 分配 |
| `cross_align = 1.0`（默认） | `align-items: stretch` | 只对 cross:auto 且无 auto cross margin 的子项生效；fixed/max 子项不拉伸 |
| `cross_align = 0` + `cross_pos = 0/0.5/1` | `align-items: flex-start/center/flex-end` | `cross_pos` 就是“剩余空间怎么放”，与 align-items 同义；也可逐项 align-self |
| `0 < cross_align < 1` | **无 CSS 等价** | CSS stretch 只有 0/1；连续拉伸是现有引擎的扩展，迁移时保留为扩展或改二值 |
| `cross_align = 1` + `cross_pos` | 无效果 | 已撑满，slack=0；与 CSS stretch 一致 |
| `Grow::No` | `flex: 0 1 auto` + 显式 min ↔ `min-width` | 现有压缩是“压到声明的 min”且按 `(hint−min)` 比例分摊；CSS 默认 min=min-content、shrink 按 basis 缩放。要保留旧行为需 shrink:0 或自定义权重 |
| `Grow::Yes` | `flex: 1 1 auto`（grow:1, basis:auto） | basis:auto → width，否则内容 max-content；是在内容之上增长 |
| `Grow::Flex(w)` | `flex-grow: w; flex-basis: auto`（`flex: w 1 auto`） | 权重只决定剩余空间比例 |
| `Grow::Split(w)` | `flex: w 1 0`（basis 为 0 长度，不是 0%） | 等权等大；现有实现亏空时先退回实测尺寸，CSS 里对应 `min-width:auto` 的 min-content 下限 |
| `AxisSize.hint` | `width/height`（仅 basis:auto 时） | 非 auto basis 时 hint 被忽略 |
| `AxisSize.min` | `min-width/min-height` | CSS 默认 `auto`=min-content；现有 min 是显式下限 |
| `AxisSize.max` | `max-width/max-height` | 成为 §9.7 的冻结条件 |
| `fixed(v)` | `width:v; min-width:v; max-width:v`（或 `flex: 0 0 v`） | 不参与增长/收缩 |
| `hug(min,hint)` | `width:auto`（内容尺寸）+ `min-width:min` | CSS 用 content 尺寸，hint 由测量得到 |
| `fill(min,hint)` | `flex-grow:1; flex-basis:auto; min-width:min` | 等价 `Grow::Yes` |
| 交叉轴 `grow_x/grow_y = Yes` | `align-self: stretch` | `divider.rs` 的用法；**不是** cross 轴的 flex-grow |
| 交叉轴 `Grow::Flex/Split` | **无等价** | flex 只在主轴分配；需改为 stretch 或省略 |
| `scrollable` | `overflow:auto` + 主轴不收缩（shrink:0 或 min-content 下限） | 现有把可用主轴设成内容总量来避免压缩；CSS 靠 overflow + 自动最小尺寸 |
| `Stack` / `Overlay` | 不是 flex | 保留为独立容器 / abspos 类；Overlay 对应 flex 内的绝对定位项 |

一句话总结映射：`cross_pos` ↔ `justify-content` 是**错的**——它作用在交叉轴，等价 `align-items`/`align-self`；
主轴剩余空间的分配才对应 `justify-content`，而现有引擎根本没有这个自由度（grow 吃掉全部剩余）。
