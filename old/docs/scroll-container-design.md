# 统一滚动容器抽象 — 设计方案

> 状态：已定稿（2026-10-31）。对应队列第 3 项。调研依据：
> `docs/research/{gpui,egui,qt}-scroll.md`，约束汇总见 `docs/research/scroll-design-notes.md`。

## 目标

把散落在 gallery nav 抽屉 / `List` / carousel 里的手管滚动收敛成一个统一滚动容器控件，
对齐主流框架（GPUI / egui / Qt）的滚动模型，同时解掉 `docs/engine-gaps.md` §2 的
「读不回滚动偏移」。

## 分层（照 GPUI：物理层与容器层分离）

```
输入层（引擎）         容器层（控件）            消费层（应用）
Event::Scroll         Scroll 容器              读 offset（Cx 查询）
  axis delta           收 Change::Value →       滚动条几何现算
  Lines/Pixels         累加 offset → clamp        app-bar 折叠 / carousel 吸附
```

## 实现顺序调整（2026-10-31）

GPUI 子代理建议的顺序是"轴化事件 → 可读状态 → 统一容器 → 滚动条"。但对照实际用例
重排：**当前所有滚动场景都是纵向滚轮**（nav 抽屉、`List`、carousel 用拖拽不走 Scroll 事件），
轴化事件是"为未来触控板/横滚准备"，不是当前痛点。因此先做价值最高的统一容器控件，
轴化事件作为可选步骤留后（见 §轴化）。

## 步骤 ① 统一滚动容器控件（ui-widgets `scroll.rs`）— 当前实现



## 步骤 ② 可读滚动状态（`Cx` 加滚动几何表）— 已完成（2026-10-31）

`engine-gaps.md` §2 点名：`Routing`/`Cx` 暴露 `id → (scroll_extent, offset)` 表。

实际实现比设计更完整——`Cx::scroll_of(id) -> Option<ScrollGeom>` 返回
`id/extent_w/extent_h/viewport_w/viewport_h/offset_x/offset_y/active`（viewport 让滚动条
一次读取即可现算 thumb，`active` 驱动自动滚动条显隐）。表由引擎在 view 前从上一帧树
收集（`view.rs` 遍历 `SCROLL_OWNED` 节点），`Dep::Scroll` 参与声明缓存比较。验证时序：
view 阶段读到的是一帧前的布局（egui 调研结论）。这是滚动条几何、app-bar 联动、
carousel 吸附的公共通道。

**2026-10-31 轴化（C 项）**：偏移表改为 `ScrollOffset { x, y }`，`Event::Scroll { pos, delta: Point }`
（`Event::wheel(pos, dy)` 供普通鼠标），wayland 后端映射 `HorizontalScroll`。滚轮按容器
`clip` 声明的那一轴累计（`scroll_owned_delta`），双轴容器两轴各自 clamp。写回同时设
`scroll_x`/`scroll_y`。**读写的第二扇门**：`Cx::set_scroll(id, x, y)` 让应用自己移动容器
（拖拽、翻页按钮），请求在 view 返回后合并进引擎偏移表，再于 apply 后写回树——应用
不必再持有第二份偏移。

## 步骤 ③ 滚动条（叠加装饰，`ScrollbarConf`）— 当前实现

照 GPUI（absolute 覆盖，不参与布局）+ egui（floating/solid）。

**2026-10-31 已实现**：

```rust
pub struct ScrollbarConf {
    pub visibility: ScrollbarVisibility,  // Auto(默认)/Always/Never
    pub style: ScrollbarStyle,            // Thin(floating, 默认)/Solid(占空间)
    pub thumb_min: f32,                   // 默认 25（GPUI）
    pub thumb: Color,                     // 包未提供角色时的默认色
    pub track: Color,                     // 透明 = 不画轨道
}

pub enum ScrollbarVisibility { Auto, Always, Never }
pub enum ScrollbarStyle { Thin, Solid }
```

- 几何纯函数 `thumb_geometry(viewport, extent, offset, min) -> (size, start)`，公式对齐 GPUI：
  ```
  visible_percentage = viewport / extent          // = viewport/(viewport+max_offset)
  thumb_size        = max(min, viewport * visible_percentage)
  thumb_start       = (offset / max_offset) * (viewport - thumb_size)
  ```
- `Scroll::resolve_in(cx)` 从 `cx.core().scroll_of(id)` 读上一帧几何现算，thumb 以
  `Shape::Rect` 画在容器自身节点（叠加装饰，不参与布局、不受滚动平移影响）。
- 显示策略 Auto：`hovered_within || active` 显形；`active` 是引擎在 `scroll_owned_delta`
  时打的"上一帧滚过"信号（`scrolled_since` 集合），view 前收集进 `ScrollGeom` 后清空。
  淡入淡出动画留给消费方（用 `cx.animate`），引擎只提供显隐事实。
- 数据通路：`Cx::scrolls: Vec<ScrollGeom>`（双轴 extent/offset + viewport + active），
  `scroll_of` 返回 `Option<ScrollGeom>`，`Dep::Scroll` 带 viewport 尺寸 + active 参与缓存比较。

**交互（2026-10-31 完成，C 项）**：画出的条同时是命中区。

- `ContainerParams::scrollbar: Option<ScrollbarHit { thumb, track }>`（容器自身坐标），
  由 `Scroll::decorate` 在画出 thumb 时一并写入。
- 引擎 `PointerPressed` 先问 `scrollbar_at(pos)`：指针落在 track 内就把这次按下变成
  thumb 拖拽（`ScrollbarDrag { id, press_y, start_offset, travel, overflow }`），
  不设 `pressed`、不泼墨、不移动焦点——与 GPUI 的 BlockMouseExceptScroll 同义。
- 按在 thumb 之外先跳位（thumb 中心对到指针），拖拽从跳转后的偏移继续。
- `PointerMoved` 期间偏移 = `start_offset + Δy / travel × overflow`，clamp 到
  `[0, overflow]`；释放即结束。
- 验证：ui-platform `dragging_the_scrollbar_thumb_maps_track_pixels_onto_the_offset_range`
  （半程 → 一半溢出），先红后绿；ui-widgets 断言命中几何与画出的 thumb 一致。

## 步骤 ④ 统一滚动容器控件（ui-widgets `scroll.rs`）— 已完成（步骤 ① 并入）

链式注册风格（对齐 egui `ScrollArea` + GPUI `Div` + Qt `QScrollArea`），
实际实现为 `Scroll::vertical(id, max_h)` / `both(id)` / `fixed(id, w, h)` 三个构造 + `.child()` + `.with_scrollbar()`。详见队列文件步骤 ①。

## 步骤 ④b 迁移（2026-10-31）— gallery nav 抽屉已迁移

**nav 抽屉迁移完成**：`shell.rs sidebar` 用 `Container(NAV_SCROLL, Column)` + `.scrollable()` +
`.sense(SCROLL_OWNED)` 取代原"app 自管 `scroll_y` + wheel clamp"。app.rs 删掉
`nav_scroll`/`nav_content_h`/`nav_visible_h` 三个字段与 `NAV_SCROLL` wheel 分支。
selftest 的 `scroll_into_view` 改用 `AppLoop::scroll_of(NAV_SCROLL)` 读引擎偏移。
`AppLoop` 新增公开 `scroll_of(id) -> Option<f32>` 读引擎表。

**关键教训（为什么 nav 不用 `Scroll` 控件）**：`Scroll::child(Decl)` 收**已求值的声明**，
而 md3 `Md3NavDrawer` 必须作为 **`Styled` Element** 存活——只有元素适配器会把引擎的
clicked 状态传给 `Widget::change`，drawer 才能上报 `Change::Selected(Some(row))`。
若把 `drawer.decl(&cx)` 塞进 Scroll，drawer 的 `change` 永不触发，导航点击失效
（selftest nav click 从 35/35 变 1/35 的回归即由此来）。迁移形态：
**内容是需要上报交互的 Element → 用 `Container` + SCROLL_OWNED**（drawer 保持 Styled）；
**内容是纯声明 → 用 `Scroll` 控件**（页面内容、List 行）。`Scroll` 控件若要接收
Element child，需要 `&mut Cx`（Widget::decl 只给 `&Cx`），是已知缺口，留后。

验证：nav click 35/35 ✓、three columns 35/35 ✓、slider sizes ✓、drawer picked ✓。
剩余失败仅预存（fab 从 closed 打开、grid/inline/positioning），与本项无关。

## 步骤 ④c List 迁移与 carousel 的结论（2026-10-31）

- **List 已迁移**：`ui_widgets::List` 不再收 `scroll_y`，内部改成
  `Scroll::fixed(id, w, h).background(..)`，偏移归引擎（`SCROLL_OWNED`）。新增
  `List::resolve_in(cx)` 让滚动条读几何并画出；`resolve()` 无上下文，不画条也不写命中区。
  thumb 默认色是半透明黑，在深色列表上不可见，List 改为取包里 `OnSurface` 的 40%。
- **carousel 不迁移**，原因记入 `docs/lessons/design-errors.md`：shrink 布局
  （FullScreen/MultiBrowse）的 `scroll` 只决定槽位重排，strip 不平移，其
  `max_scroll = page × (count-1)`，与布局算出的 `extent - viewport` 无关。引擎的 clamp
  建在几何溢出上，接不了"逻辑页位置"。它属于吸附式分页视图（自带拖拽 + 吸附策略），
  offset 继续归控件/应用；引擎侧的双轴偏移与水平滚轮已就绪，纯几何的横向列表可直接用。

## 步骤 ⑤ 编程滚动（ScrollStrategy，延迟到 layout 后）

```rust
pub enum ScrollStrategy { Top, Center, Bottom, Nearest }
// Cx::scroll_to_item(id, ix, strategy) — 请求延迟到 layout 后执行（GPUI 模式）
```

## 步骤 ⑥ 等高虚拟化（预留，不在此步实现）

`UniformList` 最小形态 = 代表 item 测量 + visible_range + `render_items(Range)`。
等 MD3 需要长列表再上。变高 SumTree 太重建模成本高，不做。

## 步骤 ⑦ 惯性（可选）

照 egui 恒定摩擦（每帧减 1000·dt，停速 20px/s）或 GPUI 指数衰减（0.998/ms 闭式）。
都放输入层（合成 Scroll 事件流），容器只负责 clamp。

## 步骤 ⑧ 拖拽平移 / 橡皮筋 / 惯性（2026-11-01，推翻上一版「明确不做」）

上一版把这三项列为不做，理由是「桌面鼠标无此交互」。用户要求可拖拽，
并且要有触底回弹与惯性，因此推翻——记录原因，避免下次又按旧结论拒绝。

**落点（引擎，不是控件）**：三项都是"指针手势 → 偏移轨迹"的物理，属于输入层，
放进 `ui-platform/src/view.rs`，任何 `SCROLL_OWNED` 容器（`Scroll` 控件、`List`、
gallery nav 抽屉、页面骨架）自动获得，控件侧零改动。

- **谁能平移**：从按下点向上走到最近 `SCROLL_OWNED` 容器，途中若遇到 `DRAG`/`SELECT`
  节点则手势归它（滑块、可选标签、可拖卡片照旧工作）。纯 `CLICK` **不**阻挡——
  拖拽本来就会取消点击，这与 iOS 一致（在按钮上拖＝滚动，不＝点按钮）。
- **何时开始**：过 `DRAG_THRESHOLD` 才接管，阈值内的位移丢弃（内容从越过阈值处开始走），
  点击因此不受影响。
- **跟随**：`raw ∓= 指针位移`，内容与手指 1:1；越过端点后按 iOS 公式
  `(1 − 1/(x·0.55/dim + 1))·dim` 阻尼，越拖越"顶手"。
- **越界可见**：布局的 `clamp` 原先会把越界偏移夹掉，橡皮筋与回弹因此看不见。
  现在 `ContainerParams::scroll_elastic` 由引擎在"该容器正被拉伸"的帧置位，布局据此不夹。
  其余情况（应用自己给的 `scroll_y`）仍夹——那才是这道 clamp 的原始职责。
- **松手**：还在越界 → 引擎弹簧（`AnimStore::spring_hinted`，k=280/c=30，先用当前偏移
  播种再以松手速度重新定目标，否则第一帧会跳到目标）；在范围内有速度 → 指数衰减惯性
  （GPUI 的 0.998/ms ⇒ 每秒 `e^-2`，低于 30 px/s 停）。惯性撞到端点时把速度交给同一个弹簧。
- **速度来源**：按**帧**测（`Δoffset / frame_step`，0.6 平滑），不按事件测——事件没有
  时间戳，而帧步长就是引擎的时间刻度；`frame_step` 上限 0.05s，卡顿不会把内容甩出去。
- **刹车**：滚轮一进就清掉该容器的运动（滚轮重新接管）；`scroll_motion` 非空时
  `FrameOutcome::animating` 为真，循环因此持续提交帧。

验证：`ui-platform/src/view.rs` 六条测试（拖拽改偏移、越界阻尼、松手回弹、惯性滑行到
远端、`DRAG` 子节点保住指针）；gallery 里 `GALLERY_DRAG=100,800,100,200` 使导航抽屉
滚动 600dp 且**没有**触发被拖过的那一行的点击。

## 明确不做

- 变高 SumTree 虚拟化（太重）

## 验证

- 现有测试不回归：`cargo test --workspace --features ui-text-fontique/system`
- gallery nav 抽屉 / `List` / carousel 迁到新控件，selftest 验证滚动位置
- 新控件自身测试：尺寸策略（fill vs scroll）、轴分流、滚动条几何、clamp 边界
