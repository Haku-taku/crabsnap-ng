# 链式调用 / 修饰符 / 作用域 —— 六个框架的真实机制与其和 `Spec`/`Decl`/`Widget`/`Env` 的映射

Mined 2026-10-27 by subagent `layout-inventory` (shared task `task-12`). Read-only against every
source; the only file written is this one. **No code was modified.**

本文件**不重复** `docs/research/declaration-to-rendering.md`（那份覆盖「声明 → 渲染」的转换路径）。
这里只回答一个问题：`.scoped(..)` / `.child(..)` / `.color(..)` / `.padding(..)` 这类调用，
**在成熟框架里实际做了什么**，并映射到我们的 `Spec` / `Decl` / `Widget` / `Env`。

---

## 0. 结论先行

| 结论 | 证据 |
|---|---|
| **没有一家框架既用 (a) 又用 (b) 又用 (c) 来表达同一层**。每个框架选定一种*主*机制，其余机制出现在不同的层（作用域/环境、状态复用） | 见 §1–§6；总表在 §7 |
| **纯 builder 链（改字段、返回 `Self`）在四家里是完全相同的写法**：GPUI、egui、windows-reactor、Flutter 的 `Card`/`AppBar` | `styled.rs:44-47`、`frame.rs:247-252`、reactor 的 `LayoutControl`、`card.dart:240-268` |
| **"包一层"（b）在 Flutter 里是**默认**做法**，因为每个视觉属性都是一个独立 `Widget` | `card.dart:255`（`Padding`）、`material.dart:228`（`AnimatedDefaultTextStyle`）、`badge.dart:200-206`（`Container` 一次带 4 个属性） |
| **"沿树继承"（d）只有 Flutter / GPUI / egui / Qt / GTK 有，且各自用不同的键** | Flutter = 类型（`dependOnInheritedWidgetOfExactType`）；GPUI = `TypeId` + 专项 `text_style_stack`；egui = `Arc<Style>` 快照；Qt/GTK = CSS 选择器 |
| **真正的"分层默认值 + 局部覆盖"只有 GPUI 做成了通用设施** | `refineable/src/refineable.rs:29-132`（`Refineable` / `Cascade`） |
| **"立即执行"（c）只在 egui 是主机制**，且证据很硬：闭包在 `scope_dyn` 内被**同步调用**，返回值即结果 | `ui.rs:2211`（`let ret = add_contents(&mut child_ui);`） |

---

## 1. Flutter

**证据类型说明**：本地 checkout 只有 `material/`、`painting/`、`animation/`；
`packages/flutter/lib/src/widgets/` 与 `cupertino/` **不存在**（`ls` → *No such file or directory*）。
所以 `Padding` / `Container` / `InheritedWidget` 的**实现**拿不到；下文对它们只给
「`material/` 内的真实用法（行号）」+「api.flutter.dev URL」，并在行尾标注来源类型。

### 1.1 链式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| `Widget` 子类构造（如 `Card(elevation: 8)`） | **(a) 构造描述数据** — `Widget` 是不可变描述，真正布局在 `RenderObject` | 类声明 `card.dart:73`；`elevation` 字段 `card.dart:185`（只存值，不做任何事） |
| `Padding(padding:, child:)` | **(b) 包一层** — 把子项包进一个新 widget | 用法 `card.dart:255-257`（本地）；实现 `无本地源码`；https://api.flutter.dev/flutter/widgets/Padding-class.html |
| `Container(padding:, color:, decoration:, alignment:, child:)` | **(b) 包多层** — 一个 `Container` 等于若干个嵌套 widget 的组合；实际 usage 里一次带 4~5 个属性 | `badge.dart:200-206`（`clipBehavior`/`decoration`/`padding`/`alignment`/`child`）；`autocomplete.dart:295-299`（`color`/`padding`/`child`）。实现 `无本地源码`；https://api.flutter.dev/flutter/widgets/Container-class.html |
| `Material(color:, elevation:, shape:, child:)` | **(b) + (d)** —— 自己包 `AnimatedDefaultTextStyle` 给子树设文字样式（b），同时 `Theme.of(context)` 读环境（d） | `material.dart:183`（类）；`:228-232`（包 `AnimatedDefaultTextStyle`）；`:184`（`Theme.of(context)`） |
| `Card` 三级回退：`widget → cardTheme → defaults` | **(a) + 读取层** — 在 `build` 里做优先级解析，不包额外层 | `card.dart:240-268`；具体三处：`margin ?? cardTheme.margin ?? defaults.margin!`（`:256`）、`color ?? …`（`:259`）、`elevation ?? …`（`:263`） |
| `ScaffoldMessengerState.showSnackBar(snackBar)` | **(c) 立即执行的副作用** — 直接 `setState` 改内部列表并动画前进 | `scaffold.dart:314`（签名）、`:345-350`（`setState(() { _snackBars.addLast(controller); })`）、`:331-333`（`_snackBarController!.forward()`） |
| `Theme(data:, child:)` | **(b) 外观 + (d) 实质** — 形式上包一层，但被包的那层是 `InheritedWidget`，子项靠类型查找取值 | `theme.dart:214-223`（`build` 返回 `_InheritedTheme`）；`theme.dart:234-246`（`_InheritedTheme`） |

### 1.2 有没有作用域/环境继承；类型怎么查

**有**，且**按类型精确查找**（不是 key enum、不是 downcast 链）：

- `Theme.of(context)` 的实现就是 `context.dependOnInheritedWidgetOfExactType<_InheritedTheme>()`
  —— `theme.dart:136-139`（`Theme.of` 起始 `:136`）。
- 全 `material/` 里对 `dependOnInheritedWidgetOfExactType<...>` / `getInheritedWidgetOfExactType<...>`
  的调用按被查类型统计（`grep -rho` 后 `sort | uniq -c`）：
  `_InheritedTheme` 3、`FlexibleSpaceBarSettings` 3、`ProgressIndicatorTheme` 2、
  `_ScaffoldScope` 2、`_ScaffoldMessengerScope` 2、`InheritedCupertinoTheme` 2、`_FocusedDate` 2，
  另有 `TabBarTheme`、`SliderTheme`、`TooltipTheme`、`SwitchTheme`、`SnackBarTheme`、`TimePickerTheme`、
  `TextSelectionTheme`、`TextButtonTheme`、`ToggleButtonsTheme`、`_MenuAnchorScope`、
  `_TooltipVisibilityScope`、`_TabControllerScope` 各 1。
  **→ 每个可覆盖的"主题"是一个独立类型**，不是一张 key→value 表。
- `_InheritedTheme.updateShouldNotify` 用值相等判断是否通知子树 —— `theme.dart:245`。
- `_InheritedTheme.wrap(context, child)` 返回 `Theme(data: theme.data, child: child)`
  —— `theme.dart:239-242`，这是 Flutter 用来把捕获的主题**重放**到别的子树（如路由）里的手段。

### 1.3 有没有「实例默认值 + 局部覆盖」的三层优先级

**有，而且是逐属性的 `??` 链**，不是通用机制：`widget 参数 ?? ThemeData 字段 ?? M3 默认值`。
`card.dart:256/259/263` 三行是同一形状的三次重复；默认值来自生成块 `_CardDefaultsM3`
（`card.dart:293` 起，`BEGIN GENERATED TOKEN PROPERTIES - Card`）。

### 1.4 映射

| 我们的概念 | Flutter 里对应 |
|---|---|
| `Decl`（不可变描述树） | `Widget` 树（不可变，每帧重建）。真正的持久层是 `Element` 树 —— 见 `declaration-to-rendering.md` §2.4 |
| `Widget`（行为/渲染单元） | `Widget` + 它的 `createElement` / `createRenderObject`；`RenderObject` 才是布局绘制 |
| `Env`（作用域） | `InheritedWidget` 子树 + `dependOnInheritedWidgetOfExactType<T>()`（类型键） |
| `Spec`（风格默认值） | `_XxxDefaultsM3` 生成类（`card.dart:293+`）+ `ThemeData` 字段 |

---

## 2. GPUI

源码根：`/home/ollie/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/crates/gpui/src/`，
`Refineable` 机制在 `crates/refineable/src/refineable.rs`。

### 2.1 链式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| `.bg(..)` / `.p(..)` / `.px(..)` / `.text_color(..)` / `.flex()` | **(a) 改结构体字段** — 每个方法都是 `self.style().<字段> = Some(..); self` | trait `styled.rs:22`；`fn style(&mut self) -> &mut StyleRefinement` `styled.rs:24`；`.flex()` `styled.rs:44-47`；`.text_color()` `styled.rs:514-517`；`.font_weight()` `styled.rs:522-525` |
| `.text_color()` 为什么"级联" | 它写进 `style().text.color`，而 `text` 字段是 `TextStyleRefinement`（`#[refineable]`），**在 prepaint 时被压进 window 的样式栈** | 文档注释 "This value cascades to its child elements." `styled.rs:511-512`；字段 `style.rs:293-296`；压栈 `window.rs:4128-4138` |
| `.child(..)` / `.children(..)` | **(a) 改结构体字段** — `ParentElement::child` 只是 `extend(once(child))` 再返回 `self`；`Div::extend` 把元素推进 `children: SmallVec<…>` | `element.rs:189-210`（trait + 默认实现）；`elements/div.rs:1933-1941`（`impl ParentElement for Div`）；字段 `elements/div.rs:1857` |
| `div()` 工厂 | **(a) 构造描述数据** — 只是 `Div { interactivity: Interactivity::new(), … }` | `elements/div.rs:1844-1853` |
| `.hover(f)` / `.group_hover(..)` | **(a) 存一个 refinement 闭包**，不立即执行；在 prepaint 的样式计算里按状态被 `refine` 进去 | 字段 `elements/div.rs:2204-2208`（`hover_style`/`group_hover_style` 等 `Option<Box<StyleRefinement>>`）；应用 `elements/div.rs:3496-3525` |

**结论：GPUI 的链式调用没有一层"包住子项的装饰器"。** 它把配置写进**同一个元素**的
`StyleRefinement`，在 prepaint 时合成为最终 `Style`。唯一"包一层"的是 trait 的 `Style`→
`StyleRefinement` 派生关系，不是运行时结构。

### 2.2 作用域/环境继承；类型怎么查

**有，分三种，键都是 `TypeId` 或专用栈**：

| 机制 | 查找方式 | 证据 |
|---|---|---|
| 文字样式级联 | **专用栈** `text_style_stack: Vec<TextStyleRefinement>`，父亲压栈→子渲染→弹栈；读取时从头 `refine` 到底 | 字段 `window.rs:1036`、`:1246`；压/弹 `window.rs:4134-4136`；合成 `window.rs:2478-2484`（`pub fn text_style(&self) -> TextStyle`：`let mut style = TextStyle::default(); for refinement in &self.text_style_stack { style.refine(refinement); }`） |
| 全局值（主题、外观） | **`TypeId` → `Box<dyn Any>` + `downcast_ref`** | `globals_by_type: TypeIdHashMap<Box<dyn Any>>` `app.rs:907`；`global::<G>()` `app.rs:2522-2527`（`self.globals_by_type.get(&TypeId::of::<G>()).map(|any_state| any_state.downcast_ref::<G>().unwrap())`）；`set_global` `app.rs:2560-2564` |
| marker trait 把键限制到白名单类型 | `pub trait Global: 'static {}`（**空 trait**），`ReadGlobal`/`UpdateGlobal` 对 `T: Global` 做 blanket impl | `global.rs:21-24`（空 trait）、`global.rs:36-40`（`ReadGlobal` blanket）、`global.rs:55-60`（`UpdateGlobal` blanket） |
| 主题外观 | `GlobalTheme` 是 `Global`，`Appearance{Light,Dark}` 挂在 `Theme` 上 | `theme/src/theme.rs:82-88`（`enum Appearance`）、`:237-247`（`Theme{appearance, styles}`）、`:322-326`（`pub struct GlobalTheme{..}; impl Global for GlobalTheme {}`） |

### 2.3 有没有三层优先级

**有，而且是全部六家里唯一把它做成通用设施的**：`Refineable` + `Cascade`。

- 派生宏为 `struct Style` 生成 `StyleRefinement`（每个普通字段包 `Option<T>`，`#[refineable]` 字段
  用嵌套 refinement 类型）—— 文档 `refineable/src/refineable.rs:11-19`；属性 `:27-28`。
- `fn refine(&mut self, refinement: &Self::Refinement)`：只应用非空值，嵌套字段递归
  —— `refineable/src/refineable.rs:29-38`。
- `fn refined(self, …) -> Self` / `fn from_cascade(&Cascade<Self>) -> Self` —— `:40-50`。
- `pub struct Cascade<S>(Vec<Option<S::Refinement>>)`，**"later entries take precedence"**，
  slot 0 恒为 base —— 文档 `:71-78`；`default()` 用 `vec![Some(Default::default())]` `:82-86`；
  `merged()` 依次 `merged.refine(refinement)` `:120-130`。
- 运行时把它用在哪：`compute_style_internal` 先 `let mut style = Style::default(); style.refine(&self.base_style);`
  再按 focus / focus-visible / group-hover / hover 顺序继续 `refine`
  —— `elements/div.rs:3465-3525`（起始两行 `:3472-3473`）。

**→ GPUI 的 `base_style`（实例） + 状态样式（hover/focus） + `Style::default()`（默认值）构成三层，
且层与层之间是同一个 `refine` 算子。**

### 2.4 Element → Scene 的转换点、`ElementId` 复用

- 单帧入口：`Window::draw(&mut self, cx: &mut App) -> ArenaClearNeeded` —— `window.rs:3441`。
- 每个元素由 `Element::request_layout` → `prepaint` → `paint` 三段推进；`Div` 的三段在
  `elements/div.rs:1993`（request_layout）/ `:2013`（prepaint）。
- 元素状态复用：`Window::with_optional_element_state<S,R>(global_id, f)` —— 没有 id 时直接执行 lambda、
  不给持久状态（`window.rs:4474-4494`）。
- id 身份：`GlobalElementId { ids: Arc<[ElementId]>, hash: u64 }`，路径哈希逐层折进；
  文档明说"Every element with an id is looked up in the element-state maps of two frames"
  —— `element.rs:212-224`；`extend_path_hash` `:231-238`。

### 2.5 映射

| 我们的概念 | GPUI 里对应 |
|---|---|
| `Decl` | 每帧构造的 `Element` 树（`Div`/`AnyElement`），`IntoElement`/`RenderOnce` 产出 → `element.rs:143-158`、`:170-178` |
| `Widget`（行为单元） | `impl Element` 的类型（`Div`）；有状态行为的容器是 `Render` / `Entity`（`element.rs:160-167`） |
| `Env` | ① `Global` + `TypeId` 查找（主题）；② `with_text_style` 压栈的 `text_style_stack`（文字样式级联） |
| `Spec`（风格默认值） | `Style::default()` + 其 `StyleRefinement`；级联容器 `Cascade<S>` |

---

## 3. egui

源码根：`/home/ollie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src/`。

### 3.1 链式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| `ui.horizontal(\|ui\| ..)` / `ui.vertical(\|ui\| ..)` | **(c) 立即执行** — 闭包被同步调用；子 `Ui` 是当场创建的 | `ui.rs:2315-2317`（`horizontal` → `horizontal_with_main_wrap_dyn`）；`:2371-2389`（构造 layout 后调 `allocate_ui_with_layout_dyn`）；`:1331-1348`（`scope_dyn(..)`） |
| 闭包立即执行的**决定性一行** | `let ret = add_contents(&mut child_ui);` | `ui.rs:2211`（在 `scope_dyn` 内，`ui.rs:2203-2215`） |
| `ui.add(widget)` | **(c) 立即执行** — 直接 `widget.ui(self)`，返回 `Response` | `ui.rs:1521-1523`（`pub fn add(&mut self, widget: impl Widget) -> Response { widget.ui(self) }`） |
| `Widget` 是什么 | **不是描述数据，是"一次绘制动作"** — `fn ui(self, ui: &mut Ui) -> Response`；文档明说 widgets 是 builder 且不持有状态 | trait `widgets/mod.rs:63-67`；文档 `widgets/mod.rs:55-61`（"the widgets … are builders, and not objects that hold state"）；lib.rs 摘要 `lib.rs:264-267` |
| `Frame::none().fill(..).stroke(..)` | **(a) 构造配置**（builder 改字段，`containers/frame.rs:247-269`），但**必须**再调用 `show/ui` 才发生绘制 —— 即 **(a) 然后 (c)** | struct `containers/frame.rs:96-116`；builder `:247-269`；`show` `:404-406` → `show_dyn` `:411-418`；`begin` 才真正分配与记录 shape `:378-399` |
| `Frame::begin` 里的副作用 | 立刻向 painter **追加一个占位 shape**，稍后回填 | `containers/frame.rs:379`（`let where_to_put_background = ui.painter().add(Shape::Noop);`） |

### 3.2 作用域/环境继承；类型怎么查

**有，但键是"值快照"不是类型**：每个 `Ui` 持有一个 `Arc<Style>`。

- 字段 `style: Arc<Style>` —— `ui.rs:66`。
- 子 `Ui` **默认克隆父的 `Arc`**：`let style = style.unwrap_or_else(|| Arc::clone(&self.style));`
  —— `ui.rs:237`（在 `new_child` 内）。
- 顶层 `Ui` 从 `Context` 取全局样式 —— `ui.rs:136`（`let style = style.unwrap_or_else(|| ctx.global_style());`）。
- `Ui::style_mut()` 是 **copy-on-write**：`Arc::make_mut(&mut self.style)`
  —— `ui.rs:380-382`。→ **改样式天然只在当前 `Ui` 及其后续子项生效**，兄弟和父不受影响。
- `Ui::set_style` / `reset_style` 的文档把作用域说得很直白：**"Changes apply to this [`Ui`] and its
  subsequent children."** —— `ui.rs:384-387`；`reset_style` 回落 `ctx().global_style()` `:389-391`。
- 全局层：`Context::global_style()` / `global_style_mut()` / `set_global_style()` /
  `all_styles_mut()`（同时改 dark+light）—— `context.rs:2174-2240`（`global_style` `:2174-2177`、
  `global_style_mut` `:2189`、`all_styles_mut` `:2213`）。

### 3.3 有没有三层优先级

**有，但是"三层值"而不是"三层 refine"**：

1. `UiBuilder::style`（若显式给）—— `ui_builder.rs:28`、`:155`
2. 父 `Ui` 的 `Arc<Style>`（克隆）—— `ui.rs:237`
3. `Context` 的全局样式（暗/亮各一份，`style_of(Theme)`）—— `context.rs:2221-2230`

层与层之间是**整份 `Style` 替换**（`Arc::clone`），不是逐字段合并。要"只改一个字段"必须
`style_mut()` 拿到 `&mut Style` 手动改。**这是 egui 与 GPUI 的关键差异：egui 没有 partial refinement
算子，GPUI 有。**

### 3.4 映射

| 我们的概念 | egui 里对应 |
|---|---|
| `Decl` | **无对应**。egui 没有描述树：`Ui` 在闭包执行中边分配边绘制（`ui.rs:2211`） |
| `Widget` | `Widget` trait = 一次 `ui(&mut Ui) -> Response` 动作（`widgets/mod.rs:63-67`） |
| `Env` | `Ui.style: Arc<Style>` 的继承链（`ui.rs:66`、`:237`）+ `Context` 的全局样式（`context.rs:2174`） |
| `Spec`（风格默认值） | `Context` 的 `dark_style`/`light_style`（`context.rs:2213-2230`） |

---

## 4. Qt（QML `anchors`/`parent`、QtWidgets 样式表作用域）

本地无 Qt 源码（只有 `/usr/include/qt6/QtWidgets/{qlayoutitem.h,qsizepolicy.h}` 头文件，未用于本节结论）。
以下全部来自官方文档。

### 4.1 链式/属性式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| QML `anchors.left: other.right` 等 | **(a) 描述关系** — 不是包一层，而是把"相对谁/哪条边"记在 `Item` 的锚点属性上，由布局器求解 | "Anchors provide a way to position an item by specifying its relationship with other items." https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors-prop |
| `anchors` 的**约束** | **只能锚到兄弟或父** —— 这是一条硬规则，不是建议 | 同上页面原文："Note: You can only anchor an item to siblings or a parent." |
| `parent` 属性 / 对象树 | **(a) + 所有权**：对象树决定可视父子与内存归属 | https://doc.qt.io/qt-6/objecttrees.html |
| `QWidget::setStyleSheet(..)` | **(a) 存一份文本规则**；匹配与层叠发生在样式系统里，不是立即执行 | https://doc.qt.io/qt-6/qwidget.html#styleSheet-prop |
| `QApplication::setStyleSheet` vs `QWidget::setStyleSheet` | **(d) 作用域** —— 应用级 vs "某个 widget 及其子项" | https://doc.qt.io/qt-6/stylesheet.html §Overview："Styles sheets are textual specifications that can be set on the whole application using QApplication::setStyleSheet() or on a specific widget (and its children) using QWidget::setStyleSheet()." |

### 4.2 作用域/继承；怎么查

**有，用的是 CSS 选择器，不是类型 id**：

- 级联原文："If several style sheets are set at different levels, Qt derives the effective style sheet
  from all of those that are set. This is called cascading."
  —— https://doc.qt.io/qt-6/stylesheet.html
- 选择器全集（类型/属性/类/ID/后代 `E F`/子 `E > F`）：见
  https://doc.qt.io/qt-6/stylesheet-syntax.html §Selector Types。
  - **Type Selector `QPushButton` 匹配该类和其子类**（原文 "Matches instances of QPushButton and of its subclasses"）。
  - **Class Selector `.QPushButton` 只匹配自身、不匹配子类**（原文 "but not of its subclasses"）——
    这是 Qt 与直觉不同的地方。
- 优先级：样式表**优先于** `QWidget::setFont` / `QTreeWidgetItem::setBackground` 这类 API
  —— https://doc.qt.io/qt-6/stylesheet.html 的 Note。
- 运行时开销的官方定性：样式表是"for prototyping and for small, local adjustments"，
  生产外观应实现 `QStyle`；且"Changing the style sheet at runtime can lead to inconsistent results …
  Don't use this function to switch themes at runtime."
  —— https://doc.qt.io/qt-6/stylesheet.html

### 4.3 有没有三层优先级

**有**，形状是：`应用级样式表 → widget 级样式表 → QStyle（原生外观）`。
原文："Style sheets are applied on top of the current widget style" —— https://doc.qt.io/qt-6/stylesheet.html。
但**层内**的先后由 CSS 选择器特异性决定，不是简单的"子覆盖父"。

### 4.4 映射

| 我们的概念 | Qt 里对应 |
|---|---|
| `Decl` | QML 对象树 / `QWidget` 对象树（`objecttrees.html`）。注意它是**可变、持久**的，不像 Flutter `Widget` |
| `Widget` | `QWidget` 实例（有身份、有父、可被 selector 命中） |
| `Env` | 样式表作用域（应用级 / widget 及其子项），键是 CSS 选择器 |
| `Spec` | `QStyle` 派生类（官方推荐的生产级"默认值"载体） |

---

## 5. GTK 4

本地无 GTK 源码（`/usr/include/gtk-4.0/gtk/gtklayoutmanager.h` 头文件未用于本节结论）。以下来自官方文档。

### 5.1 链式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| `gtk_box_append(box, child)` | **(c) 立即执行的副作用** — 直接改容器的子列表（"Use repeated calls to gtk_box_append() to pack widgets into a GtkBox from start to end."） | https://docs.gtk.org/gtk4/class.Box.html |
| `gtk_box_set_spacing` / `set_homogeneous` | **(c) 立即改属性**（与 Flutter/GPUI 的"构造时传参"不同，GTK 是 setter 风格） | 同上 |
| `GtkBox` 的 CSS 节点 | **一个 widget 一个 CSS 节点**（"GtkBox uses a single CSS node with name box."） | 同上 |
| `gtk_widget_add_css_class` / `Gtk.Widget:css-classes` | **(c) 立即改属性**，供 CSS 选择器匹配 | https://docs.gtk.org/gtk4/class.Widget.html（实例方法 `add_css_class`；属性 `Gtk.Widget:css-classes`、`Gtk.Widget:css-name`） |

### 5.2 作用域/继承；怎么查

**有**，键是 **CSS 节点树 + 选择器**，且 GTK 明确说它**模仿 Web CSS 而不是另创一套**：

- "GTK applies the style information found in style sheets by matching the selectors against a tree of
  nodes. Each node in the tree has a name, a state and possibly style classes. … Every widget has one or
  more of these CSS nodes, and determines their name, state, style classes and how they are laid out as
  children and siblings in the overall node tree."
  —— https://docs.gtk.org/gtk4/css-overview.html §CSS nodes
- 选择器表（`*`、`E`、`E.class`、`E#id`、`:nth-child`、`E F`、`E > F`、`E ~ F`、`E + F` 等）
  —— 同页 §Selectors
- **状态也是选择器**：`E:hover`/`E:active`/`E:focus` 对应 `GTK_STATE_FLAG_*`；两个非 Web 语义要留意：
  `E:focus-within` = "Set on all ancestors of the focus widget, unlike CSS"、
  `E:focus-visible` = "Set on focus widget and all ancestors, unlike CSS"
  —— 同页选择器表
- **属性继承**：`inherit` / `initial` / `unset` 关键字"with the same meaning as defined in the CSS Cascading
  and Inheritance spec"
  —— https://docs.gtk.org/gtk4/css-properties.html §Basic types
- 主题/应用的优先级是**数字优先级常量**：`GTK_STYLE_PROVIDER_PRIORITY_FALLBACK` < 应用自定
  < `GTK_STYLE_PROVIDER_PRIORITY_APPLICATION` < `GTK_STYLE_PROVIDER_PRIORITY_USER`（用户
  `XDG_CONFIG_HOME/gtk-4.0/gtk.css` 最高）
  —— https://docs.gtk.org/gtk4/class.StyleContext.html §Custom styling in UI libraries and applications
- `GtkStyleContext` **自 4.10 起废弃**，"The relevant API has been moved to GtkWidget where applicable"
  —— 同页顶部 `deprecated: 4.10`

### 5.3 有没有三层优先级

**有，但是"provider 优先级"而不是"实例/主题/默认"三层**：fallback → application → user，
外加每个 widget 的 `css-classes` 提供局部选择。**没有**类似 Flutter 的"widget 参数优先于主题"那种
逐属性三元 `??` 链 —— 在 GTK 里"局部覆盖"靠**加 CSS class + 更具体的选择器**。

### 5.4 映射

| 我们的概念 | GTK 里对应 |
|---|---|
| `Decl` | **无对应**：GTK 是保留模式的 widget 树（`append` 当场生效） |
| `Widget` | `GtkWidget` 实例（有 CSS 节点、有 css-classes） |
| `Env` | CSS 节点树 + provider 优先级（`GtkStyleProvider`） |
| `Spec` | 主题 CSS（`GtkCssProvider`），按优先级参与级联 |

---

## 6. windows-reactor（microsoft/windows-rs）

**有公开资料**：crates.io `windows-reactor` 0.100.0（2026-09-03），仓库 `https://github.com/microsoft/windows-rs`，
指南 `docs/reactor.md`，API 索引 `https://docs.rs/windows-reactor/latest/windows_reactor/`。
本地**没有**该仓库源码（未在 `/home/ollie/git/` 与 cargo registry 中找到），所以本节全部是**官方文档**级证据，
没有 `path:line`。这是本文件里唯一"有文档但无源码"的一节。

### 6.1 链式调用实际做什么

| API | 机制 | 证据 |
|---|---|---|
| `StackPanel::new().spacing(8.0).children((..))` | **(a) 构造描述数据** — `children` 返回 `View`（不是 `Self`）：`fn children(self, children: impl IntoViews) -> View` | https://docs.rs/windows-reactor/latest/windows_reactor/trait.ChildrenControl.html |
| `.width(..)` / `.margin(..)` / `.opacity(..)` / `.horizontal_alignment(..)` | **(a) 改字段** — `LayoutControl` 的默认方法，`fn width(self, value: impl Into<Option<f64>>) -> Self`；对**50+ 个原生控件**逐个 impl | https://docs.rs/windows-reactor/latest/windows_reactor/trait.LayoutControl.html |
| 为什么是"改字段"而不是"包一层" | 该 trait 的说明是 **"Applies layout, opacity, margin, and exit-transition properties to native controls"** —— 属性加到**这个控件本身**，不产生包裹元素 | 同上，Expand description |
| 未设置的属性怎么处理 | 文档原句：**"Passing `None` to an optional property leaves it inherited or unset."** —— 即 `None` = 让给继承/默认，不是"设成无" | 同上 |
| `.spacing(8.0)` 这类 | **(a)**：值存在 builder 上，`.into()` 或 `children(..)` 时才成为 `View`/`Element` | 指南 §Layout：`vstack((..)).spacing(12.0).into()`（https://github.com/microsoft/windows-rs/blob/master/docs/reactor.md） |
| `.provide(&THEME, theme)` | **(d) 环境注入 + (a)** — 在 view 树上标一个 provider，子树用 `cx.use_context(&CTX)` 读 | 指南 §Context（Dependency Injection）：`vstack((..)).provide(&THEME, theme)`；hook 表 `cx.use_context(&CTX)` |
| `context.message(Msg)` / `context.forward()` | **(a) 包一个回调值**，不是立即派发 | 指南 §Components / readme 计数器示例 |

### 6.2 作用域/继承；怎么查

**有，键是"带自己身份的 typed context 通道"**（不是 `TypeId`，也不是 CSS 选择器）：

- `pub struct Context<T> { /* private fields */ }`，文档：**"A typed context channel with a fallback value.
  Each value created by `new` has its own identity, even when defaults are equal."**
  —— https://docs.rs/windows-reactor/latest/windows_reactor/struct.Context.html
- `Context::new(default)`："Creates a context that returns `default` when no ancestor provides a value."
  —— 同页
- 读取：`cx.use_context(&THEME)`；写入：祖先的 `.provide(&THEME, value)` —— 指南 §Context。
- 另有系统级订阅 hook：`cx.use_color_scheme()`、`cx.use_dpi()`、`cx.use_inner_size()`
  —— 指南 §Hooks 表。

**关键差异**：`Context<T>` 的身份是**实例**而不是类型 —— 所以同一个 `T` 可以开多个互不干扰的通道
（官方原话 "its own identity, even when defaults are equal"）。这与 Flutter（类型键）和 GPUI（`TypeId` 键）
都不同。

### 6.3 有没有三层优先级

**文档可确认两层，第三层未在文档中出现**：

1. 祖先 `.provide(..)` 的值
2. `Context::new(default)` 的 fallback（"returns `default` when no ancestor provides a value"）
3. （是否存在"风格级默认值"这一层 —— **未查到**，文档未描述）

对布局属性另有一条可确认规则：`None` = "leaves it inherited or unset"（`LayoutControl` 文档），
即**属性级的三态**（设值 / 显式 None / 不传），而不是数值级的三层默认。

### 6.4 映射

| 我们的概念 | windows-reactor 里对应 |
|---|---|
| `Decl` | **`View`**（"A declarative Reactor subtree"，docs.rs 索引）+ reconciler 对比新旧 `View` 后改原生树 |
| `Widget` | 原生 WinUI 控件（`StackPanel`/`Button`/`NavigationView`… 55+），由 builder 包一层 Rust 类型 |
| `Env` | `Context<T>` 通道 + `.provide(..)`（身份 = 通道实例，不是类型） |
| `Spec` | **未查到**（文档描述的是 `ThemeRef` brush 绑定与运行时明暗切换，没有"风格参数包"概念） |

---

## 7. 总表

### 7.1 机制对照（a=描述数据 / b=包一层 / c=立即执行 / d=环境继承）

| 框架 | `.child(...)` | 样式链（`.color`/`.padding`/`.bg`） | 作用域/环境 | 默认值覆盖 | 键怎么查 | 出处 |
|---|---|---|---|---|---|---|
| **Flutter** | b（`Padding`/`Container` 各包一层） | b（每属性一个新 widget） | d（`InheritedWidget` 子树） | 逐属性 `a ?? theme ?? defaults` | **类型**：`dependOnInheritedWidgetOfExactType<T>` | `card.dart:255`、`theme.dart:136`、`card.dart:256/259/263` |
| **GPUI** | a（推进 `SmallVec<AnyElement>`） | a（写 `StyleRefinement` 字段） | d（`text_style_stack` + `Global`/`TypeId`） | **通用 refine 级联**（`base_style`>`Style::default()`，加状态样式） | **`TypeId`**（`globals_by_type`）/ 专用栈 | `elements/div.rs:1933`、`styled.rs:44`、`window.rs:2478`、`app.rs:2522`、`refineable.rs:29-130` |
| **egui** | —（无子项概念，闭包内继续画） | a 然后 c（`Frame` 构配置，`show` 才画） | d（`Arc<Style>` 逐 `Ui` 继承，COW） | **整份 `Style` 替换**（3 层值，无 partial merge） | 无键：值快照沿 `Ui` 链 | `ui.rs:237`、`ui.rs:380`、`frame.rs:247/404/411` |
| **Qt** | a（`parent`/对象树） | a（`anchors`/样式表文本） | d（应用级 vs widget 及其子项，**CSS 选择器**） | 应用表→widget 表→`QStyle` | **CSS 选择器**（Type 含子类 / Class 不含子类） | stylesheet.html、stylesheet-syntax.html、qml-qtquick-item.html |
| **GTK 4** | c（`gtk_box_append` 当场生效） | c（setter 立即改属性） | d（CSS 节点树 + provider 优先级） | provider 优先级 fallback→app→user | **CSS 选择器**（含 `:focus-within` 等非 Web 语义） | class.Box.html、css-overview.html、class.StyleContext.html |
| **windows-reactor** | a（`children(..) -> View`） | a（`LayoutControl` 改字段） | d（`Context<T>` + `.provide`） | 祖先 provide → `Context::new` fallback；（第三层未查到） | **通道实例身份**（不是类型） | ChildrenControl / LayoutControl / struct.Context（docs.rs） |

### 7.2 四件套映射对照

| 我们的概念 | Flutter | GPUI | egui | Qt | GTK 4 | windows-reactor |
|---|---|---|---|---|---|---|
| `Decl`（不可变描述树） | `Widget` 树 | `Element` 树（每帧） | **无** | 对象树（可变持久） | **无**（保留模式 widget 树） | `View` |
| `Widget`（行为单元） | `Widget`+`createRenderObject` | `impl Element` / `Render` | `Widget` = 一次 `ui()` 动作 | `QWidget` | `GtkWidget` | 原生控件 + builder |
| `Env`（作用域） | `InheritedWidget`（类型键） | `Global`+`TypeId`；文字样式栈 | `Arc<Style>` 链 | 样式表作用域（选择器） | CSS 节点 + provider 优先级 | `Context<T>`（实例身份） |
| `Spec`（风格默认值） | `_XxxDefaultsM3` 生成类 | `Style::default()` / `Cascade<S>` | `Context.dark_style`/`light_style` | `QStyle` | 主题 CSS provider | 未查到 |

### 7.3 有没有"三层优先级"（实例默认值 + 局部覆盖）

| 框架 | 有？ | 形状 | 出处 |
|---|---|---|---|
| Flutter | 有 | `widget 参数 ?? ThemeData ?? M3 生成默认` | `card.dart:256/259/263` |
| GPUI | 有（通用设施） | `Style::default()` ← `base_style` ← focus/hover…，同一个 `refine` 算子 | `elements/div.rs:3472-3525`、`refineable.rs:29` |
| egui | 有（但粗粒度） | `UiBuilder.style` → 父 `Arc` 克隆 → `Context` 全局 | `ui.rs:136/237`、`context.rs:2174` |
| Qt | 有 | 应用样式表 → widget 样式表 → `QStyle`；层内按选择器特异性 | stylesheet.html |
| GTK 4 | 有（另一种形状） | provider 优先级 fallback → application → user | class.StyleContext.html |
| windows-reactor | 部分 | 祖先 `provide` → `Context::new` fallback | struct.Context.html |

---

## 8. 对我们的三条建议

> 下面第 8.1 节写的是「哪条已对齐 / 哪条要改 / 哪条无统一答案」，第 8.2 节是我**自己的判断**
> （已明确标注），不是文献事实。

### 8.1 已对齐 / 要改 / 无统一答案

**已对齐（1）：我们的 `Env` 是"拒绝机制"的通用形状，与 GPUI/Flutter 的类型键查找同构，且比 windows-reactor 更宽。**

我们的 `Env::find<P: 'static>` 沿 parent 链找**最近一个参数类型为 `P` 的层**，类型不符跳过不报错
—— `ui/crates/ui-style/src/env.rs:57-69`，并有两条测试固定该行为（`env.rs:87-98`、`:100-107`）。
这与 Flutter 的 `dependOnInheritedWidgetOfExactType<T>`（类型键，`theme.dart:136-139`）
和 GPUI 的 `globals_by_type.get(&TypeId::of::<G>()).downcast_ref::<G>()`（`app.rs:2522-2527`）
是同一族做法。**结论：`Env` 的取值语义不需要推翻。**

**要改（1）：`ScaffoldSpec::scoped` 现在的"立即生效"语义，与六家里的任意一家都不对应。**

我们现在的实现是：`pub fn scoped<P: 'static>(mut self, params: P) -> Self`
（2026-10-29 起不再收 `StyleId`——它从未被读取，见 `docs/style-object-design.md` §9）
**立即**调 `Env::with(..)` 并把结果存回 `self.env` —— `ui/crates/ui-widgets/src/scaffold.rs:82-85`。
证据化的问题有两条：

1. 它把**作用域与"页面骨架"这个具体控件焊死**。六家里只有 Flutter 的 `Theme` 类做法接近，
   但 Flutter 把它做成**独立 widget**（`theme.dart:214-223`），不是某个控件的方法；
   GPUI/egui/Qt/GTK/reactor 则都没有"某个控件调用就改变子树环境"的 API。
2. 它**在构造期就固定了环境**，之后的 `.with_env(..)`（`scaffold.rs:93-95`）会整体覆盖，
   两者语义重叠且互相能踩。GPUI 的做法是**环境在绘制期压栈/弹栈**（`window.rs:4128-4138`），
   作用域边界由"谁执行 subtree 绘制"决定，不由"谁构造了哪个对象"决定。

**成熟框架也没有统一答案（1）：作用域的"键"用什么。**

六家里出现了 **4 种键**，且都自洽：

| 键 | 谁用 | 出处 |
|---|---|---|
| 类型 / `TypeId` | Flutter、GPUI | `theme.dart:136-139`、`app.rs:2522-2527` |
| 通道实例身份 | windows-reactor | struct.Context.html（"its own identity, even when defaults are equal"） |
| CSS 选择器 | Qt、GTK | stylesheet-syntax.html、css-overview.html |
| 值快照（无键） | egui | `ui.rs:66`、`:237` |

**→ 没有"哪种对"的文献结论。我们的 `Env` 选了"类型键"，有 Flutter/GPUI 两个先例，
但不能声称这是唯一正确解。**

### 8.2 我的判断（**这是我的判断，不是事实**）

**`.scoped` 应该实现成 (d) 环境作用域 + (a) 构造期描述数据，而不是 (b) 包一层，也不是 (c) 立即执行。**

理由，按证据强度排序：

1. **我们的构建是拉式的**（`view()` 返回 `Decl` 树）。在这一架构下，(b)「包一层装饰器」没有承载点
   —— 六家里唯一把 (b) 当默认机制的是 Flutter，而它之所以能这么做，是因为它有 **`Element` 这层持久树**
   去承载包裹关系（`declaration-to-rendering.md` §2.4 已说明 `Widget`→`Element`→`RenderObject` 分层）。
   我们没有 `Element` 层，硬做 (b) 就得先长出这一层。
2. **GPUI 是与我们最像的参照**：它也是"每帧构造元素树"，而它的样式**完全不做包裹**
   —— 全部写进同一个 `StyleRefinement` 字段（`styled.rs:44-47`），在 prepaint 阶段用统一的
   `refine` 算子合成（`elements/div.rs:3472-3473`）。这证明"拉式 + 无包裹"是可行的。
3. **(c) 立即执行不可取**：(c) 只有在 egui 那种"闭包当场分配、当场绘制"的模型里成立
   （决定性证据 `ui.rs:2211`）。我们是拉式的，没有"当场绘制"这个时机；
   `ScaffoldSpec::scoped` 现在的立即写 `self.env` 属于**构造期改自己的字段**——
   这其实是 (a) 的一种（改 builder 字段），只不过它改的是**环境**而不是样式值，
   而"环境"按定义是给子树的，一个自身字段承载不了"只对子树生效"。
4. **但 (d) 目前缺一个东西**：GPUI 能把 `text_color` 做成级联，是因为有
   `with_text_style` 的**压栈/弹栈 try/finally 结构**（`window.rs:4134-4136`）包住子树绘制。
   我们的 `Env` 是一条**不可变的 parent 链**（`env.rs:31-36`，`Rc<Env>`），
   作用域边界目前靠**调用方把 env 传下去**（`scaffold.rs:86-92` 的 `env()` 注释：
   "the app also passes the env to every control it builds into it"）。**这条"调用方传参"契约
   正是 (d) 与 (a) 的分界线，也是当前设计里最脆的一环**：它意味着作用域不是由树结构强制的，
   而是由调用约定维持的。

综上所述，我认为正确的形态是 **"`.scoped` 产出的是作用域描述（(d) 的数据形态），
由构建趟在进入子树时显式携带；它不能是立即执行的副作用，也不需要包一层元素"**。
如果要做成 GPUI 式的自动压栈，前提是构建趟有"进入/离开子树"的回调——
那是我们目前 `view()` 拉式接口里**不存在**的时机，需要先补。

---

## 9. 未证实清单

| # | 条目 | 状态 | 为什么 |
|---|---|---|---|
| 1 | Flutter `Padding` / `Container` / `InheritedWidget` 的**实现**行号 | **查不到** | `packages/flutter/lib/src/widgets/` 与 `cupertino/` 不在 checkout 内（`ls` → no such file）。本文件只给 `material/` 内**用法**行号 + api.flutter.dev URL |
| 2 | Flutter `InheritedWidget` 的 `dependOnInheritedWidgetOfExactType` **实现** | **查不到** | 同上；只能引用调用点（`theme.dart:138-139`）与类型清单 |
| 3 | Qt 的 `path:line` | **不适用** | 本地无 Qt 源码；只有官方文档 |
| 4 | GTK 的 `path:line` | **不适用** | 本地无 GTK 源码；只有官方文档 |
| 5 | windows-reactor 的 `path:line` | **查不到** | 本地无该仓库源码；只用 crates.io + docs.rs + `docs/reactor.md` |
| 6 | windows-reactor 是否存在"风格级默认值"层 | **未查到** | docs.rs 索引与 `docs/reactor.md` 均未描述"风格参数包"；`ThemeRef` 是 brush 绑定，不等价 |
| 7 | windows-reactor 的 `View` 与 reconciler 的**具体**对比算法 | **未查到** | 只在 crates.io 摘要里读到"A Component … returns a View. Reactor reconciles each new view with the native UI tree"（https://crates.io/crates/windows-reactor） |
| 8 | Qt `setStyleSheet` 的**运行时**重解析代价 | **未证实（仅官方定性）** | 原文是"can lead to inconsistent results"，没有给数字 |
| 9 | GTK 的 `Gtk.Box` 是否还有 `gtk_box_pack_start` 这类旧 API | **未核实** | 官方 4.0 文档列的是 `append`/`prepend`/`insert_child_after`，未提旧 API |
| 10 | egui `Ui::scope` 与 `scope_dyn` 的差别是否仅限装箱 | **未逐行核对** | `ui.rs:2186-2190` 与 `:2203-2215` 相邻，本文件只逐行读了 `scope_dyn` |
| 11 | 我们的 `scoped` 在真实构建路径里是否**已经**被调用 | **未核实** | 只读了定义（`scaffold.rs:82-85`）与 `docs/style-system-design.md:624` 的示例；未追调用方 |

### 9.1 需要 Lead 注意的两处「文档与代码可能不一致」

- `docs/style-system-design.md:624` 的示例写法是
  `Page::new(id).scoped(Md3, Md3Params { .. }).main(content)`，
  而仓库里能读到的类型名是 **`ScaffoldSpec::main_only(...)`**（`scaffold.rs:81` 附近的测试
  `a_builder_chain_sets_exactly_the_regions_it_names`）。设计文档里的 `Page` 与实际类型
  **名字不同**；本文件不判断哪个是目标，只记录这条差异。
- `scaffold.rs:82-85` 的 `scoped` 同时存在 `.with_env(..)`（`:90-92`）这个**覆盖式**入口，
  两者的优先级在代码里没有注释说明，也没有测试覆盖两者同时使用的顺序。

---

## 10. 核对方式

本文件所有本地行号均由 `sed -n` / `grep -n` 在读文件时当场核对；所有 URL 于 2026-10-27
用 `curl -sSL -o /dev/null -w '%{http_code}'` 逐条检查，返回 200：

```bash
# Qt
https://doc.qt.io/qt-6/qml-qtquick-item.html
https://doc.qt.io/qt-6/stylesheet-syntax.html
https://doc.qt.io/qt-6/stylesheet.html
https://doc.qt.io/qt-6/qwidget.html#styleSheet-prop
https://doc.qt.io/qt-6/objecttrees.html
# GTK 4
https://docs.gtk.org/gtk4/class.Box.html
https://docs.gtk.org/gtk4/css-overview.html
https://docs.gtk.org/gtk4/css-properties.html
https://docs.gtk.org/gtk4/class.StyleContext.html
https://docs.gtk.org/gtk4/class.Widget.html
# windows-reactor
https://docs.rs/windows-reactor/latest/windows_reactor/
https://docs.rs/windows-reactor/latest/windows_reactor/trait.LayoutControl.html
https://docs.rs/windows-reactor/latest/windows_reactor/trait.ChildrenControl.html
https://docs.rs/windows-reactor/latest/windows_reactor/struct.Context.html
https://github.com/microsoft/windows-rs/blob/master/docs/reactor.md
https://crates.io/crates/windows-reactor
```

Flutter 的 api.flutter.dev URL 本文件作为"实现不可读"的补充出处引用
（`Padding`/`Container`/`Row`/`Stack` 等），未逐条 curl；它们只用于标注"该 widget 存在且有官方文档"，
不用于任何具体数值结论。
