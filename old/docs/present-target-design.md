# 呈现归属：引擎递场景，目标只负责上屏

> 一屏笔记：应用不碰图集、不碰提交、不碰光标。它只声明内容。

## 1. 问题（2026-11-05 实测）

GPU 路径长期绕过 `PresentTarget`：gallery 每帧自己调 `sync_atlas`、`present`、
`win.set_cursor`、`win.set_ime`，并自己判断「这一帧要不要提交」。于是同一套帧规则
在 CPU 路径（引擎里）和 GPU 路径（应用里）各写了一遍，提交条件里的
`win.ime_pending()` 只在应用那一份存在。

## 2. 契约

两个 trait，各自对应一种上屏方式，都在 `ui-platform`：

| trait | 形态 | 谁实现 |
|---|---|---|
| `PresentTarget` | 引擎渲染进 `pixels_mut()`，再 `present(damage)` | `OffscreenTarget`、CPU 光栅窗口 |
| `SceneTarget` | 引擎交出 `DrawList` + `TextPass` + `Damage` | GPU 呈现器（`ui-platform-wayland`） |

两者都有 `set_cursor` / `set_ime`，`SceneTarget` 另有 `commit_pending()`（表面欠一次
与画面无关的提交，例如待发的输入法状态）。

`AppLoop::frame_scene(target, pointer)` 的返回值是
`SceneFrameOutcome { outcome, presented: Option<PresentStats> }`——`presented` 为
`None` 就是「这一帧没有值得上屏的东西」。

## 3. 提交规则（引擎唯一一处）

```
presented = damage 非空 || 仍在动画 || 输入法状态变化 || target.commit_pending()
```

第四条不能省：跳过它会留下合成器里的旧插入符矩形，而下一帧可能永远不来。

## 4. 循环：`Driver` 持有状态，应用只拿 `Turn`

```rust
pub trait FrameHost {          // 窗口实现一次（ui-platform-wayland）
    fn size(&self) -> (u32, u32);
    fn scale(&self) -> f32;
    fn pointer(&self) -> Option<Point>;
    fn can_draw(&self) -> bool;
    fn wait(&mut self);
    fn wait_for_frame(&mut self);
    fn should_close(&self) -> bool;
    fn take_events(&mut self) -> Vec<Event>;
    fn take_silent_change(&mut self) -> bool;
}

pub enum Turn { Idle, Waiting, Draw { pointer: Option<Point> } }
```

`Driver::step` 做三件事：把事件摄入 `AppLoop`、判定这一帧要不要画（`take_dispatched` ∨
`take_silent_change` ∨ `is_animating` ∨ 尺寸变化）、按判定睡觉或等帧回调；要画时顺手把
`set_scale` 设好、把指针交回。`Driver::pace` 在提交之后等下一个回调，让下一轮落进新的
帧窗口。`uncapped` 是驱动的一个开关，不是应用的一个分支。

判据：**应用不该知道「什么算需要一帧」**，也就没有第二份判定条件。

## 5. 生命周期：`Session` 持有窗口、呈现器与循环

`ui-platform-wayland::Session<A>` 是引擎交给宿主的唯一句柄：

```rust
pub struct Session<A: App> {
    gpu: Option<GpuWindow>,   // 字段顺序 = 拆除顺序
    driver: Driver,
    lp: AppLoop<A>,
    win: WaylandWindow,
}
```

- `open(lp, SessionOptions)` 开窗、选呈现器；GPU 不可用时返回错误，除非要求 CPU 呈现器。
- `tick()` 一轮：驱动摄入/判定/等帧 → `frame` 或 `frame_scene` → 配速，返回 `Tick`。
- 拆除顺序由字段声明顺序保证：表面还在时先放 GPU 对象，再放循环，最后放窗口。
- `split()` 只在宿主确有跨层需求时使用（`route_clipboard`、脚本输入）；`set_pointer`
  让脚本指针成为本帧的平台指针。

应用侧因此只剩：`view`、自己的旋钮、以及 `tick()` 返回值的诊断。菜单外观也不再由
应用复制——`App::menu_style` 是声明，引擎在每次 `view()` 之后自取。

## 6. 主流对照

- **GPUI**：`Window::on_request_frame` 检查 `invalidator.is_dirty()`，命中则
  `Window::draw` → `platform_window.draw(scene)` → `surface.get_current_texture()`
  → render → `queue.present`；光标在每帧开头 reset，IME 在画完后 update。
- **iced**：runtime 拥有 winit 循环，`RedrawRequested` → `Renderer::draw` + `present`。
- **eframe**：`request_repaint()` 变成 repaint deadline；`pre_present_notify()` 让
  winit 在帧回调到达前扣住 `RedrawRequested`；缺回调的兜底明确归 toolkit。

三家一致：**应用只产出场景，提交时机与上屏细节属于框架。**

## 7. 验证

- `a_scene_frame_presents_only_when_it_has_something_to_carry` 钉住第 3 节的前两条
  与第四条（删掉 `!target.commit_pending()` 即红）。
- `CRABSNAP_TARGET_CHECK=1` 在 progress 页跑满 1814 帧，全程 `differing=0`、无
  `[frame-outside]`：这次搬家没有改变任何像素。
