# 过渡与尺寸动画：声明目标，引擎插值

> 状态：**阶段 A（尺寸过渡）、B1（入场）、B2（缩放）、B3（退场）、C2（Expressive nav rail
> 宽度过渡）、C2b（search 展开）、C2c（对话框入场缩放）、C2d（WinUI 对话框两条通道）
> 已实现**（2026-11-05）。其余见 §6。

## 1. 问题

- **#1**：动画只能驱动 `f32` 与颜色。尺寸要拆成两个实例（switch 的 `SIZE_OFF`/`SIZE_ON`），
  矩形拆四个，枚举无法插值只能锁存。
- **#13**：没有进入/退出过渡。对话框出现即终态，关闭即消失；应用想播完再卸载只能自己
  跨帧持有状态。

## 2. 主流对照（调研结论，2026-11-05）

| 问题 | 结论 | 依据 |
|---|---|---|
| 布局值怎么动 | 参与布局的值就**每帧重排**；只有 transform/opacity 走纯绘制 | Flutter `RenderAnimatedSize` 上报动画尺寸、`AnimatedContainer` 每 tick 重建；Compose `animateContentSize` 返回动画 `IntSize`；CSS width transition；WPF `LayoutTransform`；Qt `Behavior on width`；GPUI `with_animation` 后 `request_layout` |
| 声明写什么 | 写**目标**，引擎/修饰节点在布局前替换（模型 B） | CSS transition、Compose `Modifier.animateContentSize`、Android property animator；只有 Flutter implicit widget 是每 tick 写回插值 |
| 退出期节点归谁 | 必须由**容器或引擎显式持有**，播完再释放 | Compose `AnimatedVisibility` 的 `shouldDisposeBlock`；Flutter `_outgoingEntries`；Android `mDisappearingChildren`；Vue `<Transition>` |
| 退出期兄弟是否重排 | **显式策略**，不能隐式选 | 三种：占位（CSS/Vue、Flutter `AnimatedSwitcher` 的 Stack）、收缩重排（Compose `shrink*`）、移出流（Vue `TransitionGroup` 用 absolute、Android disappearing children） |
| 进入的起始值 | 只能是**显式给定**或由自身目标尺寸推导；没有框架从不存在的节点推快照 | CSS `@starting-style`；Compose `initialSize`；GPUI `SpringAnimation::from`；Android `TransitionManager` 捕获的是"变更前已存在"的 scene |
| 被缓存/未重建的子树 | 过渡节点必须每帧驱动 | 与本仓库的声明缓存直接冲突，见 §4.3 |

反面对照：`iced` 核心没有布局动画；社区 `iced_anim` 用 `animates_layout(bool)`（默认 **false**）
把「绘制动画 vs 布局动画」暴露成显式开关——说明默认偏好性能、需要重排的人自己开。

## 3. 契约

### 3.1 尺寸过渡（阶段 A，已实现）

```rust
container.transition_size(AnimSpec::new(0.25, Easing::Emphasized))
```

- 只对**固定轴**（`Sizing::Definite(Len::Px)`）生效；`hug`/`fill` 的轴不插值
  （内容尺寸动画见 §6）。
- 应用每帧仍声明**目标**；引擎在 `apply` 之后、`layout` 之前把插值写进 `size_class`，
  兄弟随之重排——与 CSS/QML 的语义一致。
- 落定后 `size_class == 声明值`，不产生脏区、不要求下一帧。
- 中断/反向从**当前值**继续（`AnimStore` 已有的重定向逻辑）。
- 缓存住的子树也必须被驱动：`Tree::transitioned()` 由 apply 收集，`mark_reused_subtree`
  同样登记，声明未变时过渡不会冻结。

### 3.2 进入（阶段 B1，已实现）

```rust
container.enter(Presence::fade(AnimSpec::new(0.15, Easing::EaseOut), 0.0))
container.enter(Presence::slide(spec, 0.0, Point::new(0.0, 8.0)))
```

- 起始值显式写在声明里（与 CSS `@starting-style` 同理）；起始不透明度与位移**插值**到
  声明值，而不是拿 presence 相乘——否则「保持不透明的滑入」也会淡入。
- **起点是事件那一刻，不是上一帧的时间戳**：`opened_at` 之类必须取 `FrameClock::live()`。
  引擎时钟只在建帧时前进，而空闲的应用不建帧；用上一帧的戳会让入场在过去开始——
  停一秒再右键，菜单第一帧就已落定（`docs/lessons/design-errors.md` 有该条）。
- 引擎侧 presence：节点出现的那一帧 `presence = 0`，之后每帧 `travel(0 → 1)`；
  进场的 id 每次出现都 `AnimStore::forget`，同一个 `Id` 再次出现因此重新进场。
- 位移只进 `visual_rect()`（绘制与命中），布局、兄弟、脏区计算都不受它影响；
  脏区靠图元矩形变化得出（旧∪新），不需要额外标脏。
- 不透明度折进组不透明度（`ContainerParams::opacity` 同一个层），整棵子树作为一个单元淡入。
- **时长照抄成对表**，不要随手配：M3 的「建议配对」把 emphasized decelerate 绑在 400 ms
  （进入屏幕），200 ms 属于 emphasized accelerate（离开屏幕）。decelerate 曲线前段极陡，
  用 200 ms 时 75% 的淡入落在前 50 ms，观感就是「瞬间出现」。出处：
  m3.material.io/styles/motion/easing-and-duration（Suggested easing and duration pairs）。

### 3.3 退出（阶段 B3，已实现，仅 `OutOfFlow`）

```rust
container.exit(Presence::fade(AnimSpec::new(0.1, Easing::Linear), 0.0))
```

```
声明里消失         → hold：整棵子树标 held，presence 1→0 走 exit
presence 到 0      → release：回收子树并重排
```

- **持有要真正脱离父节点**：`rebuild_index` 按 `node.parent` 重建同层索引，所以只把节点
  移出 `by_parent` 而留着 `parent`，下一帧它会被当回子节点重新挂上。做法是 rebuild 之后
  跑 `detach_held`：把被持有的根从父节点的**每一份**同级列表（`order`、绘制用的 `paint`、
  `by_id`）里摘掉，并把整棵子树的 id 从全树 `by_id` 里删掉（只删仍指向被持有节点的那些，
  重开的同名节点不能被误删）。子树内部的 `by_parent` 条目保留，绘制与回收才有子节点可走。
  漏掉 `paint` 的症状很隐蔽：父节点的绘制遍历把它再画一遍，它同时也还在 `held()` 的
  延迟队列里，于是整棵子树画两次——不透明节点看不出来，半透明遮罩会把自己的 alpha
  叠两遍（MD3 退场首帧的遮罩从 117 突变到 53）。
- 可见性策略是 `OutOfFlow`：兄弟立刻合拢，只有绘制留到退场结束。`KeepSpace`/`Collapse`
  见 §6 的 B4。
- 被持有的节点不在 `walk()`、命中索引与焦点查找里，画在活动内容之上（与模态同样的延后
  绘制顺序）。同一 `Id` 重新出现时应用新建节点，旧的那个继续淡出，互不干扰。
- 退场动画实例按 **NodeId** 取键：一个节点一生只被持有一次，句柄因此唯一标识这次退场，
  不会继承上次落定留下的 0。

### 3.4 绘制期属性

`opacity`（已有）、`offset`（已有，`Node.offset` 是纯视觉位移）、`scale`（engine-gaps #27）。
enter/exit 的淡入、滑入、缩放入场由三者承担；**只有尺寸**进入布局。

缩放的落点（阶段 B2，进行中）：

- `ui_core::draw::Transform` 是**仿射**的 `{ scale, offset }`，不是「绕某点缩放」：两个不同
  中心的分段缩放复合出来是平移，定点表达不了，嵌套缩放的子节点会朝错的中心漂移。
- `DrawList::transform` 在压入时施加，覆盖矩形、圆角、描边宽度、缺口、渐变跨度、弧、阴影、
  图像、字形、裁剪盒、层包围盒与背景模糊——一处设置，整棵子树一致。
- 文本要先成形才有字形，因此缩放随 `TextPrim::zoom` 同行，由文本趟在发射字形/高亮/插入符
  时施加。代价：缩放每帧都变，图元随之不等，成形缓存因此在动画期间每帧未命中——动画只有
  200 ms 且只覆盖被缩放的那几段文字，暂不优化；要优化就让缓存比较忽略 `zoom`。
- 两条通道可以各走各的时钟：`Presence::scaled_over(1.05, spec)` 让缩放用 `scale_spec`
  而不是 presence 自己的 `spec`（引擎用 `id.with("presence-scale")` 单独 travel）。
  WinUI 的对话框正是这样：不透明度 83 ms 线性到位，缩放 250 ms 才走完——共用一个值
  必然让其中之一错，读起来就是「直接出现」。
- 接线：`Presence::scaled(0.8)` 给出端点缩放，引擎写 `Node::presence_scale`；绘制端在一棵
  子树的范围里设 `list.transform`（origin 取该节点 `visual_rect()` 的中心），从自己的形状到
  子节点、裁剪、层、字形一并缩放；命中用同一套复合（`Tree::scaled_visual_rect`），因此点
  在画出来的像素上。落定值取整为 1，避免子树永久走变换路径。
- 未做：静态（非进出场）缩放与逐轴非等比（jelly），仍归 engine-gaps #27。

## 4. 实现落点

### 4.1 阶段 A 代码

| 位置 | 作用 |
|---|---|
| `ui-core/src/tree.rs` `ContainerParams::size_transition` | 声明槽 |
| `Decl::transition_size` / `Container::transition_size` | 构建期入口 |
| `Node::declared_size` | 声明的目标；`size_class` 会被插值覆盖，目标另存 |
| `Tree::animated()` | 本帧要驱动的节点；缓存子树也登记 |
| `AppLoop::resolve_size_transitions`（`view.rs`） | apply 后、layout 前写入插值 |

阶段 B1 代码：

| 位置 | 作用 |
|---|---|
| `ui-core/src/tree.rs` `Enter` | 起始值（`AnimSpec` + 不透明度 + 位移） |
| `ContainerParams::enter` / `Decl::enter` / `Container::enter` | 声明槽与构建入口 |
| `Node::presence` / `presence_alpha` / `presence_shift` | 引擎每帧写回，绘制读 |
| `Node::visual_rect()` / `paint_offset()` | 位移在此折入；布局与命中各走各的 |
| `Tree::appeared()` | 本帧带 `enter` 出现的节点（引擎据此 `forget` 重新进场） |
| `AppLoop::resolve_presence` | apply 后驱动 presence，写回 alpha 与位移 |
| `AnimStore::travel` / `forget` | 从显式起点出发；复用的 id 重新进场 |

阶段 B3 代码：

| 位置 | 作用 |
|---|---|
| `ContainerParams::exit` / `Decl::exit` / `Container::exit` | 退场声明 |
| `Node::held` / `Tree::held()` | 被持有的子树：保活、不参与布局与命中 |
| `Tree::hold`（apply 的回收循环里） | 声明消失且声明了 `exit` 时不回收，改为持有 |
| `Tree::detach_held` | rebuild 之后把被持有的子树摘出同级索引与全树 id 表 |
| `Tree::release` | 落定后回收子树并 mark dirty，兄弟随之重排 |
| `AppLoop::resolve_presence` 的退场段 | 推 presence 到 0；到 0 即 release |
| `AppLoop::paint_tree` | 被持有的根排在延后队列末尾，画在活动内容之上 |

### 4.2 为什么目标要另存

`size_class` 是布局的输入。若把插值写回它，下一帧 apply 若走缓存路径不会重置，插值就成了
"声明值"，动画会冻结。`declared_size` 每帧由 apply 写入（缓存子树保持应用它时的值，
因为声明对象没变），因此目标永远正确。

### 4.3 缓存子树

`same_src` 快路径整棵跳过。过渡节点若躲在这样的子树里就再也不会被驱动，所以
`mark_reused_subtree` 也扫描 `size_transition` 并登记。反过来，这也意味着**过渡节点本身
不阻止缓存**——它只是被登记，开销是一次参数检查。

## 5. 验证

- `a_size_transition_reaches_its_target_through_the_layout`（`ui-platform`）：声明只写 100/200，
  半程必须约 150；去掉 `resolve_size_transitions` 即红在「retarget 从当前值起步」。
- `an_entering_node_is_painted_from_its_start_offset`：第一帧卡片画在起始位移处，
  presence 落定后回到声明矩形；去掉 `resolve_presence` 即红在「卡片没有下移」。
- `an_entering_node_fades_in_from_its_declared_opacity`：presence 0 时该像素与背景相同，
  半程出现颜色；同一处去掉调用即红在「循环没有被唤醒」。
- `an_entering_node_scales_about_its_centre_and_hits_where_it_is_painted`：半尺寸时角上的
  像素是背景、按在那里也命不中；落定后两者一起到位。去掉绘制端的变换即红在「那里没有像素」，
  让 `presence_scaling()` 恒假即红在「角上仍能命中」。
- `a_zoomed_text_runs_its_glyphs_through_the_same_transform`（`text_pass`）：同一段文字带
  `zoom` 解析出的字形矩形，必须等于未缩放字形经同一变换后的矩形。
- `a_leaving_node_keeps_painting_until_its_exit_lands`：声明撤掉后仍在画布上、引擎持有它、
  它已不在活树与布局里，淡完即消失并释放；`hold()` 直接返回 false 即红在「卡片没有离开」。
- `an_exit_starts_from_the_pixels_its_subtree_rested_on`：半透明遮罩（black54）落定后
  像素必须是**一次**合成（117），撤掉声明后第一帧必须与静止帧逐字节相同。去掉
  `detach_held` 里的 `paint.retain` 即红在 53 ≠ 117——被画了两遍的遮罩。
- `a_modal_scrim_fades_instead_of_appearing_dark`：模态遮罩淡入（0 → 半程 → 全黑），
  且**落定那一帧必须真的被提交**。淡入结束时组不透明度到 1，层随之消失；层不是图元，
  它的消失此前不留脏区，于是最后一帧 `damage` 与 `animating` 双双为假而被跳过——
  画面停在 0.97 的透明度上，直到下一次无关更新。去掉 `compute_damage` 里「消失的层也要
  标脏」即红在「落定帧没有被画」。
- `the_expressive_rail_widens_between_its_two_ends_and_the_pill_opens_with_it`
  （`ui-widgets-md3`）：收起时胶囊恰好是图标加两侧 16dp（56×32，列内居中），展开时横跨
  轨道减去 16dp 内缩（188×56），图标在两端都保持 16dp 前空；半程时声明宽度仍是目标
  （220），而胶囊高度与标签墨色停在 0..1 之间。去掉 `transition_size` 即红在「声明里没有
  过渡」——只声明目标不会让宽度走过去。
- 观感（阶段 C）：嵌套会话跑 `GALLERY_SELFTEST=1` 全绿（含 md3 dialogs 的开、点遮罩关、
  连开两次），`GALLERY_PAGE=dialogs` + 脚本点击截屏确认弹出后表面完全不透明（presence 落定）、
  遮罩铺满窗口；把 `ENTER`/`LEAVE` 临时拉到 10 秒截屏，确认中途帧真的按 presence 落像素
  （表面半透明且已缩小、遮罩只压暗一半）。
- `the_surface_opens_from_the_bar_over_the_results_on_one_clock`（`ui-widgets-md3`）：静止时
  表面是 28dp 圆角的 56dp 一行，分隔线与结果**根本不挂**；半程时圆角、分隔线与结果透明度都
  停在 0..1 之间，而声明宽高已是整个目标（380dp），头在 56..72 之间长；落定后圆角 0、头 72、
  结果全不透明。去掉 `transition_size` 即红在「声明里没有过渡」；把头的插值换成布尔值即红在
  「头已经到 72」；去掉 `t > 0` 门控即红在「静止的栏下面还有东西」。
- 观感（C2，nav rail）：`GALLERY_PAGE=nav-rail` 的 Expressive 样例点 `expand / collapse`
  后，220dp 面板上胶囊横跨轨道、图标 16dp 前空、标签在胶囊内；收起态 96dp 列里只剩图标。
  截屏核对两端，中途帧由上面的单测钉住（本机 `GALLERY_FOCUS` 注入点击在启动期有时抢在
  窗口映射之前，导致窗口不出现——探针问题，不是控件问题）。
- `a_scale_on_its_own_clock_outlives_the_fade_that_rides_the_presence`（`ui-platform`）：
  同一个节点，淡 100 ms、缩放 300 ms；150 ms 时不透明度已到 1 而缩放仍在 1..2 之间。
  把 `scale_spec` 丢掉（只用 `value`）即红在「缩放已经落定」。
- `a_menu_opened_after_an_idle_gap_starts_its_entrance_at_the_click`（`ui-platform`）：
  空闲 250 ms 后右键（entrance 167 ms），菜单的 presence 必须仍接近 0。把 `open_menu` 的
  `opened_at` 换回 `self.frame_now` 即红在 `got 1`——旧的行为就是用户报的「直接出现」。
- `a_dialog_declares_its_way_in_and_out_instead_of_arriving_at_rest`
  （`ui-widgets-fluent`）：根带 83 ms 线性淡入淡出（`enter` 与 `exit` 都在），表面在自己的
  时钟上从 `ScaleTransform` 1.05 走到 1（`DIALOG_SCALE_IN` 250 ms）。控件侧补间换回来
  即红在「没有起点」——上一帧不存在的节点上无值可插值，第一帧就是终态。
- 观感（C2d，Fluent/WinUI 对话框）：`GALLERY_PAGE=dialogs` 点 Fluent 列的
  `Show content dialog` 即弹出；把 `DIALOG_FADE`/`DIALOG_SCALE_IN` 临时拉到 1 s / 3 s 截屏，
  两张相隔 0.9 s 的图里表面从略大（1.05）收到静止、烟雾层同步加深——两条通道都在走。
- 观感（C2b，search view）：`GALLERY_PAGE=search` 最后一行是 search view，`GALLERY_FOCUS`
  点在静止的栏上即展开（380dp 一页：72dp 头、分隔线、结果），点头部右端的 × 收起；展开态
  截屏确认表面直角、分隔线在、结果按进度在。**注意** `GALLERY_CLICK` 是「按下 → 拖 300px →
  松开」的拖拽脚本，越过阈值后按下作废、**不会**产生点击——验证点击路径要用 `GALLERY_FOCUS`。

## 6. 分阶段

| 阶段 | 内容 | 状态 |
|---|---|---|
| A | 固定尺寸过渡 + 测试 | ✅ |
| B1 | `enter`：presence 0→1，起始不透明度 + 位移（绘制与命中） | ✅ |
| B2 | 绘制期 `scale`（进出场用，含绘制与命中） | ✅ |
| B3 | `exit`：引擎持有退出中的节点（脱离父节点），仅 `OutOfFlow` | ✅ |
| B4 | `KeepSpace` / `Collapse` 策略 | ⬜ |
| C | MD3 对话框淡入淡出（400 ms emphasized decelerate 进、200 ms emphasized accelerate 出） | ✅ |
| C2 | Expressive nav rail 宽度过渡（96 ↔ 220dp，300 ms Standard，胶囊随之张开） | ✅ |
| C2b | search 展开（同一表面 56 → 380dp，300 ms Standard；头 56→72、圆角 28→0、分隔线与结果按同一进度淡入） | ✅ |
| C2c | 对话框的入场缩放（MD3 声明 `Presence::scaled(0.8)`，与淡入同一时钟） | ✅ |
| C2d | WinUI 对话框的两条通道：淡 83 ms 线性 + 缩放 250 ms/167 ms 快出慢入；Fluent 对话框改为声明式 enter/exit，应用不再预热挂载 | ✅ |
| D | 内容尺寸动画（`hug` 轴：量到目标 → 反向布局子树，参照 Flutter `AnimatedSize`） | ⬜ |
