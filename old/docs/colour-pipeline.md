# 色彩管线：CPU/GPU × HDR/SDR 的四种情况

在实现前先把四种组合分别摊开。它们不是「同一管线换个格式」，而是四条数据流，
共享的只有布局与绘制列表。

## 1. 现状（SDR，两条后端都跑通）

| 环节 | CPU 路径 | GPU 路径 |
|---|---|---|
| 像素类型 | `ui_paint::Pixels`，预乘 BGRA8，sRGB 编码 | dma-buf ARGB8888，GL 纹理 RGBA8 |
| 混合域 | **编码域**（sRGB 值直接线性插值） | **编码域**（`ONE, ONE_MINUS_SRC_ALPHA`） |
| 呈现 | `wl_shm` ARGB8888 | `zwp_linux_dmabuf_v1` `create_immed` |
| 读回 | mmap 直接读 | `glReadPixels`（直通 RGBA）→ `to_premultiplied_bgra` |

**已知偏差**：在编码域混合在物理上不成立（sRGB 是分段幂函数），但在 8-bit SDR 上
误差小于一个 LSB 量级，业界普遍接受。这是有意的取舍，不是疏漏。

## 2. 四种组合

### 2.1 CPU + SDR —— 已实现，保持不变

唯一需要改的是把「编码域混合」这个事实**写进类型或注释**，避免以后有人在
HDR 路径上照抄它。

### 2.2 GPU + SDR —— 已实现，保持不变

同上。

### 2.3 GPU + HDR

| 环节 | 需要什么 |
|---|---|
| 缓冲格式 | `XRGB2101010`（≤1000 nit）或 `ABGR16161616F`（>1000 nit） |
| 混合域 | **必须线性**：PQ 编码域里做 alpha 混合会产生可见的色相偏移 |
| 中间纹理 | 效果链的中间结果也要 f16，否则每趟重新量化回 PQ 会累积色带 |
| 色彩元数据 | 必须通过 `wp_color_management_v1` 告诉 compositor 这是 PQ/BT2020，否则被当作 sRGB 解释，画面发灰 |
| 回退 | compositor 不支持时**色调映射**到 SDR，而不是直接换成 8-bit |

### 2.4 CPU + HDR —— 这是最麻烦的一条

`wl_shm` 的格式集合由 compositor 声明，实际可用的是 `ARGB8888` / `XRGB8888`，
**没有 10-bit 或 f16**。所以 CPU + HDR 只有三条路：

| 路线 | 做法 | 代价 |
|---|---|---|
| A. CPU 写 dma-buf | CPU 光栅器 mmap 一块 10-bit/f16 bo 直接写 | 需要第二种像素类型；`Pixels` 不再是 BGRA8 |
| B. CPU 算、GPU 转 | CPU 出 f16 到内存，GPU 上传再转格式 | 多一次全屏传输，违背 CPU 路径「零依赖 GPU」的定位 |
| C. CPU 只做 SDR | HDR 内容在 CPU 后端降级为色调映射后的 SDR | 语义一致，但「CPU 后端不支持 HDR」成为公开限制 |

**建议选 C**，理由：CPU 后端的存在意义是「没有 GPU 也能出图」的诊断与降级路径，
把它扩展成 HDR 会让 `Pixels` 抽象复杂一倍，而收益是「在没有 GPU 的机器上看 HDR」——
这个场景本身不成立（HDR 显示几乎必然有 GPU）。

若将来确实需要 A，前提是先把 `Pixels` 抽象成 `PixelFormat` + 泛型缓冲，
那是一次独立的重构，不该和 HDR 一起做。

## 3. 四个必须现在决定的问题

### 3.1 混合域是否要在线性空间？

- SDR：保持编码域（现状，误差可接受）
- HDR：**必须线性**

结论：混合域由**输出描述**决定，不是全局常量。`ui-paint` 与 GPU 着色器都要能切换。

### 3.2 预乘在 PQ 编码域里是错的

预乘的前提是「乘 alpha 与加法在同一向量空间」。PQ 编码域不是线性空间，因此
「PQ 值 × alpha」没有物理意义。HDR 路径必须：

1. 解码到线性
2. 预乘
3. 混合
4. 编码回目标传输函数

而 SDR 路径现在的「预乘 BGRA8 + 编码域混合」是自洽的近似。

**这条决定了 HDR 不能复用 `Pixels`**：`Pixels` 的语义是「已预乘的编码值」，
在 HDR 下这个语义不成立。

### 3.3 效果链（模糊）的中间精度

`effect-cache-plan.md` 记过「RGBA8 双重量化破坏位相同」。HDR 下这不是性能问题而是
**正确性**问题：每趟重编码到 PQ 会累积色带。

结论：HDR 路径的效果中间纹理必须是 f16；SDR 路径保持 RGBA8（现状）。

### 3.4 格式协商与回退链

```
输出描述 → BufferFormat::choose → dma-buf fourcc + modifier → 询问 compositor 是否接受
                                                              ↓ 拒绝
                                                        下一档（f16 → 10-bit → 8-bit）
                                                              ↓ 全部拒绝
                                                        色调映射到 SDR
```

`BufferFormat::choose` 只回答「理论上够不够」，**不回答「这个 compositor 收不收」**。
后者要靠 `zwp_linux_dmabuf_v1` 的 feedback 或直接尝试导入。两者必须分开，
否则会把「设备能力」和「协议能力」混成一个判断。

## 4. 建议的实施顺序

1. **`BufferFormat` → dma-buf 格式映射**（`ui-gpu-egl`），含 modifier 处理
2. **尝试导入 + 回退链**（`ui-platform-wayland`），把「收不收」变成运行时可回答的问题
3. **混合域切换**（`ui-paint` + 着色器）：先只做「线性混合」这一个开关，用 SDR 验证
   它不破坏现有画面（8-bit 下差别应在 1 LSB 内）
4. **HDR 缓冲 + 色彩元数据**（需要 `wp_color_management_v1`）
5. **色调映射**（HDR 内容 → SDR 输出）
6. CPU + HDR：按 §2.4 的结论**不做**，把限制写进文档

第 3 步是关键的分水岭：它验证「混合域可切换」这个抽象成立。如果 SDR 下线性混合
与编码域混合的差异超出预期，说明前面的判断有问题，必须在接 HDR 之前解决。

## 5. 风险清单

| 风险 | 表现 | 缓解 |
|---|---|---|
| 把「设备能力」与「协议能力」混为一谈 | 选了 10-bit，compositor 不收，画面全黑 | 两步分开，第 2 步专门验证 |
| 预乘语义在 HDR 下失效却被沿用 | 半透明区域色相偏移 | §3.2 写进类型约束 |
| 效果中间纹理仍用 RGBA8 | 模糊区域出现色带 | 中间纹理精度跟输出描述走 |
| 忘设色彩元数据 | 画面发灰（被当 sRGB 解释） | 元数据与格式在同一处设置，不分开 |
| CPU 后端被强行扩展 | `Pixels` 抽象膨胀一倍 | §2.4 明确不做 |

## 6. `Pixels` 如何承载语义（对 §2.4 的修正）

### 6.1 两条正交的轴，不是一个枚举

「语义」实际是两件事，混成一个枚举会把可检查的信息重新变成运行期才知道：

| 轴 | 取值 | 何时确定 |
|---|---|---|
| 存储布局 | `Bgra8` / `Rgba10a2` / `Rgba16F` | 编译期可选，运行期固定 |
| 色彩语义 | 传输函数（sRGB/PQ/线性）+ **是否预乘** | 由输出描述决定，运行期可变 |

组合是真实存在的：`Bgra8` 可以是 sRGB 编码（SDR 呈现），也可以是线性值（线性工作缓冲）；
`Rgba16F` 只能是线性（PQ 值存 f16 是浪费）。一个枚举表达不了这种「布局 × 编码」的矩阵。

### 6.2 预乘应该进类型

预乘/直通混淆已经在本项目里犯过四次（见 `AGENTS.md` §1.5 最后一条），
每次都靠人工推导通道序来发现。把它做成类型状态是唯一能根治的办法：

```rust
pub struct Encoding {
    pub transfer: TransferFunction,
    /// Whether colour channels were multiplied by alpha. Not inferable from bytes.
    /// 颜色通道是否已乘 alpha。无法从字节推断。
    pub premultiplied: bool,
}
```

### 6.3 代价不在枚举，在泛型内循环

只加一个枚举值不会让任何东西变简单。真正的成本是**每个逐像素循环都要泛型化**：

`blend_pixel`、`coverage_rounded_rect`、`composite_over`、`apply_effects`（模糊的横向/纵向趟）。
每一处都要在目标像素类型上正确实现「解码 → 预乘 → 混合 → 编码」。

### 6.4 一个反直觉的约束：覆盖率混合想留在编码域

HDR 路径「必须线性混合」这条只对**颜色合成**成立。字形/抗锯齿的**覆盖率混合**是另一回事：

在编码域混合覆盖率，文字笔画看起来粗细正常；搬到线性域后，同样的覆盖率会得到
更细的笔画（线性域里暗部被压缩，边缘过渡的整体亮度下降）。多数引擎为此保留
编码域的文字混合，或专门做 gamma 校正的文字管线。

所以管线需要**两个混合域**，而不是一个：

| 混合对象 | 域 |
|---|---|
| 颜色（背景、边框、层合成） | 线性（HDR 必须；SDR 可选） |
| 覆盖率（字形、抗锯齿边缘） | 编码域（保持文字视觉重量） |

这条比「CPU+HDR 是冷门场景」更强：即使只做 GPU + HDR，也必须区分这两类混合，
而 `Pixels` 现在对两者一视同仁。

### 6.5 结论

**可以承载语义，但现在不做泛型化。** 顺序：

1. 先引入 `Encoding { transfer, premultiplied }` 作为**值**，挂到渲染器与输出描述上，
   `Pixels` 仍是 BGRA8 —— 这一步零风险，且能立刻消除「预乘/直通」这一类错误
2. 等 HDR 真正落地时，再把 `Pixels` 泛型化为 `Pixels<L: Layout>`，并逐个内循环迁移
3. 覆盖率混合域单独作为一个开关，与颜色混合域分开验证

**为什么不现在做泛型化**：泛型化会重写全部逐像素循环，而当前没有一条能失败的测试
能证明「泛型化后的 SDR 结果与现在逐字节相同」。先做值语义，等 HDR 带来真实的
第二实现时，泛型化才有可对照的第二个数据点。

## 7. `Encoding` 用于中间产物

### 7.1 该挂在哪

中间产物是域错配的主要发生地：没有「用户能看见的画面」来暴露它，错误表现为
颜色偏一点、渐变有色带、半透明区域色相不对——都像「审美问题」而不是 bug。

`Encoding` 应当挂在每一个**会被两个以上的环节触碰**的缓冲上：

| 位置 | 为什么 |
|---|---|
| `Pixels`（CPU） | 光栅器、效果、合成、呈现都读写它 |
| 层纹理 / 效果 scratch（GPU） | 效果趟的 src 与 dst 是不同对象，最容易错配 |
| 效果 scratch（`Scratch`） | 仅作乒乓暂存，帧间不含有意义内容；域变化仍要重新分配 |
| 呈现目标（dma-buf / shm） | 交给 compositor 前的最后一道语义声明 |

### 7.2 检查点只有五处

不必在每个函数上都传，只需在**两个缓冲相遇**的地方检查：

| 边界 | 检查内容 |
|---|---|
| 效果趟 src → dst | 同域，或显式转换 |
| 效果链输出 → 合成 | 同域，或显式转换 |
| 合成 → 呈现 | 与输出描述一致 |
| 读回 → 比较 | 与期望的域一致（否则测试会「通过」） |
| 阶段暂存复用 | 域的哈希参与分配决策（现无跨帧内容，故只是分配而非失效） |

五处都是 `debug_assert_eq!` 级别，release 下零成本。**但遇到不一致时必须是真转换，
不能静默重解释**——静默重解释就是现在这类 bug 的来源。

### 7.3 域必须跟随每一次分配（与现有代码直接相关）

跨帧效果缓存已撤除（`docs/effect-cache-plan.md` §11），因此不再有"缓存键缺域"这个问题：
`Scratch` 里只有乒乓纹理，每次 `run` 都从当帧场景重算，纹理内容不跨帧生效。
仍然要保证的是**分配**时用当前域：`ensure_target(..., format)` 在尺寸或格式变化时重新
分配存储。输出描述从 SDR 切到 HDR 而格式没跟上，会让滤波在半精度与 8 位之间错配，
错误同样只出现在切换的那一帧。

这和我们之前踩的「合成范围与清空范围不一致」是同一类错误：**缓存的有效性条件少列了一项**。

### 7.4 「同布局」不等于「同语义」

两个 `Rgba16F` 缓冲，一个装线性预乘值，一个装 PQ 编码值——布局相同，语义完全不同。
所以检查必须针对 `Encoding`，不能针对 `Layout`：

```rust
pub struct Encoding {
    pub transfer: TransferFunction,
    pub premultiplied: bool,
}
```

`Layout` 决定怎么读字节，`Encoding` 决定字节是什么意思。两者都对了才算对。

### 7.5 工作域归谁所有

中间产物的域不是各自决定的，而是**跟随输出描述**：

```
OutputDescription → 工作域（transfer + premultiplied）→ 所有中间产物分配时打标
```

所以「当前工作域」是渲染器的一个字段，而不是每个缓冲自己传参。切换输出描述时，
渲染器改一次字段，所有后续分配与缓存键自动跟随——这也是 §7.3 里缓存必须失效的原因。

### 7.6 结论

`Encoding` 用于中间产物是**该做且成本低**的：五个 `debug_assert`、一个渲染器字段、
一个缓存键字段。它把「预乘/直通/编码域」这类已经犯过四次错误的问题，从人工纪律
变成可执行的检查。

泛型化（§6.5 第 2 步）仍然留到 HDR 真正落地时再做——那时 `Encoding` 已经在位，
泛型化只需要把值改成类型参数，而不需要同时发明语义。

## 8. 实测修正：线性混合不是「1 LSB 的差别」

§4 第 3 步曾写「SDR 下线性混合与编码域混合的差异应在 1 LSB 量级」。**实测是错的。**

`ui-paint` 的 `blend_domain_tests` 用「50% 白叠在不透明黑上」测出：

| 做法 | 结果 |
|---|---|
| 编码域混合（现状） | **128** |
| 解码 → 混合 → 编码 | **187** |

差 **59 级**，不是 1 级。

### 8.1 为什么差这么多

`0.5` 的 sRGB 编码值对应 `0.214` 线性。在编码域里「50% 白」就是 128；在线性域里，
50% 的 alpha 意味着「取 50% 的光」，编码回 sRGB 后是 187。两者都是自洽的，
差别在于**「50% 混合」到底指什么**。

### 8.2 结论：这是外观变更，不是 bug 修复

把 SDR 切到线性混合会让所有半透明区域明显变亮。因此：

| 路径 | 混合域 | 理由 |
|---|---|---|
| SDR | **编码域（不变）** | 与既有截图、设计预期一致；切换等于重新设计视觉 |
| HDR | 线性 | 缓冲区本来就存线性值，**合成时无需解码** |

关键推论：**「切换混合域」不是一个在合成阶段生效的开关**。线性路径的缓冲区里存的
就是线性值，光栅化时写入线性、合成时线性相加、呈现前编码一次。如果在合成时才解码，
面对预乘存储必须先解预乘——那条路能跑通但不是 HDR 的做法。

所以 `composite_over_in` 保留为**诊断工具**（能量化两个域的差异），而不是 SDR 的运行时
开关。SDR 继续走 `composite_over` 的整数路径，逐字节不变。

### 8.3 对 HDR 实施的简化

原先担心的「同一套代码要支持两个混合域」并不成立：

- SDR：编码域存储 + 编码域混合（不变）
- HDR：线性域存储 + 线性混合

两条路径的**缓冲区内容**不同，因此混合代码也不同——不是同一个函数里的分支，
而是两个数据流。这与 §2 的结论一致：四种情况是四条数据流，不是一条管线换格式。

## 9. 实测：本机 HDR 格式可用性

`examples/hdr_probe.rs` 分别回答三个独立问题：格式是否被通告、分配器返回什么 modifier、
能否作为纹理导入。三者失败原因不同，不能合并成一个布尔值。

### 9.1 结果（Hyprland 0.56，`zwp_linux_dmabuf_v1` v3）

| GPU | 8-bit | 10-bit (AB30) | f16 (AB48) | modifier |
|---|---|---|---|---|
| NVIDIA RTX 5060 Ti（驱动显示） | explicit | **explicit** | **explicit** | `0x300000000e08013`（平铺） |
| Intel ARL iGPU | implicit-only | implicit-only | implicit-only | `0x0`（线性） |

**结论：驱动 DP-6 的那块 GPU（NVIDIA）对 10-bit 与 f16 都是 explicit**，即
`(fourcc, modifier)` 组合被 compositor 明确通告，可原样发送。这台机器具备 HDR 呈现条件。

### 9.2 探针自身的两个缺陷（已修）

1. **用 modifier 0 查 `support()`**：那只回答「格式被通告」，没回答「这个组合被接受」。
   NVIDIA 实际返回平铺 modifier，必须用分配后拿到的值再查一次。
2. **把 `implicit-only` 当成不可用**：按 `dmabuf.rs` 的约定，它表示「以
   `MODIFIER_INVALID` 发送」，是合法路径。当成不可用会误报 Intel 完全不支持 HDR。

两处都属于「探针插错位置」——量到的不是想量的事。

### 9.3 尚未验证

- **实际呈现**：导入成功 + 能画进去 ≠ compositor 会正确显示。需要一次真实 commit 并用
  截图确认（10-bit 缓冲被当 8-bit 解释会表现为色带或偏色）。
- **色彩元数据**：即使格式对了，不通过 `wp_color_management_v1` 声明 PQ/BT2020，
  compositor 会按 sRGB 解释，画面发灰。
- **Intel 的 implicit-only 路径**：格式通告了，但 `MODIFIER_INVALID` 的实际行为未测。

### 9.4 已核实：10-bit 呈现通路正确

`window_gpu` 支持 `CRABSNAP_FORMAT=10bit`，与 8-bit 在同一场景上逐像素对比：

| 指标 | 结果 |
|---|---|
| 区域像素数 | 277380 |
| 通道差 > 2 的像素 | **0** |
| 最大通道差 | **1** |

**这条证明了什么**：AB30 四cc + 平铺 modifier 的缓冲被 compositor 正确解释为 10-bit，
而不是当成别的格式。如果格式解释错了，会看到明显的色带或偏色，而不是差 1。

**这条没有证明什么**：画面相同是**预期**的——渲染器写入的仍是 8-bit sRGB 值，
只是存进 10-bit 容器。所以真正的 HDR 还缺两件事：

1. **渲染进宽域**：光栅化时就写线性值，而不是把 8-bit 值放进大容器
2. **色彩元数据**：声明 PQ/BT2020，否则 compositor 按 sRGB 解释

第 1 件是数据流变更（见 §8.3），第 2 件需要 `wp_color_management_v1`。
格式通路已经就位，这两件都可以在它之上推进。

### 9.5 使用方式

```bash
# 默认 8-bit
cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts

# 10-bit（ABGR2101010）
CRABSNAP_FORMAT=10bit cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts

# 16-bit 浮点（ABGR16161616F）
CRABSNAP_FORMAT=f16 cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts
```

## 10. 实测：本机 compositor 的能力

`examples/globals_probe.rs` 枚举全部全局对象（共 72 个）。

### 10.0 先前结论是错的：探针用了错的接口名

第一次探测报告「色彩管理与背景模糊都不可用」。**这是假阴性**——查的是 XML 文件名，
不是全局对象名：

| 查的名字（错） | 实际接口名 |
|---|---|
| `wp_color_management_v1` | **`wp_color_manager_v1` v3** |
| `ext_background_effect_v1` | **`ext_background_effect_manager_v1` v1** |

两者都可用。这是本会话第三次「量到的不是想量的事」——前两次分别是
用 modifier 0 查格式支持、把 `implicit-only` 当成不可用。

### 10.1 结果

| 主题 | 可用 | 协议 |
|---|---|---|
| **色彩管理** | ✅ | `wp_color_manager_v1` **v3**，另有 `hyprland_ctm_control_manager_v1` v2 |
| **HDR 元数据** | ✅ | 经 `wp_color_manager_v1` 的 image description |
| **surface 背景模糊** | ✅ | `ext_background_effect_manager_v1` v1 |
| 文本输入 | ✅ | `zwp_text_input_manager_v3` |
| dma-buf | ✅ | `zwp_linux_dmabuf_v1` v5（我们绑定 3） |
| 分数缩放 | ✅ | `wp_fractional_scale_manager_v1` |
| 同步 | ✅ | `wp_linux_drm_syncobj_manager_v1`、`wp_fifo_manager_v1` |
| 呈现时机 | ✅ | `wp_presentation` v2 |

环境：Hyprland 0.56，wayland-1，DP-6。

### 10.2 对计划的影响：无阻塞

原以为第 4、5 步（色彩元数据、色调映射）被阻塞，**不成立**。`wp_color_manager_v1` v3
可用，HDR 声明路径是通的。`wayland-protocols` 0.32 已带 `staging` feature，
`color-management-v1.xml` 就在其中，不需要自己 vendor。

背景模糊也可用（`ext_background_effect_manager_v1`），对应 §17 的能力抽象。

### 10.3 结论

按 §4 的顺序继续：

1. ~~`BufferFormat` → fourcc~~ ✅ 完成并验证（§9.4）
2. ~~尝试导入 + 回退链~~ ✅ 探针可回答（`FormatSupport`）
3. 混合域切换 —— §8 已修正为「不是运行时开关」
4. **色彩元数据**：接 `wp_color_manager_v1` v3
5. 色调映射
6. CPU + HDR：不做

下一步做第 4 步：绑定 `wp_color_manager_v1`，为 surface 创建 PQ/BT2020 的 image description。

## 11. 色彩元数据已接通并验证生效

### 11.1 实现

| 位置 | 内容 |
|---|---|
| `session.rs` | `Capabilities.color_manager: Option<WpColorManagerV1>`，绑定 `1..=3` |
| `colour.rs` | 三个 `Dispatch` 实现（manager / surface / image description） |
| `window.rs` | `declare_pq_bt2020(max_lum)`、`has_color_management()` |

流程：`create_parametric_creator` → `set_tf_named(St2084Pq)` →
`set_primaries_named(Bt2020)` → `set_luminances(0, max, 200)` → `create` →
**roundtrip** → `get_surface` → `set_image_description(desc, Perceptual)`。

`roundtrip` 不能省：image description 只有在 `done` 事件之后才有效，提前挂上会被
compositor 静默忽略——`Failed` 事件是唯一的反馈，已存进 `WindowState.colour_failed`。

### 11.2 验证：声明生效会改变画面

同一场景、同样 10-bit 缓冲，只切换 `CRABSNAP_HDR`：

| 指标 | 结果 |
|---|---|
| 采样点 | 69345 |
| 差异 > 4 的占比 | **98.1%** |
| 中位通道差 | 20 |
| 最大通道差 | 203 |
| 样本像素 | `(60,130,220)` → `(0,111,255)` |

**这条证明了什么**：compositor 确实按 PQ/BT2020 解释该 surface，元数据通路打通。

**这条没有证明什么**：画面是**错的**——渲染器写入的仍是 sRGB 值，被当作 PQ 解释。
所以下一步必须让渲染端产出 PQ 编码值，元数据才有意义。

### 11.3 用法

```bash
CRABSNAP_HDR=1 CRABSNAP_FORMAT=10bit \
  cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts
```

输出 `colour management: available=true declaration=accepted` 表示声明被接受。

### 11.4 剩下的一半

现在缺的是**内容**而不是声明：

1. 渲染器把线性工作域的值编码为 PQ（`TransferFunction::Pq::encode`）再写入缓冲
2. 颜色混合在线性域，覆盖率混合留在编码域（§6.4）
3. 效果中间纹理用 f16（§3.3）

第 1 条做完，`CRABSNAP_HDR=1` 的画面才会变成**正确**的 HDR 而不是偏色。

## 12. 数值证据：为什么是「PQ + 10-bit」而不是「8-bit 换成 10-bit」

截图不能作为 HDR 的判据（compositor 对 HDR 内容的截图回读不一定正确），
所以验证落在纯数值上。`examples/quantisation_probe.rs` 统计一段暗部渐变
（线性 0.0002 → 0.0015，64 级）在量化后还剩多少个不同编码：

| 格式 | 不同编码数（共 64 级） |
|---|---|
| sRGB 8-bit | **5** |
| sRGB 10-bit | 17 |
| **PQ 10-bit** | **64** |
| PQ 12-bit | 64 |

### 12.1 结论

**瓶颈是传输函数，不是位深。** 把 8-bit 换成 10-bit 只能从 5 级改善到 17 级，
仍然严重色带；换 PQ 后 10-bit 就能完整分辨 64 级。

这解释了 §1 里 `BufferFormat::choose` 的判据为什么写成「传输函数是不是 HDR」而不是
「峰值亮度多少」——两者是同一个结论的两种表述。

### 12.2 对应的测试

`ui-core::colour` 里有两条：

- `ten_bit_pq_separates_dark_steps_that_eight_bit_srgb_collapses`
  —— 钉住 8-bit sRGB 在暗部塌缩、10-bit PQ 能分辨（比例 > 2 倍）
- `a_pq_round_trip_through_ten_bits_stays_within_one_step`
  —— 编码→量化→解码的误差不超过一个编码

### 12.3 视觉判定交给使用者

HDR 的最终正确性（画面是否自然、亮度是否合适）由使用者在真实显示器上判定。
本项目只保证：数值链路正确、元数据声明被接受、格式被 compositor 正确解释。

## 13. 渲染端 PQ 编码：位置与公式

### 13.1 协议不管 alpha

`color-management-v1.xml` 全文没有 `premultiplied` / `alpha` 字样——它只描述色彩编码，
预乘语义由 buffer 约定继承（与今天 sRGB8 的做法相同）。

所以 HDR 缓冲沿用同一约定：**颜色通道是「已乘 alpha 的编码值」**。这对 PQ 在数学上
同样是个近似（PQ 非线性，预乘本无意义），但与 compositor 实际做的混合算术一致，
与 SDR 的处理方式也一致。

### 13.2 正确的变换

PQ 是绝对亮度：编码 `c` 对应 `10000 × pq_decode(c)` nit。SDR 白约定 203 nit。
要把一个 SDR 编码值 `s` 变成「视觉上相同」的 PQ 值：

```
L = srgb_decode(s)              // 0..1 线性，1.0 = SDR 白
c = pq_encode(0.0203 × L)       // 0.0203 = 203 / 10000
out = c × alpha                 // 编码后再乘 alpha，与 compositor 的约定一致
```

`0.0203` 是关键常数：它把「SDR 白」放在 PQ 量程的 2.03% 处，留下 98% 的量程给高光。

### 13.3 变换只能做一次，且位置在输出端

一个容易写错的地方：如果在每个图元着色器里编码，会**重复编码**——图元画进层纹理，
层再合成进帧缓冲，最后 blit 到 dma-buf，同一条链上经过多个着色器。

正确的落点是**唯一的一次输出变换**：

| 位置 | 现状 | HDR |
|---|---|---|
| 图元 / 层合成 | 直接写 sRGB 值 | 不变，仍写 sRGB 值 |
| 帧缓冲 → dma-buf 的 blit | 直接拷贝 | **加一趟全屏编码**：`pq_encode(0.0203 × srgb_decode(c)) × a` |

这样：
- 所有既有绘制代码不动
- 只有一处做色彩变换，容易验证
- 关闭 HDR 时那一趟直接跳过，零成本

### 13.4 实现清单

1. `shader.rs`：新增 `OUTPUT_FRAGMENT`（采样 + 上述公式）
2. `GpuRenderer`：`hdr: bool` 字段 + `set_hdr()`；一个全屏 scratch 纹理
3. `render` 的 blit 步骤：`hdr` 时先跑编码趟到 scratch，再 blit scratch
4. 数值验证：给一个已知 `s`，编码后应为 `pq_encode(0.0203 × srgb_decode(s))`，
   可在 CPU 侧用 `TransferFunction` 直接算，不需要 GPU

第 4 条是这次唯一的自动化验证手段——截图不可信（compositor 对 HDR 内容的回读不一定正确）。

## 14. 三个亮度值都由 compositor 决定，不是标准常量

`color-management-v1.xml` 的 `set_luminances(min_lum, max_lum, reference_lum)` 原文：

> `reference_lum` specifies the luminance of the reference white as reproduced by the
> targeted display, and reflects the **targeted viewing environment**.

> With `transfer_function.st2084_pq` the given `max_lum` value is **ignored**, and
> `max_lum` is taken as `min_lum + 10000 cd/m²`.

协议给的默认值（sRGB，来自 ICC chardata）：min 0.2、max 80、**reference 80**。

### 14.1 三条修正

| 之前的假设 | 实际 |
|---|---|
| SDR 白 = 203 nit（BT.2408） | 由 compositor / 用户设置决定，**每个 image description 自带** |
| 峰值亮度决定缓冲格式 | PQ 下 `max_lum` **被忽略**，量程固定为 `min_lum + 10000` |
| `0.0203` 是常数 | 应为 `reference_lum / 10000`，且带 `min_lum` 偏移 |

第三条尤其重要：`0.0203` 只在「SDR 白 = 203 nit」时成立。Hyprland / KDE 都允许用户
调 SDR 亮度，那样写死就会整体偏亮或偏暗。

### 14.2 正确的取法

不要自己猜，问 compositor：

| 来源 | 提供 |
|---|---|
| `wp_color_management_output_v1.get_image_description()` | 该输出的**首选**描述（tf / primaries / 三个亮度） |
| `wp_color_management_surface_feedback_v1.get_preferred()` | 该 surface **应该**用的描述 |
| 我们 `set_luminances` 声明的 | 我们内容**自身**的目标（reference 白由我们定，但要与 compositor 的锚定一致） |

协议原文还要求「all content is anchored」：不同 image description 上相同的
`reference_lum` 输入信号应对应相同的实际亮度。所以我们的 `reference_lum` 必须
取自 compositor 的首选描述，不能自选。

### 14.3 对编码公式的修正

```
scale  = reference_lum / 10000        // 不是常数 0.0203
offset = min_lum / 10000
c      = pq_encode(clamp(scale * L + offset, 0, 1))
```

`reference_lum` 从 compositor 的首选描述读；`min_lum` 同样。

### 14.4 API 修正

`WaylandWindow::declare_pq_bt2020(max_lum)` 的 `max_lum` **对 PQ 无效**（协议明确忽略）。
已改为 `declare_pq_bt2020(min_lum, reference_lum)`，并在文档里注明 max 由协议推出。

## 15. 调查记录：为什么读不到 compositor 的首选亮度

### 15.1 第一个真 bug：`ready2` 而不是 `ready`

协议原文（`wp_image_description_v1.ready`）：

> Starting from interface version 2, the `ready2` event is sent **instead of** this event.

我们绑定 v3，所以只匹配 `Ready` 的分支永不触发——表现为「`failed=None` 且无任何事件」。
加入 `Ready2` 后事件正常到达。旧项目的注释也指出了这一点（已用协议原文独立验证）。

### 15.2 第二个问题：`no_information` 是定义好的协议错误

错误码 1 在 `wp_image_description_v1.error` 里是：

```
<entry name="no_information" value="1" summary="get_information not allowed"/>
```

不是版本问题——运行时打印三个对象版本均为 3：

```
manager=3 feedback=3 description=3
```

### 15.3 分流实验：拒绝只发生在「首选」描述上

| 描述来源 | `get_information` 结果 |
|---|---|
| 我们自己用 parametric creator 创建的 | **接受**，无协议错误 |
| `get_preferred()` 返回的 | **协议错误 1 `no_information`**，且会断连 |

所以本机 compositor 的规则是「只允许在客户端创建的描述上索取信息」。

### 15.4 但客户端创建的描述也不回内容

接受请求后，`wp_image_description_info_v1` 的 `luminances` 事件**从未到达**
（两轮往返后仍为 `None`，且无 `failed`）。

结论：**本机 compositor 没有实现 image description 的信息事件**。因此在 Hyprland 0.56 上
`reference_lum` / `min_lum` 无法从协议读出。

### 15.5 处理方式

代码按来源分流（`WindowState.allow_info`），避免对首选描述发请求导致断连：

- 首选描述：不发 `get_information`
- 客户端创建的描述：发，但当前得不到内容

参考白改为**调用方提供**（协议默认 min 0.2 / reference 80），编码公式
`scale = reference_lum / 10000` 与取值来源无关。若将来 compositor 实现了信息事件，
只需打开 `allow_info` 并读取 `preferred_luminances`。

### 15.6 三条可复用的判据

1. 协议里带 `since` 的事件被替代时（`ready` → `ready2`），**必须两个都匹配**
2. 定义好的协议错误（如 `no_information`）意味着「实现选择」而不是「用法错误」——
   先按来源/对象类型分流，再考虑回退
3. 「请求被接受」不等于「数据会来」：请求成功但没有事件，是独立的第三种状态

## 16. 参考旧项目后的三处修正（均已按协议原文核对）

### 16.1 `min_lum` 在协议上放大 10000 倍

`luminances` 事件与 `set_luminances` 请求的 `min_lum` 都是 `cd/m² × 10000`：

```
<arg name="min_lum" type="uint" summary="minimum luminance (cd/m²) * 10000"/>
<arg name="max_lum" type="uint" summary="maximum luminance (cd/m²)"/>
```

`max_lum` 与 `reference_lum` **不缩放**。之前的实现直接把 `min_lum` 当 cd/m² 传，
只在 `min_lum = 0` 时恰好无害。

### 16.2 `set_luminances` 需要 compositor 广播 feature

协议原文：

> This request can be used if the compositor advertises
> `wp_color_manager_v1.feature.set_luminances`. Otherwise this request raises the
> protocol error **unsupported_feature**.

我们从未检查 feature——在本机侥幸没炸，换一个 compositor 会直接断连。现在：

| feature | 值 | 门控的请求 |
|---|---|---|
| `parametric` | 1 | `create_parametric_creator` |
| `set_luminances` | 4 | `set_luminances` |

未广播时跳过对应请求，而不是发出去等协议错误。

### 16.3 旧项目读亮度走的是**输出**路径

`color_proto.rs` 的流程是：

```
get_output(wl_output) → wp_color_management_output_v1
                      → get_image_description()
                      → get_information() + roundtrip
```

**不是** surface 的 `get_preferred()`。这解释了为什么旧项目没撞到 §15.2 的
`no_information`——它问的是输出的描述，我们问的是 surface 的首选描述。

这条是下一步的关键线索：需要在 `WaylandWindow` 里绑定 `wl_output`（当前完全没有），
才能走输出路径拿到真实亮度。

### 16.4 当前状态

| 项 | 状态 |
|---|---|
| `ready2` 事件 | ✅ 已修 |
| 首选描述读信息 | ❌ 本机 `no_information`，已按来源分流避免断连 |
| 客户端创建的描述读信息 | ⚠️ 请求被接受，但无事件 |
| feature 门控 | ✅ 已加 |
| `min_lum` 缩放 | ✅ 已修 |
| 输出路径读亮度 | ❌ 未实现（需绑定 `wl_output`） |

## 17. 解决：输出路径读到了真实亮度

绑定 `wl_output` 后走旧项目那条路（`get_output` → `get_image_description` →
`get_information`），**成功**：

```
output luminances: Some((2000, 80, 80))
reference white: 80 nits (from output)
```

解读：

| 字段 | 原始值 | 实际 |
|---|---|---|
| `min_lum` | 2000 | **0.2 cd/m²**（÷10000，验证了 §16.1 的缩放） |
| `max_lum` | 80 | 80 cd/m² |
| `reference_lum` | 80 | 80 cd/m² |

这三个值正好是协议文档里 sRGB 的默认亮度，说明当前输出处于 SDR 模式。若在 Hyprland
里开启 HDR，这里应变成更大的 `max_lum` 与 `reference_lum`。

### 17.1 两种描述的差别被证实

| 描述来源 | `get_information` | 结果 |
|---|---|---|
| surface 的 `get_preferred()` | 协议错误 `no_information` | ❌ |
| **output 的 `get_image_description()`** | 允许 | ✅ 拿到三个亮度 |

所以 §15 的结论要修正一句：不是「本机没实现信息事件」，而是**本机只允许在输出描述上
查询信息**。surface 首选描述被限制。

### 17.2 三级回退已实现

```rust
pub fn reference_nits(&mut self) -> (u32, &'static str)
// 1. CRABSNAP_REFERENCE_NITS 环境变量
// 2. output_luminances() 的 reference_lum
// 3. 协议默认 80
```

返回的第二个值说明来源，日志里能直接看出走的是哪一级。

### 17.3 编码公式的输入已齐备

```
scale  = reference_lum / 10000        // 现在可读，也可覆盖
offset = min_lum / 10000              // 0.2 / 10000
```

下一步实现渲染端编码时，`scale` 不再需要猜测。

## 18. 编码公式已实现并钉住

```rust
pub fn encode_for_pq_output(linear: f32, reference_nits: f32, min_nits: f32) -> f32 {
    let scale = reference_nits / 10_000.0;
    let offset = min_nits / 10_000.0;
    TransferFunction::Pq.encode((scale * linear + offset).clamp(0.0, 1.0))
}
```

三条测试：

| 测试 | 断言 |
|---|---|
| `sd_white_lands_near_the_middle_of_the_pq_range` | 80 nit 参考白 → 编码约 **0.486**（PQ 量程的 0.8%） |
| `a_brighter_reference_white_shifts_every_code_up` | reference 203 比 80 编得更高 |
| `black_stays_at_the_bottom_of_the_range` | 黑电平为 0 时线性 0 → 编码 0 |

**为什么先做这一步**：着色器是这个函数的转录。数学在 CPU 上钉住后，GPU 侧只需要保证
「输入相同、公式相同」，而这一点可以用一个已知像素对来验证，不需要理解整条管线。

### 18.1 剩下的 GPU 工作

| 项 | 内容 |
|---|---|
| `OUTPUT_FRAGMENT` | 采样 + `encode_for_pq_output` 的 GLSL 转录 |
| `GpuRenderer` | `hdr: Option<(reference, min)>` 与一个全屏 scratch 纹理 |
| `render` | blit 前插一趟编码：帧缓冲 → scratch → 目标 |

关闭时该趟跳过，零成本。

## 19. GPU 输出趟已实现

### 19.1 结构

| 位置 | 内容 |
|---|---|
| `shader.rs` | `OUTPUT_FRAGMENT`：解预乘 → sRGB 解码 → `pq_encode(scale·L + offset)` → 重预乘 |
| `renderer.rs` | `output: Program`、`hdr: Option<[f32;2]>`、`output_scratch` |
| `render` | blit 前插一趟：帧缓冲 → scratch（PQ）→ 目标 |
| `ui-core` | `reference_scale()` / `min_offset()` 与着色器共用同一公式 |

着色器是 `encode_for_pq_output` 的逐行转录，数学在 CPU 侧有 3 条测试钉住。

### 19.2 探针确认这趟确实执行

```
[hdr] output pass: scale=0.008 offset=0.00002
```

`0.008 = 80 / 10000`，`0.00002 = 0.2 / 10000`——两个值都来自 compositor 报的亮度，
不是常量。这条证明「读到的亮度」确实进了着色器。

### 19.3 视觉正确性未验证

截图对比不可靠（compositor 对 HDR 内容的回读不一定正确），所以：

- **已验证**：公式（CPU 单测）、参数来源（输出描述）、这趟执行、无协议错误
- **未验证**：屏幕上看起来是否正确

后者交由使用者在真实显示器上判定。若画面整体过暗，说明 scale 偏小（参考白读低了）；
若高光过曝，说明 scale 偏大。可用 `CRABSNAP_REFERENCE_NITS` 覆盖后对比。

### 19.4 用法

```bash
CRABSNAP_HDR=1 CRABSNAP_FORMAT=10bit \
  cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts

# 覆盖参考白（三级回退的第一级）
CRABSNAP_HDR=1 CRABSNAP_FORMAT=10bit CRABSNAP_REFERENCE_NITS=203 ...
```

## 20. 静止帧发白：第二条交出路径绕过了输出变换

### 20.1 现象

有事件但画面不变的帧（静止帧）颜色异常发亮发白。

### 20.2 原因

`render` 里有两条「把帧交给目标」的路径：

| 路径 | 何时走 |
|---|---|
| 提前返回（`damage.is_none()`） | 静止帧 |
| 正常结束 | 有重绘 |

HDR 输出变换只加在了**正常结束**那条上。静止帧直接把帧缓冲 blit 出去，缓冲里是
sRGB 值，而 surface 已被声明为 PQ——compositor 按 PQ 解码，画面就发白发亮。

### 20.3 修法

抽出唯一的一个交出点：

```rust
fn blit_frame(&mut self, gl, target, size, frame_fbo, frame_tex) -> Result<(), GpuError>
```

两条路径都调它，变换只写一次。

### 20.4 结构性教训

**新增一条「交出/呈现」路径时，任何输出端变换都必须同步。** 这条 bug 的模式是
「有两处做同一件事，只改了一处」——与之前「合成范围与清空范围不一致」同类。

判断方法：`grep -c 'blit_framebuffer'` 应等于 1。多于 1 就说明有第二个交出点。

## 21. HDR 与 SDR 观感不一致：基色声明错误

### 21.1 原因

我们声明的是 **PQ + BT.2020**，但内容从未做 sRGB → BT.2020 的基色转换。
compositor 把 sRGB 基色的数值当作 BT.2020 解释，所有颜色都会偏移。

亮度这一侧本来是对的（`scale = reference/10000`，reference 取自输出描述），
所以差异主要表现为**色彩**而非明暗——这与「观感不一致」的描述吻合。

### 21.2 修法

声明与内容一致的基色：

```rust
creator.set_tf_named(TransferFunction::St2084Pq);
creator.set_primaries_named(Primaries::Srgb);   // 不是 Bt2020
```

函数名从 `declare_pq_bt2020` 改为 `declare_pq`：名字里写 BT.2020 而实际不转换，
本身就是个会误导后来人的陷阱。

**规则**：声明的基色必须等于内容实际使用的基色。要么转换，要么别声明。

### 21.3 仍会存在差异的地方

| 区域 | 是否应一致 | 原因 |
|---|---|---|
| 不透明像素 | ✅ 应一致 | 只是传输函数不同，数学上等价 |
| 半透明像素 | ⚠️ 会略差 | 缓冲存的是 `pq(L) × a`，compositor 在 PQ 域做预乘混合；SDR 路径在 sRGB 域做。两者都是近似，但近似方式不同 |

要完全一致，需要 compositor 在线性域混合——这对 PQ 缓冲同样不成立，
所以这是 PQ 预乘存储的固有限制（§13.1 已记录）。

### 21.4 若仍不一致，按这个顺序排查

1. **亮度整体偏** → `reference_lum` 与 compositor 的 SDR 白不同；用
   `CRABSNAP_REFERENCE_NITS` 试 80 / 203 / 100 看哪个吻合
2. **色彩偏** → 基色仍不匹配（本机已改为 sRGB，若换成 BT.2020 内容需真正转换）
3. **只有半透明区偏** → §21.3 的固有限制

## 22. 略微偏暗的根因：输出端不在 HDR 模式

`hyprctl monitors` 的 DP-6：

```
currentFormat: XRGB8888          # 扫描输出 8-bit
colorManagementPreset: srgb      # 输出端 SDR
sdrMinLuminance: 0.2
sdrMaxLuminance: 80              # 与 output_luminances 读到的完全一致
hardwareDetails: hdr: yes        # 硬件支持，compositor 未启用
```

### 22.1 结论

我们声明的 PQ 是 **surface 级**的，输出端仍是 SDR 80 nit / 8-bit。因此 compositor 必须把
PQ 内容**色调映射**到 SDR——这条曲线由 compositor 决定，不受我们控制。

**略微偏暗是色调映射器的滚降，不是编码错误。** 佐证：`scale` 用的 80 nit 正是
compositor 报的 `sdrMaxLuminance`，取值无误差。

### 22.2 由此推出的两条设计结论

1. **surface 声明 PQ ≠ 输出进入 HDR**。要真正呈现 HDR，输出的
   `colorManagementPreset` 必须是 HDR、`currentFormat` 必须是 10-bit；否则 10-bit 缓冲的
   精度也拿不到（被降到 8-bit 扫描输出）。
2. **HDR 内容 → SDR 输出的色调映射应由我们自己实现**（§4 第 5 步），而不是交给
   compositor。交给它就意味着外观取决于它的实现，我们无法保证与 SDR 路径一致。

### 22.3 已加诊断

输出端为 SDR 时程序会打印：

```
note: output is in SDR mode (max 80 nits); PQ content is tone-mapped
```

避免以后把色调映射的差异误判成编码错误。

## 23. 输出端切到 HDR 之后的实测对比

### 23.1 两种输出状态

| 状态 | `currentFormat` | `colorManagementPreset` | 我们读到的亮度 |
|---|---|---|---|
| SDR | `XRGB8888` | `srgb` | `(2000, 80, 80)` |
| **HDR** | **`XBGR2101010`** | **`hdr`** | **`(0, 417, 203)`** |

HDR 状态下 `reference = 203`，于是 `scale = 203/10000 = 0.0203`——正是 BT.2408 的值，
但现在是**读出来的**，不是常量。`min_lum = 0`，所以 `offset = 0`。

### 23.2 差异随输出状态变化

同一场景，只切换是否声明 PQ：

| 指标 | SDR 输出 | HDR 输出 |
|---|---|---|
| 中位亮度比 | 0.93 | **0.965** |
| p90 比 | 0.84 | **0.969** |
| 最大值比 | 0.76 | **0.938** |

**关键观察**：SDR 输出时比值随亮度下降（0.93 → 0.84 → 0.76），是色调映射滚降的特征；
HDR 输出后这个趋势消失，只剩一个接近均匀的 ~3.5%。

这条差异趋势本身就是判据：**均匀偏差 = 参考白/缩放问题；随亮度加剧 = 色调映射。**

### 23.3 剩余的 ~3.5%

在截图测量能分辨的范围内。可能来源：compositor 的 10-bit 抖动、grim 对 HDR 内容的
处理、以及我们 8-bit 帧缓冲先量化再编码。这些都不构成「编码错误」的证据。

判定方式改为人工观感（见 §12.3）。

### 23.4 代码已按输出状态分流

```rust
let output_is_hdr = lum.map(|(_, max, _)| max > 100).unwrap_or(false);
if !output_is_hdr && !force {
    println!("output is SDR: staying in SDR (set CRABSNAP_FORCE_HDR=1 to test anyway)");
}
```

在 SDR 输出上不再声明 PQ——那只会让 compositor 用不可见的曲线压缩高光（实测峰值压到 0.76）。
`CRABSNAP_FORCE_HDR=1` 保留强制测试的能力。

## 24. 自检：输出趟与 CPU 公式逐值一致

`examples/gpu_hdr_scene.rs` 把已知 sRGB 灰阶经输出趟写入缓冲，**读回缓冲**（不是屏幕）
与 `encode_for_pq_output` 对比。参考白 203 nit，黑电平 0：

| sRGB | 期望 | 实测 | 偏差 |
|---|---|---|---|
| 0 | 0.0000 | 0.0000 | -0.0000 |
| 32 | 0.2119 | 0.2118 | -0.0001 |
| 128 | 0.4278 | 0.4275 | -0.0004 |
| 192 | 0.5149 | 0.5137 | -0.0012 |
| 255 | 0.5807 | 0.5804 | -0.0003 |

**最大偏差 0.0012（约 0.3 个 8-bit 步长）**。着色器与 CPU 公式一致。

第二条断言反向钉住「变换没做事」这种情况：白色必须编到明显低于 1.0 的位置
（实测 0.5807）。否则一个恒等变换会通过所有「与输入相同」的检查。

### 24.1 这条证明了什么

**我们这一侧没有误差。** 于是屏幕上剩余的偏暗只能来自 compositor 对 PQ 缓冲的处理：
它解码 PQ 后按什么白点、有没有滚降，协议不告诉客户端，`hyprctl` 只暴露 `sdrMaxLuminance`。

### 24.2 因此的处理方式

不能靠推导，只能校准：

```bash
CRABSNAP_HDR=1 CRABSNAP_FORMAT=10bit CRABSNAP_REFERENCE_NITS=250 cargo run ...
```

`scale = reference/10000`，调高即整体变亮。找到与 SDR 观感一致的值，那个值就是
「本机 compositor 的实际 SDR 白」——203 只是它自称的。

若**任何单一标量都无法让两者一致**（暗部对上高光又不对），说明是曲线差异，
需要自己做色调映射（计划第 5 步）而不是校准。

### 24.3 这个例子是回归护栏

改动 `OUTPUT_FRAGMENT` 或 `encode_for_pq_output` 后跑一次即可发现问题；
它不需要窗口、不需要 compositor，只读自己的缓冲。

## 25. 结论：本机无法用「声明 PQ」达到与 SDR 一致的外观

### 25.1 实测事实

1. 让白色与 SDR 一致需要约 **414 nit**（≈ 显示器峰值 417）
2. 同一参考白下，**更暗的颜色偏亮**
3. 让不同颜色各自一致，**所需的 nit 不同**

### 25.2 推论

线性模型下，某颜色 `L` 要显示正确需要 `(ref·L)/K = L`，即 `ref = K`——**`L` 被约掉**。
所以「所有颜色所需 ref 相同」是线性模型的必然结果，且该值就是 compositor 的实际白点 `K`。

实测「所需 nit 随颜色而变」，直接证伪线性模型：**映射是一条曲线**。
注意这个推论与「取样的是纯白还是 55% 透明胶囊」无关——`L` 已约掉。
`K ≈ 414`（≈ 峰值 417）；按 414 编码时白色对上而暗色偏亮，说明低端被**抬升**，
是色调映射器的典型形状（顶部 shoulder + 底部 lift，BT.2390 那类 EETF）。

我们的编码侧已证明精确（§24，偏差 ≤ 0.0012），因此这条曲线在 compositor 一侧。
用单一参考白去补偿未知曲线，本质上是用标量拟合曲线——必然只能对上一个点。

### 25.3 决定：把色调映射收回自己手里

| | 声明 PQ（当前） | 内部 HDR（改为） |
|---|---|---|
| 外观一致性 | 取决于 compositor 的未知曲线 | **构造上保证** |
| 中间精度 | 10-bit | f16 |
| 依赖 | `wp_color_manager_v1` 的实现质量 | 无 |

做法：HDR 只在内部成立（线性工作域、f16 中间纹理），输出前由我们自己映射回 sRGB，
按普通 sRGB surface 呈现。**不声明 PQ。**

代价：拿不到显示器真正的 HDR 呈现。但那条路依赖 compositor 正确实现，
本机实测不具备。

### 25.4 保留的东西

`wp_color_manager_v1` 的绑定与读取代码**不删**——它已经能正确读到
`(0, 417, 203)`，一旦某个 compositor 版本把 PQ 映射做对了（或提供可查询的映射曲线），
把 `set_hdr` 打开即可。当前默认关闭，并由 `CRABSNAP_FORCE_HDR=1` 保留实验入口。

### 25.5 一条方法论

**当校准需要「每个点一个参数」时，说明模型错了。** 一个正确的模型应该能用
一个或两个参数描述全部观测；对不上时，先怀疑模型（把变换放在错误的层），
而不是继续调参数。

## 26. 根因：基色不匹配（不是亮度曲线）

### 26.1 读到了 compositor 期望的 tf 与基色

`wp_image_description_info_v1` 除 `luminances` 外还发 `tf_named` 与 `primaries_named`。
之前只收集了亮度，漏了这两个——而它们才是关键：

```
[colour] output prefers tf=11 primaries=6
```

按协议枚举：`11 = st2084_pq`，`6 = bt2020`。

### 26.2 因此

我们声明的是 PQ + **sRGB** 基色，compositor 期望 PQ + **BT.2020**。
它按 BT.2020 解释我们的 sRGB 数值 → **通道相关**的偏移，正是实测的
R 0.72 / G 0.82 / B 0.83 那种形态（而不是均匀的亮度差）。

§25 把它归因为「compositor 的未知曲线」是错的：曲线是**我们没做基色转换**造成的。

### 26.3 修法

1. 声明 `Primaries::Bt2020`
2. 输出趟里真的做 sRGB → BT.2020 的线性光矩阵转换

### 26.4 一个实现陷阱

GLSL 的 `mat3(...)` 参数是**列**，不是行。按行写会得到转置矩阵，行和不为 1，
灰色不再映射到灰色。`gpu_hdr_scene` 立刻抓到了这一点（灰色期望 0.2119 实得 0.1922）——
这正是该自检存在的价值：它用中性色，而转置错误必然破坏中性色。

### 26.5 待验证

画面是否与 SDR 一致，由使用者判定。若一致，则 §25 的「收回色调映射」结论**不成立**，
应保留 PQ 声明路径。

## 27. 锚点改取显示峰值

基色修正后仍偏暗，结合使用者早前的观测（白色需要约 414 nit ≈ 峰值 417），
改为以 `max_lum` 而非 `reference_lum` 作为参考白：

```rust
Some((_, max, _)) if max > 100 => (max, "output peak"),
```

依据：**本机 compositor 把 SDR 内容渲染在显示峰值上**，而描述里的
`reference_lum = 203` 只是 HDR 映射的目标值，不是 SDR 路径的实际白点。

### 27.1 这解释了早前「暗色偏亮」

早前观测到「白色对上后暗色偏亮」，当时归因为色调曲线。现在看更可能是**基色误解释**
（sRGB 数值被当 BT.2020）造成的通道相关偏差，被误读成亮度曲线。

两个错误叠在一起时，用单点校准永远对不上——这也是 §25.5「每个点一个参数说明模型错了」
的真实成因：模型错在基色，不在亮度。

### 27.2 待验证

参考白取峰值后，颜色与亮度是否同时与 SDR 一致。
若一致：HDR 路径成立，`set_hdr` 保留。
若仍不一致：说明亮度模型还有一层，需要读 `wp_color_management_output_v1` 的
其他信息（例如 `target_luminance`）。

## 28. 达成：肉眼一致，残余为中性色偏差

### 28.1 结果

`CRABSNAP_HDR=0` 与 `=1` 两窗口肉眼无法区分；VShot 取色：

| 通道 | HDR=0 | HDR=1 | 差 |
|---|---|---|---|
| R | 59 (`0x3B`) | 55 (`0x37`) | **-4** |
| G | 127 (`0x7F`) | 128 (`0x80`) | +1 |
| B | 215 (`0xD7`) | 215 (`0xD7`) | 0 |

### 28.2 剩余偏差的来源

我们矩阵的行和是 `0.9947 / 1.0016 / 1.0030`，不是 1.0：

- 标准 BT.2020 矩阵行和精确为 1，中性灰在任何色域里保持中性
- 我们用输出报的 **EDID 白点**做目标，它与 sRGB 的 D65 不同
- 因此 sRGB→该色域的转换**不保持中性**，留下 R 偏低、G/B 略高的残余

方向与实测完全一致（R -4、G +1、B 0）。

### 28.3 正确的修法：色适应

现在只做了原色转换，没有白点适配。完整做法是
`M = inv(XYZ_target) · Bradford(src_white → dst_white) · XYZ_srgb`。
Bradford 之后矩阵行和回到 1，中性色不再偏移。

### 28.4 为什么暂不修

残余 4/255（约 1.6%）在肉眼不可见，而引入色适应需要：

1. 判断 compositor 是否**已经**做过适应（它把我们的内容当目标白点下的内容处理）
2. 若双方都做，会**过适应**，偏差反而变大

在无法观测 compositor 内部的情况下，加一层不可验证的变换风险高于收益。
记录为已知残余，若将来需要精确匹配再实测决定。

## 29. 达成：HDR 与 SDR 输出一致（残余 1 LSB）

### 29.1 实测结果

开启 Bradford 白点适配后，VShot 取色：

| 通道 | HDR=0 | HDR=1 | 差 |
|---|---|---|---|
| R | `0x3B` | `0x3B` | 0 |
| G | `0x7F` | `0x7F` | 0 |
| B | `0xD7` | `0x80`→`0x7F` | **1 LSB** |

从 4 LSB 降到 1 LSB，已到 8-bit 量化下限。

### 29.2 这同时测量出 compositor 的行为

Bradford 让中性色精确落位，说明 **compositor 自己不做白点适配**。
这是协议不告诉客户端的信息，只能通过这样的对照实验测出来。

所以 Bradford 现在是 HDR 路径的默认；`CRABSNAP_WHITE_ADAPT=0` 可关闭以重测。

### 29.3 完整链路（全部取自输出自己的描述）

| 环节 | 来源 |
|---|---|
| 传输函数 | 输出描述 `tf = st2084_pq` |
| 基色 | 输出描述携带的 EDID 色度坐标 |
| 白点适配 | Bradford（实测 compositor 不做） |
| 参考白 | 输出描述 `reference_lum = 203` |
| 描述本体 | **直接复用输出自己的** `wp_image_description_v1` |

### 29.4 一条可复用的判据

**「构造一个描述」和「复用对端的描述」不是等价的。** 构造的描述会声明自己的峰值
（PQ 下被协议规定为 10000 nit），于是对端必须压缩；复用对端的描述则没有可映射的差异。
后者才是「捕获后贴回」能做到逐像素一致的原因。

## 29. 内部存储精度可提升到 f16

### 29.1 抽象

内部目标精度是**管线**的属性，与被呈现的缓冲格式无关——两者是不同的轴：

| 轴 | 类型 | 取值 |
|---|---|---|
| 内部存储 | `TargetFormat` | `Rgba8` / `Rgba16F` |
| 被呈现缓冲 | `BufferFormat` | `Bgra8Srgb` / `Rgb10a2` / `Rgba16Float` |

```rust
pub enum TargetFormat { Rgba8, Rgba16F }
impl TargetFormat {
    fn internal(self) -> i32;        // GL 内部格式
    pub fn bytes_per_pixel(self) -> u32;
}
```

`LayerTarget` 记录自己的 `format`，`ensure_target` 在**尺寸或格式变化**时重新分配——
只比尺寸会让格式切换静默复用旧存储，这是最容易漏的一处。

### 29.2 接线

| 位置 | 内容 |
|---|---|
| `effect_pass.rs` | `TargetFormat`、`ensure_target(..., format)`、`run(..., format)` |
| `renderer.rs` | `GpuRenderer::set_target_format()`；帧、层目标、效果 scratch、输出 scratch 全部跟随 |
| `lib.rs` | 导出 `TargetFormat` |
| 示例 | `CRABSNAP_FRAME_F16=1` |

### 29.3 用法

```bash
CRABSNAP_FRAME_F16=1 CRABSNAP_HDR=1 CRABSNAP_FORMAT=10bit \
  cargo run -p ui-platform-wayland --example window_gpu --features gpu,system-fonts
```

输出 `frame storage: RGBA16F` 表示生效。

### 29.4 它解决什么

8-bit 帧缓冲在**到达输出之前**就把值量化了：暗部渐变实测只有 5 个不同编码
（§12，64 级中），f16 是全部 64 级。这影响的是渐变与模糊边缘的色带，
与是否声明 HDR 无关——所以这条收益在 SDR 输出上同样成立。

## 30. 层的圆角合成（两个后端）

`LayerEntry.radius` 一直存在于绘制列表里，但**两个后端都没用**。现在补齐。

### 30.1 CPU

`composite_over_rounded(dst, src, opacity, region, bounds, radius)`：逐像素用
`coverage_rounded_rect` 算覆盖率，乘到层的 alpha 上。半径是设备空间属性，
因此 `bounds` 经 `to_device` 换算、半径乘 `scale`。

### 30.2 GPU

`LAYER_FRAGMENT` 增加 `u_rect` / `u_radius` / `u_exp`，用与 CPU **相同的等值线**
（`snorm(x, y, n)` 即 `(|dx|^n + |dy|^n)^(1/n)`）算覆盖率，再乘到采样值上。
顶点着色器多输出 `v_device`，因为覆盖率要在设备坐标里算。

`layer_radius` 与 `layer_bounds` 平行存放，进入层时按 `scale` 换算后记录。

### 30.3 不变量

`examples/gpu_round_layer_scene.rs`：

| 断言 | 含义 |
|---|---|
| [1] 角落 alpha = 0（两后端） | 圆角确实被切掉 |
| [2] 中心 alpha = 255 | 层内容仍然被画出来 |
| [3] 半径 0 时角落 alpha = 255 | 切角来自**半径**，不是层为空 |
| [4] 两后端被切掉的量相差 < 2% | 覆盖率 AA 是实现定义的，因此比总量不比逐像素 |

实测：cpu mass 531556 / gpu 531604，差 48（0.009%），全部来自 AA 的边缘像素。

### 30.4 [3] 是这套断言的关键

只有 [1] 与 [2] 时，一个「根本不画层」的实现也会通过 [1]（角落本来就是 0）。
[3] 用半径 0 的对照组把「被切掉」与「没画」区分开——这是不变量设计里最容易漏的一半。
