# Layout & Page-Skeleton Widget Inventory — MD3 / WinUI 3 Fluent / SwiftUI

Mined 2026-10-27 by subagent `layout-inventory` (shared task `task-5`). Read-only against every
source; the only file written is this one. **No code was modified.**

Question this document answers, and only this question: **which控件 carry "page structure"?**
It separates two classes and never mixes them:

- **Class A — pure layout containers.** They only arrange. They carry no "this is the app bar"
  or "this is the sidebar" meaning.
- **Class B — page-skeleton controls.** They carry a *region* meaning (app bar / rail / pane /
  tab strip / page shell / card shell) but not the concrete content.

It deliberately contains **no abstraction proposal**. Per the task brief, the scheme is the
Lead's to write; this file supplies facts and cross-references only.

---

## 0. Provenance, and what is actually readable

| id | source | authority | state |
|---|---|---|---|
| **MD3** | `/home/ollie/git/widget-ref/flutter/packages/flutter/lib/src/material/` | **primary source** (official Flutter repo) | readable. 182 `.dart` files |
| **FLU** | `/home/ollie/git/widget-ref/fluent_ui/` | **secondary** — a Flutter *port* of Windows UI, not Microsoft code | readable. `pubspec.yaml` version `4.17.0` |
| **WIN** | `learn.microsoft.com/en-us/windows/...` | **primary** (Microsoft design guidance + WinRT API docs) | fetched 2026-10-27, all URLs re-checked HTTP 200 |
| **SWIFT** | `developer.apple.com/documentation/swiftui/...`, `.../design/human-interface-guidelines/...` | **primary** (Apple) | fetched through Apple's docs JSON API, all URLs re-checked HTTP 200 |

Checked, not assumed:

- The Flutter checkout is at commit `950bf4e` (2026-10-26, `[flutter_tools] Migrate iOS devices…`).
  `git rev-parse --short HEAD` in `/home/ollie/git/widget-ref/flutter`.
- `packages/flutter/lib/src/` contains **only** `animation/`, `material/`, `painting/`;
  `ls widgets` → *No such file or directory*, `ls cupertino` → *No such file or directory*.
  **So every Class A container for Flutter (`Row`, `Column`, `Stack`, `Padding`, `ListView`,
  `CustomScrollView`, …) is unreachable in this checkout** — their `path:line` cannot be cited.
  For those rows the source column gives **two** citations when both exist:
  `material/<file>:<line>` for a framework call-site that *uses* the primitive (proves it exists
  and how MD3 composes with it), plus the `api.flutter.dev` URL for the primitive itself.
  When only the URL exists, the row says `无本地源码` — do not read that as an oversight.
- Apple has no local source at all. Every SwiftUI / HIG / AppKit row cites a URL.
- `fluent_ui` is a port. Where a port row and a Microsoft row disagree, **the Microsoft row is
  the fact** and the divergence is recorded in §B-FLU and in the row's 备注 column.

Apple pages were fetched via Apple's public JSON endpoints (the HTML does not survive a generic
text extractor):

- `https://developer.apple.com/tutorials/data/documentation/swiftui/<symbol>.json`
- `https://developer.apple.com/tutorials/data/design/human-interface-guidelines/<page>.json`

Quoted sentences below come from `abstract` / `primaryContentSections` of those documents.
The human-readable equivalent of each JSON URL is the URL printed in the tables.

A scratch reader used for this is at `scripts/_apple_probe.py`; it is an inspection aid, not part
of the product and not referenced by any crate.

---

## 1. Class A — pure layout containers

Column meanings: **作用** = what it does; **关键参数** = the knobs that change geometry;
**能否嵌套其它骨架控件** = whether the container can host a Class B control (which decides
whether a "page shell" can be a child of it).

### A-1. Material 3 / Flutter

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `Row` / `Column`（Flex 主轴排列） | MD3/Flutter | A | 单轴排列子项，主轴分增长空间，交叉轴按对齐摆放 | `mainAxisAlignment`, `crossAxisAlignment`, `mainAxisSize`, `textDirection` | 能。MD3 官方示例明确把 `NavigationRail` 放进 `Scaffold.body` 的 `Row` | 无本地源码（`src/widgets/` 未 checkout）；调用点 `material/navigation_rail.dart:88`（"Typically used within a [Row] that defines the [Scaffold.body] property"）；https://api.flutter.dev/flutter/widgets/Row-class.html |
| `Stack`（叠层，左上角定位） | MD3/Flutter | A | 子项叠放，后画的盖住先画的；非 positioned 子项按 `alignment` 对齐 | `alignment`, `fit`, `clipBehavior`, `textDirection` | 能（`Scaffold` 的布局委托明确按叠放次序处理） | 无本地源码；调用点 `material/scaffold.dart:1028-1032`（布局委托注释：appBar 在 body **之后**入栈，所以 appBar 的阴影画在 body 之上）；https://api.flutter.dev/flutter/widgets/Stack-class.html |
| `Padding` | MD3/Flutter | A | 只内缩子项，不改变自身背景/边框绘制 | `padding` | 能 | 无本地源码；调用点 `material/app_bar.dart:2303`、`:2315`（折叠时按 `bottomHeight` 内缩）；https://api.flutter.dev/flutter/widgets/Padding-class.html |
| `Spacer`（Flex 内的弹性空位） | MD3/Flutter | A | 在 Flex 中占满剩余空间，把兄弟项挤开 | 无（等价 `Expanded(child: SizedBox.shrink())`） | 能（自身无子项） | 无本地源码；https://api.flutter.dev/flutter/widgets/Spacer-class.html |
| `CustomScrollView` + slivers | MD3/Flutter | A | 滚动容器，子项是 sliver（`SliverAppBar`/`SliverList`/`SliverGrid`），滚动区域本身可承载「折叠头」 | `controller`, `physics`, `cacheExtent`, `center`, `anchor`, `slivers` | 能。`Scaffold.body` 可放它；`SliverAppBar` 是 B 类（见 §2.A） | 无本地源码；调用点 `material/data_table.dart:406`（文档把 `CustomScrollView` 列为大表替代方案）；https://api.flutter.dev/flutter/widgets/CustomScrollView-class.html |
| `ListView` | MD3/Flutter | A | 线性滚动列表，可懒加载 | `scrollDirection`, `padding`, `shrinkWrap`, `itemExtent`, `cacheExtent` | 能（可放任意页面骨架控件作为子项） | 无本地源码；调用点 `material/drawer.dart:100-106`（文档：`Drawer` 的四个条目由一个 `ListView` 承载并可滚动）；https://api.flutter.dev/flutter/widgets/ListView-class.html |
| `GridView` | MD3/Flutter | A | 二维网格滚动，按 `gridDelegate` 定列宽 | `gridDelegate`, `padding`, `scrollDirection` | 能 | 无本地源码；调用点 `material/data_table.dart:404-409`（文档把 `CustomScrollView` 列为大表替代方案，说明滚动容器与表壳分离）；https://api.flutter.dev/flutter/widgets/GridView-class.html |
| `Table` | MD3/Flutter | A | 固定列宽的二维表格容器（非滚动，行列尺寸由 `TableColumnWidth`/`TableRow` 指定） | `columnWidths`, `defaultColumnWidth`, `border`, `children` | 能（`DataTable` 外壳内部即用它，见 §2.A `DataTable`） | 无本地源码；调用点 `material/data_table.dart:446-520`（`DataTable` 文档把自身描述为表格外壳）；https://api.flutter.dev/flutter/widgets/Table-class.html |
| `Wrap`（流式换行；MD3 里为 `Wrap`，无 `Flow` 的常规用途） | MD3/Flutter | A | 子项按行/列流式排列，放不下就换到下一行/列 | `direction`, `alignment`, `spacing`, `runSpacing`, `crossAxisAlignment` | 能 | 无本地源码；https://api.flutter.dev/flutter/widgets/Wrap-class.html |
| `AspectRatio` | MD3/Flutter | A | 给子项施加固定宽高比 | `aspectRatio` | 能 | 无本地源码；https://api.flutter.dev/flutter/widgets/AspectRatio-class.html |
| `Align` / `Center` | MD3/Flutter | A | 在自身占位内对齐子项；`Center` 是 `Align(center)` 的简写 | `alignment`, `widthFactor`, `heightFactor` | 能 | 无本地源码；https://api.flutter.dev/flutter/widgets/Align-class.html |
| `SizedBox`（含 `SizedBox.expand`） | MD3/Flutter | A | 强制尺寸或占位；`ScaffoldPage` 的默认 content 就是 `SizedBox.expand()` | `width`, `height`, `child` | 能 | 无本地源码；https://api.flutter.dev/flutter/widgets/SizedBox-class.html |
| `LayoutBuilder` | MD3/Flutter | A | 把父给的最大/最小约束交给 builder，用来做断点分支（页面级响应式的落点） | `builder` | 能（builder 返回值可为任意骨架控件） | 无本地源码；https://api.flutter.dev/flutter/widgets/LayoutBuilder-class.html |
| `SafeArea` | MD3/Flutter | A | 用 `MediaQuery` 的 padding 内缩，避开刘海/状态栏/圆角 | `top`, `bottom`, `left`, `right`, `minimum`, `maintainBottomViewPadding` | 能 | 无本地源码；调用点 `material/app_bar.dart:1190`（`SafeArea(bottom: false, child: appBar)`）、`material/bottom_app_bar.dart:232`、`material/navigation_bar.dart:291`、`material/navigation_drawer.dart:189`、`material/navigation_rail.dart:556`；https://api.flutter.dev/flutter/widgets/SafeArea-class.html |
| `MediaQuery` / `MediaQuery.of(context).padding` | MD3/Flutter | A | 读取视口尺寸、内边距（安全区）、键盘 inset，作为布局输入 | `size`, `padding`, `viewPadding`, `viewInsets`, `textScaler` | 不适用（非容器，是数据源） | 无本地源码；调用点 `material/scaffold.dart:2776-2777`（`resizeToAvoidBottomInset` 默认 true，据此调整 body）；https://api.flutter.dev/flutter/widgets/MediaQuery-class.html |

### A-2. WinUI 3 / Fluent

Microsoft's own comparison table is the source for the first six rows: it lists each panel's
positioning rule, whether it clips, and whether it constrains children. Read in
`https://learn.microsoft.com/en-us/windows/apps/design/layout/layouts-with-xaml` §"Layout panels".

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `Grid` | WinUI 3 | A | 行列布局；支持 Auto / 星号比例（`*`、`2*`）/ 固定像素三种尺寸；默认列宽为 `*` | `RowDefinitions`, `ColumnDefinitions`（`Width="Auto"/"*"/"2*"/"44"`）、`Grid.Row`/`Grid.Column`/`RowSpan`/`ColumnSpan` 附加属性、`MinWidth`/`MaxWidth` | 能（官方布局文章的所有示例都把控件放进 `Grid`；`SplitView` 的 content 就是 `Grid`） | https://learn.microsoft.com/en-us/windows/apps/design/layout/layouts-with-xaml ；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.grid |
| `StackPanel` | WinUI 3 | A | 单行/单列堆叠（`Orientation`） | `Orientation`, `Spacing` | 能 | 同上布局文章；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.stackpanel |
| `RelativePanel` | WinUI 3 | A | 按「相对面板边缘/中心」或「相对兄弟元素」定位，用附加属性表达；用于自适应重排 | 面板对齐、兄弟对齐、兄弟位置三类附加属性 | 能 | 同上布局文章；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.relativepanel |
| `Canvas` | WinUI 3 | A | 绝对定位，不参与流式布局；忽略 `HorizontalAlignment`/`VerticalAlignment` 的 Stretch | `Canvas.Top`, `Canvas.Left`, `Canvas.ZIndex` | 能，但子项不随窗宽自适应；官方定位为"特殊用途/静态区域" | 同上布局文章；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.canvas |
| `VariableSizedWrapGrid` | WinUI 3 | A | 行列自动换行的网格，单元格可跨行跨列 | `ItemWidth`, `ItemHeight`, `Orientation`, `RowSpan`/`ColumnSpan` | 能 | 同上布局文章（`VariableSizedWrapGrid` 行）；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.variablesizedwrapgrid |
| `Border` / `ContentPresenter`（XAML 装饰与内容宿主） | WinUI 3 | A | `Border` 只负责描边+圆角+内边距；`ContentPresenter` 是控件模板里放内容的宿主 | `BorderBrush`, `BorderThickness`, `CornerRadius`, `Padding`, `Child` | 能 | https://learn.microsoft.com/en-us/windows/apps/design/layout/alignment-margin-padding （HIG：Alignment/Margin/Padding 三者的区别） |
| `ScrollViewer` | WinUI 3 | A | 滚动/平移/缩放容器；视口 = viewport，内容 = extent；滚动条覆盖在内容上 16px | `HorizontalScrollBarVisibility`, `VerticalScrollBarVisibility`, `ZoomMode`, `HorizontalScrollMode` | 能（可包住任意骨架控件） | https://learn.microsoft.com/en-us/windows/apps/design/controls/scroll-controls ；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.scrollviewer |
| `ScrollView`（第二套滚动控件） | WinUI 3 | A | 与 `ScrollViewer` 行为/用法相似，但基于 `InteractionTracker`，支持动画驱动的视图切换，并为 `ItemsRepeater` 做完整适配 | 同上（新的交互属性集） | 能 | https://learn.microsoft.com/en-us/windows/apps/design/controls/scroll-controls （"WinUI 3 has two different scroll viewer controls available"） |
| `ItemsRepeater` | WinUI 3 | A | **数据驱动的面板**：无默认 UI、不含焦点/选择策略，布局由 `Layout` 决定；概念上是 panel 而非完整控件 | `ItemsSource`, `ItemTemplate`, `Layout` | 能（可承载任意模板内容） | https://learn.microsoft.com/en-us/windows/apps/design/controls/items-repeater |
| `Frame`（页面宿主 / 导航容器） | WinUI 3 | A/B 边界 | 承载 `Page` 实例并维护返回栈；`NavigationView` 示例把 `<Frame x:Name="ContentFrame"/>` 作为 content | `SourcePageType`, `CanGoBack`, `GoBack()/GoForward()`, `Navigate()` | 能（"A common way to add complex UI as the content of a tab is to encapsulate it in a UserControl or a Page"，而 Page 一般装在 Frame 里） | 见 §2.WIN 的 `NavigationView` 行与 `TabView` 行；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.page |

### A-3. SwiftUI

Platform availability below is the `introducedAt` Apple's doc JSON reports for the symbol; it is
part of the fact (e.g. `Grid` is not available before iOS 16).

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `HStack` / `VStack`（及 `LazyHStack`/`LazyVStack`） | SwiftUI | A | 单轴排列；非 lazy 版本一次性渲染全部子视图，lazy 版本只创建需要显示的项 | `alignment`, `spacing`；（lazy）`pinnedViews` | 能 | https://developer.apple.com/documentation/swiftui/hstack （iOS 13.0+）、https://developer.apple.com/documentation/swiftui/lazyvstack （iOS 14.0+） |
| `ZStack` | SwiftUI | A | 叠放子视图，后者在 z 轴更高；`Alignment` 决定两轴对齐（默认 center） | `alignment`(, `spacing`) | 能 | https://developer.apple.com/documentation/swiftui/zstack （iOS 13.0+） |
| `Grid`（+ `GridRow`） | SwiftUI | A | 二维网格；行与列作为一次操作同时生成，对齐与间距作用于单元格而不是"先分行再分列" | `alignment`, `horizontalSpacing`, `verticalSpacing` | 能 | https://developer.apple.com/documentation/swiftui/grid （iOS 16.0+） |
| `Group` | SwiftUI | A | 把多个视图收成一个实体，**不改变布局**；加在 group 上的修饰符作用于全部成员 | `init(content:)` | 能 | https://developer.apple.com/documentation/swiftui/group （"without affecting the layout of those views"） |
| `Spacer` | SwiftUI | A | 沿所在栈的主轴扩张的无内容视图；不在栈中时两轴都扩张 | `minLength` | 能（自身无子项） | https://developer.apple.com/documentation/swiftui/spacer |
| `ScrollView` | SwiftUI | A | 可滚动区域，水平/垂直/双向均可；**不提供缩放** | `axes`（默认 `.vertical`）, `showsIndicators`（默认 `true`） | 能 | https://developer.apple.com/documentation/swiftui/scrollview ；初始化器 https://developer.apple.com/documentation/swiftui/scrollview/init(_:showsindicators:content:) |
| `.aspectRatio(_:contentMode:)` | SwiftUI | A | 把视图尺寸约束到给定宽高比 | `aspectRatio: CGFloat?`, `contentMode: .fit/.fill` | 能 | https://developer.apple.com/documentation/swiftui/view/aspectratio(_:contentmode:) （iOS 13.0+） |
| `padding` / `scenePadding` / `contentMargins` | SwiftUI | A | 内缩；`contentMargins` 作用于滚动型容器的内容边距 | `edges`, `length`, `for:` | 能 | https://developer.apple.com/documentation/swiftui/view-layout （"Padding and spacing" 分组） |
| `frame(minWidth:idealWidth:maxWidth:…)` / `fixedSize()` / `layoutPriority(_:)` | SwiftUI | A | 交出自适应尺寸的提议（ideal ← 父在 min..max 之间决定）；`fixedSize` 拒绝被压；`layoutPriority` 参与空间竞争 | `minWidth`, `idealWidth`, `maxWidth`, `alignment`, 优先级 | 能 | https://developer.apple.com/documentation/swiftui/view-layout （"Size" 分组） |
| `Layout` 协议（`HStackLayout`/`VStackLayout`/`AnyLayout` 等） | SwiftUI | A | 自定义容器：实现 `sizeThatFits(proposal:subviews:cache:)` 与 `placeSubviews(in:proposal:subviews:cache:)`，可报告 `layoutProperties`、`spacing`、`explicitAlignment` | `proposal`, `subviews`, `cache`, `layoutProperties` | 能（容器可包含任意视图） | https://developer.apple.com/documentation/swiftui/layout （iOS 16.0+） |
| `.safeAreaInset(edge:…)` | SwiftUI | A（带区域语义，见 §4） | 在指定水平边放一块内容，**并把被修饰视图按该内容尺寸内缩**，同时增大其安全区 | `edge: HorizontalEdge`, `alignment: VerticalAlignment`, `spacing` | 能（content 可放骨架控件） | https://developer.apple.com/documentation/swiftui/view/safeareainset(edge:alignment:spacing:content:) （iOS 15.0+；doc JSON 中 `edge` 参数类型为 `HorizontalEdge`） |
| `.safeAreaBar(edge:…)` | SwiftUI | A（同上，iOS 26 新增） | 与 `safeAreaInset` 同族，但语义是"自定义 bar" | 同上 | 能 | https://developer.apple.com/documentation/swiftui/view/safeareabar(edge:alignment:spacing:content:) （iOS 26.0+） |
| `.ignoresSafeArea(_:edges:)` | SwiftUI | A | 让视图扩张到安全区之外；`regions` 可选 `.all`/`.container`/`.keyboard` | `regions: SafeAreaRegions`, `edges: Edge.Set` | 能 | https://developer.apple.com/documentation/swiftui/view/ignoressafearea(_:edges:) ；`SafeAreaRegions` https://developer.apple.com/documentation/swiftui/safearearegions |
| `ContainerRelativeShape` | SwiftUI | A | 形状尺寸由「当前容器形状」的内缩版本决定；无容器形状时退化为矩形 | `init()` | 能 | https://developer.apple.com/documentation/swiftui/containerrelativeshape （iOS 14.0+） |
| `@ViewBuilder`（结果构建器） | SwiftUI | A（语法设施） | 把闭包里的多个视图合并成一个泛型视图；条件分支走 `buildEither` | `buildBlock`, `buildEither`, `buildIf` | 不适用 | https://developer.apple.com/documentation/swiftui/viewbuilder |

---

## 2. Class B — page-skeleton controls

Each row states the region the control owns, the knobs that change how that region behaves, and
whether a *nested* page shell is possible. For cards the note column answers the three questions
the brief demands: **elevation? expandable? header/media/actions partitions?**

### 2.A Material 3

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `Scaffold` | MD3 | B | 整个页面的外框：统一编排 12 个具名插槽，并独占页面几何 | `appBar`, `body`, `floatingActionButton`(+`floatingActionButtonLocation`/`…Animator`), `persistentFooterButtons`, `drawer`, `endDrawer`, `bottomNavigationBar`, `bottomSheet`, `backgroundColor`, `resizeToAvoidBottomInset`(默认 true), `primary`, `extendBody`, `extendBodyBehindAppBar`, `drawerEdgeDragWidth` | **能**：`body` 可放任意骨架控件；`drawer`/`endDrawer` 是 B；`appBar` 必须是 `PreferredSizeWidget`（即 B，如 `AppBar`/`SliverAppBar`） | `material/scaffold.dart:1683`（类）；插槽 ctor `:1686-1712`；插槽枚举 `:64-77`（`body, appBar, bodyScrim, bottomSheet, snackBar, materialBanner, persistentFooter, bottomNavigationBar, floatingActionButton, drawer, endDrawer, statusBar`）；布局委托 `:988`；`performLayout` 逐槽位 `:1036-1287` |
| `ScaffoldMessenger`（页面级通知宿主，非布局但属于外框职责） | MD3 | B | 在 `Scaffold` 之上悬挂 SnackBar / MaterialBanner；**只作用于子树里的根 Scaffold**，嵌套 Scaffold 需要新的 Messenger 作用域 | `showSnackBar`, `showMaterialBanner`, `hideCurrentSnackBar`, `clearSnackBars` | 能（作为 `MaterialApp`/页面外层） | `material/scaffold.dart:112`（类）；"only present the notification to the root Scaffold of the subtree of Scaffolds" `:93-97` |
| `AppBar` | MD3 | B | 顶部工具栏区域：leading / title / actions / bottom 四个语义分区，可随滚动改变 elevation | `leading`, `automaticallyImplyLeading`(默认 true), `title`, `actions`, `automaticallyImplyActions`, `flexibleSpace`, `bottom`(`PreferredSizeWidget`), `elevation`, `scrolledUnderElevation`, `notificationPredicate`, `toolbarHeight`, `leadingWidth`, `titleSpacing`, `centerTitle`, `primary`, `forceMaterialTransparency`, `actionsPadding` | **能**：`bottom` 常放 `TabBar`（B） | `material/app_bar.dart:188`（类）；ctor `:196-227`；字段位置 `:292, :337, :384, :405, :417, :444, :465, :756(forceMaterialTransparency 邻近), :771` |
| `SliverAppBar`（`SliverAppBar.medium/.large`） | MD3 | B | 滚动区内的 AppBar：随滚动折叠/展开，把 toolbar 变成 sliver | `pinned`, `floating`, `snap`, `expandedHeight`, `flexibleSpace`, `stretch`, `pinned+floating` 组合、`bottom` | 能（`Scaffold.body` 的 `CustomScrollView` 内；`bottom: TabBar()`） | `material/app_bar.dart:1560+`（`SliverAppBar` 类；ctor 起始 `:1571`） |
| `BottomAppBar` | MD3 | B | 底部工具栏区域，可给 FAB 让出缺口 | `color`, `elevation`, `shape`（`NotchedShape`）, `notchMargin`(默认 4.0), `padding`, `height`, `child` | 能（`Scaffold.bottomNavigationBar` 或 `bottomSheet` 位置） | `material/bottom_app_bar.dart:64`（类）；ctor 参数 `:75-84` |
| `NavigationBar`（MD3 底部导航） | MD3 | B | 底部目的地栏；M3 默认高 80、elevation 3、底色 `surfaceContainer` | `destinations`（`>=2` 断言）, `selectedIndex`, `onDestinationSelected`, `elevation`, `height`, `labelBehavior`, `indicatorColor/Shape`, `maintainBottomViewPadding` | 能（放进 `Scaffold.bottomNavigationBar`） | `material/navigation_bar.dart:97`（类）；ctor `:103-121`；断言 `:122-123`；生成默认 `:1430-1440` |
| `NavigationDestination` | MD3 | B（B 的子项类型） | 底部导航的一个目的地：icon / selectedIcon / label | `icon`, `selectedIcon`, `label`, `tooltip`, `enabled` | 否（只作为 `destinations` 元素） | `material/navigation_bar.dart:368`（类）；字段 `:386, :398, :409` |
| `NavigationRail` | MD3 | B | 竖向侧栏（窄屏→宽屏的导航栏）；MD3 默认 elevation 0、底色 `surface` | `extended`(仅在 `labelType` 为 null 或 `none` 时可用), `leading`, `trailing`, `destinations`, `selectedIndex`, `labelType`(`none`/`selected`/`all`), `groupAlignment`, `minWidth`, `minExtendedWidth`, `indicatorShape/Color`, `leadingAtTop`, `trailingAtBottom`, `scrollable`, `mainAxisAlignment` | 能（文档建议放进 `Scaffold.body` 的 `Row`） | `material/navigation_rail.dart:68`（类）；ctor `:91-114`；约束文档 `:70-82`；`NavigationRailLabelType` `:1082-1094`；生成默认 `:1236-1248` |
| `NavigationDrawer`（+ `NavigationDrawerDestination`） | MD3 | B | 抽屉式导航：header + destinations + footer 三段 | `children`, `header`, `footer`, `backgroundColor`, `elevation`, `indicatorColor/Shape`, `selectedIndex`, `onDestinationSelected`, `tilePadding`(默认水平 12) | 能（放进 `Scaffold.drawer`/`endDrawer`） | `material/navigation_drawer.dart:54`（类）；ctor `:58-69`；字段 `:122-151` |
| `Drawer` | MD3 | B | 抽屉的**外观容器**（背景/圆角/宽度），不含导航语义；开合由 `DrawerController` 负责 | `width`, `shape`, `elevation`, `backgroundColor`, `child`, `semanticLabel`, `clipBehavior` | 能（`child` 可放 `NavigationDrawer`/`ListView`） | `material/drawer.dart:138`（类）；ctor `:146-152`；`DrawerController` `:323` |
| `TabBar`（primary / secondary 两种） | MD3 | B | 横向 tab 条；MD3 的 `tabAlignment` 取值来自 Compose 的 `TabRow` | `tabs`, `controller`, `isScrollable`, `indicatorColor/Weight/Padding/Size`, `labelColor`, `tabAlignment`(`start`/`startOffset`/`fill`/`center`), `indicatorAnimation`(`linear`/`elastic`) | 能（通常作为 `AppBar.bottom`） | `material/tabs.dart:992`（类）；ctor `:1010-1044`；`TabAlignment` `:63-88`；`TabIndicatorAnimation` `:95-103` |
| `TabBarView` | MD3 | B | 与 `TabBar` 配对的内容翻页区，一 tab 一 child | `children`, `controller`, `physics`, `viewportFraction`, `clipBehavior` | 能（放在 `Scaffold.body`） | `material/tabs.dart:2214`（类 + ctor） |
| `BottomSheet`（+ `showModalBottomSheet` / `showBottomSheet`） | MD3 | B | 底部面板区域，持久式或模态式 | `enableDrag`, `showDragHandle`, `dragHandleColor/Size`, `elevation`, `shape`, `clipBehavior`, `constraints`, `builder`；`showModalBottomSheet` 额外有 `isScrollControlled`, `useSafeArea`(默认 false), `sheetAnimationStyle` | 能（`Scaffold.bottomSheet` 或 push 路由） | `material/bottom_sheet.dart:77`（类）；ctor `:86-99`；两种形态说明 `:45-58`；模态接口 `:1286-1309` |
| `Card`（`Card` / `Card.filled` / `Card.outlined`） | MD3 | B | 内容分组的外壳；M3 三变体共享 `CardThemeData` | `color`, `shadowColor`, `surfaceTintColor`, `elevation`, `shape`, `borderOnForeground`, `clipBehavior`, `margin`, `semanticContainer`, `child` | 能（可含 `ListTile`/`ExpansionTile`/`DataTable`） | `material/card.dart:73`（类）；三变体 ctor `:80-91/:102-113/:130-141`；变体说明 `:52-60` |
| `ListTile` | MD3 | B | 列表行的三分区（leading / title+subtitle / trailing） | `leading`, `title`, `subtitle`, `trailing`, `isThreeLine`, `dense`, `visualDensity`, `shape`, `contentPadding`, `enabled`, `selected`, `selectedTileColor`, `tileColor`, `minVerticalPadding`, `minLeadingWidth`, `titleAlignment`, `onTap` | 能（`Scaffold.drawer` 的常用行、`Card` 内行） | `material/list_tile.dart:385`（类）；ctor `:387-428`；字段 `:435-716` |
| `ExpansionTile` | MD3 | B | 单行行头 + 可展开的 `children`；行头本身就是 `ListTile` | `leading`, `title`, `subtitle`, `trailing`, `children`, `initiallyExpanded`, `maintainState`, `controller`(`ExpansibleController`), `shape`, `collapsedShape`, `backgroundColor`, `collapsedBackgroundColor`, `controlAffinity`, `expandedCrossAxisAlignment/Padding` | 能（`children` 可放任意骨架控件；文档建议配 `ListView`） | `material/expansion_tile.dart:118`（类）；ctor `:120-149`；字段 `:169-411` |
| `ExpansionPanelList`（+ `ExpansionPanel` / `ExpansionPanelRadio`） | MD3 | B | 多面板的外壳列表，带展开动画与分隔 | `children`, `expansionCallback`, `animationDuration`(默认 `kThemeAnimationDuration`), `elevation`(默认 **2**), `materialGapSize`(默认 16.0), `expandedHeaderPadding`, `dividerColor`, `initialOpenPanelValue`（radio 版） | 能（`body` 可为任意骨架控件） | `material/expansion_panel.dart:185`（类）；ctor `:188-223`；`ExpansionPanel` 数据类 `:76-139`；`ExpansionPanelRadio` `:144`；`MergeableMaterial` 使用 `:484-487` |
| `DataTable`（+ `DataColumn` / `DataRow` / `DataCell`） | MD3 | B | 表头 + 数据行的外壳；自身不处理滚动与大数据量 | `columns`, `rows`, `sortColumnIndex`, `sortAscending`, `showCheckboxColumn`(默认 true), `headingRowHeight`, `dataRowMinHeight`/`dataRowMaxHeight`, `columnSpacing`, `horizontalMargin`, `dividerThickness`, `showBottomBorder`, `border`, `clipBehavior`(默认 `Clip.none`), `decoration`, `dataRowColor`, `headingRowColor` | 能（文档建议外层包 `SingleChildScrollView`/`PaginatedDataTable`） | `material/data_table.dart:446`（类）；ctor `:468-520`；字段 `:522-777`；替代方案说明 `:398-409` |
| `Divider` / `VerticalDivider` | MD3 | B（分区标记） | 区域分隔线，占位高度可独立于线宽 | `height`, `thickness`, `indent`, `endIndent`, `color`, `radius` | 能（常用于 `ExpansionTile` 的 shape/`Column` 之间） | `material/divider.dart:56`（类）；ctor `:61-73`；`VerticalDivider` `:244` |
| `CarouselView` | MD3 | B | 横向主视觉区（仅 MD3 有该控件） | `itemExtent`, `padding`, `shrinkExtent`, `onTap`, `itemBuilder`, `initialItem` | 能 | `material/carousel.dart:132`（类） |

### 2.B WinUI 3（官方为准）— 与本地 `fluent_ui` 移植的差异单列

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `NavigationView` | WinUI 3 | B | **应用级**顶层导航 + 页面外框：pane（可 Left/Top）+ header + content 三区一体 | `PaneDisplayMode`(`Auto`/`Top`/`Left`/`LeftCompact`/`LeftMinimal`), `IsPaneOpen`, `IsPaneToggleButtonVisible`, `MenuItems`, `FooterMenuItems`, `PaneTitle`, `PaneHeader`, `PaneCustomContent`, `PaneFooter`, `Header`, `AlwaysShowHeader`, `IsBackEnabled`, `IsSettingsVisible`, `AutoSuggestBox`, `CompactModeThresholdWidth`, `ExpandedModeThresholdWidth` | **能**：content 常放 `Frame`；`NavigationViewItem` 可展开为层级 | https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview （pane/header/content 解剖、display modes、adaptive 断点 1008/641/640、header 固定高 52px、"acts as a scroll clipping point"、content 边距 12px(Minimal)/24px） |
| `NavigationViewItem` / `NavigationViewItemHeader` / `NavigationViewItemSeparator` | WinUI 3 | B（子项类型） | pane 内的导航项 / 分组标题 / 分隔 | `Content`, `Icon`, `MenuItems`（层级） | 否（只作 pane 元素） | 同上页面 "Pane" 小节 |
| `SplitView` | WinUI 3 | B | 抽屉式「可展开/折叠的 pane + 内容区」；内容区始终可见 | `IsPaneOpen`, `DisplayMode`(`Overlay`/`Inline`/`CompactOverlay`/`CompactInline`), `OpenPaneLength`(示例 296), `CompactPaneLength`(默认 48) | 能（pane 与 content 各可放任意内容） | https://learn.microsoft.com/en-us/windows/apps/design/controls/split-view ；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.splitview |
| `TabView` + `TabViewItem` | WinUI 3 | B | 文档式页签：页签条(header+footer) + 每个 `TabViewItem` 自带的 content | `SelectedIndex`, `TabItems`, `TabWidthMode`(`Equal`/`SizeToContent`/`Compact`), `CloseButtonOverlayMode`(`Auto`/`OnPointerOver`), `AddTabButtonCommand`, `TabStripHeader`/`TabStripFooter`；`TabViewItem`: `Header`, `IconSource`, `IsClosable`, `Content` | 能（content 常封装为 `UserControl`/`Page`） | https://learn.microsoft.com/en-us/windows/apps/design/controls/tab-view （"Although the content is displayed inside of the TabView control, the content is actually a part of the TabViewItem"） |
| `Expander` | WinUI 3 | B | 常显 `Header` + 可折叠 `Content`；**展开时把其它元素推开，不覆盖**；可向上或向下展开 | `Header`, `Content`, `IsExpanded`, `ExpandDirection`, `HorizontalContentAlignment`/`VerticalContentAlignment` | 能（官方明示 content 里可**嵌套** `Expander`） | https://learn.microsoft.com/en-us/windows/apps/design/controls/expander ；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.expander |
| `InfoBar` | WinUI 3 | B | 应用级状态条：**占据布局空间、像普通子元素一样参与布局，不覆盖也不浮在内容上** | `Severity`(Informational/Success/Warning/Error), `Title`, `Message`, `IsOpen`, `IsClosable`, `ActionButton` | 能（"inline with other UI content"） | https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar （"An InfoBar will take up space in your layout and behave like any other child element. It will not cover up other content or float on top of it."） |
| `ListView` / `GridView` | WinUI 3 | B | 带完整 UI/交互/虚拟化的集合控件，同源自 `ListViewBase`；`ListView` 单列纵排，`GridView` 行列纵横排 | `ItemsSource`, `ItemTemplate`, `SelectionMode`, `IsItemClickEnabled` | 能 | https://learn.microsoft.com/en-us/windows/apps/design/controls/listview-and-gridview ；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.listview 、https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.gridview |
| `Page`（+ `Frame` 导航） | WinUI 3 | B | 一"页"的宿主对象；`Frame` 持有 `Page` 栈 | `NavigationCacheMode`, `Frame` 属性 | 能（`Frame` 可作为 `NavigationView`/`TabViewItem` 的 content） | 同 `NavigationView`/`TabView` 行；API https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.page |
| **Card** | WinUI 3 | B | WinUI 3 **没有** `Card` 控件类；卡片是 `Border`/`Grid` 组合出的样式，或由 `Expander`/`ListView` 项模板提供 | 无官方 Card API | 能（组合式） | 反证：`https://learn.microsoft.com/en-us/windows/apps/design/controls/cards` 返回 **404**；https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.card 返回 **404**；官方设计文档里引导用 `Expander` + `InfoBar` + `ListView` 项（见本表各行）。**所以 WinUI 的"卡片"不是控件，是模板组合。** |
| Acrylic / Mica（材质，非控件） | WinUI 3 | B（区域外观，非布局） | 面板底材：`AcrylicBrush` 做半透明纹理；`Window.SystemBackdrop` + `DesktopAcrylicBackdrop`/`MicaBackdrop` 做窗口级背景 | `AcrylicBrush.TintColor/TintOpacity/FallbackColor`, `SystemBackdrop` | 不适用 | https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic （"Acrylic is a type of Brush that creates a translucent texture."） |

#### B-FLU. 本地 `fluent_ui` 移植 vs 官方（**移植是二手，官方为准**）

| 移植控件 | 本地位置 | 官方对应 | 一致？ | 记号 |
|---|---|---|---|---|
| `NavigationView` | `lib/src/controls/navigation/navigation_view/view.dart:127`；ctor `:129-140`（`titleBar`, `pane`, `content`, `clipBehavior`, `contentShape`, `onOpenSearch`, `transitionBuilder`, `paneBodyBuilder`, `onDisplayModeChanged`） | `NavigationView` | **部分一致**。移植把"pane + content"拆成 `NavigationView(pane: NavigationPane, content:)` 两个对象；官方是同一控件上的属性。移植的断点 `1008/641/640` 与官方文档一致（`view.dart:81-84` vs 官方 adaptive 小节） | 移植多了 `titleBar` 插槽（官方的窗口标题栏是 `AppWindowTitleBar`，不在 `NavigationView` 内） |
| `NavigationPane` + `PaneDisplayMode` | `pane.dart:97`（类）、`pane.dart:35`（枚举 `top/left/leftCompact/leftMinimal/auto`） | 同 | 一致（枚举名与官方 5 种 display mode 对齐） | 移植把官方 `PaneDisplayMode` 的 5 值与文档措辞一一对应（`pane.dart:36-48` 的注释逐段复制官方"Use top navigation when…"） |
| `PaneItem`/`PaneItemHeader`/`PaneItemSeparator`/`PaneItemExpander`/`PaneItemAction`/`PaneItemWidgetAdapter` | `pane_items.dart:127/718/670/830/789/1283` | `NavigationViewItem`/`…ItemHeader`/`…ItemSeparator` | 名称不同、能力相当，**多出** `PaneItemExpander`（层级）与 `PaneItemWidgetAdapter`（任意 widget）；官方层级由 `NavigationViewItem` 的 `MenuItems` 表达 | 移植的层级走独立类，官方走属性 |
| `TabView` | `tab_view/tab_view.dart:62`；ctor `:66-86` | `TabView` | 一致方向：`currentIndex`, `tabs`, `onChanged`, `onNewPressed`, `closeButtonVisibility`(默认 `always`), `tabWidthBehavior`(默认 `equal`), `header`/`footer`, `showScrollButtons`, `onReorder` | 官方用 `AddTabButtonCommand`，移植用 `onNewPressed` 回调；官方 `TabWidthMode` 三值 `Equal/SizeToContent/Compact`，移植 `TabWidthBehavior` 同名三值（`tab.dart:12-23`） |
| `Tab` | `tab_view/tab.dart:197`；ctor `:200-215`（`text`, `body`, `icon`, `closeIcon`, `onClosed`, `semanticLabel`, `disabled`） | `TabViewItem` | **结构一致**：都同时承载"页签头"和"内容体" | 官方 `Content` 在 `TabViewItem` 上；移植 `body` 在 `Tab` 上（`tab.dart:243`）——同一个事实：**内容属于页签项，不属于页签条** |
| `ScaffoldPage` / `PageHeader` | `lib/src/controls/layout/page.dart:65` / `:261` | **无直接对应**：官方没有"页面内容壳"控件（`Page` 只是导航宿主） | **移植独有**。`ScaffoldPage(header, content, footer, padding, backgroundColor, resizeToAvoidBottomInset)`；padding 默认 24 纵向、横向 ≤640 时 12 否则 24（`page.dart:16-24` 文档） | 这正是 §4 里"谁负责页面外框"差异的关键：Windows 侧官方把内容边距写成**建议**（12/24px），移植把它变成**控件行为** |
| `Expander` | `surfaces/expander.dart:96`；ctor `:97-120`（`header`, `content`, `leading`, `icon`, `trailing`, `direction`, `initiallyExpanded`, `onStateChanged`, `headerBackgroundColor`, `headerShape`, `contentBackgroundColor`, `contentPadding`, `contentShape`） | `Expander` | 一致（都是 header+content，可嵌套，展开推开） | 移植新增 `leading`/`trailing`/`icon` 三个 header 内插槽，官方 header 本身即任意内容 |
| `Card` | `surfaces/card.dart:46`；ctor `:48-56`（`child`, `padding`(默认 12), `margin`, `backgroundColor`, `borderColor`, `borderRadius`(默认 4)） | **官方无 Card 控件** | **移植独有** | 详见 §2.B `Card` 行的 404 反证 |
| `InfoBar` | `surfaces/info_bar.dart:208`；`InfoBarSeverity` `:105`（`info/warning/error/success`） | `InfoBar` | 一致（四档 severity） | 字段 `:276-324`（`severity`, `style`, `title`, `content`, `action`, `onClose`, `isLong`） |
| `CommandBar` | `surfaces/commandbar.dart:147`；`primaryItems` `:149`, `secondaryItems` `:155`, `overflowBehavior` `:166`, `compactBreakpointWidth` `:173`, `isCompact` `:187` | WinUI 的 `CommandBar`（本文件未逐字核对官方页，见 §5 缺口） | 未完全核对 | 形态（primary/secondary + overflow）与官方公开描述一致，但**本文件没有官方 URL 行**，按 §5 记一笔 |
| `BreadcrumbBar` | `navigation/breadcrumb_bar.dart:109`；`items` `:113`, `onItemPressed` `:148` | 官方有 `BreadcrumbBar`（未核对官方页） | 未核对 | 同上 |
| `HorizontalScrollView` | `layout/horizontal_scroll_view.dart:14`；`child` `:18`, `scrollPhysics` `:21`, `mouseWheelScrolls` `:27`, `scrollDirection` `:30` | 官方 `ScrollViewer` 的水平配置 | 等价配置**被提升为独立控件**（Windows 没有单独的 `HorizontalScrollView`） | 这是移植的便利包装，不是官方控件 |
| `SplitView` | **不存在**（`grep -rn "SplitView" lib/src` → 无结果） | `SplitView` | **缺失** | 移植用 `NavigationView` 的 compact/minimal pane 代替；官方把 `SplitView` 与 `NavigationView` 明确分开（官方 split-view 页："If you'd like to build a navigation menu … use the NavigationView control instead"） |

### 2.C SwiftUI

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `NavigationSplitView` | SwiftUI | B | **两列或三列**的根级结构：前导列的选择驱动后续列的内容；典型作为 `Scene` 的根视图 | `sidebar:`, `content:`(可选), `detail:`；`columnVisibility:`, `preferredCompactColumn:` | **能**：官方明示"可以在一列里再嵌入 `NavigationStack`" | https://developer.apple.com/documentation/swiftui/navigationsplitview （iOS 16.0+/macOS 13.0+；"You can also embed a NavigationStack in a column."） |
| `NavigationStack` | SwiftUI | B | 单列的后退栈：根视图 + 可压入的更上层视图；根视图不可被移除 | `root:`, `path:` (`Binding`), `navigationDestination(for:destination:)` | 能（可放进 `NavigationSplitView` 的某一列） | https://developer.apple.com/documentation/swiftui/navigationstack （iOS 16.0+） |
| `TabView` | SwiftUI | B | 在多个子视图之间切换；`sidebarAdaptable` 样式在 iPadOS/macOS 会变成侧栏 | `init(content:)`, `init(selection:content:)`, `.tabViewStyle(...)`, `.badge(...)`；子项 `Tab` | **能**：iPadOS 的 `sidebarAdaptable` 允许用 `TabSection` 声明二级层级 | https://developer.apple.com/documentation/swiftui/tabview （iOS 13.0+）；https://developer.apple.com/documentation/swiftui/tab （iOS 18.0+） |
| `SidebarAdaptableTabViewStyle` | SwiftUI | B（同一控件的样式） | 让 `TabView` 按平台变身：iPadOS 顶栏可自适应为侧栏；iOS 底栏；**macOS 与 tvOS 恒为侧栏** | `init()`；通过 `.tabViewStyle(.sidebarAdaptable)` 应用 | 能 | https://developer.apple.com/documentation/swiftui/sidebaradaptabletabviewstyle （iOS 18.0+） |
| `List` | SwiftUI | B | 单列数据行容器，可选中（单选/多选）、可重排、可分组 | `selection:`, `init(_:rowContent:)`, `init(_:selection:rowContent:)`, `init(_:children:…)`（层级）, `editActions:`, `.listStyle(...)` | 能（可作 `NavigationSplitView.sidebar` 的根） | https://developer.apple.com/documentation/swiftui/list （iOS 13.0+） |
| `Section` | SwiftUI | B（分区标记） | 在 `List`/`Picker`/`Form` 内给内容分节，可带 header/footer | `init(content:header:footer:)`, `init(content:header:)`, `init(isExpanded:content:header:)`（**可折叠**） | 能（可嵌套 `List` 行内骨架） | https://developer.apple.com/documentation/swiftui/section （iOS 13.0+；可折叠项需 iOS 17，本文件仅核对到符号存在） |
| `Form` | SwiftUI | B | 数据录入页的**分组容器**；平台各自渲染（iOS 分组列表 / macOS 竖向栈） | `init(content:)`, `init(_:)` | 能 | https://developer.apple.com/documentation/swiftui/form （iOS 13.0+） |
| `DisclosureGroup` | SwiftUI | B | label + 可展开/收起的 content，可绑定外部展开状态 | `init(_:content:)`, `init(content:label:)`, `init(_:isExpanded:content:)` | 能（官方示例中可**嵌套** `DisclosureGroup`） | https://developer.apple.com/documentation/swiftui/disclosuregroup （iOS 14.0+） |
| **Card** | SwiftUI | B | SwiftUI **没有** `Card` 类型。卡片由 `GroupBox`/`List` 行/自定义 shape+背景组合；HIG 提供 "Boxes" 作为概念 | 无官方 Card API | 能（组合式） | https://developer.apple.com/design/human-interface-guidelines/boxes （HIG 概念页："By default, a box uses a visible border or background color to separate its contents"）。本文件**没有**核到 `GroupBox` 的 doc JSON，故不列该符号 |
| Toolbar（`.toolbar {}` 修饰符） | SwiftUI | B | 顶/底工具栏区域：标题、导航控件、动作项三类内容 | `.toolbar { ToolbarItem(placement:) }`, `.navigationTitle(_:)` | 能 | https://developer.apple.com/design/human-interface-guidelines/toolbars （HIG："A toolbar consists of one or more sets of controls arranged horizontally along the top or bottom edge of the view, grouped into logical sections."）。**注意**：这是 HIG 的设计描述，`.toolbar` 修饰符本身的 doc JSON 本文件未取 |
| 侧栏（sidebar）作为**区域**而非控件 | SwiftUI | B | 在 `NavigationSplitView` 里是具名 slot；在 `List` 里是 `.listStyle(.sidebar)`；在 `TabView` 里是 `sidebarAdaptable` 样式的产物 | 见上三行 | 能 | https://developer.apple.com/design/human-interface-guidelines/sidebars （HIG："A sidebar requires a large amount of vertical and horizontal space."） |

### 2.D AppKit（macOS 原生，用于"页面外框"对照）

| 名称 | 体系 | 类别(A/B) | 作用 | 关键参数 | 能否嵌套其它骨架控件 | 源码/文档位置 |
|---|---|---|---|---|---|---|
| `NSSplitViewController` | AppKit | B | 管理一组相邻子视图，并持有一个管理分隔条的 split view 对象 | 子项 `NSSplitViewItem`（含 `sidebarWithViewController:`）、分隔条行为 | 能 | https://developer.apple.com/documentation/appkit/nssplitviewcontroller （macOS 10.10+；abstract："An object that manages an array of adjacent child views, and has a split view object for managing dividers between those views."） |

---

## 3. 卡片类的三个必答问题（elevation / 可展开 / 分区）

| 体系 | elevation | 可展开 | header / media / actions 分区 | 出处 |
|---|---|---|---|---|
| MD3 `Card`（elevated） | **有**，默认 `elevation: 1.0`，`shadowColor = colorScheme.shadow`，`surfaceTintColor = Colors.transparent`（即 M3 不再用 tint 加深度） | 否 | 无固定分区；`child` 任意（官方"See also"指向 `ListTile` 用于行式卡片） | `material/card.dart:305, :313, :320`；分区线索 `:68-70` |
| MD3 `Card.filled` | **无**，`elevation: 0.0`，底色 `surfaceContainerHighest` | 否 | 同上 | `material/card.dart:340, :348` |
| MD3 `Card.outlined` | **无**，`elevation: 0.0`，底色 `surface` + `outlineVariant` 描边 | 否 | 同上 | `material/card.dart:375, :383, :392-394` |
| MD3 `ExpansionTile` | **无固定 elevation**；仅在给了 shape 时包一层 `Material(clipBehavior, color, shape)`，否则用 `DecoratedBox` + 分隔线（展开态上下各一条 `theme.dividerColor`） | **是**（`children`），`initiallyExpanded` 默认 false | 行头是 `ListTile`（leading/title/subtitle/trailing），无独立 media 区 | `material/expansion_tile.dart:724-750`（`Material` 包裹条件）、`:812-830`（shape 补间与分隔线） |
| MD3 `ExpansionPanelList` 的面板 | **有**，`elevation` 默认 **2**；内部用 `MergeableMaterial(elevation: …)` | **是**（`isExpanded`，radio 版互斥） | `headerBuilder` + `body` 两区；`canTapOnHeader` 决定行头是否可点 | `material/expansion_panel.dart:190-197, :195, :272-275, :484-487` |
| WinUI `Expander` | 无 elevation 属性语义；外观由主题资源（Fluent 材质/描边）提供 | **是**（`IsExpanded`，可向上/下） | `Header` + `Content` 两区，header 内可放任意内容（含图标） | https://learn.microsoft.com/en-us/windows/apps/design/controls/expander |
| WinUI `InfoBar` | 无；它是**占据布局空间**的内联条 | 否（可关闭） | `Title` / `Message` / `ActionButton` / 关闭按钮 | https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar |
| 移植 `fluent_ui.Card` | 无 elevation 字段；只有 `borderColor` + `borderRadius`(默认 4) + `backgroundColor` | 否 | 无分区，单一 `child` | `fluent_ui lib/src/controls/surfaces/card.dart:48-56` |
| SwiftUI | 无 Card 类型 | `DisclosureGroup` **是**（`isExpanded`） | `DisclosureGroup` 为 label + content 两区；HIG "Boxes" 描述的是"可见边框/背景 + 可选标题" | https://developer.apple.com/documentation/swiftui/disclosuregroup ；https://developer.apple.com/design/human-interface-guidelines/boxes |

---

## 4. 跨体系对照表（同一件事，三家分别叫什么）

"无对应"= 在该体系的官方文档/源码里**找不到**同一职责的控件，括号里写它用什么替代。

| # | 职责 | MD3 (Flutter) | WinUI 3 | SwiftUI / AppKit | 备注 |
|---|---|---|---|---|---|
| 1 | **页面外框（统一编排各区域）** | `Scaffold`（12 槽位固定枚举） | **无单一控件**：`NavigationView`(pane+header+content) / `SplitView` / `TabView` 各自成一个外框；`Page`+`Frame` 只管导航栈 | **无**页级外框控件：`NavigationSplitView` / `TabView` / `List` / `.safeAreaInset` 组合 | **这是三家最大的分工差异**，见 §5.1 |
| 2 | 顶部标题/工具栏区 | `AppBar`（`leading/title/actions/bottom`） | `NavigationView.Header`（固定 52px，Minimal 模式恒显示，可作"滚动裁剪点"）或 `TabStripHeader` | `.toolbar` / `.navigationTitle`（HIG "toolbars"）；无具名"AppBar"控件 | MD3 的 AppBar 是独立控件；WinUI 的 header 是 NavigationView 的一个属性；SwiftUI 是修饰符 |
| 3 | 竖向侧栏（图标或图标+文字） | `NavigationRail`（`labelType: none/selected/all`, `extended`） | `NavigationView.PaneDisplayMode = Left / LeftCompact`（`CompactPaneLength` 默认 48） | `NavigationSplitView.sidebar`；`List.listStyle(.sidebar)`；`TabView().tabViewStyle(.sidebarAdaptable)`（macOS 恒侧栏） | 三家的"侧栏"都不是同一个东西：MD3 是独立控件，WinUI 是同一控件的模式，SwiftUI 是同一控件的 slot/样式 |
| 4 | 抽屉式侧栏（开合覆盖内容） | `Drawer` / `NavigationDrawer` + `DrawerController`（`Scaffold.drawer`） | `SplitView(DisplayMode=Overlay)` 或 `NavigationView(PaneDisplayMode=LeftMinimal)` | 无对应（`NavigationSplitView` 的列在 compact 环境折叠为单栈，不是抽屉） | WinUI 官方把两者分得很清楚："If you'd like to build a navigation menu … use the NavigationView control instead"（split-view 页） |
| 5 | 底部导航栏 | `NavigationBar`（M3，默认高 80、elevation 3、≥2 目的地） | **无对应**（Windows 设计里底部导航不是标准模式；最接近的是 `TabView` 或顶部 `PaneDisplayMode=Top`） | `TabView` 默认底栏（iOS）；`sidebarAdaptable` 下 iPadOS 为顶栏、macOS/tvOS 为侧栏 | WinUI 一行是本表少见的"某一家完全没有" |
| 6 | 页签（平级页切换） | `TabBar` + `TabBarView`（`TabAlignment`, `isScrollable`） | `TabView` + `TabViewItem`（文档式：可新建/关闭/重排/拖到别的窗口） | `TabView` + `Tab`（iOS 18 起 `Tab` 为具名子项）；`TabSection` 表达二级 | MD3 的 tab 是**导航**；WinUI 的 `TabView` 官方定位是**文档**（"By default, TabView is configured to provide document tabs"）；静态页签官方反而建议用 `NavigationView` |
| 7 | 后退栈 | `Navigator` / `MaterialPageRoute`（不在本仓 checkout 的 `material/` 里，属于 `widgets/`+routing） | `Frame`（`CanGoBack`, `GoBack()`）+ 各页面自身的返回按钮 | `NavigationStack`（`path:` 绑定） | WinUI 的返回按钮本身在 `NavigationView` 里（"does not automatically handle backwards navigation"） |
| 8 | 两/三列主从结构 | 无对应（用 `Row` + 两个 body 手工拼；`Scaffold` 没有"列"概念） | 无对应（用 `Grid` 或 `NavigationView`+`Grid` 手工拼） | `NavigationSplitView`（2~3 列的**一等**结构）；AppKit `NSSplitViewController` | SwiftUI 是唯一把"两/三列"做成控件语义的 |
| 9 | 可展开的内容块 | `ExpansionTile`（带 shape/背景，行头为 `ListTile`）与 `ExpansionPanelList`（带 elevation=2，radio 互斥版） | `Expander`（header/content，展开推开不覆盖） | `DisclosureGroup`（label/content，可与 `isExpanded` 绑定） | 三家都有，但**默认值不同**：只有 MD3 的 panel 版默认带 elevation |
| 10 | 状态/通知条（内联占位） | `MaterialBanner`（经 `ScaffoldMessenger`，占 `Scaffold` 的 `materialBanner` 槽） | `InfoBar`（"take up space in your layout"） | 无对应（HIG 用 banner/列表行等模式） | WinUI 的 `InfoBar` 是内联占位；MD3 的 `SnackBar` 是浮层——两者**不是**同一语义 |
| 11 | 浮层通知 | `SnackBar`（`ScaffoldMessenger.showSnackBar`；`Scaffold` 有 `snackBar` 槽） | 无对应（`InfoBar` 不浮；官方明确不覆盖内容） | 无对应 | 语义相反的两条（"浮" vs "不浮"）是 §5 的关键缺口 |
| 12 | 底部面板 | `BottomSheet`（持久/模态，`useSafeArea` 默认 false） | 无对应（`SplitView` 的 pane 可与底部无关） | 无对应（`.sheet` 是模态呈现修饰符，非布局区域） | 只有 MD3 把"底部面板"做成布局槽位 |
| 13 | 抽屉式面板（覆盖内容） | `Drawer` + scrim（`Scaffold` 的 `drawer`/`endDrawer` 槽） | `SplitView(Overlay/CompactOverlay)` | 无对应 | |
| 14 | 卡片外壳 | `Card`（elevated/filled/outlined，三变体） | **无对应控件**（`Border`/`Grid` 组合；官方无 Card API，见 404 反证） | **无对应控件**（组合；HIG 有 "Boxes" 概念） | **只有 MD3 把 Card 做成控件** |
| 15 | 列表行 | `ListTile`（leading/title/subtitle/trailing） | `ListView`/`GridView` 的项模板（无具名行控件） | `List` + 行内容（无具名行控件） | MD3 的 `ListTile` 是"有语义分区的行" |
| 16 | 数据表 | `DataTable`（+`PaginatedDataTable`） | 无对应（官方在 list/grid 页指向 **Community Toolkit 的 `DataGrid`**，非 WinUI 自带） | `Table`（`TableColumn`/`TableRow`，`TableColumnAlignment`） | WinUI 的数据表不是框架自带：https://learn.microsoft.com/en-us/windows/apps/design/controls/listview-and-gridview 明确建议 "consider using a DataGrid control"（指向 communitytoolkit） |
| 17 | 分隔线 | `Divider` / `VerticalDivider`（`height`/`thickness`/`indent` 三个独立数） | `NavigationViewItemSeparator`（仅 pane 内）；通用分隔用 `Border`/`Rectangle` | `Divider()` 视图 | WinUI 没有通用 Divider 控件（本文件只核到 pane 内 separator 与 Border 方案） |
| 18 | 轮播/主视觉横滑 | `CarouselView`（`itemExtent`, `shrinkExtent`） | 无对应（`ListView`+横向 `StackPanel` 手工拼） | `ScrollView(.horizontal)` + `LazyHStack`，或 `TabView` + page 样式 | 只有 MD3 把轮播做成控件 |
| 19 | 纯排列：横/纵/叠 | `Row` / `Column` / `Stack` | `StackPanel` / `RelativePanel` / `Grid`（叠放靠 `Canvas` 或 `Grid` 同格） | `HStack` / `VStack` / `ZStack` | |
| 20 | 二维网格 | `GridView` / `Table` / `Wrap` | `Grid` / `VariableSizedWrapGrid` | `Grid`(+`GridRow`) / `LazyVGrid` | WinUI 的 `Grid` 是**布局面板**，SwiftUI 的 `Grid` 也是布局容器，MD3 的 `GridView` 是**滚动列表**——同名不同物 |
| 21 | 滚动视口 | `ListView` / `SingleChildScrollView` / `CustomScrollView` | `ScrollViewer` 与 `ScrollView` **两套并存** | `ScrollView`（不支持缩放） | MD3 没有"给任意 widget 加滚动"之外的第二套语义；WinUI 3 同时有两套 |
| 22 | 宽高比 | `AspectRatio` | 无对应面板（用 `Grid`+`Viewbox` 或手算；本文件未核到 AspectRatio 页） | `.aspectRatio(_:contentMode:)` | |
| 23 | 安全区 | `SafeArea` / `MediaQuery.padding` | XAML 有 `ApplicationView.VisibleBounds`/`SystemInsets` 类概念（本文件未逐字核对，见 §6 缺口） | `.safeAreaInset` / `.ignoresSafeArea` / `SafeAreaRegions`（`all`/`container`/`keyboard`）；HIG "layout" 要求 "respecting system-defined safe areas, margins, and guides" | 只有 SwiftUI 把"在安全区内插入一条 bar 并让主视图让位"做成修饰符 |
| 24 | 断点/自适应 | `LayoutBuilder` + `MediaQuery`（应用自己写断点） | `NavigationView` **内建**断点：`Auto` 下 ≥1008 展开、641–1007 紧凑、≤640 最小；可改 `CompactModeThresholdWidth`/`ExpandedModeThresholdWidth` | `NavigationSplitView` 的 compact 环境折叠；`sidebarAdaptable` 的平台自适应 | **WinUI 把断点做进了控件默认值**，另两家要靠应用层 |
| 25 | 页面内容边距 | 应用自选（`Padding`/`SafeArea`） | 官方**建议**：Minimal 模式 12px、其余 24px（`navigationview` 页 "We recommend 12px margins … 24px margins otherwise"） | HIG "layout"：遵循系统定义的安全区/边距/参考线；tvOS 有具体数值（上下 60pt、两侧 80pt） | 移植 `fluent_ui` 把这条建议升级成了 `ScaffoldPage` 的默认行为（`page.dart:16-24`），官方只当建议 |
| 26 | 页面级"栅格基准" | `Wrap`/`GridView` 的间距由应用定 | 官方要求：尺寸/边距/位置取 **4 epx 的整数倍**（缩放假板 100%–400%） | 无 global 栅格数值（HIG 只说对齐与缩进表达层级） | WinUI 是唯一有硬性 4px 网格规则的 |
| 27 | 标题栏（窗口级） | 无（交给平台） | 窗口标题栏不在 `NavigationView` 内，是 `AppWindowTitleBar`（本文件未逐字核对该页） | HIG "windows" | 移植 `fluent_ui.TitleBar`（`title_bar.dart:21`）是**移植独有**，官方对应物在窗口层 |

---

## 5. 一致性缺口（哪一家有、别人没有；以及"谁负责页面外框"）

### 5.1 谁负责页面外框 —— 三家分工不同（抽象方案的关键输入）

| | MD3 | WinUI 3 | SwiftUI |
|---|---|---|---|
| 外框是**一个控件**吗 | **是**：`Scaffold` 一个控件拥有全部槽位 | **不是**：`NavigationView` / `SplitView` / `TabView` 三种外框并存，选哪个由应用决定 | **不是**：没有页级外框类型；用 `NavigationSplitView`/`TabView`/`List`/`.safeAreaInset` 组装 |
| 槽位是**固定枚举**吗 | **是**：`_ScaffoldSlot` 12 值编译期固定（`scaffold.dart:64-77`） | 否：属性集合随控件而变（pane/header/content 在 `NavigationView`，pane/content 在 `SplitView`，tab strip + items 在 `TabView`） | 否：用结构（`VStack`）+ 修饰符（`.safeAreaInset`/`.toolbar`）表达 |
| 谁提供**设备内边距/键盘让位** | `Scaffold`：`resizeToAvoidBottomInset` 默认 true，自己读 `MediaQuery` 改 body 约束（`scaffold.dart:2776-2777`, `:3030-3031`） | 系统 + 窗口层（`AppWindow`/XAML 系统 inset），不在 `NavigationView` 里 | 环境值（safe area / keyboard region）：`SafeAreaRegions` 显式区分 `.container` 与 `.keyboard` |
| 谁决定**页面内容边距** | 应用；`Scaffold` 不替应用定（`persistentFooterAlignment` 等只管它自己的槽） | 官方**建议数值**；控件不强制 | HIG 要求遵循系统边距；控件不强制数值 |
| 谁决定**显示模式/断点** | 应用（`LayoutBuilder` + 自己拼 `NavigationRail`/`NavigationBar`/`Drawer`） | **控件内建**：`PaneDisplayMode=Auto` 的三档断点 + 可改阈值 | 平台+样式：`sidebarAdaptable` 让**同一份代码**在 iPadOS/macOS/iOS 变身 |
| 外框对内容的**裁剪/滚动责任** | `Scaffold` 只给 body 约束；滚动是 body 自己的事（`CustomScrollView` 等） | `NavigationView.Header` "acts as a scroll clipping point for the content area" —— **外框参与滚动语义** | 安全区被 `.safeAreaInset` 增大，滚动容器据此算内容区 —— **外框改变滚动容器的几何** |
| 外框能否**嵌套自己** | `ScaffoldMessenger` 文档明确讨论嵌套 Scaffold 的作用域（`scaffold.dart:93-100`） | `TabViewItem` 内容可按官方建议封装成 `Page`/`UserControl`，即外框内的"页" | `NavigationStack` 可嵌入 `NavigationSplitView` 的列 |

**可直接引用的分工差异（三条）**

1. **"外框"在 MD3 是一个类型，在 WinUI 是一族类型，在 SwiftUI 是一组组合。**
   证据：`_ScaffoldSlot`（`scaffold.dart:64-77`）；WinUI 的 split-view 页与 navigationview 页是两篇
   互斥的选择指引；SwiftUI 的 `NavigationSplitView` abstract 只说"presents views in two or three
   columns"，没有"页面"概念。
2. **只有 WinUI 把断点写进控件默认值**，且数值是 1008/641/640
   （`.../navigationview` "Adaptive behavior" 节）。MD3 与 SwiftUI 都把这交给应用或平台。
3. **只有 SwiftUI 有"插入一条 bar 并让主视图让位"的一等修饰符**（`.safeAreaInset`，
   abstract："Shows the specified content beside the modified view"，实现方式是"inset the modified
   view … with its safe area increased by the same amount"）。MD3 对应的做法是把 bar 放进
   `Scaffold` 的具名槽，由 `Scaffold` 的布局委托反算 body 约束（`scaffold.dart:1036-1111`）。

### 5.2 只有某一家有的控件（逐条，带出处）

| 控件 | 独有于 | 证据 | 另外两家怎么代替 |
|---|---|---|---|
| `Scaffold`（统一外框 + 固定槽位枚举） | MD3 | `material/scaffold.dart:1683`, 槽位 `:64-77` | WinUI 一族外框；SwiftUI 组合 |
| `BottomSheet`（持久+模态，作为布局槽） | MD3 | `material/bottom_sheet.dart:45-58`，`Scaffold.bottomSheet` | 无对应（`.sheet` 是模态呈现，不参与布局） |
| `CarouselView` | MD3 | `material/carousel.dart:132` | WinUI/SwiftUI 手工拼 |
| `ListTile`（有语义分区的行控件） | MD3 | `material/list_tile.dart:385` | WinUI/SwiftUI 用模板/普通行内容 |
| `DataTable`（框架自带） | MD3 | `material/data_table.dart:446` | WinUI 指向 Community Toolkit `DataGrid`；SwiftUI `Table` |
| `Card`（三变体、带 elevation 语义） | MD3 | `material/card.dart:73` + `:305/:340/:375` | WinUI/SwiftUI 无控件（组合） |
| `NavigationView`（应用级外框：pane+header+content 一体 + 内建断点） | WinUI 3 | `.../navigationview` | MD3 要 `Scaffold`+`NavigationRail`+`AppBar` 自己拼；SwiftUI 要 `NavigationSplitView` |
| `SplitView`（独立的抽屉面板控件） | WinUI 3 | `.../split-view` | MD3 `Drawer`；SwiftUI 无 |
| `TabView` 文档式页签（新建/关闭/重排/跨窗口拖动 + 键盘快捷） | WinUI 3 | `.../tab-view`（"letting a user rearrange, close, or open new tabs"；`Ctrl+T`/`Ctrl+W`/`Ctrl+Tab`/`Ctrl+F4`） | MD3 `TabBar` 只做切换；SwiftUI `TabView` 只做切换 |
| `InfoBar`（**不浮**的内联状态条，明确占布局空间） | WinUI 3 | `.../infobar` | MD3 `MaterialBanner`（占槽，但由 Messenger 驱动）/"SnackBar"（浮层，语义不同）；SwiftUI 无 |
| `NavigationSplitView`（2~3 列主从**控件**） | SwiftUI | `.../navigationsplitview` | MD3/WinUI 都只能手工拼 `Row`/`Grid` |
| `NavigationStack`（`path` 绑定的后退栈控件） | SwiftUI | `.../navigationstack` | WinUI `Frame`；MD3 `Navigator`（不在本仓 checkout） |
| `.safeAreaInset` / `.safeAreaBar` / `SafeAreaRegions`（`.container` vs `.keyboard` 分开） | SwiftUI | `.../safeareainset(...)`（iOS 15+）；`.../safeareabar(...)`（iOS 26+）；`.../safearearegions` | MD3 `SafeArea`+`MediaQuery`（`viewInsets` 与 `padding` 是两个字段）；WinUI 系统 inset |
| `@ViewBuilder` / `Layout` 协议（自定义容器一等公民） | SwiftUI | `.../viewbuilder`；`.../layout` | MD3 有 `CustomMultiChildLayout`（在未 checkout 的 `widgets/`）；WinUI 有 `Panel` 子类化（本文件未核） |
| `sidebarAdaptable`（同一控件按平台变侧栏/底栏） | SwiftUI | `.../sidebaradaptabletabviewstyle`（"macOS and tvOS always show a sidebar"） | WinUI 靠 `PaneDisplayMode`；MD3 靠应用断点 |
| `NSSplitViewController`（分隔条拖动、sidebar item） | AppKit | `.../appkit/nssplitviewcontroller` | SwiftUI `NavigationSplitView` 之下就是它 |

### 5.3 语义相反、不可合并的三对

| 对 | 一家 | 另一家 | 冲突点 |
|---|---|---|---|
| `SnackBar` vs `InfoBar` | MD3：`Scaffold` 的 `snackBar` 槽是**浮层**（有 scrim/浮动宽度 `snackBarWidth`） | WinUI：**"will not cover up other content or float on top of it"** | 一个占位、一个覆盖；合成一个"通知条"会错 |
| `TabView`（文档） vs `TabBar`（导航） | WinUI：默认是**文档页签**（可关、可重排、可拖出窗口） | MD3：tab 是**导航**，静态页签官方反而建议用 `NavigationView` | 同叫 tab，状态所有权不同（文档有"存在性"，导航没有） |
| `Card` vs `Box`/`ExpansionTile` | MD3：卡片是**控件**，有自己的 elevation 阶 | WinUI/SwiftUI：卡片是**组合**，没有 elevation 令牌 | 把"卡片"抽象成控件会强迫非 MD3 风格提供不存在的令牌 |

### 5.4 同名不同物（易混 3 例）

| 名字 | MD3 | WinUI 3 | SwiftUI |
|---|---|---|---|
| `Grid` | `GridView` = **滚动列表**（`gridDelegate`） | `Grid` = **布局面板**（`RowDefinitions`/`ColumnDefinitions`，直接子元素放同一格即叠放） | `Grid` = **布局容器**（`GridRow`，单元格级对齐） |
| `ScrollView` | 无此名（有 `SingleChildScrollView`/`ListView`/`CustomScrollView`） | `ScrollViewer` **与** `ScrollView` 两套并存 | `ScrollView`（不支持缩放） |
| `SplitView` | 无 | 独立控件（`DisplayMode` 四态） | 无（`NavigationSplitView` 是列结构，不是抽屉） |

---

## 6. 缺口与未证实项（诚实清单）

| # | 事项 | 状态 | 影响 |
|---|---|---|---|
| 1 | Flutter `src/widgets/`、`src/cupertino/` 未 checkout | **不可读**（`ls` 报 no such file） | Class A 的 `Row/Column/Stack/Padding/Spacer/ListView/GridView/Table/Wrap/AspectRatio/Align/SizedBox/LayoutBuilder/SafeArea/MediaQuery` **都没有本地 `path:line`**；本文件只给官方 API URL +（有的）`material/` 内调用点 |
| 2 | MD3 `Navigator`/`MaterialPageRoute` 不在 `material/` 内 | 未核 | 对照表第 7 行（后退栈）的 MD3 列只有类名，没有行号 |
| 3 | WinUI 的 `Card` 是否存在 | **官方 404 反证**（两处 URL 均 404） | 结论"WinUI 无 Card 控件"基于**文档不存在**，不是基于文档明文否认；若将来官方新增，此行作废 |
| 4 | `fluent_ui.CommandBar`、`BreadcrumbBar`、`TreeView` 的官方页 | **未核对**（只有移植源码行号） | §2.B 未列这三行；§B-FLU 已标明"未完全核对" |
| 5 | WinUI 安全区 / `AppWindowTitleBar` / 4epx 网格之外的具体数值 | 部分核对 | 安全区一行（对照表 23）写的是"类概念"，无逐字引用 |
| 6 | SwiftUI `.toolbar` 修饰符、`GroupBox`、可折叠 `Section` 的引入版本 | **只核到 HIG 与符号存在**，未取 doc JSON | 对照表第 2 行与卡片表的 SwiftUI 格标注了这一点 |
| 7 | SwiftUI `Grid` 之外的分组网格（`LazyVGrid`） | 未核 | 对照表第 20 行只写 `Grid` |
| 8 | MD3 `CarouselView` 的官方 M3 spec 页 | 未核 | 只引源码行号 |
| 9 | Apple HIG 具体页面的子路径（`tab-bars`/`toolbars`/`boxes`/`windows`/`lists-and-tables`/`scroll-views`/`sidebars`/`split-views`/`layout`） | 全部 200 | 但 `navigation-and-search/tab-bars` 这类旧路径 404；只用已验的扁平路径 |
| 10 | 本文所有 URL | 2026-10-27 全部 `HTTP 200`（WinUI 9 条 design 页 + 15 条 API 页 + acrylic；Apple 28 条） | 见 §7 核对命令 |

### 6.1 只有二手来源的行（移植 `fluent_ui`）

以下行的**参数列表**只来自移植源码（二手），官方文档只用于确认"该控件存在、职责一致"：

`fluent_ui NavigationView`（`view.dart:129-140`）、`NavigationPane`（`pane.dart:97-113`）、
`PaneItem*`（`pane_items.dart:127/670/718/789/830/1283`）、`TabView`（`tab_view.dart:66-86`）、
`Tab`（`tab.dart:200-215`）、`ScaffoldPage`（`page.dart:68-75`）、`PageHeader`（`page.dart:263`）、
`TitleBar`（`title_bar.dart:23-41`）、`Expander`（`expander.dart:97-120`）、`Card`（`card.dart:48-56`）、
`InfoBar`（`info_bar.dart:105-324`）、`HorizontalScrollView`（`horizontal_scroll_view.dart:14-30`）。

**风险**：移植是二手证据；参数名/默认值与 WinUI 官方**可能**不同（已能证实的差异已写进 §B-FLU）。
任何进入实现的结论必须回到官方页或 WinRT API 复核。

---

## 7. 覆盖度与核对方式

**计数（本文件表格行数）**

```
grep -c '^| ' docs/research/layout-inventory.md     # ≥ 60 required
```

明细：

| 分区 | 行数（约） | 内容 |
|---|---|---|
| §1 Class A | 14 (MD3) + 10 (WinUI) + 14 (SwiftUI) = 38 | 纯布局容器 |
| §2 Class B | 20 (MD3) + 12 (WinUI) + 11 (SwiftUI) + 1 (AppKit) = 44 | 页面骨架控件 |
| §3 卡片三问 | 9 | 每条含 elevation/可展开/分区 |
| §4 跨体系对照 | 27 | 同一职责三列并排 |
| §5.1 外框分工 | 6 行 × 差异维度 | 谁负责外框 |
| §5.2 独有控件 | 14 | 只某一家有 |
| §5.3/5.4 冲突与同名 | 3 + 3 | |

**URL 核对（2026-10-27 执行，全部返回 200）**

```bash
for u in https://learn.microsoft.com/en-us/windows/apps/design/{controls/navigationview,controls/tab-view,controls/split-view,controls/scroll-controls,controls/listview-and-gridview,controls/expander,controls/infobar,controls/items-repeater,layout/layouts-with-xaml,layout/screen-sizes-and-breakpoints-for-responsive-design} \
         https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.{grid,stackpanel,relativepanel,canvas,variablesizedwrapgrid,scrollviewer,scrollview,splitview,expander,infobar,tabview,tabviewitem,listview,gridview,page} \
         https://developer.apple.com/documentation/swiftui/{navigationsplitview,navigationstack,tabview,tab,scrollview,lazyvstack,grid,group,section,form,list,disclosuregroup,containerrelativeshape,hstack,vstack,zstack,spacer,layout,sidebaradaptabletabviewstyle,safearearegions,viewbuilder} \
         "https://developer.apple.com/documentation/swiftui/view/safeareainset(edge:alignment:spacing:content:)" \
         "https://developer.apple.com/documentation/swiftui/view/safeareabar(edge:alignment:spacing:content:)" \
         "https://developer.apple.com/documentation/swiftui/view/aspectratio(_:contentmode:)" \
         "https://developer.apple.com/documentation/swiftui/view/ignoressafearea(_:edges:)" \
         https://developer.apple.com/documentation/appkit/nssplitviewcontroller \
         https://developer.apple.com/design/human-interface-guidelines/{layout,split-views,sidebars,toolbars,tab-bars,boxes,windows,lists-and-tables,scroll-views} ; do
  printf '%s  %s\n' "$(curl -sSL -o /dev/null -w '%{http_code}' -m 25 -A 'Mozilla/5.0' "$u")" "$u"
done
```

**未查到的项（不是"没有"，是"本文件没核到"）**

- Flutter `Row`/`Column`/`Stack`/… 的本地源码行号（`widgets/` 未 checkout）——见 §6 #1。
- WinUI 的通用 `Divider`、`AspectRatio`、安全区 API 的逐字引用——见 §6 #3、#5。
- `fluent_ui` 的 `CommandBar`/`BreadcrumbBar`/`TreeView` 官方页——见 §6 #4。
- MD3 的 `CarouselView` 官方 M3 spec 页——见 §6 #8。
