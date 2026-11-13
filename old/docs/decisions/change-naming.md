# Change 该不该改名

> 状态：**提议，待用户拍板**。数字都算过，改名随时可做。

## 起因

`Change` 如今装着 `Toggled / Value / Text / Selected / Activated / DragStarted / Moved /
Shortcut / KeyChord / Data …`（13 个变体）。其中 `Activated`、`Shortcut`、`KeyChord`
根本不是「变化」，是控件层的通用上报。名字确实开始说谎了。

## 为什么不能叫 `Event`

`Event` 在本仓库已经被**输入事件**占用，而且是引擎里最基础的那个名字：

| | `ui_core::Event`（输入） | `ui_core::Change`（控件上报） |
|---|---|---|
| 位置 | `ui-core/src/event.rs` | `ui-core/src/change.rs` |
| 词边界出现次数 | **381** | 198（连同 `*Change` 组合词共 247） |
| 产生者 | 平台后端（wayland / 测试桩） | 控件 |
| 消费者 | `AppLoop::dispatch` | `App::changed` |

两个名字会同时出现在 `view.rs`（那里 `Event` 单独就有 205 处），于是每次都要
`ui_core::Event::PointerMoved` 与 `ui_core::Event::Activated` 这样带路径地写——比现状更糟。
另外 wayland 后端里的 `wl_keyboard::Event`、`Proxy::Event` 是 wayland 自己的 trait，
不可能让名。

## 成熟框架的叫法

把「输入事件」与「控件的语义上报」分成两个词的，是清一色的：

| 框架 | 输入事件 | 控件上报 |
|---|---|---|
| Qt | `QEvent` | **signal**（`clicked()`、`toggled(bool)`） |
| GTK | `GdkEvent` | **signal**（`g_signal_emit_by_name("toggled")`） |
| Flutter | `Event`/`SemanticsEvent` | `ValueChanged<T>`、`ChangeNotifier` |
| Elm/Tea | 消息总线 | **msg** |

也就是说：**`Event` 这个词在工业界专指输入流**，控件层上报另有其名。
把我们的控件上报改叫 `Event`，会站到这套共识的反面。

## 三个选项

1. **保持 `Change`**（成本 0）。理由：Flutter 用的就是 `ChangeNotifier`/`ValueChanged`，
   「change」在响应式语境里站得住；改名波及 247 个标识符 + 36 处文档引用，纯属名义。
2. **改成 `Signal`**（推荐，如果要在名义上较真）。Qt/GTK 的原词，正好表达
   「一个对象语义级、不带输入数据的上报」；不与任何现有名字冲突；模块 `change.rs → signal.rs`，
   `App::changed → App::signalled`（或保留 `changed`，参数类型改 `Signal`）。
3. **改成 `Event`**：如上，与输入事件同名，不建议。

## 时机

宽匹配那批要把 `Change::Shortcut(id)` 改成带 chord 载荷（`docs/shortcut-system-design.md` §2.1），
改的正是这个枚举。**若要改名，应在宽匹配之前做**，否则同一批行要动两遍。
