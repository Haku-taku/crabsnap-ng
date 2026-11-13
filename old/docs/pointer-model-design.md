# 指针事件模型设计：一个归一化的手势结构

> 状态：**设计（未实现）**。本文回答两个问题：
> 1. 引擎现在怎样把指针事件变成控件与 app 能看到的东西；
> 2. 是否应当把这些分类合并成单一结构体，以及合并后整体是什么形状。
>
> 文中每条事实都标了 `file:line`，均已在本仓库读代码核实。参考框架的做法单独标注来源。

---

## 0. 结论摘要

| 问题 | 结论 |
|---|---|
| 该不该把 `Event` 的九个变体合并？ | **不该**。`Event` 是平台协议的 1:1 映射，每个变体的载荷不同（键码、文本、滚轮量），合并会丢掉协议信息 |
| 该不该把「派生出的事实」合并？ | **该**。`WidgetState::local` + `drag_step` + `pressed` + `dragging` 描述的是同一件事的四个碎片，应合成一个 `PointerGesture` |
| 涟漪做不出来，根因是什么？ | 按下原点从未到达控件。`DragEvent` 里有 `from`（`drag.rs:70-77`），但 `report_drag` 把它丢掉只留 `Change::Moved{dx,dy}`（`view.rs:831-840`），控件因此只能从中心长 |
| 引擎锁存点怎么解？ | 把**按下墨迹（press ink）上移到引擎**：控件只声明「我按下时用什么颜色、多大」，splat 的原点、扩张、淡出全归引擎，跨松手存活。Material 的 `InkResponse`、Compose 的 `RippleHostView`、GTK 的 ripple 都是这么分的 |
| 点击与拖拽的边界在哪？ | 同一条 `DRAG_THRESHOLD`：越过它就不再是点击（D6，2026-10-29 已修）。拖拽只对声明了 `Sense::DRAG` 的节点成立，点击的作废却必须对**所有**节点成立 |

---

## 1. 现状：四层模型

### 1.1 `Event`（平台 → 引擎）

`crates/ui-core/src/event.rs:52-83`，九个平铺变体：

```rust
PointerMoved { pos }        PointerLeft
PointerPressed { pos, button }   PointerReleased { pos, button }
Text(String)                KeyPressed { key } / KeyReleased { key }
Scroll { pos, delta }       Modifiers(Modifiers)
```

它是 wl_pointer / wl_keyboard 的直译：`Scroll` 带量、`Text` 带字符串、`KeyPressed` 带键码。
**这一层不该合并**：合并只能靠 `Option` 字段，把「编译期穷尽」换成「运行期判空」。

### 1.2 `AppLoop::dispatch`（引擎内部）

`crates/ui-platform/src/view.rs:878-1040`。一次事件进来，引擎做四件事：

1. **维护路由事实**：`hovered` / `pressed` / `focused` / `clicked` / `dropped` 以及四条祖先路径；
2. **驱动拖拽状态机**：`DragTracker`（`crates/ui-core/src/drag.rs`）持有 `Pressed{on, at}` 与
   `Dragging{on, from, current}`，越过 `DRAG_THRESHOLD = 4.0` 才成为拖拽；
3. **把手势翻译成 `Change` 送给 app**（`report_drag`，`view.rs:823-848`）；
4. **不把任何原始事件交给 app**：`App` 只有 `view()` 与 `changed(id, change)` 两个方法，
   注释明确写了「app 没有原始事件钩子可回答」（`view.rs:1035-1036`）。

### 1.3 `Change`（引擎 → app）

`crates/ui-core/src/change.rs:17-66`，八个语义变体。与指针有关的是四个：

| 变体 | 何时发出 | 载荷 |
|---|---|---|
| `Activated` | 点击落定，或焦点节点上的 Enter/Space | 无 |
| `DragStarted` | 按下越过 4dp 阈值 | 无 |
| `Moved { dx, dy }` | 拖拽中每次移动 | **相对按下点的位移** |
| `Dropped(Option<Id>)` / `Cancelled` | 拖拽结束 | 落点 / 无 |

### 1.4 `WidgetState`（引擎 → 控件）

`crates/ui-platform/src/state.rs:8-38`，控件每帧通过 `cx.state(id)` 拿到的快照：

```rust
hovered, pressed, clicked, dropped, focused, focus_visible, dragging, disabled, selected,
local: Option<Point>,      // 「按下或拖拽时，指针在本节点自身空间的位置」
drag_step: Option<Point>,  // 「拖拽进行中，指针相对上一次事件的位移」
```

---

## 2. 缺陷（每条都能指到一行代码）

### D1　`local` 是**帧级共享值**，不是每节点坐标

`Routing.local` 是一个 `Option<Point>` 字段（`crates/ui-core/src/cx.rs:41`），
`Cx::local()` 对**任何** `id` 返回同一个值（`cx.rs:169-171`）。
它由 `pointer_local()` 算出（`view.rs:811-821`）：取拖拽所有者（或按下节点）的矩形原点，
把表面坐标减成该节点的本地坐标。

于是：`WidgetState::of(core, other_id).local` 返回的是**别人的坐标系里的点**。
目前没炸，只是因为读它的控件（滑块、页签栏）恰好就是按下节点。
任何一个想在悬停时读 `local` 的控件（例如按指针位置决定高光方向）都会拿到垃圾值。

### D2　按下原点从未到达控件

`DragEvent::Started { on, from, local }` 与 `Moved { on, delta, step, local }`
都带 `from`（按下点，`drag.rs:70-77`），`DragTracker` 也一直存着它
（`DragState::Pressed{on, at}` / `Dragging{on, from, current}`，`drag.rs:20-30`）。

但 `report_drag` 把它全部丢掉，只留 `dx/dy`（`view.rs:831-840`），
`WidgetState` 里也没有它。**引擎知道原点，控件不知道。**

### D3　同一件事有两条通道，语义不一致

`Change::Moved{dx,dy}` 与 `WidgetState::{local, drag_step}` 描述同一次移动：

- 前者给 app，单位是**表面像素**；
- 后者给控件，单位是**节点本地像素**，而且（见 D1）只对按下节点正确。

一个控件若既想映射值（要 `local`）又想告诉 app 位移（要 `Moved`），就得同时走两条路。
滑块现在是这么写的：`change()` 里报 `Change::Value`，`decl()` 里读 `state.local`。

### D4　涟漪只能从中心长出

`state_disc`（`crates/ui-widgets-md3/src/layer.rs:36-70`）是 MD3 全部控件共用的状态层：
悬停与焦点直接满盘，**按压时半径 = `reaction × full`，圆心恒在控件中心**。

MD3 规范要求按压墨迹**从按下点**向外扩（Flutter `InkRipple`、Compose `Ripple`、
material-web `MdRipple` 都是 `origin` 参数）。我们要么照规范做，要么承认做不到——
而做不到的原因不是渲染，是 D2。

### D5　锁存点在控件侧无处安放

即使把 `origin` 塞进 `WidgetState`，控件也留不住它：`Widget` 是无跨帧状态的
（`decl` 是纯函数，所有动画都走 `AnimStore` 的标量实例）。
涟漪的淡出发生在松手**之后**，那时 `pressed` 已是 false，`gesture` 也已结束，
原点就没了。唯一的出路是让引擎记住它——见 §3.4。

### D6　点击路径完全不看指针走了多远（2026-10-29 已修）

`PointerPressed` 只在按下节点有 `Sense::DRAG` 祖先时才喂 `DragTracker`
（`view.rs:921-923` 的 `if let Some(on) = self.drag_owner()`）。其余节点的
`PointerMoved` 无人查看，而松手分支只要「按下过」就报 `clicked`（原 `view.rs:928-958`）。

于是：按下按钮 → 把指针拖到 200px 外的空白 → 松手，按钮照样激活。
阈值只有拖拽一条链路在用，点击是另一条，两者之间没有任何联系。

修法（`ui-core/src/drag.rs` 的 `past_threshold` + `view.rs` 的 `press_at`）：

- 阈值判据收敛成一个函数 `past_threshold(from, to)`，`DragTracker::moved` 与点击路径共用；
- `View` 记 `press_at`：移动越过阈值而拖拽未启动时，直接作废这次按下
  （`pressed = None`），它既不是点击也不是拖拽；
- 松手位置也是指针路径的一点：先把松手点喂给 `DragTracker`（快速甩动可能没有中间
  `PointerMoved`），再判 `release`；落在阈值外同样不报点击。

### D7　单帧事实在事件入口被清零（2026-10-29 已修）

`dispatch` 开头无条件清掉 `clicked` / `dropped` / `drag_step`（原 `view.rs:882-884`），
但这三者是「到下一帧为止」的事实。事件成批到达——真实循环是
`for ev in win.take_events() { lp.dispatch(ev) }`（`gallery.rs:4408`）——因此：

- 松手之后又落进一次 `PointerMoved`，这次点击就没了，表现为「点了没反应」；
- 拖拽结束的 `dropped` 同样被抹掉，开关按落点判定会整段跳过；
- `drag_step` 每帧只剩最后一个事件的切片，一帧内多次移动时控件少走距离。

修法：只在帧消费时清零（`view.rs:641-643`），`drag_step` 改为**累加**（`view.rs:846`），
`DragStarted` 把它归零（新手势不带上一段的位移）。

---

## 3. 提案

### 3.1 一个归一化的手势结构

```rust
/// One press cycle seen from one node: where it started, where the pointer is, what phase.
/// 从某个节点看一次按下循环：从哪里开始、指针在哪、处于哪个阶段。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerGesture {
    /// The press point, in this node's own space.
    pub origin: Point,
    /// The latest pointer position, in this node's own space.
    pub current: Point,
    /// `current - origin`.
    pub delta: Point,
    /// Since the previous event; the reference accumulates this for inertial scroll.
    pub step: Point,
    pub phase: GesturePhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GesturePhase {
    /// Down, not yet past `DRAG_THRESHOLD`.
    Pressed,
    /// Past the threshold; the node has the pointer captured.
    Dragging,
    /// Released on this node this frame; the last frame the origin is still knowable.
    Released,
}
```

`WidgetState` 随之变成：

```rust
pub struct WidgetState {
    pub hovered: bool,
    pub pressed: bool,          // 便捷布尔，等价于 gesture.phase == Pressed|Dragging
    pub clicked: bool,
    pub dropped: bool,
    pub focused: bool,
    pub focus_visible: bool,
    pub disabled: bool,
    pub selected: bool,
    pub gesture: Option<PointerGesture>,   // 取代 local + drag_step + dragging
}
```

**关键改动是 `gesture` 每节点独立**。做法：`Routing` 不再存一个 `local`，而存

```rust
pub struct Routing {
    // ...
    /// The live press cycle in surface space; `None` when no primary button is down.
    pub press: Option<PressTrace>,
    /// `(id, rect origin)` for every node on the pressed path, root first — the only nodes
    /// that may legitimately ask "where was I pressed".
    /// 按下路径上每个节点的 `(id, 矩形原点)`，根在前——只有它们有权问「我在哪被按下」。
    pub press_origins: Vec<(Id, Point)>,
}
```

`Cx::gesture(id)` 在 `press_origins` 里查 `id`；查不到就返回 `None`。
这样 D1 自动消失：不在按下路径上的节点**拿不到**手势，而不是拿到别人的坐标。
表长等于按下路径长度（实测 3–6 项），构造代价可以忽略。

### 3.2 `Change` 收敛

- **删掉 `Moved { dx, dy }`**。它是 D3 的重复通道；控件已经从 `gesture` 拿到位置，
  要报语义值时自己报 `Change::Value`。app 从来不需要「移动了多少像素」。
- **保留 `DragStarted` / `Dropped` / `Cancelled`**。它们不是位置，是**结果**；
  而且 `Change` 是 app 唯一能及时听到的通道（`view` 里读到的状态是上一帧的）。
- `DragEvent`（`drag.rs:80-95`）**留在引擎内部**，不再外泄为 `Change` 的等价物。

### 3.3 控件侧写起来是什么样

滑块不再需要「按下就写值」的特判，也不必自建 `dragging` 字段：

```rust
let g = state.gesture;                       // Option<PointerGesture>
let track_x = g.map(|g| g.current.x - inset); // 只在按下的节点上有意义
```

页签栏的 `tab_at(state.local?, ...)` 改成 `tab_at(g.current, ...)`，
并且只在 `g` 存在时求值——语义不变，坐标来源变成明确的。

### 3.4 涟漪上移到引擎：`Decl::press_ink`

> **状态：已实现（2026-10-29）**。`PressInk`/`InkRadius` 在 `crates/ui-core/src/draw.rs`，
> splat 状态机在 `crates/ui-platform/src/view.rs` 的 `Splat`，MD3 侧见下表。

```rust
pub struct PressInk {
    pub colour: Color,
    pub radius: InkRadius,   // Cover：盖住参考框；Fixed(r)：绕落点的固定半径
    pub target: InkTarget,   // 有效背景：涟漪以它为尺寸与裁剪基准
    pub fade_in: AnimSpec,   // 75ms
    pub grow: AnimSpec,      // 225ms，Curves.ease
    pub fade: AnimSpec,      // 150ms，松手后延迟 225ms 才开始
}

pub enum InkTarget {
    Node,                  // 节点自身盒子 + 声明/推断的曲线
    Shape(usize),          // 本节点第 n 个绘制形状的矩形 + 曲线（声明顺序）
    Descendant(Id),        // 某后代节点的盒子 + 曲线（Flutter `getRectCallback`）
}
```

引擎的契约：

1. 落在本节点（含后代）的一次主键按下，**在按下点**开始一次 splat；
2. `Cover`：圆心边扩**边漂到参考框中心**，终点半径取对角线的一半（从中心刚好盖住参考框）；
   `Fixed(r)`：圆心不动；
3. 半径从终点的 30% 起步（轻点也立刻看得见一个点），`fade_in` 让它在 75ms 内显形；
4. 松手后 `fade` 延迟 225ms、再淡出 150ms；**淡入在松手后继续走**——若在松手处定格，
   一个「按下与松手落在同一批事件里」的快点会把透明度定格在 0，墨迹永远不出现；
5. 多个 splat 可并存（快速连点两次是两个同心墨迹）；
6. 绘制顺序在节点自身图元**之上**、子节点**之下**，裁剪到参考框的圆角形状；
   `target` 决定参考框与裁剪目标（每帧解析，形状或后代处于动画中段也跟随）：
   `Node` 用节点盒子，`Shape(n)` 用本节点第 n 个绘制形状，`Descendant(id)` 用某后代的盒子
   ——对齐 Flutter `_IndicatorInkWell`（`containedInkWell` + `getRectCallback` 返回图标框）。
   nav bar / nav rail 的墨迹用 `Descendant(indicator)`，carousel 条目用 `Shape(0)`（背景恒为首个形状）。
   `Shape(n)` 的索引是**声明顺序**：控件必须保证目标形状的位置稳定（条件性 push 会让索引漂移，
   这正是 Text button 圆角跳变的同类风险），不能稳定时用 `Node` 或 `Descendant`。

为什么放在引擎而不是控件：

| 理由 | 依据 |
|---|---|
| 控件无跨帧状态，留不住原点 | `Widget::decl` 是纯函数，见 §2 D5 |
| 三个生态都这么分 | Flutter：`InkResponse` 持有 `InkHighlight`/`InkSplash`，widget 只给颜色；Compose：`RippleHostView` 是每个节点的 View 子类；material-web：`MdRipple` 是独立元素，`attach(pointerdown)` 自己算原点 |
| 引擎已拥有绘制顺序与裁剪 | `plan_batches` 决定 mask 与批次；控件画的形状拿不到自己的圆角裁剪 |
| 涟漪是「按下的可视化」，与风格无关 | MD3/Fluent/Glass 都有，只有颜色与半径不同 |

**哪些控件有落点涟漪**——这是 Flutter 自己划的界，不是我们的发明：

| 有落点涟漪（`InkResponse` 系） | 只有中心圆盘（`Toggleable` 系） |
|---|---|
| 按钮、芯片、列表行、可点击卡片、页签、FAB 菜单项与关闭按钮 | 复选框、单选框、开关、滑块（圆盘钉在手柄上） |

依据：`ToggleableStateMixin` 只维护一个 100ms 的 `reaction` 控制器，`paintRadialReaction`
的 `origin` 由子类给出，而复选框/单选框/开关传的是控件中心——文档明说它「通常不同于
`downPosition`」；只有 `InkWell`/`InkResponse` 才走 `InkRipple`。所以 `state_disc` 的
按压分支**保留**给这四类，按钮/芯片/行/页签/卡片的平坦状态层只留悬停与焦点，
按下改由 `decl.press_ink(Some(PressInk { .. }))` 声明。

官方时长（`flutter/lib/src/material/ink_ripple.dart`、`material-web/ripple/internal/ripple.ts`）：

| 项 | Flutter `InkRipple` | material-web `md-ripple` | Compose |
|---|---|---|---|
| 起点 | 按下点 | 按下点 | 按下点 |
| 圆心 | 动画到参考框中心 | 动画到元素中心 | 不移动 |
| 半径 | 0.3R→R+5，225ms，`Curves.ease` | 20%→覆盖，450ms，standard | 225ms |
| 淡入 | 75ms | 按下透明度过渡 | 75ms |
| 淡出 | 松手后 375ms，最后 150ms | 至少按住 225ms | 150ms |

三家里 Compose 不漂移，Flutter 与 material-web 都漂，我们跟多数；material-web 的
450ms 与它的 150ms 触摸判定延迟配套，单指针桌面路径不需要那份等待。

### 3.4b 滚动条拖拽（2026-10-31）

滚动条是**容器的固有装饰**，不是独立节点，所以它的手势必须由引擎特判，而不是路由给
某个控件：

- `Scroll` 控件画 thumb 时把 `thumb`/`track` 矩形写进 `ContainerParams::scrollbar`。
- `PointerPressed` 的第一件事是 `scrollbar_at(pos)`：从命中节点向上找声明了滚动条的
  容器，指针落在 `track` 内即接管这次按下——不设 `pressed`、不泼墨、不改焦点
  （GPUI `BlockMouseExceptScroll` 的同义物）。
- 状态 `ScrollbarDrag { id, press_y, start_offset, travel, overflow }` 与选区拖拽同级，
  在 `PointerMoved` 里优先于内容拖拽，释放即结束。
- 点轨道先跳位（thumb 中心对到指针），拖拽从跳位后的偏移继续。

### 3.5 光标

光标形状是**每帧解析、每帧重新声明**的 seat 属性，已经在
`PresentTarget::set_cursor`（`crates/ui-platform/src/present.rs`）上收敛为一个交出点。
它与本文的手势模型共享同一份「指针在哪、在谁上面」，但不需要 `PointerGesture`
里的任何字段——光标只看 `pointer_path`。细节见 `docs/cursor-system-design.md`。

---

## 4. 迁移顺序与验证

| 步 | 改动 | 怎么证明它成立 |
|---|---|---|
| 1 | `Routing` 加 `press`/`press_origins`，`Cx::gesture(id)` 上线；`local`/`drag_step` 保留但标记弃用 | 新测试：`a_node_off_the_press_path_sees_no_gesture`——按下 A 时读 B 的 `gesture` 必须是 `None`（今天会拿到 A 的坐标） |
| 2 | `WidgetState` 换成 `gesture`，改掉滑块/页签栏/开关三处读取点 | 既有 672 条测试全绿；`slider_drag_value` 与 `tab click` 两条 selftest 不变 |
| 3 | `Decl::press_ink` + 引擎 splat 状态 | 新测试：`the_splat_starts_where_the_press_landed`——按下 (40,10) 后第一帧墨迹圆心在 (40,10) 附近，不是 (50,20) |
| 4 | MD3 控件改用它，按钮/芯片/行/页签/卡片的平坦层只留悬停与焦点 | 画廊里按下 Large 按钮左端，墨迹从左端起、边扩边漂向中心（`GALLERY_PRESS=x,y` 截图为证） |
| 5 | 删 `Change::Moved`，`report_drag` 不再发它 | 破坏性验证：把 `Moved` 加回去，`changed` 里的 `_ => {}` 分支必须没有行为差异 |

每一步都按 AGENTS.md §7.6：先让新断言**红**，再改实现。

---

## 5. 明确不做的

- **不合并 `Event`**。它是协议直译，合并只会把穷尽匹配换成 `Option` 判空（§1.1）。
- **不把 `Change` 变成事件流**。app 现在只能 `match` 语义变体，这是刻意的：
  「点击的含义归控件判断，引擎负责路由」（`view.rs:966-974` 的注释）。
- **不引入「指针捕获」新概念**。`DragState::captures_pointer()`（`drag.rs:56-59`）已经存在，
  本文只是把它暴露成 `GesturePhase`。
- **不改 `DRAG_THRESHOLD`**。4dp 是手感参数，与本次重构无关。
