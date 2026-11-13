# 盒模型与定位：规则、来源与取舍

> 为布局引擎重写确定「盒模型 + 定位」的规则面。规范按 `规范 §章节`、实现按 `path:line`
> 标注；规范原文用本机下载的 `/tmp/specs/*.html` 核对过。旧项目未参与结论。

## 0. 结论
CSS 盒模型可分三层：**尺寸层**（`width/height/min/max` 与 `box-sizing`）、**空间层**
（margin/border/padding）、**坐标层**（position/inset/包含块）。前两层是纯代数，可完整
吸收；第三层的难点是**包含块链**与 **auto 的三义性**（auto 尺寸、auto margin、auto
inset）。float 与 margin collapsing 是块级排版遗产，现代 UI 框架普遍不实现，我们也不实现。

## 1. 盒模型
### 1.1 四个盒
从内到外是 content / padding / border / margin 四个矩形（`css-box-4 §2`）。`width`/
`height` 默认指 content box。padding 百分比按**包含块宽度**解析（`CSS2.2 §8.4`），margin
同样（`§8.3`），可为负；border 宽度是长度，**占据布局空间**而不只是绘制属性
（`css-box-4 §2`）。当前引擎的 border 只是 `Shape::Box` 的绘制字段，不参与尺寸，必须补。

### 1.2 box-sizing
`content-box | border-box`，初始 **content-box**（`css-sizing-3 §3.5`）：

| 值 | `width:100px` 的含义 | 内容盒 |
|---|---|---|
| content-box（CSS 默认） | 内容盒 = 100 | 100 |
| border-box | 边框盒 = 100 | `max(0, 100 - padding - border)` |

内容盒不得为负：padding+border 超过 border-box 值时盒子反而变大（`css-sizing-3 §3.5`）。
`box-sizing` 同时影响 `min/max/flex-basis`。taffy 的默认是 **BorderBox**
（`taffy-0.15.0/src/style/mod.rs:446-452`），content-box 差值加到 style size 上
（`compute/block.rs:397-406`、`compute/oof.rs:488-492`）；GPUI 沿用 taffy 默认、
未设置该字段（`gpui/src/taffy.rs:494-529`）。→ 我们建议**默认 border-box**（符合
`fixed(380)` 的直觉），但保留 content-box 并可切换；这是有意偏离 CSS 默认，需写进类型文档。

### 1.3 `width/height: auto`
`auto` 不是 0，由上下文决定：

| 上下文 | `width:auto` | `height:auto` |
|---|---|---|
| 普通块级、非替换 | 填满包含块（解 `margin+border+padding+width=CB`，`CSS2.2 §10.3.3`） | 内容高度（`§10.6.3`） |
| float / abspos 单轴 auto | shrink-to-fit / fit-content | 内容高度 |
| flex/grid item | 由 flex/grid 算法决定（内容基准 + grow/shrink） | 内容或拉伸 |

shrink-to-fit：`min(max(preferred minimum, available), preferred)`（`CSS2.2 §10.3.5`）；
现代写法 `clamp(min-content, stretch-fit, max-content)`（`css-sizing-3 §2`）。
→ 引擎内部只需 **min-content 与 max-content** 两个 intrinsic 测量，所有 auto/fit-content/
hug 由它们与可用空间组合。扣减顺序：

```
content_avail = containing_block - margin(两向) - border(两向) - padding(两向)
```

负 margin **增加**可用空间；内容盒至少为 0（`css-sizing-3 §3.1`）。margin 不参与背景绘制，
`rect` 应保持 **border box**，margin 只影响摆放。

### 1.4 margin collapsing（决定：不实现）
CSS 只让**同一 BFC 内、垂直相邻、无 border/padding/line box 分隔**的块级 in-flow 盒
合并；flex/grid item、float、abspos、inline-block、新 BFC 都不合并；负 margin 的合成值是
「最大正值 + 最负负值」（`CSS2.2 §8.3.1`）。不实现理由：我们不做通用文档流，Row/Column
是 flex 语义（flex 本就不折叠），文本节点无块级子节点；实现它需 BFC、clearance、
line box、collapse-through 四套状态。→ 见 §7。

## 2. 尺寸约束与通用 clamp
### 2.1 一套 clamp 足够
固定优先级：**min 最强**，max 次之，`width/height` 只是 tentative：

```
used = max(min, min(tentative, max))       // css-sizing-3 §3.1
if max < min { used = min }                // CSS2.2 §10.4
```

taffy 的 `maybe_clamp` 正是 `base.min(max).max(min)`（`util/math.rs:47-54`），block 另有
`max<=min→min`（`compute/block.rs:425-431`）。**不能**写成 `v.max(min).min(max)`，那会让
max 胜出。min/max 的解释盒与 `width` 一致：content-box 下都指内容盒，border-box 下都指
边框盒（`css-sizing-3 §3.5`），所以先做 box_sizing 调整再 clamp（taffy：`block.rs:397-431`）。

### 2.2 auto 宽度下的 min/max；`min-width: auto`
先解出 auto 宽度（fill 或 fit-content），再 clamp。`min-width` 初始 0、`max-width` 初始
`none`（`CSS2.2 §10.4`），默认不改变 auto 结果。百分比 min/max 按包含块宽度解析；若包含
块宽度依赖本元素，CSS 2.2 称结果未定义。`auto` 这个初始值本身随上下文变义：普通块级等价 0；
**flex/grid item 主轴**是 content-based minimum size，**overflow 非 visible 的滚动容器则为 0**；
该最小值 = 主轴 min-content size，被指定尺寸与 max 夹住（`css-flexbox-1 §4.5`）。taffy 只在
flex 主轴路径实现它（`compute/flexbox.rs:1122`，min-content 夹住 `:1142`）。→ Row/Column 若
定义为 flex 语义，必须实现「主轴 auto 最小 = min-content，滚动容器为 0」，否则长文本会被压到
0 或撑爆兄弟。

### 2.4 能否只用一个 clamp？
能，但输入必须分三类，不能塞进一个 `(min,hint,max)`：①内容贡献（min/max-content，测量得到）；
②作者指定值（`width`、`min/max`，可 definite/auto）；③可用空间（扣完 margin/border/padding）。
当前 `AxisSize{min,hint,max}`（`ui-core/src/layout.rs:14-24`）把 ①与②混在 `hint/max`，
`Grow` 又绕过 clamp，重写时必须拆开。

## 3. overflow
### 3.1 值语义

| 值 | 裁剪？ | 程序滚动？ | 滚动条 | scroll container |
|---|---|---|---|---|
| visible | 否，画到盒外 | — | — | 否 |
| hidden | 是，裁到 **padding box** | 是 | 不提供 UI | 是 |
| clip | 是，裁到 overflow clip edge | 否 | 不提供 | 否 |
| scroll | 是，裁到 padding box | 是 | 始终显示 | 是 |
| auto | 有溢出同 scroll，否则同 hidden | 是 | 有溢出才显示 | 是 |

细节：一轴非 `visible/clip` 时，另一轴 `visible/clip` 的计算值分别变 `auto/hidden`
（`css-overflow-3 §3.1`），所以「一轴裁一轴不裁」无法指定；`hidden` **仍建立 scroll
container 与新 BFC**（`§3.1`、`CSS2.2 §9.4.1`）。

### 3.2 对内容尺寸的影响
- scroll container 的**自动最小尺寸为 0**（`css-flexbox-1 §4.5`），这是它不被内容撑爆的前提。
- 内容溢出**不**参与父容器的 scrollable overflow（taffy 用 `Overflow::is_scroll_container`
  区分，`style/mod.rs:475-516`）。
- **滚动条占位**：classic scrollbar 在 border 与 padding 之间形成 gutter，占内容空间；
  **overlay scrollbar 不占位**（`css-overflow-3 §4.2`）。CSS 要求先假设「不需要滚动条」算
  一次，发现溢出再加 gutter 重算（`§4.1`）。taffy 把这件事做成一个标量
  `Style::scrollbar_width`（`style/mod.rs:731-733`），为 0 时 `Scroll` 等价 `Hidden`。我们的
  滚动条是 overlay，默认无需 gutter；将来做 classic 滚动条时必须把它计入 padding box 内缩，
  而不是画在内容上。
- 滚动容器的包含块是它的 **padding box**；滚动偏移只改内容坐标，不改包含块大小
  （`css-position-3 §2.1`）。
- 现状：`mask = parent_mask ∩ rect` 对每个节点无条件求交（`layout_engine.rs:424`），等价于
  「处处 overflow:hidden」；没有 `overflow` 字段，`scrollable` 只让主轴不压缩
  （`tree.rs:211`、`layout_engine.rs:482-491`）。visible 无法表达，滚动容器没有
  「内容尺寸/视口尺寸」两套值，也没有 gutter。

## 4. 定位
### 4.1 五个值

| position | 脱离流 | inset 作用 | 包含块 | 建立 CB |
|---|---|---|---|---|
| static | 否 | 无 | 格式化上下文 | 否 |
| relative | 否（保留槽位） | 绘制/命中偏移，不影响兄弟 | 同 static | **是** |
| absolute | 是 | 相对 CB 解方程 | 最近 positioned 祖先的 padding box | **是** |
| fixed | 是 | 相对视口 | 视口（或建立 fixed CB 的祖先） | **是** |
| sticky | 否 | 相对最近 scrollport 的阈值 | 同 static | **是** |

`relative` 左右都指定时 over-constrained：LTR 下 `left` 胜、`right=-left`；上下都指定时
`bottom` 被忽略（`CSS2.2 §9.4.3`）。它保留原槽位，不重排兄弟。`fixed` 相对视口、不随滚动
移动（`css-position-3 §2.1`）；我们的视口是根节点的 border box，因此 fixed 可退化为
「CB 固定为根」的 absolute。

### 4.2 包含块（最易错）
`absolute` 的包含块 = **最近 positioned 祖先**（`position != static`）的 **padding box**；
没有则为 initial containing block（`css-position-3 §2.1`）。是 padding edge，不是 border box
或 content box。`fixed` = 视口，除非祖先经 `transform/will-change/contain` 建立 fixed CB
（`§2.1`）。`relative/sticky` 的 CB 仍由格式化上下文决定，但它们**自己**成为后代的 CB。
scroll container 不会仅因 overflow 就成为 absolute 的 CB，必须 positioned。
OOF 需在祖先最终尺寸确定后求解，故要两阶段（in-flow 后 OOF）；taffy 的
`compute/oof.rs:348`、`:445` 即此。

### 4.3 abspos 的 over-constrained 方程
```
left + margin-left + border-left + padding-left + width
     + padding-right + border-right + margin-right + right = CB width
```
求解（`CSS2.2 §10.3.7`；新模型 `css-position-3 §4`）：
1. 用 inset 把 CB 缩成 **inset-modified CB（IMCB）**：一轴只有一个 auto → 置 0；两个都
   auto → 用 static position/self-alignment 解。IMCB 为负时缩减「较弱」inset（end 边；
   只有一个 auto 时就是它）（`§3.5.1`）。
2. `width:auto`：两轴 inset 都确定且对齐为 stretch（或 normal + 非替换）→ **stretch-fit**
   = `IMCB - 非 auto margin`；否则 → **fit-content**。abspos 的自动最小尺寸**恒为 0**（`§4.1`）。
3. auto margin：任一 inset 为 auto 时 = 0；否则均分剩余；inline 轴剩余为负且两 margin 都
   auto 时全给 end 边（`§4.2`）。
4. 两条 inset 都确定且无 auto margin 时，按 self-alignment 把 margin box 放进 IMCB（`§5`）。
垂直轴同理，方程与 top/height/bottom 分支见 `CSS2.2 §10.6.4`。static position 是「假如
static 会落在哪」的近似（`css-position-3 §3.5.2`），对 Row/Column 即内容原点/下一个流位置。

### 4.4 sticky
包含块由格式化上下文决定；inset 是相对**最近 scrollport** 的**阈值**，不是偏移
（`css-position-3 §3.4`）。sticky view rectangle = scrollport 各边内缩 inset；若比盒子小，
end 边 inset 可变负把它撑到盒子大小。盒子的边若在矩形外，按 relative 向内平移，但不能超出
其包含块（position box = margin box）。**taffy 0.15 只实现 sticky 的包含块语义，不实现平移**：
`Position::Sticky` 只出现在枚举与解析（`style/mod.rs:354-376`、`:428`），注释明说 offset 由
调用方在知道滚动位置后处理（`:373-375`），`is_positioned()` 把它当 CB（`:389-391`）。
→ 我们的滚动偏移本就是 app 每帧传入的视图值（`tree.rs:219-228`），sticky 最好做成
**绘制/命中期平移**，布局仍按 static 放。

### 4.5 z-index 与绘制顺序
CSS：背景/边框 → 负 z positioned → in-flow 块 → float → in-flow inline → z=0/auto
positioned → 正 z（`CSS2.2 Appendix E`）。**z-index 只对 positioned 元素生效**，static
元素无视 z-index。我们的绘制顺序是声明顺序（`Node::order`，`tree.rs:397`；`children_of`
按 order 排序 `:960-972`），模态子树整体推后（`:1248-1272`）。重写建议：声明顺序 = 基线层，
只允许 `position != static` 的节点带整数 z-index，在同一 stacking context 内排序；模态保留
「最后绘制 + 焦点作用域」。不实现负 z 与复杂 stacking context（transform/opacity 成层）。

## 5. float 与 clear（决定：不实现）
规则（`CSS2.2 §9.5`）：float 脱离流但不脱离行内内容——块级盒假装它不存在，后续 line box
缩短以避开其 margin box；放不下就下移；BFC 根的 border box 不得与 float 的 margin box
重叠；`clear` 把盒子推到前面 float 下方（`§9.5.2`）。
不实现判断：①唯一不可替代能力是「文字环绕图片/图形」，我们的文本节点没有「旁边放 float」的
API；②工业界一致：Flutter/Qt/GTK 的布局盒无 float，GPUI 连 taffy 的 float 都没接
（`gpui/src/taffy.rs:494-529` 无 `float/clear`）；③成本高：BFC、line box 避让、clearance、
父高度包含四件事只为边缘场景服务。taffy 实现了 float（默认 feature 开启，
`Cargo.toml:64-75`；算法 `compute/float.rs:1-27`、`compute/block.rs:167/194/226`），但那是
为 block 排版服务的。→ 真需要图文环绕时做显式 inline media wrap，而不是通用 float。

## 6. 居中对齐的最小完备集
单轴只需两组概念：容器怎么排（`justify-content`/`align-items`）与单个子项怎么覆盖
（`align-self`/auto margin）。

| 想要 | flex/Row/Column 写法 | 等价写法 |
|---|---|---|
| 交叉轴居中 | `align-items:center` | 子项 `margin-block:auto` |
| 交叉轴拉伸 | `align-items:stretch` + 子项交叉尺寸 auto | — |
| 主轴居中 | `justify-content:center` | 子项两侧 `margin:auto` 吸收剩余 |
| 主轴两端 | `justify-content:space-between` | 首尾 auto margin |
| 单个覆盖 | `align-self:start/end/center/stretch` | 子项 `margin-*:auto` |
| abspos 居中 | `inset:0` + `margin:auto` | `place-self:center` |

关键交互：**auto margin 在 flex 中先于 justify-content 吸收剩余**；`align-self:stretch`
只对交叉尺寸 auto 的项生效，auto margin 取消 stretch（`css-flexbox-1 §8.1`）；abspos 中
auto margin 均分剩余，inline 轴可为负（`css-position-3 §4.2`）。我们的
`cross_align`/`cross_pos`（`tree.rs:191-195`）是连续比例，能表达 stretch/center/end，但无法
表达 space-between 与 per-child override。重写时降级为语法糖：`cross_align=1` →
`align-items:stretch`；`0,0.5` → `center`；`cross_pos=1` → `end`；存储用离散枚举 +
`align_self: Option<Align>`。

## 7. 必须实现 vs 可以放弃
### 7.1 必须实现

| 规则 | 来源 |
|---|---|
| content-box / border-box 两种解释（默认 border-box） | `css-sizing-3 §3.5` |
| `width/height:auto`（fill / fit-content / stretch），需 min/max-content 测量 | `CSS2.2 §10.3.3/§10.3.5`、`css-sizing-3 §2` |
| margin（auto、负值）、border、padding 参与布局 | `css-box-4 §2`、`CSS2.2 §8.3/§8.4` |
| min/max clamp，min 胜出，`max<min→min`，作用于每条尺寸路径（含 grow） | `css-sizing-3 §3.1`、`CSS2.2 §10.4` |
| flex item 主轴 `min-width:auto` = content-based；滚动容器为 0 | `css-flexbox-1 §4.5` |
| overflow visible/hidden/scroll/auto 按轴，裁到 padding box | `css-overflow-3 §3.1` |
| overlay 滚动条不占位；classic 计入 gutter | `css-overflow-3 §4.1/§4.2` |
| position static/relative/absolute/fixed；sticky 可后置 | `css-position-3 §2.1/§3` |
| 包含块链（positioned 祖先的 padding box），两阶段解 OOF | `css-position-3 §2.1` |
| IMCB + auto 宽 + auto margin + static position | `css-position-3 §3.5/§4` |
| z-index 仅对 positioned 生效，同一 stacking context 内排序 | `CSS2.2 Appendix E` |
| align-items / justify-content / align-self / margin:auto | `css-flexbox-1 §8`、`css-position-3 §4.2` |

### 7.2 可以放弃

| 放弃 | 理由 |
|---|---|
| margin collapsing | 只服务块级文档流；flex/grid/abs 本就不折叠；需 BFC/clearance/line box 四套状态 |
| `float` / `clear` | 唯一不可替代场景是文字环绕图片；Flutter/Qt/GTK 无此概念，GPUI 未接 taffy float |
| `sticky` 的布局期实现 | 可由 app 每帧滚动偏移在绘制/命中期平移；taffy 自己也不实现偏移；保留 `Sticky` 作 CB 标记 |
| `overflow: clip` | 与 hidden 的差别是禁止程序滚动；我们没有程序滚动 API |
| 一轴 visible 一轴 scroll 的计算值传播 | 允许两轴独立更直观；偏离 CSS，需注明 |
| 负 z-index 与嵌套 stacking context | 只在复杂叠层需要；可用显式「层」字段后补 |
| 书写模式 / RTL / 逻辑属性 | 全仓库 LTR、水平书写；物理 `left/right` 更简单 |
| 百分比高度（父高不定时） | UI 里固定/内容高度占绝大多数；先只支持 definite 父高 |
| `aspect-ratio` + 替换元素 intrinsic ratio | 不在最小集；图片 fit 已有 `ImageFit` |
| baseline 对齐 | 需文本引擎暴露基线；先做 start/end/center/stretch |
| grid 及其 `min-width:auto` 变体 | 单独子系统，不在本题范围 |
| `scrollbar-gutter:stable/both-edges` | 只有 overlay 滚动条，无 gutter |
| `min-content/max-content/fit-content()` 关键字与 `calc()` | 内部保留 intrinsic 测量；作者 API 先只暴露 auto/length/percent |
| block 流里的垂直 `margin:auto` | 只在 flex/abspos 中支持 auto margin |
