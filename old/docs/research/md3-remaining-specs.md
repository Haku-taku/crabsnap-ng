# MD3 尚未做成控件的导出：令牌面、引擎前置与顺序

> 目的：把 `ui/crates/ui-widgets-md3/tokens/` 里**还没有对应控件**的设计系统导出逐个摊开，
> 给出「要写控件所需的最小数值集 / 需要哪些当前引擎没有的能力 / 能不能复用现成部件 /
> 画廊与自检该钉住什么」，并回答两个具体问题（menus 与 app-bar 溢出、menus 与右键菜单）。
>
> 采信纪律按 `AGENTS.md` §1.2：每条承重结论给 `path:line`（本仓库）或 JSON 里的具体键路径，
> 外部规范给 URL；三条通道都不满足的标 `[未证实]`。旧项目 `/home/ollie/git/crabsnap` 未参与结论。
>
> 令牌数值用 `python3 tokens/gen_spec.py inspect <json> --prefix <set>` 抽样读得（不整份载入）。
> 本文只读分析，未改动任何代码。

## 0. 总表

| 导出 | 令牌前缀（`tokenSets`） | 关键前置 | 现在可做 / 阻塞 | 建议顺序 |
|---|---|---|---|---|
| `menus.json` | `md.comp.menu`（只有容器）、`md.comp.menus`（条目全量）、`md.comp.menus.vibrant` / `.standard` | 已有 `spec/menu_tokens.rs` + `menu_style.rs`；缺**接线**（`set_menu_style` 零调用方）与浮层摆放 | **可做**（接线是纯控件/画廊侧） | 1 |
| `buttons/segmented-buttons.json` | `md.comp.outlined-segmented-button` | 复用 `Md3Button` + `GroupPosition`；缺 `gen_spec` 词表与等分（`Grow::Split` 已有） | **可做** | 2 |
| `indicators/progress-indicators.json` | `md.comp.progress-indicator`(+`.circular`/`.linear`)、`md.comp.linear-/circular-progress-indicator`（后者整族废弃） | 线性**可做**；环形**阻塞**（无弧线/旋转图元） | 3（线性） |
| `indicators/loading-indicator.json` | `md.comp.loading-indicator` | 48dp 圆容器内 38dp 标记，可走 `Icon`（`progress_activity` 码点已在表里）；需**逐帧自转**能力 | **可做**（近似：图标自带旋转形 + 关键帧） | 4 |
| `navigation/navigation-bar.json` | `md.comp.navigation-bar`（条目态）、`md.comp.nav-bar`(+`.item.horizontal`/`.vertical`) | 等分用 `Grow::Split(1)`（缺口 #7 已修）；无需浮层 | **可做** | 5 |
| `navigation/navigation-rail.json` | `md.comp.navigation-rail`、`md.comp.nav-rail`(+`.expanded`/`.collapsed`/`.item`) | 同上；`expanded`/`modal` 与抽屉重叠 | **可做** | 6 |
| `navigation/navigation-drawer.json` | `md.comp.navigation-drawer` | 条目 = `Md3ListItem` 换皮；`modal` 变体要浮层 O1–O3 | 分两半：standard **可做**，modal **等浮层** | 7 |
| `buttons/split-button.json` | `md.comp.split-button.{xsmall..xlarge}` + `md.comp.button.{elevated,filled,outlined,tonal}` | 复用 `Md3ButtonGroup`/`GroupPosition` 的圆角派发；箭头菜单**要浮层 O1–O3** | 表面可拼，菜单**阻塞** | 8 |
| `toolbars.json` | `md.comp.toolbar.{floating,docked,vibrant,standard,floating.fab}`、`md.comp.bottom-app-bar` | 容器 = `app_bar`/`nav_bar` 的近亲；**成员是 icon-button**，而 `icon-buttons.json` 没生成表 | standard/docked **可做**（成员先用现算值） | 9 |
| `snackbar.json` | `md.comp.snackbar` | 浮层摆放可用 **`fixed`+`inset`（P5 已有）** 而非 O1；进出过渡是缺口 #13，可用「常驻树内 + opacity 0」绕 | **可做**（用绕法） | 10 |
| `tooltips.json` | `md.comp.plain-tooltip`、`md.comp.rich-tooltip` | 跟随指针需 `Cx::pointer()`（**已有**）；**延时**要一个到点唤醒机制，当前没有 | 摆位可做，**延时阻塞**（见 §12） | 11 |

**同时澄清**：`extended-fab` 与 `icon-buttons` **没有被现有控件覆盖**——`src/spec/` 里没有
`extended_fab_tokens.rs` / `icon_button_tokens.rs`（`ls src/spec/` 27 个文件，二者皆无），
`gen_spec.py` 的 `OBJECTS_BY_CONTROL` 也没有这两个前缀；进度文档 §4.12 已明写
「**本仓没有 icon-button 的令牌导出**」，那句话是**导出改名/重命名之前的旧判断**，如今
`tokens/buttons/icon-buttons.json` 确实在（13 个 `tokenSets`），仍未生成表。

## 1. `menus.json`

### 1.1 令牌面

四个 tokenSet，**不是一份规格的两遍**：

| tokenSet | 内容 | 关键数值 |
|---|---|---|
| `md.comp.menu` | **只有容器**：`container.color = surface-container`、`elevation = 3.0`、`shadow-color = shadow`、`shape = rounded_corners` | 条目令牌在旧的 `menu.*` 下**整族废弃**（`inspect` 的 `skipped_deprecated` 列出 43 条 `menu.list-item.*` / `menu.divider.*`），别用它写条目 |
| `md.comp.menus` | 条目全量 + 分栏 + 水平菜单（Expressive） | 见下 |
| `md.comp.menus.standard` / `.vibrant` | 容器色的两个变体：`surface-container-low` vs `tertiary-container` | 与 `standard`/`vibrant` 两条轴等价，走 `menus_spec` 的 `standard`/`vibrant` 开关即可，不必单独生成表 |

`md.comp.menus` 的关键数值（键路径均 `md.comp.menus.`）：

- 容器：`container.elevation = 3`、`container.shape = rounded_corners`、`container.top-space = container.bottom-space = 8`（`horizontal` 下也是 8）；面板自身**没有宽度令牌**（`min-width` / `max-width` 不在导出里）。
- 条目：`menu-item.height = 44`、`menu-item.top-space = bottom-space = 8`、`menu-item.leading-space = trailing-space = 16`（`horizontal` 分支是 12）、`menu-item.between-space = 12`、`menu-item.shape = rounded_corners`、`menu-item.first-child.shape` / `last-child.shape`（首末项各自的圆角）、`menu-item.label-text = label-large` 族（`size = 14`、`line-height = 20`、`weight = 500`）。
- 图标：`menu-item.leading-icon.size = 20`、`menu-item.trailing-icon.size = 20`（**20，不是列表项的 24**）。
- 焦点：`menu-item.focus.indicator.thickness = 3`、`focus.indicator.color = secondary`、`focus.indicator.outline.offset = -3`。
- 状态层：`hovered = 0.08`、`focused / pressed = 0.1`；选中容器 `selected.menu-item.container.color`（standard 下是 `tertiary-container` 一侧）。
- 分栏 / 组：`group.padding = 2`、`root.gap = 2`、`horizontal.icon-only.gap = 4`；`section-label-text` 一族（分组标签）。
- 废弃项提醒：`md.comp.menu.container.surface-tint-layer.color` 与全部 `menu.list-item.*` 都带 `deprecationMessage`——`gen_spec.py` 会跳过，所以**不要指望 `menu_spec` 给出条目**。

### 1.2 引擎前置

`spec/menu_tokens.rs`（21979 字节，`menus_spec(...)` 有 17 个 bool 开关）与 `menu_style.rs`
**都已存在**，缺的是**接线**：

- `AppLoop::set_menu_style` 定义在 `ui-platform/src/view.rs:642`，**全仓库零调用方**
  （`grep -rn set_menu_style ui/` 只命中定义与其文档注释 `view.rs:541-542`）。
- `Md3Kit::menu_style(cx)` 定义在 `ui-widgets-md3/src/style.rs:361`，**同样零调用方**。
- 结果：引擎今天画的上下文菜单用的是 `MenuStyle::default()`（`view.rs:636`），
  即 `row_height = 34`、`radius = 8`、`border = 白色 1px`、`font_size = 14`、无最小宽（`menu.rs:39-55`）——
  **不是 MD3**。按 `AGENTS.md` §5「实物到了 ≠ 接线到了」，这条要单独立项。

除接线外，控件级菜单（要富内容、要选中态、要级联）确实需要浮层 O1–O3
（`docs/popover-tooltip-design.md` §8：`OverlayPlacement`、命中/z、`Change::Dismissed`）。
核对结论：**O1–O4 一行代码都没落地**——`grep -rn 'OverlayPlacement|Anchor::|Dismissed|HoverChanged' ui/`
只命中 wayland layer-shell 的 `Anchor`（`ui-platform-wayland/src/window.rs:1025-1028`），
`ui-core` 里没有任何浮层类型。

### 1.3 与既有控件的关系

- 引擎兜底菜单（`ui-platform/src/menu.rs`）：`OpenMenu` **不在树里**，由 `paint_menu`
  画在树之上（`view.rs:1793-1849`，紧随 `paint_tree` 于 `:1800`），自算行矩形（`menu.rs:107-116`）、
  自命中（`row_at` / `enabled_row_at`，`menu.rs:128-140`）、自上报 `Change::MenuItem(item.id)`
  （`view.rs:1742`）。`MenuItem { id, label, enabled }`（`ui-core/src/menu.rs:11-16`）**只有文字**，
  没有图标、没有选中、没有级联、没有分隔线。
- `menu_style.rs` 已经把 `md.comp.menus` 的容器色 / 墨色 / 悬停层 / 字号 / 行高 / padding 喂进
  `MenuStyle`（`menu_style.rs:24-48`），`RADIUS = 4.0` 与 `MAX_WIDTH = 560.0` 是文件里的常量
  （`:14`、`:17`）。**注意**：`MAX_WIDTH = 560` 与 `min_width = 112` 都不来自导出，是本仓自补的
  （导出里没有宽度令牌）——`112` 与 Compose/Flutter 的 `MenuHorizontalMargin/最小宽` 同值，
  但 560 只是「超过就换行」的经验上限，**[未证实]**（找不到规范出处）。
- `list_tokens.rs`（`md.comp.list`）与菜单条目是**两套数值**（列表项高 56/72/88、图标 24，
  菜单项高 44、图标 20），不要把菜单条目并进 `Md3ListItem`。

### 1.4 建议验收

画廊页 `Controls · Menu`（或 `Layouts · Menu`）应钉：面板宽 = `clamp(112, 最宽标签 + 2×pad_x, 560)`；
条目高恰为 `menus_spec(...).menu_item.height`（44）；面板上下内衬恰为 `container.top-space`（8）；
选中行容器色 == `menus_spec(selected, ..., standard)` 的 `menu-item.container.color`，
墨色 == 同一张表的 `label-text.color`（与 `menu_style.rs` 用同一条解析路径，防「绘制与命中不同源」）。
自检还须断言**兜底菜单与控件级菜单取色一致**（否则接线换掉一份而另一份悄悄漂移）。

## 2. `buttons/segmented-buttons.json`

### 2.1 令牌面

唯一 tokenSet `md.comp.outlined-segmented-button`（46 条有效令牌）。要点：

- 默认态是 `unselected`（`inspect` 的 `default_state`），参数：`selected`、`disabled`、
  `focus`、`hover`、`pressed`、`with-icon`。
- 几何：`container.height = 40`、`shape = circular`（胶囊，半径 = 20）；
  `label-text = label-large`（`size = 14`、`line-height = 20`、`weight = 500`）；
  `outline.color = outline`、`outline.width = 1`；`with-icon.icon.size = 18`。
- 选中容器 `selected.container.color = secondary-container`，墨色 `on-secondary-container`；
  未选中墨色 `on-surface`。禁用：内容 0.38、描边 `on-surface` @ 0.12。
- 状态层 0.08 / 0.1；焦点环 `focus.indicator.thickness = 3`、`color = secondary`、
  `focus.indicator.outline.offset = 2`（**外扩 2**，声明期拿不到最终矩形，同 §4.3/§4.4 的处理）。
- 导出**不含**任何宽度 / 内衬 / 分隔线令牌：等分槽位与「成员之间用描边而非分隔线」是规范约定，
  见 §2.4。

### 2.2 引擎前置

**现在就能做。** 需要 `Grow::Split(1)` 让各段等宽——缺口 #7「等分容器做不到」**已修**
（`docs/engine-gaps.md:99-108`，`Grow::Split` 在 `ui-core/src/layout.rs:159`，
测试 `a_zero_basis_grower_splits_the_row_evenly_whatever_its_content`）。
`docs/lessons/design-errors.md` 的 `cross_pos` / `cross_align` 两行提醒过它们**是父容器的参数**，
分段条按等分排就行，不必给子节点写。

不需要浮层、不需要过渡、不需要按下点。唯一会踩到的是缺口 #3（`decl()` 不知自身尺寸）：
分段条自己的宽度由 `Split` 分出来，但**每段内部的图标+文字水平居中**要靠 `Row` 的
`cross_align`/`cross_pos`，不需要读到宽度，所以不阻塞。

### 2.3 与既有控件的关系

核对结论：**`Md3ButtonGroup`（`src/button_group.rs`）没有覆盖这份导出，两者是不同的 MD3 部件。**

| | `Md3ButtonGroup`（`buttons/button-groups.json`） | segmented-buttons |
|---|---|---|
| 前缀 | `md.comp.button-group.connected`（`button_group.rs:18` 引 `button_group_connected_spec`） | `md.comp.outlined-segmented-button` |
| 高度 | 32/40/56/96/136 五档（`button_group_tokens.rs:67-78`） | 只有 40 |
| 形态 | 共用一颗胶囊，`between_space = 2` 的缝透出组底色（`button_group.rs:132`） | 外轮廓胶囊、成员之间以 `outline` 1dp 分隔 |
| 语义 | 成员是普通/切换按钮，组只是「圆角协商者」 | 整组就是**单选或多选的过滤器**，成员是选择项 |

可复用的现成件：`GroupPosition::corners(outer, inner)`（`ui-widgets/src/group.rs:41-50`）
正是分段条首末项需要的圆角派发；`Md3Button`（`src/button.rs`）的 `with-icon`、状态层、
`spec_ink`、`DISABLED_CONTENT_OPACITY` 全部可用。真正要新写的是「描边 + 选中填充 + 单选」
这一层，以及给 `gen_spec.py` 补 `md.comp.outlined-segmented-button` 的 `OBJECTS_BY_CONTROL`
词表（当前 21 个前缀里没有它）与 `VARIANT_GROUPS` 的 `selection` 轴（照 `md.comp.button` 的
`toggle` 轴写法，`gen_spec.py:497-505`）。

外部对齐（通道②）：material-web 的 `md-filter-chip` 组与 `button-group` 分开、Compose
`SegmentedButton` 与 `ButtonGroup` 也是两个可组合件；规范
https://m3.material.io/components/divider 的 segmented 段（「Segmented divider」用于分段按钮内
分隔）说明**成员间是 1dp 描边而非独立分隔线**——与导出给 `outline.width = 1` 一致。

### 2.4 建议验收

画廊页钉：总宽 = 段数 × 单段宽（每段差 < 0.5，照 `selftest/tabs.rs:19-27` 的等分断言写法）；
组高恰 40、圆角恰 20；点某一段上报 `Change::Selected(Some(id))` 且**该段**取
`secondary-container`；多选模式下已选段的描边不再出现（选中段填色盖住描边）。

## 3. `indicators/progress-indicators.json`

### 3.1 令牌面

5 个 tokenSet，**只有 `md.comp.progress-indicator` 是有用的那份**：

- `md.comp.progress-indicator`：`active-indicator.color = primary`、`track.color = secondary-container`、
  三族 `shape = circular`、`stop-indicator.color = primary`；
  分支 `circular.size = 40`、`circular.track-active-indicator-space = 4`、
  `circular.active-indicator.thickness = track.thickness = 4`；
  `linear.height = 4`、`linear.active-indicator.thickness = track.thickness = 4`、
  `linear.stop-indicator.size = 4`；带波浪：`circular.with-wave.size = 48`、
  `linear.with-wave.height = 10`、`linear.wave.amplitude = 3`、`wavelength = 40`、
  `circular.wave.amplitude = 1.6`、`wavelength = 15`；`indeterminate.linear.wave.wavelength = 20`。
- `md.comp.progress-indicator.circular` / `.linear`：上面那份的**子集**（7 / 9 条），值一致。
- `md.comp.linear-progress-indicator` / `md.comp.circular-progress-indicator`：
  `inspect` 给出 **0 条有效令牌**——全部带 `deprecationMessage`（`skipped_deprecated` 列了
  `active-indicator.{color,shape,height/width}`、`track.*`、`four-color.*`、`size`）。
  **不要为它们生成表**，`gen_spec.py` 会产出空结构。

废弃名单里还包含 `active-indicator.thickness`、`track.thickness`、`stop-indicator.size`、
`active-indicator-track-space`、以及全部 `thick` 分支——即 **4dp 粗细本身已被上游标废弃**，
数值只能取「废弃项」，与 slider 的刻度点颜色同一处境（进度文档 §4.2「导出的 with-tick-marks
已废弃且替代令牌不在导出里」）。这条必须写进控件注释旁的取舍说明，否则后来人以为是漏解析。

### 3.2 引擎前置

- **线性进度条：现在就能做。** 几何 = 一个 `Box`（轨道，4dp 高、`radius = 2`）叠一个 `Box`
  （已行进度）。填充长度是 `value × width`：控件读不到自己的布局宽度（缺口 #3，
  `docs/engine-gaps.md:46-60`，2026-10-31 对账「引擎侧数据已就位、控件仍拿不到」），
  所以和 `Md3Slider::with_width` / `Md3TabBar::new_with(..., width, ...)` 一样**由应用给宽度**。
  进度值由应用给（`Change` 不参与，进度条不产语义变化）。
- **不确定态（indeterminate）阻塞**：需要**逐帧自走的循环动画**。引擎动画是
  「目标驱动、到点落定」（`ui-core/src/anim.rs`，`Entry::settled` 即 `progress >= 1`，`:343-345`），
  没有 `repeat` / `loop`；`is_animating` 只覆盖未落定的实例（`:660-662`）。可行的绕法是
  「每帧把目标往前推一格」，但**空闲时没有帧**：画廊主循环在没有脏区且 `!animating` 时
  `win.wait()` 阻塞等输入（`ui-gallery/examples/gallery/main.rs:399`、`:437`），而 `wait` 只有
  250ms 上限的 poll（`ui-platform-wayland/src/window.rs:1613-1619`），不是定时器。
  这一条与缺口 #13 的「控件不知道自己是第一次出现」是同一个根因族的两个面。
  环形还额外要 §3.3 的图元缺口。
- **环形进度条阻塞（图元缺失）**：`Shape` 只有 `Box` / `Rect` / `Shadow`
  （`ui-core/src/draw.rs:266-295`），`PrimKind` 只有 `Rect/Text/Glyph/Image/Shadow`
  （`draw.rs:438-444`），**没有弧/扇形/路径图元**，渐变只支持两段线性
  （`draw.rs:26-30` `GradientAxis::{Vertical,Horizontal}`）。并且 `grep -rn 'rotate|transform' ui-core`
  为空——**没有旋转/变换矩阵**（进度文档 §4.5 已记「引擎没有变换矩阵，MD3 的缩放+位移无法表达」）。
  所以确定态圆环（一段弧）与旋转不确定态（转动的弧）**都画不出来**。
  唯一可用的近似：`ui-widgets/src/icon_codepoints.rs:3035` 有 `progress_activity`（U+E9D0），
  Material Symbols 自带这个「分段圆环」字形，可用 `Icon` 画静态环 + 用关键帧驱动它的
  `FILL`/轴（不可行，轴只影响可变字体）——**结论：环形等图元或等字形动画，先不做**。
- 波浪（`wave` / `with-wave`）需要曲线绘制，同样缺图元，明确不做。

### 3.3 与既有控件的关系

`docs/engine-gaps.md` §3 与 §4.2 都把 progress 列为「拿不到宽度」的同病控件（「progress/carousel 同理」）。
可复用：状态层/圆角/`spec_ink`/`role` 全部现成；`md.comp.progress-indicator` 需给 `gen_spec.py`
补词表（对象：`active-indicator`、`track`、`stop-indicator`、`""`；轴：`circular`/`linear`）。

外部对齐（通道②）：Flutter `progress_indicator.dart` 的 M3 默认走
`defaults.linearMinHeight`（`progress_indicator.dart:605-606`）与
`_CircularProgressIndicatorPainter`（`:665` 起，`strokeCap = StrokeCap.round`，`:728`）——
「round 端帽」正是 `shape = circular` 的落法；Compose 侧 `CircularProgressIndicator` 用
`drawArc`。**两家都靠弧原语**，我们缺这个原语，这是「阻塞」判据的直接依据。

### 3.4 建议验收

线性页钉：轨道高 == `progress_indicator_spec(...).linear.height`（4）、圆角 == 2（高的一半）；
填充宽 == `value × 声明宽` 且 `value = 0` 时不画（段宽 > 0 才画，沿用 slider 的判据，
进度文档 §4.2）；`value = 1` 时填充右端 == 轨道右端（差 < 0.5）。不确定态与环形**在图元
落地前不进画廊**，避免留下「用矩形凑的圆」这种会误导后来人的样本。

## 4. `indicators/loading-indicator.json`

- tokenSet 只有 `md.comp.loading-indicator`（27 条里 7 条有效）：
  `active-indicator.color = primary`、`active-indicator.size = 38`、
  `container.height = container.width = 48`、`container.shape = circular`；
  `contained` 分支：`container.color = primary-container`、
  `active-indicator.color = on-primary-container`；
  `skipped_deprecated` = `container.color`（非 contained 时容器无填充，即透明）。
- **语义**：这是「按钮在加载时替换自身内容」的那颗 48dp 圆（Expressive 的新部件），
  与 §3 的 `circular` 进度指示是两份不同的令牌集，不要合并。
- 前置：容器方 = 48dp 圆（`Shape::Box` 可做）。里面的 38dp 标记 = 一个**旋转**的弧/刻度环——
  同 §3 的图元缺口，**旋转不可用**。可退化成 `Icon::new(id, "progress_activity", 38, ink)`
  静态显示（不转），那是「有占位但没有反馈」。所以判为**阻塞**，与 §3 合并处理。
- 与既有控件的关系：`contained` 的配色轴 = `fab_menu.rs` 里 `FabMenuColour` 那种
  「一个角色同时决定容器与墨色」的写法可照搬（进度文档 §4.5）。

## 5. `navigation/navigation-bar.json`

### 5.1 令牌面

四个 tokenSet 是**两代规格**：

| tokenSet | 角色 | 关键数值 |
|---|---|---|
| `md.comp.navigation-bar` | 经典底部栏（有完整条目态） | `container.height = 80`、`container.color = surface-container`、`elevation = 3`、`shape = rounded_corners`；`active-indicator = {secondary-container, 64 × 32, circular}`；`icon.size = 24`；`label-text = label-medium`（12 / 行高 16 / 500），**选中加粗到 700**；`active.icon.color = on-secondary-container`、`active.label-text.color = on-surface`；`inactive.icon/label = on-surface-variant`；状态层 0.08 / 0.1；`focus.indicator.thickness = 3`、`outline.offset = -3` |
| `md.comp.nav-bar` | Expressive 顶部/侧栏（**没有态色**） | `container.height = 64`、`between-space`（`item.vertical` = 6）；`item.active-indicator.{height=40,icon-label-space=4,leading-space=16,trailing-space=16}`（horizontal）/ `{height=32,width=56}`（vertical） |
| `md.comp.nav-bar.item.horizontal` / `.vertical` | 上表的两条轴的切片 | 同上 |

- `md.comp.navigation-bar` 的废弃项含 `badge.*` / `large-badge.*`（徽标走 `badges.json` 那份，
  已有 `Md3Badge`）与 `container.shadow-color`、`surface-tint-layer.color`。
- **`nav-bar` 与 `navigation-bar` 不能合并成一张表**：`nav-bar.*` 的颜色挂在
  `active/item`（`active/item.icon.color`）这种「段序打乱」的写法上——正是进度文档 §6.12
  记过的坑（`gen_spec.py` 需要「对象各段按顺序但不连续」的贪心匹配）。
- 规范（通道②）：`md.comp.navigation-bar` 的 80dp / 64×32 指示片与 Flutter
  `_NavigationBarDefaultsM3` 完全同值（`navigation_bar.dart:1383`、`height: 80.0`，
  `indicatorColor = secondaryContainer`、`indicatorShape = StadiumBorder()`、
  `labelPadding = EdgeInsets.only(top: 4)`），`labelBehavior` 三态（alwaysShow / alwaysHide /
  onlyShowSelected，`navigation_bar.dart:342`）在导出里没有令牌，属应用参数。

### 5.2 引擎前置

**现在就能做**：等分槽位靠 `Grow::Split(1)`（缺口 #7 已修）；每段内部是「图标 + 标签」的列，
垂直居中/内衬靠 `Row`/`Column` 的 `cross_align`/`cross_pos`（父参数，design-errors 已钉）。
`icon-label-space = 4` 直接写进 `Column` 的 `gap`。选中态由应用给（与 tabs 一条约定）。
唯一需要参数化的是栏宽（缺口 #3），与 `Md3TabBar::new_with(..., width, ...)` 同处理；
等分之后其实不必知道宽度，只有「指示片宽度取 64dp 定值」不需要读布局，所以不阻塞。

不需要浮层、不需要过渡。选中指示片**要滑过去**才像 Compose，那就落到缺口 #1（动画只驱动
f32）与 #3（行程要栏宽）——但 tabs 已经用「整条栏共用一条 `Overlay` 下划线 + 应用给宽度」
趟过这条路（进度文档 §4.3），照抄即可。

### 5.3 与既有控件的关系

`Md3TabBar`（`src/tab_bar.rs`）与 `secondary_navigation_tab_spec` 是**同一套结构的近亲**
（图标+标签、选中指示片、等分、共用滑动件）。差别：页签有 2dp `active-indicator` 下划线带与
底部 `divider`，导航栏是 64×32 胶囊指示片 + 80dp 栏高 + 选中字重 700。建议**新开 `nav_bar.rs`
而不是给 tab bar 加布尔参数**——两者的字级（label-medium 12 vs 页签 label-large 14）与
指示片几何不同源，混在一张表里会让 §6.14 的「完成判定」变成两份断言互相迁就。

### 5.4 建议验收

画廊页（`Navigation · Bar`）钉：栏高 == `navigation_bar_spec(...).container.height`（80）；
各段等宽（差 < 0.5）；指示片 == 64 × 32 且水平居中于选中段；选中段标签字重 700 而其余 500；
点非选中段上报 `Change::Selected(Some(id))` 且不重报当前段（照 tabs「只有按下另一个页签才上报」，
进度文档 §4.3）；`labelBehavior` 三态在画廊用构建期变体展示（alwaysShow / alwaysHide /
onlyShowSelected）——**只有 `onlyShowSelected` 会让选中段与其余段高度不同**，自检要钉住
栏高恒为 80（防止指示片挤扁布局）。

## 6. `navigation/navigation-rail.json`

- 七个 tokenSet，两份「经典 rail」+ 一份 Expressive `nav-rail` 族：
  `md.comp.navigation-rail`（**条目态齐全**）：`container.width = 80`、`container.color = surface`、
  `elevation = 0`、`active-indicator = {secondary-container, 56 × 32, circular}`、`icon.size = 24`、
  `label-text = label-medium`（12 / 16 / 500，选中 700）；`no-label.active-indicator.height = 56`
  且 `shape = circular`（**无标签时指示片变 56×56 正圆**，与 32×56 的胶囊是两条外观）；
  `md.comp.nav-rail.expanded`：`top-space = 44`、`container.{surface, elevation 0, rounded}`、
  `modal` 分支 `surface-container` + `elevation 3`、`width` 分支 `minimum = 220` / `maximum = 360`；
  `md.comp.nav-rail.collapsed`：`container.width = 96`、`narrow` 分支 `80`、`item.vertical-space = 4`；
  `md.comp.nav-rail.item`：`container.height = 64`（`short` 分支 56）、`header-space-minimum = 40`、
  `active-indicator.leading/trailing-space = 16`、`icon-label-space` vertical 4 / horizontal 8。
- `skipped_deprecated` 一长串：`navigation-rail.badge.*` / `large-badge*`、`menu.*`（rail 顶部
  那颗汉堡按钮的旧令牌）、`label-text.{letter-spacing,font-size,font-family}`（旧拼法）。
  **`navigation-rail.menu.*` 全族废弃**这一点直接关系到 §11（rail 的菜单按钮不是一份独立的
  icon-button 令牌来源，只能按 icon-button 规格现算）。
- 前置：**与 §5 同**（等分不需要，rail 是定宽 80 + 纵向堆叠，用 `Column` + `gap`）→ **可做**。
  `expanded` ↔ `collapsed` 的**宽度过渡**是缺口 #1（尺寸动画要拆 f32）+ #13（进出过渡），
  但「应用给 `expanded: bool`、控件按 f32 插值宽度」正是 switch 的锁存写法
  （`docs/engine-gaps.md:22-29`），可做。
- 与既有控件的关系：条目 = 同一颗「胶囊指示片 + 图标 + 标签」，与 §5 的 nav bar **共用一个
  `NavDestination` 内部件**（`Md3NavItem`）比共用一个控件类更合适；rail 只是它的纵向定宽宿主。
- 验收：宽 == `container.width`（collapsed 96 / narrow 80 / expanded 220–360 由应用给）；
  指示片 == 56×32（无标签变体 == 56×56，自检分别钉住两条，因为 `no-label` 是唯一改指示片
  形状的分支）；`top-space` == 44。

## 7. `navigation/navigation-drawer.json`

- 一个 tokenSet `md.comp.navigation-drawer`（62 条有效）：
  `container.{width = 360, height = 100, shape = rounded}`；`standard` 分支 `surface` + `elevation 0`、
  `modal` 分支 `surface-container-low` + `elevation 1`；条目 `active-indicator.{secondary-container,
  336 × 56, circular}`、`icon.size = 24`、`label-text = label-large`（14 / 20 / 500，选中 700）、
  `headline = title-small`（14 / 20 / 500，`on-surface-variant`）即分组标签、
  `large-badge-label = label-large`（`on-surface-variant`）、`focus.indicator.{secondary, 3, offset -3}`、
  状态层 0.08 / 0.1；`bottom.container.shape`（底部对齐组的圆角）。
  **`scrim.color` / `scrim.opacity` / `divider.color` / `container.color` / `surface-tint-layer.color`
  全部废弃**——遮罩色不能从这份导出取，只能沿用 dialog 已经落地的 `SCRIM_OPACITY` + `spec_ink`
  写法（进度文档 §6.20 正是被这条坑过：常量写了没人用）。
- 前置分两半：
  - **standard（常驻）**：**可做**。它就是 `Md3Scaffold` 的 `nav` 槽（`src/scaffold.rs:10`
    `SCAFFOLD_NAV_W = 80` 是给 rail 的，抽屉要 360）里的一列 `Md3ListItem`。
  - **modal（浮出 + 遮罩）**：**等浮层 O1–O3**。不过遮罩本身可借道模态机制——
    `Decl::modal(ModalTarget::Global)`（`ui-core/src/tree.rs:804`）已经能把节点放到窗口矩形
    （`layout_engine.rs:1255-1257`）并**摆脱祖先裁剪**（`compute_masks` 的
    `base = visual`，`layout_engine.rs:769-773`），walk/hit 把模态子树排最后（`tree.rs:1481-1500`），
    键盘也被模态独占（`view.rs:2599-2620`、`:2628-2640`）。这正是 dialog 已经吃到的能力。
    代价：`modal` 会连带**独占键盘**，而 MD3 的 modal drawer 是**非模态可用**还是模态要按
    场景选——若要做「点外面关掉但不抢键盘」，才真的需要 O3 的 `Change::Dismissed`。
- 与既有控件的关系：条目**直接复用 `Md3ListItem`**（`src/list.rs`，`ListItemParams` 有
  `label` / `leading` / `selected` / `trailing` / `overline`，`list.rs:83-106`）。
  注意**高度不同源**：抽屉的指示片是 56 高（`active-indicator.height = 56`）与
  `md.comp.list` 的 one-line `container.height = 56` 恰好同值但不等义；选中色抽屉用
  `secondary-container`（`active-indicator.color`）而 list 的选中用 `secondaryContainer`
  作为**容器**（进度文档 §4.8）——落地上抽屉条目 = `Md3ListItem` + `selected` 即可，
  但**必须加一条断言**证明两处选中色取到同一个角色（照 chips 那条「绘制与命中同源」的做法）。
- 验收：宽 == `container.width`（360）；`modal` 时遮罩 alpha == 给定常量（防 §6.20 复发）；
  选中行容器 == `secondary-container` 且字重 700；分组标签用 `headline`（title-small）。

## 8. `buttons/split-button.json`

- 9 个 tokenSet：五档尺寸 `md.comp.split-button.{xsmall,small,medium,large,xlarge}` + 四个
  **按钮样式表** `md.comp.button.{filled,elevated,outlined,tonal}`（这四份是来路引用，
  本仓已由 `buttons.json` 生成过 `button_tokens.rs`，`ButtonStyle::{Base,Elevated,Filled,Outlined,Text,Tonal}`，
  `button_tokens.rs:94-103`）。
- 尺寸表给的全是**几何**，不含颜色（颜色来自按钮本体）：
  高度 32/40/56/96/136（与 `button_spec` 同刻度，`button_tokens.rs:217-249`）；
  `between-space = 2`；`container.shape = circular`；
  `leading-button.leading/trailing-space`：12/10、16/12、24/24、48/48、64/64；
  `trailing-button.leading/trailing-space`：13、13、15、29、43；`trailing-button.icon.size`：
  22/22/26/38/50；**内角随交互变化**：`inner-corner.corner-size` = 4（hover 12 / press 12，
  xlarge 12/20，large 8/20），`outer-corner.corner-size = 50`（即胶囊端）。
  `inner-corner/selected/trailing-button.corner-size = 50`——菜单打开时尾段变成整圆端。
- 前置：**表面可做，菜单阻塞**。
  - 两个半段 + 共用胶囊 + 内角随 hover/press/展开变化：全部靠 `GroupPosition::corners`
    （`ui-widgets/src/group.rs:41-50`）加「内角半径来自 `inner-corner.corner-size`、
    外角 = 高/2」即可，`Md3ButtonGroup` 已经是这套做法（`button_group.rs:112-113` 把
    `corner_size.min(height*0.5)` 传进去）。**不需要新引擎能力**。
  - 尾段点击要**开一个菜单**：需要浮层 O1–O3（§1.2 已证一行未落地）。
    `menu_style.rs` 那条兜底路只支持文字行 + 只在右键时开（`view.rs:2208-2223` 的
    `PointerButton::Secondary` 分支），**没有「点某个节点就开菜单」的入口**——`OpenMenu::open`
    是 `pub(crate)`（`menu.rs:84`），`menu` 字段私有（`view.rs:540`）。
  - 缺口 #8「控件内部分区命中」在这里**不阻塞**：split button 是两个独立 `Decl` 子节点，
    各自带自己的 id 与 `Sense::CLICK`，不是「一颗胶囊里两个热区」。**但**这依赖缺口 #6
    的另一面：`collect_changes` 会给每个 `Decl` 子节点各调一次 `change()`
    （`view.rs:241-250`），所以两个子按钮必须**作为 element 子节点**挂进去（dialog 的 `decl`
    里按钮是 `Decl` 而非 element，因此它只能在构建期读 `cx.state(action_id).clicked`，
    进度文档 §4.9）——照 dialog 的做法即可，记清楚「谁执行 change」。
- 与既有控件的关系：`Md3Button`（本体色/尺寸/状态层）+ `Md3ButtonGroup`（圆角派发的现成范式）。
  新写的是「内角按交互变半径」这条规则，以及尾段的 `expand_more` 图标
  （`ui-widgets/src/icon_codepoints.rs` 有 `expand_more` U+E5CF）。
- 验收：五档高度逐一 == 导出；内角半径 == `inner-corner.corner-size`（静息/悬停/按下/展开
  四个值都要断言，**这是本控件唯一容易被写死的数**）；外角 == 高/2；尾段宽 ==
  `2 × trailing-space`（无标签，正方形），点主段只发 `Activated`、点尾段发一条独立的
  `Change::Selected(尾段 id)`（两者分不开就会「点主按钮也弹菜单」）。

## 9. `toolbars.json`

- 6 个 tokenSet，**两代 + 两个主题**：
  | tokenSet | 关键数值 |
  |---|---|
  | `md.comp.toolbar.docked` | `container.height = 64`、`leading/trailing-space = 16`、`min-spacing = 4`、`max-spacing = 32`、`shape = rounded`（`container.color` 废弃） |
  | `md.comp.toolbar.floating` | `container.elevation = 6`、`leading/trailing-space = 8`、`between-space = 4`、`shape = circular`；`horizontal.container.{height=64, external-space=16}`、`vertical.container.{width=64, external-space=24}`（`container.height`、`external-padding` 废弃） |
  | `md.comp.toolbar.standard` | 成员按钮态表：`container.color = surface-container`、`shape = circular`、`icon/label-text = on-surface-variant`；`button.selected.container.color = secondary-container` + `on-secondary-container`；状态层 0.08 / 0.1；禁用 0.38 / 0.12 |
  | `md.comp.toolbar.vibrant` | 同结构，`container.color = primary-container`、墨 `on-primary-container`，选中容器 `surface-container` |
  | `md.comp.toolbar.floating.fab` | `container.{56 × 56, rounded}`、`elevation 1`（`medium` 80×80 + elevation 3 + icon 28）、`between-space = 8`、`icon.size = 24`、standard `secondary-container` / vibrant `tertiary-container` |
  | `md.comp.bottom-app-bar` | 只有 4 条：`container.{surface-container, elevation 3, height 80, rounded}`（`with-fab.container.height`、`surface-tint-layer.color` 废弃） |
- **成员是 icon-button，而本仓没有 icon-button 的生成表**：`toolbar.standard` 只给
  `icon.color` / `label-text.color` / `state-layer`，**不给 48dp 触控区、不给 40dp 状态层盘、
  不给焦点环**。这正是进度文档 §4.12 记过的坑（app-bar 的槽位「48dp 触控区与 40dp 盘是
  MD3 icon button 的规格；本仓没有 icon-button 的令牌导出」）。**该断言现在过期**：
  `tokens/buttons/icon-buttons.json` 在（`md.comp.icon-button` 268 条 + `filled`/`outlined`/
  `tonal`/五档尺寸），只是没生成表。做 toolbar 之前先把 icon-button 的表生成出来，
  app-bar 与 toolbar 两处就能共用同一颗槽位。
- 前置：**可做**（不需要浮层、不需要过渡）；成员溢出成菜单时才要浮层（规范原文
  「When actions don't fit in a toolbar, add a **menu**」，
  https://m3.material.io/components/toolbars/guidelines）。
- 与既有控件的关系：`md.comp.bottom-app-bar` 与 `navigation-bar` **高度同值 80 但不同源**
  （一个是容器高、一个是栏高 + 指示片）；`toolbar` 与 `app_bar` 的关系见 §10.4。
- 验收：docked 64 / floating 64（竖版 64 宽）；floating 圆角 == 高/2（`shape = circular`）；
  FAB 变体 56×56 / medium 80×80 与图标 24 / 28 成对断言；成员槽 48 / 盘 40 取自 icon-button 表
  （生成之后），并断言与 `app_bar.rs:20-24` 的 `APP_BAR_SLOT = 48` / `APP_BAR_ICON = 24` 相等
  （**两处必须同源**，否则 app-bar 换 token 时 toolbar 不会跟着变）。

## 10. menus 与 app-bar / 右键菜单：正面回答用户的问题

### 10.1 ① 规范里 app-bar 溢出菜单是不是同一个 menu 组件

**是，而且是同一个 `Menus` 组件的使用场景之一，不是独立组件。**

- 规范把两者并列为同一组件的用途：「Use menus in situations that need extra actions, like:
  **Overflow menus** / Text field dropdown menus / Select menus / **Context menus**」
  — https://m3.material.io/components/menus/guidelines
- **Context menu 有专门小节且明确是同一个组件**：「Context menus provide a list of additional
  actions a person can take on an item. A secondary click, like a right-click on a mouse or a
  two-finger tap on a trackpad, opens a context menu.」— 同上 URL
- 触发方式同一条：「A menu should open when a person: Selects an element, such as an icon,
  button, or text field / Performs a specific action to trigger the menu, like **right-click**
  or press-and-hold」— 同上
- M3 已把 M2 的 dropdown / exposed dropdown 合成一个组件：「Dropdown menu and exposed dropdown
  menu are now both referred to as menu, since they differ only in the element which opens the
  menu surface」— https://m3.material.io/components/menus/overview
- **最直接的一条**：Toolbar 规范里「溢出」二字直接链回 Menus 组件——原文「When actions don't
  fit in a toolbar, add a **menu**」，该 "menu" 的 `link-url = /m3/pages/menus/overview`
  — https://m3.material.io/components/toolbars/guidelines
- App bar 侧只有用量建议、**没有**独立组件条目：「If the product has many actions, place those
  in a toolbar. Avoid placing an overflow menu in the app bar when possible.」以及「Resizing may
  cause actions at the trailing edge of the app bar to collapse into an overflow menu at smaller
  breakpoints.」— https://m3.material.io/components/app-bars/guidelines
  （注：M3 Expressive 已把 "top app bar" 改名 "App bars"，`.../top-app-bar/overview` 现 302 到
  `.../app-bars/overview`；规范里**不存在** "top app bar overflow menu" 这一组件条目。）

两份官方实现也只有一个 menu 原语：material-web 的 `md-menu` 内部类是 `Menu extends LitElement`
（`menu/internal/menu.ts:88`），渲染成 `.menu` > `md-elevation` + `.items` > slot
（`:500-511`），`md-menu-item` **是 list item 的变体**（`menu/menu-item.ts:26-30` 注释明写
"menu items are list items"，并要求带 `md-list-item` 属性，`internal/menu.ts:413`）；
`md-top-app-bar` 在 material-web 里根本不存在（repo 根目录无 `top-app-bar`，
https://github.com/material-components/material-web）。Compose 侧 `TopAppBar(actions:)` 只收一个
action row、不含任何菜单逻辑（`AppBar.kt:170-174`），官方示例正是
`IconButton { Icon(Icons.Default.MoreVert, "More options") }` + `DropdownMenu { … }`
（https://developer.android.com/develop/ui/compose/components/menu）。**差异只在锚点与摆放，
不在条目与表面结构**（material-web 的 `anchor` / `SurfacePositionController`，
Compose 的 `MenuAnchorPosition`）——这条决定了：把差异做成 menu 的**锚点参数**，而不是第二个控件类型。

### 10.2 ② 本仓库 `src/app_bar.rs` 现在的溢出/菜单是怎么做的

**根本不做溢出，也不走 `menu_style`。** 证据（`grep -n 'menu\|Menu\|overflow' app_bar.rs` = 空）：

- 尾部动作是 `AppBarAction { id, icon }`（`app_bar.rs:118-124`），图标由应用用
  `fn(Id, Color) -> Decl` 现画（`Md3AppBar::new_with` 里 `icon_slot(...)`，`:275-287`）；
- 布局把槽位**硬算进标题宽度**：`slots = width - title_left - APP_BAR_TITLE_INSET`、
  `inline_slots = slots - actions.len() * APP_BAR_SLOT - base.root.trailing_space`（`:226-228`）——
  动作数一多就把标题挤窄，**没有任何「放不下就折进溢出」的分支**；
- `app-bar.json` 里也**没有** overflow / menu / action 相关令牌（对象统计里只有
  `container` / `on-scroll` / `title` / `subtitle` / `leading-icon` / `trailing-icon` /
  `avatar` / `icon` / `leading-space` / `trailing-space` / `icon-button-space`）——
  **导出本身就不含溢出规格**。
- 画廊的 `pages/app_bar.rs:33` 那个 `app_bar_menu` 只是**前置槽位的汉堡图标**
  （`AppBarLeading::Icon(app_bar_menu)`，`:198`），不是溢出菜单。

### 10.3 ③ 右键菜单目前算不算「已经在用 menus.json」

**语义上「配置已就绪」，运行时「没有在用」。**

- 取令牌这一半**确实做了**：`menu_style.rs:9` 引 `crate::spec::menus_spec`，`:24` 与 `:27` 两次
  解析（容器读无条目态、行读 `menu_item + standard` 态），并把 `container.color` /
  `menu-item.label-text.color` / `menu-item.state-layer.{color,opacity}` /
  `label-text.size` / `menu-item.height` / `leading_space` / `top_space` / `container.elevation`
  灌进 `MenuStyle`（`:29-47`）。
- **但没人把它交给引擎**：`AppLoop::set_menu_style`（`view.rs:642`）**零调用方**，
  `Md3Kit::menu_style`（`style.rs:361`）**零调用方**（`grep` 已复核，见 §1.2）。
  所以右键打开的菜单画的是 `MenuStyle::default()`（`view.rs:636`）：34dp 行高、8dp 圆角、
  1px 白描边——`row_height` 应为 44、`radius` 应为 4、`border` 应为 `None`（MD3 用阴影界定，
  `menu_style.rs:37-39` 已经这样写了）。
- 声明入口是有的：`Decl::menu(Vec<MenuItem>)`（`tree.rs:835`），消费者只有
  `text_field.rs:734` 与画廊 `pages/text_field.rs:174`。触发只认右键
  （`view.rs:2208-2223`），`menu_for` 沿命中祖先向上找第一个非空 `params.menu`
  （`view.rs:1617-1642`），并在文本节点上**自动追加**引擎自己的四行（`select all / copy / cut /
  paste`，`view.rs:1636-1641`，常量 `MENU_SELECT_ALL` 等在 `view.rs:192-195`）。

### 10.4 ④ 结论：应否让 app-bar 溢出复用同一个 menu 实现，复用哪一层

**应当复用，而且复用同一个 menu 的「内容 + 外观」这一层；引擎兜底菜单只配当外观，不配当宿主。**

理由，按三条通道：

1. **规范（通道②）**：溢出与右键是同一个 `Menus` 组件的两个触发场景（§10.1），
   material-web / Compose 都只给一个 menu 原语。**再造第二个 menu 控件是违背规范。**
2. **本仓库事实（通道①可验证）**：兜底菜单是 `MenuItem { id, label, enabled }` 三字段
   （`ui-core/src/menu.rs:11-16`），`paint_menu` 只画「表面 + 每行一条文字 + 悬停矩形」
   （`view.rs:1815-1850`）——**没有前置图标（规范要求 leading/trailing icon 24dp）、没有选中态、
   没有分隔线、没有单选/复选行**。而 app-bar 溢出需要 `check` 行与 `divider` 的那一刻，
   兜底菜单就只能被继续加字段，这与 `popover-tooltip-design.md` §0.2「菜单那种『游离于树外』的
   旧路不必复制」的判断一致。
3. **第一性原理（通道③）**：兜底菜单**不在树里**（`view.rs:1798-1800` 画在 `paint_tree` 之后），
   所以它拿不到布局、焦点、命中继承，也就**不可能**长出「行里有真控件」的东西；
   而 MD3 的菜单项按规范就是 list item 的变体（§10.1），**本来就该在树里**。

**具体落法（不动代码，只给方向）**：

- **短期（无引擎改动）**：把 `menu_style()` 接线（在 app 构造 `AppLoop` 后 `set_menu_style(kit.menu_style(cx))`），
  右键菜单立刻拿到 MD3 外观；app-bar 溢出的动作行用 `Decl::menu(...)` 声明，
  与文本字段的四行同一条机制（`menu_for` 已经是「祖先链 + 追加」的合并逻辑，`view.rs:1636-1641`）。
  溢出**内容**（More options 图标 + 一行行动作）用兜底菜单足够，**前提是**溢出项只有文字。
- **中期（等浮层 O1–O3）**：app-bar 溢出改成控件级 menu——宿主是 `Popover`（锚在 `more_vert`
  槽位、`match_anchor_width = false`、翻转 + 夹进视口），内容是 `Md3ListItem` 家族的菜单行。
  此时**兜底菜单继续服务右键**（引擎自带、无需应用声明、Esc/外按自动关，这套已经跑通），
  两者共享的是**同一份 `md.comp.menus` 取值函数**（`menus_spec`），不是同一份绘制代码。
  这条与 `popover-tooltip-design.md` §6 表里「右键菜单：保持树外兜底，`MenuStyle` 换皮够用；
  控件级菜单可另走 popover」和 §7「控件自带菜单（如 app-bar 溢出）若需要富内容，用 popover
  宿主而非扩展兜底菜单」完全一致——**本节的独立调研支持了该文的设计决定**，不是反过来引用它当依据。
- **必须同源的一条**：无论走哪条路，`radius / row_height / pad_x / pad_y / 墨色 / 状态层`
  只能有一个解析点。建议在 `menu_style.rs` 旁边补一个 `menu_metrics()`（返回同一批数值的结构），
  兜底菜单与控件级菜单都从它取值；否则会出「右键菜单 44dp、溢出菜单 48dp」这种两套真相。

**`toolbars` 与 `app_bar` 的关系**（同题顺带）：规范里两者是**分工**而非父子——
「Use a menu to show a temporary set of actions. To show actions on screen at all times,
use a **toolbar** instead」（https://m3.material.io/components/menus/overview）。
本仓的 `app_bar`（`md.comp.app-bar`，顶部标题栏，带折叠/on-scroll）与 `toolbars`
（`md.comp.toolbar.*`，一组动作的容器，可浮动/停靠/横向/纵向）**是两个部件**：
toolbar 是「成员 = icon-button 的一条胶囊/矩形容器」，app bar 是「槽位 + 标题 + 动作」的
顶部栏。可共享的只有**那颗 icon-button 槽位**（见 §9 的生成表建议），不要合并成一个控件。

## 11. `snackbar.json`

### 11.1 令牌面

单 tokenSet `md.comp.snackbar`（74 条里 40 条有效）：

- 容器：`container.color = inverse-surface`、`container.elevation = 6.0`、
  `container.shadow-color = shadow`、`container.shape = rounded_corners`；
  高度是**分支**而非基值：`with-single-line.container.height = 48`、
  `with-two-lines.container.height = 68`。
- 文本：`supporting-text = body-medium`（14 / 行高 20 / 400，`inverse-on-surface`）；
  动作 `action.label-text = label-large`（14 / 20 / 500，`inverse-primary`）——
  注意 **`label-text` 只出现在 `action` 分支下**，正文叫 `supporting-text`，别按别的控件的习惯找 `label-text`。
- 图标：`icon.size = 24`、`icon.color = inverse-on-surface`，带自己的
  `icon.{hover,focus,pressed}.state-layer.{color,opacity}`（24dp 标记 + 40dp 盘是 icon-button 规格，
  导出这里没给盘）。
- 动作态：`action.{hover,focus,pressed}.state-layer.{color = inverse-primary, opacity = 0.08/0.1/0.1}`。
- `UNKNOWN_STATES` = `action`、`icon`、`with-single-line`、`with-two-lines`——
  前两个要进 `VARIANT_GROUPS`（`("slot", ["action","icon"])` 之类），后两个是一条
  `("lines", ["with-single-line","with-two-lines"])` 轴，否则 `gen_spec.py` 会把它们当散状态叠。
- **导出里没有任何宽度 / padding / margin / 时长 / 缓动令牌**：`snackbar.json` 的
  `motion` / `duration` / `spring` 命中数为 **0**（逐文件核过）。所以 §11.2 的数值全部来自外部权威。

### 11.2 规范与参考实现（通道②）

- 圆角 **4dp**：`md.sys.shape.corner.extra-small` —
  https://github.com/material-components/material-web `tokens/versions/latest/sass/_md-comp-snackbar.scss:67`
  + `_md-sys-shape.scss:45`。**不是 8dp**（MDC 旧值是 4，一致）。
- 高度 48 / 68、图标 24、elevation `level3` = 6：**与本仓 JSON 完全一致**
  （`_md-comp-snackbar.scss:25,27,29,63`；Compose `tokens/SnackbarTokens.kt:42-43` 同值）。
  规范正文另一处写「expand vertically from **48dp to 64dp**」与 token 的 48/68 冲突
  （https://m3.material.io/components/snackbar/guidelines）——**以导出为准**（进度文档通篇的取数纪律）。
- 垂直内衬 14dp、水平 16（浮动）/ 24（固定）：MDC 推导「14px top/bottom padding needed to make
  the height 48px」`packages/mdc-snackbar/_snackbar.scss:212-214`；Flutter
  `snack_bar.dart:27` `const double _singleLineVerticalPadding = 14.0`，
  默认 `padding` 文档写明「Top and bottom paddings are 14; Left padding is 24 if fixed, 16 if
  floating」（`snack_bar.dart:342-353`）。
- 宽度上下限**不在 M3 令牌集**：material-web 自补 `--min-width: 344px / --max-width: 672px`
  并留注释 `// EDIT: No upstream Spec token.`（`labs/gb/components/snackbar/_snackbar-tokens.scss:49-55`）；
  Compose 用 `ContainerMaxWidth = 600.dp`（`snack_bar.dart` 对应 `Snackbar.kt:603`）。
  **→ 本仓取哪个值要用户拍板；[未证实] 规范数值。**
- 时长：规范只给「**4–10 seconds**」且「depending on platform」
  （https://m3.material.io/components/snackbar/guidelines、/accessibility）；
  **有 action 就不该自动消失**：「Snackbars with actions shouldn't auto-dismiss.」
  （/accessibility）；MDC 默认 5000ms（`packages/mdc-snackbar/constants.ts:46-49`）；
  Compose `SnackbarDuration.Short = 4000L / Long = 10000L / Indefinite`，默认
  `actionLabel == null ? Short : Indefinite`（`SnackbarHost.kt:286-306, 104-105`）。
  **pause on hover：规范未提及、Compose 未实现** — 不做，[未证实]。
- 队列：「**Only one snackbarmay be displayed at a time**」+「Consecutive snackbars must appear
  one at a time」，且新的可立即替换旧的；并排明确禁止
  （https://m3.material.io/components/snackbar/guidelines）。Compose 用单一
  `currentSnackbarData` + `mutex` 串行（`SnackbarHost.kt:104-231`）。
  **→ 队列归应用**（本仓契约：控件不藏状态，进度文档 §6.22）。
- 进出动画：M3 **没有 snackbar 专用 motion token**（`md.sys.motion.duration.default4` 这类
  不存在，sys motion 只有 short/medium/long/extra-long 四族，`_md-sys-motion.scss:18-107`）。
  实现值：materialWeb enter **150ms `cubic-bezier(0,0,0.2,1)`**（= `easing.standard.decelerate`）、
  exit **75ms `cubic-bezier(0.4,0,1,1)`**（standard.accelerate），enter 用 `scaleY(0)→none`
  （origin `center bottom`），**exit 是纯淡出**；`prefers-reduced-motion` → 0.001ms
  （`labs/gb/components/snackbar/snackbar.scss:100-143`）。MDC 同值 150/75
  （`mdc-snackbar/constants.ts:52-53`）。Flutter 的 M3 写法不同：入场高度动画
  `Curves.easeInOutQuart`（`_snackBarM3HeightCurve`，`snack_bar.dart:31`）+ 淡入
  `Interval(0.4, 0.6, easeInCirc)`（`:34`），退场**直接淡**（`reverseCurve: Threshold(0.0)`，`:585-592`），
  总时长 `_snackBarTransitionDuration = 250ms`（`:28`）。
- 位置：规范只给「底部、前置对齐、可上移避开 FAB/toolbar、**应出现在 FAB 之上**、不贴单边、
  不挡导航」（guidelines）；数字来自实现——Web `--bottom-offset: 16px`（`_snackbar-tokens.scss:52`）、
  MDC `viewport-margin-narrow 8 / wide 24`、`mobile-breakpoint 480`
  （`_snackbar-theme.scss:60-68`）、Flutter `insetPadding = EdgeInsets.fromLTRB(15,5,15,10)`
  （`snack_bar.dart:921`）、**Compose Scaffold 抬到 FAB 上方、间距 16dp**
  （`Scaffold.kt:242-251, 355`：`snackbarOffsetFromBottom = snackbarHeight + (fabOffset ?: bottomBarHeight)`，
  `FabSpacing = 16.dp`）。
- 无遮罩、不阻塞交互：「appear without warning, but they **don't block** users from interacting
  with page content」（guidelines）——**spec 里没有 scrim 概念**，别给 snackbar 加遮罩。

### 11.3 引擎前置

- **摆放：现在就能做，不必等 O1。** 引擎已经有两块拼图：
  ① `Position::Fixed` + `Decl::fixed()`（`ui-core/src/layout.rs:212`、`tree.rs:602`）的包含块
  **固定为根节点矩形**（`layout_engine.rs:1546-1551`），`inset(bottom: 16, left/right: 15)` 直接
  钉底部；② 默认 `Clip::VISIBLE`（`layout_engine.rs:392-417`，`Overflow::Visible` 是 `#[default]`），
  内容列没声明裁剪时浮层不被裁。**把 snackbar 声明在窗口根上**（与进度文档 §4.10
  「模态必须挂在窗口根上」同一条纪律）即可越出内容列。
  真正需要 O1 的时刻是「锚到某个节点」（snackbar 不锚节点，只锚视口）——所以 §0 判「可做」。
- **不抢输入**：根节点若带 `Sense::CLICK` 会吞掉整窗点击；snackbar 的宿主根应带
  `Sense::PASS_THROUGH`（`ui-core/src/sense.rs:39`），`hit_behavior_of` 会返回
  `HitBehavior::PassThrough`（`view.rs:560-562`），`topmost_consumer` 只认 `Opaque`
  （`ui-core/src/hit.rs:136-138`）→ 点得穿。**这条与 text field 的标签层踩的是同一机制**
  （design-errors「装饰浮层必须 `Sense::PASS_THROUGH`」）。
  **不要用 `Decl::modal(...)`**：`modal_root()` 会把键盘独占给最后一个模态
  （`tree.rs:1557-1559`、`view.rs:2599-2620` 的 `focusables` 按 `modal_root` 过滤），
  snackbar 是**非模态**，用 modal 会把焦点圈锁死在它里面。这是本仓库代码事实推出来的，
  不是规范里的话。
- **进出过渡：缺口 #13**（`docs/engine-gaps.md:185-194`，「控件在被构建出来的那一帧就已经是
  最终外观」）。绕法已在 `fab_menu.rs` 落地过：**控件常驻树内、用 `animate` 驱动 0→1 的
  `open` 标志、不画关闭态的内容**（`fab_menu.rs:256` `root.animate(id.with(PROGRESS), f32::from(open), OPEN)`，
  进度文档 §4.5「`open` 淡到 0 时不画菜单项」）。snackbar 照此即可拿到淡入/淡出；
  **缩放/位移无法表达**（无变换矩阵，§3.2），与 FAB 菜单一样按「淡入是近似」记录。
- **自动消失的延时：可做，但要验证**。引擎没有定时器（§3.2 已证：空闲即 `wait()` 阻塞），
  但 `AnimSpec.delay` + 一条 4s 的 `animate` 实例**本身就是一段会持续产生帧的运动**
  （`is_animating` → 每帧 present，`view.rs:1107`），落定时 `status().settled`
  （`ui-core/src/anim.rs:368-374`、`cx.rs:558`）就能当作「到点」。
  **这条我没有跑过，属「需运行核实」**；若成立，则 snackbar 的 4s/10s 与 tooltip 的 1.5s
  消失共用同一机制。反过来说：零时长 + delay 的实例是「延迟写入」
  （`anim.rs:282-286`「零时长的一段就是一次写入：延迟一过就落地」），也能当一次性闹钟。
- 宽度上下限（344/672 或 600）**[未证实]**，需要一次裁定；本仓可先按 `fixed(width)` 由应用给
  （与 dialog 的 `width` 参数同一约定，进度文档 §4.9）。

### 11.4 与既有控件的关系

- 动作按钮**直接复用 `Md3Button(Text, Base)`**——dialog 已经这么做了（进度文档 §4.9），
  并且踩过的坑要一起继承：**动作是 `Decl` 不是 element，所以 `change()` 不执行**，
  必须在构建期读 `cx.state(action_id).clicked` 记进 `fired`，再在 `change()` 里翻成
  `Change::Selected(Some(action_id))`。
- 图标 24dp 用 `Icon`（`ui-widgets/src/icon.rs:58`），墨色 `inverse-on-surface`
  （`ui-widgets-md3/src/pack.rs:35-36,46` 已注册 `inverseSurface` / `inverseOnSurface` /
  `inversePrimary` 三个角色——**snackbar 需要的三个角色本仓都有**，这点可当场核）。
- 与 `Md3Scaffold` 的关系：scaffold 目前有 `inline`（48dp，`scaffold.rs:12`）与 `overlay`
  （`scaffold.rs:63-66`）两个槽，**snackbar 应该占 `overlay`**；`SCAFFOLD_INLINE_H = 48` 恰好
  是 snackbar 的单行高，别把两者混用同一个槽。

### 11.5 建议验收

画廊页 `Overlays · Snackbar`：钉单行高 == 48、两行 == 68（两条都从 `spec` 取，别写字面量）；
圆角 == 4；水平内衬 16（浮动）；`supporting-text` 用 `body-medium`、action 用 `label-large`
且墨色 == `inverse-primary`（照 dialog 那条「动作标签与容器取色一致」的断言写法，进度文档 §4.9）；
底距 == 声明的 inset（16）；**同一时刻只有一个**：连开两条时只有一条在树里可见
（自检断言第二条出现时第一条的 `opacity` 实例为 0）；有 action 时**不自动消失**
（断言 5s 后 `visible` 仍为真）。

## 12. `tooltips.json`

### 12.1 令牌面

两个 tokenSet：

- `md.comp.plain-tooltip`（9 条有效）：`container.color = inverse-surface`、
  `container.shape = rounded_corners`、`supporting-text = body-small`
  （`size = 12`、`line-height = 16`、`tracking = 0.1`、`weight = 400`、`inverse-on-surface`）。
  **没有 elevation、没有 padding、没有尺寸、没有延时**——整个 tokenSet 只有颜色/形状/文字
  （material-web 侧同样如此，`_md-comp-plain-tooltip.scss` 全 45 行只有这四组），
  即 **plain = 0dp elevation**（按令牌缺失推得）。
- `md.comp.rich-tooltip`（34 条有效）：`container.color = surface-container`、
  `container.elevation = 3.0`、`shadow-color = shadow`、`container.shape = rounded_corners`；
  `subhead`（对象为空 = 控件自身）= `title-small`（14 / 20 / 500，`on-surface-variant`）；
  `supporting-text` = `body-medium`（14 / 20 / 400，`on-surface-variant`）；
  `action.label-text` = `label-large`（14 / 20 / 500，`primary`）+
  `action.{hover,focus,pressed}.state-layer.{primary, 0.08/0.1/0.1}`；
  `skipped_deprecated` = `container.surface-tint-layer.color`。

**形状名换算**（两份导出都只给 `rounded_corners` 这个名字，不给数值）：
`plain` 的 `rounded_corners` 是 `corner.extra-small` = **4dp**；`rich` 的是 `corner.medium` =
**12dp**（`_md-comp-plain-tooltip.scss:25` / `_md-comp-rich-tooltip.scss:54-61` +
`_md-sys-shape.scss:31,45`；Compose 一致：`PlainTooltipTokens.ContainerShape = CornerExtraSmall`、
`RichTooltipTokens.ContainerShape = CornerMedium`）。**同一个名字对应两个数值**——
本仓已有先例是把半径写成控件本地常量（`menu_style.rs:13-14` 的 `RADIUS`、`chip.rs:27-28`、
`carousel.rs:31`），照此办理，并在两处分别注释各自的 shape key。

### 12.2 规范与参考实现（通道②）

- 尺寸 / 内衬（**实现值，不是 spec token**）：min 40×24dp、max width 200dp、
  padding 水平 8 / 垂直 4 — MDC `packages/mdc-tooltip/_tooltip.scss:36-60`、
  `constants.ts:44-47`（`MIN_WIDTH 40 / MIN_HEIGHT 24 / MAX_WIDTH 200`）；
  Compose 同值：`TooltipMinWidth = 40.dp`、`TooltipMinHeight = 24.dp`、
  `PlainTooltipHorizontalPadding = 8.dp`、`PlainTooltipVerticalPadding = 4.dp`、
  `plainTooltipMaxWidth = 200.dp`、`SpacingBetweenTooltipAndAnchor = 4.dp`
  （`Tooltip.kt:1448-1454`、`:535`）。Flutter 的桌面默认同一批：
  `_getDefaultTooltipHeight() → 24.0`（linux/macOS/windows）、`_getDefaultPadding() →
  horizontal 8 / vertical 4`（`tooltip.dart` 内两处 `switch (platform)`）。
- rich 的尺寸：max width **320dp**、水平内衬 16、subhead 首行基线 28、subhead→正文 24、
  正文底内衬 16、动作最小高 36、动作底内衬 8 — Compose `Tooltip.kt:538, 1444-1466`
  （`richTooltipMaxWidth = 320.dp`、`HeightToSubheadFirstLine = 28.dp`、
  `HeightFromSubheadToTextFirstLine = 24.dp`、`ActionLabelMinHeight = 36.dp`）。
  rich 的 max-height **没有常量 [未证实]**。MDC 的 rich corner 写 4px，是 M2 遗留，
  **与新令牌的 12dp 冲突，不照搬**。
- 内容上限：「Rich tooltips can have up to two text buttons」、subhead「ideally to one line」
  （https://m3.material.io/components/tooltips/guidelines）；overview 说 rich 可含
  「optional title, link, and buttons」（https://m3.material.io/components/tooltips/overview）。
- **停留时长：规范明确 1.5 秒**——「Both plain and rich tooltips disappear **1.5 seconds**
  after navigating away from the target region.」+「Triggering a new tooltip immediately closes
  any other open tooltip.」+「Only display one tooltip at a time.」+「Tooltips **shouldn't cover
  the parent element**」— 同上 guidelines URL。**「10 秒」不在规范里**（不要按 10s 实现）。
- **出现延时：规范正文没有任何 delay 数字**。500ms / 1000ms 都**不是 spec 结论**
  — **[未证实]**。实现值：MDC `SHOW_DELAY_MS 500 / HIDE_DELAY_MS 600`
  （`mdc-tooltip/constants.ts:42-43`）；Flutter `_defaultWaitDuration = Duration.zero`、
  `_defaultShowDuration = 1500ms`、`_defaultExitDuration = 100ms`（`tooltip.dart:392-394`）。
- persistent rich：「remain active even when leaving the target region. They only disappear once
  a person interacts with another UI element. **Hovering doesn't trigger** the tooltip.」触发为
  clicked 或 page load；「Avoid using persistent rich tooltips on icon buttons」— guidelines URL。
- 触发方式：「hover on the parent element on desktop, or **tap and hold** the element on mobile」
  — guidelines URL。

### 12.3 引擎前置

- **摆位可做**：`Anchor::Pointer` 想要的「跟随指针」，`Cx::core().pointer()` 已经给出
  （`ui-core/src/cx.rs:602`，`ui-widgets/src/cx.rs:90` 的 `core()` 暴露它；`NodeCx` 侧
  `WidgetState::local` 在 `change()` 里按本节点重算，`view.rs:235-240`）。
  配合 `fixed()` + `inset` 即可贴指针摆放（§11.3 同一条机制）。
  **但**「越出被裁祖先 + flip + clamp」是 O1，`OverlayPlacement`/`Anchor` 在代码里一行都没有
  （§1.2 已 `grep` 核实）。用 `fixed` 只能钉到**视口**坐标，算不出「锚点下方 4dp、放不下就翻到上方」——
  那需要锚点矩形，而**控件读不到别的节点的矩形**（缺口 #3：`decl()` 拿不到布局结果，
  `docs/engine-gaps.md:46-60`；`Cx` 上没有 `rect_of`/`measure_text`，`grep` 空）。
  → **plain tooltip 的锚点摆位等 O1**；若允许「应用把锚点矩形当参数传进来」（tabs / carousel /
  app-bar 全是这个套路，进度文档 §4.3/§4.12/§4.13），**现在就做得成**。
- **延时是真正的阻塞点**：hover 时长 500ms、离开后停留 1.5s 都需要**到点产生一帧**。
  `popover-tooltip-design.md` §4 已给出建议（「应用侧用一个 `f64` 时间戳自己攒延时」），
  并明确「延时用 `cx.animate` + `AnimSpec.delay` 做不到（那是插值不是触发）」。
  本节对这条做一次**修正性核实**：`AnimSpec.delay` 确实把「落地」推到 delay 之后
  （`anim.rs:281`），且 `is_animating` 会把「待发的延迟」也算作运动（`view.rs:4462` 的测试
  「a pending delay is still motion」就是这个断言），因此**用一条 `animate` 实例当闹钟在机制上成立**
  ——该文那句「做不到」只描述了「插值不是触发」，没考虑「settled 当作触发」。
  这条**仍需跑一次才能作数**（同 §11.3），我不把它当结论；若成立，则 tooltip 延时不再是硬阻塞。
- 键盘 / 焦点：tooltip 内容无交互（`Sense::NONE`，`sense.rs` 有定义），不该进命中
  （`hit_behavior_of` 对 `Sense::NONE` 的**非文本绘制节点**仍返回 `Opaque`，
  `view.rs:570`——所以必须显式带 `Sense::PASS_THROUGH`，否则 tooltip 会吞掉它身下的点击；
  这就是 design-errors 里那条「命中落在标签行」的同一条坑）。
- `Change::HoverChanged`（O4 建议）：**代码里不存在**（`grep` 空，`Change` 变体见
  `ui-core/src/change.rs:17-75`）。

### 12.4 与既有控件的关系

- rich tooltip 的两个动作**复用 `Md3Button(Text, Base)`**，与 dialog / snackbar 同一套
  「构建期读 `clicked`」的接法（进度文档 §4.9）。
- 与 `shortcut_field` / `search_bar` 无重叠。`ui-widgets/src/icon_codepoints.rs:3807` 有
  `tooltip` 码点，但那是**表示 tooltip 的图标**，不是 tooltip 本体，别拿它当实现。

### 12.5 建议验收

画廊页（等 O1/O4 之后，`popover-tooltip-design.md` §8 建议 `Layouts · Popover` 与
`Layouts · Tooltip` 两页）钉：plain 高 == 24、圆角 == 4、内衬 8/4、字级 `body-small`；
rich 圆角 == 12、elevation == 3、max width == 320、两个动作都在面板内；
**面板越出被裁祖先**、**flip 后面板在锚点上方**（这两条正是 O1 的验收测试，该文 §8 已列名）；
`pointer` 移出 1.5s 后消失（断言 `visible` 由真变假）；同一时刻只有一个 tooltip；
tooltip 不吃锚点的点击（断言悬停到 tooltip 上时锚点仍 `hovered`）。

## 13. 建议的实现顺序

**第一批（不动引擎，纯控件/画廊）：**

1. **接线 `menu_style`**——一行调用把 MD3 外观交给右键菜单（§10.3）。这是全部条目里
   投入最小、症状最明显的一条（现在屏幕上的菜单是 34dp 行高 + 白描边）。
   同批把「兜底菜单取 == 控件菜单取」的同源断言加进 `src/tests/`。
2. **`segmented-buttons`**（§2）——令牌齐、等分能力已在、部件（`GroupPosition`、`Md3Button`）已在。
3. **`progress-indicators` 线性**（§3）——宽度由应用给即可；环形/不确定态明确不做并写明原因
   （缺弧/旋转，§3.2）。
4. **`navigation-bar` + `navigation-rail`**（§5、§6）——同一颗 `Md3NavItem` 内部件，
   两页画廊，全部靠 `Grow::Split` / `cross_*` 现成能力。
5. **`navigation-drawer` 的 standard 变体**（§7）——`Md3ListItem` 换皮 + 一列。

**第二批（需要少量引擎工作，但不等 O1–O4）：**

6. **`snackbar`**（§11）——用 `fixed`+`inset`+`Sense::PASS_THROUGH` 摆位（引擎已有），
   用「常驻树内 + `animate` 的 0→1」做进出（fab_menu 已趟过，缺口 #13 的绕法），
   用 `AnimSpec.delay` / `status().settled` 做自动消失（**先跑一次验证这条闹钟成立**）。
   宽度上下限与「有 action 不自动消失」需要一次裁定（§11.2）。
7. **`split-button` 的表面**（§8）——胶囊 + 内角随交互变半径可先做，尾段先当普通动作上报，
   **不接菜单**（等浮层）。

**第三批（被引擎缺口卡住，按 `popover-tooltip-design.md` §8 的 O1→O4 走）：**

8. **`toolbars`**（§9）——先把 `md.comp.icon-button` 的表生成出来（app-bar 与 toolbar 共用一颗
   槽位，并顺手补上进度文档 §4.12 那句已失效的「本仓没有 icon-button 的令牌导出」）；
   溢出折叠要浮层 + 声明期测量（缺口 #12）。
9. **控件级 menu / app-bar 溢出**（§10.4）——等 O1–O3。
10. **`tooltips`**（§12）——等 O1 + 延时机制（§12.3 的闹钟验证）。
11. **`loading-indicator`**（§4）与**环形进度 / 波浪**（§3）——等一个真正的**弧线/旋转**能力
    （新图元，或 `Icon` 的轴动画）。这两条在能力落地前**不进画廊**。

**贯穿的一条**：`docs/engine-gaps.md` 的 #3（声明期不知尺寸）、#12（声明期测不了文字）、
#13（无进出过渡）在本批里被反复以「由应用给宽度/由应用给值」绕过（§3.2、§8、§11.3、§12.3）。
这与进度文档 §4.9 / §4.12 / §4.13 已有的约定一致，**不是新问题**；但 `popover-tooltip-design.md`
§4 那句「`AnimSpec.delay` 做不到触发」需要修正或补一条实测（§12.3），否则下一个执行者会
据此再绕一遍。按 `AGENTS.md` §1.2 通道①，那应当写成一条可跑的红/绿测试再定稿。
