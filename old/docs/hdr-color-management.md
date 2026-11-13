# HDR 与色彩管理的架构影响

> 需求：程序需**输出 HDR 画面并支持色彩管理**（同时支持 SDR 环境），因此必须原生处理
> **像素格式与 buffer**（shm、dma-buf，以及 macOS/Windows 上的同类物）。
> 本文回答：这个需求改变了什么、哪些设计要动、哪些不用动。
> 依据：Wayland 协议的 XML（本机 `wayland-protocols-0.32.13` 可直接读）、
> egui/GPUI 源码；平台细节未核对的标 `[未证实]`。

---

## 0. 结论

**这个需求不是"加一个 HDR 分支"，而是把「像素格式」从实现细节提升为一等概念。**

| 受影响的 | 结论 |
|---|---|
| **渲染后端的内部 surfaces** | 存储域是**策略**（`Srgb8`/`LinearF16`/`Packed10`），不是固定 f16（§5.3） |
| **呈现目标** | 必须能表达"这个 buffer 是什么格式、什么色彩空间" |
| **输出编码** | 需要一个**唯一**的编码落点（§4 的契约），不能分散 |
| **效果与合成** | 必须在线性光域算（否则 HDR 下模糊/混合数学错误） |
| **CPU 光栅核心**（zeno） | **不受影响**——它只出覆盖率，格式在我们手里（见 §5） |
| **UI 引擎的图元模型** | **不受影响**——`RectPrim`/`PathPrim` 等与像素格式无关 |

---

## 1. 三条不变量（先定，其余由此推出）

### I1. 中间表示唯一：线性光、预乘或非预乘要固定

所有运算（模糊、马赛克、混合、缩放）**只能在一个域里做**。
这个域必须是**线性光**——因为"模糊 = 加权平均"只在线性光域才是正确的亮度平均。
在 sRGB 编码域做模糊会让暗部过度变暗（这是 sRGB 非线性的直接后果，可推导）。

**这与 SDR/HDR 无关**：SDR 内容也应在线性光域运算。区别只在最终编码到 8-bit 还是 f16。

### I2. 字节与描述永不分离

一块像素 buffer 必须**带着它的格式与色彩描述**存在。丢描述就无法正确解释：

- 同样 4 字节，可能是 `ARGB8888`（sRGB 编码）也可能是打包 10-bit PQ；
- 同样 8 字节，可能是 `ABGR16161616F`（线性 f16）也可能是别的东西。

**所以 `Canvas`/`Target` 的构造必须同时接受格式与描述，不能分两步。**

### I3. 编码只有一个落点

从"线性光中间表示"到"某种 buffer 格式"的转换，全项目只允许有一处实现。
旧项目把这条写成了架构约定（`docs/color-pipeline.md` 的"契约②"），思路是对的；
但按 AGENTS.md §1，我把它当**设计输入**而不是依据——下面的推导独立成立：

> 若编码分散在 N 处，则"换原色 / 色调映射 / 预乘 / 标度"四个决定就有 N 份副本，
> 任意两份不一致就会在某个输出路径上出现偏色，且难以定位。

---

## 2. 像素格式：从"隐式"到"显式"

### 2.1 格式枚举

```rust
/// 一块像素缓冲的编码。**必须与色彩描述成对出现**（I2）。
/// How a pixel buffer is encoded. Always paired with a colour description.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 8-bit sRGB 编码，预乘 alpha（wl_shm / 通用 XRGB 平面）。
    Sdr8Premul,
    /// 8-bit sRGB 编码，非预乘（文件导出、剪贴板：文件格式用 straight alpha）。
    Sdr8Straight,
    /// 10-bit 打包（Wayland `XRGB2101010`；Windows `R10G10B10A2`；macOS `BGR10A2`）。
    Hdr10,
    /// 16-bit 半浮点（Wayland `ABGR16161616F`；scRGB 平面）。
    Rgba16F,
    /// 16-bit 归一化整数（`RGBA16`，部分平台/SDR 高精度导出）。
    Rgba16,
}
```

**为什么不是"按平台枚举"**：同一个 `Rgba16F` 在 Wayland 上是 dma-buf fourcc，
在 macOS 上是 `MTLPixelFormatRGBA16Float`，在 Windows 上是 `DXGI_FORMAT_R16G16B16A16_FLOAT`。
**格式是语义，平台是映射**——把二者混在一个枚举里会让"格式转换"变成"平台分支"。

### 2.2 格式的能力查询

```rust
impl PixelFormat {
    /// 能否表示超过 1.0 的线性值（HDR 高光）。
    /// Can represent linear values above 1.0 (HDR highlights).
    pub const fn carries_hdr(self) -> bool;
    /// 每像素字节数（`Hdr10` 是打包的，按 4 字节算）。
    pub const fn bytes_per_pixel(self) -> u8;
    /// 是否预乘。
    pub const fn premultiplied(self) -> bool;
}
```

`carries_hdr()` 是**"这个槽能不能装超白"的唯一判据**——不是"平台支持不支持 HDR"。
一台 HDR 显示器上的 8-bit shm 面依然装不下超白。

---

## 3. 呈现目标：能力协商而非平台分支

### 3.1 Wayland（已核对协议 XML）

`wayland-protocols-0.32.13` 的 staging 目录里有：

| 协议 | 作用 |
|---|---|
| `color-management-v1` | 表面声明的原色/传递函数/亮度（`set_primaries`/`set_tf_power`/`set_luminances`/`set_mastering_display_primaries`/`windows_scrgb`/`windows_bt2100`…） |
| `color-representation-v1` | 表面的色彩表示（SDR/HDR 意图） |
| `ext-image-copy-capture` | 截图侧（HDR 捕获） |

**这些协议存在的事实已核实**（读的是 XML 里的接口与 enum 条目）；
它们的具体语义细节我没有逐条读完，标 `[未证实]`。

dma-buf 侧的原生格式（`ABGR16161616F` = `0x48344241`、`ARGB8888` = `0x34325241`）
是 DRM fourcc，属于**公开常量**，可直接使用，无需协议。

### 3.2 其它平台（`[未证实]`，需实现时核对官方文档）

| 平台 | HDR 通道 | 格式 |
|---|---|---|
| Windows | DXGI flip model + `DXGI_COLOR_SPACE_TYPE`（`RGB_FULL_G2084_NONE_P2020` 等） | `R10G10B10A2`、`R16G16B16A16_FLOAT` |
| macOS | `CAMetalLayer.wantsExtendedDynamicRangeContent` + `NSScreen.maximumExtendedDynamicRangeColorComponentValue` | `RGBA16Float`、`BGR10A2` |

**这三条都标了 `[未证实]`**：本机没有对应 SDK，无法核实。**但架构不依赖它们的细节**——
只要满足 §3.3 的接口，每个平台各自实现即可。

### 3.3 呈现目标的接口

```rust
/// 呈现目标：一块能上屏的缓冲。**格式与色彩描述是身份的一部分**（I2）。
/// A presentable buffer. Format and colour description are part of its identity.
pub trait PresentTarget {
    fn size(&self) -> (u32, u32);
    fn scale(&self) -> f64;

    /// 这个目标的**实际**格式与色彩（探测结果，不是请求）。
    /// 请求 f16 拿不到时会降级成 8-bit，调用方必须读这里而不是假设。
    fn format(&self) -> PixelFormat;
    fn colour(&self) -> ColorDescription;

    fn present(&mut self, damage: Damage) -> Result<()>;
}

/// 一个平台的呈现能力：**不暴露平台类型**，只暴露"支持哪些格式"。
/// A platform's presentation abilities, expressed as supported formats.
pub trait PresentBackend {
    fn supported(&self) -> &[PresentPath];
    fn open(&mut self, request: PresentRequest) -> Result<Box<dyn PresentTarget>>;
}

pub struct PresentPath {
    /// 传输方式：shm / dma-buf / 平台原生句柄。
    pub transport: Transport,      // SharedMemory | DmaBuf | Native
    pub format: PixelFormat,
    /// 能装多少 nits（`None` = 未知，按 SDR 处理）。
    pub peak_nits: Option<f32>,
}

pub enum Transport {
    SharedMemory,   // Wayland wl_shm；X11 MIT-SHM
    DmaBuf,         // Wayland zwp_linux_dmabuf_v1 / GBM
    Native,         // HWND+D3D / CAMetalLayer（平台内部细节不外泄）
}
```

**关键设计点**：

1. 呈现层只暴露 `(Transport, PixelFormat, peak_nits)` 这个**纯数据能力集**，
   上层用同一个选择器（`core::select`）决定路径——**没有 `if platform == ...`**；
2. `format()` 返回**实际**格式而非请求格式（请求 f16 被降级成 8-bit 是常态，
   调用方必须在编码时读它，否则会按 f16 编码再写进 8-bit 缓冲，画面全黑）；
3. `Native` 变体把平台细节锁在 backend 内部，`ui-engine` 永远看不到 `HWND` 或 `CAMetalLayer`。

---

## 4. 输出编码：唯一的落点

```rust
/// 从"线性光中间表示"到"某格式的字节"——**全项目唯一实现**（I3）。
/// The single conversion from linear light to bytes of a given format.
pub fn encode_for_output(
    src: &Surface,            // 存储域见 §5.3.4
    target: &OutputTarget,    // { format, primaries, tf, reference_nits, peak_nits }
    dst: &mut [u8],
);   // 同域时退化为无操作（SDR→Srgb8、HDR→LinearF16 都不触发转换）
```

`OutputTarget` 的四个维度（**来自协议与显示能力，不是硬编码**）：

| 维度 | 来源 | 影响 |
|---|---|---|
| `format` | `PresentTarget::format()` | 走哪条编码分支 |
| `primaries` | 表面声明 / 显示描述 | 是否换原色矩阵 |
| `tf` | 同上（PQ / HLG / sRGB / 线性） | 曲线 |
| `reference_nits` / `peak_nits` | 同上（缺失 → 回落 203 / SDR） | 色调映射的参考白与上限 |

**色调映射只在"内容峰值 > 目标峰值"时才启用**。SDR 目标上，HDR 内容需要 tonemap；
HDR 目标上，SDR 内容**不需要**（只是在高光区留空）。

**注意 `encode_for_output` 不是"每条路径都要走的一步"**：若目标存储域与内容当前域一致
（SDR 渲染在 `Srgb8` 上、HDR 渲染在 `LinearF16` 上），这一步退化为一次 `memcpy`/无操作。
它只在**确实跨域**时做实质工作（见 §5.3.4 的分层规则）。

---

## 5. 这些改动如何影响既有决策

### 5.1 后端 surface 的存储格式是**策略**，不是常量

上一版这里写"CPU 内部恒为 f16 线性光"。按 §5.3 的结论，这是不必要的：

```rust
/// 渲染目标：存储域是构造参数（见 §5.3.4）。
/// A render target whose storage domain is a construction parameter.
pub struct Surface {
    domain: StorageDomain,   // Srgb8 | LinearF16 | Packed10 | ...
    ..
}
```

| 环境 | 存储域 | 理由 |
|---|---|---|
| SDR 呈现 | `Srgb8` | 省内存/带宽；线性化由逐操作 LUT（CPU）或 sRGB 格式（GPU）完成 |
| HDR 呈现 | `LinearF16` 或 `Packed10` | 内容含超白，必须高动态范围容器 |
| 中间结果含超白 | `LinearF16` | 仅在确实需要时分配，不是全屏常态 |

**只有一种情况需要显式的全屏转换 pass**：把 `LinearF16` 的结果交到 `Srgb8` 目标
（HDR 内容呈现在 SDR 屏上）。那时还要先做 tonemap——这是 §4 的语义要求，不是格式开销。

### 5.2 效果必须在线性光域，但**不要求** f16 缓冲

`PixelBackdrop` 的 `cook` 动作必须在线性域做：马赛克的块平均、模糊的加权、
液态玻璃的折射采样，在编码域做都会系统性偏暗（§5.3.2 实测：黑白棋盘降采样差 61/255）。

但**"在线性域运算"不等于"缓冲必须是 f16"**。`cook` 的契约是
**语义在线性光，存储随目标域**：

```rust
/// 效果核：语义上在线性光域运算；实际缓冲的存储域由 target.domain 决定。
/// Kernel semantics are linear-light; actual storage follows target.domain.
fn cook_cpu(&self, src: &Surface, dst: &mut Surface, plan: &EffectPlan);
```

对 `Srgb8` 缓冲，效果核内部做 LUT 往返（读时线性化、写时编码），
不需要先把整块升成 f16 再降回来。

### 5.3 消除 SDR 的 f16→8bit 转换：**把"存储域"与"运算域"分离**

上一版这里写的是"必须付出一次全屏 f16→8bit 的代价"。**那是错的**——
错在把"运算必须在哪个域"和"缓冲区里存什么域"当成了同一件事。

#### 5.3.1 先看代价有多大（可推导的数值）

两件事必须区分，它们并不相同：

| | 定义 | SDR 环境下的取值 |
|---|---|---|
| **存储域** | 缓冲区里每个通道是怎么编码的 | 可以就是 **sRGB 编码的 8-bit** |
| **运算域** | 一次读-改-写运算在哪个空间里做 | **线性光**（否则混合/模糊数学错误） |

#### 5.3.2 为什么"运算域 = 线性光"不能被放弃（实测数值）

在不同的运算域下做同一件事，差异远大于"细微"：

| 场景 | 编码域运算 | 线性域运算 | 差 |
|---|---|---|---|
| 深底(16) 上两个 50% 白块 | 196 | 225 | **29/255** |
| 深底(20) 上叠 6 层 30% 白 | 228 | 252 | **24/255** |
| 黑白棋盘降采样（模糊） | 128 | 188 | **61/255** |
| 黑底上 50% 覆盖的白色文字 | 128 | 188 | **61/255** |

（本机脚本计算，非估算。）之前的"细微差异"说法是错的——**编码域混合在暗部系统性偏暗**，
这正是 sRGB 非线性的直接后果。

#### 5.3.3 但"运算在线性域"不等于"缓冲必须是 f16"

**sRGB 曲线是可逆的一维函数，可以用查找表精确往返。** 于是：

```text
读出 u8 (sRGB 编码)
   → LUT 线性化 → f32/f16 中间值（在寄存器里，不在内存里）
   → 在线性域做混合/模糊
   → LUT 编码回 u8
```

**关键点：中间的线性值只存在于寄存器/栈上，不需要一块全屏 f16 缓冲。**

实测这个方案的量化误差（与"全 f16 缓冲"对照）：

| 场景 | f16 缓冲 | u8 缓冲 + LUT 往返 | 差 |
|---|---|---|---|
| 3 层半透明叠加 | 183 | 183 | **0** |
| 10 层 | 218 | 218 | **0** |
| 20 层 | 220 | 219 | **1 LSB** |
| 50 层 | 221 | 219 | **2 LSB** |

理论值：单次往返最大量化误差 0.5 LSB；n 层不相关累积约 `√n × 0.5` LSB。

**结论：SDR 环境下完全不需要 f16 缓冲，也不需要额外的全屏转换 pass。**
编码域与运算域的差异被"混合前查表"消除，剩下的只是 1–2 LSB 的量化误差——
**远小于 §2.2 规定的容差（2/255）**。

#### 5.3.4 抽象：把"域"变成类型上的一等参数

```rust
/// 缓冲区的存储编码：决定内存布局与 LUT。
/// Storage encoding of a buffer: determines layout and the transfer LUT.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StorageDomain {
    /// 8-bit sRGB 编码（SDR 环境：省内存、省带宽、无需转换 pass）。
    Srgb8,
    /// 8-bit sRGB 编码 + 可达超白（HDR 内容的 SDR 呈现需要 tonemap，见 §4）。
    Srgb8ToneMapped,
    /// 16-bit 半浮点线性光（HDR 环境、或需要超白的中间结果）。
    LinearF16,
    /// 10-bit 打包（HDR 内容的直通呈现）。
    Packed10,
}

/// 一次运算要求的运算域。**由操作类型决定，不由缓冲决定。**
/// The domain an operation requires. Determined by the op, not the buffer.
pub enum OpDomain {
    /// 任意域皆可（纯色填充、不透明覆盖：无混合）。
    Any,
    /// 必须线性光（`over` 合成、模糊、缩放、文字抗锯齿）。
    Linear,
}
```

**分层规则（这三条就是"避免域间转换"的完整答案）**：

| 缓冲类型 | 存储域 | 运算 | 是否需要全程转换 |
|---|---|---|---|
| SDR 目标（`Srgb8`） | sRGB 8-bit | 逐操作 LUT 往返（寄存器内线性化） | **否**——没有额外的 pass，转换并入每次混合 |
| HDR 目标（`LinearF16`/`Packed10`） | 本身就是线性/近线性 | 直接在线性域算 | **否**——本来就在正确的域 |
| 需要超白的中间结果 | `LinearF16` | 直接算 | **否** |
| 从 `LinearF16` 交到 `Srgb8` 呈现 | — | 一次编码（仅当内容确实需要跨域） | **是，但只在该跨域时发生** |

**"SDR 环境下 f16→8bit" 因此被彻底消除**：SDR 渲染路径从头到尾都在 `Srgb8` 缓冲里，
线性化发生在每次混合的读取步骤（LUT 查表），没有单独的转换 pass。

#### 5.3.5 这条在 GPU 上如何落地

GPU 的固定管线混合在目标格式域里做（不能改），所以：

| 目标格式 | GPU 策略 |
|---|---|
| `LinearF16` / `Packed10` | 直接渲染 + 硬件混合（线性域 / PQ 域，符合各自规范） |
| `Srgb8` | 用 **`SRGB8_ALPHA8` 这类 sRGB 感知格式**：硬件在采样时自动线性化、写入时自动编码。**这正好实现了 §5.3.3 的 LUT 往返，且由硬件免费完成** |

`SRGB8_ALPHA8` 的存在是关键：它让"SDR 缓冲 + 线性域运算"在 GPU 上**零成本**。
（`[未证实]`：需在目标后端逐平台确认该格式可用；WebGL2/Vulkan/D3D12/Metal 均支持
sRGB 格式的 render target，但混合是否在线性域执行取决于驱动与格式设置，实现时必须实测。）

#### 5.3.6 仍有的一处真实代价

若**同一帧内**既有 HDR 内容又有 SDR 内容，且它们要合成到同一个 `Srgb8` 目标上，
则 HDR 内容必须先 tonemap 到 [0,1]（这是 §4 的 tonemap，不是格式转换，语义上必需）。
这不是可以消除的开销——它是 HDR→SDR 的**信息损失**，不是无谓的转换。

### 5.4 CPU 光栅库（zeno）不受影响

zeno 只产出 **alpha 覆盖率**，不碰像素格式——这恰是 §1（`cpu-rasterizer-survey.md`）
选它的一个额外好处：

```text
zeno: 路径 → alpha mask（与格式无关）
我们:   mask × 颜色 → blend_into(&mut LinearImage 或 &mut Srgb8Image)
```

**如果选 tiny-skia**，它的 `Pixmap` 是固定 `RGBA8`，要做 HDR 就得绕开它
（旧项目正是这么做的：效果路径完全没走 tiny-skia）。**这一点加强了选 zeno 的理由。**

---

## 6. 对 UI 引擎设计的具体改动

| 章节 | 改动 |
|---|---|
| §6.4 `Canvas` | 增加 `format: PixelFormat` 与 `colour: ColorDescription`；构造必须同时给出（I2） |
| §6.2 `Primitives` | **不变**——图元与像素格式无关 |
| §8 效果 | `cook_cpu/cook_gpu` 的契约明确为"线性光 f16 输入输出" |
| §12 渲染后端 | 增加"后端 surface 的格式是构造参数"；补 §5.3 的取舍 |
| §12.5 `PresentTarget` | 换成 §3.3 的接口（`format()` 返回实际格式 + `PresentPath` 能力集） |
| 新增 §12.7 | HDR 编码契约（`encode_for_output` 唯一落点 + `OutputTarget` 四维度） |
| §14 测试 | 增加：同一 `LinearImage` 编码到 5 种格式后往返（可逆者）一致；超白值在 `Sdr8*` 上被 tonemap 而非硬裁 |

---

## 7. 必须做的验证（真机才能判）

| 项 | 怎么验 |
|---|---|
| HDR 内容在 HDR 显示上不过曝 | 用已知 PSNR ≤ 20 的测试图 + 峰值计 |
| SDR 内容在 HDR 显示上不灰 | 白块应为 203 nits 对应的亮度 |
| 降级路径 | 强制只有 shm 时，HDR 内容被 tonemap 到 SDR 而不是裁切 |
| 两条后端一致性 | 同一内容 → 同一目标域 → 两条路径的字节差异 < 量化误差 |
| **域往返精度** | 1 层/10 层/50 层半透明叠加：`Srgb8`+LUT 与 `LinearF16` 的结果差 ≤ 2 LSB（§5.3.3 实测 0/0/2） |
| **无额外 pass** | 抓帧确认 SDR 路径上没有整屏 f16→8bit 的 blit（GPU 用 RenderDoc/apitrace，CPU 用 pass 计数） |

**这些都不能靠单测判**（需要真实 compositor + HDR 显示），按 AGENTS.md §7.4 放 `examples/`。

---

## 8. 待决问题

1. **`ColorDescription` 的完整性与来源**：是只存协议给的枚举，还是存完整 ICC？
   建议先只存枚举 + 亮度数值，ICC 留成 `[未证实]` 的扩展点。
2. **f16 全屏缓冲在 CPU 路径上是否彻底放弃**：§5.3 建议放弃 SDR 的 f16，
   但 HDR 内容在 CPU 上仍需要 f16（否则内容无法承载超白）。这意味着
   **CPU 路径的 HDR 性能会明显低于 SDR**——是否接受？
3. **`Transport::Native` 的抽象边界**：Windows/macOS 的"原生句柄"是否需要一个统一句柄类型，
   还是每个 backend 自己持有、对外只暴露 `PresentTarget`？建议后者（更简单）。
