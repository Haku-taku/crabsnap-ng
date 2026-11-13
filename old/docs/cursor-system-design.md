# 光标系统设计：逐帧解析、单点交出、可覆盖

> 状态：**运行时缺陷已修（2026-10-29），模型设计待实现**。
> 本文记录两件事：为什么之前「启动后显示上一个应用的光标、且除箭头外从不出现别的形状」，
> 以及修好之后这个子系统应当长什么样。参考实现的行号均已在本机核对。

---

## 1. 症状与根因

用户报告两条：

1. 程序启动后光标是**上一个应用留下的**形状；
2. 用起来**只看得到箭头**——悬停在按钮/复选框上也没有手形。

根因是两个独立的缺陷叠加，任缺其一都不会表现为「永远箭头」：

| # | 缺陷 | 位置 | 后果 |
|---|---|---|---|
| A | `FrameOutcome.cursor` 算出来了，但**只有个别 example 消费它**；`AppLoop::frame` 与画廊都没把它交给窗口 | `crates/ui-platform/src/present.rs`（新增 `PresentTarget::set_cursor`）、`view.rs::frame_with_dt` | 形状永不落到 compositor |
| B | Wayland 窗口的 `cursor_shape` 缓存**从不在 enter/leave 时清空** | `crates/ui-platform-wayland/src/window.rs:491-506` | 即使应用了，重新进入表面后的重申会被缓存吞掉 |

修法（已完成）：

- 光标成为 `PresentTarget` 的契约，`frame_with_dt` 在**任何提前返回之前**应用它
  （静止帧会走独立的早退路径，见 AGENTS.md §1.5「输出变换只需加在正常结束路径」）；
- `wl_pointer::Enter` / `Leave` 把缓存置 `None`，因为 compositor 在指针离开时会丢掉我们设的形状；
- `set_cursor` 立即 `flush`：指针静止时没有后续事件捎带请求。

**已核实的端到端证据**：在嵌套会话里把指针移到按钮上，compositor 依次收到
`PointingHand`（按钮）→ `Arrow`（空白）→ `PointingHand`（再回按钮）。
临时探针已移除（`grep -c CRABSNAP_TRACE_CURSOR` = 0）。

---

## 2. 现状模型

```
控件声明        Decl.cursor: Option<CursorShape>          （ui-platform/src/element.rs:180）
   ↓ 每帧随树重建
引擎解析        resolve_cursor()：沿 pointer_path 逆序取第一个 Some
                                                          （view.rs:854-861）
   ↓
帧输出          FrameOutcome.cursor: CursorShape（永远确定）
                                                          （view.rs:153-155）
   ↓
平台交出        PresentTarget::set_cursor(shape)          （present.rs）
   ↓
Wayland         set_shape(enter_serial, shape) + flush    （window.rs:1117-1132）
```

四条性质：

1. **永远确定**：无人声明就是 `Arrow`，所以「清除」不会卡住旧形状；
2. **最近者胜**：沿祖先路径逆序，子节点的 `Arrow` 能阻断父节点的 `Grab`（`cursor.rs` 的
   `Option` 而非默认枚举值，正是为此——见 AGENTS.md §1.5「形状用一个默认枚举值表示未设置」）；
3. **每帧重算**：不缓存到控件，控件无状态；
4. **协议陷阱**：`set_shape` 的 `serial` 必须是最近一次 `wl_pointer.enter` 的 serial，
   否则被 compositor **静默丢弃**且不报错。

---

## 3. 工业界怎么做（本机核对）

| 框架 | 声明方式 | 解析 | 交出 | 覆盖机制 |
|---|---|---|---|---|
| **egui 0.36.2** | `ui.ctx().set_cursor_icon(..)`，写进 `PlatformOutput.cursor_icon` | 后写的覆盖先写的（UI 处理顺序） | integration 每帧读 `PlatformOutput`，注释写明「The backend should use this」 | `cursor_image` 覆盖图标；无「捕获」概念 |
| **GPUI 0.2.2** | `window.set_cursor_style(style, &hitbox)`，压入 `next_frame.cursor_styles` | 每帧清空重建；`cursor_style()` **逆序**遍历，取第一个「其 hitbox 正被悬停」的请求（`window.rs:1158-1171`） | `PlatformWindow::set_cursor_style`（`platform.rs:568`） | `set_window_cursor_style`：`hitbox_id: None` 的请求**立即命中并终止遍历**（`window.rs:4157-4163`），注释说明「优先于任何按 hitbox 设置的样式」 |
| **Qt 6** | `QWidget::setCursor`（`qwidget.h:332`），沿父链继承 | 窗口系统按 widget 层级解析 | 平台 | `QGuiApplication::setOverrideCursor`（`qguiapplication.h:89`），全局覆盖栈 |
| **GTK 4** | `gtk_widget_set_cursor` / `_from_name`（`gtkwidget.h:665-669`） | 属性沿树继承，子控件设置即覆盖 | 平台 | 无单独覆盖 API；拖拽期间由 DnD 侧自己设 |
| **Wayland** | `wp_cursor_shape_device_v1.set_shape(serial, shape)` | 协议不做解析 | 客户端负责 | 协议提供 36 个形状 |

**三条可迁移的结论**：

1. **光标是每帧输出，不是状态**。egui 与 GPUI 都是每帧重建一张表，再交给平台——与我们一致。
2. **「最近者胜」之外还需要一个「覆盖层」**。GPUI 用 `hitbox_id: None` 的请求、
   Qt 用 `setOverrideCursor` 栈。缺了它，拖拽中把指针移出控件就会丢掉 `Grabbing`。
3. **平台侧必须允许重复声明**。GPUI 的 `set_cursor_style` 每次 paint 都调用；
   egui 的 integration 每帧读。Wayland 的 enter-serial 规则强迫我们**缓存并在必要时重申**，
   这是协议特有的一层，不能省。

---

## 4. 差距清单

| # | 差距 | 影响 | 优先级 |
|---|---|---|---|
| G1 | 无覆盖/捕获：解析只看 `pointer_path`（悬停路径）。拖拽捕获期间指针移出控件，光标回落到箭头 | 滑块拖到控件外、拖拽排序时手感错 | **高** |
| G2 | `CursorShape` 只有 20 个，协议有 36 个。缺 `Help`、`Progress`、`Wait`、`Cell`、`VerticalText`、单向 `ResizeN/S/E/W`、`ContextMenu`、`DndAsk` 等 | 未来做表格/文本/分隔条时无处可声明 | 中 |
| G3 | ~~没有任何控件声明 `Text`~~ **已修**：文本框（text field / search bar / shortcut field / time picker）自己声明 `Text`；引擎对 `Sense::SELECT` 的文本节点自动给 `Text`，应用不必逐个声明 | — | 已修 |
| G4 | GPU 呈现路径必须由 app 自己调 `set_cursor`（`gallery.rs` 的 `Presenter::Gpu` 分支），CPU 路径在 `frame_with_dt` 里自动做 | 新写一个 GPU 样例时**很容易漏**，且漏了没有任何征兆 | **高** |
| G5 | 首次 `wl_pointer.enter` 之前无法设置任何形状（协议要求 serial） | 启动瞬间仍显示上一个应用的光标；这是协议行为，不可修，只能记录 | 记录 |

---

## 5. 目标模型

### 5.1 声明：`Decl.cursor` 不变

一个节点声明它被悬停时的形状。`None` 表示「不表态」，让祖先决定；显式 `Arrow` 表示
「到此为止」。这条语义已经正确，不改。

### 5.2 解析：三层，从内到外

```rust
fn resolve_cursor(&self) -> CursorShape {
    // 1. Window-wide override: a live drag or an explicit grab wins over any hover.
    //    窗口级覆盖：进行中的拖拽或显式抓取压过一切悬停。
    if let Some(shape) = self.cursor_override {
        return shape;
    }
    // 2. Captured node: the drag owner keeps its shape even when the pointer leaves it.
    //    捕获节点：拖拽所有者即使指针离开也保留自己的形状。
    if let Some(id) = self.drag_owner() {
        if let Some(shape) = self.declared_cursor(id) {
            return shape;
        }
    }
    // 3. Nearest declaration along the hover path, innermost first.
    //    悬停路径上最近的声明，由内向外。
    for id in self.pointer_path.iter().rev() {
        if let Some(shape) = self.declared_cursor(*id) {
            return shape;
        }
    }
    CursorShape::Arrow
}
```

`cursor_override` 的写入者只有两个：引擎（拖拽开始/结束，`Grabbing`/`Grab`）与
`AppLoop::set_cursor_override(Option<CursorShape>)`（app 自己的模态拖拽，对应 Qt 的
`setOverrideCursor`）。**它是一个显式的 `Option`，不是一个有默认值的枚举**——理由与
`Decl.cursor` 相同。

### 5.3 交出：一条路，且在早退之前

现状是两条路（CPU 在 `frame_with_dt`，GPU 由 app 自己调）。改成：

```rust
/// Apply the non-pixel part of a frame. Every presenter path calls this exactly once.
/// 应用一帧中与像素无关的部分。每条呈现路径都必须且只能调用它一次。
pub fn apply_outcome(win: &mut impl PresentTarget, outcome: &FrameOutcome) {
    win.set_cursor(outcome.cursor);
    win.set_damage(outcome.damage.clone());   // 目前散在两条路径里
}
```

CPU 路径内部调用它；GPU 路径的样例改为调用它。这样「漏了会怎样」从「画面没反应」
变成「少了一行显眼的调用」，而且两条路径共用同一份代码。

### 5.4 平台：缓存 + 重申 + 立即 flush

保持现状，但把三条不变量写进类型：

| 不变量 | 现在的保证 |
|---|---|
| 形状缓存只在**确实发出请求**后更新 | `set_cursor` 在 `device`/`serial` 缺失时提前返回且不改缓存 |
| `Enter`/`Leave` 清空缓存 | `window.rs:495, 506` |
| 发出后立即 `flush` | `window.rs:1130` |

### 5.5 形状表补全

`CursorShape` 扩到协议的 36 个，`protocol_shape` 一一对应。
`cursor_shape.rs` 已有 `every_shape()` 测试保证每个变体都有映射；扩表时它会强制同步。

---

## 6. 验证

| 断言 | 层次 | 说明 |
|---|---|---|
| `every_shape()` 覆盖全部变体且互不相同 | 单元（已有） | 扩表时自动生效 |
| `a_node_off_the_hover_path_does_not_affect_the_cursor` | 单元 | 悬停在 A 上、B 声明了手形，结果必须是 A 的 |
| `a_captured_drag_keeps_its_shape_when_the_pointer_leaves` | 单元 | G1 的回归：拖拽中把指针移出，仍是 `Grabbing` |
| `the_window_override_beats_the_hovered_node` | 单元 | 5.2 第 1 层 |
| 画廊 selftest：按钮上 `PointingHand`、空白处 `Arrow` | 端到端（已有） | `selftest: cursor over a button …` |
| compositor 真的收到 `PointingHand` | 手工（已做一次） | 探针 `eprintln!` + 嵌套会话移动指针；**不常驻**，因为需要真 compositor |

---

## 7. 不做的事

- **不做自绘光标**（`cursor_image`）。egui 支持，我们没有需求；协议路径也已经能覆盖全部用例。
- **不把光标放进 `PointerGesture`**。它只依赖悬停路径与捕获状态，与按下原点无关；
  放进手势结构会让「没按下时没有光标」这种错误变得可能。
- **不为 G5 做补救**。首次 enter 之前无法设置形状是协议决定的，任何「补救」都只是
  在下一个 enter 时重申——那已经是我们现在做的。
