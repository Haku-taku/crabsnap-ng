# 三体系共通控件：WinUI 3 优先的移植计划

> 本文件是 2026-11-02 夜间自主迭代的**任务清单与进度**，随工作推进更新。
> 目标与约束来自用户原话（见 §1）；规格来源见 §2；架构决定见 §3；任务见 §4。
> 旁边的两份材料：控件清单与对应关系在 `docs/research/control-mapping-three-systems.md`，
> WinUI 逐控件数值在 `docs/research/fluent-reference-implementations.md`。

---

## §1 任务理解

**目标**：以 MD3 / Apple（Liquid Glass·SwiftUI）/ WinUI 3 三套体系的**共通控件**为范围，
用**规范且统一**的方式编写控件；每个体系各自忠实还原自己的**风格 + 参数 + 材质 + 行为**。
顺序上**先做 WinUI 3（Fluent）**，素材以 WinUI 3 的 XAML 源码与官方文档为第一权威，
Apple 侧以官方文档与 `/home/ollie/git/widget-ref/liquid_glass_widgets` 为参照。

**硬约束**（用户长期指令）：

1. 引擎能力不足时**报告并给出改造方案**，不静默近似、不绕远路；
2. 数值必须来自参考实现或文档，**不猜**；
3. 复刻要**忠实、完全 1:1**（以参考实现中最接近目标平台的那一路径为准）；
4. 需要彻底重构就彻底重构，**不为省事把引擎设计弄畸形**。

**不在本轮范围**：三体系中只有一方拥有的控件（FAB、chips、BreadcrumbBar、
PersonPicture 等）；媒体/平台能力（WebView、Ink、Map）；MD3 现有控件的大改，
除非共通控件的统一化必须触及它们（届时单独列条目并先报告）。

---

## §2 权威来源与口径

| 体系 | 第一权威 | 交叉验证 | 本地位置 |
|---|---|---|---|
| WinUI 3 | `microsoft-ui-xaml` 的 `*_themeresources.xaml`（模板、状态、`ThemeResource` 值） | Fluent UI React v9、`windows-reactor` | `/home/ollie/git/widget-ref/winui/`（按 SHA 固定，见 §2.1） |
| Apple | HIG + SwiftUI 文档 | `liquid_glass_widgets`、`flutter_liquid_glass` | `/home/ollie/git/widget-ref/` |
| MD3 | 官方导出令牌（已在 `ui-design/data/md3_data.rs`） | Flutter `material/*.dart` | 已在仓库 |

### 2.1 WinUI 源码固定

- 仓库：`microsoft/microsoft-ui-xaml`
- SHA：`868fef9ac230e79ab54a90a79d2048c939299c70`
- 已下载到 `/home/ollie/git/widget-ref/winui/`（文件名即 XAML 文件名）。
- `*_themeresources.xaml` 只管令牌与模板；以 C++ 组合实现的控件（ProgressBar / ProgressRing）
  另有 `controls/dev/<Control>/*.xaml` 与 `*.cpp`，已按同一 SHA 补进语料
  （`ProgressBar.xaml`、`ProgressBar.cpp`）。
- `dxaml/xcp/dxaml/themes/generic.xaml` 是**旧 UWP** 一代的主题文件（例：那里
  `SliderTrackThemeHeight` = 2，现代是 4），**不得**作为 WinUI 3 的依据，故不入语料。
- 引用格式：`Button_themeresources.xaml:123`。数值写进代码时，注释只留一行指针
  （AGENTS §6.2 的两行预算），出处细节在本文件与调研文档里。

### 2.2 判定"忠实"的检查点

1. 几何（min 尺寸、padding、圆角、描边、图标尺寸）逐项对上 XAML；
2. 颜色逐项对上 `ThemeResource` 名，而不是"看起来差不多"的近似令牌；
3. 状态矩阵（rest/hover/pressed/disabled/selected/checked/focus）逐项对上 VisualState；
4. 动效（时长、曲线）对上 `Control*AnimationDuration` 与 `ControlFastOutSlowInKeySpline`；
5. 行为（点击/键盘/开关语义/焦点移动）对上控件契约，而非只画皮。

---

## §3 架构决定

现状（2026-11-02）：

- `ui-widgets-md3` 成熟：每控件一个文件 + `spec/` 令牌层（官方导出生成）+ `Params` 构建器；
- `ui-widgets-fluent` 只有 slider / toggle / scaffold 三个控件，颜色查询混用 Web 令牌名；
- 共享层 `ui-widgets` 有一个包驱动的通用 `Button`，但它的回退逻辑（默认强调色填充、
  12pt 标签）不符合 WinUI 默认按钮，MD3 也没用它；
- `ui-style` 的 `MetricsSet` 只有 control/surface/overlay 三层，表达不了"每个控件一套几何"；
- 焦点视觉：每个 MD3 控件各自手写，没有共享的"双环"原语；
- 动效表：`MotionTable::fluent()` 是 Flutter 值（358 ms / EaseInOut），不是 WinUI 值。

因此本轮的四层分工（**这就是"统一"的含义**：同一套词汇、同一套构建模式，而不是同一个控件实现）：

| 层 | 位置 | 职责 |
|---|---|---|
| ① 数据 | `ui-design/src/winui/` + `data/winui_data.rs`（生成） | WinUI `ThemeResource` 全表（83 色 × 3 主题 + 圆角/动效键），带 `Source::Spec` |
| ② 风格包 | `ui-widgets-fluent/src/pack.rs` | 把 ① 接成 `Style`；Web 令牌名以别名指向 WinUI 名，不再两套并存 |
| ③ 规格 | `ui-style` 新增 `ControlMetrics` 模式；`ui-widgets-fluent/src/spec/` | 每控件几何/状态表，数值带 `file:line`；跨体系共用**模式**，各体系填自己的数 |
| ④ 控件 | `ui-widgets-fluent/src/<control>.rs` | 沿用 MD3 的 `Params` + 解析后结构 + `Widget` 模式 |

跨体系共享的**原语**（不是控件）放进 `ui-widgets`：

- `focus.rs`：双环焦点视觉（WinUI）与单环（MD3/Apple 已有形状）的统一构造；
- `control.rs`：`ControlMetrics`、密度、状态角色名的公共词汇。

不采用的方案：

- **不**把 Fluent 控件做成 `ui-widgets` 里通用 `Button` 的配置。MD3 与 WinUI 的按钮在
  尺寸语法、状态语义、图标 lockup 上差异过大，硬统一会退化成 `if system ==`。
  统一发生在**层与词汇**，不发生在控件实现。
- **不**用 `CommonColor` 当 Fluent 的主词汇。它是跨体系路径，保留给共享控件；
  Fluent 控件读自己的 `ThemeResource` 名，经 ② 解析。

---

## §4 任务清单

标记：`[x]` 完成　`[~]` 进行中　`[ ]` 待办　`[!]` 阻塞（写明原因）

### P0 地基（不新增控件）

- [x] **0.1 WinUI ThemeResource 表**
  - [x] `data/gen_winui.py`：解析 31 个 `*_themeresources.xaml`（含 `_any`），生成
        `data/winui_data.rs`：1191 条（Paint 876 / Thickness 127 / Gradient 3+）
  - [x] `ui-design/src/winui.rs`：`WinUISystem`（`Palette` + `StyleSystem`），
        `Source::Spec` 指到固定 SHA；`@system` 哨兵在高对比下拒绝而非涂红
  - [x] 覆盖率测试 8 条：链式解析、两主题非反色、Button 模板名全解析、高对比拒绝、强调色映射
- [x] **0.2 风格包换血**：`pack.rs` 改用 `WinUISystem`（Desktop 上下文）；所有控件改读 WinUI 键名
      （`ControlAltFillColor*`、`TextFillColor*`、`AccentFillColorDefaultBrush`…），Web 名不再有解
- [x] **0.3 `ControlMetrics` 模式**（`ui-style`）：`ControlKind` / `Density` / `ControlMetrics` /
      `ControlMetricsTable`，挂在 `Style.controls`，包用 `StyleBuilder::controls` 填入
  - [x] `ui-widgets-fluent/src/spec.rs` 录 P1 五个控件的 Standard 值（出处逐行标注）
  - [x] Compact 回退 Standard：WinUI 3 没有 density 字典（WinUI 2 的 `DensityStyles` 已移除），
        只有 NavigationView 紧凑窗格与 ListViewItem 紧凑选中条，不属密度；表结构保留，不臆造数值
- [x] **0.4 双环焦点视觉**（引擎）：`ui_core::draw::FocusVisual` + `Decl::focus_visual` +
      `view.rs::paint_focus_visual`，布局后画在盒子之外；测试钉住 2px/1px 与 −3 外扩
  - [x] 数值：外环 `FocusStrokeColorOuter` 2px、内环 `FocusStrokeColorInner` 1px（ListViewItem 显式写 2/1）
  - [x] 先接 Button / CheckBox / RadioButton / ToggleSwitch（P1 随控件接线）
        补记：ToggleSwitch 原先漏接，现按 `FocusVisualMargin -7,-3`、2/1 双环接上；
        测试 `a_switch_hangs_its_focus_ring_at_the_template_margin`（拆掉接线会红）
- [x] **0.5 动效表**：新增 `MotionName::ColourChange`（WinUI `BrushTransition` 线性 83ms）；
      `MotionTable::winui()` = 83/167/250，曲线 `Cubic(0,0,0,1)`；Fluent 包已使用
- [x] **0.6 图标集**：`FluentSystemIcons-Regular.ttf`（MIT，`microsoft/fluentui-system-icons`）随 gallery 分发，
      注册到 `ui_widgets_fluent::glyphs::ICON_FAMILY`；`scripts/gen_fluent_codepoints.py` 从 cmap 生成
      `ui-widgets-fluent/src/fluent_codepoints.rs`（16 个已用名字）；字号与 Segoe Fluent Icons 一致
  - [x] Segoe Fluent Icons 是专有字体，不可分发；Fluent System Icons 是同源的 MIT 公开版，形状一致
- [~] **0.7 Gallery 骨架**：Fluent 列已真列化（buttons / checkbox / radio / switch / slider /
      progress / text-field）；其余页面随控件落地
  - 模式：`FluentKit::{button,checkbox,radio,...}()` + `scoped.element(control)` + `column(page,"fluent",...)`，
    glass 仍用 `pending_column`

**P0 验收**：`cargo test -p ui-design`、`cargo test -p ui-style` 绿；每个 P1 控件用到的 key 都能解析；
`cargo build --workspace --tests --examples` 零 warning；嵌套会话 `GALLERY_PAGE=fluent-system` 截图无洋红。

### P1 基础控件（三体系共通，WinUI 先）

- [~] **1.1 Button**（Default / Accent / Subtle）
  - 已落地 `ui-widgets-fluent/src/button.rs`：三族画刷表（`Button*` / `AccentButton*` / `SubtleButton*`），
    背景走 `ColourChange` 83 ms `BrushTransition`，前景与描边按 `DiscreteObjectKeyFrame` 瞬切，
    描边取 `stroke_of()`（立面渐变含翻转），焦点环 2/1 px、外扩 −3
  - 几何全部来自 `Style.controls`（`spec.rs` 的 Button 行），控件里没有尺寸字面量
  - 测试：三变体各读自己的画刷族、禁用 > 按下 > 悬停（故意破坏验证过会红）
  - [ ] 图标 + 标签 lockup（参考实现用 `StackPanel Spacing="8"`，需先找到权威出处，不臆造）
  - [x] TextLink（`HyperlinkButton_themeresources.xaml` 自成一份模板）：强调文字色族
    `HyperlinkButtonForeground*`、静止填充 `SubtleFillColorTransparent`、四态描边全透明；
    几何与 Default 共用（同 `ButtonPadding`、同 `ControlCornerRadius`）
  - 测试：`a_text_link_is_accent_ink_over_no_fill_until_pointed_at`（改成 `SubtleButton*` 会红）
- [x] **1.2 CheckBox**：`ui-widgets-fluent/src/checkbox.rs`
  - 20 方块 / 12 字形 / 行高 32 / minWidth 120 / gap 8（padding 左）/ corner 4 / border 1 / focus −7,−3
  - 三值画刷表（Unchecked/Checked/Indeterminate × rest/hover/pressed/disabled），全部按模板名读
  - 字形用 Fluent 图标字体（`checkmark_12_regular` / `subtract_12_regular`），与模板的 fallback FontIcon 一致
  - [!] 三态语义仍受引擎缺口 #4 限制：混合态点击上报 `on`（与 MD3 同口径），已在 `change()` 注明
- [x] **1.3 RadioButton**：`ui-widgets-fluent/src/radio.rs`
  - 20 外圈 / 圆点 12（悬停 14、按下 10，`Standard` 250 ms）/ 按下态额外 4→10 点（`Quick` 167 ms）
  - 行高 32 / minWidth 120 / gap 8；画刷表按 `RadioButton*` 名读
  - [!] 按下小点的透明度与直径用 `NodeCx::animate` 驱动，按 XAML 的 `LinearDoubleKeyFrame` 即瞬时到位
- [x] **1.4 ProgressBar**：`ui-widgets-fluent/src/progress.rs`，对照 `ProgressBar.xaml` + `ProgressBar.cpp`
  - 槽高 3（`ProgressBarMinHeight`）、轨道 1（`ProgressBarTrackHeight`）/ 圆角 0.5、
    指示条圆角 1.5（`ProgressBarCornerRadius`）、根节点按 `TemplateSettings.ClipRect` 裁切
  - 确定态宽度 = `maxIndicatorWidth × (value-min)/(max-min)`；track 用 `ProgressBarBackground`
    （`ControlStrongStrokeColorDefault`），填充 `ProgressBarForeground`，
    错误 `SystemFillColorCritical`、暂停 `SystemFillColorCaution`
  - 不确定态：两条扫动，周期 2 s（`ProgressBar.cpp` 的 `ContainerAnimation*`）——
    第一条 40% 宽，1.5 s 内 -40%→120%（`ContainerAnimationStart/EndPosition`），余下 0.5 s 保持；
    第二条 60% 宽，静止 -90% 到 0.75 s，再以同曲线到 99.6%；曲线 `0.4,0,0.6,1`
  - 百分比定位：两条指示条用**左右百分比内边距**夹出宽度（CSS 绝对定位语义），
    控件因此不需要知道自己的像素宽度（引擎侧见 E9）
  - 时钟：只有不确定态读 `NodeCx::age`（引擎侧见 E10），确定态一帧也不多要
  - 偏离：`RepositionThemeAnimation`（值变化时新增长度滑入）未接——主题动画的时长不在已固定
    的 XAML 语料里，`[未证实]`，暂按离散更新；`IndeterminateError/Paused` 的换相编排属 P4
  - gallery `progress` 页 Fluent 列：0/25/60/100 + 错误 + 暂停 + 不确定态；两次截图确认扫动在走
- [x] **1.5 ProgressRing**：`progress_ring.rs`，确定态 + 旋转态，均由 Lottie 数值直接换算
  - WinUI 3 的环**不是 XAML 模板**：`ProgressRing.cpp` 用 `AnimatedVisualPlayer` 播 Lottie，
    生成代码在 `AnimatedVisuals/ProgressRingDeterminate.cpp` / `...Indeterminate.cpp`
  - 几何：确定态画布 32、椭圆半径 8、精灵缩放 1.77、描边 1.5 → 中线 14.16、线宽 2.655；
    旋转态画布 80、半径 7、缩放 5 → 中线 14、线宽 3（两种态线宽**本来就不同**）。
    `Sector` 以外缘为半径，故取「中线 + 线宽/2」
  - 确定态：`TrimEnd` 走 Lottie 自己的关键帧表（0/25/50/75/98.33/99.17%），端点 1e-4；
    该曲线对称（控制点 x=y），除最后 2% 外弧长即数值。0% 因此是一个圆帽点，与 WinUI 一致
  - 旋转态：两秒一圈 900°，每半圈单独缓动（中点半圈处停顿）；前半环头从 0 长到 0.5，
    后半环尾巴从 0 追到 0.5（两个精灵按不透明度硬切换）。弧的零点在**十二点**，
    弧图元的零点在三点，控件里减四分之一圈
  - 轨道：`ProgressRingBackgroundThemeBrush` 默认是 `ControlFillColorTransparentBrush`，
    WinUI 3 的环没有可见底环；仍按令牌画出（透明）
  - 时钟：只有旋转态读 `NodeCx::age`；`IsActive=false` 时不画也不读表
  - gallery `progress` 页 Fluent 列：0/25/60/100 + 旋转 + 未激活；截图量得外径 31、
    描边 2.6（对应 15.49/2.655），两帧差 204 像素确认在转
- [x] **1.6 修正 ToggleSwitch**：`toggle.rs` 对照 `ToggleSwitch_themeresources.xaml` 复核
  - 轨道 40×20 / 圆角 10 / 描边 1（`ToggleSwitchOuterBorderStrokeThickness`），
    knob 格子 20×20（`SwitchKnob`），格子沿轨道走 0→20（`TranslateTransform`），knob 居中于格子
  - knob 12 静息 / 14 悬停 / 17 选中按下 / 14 未选中按下（`SwitchKnobOn|Off` 关键帧）
  - 选中态描边 0（`ToggleSwitchOnStrokeThickness` = 0），未选中描边 `ToggleSwitchStrokeOff`
  - 上行 `ToggleSwitchPreContentMargin`/`PostContentMargin` 各 10 → 行高 40（已由 metrics 表承担）
  - 动效 83 ms `ControlFastOutSlowInKeySpline`（`Cubic(0,0,0,1)`），颜色关键帧线性同钟
  - 偏离：`SwitchKnobOn` 的 `BorderBrush=CircleElevationBorderBrush` 实际 `BorderThickness` 为 0，
    模板里不可见，故未画；knob 在格子内的 ±0.5 偏移（`Margin=0,0,1,0` / `-1,0,0,0`）未复刻（亚像素噪声）
- [x] **1.7 修正 Slider**：`slider.rs` 对照 `Slider_themeresources.xaml`
  - track 4 / 圆角 2（`SliderTrackThemeHeight`、`SliderTrackCornerRadius`）、行高 32（`SliderHorizontalHeight`）
  - thumb 盒 18，外框 `Margin="-2"` → 22、圆角 10（`SliderThumbCornerRadius`）、
    填充 `SliderOuterThumbBackground`、描边 `SliderThumbBorderBrush`（立面渐变，含明色镜像）
  - 内点 12 静息 / 14 悬停 / 10 按下 / 14 禁用；静息与禁用 83 ms、悬停与按下 250 ms（模板里两组时钟不同）
  - 轨道内缩 9（半个 thumb）：`SliderHorizontalThumbWidth/2`，滑块中心 9..231
  - 头部 `SliderTopHeaderMargin` 0,0,0,4；禁用时 `SliderHeaderForegroundDisabled`
  - gallery 的 `slider_track_h: 8` 覆盖已删除，`FluentOptions` 默认 4
  - 修：根节点必须带控件 id（`WidgetState::of(cx, decl.id)` 与命中测试都按声明根取），
    此前把 sense 挂在内层 `bar` 上会让滑块收不到交互

**P1 验收**：五个控件在 gallery 的 Fluent 列可交互；状态矩阵截图；单元测试只钉几何/状态/时长；
`GALLERY_SELFTEST=1` 通过。

### P2 文本、列表、页签、提示

- [~] **2.1 TextBox**：`ui-widgets-fluent/src/textbox.rs`，对照 `TextBox_themeresources.xaml`
  - 高度 32 / 最小宽 64（`TextControlThemeMinHeight/MinWidth`）、padding `10,5,6,6`、
    圆角 4（`ControlCornerRadius`）、标题间距 8（`TextBoxTopHeaderMargin`）
  - 四态（Normal / PointerOver / Focused / Disabled）互斥且焦点优先，逐项读
    `TextControlBackground*` / `TextControlBorderBrush*` / `TextControlForeground*` /
    `TextControlPlaceholderForeground*`；描边走 `stroke_of`（立面渐变，含明色镜像）
  - 清除按钮：宽 30、`TextControlButtonForeground` 的 `dismiss_12_regular`（Fluent 图标字体），
    有文字时出现
  - 输入：根节点 `text_input(buffer)` + `Sense::FOCUSABLE`，插入符/组合/选区由引擎给，
    `NodeCx` 新增 `caret()/preedit()/selection()` 快照（此前只有 `Cx` 能读）
  - [ ] 遗留：`TextControlBorderThemeThicknessFocused` = `1,1,1,2` 的**逐边宽度**引擎不支持，
    现在用「1px 均匀描边 + 聚焦时底部 1px 细条」逼近（见 E12）；校验/描述行、多行、
    清除按钮的 hover/pressed 底色（需要子节点状态）尚未接
  - [ ] 标题色：WinUI 3 的字典没定义 `TextControlHeaderForeground`（只在旧 generic.xaml 里），
    暂用 `TextFillColorPrimaryBrush` 等价，已在代码注明
- [!] **2.2 ListView / ListViewItem**：暂缓——WinUI 3 仓库只有 `ListViewItem_themeresources.xaml`
      （令牌），**模板在 OS 的 generic.xaml 里**，仓库内没有 `ListViewItemPresenter` 的实现可对照；
      行内边距/指示条几何只能引 WinUI 2 或系统文件，不满足「对照本仓 WinUI 3 源码」。先推进
      WinUI 3 仓库自带模板的控件（2.3–2.5），ListView 等到有 OS XAML 语料或改用 WinUI 3 的
      `ItemsView`（`controls/dev/ItemsView`）再定。行 minHeight 40 / minWidth 88 两条已由令牌确认
- [x] **2.3 TabView**：header padding `0,8,0,0`、item `8,3,4,3`、选中 `9,3,5,4`、
      minHeight 32 / minWidth 100 / maxWidth 240 / font 12 / icon 16 / close 32×24
  - 结构照 `controls/dev/TabView/TabView.xaml` 的 `TabContainerGrid`：四列 `Auto | Auto | Auto | *`
    ——页头列表取自身宽度（`Grow::No`），新建按钮紧跟最后一个页头；底部细线横贯整条带，
    选中页头用 1px 同色块盖住它，于是与下方内容并成一块
  - 页头宽度 = 内容夹在 `[100, 240]`；实测短标签 100（pitch 100、选中块 100×31、顶部圆角 8）
  - 关闭 32×24、`dismiss_12_regular`；分隔线在选中项、其左侧一项、悬停/按下时隐藏
    （`SetTabSeparatorOpacity`）；关闭走 `Change::Data(TabViewChange::Closed)`，
    选中走 `Change::Selected`，新建按钮走 `Change::Activated`
  - 横向滚动：列表 `Clip { x: Scroll }` + `Sense::SCROLL_OWNED`，溢出时列表收缩后内部滚动
  - 溢出箭头：`‹`/`›` 32×24、字形 8、左右容器内衬 `8,0,3,3` / `3,0,8,3`；溢出才出现，
    偏移 0 处左箭头禁用、到头右箭头禁用（`UpdateScrollViewerDecreaseAndIncreaseButtonsViewState`），
    每步 50dp 缓动（`Change::ScrollBy` → 引擎找最近的滚动容器，engine-gaps #42）
  - 箭头连发与拖动重排（`CanReorderTabs`）见 P4
  - 引擎改造（同批）：engine-gaps #39（Fit 的 hint 被当基准下限）、#40（流内 margin 被丢弃）、
    #41（flex 项从不收缩）
- [x] **2.4 ToolTip**：font 12、padding `9,6,9,8`、maxWidth 320、corner 4、1px
      `SurfaceStrokeColorFlyoutBrush`、背景 `AcrylicInAppFillColorDefaultBrush`、`TextWrapping=Wrap`
  - 数值全取 `ToolTip_themeresources.xaml`：`ToolTipContentThemeFontSize 12`、
    `ToolTipBorderPadding 9,6,9,8`、`ToolTipMaxWidth 320`、`ControlCornerRadius 4`、
    `ToolTipBorderThemeThickness 1`；`MaxWidth` 封整块面板（含内衬），文字在内部换行
  - 丙烯酸取 `AcrylicBrush_themeresources.xaml` 的暗/亮两列（`#2C2C2C/0.15/0.96`、
    `#FCFCFC/0/0.85`），面板盒形保持透明——着色由 acrylic 那一趟做
  - `OpenStates` 的 `FadeInThemeAnimation`/`FadeOutThemeAnimation` = `ControlFastAnimationDuration`
    0.167s 线性；`IsOpen=false` 时面板什么都不画，应用可让它留在树里走完淡出
  - 延时归应用：Windows 的 `SPI_GETMOUSEHOVERTIME`（默认 400ms）之后才展开，
    画廊页用 `cx.age()` 的逐帧读取撑住这段延时
  - [!] 偏离：`BackgroundSizing=InnerBorderEdge` 未建模（边框压在内衬上，视觉差 1px）
  - 引擎改造（同批）：engine-gaps #43（组不透明度层内的背景模糊被跳过）、
    #44（层的不透明度变化不产生脏区）、#45（浮层命中推后而绘制不推后）
  - 关闭态也声明为浮层：0×0 的流内占位会占掉 Column 的一个 gap 槽位，悬停时把页面顶动
- [x] **2.5 InfoBar**（WinUI 独占，但与 MD3 snackbar / Apple 系统提示同角色）：
      minHeight 48、四种 severity、close 38
  - 数值全取 `InfoBar_themeresources.xaml`：`InfoBarMinHeight 48`、`InfoBarContentRootPadding
    16,0,0,0`、`InfoBarIconMargin 0,16,14,16`（16pt 图标）、`InfoBarPanelMargin 0,0,16,0`、
    `InfoBarCloseButtonSize 38` + 5 边距 + 16 字形、title/message 14、圆角 4、描边 1
  - 标题/消息间距按朝向两套：同排 `InfoBarTitleHorizontalOrientationMargin 0,14,0,0` 与
    `InfoBarMessageHorizontalOrientationMargin 12,14,0,0`；堆叠时消息改 `0,4,0,0`
    （`Decl::margin_alt`）。堆叠判据是 `AxisSwitch { max_cross: Some(48) }`——某个子节点
    高过单行高度就换向，与模板的 `StackPanel` 换向同源
  - 四种 severity 各取三支画刷（`BackgroundBrush` / `IconBackground` / `IconForeground`）；
    informational 的「i」自带字面只有带圈版本，按 2pt 点 + 6pt 竖条自拼
  - 关闭按钮贴右上（模板 `VerticalAlignment=Top`），命中区由 `close_box(width)` 给出
    （没有自己的 element）；`IsOpen=false` 走流外 0×0，不占间隙也不绘制
  - [!] 偏离：`ActionButton`（模板里消息右侧的按钮槽）未做；关闭按钮的 `SubtleButton`
    状态只取悬停/按下/禁用三档底色，没有按下的缩放动画
  - 引擎改造（同批）：engine-gaps #46（收缩后的子内容按旧宽度排版）、#47（子节点复用父
    节点 Id 时静默合并）；`Decl::box_model` 从 `ContainerParams` 上移，叶子才能带 margin；
    `ui_text` 新增 UAX#14 的 `min_content_width` 并接进三条测量路径（见设计错误表）
  - 测试：`each_severity_reads_its_own_background_and_icon_brushes`、
    `a_closed_info_bar_takes_no_space_and_paints_nothing`（各先红后绿）

**P2 验收**：`text-field`、`lists`、`tabs`、`tooltip`、`info-bar` 页的 Fluent 列；
截图；selftest。

### P3 浮层与导航

- [x] **3.1 ContentDialog**：320/548 宽、24 padding、corner 8、title 20 semibold、
      按钮间距 8、遮罩、焦点陷阱
      落地：`FluentContentDialog` + gallery `dialogs` 页 Fluent 列（三种按钮组合）；
      Enter 按默认按钮、Escape 按关闭按钮（`DIALOG_DEFAULT_SHORTCUT` / `DIALOG_CLOSE_SHORTCUT`，
      走 engine-gaps #6 修好的 `WidgetState::shortcut`）；模态出现即接管焦点（`settle_focus`）。
      未做：开合 1.05→1.0 缩放（engine-gaps #27 无变换原语）。
- [x] **3.2 MenuFlyout**：corner 8、item padding `11,8,11,9`、corner 4、acrylic、键盘导航
      落地：`FluentMenuFlyout`（presenter 0,2,0,2 + 8 圆角 + 1 描边 + 应用内丙烯酸；条目 4,2,4,2
      外边距、11,8,11,9 内衬、4 圆角、16 图标槽 + 28 占位缩进、加速键右列、1dp 分隔线 -4 外扩；
      焦点环按 XAML 的「Margin 在控件内侧」语义包住 4,2,4,2 外边距，2px 主环 + 1px 次环、方角）
      + gallery `menu-flyout` 页 + selftest（轻关闭、锚点重开、下方/起点对齐、行边距、键盘走行、
      Escape、选中收起）。
      引擎：`Tree::popup_root`（以 `Sense::FOCUSABLE` 认「要键盘的浮层」）、`Cx::dismissed` /
      `Change::Dismissed`（外部按下）、`settle_focus` 浮层接管并交还焦点、方向键在浮层内环绕、
      Home/End 跳首尾；`focusables` 排除作用域根、并把「声明了焦点环」也算作 Tab 停靠点
      （菜单行静息底色透明，否则整张菜单不可达）。四条测试先红后绿。
      未做：ThemeShadow（无 XAML 令牌，不猜数值）、50ms 展开过渡（engine-gaps #27）、
      `MenuFlyoutSubItem` / `ToggleMenuFlyoutItem` / 触控 vs 笔的窄内衬判定（引擎无输入设备事实）、
      `FlyoutThemeMinWidth`（本地语料无此键，[未证实]）。
- [x] **3.3 ComboBox**：64/32、padding `12,5,0,7`、popup acrylic + corner 8、chevron 12
      落地：`FluentComboBox`（最小 64×32、内衬 12,5,0,7、1dp 描边、4 圆角；字段药丸 3×16 只在
      持焦点时出现，占内容的前导 12 而不额外加宽；箭头 12 字形在 38dp 列里、距右缘 14；
      面板应用内丙烯酸 + `OverlayCornerRadius` 8 + 浮层描边，锚在字段下方 0.5dp，
      `match_anchor_width` 跟随字段宽；内容边距 0,4；行外边距 5,2,5,2、内衬 11,5,11,7、
      3 圆角、选中药丸 3×16 吃掉 11dp 前导，标签因此同列；条目焦点外扩 -3）。
      行为：打开落在选中行（`focus_initial`），Escape / Alt+Up 关闭、Alt+Down 打开、
      Up/Down/Home/End 步进（回绕、跳过禁用），外部按下轻关闭；`Change::{Selected,
      Dismissed, Activated}`。+ gallery `combo` 页（带 header 的活字段、空提示、禁用三个样本
      + 读出）+ selftest（面板在下方/同宽/起点对齐、行边距与前导、药丸尺寸与居中、打开落点、
      方向键、Escape、选中收起、外部按下收起）。
      令牌缺口：`ComboBoxDropDownGlyphForegroundFocusedPressed` 的明暗列指向
      `SystemControlHighlightAltBaseMediumHighBrush`（旧别名，固定语料里只有引用没有定义），
      改用 `ComboBoxForegroundFocusedPressed`——它在三个主题列上与前者等值；
      `ComboBoxDropDownBackground` 是应用内丙烯酸画刷，由背景趟绘制，盒子本身只留形状。
      未做：开合过渡（收起即卸载，engine-gaps #27）、`MaxDropDownHeight 504` 的滚动
      （需要滚动容器，见 engine-gaps 的滚动条目）、`IsEditable` 可编辑变体、
      按下时药丸 0.625 的竖向缩放动画。
- [x] **3.4 NavigationView**：compact 48 / open 320、item minHeight 36、icon 16、指示条 3×16
      落地：`FluentNavigationView`（窗格宽展开 320 / 折叠 48；条目外边距 4,2、行高 36、
      图标盒 40×16 内居中 16dp 字形、标签前导 4、展开时右侧留 14；选中条 3×16 圆角 2，用
      `margin(left:-3)` 叠在图标盒前而不挤动它；内容区 `LayerFillColorDefault` + 顶/左各 1dp
      `CardStrokeColorDefault` 描边、展开时左上圆角 8、打开态左侧再让出 1dp；开关 40×36 外边距
      0,4）。条目填充/墨色按 `NavigationViewItemBackground{Selected,Disabled}` 与
      `NavigationViewItemForeground*` 四态取，开关按 `NavigationViewButton*` 取。
      行为：开关发 `Change::Toggled{on}`，条目发 `Change::Selected`（禁用条目不发）。
      + gallery `nav-view` 页（四个目标 + 读出正文）+ selftest（窗格宽度、内容相邻、条目内外距、
      图标盒与字形居中、标签前导、指示条尺寸/贴边/居中/只在选中项、折叠与展开、点选后指示条跟随）。
      偏离：语料里没有汉堡字形，开关用 `chevron_left_12_regular`（代码内有注释）；
      未做折叠/展开过渡动画、PaneFooter/Settings 项、InfoBadge、层级条目、Top 显示模式
      （P4 单列）、折叠窗格的丙烯酸底。

**P3 验收**：四页 Fluent 列；浮层 hit/placement 测试；截图。

### P4 变体与高级状态

- [x] Button 变体全状态（Accent/Subtle/TextLink，见 1.1）
- [x] CheckBox indeterminate（引擎缺口 #4 已修）
      落地：`Change::Toggled { on: Option<bool> }`（`None` = 不确定，两态控件只报 `Some`）；
      `CheckBoxParams::three_state()` 按 `ToggleButton.OnToggle` 循环 未选 → 选中 → 不确定 → 未选。
      gallery Fluent 列新增 `Select all` 样本（初始不确定）；MD3 保持 material-web 语义
      （点不确定态落定为选中），gallery 的 `ftri` 走 Fluent 那条。
      验证：单测 + selftest `fluent three-state box walks off→on→mixed ✓`，反向改 `Checked → Some(false)` 先红。
- [x] TabView 箭头的按住连发（`RepeatButton Delay="50" Interval="100"`）
      落地：`scroll_repeat_steps(held)` + `NodeCx::age`（读它即请求每帧，指针不动也能连发）+
      `NodeCx::forget_age`（松开即重置）；一帧内按下又松开的轻点仍补一步。
      顺带修掉一个真 BUG：箭头在列表**旁边**，`Change::ScrollBy` 只沿祖先找容器，
      于是箭头一直什么都不滚。`ScrollBy` 增 `owner: Option<Id>`，见 design-errors。
      验证：单测 `a_held_scroll_button_steps_on_the_template_cadence`；selftest
      `fluent strip arrow steps 50.0 ✓` 与 `... repeats 106.6 over 480 ms ✓`
      （两处各自反向验证过：`owner: None` → 0.0；`steps` 恒 1 → 50.0）。
- [x] TabView 拖动重排（`CanReorderTabs`）
      落地：页头越阈值即浮起（`Sense::DRAG` + 累积 `drag_step` 作 `.offset`），被越过的页头
      以 `.offset` 让开一个页头宽（不能用 margin：外边距会进布局，重排的预览会喂回下一帧的
      几何而抖动）；落点由槽位几何算出（`slot_at` + 累积宽度 `edges`），**不**取指针下的节点——
      缺口里没有任何节点接受放下。`TabViewChange::Dropped { from, to }` 按下标上报。
      验证：selftest `fluent drag slides the crossed header ✓` 与 `fluent drag reorders
      [1, 2, 0, 3] ✓`（两处各自反向验证过：读不到几何 → 不滑动；落地帧不量几何 → 报不出落点）。
- [x] NavigationView display mode（Minimal/Compact/Expanded）
      落地：`PaneDisplayMode::{Expanded, Compact, Minimal}` 对应 WinUI 的 `Left` /
      `LeftCompact` / `LeftMinimal`。`Expanded` 是原先的行为（`open` 就地 48 ↔ 320）；
      `Compact` 把 48 窄条留在流内，展开时窗格盖在内容上（内容不动）；`Minimal` 没有窄条，
      开关自成 44 高的一行位于内容上方，展开的是同一块浮层。浮层：`OverlayPlacement`
      锚在视图自身、`Side::Left` + `offset:-OPEN_PANE_W`（以自身宽度压住宿主，起于左缘）、
      `match_anchor_height`（新增：高度取锚点而非视口）、`Sense::FOCUSABLE` + Escape，
      因此外部按下与 Escape 都走引擎的 `Change::Dismissed`。浮层用
      `NavigationViewDefaultPaneBackground`（应用内丙烯酸）+ 右缘 1dp
      `CardStrokeColorDefaultBrush`（顶替参考的 `ThemeShadow`）。展开的浮层盖住窄条，
      窄条的条目因此退出树（两个节点不能共用一个 id）。
      gallery `nav-view` 页新增三个模式按钮（当前模式用 Accent）；应用在
      `Compact`/`Minimal` 下选中目标后收起窗格。
      验证：selftest 六行（compact 保留 48 窄条、展开 320 盖住内容且内容不动、外部按下关闭、
      minimal 无窄条且开关在内容上方、minimal 同一块浮层、Escape 关闭、回到 expanded），
      三处各自反向验证过（`offset: 0` → 起于视图左侧；`match_anchor_height(false)` → 204 高；
      不读 `dismissed` → 外部按下不关）。
      引擎：`OverlayPlacement::match_anchor_height` + `.flip(bool)`；单测
      `an_overlay_spanning_its_anchor_takes_the_anchors_height` 先红后绿。
      未做：`Top` 显示模式、开合过渡（engine-gaps #27）、`Auto` 模式（需要窗口宽度事实）。
- [x] ProgressBar error/paused（`demonstrate` 已带 `error`/`paused` 参数与颜色，见 1.4）
- [x] InfoBar closable（随 P2.5 完成：`closable` 参数 + `Change::Activated`，命中区 38）

### G1 Liquid Glass（Apple 档，`ui-widgets-glass`）

参照 `/home/ollie/git/widget-ref/liquid_glass_widgets`（Flutter 实现）与 HIG。材质在
`material.rs`，弹簧与共享图元在 `shared.rs`，每控件一个文件。

- [x] **GlassButton**（`button.rs`）：Filled / Prominent / Transparent 三档；56 方形图标胶囊、
      44 高文字胶囊；按下立面按最长边外扩 `BUTTON_PRESS_GROWTH`，环境光随 hover/focus 抬升。
      gallery `buttons` 页玻璃列，selftest 四条（形状、按压计数、禁用不响应、立面外扩）。
- [x] **GlassSegmentedControl**（`segmented.rs`）：32 高、2 内衬胶囊；滑块与 tab bar 共用
      `shared::lens_*`（即参考的 `AnimatedGlassIndicator`）；图标/文字两式，逐段禁用 0.38，
      选中 13pt w600 / 未选 w500 @60%。gallery `segmented` 页玻璃列 + selftest。
- [x] **GlassMenu**（`menu.rs`）：200 宽 / 32 圆角 / 12 内衬 / 行 44 高相距 2；行内 16 内衬 +
      20 图标 + 12 间距，标题 17pt w400 @90% 墨色，悬停 10% / 按下 15% 白；分隔带 12 高；
      禁用 0.4；Escape 与外部按下 → `Change::Dismissed`。行声明 `focus_ring`
      （1.5dp `systemBlue`、外扩 3dp，源自 `GlassFocusRingPainter`），既画焦点环也让它成为
      Tab 停靠点——否则透明底的行不进入键盘作用域。gallery `menu-flyout` 页玻璃列，
      selftest 三条（几何/前缘对齐、选中上报、Escape 关闭）。
      **偏离**：未做参考的触发器圆形 morph、J 曲线飞行、子菜单、滑动导航、全屏 dismiss barrier，
      只保留弹簧淡入（`reveal`）。
- [x] **GlassChip**（`chip.rs`）：12/8 内衬胶囊、6 间距、16 图标/删除、14pt w500；
      选中底色 = 基准墨 10%（浅）/30%（深）；按压 1.03 等比；删除字形自带盒子
      （文本节点不接指针）并点名 `id.with("delete")`；点击/键盘激活/删除分别上报。
      gallery `chips` 页玻璃列 + selftest 四条，三条反向验证过。
- [x] **GlassProgress**（`progress.rs`）：圆环 20dp / 描边 2.5dp，`Sector` 的半径取外缘（= 盒边）、
      厚度向内长；横条 4dp 高、最窄 200dp、扫动条占轨道 30%。扫动每秒一圈（`Cx::age` 让帧循环自持），
      确定态不读时钟。轨道按 `systemFill` 语义自适应（浅色 15% 黑 / 深色 15% 白）。
      gallery `progress` 页玻璃列 + selftest 三条（直径 20/40、条高与最小宽、填充 = 数值比例），
      填充比例做过反向验证。
      **偏离**：① 参考的 4dp 模糊辉光改为更宽的半透明描边——引擎没有弧的模糊，硬边的宽描边
      会读成第二道环；② 横条扫动用百分比内边距 + 轨道裁剪，参考则让浮条越出轨道两端。
- [x] **GlassCard / GlassContainer**（`card.rs`）：`card` 预设 16dp 内衬 + 12dp 连续圆角，
      `container` 无内衬 + 16dp 圆角；`solid` 表面取 iOS 内嵌分组卡片的白 / `#1C1C1E` 与 26dp 圆角。
      本体走新增的 `Control::Container`（`LiquidGlassSettings()` 默认：厚度 30、磨砂 3、受光 2、
      色散 0.5，另加 6% 洗色）。宽度填满、高度贴合内容，子节点按声明顺序排成一列。
      gallery `cards` 页玻璃列 + selftest 三条（16 内衬、高度 = 内容 + 32、普通容器无内衬）。
      **偏离**：立面用节点自身的盒子（宽度要到布局后才知道），因此画不出 `glass_face` 那圈外扩
      1dp 的发丝线；棱边由材质自身的 rim/fresnel 承担。
- [x] **GlassListTile**（`list_tile.rs`）：16/12 内衬；前置图标 22 装在 28 宽的槽里居中、其后 12；
      标题 16pt w500、副标题 13pt @65%、间距 2；尾部字形 20pt @54%、其前 8。按压与键盘焦点都画
      8% 底色（浅色黑 / 深色白；按下立即、松开 150ms easeOutCubic 退去），与参考一样**不画外扩
      焦点环**。文字列 `grow_x` 吃掉两侧字形之外的宽度；`standalone()` 自带 12dp 玻璃立面，
      `still()` 是无动作的行。gallery `lists` 页玻璃列（卡内三行、独立行、静止行）+ selftest 三条
      （56dp 文字边与 12 顶内衬、尾部距右缘 16、分隔线 x=56 且高 0.5、激活计数、底色 0.08→0），
      内衬与底色做过反向验证。
      **偏离**：参考的编辑态（删除/插入徽章、选择圈、重排把手、滑动操作、右键菜单）未做。
- [x] **GlassDivider**（`divider.rs`）：0.5dp 线装在 1dp 带里，浅色 10% 黑 / 深色 20% 白；
      `indent`/`end_indent`/`band` 可调。带是行（或列）容器：线沿主轴伸展，靠 `cross_pos(0.5)`
      在交叉轴上居中——没有写死的偏移。
- **GlassCard 改为裁切内容**：圆角表面按自身形状裁切子树，按下行的方角底色不会把卡片圆角削方。
- [x] **GlassTextField**（`text_field.rs`）：16/12 内衬、10dp 圆角、图标间距 12，文字 16pt @90%、
      占位 50%、插入符 2dp；`.search()` 变体 12/8 内衬、44 高、图标间距 8、22dp 圆角。
      固定高度时去掉纵向内衬、由 `cross_pos(0.5)` 居中文字行（参考同样这么做，理由是不受系统
      字号缩放影响）；贴合高度时保留内衬。占位文字在**未聚焦**时也显示（Cupertino 的占位不是
      焦点效果），引擎编辑经 `Change::Text` 回到应用；禁用时文字与图标降到 0.38 且不接指针。
      玻璃立面沿用 `Control::Container` 配方（与卡片一致：透明底 + backdrop，节点自身盒子）。
      gallery `text-field` 页玻璃列（活字段、搜索变体、禁用样本）+ selftest 五条（图标槽 x=16、
      文字行 x=内衬+图标+间距、两轴居中、搜索 44 高与 12+20+8、空字段有提示、输入回应用、
      禁用不取焦且更淡），内衬间距与淡化比例做过反向验证。
      文字行的可编辑核心（`typed_input`/`caret_scroll`/`LineEdit`）上提到 `ui-widgets/src/text.rs`，
      MD3 文本框与玻璃文本框共用同一份，不各写一遍。
      **偏离**：参考的 `maxLines`/多行、`obscureText`、`onSuffixTap`（尾部图标的独立按钮语义）、
      输入格式化器未做。
- [x] **GlassSearchBar**（`search_bar.rs`）：`GlassTextField.search` 胶囊（半径 = 高/2）+ 尾部
      清除 ×（18dp，`AnimatedSwitcher` 的 180ms 线性淡入 + 0.5→1 缩放，点击上报
      `Selected(Some(trailing))`）+ 可选取消按钮（44×44 玻璃圆钮、24dp ×，`AnimatedSize` 的
      250ms 由槽位宽度裁切滑入，点击上报 `Selected(Some(cancel))`）。文字宽度 = 栏宽 − 取消按钮
      与 10dp 间距 × 滑入进度——让位由布局算出，没有写死偏移。
      gallery `search` 页玻璃列（带取消 / 不带取消）+ selftest 五条（44 高、按钮 44 宽且贴右缘、
      字段让出 44+10、空查询时 × 不可见、输入后 × 可点并清空、取消按钮回应用），间距做过反向验证。
      **偏离**：取消按钮只有尺寸滑入、没有 180ms 的透明度淡出（玻璃按钮不接受外部 alpha）。
- [x] **GlassDialog**（`dialog.rs`）：`GlassContainer.card` 表面（20dp 内衬、12dp 圆角、默认
      280 宽）＋标题 18 粗体居中、正文 14 @60% 墨色（1.4 行高折成 0.2em 行距）居中，带间 8、
      动作之上 12。动作按钮复用 `GlassButton`（44 高、12dp 圆角、16pt；主操作用粗体、破坏性用
      `systemRed`），一到两个并排各占半条带、三个竖排各占整宽；标签放不下半条带时改竖排。
      **居中靠布局**：卡片列 `cross_align(1.0)` 让各带撑满，文字再在自己的整宽盒子里
      `TextAlign::Center`——没有任何固定偏移。模态走引擎的 `ModalTarget`（默认窗口、可指定
      邻近宿主），遮罩 20% 黑（Cupertino 的 `_kCupertinoModalBarrierColor`，非 Material 的 54%），
      按下的动作上报 `Selected(Some(id))`、点遮罩上报 `Cancelled`（仅 `dismissible`）。
      gallery `dialogs` 页新增玻璃列（两动作 / 三动作 / 单动作）+ selftest 六条（280 宽且在列内、
      标题带撑满内宽、两动作等宽同排且不溢出表面、三动作逐行下移、单动作占整宽、正文墨色为
      标签色 ×0.6、按下指出动作）。撑满、行间隙与单动作整宽三处做过反向验证（红→绿）。
      **偏离**：参考的 `maxWidth` 是上限（内容更窄时卡片更窄），这里表面是固定宽度；动作的
      蓝色/红色光晕没有对应实现；进出场过渡未做（引擎 E4）。
- [x] **GlassPopover**（`popover.rs`）：与菜单共用 `overlay.rs` 的浮出片（280dp 宽、24dp 圆角、
      16dp 内衬、snappy(320ms, 0.15) 到位、磨砂底、Escape 绑定），内容由应用给一组 `Decl`。
      内容里的控件归应用所有，因此片只按 `.presses(vec![id])` 读这些 id 的 `clicked`，
      按下即上报 `Selected(Some(id))`；`barrierDismissible` 只管「片外的按下」，Escape 始终有效。
      gallery `menu-flyout` 页玻璃列（标题 / 正文 / Done 按钮）+ selftest 九条（280 宽、锚点下方、
      16dp 内衬、三块按序落在片内、Done 上报并关片、锚点重开、Escape 关闭）。
      **引擎动作**：`Params.focus_stop` + `Decl::focus_stop(true)`——把焦点环画在子节点上的控件
      （玻璃 Button / Chip）此前既不算 Tab 停靠点、也拿不到浮层/模态的初始焦点，于是浮出层的
      Escape 无人应答；现在控件自己声明停靠点，`focus_stop()` 不再只靠「画了墨迹」推断。
      **偏离**：无箭头（参考有指向锚点的小三角）；无进出场缩放，只有淡入；内容控件不自动接
      键盘，应用要自己给可聚焦的块（`.presses` 只解决上报）。
- [x] **GlassActionSheet**（`action_sheet.rs`）：贴底的两块卡片——动作卡（页头带＋逐行动作＋
      发丝线）与下方独立的取消卡，整块 `EdgeInsets.all(8)` 内衬、14dp 圆角、两卡间 8dp。
      行是 16dp 内衬包一条 17pt 行（可选 20dp 图标、间距 12），按压或键盘焦点的底色是主墨 6%，
      取消行粗体，破坏性动作取 `systemRed`。卡色是黑/白 65% 压在玻璃之上，遮罩 54% 黑
      （`GlassDefaults.barrierColor`，与 Dialog 的 Cupertino 20% 不同）。
      **贴底靠布局**：遮罩列里放一个零基准增长者，它吃掉全部余高把面板压到底缘，没有手工偏移；
      行内居中走 `Stack` + `stack_align(Center)`，页头靠带撑满 + `TextAlign::Center`。
      上报：动作 `Selected(Some(id))`、取消卡与遮罩 `Cancelled`。
      gallery `action-sheet` 页玻璃列（触发器 + 读数行）+ selftest 五条（左右/底各内缩 8、
      两卡相隔 8、各行等高且 > 48、选中上报并关闭、取消计一次关闭、遮罩按下关闭）。
      **踩坑**：该模块此前写好却从未进 `lib.rs`（`实物到了 ≠ 接线到了`），`GlassCard::new`
      的三参调用也一直没被编译过。
      **偏离**：参考的 `letterSpacing: -0.3` 没有对应实现（引擎无 tracking，见 `engine-gaps`
      #54）；进场用本仓浮层的惯例——上移 24dp + 淡入，不是参考的 popup 过渡。
- [x] **GlassToast**（`toast.rs`）：非模态的胶囊提示条——48dp 最小高、400dp 宽上限、24dp 圆角、
      距窗口边 16dp，黑/白 70% 立面 + 语义色 20% 边框 + 语义色 15%、20dp 模糊、外扩 2dp 的辉光。
      内容：20dp 图标（按类型取 `systemGreen`/`systemRed`/`systemBlue`/`systemOrange`，
      `Neutral` 是主墨 60%）、间距 12、15pt w500 正文（最多两行）、可选动作（14pt w600、12/6 内衬、
      32×32 最小、按压 10% 底色）。类型自带 SF 默认字形。
      **位置靠布局**：整层是 `fixed` + 四周内衬（上/下 16，另一侧 `Auto`），胶囊在其中居中——
      没有手工偏移；`Center` 时内衬归零、层占满窗口。
      **非阻塞**：整层 `Sense::PASS_THROUGH`，只有胶囊接拖拽，下层应用照常拿得到指针。
      **滑走关闭**：胶囊按离开方向跟手（`DRAG_FOLLOW`），越过 24dp 即上报 `Cancelled`，
      未达阈值弹回；动作上报 `Selected(Some(id))`。
      gallery `toast` 页玻璃列（两种触发 + 读数行）+ selftest 五条（贴边 16、胶囊居中、
      48 高且不超 400 宽、动作上报并关闭、下滑关闭、顶部贴边）。
      **引擎动作**：`ShadowSpec` 从「只有 alpha 的黑影」改为带 `colour` 与 `spread`（新增
      `black`/`glow` 构造器），盒子的辉光因此不必再靠兄弟节点伪造；MD3 的 9 处调用随之改成
      `ShadowSpec::black`，像素不变。
      **踩坑**：拖拽状态必须读**胶囊**的 `WidgetState`——`dragging_within` 是精确相等，
      根节点读不到子节点的拖拽，第一版 selftest 因此 `0->0` 不关闭。
      **偏离**：自动消失归应用（引擎没有时钟）；参考的 `blur: 6`/`saturation: 1.3` 在材质里
      没有对应旋钮；进场按自身最小高（48dp）上移，参考是按自身实测高度。
- [x] **GlassModalSheet**（`modal_sheet.rs`）：贴底的模态面板——半高占窗口 45%、全高距顶 90dp
      且两侧与底部贴边，顶部圆角 56dp→46dp（在 0.92 处收完），黑 54% 遮罩，抓手 36×4、距顶 8dp、
      其下 8dp 起内容。玻璃取 `Control::Container` 配方并把折射率抬到 1.15。
      **档位靠布局**：面板是 `fixed` + 上内衬 `Pct((1−位置)·100)` + 下内衬 `Px(6)`；两内衬都确定
      且高度 hug 时由布局解出高度，没有手工算高度。全高时内衬与圆角随位置收拢，「贴边」因此也是
      布局结果而不是特例。
      **状态机**：静止档位是阶梯（可关闭时低端再加「隐藏」）。拖拽走未经压缩的行程（`raw` tween），
      显示位置由另一条弹簧 `snap` 承载；松手按参考 `resolveTarget` 的阈值落档，越界按 8% 受压。
      **松手帧从手指离开处起步**（先 `forget_spring` 再就地播种），否则弹簧存的是拖拽前的状态，
      面板会先瞬移回去再弹。上报：内容里的 `.presses` 与落档 `Selected(Some(id))`、
      遮罩与 Escape `Dismissed`。
      gallery `modal-sheet` 页玻璃列（触发器 + 读数行）+ selftest 七条（半高几何、上拖到全高并收到
      档位、下拖回半高、下滑关闭并计数、遮罩关闭、Escape 关闭、内容按钮关闭）。
      **踩坑**：拖拽期间弹簧值仍是拖拽前的旧值（第一版松手因此弹回半高）；`drag_step` 只在
      `WidgetState` 上，`ui_widgets::Cx` 没有；落定弹簧每帧最多推进 0.1s，selftest 读几何前要多走几帧。
      **偏离**：参考的档位判定还读速度，引擎不上报速度，只按位置阈值；进出场过渡未做。

- [x] **GlassStepper**（`stepper.rs`）：iOS 26 `UIStepper` 的玻璃版——94×34 胶囊，圆角取高度一半，
      左右两半各 `Grow::Split(1)`，中间 0.5dp 分隔线取 25% 墨色（浅色黑、深色白），两侧各一枚 20dp
      `minus`/`plus`，值本身不画（参考也把值留给应用的标签）。
      **值逻辑**：按下沿先走一步（`pressed` 上升沿，不是 `clicked`——参考在 tap-down 就动作），
      按住 `repeatDelay` 0.5s 后按 `repeatInterval` 0.1s 重复；越界按参考的
      `min + (next − max) % span` 循环或夹紧，键盘激活只走一步、不重复。
      **重复由引擎时钟驱动**：`age` 记按住时长，每帧换算成「应已发生的节拍数」，与已上报数相减，
      一帧内补足差额——低帧率不会丢步（selftest 用 1s 一帧验证一帧走六步）。
      **按压/禁用曲线**：图标 0.88 缩放（80ms easeOut）、不可用一侧淡到 0.3（150ms 线性），
      都在 `new` 里推进，`decl()` 保持纯函数。
      gallery `stepper` 页玻璃列（值标签 + 三个步进器 + 读数行）+ selftest 六条（94×34 与两侧
      46.75/分隔线 0.5、加减各一步、按住一帧补六步并停在顶、到顶后按加号不上报、循环步进器
      4→5→1、禁用步进器不上报）。
      **踩坑**：到顶后连发仍会算出「一步」，若照常上报，应用会收到同一个值并误计次数——上报前要
      滤掉「值没变」；按住期间必须每帧读 `age`（`Dep::Age` 才会让子树重建），松手 `forget_age`。
      **偏离**：参考用 `Timer` 在 UI 线程外计时，这里由帧时钟驱动；无触觉反馈与无障碍标签
      （引擎两者都还没有）；按压缩放作用在图标上，与参考一致。

### 引擎改造（随控件推进，先报告再改）

| # | 症状 | 触发控件 | 状态 |
|---|---|---|---|
| E1 | 焦点视觉画不到控件外（负 margin） | 全部 WinUI 控件 | 待验证 |
| E2 | `Change` 无三态 | CheckBox | 已知（engine-gaps #4） |
| E3 | 声明期拿不到文字尺寸 | TextBox header / TabView | 已知（engine-gaps #12，先查现状） |
| E4 | 无进出过渡 | Dialog / Menu | 已知（engine-gaps #13） |
| E5 | 浮层定位/焦点陷阱 | Menu / ComboBox / Dialog | 待验证 |
| E6 | 画刷是渐变而非纯色（`Palette` 只能答颜色） | Button 立面描边 | **已修**：`Palette::native_brush` + `Style::brush`；`Gradient.flip`（明色立面镜像，数据由生成器从 `ScaleTransform` 读出）；CPU 渐变按逻辑单位采样 |
| E7 | `Decl::align_self` / `justify_self` 只有网格子项读，流式子项上静默无效 | 居中图标、列里按内容宽的块 | 已知（engine-gaps #48）；暂用「容器自身画盒子 + padding 定位子节点」或套一层 `Row.cross_align(0.0)` 绕开 |
| E8 | 引擎无矢量路径填充图元 | WinUI 图标 | **改走字体**：`IconSource::Glyph` + Fluent 图标字体，无需新图元；若将来要 SVG 路径再评估 |
| E9 | 定位内边距只能是像素，写不出 CSS `left: 50%` | ProgressBar 不确定态、一切按比例摆放的装饰 | **已修**：`layout::LenInsets`（`Len::Px`/`Pct`/`Auto`）+ `Placement.inset` 改类型 + `Decl::inset(impl Into<LenInsets>)`；`relative_shift` 与流外求解各自按包含块解析。测试钉住「百分比按包含块、相对定位只移绘制不移槽位」 |
| E10 | 目标驱动动画落定即停帧，程序化循环无法自持 | ProgressBar 不确定态、旋转指示、任何自走动画 | **已修**：`AnimStore::age`（按 id 记出生时刻，首帧为零）+ `Dep::Age` + `Recorder` 的 `time_driven` 标记并入 `FrameOutcome.animating`；读数者每帧重建、循环不休，不读则让循环歇下。测试钉住「读时钟→持续要帧，停读→循环歇下」 |
| E11 | 绘制端无描边弧线/路径图元 | ProgressRing（WinUI 用 Lottie 的 trim 椭圆） | **已修**：`draw::Sector` / `Shape::Sector` / `PrimKind::Sector`；CPU `draw_sector` 逐像素算环带 + 角度区间 + 圆帽，GPU 在片元里做同一 SDF（每实例 11 浮点）。`Sector::sweep()` 两端共用，跨度 ≥ 一整圈取整环（否则 100% 的环会折成零） |
| E12 | 描边只有统一宽度，写不出 CSS/WinUI 的逐边 `1,1,1,2` | TextBox 聚焦边框、任何上下异宽的控件 | 已知；`Shape::Box.border` 与 `Shape::Outline` 都只收一个 `width`。当前用「1px 均匀 + 底部 1px 细条」逼近；真修法是给描边加四边宽度（CPU/GPU 两端都要改） |
| E13 | `hug(min, hint)` 的 hint 被当基准下限，Fit 项永远占满舒适值 | TabView 页头、任何「贴合内容但有舒适值」的项 | **已修**（engine-gaps #39）：Fit 基准 = `content.clamp(floor, ceiling)`；测量趟对 Fit 不再用 hint 抬贡献下限（`hint_floor`）。测试 `a_fitting_item_hugs_its_content_under_a_comfort_hint` 先红后绿 |
| E14 | 流内 margin 被布局整体丢弃（只有流外读） | TabView 图标间距、新建按钮上边距 | **已修**（engine-gaps #40）：`flow_margin()`，测量与定位两趟在主轴、交叉轴都计入。两条测试先红后绿 |
| E15 | flex 项从不收缩（`Grow::No` → shrink 0），与 CSS `flex: 0 1 auto` 相反 | TabView 溢出、任何 Row 里超宽的子项 | **已修**（engine-gaps #41）：`Grow::No => (0.0, 1.0)`，`fixed` 由自身下限挡住。测试 `a_row_squeezes_a_scrollable_child_to_the_line_before_it_overflows` 先红后绿 |
| E16 | 没有锚定浮层：越出祖先裁剪、按锚点摆放、命中优先都得各控件自己搓 | ToolTip / MenuFlyout / ComboBox / ContentDialog | **已修 O1+O2**：`OverlayPlacement` + `Anchor::{Node,Point}` + `Decl::overlay`；流外那趟按锚点解位（flip → clamp），`walk()` 与模态一起推后（绘制在上、命中优先），`compute_masks` 从浮层自身重置遮罩。三条测试先红后绿；**O3 已修**（`Cx::dismissed` + `Change::Dismissed`，随 3.2） |

---

## §5 每步纪律

- 改文件用编辑工具；改完 `grep -c` 读回；测试先看红再看绿；
- 注释 2 行英文 + 2 行中文，数值出处留一行指针；
- 图形验收只在嵌套会话，一个任务内 start/stop 各一次；
- 单元测试只钉不变量：几何常量、状态矩阵、动效时长、令牌覆盖；不写快照与覆盖率测试；
- 每完成一条，回来勾选并写"实际值 / 偏离点"。

## §6 进度日志

| 日期 | 事项 | 结果 |
|---|---|---|
| 2026-11-02 | 建计划、下载 WinUI XAML（31 个文件，SHA 固定） | 本文 |
| 2026-11-02 | P0.1/0.2/0.4/0.5 | WinUI 令牌表 1191 条落地；Fluent 包换血；引擎焦点装饰；WinUI 动效表 |
| 2026-11-02 | P0.3 | `ControlMetrics` 表落地并挂到 `Style.controls`；P1 五行数值来自 XAML |
| 2026-11-02 | E6 | 渐变画刷进包（`native_brush`/`brush`/`Gradient.flip`）；重新生成数据（6 条渐变带 flip） |
| 2026-11-02 | P1.1 | Fluent Button 三变体落地，2 条测试（破坏验证） |
| 2026-11-02 | P0.6 | Fluent 图标字体接入（MIT），生成码点表，gallery 注册 |
| 2026-11-02 | P1.2/1.3 | CheckBox、RadioButton 落地；gallery 两列可交互；截图核对（暗色） |
| 2026-11-02 | P1.6/1.7 | ToggleSwitch、Slider 按 XAML 复核；滑块根节点 id 修复（此前交互全丢） |
| 2026-11-02 | E9/E10 | 百分比定位内边距；时钟驱动循环（`age` + 帧循环唤醒） |
| 2026-11-02 | P1.4 | ProgressBar 确定/不确定/错误/暂停落地；gallery Fluent 列截图核对 |
| 2026-11-02 | E11/E38 | 描边弧图元：`Sector` + `PrimKind::Sector`，CPU 逐像素 + GPU 片元 SDF；4 条测试（整圈折叠先红后绿） |
| 2026-11-02 | P1.5 | ProgressRing 落地：确定态按 Lottie 关键帧、旋转态按双段旋转 + 尾追；gallery 截图核对 |
| 2026-11-02 | E13–E15 | 布局引擎三处：Fit 的 hint 上限被当基准下限、流内 margin 丢弃、不增长即不收缩；4 条测试（各先红后绿） |
| 2026-11-02 | E16/O1–O2 | 锚定浮层进引擎：`OverlayPlacement`/`Anchor`/`Decl::overlay`，按锚点解位 + flip + clamp + `match_anchor_width`；绘制与命中排在整棵树之后；遮罩越过祖先裁剪。三条测试先红后绿 |
| 2026-11-02 | P2.3 | TabView 落地：页头 `[100, 240]` 贴合、关闭/新建、选中并入内容板、溢出滚动；加号跟随最后一个页头（模板 `Auto` 列）；溢出箭头 `‹`/`›` 按 `Change::ScrollBy` 步进（engine-gaps #42）；偏离：箭头不连发、无拖动重排 |
| 2026-11-02 | P2.1 | Fluent TextBox 核心落地（标题/占位符/四态/清除按钮/输入接线）；gallery Fluent 列截图核对 |
| 2026-11-02 | P2.4 | ToolTip 落地：320 封顶 + `9,6,9,8` + 4 圆角 + 1px 浮层描边 + 丙烯酸；`OpenStates` 167ms 淡入淡出；应用侧 400ms 悬停延时；引擎修 engine-gaps #43（层内背景模糊）；gallery `tooltip` 页截图核对 |
| 2026-11-02 | P2.4 修正 | 用户报「tooltip 被背景遮挡 / 悬停时小卡片跳变」：`paint_node` 补上浮层推后（engine-gaps #45），关闭态改为流外浮层；两条测试先红后绿，截图核对 |
| 2026-11-02 | P2.5 | InfoBar 落地：48 高 / 16 内衬 / 14 图标边距 / 38 关闭 / 四级别画刷；同排 12dp、堆叠 4dp 由 `margin_alt` 表达；`IsOpen=false` 走流外。引擎四处：`BoxModel` 上移进 `Decl`、文本 min-content（UAX#14）、测量趟二次收缩、父子同 Id 断言；gallery `info-bar` 页截图核对（窄槽 320、消息 191.99 宽内换行、条带 130.42 高） |
| 2026-11-03 | P4 | TabView 拖动重排：浮起/让位都走 `.offset`（margin 会进布局，预览会抖）；落点按槽位几何算，`Dropped { from, to }`；selftest 两条（滑动 + 重排），各自反向验证过 |
| 2026-11-03 | E4/P4 | `Change::Toggled { on: Option<bool> }` 收口三态（engine-gaps #4）；Fluent `three_state()` 按 `ToggleButton.OnToggle` 循环；单测 + selftest 各反向验证过 |
| 2026-11-03 | G1 | 玻璃档 Button / SegmentedControl / Menu 落地（gallery 列 + selftest）；新增 `shared::focus_ring`（`systemBlue` 1.5dp、外扩 3dp），透明底的行由此进入键盘作用域——没有它，面板的 Escape 与方向键都无人接 |
| 2026-11-03 | G1 | 玻璃档 Chip 落地（胶囊贴合内容、选中底色、可删字形）；焦点环统一到 `focus_ring_ink`/`focus_ring_shape`，按钮的白色 2dp 环换成参考的蓝环 |
| 2026-11-03 | G1 | 玻璃档 Progress 落地（20dp 环 / 4dp 条，确定态与每秒一圈的扫动）；轨道改按 `systemFill` 自适应——参考硬编码的 15% 白在浅色面上会消失；顺带修好 `menu-flyout` selftest 一条早已失效的前提（页面本来一进来就展开面板，「导航那次按下关掉了它」无从成立），改为按页面别处并断言轻关闭 |
| 2026-11-03 | G1 | 玻璃档 Card / Container 落地（16/12 卡片、无内衬 16 圆角容器、纯色 26 圆角卡）；新增 `Control::Container` 材质（包默认本体） |
| 2026-11-03 | G1 | 玻璃档 ListTile / Divider 落地（16/12 行、56dp 文字边、8% 按压底色、0.5dp 发丝线）；卡片改为按自身圆角裁切内容 |
| 2026-11-03 | G1 | 玻璃档 TextField 落地（16/12 内衬、10dp 圆角；搜索变体 44 高/22 圆角）；可编辑行核心上提到 `ui-widgets::text::LineEdit` 供 MD3 与玻璃共用；占位文字改为空缓冲即显示（`hint_always`） |
| 2026-11-03 | G1 | 玻璃档 SearchBar 落地（胶囊字段 + 180ms 淡入的清除 × + 250ms 滑入的 44dp 取消按钮）；字段让出的宽度由取消按钮的滑入进度算出 |
| 2026-11-03 | G1 | 玻璃档 Dialog 落地（280dp 卡面、20dp 内衬、标题 18 粗体/正文 14 @60%，动作复用 `GlassButton` 44 高 12 圆角，≤2 并排、3 竖排、标签放不下则竖排）；居中走「带撑满 + `TextAlign::Center`」而非偏移；`GlassButtonParams` 增 `radius`/`label_size`/`label_weight`/`label_ink` 供调用方覆盖标签 |
| 2026-11-03 | G1 | 玻璃档 Popover 落地（与菜单共用 `overlay.rs` 浮出片；内容为应用的 `Decl`，`.presses` 读内容 id 的点击并上报 `Selected`）；引擎新增 `Params.focus_stop`/`Decl::focus_stop`，把环画在子节点上的控件由此成为 Tab 停靠点并能接住浮层初始焦点（此前 Escape 无人应答）；顺带把 `menu-flyout` 自检里「一进来就展开」的脆弱前提改成「锚点能打开」 |
| 2026-11-03 | G1 | 玻璃档 ActionSheet 落地（贴底动作卡＋取消卡、8dp 内衬、14dp 圆角、行 16dp 内衬包 17pt、6% 按压底色、卡色 65%、遮罩 54%）；贴底靠零基准增长者而非偏移；该模块此前从未进 `lib.rs`，此次一并接线并补 gallery 列 + selftest 五条 |
| 2026-11-03 | G1 | 玻璃档 Toast 落地（非模态胶囊、48 高 / 400 宽上限、16dp 贴边、语义色边框与辉光、按类型取图标与强调色、下滑关闭）；整层 `PASS_THROUGH` 因此不挡指针；`ShadowSpec` 增 `colour`/`spread`，盒子辉光不必再伪造兄弟节点 |
| 2026-11-03 | G1 | 玻璃档 ModalSheet 落地（半高 45% / 全高距顶 90dp、档位阶梯、拖拽跟手、松手按阈值落档、越界 8% 受压、遮罩与 Escape 关闭）；档位位置由 `Pct` 上内衬 + 固定下内衬交给布局解高度；gallery 列 + selftest 七条 |
| 2026-11-03 | G1 | 修 ActionSheet 取消卡的 id 撞车：卡片壳与应用的取消动作都取 `id.with("cancel")`，树按 id 合并，取消行静默并进卡片（debug 断言抓到，release 无提示）；卡片改 `id.with("cancel-card")`，条目见 `docs/lessons/design-errors.md` |
| 2026-11-03 | G1 | 玻璃档 Stepper 落地（94×34 胶囊、0.5dp 分隔线、两侧 0.88 按压缩放与 0.3 禁用淡出、按下沿一步 + 按住 0.5s 后每 0.1s 重复、越界循环或夹紧）；重复由 `age` 帧时钟驱动，一帧内补足欠下的节拍；到顶的连发不上报（否则应用会误计次数）；gallery 列 + selftest 六条 |
