# 三套 UI 体系的控件清单与对应关系（MD3 / Liquid Glass·SwiftUI / WinUI 3·Fluent）

> 本文给出三份控件清单和一张逐功能对应表，并单列每一方的缺口。
> 调研日期 **2026-11-02**。只做清单与映射，不给像素规格；规格来源见
> `docs/research/fluent-reference-implementations.md`、`docs/research/liquid-glass-analysis.md`。

来源（均为官方文档，当日抓取）：

- **MD3**：<https://m3.material.io/components>，36 个组件页（2026-09 更新）。
- **Apple**：HIG *Components* 八个分类页（<https://developer.apple.com/design/human-interface-guidelines/components>）
  与 SwiftUI 文档的 *Controls and indicators*、*Text input and output*、*Menus and commands*、
  *Navigation*、*Modal presentations*、*Lists*、*View groupings* 页；
  Liquid Glass 取 *Adopting Liquid Glass*（<https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass>）。
- **Windows**：WinUI 3 控件索引
  <https://learn.microsoft.com/en-us/windows/apps/design/controls/>（2026-10-13，官方称 "over 45 controls"）。

---

## §0 结论

1. **粒度不同**。MD3 的 36 个是"设计组件"（含变体与状态）；Apple 是 HIG 的约 54 个组件 + SwiftUI 的视图类型；
   WinUI 是约 50 个 XAML 控件类型 + 若干"模式"（Forms、List/details）。
   三者能一一对应的只有三十来个，其余都需要"近似"或"没有"。
2. **Liquid Glass 不是新的控件集**。Apple 原文：标准组件
   （"bars, sheets, popovers, and controls"）"automatically adopt this material"；
   它改变的是材质、形态（更圆的角、可选超大号）与控件间的 morph，不新增控件类型。
   因此下表 Apple 一列的控件 = SwiftUI/HIG 控件，LG 只加注记。
3. **各有一批独占物**（详见 §4）：
   - 仅 MD3：FAB / extended FAB / FAB menu、chips、card、snackbar、loading indicator、
     移动端 navigation bar、carousel 的多件同屏布局。（分段控件 MD3 与 Apple 都有，只缺在 WinUI。）
   - 仅 Apple（严格独占）：ShareLink/activity view、PasteButton、RenameButton/EditButton、token field、
     activity rings、ContentUnavailableView、Table、ornament；
     另有只有近似物的（path control、action sheet、Stepper、Gauge、page control、inspector，见 §4.2）。
   - 仅 WinUI（严格独占）：InfoBar、TeachingTip、SemanticZoom、AnnotatedScrollBar、ItemsRepeater、
     PersonPicture、InkCanvas/InkToolbar、RatingControl、NumberBox、RepeatButton；
     另有一批只有近似物（BreadcrumbBar、SelectorBar、Pivot、FlipView、PipsPager、AutoSuggestBox 等，见 §4.3）。
4. **同名不同义最危险**：`sheet`（Apple 模态任务面 vs MD3 底部补充面）、`tabs`（内容页 vs 文档标签 vs 视图切换）、
   `menu`、`dialog`、`badge`、`split button`、`carousel`。§4.4 单列"翻译表"。

## §1 口径与边界

| 口径 | 规则 |
|---|---|
| 收什么 | 用户可交互的控件，以及有明确角色的状态/容器组件（dialog、sheet、info bar、divider 等） |
| 不收什么 | 布局原语（HStack/Grid/StackPanel）、宿主（Window/Frame/Page）、系统级体验（widgets/notifications）。它们在 §5 单列 |
| 变体计数 | 不单独计数：MD3 的 filled/outlined/text、Apple 的 style、WinUI 的 AccentButtonStyle 都算同一控件 |
| "有对应"的判定 | **●** 同一交互、同一角色；**◐** 能表达同一意图但形态/语义有可见差异；**○** 对方没有对应物 |
| 证据等级 | 清单条目均来自上面的官方页面；本仓库实现状态来自 `docs/control-completion-progress.md` §3 与 `ui/crates/ui-widgets-md3/src/` 的源码 |

---

## §2 三份清单

### 2.1 MD3（36 个官方组件）

| # | 类别 | 组件 | 变体 / 要点 | 本仓库 |
|---|---|---|---|---|
| 1 | 动作 | buttons | elevated / filled / tonal / outlined / text 五级强调 | ✅ |
| 2 | 动作 | icon buttons | standard / filled / tonal / outlined，24dp 图标 + 40dp 触控区 | ❌（`Md3Button` 只有前置图标槽） |
| 3 | 动作 | FAB | small / medium / large，primary→次色 | ❌ |
| 4 | 动作 | extended FAB | 图标 + 标签 | ❌ |
| 5 | 动作 | FAB menu | FAB 开出的动作菜单 | ✅ |
| 6 | 动作 | segmented buttons | 轮廓式 / 等分，选中实填充 | ✅ |
| 7 | 动作 | split buttons | 主动作 + 尾段菜单 | ❌ |
| 8 | 动作 | button groups | 相邻按钮的连接与间隔规则（Expressive） | ✅ |
| 9 | 选择 | checkbox | 三态 | ✅ |
| 10 | 选择 | radio button | 单选 | ✅ |
| 11 | 选择 | switch | 开/关 | ✅ |
| 12 | 选择 | chips | assist / filter / input / suggestion | ✅ |
| 13 | 选择 | sliders | continuous / discrete / centered / range | ✅ |
| 14 | 输入 | text fields | filled / outlined，单行 | ✅（多行仍缺） |
| 15 | 输入 | search | search bar / search view | ⚠️ 仅停靠式 bar |
| 16 | 日期 | date pickers | docked / modal / input | ⚠️ docked 日历；无输入式与 modal |
| 17 | 日期 | time pickers | dial / input | ⚠️ 仅输入式 |
| 18 | 导航 | top app bar | small / medium / large / flexible / search | ✅ |
| 19 | 导航 | navigation bar | 紧凑窗口底部目的地栏 | ✅ |
| 20 | 导航 | navigation rail | 中等窗口侧栏 | ✅（Expressive 展开族未做） |
| 21 | 导航 | navigation drawer | 标准 / modal | ⚠️ 仅标准 |
| 22 | 导航 | tabs | primary / secondary | ✅ |
| 23 | 导航 | toolbars | docked / floating，standard / vibrant | ⚠️ 浮动 FAB 变体未做 |
| 24 | 集合 | lists | 行数 × 选中 × 布局 × 变体 | ✅ |
| 25 | 集合 | carousel | multi-browse / hero / 居中 hero / uncontained / full-screen | ✅ |
| 26 | 容器 | cards | elevated / filled / outlined | ✅ |
| 27 | 容器 | divider | 全宽 / 内缩 | ✅ |
| 28 | 容器 | badges | 数字 / 小圆点 | ✅ |
| 29 | 容器 | bottom sheets | 标准 / modal | ❌ |
| 30 | 容器 | side sheets | 标准 / modal | ❌ |
| 31 | 浮层 | dialogs | 图标 / 动作数 / 标题；模态与内联 | ✅ |
| 32 | 浮层 | menus | 标准 / vibrant，分组与子菜单 | ❌（仅引擎右键菜单借用 MD3 外观） |
| 33 | 反馈 | snackbar | 单行 / 双行，动作 | ✅ |
| 34 | 反馈 | progress indicators | 线性 / 环形，确定 / 不定 | ⚠️ 仅线性确定态 |
| 35 | 反馈 | loading indicator | Expressive 的不定加载 | ❌ |
| 36 | 反馈 | tooltips | plain / rich | ❌（有设计文档 `docs/popover-tooltip-design.md`） |

**合计**：官方 36，本仓库 ✅ 21 完整 + ⚠️ 6 部分（变体未做齐）+ ❌ 9 未实现。
未实现的九个：icon buttons、FAB、extended FAB、split buttons、bottom sheets、side sheets、
menus、loading indicator、tooltips。

### 2.2 Apple：HIG 组件 + SwiftUI 类型

HIG 的 8 个分类给的是"组件"，SwiftUI 给的是"视图类型"；两者不是一一对应（有些 HIG 组件没有 SwiftUI 控件，
有些 SwiftUI 控件没有独立 HIG 页）。下表以 HIG 分类为骨架，补上 SwiftUI 类型。

| # | 类别 | HIG 组件 | SwiftUI 类型 | Liquid Glass 注记 |
|---|---|---|---|---|
| 1 | 动作 | Buttons | `Button`、`Link`、`ShareLink`、`EditButton`、`PasteButton`、`RenameButton`、`HelpLink`、`TextFieldLink` | `.buttonStyle(.glass)`；按钮可按压缩成菜单/popover |
| 2 | 动作 | Pop-up buttons / Pull-down buttons | `Menu`、`Picker(.menu)` | 菜单以玻璃为底；按下项有高亮 morph |
| 3 | 动作 | Menus / Context menus / Menu bar | `Menu`、`contextMenu`、`Commands`/`CommandMenu`、`MenuBarExtra` | 菜单栏与菜单均为玻璃材质 |
| 4 | 动作 | Toolbars | `.toolbar`、`ToolbarItem` | 工具栏是 LG 的核心承载面 |
| 5 | 动作 | Activity views | `ShareLink` + `SharePreview` | 分享面板为系统面 |
| 6 | 选择 | Segmented controls | `Picker(.segmented)` | 分段底与选中块随材质变形 |
| 7 | 选择 | Toggles | `Toggle`（`.switch` / `.button` / macOS `.checkbox`） | 开关旋钮在切换时 morph |
| 8 | 选择 | Pickers | `Picker`（wheel / menu / segmented / radioGroup） | 滚轮在玻璃上裁切 |
| 9 | 选择 | Sliders | `Slider` | 拖动时拇指/轨道液态变形 |
| 10 | 选择 | Steppers | `Stepper` | 两段按钮 |
| 11 | 选择 | Color wells | `ColorPicker` | 系统取色面板 |
| 12 | 选择 | Combo boxes（macOS） | `Picker` / `ComboBox`（AppKit 桥) | — |
| 13 | 选择 | Digit entry views | 无 SwiftUI 控件（自定义） | — |
| 14 | 选择 | Image wells（macOS） | 无（`Image` + 拖放） | — |
| 15 | 输入 | Text fields | `TextField`、`SecureField`、`TextEditor` | 输入框在栏内自动获得玻璃底 |
| 16 | 输入 | Search fields | `.searchable` 的搜索栏 | 搜索栏浮在玻璃上；新增 `Tab(role: .search)` 的搜索页签 |
| 17 | 输入 | Token fields | 无 SwiftUI 控件（AppKit `NSTokenField`） | — |
| 18 | 内容 | Labels | `Text`、`Label`、`LabeledContent` | 标签随内容层，不进玻璃层 |
| 19 | 内容 | Text views | `Text`、`TextEditor` | — |
| 20 | 内容 | Image views | `Image`、`AsyncImage` | — |
| 21 | 内容 | Web views | `WebView`（WebKit 桥） | — |
| 22 | 内容 | Charts | Swift Charts（`Chart` 等） | — |
| 23 | 布局 | Boxes | `GroupBox` | — |
| 24 | 布局 | Collections | `LazyVGrid`/`LazyHGrid`/`LazyVStack` + `ScrollView` | — |
| 25 | 布局 | Column views | 无 SwiftUI 控件（`Table`/`OutlineGroup` 近似） | — |
| 26 | 布局 | Disclosure controls | `DisclosureGroup` | — |
| 27 | 布局 | Lists and tables | `List`、`Table`、`Section` | 行背景随材质，节头浮起 |
| 28 | 布局 | Lockups | `Label`、`LabeledContent` | — |
| 29 | 布局 | Outline views | `OutlineGroup` | — |
| 30 | 布局 | Split views | `NavigationSplitView`、`HSplitView`、`VSplitView` | 分栏分隔与浮层随 LG 重绘 |
| 31 | 布局 | Tab views | `TabView` + `Tab` + `TabSection` | **浮动玻璃 tab bar**；`Tab(role: .search)` |
| 32 | 导航 | Path controls | 无 SwiftUI 控件（AppKit `NSPathControl`） | — |
| 33 | 导航 | Sidebars | `NavigationSplitView` 的 sidebar | 侧栏玻璃化 |
| 34 | 导航 | Tab bars | `TabView`（iOS 底栏） | 浮动、可收缩为胶囊 |
| 35 | 导航 | Navigation | `NavigationStack`、`NavigationLink`、`.navigationTitle` | 导航栏为玻璃 |
| 36 | 浮层 | Action sheets | `.confirmationDialog` | 系统动作表 |
| 37 | 浮层 | Alerts | `.alert` | — |
| 38 | 浮层 | Popovers | `.popover` | 玻璃底面 + 箭头 |
| 39 | 浮层 | Sheets | `.sheet`、`.fullScreenCover`、`presentationDetents` | 玻璃 sheet，可从控件 morph 出来 |
| 40 | 浮层 | Panels（macOS） | `.inspector` / `NSPanel` | 检查器玻璃化 |
| 41 | 浮层 | Page controls | 无独立控件（`PageTabViewStyle` 自带圆点） | — |
| 42 | 浮层 | Scroll views | `ScrollView`、`ScrollViewReader` | 内容滚到边时出现玻璃渐隐 |
| 43 | 浮层 | Windows | `Window`/`WindowGroup` | 窗口控件玻璃化 |
| 44 | 状态 | Progress indicators | `ProgressView`（linear / circular） | — |
| 45 | 状态 | Gauges | `Gauge` | — |
| 46 | 状态 | Rating indicators | 无 SwiftUI 控件（自定义） | — |
| 47 | 状态 | Activity rings | 无（watchOS 专属） | — |
| 48 | 其它 | Content unavailable | `ContentUnavailableView` | — |
| 49 | 其它 | Ornaments（visionOS） | `.ornament` | — |
| 50 | 反馈 | Haptics | `sensoryFeedback` | — |

**2.2.1 Liquid Glass 的控件事实（不是新控件）**

- 原文口径：标准组件 **bars、sheets、popovers、controls** "automatically adopt this material"；
  导航（tab bar / sidebar / toolbar / split view / inspector）、菜单（含 iPadOS 菜单栏）、
  模态（sheet / popover / action sheet）、窗口控件、列表/表格/表单/节头、搜索栏、App 图标都受它影响。
- 新增的是**效果与样式 API**：`glassEffect(_:in:)`、`GlassEffectContainer`、`UIGlassEffect`、
  `NSGlassEffectView`、`.buttonStyle(.glass)`、超大号控件尺寸，以及控件间的 morph（按钮→菜单/sheet）。
- 参考实现 `liquid_glass_widgets`（`/home/ollie/git/widget-ref/liquid_glass_widgets`）把 LG 读成一套
  **Glass\* 控件**，可作为"若把 LG 当控件体系"的清单：
  容器 `GlassCard` `GlassContainer` `GlassDivider` `GlassDisclosureGroup` `GlassGroupedSection`
  `GlassListTile` `GlassReorderableList` `GlassStepper` `GlassSwipeActions`；
  输入 `GlassTextField` `GlassTextArea` `GlassPasswordField` `GlassSearchBar` `GlassFormField`
  `GlassPicker` `GlassDatePicker`；
  交互 `GlassButton` `GlassIconButton` `GlassButtonGroup` `GlassChip` `GlassBadge` `GlassSlider`
  `GlassSwitch` `GlassSegmentedControl` `GlassPageControl` `GlassPullDownButton` `GlassEditButton`；
  反馈 `GlassProgressIndicator`；
  浮层 `GlassMenu` `GlassMenuItem` `GlassContextMenu` `GlassDialog` `GlassSheet` `GlassModalSheet`
  `GlassFullScreenCover` `GlassPopover` `GlassActionSheet` `GlassToast`；
  表面 `GlassAppBar` `GlassTabBar` `GlassToolbar` `GlassNavigationShell` `GlassNavigationStack`
  `GlassScaffold` `GlassLargeTitle` `GlassVerticalBar` `GlassBarItem`。
  注意其中 `GlassToast`、`GlassReorderableList`、`GlassPageControl` 等是**包作者对 Apple 交互的补充**，
  不是 Apple 官方组件。

### 2.3 WinUI 3 / Fluent（官方索引的 12 个分类）

| # | 类别 | 控件（XAML 类型） | 说明 |
|---|---|---|---|
| 1 | 基础输入 | `Button`、`RepeatButton`、`ToggleButton`、`DropDownButton`、`SplitButton`、`HyperlinkButton` | 索引里合并称为 "Buttons" |
| 2 | 基础输入 | `CheckBox` | 三态 |
| 3 | 基础输入 | `ComboBox`（`IsEditable` 可编辑） | 下拉列表 |
| 4 | 基础输入 | `RadioButton`、`RadioButtons` | 单选 / 单选组 |
| 5 | 基础输入 | `RatingControl` | 星级评分 |
| 6 | 基础输入 | `Slider` | 单值 |
| 7 | 基础输入 | `ToggleSwitch` | 开关 |
| 8 | 集合 | `ListView`、`GridView` | 列表 / 网格 |
| 9 | 集合 | `ItemsView` | 可换布局的集合视图（1.5+） |
| 10 | 集合 | `ItemsRepeater` | 数据驱动布局原语 |
| 11 | 集合 | `FlipView` | 一次一件翻页（轮播） |
| 12 | 集合 | `PipsPager` | 分页圆点 |
| 13 | 集合 | `TreeView` | 层级树 |
| 14 | 集合 | `SwipeControl` | 滑出操作 |
| 15 | 集合 | `RefreshContainer` | 下拉刷新 |
| 16 | 集合 | `SemanticZoom` | 语义缩放 |
| 17 | 浮层 | `ContentDialog` | 模态对话框 |
| 18 | 浮层 | `Flyout`、`MenuFlyout`、`CommandBarFlyout` | 浮出层三族 |
| 19 | 浮层 | `Popup` | 通用浮层容器 |
| 20 | 浮层 | `TeachingTip` | 教学提示 |
| 21 | 菜单 | `MenuBar` | 窗口菜单栏 |
| 22 | 菜单 | `CommandBar`、`AppBar` | 命令栏 / 应用栏 |
| 23 | 导航 | `NavigationView` | Left / Top / LeftCompact / LeftMinimal 四种模式 |
| 24 | 导航 | `TabView` | 文档标签页 |
| 25 | 导航 | `Pivot` | 内容分区切换（旧 UWP） |
| 26 | 导航 | `SelectorBar` | 少量视图切换 |
| 27 | 导航 | `BreadcrumbBar` | 路径面包屑 |
| 28 | 导航 | `Frame` + `Page` | 导航宿主（框架） |
| 29 | 导航 | `SplitView`、`TwoPaneView` | 单侧栏 / 双栏自适应 |
| 30 | 选择 | `CalendarDatePicker`、`DatePicker`、`CalendarView` | 日期下拉 / 日期选择 / 日历 |
| 31 | 选择 | `TimePicker` | 时间 |
| 32 | 选择 | `ColorPicker` | 颜色谱 |
| 33 | 文本 | `TextBlock`、`RichTextBlock` | 纯文本 / 富文本显示 |
| 34 | 文本 | `TextBox`、`RichEditBox` | 单行或多行 / 富文本编辑 |
| 35 | 文本 | `PasswordBox` | 密码 |
| 36 | 文本 | `NumberBox` | 数字与表达式 |
| 37 | 文本 | `AutoSuggestBox` | 输入建议 |
| 38 | 状态 | `ProgressBar`、`ProgressRing` | 线性 / 环形 |
| 39 | 状态 | `InfoBar` | 内联状态条 |
| 40 | 状态 | `InfoBadge` | 徽标 |
| 41 | 状态 | `ToolTip` | 工具提示 |
| 42 | 布局 | `Expander` | 折叠面板 |
| 43 | 布局 | `ScrollViewer`、`AnnotatedScrollBar` | 滚动 / 带注释滚动条 |
| 44 | 媒体 | `Image`、`MediaPlayerElement`、`WebView2` | 图片 / 视频 / 网页 |
| 45 | 媒体 | `InkCanvas`、`InkToolbar` | 墨迹与墨迹工具 |
| 46 | 媒体 | `FontIcon`、`SymbolIcon`、`PathIcon`、`BitmapIcon`、`AnimatedIcon` | 图标族 |
| 47 | 人物 | `PersonPicture` | 头像 |
| 48 | 窗口 | `TitleBar` | 自定义标题栏 |
| 49 | 模式 | Forms、List/details | 官方明确标注为 pattern，不是控件 |

---

## §3 对应关系总表

判定：**●** 一一对应　**◐** 近似（形态或语义有差）　**○** 对方没有对应物。

### 3.1 按钮与命令

| # | 功能 | MD3 | Apple（SwiftUI / HIG） | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 1 | 普通按钮 | Button（5 级强调） | `Button`（+ 角色 / `.glass`） | `Button`（+ AccentButtonStyle） | ● | 强调分级只有 MD3 是组件级概念 |
| 2 | 图标按钮 | icon buttons | `Button` + `Label`/`Image`（无独立类型） | `AppBarButton` | ◐ | Apple 无独立图标按钮类型 |
| 3 | 超链接按钮 | text button（近似） | `Link` | `HyperlinkButton` | ◐ | MD3 是"最低强调按钮"，不表达导航语义 |
| 4 | 分割按钮 | split buttons | 无（`Menu` 近似） | `SplitButton` | ◐ | Apple 无 split 形态 |
| 5 | 下拉按钮 | split button 尾段 / menu | `Menu`（pull-down 外观） | `DropDownButton` | ◐ | |
| 6 | 切换按钮 | 无独立（chip / segmented 近似） | `Toggle(.button)` | `ToggleButton` | ○ | MD3 缺独立 toggle button |
| 7 | 重复按钮（按住连发） | 无 | 无 | `RepeatButton` | ○ | 仅 WinUI |
| 8 | FAB / extended FAB | FAB、extended FAB | 无 | 无 | ○ | 仅 MD3 |
| 9 | FAB menu | FAB menu | 无（`Menu` 近似） | 无（`DropDownButton` + `Flyout` 近似） | ◐ | 形态仅 MD3 |
| 10 | 按钮组 | button groups | `ControlGroup` | 无（`CommandBar` 近似） | ◐ | WinUI 无通用分组容器 |
| 11 | 分段控件 | segmented buttons | `Picker(.segmented)` / segmented control | `SelectorBar`（近似） | ◐ | WinUI 没有真正的分段控件 |
| 12 | 分享 | 无 | `ShareLink` + activity view | 无控件（`DataTransferManager`） | ○ | 仅 Apple 有控件形态 |
| 13 | 粘贴快捷键按钮 | 无 | `PasteButton` | 无 | ○ | 仅 Apple |
| 14 | 重命名 / 编辑模式按钮 | 无 | `RenameButton`、`EditButton` | 无 | ○ | 仅 Apple |
| 15 | 帮助链接 | 无 | `HelpLink` | 无 | ○ | 仅 Apple |
| 16 | 文本快捷输入按钮 | 无 | `TextFieldLink` | 无 | ○ | 仅 Apple |
| 17 | 工具栏 / 命令栏 | toolbars、top app bar | `.toolbar` + `ToolbarItem` | `CommandBar`、`AppBar` | ● | |

### 3.2 选择与数值输入

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 18 | 复选框 | checkbox | `Toggle`（macOS `.checkbox`；iOS 无此形态） | `CheckBox` | ● | iOS 用 switch 代替复选框 |
| 19 | 单选 | radio button | `Picker(.radioGroup)`（macOS）/ 无 iOS 形态 | `RadioButton`、`RadioButtons` | ● | |
| 20 | 开关 | switch | `Toggle(.switch)` | `ToggleSwitch` | ● | |
| 21 | 滑块 | sliders（含 range / centered / discrete） | `Slider`（单值） | `Slider`（单值） | ● | 区间与居中配置仅 MD3 |
| 22 | 步进器 | 无 | `Stepper` | `NumberBox`（近似） | ○ | MD3 无；WinUI 用数字框代替 |
| 23 | 数字输入 | 无（text field + 校验） | `TextField` + 格式化 | `NumberBox` | ◐ | 仅 WinUI 有专用控件 |
| 24 | 评分 | 无 | HIG rating indicator（无 SwiftUI 控件） | `RatingControl` | ○ | MD3 无；Apple 只有设计模式 |
| 25 | 颜色选择 | 无 | `ColorPicker`、color well | `ColorPicker` | ○ | MD3 无 |
| 26 | 组合框 | 无（menu 近似） | `Picker(.menu)`、macOS ComboBox | `ComboBox`（可编辑） | ◐ | MD3 无输入+列表一体的控件 |
| 27 | 滚轮选择器 | 无 | `Picker(.wheel)` | 无（`ComboBox`/`Picker` 近似） | ◐ | |
| 28 | 芯片 | chips（assist/filter/input/suggestion） | 无（token field 仅输入侧） | 无（`ToggleButton` 近似） | ○ | 仅 MD3 |
| 29 | Token 输入 | 无 | token field（HIG；AppKit） | 无 | ○ | 仅 Apple |
| 30 | 分段选择 | segmented buttons | `Picker(.segmented)` | `SelectorBar`（近似） | ◐ | 见 #11 |

### 3.3 文本输入与显示

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 31 | 单行文本 | text fields（filled / outlined） | `TextField` | `TextBox` | ● | |
| 32 | 多行文本 | 无（text field 单行） | `TextEditor`、`TextField(axis: .vertical)` | `TextBox`（AcceptsReturn） | ◐ | MD3 缺多行 |
| 33 | 密码 | 无（需自建） | `SecureField` | `PasswordBox` | ○ | MD3 无 |
| 34 | 富文本编辑 | 无 | `TextEditor` + AttributedString（能力有限） | `RichEditBox` | ○ | 仅 WinUI 是完整富文本编辑 |
| 35 | 富文本显示 | 无 | `Text(AttributedString)` | `RichTextBlock` | ◐ | |
| 36 | 自动建议 | search bar 的建议（搜索侧） | `.searchable` 建议 | `AutoSuggestBox` | ◐ | 仅 WinUI 是独立控件 |
| 37 | 静态文本 | 无控件（排版能力） | `Text`、`Label` | `TextBlock` | ◐ | 三者都不是交互控件 |
| 38 | 带图标标签 | 无 | `Label`、`LabeledContent` | 无（`ContentPresenter` 组合） | ○ | 仅 Apple 有语义化 lockup |
| 39 | 文本选择/复制 | 引擎选区（非控件） | `textSelection(.enabled)` | `TextBlock.IsTextSelectionEnabled` | ● | 能力三者都有，形态不同 |

### 3.4 日期、时间与日历

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 40 | 日期选择 | date pickers（docked / modal / input） | `DatePicker` | `CalendarDatePicker`、`DatePicker` | ● | |
| 41 | 日历视图 | date picker 的网格（无独立组件） | `DatePicker(.graphical)` | `CalendarView`（独立控件） | ◐ | WinUI 可独立使用 |
| 42 | 多日期 | 无 | `MultiDatePicker` | `CalendarView.SelectedDates`（近似） | ◐ | |
| 43 | 时间选择 | time pickers（dial / input） | `DatePicker(.hourAndMinute)`（无独立控件） | `TimePicker` | ◐ | Apple 无独立时间控件 |
| 44 | 数字键盘输入视图 | 无 | digit entry view（HIG） | 无 | ○ | 仅 Apple |

### 3.5 导航与外壳

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 45 | 顶部应用栏 | top app bar（6 变体） | `.toolbar` + `navigationTitle` | `AppBar`、`CommandBar`、`TitleBar` | ◐ | WinUI 把它拆成命令栏与标题栏 |
| 46 | 底部目的地栏 | navigation bar | `TabView`（iOS 底栏） | 无（`NavigationView` Top 模式近似） | ◐ | Windows 无移动式底栏 |
| 47 | 侧边导航（窄） | navigation rail | sidebar（`NavigationSplitView`） | `NavigationView` LeftCompact | ◐ | |
| 48 | 侧边抽屉（宽） | navigation drawer | sidebar / `NavigationSplitView` | `NavigationView` Left / LeftMinimal | ● | |
| 49 | 标签页 | tabs（primary / secondary） | `TabView` + `Tab` | `TabView`（文档）、`Pivot`、`SelectorBar` | ◐ | 语义三分：内容页 / 文档 / 视图切换 |
| 50 | 面包屑 | 无 | path control（文件路径，macOS） | `BreadcrumbBar` | ○ | 仅 WinUI 有通用面包屑 |
| 51 | 菜单栏 | 无 | `Commands`、`MenuBarExtra`（macOS/iPadOS） | `MenuBar` | ○ | MD3 无 |
| 52 | 分栏 | 无 | `NavigationSplitView`、`HSplitView`/`VSplitView` | `SplitView`、`TwoPaneView` | ○ | MD3 无分栏组件 |
| 53 | 导航宿主 | 无 | `NavigationStack`、`NavigationPath` | `Frame` + `Page` | ○ | 框架层 |
| 54 | 标题栏 | top app bar 近似 | 窗口标题 / `.toolbar` | `TitleBar`（自定义） | ◐ | |

### 3.6 集合与数据展示

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 55 | 列表 | lists | `List` | `ListView` | ● | |
| 56 | 网格 | 无（lists 有布局变体） | `LazyVGrid`/`LazyHGrid`（布局）、`Grid` | `GridView`、`ItemsView` | ○ | MD3 无网格组件 |
| 57 | 树 | 无 | `OutlineGroup`、`DisclosureGroup` | `TreeView` | ○ | MD3 无 |
| 58 | 表格 | 无 | `Table`、`TableColumn` | `ItemsView` / `ListView` 列布局（近似） | ○ | 仅 Apple 有表格控件 |
| 59 | 轮播 | carousel（5 种布局） | `ScrollView(.horizontal)` + page control / `TabView(.page)` | `FlipView`（单件翻页） | ◐ | WinUI 只有单件；MD3 多件同屏 |
| 60 | 分页指示 | 无 | page controls（HIG） | `PipsPager` | ○ | MD3 无 |
| 61 | 下拉刷新 | 无 | `.refreshable` | `RefreshContainer` | ○ | MD3 无 |
| 62 | 滑动操作 | 无 | `swipeActions` | `SwipeControl` | ○ | MD3 无 |
| 63 | 可重排列表 | 无（引擎拖拽） | `List.onMove` | `ListView.CanReorderItems` | ◐ | |
| 64 | 集合布局原语 | 无 | `LazyVStack`/`LazyHStack`/`LazyVGrid` | `ItemsRepeater` + 布局 | ○ | 仅 WinUI 把它做成可组合控件 |
| 65 | 语义缩放 | 无 | 无 | `SemanticZoom` | ○ | 仅 WinUI |
| 66 | 带注释滚动条 | 无 | 无 | `AnnotatedScrollBar` | ○ | 仅 WinUI |

### 3.7 菜单与浮层

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 67 | 菜单 | menus | `Menu` | `MenuFlyout`、`CommandBarFlyout` | ● | |
| 68 | 上下文菜单 | 引擎右键菜单（无 MD3 组件） | `contextMenu` | `MenuFlyout`（附着任意元素） | ● | |
| 69 | 对话框 | dialogs | `.alert`、`.confirmationDialog` | `ContentDialog` | ◐ | Apple 的 alert 语义更窄（无任意内容） |
| 70 | 底部面板 | bottom sheets | `.sheet` + `presentationDetents`（仅模态态） | 无（`ContentDialog`/`Flyout` 近似） | ◐ | **同名不同义**：MD3 还有常驻的标准态，见 §4.4 |
| 71 | 侧面板 | side sheets | `.inspector`、panel（macOS/iPadOS） | 无（`SplitView` 面板近似） | ◐ | |
| 72 | 浮出层 | menu / tooltip 近似 | `.popover` | `Flyout`、`Popup` | ◐ | |
| 73 | 动作表 | dialog（近似） | action sheet / `.confirmationDialog` | `ContentDialog`（近似） | ◐ | Apple 的动作表是按钮列表，不带正文 |
| 74 | 教学提示 | 无 | TipKit（框架，非控件） | `TeachingTip` | ○ | 仅 WinUI 有控件 |
| 75 | 临时提示条 | snackbar | 无（系统通知） | `InfoBar`（内联常驻，语义不同） | ○ | **仅 MD3** 有 snackbar |
| 76 | 内联状态条 | 无 | 无 | `InfoBar` | ○ | 仅 WinUI |
| 77 | 工具提示 | tooltips | `.help`（macOS） | `ToolTip` | ◐ | |

### 3.8 状态、反馈与徽标

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 78 | 线性进度 | progress indicators（linear） | `ProgressView(.linear)` | `ProgressBar` | ● | |
| 79 | 环形进度 | progress indicators（circular） | `ProgressView(.circular)` | `ProgressRing` | ● | |
| 80 | 不定加载 | loading indicator | `ProgressView()`（不定） | `ProgressRing`（IsActive） | ◐ | MD3 把它做成独立组件 |
| 81 | 仪表 | 无 | `Gauge` | 无 | ○ | 仅 Apple |
| 82 | 活动环 | 无 | activity rings（watchOS） | 无 | ○ | 仅 Apple |
| 83 | 徽标 | badges | `.badge`（Tab / List） | `InfoBadge` | ● | |
| 84 | 空状态 | 无 | `ContentUnavailableView` | 无（自定义） | ○ | 仅 Apple |
| 85 | 通知 / 横幅 | snackbar（近似） | 系统通知 | `InfoBar` / AppNotification | ◐ | 三者路线不同 |

### 3.9 容器与结构

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 86 | 卡片 | cards | 无（`GroupBox` / 自定义） | 无（`Border` + 主题资源） | ○ | **仅 MD3** 有卡片组件 |
| 87 | 分组框 | 无（card 近似） | `GroupBox` | 无（`Expander` 近似） | ○ | 仅 Apple |
| 88 | 折叠面板 | 无 | `DisclosureGroup` | `Expander` | ○ | MD3 无 |
| 89 | 表单 | 无 | `Form` | Forms（模式，非控件） | ○ | |
| 90 | 分割线 | divider | `Divider` | 无通用控件（`MenuFlyoutSeparator` / `Border`） | ○ | WinUI 无 divider；见 §4.3 |
| 91 | 滚动容器 | 引擎滚动（非控件） | `ScrollView` | `ScrollViewer` | ◐ | |

### 3.10 媒体与图形

| # | 功能 | MD3 | Apple | WinUI 3 | 判定 | 说明 |
|---|---|---|---|---|---|---|
| 92 | 图片 | 无（内容） | `Image`、`AsyncImage` | `Image` | ○ | MD3 不提供 |
| 93 | 视频 | 无 | `VideoPlayer`（AVKit） | `MediaPlayerElement` | ○ | MD3 不提供 |
| 94 | 网页 | 无 | `WebView`（WebKit） | `WebView2` | ○ | MD3 不提供 |
| 95 | 图表 | 无 | Swift Charts | 无（第三方） | ○ | 仅 Apple |
| 96 | 绘图 / 画布 | 无 | `Canvas`、`Shapes` | `Canvas`、Shapes | ○ | MD3 不提供 |
| 97 | 墨迹 | 无 | PencilKit（UIKit） | `InkCanvas`、`InkToolbar` | ○ | 仅 WinUI 有控件组 |
| 98 | 图标 | Material Symbols（非组件） | SF Symbols + `Image` | `FontIcon`/`SymbolIcon`/`AnimatedIcon` | ◐ | |
| 99 | 头像 | 无 | 无独立控件 | `PersonPicture` | ○ | 仅 WinUI |
| 100 | 地图 | 无 | `Map`（MapKit） | 无（WebView2 / 第三方） | ○ | 仅 Apple |

---

## §4 缺口清单（谁有、谁没有）

### 4.1 只有 MD3 有（Apple 与 WinUI 都没有）

| 组件 | 说明 | 最接近的替代 |
|---|---|---|
| FAB、extended FAB | 悬浮主操作按钮 | 无；WinUI 用 `CommandBar` 或普通 `Button` |
| FAB menu | FAB 开出的动作菜单 | `Menu`（Apple）/ `DropDownButton`+`Flyout`（WinUI） |
| chips | 辅助/筛选/输入/建议四型 | Apple token field（仅输入）；WinUI `ToggleButton` |
| cards | 三种卡面 | Apple `GroupBox`；WinUI `Border` + 主题资源 |
| snackbar | 底部临时提示条 | Apple 系统通知；WinUI `InfoBar`（内联、常驻） |
| loading indicator | Expressive 不定加载 | `ProgressView()` / `ProgressRing`（不定） |
| navigation bar | 移动端底部目的地栏 | Apple `TabView`（底栏）；WinUI 无移动底栏 |
| carousel 的多件同屏布局 | multi-browse / hero / 居中 hero | Apple `ScrollView` + page control；WinUI `FlipView`（单件） |

### 4.2 只有 Apple 有（MD3 与 WinUI 都没有）

ShareLink/activity view、PasteButton、RenameButton、EditButton、HelpLink、TextFieldLink、
token field、path control、page control、Stepper、Gauge、activity rings、rating indicator（设计模式）、
`ContentUnavailableView`、`Table`、action sheet、popover（WinUI `Flyout` 近似）、inspector、
digit entry view、GroupBox、image well、color well、ornament、lockup、column view。

其中真正**没有近似物**的：ShareLink/activity view、PasteButton、RenameButton/EditButton、
token field、activity rings、`ContentUnavailableView`、`Table`、ornament。
`path control` 与 WinUI `BreadcrumbBar`、`action sheet` 与 WinUI `ContentDialog`/`MenuFlyout`
互为近似，不算独占。

### 4.3 只有 WinUI 有（MD3 没有；Apple 侧按有无近似分两档）

| 控件 | Apple 侧最接近 | 档 |
|---|---|---|
| InfoBar | 无控件（系统通知/自定义） | **严格独占** |
| TeachingTip | 无控件（TipKit 是框架 API） | **严格独占** |
| SemanticZoom | 无 | **严格独占** |
| AnnotatedScrollBar | 无 | **严格独占** |
| ItemsRepeater | `LazyVStack`（布局而非控件） | **严格独占** |
| PersonPicture | 无独立控件（`Image` + 裁剪） | **严格独占** |
| InkCanvas / InkToolbar | PencilKit（UIKit 框架） | **严格独占（控件层）** |
| RatingControl | 无控件（HIG 只有设计模式） | **严格独占（控件层）** |
| NumberBox | 无（`TextField` + 格式化） | **严格独占（控件层）** |
| RichEditBox / RichTextBlock | `TextEditor` / `Text` 能力有限 | 近似但能力差一档 |
| BreadcrumbBar | path control（仅文件路径，macOS） | 近似 |
| SelectorBar | `Picker(.segmented)` | 近似 |
| Pivot | `TabView` | 近似 |
| FlipView | `TabView(.page)` / 分页 `ScrollView` | 近似 |
| PipsPager | page control（HIG） | 近似 |
| AutoSuggestBox | `.searchable` 建议 | 近似 |
| RefreshContainer | `.refreshable` | 近似 |
| SwipeControl | `swipeActions` | 近似 |
| TreeView | `OutlineGroup` | 近似 |
| TwoPaneView / SplitView | `NavigationSplitView`、`HSplitView` | 近似 |
| TitleBar | 窗口 chrome / `.toolbar` | 近似 |
| CalendarView | `DatePicker(.graphical)` | 近似 |
| MediaPlayerElement | `VideoPlayer`（AVKit） | 近似 |
| AnimatedIcon | SF Symbols 动画 | 近似 |
| RepeatButton | 无（需自定义） | **严格独占（控件层）** |
| ToggleButton | `Toggle(.button)` | 近似 |
| PasswordBox | `SecureField` | ●（仅 MD3 缺） |

注：`Frame`/`Page` 是导航宿主，不计入控件（§5）。

### 4.4 同名不同义（迁移时最容易错）

| 名字 | MD3 | Apple | WinUI 3 | 风险 |
|---|---|---|---|---|
| **sheet** | 底部/侧边**补充内容**面，可常驻 | 模态**任务**面（可 detents） | 无此概念 | 直接把 MD3 bottom sheet 叫 sheet，会误导为模态 |
| **tabs** | 内容页切换（primary/secondary） | `TabView`：iOS 是**目的地**，iPad/Mac 是侧栏/分栏 | `TabView`=文档标签，`Pivot`=内容分区，`SelectorBar`=视图切换 | 三种语义互不相同 |
| **menu** | 锚定菜单（含子菜单） | `Menu` / 菜单栏 / 上下文菜单三套 | `MenuFlyout` / `MenuBar` / `CommandBarFlyout` | 名称覆盖不同层级 |
| **dialog** | 模态或内联对话框（任意内容） | `.alert`（窄）、`.confirmationDialog`、sheet | `ContentDialog` | Apple 的 alert 不能装任意内容 |
| **badge** | 导航图标上的数字/圆点 | `.badge`（Tab/List） | `InfoBadge` | 位置语义不同 |
| **split button** | 主动作 + 尾段菜单 | 无（`Menu` 近似） | `SplitButton`（主+菜单） | MD3 与 WinUI 接近，Apple 没有 |
| **carousel** | 同屏多件、按权重缩放 | 横向 `ScrollView` + 分页 | `FlipView`（一次一件） | 布局模型完全不同 |
| **page control / pips** | 无 | page control（iOS） | `PipsPager` | 只有指示，不含翻页 |
| **toolbar** | 页面级动作条（停靠/浮动） | 导航栏上的 `.toolbar` | `CommandBar`（自适应） | 承载面不同 |

### 4.5 三方都没有“控件化”的能力（但有框架 API）

- 剪贴板：框架都提供 API（WinUI `Clipboard`、Apple `UIPasteboard`），MD3 只是设计体系；
  只有 Apple 把"粘贴"做成了控件 `PasteButton`。
- 拖放：三者都是 API/修饰符，没有独立控件。
- 焦点环 / 键盘导航：三者都是系统行为，不是控件。
- 富文本编辑在 MD3 完全缺位；Apple 只有受限的 AttributedString 编辑。

---

## §5 不算控件、但映射时容易混进来的东西

| 家族 | MD3 | Apple | WinUI 3 |
|---|---|---|---|
| 布局原语 | 无（设计体系不含） | `HStack`、`VStack`、`ZStack`、`Grid`、`LazyVGrid` | `Grid`、`StackPanel`、`RelativePanel`、`Canvas`、`VariableSizedWrapGrid` |
| 窗口 / 宿主 | 无 | `Window`、`WindowGroup`、`Scene` | `Window`、`AppWindow`、`Frame`、`Page` |
| 系统体验 | 无 | Widgets、Live Activities、App Shortcuts、Complications、Controls | App notifications、Jump lists、Share contract（API） |
| 材质 / 效果 | tonal surface、elevation | Liquid Glass、`.glassEffect`、materials | Mica、Acrylic（`AcrylicBrush`）、Reveal |

---

## §6 对本仓库的直接含义

1. **MD3 这一列是本仓库的主线**：官方 36 个组件里已落地 27 个（完整 21 + 变体未做齐 6），缺口 9 个
   （icon buttons、FAB、extended FAB、split buttons、menus、bottom sheets、side sheets、loading indicator、tooltips）。
   其中只有 **menus** 与 **tooltips** 在 `docs/` 里已有设计或引擎半成品，
   其余七个是纯新增工作量。
2. **跨体系需求先查本表**：如果需求写的是 "sheet"、"tabs"、"carousel"，
   先确认它指的是 §4.4 里哪一行的语义，再决定用 MD3 的哪个组件承接；
   不要按名字直接对标。
3. **Apple/WinUI 的独占物大多不是本仓库该补的**：`PersonPicture`、`InkCanvas`、`MediaPlayerElement`、
   `WebView2`、`BreadcrumbBar` 属于平台/媒体能力，只有在真要做对应场景时才立项；
   反过来，`FAB`、`chips`、`snackbar`、`carousel` 这些 MD3 独占物是设计体系自带的能力，应保留。
4. **映射表不替代规格**：数值、状态层、动效仍分别以
   `fluent-reference-implementations.md`（WinUI XAML）、`liquid-glass-analysis.md`（LG 着色器）、
   以及 MD3 导出令牌为准。

---

## 附：本文用到的官方页面

- MD3 组件索引：<https://m3.material.io/components>（36 个组件页）
- Apple HIG Components：<https://developer.apple.com/design/human-interface-guidelines/components>
  （content / layout-and-organization / menus-and-actions / navigation-and-search /
  presentation / selection-and-input / status / system-experiences）
- SwiftUI：*Controls and indicators*、*Text input and output*、*Menus and commands*、
  *Navigation*、*Modal presentations*、*Lists*、*View groupings*
- Liquid Glass：<https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass>
- WinUI 3 控件索引：<https://learn.microsoft.com/en-us/windows/apps/design/controls/>
- 本地参考实现：`/home/ollie/git/widget-ref/liquid_glass_widgets`、`/home/ollie/git/widget-ref/fluent_ui`
