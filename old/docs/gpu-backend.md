# GPU 后端与 dma-buf 零拷贝

> 本文记录**已实测**的技术选型，不是推测。每个结论都标了验证方式。

---

## 1. 为什么不是 wgpu

| 方案 | dma-buf 导出 | 结论 |
|---|---|---|
| **wgpu 30.0.1** | **无公开 API**（`wgpu-hal` 的 Vulkan 后端只在内部提到 external memory） | 做不了零拷贝，除非用不稳定的 hal 内部接口 |
| **EGL + GBM + OpenGL（glow）** | `EGL_EXT_image_dma_buf_import` + gbm_bo fd | **采用** |

这是本次最关键的一条否决：**wgpu 的跨平台收益换不来零拷贝**。
如果将来要 Windows，GPU 后端的 trait 边界允许再挂一个 wgpu 实现。

---

## 2. 实测环境

| 项 | Intel | NVIDIA |
|---|---|---|
| render 节点 | `/dev/dri/renderD128`（i915） | `/dev/dri/renderD129`（nvidia） |
| GL_RENDERER | Mesa Intel(R) Graphics (ARL) | GeForce RTX 5060 Ti, OpenGL ES 3.2 |
| `EGL_KHR_image_pixmap` | ✅ | ❌ |
| `EGL_EXT_image_dma_buf_import` | ✅ | ✅ |
| `EGL_EXT_image_dma_buf_import_modifiers` | ✅ | ✅ |
| `EGL_MESA_image_dma_buf_export` | ✅ | ✅ |

**关键差异**：NVIDIA 不支持 `EGL_KHR_image_pixmap`，
所以「gbm_bo 直接当 native pixmap 建 EGLImage」这条路在 NVIDIA 上不通。
**两边通用的路径是先把 bo 的 dma-buf 导入成 EGLImage。**

---

## 3. 零拷贝闭环（已在两块 GPU 上验证）

```text
gbm_bo_create(w, h, GBM_FORMAT_ARGB8888, GBM_BO_USE_RENDERING)
  → gbm_bo_get_fd / get_stride / get_offset / get_modifier
  → eglCreateImageKHR(EGL_LINUX_DMA_BUF_EXT, plane0 fd/pitch/offset/modifier)
  → glEGLImageTargetTexture2DOES  → GL texture
  → FBO 渲染
  → wl_buffer（zwp_linux_dmabuf_v1，同一个 fd）
```

验证方式：GL 清屏为纯绿，然后用 `mmap(fd)` 从 CPU 读同一块内存：

```text
Intel  : fd=8  stride=256 modifier=0x0              → 读到 [0, 255, 0, 255]
NVIDIA : fd=32 stride=256 modifier=0x300000000e08012 → 读到 [0, 255, 0, 255]
```

**这一步证明 GPU 写入的正是要交给 compositor 的那块内存**——这才是零拷贝的定义。

---

## 4. 踩到的两个坑

### 4.1 `EGL_WIDTH` / `EGL_HEIGHT` 是必填的

`eglCreateImageKHR(EGL_LINUX_DMA_BUF_EXT)` 的属性列表如果只给 format + plane 信息，
两块 GPU 都返回 `EGL_BAD_PARAMETER`。必须显式给宽高：

```rust
EGL_WIDTH, w, EGL_HEIGHT, h, EGL_LINUX_DRM_FOURCC_EXT, format, ...
```

### 4.2 GBM 配置不要指定 `SURFACE_TYPE`

Intel 的 GBM 平台用 `SURFACE_TYPE = EGL_PBUFFER_BIT` 找不到任何 config。
去掉该属性即可（我们本来也不需要表面，只渲染到纹理）。

### 4.3 modifier 必须传给 compositor

Intel 返回 `modifier=0x0`（linear），NVIDIA 返回 `0x300000000e08012`（tiled）。
**不能假设线性布局**：一字节偏移算错就整屏花屏。
`zwp_linux_buffer_params_v1` 的 `modifier_lo/modifier_hi` 必须如实传，
并且要按 `zwp_linux_dmabuf_v1` 的 modifier 通告选择 compositor 接受的组合。

---

## 5. 渲染器实现

crate：`ui/crates/ui-paint-gpu`，只依赖 `ui-core`（与 `ui-paint` 同层）。

| 模块 | 职责 |
|---|---|
| `ffi.rs` | `libgbm` 的 dlopen 绑定 + EGL 扩展常量 |
| `context.rs` | EGL 显示、GL 上下文、dma-buf 导入 |
| `target.rs` | `GpuTarget` = gbm_bo + EGLImage + GL 纹理 + FBO |
| `shader.rs` | 两个程序：SDF 圆角矩形、字形图集 |
| `renderer.rs` | 消费 `DrawList`，按 `Batch` 发出实例化绘制 |

### 5.1 两个着色器，一个混合方程

两者都输出**预乘 alpha**，所以 `glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA)` 对两者都成立。

- **矩形**：实例化的有符号距离场，1 设备像素羽化；边框用 `outer - inner` 的环带覆盖度
- **字形**：采样图集的 R 通道覆盖率，乘颜色后预乘

### 5.2 踩到的坑：输出直通颜色却用预乘混合

初版矩形着色器写的是 `frag = v_fill * outer`——rgb 只乘了覆盖度、没乘 alpha，
而混合方程是预乘的 `ONE, ONE_MINUS_SRC_ALPHA`。结果**半透明图层被整块加上去**：

```text
源色 (255, 200, 40) alpha=0.25，叠 5 层
CPU 实测 (198, 157, 36)   ← 正确，= 源色 × 0.762
GPU 实测 (255, 255, 128)  ← 绿通道 255 超过了源色的 200，物理上不可能
```

**判据**：混合后的任何通道都不该超过源色与该通道的较大者。
绿 255 > 200 直接证明是加法而非插值。

修法是显式预乘：`vec4(rgb * a * coverage, a * coverage)`。

### 5.3 四条不变量的实测结果

`cargo run -p ui-paint-gpu --example gpu_scene [render-node]`

| 不变量 | Intel D128 | NVIDIA D129 | 阈值 |
|---|---|---|---|
| 面积守恒 | **0.0000%** | **0.0000%** | < 0.5% |
| 无溢出像素 | **0.0000%** | **0.0000%** | < 0.05% |
| 总墨量 | **0.3558%** | **0.3558%** | < 0.5% |
| 边缘位移 | **0 px** | **0 px** | ≤ 1 px |

两块 GPU 的结果完全一致（都是 Mesa/NVIDIA 的 GLES 3.2 实现）。

### 5.4 一个必须记的字节序事实

`ui_paint::Pixels` 存的是**预乘 BGRA**，而 `glReadPixels(GL_RGBA)` 返回**直通 RGBA**。
两者直接比较会得到假差异：

- 不还原预乘 → 半透明区域偏暗（实测总墨量差 4.8%）
- 不交换 R/B → 红蓝颠倒（我一开始就踩了这个，蓝色盒子显示成橙色）

`GpuRenderer::read_pixels` 负责翻转 Y 并还原预乘；调用方仍需注意 CPU 侧的 BGRA。

---

## 6. 尚未做的

- **层的离屏渲染**（`Op::PushLayer`）：当前只处理裁剪，没做 layer 合成
- **`PrimKind::Text`**（未预光栅的原始文本）：GPU 侧无路径，需先过 text 栈
- **多缓冲轮转**：`GpuTarget` 目前一个窗口一个，没有 buffer pool
- **sRGB 正确性**：目标按线性存储处理，未做 `GL_FRAMEBUFFER_SRGB`
- **Windows/macOS**：`ui-paint-gpu` 的 trait 边界允许再挂一个 wgpu 后端
- **`PresentTarget` 是 CPU 像素导向的**：它要求 `pixels_mut() -> &mut Pixels`，
  GPU 目标没有 CPU 像素，套不进去。GPU 呈现需要独立路径，
  或把 trait 拆成"尺寸 + 提交"与"CPU 像素"两部分。这是目前的架构债。

---

## 7. 零拷贝端到端（`window_gpu`）

`cargo run -p ui-platform-wayland --example window_gpu --features gpu`

实测：2160 帧 / 2158 次 `wl_buffer.release` / 恒定 2 个 in-flight。
`release` 数 ≈ 帧数 − 2 正是双缓冲的预期行为。

屏上验证（`grim` 全屏扫描）：

```text
蓝色卡片  x 2382..2490  y 81..162      ← 窗口顶部，符合 y=40..130 的逻辑坐标
绿色字形  x 1347..1449  y 1341..1368   ← 窗口底部，符合 y=1301..1333
```

### 7.1 坑一：`glFinish` 是必需的

第一版没有 `glFinish`，窗口内容是**全黑**——但同一块 bo 用 CPU `mmap` 读出来是正确的
（实测 GL 读回 `[18,20,26]`，mmap 读回 BGRA `[26,20,18]`，一致）。

原因：GPU 写入尚未完成，compositor 就采样了这块 dma-buf。
交给另一个进程的缓冲必须先把写入完成（隐式同步没有自动覆盖这条跨进程路径）。

### 7.2 坑二：必须渲染在"驱动已连接输出"的那块 GPU 上

本机是双 GPU（Intel i915 + NVIDIA RTX 5060 Ti），**DP-6 由 NVIDIA 驱动**。

在 Intel（renderD128）上渲染、交给 NVIDIA 的 compositor：**导入成功、`release` 正常、但显示全黑**。
改成 NVIDIA（renderD129）立刻正确。

`output_render_node()` 从 `/sys/class/drm/card*/…/status` 找 `connected` 的端口，
再按 `device` 符号链接匹配 `renderD*`。`window_gpu` 默认就用它。

> **这条推翻了"两块 GPU 都能导入"的乐观结论**：协议层"能导入"不等于"能正确显示"。

### 7.3 坑三：Y 方向不要翻转

GL 帧缓冲原点在左下，习惯上会写 `ndc.y = 1 - y/H*2`。但对 dma-buf 这是错的：

- GL 的第 0 行（`y=0`）与 dma-buf 的第 0 行是**同一条内存行**
- compositor 把该行当作图像的**顶部**

所以投影必须是 `ndc.y = y/H*2 - 1`，让 UI 的 `y=0` 落在 GL 的 `y=0`。
第一版沿用了习惯写法，结果**整幅画面上下颠倒**（蓝卡跑到窗口底部）。

连带修正：`read_pixels` 不再翻转 Y（渲染已是自顶向下）。四条不变量在修正后仍全部通过。

### 7.4 一个流程坑（不是代码坑）

`pkill -f 'examples/window_gpu'` 会**匹配到执行它的 shell 自己的命令行**，把 shell 杀掉，
于是同一条命令里后续的编辑脚本根本没执行——我因此白跑了一轮"修复已验证"的假象。
用 `pkill -x window_gpu`（按进程名精确匹配）。

---

## 8. Windows：交叉编译 + Wine 的可行边界

**问题**：能否用交叉编译 + Wine 开发/验证 Windows 后端，而不必先备好 Windows 机器？

**结论**：**可以，而且比预期走得更远**——但有三条边界必须清楚。

### 8.1 实测工具链

| 项 | 状态 |
|---|---|
| `mingw-w64-gcc` / `-ld` | 已装（`x86_64-pc-windows-gnu` 链接用） |
| Rust Windows target | 独立 rustup：`RUSTUP_HOME=~/.rustup-win`，`CARGO_HOME` 复用系统 registry |
| Wine | 11.20 Staging，含 `d3d11/d3d12/dxgi/dcomp/dwrite` + DXVK |
| XWayland | `DISPLAY=:0`，Wine 的窗口被 Hyprland 正常平铺 |

版本正好对齐：rustup 拉到 `1.99.1`，与系统 rustc 一致。

### 8.2 实际跑通的东西

| 验证项 | 结果 |
|---|---|
| 交叉编译本项目 5 个可移植 crate | ✅ `ui-core/ui-paint/ui-text/ui-text-fontique/ui-platform` |
| `ui-platform-wayland` | ❌ 预期内（`wayland-sys` 需要 unix），OS 层被正确隔离 |
| Win32 窗口（`windows-sys`） | ✅ 窗口在 Wine 下真实显示，标题栏正确 |
| GDI 文字绘制 | ✅ 屏上可见 |
| **DirectWrite 字体枚举** | ✅ `DWriteCreateFactory` + `IDWriteFontCollection`，**160 个字体族** |
| 通用族解析 | ✅ Sans/Serif/Mono 都解析到 face |
| **42 个文本栈测试在 Wine 下通过** | ✅ 单线程与并行都是 42 passed |

DirectWrite 那条是关键：`fontique/src/backend/dwrite.rs` 用的是真的 `DWriteCreateFactory`，
不是扫字体目录。所以**文本栈在 Windows 上不是"仅编译通过"，而是功能上跑通了**。

### 8.3 三条边界（Wine 不能替代真机）

1. **字体回退数据不完整**
   `fallback[Script::ARABIC]` 在 Wine 下返回 **0 个 face**（Linux 上有 1 个）。
   所以阿拉伯文/希伯来文的回退行为**不能**用 Wine 验证，必须真机。

2. **D3D12 与 DirectComposition**
   Wine 的 D3D 走 DXVK→Vulkan 转译，`dcomp.dll` 只是部分实现。
   基于 DirectComposition 的呈现器、D3D12 的特殊特性、以及 DWM 的合成行为
   （阴影、圆角、遮挡、vsync 语义）**都不能**在这里验证。

3. **HDR / 色彩管理**
   完全无法验证。

4. **并发初始化 DirectWrite 会崩**
   这是实测出来的，结论很明确：

   | 配置 | 结果 |
   |---|---|
   | 并行 + 不用 DirectWrite（无 `system` feature） | ✅ 41 passed, exit 0 |
   | 并行 + 用 DirectWrite | ❌ `page fault on read access to 0xf`, exit 5 |
   | 串行 + DirectWrite | ✅ 42 passed, exit 0 |
   | 2 线程 + DirectWrite | ❌ 同样崩 |

   即 **Wine 下多线程同时初始化 DirectWrite 会崩**。我们的测试各自新建 provider、
   不共享状态，串行跑完全正常，所以这更像 Wine 的 COM/DirectWrite 实现问题，
   而不是我们的代码缺陷。

   **实践**：Wine 下跑测试加 `--test-threads=1`。
   **但这条必须在真机 Windows 上复核**——若真机上也会崩，那就是必须修的真实缺陷
   （届时要把 provider 初始化收敛到单点或加锁）。

### 8.4 一个被这套工具链抓到的真实测试缺陷

跑 Windows 测试时 `blit_run_rasterizes_at_device_resolution` 失败：

```text
2x must rasterize twice as tall: 12 -> 21
```

13px 下小字号，轮廓整数取整占高度的比例太大（12/13 vs 21/26 = 1.75 而非 2.0）。
**这条测试在 Linux 上只是靠特定字体侥幸通过**——换个字体就暴露了。
修法是把字号提到 64px（取整影响可忽略），容差从 ±20% 收紧到 ±10%。

这正是交叉编译的价值：**不同平台的字体不同，能照出"依赖特定字体才成立"的测试**。

### 8.5 另一个测试缺陷：静默跳过

原先的字体样本用 `fc-match`（Linux 工具）：

```rust
let Some(data) = any_system_font() else { return; };   // Windows 上永远走 return
```

38 个测试依赖它 → 在 Windows 上**全部空跑通过**，看起来是绿的，实际什么都没验。

已改为走平台 provider（`system_or_isolated()` + `resolve/face_data`），
fontconfig 与 DirectWrite 都有效；并补了一条 `platform_provider_supplies_a_sans_fixture`
作为哨兵——样本取不到时它单独报红，避免其余测试用"通过"掩盖空跑。

### 8.6 建议的工作方式

```bash
# 内层循环：Linux 原生（快）
cargo test --workspace --features ui-text-fontique/system

# 提交前：交叉编译 Windows 并在 Wine 下跑测试
export RUSTUP_HOME=~/.rustup-win CARGO_HOME=~/.cargo
export PATH=~/.cargo-win/bin:$PATH
cargo build -p ui-core -p ui-paint -p ui-text -p ui-text-fontique -p ui-platform \
    --features ui-text-fontique/system --target x86_64-pc-windows-gnu --offline
cargo test -p ui-text-fontique --features system \
    --target x86_64-pc-windows-gnu --offline -- --test-threads=1
```

真机 Windows 留给：字体回退数据、DirectComposition/D3D12 呈现、HDR、
以及复核上面第 4 条并发崩溃。

### 8.7 工具链清理

全部在用户目录，一条命令可移除：

```bash
rm -rf ~/.rustup-win ~/.cargo-win
```
