# 控件补全长期任务 —— 进度与工作记录

> 这份文件是**跨会话的锚点**。上下文被压缩后，从这里恢复：目标、清单、当前进度、已知坑。
> 每次推进一个控件就更新一次本表。**不要**把它写成流水账，只保留「现在在哪、下一步是什么」。

## 1. 目标

把 `ui/crates/ui-widgets-md3/tokens/` 下的每一份设计系统导出，都变成一个可用的 MD3 控件：
令牌表（`src/spec/*_tokens.rs`，由 `gen_spec.py` 生成）+ 控件实现（`src/*.rs`）+ gallery 页面（含有效变体）。

已完成的参考实现：**switch**（`src/toggle.rs` + `src/spec/switch_tokens.rs` + gallery `Page::Switch`）。
它的每一处设计决策都可以当作后续控件的模板；但它**不是**免检的，照抄前先读懂。

FluentUI / HIG(SwiftUI + LiquidGlass) 的 gallery 页面本轮**留空**（未实现）。

## 2. 工作方式（每个控件的固定动作）

1. `python3 tokens/gen_spec.py inspect <json>` 看令牌、对象、状态；必要时先修 `gen_spec.py`。
2. 用 `gen_spec.py gen` 生成 `src/spec/<control>_tokens.rs`。
3. 读导出的令牌语义 + Flutter 参考实现（`/home/ollie/git/widget-ref/flutter/packages/flutter/lib/src/material/`）
   与 MD3 规范，写控件：几何、配色、动效、`change()` 语义。
4. 写测试：只钉**行为不变量**（§7），每条都做过「破坏 → 变红 → 恢复 → 变绿」。
5. 在 gallery 加页面：每个控件必须展示**有效变体**——即「运行中无法切换、只能创建时决定」的变体
   （尺寸档、有无图标、容器形状…），运行中可切换的状态（selected/hover/disabled）用交互演示。
6. 全量验证：`cargo test --workspace --features ui-text-fontique/system`、`cargo build --workspace --tests --examples`（0 warning）、
   `GALLERY_SELFTEST=1`（**只在 `scripts/nested-session.sh` 起的嵌套会话里跑**）。

## 3. 清单与进度

| # | 控件 | 导出文件 | 令牌前缀 | 状态 |
|---|---|---|---|---|
| 0 | switch | `switch.json` | `md.comp.switch` | ✅ 完成 |
| 1 | divider | `divider.json` | `md.comp.divider` | ✅ 完成 |
| 2 | badge | `badges.json` | `md.comp.badge` | ✅ 完成 |
| 3 | checkbox | `checkbox.json` | `md.comp.checkbox` | ✅ 完成 |
| 4 | radio button | `radio-button.json` | `md.comp.radio-button` | ✅ 完成 |
| 5 | slider | `sliders.json` | `md.comp.slider` | ✅ 完成（完全重写） |
| 6 | tabs | `tabs.json` | `md.comp.secondary-navigation-tab` | ✅ 完成 |
| 7 | buttons | `buttons/buttons.json` | `md.comp.button` | ✅ 完成 |
| 8 | FAB menu | `buttons/fab-menu.json` | `md.comp.fab-menu.*` | ✅ 完成 |
| 9 | chips | `chips.json` | `md.comp.assist-chip` 等 | ✅ 完成（四种：assist/suggestion/filter/input） |
| 10 | cards | `cards.json` | `md.comp.elevated-card` 等 | ✅ 完成（三种变体：elevated/filled/outlined） |
| 11 | lists | `lists.json` | 见文件 | ✅ 完成（行数 × 选中 × 布局 × 变体 × 视频） |
| 12 | dialogs | `dialogs.json` | `md.comp.dialog` | ✅ 完成（图标 / 无图标、1–3 个动作、有无标题、模态与内联两种） |
| 13 | app bar | `app-bar.json`（原 `app-bars?.3ec6…`） | `md.comp.app-bar` | ✅ 完成（六种变体：small/medium/large/两种 flexible/search，含 on-scroll 与收起动画） |
| 14 | carousel | `carousel.json` | `md.comp.carousel-item` | ✅ 完成（五种布局：multi-browse / hero / 居中 hero / uncontained / full-screen；每页焦点右移一格、越界回弹、按钮缓动翻页、`infinite` 无缝滚动、点击居中，含滚动与点击上报） |
| 15 | text fields | `text-fields.json` | `md.comp.outlined-text-field` 等 | ✅ 完成（填充式 / 描边式，标签浮动、缺口、图标、错误 / 禁用，输入归应用） |
| 16 | date pickers | `date-pickers.json` | `md.comp.date-picker.docked` / `md.comp.date-input.modal` | ✅ 完成（停靠式：日 / 月 / 年三种主体、今天 / 选中 / 月外 / 禁用、可选范围、周起始、**网格内方向键导航**（格子在静息时不画墨迹，因此显式 `focus_stop`；方向键走到相邻格、Enter 选中）。文本输入式：328×512 表面、120 标题带（辅助行 / 32sp 标题 / 日历开关）、描边字段占侧内衬留下的 280dp、取消与确定在末端右对齐，三个动作各上报自己的 id，缓冲仍由引擎编辑——见 §4.16）；**未做**模态与范围选择（`date-picker.modal.*` 含 `range-selection.*`，见 §3.1） |
| 17 | time pickers | `time-pickers.json` | `md.comp.time-input` | ✅ 完成（输入式 12 小时制：小时 / 分钟两个真文本输入格、AM/PM 周期选择器、聚焦 primary-container + 2dp primary 描边、禁用样本；导出里**没有**表盘 dial 变体） |
| 18 | search | `search-bar-view.json`（原 `app-bars?.0734…`） | `md.comp.search-bar` / `search-view` | ✅ 停靠式 search bar（56dp 胶囊、前置图标、真文本输入 + 单行滚动、尾部动作点自己的 id、hover/press 状态层、聚焦环、contained 变体）；✅ search view（同一表面在两个尺寸间走：收起是一行栏、展开是盖住结果的一页，头 56→72、圆角 28→0、分隔线与结果按进度淡入，点静止的栏请求展开、返回动作收起）；**未做** avatar 头像位、supporting text；全屏浮层摆放归应用——见 §6.25 |
| 19 | segmented buttons | `buttons/segmented-buttons.json` | `md.comp.outlined-segmented-button` | ✅ 完成（轮廓式：等宽分段、位置决定圆角、选中实填充 / 未选中描边、点击上报选中 id） |
| 20 | progress indicator | `indicators/progress-indicators.json` | `md.comp.progress-indicator` | ✅ 线性与环形的确定态 / 不确定态：线性按 Android `progress_indeterminate_material.xml` 的两段扫动；环形按 Flutter `_CircularProgressIndicatorPainter`——`_startAngle` 十二点起、`arcSweep = value·(2π−ε)`、数值弧平头、轨道圆头且两端各让出 `track_active_indicator_space`（4dp）；**未做** Expressive 波浪变体（`wave.amplitude` / `wave.wavelength` 已在 spec 里读入，绘制端不画） |
| 21 | navigation bar | `navigation/navigation-bar.json` | `md.comp.navigation-bar` | ✅ 完成（80dp 栏、等分目标、64×32 胶囊按 500ms `easeInOutCubicEmphasized` 横向展开、选中标签 700 / 其余 500、三种 `labelBehavior`）；栏为方角、无阴影——见 design-errors 的 shape 解析那条 |
| 22 | navigation rail | `navigation/navigation-rail.json` | `md.comp.navigation-rail` | ✅ 经典轨道（80dp 定宽列、56×32 胶囊、`no-label` 时 56×56 正圆、`RailLabelType::{None,Selected,All}` 三种标签模式、不等高目标的条带命中）；✅ Expressive 一对（`RailVariant::Expressive{expanded}`：96dp 收起 ↔ 220dp 展开，300ms standard 由引擎插值宽度，胶囊随之 56×32 → 横跨轨道 × 56，标签从胶囊裁剪边缘露出）；**未做** 顶部菜单按钮（`menu.*` 令牌全族废弃） |
| 23 | navigation drawer | `navigation/navigation-drawer.json` | `md.comp.navigation-drawer` | ✅ 标准抽屉（360dp 表面、尾侧 16dp 圆角、12dp 内缩的 336×56 胶囊、`title-small` 分组标题、选中 700/`on-secondary-container`）；**未做** modal 变体与遮罩（`scrim.*` 令牌已废弃，且需浮层 O1–O3）；条目未复用 `Md3ListItem`——它的选中容器是整宽矩形，抽屉的是内缩胶囊，几何不同源 |
| 24 | toolbar | `toolbars.json` | `md.comp.toolbar.{docked,floating,standard,vibrant}` | ✅ 停靠式 / 浮动式（64dp、方角 / 胶囊、elevation 6 阴影、16/8 内缩与 4dp 间距）、标准 / 鲜艳两套成员主题（48dp 触控区、40dp 圆盘、24dp 图标，常量与 `APP_BAR_SLOT`/`APP_BAR_ICON` 断言同源）；**未做** 浮动式的 FAB 变体（`floating.fab.*`，已从表里排除）与图标按钮的完整令牌表（`icon-buttons.json` 457 条，其 40/24/焦点环已按值取用） |

### 3.1 剩余项（2026-11-04 逐条核对导出令牌）

§3 的 25 个控件都完成了主形态；下表是各行的「未做」部分，**已按导出令牌逐名核对**
（`crates/ui-widgets-md3/tokens/*.json`，取每条令牌的 `tokenName`），不是凭印象列的。

| # | 控件 | 缺什么 | 依据 | 依赖 |
|---|---|---|---|---|
| 16 | date picker | 模态与范围选择 | `md.comp.date-picker.modal.*` 含 `range-selection.*` | 需浮层（O1–O3、缺口 #13） |
| 18 | search | avatar 位、supporting text | `md.comp.search-bar.avatar.*`、`header.supporting-text.*` | — |
| 20 | progress | Expressive 波浪变体 | `…circular.active-indicator.wave.{amplitude,wavelength}`、`…linear.*`、`…with-wave.*`；`progress_indicator_tokens.rs` 已读入，绘制端不画 | 正弦起伏需要路径图元（`Sector` 只画圆弧） |
| 22 | nav rail | 顶部菜单按钮 | `md.comp.nav-rail.menu.*` 全族废弃，不复活 | — |
| 23 | nav drawer | 模态变体 | `md.comp.navigation-drawer.modal.*` **只有 2 条**（container 的 color / elevation），没有遮罩色 | 浮层 + 遮罩自定（`scrim.*` 令牌已废弃） |
| 24 | toolbar | 浮动式 FAB 变体 | `md.comp.toolbar.floating.fab.*` 存在，本任务范围已排除 | — |

**2026-11-04 更正三处旧说法**：第 16 行原写「导出里没有后两者（输入模式、模态）」——不成立，
导出同时有 `date-input.modal.*` 与 `date-picker.modal.*`；第 22 行原写「`menu.*` 令牌全族废弃」——
`md.comp.navigation-rail.menu.*` 仍在导出里；本表初版把「网格内方向键导航」列成缺失并声称
「引擎没有按方向找焦点的 API」——**引擎早就有** `AppLoop::focus_spatial` + `spatial_score`
（`view.rs`，还有 `arrows_move_within_the_visual_grid` 测试）；真正缺的是日期格没声明
`focus_stop`，见 `docs/lessons/design-errors.md` 最新一行。

## 4. 已决事项

### 4.1 `app-bars` 的两个文件不是重复

两个文件的 `tokenSets` 完全不同，是**两个不同的组件**被同一个（被截断/被清洗过的）文件名保存了：

| 文件 | tokenSets | 实际内容 |
|---|---|---|
| `app-bars?.07341534518f78d0.json`（1.8 MB，147 令牌） | `md.comp.search-view`、`md.comp.search-bar` | **搜索栏 / 搜索视图** |
| `app-bars?.3ec622835c7987b5.json`（3.8 MB，275 令牌） | `md.comp.app-bar` 及 `medium/small/large`、`md.comp.top-app-bar.*` | **应用栏（顶栏，含 medium/large 折叠变体）** |

结论：**两个都是正确的**，各管一个组件。已重命名为 `search-bar-view.json` 与 `app-bar.json`。

### 4.2 slider 的几个已决细节（重写时定下的）

| 事项 | 决定 | 依据 |
|---|---|---|
| 轨道几何 | 两段轨道在**手柄中心**两侧各留 `trackGap = 6`；外侧圆角为轨道高的一半，内侧 2dp | Flutter `GappedSliderTrackShape`（`slider_parts.dart:1114-1169`），`trackGap` 默认 6（`slider.dart:2371`） |
| 手柄位置 | 中心在 `track.shape.leading + value × (w − leading − trailing)` | 导出为每个尺寸给出内缩；Flutter 的 `thumbPadding` 夹取（`trackHeight/2`）在 XSmall 上恰好等于导出的 8，其余尺寸以导出为准 |
| 按压收窄 | 手柄 4→2dp，**缺口同步**从 6→5，可见空隙恒为 4dp | Flutter `slider.dart:1765-1784` 的 `trackGap - delta/2`；我们把它连续化到动画宽度上 |
| 停止点 | 位置 = 轨道末端内缩 `trailing-space(4) + 半径`，**范围滑块两端各一个**；填充盖过它时换成 `onPrimary`；离散滑块不画（改由刻度点标记） | 导出的 `stop-indicator.container.color` 给了 active/inactive 两色；Flutter 是「盖过就隐藏」且恒用 activePaint，这里以导出为准。两端各一个见 `GappedRangeSliderTrackShape`（`range_slider_parts.dart:1085-1105`）；`!isDiscrete` 判据见 `slider_parts.dart:1229` |
| 状态层 | 48dp 圆盘，按压/聚焦 `primary@10%`、悬停 `8%`，按压时从中心长出 | Flutter `_SliderDefaultsM3.overlayColor` + `RoundSliderOverlayShape`（半径 0→24） |
| 绘制判据 | 段宽 > 0 就画（**不用** Flutter 的 `thumbCenter > left + trackHeight/2`） | Flutter 那条判据在大轨道上留洞：XL（96dp 轨道）在 value≈0.1 时左侧 48dp 无任何轨道。半径由引擎按短边一半夹取后，短段本就是合法胶囊 |
| `step` | `SliderOpts::step`：`value_at` 仍是连续映射，`change()` 量化到最近的 `round(v/s)*s`；落在当前刻度上不上报 | material-web `step`、Flutter `divisions`；量化放在上报前，绘制跟随应用回填的值 |
| `ticks` | `SliderOpts::ticks`：只在**内部**刻度画点（两端由圆端帽吞掉），被手柄盖住的点跳过，点距 < 3×直径时整组不画 | Flutter `slider.dart:1811-1830` 的密度判据；端点处理是偏离：Flutter 把刻度缩进 `trackHeight/2`，大轨道上会与手柄位置错开 28dp |
| 刻度颜色 / 尺寸 | `onPrimary@38%` / `onSurfaceVariant@38%`（禁用 `onSurface@38%`），直径 2dp | 导出 `with-tick-marks.*` 令牌**已废弃**且替代令牌不在导出里，数值只能取废弃项；Flutter 的生成默认值是直径 4，以导出为准 |
| `range` | `SliderOpts::end`：三段轨道（**外侧两段都是非激活色**，只有中间选中段是激活色），两个手柄，`Change::Data(Md3Range)` 一次上报两个值 | 手势按「离谁近」挑手柄；移动中的手柄总离自己最近，因此无需在控件里存 latch 状态（引擎尚无按下点，见 `engine-gaps.md`）。配色见 Flutter `RangeSliderTrackShape`：「The middle track segment is the selected range and is active, and the two outer track segments are inactive.」 |
| 值指示器 | 不实现 | 需要标签文本与另一套几何，留待 `Slider` 语义扩展 |

### 4.3 tabs 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 高度 | 页签带 = 页签盒（纯文字 46 / 带图标 72）+ 下划线工带 2；控件另加 1dp 分隔线（49 / 75） | Flutter `_kTabHeight`、`_kTextAndIconTabHeight` + `indicatorWeight`；导出的 `container.height = 48` 正好是 46+2，已作为断言钉住 |
| 分隔线位置 | 画在页签带**下方**，而不是 Flutter 那样压在栏的最后一行 | 下划线占用页签带最后 2dp，重叠会把线盖住；偏离已记录 |
| 等分 | `Grow::Split(1)`（零基准），不是 `Grow::Yes` | Flutter 用 `Expanded`；`Yes` 是「内容之上再增长」，短标签的页签会变窄（见 engine-gaps #7） |
| 下划线 | **整条栏共用一个 `Overlay` 节点**，按选中项滑过去（300ms 线性），不再每个页签各自淡变 | Flutter 的 `TabBar` 也是共用一个 indicator；滑动需要栏宽，而 `decl()` 拿不到（engine-gaps #3），故由调用方把 `width` 传进来 |
| 焦点环 | 画在页签自身边缘 | 导出的 `outline.offset = -3` 要求内缩，声明阶段没有宽度 |
| 页签 id | 由 app 给出（`Md3TabItem { id, .. }`） | `Change::Selected` 必须指向 app 认得的东西 |
| 上报时机 | 只有按下**另一个**页签才上报；重选当前页签不报 | 语义门只上报「变化」；Flutter 仍会调 `onTap`，此处故意不同 |
| 键盘 | 焦点可移动，但不改变选择 | engine-gaps #6：Enter/Space 的 `Activated` 绕过控件 |

### 4.4 buttons 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 普通按钮 vs 切换按钮 | 生成器加了 `toggle` 变体轴（`Base` / `Unselected` / `Selected`），`Base` 才是无状态基态；`Base` 是普通按钮，另两个由 app 持有状态 | 导出描述的是**切换**按钮：`unselected` 的容器色是 `surfaceContainer`（一种真实外观）。沿用它会令所有普通填充按钮变灰底（真机实测 `(33,31,36)` vs 应有的 `primary` `(207,189,254)`） |
| API | `style.button(..)` 无状态、`style.toggle_button(.., selected)` 有状态 | 调用方不必知道 `Base` 的存在，也就不会漏写 |
| 容器有无 | `text` 从不画；`outlined` 只在**选中**时画（其选中色 `inverseSurface` 是真填充）；其余恒画 | 导出里 `outlined` 未选中无 `container.color`，选中时给了实色 |
| 胶囊半径 | `height / 2`，随尺寸档变化 | MD3 默认形状是 `StadiumBorder` |
| 标签字级 | base/xsmall/small → `label-large`，medium → `title-medium`，large → `headline-small`，xlarge → `headline-large` | 导出的 `contextualReferenceTrees` 把 `label-text` 指向 `md.sys.typescale.*`；数值取自 Flutter `text_theme.dart`（2021 表），落成 `typescale.rs` |
| 立面 | `blur = elevation × 2`、`alpha = 0.18`、`dy = elevation × 0.5` | 引擎只有一条解析阴影（`ShadowSpec`），MD3 的 1dp 环境+投射两重只能近似；已记录为近似而非等价 |
| 焦点环 | 画在按钮自身边缘 | 导出的 `outline.offset` 要求外移，声明阶段拿不到最终矩形（同 tabs） |
| 禁用 | 内容 38%、容器 10%（`onSurface`） | 导出的 `opacity` 令牌；`handles_disabled() = true` 由控件自己画，不走引擎的整树淡化 |

### 4.5 FAB menu 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 色调耦合 | 公开一个 `FabMenuColour`，内部同时派生 `FabMenuColor`（关闭按钮）与 `FabMenuContainer`（菜单项） | 导出把 FAB 的色调拆成两个轴；独立选取会出现「主色 FAB 开出次色菜单」 |
| 两种间隙 | 列用 `gap(0)` + 显式占位节点：项间 4dp、锚点前 8dp | 列只有一种 `gap`；用同一间隙会把锚点挪离 FAB 的位置 |
| 对齐 | 尾部对齐（`cross_pos(1.0)`） | 菜单项比锚点宽，MD3 让它们右边缘齐平 |
| 收起 | `open` 淡到 0 时**不画**菜单项，锚点照画 | 淡到零会留下仍能响应点击的幽灵节点 |
| 动画 | 150ms `EaseOut` 整体淡入 | 引擎没有变换矩阵，MD3 的缩放+位移无法表达；淡入是近似，命中几何因此保持精确 |
| 命中 | 按行高自顶向下算，不查表 | 行高与间隙都来自导出，且与绘制同源 |
| 上报 | 项 → `Change::Selected(Some(id))`；锚点 → `Change::Activated` | 选择与动作是两回事，应用要能区分 |
| 禁用 | 内容 38%、容器 12%（`onSurface`） | 导出没有禁用令牌，按 MD3 通用规则 |

### 4.6 chips 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 一份控件四张表 | `Md3Chip` 带 `ChipKind`，内部把四份导出归一成同一个 `Parts` 结构 | 四种芯片的几何完全相同（32dp 胶囊、8dp 圆角、14sp/500 标签），差别只在令牌表 |
| 扁平 / 立面 | `ChipVariant::{Flat, Elevated}` 是构建期枚举；input 没有立面形式，传了也不生效 | 导出里只有 assist/suggestion/filter 有 `elevated` 分支 |
| 选中 | `selected: bool` 归应用；只有 filter/input 有意义 | assist/suggestion 的导出没有 `selected` 变体 |
| 语义 | filter/input → `Toggled { on: !selected }`；assist/suggestion → `Activated` | 前两者是开关、后两者是动词 |
| 内衬 | 有标记的一侧 8dp，否则 16dp；标记 18dp；标记与标签间隔 8dp | 导出只给 `icon.size` 与 `height`，内衬取自 MD3 芯片规范 |
| 圆角 | 8dp（`rounded_corners`），不是胶囊 | 导出 `container.shape = rounded_corners` |
| 禁用 | 直接用导出的 `opacity`（标签 38%、描边/容器 12%） | 四份导出都自带禁用分支，无需手写规则 |
| 尾随图标 | input/filter 的尾随标记与芯片本体共用一个命中区，按下时**无法**区分是否点在尾随标记上 | 引擎只把点击交给整个控件（engine-gaps #8） |

### 4.7 cards 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 一份控件三张表 | `Md3Card` 带 `CardVariant::{Elevated,Filled,Outlined}`，内部归一成 `Parts` | 三份导出的几何完全相同（12dp 圆角、16dp 内衬、同一组状态层/焦点环），差别只在容器与描边 |
| 内容 | `content: Vec<Decl>` 由应用传入，卡片只负责表面与内衬 | 卡片本身没有语义内容；导出只有 `icon`（供内部使用的图标） |
| 是否可交互 | `clickable: bool` 是**构建期**参数：静态卡片不发 `Sense`、不画状态层、不发焦点环 | 导出的状态层只在有交互时才有意义；静态卡片用户根本悬停不到 |
| 语义 | 可交互卡片点击 → `Activated`；禁用/静态 → `None` | 卡片是应用挂动作的唯一入口 |
| 描边 | 只有 outlined 画环（`outlineVariant`，1dp） | elevated/filled 的导出根本没有 `outline` 对象 |
| 禁用 | elevated/filled 用导出的 `container.opacity = 0.38`；outlined 的导出没有 `container.opacity`，禁用在描边上（`outline@0.12`），故 `fill_op` 固定 1.0 | 三份导出的禁用分支写法不同，强行统一会让 outlined 的填充也变淡 |
| 立面 | `ShadowSpec` 由 `container.elevation` 推出（blur = 2×elevation，alpha 0.18，dy = 0.5×elevation） | 导出只给数字（1/3/8），不给阴影参数；与 switch 的阴影约定一致 |
| 焦点环 | 画在卡片自身边缘，未按 `focus.indicator.outline.offset` 外移 | 声明阶段拿不到最终矩形（与 chip/button 同一处理） |

### 4.8 lists 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 行数决定高度 | `ListLineCount::{One,Two,Three}` → 56/72/88，取自 `container.height` | 三份导出的高度各不相同，共用一个会把副文本挤进单行 |
| 前置槽 | `ListLeading::{None,Icon,Avatar,Image,Video}` 是构建期枚举；槽宽取 `leading_icon.size` / `leading_avatar.size` | 槽的尺寸与内容类型绑定，运行中不会变 |
| 布局轴 | `segmented: bool` 让容器取 12dp 圆角，普通行是方角 | 导出里 `segmented.container.shape = rounded_corners`，其余行没有形状 |
| 选中 | `selected: bool` 归应用，选中取 `secondaryContainer`，未选中取 `surface` | 列表项画填充的唯一理由就是选中 |
| 语义 | 可交互行点击 → `Activated`；禁用行 → `None` 且不发指针手型 | 行是应用挂动作的那一行 |
| 内衬 | 左内衬取 `root.leading_space`（16dp），槽与文字之间取 `root.between_space`（12dp） | 导出只给这两个数，没有逐段的更细划分 |
| 分隔线 | `divider_insets()` 由导出的 `divider.leading_space` 给出，调用方自己插入 `Md3Divider` | 列表项不持有兄弟节点 |

### 4.9 dialogs 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 遮罩与表面 | `barrier: bool` 是构建期变体：`true` 时控件根是铺满 `area` 的遮罩、表面居中；`false` 时控件根就是表面本身 | 并排预览多个对话框时遮罩会互相盖住；真实模态与内联表面是两种用法，运行中不切换 |
| 居中方式 | 两个 `Grow::Yes` 的零基准增长者夹住表面 | 引擎没有主轴定位（engine-gaps #11）；增长者分余量，亏空时表面被压进区域 |
| 表面宽度 | 应用传入 `width`（下限 280），文字在 `width − 48` 处换行 | 声明阶段测不了内禀宽度，`IntrinsicWidth` 做不到（engine-gaps #12） |
| 动作 | 复用 `Md3Button(Text, Base)`；对话框在**构建期**读 `cx.state(action_id).clicked` 决定上报哪个 | 动作是 decl 不是 element，按钮自己的 `change()` 不会执行；点击只存在一帧而 `change()` 没有上下文 |
| 动作上报 | `Change::Selected(Some(action_id))` | `Activated` 不携带身份，多个动作分不开 |
| 遮罩点击 | 点击路径含表面 → 不是取消；只含根 → `Change::Cancelled` | 落在表面内衬上的按下不能关掉对话框 |
| 文本对齐 | 有图标时标题与正文都居中，无图标都起始；判据用**解析出的图标**而不是请求的枚举 | Flutter `textAlign: icon == null ? start : center`，标题与正文共用一条判据 |
| 图标 | `DialogIcon::{None,Slot}` + `build_icon: fn(Id, Color) -> Decl`，墨色取 `secondary` | 图标是应用的图元，对话框只给尺寸与墨色 |
| 状态层 | 表面不画状态层 | 导出的 `state-layer` / `label-text` 属于动作标签，由按钮自己的令牌解析；有一条测试钉住两者取色一致 |
| 进出场 | 根节点声明 `enter` / `exit`（400 ms emphasized decelerate 进、200 ms emphasized accelerate 出），整个遮罩+表面作为一个单元淡入淡出；应用只管声明/撤掉对话框 | 引擎持有退场节点（`docs/transition-design.md` §3.2–3.3）；时长不在曲线附近随手配——M3 的配对表把 400 ms 给「进入屏幕」，200 ms 只给「离开」，用短时长进入会显得瞬间出现 |

### 4.10 本轮（2026-10-29）回看已有控件时定下的几条

| 事项 | 决定 | 依据 |
|---|---|---|
| 复选框三态 | `CheckState { Off, On, Mixed }`；`Mixed` 与 `On` 共用填充外观，只换字形；点击时 `Mixed → On`（上报 `on: true`） | material-web 的规则；`Change` 仍只有两态（engine-gaps #4），app 想把框打回 `Mixed` 只能自己置值 |
| 字形变形 | 两个字形实例反向行程：离开的那个在前半沿自身长度收缩，到达的那个在后半长出 | Flutter `_CheckboxPainter`（`_drawCheck` / `_drawDash` + `tShrink`）。Compose 的写法更连续：同一路径按 `checkDrawFraction` 画出，再用 `crossCenterGravitation` 把三个点拉向中线，勾因此「压扁」成横杠；作为备选记录，未采用 |
| 复选框焦点环 | 形状是 25% 圆角方框（与 Compose 的 `RoundedCornerShape(25)` 一致），范围**已按官方改到 48dp 触控区**：`checkbox.rs` 用 `TOUCH` 作边长、内缩 `focus_indicator_outline.offset` | AOSP `Checkbox.kt` 的 `ripple(..., focusRingShape = RoundedCornerShape(25))` + `minimumInteractiveComponentSize`。2026-10-29 用户裁定按官方标准，现有观感不作理由（发布前都是非稳定版本） |
| 页签下划线 | 见 §4.3：整条栏共用一个 `Overlay`，按选中项滑动 | Flutter `TabBar` 也是共用 indicator |
| FAB 菜单展开/收起 | 菜单项在**保留的槽位**里滑向锚点（`slide(k)`），锚点因此不动；收起时整组淡出，且收起期间不响应点击 | 槽位若跟着收缩，FAB 会上下跳 |
| 对话框二次打开 | 根因是引擎 `Tree::apply` 的重复释放（engine-gaps #14），不是控件或 gallery | ui-core 单元测试先红后绿 |
| 复选框 `Mixed` 的可达性 | 点击单个方框只会 `Mixed → On`（material-web 规则），所以**混合态在画廊里点不出来**。gallery 加三个组按钮（all off / all mixed / all on）整列写入，这是抵达混合态的唯一途径 | 用户报告「无法点击进入 undetermined」；根因是规范本身不允许，不是控件缺陷 |
| FAB 菜单的展示方式 | 每个变体放进一个 220×320 的固定尺寸「屏幕」，锚点用增长者 + `cross_align(0.0).cross_pos(1.0)` 钉在右下角 16/16 处；菜单自身高度恒为 240（3×56 + 2×4 + 8 + 56），屏幕据此留出 48dp 说明带 | 用户要求「用固定尺寸容器表现实际使用场景中悬浮在右下角的状态」；selftest 断言锚点内缩恰为 16/16 |
| 光标 | 形状现在是 `PresentTarget` 的契约，`frame_with_dt` 在**任何早退之前**交出；Wayland 侧 enter/leave 清缓存 + 发出后立即 flush | 端到端验证过 compositor 依次收到 `PointingHand → Arrow → PointingHand`。完整模型见 [`docs/cursor-system-design.md`](cursor-system-design.md) |
| 模态必须挂在窗口根上 | 模态原先是 MD3 列的子节点：遮罩只压暗那一列，表面还盖住了预览样例，看起来像「列内其他元素被动过」。改为根是 `Box`、外壳 `Row` 是流内子节点、模态是悬浮兄弟，`area` 取窗口尺寸 | 先量过：打开模态前后全树 `rect` 逐一比对 **0 个移动**，说明位移不是布局造成的，而是覆盖；selftest 加 `window-scrim` 断言（改回列尺寸立刻变红） |
| 页签下划线时长 | `AnimStore` 的「剩余距离成比例缩短时长」只在**被打断**的行程上生效；已落定的一段不得参与缩放 | 用户报「和网页 demo 时长不符」。实测下划线 112ms 到位（spec 是 250ms）；`AnimStore` 条目显示 `dur=0.125`，因为上一段 `2→0` 已落定仍被当作单位。回归测试 `a_settled_instance_takes_the_declared_time_whatever_the_last_leg_spanned`（标量与颜色各一条，破坏后变红） |

### 4.11 四份子系统设计文档（2026-10-29，一份已实现，三份待做）

| 文档 | 覆盖 | 触发它的用户反馈 |
|---|---|---|
| [`docs/pointer-model-design.md`](pointer-model-design.md) | 事件 → 路由 → `Change` → 控件四层；提出 `PointerGesture` 与引擎自持的按压波纹 | 涟漪只能从中心发出；引擎把按下原点丢了 |
| [`docs/cursor-system-design.md`](cursor-system-design.md) | 逐帧解析 + 单点交出 + 覆盖层；36 个协议形状 | 启动时显示上个应用的光标；除箭头外从不出现别的形状 |
| [`docs/style-object-design.md`](style-object-design.md) | `Style` 改为不可变解析快照（`[Color; 22]` + `TypeRole` + `MaterialRecipe`），带 `Resolution` 与插值规则 | 「重新审视 `Style` 类的命名/设计/架构遗留问题」 |
| [`docs/icon-text-design.md`](icon-text-design.md) | `Family` / `Variations` / `Features`、字体注册、轴位置驻留、`Icon` 控件、`script_face` 污染 — **✅ 已实现**（偏差见该文 §5.1） | 「常规图标无法正常使用应是引擎缺口」 |

四份都按「完整设计」写（数据类型 + 数据流 + 缓存键 + 迁移步骤 + 验证），不是「要加哪几个字段」。

### 4.12 app bar 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 变体 → 令牌集 | `Medium`/`Large` 取 `medium`/`large` 的**展开**高度与字级（112/headline-small、152/headline-medium）；`MediumFlexible`/`LargeFlexible` 另叠 `with-subtitle`（136/152） | 导出的 `container.height` 与 `*.title.font`；Flutter 的 `_MediumScrollUnderFlexibleConfig`/`_Large…` 展开态与之同值 |
| 标题位置 | small/search 与槽位同一行、垂直居中；medium/large/flexible 在**底部**，`cross_pos(1.0)` + 底距 20/28dp | Flutter `_ExpandedTitleBox` 用 `AlignmentDirectional.bottomStart`（`app_bar.dart:2340`）；底距取 `expandedTitlePadding` 的 20/28 |
| 标题起点 | 有前置槽位时 `leading-space + 48 + 16`，否则 16；medium/large/flexible 一律 16（图标在上一条带里，标题从容器边起排） | `NavigationToolbar.kMiddleSpacing = 16`；Flutter 展开态 `expandedTitlePadding` 左右都是 16 |
| 收起 | `collapsed: bool` 由应用给，控件内部按 0.3s 强调减速插值；高度从展开值插到 64，标题字级插到 `title-large`，副标题淡出 | 引擎没有滚动偏移可喂（engine-gaps），Compose 的 `TopAppBarScrollBehavior` 那种连续输入暂时无处可接 |
| on-scroll | 只换容器色 + 抬升到 3；阴影沿用本仓的 `ShadowSpec { blur: e*2, alpha: 0.18, dy: e*0.5 }` | 导出 `on-scroll.container.{color,elevation}`；实测 `surface` #141218 → `surfaceContainer` #211F26 |
| 槽位 | 48dp 触控区、24dp 标记、40dp 圆形状态层、按压墨迹半径 20 | 导出给了 `icon.size = 24` 与 `leading-space = 4`，48dp 触控区与 40dp 盘是 MD3 icon button 的规格；**本仓没有 icon-button 的令牌导出** |
| 布局结构 | 根是 Column：上带（Row，64 高）+ 悬浮的标题带；标题带本身是 Overlay，里面再套一层 Row 持有底部对齐 | `cross_pos` 是**容器**的参数，底部锚定必须由标题的父容器给出，不能写在标题自己身上（见 design-errors） |
| 标题宽度 | `AppBarParams::width` 由应用给，标题按 `width − 起点 − 右内衬` 换行 | 与 tab bar 同一条约定：声明阶段拿不到布局宽度 |

### 4.13 carousel 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 条目尺寸从哪来 | 导出只给了间距与小号范围（40–56），没给条目宽高；用**权重**把容器宽度分给同屏的三档：multi-browse `[3,2,1]`、hero `[7,1]`、居中 hero `[1,5,1]` | Flutter `_RenderSliverWeightedCarousel` 的加权模型：`unit = (可用宽 − 间距)/权重和`，前缘条目 `= w0·unit` |
| 页步长 | 一页走多远 = **最宽一件**的宽度（`max(w)·unit`），不是前缘条目的宽度；前缘槽位的宽度另由 `leading_extent`（`w0·unit`）给出。居中 hero 的前缘是一格小条目，把它的宽度当页步长会让四件条带只有 134dp 行程 | Compose `CarouselPageSize` 用 `itemMainAxisSize`（未遮蔽条目的宽度）作页宽，pager 每页前进 `itemMainAxisSize + pageSpacing`；见 design-errors |
| 几何常量 | 左右 16、上下 8、条目间距 8（全屏 16 且无内衬）、圆角 28 | 导出 `carousel.json` 的 padding/gap 与 `rounded_corners`；28 在 MD3 形状刻度上 |
| 滚动方式 | 收缩布局**原地收缩**：前缘条目随滚动变窄到 0，后一件长大顶上，条带本身不横移；非收缩布局（uncontained / full-screen）才真正平移 | Flutter 同一渲染对象的行为；MD3 的 multi-browse 动图也是前缘条目让位而非整条左移 |
| 滚动量归谁 | 引擎按条带 id 持有偏移（`Cx::scroll_of` / `set_scroll` / `scroll_to`），控件每帧**读回**并据此算槽位；`carousel_max_scroll` / `carousel_content` 由 kind、可用宽度与条目数算出，画布长度即 `maxScroll + usable` | 引擎按画布范围钳位，偏移与钳位因此是同一个数；gallery 不再有全局偏移变量 |
| 横向滚动 | 引擎原本只有 `scroll_y`；本次给 `ContainerParams` 加了 `scroll_x`（`layout_engine.rs` 的 `content_origin`），uncontained 条带靠它滑过前缘内衬 | 缺口记在 `engine-gaps.md` §18；有单元测试钉住"子节点左移 + 容器遮罩裁剪" |
| 滚轮/拖拽接线 | 条带声明 `SCROLL_OWNED`：滚轮由引擎累进、拖拽由引擎接管（含惯性、橡皮筋与回弹）；竖直滚轮在只横滚的条带上按横向处理 | engine-gaps §2 的读回与 §18 的 `scroll_x` 都已落地，控件层无需再自建通道 |
| 末端掉头 | 每翻一页，最大权重所在位置**右移一格**（`turned_weights(w, step)`，页数 `count − 1`）：每个条目轮流当一次焦点、都长到最大宽度，最后一页是尾部对齐的花样。`max_scroll = (count − 1) × page`，整页始终占满 | MD3「第一件对齐起点、最后一件对齐终点」+ Compose keyline 的末端移位。单页掉头（`[3,2,1] → [1,2,3]` 一页到位）会让倒数第二件停在中等宽度，见 design-errors |
| 越界回弹 | 条目按**钳位后**的偏移摆放（`offset.clamp(0, max)`），引擎平移的是原始偏移：两者的差就是可见的橡皮筋。布局若按原始偏移摆放，回弹被布局自己抵消，画面上什么都看不到 | 用户报「offset 显示负数说明有弹性，但显示效果里没有回弹」；`params.scroll_elastic` 只在拖拽/惯性期间放开钳位 |
| 按钮翻页 | 上一个/下一个算目标偏移后交给 `Cx::scroll_to`，由引擎的弹簧缓动过去；不用 `set_scroll` 瞬移 | 用户报「点击按钮切换时没有动画过渡」；selftest 的 `eased` 用两帧 60Hz 断言条带停在两页之间（改回 `set_scroll` 即红） |
| 无限滚动 | `CarouselParams::infinite`：铺 `LOOP_PASSES`（101）遍、从中段起步，偏移低于一遍时整体加回中段；索引 `rem_euclid`，没有掉头页。重设不可见，因为布局对整遍严格周期 | 引擎的画布必须有尽头；遍数按 f32 精度选（12.6 万 dp 时 ulp 0.008dp），不是随手取的大数 |
| 点击居中 | 条目被按下时，控件按自身几何二分出「该件正中落在视口正中」的偏移，再用 `Cx::scroll_to` 交给引擎缓动；条带到不了正中时取最近的端点，循环条带取离当前偏移最近的那一份副本 | `centre(s)` 在「该件领先的前一页」内单调递减，二分即解；动画走引擎的弹簧（engine-gaps §2） |
| 画廊不裁剪 | 自检里有一条 `whole`：第 4 个样板的条目遮罩必须等于它的矩形 | 曾经因为列内容高出 0.8px 被 `distribute` 挤压、遮罩把条目切掉（见 design-errors），这条钉住它 |

### 4.14 text fields 的几个已决细节

| 事项 | 决定 | 依据 |
|---|---|---|
| 两张令牌表如何共用 | `Parts` 把填充式与描边式的取值归一到一处（填充 / 描边 / 状态层 / 指示条 / 文字 / 图标 / 字号），控件只读 `Parts` | 两份导出的同名字段语义一致，缺的字段就是「该变体没有这一项」：填充式无描边、描边式无填充与指示条 |
| 高度 | 两种都是 56dp；**描边式在盒子上方另留 8dp 标签带**，根节点因此 64dp | 偏离 Flutter：它让 12sp 标签溢出盒子（`outlinedFloatingY` 为负），我们的遮罩按节点矩形裁剪，溢出的半截会被切掉 |
| 缺口怎么切 | 标签底色（`backdrop`）画在描边上取其一部分；不测量标签宽度 | Flutter 走 `Path.combine(difference)`，需要布局期知道标签宽度；本例用不透明底色盖住，只需知道字段背后的颜色 |
| 标签何时浮起 | `focused \|\| !text.is_empty()`，167ms `fastOutSlowIn`；描边加粗只看焦点，两条时钟分开 | Flutter `_kTransitionDuration` + `floatingLabelProgress`；内容足以浮起时不应同时把描边变粗 |
| 输入归谁 | 应用持有值：`ContainerParams.text_input` 声明当前缓冲，引擎处理 `Event::Text` / Backspace / Delete / ← → / Home / End / Ctrl+A，并以 `Change::Text(整段)` 上报，插入符、选区与预编辑归引擎（`Routing.caret` / `selection` / `preedit`） | 与其它控件同一条约定：控件不藏状态。点击定位插入符、字段内选区、输入法预编辑均已落地（engine-gaps §19）；剪贴板（`ClipboardCommand`，Ctrl+C/X/V 与右键菜单）已落地，多行仍缺 |
| 空缓冲的插入符 | 空字段聚焦时插入符是一个 2×`size*1.4` 的盒子声明，放在内容原点 | `TextBlock::flatten` 会丢掉空 run，绘制端没有可整形的区间；非空时由 `TextPrim.caret` 在字形流里画 |
| 单行溢出 | 输入节点标 `nowrap`（`NodeData::Text.nowrap` / `Decl::nowrap` / `Text::nowrap`），可见区按 `Routing.caret_in_text` 平移 | 行宽由布局取 `min(可用宽, size.max)`，`Text::max_width(INFINITY)` 从来不是「不换行」；插入符只有绘制端量得出，因此由 `caret_in_text` 这一条事实送回控件，且必须在**布局坐标**里量——按 `visual_rect` 量会把控件自己施加的偏移折回来，每帧再滚一截 |
| 图标与内衬 | 有图标的侧内衬 4dp，图标槽 48dp；无图标侧 16dp（导出的 12 + 4 间距）；图标槽 48×56，标记 24dp 居中 | 导出与 Flutter `prefixIcon` 的 48dp 触控区一致；文本起点因此是 52 或 16 |
| 焦点描边 | 3dp（导出 `outline.width` 的 focused 档） | 导出优先于 Flutter 默认的 2dp（`BorderSide.width`）；以后若要改，改的是导出的令牌 |
| 浮层要透传 | 标签层是盖住整个字段的 `Overlay`，必须带 `Sense::PASS_THROUGH` | 引擎把每个画了东西的非文本节点都当不透明命中目标：不加这一位，点字段的任意位置都会命中标签层，焦点落不到字段（design-errors 里记了一笔） |

### 4.15 圆环的直径按令牌，不按 Flutter 的 4dp 内衬（2026-11-04）

Flutter 的 M3 圆环用 `circularTrackPadding = EdgeInsets.all(4)`，因此一个 40dp 的
`CircularProgressIndicator` 画出来的环外缘只到 32dp；Compose 的 `size(40.dp)` 则画满 40dp。
导出令牌只给了 `size = 40` 与 `thickness = 4`，所以按令牌取「外缘 40、描边向内 4」，
并让 `Sector::radius` 的语义（外缘、向内生长，与 WinUI 的 `StrokeAlignInside` 同）保持唯一。
`track_active_indicator_space = 4` 则**确实**同时出现在导出与 Flutter（`trackGap = 4.0`），
所以轨道空隙照做。

### 4.16 日期输入（文本模式）的几条取舍（2026-11-04）

导出给 `md.comp.date-input.modal` 的是 **328×512** 的一块表面与 120 高的标题带。几何全按
令牌取，只有三处需要判断：

1. **512 照收，动作钉在底边。** 输入式对话框只有标题、一个字段与两个动作，内容不到 512；
   导出仍给这个高度（modal 日历与它共用同一块 modal 表面），因此中间留白，取消 / 确定贴底。
2. **120 是下限，不是定值。** Compose 的 `DatePickerHeader` 用 `defaultMinSize(120.dp)`，
   所以 `header` 取 `AxisSize::hug(120, …)`。但引擎当前不协商换行后的文本高度（缺口 #3
   的多行投影，见 engine-gaps #23），两行标题只按一行计入盒子——多出的那一行落进带内
   12dp 底衬与两行之间的余量里，视觉上仍不越界。
3. **日历开关放在标题行末端，不放进字段的尾部图标槽。** 导出与 M3 都把日历图标画在字段
   尾部，而本仓库的 `Md3TextField` 尾部槽只是绘制（不订阅点击），把它做成按钮会改动所有
   字段的契约。Flutter 的 `entryModeButton` 本来就在标题行（`DatePickerModeTogglePadding`
   = end 12 / bottom 12），因此按 Flutter 的位置放一个 40dp 图标槽，订阅点击并上报
   `DateInputAction::Calendar`；宿主没有日历面时 `calendar = false`，连这个动作也不再上报。

字段宽度 280 = 328 − 2×24，与 Compose 的 `InputTextFieldPadding`（start/end 24）同源。

### 4.17 divider / badge 的样例挂在列表页（2026-11-04）

两者此前只在表里打 ✅，**没有样例也没有自检**——按表看是「完成」，按"有东西能看"是空白。
它们不单开页面：MD3 里徽标挂在导航项 / 列表行的槽里，分隔线是列表行之间那条线，单独摆一个
徽标没有任何组合语义。因此样例落在 Lists 页的 MD3 列（`trailing badges` 组）：

- 胶囊徽标走列表项的后置槽（`ListItemSlot::Trailing`），顺带成为 `SlotFills` 的第一个使用例；
- 圆点徽标同一个槽，无标签；
- 全宽分隔线与 16dp 内缩分隔线各一条。

自检钉的是**令牌给出的数**而不是表里写的数：胶囊 21×16（`minWidth` 16 生效，标签 "9+" 撑到
21）、圆点 6×6、全宽线 356×1 与行同宽、内缩线 dx 16 / 宽 340。把内缩改成 24 时该行报红
（`inset dx 24.0 w 332.0 ✗`），改回 16 转绿。

顺带修掉一个既有隐患：这一列原本用固定 800 高（`COL_TALL_H`），加上这组之后末行被卡片切掉、
两行说明文字还叠在一起；改用 `column_hug` 让卡片随内容长高（页面上其他高列早已这么做）。

## 5. 引擎能力缺口
记录在 [`docs/engine-gaps.md`](engine-gaps.md)。每条包含：症状、为什么现在做不到、建议的引擎改动、临时绕过。

## 6. 已知坑（本任务内累积）

### 6.1 脚本化时钟会把后面的真实帧冻住（已修）

`GALLERY_SELFTEST` 用 `build_with_dt(.., 1.0)` 一帧推进一秒，把引擎时钟推到墙钟前面；
主循环随后用 `build()`（墙钟）算 `now = Instant::now() - epoch`，时间**倒退**，
所有 `committed_at` 在未来的实例永远读到 `from`。表现是：selftest 点过的控件在截图里
停在旧状态，而它的文字标签已经更新。

修法：`FrameClock::advance_by` 同时重设 `epoch`，使真实帧从脚本时间继续。
测试 `a_scripted_step_does_not_let_the_wall_clock_rewind_time`。
**看到「标签变了但控件没变」时，先怀疑时钟，而不是控件。**

### 6.2 截图前先确认窗口还活着

示例有 `GALLERY_SECONDS` 超时；超时后 `grim` 仍返回 0 并拍下整个桌面（旧项目的教训）。
截图窗口期内必须让进程常驻。

### 6.3 `tokens/` 里的文件名不可信

导出文件被截断过（`app-bars?.<hash>.json`），名字与实际内容无关。先 `gen_spec.py inspect`
看 `tokenSets`，再决定用哪个前缀。

### 6.5 导出的 `track.shape` 两侧内缩分在两个状态里

`md.comp.slider.xsmall.active.track.shape.leading = 8`，而 `trailing = 8` 只在 **inactive**
分支上。只解析 `slider_spec(active=true, …)` 会拿到 `trailing = 0`，轨道右端多出 8dp，
值映射与手柄位置一起偏。测试里必须先取两个分支再断言它们相等（`insets()` 助手）。
**同类陷阱**：任何「一侧令牌挂 active、另一侧挂 inactive」的导出都值得先 inspect 一遍。

### 6.6 截图单页需要 `GALLERY_PAGE`

gallery 默认停在 Switch 页，selftest 结束停在 Radio 页。`GALLERY_PAGE=<tag>`（如 `slider`）
让进程直接打开某一页，配合 `GALLERY_SECONDS` 常驻后 `nested-session.sh shot` 截图。

### 6.8 `GALLERY_DUMP` 拍到的不是你想看的那一页

开了 `GALLERY_SELFTEST=1` 时，selftest 会自己切页，结束时停在 **Radio** 页；`GALLERY_DUMP`
写的是退出前最后一帧，于是「`GALLERY_PAGE=buttons` + 自检 + 导出」得到的是 Radio 的画面。
要导出某一页的像素，**不要**同时开自检；自检与导出分两次跑。

### 6.10 导出的「基态」可能是一种真实外观

`md.comp.button` 导出的是**切换**按钮：`unselected` 不是无状态基态，而是一种真实外观
（`surfaceContainer` 容器、`onSurfaceVariant` 文字）。生成器若把变体轴的首值当作默认，
普通按钮就拿到切换按钮的未选中配色。做法是在 `VARIANT_GROUPS` 里显式加一个 `Base`
（不覆盖任何令牌）作为默认值。**遇到任何带 `selected`/`checked` 变体的导出，先问一句
「它描述的是不是切换控件」。**

### 6.12 导出把状态夹在对象的中间

`md.comp.fab-menu.primary.close-button.hovered.container.elevation` 的对象是
`close-button.container`，而变体态 `primary` 与交互态 `hovered` 分别落在它的**前面**和
**中间**。`gen_spec.py` 原先只认「对象在头」与「对象在尾」两种写法，于是这类令牌被静默丢弃，
`inspect` 里看不到任何 hovered/focused/pressed。现已加「对象各段按顺序但不连续」的贪心匹配。
**症状识别**：`inspect` 的 `params` 里少了本该有的交互态。

### 6.13 字级名藏在字号令牌的名字里

复合排版值指向 `md.sys.typescale.title-medium.size`，而该令牌的值是**裸数字**
（`{"fontSize": {"value": 16}}`），跟着引用走拿不到名字。字阶名要直接从
`fontSizeTokenName` 的路径第 4 段取。修好之后 button 的 `root.label_text` 也从 `"?"`
变成正确的 `label-large`/`title-medium`/`headline-small`/`headline-large`。

### 6.14 每个控件的完成判定

`src/spec/<c>_tokens.rs` 生成 + `src/<c>.rs` 实现 + `src/tests/<c>.rs` 全红验证 +
gallery 页面（含构建期变体）+ `GALLERY_SELFTEST` 通过 + 全量测试绿。少一样都算没做完。

### 6.15 禁用卡片在深色下比启用卡片**更亮**

`filled` 卡片的禁用容器是 `onSurface @ 0.38`（深色下 `onSurface` 接近白），叠在深色背景上
得到中灰 `(106,105,114)`，而启用态是 `surfaceContainerHighest` `(54,52,58)`。看起来像「禁用没
生效」，其实是导出的定义。判断禁用是否生效要看**是否有状态层与指针手型**，不要看亮度。
同理 `elevated` 禁用是 `surface @ 0.38`，因为底下还压着自己的阴影，实测 `(23,23,29)` 略深于
「`surface` 叠在列背景上」的闭式值 `(26,27,32)`——这是阴影参与合成的结果，不是错色。

### 6.16 列表行画出来是圆角，而控件声明的是方角

`Md3ListItem` 的 `Corners::all(0.0)` 是对的，绘制端拿到的 `radius` 也确实全零，但屏幕上
第一行方、第三行圆角 12dp。根因不在控件，而在**两个引擎缺陷**叠在一起：

1. `measure_node` 把父的 `available` 原样交给子节点，`fixed(380)` 的列仍按整窗宽量文字，
   列表行因此比列宽 86px（356 → 442.2）并溢出（engine-gaps #9）；
2. 溢出使行的 `mask` 比 `rect` 矮 1.5px，行于是 `push_clip` 一个方角裁剪；而
   `plan_batches` 的圆角取「最内层**非零**圆角」，外层面板的 12dp 就贴到了这个更窄的方角
   矩形上（engine-gaps #10）。

教训与 §1.5 里那条一致：**控件声明的属性正确，不等于屏幕上的形状正确**——中间还有测量与裁剪
两层。定位手段是逐图元打印 `rect`/`radius`/`owner`，再打印每次 `push_clip` 的 `mask`/`corners`，
两相对照才看出圆角不是控件画的。

### 6.18 `NodeKind::Box` 是叶子，不能挂子节点（同一坑第二次）

写 dialog 的遮罩根时打算用 `NodeKind::Box` 再 `.child(surface)`——`Decl::child` 不检查类型，
编译期不会报错，运行起来子节点根本不存在。动手前读 `NodeKind` 的注释才发现：
`Box` 是 "Leaf that paints a filled, possibly rounded box"（`ui-core/src/tree.rs:27`），
要装子节点只能用 `Column` / `Row` / `Stack` / `Overlay`。AGENTS §1.5 里已有同一条（
「`Container` 与 `Box` 可以自由组合」），这次仍差点重犯：**给节点加子节点前，先看 `NodeKind`
的注释里有没有 "Leaf"**。

### 6.19 无屏障时表面没有携带控件自己的 id

gallery 里三个内联预览全部查不到节点（selftest `0/3`），动作也不上报。根因：`decl()` 在
`barrier == false` 时直接返回表面，而表面的 id 是 `id.with("surface")`——控件自己的 id 从未
进入树，`cx.state(id)` 与 app 的 `rect_of` 全部落空。修法是让表面在无屏障时用控件自己的 id。

教训：**控件的根节点必须携带控件自己的 id**，否则悬停、点击、查找一起断。单测只检查 decl 的
结构，发现不了这件事；是 gallery 的「节点是否存在」检查把它逼出来的。

### 6.20 遮罩用了不透明黑

模态打开后整列变成纯黑，身后的预览完全看不见。根因：`scrim: role(pack, "scrim")` 直接取了
角色色（`a = 1`），而 `SCRIM_OPACITY = 0.54` 这个常量写了却没人用。改成
`spec_ink(pack, "scrim", SCRIM_OPACITY)`，并加一条测试断言 alpha。

教训：把「不透明度」抽成独立常量时，取值处极易忘掉它——而且**编译期不会有任何提示**（常量未
使用不告警）。凡是有 alpha 的令牌，测试就要断言 alpha 本身。

### 6.21 三个文字动作在 320dp 表面里放不下

`Not now / Save draft / Publish` 三个文字按钮排成一行宽 361.6，超出内宽 272，第三个被压成
`Pu`。Flutter 的 `OverflowBar` 在放不下时改成竖列，而引擎在声明阶段测不了文字
（engine-gaps #12），做不到。现行约定：表面宽度由应用给，动作标签必须能放进 `width − 48`；
gallery 改用短标签，并把「每个动作都在表面内」写进 selftest。

### 6.17 gallery 的 800 高列装不下 940 高的窗口

页头占 196px，`COL_TALL_H = 800` 的列放在 y≈208 处，底边落在 1008——超出 `WIN_H = 940`
的窗口。布局引擎的规则是「亏空则压向下限」，所以内容不会溢出窗口，而是**整体被压扁**
（分段行 56 → 54.7），最后一行贴着窗口底边被切掉。修法是把窗口改成 `1060` 高，而不是缩小
列高：列高由内容决定，压扁它只会把问题藏起来。

### 6.22 日历的「视图」不是控件能自己记的事

日 / 月 / 年三种主体只差一次点击，直觉上该由控件内部记住。但本仓库的 `Widget::decl(&self)`
是纯函数、`WidgetState` 只携带交互事实（hover/pressed/focused/…），控件**没有任何地方**存
自己的状态——因此视图与「屏幕上的月份」都必须是应用参数，和 carousel 的 `scroll` 一样。
三个连带后果：

1. 一次点击可能同时改变两件事（选中某月 → 月份变了**且**要退回日视图）。`Change` 一次只带
   一个值，`Change::Data` 里因此设计成 `Shown { date, view }` 把两个事实打成一个包，而不是
   让应用收到 `Shown` 再猜要不要切视图。
2. 格子是 `Decl`，它们自己的 `change()` 从不执行：构建期读 `cx.state(cell_id).clicked` 把动作
   记进 `fired`，再在 `change()` 里翻译成语义变化（与 carousel 同一套路）。
3. 网格恒为 6 行 42 格（含上月尾巴、下月开头），否则每换一个月表面高度都会变；导出给的是
   固定 456 高，跳变会带动整页重排。

另外两条小坑：`date-pickers.json` 三个 tokenSet **全是** `md.comp.date-picker.docked`，没有
modal / input 变体（别以为导出漏了）；`Icon::new` 的未知名字直接 panic，本控件只用了
`chevron_left` / `chevron_right` / `arrow_drop_down` 三个已存在的名字。

有意偏离导出的地方：月份 / 年份在导出里是下拉菜单的 `list-item`（48 高、选中行 `surface-variant`
+ 前置勾），这里做成 3×4 的格子，与日格同一套视觉语言，避免再引入一个浮层与其定位问题。

### 6.23 时间输入上报的必须是**文本**，不是数字

第一版把「缓冲放不进两位数字」上报成 `TimePickerChange::Hour(u32)`。在 `09` 前面输入 `0`
得到缓冲 `009`，裁成 `00`——12 小时制没有 0 点，`parse_hour` 返回 `None`，于是**什么都不报**，
应用的缓冲永远停在 `009`，输入格显示 `00`，两边再也不会一致。改成 `Hour(String)`（裁好的
文本，只要与参数不同就上报）后一帧收敛。教训：**凡是「请应用改写它自己的状态」的上报，
载荷必须足以让应用写出正确的新状态**；用一个「更语义化」的数字表达，会在边界值上丢掉信息。

同类观察：`period-selector` 的 52×72 是**整个**选择器，两块各 52×36（Flutter
`dayPeriodInputSize` 同值）；按「每块 52×72」实现会让表面高出一倍。

### 6.24 槽的外壳对布局是透明的（另一会话记入 design-errors，此处留指针）

裸槽（`Decl::slot`）用一个 `Stack` 外壳承载派生 id，父容器的 `cross_align` 会拉伸外壳而
内容不跟着拉伸；需要几何一致时用 `slot_with` 给出容器自己的 frame。详见
`docs/decisions/slots.md`。

### 6.25 search bar 的几处取舍

| 事项 | 决定 | 依据 |
|---|---|---|
| 形状与高度 | 全部取自 `container.height` / `container.shape`：56dp、半径 28（`Corners::all(H/2)`） | 导出写的是 `circular`，与高度绑定；写死 28 会在改高度时错位 |
| 查询怎么编辑 | 栏自身 `text_input`，输入节点 `nowrap` + `caret_in_text` 滚动（与文本字段同一套） | 搜索框是被编辑的，不是被阅读的；换行会把查询挤成两行 |
| 尾部动作上报什么 | 动作节点由**应用**命名（`SearchBarParams::action(id, icon)`），点击经 `Change::Data(SearchBarChange::Action(id))` 回来 | 导出没有动作语义，只有 `trailing-icon` 颜色；引擎不认识「麦克风」 |
| 聚焦环 | 画在栏自身边缘，不外移 `focus.indicator.outline.offset` | 声明期拿不到最终矩形，与 list / carousel 条目同一处让步 |
| 未做 | avatar 头像位、supporting text | `avatar` 一族令牌（30dp 圆形）已在表里，等有需要再接；supporting text 只有一行颜色令牌，没有位置令牌 |
| view 怎么展开 | **布局长出来**，不是位移或缩放：声明目标尺寸 + `transition_size`，引擎在布局前把宽高走过去；圆角 28→0、头 56→72、分隔线与结果透明度由控件自己的同一个 300ms 进度驱动 | Compose `SearchBarLayout` 就是这么做的：`height = lerp(startHeight, maxHeight, p)`、`cornerRadius * (1 - p)`、结果内容 `alpha = p` 且只在 `p > 0` 时组合。位移方案会把结果提前画在栏外面 |
| 收起时结果不参与布局 | `open_t > 0` 才挂分隔线与结果 | 同上：Compose 也只在 `p > 0` 时组合结果。静止的栏下面本来就没有东西，布局不该为它算一遍 |
| 展开/收起归谁 | 控件只上报 `SearchViewChange::Activated`（点静止的栏）与 `Action(id)`（尾部动作，通常是返回箭头）；`expanded` 由应用持有 | 与 dialog / sheet 同一约定：控件声明与上报，状态归应用；全屏摆放（浮层还是页内）也只有应用知道 |

## 7. 回看已有控件的待修清单（2026-10-29 定）
控件补全暂告一段，转入「回看已有控件」。用户已拍板：**先修用户报的 BUG，再按下表顺序做**，
最后补 engine-gaps #11–#13（三条都要做）。每条都要先给红/绿证据。

| # | 缺陷 | 证据 | 代价 |
|---|---|---|---|
| P2 | `NodeKind::Stack` 有声明无实现（当 Column 用） | 探针：Stack 内 40×40 与 60×60 → a(0,0)、b(0,40) | 低，无行为变更 |
| P1 | `Text::max_width` 完全无效 | 探针：`max_width=100` 与 `=∞` 在 800 宽列里逐像素相同（rect 800×91，wrap 800） | 行为变更，需核对约 8 个调用点 |
| P3 | 导出的 `line_height` 无人读取，行高来自字体度量 | `grep line_height`（除 spec/）= 0；真字体实测 24pt→29.75（令牌 32）、14pt 两行→34.4（应 40） | 中，外观变更 |
| P4 | `WidgetState::selected` 无人读取 | `state.selected` 0 处 | 极低 |
| P5 | `AxisSize::can_shrink` 无调用方 | 全仓 1 处（定义本身） | 极低 |

之后：engine-gaps #11（主轴定位）、#12（声明期文本测量）、#13（覆盖层过渡）。
