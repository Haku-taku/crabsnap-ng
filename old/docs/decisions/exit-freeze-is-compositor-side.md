# ADR: 退出冻结不在客户端拆除路径上，是 Hyprland 释放 dma-buf 的忙等

状态：**已解决**（2026-11-01）。根因是客户端每帧新建 `wl_buffer`，compositor 积压了大量
待拆的 dma-buf 导入对象；改为按 dma-buf 身份复用后冻结消失（见 `explicit-sync.md`）。
诊断开关已从代码中移除；本文记录的实验需临时加回才能重跑。

## 现象

关闭 gallery（GPU presenter）窗口后，整个输出停摆数秒：光标冻结、画面不动、Hyprland
主线程 `wchan=0`（用户态忙转），见证进程（同 compositor 另一窗口）心跳正常但帧回调停。
应用进程自身 0–5ms 就退完。

## 二分过程

| 实验 | 结果 |
|---|---|
| 应用进程退出耗时 | 永远 0–5ms，不是瓶颈 |
| `EXIT_KEEP_EGL` / `EXIT_KEEP_GL` / `EXIT_KEEP_GBM` / `EXIT_KEEP_TOPLEVEL` / `EXIT_NO_DETACH` / `EXIT_KEEP_BUFFERS` | 全部仍冻结 |
| `GALLERY_EXIT_RAW=1`（收到 Close 立即 `process::exit(0)`，零拆除裸断连） | 仍冻结 3.3s |
| 受测窗口 CPU presenter（wl_shm）退出 | **不冻结** |
| 见证窗口（dma-buf）退出 | **冻结** |
| 嵌套 Hyprland | 不冻结（环境不等价，作废） |

## 结论

冻结是 **Hyprland（0.56.0 dirty 自定义构建）释放一个 dma-buf 窗口缓冲时，在 NVIDIA
驱动路径上用户态忙转数秒**。wl_shm 窗口无此问题，这解释了其他程序（GTK/Qt/egui 多用
wl_shm）退出不卡。所有客户端拆除路径（含零拆除裸退）都无法阻止——冻结发生在 compositor
持有并释放 dma-buf 时，客户端进程已消失。

上游类似报告：Sunshine #3854（NVIDIA + Hyprland 关闭 GPU 客户端冻结，已在 beta 修复）、
xdg-desktop-portal-hyprland #403 / #434（NVIDIA dma-buf 释放路径卡死）。

## 实际修法

上面「方案」里的 1（升级 Hyprland）试过，**无效**。真正的原因是客户端侧的：

每帧 `create_dmabuf_buffer` 都新建一个 `wl_buffer`，compositor 因此每帧新建并排队销毁
一个 dma-buf 导入对象；窗口活得越久积压越多，关闭时一次性清理，NVIDIA 路径上就忙转数秒。

改为**按 dma-buf 身份复用 `wl_buffer`**（`dmabuf_cache`）后冻结完全消失——用户确认
「缓冲复用之后完全不卡顿了」。同批加上了 `wp_linux_drm_syncobj_v1` 显式同步，见
`explicit-sync.md`。

这也解释了为什么 wl_shm 窗口不冻结：shm 缓冲没有 dma-buf 导入对象可拆。

## 回归检查

`scripts/freeze-test.sh`（见证 + 关闭被测窗口）可复现；`scripts/compositor-threads.sh`
采样 compositor 线程 `wchan`，冻结表现为主线程 `wchan=0` 忙转。
