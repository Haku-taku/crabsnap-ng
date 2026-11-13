# egui 0.36.2 — ScrollArea 滚动容器调研

Sources: `/home/ollie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src/`。
路径相对各 crate 的 `src/`，每个结论都标注我读过的行号。配套实现 `egui_extras`（虚拟化 Table）单独一节。

> 本文是「统一滚动容器抽象」（`docs/research/scroll-design-notes.md` 第 3 项）的 egui 侧调研。
> 结合 `docs/research/gpui-scroll.md`、`docs/research/qt-scroll.md` 一起定稿。

---

## 1. `ScrollArea` API：链式注册，两套方向

`containers/scroll_area.rs:336-364` 是结构体。构造器按方向分四种：

```rust
// scroll_area.rs:366-394
pub fn horizontal() -> Self { Self::new([true, false]) }
pub fn vertical()   -> Self { Self::new([false, true]) }
pub fn both()       -> Self { Self::new([true, true]) }
pub fn neither()    -> Self { Self::new([false, false]) }
pub fn new(direction_enabled: impl Into<Vec2b>) -> Self
```

`direction_enabled: Vec2b`（`[x, y]` 两个 bool）。**关键设计：每一轴是独立的，垂直/水平行为完全对称**——
`[true, false]` 就是横向滚动。这不是两个方向的叠加，而是同一份逐轴逻辑跑两遍。

**字段全在 `ScrollArea` struct（`scroll_area.rs:336-364`），全部是配置，状态不在这：**

```rust
direction_enabled: Vec2b,       // 哪轴可滚
auto_shrink: Vec2b,             // 内容小则收缩容器（默认 true）
max_size: Vec2,                 // 外框上限（默认 INFINITY）
min_scrolled_size: Vec2,        // 需要滚动条的最小尺寸（默认 64.0）
scroll_bar_visibility: ScrollBarVisibility,
scroll_bar_rect: Option<Rect>,  // 滚动条可限定在更小矩形内（sticky header 场景）
id_salt: Option<IdSalt>,        // 用户提供的 id 盐
offset_x/offset_y: Option<f32>, // 一次性设定初值（None = 沿用 state）
on_hover_cursor/on_drag_cursor: Option<CursorIcon>,
scroll_source: ScrollSource,
wheel_scroll_multiplier: Vec2,  // 滚轮倍率（默认 1.0；可反/可归零）
content_margin: Option<Margin>,
stick_to_end: Vec2b,            // 粘底/粘右
animated: bool,                 // scroll_to_* 是否动画（默认 true）
```

**每个 setter 都是 `mut self -> Self` 的链式 builder**（`max_height` 在 `scroll_area.rs:432`，
`auto_shrink` 在 `605`，`stick_to_bottom` 在 `655`）。`#[must_use = "You should call .show()"]`
（`337`）防住只配置不 show 的用法。

关键配置参数与默认值：

| 参数 | 默认 | 说明 |
|---|---|---|
| `vertical()/horizontal()/both()` | — | 方向（`new([x,y])`） |
| `max_height(f32)` / `max_width` | `INFINITY` | 外框上限；默认铺满可用区 |
| `min_scrolled_height/width` | `64.0` | 需要滚动条时最小尺寸；防止嵌套滚动区塌成零（见 §3） |
| `auto_shrink(bool/[b,b])` | `true` | 内容小于容器时：true 把空白放容器**外**，false 把空白放容器**内** |
| `scroll_bar_visibility(...)` | `VisibleWhenNeeded` | 滚动条显示策略（§2.1） |
| `scroll_bar_rect(rect)` | 整个 inner | 滚动条限位矩形 |
| `id_salt(...)` | `"scroll_area"` | id 盐（§5） |
| `vertical/horizontal_scroll_offset(f32)` | state 值 | 一次性覆盖偏移 |
| `on_hover_cursor/on_drag_cursor` | None | 拖拽滚动时的光标 |
| `hscroll/vscroll/scroll(...)` | 按构造器 | 后调覆盖方向 |
| `scroll_source(ScrollSource)` | 见 §4 | 滚动来源（滚轮/滚动条/拖拽）三开关 |
| `wheel_scroll_multiplier(Vec2)` | `1.0` | 逐轴滚轮倍率，可反/可禁用 |
| `content_margin(margin)` | 样式值 | 内容边距 |
| `animated(bool)` | `true` | 编程滚动动画开关 |
| `stick_to_right/bottom(bool)` | `false` | 粘到末端（日志/终端场景） |

**显示闭包 `show`，以及它的三个变体：**

```rust
// scroll_area.rs:959-1056
pub fn show<R>(self, ui, add_contents: impl FnOnce(&mut Ui) -> R) -> ScrollAreaOutput<R>
pub fn show_rows<R>(self, ui, row_height_sans_spacing: f32, total_rows: usize,
                    add_contents: impl FnOnce(&mut Ui, Range<usize>) -> R)  // 虚拟化
pub fn show_viewport<R>(self, ui, add_contents: impl FnOnce(&mut Ui, Rect) -> R) // 拿到相对视口
```

返回的 `ScrollAreaOutput<R>`（`scroll_area.rs:89-105`）是四元组：`inner`（闭包返回值）、
`id`、`state`（`State`，见 §5）、`content_size`、`inner_rect`。**`content_size > inner_rect` 就是
"需要滚动"的判据。**

---

## 2. 滚动条

### 2.1 显示策略 `ScrollBarVisibility`

`scroll_area.rs:107-134`：

```rust
pub enum ScrollBarVisibility {
    AlwaysHidden,      // 永远隐藏；仍可滚轮/拖拽，但无视觉反馈
    VisibleWhenNeeded, // 内容超出容器才显示（默认）
    AlwaysVisible,     // 始终显示
}
```

注意**没有 "Auto"**——egui 的三态是 Hidden / WhenNeeded / Visible。而且它的判定有个一层延迟
（frame-delay）：显示态用**上一帧**的 `state.show_scroll`（`scroll_area.rs:747`），而 `end()` 里
`content_is_too_large` 算出后，若这一帧刚需要显示，会用 `animate_bool_responsive` 立刻拉高
`show_bars_factor` 来避免卡一帧（`scroll_area.rs:1262-1268`）。

### 2.2 外观配置 `ScrollStyle`

不是 per-ScrollArea 参数，而是**全局样式** `Spacing::scroll`（`style.rs:465`），三档预设：

| 预设 | floating | bar_width | 说明 |
|---|---|---|---|
| `ScrollStyle::solid()` | false | 6.0 | 占空间的实心条，内容区被压缩 |
| `ScrollStyle::thin()` | true | 10.0 | 浮动条，dormant 时 1.0 宽、hover 展开 |
| `ScrollStyle::floating()`（默认）| true | 10.0 | 默认；不占空间，漂浮在内容上，不 hover 时透明 |

```rust
// style.rs:495-583
pub struct ScrollStyle {
    pub floating: bool,               // 浮动(盖内容) 还是 实心(占空间)
    pub content_margin: Margin,
    pub bar_width: f32,               // 条最宽时的宽度（实心=6，浮动=10）
    pub handle_min_length: f32,       // 把手最小长度（12.0）
    pub bar_inner_margin: f32,        // 内容与条之间
    pub bar_outer_margin: f32,
    pub floating_width: f32,          // 浮动条 dormant 宽度（2.0）
    pub floating_allocated_width: f32,// 浮动条占空间宽度（默认 0.0）
    pub foreground_color: bool,
    pub dormant/active/interact_*_opacity: f32,  // 背景与把手的 3 态透明度
    pub fade: ScrollFadeStyle,        // 边缘淡出提示
}
```

配色/透明度分 **dormant / active / interact** 三态（hover 滚动区 / hover 滚动条 / 按下拖动），
用 `animate_bool_responsive(id.with((d, "bar_hover")))` 做平滑过渡（`scroll_area.rs:1343-1345`）。

**`allocated_width()`**（`style.rs:656-662`）是尺寸协商关键：非 floating 时占 `bar_inner_margin +
bar_width + bar_outer_margin`；floating 时占 `floating_allocated_width`（默认 0）。

还有 `ScrollFadeStyle { strength: f32, size: f32 }`（`style.rs:784-802`，默认 strength 0.5 / size 20.0）：
在内容溢出侧画**渐隐蒙层**（`paint_fade_areas_impl`，`scroll_area.rs:1564-1641`），四边各自独立，
顶/左按 `offset` 接近程度计算透明度，底/右按 `distance_from_bottom = overflow - offset`。

### 2.3 交互：点击轨道、拖动把手

滚动条也是普通 widget：`ui.interact(max_bar_rect, interact_id, sense)`，`sense = CLICK | DRAG`
（可滚动时，`scroll_area.rs:1324-1334`）。关键细节：

1. **命中区是全宽 `max_bar_rect`**，即使动画中条还没展开到全宽——"为命中测试提供一致的
   目标，避免细 widget 启发式抖动"（`1330-1333`）。
2. **点击轨道 = 跳到把手中心对准指针**：`scroll_start_offset_from_top_left` 记录"指针相对
   把手顶部的偏移"。若按下位置不在把手上，`new_handle_top_pos = pointer_pos - handle_size/2`
   （`scroll_area.rs:1405-1418`），即轨道点击把把手中心移到指针处。
3. **拖动把手 = 保持抓取偏移**：`new_handle_top = pointer_pos[d] - scroll_start_offset`，
   再用 `remap(new_handle_top, handle_travel, 0..=max_offset)` 反算 offset（`1420-1427`）。
4. **把手尺寸 = 视口/内容比例映射**（`calculate_handle_rect`，`1374-1399`）：
   ```rust
   handle_size = from_content(offset + inner_size) - from_content(offset)
                 .max(scroll_style.handle_min_length);   // 最小长度钳制
   ```
   `from_content` 把 `0..=content_size[d]` 线性映射到滚动条矩形。
5. 拖动后清 `scroll_stuck_to_end` 和 `offset_target`（`1430-1431`）——任何手动动作取消粘底和动画。

---

## 3. 内容 measure：viewport 固定，闭包自由布局

这是整个设计里最值得抄的部分。egui 的布局是"内容向上生长"的流程式布局，滚动区要解决的
核心矛盾是：**闭包里的内容想长多高长多高，但视口要固定裁剪**。

`begin()`（`scroll_area.rs:711-954`）分两步：

1. **先定 outer / inner 尺寸**（`763-782`）：
   ```rust
   let available_outer = ui.available_rect_before_wrap();
   let outer_size = available_outer.size().at_most(max_size);   // 外框 = 可用区 ∩ max_size
   let inner_size = outer_size - current_bar_use;               // 减滚动条占位
   // 每轴：direction_enabled 时不低于 min_scrolled_size，防嵌套塌零（github#1097）
   ```

2. **给内容子 Ui 设 max_rect**，内容布局从偏移后的原点开始：
   ```rust
   let content_max_rect = Rect::from_min_size(inner_rect.min - state.offset, content_max_size);
   // 对齐到像素，避免滚动非整数点时 widget "漂浮"
   let content_max_rect = content_max_rect.round_to_pixels(ppi).round_ui();
   let mut content_ui = ui.new_child(UiBuilder::new().max_rect(content_max_rect));
   ```

   有一个被 `if true { } else { }` 包住的决策点（`786-796`）：当前实现让 inner Ui 的
   `content_max_size = inner_size`，即**尽量让内容适合视口（换行/缩图而不是出横滚动条）**；
   被注释掉的 else 分支才是"沿可滚轴给 INFINITY"（内容随便长）。这个 `if true` 是死分支，
   注释说明了取舍：宁可换行也别出横滚动条。

3. **clip**（`811-826`）：可滚轴夹到 `inner_rect`，不可滚轴跟随父 clip 减滚动条占位，
   最后 `intersect(ui.clip_rect())` 保证不放大父裁剪。

**内容布局时内容 Ui 不知道自己在滚动**——它只是从一个负偏移的原点开始、在一个限制宽度内
自由生长。滚动偏移 `state.offset` 改变的是 **max_rect 的原点**，不是布局结果。布局结束
`content_ui.min_size()` 就是 `content_size`（`1082`）。

`end()`（`1060-1559`）里完成"后处理"：

- **inner 尺寸四象限表**（`1183-1190`），这是 `auto_shrink` 的语义核心：
  ```rust
  match (direction_enabled[d], auto_shrink[d]) {
      (true,  true)  => inner_size[d].min(content_size[d]), // 内容小则收缩滚动区
      (true,  false) => inner_size[d],                      // 滚动区保持大，空白在内部
      (false, true)  => content_size[d],                    // 不可滚轴跟随内容伸缩
      (false, false) => inner_size[d].max(content_size[d]),  // 不可滚轴撑到内容
  }
  ```
  对不可滚的轴，`auto_shrink=true` 时滚动区"跟随内容"（像普通 Ui 一样包裹内容）——
  **一个滚动容器只在一根轴上滚，另一根轴仍然是普通盒模型**。
- **`content_is_too_large`**（`1203-1206`）：`inner_rect 尺寸 < content_size`（逐轴，取整比较）。
- **`max_offset = content_size - inner_rect.size()`**（`1208`）——offset 的钳位上界。
- **最终 `outer_rect = inner_rect + current_bar_use`**，`ui.advance_cursor_after_rect(outer_rect)`
  让后续 widget 在滚动区**下面**继续排（`1273`）。

`viewport` 字段是 `Rect::from_min_size(Pos2::ZERO + state.offset, inner_size)`（`828`）——
传给 `show_viewport` 闭包的相对视口，供上层做懒渲染。

---

## 4. 输入：滚轮、拖拽滚动、惯性

### 4.1 滚动来源三开关 `ScrollSource`

`scroll_area.rs:189-303`。这是 0.36 的"可配置输入来源"设计：

```rust
pub struct ScrollSource { scroll_bar: bool, drag: DragScroll, mouse_wheel: bool }
pub enum DragScroll { Never, OnTouch /*默认*/, Always }
```

预设常量：`NONE` / `ALL` / `SCROLL_BAR` / `DRAG` / `MOUSE_WHEEL`，且实现了 `BitOr`（`|` 取更
宽松者），所以可以 `ScrollSource::SCROLL_BAR | ScrollSource::DRAG` 拼装。这是把
`drag_to_scroll` 从 bool 升级成"来源位掩码"的演进：**滚轮、滚动条、拖拽三个来源可独立开关**。

### 4.2 滚轮输入：全局平滑 + 命中区判定 + 消费

滚轮 delta 不直接进 `ScrollArea`，而是**先在输入层平滑**：

- `WheelState`（`input_state/wheel_state.rs:39-68`）维护三态 `Status::{Static, Smoothing, InTouch}`
  和两个缓冲 `unprocessed_wheel_delta` / `smooth_wheel_delta`。离散滚轮（鼠标）进
  `unprocessed`，触摸板（有 Start/End phase）进 `smooth`（`on_wheel_event`，`84-159`）。
- 每帧 `after_events`（`161-191`）：`unprocessed` 按指数平滑因子 `exponential_smooth_factor(0.90, 0.1, dt)`
  稀释进 `smooth`；平滑完成/超时（150ms 无事件）回到 `Static`。**这段平滑统一了"咔嗒鼠标滚轮"
  和"顺滑触摸板"两种体验**，`ScrollArea` 只读 `input.smooth_scroll_delta`。
- Shift/Alt 修饰键把滚动轴翻转（`wheel_state.rs:120-133`；`InputOptions::horizontal_scroll_modifier
  = SHIFT`，`input_state/mod.rs:117`）。

`ScrollArea` 消费（`scroll_area.rs:1219-1254`）：

```rust
let is_hovering_outer_rect = ui.rect_contains_pointer(outer_rect)
    && ui.ctx().dragged_id().is_none()
    || is_dragging_background;
if scroll_source.mouse_wheel && ui.is_enabled() && is_hovering_outer_rect {
    let scroll_delta = input.smooth_scroll_delta()[d] * wheel_scroll_multiplier[d];
    // 钳制：只在 offset 在 0..max_offset 内、方向对才消费
    state.offset[d] -= scroll_delta;
    // 消费后清零，防止父滚动区再用同一 delta：
    input.smooth_scroll_delta[d] = 0.0;
}
```

**"消费即清零"是关键不变量**：`smooth_scroll_delta` 是全局的，谁滚了谁清零，嵌套滚动区
（外层横向、内层纵向）才能各取所需、互不叠加。还有个 `always_scroll_the_only_direction`
样式开关（`style.rs:335`）：单向滚动区里横竖 delta 合并给那一轴，不用按 Shift 就能横滚。

### 4.3 拖拽滚动 + 惯性

`scroll_area.rs:831-898`。三点：

1. **必须在内容之前 interact**，否则抢走子 widget 的输入：
   ```rust
   let content_response = state.interact_rect   // 上一帧记下的内容区
       .map(|rect| ui.interact(rect, id.with("area"), Sense::DRAG));
   ```
   用**上一帧**的 `state.interact_rect`（`State` 里存的，见 §5）——这一帧的内容 rect 还没算出来。
   只有 `content_is_too_large.any()` 时才注册 drag（`833`）。
2. **拖动 = offset 直接减 pointer delta**（`845-854`），同时清 `scroll_stuck_to_end` 和
   `offset_target`。release 时把 `input.pointer.velocity()` 存入 `state.vel`（`857-863`）。
3. **惯性（kinetic scrolling）**（`864-879`）——一个非常朴素、无物理引擎的实现：
   ```rust
   let stop_speed = 20.0;          // px/s 以下停止
   let friction_coeff = 1000.0;    // px/s²
   let friction = friction_coeff * dt;
   if friction > state.vel[d].abs() || state.vel[d].abs() < stop_speed {
       state.vel[d] = 0.0;
   } else {
       state.vel[d] -= friction * state.vel[d].signum();  // 每帧减固定摩擦
       state.offset[d] -= state.vel[d] * dt;              // 负号：坐标系相反
       ctx.request_repaint();                             // 直到停
   }
   ```
   即**恒定摩擦减速**（每帧减 1000·dt），停速阈 20px/s。速度来自 `pointer.velocity()`：
   `InputState` 用 `pos_history`（带时间戳的位置环）最小二乘拟合速度（`input_state/mod.rs:1234`），
   触摸释放时**不清历史**（`1226-1228`，"恰恰是手指离屏时我们想把速度甩给内容"）。

`DragScroll::OnTouch` 的判定是 `ctx.input(|i| i.has_touch_screen())`——只要收到过任何 touch
事件就算（`input_state/mod.rs:845-847`），实现极简。

### 4.4 编程滚动：`scroll_to_*` / `scroll_with_delta`

`ScrollArea` 不直接暴露"滚到哪"，而是走**全局 per-frame 请求通道** `PassState`：

```rust
// pass_state.rs:212-224
pub scroll_target: [Option<ScrollTarget>; 2],
pub scroll_delta: (Vec2, ScrollAnimation),
```

- `Ui::scroll_to_rect(rect, align)`（`ui.rs:1400-1419`）：把 rect 的每轴 `Rangef` 放进
  `state.scroll_target[d]`。闭包里任何地方都能调，`ScrollArea::end()` 在 `1086-1168` 读出来换算。
- `Response::scroll_to_me(align)`（`response.rs:842`）是 `scroll_to_rect(self.rect)` 的糖。
- `Ui::scroll_with_delta(delta)`（`ui.rs:1444-1451`）：累加进 `state.scroll_delta.0`，同帧多次
  调用求和。

`end()` 里的换算（`1105-1140`）处理三种对齐：`align=Some` 时按 `center_factor` 让目标居中；
无 align 时只滚到目标刚好进入 clip（`start < clip_start` 就上滚、`end > clip_end` 就下滚），
已是可见范围则 delta 为 0。动画用 `ScrollAnimation`（`style.rs:827-855`）：
`points_per_second`（默认 1000）+ `duration: Rangef(0.1, 0.3)`——**动画时长 = 距离/速度，
再钳到 [0.1, 0.3]s**，`ScrollAnimation::none()` 即瞬时。

---

## 5. `State`：状态存什么，id 怎么管

`State`（`scroll_area.rs:23-72`）：

```rust
pub struct State {
    pub offset: Vec2,                                   // 滚动偏移（正 = 下/右）
    offset_target: [Option<ScrollingToTarget>; 2],      // 动画目标 + 时间窗
    show_scroll: Vec2b,                                 // 上一帧滚动条可见性
    content_is_too_large: Vec2b,                        // 上一帧内容是否溢出
    scroll_bar_interaction: Vec2b,                      // 上一帧是否在弄滚动条
    vel: Vec2,                                          // 惯性速度（serde skip）
    scroll_start_offset_from_top_left: [Option<f32>; 2],// 拖动把手时的抓取偏移
    scroll_stuck_to_end: Vec2b,                         // 粘底状态
    interact_rect: Option<Rect>,                        // 上一帧内容 rect（拖拽用）
}
```

**id 管理**（`begin()`，`733-740`）：

```rust
let id_salt = id_salt.unwrap_or_else(|| IdSalt::new("scroll_area"));
let id = ui.make_persistent_id(id_salt);
ctx.check_for_id_clash(id, ...);  // debug 下报 id 冲突
let mut state = State::load(&ctx, id).unwrap_or_default();
```

默认盐是字面量 `"scroll_area"`——所以同一屏两个滚动区**必须**给 `id_salt`，否则 id 冲突。
状态经 `ctx.data`（`State::load/store`，`74-81`）以 id 持久化，跨帧/跨窗口存活（serde 可选）。
子 id：滚动条交互 `id.with(d)`、hover 动画 `id.with((d, "bar_hover"))`、拖拽区 `id.with("area")`
——一个滚动区派生出 5 个左右的子 id，各管各的动画/交互状态。

注意**没有 "drag to scroll" 标志在 State 里**：`DragScroll` 是配置（编译期形态），
`vel`/`interact_rect` 才是运行态。

---

## 6. 虚拟化：没有内建，但给了两件工具

**egui 核心没有真正的虚拟化**——`ScrollArea` 每帧让整个内容布局、绘制，只靠 clip 裁掉不可见部分。
但它提供了两件工具让上层自建：

1. **`show_rows`**（`scroll_area.rs:983-1014`）——**定高行**的懒渲染：
   ```rust
   let row_height_with_spacing = row_height_sans_spacing + spacing.y;
   ui.set_height((row_height_with_spacing * total_rows as f32 - spacing.y).at_least(0.0));
   let mut min_row = (viewport.min.y / row_height_with_spacing).floor() as usize;
   let mut max_row = (viewport.max.y / row_height_with_spacing).ceil() as usize + 1;
   // 溢出时反向补偿，保证"可见行数"恒定（窗口在末尾时行为稳定）
   if max_row > total_rows {
       let diff = max_row.saturating_sub(min_row);
       max_row = total_rows; min_row = total_rows.saturating_sub(diff);
   }
   ```
   手法：**先 `set_height` 告诉布局"总高度是 N 行"**，再只让可见行布局；为了 ID 稳定，
   子 Ui 用 `skip_ahead_auto_ids(min_row)` 跳过前 min_row 个自动 id（`1009`）。

2. **`show_viewport`**（`1020-1026`）——把相对视口 rect 传给闭包，闭包自己决定画什么。

真正的虚拟化在 `egui_extras` 的 `Table::rows`（`egui_extras-0.36.2/src/table.rs:1029-1085`）：
行高固定时，`min_row = (scroll_offset_y / row_height_with_spacing).floor()`，
`max_row = (scroll_offset_y + max_height) / row_height_with_spacing + 1`，不可见区用
`add_buffer(高度)` 占位（`1051`、`1079`），总高度保持为 `total_rows * row_height`。
`heterogeneous_rows`（`1099-1163`）支持变高行：先累加行高找到起止，但依然只有固定行高
版本做"跳过"，变高版本 `scroll_offset_y` 不够精确时全渲染。

**结论：egui 应对大列表的方式是"定高行 + 懒渲染 + 固定总高占位"**，数据侧由 `show_rows`/
`Table::rows` 做窗口化；`ScrollArea` 本身不区分"内容是否存在"，它只处理"偏移 + 裁剪"。

---

## 7. 对我们引擎的启示（对照 `scroll-design-notes.md` 缺口）

我们已有（已核实）：`Overflow::Scroll` clip + `scroll_x/y` 偏移（`tree.rs:252-259`）、
`scroll_extent` 落盘（`layout_engine.rs:1274`）、`Event::Scroll{pos,delta}`（`event.rs:101`）、
`scroll_target()` 命中路由（`view.rs:2627-2644`）、`Change::Value(-delta)` 上报
（`view.rs:2339-2351`）、`Sense::SCROLL`/`DRAG`。缺统一滚动容器、滚动条、拖拽、惯性。

### 7.1 直接可借鉴的

1. **逐轴对称性**：`Vec2b` 方向 + 每轴跑同一份逻辑，比"纵向为主、横轴特殊"干净。
   我们 `scroll_x/scroll_y` 已是对称字段，抽象应对齐。

2. **inner 四象限表**（§3 的表）是 `auto_shrink` 的精确语义。我们的布局引擎是 CSS 子集重写，
   CSS 里对应 `overflow: auto` 时主轴副轴的 shrink 行为——值得写成同款四象限。

3. **"offset 改原点，不改布局"**：egui 用负偏移原点建子 Ui + 固定 clip。我们的
   `layout_engine.rs:1287` 已经是 `origin - clamp(scroll_x,...)` 的同类做法，说明方向对。

4. **消费即清零**（§4.2）：滚轮 delta 是全局的，谁滚谁清。我们 `scroll_target()` 已把事件
   路由到唯一节点，天然单消费；但若做嵌套滚动区，这个不变量要显式保持。

5. **上一帧 rect 用于本帧命中**（`state.interact_rect`）：拖拽区、滚动条命中都在内容布局
   **之前**发生，只能信上一帧的 rect。我们的 immediate 引擎同样面临"闭包还没跑就知道
   rect"的时序，这是可复制的解法。

6. **滚动条也是普通 widget**：全宽命中区 + `Sense::CLICK|DRAG`，轨道点击把手居中、
   拖动保持抓取偏移、把手长度 = 视口/内容比例且最小长度钳制——这套几何换算与
   `scroll_extent` 完全对得上，直接照搬几何即可。

7. **惯性 = 恒定摩擦**（§4.3）：没有物理引擎，`vel -= friction*sign(vel)`，两常量可调。
   对我们这种控件库是足够、且容易做成可选项的。速度来源需要"指针位置历史 + 时间戳"，
   我们现在 `drag_step` 是逐帧位移，需要补一个带时间的环形缓冲。

8. **滚轮平滑层**（§4.2）：鼠标咔嗒轮 vs 触摸板是两类输入，统一平滑后再给滚动区。
   这在输入层做（`WheelState`），滚动区只消费 `smooth_delta`。我们 `Event::Scroll{delta: f32}`
   目前是单值，没有平滑层——大列表或触摸板场景会暴露。

### 7.2 需要权衡、不必照搬的

9. **滚动条三态 + 渐变淡入**：`ScrollBarVisibility::{AlwaysHidden, VisibleWhenNeeded, AlwaysVisible}`
   无 Auto 态。我们做 MD3 风格滚动条时可加"hover/拖动中显示"态，egui 的 floating 预设就是
   用 opacity 三态实现的——本质是 **显示策略 × 透明度三态**两个维度，别绑死。

10. **编程滚动走全局 per-frame 请求通道**（`PassState::scroll_target`）：egui 是 immediate
    mode，闭包随时能调 `scroll_to_me`，只能走全局暂存、`end()` 统一换算。我们若要在 decl
    阶段提供 `scroll_to(id, target)`，可以复制这个"请求暂存、布局后统一兑现"的通道；但
    我们是 retained 树 + change 事件，更自然的做法可能是控件自己持 target、在布局回调里兑现。

11. **`check_for_id_clash`**：我们 `id.rs` 是否有同款 debug 检测？多滚动区忘给 id 是真实
    事故源，值得保留这一防护。

12. **frame-delay 处理**：egui 滚动条显示有一帧延迟，靠 `animate_bool_responsive` 提前拉高
    补偿。我们如果滚动条可见性由控件内部 State 决定（`scroll-design-notes.md` 缺口 3），
    第一帧"先布局后画条"的顺序天然避免这一帧——设计上注意别引入同款卡帧。

### 7.3 与现有设计约束的对照

- `scroll-design-notes.md` 约束"复用 `scroll_target()`，控件在 `changed()` 收 `Value` 累加
  offset"：与 egui 的"滚动条直接改 offset、清动画目标"是同一行为，方向一致。
- 约束"offset 与滚动条位置必须由控件内部 State 持有并跨帧重读"：与 egui 的 `State`（§5）
  一致——egui 把 `offset/vel/stuck/上一帧 rect` 全放 state，按 id 持久化。
- 约束"滚动条需要视口/内容尺寸，decl() 拿不到"：egui 的解是滚动条在 `end()`（内容布局
  完成后）才画、用上一帧的 rect 做命中。我们控件在 `change()` 能拿到 `bounds`，但滚动条
  绘制若要逐帧正确，命中仍需上一帧 rect——这与 egui 的 `interact_rect` 是同一个答案。

### 7.4 我们缺、egui 也没有的（需自建）

- 真正的变量高虚拟化（egui 只有定高行窗口化）。
- 吸附 / 分页（carousel 现在手写 `carousel_max_scroll`，egui 无对应物）。
- 嵌套滚动区之间的手势竞争仲裁（egui 只有"消费即清零"）。

---

## 附：关键行号索引

| 主题 | 位置 |
|---|---|
| `ScrollArea` struct / builder | `containers/scroll_area.rs:336-659` |
| `begin()`（outer/inner/clip/视口/拖拽）| `containers/scroll_area.rs:711-954` |
| `end()`（内容尺寸/粘底/滚轮/滚动条绘制）| `containers/scroll_area.rs:1060-1559` |
| `ScrollBarVisibility` | `containers/scroll_area.rs:107-134` |
| `ScrollSource` / `DragScroll` | `containers/scroll_area.rs:144-303` |
| 惯性实现 | `containers/scroll_area.rs:864-879` |
| 滚动条命中/把手几何 | `containers/scroll_area.rs:1324-1434` |
| `State` / `ScrollAreaOutput` | `containers/scroll_area.rs:23-105` |
| `show_rows`（定高虚拟化）| `containers/scroll_area.rs:983-1014` |
| `ScrollStyle` 三预设 | `style.rs:495-663` |
| `ScrollAnimation` | `style.rs:827-855` |
| `WheelState` 滚轮平滑 | `input_state/wheel_state.rs:39-233` |
| `PassState::scroll_target/delta` | `pass_state.rs:212-224` |
| `Ui::scroll_to_rect/cursor/delta` | `ui.rs:1400-1451` |
| `Response::scroll_to_me` | `response.rs:842-847` |
| 指针速度（惯性来源）| `input_state/mod.rs:1226-1240` |
| 变高/定高虚拟化表 | `egui_extras-0.36.2/src/table.rs:1029-1163` |
