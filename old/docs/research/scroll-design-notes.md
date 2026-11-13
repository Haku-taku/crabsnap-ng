# 统一滚动容器抽象 — 设计约束笔记

> 本文件是第 3 项「统一滚动容器抽象」的设计草稿。调研笔记：
> `docs/research/{gpui,egui,qt}-scroll.md`（三份均已完成，2026-10-31）。

## 引擎现状（已勘察，2026-10-31）

**可复用能力：**
- `Overflow::Scroll` clip（`layout.rs`）+ `scroll_x/y` 偏移（`tree.rs:252-259`）
- `Node.scroll_extent` 已落盘（`layout_engine.rs:1274`），被滚动钳位读取
- `Event::Scroll { pos, delta }` 事件（`event.rs:101`）
- `scroll_target()`（`view.rs:2627`）：滚轮事件向上找第一个 `clip.scrolls()` 或
  `Sense::SCROLL` 的祖先 → `Change::Value(-delta)` 上报（`view.rs:2339-2349`）
- `Sense::SCROLL` / `Sense::DRAG` / `Sense::HOVER` / `Sense::FOCUSABLE`（`sense.rs`）
- `Change::Value(f64)` 上报通道已通，app 在 `changed()` 收到（`change.rs:26`）
- `Decl::scrollable()` / `scroll_x()` / `scroll_y()`（`tree.rs:730,755,764`）

**现有滚动用例：**
| 位置 | 方式 |
|---|---|
| gallery `shell.rs:609` nav 抽屉 | 手管 `nav_scroll`，`changed(on==NAV_SCROLL)` 累加 clamp |
| `ui-widgets/src/list.rs` `List` | 手管 `scroll_y`，app 传值 |
| `ui-widgets-md3/src/carousel.rs:484,506` | 手管 `scroll`，根节点 `scrollable()+Sense::DRAG` 自己处理 |

## 缺口（第 3 项要建）

1. **统一滚动容器控件**：尺寸策略（固定 / min / 适配父容器 `fill`）、溢出滚动
2. **offset 持有者从 app 移到控件内部**（控件自己的 State）
3. **滚动条**：`scroll_extent` 算出来了但没人画；显示策略（常显/需要时/hover/拖动中）、
   交互（点击轨道翻页、拖动把手）
4. **拖拽滚动**、**弹性+惯性可选**
5. **按 id 查询滚动偏移的 API**（`engine-gaps.md` §2：`Routing`/`Cx` 暴露滚动偏移表）
   —— app bar 连续折叠、carousel 分页吸附需要它

## 设计约束

- 复用现有引擎通道，不自建平行机制：`scroll_target()` 已处理命中，控件只需在
  `changed()` 里收 `Value` 累加 offset
- 滚动条需要视口/内容尺寸，控件在 `change()` 能拿到 `bounds`，但 `decl()` 拿不到——
  offset 与滚动条位置必须由控件内部 State 持有并跨帧重读
- 遵循 `AGENTS.md` §2：参考 GPUI/egui/Qt 的实现与不变量，不抄结构
- 遵循 `AGENTS.md` §6：注释英文在前、中文对照、每种最多两行

## 调研启示（egui/Qt 已落地，GPUI 待定）

**Qt：**
- viewport 模型：`widgetResizable=true` 内容放大到视口、false 按固有尺寸出滚动条——
  对应两种尺寸策略：`fill`（内容自适应视口）vs `scroll`（内容固定、视口裁剪）
- scrollbar 三角：`max = 内容 − 视口`、`pageStep = 视口`、`value = 像素偏移`——
  正是 `scroll_extent` 与视口尺寸的关系；`Change::Value` 通道复用即可
- scrollbar 交互两类 action：点轨道 = page step 翻页、拖把手 = clickOffset +
  sliderValueFromPosition 反算
- 惯性：QScroller 状态机 `Inactive→Pressed→Dragging→Scrolling`，释放按
  ScrollingCurve（默认 OutQuad）+ DecelerationFactor 减速。**Widgets 无 FrictionCoefficient**
  （那是 QML Flickable 的属性），按"位置曲线 + 减速度"建模
- 弹性：OvershootPolicy 可选，拖拽越界按阻力衰减、惯性越界限幅、回弹按时间——
  = 偏移超界时额外"橡皮筋"段 + 松手回正
- 位置无持久化（`ensureVisible` 是编程滚动）；大列表虚拟化与普通滚动**共用同一
  scrollbar range 语义**（内容尺寸由数据源虚报）

**egui：**
- 逐轴对称：`new([x, y])` 两个 bool，垂直/水平完全对称，不是纵向加横轴特例
- 滚动条三态 `AlwaysHidden / VisibleWhenNeeded / AlwaysVisible`（无 Auto）；外观走全局
  `ScrollStyle`（solid/thin/floating 三档），floating 浮在内容上不占空间
- 交互：全宽命中区、轨道点击把手居中、拖动保持抓取偏移、把手长度 = 视口/内容比例
  钳最小长度
- 内容 measure：**offset 改子 Ui 的 max_rect 原点，不改布局**——内容从视口原点偏移
  后在限宽空间自由生长，clip 固定视口。`layout_engine.rs:1287` 已采用同手法
- auto_shrink 是四象限表（方向×shrink → 容器尺寸）；不可滚轴仍走普通盒模型
- 输入三来源开关 `ScrollSource{scroll_bar, drag: Never/OnTouch/Always, mouse_wheel}`；
  滚轮**先在输入层平滑**（离散滚轮指数稀释、触摸板直通），滚动区只消费
  `smooth_scroll_delta`，消费即清零（嵌套不叠加）
- 惯性：**恒定摩擦**（每帧减 1000·dt，停速 20px/s），速度来自带时间戳的指针位置环
  的最小二乘拟合
- State 按 id 持久化（同屏多个必须 id_salt），一个滚动区派生约 5 个子 id；
  拖拽/滚动条命中在内容布局之前，**只能信上一帧的 interact_rect**
- 编程滚动走 per-frame 请求通道（immediate 特有，需权衡）；变量高虚拟化/吸附分页/
  嵌套滚动仲裁 egui 也没有，需自建

**GPUI：**
- 无 `ScrollableAxis`/`Scrollable` trait（第三方命名，已废弃）；轴是 `Axis` 枚举，可滚由
  style `overflow: Point<Overflow>` 表达——与我们的 `Clip{x,y:Overflow}` 同构
- 滚动条在 Zed 的 `crates/ui`（`scrollbar.rs` 1914 行）：**叠加装饰**（absolute 覆盖，不参与
  布局），几何从 `max_offset/viewport` 现算；thumb ≥ 25px、`start = offset/max_offset` 比例映射；
  显示策略 `Auto/System/Always/Never` + `ScrollbarRevealPolicy`，`VisibilityState` 状态机淡入淡出
  （显示 50ms/隐藏 400ms/1s 无活动隐藏）；命中区 `BlockMouseExceptScroll`（挡 hover 不挡滚动）
- 状态：`ScrollHandle(Rc<RefCell>)` 用户持有（offset 负值），或无 handle 时存元素跨帧表
  `(GlobalElementId, TypeId)`；`top_item()/logical_scroll_top()` 把像素偏移转逻辑偏移
- clamp 在 prepaint/layout（每帧兜底），事件只管 `offset += delta`；`scroll_max = content−viewport`
  2 位小数舍入
- 惯性只在触摸层（手势识别器 + `ScrollPhysics` 指数衰减 0.998/ms 或 Android friction spline，
  闭式可测）；**滚轮无惯性**；**无弹性回弹**（边界策略是容器的事，默认硬 clamp）
- **无鼠标拖拽滚动**（只有滚轮/触控板 + 触摸 pan + 滚动条 thumb 拖拽）——MD3 桌面也不强制，
  不建议进通用容器（与文本选择冲突）
- 虚拟化：`UniformList`（等高，只实例化可见区间，代表 item 测量 + `ScrollStrategy{Top,Center,
  Bottom,Nearest}`）、`List`（变高 SumTree + 逻辑偏移 `ListOffset{item_ix,offset_in_item}` +
  splice 锚定 + follow-tail）
- 落地顺序（GPUI 子代理建议）：①轴化滚动事件 → ②可读滚动状态 → ③统一滚动容器 → ④滚动条
  装饰 → ⑤ScrollStrategy 延迟到 layout 后 → ⑥等高虚拟化 → ⑦惯性可选

**三框架共同的取舍点：**
1. 滚动条占空间（solid）vs 浮在内容上（floating）——两种都主流，控件要可配置
2. 惯性建模：Qt 用曲线+减速度因子，egui 用恒定摩擦，GPUI 用指数衰减——都非"物理精确"，够用即可
3. 拖拽滚动：egui 可关、Qt 需显式 QScroller、GPUI 无——**不建议进通用容器**
4. 滚轮平滑层：egui 有（离散滚轮稀释）、GPUI 走 `ScrollDelta{Lines,Pixels}`、Qt 依赖系统——
   我们 `Event::Scroll` 无平滑层，需升级为轴化 + Lines/Pixels

**落地定稿（综合三框架 + 引擎现状）：**
- 分层照 GPUI：物理/输入层（ScrollDelta 轴化）与容器层（clamp/bounce）分离
- 尺寸策略照 Qt：`fill`（内容自适应视口）vs `scroll`（内容固定、视口裁剪）
- 滚动条几何照 GPUI + egui：`visible_percentage = viewport/(viewport+extent)`，thumb 钳最小，
  `BlockMouseExceptScroll` 命中语义（overlay 挡 hover 不挡滚动）
- 惯性照 egui 恒定摩擦（实现简单）或 GPUI 指数衰减（更接近 iOS）；都可选
- 不做：鼠标拖拽滚动、弹性回弹（MD3 不强制）、变高 SumTree 虚拟化（太重）
