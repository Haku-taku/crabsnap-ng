# Qt 滚动容器调研 — QScrollArea / QAbstractScrollArea / QScrollBar / QScroller

> 目的：为 crabsnap-ng 引擎的统一滚动容器抽象提供参考。本文是设计调研，只读不写代码。
> 配套草稿见 `docs/research/scroll-design-notes.md`（引擎现状与缺口）。
> 来源：Qt 6 官方文档（doc.qt.io）、Qt 源码 `src/widgets/*`（qtbase，经 github 镜像与 DeepWiki）、Qt 5.0–5.15 历史文档。

## 0. 结论速览（给引擎的启示先行）

| 调研点 | Qt 做法 | 对我们引擎的启示 |
|---|---|---|
| 1 容器结构 | `viewport`（裁剪口）+ 内容 widget 分离；`QScrollArea` 用 `widget->move()` 实现像素级滚动 | 已有 `Overflow::Scroll` clip + `scroll_x/y`，缺的是把"viewport 几何 + offset 状态 + scrollbar 几何"收敛成一个控件 |
| 1 widgetResizable | `widgetResizable=true` 时内容被放大到视口（`p.expandedTo(min)`），max 钳位；false 时按内容固有尺寸出滚动条 | 控件需要两种尺寸策略：`fill`（内容自适应视口）vs `scroll`（内容固定、视口裁剪） |
| 2 scrollbar 模型 | `min/max/singleStep/pageStep/value`，`pageStep=视口尺寸`，`max=内容−视口`；`valueChanged`/`rangeChanged`/`sliderPressed/Moved/Released`/`actionTriggered` 六信号 | range/value/pageStep 三角（`content = max−min+pageStep`）正是我们 `scroll_extent` 与视口尺寸的关系 |
| 3 scrollbar 交互 | 点击轨道 = 按 `pageStep` 翻页（`SliderPageStepAdd/Sub`），点击把手开始拖动、拖动用 `clickOffset`（点按把手内部的相对位置）+ `sliderValueFromPosition` 反算 | 交互拆成两类 action（page step / handle move），复用 `Change::Value` 通道即可 |
| 3 惯性滚动 | `QScroller` 状态机 `Inactive→Pressed→Dragging→Scrolling`，`InputPress/Move/Release` 驱动；释放时按释放速度 + `ScrollingCurve`(默认 OutQuad) + `DecelerationFactor` 生成减速段 | 拖拽/惯性是独立于 scrollbar 的"输入→offset 累加器"，与 scrollbar 的 value 同源但不同质 |
| 4 弹性滚动 | **支持**。`OvershootPolicy`（WhenScrollable/AlwaysOff/AlwaysOn）；拖拽越界按 `OvershootDragResistanceFactor`（0–1 阻力）、惯性越界按 `OvershootScrollDistanceFactor×视口尺寸` 限制距离，回弹用 `OvershootScrollTime` | 引擎层做 clip + scroll_extent，弹性 = 偏移超界时的额外"橡皮筋"段 + 松手回正，可做成可选效果 |
| 5 位置保存 | 无持久化。`QScrollArea` 不存滚动位置；只有 `ensureVisible`/`ensureWidgetVisible` 把任意点/子控件滚进视口 | 滚动偏移是纯运行时状态，控件内部持有即可；需要"按 id 查偏移"（engine-gaps §2）而非持久化 |
| 6 虚拟化 | `QAbstractItemView` 的 model/view 是按需创建 widget/绘制可见行的虚拟化；`QScrollArea` 不虚拟化，滚动条 range 是 `内容尺寸−视口` | 大列表 = 内容尺寸"虚报"+ 按偏移只实例化可见项；scrollbar 的 range 语义两条路通用 |

**关于 `FrictionCoefficient`（用户问题 3）**：Qt Widgets 的 `QScrollerProperties` 枚举从 Qt 4.8 引入至今（5.0/5.5/5.7/5.15/6.x）**从来没有** `FrictionCoefficient`。Qt 里真正叫 "friction" 的是 **QML `Flickable.friction`** 属性（Qt Quick，纯声明式）。Widgets 侧表达减速度的是 `ScrollMetric::DecelerationFactor`（作用在 `ScrollingCurve` 上）。设计时应按"曲线 + 减速度因子"建模，而非"摩擦系数"。

---

## 1. QAbstractScrollArea 结构：viewport 与内容 widget 的关系

**来源**：QAbstractScrollArea 文档（doc.qt.io/qt-6/qabstractscrollarea.html）、QScrollArea 文档、qscrollarea.cpp（DeepWiki 代码追踪）。

### 1.1 viewport 是"滚动内容的绘制表面"

QAbstractScrollArea 是滚动区域的低层抽象，结构为：

```
QAbstractScrollArea (带边框 frame)
 ├─ viewport   — 中央 widget，内容的可视区域（裁剪口），滚动内容画在这里
 ├─ verticalScrollBar — 右侧
 └─ horizontalScrollBar — 下方
```

- 内容全部绘制在 `viewport` 上，不用外层 widget 的 `paintEvent`——子类画图一律画 viewport（`paintEvent` 注释明确要求 QPainter 操作在 `viewport()` 上）。
- 滚动条策略 `Qt::ScrollBarAsNeeded`（默认）：滚动条有非零 range 才显示；**隐藏滚动条时 viewport 扩大，出现时 viewport 缩小**——viewport 尺寸与 scrollbar 存在性是联动的。
- 三个配套机制：
  - `setViewportMargins()`：视口四周留白，用于"锁定行列"（如表格表头、`QHeaderView`）。锁定内容不滚、滚动内容从留白下方开始。
  - `setCornerWidget()` / `addScrollBarWidget()`：两条滚动条夹角处的 corner widget、滚动条旁的附属 widget（如表头）。
  - `setViewport()` / `setupViewport()`：可替换 viewport widget（子类初始化新 viewport 的入口）。

### 1.2 QAbstractScrollArea 子类的责任（四条）

文档明确列出的子类义务：
1. 控制滚动条的 **range / value / pageStep** 并跟踪其移动；
2. 按滚动条 value 把内容画到 viewport 上；
3. 在 `viewportEvent()` 里处理 viewport 事件（resize 等）；
4. 用 `viewport->update()` 而非 `update()` 触发重绘。

文档给了最小实现骨架（**这就是 QScrollArea 的核心逻辑**）：

```cpp
// 内容或视口尺寸变化时：
verticalScrollBar()->setPageStep(areaSize.height());
verticalScrollBar()->setRange(0, widgetSize.height() - areaSize.height());
// 滚动条 value 变化时：
widget->move(viewport()->rect().topLeft() - QPoint(hvalue, vvalue));
```

- 滚动条 **value 直接就是内容偏移**（像素）。`move` 的是内容 widget 相对 viewport 的位置。
- 追踪滚动用虚函数 `scrollContentsBy(dx, dy)`：默认实现 `viewport->update()` 整片重绘；QScrollArea 重写为移动内容 widget。**编程式滚动不允许直接调 `scrollContentsBy`，要走 `setValue`**——value 是唯一事实源。
- 事件转发：QAbstractScrollArea 把 viewport 的 paint/mouse/wheel/drag/resize 等事件重映射成自己的 `*Event` 虚函数，子类不用装 eventFilter。

### 1.3 QScrollArea：像素级滚动 + widgetResizable

QScrollArea 继承 QAbstractScrollArea，为任意 widget 提供平滑（像素级）滚动。属性只有两个：`alignment` 和 `widgetResizable`。

`widgetResizable` 语义（关键）：
- **`false`（默认）**：尊重内容 widget 的固有尺寸（sizeHint / 显式 resize）。内容超过视口 → 出滚动条。
- **`true`**：自动调整内容 widget 尺寸，"尽量避免滚动条、或利用多余空间"。源码逻辑（`QScrollAreaPrivate::updateScrollBars`）：
  ```
  视口尺寸 p，内容 widget 尺寸 v
  widgetResizable 时：widget->resize(p.expandedTo(widgetMin).boundedTo(widgetMax))
    —— 内容被放大到视口（受 min/max 约束）；内容固有 sizeHint 比视口大时保持大尺寸 → 出滚动条
  ```
  即"内容可被拉伸填满视口，但内容本身想更大的时候仍然滚动"。
- 两种情况下滚动条 range 都是 `v − p`（`hbar->setRange(0, v.width()−p.width())`，负数被钳成 `(0,0)` = 无滚动）。`resizable=true` 只是把 `v` 也写大，range 公式不变。

`updateWidgetPosition()` 再算 widget 的位置：
- 内容宽 < 视口宽：按 `alignment` 对齐（默认左上角，可选 HCenter/VCenter 等）；
- 内容宽 > 视口宽：`widget->move(viewport.topLeft() − (hvalue, vvalue))`。

两个便捷 API：
- `ensureVisible(x, y, xmargin=50, ymargin=50)`：把内容里的点滚进视口（带边距，够不到就滚到最近合法位）。
- `ensureWidgetVisible(childWidget, ...)`：把内容的子控件滚进视口。

### 1.4 对引擎的启示

- **viewport 是必须存在的中间概念**：滚动容器的本质是"一个裁剪口 + 一套内容坐标偏移 + 两条滚动条"。我们引擎里 `Overflow::Scroll` clip 已经承担了裁剪口，缺的是把 viewport 尺寸、内容尺寸、offset 三者**算在一处**并暴露给滚动条绘制的控件层。
- widgetResizable 两种语义 = 引擎两种尺寸策略：内容可被父容器"填充"（`fill`）或内容固定、视口滚动（`scroll`）。布局引擎的 `Overflow::Scroll` 目前只管剪裁不管"内容是否填满视口"，这是控件层要补的策略。
- 滚动条 range = `内容尺寸 − 视口尺寸`，pageStep = 视口尺寸：这就是我们 `scroll_extent`（内容超出视口的部分）的坐标化表述。

---

## 2. QScrollBar：range / value / pageStep 模型与交互

**来源**：QScrollBar 与 QAbstractSlider 文档、qscrollbar.cpp / qabstractslider.cpp（DeepWiki 代码追踪）。

### 2.1 QAbstractSlider 的属性模型

QScrollBar 继承 `QAbstractSlider`。核心属性：
- `minimum` / `maximum`：合法值域（整数）。`setRange(min, max)` 一次设两个；改 max 会联动修 min 与 value 使值域合法。
- `value`：当前值，永远 `min ≤ value ≤ max`。这是内容偏移（像素）。
- `singleStep`：小步进（方向键/滚动箭头一次移动量，文本场景 = 一行 ≈ 20px）。
- `pageStep`：大步进（PageUp/Down、点击轨道一次移动量，通常 = 视口尺寸）。
- `tracking`：拖动过程中是否持续发 `valueChanged`（默认 true 拖动即发；false 只松手发）。
- `sliderPosition`：当前把手位置；`tracking=false` 时与 `value` 解耦（`valueChanged` 只在 release 发）。

**range/value/pageStep 的三角关系**（文档原话）：
```
文档长度 = maximum − minimum + pageStep
```
QScrollArea 的用法就是：`max = widgetSize − viewportSize`，`pageStep = viewportSize`，`min = 0`。例如 100 行内容、视口 20 行 → `max=80, pageStep=20`，共 5 页。

### 2.2 信号全集（valueChanged 是最重要的）

| 信号 | 时机 |
|---|---|
| `valueChanged(value)` | value 变化（拖动中是否发取决于 `tracking`） |
| `rangeChanged(min, max)` | 值域变化（内容/视口尺寸变化时） |
| `sliderPressed()` | 用户按下把手 |
| `sliderMoved(value)` | 拖动把手（即使 tracking=false 也发） |
| `sliderReleased()` | 松开把手 |
| `actionTriggered(action)` | 由用户交互或 `triggerAction()` 触发的动作（此时 sliderPosition 已调整、value 与显示尚未传播，可在槽里改写） |

### 2.3 交互细节（源码：mousePress/Move/Release）

QScrollBar 的鼠标交互由源码 `qscrollbar.cpp` 确认：

1. **命中判定**：`mousePressEvent` 用 `QStyle::hitTestComplexControl(CC_ScrollBar, opt, pos)` 判定按在哪：`SC_ScrollBarSlider`（把手）、`SC_ScrollBarAddPage/SubPage`（轨道，即"page control"）、还是箭头。
2. **拖动把手**：命中把手时记录 `clickOffset` = 鼠标点相对把手上角的偏移，然后 `setSliderDown(true)`。`mouseMoveEvent` 用 `pixelPosToRangeValue` 计算新位置：
   - 取把手可移动区间（`sliderMax−sliderMin`，即轨道长度−把手长度）；
   - `QStyle::sliderValueFromPosition(min, max, pos−offset, 可用长度, 是否反向)` 把像素反算成 value；
   - `setSliderPosition()` 提交。
   - `PM_MaximumDragDistance` 兜底：拖太远可以把手弹回按下的位置。
3. **点击轨道翻页**：命中 `SC_ScrollBarAddPage/SubPage` 时 `triggerAction(SliderPageStepAdd/Sub)`，即按 `pageStep` 翻页；按住不放由 `setRepeatAction(action, threshold=500ms, repeat=50ms)` 定时重复。按住方向键同理走 singleStep。
4. **键盘**：方向键/singleStep、PageUp/Down/pageStep、Home/End 到 min/max。
5. 现代滚动条（Qt Quick / Windows 11 风格）还支持点击轨道直接跳转（`SH_ScrollBar_LeftClickAbsolutePosition`），但 Widgets 传统交互是"点轨道 = 翻一页"。

### 2.4 对引擎的启示

- scrollbar 的模型可以完全抽象成 **value 与范围操作**，不关心把手像素几何：`setRange`/`setPageStep`/`setValue` + 六个信号。把手长度由样式用 `pageStep / (max−min+pageStep)` 推算。
- 交互 = 三个 action 族：`SingleStep`（箭头/方向键）、`PageStep`（轨道点击/PageUp/Down）、`Move`（拖动把手）。三者都汇入"改 value"。
- 我们引擎的 `Change::Value` 通道天然可以承载 `valueChanged`；滚动条控件只需在命中时产出 `SliderPageStepAdd` 之类的动作。拖把手需要的 `sliderValueFromPosition` 是纯函数（像素→值），可复用。
- 滚动条出现/消失的联动：视口几何要在"有滚动条 / 无滚动条"两种状态下重算——这正是现有 `Overflow::Scroll` clip 没处理的（它不知道滚动条会占掉视口空间）。

---

## 3. QScroller：拖拽滚动 + 惯性（kinetic scrolling）

**来源**：QScroller / QScrollerProperties / QScrollPrepareEvent 文档、`src/widgets/util/qscroller.h`（github 镜像）、qscroller.cpp（DeepWiki 代码追踪）。

### 3.1 定位：给"任何滚动 widget"挂一个惯性滚动器

`QScroller` 是为任意滚动 widget/graphics item 提供动能滚动的辅助对象，**不取代 QAbstractScrollArea**，而是挂在它的 viewport 上处理触摸/鼠标拖拽。API：
- `QScroller::scroller(target)`：单例式获取（每个 target 至多一个）；`grabGesture(target, TouchGesture|LeftMouseButtonGesture|…)` 注册手势识别。
- `handleInput(Input, QPointF, timestamp)`：手势识别器把输入事件拆成 `InputPress/Move/Release` + 位置 + 时间戳喂给 scroller，返回是否消费事件。**输入设备无关**——触摸、鼠标、笔都走同一通道。
- `scrollTo(pos[, scrollTime])` / `ensureVisible(rect, margin…)`：编程式平滑滚动（默认平台时间跨度，可指定毫秒）。
- `setSnapPositionsX/Y(list | first, interval)`：分页吸附（列表头、carousel 分页）。
- `velocity()`（当前速度 m/s）、`finalPosition()`（预计终点）、`stop()`、`state()`。

### 3.2 状态机（`qscroller.h` 源码确认）

```
enum State { Inactive, Pressed, Dragging, Scrolling };
enum Input { InputPress=1, InputMove, InputRelease };
```

转表（qscroller.cpp 的 `handleInput`）：
- **Inactive + Press** → Pressed（前提：`prepareScrolling` 成功、且内容可滚或 overshoot 恒开）
- **Pressed + Move** → Dragging（位移超过 `DragStartDistance`）
- **Pressed + Release** → Inactive（没拖起来 = 点击，放行给下层控件）
- **Dragging + Release** → Scrolling（按释放速度生成滚动段）
- **Scrolling** → 段走完 → Inactive

拖动开始前发 `QScrollPrepareEvent`：目标对象在里面回报 `viewportSize` / `contentPosRange`（可滚范围）/ `contentPos`（当前偏移），并 accept 表示"开始滚"。QScrollArea 收到这个事件就能把自己伪装成任意滚动目标——**scrollbar 的 range 与 scroller 的 contentPosRange 是同一条信息的两面**。

### 3.3 惯性减速度（回答"FrictionCoefficient"）

**Qt Widgets 无 `FrictionCoefficient` 枚举**。表达减速度的是：

| ScrollMetric | 说明 |
|---|---|
| `ScrollingCurve` | 减速用的 QEasingCurve。**注意是对"位置"的缓动，不是速度**。默认 `OutQuad` → 一阶导（速度）线性下降 = 二阶导（加速度）恒定 |
| `DecelerationFactor` | 影响减速到 0 的时长，配合曲线使用，典型值 0.1–2.0 |
| `MinimumVelocity` / `MaximumVelocity` | 触发/封顶速度（m/s） |
| `DragVelocitySmoothingFactor` | 拖拽速度的低通平滑系数（0–1，越小越平滑） |
| `AxisLockThreshold` | 轴锁定（0–1，朝某轴移动的判定角度） |
| `MousePressEventDelay` / `DragStartDistance` | 手势判定的防误触参数 |

物理单位约定：**属性一律用物理单位（米/秒），由 QScroller 按 DPI 换算成像素**——与绘制层解耦。

默认值平台相关，模拟各平台手感。用户常说的 "friction" 概念对应 **QML `Flickable.friction`**（Qt Quick），Widgets 侧没有同名参数，机制上是"位置曲线 + 减速度因子"。

### 3.4 对引擎的启示

- 惯性滚动 = **独立的"输入 → 偏移"状态机**，与 scrollbar 解耦但共享同一 offset 事实源。我们把 `scroll_x/y` 的**来源**分成三种：scrollbar setValue、拖拽/惯性累加、编程式 scrollTo。
- 参数化模型不必抄 `ScrollMetric` 全家，关键是：`DragStartDistance`（拖拽判定）、释放速度、减速曲线（OutQuad 位置缓动）、`MaximumVelocity`、可滚范围（`contentPosRange`）。这些正好是 `scroll-design-notes.md` 缺口里的"拖拽滚动、弹性+惯性可选"要的参数。
- 速度用物理单位换算像素的做法不适合我们（无 DPI 概念），直接用像素/秒即可。

---

## 4. 弹性滚动（overshoot / bounce）

**来源**：QScrollerProperties 文档（`OvershootPolicy` 与四个 Overshoot metric）、qscroller.cpp（DeepWiki 代码追踪）。

### 4.1 支持，且是默认行为

QScroller **支持弹性滚动**，策略枚举：
```
enum OvershootPolicy { OvershootWhenScrollable(默认), OvershootAlwaysOff, OvershootAlwaysOn };
```
`HorizontalOvershootPolicy` / `VerticalOvershootPolicy` 分轴设置（默认 WhenScrollable = 内容可滚时才允许越界）。

### 4.2 四个 Overshoot 参数

| 参数 | 作用 |
|---|---|
| `OvershootDragResistanceFactor` | **拖动中**越界的阻力（0–1）：内容位移 = 手指位移 × 该因子，越拖越"费劲"（橡皮筋手感） |
| `OvershootDragDistanceFactor` | 拖动越界的最大距离 = 该因子 × 视口尺寸 |
| `OvershootScrollDistanceFactor` | **惯性滚动中**越界的最大距离 = 该因子 × 视口尺寸 |
| `OvershootScrollTime` | 回弹动画时长（秒） |

源码行为：拖拽越界时 `overshootPosition` 跟踪越界量，用阻力因子衰减；松手后按 `OvershootScrollTime` 从越界位置**弹回边界**；惯性滚过头同样限幅再回弹。即经典 iOS 橡皮筋。

### 4.3 对引擎的启示

- 弹性滚动的模型 = **偏移的超界段**：正常偏移 `∈ [0, max]` 之外再挂一段"橡皮筋偏移"，拖动时按阻力因子衰减、松手/惯性结束时按回弹曲线归零。clip 仍是内容裁剪，overshoot 只是偏移的额外分量。
- 引擎结构上：`scroll_extent` 给的是正常范围；弹性是控件层（或 scroller 层）在 offset 上叠加的可选效果，不该污染布局引擎。默认关闭或 `OvershootAlwaysOff` 语义，需要时按平台开。

---

## 5. 滚动位置保存

**来源**：QScrollArea / QAbstractScrollArea / QAbstractSlider 文档。

- QScrollArea **没有状态持久化**。滚动位置就是 `QScrollBar::value` 这个整数，随 widget 生命周期存在；重建 scroll area 不会恢复位置（文档无任何 save/restore 机制）。
- 提供的"把东西滚进视野"API：`QScrollArea::ensureVisible(x, y, margin)` / `ensureWidgetVisible(child, margin)`；QScroller 版 `ensureVisible(rect, margin)` / `scrollTo(pos)` 是平滑动画版。
- "恢复位置"是应用层责任：读 `verticalScrollBar()->value()` 存起来，重建后 `setValue()`。`valueChanged` 信号只用于应用层跟踪，没有内置保存。
- 注意 `tracking`：拖动中是否逐帧发 `valueChanged` 由它决定；拖动把手本身总是发 `sliderMoved`。

**对引擎的启示**：滚动偏移是**纯运行时状态**，控件内部持有即可（`scroll-design-notes.md` 缺口 2：offset 从 app 移到控件 State）。需要的是 `engine-gaps.md §2` 的"按 id 查询滚动偏移"API（app bar 连续折叠、carousel 分页吸附要用），不是持久化。

---

## 6. 大内容虚拟化：QAbstractItemView vs QScrollArea

**来源**：QAbstractItemView 文档、qlistview.cpp / qabstractitemview.cpp（DeepWiki 代码追踪）。

### 6.1 两种滚动场景的本质区别

- **QScrollArea / QAbstractScrollArea**：内容是**单个 widget**（或单个 layout），Qt 必须把整个 widget 实例化并测量。滚动条 range = `内容widget尺寸 − 视口尺寸`。**不做虚拟化**——1 万个节点也是 1 万个真实 widget。
- **QAbstractItemView（QListView/QTableView/QTreeView）**：内容来自 **model**（抽象数据源），view 只负责把**当前可见的那几行**画出来。**虚拟化**。

### 6.2 QAbstractItemView 的虚拟化机制（源码）

1. **滚动条 range 来自"虚报的内容尺寸"**：`QListView::updateGeometries` → `QCommonListViewBase::updateVerticalScrollBar` 用 `contentsSize.height()`（= 总行数 × 行高）设 range。**总行数即使 10 万也不实例化**，只有这个整数参与 range 计算。
2. **可见项计算**：绘制/命中时按 `verticalScrollBar()->value()` 反推首行：`firstRow = value / rowHeight`（ScrollPerItem 用 `scrollValueMap` 在"滚动条值 ↔ 行号"间映射）。`ScrollPerPixel` 与 `ScrollPerItem` 两种模式显式可选。
3. **滚动只重绘暴露区**：`scrollContentsBy` 移动绘制原点、只重绘新暴露的条带，不重排全量。
4. **按需取数**：`scrollTo`/滚动到末尾时调 `model->fetchMore()`（model 可分页取数）。
5. 数据变更增量处理：`dataChanged()` 只更新可见索引（Qt 6.9 加 `updateThreshold=200`，变更超过阈值直接整刷）。

关键点：**虚拟化不改滚动模型**。滚动条仍然是 `value ∈ [0, contentsSize − viewportSize]` 的整数，只是 `contentsSize` 是 model 算出来的"名义尺寸"，而非真实 widget 尺寸。

### 6.3 对引擎的启示

- 我们引擎的 `Overflow::Scroll` + `scroll_extent` 天然支持"**名义内容尺寸**"：给 `List` 之类控件设一个"总行数×行高"的 scroll_extent，然后**按偏移只实例化可见行**——这正是虚拟化，且与 QScrollArea 的滚动条模型（range = 内容−视口）完全兼容。Qt 证明了两条路（widget 滚动 / 虚拟化滚动）共用同一套 scrollbar 模型。
- 引擎现状（`scroll-design-notes.md`）：`ui-widgets/src/list.rs` 已手管 `scroll_y`，缺的是把"名义总高 + 可见区间裁剪"标准化。虚拟化应作为**滚动容器控件的可选模式**（`scroll_extent` 由数据源提供 vs 由布局算出），而不是新的平行机制。

---

## 附：关键词对照表

| 概念 | Qt API | 我们引擎对应 |
|---|---|---|
| 裁剪口 | `viewport()` | `Overflow::Scroll` clip |
| 内容偏移 | `QScrollBar::value` | `scroll_x/y` |
| 内容超出量 | `scrollBar->maximum() = widgetSize − viewportSize` | `scroll_extent` |
| 页步长 | `pageStep = viewportSize` | （新：滚动条控件算） |
| 内容填满视口 | `widgetResizable=true`（`p.expandedTo(min)`） | 尺寸策略 `fill`（新） |
| 内容固定 | `widgetResizable=false` | 尺寸策略 `scroll`（新） |
| 拖拽/惯性 | `QScroller` 状态机 + `QScrollerProperties` | 滚动输入处理器（新） |
| 弹性 | `OvershootPolicy` + 4 个 Overshoot metric | 可选效果（新） |
| 虚拟化 | `QAbstractItemView` + model（`contentsSize` 虚报） | `scroll_extent` 由数据源提供（新模式） |

## 来源列表

- Qt 6 官方文档：`QScrollArea`、`QAbstractScrollArea`、`QScrollBar`、`QAbstractSlider`、`QScroller`、`QScrollerProperties`、`QScrollPrepareEvent`、`QAbstractItemView`（doc.qt.io/qt-6/）
- Qt 5.15/5.7/5.0 历史文档（doc.qt.io/archives、w3cub、MIT 镜像）——确认 `FrictionCoefficient` 不存在、`DecelerationFactor` 存在
- qtbase 源码：`src/widgets/util/qscroller.h`（github 镜像）、`src/widgets/widgets/qscrollbar.cpp`、`qscrollarea.cpp`、`qabstractitemview.cpp`、`qlistview.cpp`（DeepWiki 代码追踪）
