# GPUI 滚动实现调研

> 面向：为我们的 Rust UI 引擎（`ui/`）设计统一滚动容器抽象提供参考。
> 参考版本：GPUI 0.2.2（Zed commit `79fe6a6`），源码在
> `/home/ollie/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/`。
> 本文件只写**已核实**的结论；行号针对上述 commit。

## 0. 结论速览

| 问题 | GPUI 的做法 | 我们缺什么 |
|---|---|---|
| 轴表达 | 没有 `ScrollableAxis`/`Scrollable` trait；轴就是 `Axis { Vertical, Horizontal }`，滚动容器靠 style 的 `overflow.x/y: Option<Overflow::Scroll>` 声明 | 已有一半：`Clip { x, y: Overflow }` + `scrolls()`；缺 `Axis` 类型、`restrict_scroll_to_axis`（手势锁轴） |
| 滚动条 | gpui 核心没有 Scrollbar 控件；在 Zed 自己的 `crates/ui`（`components/scrollbar.rs`，1914 行）。`Scrollbars` 构建器 + `WithScrollbar for Div`。显示策略 `ShowScrollbar::{Auto,System,Always,Never}` + `ScrollbarRevealPolicy` | 完全缺 |
| 滚动状态 | `ScrollHandle(Rc<RefCell<ScrollHandleState>>)`；offset 存**元素状态跨帧表**（无 handle 时）或用户持有的 handle。offset 是负数（内容相对视口左上） | `scroll_y/x` 只是 `Decl` 输入，引擎**读不回**偏移（engine-gaps §2） |
| 虚拟化 | `UniformList`（等高，`render_items(Range)`）+ `List`（SumTree 变高）都虚拟化 | 无 |
| 惯性 | 触摸 pan 由 `gestures.rs` 手势识别器产出含 `TouchPhase` 的 `ScrollEvent`，`ScrollPhysics` 状态机在**触摸层**；滚轮无惯性 | 无 |
| 弹性/回弹 | **没有**。边界行为（bounce/clamp）是容器策略；默认 clamp 到 `[-max, 0]` | 无 |
| 拖拽滚动 | **没有**鼠标按住拖滚动内容；只有滚动条 thumb 拖拽 | 无 |
| 滚动输入通道 | 滚轮/触控板 = 平台 `ScrollEvent`；触摸 pan = 合成 `ScrollEvent`，两者都走同一 `window.on_mouse_event` 分发 | 已有 `Event::Scroll` → `scroll_target()` |

---

## 1. 轴与"可滚动"的表达：没有 `ScrollableAxis`/`Scrollable` trait

任务书假设的 `ScrollableAxis` / `Scrollable` trait **不存在于 zed 仓库**（deepwiki 查证 + 本地 grep 确认）。

- 轴就是核心枚举 `Axis { Vertical, Horizontal }`（`crates/gpui/src/geometry.rs`）。Zed 的滚动条把 `Axis` 重命名为 `ScrollbarAxis` 用（`crates/ui/src/components/scrollbar.rs:5`：`Axis as ScrollbarAxis`）。
- "这是一个滚动容器"由 **style 的 `overflow`** 表达，不是 trait：

  ```rust
  // crates/gpui/src/style.rs
  pub overflow: Point<Overflow>,        // Overflow::{Visible, Hidden, Scroll}
  pub restrict_scroll_to_axis: Option<bool>,
  pub allow_concurrent_scroll: Option<bool>,
  ```
- `Div` 的链式 API（`crates/gpui/src/elements/div.rs:1568-1603`）全部是往 style 写字段：

  ```rust
  fn overflow_scroll(self)    // overflow.x = Some(Scroll); overflow.y = Some(Scroll)
  fn overflow_x_scroll(self)  // overflow.x = Some(Scroll)
  fn overflow_y_scroll(self)  // overflow.y = Some(Scroll)
  fn restrict_scroll_to_axis(self)   // 手势锁主导轴，见 §5
  fn track_scroll(&ScrollHandle)     // 把 handle 绑到本容器
  fn anchor_scroll(Option<ScrollAnchor>)
  ```

第三方 `gpui-component`（2024 年曾短暂用过 `ScrollableAxis` 枚举，PR #509 已删掉改回 `Axis`）——这个命名是从那里来的，不是 zed 主线。

**启示**：我们的 `Clip { x, y: Overflow }` + `scrolls()` 已经等价于核心表达。缺口是把「滚动手势输入」接上轴：GPUI 用 `overflow: Point<Overflow>` 让**每轴各自可滚**，滚轮 delta 按轴分流（`div.rs:3413-3430`）。

---

## 2. 滚动输入：滚轮/触控板 = ScrollEvent，轴分流 + 锁轴

滚动输入统一为 `ScrollEvent`（`crates/gpui/src/interactive.rs:522-538`）：

```rust
pub struct ScrollEvent {
    pub position: Point<Pixels>,   // 触摸 pan 时 = pan 起点（整个手势锁定起始容器）
    pub delta: ScrollDelta,
    pub modifiers: Modifiers,
    pub touch_phase: TouchPhase,   // Started/Moved/Ended/Cancelled
    pub source: ScrollSource,      // Pointer（滚轮/触控板）| Touch（触摸 pan）
}
pub enum ScrollDelta { Pixels(Point<Pixels>), Lines(Point<f32>) }
```

分发（`div.rs:3398-3444` 的 `paint_scroll_listener`）：

1. Bubble 阶段，且 `hitbox.should_handle_scroll(window)` 为真才处理——见 §6 命中语义。
2. `delta.pixel_delta(line_height)`：`Lines` 按 `window.line_height()` 换算成像素（行距来自全局 text style，不是容器自己的）。
3. **轴分流**：`overflow.x/y` 各自 `Scroll` 才接收对应轴 delta；**单向轴**时另一轴的 delta 会转嫁过来（如只有纵向滚动，横 delta 变成纵 delta，`div.rs:3416-3419`）。
4. `restrict_scroll_to_axis && delta.precise()` 时用 `OngoingScroll::filter` 锁主导轴（`gestures.rs:44-104`）。
5. `allow_concurrent_scroll` 为假时对角线只保留绝对值大的轴。
6. 直接 `scroll_offset += delta`（**不做插值/动画**，raw 累积），偏移变了就 `cx.notify(current_view)`。
7. **clamp 由下一帧 layout 的 `clamp_scroll_position` 兜底**（见 §4）——事件处理不 clamp，靠重算。

**启示**：我们已有 `Event::Scroll { pos, delta: f32 }` 但只有**单标量 delta**（没有 x/y、没有 Lines/Pixels 之分、没有 touch_phase）。要支撑轨道板（精确像素）与滚轮（行）混用、锁轴、多指并发，event 需要升级成 `ScrollDelta { Lines(Point), Pixels(Point) }` 的结构。

---

## 3. 滚动状态存哪：ScrollHandle（用户持有）或元素状态跨帧表

### 3.1 `ScrollHandle`

`crates/gpui/src/elements/div.rs:4279-4498`：

```rust
struct ScrollHandleState {
    offset: Rc<RefCell<Point<Pixels>>>,   // 内容相对视口的偏移，向下滚为负
    ongoing_scroll: Rc<RefCell<OngoingScroll>>,  // 手势锁轴状态，跨帧共享
    bounds: Bounds<Pixels>,                // 视口 bounds（layout 时写入）
    max_offset: Point<Pixels>,             // scroll_max = content - viewport，layout 时写入
    child_bounds: Vec<Bounds<Pixels>>,     // 每个子元素的 bounds（scroll_to_item 用）
    scroll_to_bottom: bool,
    overflow: Point<Overflow>,
    active_item: Option<ScrollActiveItem>, // 延迟到下次 prepaint 的 scroll_to_item 请求
}
pub struct ScrollHandle(Rc<RefCell<ScrollHandleState>>);  // Clone，用户存在 view/model 里
```

要点：
- **offset 的符号约定：负值**。`clamp_scroll_position` 把 `offset` clamp 到 `[-scroll_max, 0]`（`div.rs:2533`）。
- handle 是 `Rc<RefCell<>>`，**不依赖元素 id、不依赖 frame**——用户在 view 里持有，跨帧天然存活，这正是"读回偏移"的通道。
- `set_offset` 是用户侧显式跳转；`scroll_to_item`/`scroll_to_top_of_item` 走 `active_item` 延迟到 prepaint 再算（见 §4）。
- 还提供 `top_item()/bottom_item()/logical_scroll_top()`：基于 `child_bounds` 二分查「第几个子元素在视口顶部」，把像素偏移转成 (index, 子内偏移) 的**逻辑偏移**。

### 3.2 没有 handle 时的跨帧存储

`Div::prepaint`（`div.rs:2323-2343`）：无 `tracked_scroll_handle` 但 `overflow == Scroll` 时，从 **element state 跨帧表**取 `scroll_offset` / `ongoing_scroll`（`Rc` 使 paint 期注册的鼠标监听器能持引用）。

旧分析（`docs/research/gpui-0.2.2-analysis.md` §10.2）的定性：跨帧存储是 **`(GlobalElementId, TypeId)` 映射**（`window.rs:1048, 4399-4470`），"hover、pending_mouse_down、scroll offsets、spring velocity"都住这表，只在再次访问时迁移。`GlobalElementId` 是**增量哈希路径**（`element.rs:218-300`），O(1) 求值、可 Eq/Hash。

**启示**：两条路都可以学，但侧重点不同——
- **`ScrollHandle`（用户持有）**：解决 engine-gaps §2 的「控件读不回滚动偏移」。滚动条、app-bar 联动、carousel 吸附都需要读 offset，只有把 offset 交给应用层持有者才能读。
- **元素状态跨帧表**：不需要应用参与也能滚动（如内嵌滚动容器）。我们引擎的 `Routing`/`Cx` 已有按 id 的状态（`Cx::caret_for` 之类），加一张 `id → scroll offset` 表即可，engine-gaps §2 的建议正好就是这个。

---

## 4. measure 与 clamp：内容测一次、clamp 在 layout

`Div::prepaint(bounds, content_size, ...)` 的签名说明契约（`div.rs:2353-2362`）：**布局引擎把内容完整测出（可能大于视口），把 `content_size` 交给 prepaint**；prepaint 内部调用 `clamp_scroll_position`：

```rust
// div.rs:2486-2549
let scroll_max = Point::from(padded_content_size - bounds.size)
    .map(round_to_two_decimals)      // 2 位小数舍入，避免 sub-pixel 可滚 0.0000x px
    .max(&Default::default());
scroll_offset.x = scroll_offset.x.clamp(-scroll_max.x, px(0.));
// scroll_to_bottom 标记时 y 直接取 -scroll_max.y
```

- **滚动轴上的内容不设界**：taffy 里 scroll 轴给子节点 `AvailableSpace::Definite` 无界（`div.rs` 里没有；taffy 侧由 `to_taffy` 把 scroll 轴写成无界），内容按其自身尺寸测，`scroll_max = content − viewport`。
- clamp 时机是 **prepaint**（每帧 layout 后），不是事件处理时——事件只管 `offset += delta`。
- 旧分析（§4）的关键点：**`Window::layout_bounds` 叠加 `element_offset()`（scroll offset）**，`window.rs:5293-5306`。绘制时整个子树按 offset 平移 + `ContentMask` 裁剪（`style.rs:638-694`，`overflow` 折叠成 mask）。

**与我们引擎的对应**：`layout_engine.rs:1013-1017`（"A scrolling axis hands children no bound at all"）、`:1283-1288`（content_origin 减 scroll clamp）、`:1274` 与 `:1812` 的 `scroll_extent`——设计已经同构。我们缺的只是**把 `scroll_extent`（= GPUI 的 max_offset）和当前 offset 暴露给应用层读**。

---

## 5. 滚动条：在 Zed 的 crates/ui，不在 gpui 核心

`crates/ui/src/components/scrollbar.rs`（1914 行）。滚动条是**附着在 Div 上的叠加装饰**，不是独立容器：

```rust
// 用法（fluent，WithScrollbar trait，scrollbar.rs:105-117, 155-177）
div()
    .id("scroller")
    .custom_scrollbars(Scrollbars::new(ScrollAxes::Vertical), window, cx)
    // 内部：把 div 设成 overflow_scroll()，绑 scroll handle，加滚动条子元素
```

### 5.1 配置（`Scrollbars<T>` 构建器，scrollbar.rs:430-575）

```rust
pub struct Scrollbars<T: ScrollableHandle = ScrollHandle> {
    id: Option<ElementId>,
    get_visibility: fn(&App) -> ShowScrollbar,     // 显示策略回调（可全局/设置驱动）
    tracked_entity: Option<Option<EntityId>>,      // 滚动时 notify 谁（.notify_content()/.tracked_entity(id)）
    scrollable_handle: Handle<T>,                  // 显式 handle（.tracked_scroll_handle(&h)）或自建
    visibility: Point<ReservedSpace>,              // 每轴的保留空间策略
    style: Option<ScrollbarStyle>,                 // Regular(6px) | Editor(15px)
    reveal_policy: ScrollbarRevealPolicy,          // ScrollOrContentChange | ScrollOnly
    track_color: Option<Hsla>,
    border: bool,
}
```

链式方法：`id/notify_content/tracked_entity/tracked_scroll_handle/show_along(ScrollAxes)/style/reveal_policy/with_track_along(axis, color)/with_stable_track_along`。

**何时显示**（`ShowScrollbar` + `ScrollbarState`）：
- `ShowScrollbar::{Auto,System,Always,Never}`（scrollbar.rs:43-54）。`Auto` = Autohide，`System` 走全局 `ScrollbarAutoHide`。
- `VisibilityState` 状态机：`Hidden | ThumbHidden | Visible | Disabled | Animating`（scrollbar.rs:576-662），用 **delta 0→1 插值 + ease_in_out** 做淡入淡出（`SCROLLBAR_SHOW_DURATION=50ms`、`SCROLLBAR_HIDE_DURATION=400ms`、`SCROLLBAR_HIDE_DELAY_INTERVAL=1s`）。
- **显示触发**：滚动/内容变化（`ScrollbarRevealPolicy::should_reveal`）、鼠标进入父容器、thumb hover、拖动中。**隐藏**：1 秒无活动后淡出；`reduce_motion` 时直接跳显隐不动画。
- `ScrollbarRevealPolicy::ScrollOrContentChange`（默认）在「几何变了**或**滚动位置独立于内容变了」都显示；`ScrollOnly` 只看滚动位置（`changed_independently_of_content` 排除了"内容变长导致位置被动变"的情况，scrollbar.rs:1201-1240）。

### 5.2 几何与绘制

`ScrollbarPrepaintState`（scrollbar.rs:1178-1182）在 prepaint 里计算：
- **thumb 尺寸**：`visible_percentage = viewport / (viewport + max_offset)`，`thumb_size = max(25px, viewport * visible_percentage)`（`MINIMUM_THUMB_SIZE`，scrollbar.rs:934-967）。内容不多时 thumb 大。
- **thumb 位置**：`start_offset = (current_offset / max_offset) * (viewport_size - thumb_size)`（scrollbar.rs:958），比例映射。
- 轨道圆角：跟随父容器内角半径（`inner_corner_radii`，scrollbar.rs:213-236）。
- 保留空间：`space_to_reserve() = style.to_pixels() + 2*SCROLLBAR_PADDING`（scrollbar.rs:830-832）；**给父 div 加 padding 让内容不顶到滚动条**（render_scrollbar 里 `.pb(space)/.pr(space)`）。`ReservedSpace::{None, Thumb, Track, StableTrack}` 决定是否永远占位。
- 命中区：`HitboxBehavior::BlockMouseExceptScroll`（scrollbar.rs:1379）——滚动条挡住鼠标但**不挡滚动**（见 §6）。

### 5.3 交互（mouse 事件，scrollbar.rs:1630-1749）

- **拖 thumb**：MouseDown 在 thumb 内 → `ThumbState::Dragging(axis, offset_in_thumb)`，捕获阶段（Capture）接管；MouseMove 里 `compute_click_offset(TrackClick|ThumbDrag)` 把「thumb 在轨道内的位置」反解成 scroll offset；MouseUp 结束并 `drag_ended()`。
- **点击轨道**：`TrackClick` 把 thumb 中心对齐到点击点（`thumb_offset = thumb_size/2`，scrollbar.rs:1142）。
- `compute_click_offset`（scrollbar.rs:1125-1158）核心换算：

  ```rust
  thumb_start = clamp(pos_along_axis - track_origin - thumb_offset, 0, viewport - thumb_size);
  percentage = thumb_start / (viewport_size - thumb_size);
  -max_offset * percentage
  ```
- 拖动中 `scroll_handle().drag_started()` / `drag_ended()` 通知 list（List 用它暂停 follow-tail）。

**启示**：滚动条是「装饰叠加 + 状态机」，它**不需要参与布局**（absolute 覆盖在滚动容器上，几何从 `max_offset/viewport` 现算）。我们引擎的滚动容器控件应提供 `with_scrollbar(...)` 的叠加装饰，`scroll_extent`（已有）+ 可读 offset（缺）就是它全部的输入。

---

## 6. 命中与滚动：`should_handle_scroll` vs `is_hovered`（关键语义）

`Hitbox` 有两条不同的命中判定（`crates/gpui/src/window.rs:855-947`）：

- `is_hovered`：**被 overlay 挡住就为 false**（hover 样式、click、drag 都基于它）。
- `should_handle_scroll`：**滚轮只看 `BlockMouse`，不看 `BlockMouseExceptScroll`**——overlay 挡 hover 不挡滚动。

`HitboxBehavior::{Normal, BlockMouse, BlockMouseExceptScroll}`：
- `BlockMouse`（`.occlude()`）：挡掉背后一切，包括滚动。
- `BlockMouseExceptScroll`（`.block_mouse_except_scroll()`）：挡掉 hover/click/drag，**放行滚动**。滚动条命中区用这个，保证拖动滚动条时 wheel 还能滚。

实现（旧分析 §5）：hit test 是**反向线性扫描** hitbox 表；遇到 `BlockMouseExceptScroll` 记录 `hover_hitbox_count` 截断 hover 部分、但**继续扫描**滚动 id；`should_handle_scroll` 用完整 id 列表，`is_hovered` 用截断列表。

**启示**：这个语义对我们的 overlay/popover 直接有用——弹出层应"挡交互不挡滚动"。我们目前 `scroll_target()` 沿祖先链找（view.rs:2627-2645），没有"滚动穿透 overlay"概念；需要在命中模型里区分"可交互层"与"可滚动层"。

---

## 7. 虚拟化列表：UniformList 与 List

两者都在 `crates/gpui/src/elements/`。

### 7.1 `UniformList`（等高，uniform_list.rs，936 行）

- 用法：`uniform_list(id, item_count, |range, window, cx| -> Vec<AnyElement>)`。**只实例化可见区间**的元素。
- `request_layout`：**只测一个代表 item**（`measure_item(None)`，`uniform_list.rs:283`），`Infer` 模式下总高 = `item_height * item_count`（有界时 clamp 到可用高）。
- `prepaint`（uniform_list.rs:331-542）：计算 `first_visible_ix = floor(scroll_top / item_height)`、`last_visible_ix = ceil((scroll_top + viewport) / item_height)`，得到 `visible_range`，只对区间内 item `layout_as_root` + `prepaint_at`。
- **scroll_to_item**（延迟到 prepaint）：`ScrollStrategy::{Top, Center, Bottom, Nearest}`（uniform_list.rs:84-99）；strict 版本强制重定位，非 strict 只在不可见时才滚最小量。`scroll_to_item_with_offset(ix, strategy, offset_items)` 缩小有效视口。
- `UniformListScrollHandle` = `base_handle: ScrollHandle` + `deferred_scroll_to_item` + `last_item_size`（uniform_list.rs:115-122），`scroll_to_bottom()` 就是 `scroll_to_item(usize::MAX, Bottom)`。
- `UniformListDecoration` trait（uniform_list.rs:583-596）：**滚动条就是作为 decoration 挂进 list** 的（`ScrollbarStateWrapper::compute`，scrollbar.rs:271-288，滚动条 origin 取 `-scroll_offset`）。

### 7.2 `List`（变高，list.rs，3638 行）

- 核心数据结构：`SumTree<ListItem>`（Zed 自己的可持久化平衡树，支持 `Count`/`Height` 维度的 `cursor.seek`/`Bias`）。
- `ListState(Rc<RefCell<StateInner>>)` 由 view 持有：`items: SumTree` + `scroll_anchor: Option<ScrollAnchor>` + `alignment: ListAlignment` + `overdraw` + `scrollbar_drag_start_height`。
- **滚动位置是逻辑偏移** `ListOffset { item_ix, offset_in_item: Pixels }`（list.rs:926-948），不是纯像素——内容变化（splice/remeasure）时锚定 item 保证位置稳定。
- **锚定/重测**：`remeasure` 保持**比例**位置（`ScrollAnchorMode::Proportional`），`remeasure_items` 保持**绝对**位置；`splice` 时锚点周围的重测增量更新（list.rs:415-437）。
- 虚拟化：`visible_range` 用 cursor 沿 `Height` 维度从锚点二分（list.rs:855-865）。
- follow-tail（聊天日志）：`FollowMode::Tail`，滚到底自动跟随新内容，用户上滚即退出（list.rs:97-146, 902-904）。
- **滚动条拖拽时的高度锚定**：`scrollbar_drag_start_height` 在拖动期间固定，防止列表高度随测量变化导致 thumb 跳走（list.rs:759-773）。

**启示**：等高的 `UniformList` 是我们 MD3 `carousel` 之外最可能先用的形态（导航、列表、设置页）。它的最小实现只需要：一个代表 item 的测量 + 可见区间计算 + `ScrollStrategy`。变高 `List` 的 SumTree 对我们的布局引擎太重——但如果引入"逻辑偏移 + splice 锚定"，数据层可以先用简单 Vec + 每项高度表，再考虑平衡树。

---

## 8. 惯性滚动与弹性

### 8.1 惯性只在触摸层，滚轮没有

- 惯性是**手势识别器**（`crates/gpui/src/gestures.rs`）的一部分，只服务触摸 pan。
- `TouchGestureState::Pending → Panning → (release) → Momentum`（gestures.rs:519-594）。触摸移动超过 `touch_slop`（8px）才成为 pan；**锁轴**：pan 启动时取主导轴，后续 delta 锁在该轴。
- **velocity 估计**：`VelocityTracker`（gestures.rs:1084-1151）对最近样本做**二次最小二乘拟合**取导数（Flutter/Android 同款），而不是端点差（端点差会把释放时仍在加速的 flick 估成一半速度）。
- 释放时 `speed >= min_fling_velocity`（默认 50 px/s）才启动 `Momentum`，否则直接停。
- **`ScrollPhysics`**（gestures.rs:147-247）只管减速，不管边界：

  ```rust
  pub enum ScrollPhysics {
      Exponential { decay_per_ms: f32 },       // iOS UIScrollView：0.998/ms
      FrictionSpline { friction: f32, pixels_per_inch: f32 },  // Android OverScroller（AOSP 转写）
  }
  ```
  闭式求值 `fling_distance(speed, elapsed)`（指数积分 / AOSP SplineOverScroller 的 100 点采样表），轨迹与帧率无关。
- **momentum 每帧 tick**：`window.schedule_touch_momentum_tick()`（window.rs:5888, 5965-5970）在 `on_next_frame` 里 `tick_momentum()`，产出带 `TouchPhase::Moved/Ended` 的合成 `ScrollEvent`，**从 pan 起点坐标**派发——所以惯性滚动仍然落在启动手势的容器上。
- 注释原文（gestures.rs:143-145）是设计声明："This models deceleration only. **Boundary behavior — bouncing, edge glow, clamping — is the scroll container's policy.**"

### 8.2 弹性/回弹：GPUI 没有

本地 0.2.2 和当前 zed 主线都没有 bounce。`ScrollEvent` 处理只是 `offset += delta`，clamp 在 prepaint 硬截断到 `[-max, 0]`。**没有 overscroll 弹簧、没有 edge glow、没有 rubber-banding**。iOS/Android 的手感只存在于触摸 pan 的物理参数里，容器侧无弹性。

**启示**：
- 惯性若要上，参考 GPUI 的分层是对的：**物理（ScrollPhysics，闭式可测）在输入层，边界策略（clamp/bounce）在容器层**。我们引擎可以先把 `Event::Scroll` 升级成带 `TouchPhase`，让惯性 tick 作为合成 scroll 事件流走同一通道。
- 不做惯性也能把「滚轮/轨道板」做完整。bounce 是纯加分项，MD3 的滚动容器规范里也没有强制。

---

## 9. 鼠标拖拽滚动（drag to scroll）：没有

grep 整个 zed 仓库，**没有**"按住鼠标拖动内容"的滚动实现。滚动输入只有：
- 滚轮 / 触控板（`ScrollSource::Pointer`）
- 触摸 pan（`ScrollSource::Touch`）
- 滚动条 thumb 拖拽（`crates/ui`）

GPUI 的 `MouseMoveEvent` 有 `dragging()`（`interactive.rs:515-517`，左键按住）——但那服务于拖放/选择，不驱动滚动。

**启示**：若我们的 MD3 需要触屏/拖拽滚动（如触屏设备上的列表），这是一个**我们自己的增量**，GPUI 无先例可抄。触屏可以用 gestures.rs 的 pan 思路（slop → 锁轴 → velocity），鼠标拖拽则是另一套（左键按住 + 移动即滚，常见于 Figma/画布类）。不建议做鼠标拖拽滚动进通用容器——会与文本选择、控件拖拽冲突，MD3 桌面也没有这个交互。

---

## 10. 对我们引擎的落地建议（按依赖顺序）

现状（engine-gaps.md §2、§18 + 2026-10-31 对账）：`Clip::scrolls()`、`scroll_x/y` 偏移、`scroll_extent` 已落盘；`Event::Scroll { pos, delta: f32 }` → `scroll_target()` → `Change::Value` 已接通。**缺的是**：统一滚动容器控件抽象、滚动输入处理（轴分流/锁轴/行 vs 像素）、滚动条、惯性、可读滚动偏移。

按 GPUI 的分层逐个落地：

1. **升级滚动事件为轴化**：`Event::Scroll { pos, delta: Vec2 }`（至少 x/y 两个标量），保留 pos。滚轮行距换算放引擎（对齐 GPUI 的 `pixel_delta(line_height)`）。这是后面一切的地基。
2. **暴露可读滚动状态**：在 `Routing`/`Cx` 加 `id → (scroll_extent, offset)` 表（engine-gaps §2 已点名）。GPUI 的 `ScrollHandle(Rc<RefCell>)` 是"用户持有"方案，我们已有的 `Cx` 按 id 查询更贴合现状。这一步解锁滚动条、app-bar 联动、carousel 吸附。
3. **统一滚动容器控件**：`with_scroll(axis)` 声明式装饰，内部处理 Scroll 事件 → clamp → 更新 offset。轴分流规则照抄 GPUI（`div.rs:3413-3430`：单向轴接收双向 delta；`restrict_scroll_to_axis` 锁轴）。
4. **滚动条作为叠加装饰**：几何现算（`visible_percentage = viewport/(viewport+extent)`，thumb ≥ 25px），显示策略 `Auto/System/Always/Never` + reveal policy，1s 后淡出。照 `ReservedSpace` 决定是否占位。命中区需要"挡 hover 不挡滚动"语义（见 6）。
5. **ScrollStrategy 统一跳转**：`scroll_to_item(ix, Top/Center/Bottom/Nearest)` 延迟到 prepaint 执行——GPUI 把"请求"和"执行"分开是因为执行需要 layout 后的 bounds；我们同样在 layout 后消费。
6. **虚拟化（等高优先）**：`UniformList` 的最小形态 = 代表 item 测量 + visible_range + `render_items(Range)`。等 MD3 需要长列表再上，List 的 SumTree 不急着抄。
7. **惯性（可选，最后）**：物理闭式（`Exponential{decay_per_ms:0.998}` 或 Android friction spline）放输入层，容器只负责 clamp；不实现 bounce。

---

## 参考

- GPUI 0.2.2 源码：`/home/ollie/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/crates/`
  - `gpui/src/elements/div.rs`（scroll API / ScrollHandle / clamp / paint_scroll_listener）
  - `gpui/src/gestures.rs`（OngoingScroll / ScrollPhysics / VelocityTracker / momentum）
  - `gpui/src/interactive.rs`（ScrollEvent / ScrollDelta / ScrollSource）
  - `gpui/src/elements/uniform_list.rs`、`elements/list.rs`（虚拟化）
  - `gpui/src/window.rs`（HitboxBehavior / should_handle_scroll / momentum tick / element_state map）
  - `ui/src/components/scrollbar.rs`（Zed 滚动条，1914 行）
- 旧分析：`docs/research/gpui-0.2.2-analysis.md` §4（layout_bounds + scroll offset）、§5（命中语义）、§10（跨帧存储）
- DeepWiki 查证（当前 zed 主线无 `ScrollableAxis`/`Scrollable` trait）：https://deepwiki.com/zed-industries/zed
- 第三方 `gpui-component` 的 `ScrollableAxis`（已废弃改 `Axis`）：[PR #509](https://proxg.dev/librekeys/gpui-component/commit/11dbdb125ad89d9516d9d83370573695e59ac8ef)
