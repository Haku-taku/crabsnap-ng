# 引擎能力缺口与缺陷台账

> 本文件记录在「控件补全」过程中发现的**引擎（`ui-core` / `ui-platform` / `ui-widgets` / `ui-paint*`）
> 能力不足或缺陷**。每条必须能回答：症状是什么、为什么现在的引擎做不到、建议怎么改、我临时怎么绕。
> 修复后**不要删除**条目，改成 ✅ 并注明修在哪（`file:line`）——后来人需要知道这里曾经有个坑。

格式：

```
## N. 标题（一句话症状）
- 场景：哪个控件、什么操作
- 现状：引擎现在的行为 / 缺什么 API
- 影响：不修的话只能怎么绕，绕法有什么代价
- 建议：具体改动点
- 状态：⬜ 待修 / ✅ 已修（commit/file:line）
```

---

## 1. 动画只能驱动 `f32` 与颜色，不能驱动尺寸/矩形/枚举

- 场景：slider 的 handle 尺寸随按压/尺寸档变化；app-bar 随滚动折叠（高度 64→112→152）。
- 现状：`NodeCx::animate` 只接受 `f32` 目标，`animate_colour` 接受颜色。尺寸要拆成两个 `f32`
  实例（switch 就是这么做的：`SIZE_OFF`/`SIZE_ON`），矩形要拆成四个，枚举变化则**无法插值**，
  只能靠「锁存实例 + 纯函数」把离散跳变挡在动画之外。
- 影响：每个控件都要自己拆字段、自己锁存；离散状态一旦进入绘制路径就会跳变。
- 修法（2026-11-05）：固定轴尺寸先落地 `transition_size`（声明目标、引擎在布局前插值），
  见 `docs/transition-design.md` §3.1。矩形 = 两轴尺寸 + 偏移；枚举仍走关键帧。
- 状态：🟡 尺寸已修；矩形/枚举待接 `animate_stops`

## 2. 没有「滚动位置」作为可动画/可读取的输入

- 场景：app-bar 的 medium/large 变体随滚动折叠与展开；carousel 的分页吸附。
- 现状：`scroll_y` 只是 `Decl` 的**输入**（`tree.rs:199`），由应用从滚轮事件里自己累积
  （`ui-widgets/src/list.rs:14`）。没有任何 API 让控件**读回**某个滚动容器的偏移，也没有
  「内容滚动到哪里」的进度可供插值。
- 影响：app bar 只能收一个离散的 `collapsed: bool`，高度/字级靠内部 0.3s 动画补齐；连续
  跟随滚动（Compose 的 `TopAppBarScrollBehavior`、Flutter 的 `SliverAppBar` 联动）做不到。
  carousel 的分页吸附同样拿不到拖动进度，只能由应用算出「第几页」再交给控件。
- 建议：控件统一接受「滚动了多少」作为参数就够用（app bar 的 bool 是它的退化形式）；
  若要连续联动，需要在 `Routing`/`Cx` 里暴露一个按 id 查询的滚动偏移表。
- 布局重写后的进展（2026-10-31 对账）：`Node.scroll_extent` 已落盘且被滚动钳位读
  （`layout_engine.rs:1274`），控件读不回偏移这一点**未变**。
- 2026-11-01 对账：读回已落地——`Cx::scroll_of(id) -> Option<ScrollGeom>` 给出偏移、视口、
  内容范围与活动状态，并记 `Dep::Scroll` 依赖；carousel 据此自己算槽位。
- 2026-11-01 补齐：**可动画的滚动目标**已落地——`Cx::scroll_to(id, x, y)` 走 `ScrollRequest
  { animate: true }`，`View::start_settle` 用回弹同一条弹簧（`BOUNCE_STIFFNESS/DAMPING`）
  从当前偏移送到目标并计入 `is_animating`；carousel 的点击居中因此是缓动而非瞬跳。
  `set_scroll` 仍是瞬跳，供翻页按钮与循环条带的重设使用。
- 状态：✅ 已修（读回 + 动画目标均落地；app bar 的 `collapsed: bool` 仍可继续用）

## 3. 控件在 `decl()` 时不知道自己的最终尺寸

- 场景：slider 的轨道长度、手柄位置、按下点→值的映射都依赖控件宽度；progress/carousel 同理。
- 现状：`Widget::decl(&self, cx: &Cx)` 只能**声明**尺寸（`SizeClass`），拿不到布局结果；
  `change()` 虽然有 `bounds: Rect`，但绘制发生在布局之前，两者对不上就会「手柄不在手指下」。
- 影响：宽度只能由构造参数写死（`Md3Slider::with_width(.., width, ..)`，
  `Md3TabBar::new_with(.., width, ..)`），所以 `Grow::Yes` 对这类控件无意义，gallery 也必须
  显式给宽度。若父容器把控件压缩（布局的「亏空则压向各自下限」），绘制仍按声明宽度画，会溢出节点。
- 另一处：tab bar 的下划线要滑到选中页签，行程是 `width / tabs.len()`；拿不到宽度就整条不画。
- 建议：让控件能声明「按父给的空间自适应」，例如 `SizeClass` 的 `fill` + 一个
  `decl(&self, cx, laid_out: Rect)` 的两趟协议；或至少把布局结果回传给 `decl`。
- 布局重写后的变化（2026-10-31 对账）：**部分缓解**。P1 的 `min_content`/`max_content`
  与 P4 的 `InlineLayout`/`Node.baseline` 已落盘，控件仍拿不到（不在 `Routing`），
  但引擎侧的数据已齐——把 `Cx::text_layout(id)` 暴露出来只差接线，不再需要先补布局能力。
- 状态：⬜ 待修（slider 与 tab bar 仍用显式宽度绕开；引擎侧数据已就位）

## 4. `Change` 没有三态（半选）语义

- 场景：checkbox 的 `indeterminate`（MD3 有该状态，`checkbox_spec` 里没有对应令牌组）。
- 现状：`Change::Toggled { on: Option<bool> }`，`None` 即不确定态；两态控件只报 `Some`。
- 影响：三态控件只能上报 `Data` 或由 app 自己推断。
- 建议：`Toggled { on: Option<bool> }` 或新增 `Change::TriToggled`。
- 状态：✅ 已修（2026-11-03）。取第一种：`bool?` 正是 WinUI `IsChecked` 与 Rust `Option<bool>`
  的对应。Fluent 的 `CheckBoxParams::three_state()` 按 `ToggleButton.OnToggle` 循环
  （未选 → 选中 → 不确定 → 未选）；MD3 仍按 material-web，点击不确定态落定为选中。
  验证：单测 `a_three_state_box_reaches_indeterminate_only_when_it_is_asked_to`；selftest
  `fluent three-state box walks off→on→mixed ✓`（把 `Checked → None` 改成 `Some(false)` 先红）。

## 5. GPU 裁剪框按「尺寸」取整，丢掉分数远边的最后一行

- 场景：tab bar 的 1dp 分隔线在 GPU 上整行消失；CPU 路径正常。凡「节点矩形下端是分数」的
  控件都会掉最后一行（这台机器上文字行高 19.6，几乎每个盒子的 y 都是分数）。
- 现状：`set_scissor` 用 `floor(origin)` 配 `ceil(size)` 算设备像素裁剪框。裁剪框 207.8..256.8
  本该覆盖第 256 行，但 `207 + ceil(49) = 256` 是**开区间上界**，于是第 256 行被裁掉。
  同一错误还出现在层边界与 backdrop 写回两处（`as i32` 截断）。
- 影响：细线（分隔线、下划线、1dp 边框）在 GPU 上随机消失，而 CPU 与 GPU 的截图会「差一行」，
  很容易被误判成着色器 AA 问题。
- 建议/已改：统一走 `device_scissor(rect, scale)`——远边 `ceil`、原点 `floor`（与 CPU `clip_to`、
  Skia `roundOut` 同义）。`crates/ui-paint-gpu/src/renderer.rs:1961`；场景检查见
  `crates/ui-paint-gpu/examples/gpu_clip_scene.rs` 第 6 条（分数远边的最后一行必须被画出）。
- 状态：✅ 已修（`renderer.rs` `device_scissor`，单测 `a_scissor_keeps_the_row_its_far_edge_covers`）

## 6. 键盘 `Activated` 绕过控件，直接发给 app ✅ 已修

- 场景：tab bar / segmented button 这类「一个控件、多个可聚焦子节点」的控件。焦点在某个页签上
  按 Enter/Space，语义应该是「选中这个页签」。
- 原状：`ui-platform/src/view.rs` 在焦点节点上收到 Enter/Space 时直接
  `self.app.changed(id, Change::Activated)`，**不经过** `Widget::change`。控件因此拿不到这次按键，
  无法把它翻译成 `Change::Selected`。
- 修法：路由事实里加 `activated: Option<Id>` 与 `shortcut: Option<(Id, ShortcutId)>`
  （`cx.rs` 的 `Routing` / `Dep`），`WidgetState` 多出 `activated: bool` 与 `shortcut` 两个字段；
  `Decl::shortcut` 绑定的组合键沿焦点链解析后同样以「声明者 id」送到 `change()`。
  app 仍收到 `Change::Activated`，但控件已先有机会认领。
- 验证：`a_chord_bound_on_a_container_reaches_the_control_that_owns_it`（去掉 `self.shortcut`
  赋值即红）；`enter_presses_the_default_button_and_escape_the_close_one`（Fluent ContentDialog
  的 Enter/Escape 走同一条通道）。
- 状态：✅ 已修

## 7. `Grow` 没有「零基准」增长，等分容器做不到

- 场景：tab bar 要求各页签**等宽**（Flutter 用 `Expanded`），与标签长短无关。
- 现状：`distribute()` 的主轴基准是 `max(声明 hint, 实测内容)`，所以 `Grow::Yes`/`Flex` 是
  「在内容之上再增长」：短标签的页签窄、最后一个吃掉剩余。声明固定宽度则要控件自己知道总宽
  （缺口 #3）。
- 影响：任何等分布局（tabs、segmented、toolbar 的等宽槽位）都做不到。
- 建议/已改：新增 `Grow::Split(u16)`，基准为 0（CSS `flex: 1 1 0`）；亏空时先退回实测宽度再压缩。
- 状态：✅ 已修（`crates/ui-core/src/layout.rs`、`layout_engine.rs`；单测
  `a_zero_basis_grower_splits_the_row_evenly_whatever_its_content`）


## 8. 控件内部的分区命中（尾随图标、可点击的行内片段）

- 场景：input chip 的尾随图标是「删除」，芯片本体是「选中」；两者共用一颗胶囊。类似的还有
  下拉框的箭头、文本字段的清空按钮。
- 现状：引擎把一次按下交给**最上层命中节点**，而控件只有一个根 id；`Widget::change` 只拿到
  该根的 `WidgetState`（含 `local` 坐标），但控件在声明阶段无法知道标签宽度，因此拿不到尾随
  标记的精确位置。目前只能把整颗芯片当作一个命中区。
- 影响：input/filter chip 的尾随图标只能整块触发，无法单独响应；把尾随图标做成独立节点则
  引擎会把点击交给子节点，控件的 `change` 收不到（同 #6 的「按键绕过控件」）。
- 建议：让 `change()` 能声明「子区域 → 语义」的映射，或允许控件在 `decl` 里为子节点注册
  `Change` 处理函数。
- 状态：⬜ 待修（chips 的尾随图标暂与本体同一命中区，已记录在进度文档 §4.6）


## 9. 有上限的容器把整窗宽度交给子节点测量

- 场景：`fixed(380)` 的列里放一条长说明文字。列的内衬只有 356，文字却按整窗宽度排成一行。
- 现状（修前）：`measure_node` 把**父传下来的** `available` 直接减内衬后交给子节点，没有先用
  自身 `size_class.max` 夹一次。于是 `fixed(380)` 的列仍按 1500 宽量文字，测得 415.8 宽；
  列自己被 `max` 夹回 380，子节点却停在 415.8 并溢出。列表行因此比列宽 86px，被裁剪、
  被挤压，连锁引发下面第 10 条的圆角错乱。
- 建议/已改：容器测量前先 `available = available.min(sc.width.max)`（高度同理）。
- 状态：✅ 已修（`crates/ui-core/src/layout_engine.rs`；单测
  `a_fixed_width_column_measures_its_text_within_its_own_width`，破坏后红：wrap 从 200 变 800）


## 10. 裁剪形状取「最内层圆角」，更窄的方角裁剪会继承外层圆角

- 场景：圆角面板（12dp）里的列表行，行自身的 `mask` 比它的 `rect` 矮 1.5px（内容溢出被压），
  于是行会 `push_clip` 一个方角裁剪。结果这一行被画成 12dp 圆角——圆角来自外层面板。
- 现状（修前）：`plan_batches` 把所有裁剪矩形求交，但圆角取「最内层**非零**圆角」，
  不检查那层裁剪是否仍界定交集。外层圆角的形状因此贴到了更窄的方角矩形上。
- 建议/已改：圆角取「最内层、且其矩形仍等于交集」的那层裁剪；找不到则用方角。
- 状态：✅ 已修（`crates/ui-core/src/draw.rs`；单测
  `a_narrower_square_clip_drops_the_outer_rounded_shape` 与
  `the_tight_rounded_clip_keeps_its_corners`，分别破坏后各自变红）


## 11. 容器没有主轴定位，居中一块固定尺寸的面板只能靠增长者凑

- 场景：dialog 的表面要在铺满窗口的遮罩里水平垂直居中（Flutter 的 `Alignment.center`）。
- 现状：`ContainerParams` 只有 `cross_align` / `cross_pos`，**没有主轴位置**；主轴只能靠
  `Grow` 分余量。固定尺寸的子节点不增长，只能被摆在起点。
- 影响：现在的绕法是「两个 `Grow::Yes` 的零基准增长者夹住面板」——增长者测得 0、基准 0，
  余量按 1:1 分给两边，面板因此居中；亏空时面板被压进区域而不是溢出。代价是每个居中场景
  多两个节点，且语义不直白（读代码要靠 `spacer()` 的注释才看得懂）。
- 建议：给 `ContainerParams` 加 `main_pos: f32`（0 起点 / 0.5 居中 / 1 末端），
  `place_node` 里在算出 `main_avail - 主轴总长` 之后按它偏移。修好后 dialog 可以删掉两个
  spacer，app-bar 的折叠与 carousel 的吸附也会用上。
- 布局重写后的变化（2026-10-31 对账）：`main_pos` 仍未做；但 P5 的 `Placement`+`inset`+auto
  margin 已经给了**第二条正路**——`absolute` 配 `inset:0` + `margin:auto` 即可居中定尺寸面板
  （gallery Positioning 页有样本，测试 `an_absolute_child_uses_the_padding_box…`）。
  dialog 的两个 spacer 绕法可以换成它，但主轴 `space-between` 类分布仍缺。
- 状态：⬜ 待修（dialog 已用增长者绕过；绝对定位居中是替代方案）


## 12. 声明阶段拿不到文字尺寸，`OverflowBar` 那种「放不下就折列」做不到

- 场景：dialog 的三个文字动作（`Not now / Save draft / Publish`）在 320dp 表面里排成一行
  需要 361.6，内宽只有 272；Flutter 的 `OverflowBar` 会在放不下时改成竖列。
- 现状：`Widget::decl(&self, cx: &Cx)` 只有布局**之前**的信息，`Cx` 不暴露任何文本测量；
  控件无法判断「这一行放不放得下」，也就无法选择横排还是竖排。
- 影响：表面宽度只能由应用给定，动作标签必须能放进 `width − 48`；否则按钮被压扁、文字被
  裁掉（dialog 的 gallery 用短标签避开，selftest 断言每个动作都在表面内）。同样的问题会出现在
  app-bar 的动作溢出、chips 的换行、text-field 的标签收缩上。
- 建议：给 `Cx` 加一个只读的测量入口（`cx.measure_text(&TextBlock, max_width) -> Size`，
  用引擎当前的 `TextMeasure`），或允许控件在 `decl` 之后收到一次「测量完成」回调再决定布局。
  前者更简单，代价是 `decl` 变成「可以问测量」的纯函数。
- 修法（2026-11-01）：`ui-core/src/cx.rs` 的 `Cx::attach_measure` + `Cx::text_size` 把布局
  用的同一个测量器交给构建期；宿主在 `ui-platform/src/view.rs` 两处接线。使用方：
  `ui-widgets/src/text.rs` 的 `Text::width_in`、`ui-widgets-md3/src/button.rs` 的
  `Md3Button::width_in`；dialog 的动作带据此实现 OverflowBar 折列。
- 状态：✅ 已修（`ui-core/src/cx.rs:216` `attach_measure`、`:227` `text_size`；
  回归测试 `actions_that_cannot_share_a_line_stack_at_the_far_end`）


## 13. 没有进入/退出过渡，模态是「瞬间出现、瞬间消失」

- 场景：MD3 的对话框有 150ms 的淡入与缩放，遮罩随之淡入；关闭时反过来。
- 现状：控件在被构建出来的那一帧就已经是最终外观。`AnimStore` 只能把某个值从 A 动画到 B，
  而「这一帧第一次出现」这件事控件自己不知道（它不跨帧持有状态，符合设计），引擎也没有
  「挂载」通知或「退出后卸载」的机制。
- 影响：dialog 现在没有任何出现动画；应用也无法让它在关闭时先播一段再卸载，只能立刻移除。
- 修法（2026-11-05）：引擎提供 `Container::enter(Enter)`——起始不透明度与位移显式声明，
  出现的那一帧 `presence = 0`，引擎逐帧推到 1（`docs/transition-design.md` §3.2）。
  退场由引擎持有：`Container::exit(Presence)`，节点离开声明后仍保活、脱离布局与命中，
  在原处淡出再回收（该文档 §3.3）。缩放部分归 #27。
- 状态：✅ 入场与退场已修（`ui-core/src/tree.rs` 的 `Presence` / `Node::presence` /
  `Tree::hold`，`ui-platform/src/view.rs` 的 `resolve_presence`）。
  回归测试 `an_entering_node_is_painted_from_its_start_offset`、
  `an_entering_node_fades_in_from_its_declared_opacity`、
  `a_leaving_node_keeps_painting_until_its_exit_lands`


## 14. 子树被移除后重新出现时，会有一个节点永远缺失 ✅ 已修

- 场景：gallery 的对话框页第二次点「Show modal」后，表面内容错位、闪烁。
- 现状（修前）：`Tree::apply` 把「本帧没见到的节点」整棵交 `retire_subtree`，而调用方给的
  列表里既含子树根、也含祖先已经释放过的后代；`retire_subtree` 对每个条目都无条件
  `free.push(index)`，同一个索引于是在 `free` 里出现两次，下次 `spawn` 把同一个槽位发给两个
  节点——`by_parent` / `children` 互相覆盖，子节点丢一个。
- 影响：任何「整棵移除再整棵加回」的界面（模态、路由页、条件面板）都会随机少一个节点，
  症状是内容错位而不是崩溃，极难归因到引擎。
- 建议/修法：`retire_subtree` 先比对 `generations[index] == nid.generation()`，不匹配就跳过；
  返回实际回收数并累加进 `stats.retired`；`spawn` 加 `debug_assert` 确认拿到的槽位是空的。
- 状态：✅ 已修（`ui/crates/ui-core/src/tree.rs` 的 `retire_subtree` / `spawn`）；
  回归测试 `a_subtree_that_comes_back_keeps_every_child_it_declares` 先红后绿。


## 15. 文本栈没有字体族、可变轴与注册入口，图标字体因此不可用

- 场景：MD3 控件需要一个图标（复选框勾之外的所有 `md-icon`）；用户报告「常规图标无法正常使用」。
- 现状：六个缺口叠在一起，缺任何一个都做不成（完整设计见
  [`docs/icon-text-design.md`](icon-text-design.md)）：

  | # | 缺口 | 位置 |
  |---|---|---|
  | G1 | 没有字体注册入口（只有 `new_system` / `new_isolated`） | `ui-text-fontique/src/lib.rs:121,131` |
  | G2 | `TextStyle` 无 `family`，`FontRequest.families` 只被读一次 | `ui-core/src/text.rs:32-40` |
  | G3 | 只设 `wght` 轴，`FILL`/`GRAD`/`opsz` 不可用 | `ui-text-fontique/src/shape.rs:216` |
  | G4 | 图集键不含轴指纹 | `ui-text-fontique/src/glyph.rs:356` |
  | G5 | 无 OpenType 特性开关 | `ui-text-fontique/src/shape.rs` |
  | G6 | `script_face` 全局缓存会被图标字面污染 | `ui-platform/src/text_pass.rs:44,539` |

- 影响：图标只能继续手绘成矩形（现状），每加一个图标就多一段几何代码，且与官方字形不像。
- 建议：按 `docs/icon-text-design.md` §3 的七步迁移；G6 单独成一步，因为它是**静默**缺陷——
  图标字面一旦进 `script_face`，所有拉丁文本都会先试它（该字体有 66 个 ASCII 占位字形，
  「有字形」的判据恒真，不会回退）。
- 临时绕过：本机三族 Material Symbols 已随系统安装，因此**打包字体（G1）不阻塞**图标落地；
  G2–G6 必须先修。
- 状态：✅ 已修（G1–G6，图标已落地 `ui-widgets::icon::Icon` + 画廊 `icons` 页）。
  - G2/G3：`TextStyle` 带 `Family` 与 `Variations`（`opsz`/`FILL`/`GRAD`）；重量仍在
    `TextStyle.weight`，因为重量选**字面**而不是轴值。
  - G4 的落法与原设计不同：轴位置在 provider 里**驻留成新的 `FaceId`**（量化到 1/64），
    图集键因此不用加轴维度。回归测试 `an_axis_position_gets_its_own_face_handle`、
    `the_fill_axis_changes_the_rasterized_coverage`。
  - G1：`FontiqueProvider::register` + `TextPass::register`；
    `a_registered_family_resolves_on_an_isolated_provider` 钉住「无系统字体也能解析」。
  - G6：`script_face` 以 `(Family, Script, weight, italic)` 为键；
    `an_icon_run_does_not_poison_the_latin_stack` 钉住图标不会改写拉丁文本的整形结果。
  - 遗留：`Text` 的换行宽度不等于节点宽度时，`TextAlign::Center` 会把字形排到节点矩形
    之外并被祖先遮罩裁掉（见 `docs/lessons/design-errors.md`）；`Icon` 因此不居中，
    盒子就是字形的推进宽度。垂直居中仍无引擎支持。

## 16. 控件拿不到「按下点」，只能拿到当前指针位置

- 场景：范围滑块的拖拽要锁定「按下时离谁近」的那个手柄；涟漪要从按下点长出来。
- 现状：`WidgetState::local` 是**当前**指针位置（`ui-platform/src/state.rs:27-29`），
  按下那一刻的坐标没有被保存；`Cx` 也没有 `press_origin()`。
  （2026-10-29 起引擎内部已为阈值保存了 `View::press_at`，但仍未进入 `Routing`/`Cx`。）
- 影响：范围滑块只能每帧按「离谁近」重选，拖过另一个手柄时归属会翻转（跨过时会夹住，
  因此常见拖拽仍稳定）。涟漪这一半**已解**：`Decl::press_ink` 把落点留在引擎里，
  控件不再需要知道按下点（`docs/pointer-model-design.md` §3.4，2026-10-29）。
- 建议：`Routing` 记 `press_origin: Option<Point>`（按下时写、松手清），
  `Cx` 暴露 `press_origin_within(id)`；`WidgetState` 加一个同名字段。
  引擎侧的数据源已经有了（`View::press_at`），只差向外传递。
- 临时绕过：范围滑块用最近手柄规则（自稳定）。
- 2026-10-31 对账：`press_origin` 仍未进 `Routing`/`Cx`（`grep press_origin` 空）。数据源
  `View::press_at` 仍在，只差向外那一步。
- 状态：⬜ 待修（数据已在引擎内，未暴露）；涟漪部分 ✅ `ui-platform/src/view.rs` 的 `Splat`

## 17. `decl()` 里读到的 `local` 是「拖拽所有者」的空间，不是本节点的

- 场景：Fluent 开关在 `decl()` 里用 `state.local.x / TRACK.0` 把指针换算成进度。
- 现状：`WidgetState::of` 直接取 `Cx::local()`，那是**每帧一次**算出的值
  （`View::pointer_local`：拖拽所有者，否则按下节点）。只有 `collect_changes`
  在调 `change()` 之前按本节点矩形重算了一次（`view.rs:183-187`），`decl()` 走的
  `NodeCx::state()` 没有这一步。
- 影响：控件嵌在可拖拽祖先里时，`decl()` 读到的 `local` 相对的是祖先的原点，
  按自身宽度换算会整体偏移。目前没有控件因此出错，因为读它的地方都由
  `state.dragging` 门控，而 `dragging` 只对所有者成立——这是**巧合**而非约束。
- 建议：把「按节点重算 `local`」下沉到 `WidgetState::of`（需要传入节点矩形），
  或让 `NodeCx::at(id)` 携带矩形并在构造状态时换算。
- 临时绕过：`state.rs` 的字段注释已写明两条路径的差别。
- 状态：⬜ 待修

## 18. 只能纵向滚动，横向条带没有「移出视口」的通道 ✅ 已修

- 场景：carousel 的 uncontained / full-screen 布局，条目要滑过前缘并保持尺寸。
- 现状：`ContainerParams` 只有 `scroll_y`；横向条带只能靠 `Decl::offset` 挪，而 `offset`
  是**视觉位移**，遮罩跟着一起平移，结果是整条带连同裁剪框一起走，条目永远不会被容器裁掉。
- 影响：uncontained 的条目会画到容器外面，盖住相邻内容；没有别的绕法。
- 建议/修法：`ContainerParams` 加 `scroll_x`（`tree.rs`），`content_origin` 从 x 里减掉它
  （`ui-core/src/layout_engine.rs`），遮罩仍由容器矩形给出。
- 状态：✅ 已修（`ui-core/src/layout_engine.rs` 的 `content_origin`；测试
  `a_horizontal_scroller_shifts_children_left_and_clips_at_the_box` 钉住「子节点左移 + 首尾遮罩」）
- ~~仍未接：滚轮增量没有通道~~ **已接**（2026-10-31 对账）：`Event::Scroll` →
  `scroll_target()`（`view.rs:2578`，按 `clip.scrolls()` 或 `Sense::SCROLL` 找目标）→
  `Change::Value`。gallery 导航列（P6）就是这条通道的消费者；carousel 仍用 prev/next。

## 19. 没有文本输入：控件画得出字符，却收不到键盘 ✅ 已修（单行最小集）

- 场景：text fields 需要真正的可编辑缓冲。
- 现状：`Event::Text` 与 `KeyPressed` 只喂给「已聚焦节点的语义控件」；没有任何节点能声明
  「我接受文本」，因此键盘事件到达 app 之前就无处可去。焦点节点收到的是 `Activated`
  之类的语义变化，不是字符。
- 影响：任何输入控件都只能做成「静态样子」。
- 修法（已落地）：`ContainerParams::text_input: Option<String>` 声明当前缓冲；
  `Routing::caret: Option<(Id, usize)>` 把插入符位置交给构建期；`Cx::caret_for(id)` 让控件
  读它；引擎在 `Event::Text` / Backspace / ← → 上改写插入符，并以 `ui_core::change::Change::Text`
  把**整段新缓冲**交给 app（app 仍持有值，下一帧重新声明）。绘制端 `TextPrim.caret` 在字形流里
  画 2dp 插入符（按行区间整形前缀求笔位）。
- 状态：✅ 已修最小集；纯算术在 `ui-core/src/text_input.rs`（有单元测试），接线在
  `ui-platform/src/view.rs::dispatch`（测试 `typing_reaches_the_focused_text_input_as_the_whole_buffer`）。
- 仍缺：超出宽度时的水平滚动、密码掩码与多行。补这些之前，输入控件只能当单行短文本用。

**输入法组合（已修）**：`Event::Preedit` 让预编辑串留在引擎（`AppLoop.preedit`），应用的缓冲
只含已提交内容；绘制端按 `TextPrim.composing` 给组合区间加下划线，提交（`Event::Text`）时并入
并清空组合。`PresentTarget::set_ime(ImeState)` 每帧把「是否启用 / 插入符矩形 / surrounding」
交给后端；`enabled` 只在**本帧**算出插入符矩形时为真（候选窗定位规则见 `docs/ime-text-design.md` §6）。
验证：`composing_text_does_not_reach_the_app_until_it_commits`（先红后绿）；画廊自检
`ime composition underlined=…`（先红后绿）与截图（`にほ` 带下划线、`app buffer` 仍是 `"hi"`）。
Wayland 侧 `zwp_text_input_v3` **已接线**（`ui-platform-wayland/src/window.rs`：事件在 `done` 合批，
`enable` 单独提交后重发 surrounding/content_type/cursor_rect）；真实 fcitx5 下候选窗在插入符处，
旧项目「首次输入候选窗错位」的缺陷已复现并修掉（`docs/ime-text-design.md` §6）。
仍缺：剪贴板、多行、RTL。

**编辑状态机（已修）**：引擎持有 `editing: Option<(Id, TextSelection)>`（折叠的选区就是插入符），
支持 Shift+←/→ 扩展、Home/End、Delete、Ctrl+A、点击定位、拖拽选字，以及「在选区上输入/退格即整体替换」。
命令修饰键（Ctrl/Alt/Logo）下的字符由平台作为 `Key::Char(c)` 送来而**不再**是 `Event::Text`，
所以 Ctrl+A 不会插入 `a`。
验证：`typing_replaces_the_selected_range_in_one_edit`、`shift_extends_the_selection_and_a_plain_arrow_collapses_it`；
画廊自检 `md3 text field caret=… selection=…`（两者都先红后绿）。

**插入符闪烁（已修）**：引擎自己持有时钟——编辑期间插入符每 500 ms 明灭，插入符或选区一变即重开
周期（`caret_blink_lit` / `caret_fact`）；灭的那半程不画插入符，但照样登记「欠一次唤醒」，
`AppLoop::next_wake` 给出到下一次明灭的时长，`Driver::step` 把 `FrameHost::wait_until` 的上限交给它，
`turn` 因此在没有输入时也能按时醒来换相位。修前根本没有任何闪烁代码。
验证：`a_caret_blinks_while_an_input_is_edited_and_relights_when_it_moves`；画廊聚焦任意输入框后
相隔 0.7 s 的两张截图只在插入符那 3×20 像素上有差异。

**只读文本的选区（已修）**：`Sense::SELECT` 让普通文本显式加入命中；引擎持有**全局唯一**选区
（`Routing.selection`，`Cx::selection_for`），`AppLoop` 的按下/拖动/双击（按词）维护它；
`TextPass` 给出 `SelectionFragment` 几何，绘制由声明携带的 `SelectionPainter` 决定
（MD3 平直、Fluent 反白、Glass 圆角），没有绘制程序时回落平直矩形。
验证：`a_drag_across_selectable_text_records_the_range_it_crossed`；画廊 `Controls · Text selection`
页的自检要求三个包各自的洗色进入绘制列表、Fluent 额外出现白色字形。
仍缺：键盘选择（Shift+方向键/Ctrl+A）、多选区、复制到剪贴板、双向文本的命中。


## 20. 只有 xdg_toplevel：没有浮层/贴图表面 ✅ 已修（wlr-layer-shell）

- 场景：引擎要支撑截图 + 标注 + 贴图工具。这类工具的内容必须画在「由 compositor 摆放、
  不经窗口管理器、永远在最上层、不进任务栏」的表面上。
- 现状（修前）：`WaylandWindow::new` 只会 `xdg_wm_base.get_xdg_surface` + `get_toplevel`，
  每个表面都进任务栏、可被平铺、可被别的窗口盖住。
- 修法（已落地）：`WaylandWindow::new_layer(config, LayerConfig)`，绑
  `zwlr_layer_shell_v1`（1..=4），`get_layer_surface` 之后设置层 / 锚点 / 尺寸 / 独占区 /
  键盘交互；`configure` 事件给出尺寸（某一轴为 0 表示「锚点剩下多少算多少」）。
  `xdg_wm_base` 因此降为可选全局：纯浮层不需要窗口管理器。
- 验证：`cargo run -p ui-platform-wayland --example layer_overlay --features
  ui-platform-wayland/system-fonts` 在嵌套 Hyprland 里开出 2560×1440 的 overlay 层表面，
  面板（文本、圆角、边框、色块）正常绘制，并且**盖在同时打开的 gallery 窗口之上**
  （截图里整扇画廊被浮层的遮罩压暗）。
- 仍缺：多输出（只绑了第一个 `wl_output`，多显示器要逐输出建表面）、截屏协议本身
  （`wlr-screencopy` / `ext-image-copy-capture`）、指针约束与全局快捷键。

## 21. 颜色字体（emoji）没有光栅化通道 ✅ 已修

- 场景：文本里出现 emoji 或其他 COLR/CBDT 颜色字体字形。
- 现状：覆盖率切片已经能把 emoji 交给 Segoe UI Emoji / Twemoji，但 `GlyphCache` 走的是
  单色轮廓 + coverage 光栅化，颜色字体的字形没有轮廓，图集里是空的：位置占住了，墨迹没有。
- 影响：emoji 显示为空白（修前更糟：整段文本消失，见 `design-errors.md`）。
- 修法（建议）：图集增加 RGBA 条目，颜色字形按 `swash` 的 color layers 合成成预乘 RGBA；
  `GlyphPrim` 增加 `color: bool` 让绘制端走 RGBA 采样而不是 coverage 着色。
- 验证方式：字段里输入 `hi😀there`，emoji 位置出现彩色字形，其余字符不动。
- 修复：`ui-text-fontique/src/glyph.rs` 的 `ParsedFace::rasterize_color`（swash COLR v0 /
  CBDT / sbix）与 `GlyphCache` 的第二张 RGBA 页；`ui-core/src/draw.rs` 的 `GlyphKind`；
  `ui-paint` / `ui-paint-gpu` 按 kind 选图集采样；`ui-platform/src/text_pass.rs` 把已安装的
  emoji 字面加入回退候选（拉丁回退链不列它们），并单独上报颜色页代数。
- 仍缺：swash 0.2 的 COLR v1（paint graph）没有实现，只支持 COLR v0 层与位图字形。

## 22. 空闲窗口被合成器标 `(not responding)`；用户报退出卡顿，嵌套会话复现不出 ⬜ 调查中

- 场景：用户报告退出时卡顿（Ctrl+C 与超时自动退出都能触发）。嵌套会话里此前复现不出
  （SIGINT 43ms 退出、拆除 < 50ms）。
- 本轮实测（2026-10-31，嵌套 Hyprland，buttons 页）：
  - **`WAYLAND_DEBUG=1` 看协议线**：每个 `xdg_wm_base.ping` 都在 **13–17ms** 内回了
    `pong`（`window.rs:605` 的 `base.pong(serial)` 正常）。ping 间隔约 1.5s。
    → **怠慢 xdg Ping 这条排除了**：我们每次都及时答复。
  - 但 `/tmp/idle-t12.png` 里窗口标题仍显示 `crabsnap-ui gallery (not responding)`，
    且进程存活、`wait()` 每 250ms 轮询一次（`window.rs` `wait` 已设上限）。
  - `(not responding)` **不是我们设的标题**（`grep 'not responding'` 全仓库只命中
    `set_title(config.title)`，`window.rs:1000`），是**合成器/装饰层**根据某个判据加的标记。
  - 超时退出与 `kill` 都能让窗口干净消失（`clients | grep -c gallery` = 0），无残留。
- 结论（当前证据下）：**嵌套会话里复现不出「退出卡顿」，且 xdg Ping 及时答复。**
  用户看到的 `(not responding)` 与卡顿，其判据在**装饰/合成器一侧**（Hyprland 的标题栏或
  KDE/GNOME 之类真实桌面里的对应物），而非 gallery 怠慢协议。真实环境的 compositor 与这台
  嵌套 Hyprland 不同——用户的症状形状（窗口残留？界面冻结？终端不返回？进程仍在？）仍待用户
  给出，才能定向。
- 已做的防御性改动（此前一轮）：`wait_for_frame` 与 `wait` 都改成**有界**轮询
  （`poll_once(deadline)`，`wait` 上限 250ms），使 app 的退出截止能在帧间被发现；teardown 加了
  分段计时（presenter / ui / window，>50ms 打印）。本环境实测 SIGINT 退出 43ms、不触发计时打印。
- 下一步（拿到用户症状形状后）：① 若「进程仍在」——查退出路径是否卡在 EGL/swap 或剪贴板管道；
  ② 若「界面冻结但进程活着」——查 `service_clipboard` 与 `poll_once` 的 fd 集合；
  ③ 若「终端不返回但窗口已关」——是 shell/wait 侧，不是本引擎。
- 状态：⬜ 调查中（嵌套会话无法复现；协议层已排除怠慢 ping；等用户给症状形状）

## 23. 多行字段的「按行数长高」仍受缺口 #3（声明时不知道自身尺寸）挡着 ⬜ 待修

- 场景：`Md3TextField` 接多行（引擎侧 `text_input_multiline` + Shift+Enter + `\n` 行盒已落）。
  字段高度随 `\n` 数增长，才谈得上多行。
- 现状更正（我一度以为要新增 `caret_in_text` 的 y，**那是错的**，已收回）：绘制端本就按字节
  偏移算整条插入符矩形（`text_pass.rs:427 caret_rect_with`，逐行取 y），控件也已有该偏移
  （`self.caret`）。真正的缺口是**控件在 `decl()` 时拿不到「这段文本在当前宽度下排成几行」**
  ——即缺口 #3「控件在 `decl()` 时不知道自己的最终尺寸」在多行上的具体表现：
  `TEXT_FIELD_HEIGHT` 钉死 56，`view.rs:5099` 的单行分支还设 `nowrap: true`。
- 影响：多行内容要么被 56 的盒子裁掉，要么得让控件自己再排一遍断行数行——违背
  「布局与绘制共用唯一断行来源」。所以多行的**绘制/命中已经就绪，缺的是高度协商**。
- 建议：并入 #3 的解法（声明期可查询文本在给定宽度下的行数/高度，或允许字段高度为
  `fit` 让布局回传行数）。单靠加 `caret_in_text` 的 y 不解决问题。
- 状态：⬜ 待修（本质是 #3 的多行投影；引擎的断行/插入符/命中已支持多行）

## 24. 设计体系的菜单外观没有进引擎的通道 ✅ 已修（`App::menu_style`）

- 场景：右键菜单是引擎兜底绘制（`view.rs` `paint_menu`），外观应由设计体系给；
  MD3 早已从 `md.comp.menus` 解析出 `MenuStyle`（`menu_style.rs`），却没人递给引擎。
- 现状：`AppLoop::set_menu_style`（`view.rs:642`）**零调用方**，菜单一直画 `MenuStyle::default()`。
  `ui-platform` 不能反向依赖 `ui-widgets`，故 kit 无法在 `view()` 里推给引擎；
  引擎读 `menu_style` 的时机（`open_menu`，dispatch 内）又早于本帧 `view()`。
- 已做的绕开：gallery 侧每帧外观变化时调 `set_menu_style`（`main.rs sync_menu_style`，
  按 `dark` 去重、外观变化才重建包）。像素证实：同坐标未接线画兜底色 `(44,46,54)`、
  接线画 MD3 解析色 `(29,27,32)`。
- 遗留：① 主题过渡的 0.2s 中途，菜单色比控件慢半拍（`pack_of(dark)` 与过渡中的 scope 不同步）；
  ② 每个 app 都得自己记得接线——缺一个「引擎向本帧 `Cx` 取 `MenuStyle`」的正式通道。
- 修法（2026-11-05）：`App::menu_style(&self) -> Option<MenuStyle>` 成为声明，引擎在每次
  `view()` 之后、绘制之前复制一份；`sync_menu_style` 从 gallery 删除。过渡期「菜单慢半拍」
  同时消失——复制点就在本帧 `view()` 之后。
- 状态：✅ 通道在引擎里；`set_menu_style` 仍是给非 `App` 宿主的保留入口。

## 25. `Glass` 缺参考实现的整套交互指示器参数，着色器也不是那一个 ✅ 已修（Phase B）

- 场景：玻璃 switch / slider 的按压态（`liquid_glass_widgets` 的 standard 路径）。
- 现状（修前）：`draw::Glass` 只有 `tint/rim/ambient_rim/refractive_index/refraction/thickness/chromatic`，
  其中 `ambient_rim` 是"环的位置（设备像素）"；参考用的是一整套：
  `rimThickness`、`rimSmoothing`、`ambientRim`（强度）、`ambientStrength`、`baseAlphaMultiplier`、
  `edgeAlphaMultiplier`、`lightDirection/lightIntensity`、`glowIntensity`、`interactionIntensity`、
  `edgeAbsorption`。且 `GLASS_FRAGMENT` 是"穹顶 + 斯涅尔折射"的自研模型，
  参考的 `interactive_indicator.frag` 是"边缘法线位移 + 色散 + 合成倒角渐变 + 半月板吸收"。
- 影响：边缘永远是一条均匀 1px 描边，没有上亮下暗的倒角与环光——"玻璃感"的主体缺失。
- 修法：`Glass` 换成参考的 14 个字段（长度按设备像素，控件侧乘 scale 后传入），
  `Glass::STANDARD` 给出 standard 路径 + 有背景分支的默认值；uniform 合并成
  `u_glass/u_rim_params/u_alpha/u_light/u_effects/u_edge_zone`；
  `GLASS_FRAGMENT` 按 `shaders/interactive_indicator.frag:113-439` 逐项移植，
  采样改为 `v_device` 上的设备像素偏移（我们直接取背景层，不需要参考的截图原点与 DPR 双线性）。
- 验证：`cargo run -p ui-paint-gpu --example gpu_glass_scene` 的 11 条探针全绿
  （平面不受弯折影响、交互加弯、环光只提边缘、倒角顶部更亮、色散只分边缘、按压不采到画框外、不透明）；
  截图：静止白胶囊 38×23，按住后 52×32 的玻璃透镜，顶部边缘更亮。
- 状态：✅ 已修（`ui-core/src/draw.rs`、`ui-paint-gpu/src/{shader,renderer}.rs`）

## 26. 多形状 SDF 融合组（blend / morph）✅ 已修

- 场景：玻璃 morph、两个胶囊之间的融合（metaball）。
- 设计：形状随材质走，不另开几何纹理——`Glass` 带 `blend: [BlendShape; 4]`、
  `blend_count`、`blend_k`（dp），形状坐标相对**节点自身**；节点矩形只圈定捕获与剪裁。
- 着色：premium 趟里 `smoothUnion` 双向折叠后 0.5 混合（`sdf.glsl:236-300`，按 4 个形状展开，
  不做动态下标）；覆盖度取并集场本身，法线取场的数值梯度（解析法线只对单形状成立）。
- 上限 4 个形状（参考 16）：够 morph 的两个 blob 与标签栏透镜，再多要重排 uniform 与展开。
- 验证：`gpu_glass_scene` 探针 8/8b（k=48 时 20dp 间隙被填，且不越出组框），gallery 导航栏页
  的 morph 实拍可见液桥。
- 状态：✅ 已修（`ui-core/src/draw.rs`、`ui-paint-gpu/src/{shader,renderer}.rs`、
  `ui-widgets-glass/src/{shared,morph}.rs`）

## 27. 没有变换/缩放原语，`Transform.scale` 只能靠改盒子尺寸模拟 🟡 进行中

- 场景：slider 透镜 1.35× 膨胀、jelly 的 (x,y) 非等比形变；switch 的 `Transform.scale`。
- 现状：`Decl` 只能给固定尺寸；缩放要重算每个子形状的尺寸与位置。
  对"只有一个形状"的玻璃滑块尚可，对带文字/图标的组就得逐个重排，且描边宽度会被一起放大
  （参考里 `scale` 作用于整个子树）。slider 的白物质因此必须**单独**按未缩放尺寸摆放。
- 建议：给 `Decl` 加 `scale(origin, x, y)`（渲染期矩阵，参与命中与裁剪），
  或等价地给节点加 `transform` 字段；先支持定点缩放与平移。
- 修法（2026-11-05）：`ui_core::draw::Transform`（仿射 `{scale, offset}`）+
  `DrawList::transform`，压入时统一缩放矩形/圆角/描边/缺口/弧/阴影/图像/字形/裁剪/层/背景模糊；
  文本先成形，缩放随 `TextPrim::zoom` 由文本趟在发射时施加。进出场经 `Presence::scaled()`
  接到 `Node::presence_scale`；命中用同一套复合（`Tree::scaled_visual_rect`）。落点见
  `docs/transition-design.md` §3.4。
- 状态：🟡 等比、以节点中心为心的缩放已可用（进出场）；静态缩放与逐轴非等比（jelly、滑杆
  透镜 1.35×）仍缺——它们要 `Decl` 上的缩放声明与任意轴心

## 28. 没有跟随指针的高光层（`GlassGlow`）⬜ 待修（Phase B 附带）

- 场景：参考的按下高光——白色 12% 的径向光斑跟随手指，`transition > 0.05` 才出现。
- 现状：引擎没有"指针驱动的径向光"这一绘制原语；只能画整块状态层。
- 建议：随 #25 的移植一并做：给玻璃趟加 `touch: Option<(pos, radius, color)>`，
  在片元里按到触摸点的距离叠加即可（参考的 `GlassGlowLayer` 就是一层径向渐变）。
- 状态：⬜ 待修

## 29. 弹簧以零时长写入驱动时，帧循环认为"没有动作"，弹簧冻在振铃中途 ✅ 已修

- 场景：玻璃 slider 的 jelly 弹簧（`Spring::advance` 每帧写 `IMMEDIATE`）。
- 现状（修前）：`View::animating` 只看 `AnimStore::is_animating`，而零时长条目永远 `settled`；
  构建完这一帧后循环就睡，弹簧只推进了"有输入的那几帧"。截图里 thumb 永久停在 27×28
  （满形变）而不是 39×24。附带：固定 1/60 积分使动作速度随帧率变化。
- 修法：`AnimStore::spring(id, target, stiffness, damping, now)`——按**引擎秒**积分子步
  （≤1/240 s，单次 ≤0.1 s），静止时精确吸附到目标；`is_animating` 把未静止的弹簧算作动作，
  帧循环因此持续唤醒。`NodeCx::spring` 包装它并记录依赖。
- 验证：`anim.rs` 的两条测试——`a_spring_travels_the_same_distance_at_any_frame_rate`、
  `a_spring_keeps_the_frame_loop_awake_until_it_rests`；截图从"冻住的圆"变为 38×23 的胶囊。
- 状态：✅ 已修（`ui-core/src/anim.rs`、`ui-widgets/src/cx.rs`）

## 30. 玻璃只有 standard 指示器，没有参考的 premium 透镜（两趟几何 + 折射）✅ 已修

- 场景：`GlassQuality.premium`——参考在 Impeller 上真正跑的那套（iOS 26 观感）；
  standard 只是其余平台的近似。
- 现状（修前）：只有 #25 的 `interactive_indicator.frag` 移植，没有法线/高度、没有斯涅尔折射、
  没有半月板与菲涅尔环；`Glass` 也没有 `quality/thickness/refractive_index/fresnel/whiten`。
- 修法：`draw::Glass` 加 `quality: GlassQuality` 与透镜字段；`PREMIUM_GLASS_FRAGMENT` 按
  `shaders/liquid_glass_render.frag:340-946` 逐项移植（法线解码、`refract()` 位移、
  色散三采、保亮度着色、自适应饱和、半月板按 `sqrt(rimThickness)`、环光、菲涅尔、漂白罩）；
  倒角取 `sin_i = 1-u`、`cos_i = √(u(2-u))`、`height = cos_i·thickness`（与几何趟的
  `n_cos/n_sin` 同源），边缘带按编码高度 `cos_i` 而非 `u` 取阈值。
- **不照搬两趟几何纹理**：参考需要在离屏存法线/高度是因为 Impeller 的合成约束；
  我们有 SDF 参数，法线可以解析求出，精度更高且少一趟。两趟只在**多形状融合组**（#26）成立时
  才必要——那时法线来自多个 SDF 的 smoothUnion 梯度，解析解不再成立。
- 长度单位：premium 的 `thickness`/`ambientRim` 以 **3 倍屏像素**计量，渲染器传
  `值 × dpr / 3`（参考 `liquid_glass_render_object.dart:766,798`），否则在 2 倍屏上倒角宽 1.5 倍。
- 验证：`gpu_glass_scene` 的 12 条探针全绿。其中"透镜弯折边缘"必须量**倒角中段**：
  贴边处入射角越过临界角，`refract()` 不再给方向，边缘本身不弯——这是物理，不是缺陷。
- 状态：✅ 已修（`ui-core/src/draw.rs`、`ui-paint-gpu/src/{shader,renderer}.rs`、`ui-widgets-glass/src/material.rs`）

## 31. 控件拿不到父给的约束，只能靠宿主传宽度 ⬜ 待修

- 场景：玻璃标签栏的指示片位置是 `x = 4 + a·(innerW − cellW)`，`innerW` 来自布局约束。
- 现状：`Widget::decl(&self)` 只有自己的 `Id` 与状态，没有"父给我多宽"这一信息；
  格子可以用 `Grow::Split` 等分（不需要宽度），但**绝对定位**的指示片需要像素宽度，
  因此 `GlassTabBar` 目前由宿主传 `width`（gallery 传 `COL_W − 40`）。
- 影响：栏不会随窗口伸缩；参考里它是 `constraints.maxWidth`。
- 建议：`Decl` 支持百分比/分数偏移（`offset` 接受 `Length::Fill(k)`），或给 `Widget` 一个
  量后被调用的 `laid_out(&self, size)` 钩子——前者更小，也够这一处用。
- 状态：⬜ 待修（`ui-widgets-glass/src/tab_bar.rs`）

## 32. 滚动容器没有拖拽平移、惯性与触底回弹 ✅ 已修

- 原状：`Scroll` 只声明 `HOVER | SCROLL_OWNED`，偏移只由滚轮、滚动条拖拽与 `Cx::set_scroll`
  改动；钳位是硬 `clamp`，越过阈值但没有 drag owner 的按下直接作废。
- 修法：物理放进输入层（`ui-platform/src/view.rs`），任何 `SCROLL_OWNED` 容器自动获得，
  控件零改动。`ScrollPan`（跟随 + 橡皮筋）、`ScrollMotion`（惯性 → 弹簧）、
  `ContainerParams::scroll_elastic`（允许越界偏移进入布局），细节见
  `docs/scroll-container-design.md` §步骤 ⑧。
- 验证：`ui-platform/src/view.rs` 六条测试；gallery 拖拽导航抽屉的截图。
- 状态：✅ 已修

## 33. SF Symbols 缺字体源，而不是缺注册通道 ✅ 已修

- 原状：`ui-text-fontique` 早已有 `FontiqueProvider::register`，`TextPass::register` 已接到
  `AppLoop::register_font(name, bytes)`（`ui-platform/src/view.rs`）——通道在，缺的是字体
  文件与名字表。
- 修法：经用户同意，把参考包的 `SFSymbols.ttf` 收进 `ui/crates/ui-gallery/assets/fonts/`
  （专有素材，只限本工作副本，见同目录 README）；gallery 启动时以
  `ui_widgets::SF_SYMBOLS_FAMILY` 注册。名字→码位表**从字体自己的 cmap 生成**
  （`scripts/gen_sf_symbol_codepoints.py` → `ui-widgets/src/sf_symbol_codepoints.rs`，
  6795 条），而不是抄参考包的 22 条 Dart 常量；`IconSource::sf(name)` 接受 Apple 的点分
  写法（`house.fill` ≡ `house_fill`）。
- 验证：`a_known_sf_symbol_maps_to_the_codepoint_the_font_assigns`、
  `every_name_in_the_sf_table_is_found_by_the_binary_search`；gallery `nav-bar` 页的玻璃栏
  已画出 `house` / `magnifyingglass` / `person`。
- 状态：✅ 已修

## 34. 构建期拿不到文本宽度，描边缺口只能靠不透明底色伪造 ✅ 已修

- 原状：MD3 outlined 文本框用一块 `self.backdrop` 不透明色盖住描边来模拟缺口；
  `Cx` 没有任何文本测量查询。
- 修法（不是打补丁，而是补上缺的那层能力）：
  ① `ui_core::cx::Cx::attach_measure` / `text_size(block, max_width)`：`AppLoop` 把**同一个**
  `TextPass`（不是第二个 pass）以 `Rc<RefCell<dyn TextMeasure>>` 交给每一帧的构建，
  量到的宽与绘制用的行完全一致；`ui_widgets::Cx::text_size` 与 `Text::width_in` 是它的门面。
  ② `RectPrim.notch: Option<(f32, f32)>`：描边顶边在给定 x 区间中断，CPU 与 GPU 两条路都实现
  （着色器多两个实例槽位 `a_notch`）；`Shape::Outline { corners, colour, width, gap }`
  是它在声明层的样子——盒子的 `border` 无法被切开，所以描边独立成形状。
  ③ 文本框按 Flutter 的 `OutlineInputBorder`：缺口起点 = 标签文字左缘 − `gapPadding`，
  宽度 = `(标签宽 + 2 × gapPadding) × 浮动进度`，随浮动动画张开。
- 验证：`ui-paint` 的 `a_notch_cuts_the_top_border_and_leaves_the_fill_under_it`（禁用缺口即红）、
  `ui-paint-gpu` 的实例槽位测试、`ui-widgets-md3` 的
  `an_outlined_label_cuts_a_gap_that_tracks_its_own_text_width`（把标签宽换成常量即红）；
  gallery 截图里 "Email"/"Search" 的顶边确实绕字中断，标签背后不再有不透明补丁。
- 状态：✅ 已修

## 35. `NodeKind::Stack` 静默忽略 `cross_align` / `cross_pos` ⬜ 待修

- 现状：`layout_engine.rs` 里 Stack 的分支把每个子节点放在 `content_origin`、用各自测得的
  尺寸（`~1310`），既不读 `cross_align` 也不读 `cross_pos`；这两个旋钮只对 Row/Column 生效。
  声明了也不报错，于是"居中"看起来写了、实际没生效。
- 影响：想在叠放容器里居中一行内容，只能把 Stack 换成一个单子节点的 Row/Column，或改由外层
  交叉轴居中——`text_field` / `search_bar` 走的是后者（内容盒 hug 行高，字段行 `cross_pos(0.5)`）。
- 建议修法：Stack 里 `cross_align` 按"拉伸到内容盒"解释、`cross_pos` 按"剩余空间比例"解释，
  与 Row/Column 的交叉轴同义；改前先确认没有控件依赖当前"贴 content_origin"的行为。
- 状态：⬜ 待修（当前有可用绕法，未阻塞控件）

## 36. 定位内边距只能是像素，无法表达 CSS 的 `left: 50%` ✅ 已修

- 场景：ProgressBar 的不确定扫动要按父宽比例摆放两条指示条（-40% … 120%），
  条形宽度也是父宽的比例；引擎里 `Placement.inset` 只有 `Insets`（像素）。
- 原状：比例摆放只能靠宿主传宽度再乘百分比，或者干脆写死像素——控件于是知道了不该知道的数。
- 修法：`ui_core::layout::LenInsets { left, top, right, bottom: Len }`，
  `Placement.inset` 换成它，`Decl::inset(impl Into<LenInsets>)` 接受像素或长度；
  `relative_shift` 与流外求解各自按包含块解析（`left/right` 用宽、`top/bottom` 用高，与 CSS 一致）。
  由此"两条百分比边夹出一条宽度"就能表达不等宽的按比例布局，控件不再需要知道像素宽度。
- 验证：`a_percent_inset_resolves_against_the_containing_block`、
  `a_percent_relative_inset_shifts_paint_and_leaves_the_slot_alone`（把解析改成 +1 即红）；
  ProgressBar 的不确定态在 gallery 里可见两条扫动，宽度随列宽变化。
- 状态：✅ 已修

## 37. 目标驱动动画落定即停帧，程序化循环无法自持 ✅ 已修

- 场景：ProgressBar 的不确定态、旋转指示这类"自己按时间算相位"的动效。
- 原状：`FrameOutcome.animating` 只看 `AnimStore::is_animating`（外加墨迹与滚动），
  而动画一次落定就不再要帧；控件即便每帧重算相位，也不会有人再给它一帧。
  MD3 的进度条因此只做了确定态（旧注释写着"自走循环做不到"）。
- 修法：① `AnimStore::age(id, now)`：按 id 记出生时刻，首帧为零，给循环一个从零起的时钟；
  ② `Dep::Age`：读数即记录，重放时年龄必然不同，元素每帧重建；
  ③ `Recorder::time_driven` 标记并入 `FrameOutcome.animating`——读时钟者持续要帧，
  不读（例如确定态的进度条）就自动让循环歇下，不需要任何显式"取消"。
- 验证：`view::tests::a_clock_read_asks_for_every_frame_and_stops_asking_when_it_stops`
  （把 `time_driven` 从 `animating` 里拿掉即红）。
- 状态：✅ 已修

## 38. 绘制端没有描边弧线/路径图元 ✅ 已修

- 场景：ProgressRing。WinUI 3 的环不是 XAML 模板：`ProgressRing.cpp` 用
  `AnimatedVisualPlayer` 播 Lottie，生成代码是 `CompositionEllipseGeometry` 的
  `TrimEnd` 弧（半径 8、描边 1.5、圆头帽，画布 32 单位按 1.77 缩放）。
- 现状：`draw::Shape` 只有矩形/圆角盒/文本/图像等，没有弧、没有描边路径，也没有 trim。
- 修法：`draw::Sector`（圆心、外缘半径、向内生长的厚度、`trim` 起止圈数、圆头帽）+
  `Shape::Sector` + `DrawList::push_sector` + `PrimKind::Sector`。CPU 端 `draw_sector`
  逐像素算「环带 + 角度区间」的覆盖率并叠圆帽；GPU 端 `SECTOR_VERTEX`/`SECTOR_FRAGMENT`
  在片元里做同一件事（环带 SDF、按弧长折算的角度距离、两枚圆帽取最大覆盖率），
  每实例 11 个浮点，不必三角化。`Sector::sweep()` 是两端共用的扫动量：跨度 ≥ 一整圈
  取整环，否则取 `rem_euclid`。
- 验证：`ui-paint` 的 `a_quarter_arc_paints_its_sweep_and_nothing_else`、
  `a_full_turn_paints_the_whole_ring`；`ui-paint-gpu` 的
  `sector_instance_matches_the_shader_stride_and_offsets`、
  `a_full_turn_reaches_the_shader_as_one_whole_sweep`。gallery `progress` 页截图里
  0/25/60/100% 四环与旋转环的像素读数（外径 31、描边 2.6）与 Lottie 换算一致。
- 状态：✅ 已修（ProgressRing 落地，见计划 P1.5）

## 39. 适应内容的项把 hint 当成基准下限，hint 于是永远生效 ✅ 已修

- 场景：TabView 页头 `hug(100, 240)` 想要「宽 100–240、贴合内容」，实测每条 240。
- 原状：`AxisSize::hug(min, hint)` 的 hint 在 `sizes()` 里是**上限**（Fit 时 `max = hint`），
  flex 基准却写成 `content.max(preferred(hint))`，把它当**下限**再被 `[floor, ceiling]` 夹回 240。
  测量趟的 `placed` 与 `contribution` 同样把 hint 当下限。
- 修法：Fit 项的基准 = `content.clamp(floor, ceiling)`（`layout_engine.rs` 的 `place_flow`）；
  测量趟的 `placed` 对 Fit 直接取测得值，贡献下限改走 `hint_floor()`（Fit 返回 0）。
- 验证：`a_fitting_item_hugs_its_content_under_a_comfort_hint`（改回 `content.max(preferred)`
  即红：240 对 150）。gallery `tabs` 页 Fluent 带实测 pitch 100、选中块 100×31。
- 状态：✅ 已修

## 40. 流内 margin 被布局整体丢弃 ✅ 已修

- 场景：页头的图标右间距 10、新建按钮的上边距 13 都写 `Decl::margin`，画面上毫无效果。
- 原状：只有 `place_out_of_flow` 读 `box_model.margin`；流内求解与测量趟从不读，也不报错。
- 修法：新增 `flow_margin()`（只取有限值，`auto` 在流内按 0）。测量趟把主轴 margin 计入内容
  范围与 min/max 贡献、交叉轴计入 `widest`/`tallest`；定位趟从行宽扣除 margin 总和，cursor 在
  子节点前后各让出 margin，交叉轴可用空间扣减并把起点偏移 `margin_start`。
- 验证：`an_in_flow_margin_offsets_the_child_and_consumes_its_share_of_the_line`、
  `a_hugging_container_counts_its_childrens_margins`（`flow_margin` 返回零即红）。
- 状态：✅ 已修

## 41. flex 项从不收缩，与 CSS `flex-shrink: 1` 的默认相反 ✅ 已修

- 场景：TabView 页头溢出时不收缩，把新建按钮顶出卡片；任何 Row 里超宽的子项都直接溢出。
- 原状：`Grow::No => (0.0, 0.0)`——不增长也不收缩，而 CSS 的默认是 `flex: 0 1 auto`。
- 修法：`Grow::No => (0.0, 1.0)`。`fixed` 项由自身下限（`decl_min`）挡住，压缩不到；
  `hug` 项最多压到 min-content，滚动容器压到 0 后由自身滚动。
- 验证：`a_row_squeezes_a_scrollable_child_to_the_line_before_it_overflows`（改回 0 即红：
  150 对 100）；gallery `tabs` 页溢出时列表收缩、`+` 留在带内。
- 状态：✅ 已修

## 42. 控件无法请求「把它所在的滚动容器移动一步」 ✅ 已修

- 场景：TabView 溢出时的左右箭头（WinUI 的 `ScrollDecrease/IncreaseButton` 直接调
  `ScrollViewer.ChangeView`）。
- 原状：**读数本来就有**——`Cx::scroll_of(id)` 返回 `ScrollGeom`（extent / viewport / offset），
  并记 `Dep::Scroll` 参与重放（#42 最初写成「读不到」是错的，见设计错误表）。缺的是**动作**：
  `Cx::scroll_to` 只有 app 拿得到，控件的 `change()` 没有 `cx`，箭头只能把载荷交给应用、
  由应用自己查 id 再写偏移。
- 修法：`Change::ScrollBy { dx, dy }`——控件只说「我所在的滚动容器移动这么多」；
  `AppLoop` 在派发前接住它，沿祖先找最近的 `SCROLL_OWNED` 且确实滚该轴的容器
  （`scroll_owner_by`），按内容范围钳位后走 `start_settle` 缓动，并作废声明缓存
  （箭头自身的可用性要跟着新偏移重建）。
- 验证：`a_scroll_request_steps_the_nearest_engine_scroller`（把派发改成 `moved = false` 即红：
  「the strip moved」）；gallery `tabs` 页溢出时出现 `‹`/`›`，左箭头在偏移 0 处禁用。
- 状态：✅ 已修

## 43. 组不透明度层内的背景模糊被静默跳过 ✅ 已修

- 场景：Fluent ToolTip 的 `OpenStates` 是 167ms 的组不透明度淡入；面板的丙烯酸在淡入期间
  完全不见，淡入结束后也不出现。
- 原状：`paint_node` 的 `layered`（`opacity < 1` 或有效果链）会推一层离屏目标，而渲染器两边
  都只在 `stack.is_empty()` 时执行背景模糊——理由是「层初始为空，里面没有可模糊的东西」。
  于是**任何带组不透明度或效果链的磨砂面都只剩自己的形状**，且没有诊断输出。
  另有一个副作用：层内没有后续图元时，模糊会被推迟到层关闭之后，直接抹在根目标上。
- 修法：层内的磨砂采样**父目标**——层最终正是合成回父目标，那才是这块表面覆盖的东西。
  GPU：`backdrop_pair()` 按层深度选源纹理（`frame_tex` 或 `layer_targets[n-2].tex`）与
  目标 FBO（`layer_targets[n-1].fbo`），`apply_backdrop` 改为收 (src, dst)。
  CPU：`Band::take` 只负责「从源取外延工带并模糊」，`write_into` 负责落点；
  `frost_backdrop()` 在层内把工带写进该层的 scratch。
- 已知边界：层内更早的兄弟节点不属于父目标，因此不会被采样（WinUI 会采到）。本仓库里
  带磨砂的层都是单节点子树，暂不构成问题。
- 验证：`a_backdrop_inside_a_group_layer_frosts_its_parent`（层内直接 `return` 即红：
  `[0, 0, 0, 0]`）；gallery `tooltip` 页淡入期间面板可见、下方文字被模糊。
- 状态：✅ 已修

## 44. 组层的不透明度变化不产生脏区 ✅ 已修

- 场景：动画落定的那一帧恰好是唯一没有别的变化的帧——探针显示该帧只构建、不渲染，
  画面停在上一个 `opacity = 0.961` 的帧。
- 原状：`compute_damage` 只比对**图元指纹**（`prim_fingerprints`），组不透明度与效果链存在
  `DrawList::layers` 里，不属于任何图元，因此单独变化时指纹不变。
- 影响：动画进行中 `animating` 会强制出帧，所以只丢动画的最后一帧；但**非动画**的层不透明度
  变化（状态驱动的淡入淡出）会完全不刷新。
- 修法：`View` 留住上帧的 `layer_prints: Vec<(Rect, Vec<Effect>)>`，`compute_damage` 按
  (bounds, 整条效果链) 逐层比对，对不上就把层的 bounds 标脏；比对发生在磨砂继承脏区之前，
  因此层上方的磨砂也会跟着重绘。
- 验证：`a_group_layers_opacity_damages_its_area_when_it_changes`（把 `same` 写死为 `true`
  即红：`got None`，同帧再跑一次仍无脏区则证明没有过度标脏）。
- 状态：✅ 已修

## 45. 浮层的命中推后、绘制不推后 ✅ 已修

- 场景：Fluent ToolTip 的 `Decl` 声明在页面内容之前（它挂在按钮行上方），面板一出现就被
  后声明的渐变台面整块盖住；同一块面板**点得到**，因为它按 `walk()` 顺序先接住指针。
- 原状：`Tree::walk()` 对 `is_modal || is_overlay` 都推后，`View::paint_tree` 只推后
  `is_modal`。于是浮层的绘制顺序退回声明顺序，而命中顺序仍是推后顺序——两套顺序分叉。
- 修法：`paint_node` 与 `walk()` 用同一判据，浮层子树同样进 `deferred`，绘制与命中重新同序。
- 验证：`an_overlay_paints_over_content_declared_after_it`（去掉 `is_overlay` 即红：
  重叠处取到后声明兄弟的蓝色 `[0,0,255]`）；gallery `tooltip` 页 Save/Delete 的面板完整可见。
- 状态：✅ 已修

## 46. 测量趟不知道子项会被收缩，收缩后的子内容仍按旧宽度排版 ✅ 已修

- 场景：InfoBar 的窄样本（320 槽位）。面板按整条内宽测量，`solve_line` 在 `place_node`
  里才把消息文本压到 210，文本却在 303.8 宽下断行并溢出面板。
- 原状：`Grow::No` 已可收缩（#41），但收缩只发生在**定位**趟；测量趟每个子项都按整条内宽
  `kid_avail` 测量，容器尺寸与 cross 高度都按未收缩的结果定死。定位趟若重测子项，
  子项高度会变化，而父容器尺寸已定——破坏 16 个既有不变量（列拉伸、模态、网格）。
- 修法：把 `flex::Item` 构建与 `solve_line` 抽成 `line_items` / `flow_margins_total`
  （`layout_engine.rs:444`、`:503`），测量阶段在子项循环之后再跑一次：`squeezed_limits`
  （`:518`）给出每个子项的主轴上限，`measure_flow_children`（`:1091`）按上限重测并重算
  row/col 汇总。switchable row 与主轴可滚的容器跳过（那两处的收缩由别的机制负责）。
- 验证：`a_squeezed_child_reflows_at_the_width_the_line_gives_it`（把第二趟换成
  `None::<Vec<f32>>` 即红：wrap_width 100 对 40）；gallery 窄样本消息 303.8 → 191.99，
  条带高度 130.42 跟着长高。
- 状态：✅ 已修

## 47. 子节点复用父节点的 Id 时，两棵树静默合并 ✅ 已修

- 场景：InfoBar 的窄样本把 320 宽槽位与条带声明成同一个 Id，槽位尺寸整个消失。
- 原状：`Tree::apply` 按 Id 增量更新，子节点与父节点同 Id 时，子声明被并进父节点——
  没有报错、没有日志，只有「尺寸没生效」这一种症状。
- 修法：`apply` 的 debug 断言加一条（`tree.rs:1444`）：子节点 Id 等于父节点 Id 即 panic。
- 验证：`a_child_sharing_its_parents_id_is_rejected_in_debug`（`#[should_panic]`）。
- 状态：✅ 已修

## 48. `align_self` / `justify_self` 只对网格子项生效，流式子项上静默无效 ⬜ 待修

- 场景：Fluent 内容对话框的预览样本要在列里以 320 宽贴合，而不是吃满整列。
- 原状：`Node::align_self` / `justify_self` 写的是 `grid.align` / `grid.justify`，只有
  `solve_oof_axis`（网格与外流子项）读它们；流式容器里唯一的对齐旋钮是**父容器**的
  `cross_align`，子项无法单独退出拉伸。写 `justify_self(Start)` 在 Column 里没有任何效果，
  也不报错。
- 影响：任何「列里放一个按内容宽的块」都得套一层 `Container::new(.., NodeKind::Row)
  .cross_align(0.0)`——行的主轴按内容定宽，于是借行的主轴当列的交叉轴用。
- 建议：`place_node` 的流式分支读子项的 `grid.justify` / `grid.align`，缺省沿用父的
  `cross_align`；或者拆出独立的 `align_self` 字段，不与网格单元格对齐混用。
- 状态：⬜ 待修（gallery 的 `dialogs_fluent` 目前用行包装绕开）

## 49. 打开的浮层不接管键盘，外部按下也不通知它 ✅ 已修

- 场景：Fluent MenuFlyout 展开后，方向键仍在走身后的页面；点面板外什么也不会发生。
- 原状：引擎只认模态（`modal_root`）为键盘作用域，浮层只有绘制/命中顺序；`Cx` 也没有
  「这次按下落在了你之外」这一事实，控件只能自己猜。
- 修法：`Tree::popup_root` 取最上层、`Sense::FOCUSABLE` 的浮层（要键盘由浮层自己声明）；
  `focus_scope` = 模态优先、否则浮层；`settle_focus` 在浮层出现时一次性接管焦点并记住原焦点，
  消失时若焦点仍存活就保留、否则交还；方向键找不到目标时在浮层内环绕（`wrap_in_popup`），
  Home/End 跳首尾；按下时把落在 `popup_root` 之外的浮层 id 记进 `Cx::dismissed`，
  控件据此上报 `Change::Dismissed`。
- 补记（接管的那一下取哪个停靠点）：接管走 `focus_stops(false)`，即**不过滤遮罩**的文档序停靠点，
  因为入场动画会把面板最前面的几行裁到窗口之外，按可见性挑就会落在第 3 行；
  `focus_initial` 只在浮层根（`popup_root()`）的 params 上读，写在面板节点上不生效。
- 验证：`popup_scope_tests` 五条（去掉 `dismissed` 赋值、去掉 `focus_scope` 的浮层分支、
  去掉 `popup_owns_keys`、接管改用 `focusables()` 分别变红）；gallery `menu-flyout` 与
  `combo` selftest 走真实点击与按键。
- 状态：✅ 已修

## 50. 静息底色透明的控件行不是 Tab 停靠点 ✅ 已修

- 场景：MenuFlyout 的行只有悬停/按下才上底色，静息填充是全透明。
- 原状：`focusables` 用 `node_paints`（填充 alpha > 0）挡掉纯布局容器，于是整张菜单的条目
  都不在焦点环里：浮层接管了键盘，却没有任何东西可以聚焦，方向键与 Escape 都落空。
- 修法：新增 `focus_stop`——会绘制、是文本、或**声明了焦点环**（`ContainerParams::focus_visual`）
  三者取其一即算停靠点。声明了焦点环本身就是「这里有可聚焦的东西」的凭据。
- 验证：gallery `menu-flyout` selftest 的「arrow walks rows」「Escape closes」两行在改前为 ✗。
- 状态：✅ 已修

## 51. 「拖拽已成立」在按下时就为真，控件分不清点击与拖拽 ✅ 已修

- 场景：滑块、开关与标签栏要把「按一下」与「拖起来」分开——按下不动元素、拖起来才跟手；
  点击仍然直接设值（用户要求保留），只是元素随后**滑**过去而不是瞬移。
- 原状：`WidgetState::dragging` 的语义是「最近的拖拽订阅者拥有这次按下」，**按下即成立**
  （这是为了让控件在整个手势里保持值，`state.rs` 的文档也这么写）。控件拿它当「手指在移动」用，
  于是按下的一瞬间就把拇指/透镜放到光标处：玻璃滑块点一下就跳到点击点，玻璃标签栏点标签的
  右半边，透镜落在两格之间，Fluent 开关的滑块直接瞬移。
- 修法：新增 `Cx::drag_started_within` / `WidgetState::drag_started`——按下越过
  `DRAG_THRESHOLD` 才成立（取自 `DragTracker::is_dragging`，与 `past_threshold` 同一个判据），
  并作为 `Dep::DragStarted` 参与声明缓存校验。位置统一交给 `ui_widgets::build::drag_follow`：
  按下不动、拖拽第一帧按 `glide` 滑向手指、之后精确跟手、点击与外部设值按 `settle` 滑到新值。
- 验证：`a_press_is_not_a_drag_until_the_pointer_travels` 先红（按下帧 `drag_started` 为真）
  后绿；`a_drag_glides_to_the_finger_and_only_then_tracks_it` 先红（第一帧就返回手指位置、
  空闲直接返回值）后绿；gallery 自检 `slider` 页与 `switch` 页各加一条「按下不提交 / 不移动」，
  同样先红后绿。参考侧依据是 `glass_slider.dart` 只在 `onHorizontalDragUpdate` 改值、
  `tab_bar_drag_gesture_mixin.dart` 用 `active: tabIsDragging` 选弹簧。
- 状态：✅ 已修（`ui-core/src/cx.rs`、`ui-platform/src/state.rs`、`ui-widgets/src/build.rs`、
  `ui-widgets-glass/src/{slider,switch,tab_bar}.rs`、`ui-widgets-fluent/src/{slider,toggle}.rs`）
- 续：玻璃开关还多两层——①膨胀锚点也读了 `dragging`，按下即把锚点从前缘切成中心，胶囊朝光标
  一侧鼓出 8dp、松手再切回，读作「先跳到光标再继续走」；锚点改读 `drag_started`，位置交给
  `drag_follow`（原先手写的 `drag-base`/`drag-dx`/`was-dragging` 三段状态一并删掉）。
  ②松手帧曾把位置交还给语义值，而语义值要下一帧才由应用落下：开关按落定位置提交，拖到右端
  也读成旧值、判成没动。交接帧改为返回钉住的手指位置，空闲路径下一帧才滑向语义值。

## 52. 背景捕获只按节点矩形取景，材质无法要求更大的范围 ⬜ 待修

- 场景：LiquidGlassKit 的滑块把背景按形状的 `1 / magnification` 范围捕获再缩回，面下因此是
  0.75 倍的画面；这要求捕获范围**大于**节点自身。
- 原状：`apply_backdrop` 的滤波与写回都以 `entry.rect` 为准，`BackdropEntry` 没有"外扩"这一项。
- 现状绕过：`liquid_glass.frag.glsl` 在采样时按 `1 / m` 缩放 uv（等价于扩大取景），代价是
  面边缘采到的是节点**之外**的像素——本帧已画好，所以结果正确；若将来材质需要采到帧外
  （贴屏幕边），就只剩钳制可用。
- 修法：`BackdropEntry` 增加 `capture: f32`（取景倍率），`apply_backdrop` 按它扩大滤波区域与
  写回范围，材质只读纹理、不再自己缩放 uv。
- 验证：面下画面在形状边缘连续（无重复采样）、贴屏幕边时无拉伸条纹。

## 53. Apple 的 standard 内容材质只有名字，没有渲染路径 ⬜ 待修

- 场景：HIG 把 Liquid Glass 限定在功能层，内容层用 `ultraThinMaterial`…`ultraThickMaterial`
  （`ui-design/data/hig_data.rs` 的 `MATERIALS`，`native_colour` 对它们返回
  `PlatformResolved` 而非值）。这类材质是「模糊 + 振动色（vibrancy）+ 随明暗切换的底色」。
- 现状：`BackdropMaterial` 只有 `Plain`（原样写回模糊）/ `Acrylic`（Fluent 配方）/
  `LiquidGlass`（Apple 功能层玻璃）。内容层因此只能退到**纯色系统填充**——对列表行、字段、
  芯片是对的，但半透明侧栏、内容上方的浮层这类「应当透出内容」的表面没有材质可用。
- 影响：`GlassCard` 的 `Solid` 变体只能给固定色；想给内容层做半透明表面就得另写一套。
- 建议：新增 `BackdropMaterial::Standard { tier: Thin|Regular|Thick, vibrancy }`，
  一个独立片元程序：模糊半径按档取，底色取 `systemBackground` 系并按明暗混合，
  vibrancy 只提亮前景文字（不改变背景色相）。
- 状态：⬜ 待修（2026-11-03 记录；当前内容层用系统填充，观感已与 iOS 一致）

## 54. 文本没有字距（tracking）通道 ⬜ 待修

- 场景：HIG 的字号表按 `size / weight / tracking / leading` 四列给值（17pt 正文 tracking
  常为 −0.43，标题档为正），Cupertino 控件普遍写 `letterSpacing: -0.3`。参考的
  `GlassActionSheet` 行标签就带这一条。
- 现状：`Text` 只有字号、字重、颜色、对齐；排版层（parley）支持字距，但 `Text` 与 `Decl`
  没有把值传下去，控件只能丢掉这一列。
- 影响：玻璃档所有标签都按默认字距排，17pt 以上偏松；与 iOS 截图并排能看出来，但差一档
  不至于认错。中文字距的参考值在 HIG 里没有单列（CJK 不做负 tracking）。
- 建议：`Text::tracking(f32)`（单位 em，与 CSS `letter-spacing` 一致）→ `Decl` 字段 →
  排版样式；HIG 字号表补 `tracking` 列到 `hig_data.rs`。
- 状态：⬜ 待修（2026-11-03 记录；ActionSheet 因此偏离参考的 −0.3）

## 55. 盒子的阴影只能是无色的，也没有 spread ✅ 已修

- 场景：Toast 的辉光是语义色的 15%、20dp 模糊、外扩 2dp（`glass_toast.dart:252-258`）；
  Material 的 tonal elevation、Cupertino 的彩色光晕同理。
- 根因：`Shape::Box.shadow` 用的是 `ShadowSpec { blur, alpha, dy }`——颜色在绘制端写死为黑，
  尺寸也不外扩；彩色阴影只能改用 `Shape::Shadow`，而它要求显式的矩形，尺寸要到布局后才知道
  的盒子（紧贴内容的胶囊、卡片）因此根本画不出来。
- 修法：`ShadowSpec` 改为 `{ colour, blur, spread, dy }`，绘制端用 `colour` 并把矩形按
  `spread` 外扩（脏区同步 `blur + spread`）；新增 `ShadowSpec::black(blur, alpha, dy)` 与
  `ShadowSpec::glow(colour, blur, spread)`。MD3 的 9 处调用改为 `black`，像素不变。
- 验证：Toast 的绿色/红色辉光在浅色页面上可见；`cargo test --workspace` 全绿（MD3 阴影未变）。
- 状态：✅ 已修（2026-11-03）

