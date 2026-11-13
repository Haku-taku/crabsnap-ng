# IME 与文本编辑设计

> 状态：方案已定（2026-10-30），待实现。
> 本文只写接缝、取舍和会做错的地方；文本管线现状见 `docs/ui-framework.md`，
> 缺口台账见 `docs/engine-gaps.md §19`。
> 调研依据：GPUI `79fe6a6`、Flutter `950bf4e8`、egui 0.36.2、`text-input-unstable-v3.xml`，
> 结论与行号见本文 §3；四家都不照抄，只借它们解决的问题与不变量（`AGENTS.md §2`）。

## 1. 范围

**本批做**

- 普通文本可选中（只读），选区绘制**由风格体系在运行时提供**（见 §4 D7）。
- 可编辑单行文本：锚点/插入符双端、Shift+方向键、Home/End/Delete、Ctrl+A、点击定位、拖选、双击选词。
- IME：`zwp_text_input_v3` 接线、预编辑串显示（下划线）、提交合流、候选窗定位。
- 三套体系各自的选区样式（MD3 / Fluent / Glass），在 gallery 里并排可验证。

**本批不做**（已与用户确认）

- 剪贴板（Ctrl+C/V/X 走 `wl_data_device`）；本轮只保证不产生错误行为。
- 多行编辑、RTL/双向文本的精确命中、跨节点（多段）选择、密码掩码。

## 2. 现状与缺口

| 事实 | 位置 |
|---|---|
| 文本几何的唯一输入是 `TextPrim { block, origin, max_width, ink, caret }` | `ui-core/src/draw.rs:311-319` |
| 插入符靠"每帧整形一次前缀"定位，没有反向查询 | `ui-platform/src/text_pass.rs:339-366` |
| 测量会整形但丢掉 shaped run，绘制再整形一遍 | `text_pass.rs:404-437`、`516-594` |
| 缓冲归 app：`ContainerParams.text_input` + `Change::Text(整段)` | `ui-core/src/tree.rs:213-216`、`ui-platform/src/view.rs:1272-1317` |
| 插入符归引擎：`Routing.caret` + `Cx::caret_for` | `ui-core/src/cx.rs:39-41`、`384-388` |
| 键盘文本来自 `wl_keyboard` → xkb，没有 IME 对象 | `ui-platform-wayland/src/input.rs:122-137` |
| 只有声明 `Sense::SELECT` 的文本进命中；其余标签照样惰性 | `ui-platform/src/view.rs` `hit_behavior_of` |
| 插入符闪烁归引擎：编辑中每 500 ms 明灭，动一下就重开周期，靠 `next_wake` 定时唤醒驱动循环 | `ui-platform/src/view.rs` `caret_blink_lit` / `next_wake`、`driver.rs` `step` |

## 3. 参考结论（只列影响设计的）

- **几何必须由布局回答**：GPUI `TextLayout::index_for_position`（`elements/text.rs:863`）、
  Flutter `getBoxesForSelection`（`painting/text_painter.dart:1692-1731`）、
  egui `Galley::cursor_from_pos`（`epaint/src/text/text_layout_types.rs:1181-1225`）。
- **选区画在文字之下、插入符在文字之上**；三家都如此（`editable.dart:504-515`、
  `text_selection/visuals.rs:35-66`、`editor/element.rs:5980`）。
- **预编辑串在主流框架里都进 buffer**，因为它们拥有 buffer：Flutter `composing`
  （`services/text_input.dart:1047-1072`）、egui `ImeComposition`（`text_edit/builder.rs:1306-1321`）、
  GPUI `InputComposition` highlight（`editor/input.rs:3023-3034`）。我们的契约相反，见 D1。
- **RTL 命中连 GPUI 都没做**（`text_system/line_layout.rs:61-76` 假设 glyph 索引单调）；
  我们按"纯 RTL 正确、混合方向近似"记录限制。
- **Wayland**：被 IME 消费的键不会以 `wl_keyboard` 到达客户端（input-method-v2 的 grab 语义），
  所以键盘文本路径必须一直保留；`enable` 重置全部状态；事件在 `done` 时生效；
  `cursor_rectangle` v1 在 text-input commit 生效、v2 在下一个 `wl_surface.commit` 生效。

## 4. 决策

| # | 决策 | 理由 |
|---|---|---|
| D1 | **预编辑串归引擎，不进 app 缓冲** | app 的 `text_input` 永远是已提交值；组合期间不发 `Change::Text`。协议本身也要求 `set_surrounding_text` 排除 preedit |
| D2 | 几何**按需整形**，接口按"持久布局对象"设计 | 先最小改动拿到正确性；以后换缓存布局对象时调用方不变 |
| D3 | 选区归**引擎**（单一选区），app 不感知 | 与"控件不藏状态"一致；只读选择不需要 app 参与 |
| D4 | 普通文本**显式 opt-in**（`Sense::SELECT`） | 默认可选会抢按钮点击（`design-errors.md` 第 195 条同类） |
| D5 | 剪贴板、多行、RTL 命中**本批不做** | 已确认，避免一个批次里混两类风险 |
| D6 | 插入符与选区都记进 `Dep` | 不记就命中声明缓存，控件在选区变化时纹丝不动（`Dep::Caret` 的同类陷阱） |
| D7 | 选区**绘制程序可插拔**，随声明走 | 三家的选区差异不止颜色（Fluent 反白、MD3 纯色块、Glass 圆角/描边），引擎只给几何 |

### D7 的形状

引擎负责几何与绘制顺序，风格负责"怎么画"：

```rust
// ui-core/src/selection.rs
pub struct SelectionFragment {
    pub rect: Rect,      // 表面坐标，一行高（引擎不做二次平移）
    pub line: usize,
    pub first: bool,     // 选区的第一段
    pub last: bool,
    pub rtl: bool,
}

pub struct SelectionGeometry<'a> {
    pub fragments: &'a [SelectionFragment],
    pub fill: Color,     // 体系解析出的选区色，绘制程序可用可不用
    pub ink: Color,      // 文字自身墨色
}

pub struct SelectionPaint {
    pub rects: Vec<RectPrim>,    // 表面坐标，画在字形之下；引擎不做二次平移
    pub ink: Option<Color>,      // 选中字形的墨色覆盖（Fluent 反白用）
}

pub trait SelectionPainter: std::fmt::Debug + Send + Sync {
    fn paint(&self, geometry: &SelectionGeometry<'_>) -> SelectionPaint;
}
```

- **随声明走，不搞全局注册表**：`TextPrim.selection.painter: Option<&'static dyn SelectionPainter>`。
  注册表要"包暴露 id、app 注册、对不上就静默降级"，正是 `AGENTS.md §5` 说的接线缺口。
  `&'static` 让声明可比较（`std::ptr::eq` 连 vtable 一起比，零尺寸类型也不会混）、可 Copy、
  每帧重建风格包也不产生新指针。
- **运行时设置的门在 `Style`**：`Style.selection: Option<SelectionPainterRef>`，三包各自给默认，
  app 可按作用域覆盖（gallery 三列因此能并排显示三种选区）。
- 没有绘制程序时引擎画一个平直矩形（`fill`），纯颜色体系不需要写代码。
- 绘制程序返回**图元**（`RectPrim`）而不是 `Shape`：调用点在 `TextPass::resolve`，已经在
  decl→图元的另一侧，字形与插入符都在这一层直接产 `GlyphPrim`/`RectPrim`；返回 `Shape` 要在
  这里重做一次转换，且 `Shape::Box` 的阴影/图片字段对选区无意义。owner、裁剪与脏区指纹照旧
  （图元随节点一起登记）。见 `docs/lessons/design-errors.md`「绘制程序返回 `Shape` 即可沿用既有路径」。
- trait 带 `Send + Sync`：`SelectionPainterRef` 住在 `Style` 里，而 `Style` 必须能装进
  `StyleScope`（`ui-style` 有 `assert_send_sync::<Style>()`）。

## 5. 分层改动

### 5.1 `ui-core`

- `selection.rs`：上面的类型 + `Sense::SELECT`。
- `text_input.rs`（**未另起 `text_edit.rs`**）：`TextSelection` 在 `selection.rs`；本文件已加
  `delete_range`（按选区整体删除）与 `word_at`，其余是既有的 `clamp`/`moved`/`insert`/`backspace`/`delete`。
  全部运算以**字簇**为单位（`ui_core::grapheme`，UAX #29），见 §9。
- `Event`：`Preedit(Option<Preedit>)` 与 `DeleteSurrounding { before, after }` **已落地**；
  `Key` **已加** `Home`/`End`/`Delete` 与 `Char(char)`——后者是带命令修饰键的字符，
  平台用它替代 `Text`，因此 Ctrl+A 不会插入 `a`。
- `Routing`：`selection: Option<(Id, TextSelection)>`（**已落地**，输入与只读文本共用一个字段）、
  `preedit: Option<(Id, Preedit)>`（**已落地**）；`Cx::selection_for` / `preedit_for` 已落地。
- `NodeData::Text` / `TextPrim`：`selection: Option<SelectionPrim>`（**已落地**）、
  `composing: Option<Range<usize>>`（**已落地**，绘制端按它加下划线）。
- `Dep`：`Selection(...)`（已落地）、`Preedit(...)`（已落地）。

### 5.2 `ui-platform`

- `TextPass` 三个查询，输入与绘制同一个 `TextPrim`：
  ```rust
  fn index_at(&mut self, prim: &TextPrim, p: Point) -> usize;
  fn caret_rect(&mut self, prim: &TextPrim, index: usize) -> Option<Rect>;
  fn selection_fragments(&mut self, prim: &TextPrim, range: Range<usize>) -> Vec<SelectionFragment>;
  ```
- `TextPass::resolve`：先算片段 → 调绘制程序 → 推入形状 → 画字形（按选区切 ink）→ 画下划线 → 画插入符。
- `AppLoop`：**已落地** `editing: Option<(Id, TextSelection)>` + `editing_drag: Option<Id>`
  （拖拽选字）、`preedit: Option<(Id, Preedit)>`（**已落地**，步骤 4）。读口：`caret()` / `selection()` / `ime_state()`。
- `PresentTarget::set_ime(ImeState)`（默认空实现，沿用 `set_cursor` 的先例）——**已落地**；
  `AppLoop` 每帧在 present 之前算一次，状态变化时即使无脏区也 present（§6 规则 4）：
  ```rust
  pub struct ImeState {
      pub enabled: bool,
      pub cursor: Option<Rect>,        // 本帧布局算出的插入符矩形，逻辑坐标
      pub surrounding: Option<(String, usize, usize)>,  // 文本、caret、anchor（字节）
  }
  ```
  `enabled` 只在 `cursor` 存在时为真（§6 规则 1），`surrounding` 不含预编辑串。

### 5.3 `ui-platform-wayland`

- 绑 `zwp_text_input_manager_v3`（`1..=2`），每 seat 建一个 `zwp_text_input_v3`。
- 收到 `preedit_string`/`commit_string`/`delete_surrounding_text`/`done` 先缓存，
  **在 `done` 时合成一组事件**交给 `dispatch`；`text == null` 表示清空预编辑。
  **每个 `done` 都发一次 `Preedit`**：规范说待发值在 `done` 时应用并复位，本批没有
  `preedit_string` 就表示输入法没有预编辑（fcitx5 清空组合时不发空串，只发 `commit`）。
- 插入符取 `cursor_end`：fcitx5 对每个拼音预编辑都发 `begin=0, end=len`，客户端把插入符
  画在组合串之后；只发增量会让它停在组合串之前（「插入符不跟预编辑走」）。
- `set_ime` 做状态 diff；`enable` **只在关闭→打开时发，并单独提交一次**，surrounding /
  content_type / cursor_rect 随后在第二次提交里补齐（`enable` 会重置 compositor 的副本，
  同批发送会被吞掉，见 §6）；`disable` 也要 `commit`。启用之后 surrounding / content_type
  只在值变化时发——每个预编辑字符都重发 surrounding 会让输入法每敲一下重新同步组合。
- 键盘焦点 `enter` 之后**强制重发** `enable` 与整套状态：compositor 在离开时已丢弃会话，
  只做 diff 会认为「没变」，输入法从此不工作，直到用户手动切一次焦点。
- 键盘路径**不动**：`wl_keyboard` 继续产 `Event::Text`。

### 5.4 样式与控件

- `ui-design`：`CommonColor::TextSelection` / `TextSelectionInk`，三套体系各自给派生值：
  MD3 = `primary@40%`（Flutter `material/app.dart:1021-1052`）；
  Fluent = accent + 白色文字（WinUI `TextSelectionHighlightColorThemeBrush`）；
  HIG = tintColor 低透明度（具体 alpha 标 `[未证实]`，按观感定 0.25–0.3）。
- 三包：`role_common("textSelection", …)` + 各自的 `SelectionPainter` 实现。
  **已落地**：MD3 不注册绘制程序（引擎的平直矩形就是它的样子）；Fluent `FLUENT_SELECTION`
  平铺 accent + 字形反白；Glass `GLASS_SELECTION` 圆角收边（半径 3）。
- `ui_widgets::Text`：**已落地** `.selectable(fill, painter)` 与 `.selectable_in(pack)`
  （后者从包读 `textSelection` 角色与 `Style.selection`）；`.composing(range)` **已落地**。
- 选区状态归引擎（`Routing.selection`，**全局只有一个**）：在另一段文本上按下会替换它，
  按在其它任何地方会清掉它。gallery 的 `Controls · Text selection` 页因此一次只有一列有选区。
- MD3 字段：从 `Cx` 读选区/预编辑，预编辑单独一段 + 下划线，插入符落在预编辑内部。

## 6. IME 候选窗定位规则（旧项目遗留缺陷）

**现象**（旧项目 `crabsnap` 归档前遗留，用户报告）：重新聚焦 app/surface 后，
**第一次** IME 输入的候选窗位置不对，第二个字符起才正常。

**根因**（**新仓库已核实**，步骤 5）：候选窗由 compositor 按它**最后收到的 `set_cursor_rectangle`**
摆放。`enable` 会重置 compositor 侧保存的这项状态，所以与 `enable` 落在同一次 `commit` 里的
cursor_rectangle 被一并丢弃，compositor 退回 (0,0)，候选窗出现在左下角。只发增量更糟：
重新聚焦后 `enable` 的副作用一直留着，直到下一次状态变化才补上。

**规则**（写进实现，不许绕开）：

1. **没有本帧算出的插入符矩形就不 `enable`**。`ImeState.cursor` 为 `None` 时
   `enabled` 必须为 `false`，宁可晚一帧开 IME。
2. `set_cursor_rectangle` + text-input `commit` 与这一帧的 `wl_surface.commit` **同一批**发出：
   `AppLoop::frame` 在 `present` 之前调用 `set_ime`，Wayland 侧在这里发状态并 `commit`。
3. 焦点变化与 `enter` 都会重置 compositor 侧状态：`enable` 后必须重发 surrounding / content_type /
   cursor_rect，不能只发增量。
4. 一帧没有脏区但 IME 状态变了，也要 present（否则 v2 的两阶段矩形没有对应的 surface commit）。

**验证**（步骤 5 必须做，且先红后绿）：点字段 A 输入 → 点字段 B（不同位置）→
输入**第一个**字符，候选窗必须立刻在 B；切到别的窗口再切回，重复一次。
Hyprland 是 wlroots，只 advertise v1，这条路径必须单独测。

**验证结果**（**新仓库已核实**）：嵌套 Hyprland + 独立 fcitx5 实例（私有 `dbus-daemon`，
`fcitx5-remote -s pinyin` 切到拼音，**没有**碰用户的 fcitx5）+ `examples/type_text.rs`
经 `zwp_virtual_keyboard_v1` 发键。修 `apply_ime` 之前候选窗固定在窗口左下角，修好后
第一个字符的候选窗就在插入符处；组合串 `ni hao` 期间只有 `Event::Preedit`（`n`、`ni`、
`ni h`、`ni ha`、`ni hao`），没有 `Event::Text`，字段缓冲保持 `"hi"` 不变。

**复现步骤**：`scripts/nested-session.sh start` 后，用一个私有 `dbus-daemon --session` 的地址
（`DBUS_SESSION_BUS_ADDRESS`）加 `XDG_CONFIG_HOME=/tmp/fcitx5-nested/config` 另起 `fcitx5`
——**不要** `--replace`，那会顶掉用户正在用的输入法；焦点落到字段后 `fcitx5-remote -s pinyin`
切到拼音，再 `cargo run -p ui-platform-wayland --example type_text -- nihao` 发键。

## 7. 实施步骤

| 步 | 内容 | 验证 |
|---|---|---|
| 1 | `TextPass` 三个几何查询；插入符绘制改为复用 `caret_rect` | 真实字体单测：`index_at`/`caret_rect` 往返；临时破坏使其变红 — **已完成** |
| 2 | `Sense::SELECT` + 选区状态 + 拖选/双击 + `Dep::Selection` + 三个绘制程序 + gallery 三列 | 引擎集成测试 + 嵌套会话截图 — **已完成**：`a_drag_across_selectable_text_records_the_range_it_crossed`；画廊自检 `selection {md3,fluent,glass} wash=true` 与 Fluent `inverted=true`；三张截图各确认一种绘制 |
| 3 | 编辑状态机：锚点、Shift、Home/End/Delete、Ctrl+A、点击定位、替换选区 | 引擎测试：`Change::Text` 内容与插入符位置 — **已完成**：`editing: Option<(Id, TextSelection)>`；命令键走 `Key::Char`；画廊自检覆盖点击定位与字段内选区 |
| 4 | 预编辑渲染 + commit 合流 + `ImeState` 外送 | 引擎测试：组合期间 app 收不到 `Change::Text`，commit 后收到一次 — **已完成**：`composing_text_does_not_reach_the_app_until_it_commits`（先红后绿）；画廊自检 `ime composition underlined=…`（先红后绿）；截图见 `にほ` 带下划线且 `app buffer` 仍为 `"hi"` |
| 5 | Wayland v3 接线 + IME 状态机（纯逻辑可单测） | 嵌套会话 + 真实 fcitx5（独立实例，不得 `--replace`）— **已完成**：`zwp_text_input_v3` 接线，`done` 合批；独立 fcitx5 下候选窗在插入符处（§6），组合期间无 `Text` |

## 8. 风险

1. **声明缓存**：漏记 `Dep::Selection`/`Dep::Preedit` → 选区不动（D6）。
2. **命中**：打开 `NodeKind::Text` 的命中后，字段里的输入文本不得自己可选中，否则点输入框变成只读选择。
3. **几何分叉**：查询与绘制必须用同一个 `TextPrim`（`design-errors.md` 第 187/188 条）。
4. **Wayland**：`done` 前不应用事件；Mutter 会发非法 preedit 光标，要 clamp；labwc 不发 `done`，不能硬等。
5. **性能**：选中/聚焦时每帧多 1–3 次整形；长文本先保正确，必要时上 D2 的备选。
6. **`&'static` 绘制程序**：需要携带数据的绘制程序得自己 `OnceLock`/`Box::leak`，文档里写清楚。

## 9. 字簇与 emoji 呈现（2026-11-04 补）

编辑与整形的单位都是**字簇**，不是码点：`3️⃣`、`👨‍👩‍👧`、分解形式的 `é` 各自是「一个字符」。

- 插入符：`ui_core::grapheme` 提供 `clusters`/`snap`/`ceil`/`next`/`prev`；`clamp` 退到簇首，
  `moved`/`backspace`/`delete`/`word_at`/`step_word` 全部按簇步进，`TextPass::index_at` 的命中
  同样退回簇边界。任何内部偏移（命中、IME、过期插入符）都不会停在簇内。
- IME：`DeleteSurrounding` 的长度是**字节**（协议原文），起点退到簇首、终点扩到簇尾。
- 整形：`shape_covering_split` 只在簇边界切；一段的字簇呈现形式一致时才允许单个字面接管
  （`shape_run_parts`）。呈现形式按 UTS #51 逐簇判定（`ui_text::emoji`，`parley_emoji` 的
  状态机），`❤` 是文本、`❤️` 是 emoji、`3️⃣` 是 emoji、`4` 是文本。
- 理由与回归检查见 `docs/decisions/text-clusters-and-emoji-presentation.md`。
