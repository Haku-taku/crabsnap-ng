# 快捷键系统

> 状态：引擎与应用内部分已实现并有测试；全局热键（桌面级）只定义了接口，平台实现待批。

## 1. 模型

| 类型 | 位置 | 作用 |
|---|---|---|
| `KeyChord { key, mods }` | `ui-core/src/shortcut.rs` | 一个键 + 必须同时按住的修饰键；**精确**匹配（多一个修饰键就是另一条） |
| `ChordPattern { keys, mods }` | 同上（待实现，见 §2.1） | 宽匹配：键集合 + 每个修饰键 `Mod{On,Off,Any}` |
| `ShortcutId(&'static str)` | 同上 | 稳定名字，应用据此改写内置绑定 |
| `Change::Shortcut(id)` | `ui-core/src/change.rs` | 节点上报"我注册的某条被触发了" |
| `Change::KeyChord(chord)` | 同上 | 捕获节点上报"用户按了这个组合" |
| `ContainerParams::shortcuts` | `ui-core/src/tree.rs` | 节点上的绑定表；`Decl::shortcut(chord, id)` 追加 |

`Key::Char` 按未加 Shift 的形式存放，`same_key` 对字母忽略大小写：平台送来的 `Char('K')`
（Shift 已生效）能匹配 `Ctrl+Shift+K`。

## 2. 解析顺序

一次 `KeyPressed` 在 `AppLoop::dispatch` 里按层解析，**先命中先赢**：

1. **捕获节点**（`Sense::CAPTURE_KEYS`）：焦点链上最近的捕获节点直接拿走按键，得到
   `Change::KeyChord`。Tab 除外，否则焦点环会被困住。
2. **焦点链**：从聚焦节点向上，最近的绑定者得到 `Change::Shortcut`。
3. **命中链**：`pointer_path` 由深到浅，仅悬停即可认领。
4. **应用级**：`AppLoop::bind_shortcut` 注册的，交给 `App::shortcut(id)`。
5. **内置表**：引擎自己的文本编辑动作（见 §3）。

返回 `None` 表示无人认领，按键继续落到普通按键分支（方向键导航、Enter/Space 激活）。
契约测试：`a_chord_resolves_focus_then_the_hit_chain_then_the_app`。

### 2.1 宽匹配（待实现）

需求：注册**一条**处理程序，同时覆盖 `←/→ × shift=0/1 × ctrl=0/1` 的四种交叉，而不是写四条。
主流实现给了同一套形状——Qt 的 `PartialMatch`、GTK 的 `GtkShortcutTrigger`、VS Code 的
`when` 子句与负向修饰、Flutter 的 `SingleActivator(includeRepeats:)` 都是「未指定的修饰键 =
通配」。我们的形状：

```rust
pub enum Mod { On, Off, Any }              // Off 必须没按；Any 按不按都行
pub struct ChordPattern { keys: Vec<Key>, mods: [Mod; 4] }   // ctrl / shift / alt / super
```

- **同一层内按特异度排序**：非 `Any` 的修饰键越多越优先，其次键集合越小越优先，最后按注册顺序。
  于是「精确绑定压过通配绑定」是排序的自然结果，不需要两层表。
- **`Change::Shortcut` 必须带上实际触发的 `KeyChord`**（`Change::Shortcut(ShortcutId, KeyChord)`），
  否则一个处理程序收到四种情况时分不清是哪一个。这是破坏性改动，需同步 `dispatch` 与所有读者。
- **锁键先掩掉**：`NumLock` / `CapsLock` 不参与匹配（GTK 与 Qt 都这么做），否则同一条绑定在不同
  锁状态下行为不同。
- **影子查询**：`Bindings::shadowing(&ChordPattern) -> Option<ShortcutId>`，供快捷键输入控件判断
  「这条会和谁冲突」。注册时不静默覆盖。
- **全局热键不能宽匹配**：portal 与各平台的注册 API 都只接受具体加速键。通配模式由调用方
  展开成有限个具体组合（`2^n` 修饰键 × 键数，需设上限），或在 `GlobalHotkeys::register` 处拒绝。

## 3. 内置表与改写

内置 id 全部定义在 `ShortcutId` 上，默认绑定：

| id | 默认组合 |
|---|---|
| `text.copy` / `text.cut` / `text.paste` | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` |
| `text.select-all` | `Ctrl+A` |
| `text.delete-forward` | `Ctrl+D` |
| `text.home` / `text.end` | `Home` / `End` |
| `text.extend-left/right/up/down` | `Shift+方向键` |
| `text.extend-home/end` | `Shift+Home` / `Shift+End` |

`AppLoop::rebind_shortcut(id, chord)` 把某条改到别的组合键（留在原层），
`unbind_shortcut(id)` 让它彻底不响应。内置动作本身写在 `run_builtin`，只读取
`self.editing`，不关心是哪个控件。

**注意**：这些动作原先写死在 `dispatch` 的按键分支里，现已迁入表；`Home`/`End`/`Ctrl+*`
的旧分支已删除，否则改写不会生效。方向键（无修饰）与 `Tab`、`Enter`/`Space` 仍是普通分支：
它们是导航与激活，不是可改写的命令。

**已补（2026-10-31）**：`Ctrl+Home` / `Ctrl+End` 及 shift 变体已进表
（`text.document-home/end`、`text.extend-document-home/end`）；`Ctrl+Shift+←/→` 已从硬编码分支
迁入可重绑的 `text.extend-word-left/right`，`step_editing` 里加了「解绑后不得回退成按词步进」的
守卫。测试 `ctrl_home_goes_to_the_document_start_not_the_line_start`、
`a_rebound_word_extend_still_extends_by_word`（均先红后绿）。

## 4. 修饰键与指针

`Cx::modifiers()` 与 `WidgetState::modifiers` 暴露按住的修饰键（记录 `Dep::Modifiers`，
缓存不会留下过期预览）。这是 `Alt+拖拽` 之类"修饰键改变手势含义"的唯一入口。

## 5. 快捷键输入控件

`ui-widgets-md3/src/shortcut_field.rs`：`Md3ShortcutField`，一个描边盒子。
聚焦后订阅 `CAPTURE_KEYS`，按下即得 `Change::KeyChord`；只按住修饰键时按
`modifiers_label` 实时预览（`Ctrl + Shift + …`）。组合键文案见
`KeyChord::label()`——**尚未走 Fluent**，见 §7。

## 6. 全局热键（用户已批准，实现进行中）

`ui-platform/src/hotkeys.rs` 已定义接口 `GlobalHotkeys`（`register`/`unregister`/`poll`）
与空实现 `NoGlobalHotkeys`。平台实现各自的代价：

| 平台 | 路径 | 代价 / 风险 |
|---|---|---|
| Wayland 标准 | `org.freedesktop.portal.GlobalShortcuts`（xdg-desktop-portal 1.17+） | 需要 D-Bus 客户端依赖（`zbus` 或 `ashpd`）；绑定要经 portal 的**用户确认对话框**，是 UX 决定；旧版 portal 不支持 |
| Wayland 私有 | Hyprland IPC / `wlr-virtual-keyboard` 组合 | 与 compositor 绑定，每个 WM 各写一份 |
| Windows | `RegisterHotKey` | 本机无法验证 |
| macOS | `RegisterEventHotKey` / Carbon | 本机无法验证 |

**需要用户决定的点**：① 是否接受新增 D-Bus 依赖；② portal 的确认对话框流程是否可接受；
③ 私有 IPC 要不要做（Hyprland 优先？）。在此之前 `NoGlobalHotkeys` 是唯一实现。

### 6.1 本机 portal 实测（2026-10-31，用户已批准走 portal + 可选 feature）

- 主会话 D-Bus 上 `org.freedesktop.portal.GlobalShortcuts` **存在，`version` 属性 = 1**，
  方法 `BindShortcuts`/`ListShortcuts`/`ConfigureShortcuts` 与信号 `ShortcutsChanged` 齐全；
  backend 为 `org.freedesktop.impl.portal.desktop.gnome`/`gtk`。→ portal 路径在本机可用。
- **crate 选择的代价**：`ashpd 0.13.13` 在本地 registry，但它基于 **tokio 异步**，与本引擎
  「单线程、`poll_once` 驱动」的窗口循环冲突（`ui-platform-wayland/src/window.rs` 的 `wait`/
  `poll_once` 是同步 poll）；且 zbus 的 blocking `SignalIterator::next()` 内部 `block_on`、无超时
  变体，纯同步 poll 信号拿不到非阻塞结果。落点：**`zbus` 手写 proxy + 后台线程收信号**，
  照仓库既有先例 `ui-platform-wayland/src/theme.rs` 的 `watch_loop` + `Arc<Mutex<…>>`
  （主线程 `poll()` 读共享值），不引 tokio、不用 mpsc。
- **触发器编码（更正）**：portal 的 `BindShortcuts` 收 `a(sa{sv})`，每个 shortcut 的 info 里
  是 **`preferred_trigger (s)`**——一个**字符串**，文法由 freedesktop Shortcuts spec 定义：
  `CTRL`/`ALT`/`SHIFT`/`NUM`/`LOGO` 大写 token 用 `+` 连接，再 `+` keysym 名（去 `XKB_KEY_` 前缀）。
  例：`CTRL+SHIFT+k`、`CTRL+ALT+Return`。**不是** `(combined_key, modifiers)` 数值对。
  > 本节此前写的位掩码表（`Shift=1, Ctrl=4, Super=64` 等）是 **X11/GDK 的 `Mod*Mask`**，
  > portal 协议里没有这张表，写它是错的——本机 portal XML 中 `combined_key|modifiers` 命中数为 0。
  > 记入 `docs/lessons/design-errors.md`。
  字母/数字的 keysym 名就是字符本身，但符号键名不是字符（`/`→`slash`、`+`→`plus`），
  映射表须照 `xkbcommon-keysyms.h`，覆盖不到的键返回不注册、不猜字符串。
- 做成 `ui-platform-wayland` 的**可选 feature**（`global-hotkeys`），默认关；
  `GlobalHotkeys` trait 已存在（`ui-platform/src/hotkeys.rs`），`NoGlobalHotkeys` 是默认实现。
- **后端限制**：这个 portal **没有 Unbind/Release 方法**，`unregister` 只能丢前端
  `ShortcutId→表` 映射（session 语义内的归还），不能真正解绑。端到端 bind/确认框在无头环境不可验证。
- **一个会话只能绑一次**（XML `BindShortcuts` 说明原文：*An application can only attempt bind
  shortcuts of a session once*）→ `PortalHotkeys` **每次 `register` 各开一个 `CreateSession`**，
  并把这些会话路径记进共享信箱：`Activated` 按「是否指向我方会话」过滤，`close()` 逐个
  `Session.Close` 释放全部绑定。若沿用单个会话，第二条热键会被门户直接拒。
- **每个门户请求各有 `handle_token`**：token 决定该请求的对象路径，复用会把两个请求
  放到同一路径上互相抢回复。

### 6.2 端到端仍未验证的部分（无头环境的边界，如实标注）

`codec.rs` 的编码/解码有 6 条单测（含先红后绿），但以下**只能在真实桌面会话里验**：

1. `BindShortcuts` 往返与确认对话框（需人点确认）；
2. `ShortcutsChanged` 信号——接口有，本实现**未订阅**，`poll` 只靠 `Activated`；
   用户在系统设置里改了绑定我们不会知道，需真实会话确认差异后再补；
3. `Response` 回填的 `shortcuts` 子集（XML 说它是传入项的子集，含空集）——
   各 backend 是否真填、`Taken`（别的程序已占用）如何表达，**离线无法证实**。
   本实现按保守策略：`code == 0` 视为接受，其余按 `Refused`/`Failed`，**不猜 `Taken`**。

## 7. 已知缺口

- ~~`KeyChord::label()` 硬编码英文~~ **已解决（i18n 第一阶段）**：`KeyChord::label` 已从
  `ui-core` 移除，改为 `ui_widgets::shortcut_label(chord, cx)` / `modifiers_label(mods, cx)`，
  走 Fluent（`ui-widgets/src/shortcut.rs`，键名见 `locales/*/widgets.ftl`）。
- 应用级绑定没有"作用域"概念（Qt 的 `ShortcutContext`）：目前只有"控件内"与"整个应用"两层。
