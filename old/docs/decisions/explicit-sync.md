# ADR: dma-buf 复用 + 显式同步（`wp_linux_drm_syncobj_v1`）

状态：**已采纳**（2026-11-01，用户要求显式同步对齐主流框架）。

## 问题

两条独立的缺陷：

1. **每帧新建 `wl_buffer`**。`create_dmabuf_buffer` 每帧把 dma-buf 重新导入成 compositor 侧的
   缓冲对象，退出时 compositor 要一次性拆掉这些对象，Hyprland 主线程因此忙等数秒
   （见 `exit-freeze-is-compositor-side.md`）。
2. **没有显式同步**。客户端只提交 dma-buf，由 compositor 自己管隐式 fence；Hyprland 在
   NVIDIA 上会退化成 `glFinish()`，客户端侧的工作完成时刻对 compositor 不可见。

## 决定

1. **按 dma-buf 身份复用 `wl_buffer`**：键是 `/proc/self/fd/<fd>` 的 `(dev, ino)` 加尺寸、
   格式、stride、offset、modifier。同一块 dma-buf 始终对应同一个 `wl_buffer`。
2. **绑定 `wp_linux_drm_syncobj_manager_v1`**，导入 acquire / release 两条 timeline，
   `get_surface` 之后每帧 `set_acquire_point` + `set_release_point`（协议要求成对）。
3. **acquire fence 先试 native**（`EGL_ANDROID_native_fence_sync`）。驱动拒绝时（实测 NVIDIA
   返回 `EGL_BAD_ATTRIBUTE`）回退到 `EGL_SYNC_FENCE_KHR`，并**每帧 `glFinish`**：
   该标记只有在 GPU 工作完成后才 signal，因此 acquire point 恒为 0（已 signal）——
   语义上正确，只是没有异步收益。
4. 探测结果缓存在 `EglContext::native_fence`，只尝试一次，不在热路径上重复失败。

## 为什么

- 复用缓冲把「每帧一个新对象」变成「每窗口两个对象」，退出时的清理量与运行时长无关，
  这是冻结消失的直接原因（用户确认：缓冲复用后完全不卡顿）。
- 显式同步让 compositor 拿到确切的完成点，不必自己 `glFinish`。即使 point 为 0，
  也明确告诉 compositor「数据已就绪」，省掉它那一次隐式等待。
- `native` 标记是正确性开关，不是优化开关：`false` 时必须 `glFinish`，否则 compositor
  可能读到未落地的写入。

## 代价（明说）

- **NVIDIA 上拿不到异步收益**：`eglCreateSyncKHR(EGL_SYNC_NATIVE_FENCE_ANDROID)` 在
  NVIDIA 驱动上失败（`EGL_BAD_ATTRIBUTE`，已试 NULL / `[EGL_NONE]` /
  `[FD, EGL_NO_SYNC_KHR, EGL_NONE]` 三种属性列表）。于是每帧仍 `glFinish`。
  换成支持 native fence 的驱动（Mesa on AMD/Intel）会自动走真 fence，无需改代码。
- **release point 只设置、不消费**：缓冲可复用性仍靠 `wl_buffer.release` 事件，
  timeline 上的 release 点目前没有用于调度。若将来要做三缓冲以上的流水线，
  这里是要接上的地方。
- 缓存键含 `(dev, ino)`，依赖 `/proc` 可读；不可读时该帧退回直接创建（不缓存）。

## 回归检查

- `WAYLAND_DEBUG=1` 下 `import_timeline` / `get_surface` / 每帧成对的
  `set_acquire_point` + `set_release_point` 出现，且**没有协议错误**。
- 启动时打一行 `explicit sync: native fence` 或
  `explicit sync: no native fence, glFinish per frame`，指明本次走的路径。
- 关闭窗口后 compositor 不再停顿（`scripts/freeze-test.sh`）。
