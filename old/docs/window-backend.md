# 窗口后端：Wayland 实测记录

> 验证目标：`window + surface + cpu + ui 框架` 四者组合是否真的成立。
> 结论：**成立，已在真实 Hyprland 会话里截图确认。**

---

## 0. 结论

```text
Declaration → RetainedTree → Layout → DrawList → zeno → Pixels → wl_shm → 屏幕
```

`ui-platform-wayland` 实现 `ui_platform::PresentTarget`，因此上层代码一行没改就换到了真实窗口。
这正是 `PresentTarget` 抽象存在的理由。

截图确认：窗口标题 `crabsnap-ui live`，圆角卡片 + 1px 边框 + 三个色块，全部由 UI 框架渲染。

---

## 1. 环境实测

```text
XDG_SESSION_TYPE=wayland    WAYLAND_DISPLAY=wayland-1    compositor: Hyprland
wl_compositor v6            wl_shm v1
zwp_linux_dmabuf_v1         已存在（未来 GPU 直出路径）
wp_color_manager_v1         已存在（未来 HDR 路径）
```

---

## 2. 遇到的问题（三个，都是真 bug）

### 2.1 只渲染 1 帧就退出

**症状**：`closed after 1 frames`。

**误判**：我以为是循环逻辑写错（空闲分支 `continue` 跳过重绘）。

**真因**：`present()` 内部调用了 `pump()`，而 `pump()` 派发了 `xdg_toplevel::Close`。
打印事件序列后才看到 `configure 1272x1397` —— **compositor 改了尺寸**，
缓冲分配失败返回 `Err`，而我的循环把它当成致命错误 `break` 了。

**教训**：这是 AGENTS.md §5 第 3 条的又一次应验——
**"探针没触发"要先确认探针插对了。** 我先猜循环逻辑，错了两次才去打印事件。

### 2.2 双缓冲互相覆盖

**症状**：`present failed: could not allocate a shm buffer`。

**真因**：初版只分配**一个** `ShmBuffer`，每次 present 都用 `pool.create_buffer(offset=0, ...)`，
于是新旧 `wl_buffer` 视图指向**同一块内存**。compositor 还在显示旧 buffer 时，
我已经往里写了新帧 —— 这是经典的 shm 竞态。

**修法**：两个槽位**各自拥有独立的 pool**。`front` 被释放后轮换回 `back`，
从不复用同一个 pool 的两个 offset。

**这条值得记**：`wl_shm` 的 `create_pool` + `create_buffer(offset)` 看起来像是"从池里切一块"，
但它**不保证** offset 不重叠。想双缓冲就必须两个池，或者在 offset 上真正分工。

### 2.3 交叉轴不拉伸

**症状**：真实窗口里卡片只有 94px 宽，而 `window_hello`（离屏）里是对的。

**真因**：`ContainerParams.cross_align` 字段**存在但布局引擎从未读过**。
Column 的子节点因此永远紧贴自己的测量宽度，不会撑满。

**修法**：`place_node` 里实现交叉轴拉伸，`cross_align` 默认改为 `1.0`（撑满）。
加了测试 `column_children_stretch_across_the_cross_axis` 固定该不变量。

**这条和 §1.5 那两条同类**：字段写好了、文档写了、但其实没接线。
**"实物到了 ≠ 接线到了"**（AGENTS.md §5 第 1 条）。

---

## 3. 平铺 compositor 的尺寸问题

请求 `420x240`，Hyprland 返回 `configure 1272x1397`（它自己的平铺布局）。

试过 `set_min_size` + `set_max_size` 锁死尺寸 —— **无效**，Hyprland 忽略。

**这不是 bug，是正确的行为**：平铺型 compositor 决定窗口尺寸，应用必须适应。
UI 框架这边做对了：收到新 configure 就重建缓冲、重排布局、重绘。

浮层（layer-shell）或固定尺寸工具窗口可以用 `zwlr_layer_shell_v1` 绕开平铺，
那是后续可选项。

---

## 4. 当前能力与缺口

| 项 | 状态 |
|---|---|
| XDG toplevel 创建 / title / app_id | ✅ |
| `wl_shm` 缓冲 + 双缓冲轮换 | ✅ |
| configure → 重排 → 重绘 | ✅ |
| 指针 enter/motion/leave | ✅（悬停高亮已生效） |
| 键盘输入 | ⬜ |
| HiDPI 分数缩放（`wp_fractional_scale_v1`） | ⬜ |
| `wp_frame_callback` 节流 | 部分（callback 已创建，未等它再画） |
| dma-buf 零拷贝（`zwp_linux_dmabuf_v1`） | ⬜ |
| 其它平台窗口（Windows/macOS/X11） | ⬜ |

---

## 5. 怎么自己跑

```bash
cd ui
cargo run -p ui-platform-wayland --example window_live
```

需要 `WAYLAND_DISPLAY`。没有 Wayland 时程序会打印原因并退出（不 panic）。

---

## 6. 后续三项（帧节流 / 分数缩放 / 键盘）实测记录

### 6.1 帧回调节流

`wp_frame_callback` 一个提交一个回调；在它返回前不再画第二帧。
`WaylandWindow::can_draw()` / `wait_for_frame()` 暴露给应用循环。
等待有 500 ms 上限：**从不回应帧回调的 compositor 不能把应用卡死**。

### 6.2 分数缩放

需要三件套，缺一不可：

```rust
wp_viewporter.get_viewport(surface)                        // 把缩放后的缓冲映射回逻辑尺寸
wp_fractional_scale_manager.get_fractional_scale(surface)  // 收 preferred_scale
surface.set_buffer_scale(1);                               // 分数缩放下必须保持 1
```

**协议细节**：`preferred_scale` 的值是 1.0 的 **120 分之一**。
`192 → 192 × 100 / 120 = 160 → 1.60x`。

缓冲尺寸 = `逻辑尺寸 × scale`，UI 按逻辑单位排版，渲染器用 `set_scale()` 放大图元。

实测（Hyprland，临时用 `hyprctl eval` 设成 1.6x）：

```text
window created: logical (420, 240) scale 1.00 buffer (420, 240)
preferred_scale 192 -> 1.60x
frame 2: logical 792x857 @ 1.60x -> buffer 1267x1371
```

**注意**：`preferred_scale` 在窗口创建**之后**才到达，所以必须能处理运行时缩放变化，
不能只在构造时读一次。

### 6.3 键盘

用 `xkbcommon` 翻译 evdev → keysym。这里踩了一个**死胡同**：

| 做法 | 结果 |
|---|---|
| keymap 当 UTF-8 读出、**去掉**结尾 NUL → `new_from_string` | `XKB-822: Failed to parse input xkb string` |
| **保留**结尾 NUL → `new_from_string` | `CString::new` 返回 `NulError`，`.unwrap()` **panic**，把整段 keymap 字节刷到终端 |

**两条路都不通。** 正确做法只有第三条：

```rust
unsafe { xkb::Keymap::new_from_fd(&ctx, fd, size, XKBV1, NO_FLAGS) }
```

它内部用 `xkb_keymap_new_from_buffer(ptr, size - 1)`，把结尾 NUL 排除在解析范围外，
`CString` 根本不参与。**这就是协议为什么传 fd 而不是传字符串。**

实测（手动敲键）：

```text
keymap 35572 bytes -> parsed true
frames 60, text "ssdasdasdasda"
```

### 6.4 一个反复出现的失误

这一轮我有**至少四次** `python` 字符串替换没有真正落盘，却以为改好了——
于是花了大量时间在旧代码上找 bug。现象是"日志没打印"，而我先怀疑日志位置，
再怀疑环境变量，最后才发现文件根本没变。

**AGENTS.md §5 第 1 条说"实物到了 ≠ 接线到了"。这次是它的变体：
改动调用返回成功 ≠ 文件变了。**

每次改动后必须回读确认，尤其在"改了但没效果"的时候——
那不是调试信息不够，是改动没落地。
