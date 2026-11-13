# Popover 与 Tooltip：引擎通用能力设计

> 状态：**O1–O4 已实现**（2026-11-02）。回答用户问题：tooltip 和 popover 是否该做成
> 引擎通用能力、实现方式是否参照 caret+选区与右键菜单、以及日期/时间/菜单是否本就该是 popover。
>
> 本文依赖 P5 的定位能力（`docs/layout-engine-design.md` §4.6：`absolute` + 包含块 + `z`），
> 那些已合入。旧项目未参与结论。

## 0. 结论

1. **Popover 与 tooltip 应当是引擎通用能力**，理由与 caret+选区、右键菜单同构：它们都是
   「一块浮在内容之上、需要越过祖先裁剪与窗口边缘摆放、有独立命中与作用域的区域」。
   每个控件各自用 `absolute` 手搓会重复且不一致（溢出、堆叠、失焦关闭都得各写一遍）。
2. **实现方式：走「进树」的浮层，不是菜单的「引擎自绘」。** 现有右键菜单是
   `AppLoop.menu: Option<OpenMenu>`（`ui-platform/src/menu.rs`），**不在树里**、由 `paint_menu`
   自绘、自算行矩形、自命中——那是**引擎兜底**样式（设计体系只换 `MenuStyle` 外观）的权宜。
   Popover/tooltip 有内容控件（按钮、输入格、日期网格），必须是真 `Decl` 子树才拿得到布局、
   命中、焦点、动画。P5 之后树能表达锚定浮层，菜单那种「游离于树外」的旧路不必复制。
3. **引擎负责「浮层的生命周期与摆放」，控件负责「内容」**：切分线在
   `overlay: Option<Overlay>`（声明「我要浮、锚在哪、怎么对齐」）+ 引擎的 `Routing`
   暴露浮层的开关/命中，类比 `Sense::SELECT` 与 `caret_for`。
4. **日期选择器、时间选择器、下拉菜单在真实产品里应当是 popover**；但当前画廊把它们画成
   **docked 内联**是对的（画廊要展示控件本体，浮层会挡住别的样本）。**控件本体与浮层容器
   分离**：`Md3DatePicker` 是内容，`Popover::anchor(field)` 是宿主——两者都能独立存在。

## 1. 为什么 caret+选区、右键菜单是正确类比

三者共享同一组需求，且都已验证「引擎持有、控件消费」这套分工成立：

| | caret/选区 | 右键菜单 | popover/tooltip（本设计） |
|---|---|---|---|
| 越过静态布局 | `Routing.caret_in_text` 事实 | 引擎摆放矩形 | `absolute` 相对锚点祖先 |
| 独立命中 | `Sense::SELECT` | `OpenMenu::row_at` | 浮层子树 + `z` |
| 全局唯一 | `AppLoop.selection` 一份 | `AppLoop.menu` 一个 | 同时只一个顶层浮层（栈式） |
| 外部关闭 | 按下别处清选区 | 按下别处 `menu.take()` | 按下锚点外 / Esc 关闭 |

右键菜单已经把「一个、外部关闭、命中优先」这套跑通，popover 复用它的是**语义**，
复用它**不是**的是「树外自绘」——因为 popover 里有控件。

## 2. 数据模型

```rust
/// 浮层如何锚定。挂在浮层根 `Decl` 上，引擎据此覆盖其摆放。
pub enum Anchor {
    /// 锚在某个节点矩形上（锚点用 `Id`，与本浮层内容同树）。
    Node { id: Id, side: Side, align: Align },
    /// 跟随指针（tooltip、右键菜单）。
    Pointer,
}

/// 锚点矩形的哪一侧展开。
pub enum Side { Below, Above, Start, End, Right, Left }

/// 越界处理：先往对侧翻，翻不动再夹进视口。
pub struct OverlayPlacement {
    pub anchor: Anchor,
    pub offset: f32,          // 与锚点的间距（gap）
    pub flip: bool,           // 默认 true
    pub clamp_to: ClampTo,    // Viewport（默认）
    pub match_anchor_width: bool, // ComboBox/菜单：面板宽 = 锚点宽
}
```

- 挂在**浮层自己的根节点**上，不挂在锚点上。锚点只提供一个 `Id`。这样浮层内容可以是任意
  `Decl` 子树（含控件），且能独立于锚点的兄弟存在。
- 引擎在 **P5 的两阶段之后**再跑一遍「浮层摆放」：锚点已在 in-flow 里定了矩形，
  浮层据此解 `absolute`（`anchor` = 相对锚点，而非相对最近的 positioned 祖先）。
  这需要一个「锚点矩形」的解析入口，复用 `containing_block` 的机制但取 `anchor.id` 的 rect。

## 3. 越过祖先裁剪与窗口

这是浮层区别于普通 `absolute` 的核心，也是当前**做不到**的地方：

- 普通 `absolute` 仍受祖先 `Clip::Hidden` 裁（P5 遗留：`fixed` 也仍受祖先裁）。
  浮层**必须**画到锚点祖先的框之外。
- 方案：浮层在 `place` 阶段被**重新挂到根**（脱离其声明位置的裁剪链），命中 `z` 最高。
  `Node` 记一个 `overlay: Option<OverlayPlacement>`；`place_node` 遇到它时把该子树的
  mask 起点重置为根（只受根与视口裁），矩形由 `Anchor` 解出。这与 P5「fixed 相对根」是
  同一套机制的延伸——差别只在 fixed 锚视口、overlay 锚某节点。
- **不需要**独立 wayland 表面：全仓库没有 `xdg_popup`（`grep get_popup` 空），菜单也是画进
  同一表面的。浮层同理，留在主表面内绘制即可，省掉一整套 popup 生命周期。

## 4. 生命周期：状态机放引擎还是应用

沿用「应用持状态、控件纯函数、引擎报 `Change`」的既有契约（`AGENTS.md` 未改，
`docs/control-completion-progress.md` 通篇如此）。所以：

- **开/关是应用的状态**：`app.open_popover: Option<Id>`，点触发器时 `Change::Activated` →
  应用翻转它 → 下一帧应用声明浮层 `Decl`（带 `overlay`）。**引擎不持「哪些浮层开着」的清单**，
  只持「本帧声明的浮层如何摆放/命中/越界」的执行。
- **失焦关闭是引擎的主动作**：与 `AppLoop.menu.take()` 一样，按下浮层之外时引擎需要**通知**应用
  关掉它——发一个 `Change::Dismissed(Id)`（新增变体）给浮层的 owner 或应用。理由：
  外部按下时焦点链可能整个变，让应用自己从 `Routing` 推断「点在外面」不如引擎直接判定
  （引擎已有「点在哪、浮层矩形」两个事实）。这是唯一进引擎的浮层状态：**当前顶层浮层矩形**
  （供命中，与 `OpenMenu.rect` 同性质，非持久状态）。
- **hover 延时（tooltip）**：延时用 `cx.animate` + `AnimSpec.delay` 做不到（那是插值不是触发）。
  两种落法：① 应用侧用一个 `f64` 时间戳自己攒延时（tooltip 触发器上报 `Change::HoverChanged`）；
  ② 引擎记「悬停起点时刻」（`Routing` 加 `hover_since: Option<f64>`）。**建议 ①**：延时长度是
  产品决策（500ms/1s），不该进引擎；引擎只要在 `Routing.now()` 给一个每帧时钟就够（已有）。

## 5. tooltip 特例

Tooltip 内容是无交互的一小段文本/富文本，可**复用同一 `OverlayPlacement`**，但：

- `Pointer` 锚定时贴指针（+一个小偏移），不进命中（`Sense::NONE`），不抢焦点；
- 它比 popover 轻，可以允许**引擎自绘**（像菜单）以省一整套子树——但既然 popover 已进树，
  tooltip 用同一棵树机制更一致，且 MD3/Fluent/Glass 三包的 tooltip 外观能走 `StyleKit`。
- 建议：tooltip 也进树，走 `overlay` + `Pointer` 锚 + `Sense::NONE`。菜单是唯一保持树外的
  （它是引擎兜底，历史原因），见 §7。

## 6. 现有控件怎么迁移到 popover 宿主

| 控件 | 现状 | 目标 | 备注 |
|---|---|---|---|
| `Md3DatePicker` | docked 内联（画廊要展示本体） | 内容不变；新增 `Popover` 宿主示例页 | 产品里字段旁点开日历 |
| `Md3TimePicker` | docked 内联 | 同上 | |
| 下拉/ComboBox | 无（engine-gaps #11/#13 相关） | 面板 = popover，`match_anchor_width: true` | ComboBox 是新控件，直接按 popover 建 |
| 右键菜单 | 引擎树外自绘兜底 | 保持（`MenuStyle` 换皮够用）；控件级菜单可另走 popover | 见 §7 |
| tooltip | 不存在 | 新增，走 `overlay` + `Pointer` | |

## 7. 明确不做

- **不做独立 popup 表面 / `xdg_popup`**：浮层留主表面，理由同 §3。
- **不把浮层清单放进引擎**：开/关是应用状态（§4），引擎只摆本帧声明的。
- **不迁移右键菜单进树**：它是兜底样式，`MenuStyle` 已让设计体系换皮；进树只为服务有交互内容的
  popover。控件自带菜单（如 app-bar 溢出）若需要富内容，用 popover 宿主而非扩展兜底菜单。
- **不做层叠多浮层**：与菜单一致，同时只一个顶层浮层（栈式，Esc/外部按下关最上层）。
  `ShortcutsChanged`/嵌套 popover 等需求出现时再扩。
- **不做 tooltip 自动避让屏幕边缘 beyond flip**：flip + clamp 覆盖绝大多数；精细箭头指向
  （caret/bubble tail）属外观，留给 `StyleKit`。

## 8. 实现阶段（接 P5 之后）

- [x] **O1 摆放**：`OverlayPlacement { anchor, side, align, offset, flip, match_anchor_width }`
  + `Anchor::{Node, Point}`；`Decl::overlay(..)` 同时置 `Position::Absolute`，引擎在流外那一趟
  按锚点矩形解位置（不读 inset），越界先翻到对侧、再钳进视口。`Anchor::Point` 取代设计里的
  `Pointer`：指针位置由应用在声明时给出，布局因此仍不依赖输入。
  测试：`an_overlay_escapes_the_clip_of_its_declaring_ancestor`、
  `an_overlay_flips_above_when_it_would_overflow_the_viewport`。
- [x] **O2 命中与 z**：`Tree::is_overlay` 让浮层子树在 `walk()` 里与模态一起推后——绘制在上，
  命中索引按 walk 顺序给 `z`，于是浮层先接住指针，与声明顺序无关。遮罩在 `compute_masks`
  里同样从浮层自身重置，祖先的 `Clip::Hidden` 切不到它。
  测试：`an_overlay_takes_the_pointer_over_content_declared_after_it`。
  **修正（2026-11-02）**：此前只有 `walk()` 推后浮层，`paint_tree` 只推后模态，绘制退回声明
  顺序，面板被后声明的兄弟盖住（engine-gaps #45）。现在两处同判据。
  测试：`an_overlay_paints_over_content_declared_after_it`。
- [x] **O3 生命周期**：落地为无载荷的 `Change::Dismissed`——上报节点就是被关闭的浮层，
  不必再带 Id。引擎在按下时把落在 `popup_root` 之外的浮层记进 `Cx::dismissed`（`Routing` /
  `Dep` 同路），控件在 `change()` 里据此上报。同一批还补了浮层键盘作用域：`popup_root`、
  `settle_focus` 的接管/交还、方向键环绕、Home/End。
  测试：`a_press_outside_the_open_panel_asks_that_panel_to_close`、
  `an_open_panel_takes_the_keyboard_and_hands_it_back_when_it_closes`、
  `the_navigation_keys_walk_the_panel_as_a_closed_loop`。
- [x] **O4 tooltip**：应用侧 hover 延时（`SPI_GETMOUSEHOVERTIME` 400ms，用 `cx.age()` 的逐帧
  读取撑住这段延时）+ `FluentToolTip`；画廊加 `Controls · Tooltips` 页（三个按钮 + 渐变台面，
  面板与它模糊掉的文字同屏可见）。令牌全核：font 12、padding `9,6,9,8`、maxWidth 320、
  corner 4、1px `SurfaceStrokeColorFlyoutBrush`、`OpenStates` 167ms 淡入淡出、
  背景 `AcrylicInAppFillColorDefaultBrush`（`material.rs` 的 `AcrylicRecipe` 由此首次有使用者）。
  [!] 偏离：锚点用的是 `Anchor::Node`（按钮自身），`Anchor::Point` 仍无人使用。

依赖：P5（已合入）。**不依赖**宽匹配/Change 改名，可与其并行，但 `Change::Dismissed`
若赶上改名那批，一起改。O1–O3 在 `ui-core`/`ui-platform`，O4 触画廊。
