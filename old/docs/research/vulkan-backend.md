# Vulkan 后端：在 GL 之外再加一条渲染路径

> 给这个 2D UI 渲染器（SDF 圆角、字形图集、每个玻璃节点一次全帧背景趟、离屏效果暂存）加 Vulkan 要做什么、值不值。
> 结论：**现在不做**；将来必须做时选 `ash`，保留 gbm 分配 + dma-buf 导入，**不建 Vulkan swapchain**。
> "我们这边"的事实来自源码与本机实测；外部结论附 URL；无法核实的标 `[未证实]`。

---

## 1. 渲染器今天向后端要的，只有这些

`ui-paint-gpu` 有 **11 个 program、16 段 GLSL、54 条 `uniform`、105 条 `in`/`out`、约 208 处 `gl.*`**（实测）。

| 后端原语 | GL 调用点 | 用途 |
|---|---|---|
| 编译/链接 program，查 uniform | `renderer.rs:2165` `compile_program`、`1745` `Program::new` | 11 个 program；GLES 头在 `2171` |
| 纹理 + FBO，按需 resize，清空 | `effect_pass.rs:48` `ensure_target`；`renderer.rs:620`、`788`、`872` | frame / layer / 效果乒乓 |
| 绑定目标 + viewport | `renderer.rs:638`、`1021`、`1027`、`1180`；`target.rs:80` | 目标切换 |
| scissor | `renderer.rs:2126` `set_scissor`、`2153` `device_scissor` | 矩形裁剪；圆角由 shader |
| 预乘混合 / 关闭 | `renderer.rs:646`、`1187`；`effect_pass.rs:289` 关、`346` 开 | `ONE, ONE_MINUS_SRC_ALPHA` |
| 程序 + VAO + 实例 VBO + draw | `renderer.rs:1419-1437`、`1455-1492`、`1558-1577`、`1592-1606` | 每批一次 `draw_arrays_instanced(0,6,n)` |
| uniform 更新 | `renderer.rs:2081` `set_clip_uniforms`、`1230-1325`；`effect_pass.rs:133-152` | 裁剪、材质、效果核 |
| 纹理上传 / 图集 / 缓存 | `renderer.rs:1951` `upload_atlas`、`2042` `upload_image`、`1501` `sync_images` | R8 覆盖率、RGBA8 图集与图像 |
| 全帧趟（src → dst） | `effect_pass.rs:109` `box_pass`、`169` `effect_pass`；`renderer.rs:913-948` | 模糊 / 颜色矩阵 / PQ |
| 层合成 | `renderer.rs:1035-1086` | 层纹理 × opacity × 圆角覆盖 |
| blit 到 dma-buf | `renderer.rs:894-969` `blit_frame` | 无 HDR 时 `gl.blit_framebuffer` |
| 读回 / `finish` | `renderer.rs:1628`、`1677`、`1654` | 探针与呈现同步 |
| dma-buf → 纹理 + FBO | `context.rs:164`、`238`；`target.rs:34` | 零拷贝目标 |

最小 trait（批次遍历、层栈、背景顺序、效果链留在共享层）：

```rust
trait Painter {
    type Program; type Target; type Texture;
    fn program(&mut self, vs: &str, fs: &str, attrs: &[(&str, u32)], uniforms: &[&str]) -> Program;
    fn target(&mut self, w: u32, h: u32, fmt: TargetFormat) -> Target; // 建/扩容/清空
    fn bind(&mut self, t: &Target);
    fn scissor(&mut self, r: Option<[i32; 4]>);
    fn blend(&mut self, b: Blend);                 // Premultiplied | Replace
    fn use_program(&mut self, p: &Program);
    fn uniform(&mut self, p: &Program, u: &UniformBlock); // 一份共享 layout
    fn draw_instances(&mut self, p: &Program, data: &[f32], n: u32);
    fn upload(&mut self, t: &mut Texture, w: u32, h: u32, f: TexFormat, d: &[u8]);
    fn full_frame(&mut self, p: &Program, src: &Texture, dst: &Target, s: [i32; 4]);
    fn read(&mut self, t: &Target) -> Vec<u8>;
    fn finish(&mut self);
}
```

**第一处必须先改的**：`GpuSurface` 的 `bind(&self, gl: &glow::Context)`（`target.rs:14-17`）是 GL 形状的，Vulkan 目标实现不了。`docs/platform-boundary.md` §16.5 说"接 Vulkan 时实现同样的 `GpuSurface`"——**这句不成立**，trait 本身要先摘掉 `glow`。

---

## 2. 映射到 Vulkan

| 原语 | Vulkan | 备注 |
|---|---|---|
| FBO + 纹理 | `VkImage` + `VkImageView` | 无 `VkFramebuffer` |
| 渲染趟 | **dynamic rendering**（`vkCmdBeginRendering`） | 1.3 核心；本机两卡都是 1.4，无需 `VkRenderPass` |
| scissor / viewport | 动态状态 | 不产生 pipeline 变体 |
| 预乘混合 | `VkPipelineColorBlendAttachmentState` | 每 program 固定，烘进 pipeline |
| 实例数据 | `VkVertexInputBindingDescription`，`rate=INSTANCE`，`divisor=1` | `push_rect`/`push_glyph`/… 不改 |
| uniform | push constants；不够再 UBO | 两卡 `maxPushConstantsSize=256`；`effects` 需 224B（`u_weights[33]`+mat4），1.3 只保证 128B，故留 UBO 退路 |
| 采样器 | combined image sampler + descriptor set | 每帧约 2 张；`VK_KHR_push_descriptor` 两卡都有 |
| 读回 | `vkCmdCopyImageToBuffer` + fence | 需 `TRANSFER_SRC` 布局 |
| 呈现 | **无 swapchain**：gbm bo 导入成 `VkImage`，fd 仍交给 `zwp_linux_dmabuf_v1` | 见下 |

**呈现路径**：`EglContext::new` 用的是 `EGL_PLATFORM_GBM_KHR`，**没有 `wl_surface`**（`context.rs:44-47`）；缓冲由 gbm 分配（`target.rs:48-60`），`gpu_window.rs:260-270` 复制 fd 后走 `present_dmabuf`。Vulkan 侧对应物是 `VkImportMemoryFdInfoKHR`（`DMA_BUF`）+ `VkImageDrmFormatModifierExplicitCreateInfoEXT`，**不是 `VK_KHR_wayland_surface`**。`vkCreateWaylandSurfaceKHR` + `vkAcquireNextImageKHR` + `vkQueuePresentKHR` 只在改用 swapchain 时才需要，那会把缓冲所有权、modifier 协商与现有双缓冲（`gpu_window.rs` 的 `targets/next/last`）全部推翻，且无收益。依据：[VK_EXT_external_memory_dma_buf](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_external_memory_dma_buf.html)、[VK_EXT_image_drm_format_modifier](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_image_drm_format_modifier.html)、[VkImageDrmFormatModifierExplicitCreateInfoEXT](https://docs.vulkan.org/refpages/latest/refpages/source/VkImageDrmFormatModifierExplicitCreateInfoEXT.html)。

**本机实测**（`vulkaninfo`）：两卡都有 `VK_EXT_external_memory_dma_buf`、`VK_EXT_image_drm_format_modifier` rev 2、`VK_KHR_external_memory_fd`、`VK_KHR_external_fence_fd`、`VK_KHR_synchronization2`、`VK_KHR_dynamic_rendering`、`VK_KHR_push_descriptor`、`VK_EXT_physical_device_drm`。DRM 节点 NVIDIA 226:129、Intel 226:128；compositor 的 main device 是 `0xE281`=226:129，且有 `wp_linux_drm_syncobj_manager_v1` v1（`wayland-info`）。选卡应直接匹配 `VK_EXT_physical_device_drm` 的 render 节点，而不是复用 `candidate_nodes()` 的猜测。`[未证实]`：NVIDIA 的 tiled modifier（`gpu-backend.md` §4.3 实测 `0x300000000e08012`）可作 `COLOR_ATTACHMENT`——扩展存在，但按 format+modifier 的 `vkGetPhysicalDeviceImageFormatProperties2` 查询尚未跑过。

---

## 3. 真正难的部分

### 3.1 同步：帧纹理被读、再被原地写

`apply_backdrop`（`renderer.rs:1103`）先让 `effect_pass::run` 采样 `frame_tex`（`1128`），再由 `composite_backdrop` 写回 `frame_fbo` 并采样滤波结果（`1180`、`1220`）。GL 靠隐式顺序；Vulkan 必须在每次转向处插 image barrier：`COLOR_ATTACHMENT_OUTPUT/COLOR_ATTACHMENT_WRITE → FRAGMENT_SHADER/SHADER_SAMPLED_READ`，同时 `COLOR_ATTACHMENT_OPTIMAL ↔ SHADER_READ_ONLY_OPTIMAL`。一个玻璃节点 = 6 趟 box（`BOX_PASSES=3`，`effect_pass.rs:304` 的 `boxes*2`）+ 1 次合成，即 **7 次绘制、~7 次 barrier、2 次帧布局转换**；N 个节点就是 N 倍。首帧转换可用 `oldLayout=UNDEFINED` 丢弃旧内容（见 [layout transitions](https://docs.vulkan.org/samples/latest/samples/performance/layout_transitions/README.html)）。

**好消息**：当前设计**从不读写同一张图**——`run` 在两个中间目标间交替（`effect_pass.rs:312`），最后写 `output`，合成时 `filtered` 与 frame 是两张图。因此**不需要** `VK_EXT_attachment_feedback_loop_layout`（只在"同一 subresource 同时采样+写入"时需要，见[提案](https://docs.vulkan.org/features/latest/features/proposals/VK_EXT_attachment_feedback_loop_layout.html)）。这应立成不变量，否则以后"顺手省一张暂存图"就会引入互锁。

**工具风险**：本机 `explicit_layer.d` 里**没有** `VK_LAYER_KHRONOS_validation`（只有 Mesa/Intel 几个层）。barrier 错误是这类移植最常见的 bug，而能抓它的 synchronization validation 默认不可用；Stage 3 之前必须先装，否则等于闭眼调同步。

### 3.2 pipeline 按格式缓存

GL 的 `Program` 与目标格式无关；Vulkan pipeline 必须匹配颜色附件格式（dynamic rendering 下经 `VkPipelineRenderingCreateInfo`）。内部目标有 RGBA8 / RGBA16F（`effect_pass.rs:26-46`），呈现目标是 `B8G8R8A8_UNORM`，缓存键是 `(program, VkFormat)`，运行时最多 11×2；切 HDR 靠 `VkPipelineCache` 避免重编。`Program`（`renderer.rs:88-94`）要拆成 "pipeline 变体表 + layout + descriptor set layout + push constant 布局"。

### 3.3 着色器翻译

源串在运行时拼装（`shader.rs:919-952` 把 `CLIP_GLSL`/`COVERAGE_GLSL`/`SDF_GLSL` 拼进去），GLES 头在 `renderer.rs:2171` 加。Vulkan 需要 SPIR-V，且非不透明 uniform 必须进 block（或 push constant）、`in`/`out` 必须有 `layout(location=...)`、片元输出要有 location——**16 段源、54 条 uniform、105 条 in/out 都要机械改写**。本机有 `glslc`/`glslangValidator`/`spirv-val`，但仓库只有 `ui-i18n/build.rs` 一个构建脚本；要么把拼装移进 `build.rs`（GL 与 Vulkan 共用同一份 `String`），要么提交生成好的 SPIR-V，否则构建不再自足。

### 3.4 坐标与 scissor 的 Y 约定

`device_scissor` 返回**顶边为 0** 的矩形（`renderer.rs:2153-2158`），直接喂给 `gl.scissor`；这能成立是因为投影把 device y=0 映射到 GL 第 0 行（`renderer.rs:2136-2139`）。Vulkan 帧缓冲原点在左上：若沿用同一段 NDC（`shader.rs:435-455`），device y=0 会落到**最后一行**，画面上下颠倒。修法不是"记得翻 scissor"，而是把投影改成 `ndc.y = 1 - 2*y/H`，让 device 行号与图像行号一致；`VkRect2D.offset.y` 于是可直接用 y0。这与 `gpu-backend.md` §7.3 的"不要翻 Y"是同一件事的 Vulkan 表述。

### 3.5 呈现同步

GL 路径交付 dma-buf 前 `glFinish`（`renderer.rs:1654`、`gpu_window.rs:256`），因为跨进程写入没有隐式同步（`gpu-backend.md` §7.1）。Vulkan 等价物是提交后 `vkWaitForFences`。compositor 支持 `wp_linux_drm_syncobj_manager_v1`，理论上可去掉 CPU 停顿，但 Vulkan 导出的是 sync_file（`VK_KHR_external_semaphore_fd` 的 `SYNC_FD`），协议要 DRM syncobj，中间需 `DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE` 一类转换。`[未证实]` 这条链路是否值得；首版照抄 fence 等待。

### 3.6 格式与读回

`GBM_FORMAT_ARGB8888` 对应 `VK_FORMAT_B8G8R8A8_UNORM`；着色器输出预乘色，所以读回字节本身就是预乘 BGRA，正好是 `ui_paint::Pixels` 的约定——比 GL 的 `to_premultiplied_bgra`（`renderer.rs:1665`）更直接。但 `read_pixels` 的**契约**是自顶向下、直通 RGBA（探针都按它写），Vulkan 实现要自己做反预乘与通道交换，否则探针会在"颜色对、alpha 偏暗"上假红。

---

## 4. `ash` / `vulkano` / `wgpu`

| | `ash` | `vulkano` | `wgpu` |
|---|---|---|---|
| 层次 | 1:1 unsafe 绑定 | 安全封装 + 资源追踪 | 自有抽象（WebGPU） |
| dma-buf 导入 | 直接拼三个 `pNext` | 支持 `DMA_BUF`，`ImageCreateInfo` 有 `drm_format_modifiers` | `wgpu-hal::texture_from_dmabuf_fd`，**仅导入**，需 unsafe `as_hal` |
| dma-buf 导出 | 直接 | `[未证实]` | 无公开 API |
| 着色器 | GLSL→SPIR-V 自选工具 | `shader!` 宏，**构建期要求 `glslc` 在 PATH** | 主语言 WGSL；SPIR-V/GLSL 经 naga feature |
| 版本/维护 | 0.38.0+1.3.281，**2024-04 后无发布** | 0.35.2，2025-08 | 30.0.1，2026-08 |
| 体积 | 最小 | 中（含 ash） | 大（core+hal+types+naga，默认带 dx12/metal/gles/vulkan） |

依据：[ash crates.io](https://crates.io/crates/ash)、[vulkano crates.io](https://crates.io/crates/vulkano)、[vulkano CHANGELOG（去掉 shaderc，改要求 glslc）](https://github.com/vulkano-rs/vulkano/blob/master/CHANGELOG.md)、[wgpu crates.io](https://crates.io/crates/wgpu)、[wgpu FeaturesWGPU](https://docs.rs/wgpu/latest/wgpu/struct.FeaturesWGPU.html)、[wgpu-hal `texture_from_dmabuf_fd`](https://docs.rs/wgpu-hal/latest/wgpu_hal/vulkan/struct.Device.html)。

**推荐 `ash`**：需要的 Vulkan 面很小（11 条 pipeline、1 个 descriptor layout、1 段 push constant、几张图），而真正会出错的 barrier / descriptor / 外部内存三件事在任何封装里都仍是手工的；`vulkano` 的安全层不覆盖它们，还多一个构建期 `glslc` 依赖。`wgpu` 的问题是模型不匹配：我们要立即模式的 FBO 往返，它要 encoder/render pass；导入仍走 unsafe hal、没有导出、还多一层 WGSL。**注意**：`docs/gpu-backend.md` §1 说"wgpu 无公开 dma-buf API"已过时——30.0.1 的 `VK_EXT_external_memory_dma_buf` 导入是有的；该节应更新为"能导入、不能导出"，但结论（不用 wgpu）仍成立。

---

## 5. 分阶段计划（每阶段可独立交付）

| 阶段 | 内容 | 验证 / 抓回归的东西 |
|---|---|---|
| 0 | 抽出后端无关层：`push_*`/`device_scissor`/批次遍历/层栈/背景顺序；`GpuSurface` 去 `glow`；uniform 收敛成共享 layout | GL 零变化：`renderer.rs:2208-2440` 的单测（10 条布局/步长 + 4 条行为）随代码搬；14 个 `gpu_*` 例子断言全过，`DUMP_GLASS=1` 的三张 PPM 前后逐字节相同 |
| 1 | 新 `ui-gpu-vk`（进 `PLATFORM_BOUND` 并写理由）：instance/device、按 DRM 节点选卡、RGBA8 `VkImage`、dynamic rendering 清屏、读回 | 新 `vk_smoke` 清绿读回 `[0,255,0,255]`；开验证层；对照 `gpu_smoke` |
| 2 | 实体图元：rects/glyphs/images/shadows 四条 pipeline、实例 VBO、图集/图像上传、descriptor、scissor | `gpu_scene` 四条不变量、`gpu_text_scene`、`gpu_image_scene`、`gpu_shadow_scene`、`gpu_clip_scene`、`gpu_corner_scene` |
| 3 | 层、效果、背景：RGBA16F、乒乓暂存、barrier、合成、玻璃材质 | 同步验证层零报错；`gpu_blur_scene`、`gpu_layer_scene`、`gpu_round_layer_scene`、`gpu_acrylic_scene`、`gpu_backdrop_scene`、`cargo run -p ui-paint-gpu --example gpu_glass_scene`（**11 条** `report`，不是 12）、`gpu_hdr_scene` |
| 4 | 呈现：gbm bo → `VkImage` 导入、shader 全屏趟（不用 `vkCmdBlitImage`，它会作废压缩签名）、fence 等待、交给现有 `present_dmabuf` | 嵌套会话 `window_gpu` 端到端；`gpu-backend.md` §7 的 `wl_buffer.release ≈ 帧数 − 2`；两卡陷阱（Intel 渲染给 NVIDIA 合成 = 黑屏，§7.2） |

**前提**：探针现在硬编码 EGL（`gpu_glass_scene.rs:183-186` 的 `EglContext`/`GpuRenderer`）。不先把例子参数化成 `--backend=gl|vk`，Vulkan 就没有测试，只有"看起来对"。这是 Stage 0/2 的一部分，不是收尾。

---

## 6. 值不值得

**收益（对当前目标）**：显式内存/同步/布局控制，但 `VK_EXT_image_drm_format_modifier` 与 EGL 的 modifier 导入能力对等，不是新能力；理论上更好的多线程录制，但 `render(&mut self)` 单线程且背景趟依赖"本帧已画内容"，真要并行得先拆渲染器，Vulkan 不会自动带来；GL 驱动差异——`gpu-backend.md` §5.3 实测 Intel i915 与 NVIDIA 的四条不变量**完全一致**（0.0000% / 0.0000% / 0.3558% / 0 px），本机没有需要 Vulkan 救场的差异，AMD `[未证实]`；验证层能抓同步 bug——但本机默认**没装**，起步是负收益。

**代价**：第二套完整渲染器（11 program × 最多 3 种附件格式、~208 个 GL 调用点的等价物、16 段着色器改写与构建期 SPIR-V）；测试面翻倍，且探针必须先参数化；barrier/descriptor/pipeline 生命周期是全新 bug 类型，默认没有同步验证器；新增一个 `PLATFORM_BOUND` crate 与边界理由（`ui-platform/tests/platform_boundary.rs`）。

**结论**：**现在不做**，不是"永远不做"。GL 路径在 Linux/Wayland/NVIDIA 上已闭环且经双卡验证，Vulkan 的边际收益在本机未被任何测量支持。触发重评的条件应是具体的，而不是"Vulkan 更现代"：① 必须支持一个 EGL/GBM 零拷贝不可用或不可靠的目标环境；② 需要 GL 给不出的能力（可证明的显式同步收益、HDR 呈现/色彩管理、多 GPU/多队列）；③ 在必须支持的硬件上测到 GL 的实际缺陷。若目标其实是 Windows/macOS，Vulkan 是错误答案（应走 D3D12/Metal 或 `wgpu`），这恰好抽掉了"上 Vulkan 为了跨平台"的主要论据。真到那一天：`ash` + gbm 导入 + 无 swapchain，先做 §5 的 Stage 0，再按阶段推进。
