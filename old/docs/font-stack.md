# 字体栈：平台原生回退的抽象设计

> 需求：字体栈要能接入**各平台原生的系统字体栈回退**，采用逐平台实现。
> 本文回答：抽象边界画在哪、每个平台原生 API 是什么、已验证到什么程度。
> 依据：`fontique 0.12` 源码（本机）、fontconfig 头文件与 `fc-match`（本机实测）；
> Windows/macOS 无 SDK，相关平台细节按 AGENTS.md §1.2 标 `[未证实]`。

---

## 0. 结论

**抽象边界画在"请求"与"字面"之间，不在"字体文件"与"文本"之间。**

```text
上层（引擎/控件）      只说：我要 sans-serif、这是个中文字符串、14px
                          │  FontRequest { families, generic, weight, script }
                          ▼
FontProvider (trait)    逐平台实现，负责：枚举族、选字面、给回退链、取度量与字节
                          │  FaceId + FaceMetrics + advance + blob
                          ▼
下层（光栅器）          拿到字体文件字节，自己解析字形
```

**关键**：引擎永远不知道「Windows 用 Segoe UI、macOS 用 SF Pro、Linux 用 fontconfig 的 sans-serif」
这些事实。它只发请求。

---

## 1. 各平台原生栈是什么（已核实 API 调用点）

| 平台 | 原生 API | fontique 后端 | 核实方式 |
|---|---|---|---|
| **Linux/BSD** | **fontconfig**：`FcConfigGetFonts`、`FcFontMatch`、`FcConfigSubstitute` | `backend/fontconfig.rs:29, 646` | 读 fontique 源码；本机 `fontconfig 2.18.3` + `fc-match` 实测 |
| **Windows** | **DirectWrite**：`DWriteCreateFactory`、`IDWriteFontCollection`、`IDWriteFontFallback` | `backend/dwrite.rs:18-19, 143` | 读 fontique 源码（`[未证实]`：无 SDK 可跑） |
| **macOS/iOS** | **CoreText**：`CTFontCollection::from_available_fonts`、`CTFontDescriptor` | `backend/coretext.rs:18, 94` | 读 fontique 源码（`[未证实]`：无 SDK 可跑） |
| **Android** | 自有 `system_fonts.xml` 解析 | `backend/android.rs` | 未细读 |

**重要发现（`coretext.rs:130` 的注释）**：Apple 在 `CTFontCollection` 里**隐藏了部分字体**，
包括 SFNS.ttf（San Francisco）——这正是系统 UI 字体。fontique 为此专门做了补洞。
**这类平台怪癖是"逐平台实现"不可替代的理由**，纯跨平台方案必然漏掉它。

---

## 2. 本机实测结果（可复现）

用 `fontique 0.12` 跑真实系统：

```text
1) family count: 171
2) resolve("MiSans VF")        -> Some(2 fonts)
2) resolve("Noto Sans CJK SC") -> Some(2 fonts)
3) chain[Latn] -> ["MiSans VF"]
3) chain[Hani] -> ["MiSans VF"]
3) chain[Arab] -> ["Arial"]
3) chain[Thaa] -> ["MV Boli"]      ← 塔安娜文，回退到专门的字体
4) face_data: 74464 bytes, source=Path("/usr/share/fonts/TTF/ntailu.ttf")
```

**四项能力全部可用**：枚举、按名解析、按脚本回退、取原始字节。

对照 `fc-match`（fontconfig 自己的命令行）：

```text
$ fc-match sans-serif    → MiSans VF.ttf
$ fc-match "sans:lang=zh" → MiSans VF.ttf
$ fc-match Arial          → MiSans VF.ttf   ← fontconfig 的替换规则
```

**一个反直觉的地方**：`fc-match Arial` 返回 MiSans VF，因为 fontconfig 有别名/替换规则。
而 fontique 的 `family_by_name("Arial")` 返回 `NOT FOUND`（本机没装 Arial，也不走 fontconfig 的别名）。
**两者语义不同**：fontconfig 做"最佳匹配"，fontique 做"精确查名 + 脚本回退"。
我们的 `FontProvider` 两种都需要，但要分清：
- **具名族**用精确查（用户选了 Arial 就该是 Arial，或明确告知没有）；
- **通用族**用 fontconfig/平台的默认（系统说 sans-serif 是什么就是什么）。

---

## 3. 抽象的六个方法

```rust
pub trait FontProvider {
    fn name(&self) -> &'static str;
    fn family_names(&self) -> Vec<String>;
    fn resolve(&self, request: &FontRequest) -> Option<FaceId>;
    fn fallback_chain(&self, script: Script) -> Vec<FaceId>;
    fn has_glyph(&self, face: FaceId, c: char) -> bool;
    fn metrics(&self, face: FaceId) -> FaceMetrics;
    fn advance(&self, face: FaceId, text: &str, size: f32) -> f32;
    /// 原始字体字节，供光栅器自己解析。
    fn face_data(&self, face: FaceId) -> Option<Arc<[u8]>> { None }
}
```

**为什么是这六个**（每一个都不可省，且都对应一个平台事实）：

| 方法 | 为什么必须有 | 平台差异 |
|---|---|---|
| `family_names` | 字体选择器要列出来 | 各族数量不同（本机 171） |
| `resolve` | 具名族要精确 | fontconfig 有别名，DirectWrite/CoreText 没有 |
| `fallback_chain` | 缺字形时要换字体 | **每个平台的回退算法完全不同**：fontconfig 用配置规则，DirectWrite 用 `IDWriteFontFallback` 的系统映射表，CoreText 用 `CTFontCreateForString` |
| `has_glyph` | 判断要不要回退 | 需要读 cmap；各平台 API 不同 |
| `metrics` | 行高、基线 | 单位不同（每 em 单位 vs 已缩放） |
| `advance` | 布局宽度 | **需要 shaped 文本**才是精确的；简化版按字符累加 |
| `face_data` | 光栅器要解析字形 | 有些平台给路径，有些给内存 blob |

**`advance` 的诚实说明**：真正的版本需要整形（连字、字距、复杂脚本重排）。
当前实现按"每字符前进宽度"估算，这是**已知的近似**，整形器落地后替换。
它不影响抽象——`advance` 的签名不变。

---

## 4. 逐平台实现的落地方式

### 4.1 三个 provider crate（不是三个 feature）

```text
ui-text/                     trait + FontStack + 回退逻辑（零平台依赖，可单测）
ui-text-fontique/            Linux/Windows/macOS 统一原生实现（feature 选后端）
ui-text-static/              打包单一字体（内置 Noto），无系统依赖
```

**为什么用 crate 而不是 feature**：按 AGENTS.md §3，"跨 crate 不使用编译期 cfg"。
如果 `ui-text` 里用 `#[cfg(target_os)]` 选后端，那 `ui-text` 就有了平台依赖，
它的单测也就要按平台条件编译。分成 crate 后：

- `ui-text` 保持零平台依赖，`FontStack` 的逻辑（回退、缓存、度量）可以在任何机器上测；
- 选哪个 provider 是**主程序**的事（它在 `main` 里挑，或在 `arch.toml` 里配）。

### 4.2 与 `fontique` 的关系

建议 **`ui-text-fontique` 复用它，而不是自己写三个平台后端**。理由：

1. 它已经把三个平台的原生调用写好了（含 §1 提到的 CoreText 隐藏字体补洞）；
2. 它是 parley 的字体层，parley 是成熟排版栈；
3. 自己实现 DirectWrite/CoreText 绑定是数千行 unsafe 代码，收益为零。

**但它的 API 不直接就是我们的 `FontProvider`**（实测差异）：
- 它的 `family_by_name` 是精确查，没有 fontconfig 的别名语义；
- 它没有 `has_glyph`（要自己从 `FontInfo` 的 charmap 查）；
- 它的 `advance` 不存在（要自己做整形或估算）。

所以 `ui-text-fontique` 是一个 **adapter**：把 fontique 的形状翻译成我们的 trait。
这正是抽象的价值——**换掉 fontique 时，只需重写这个 adapter**。

### 4.3 无系统字体的环境（测试、最小镜像）

`StaticProvider`（已在 `ui-text` 里）用固定字面列表，零依赖。
它让 `FontStack` 的回退逻辑可测，也让 CI 不依赖系统字体。

---

## 5. 已实现的部分

| 项 | 状态 |
|---|---|
| `FontProvider` trait（8 个方法） | ✅ |
| `FontRequest` / `GenericFamily` / `Weight` / `Script` | ✅ |
| `StaticProvider`（含部分覆盖，可测回退） | ✅ |
| `FontStack`：回退链缓存、逐字覆盖检查、`FallbackOutcome` 三态、统计计数 | ✅ |
| `StackMeasure`（接 `ui_core::TextMeasure`） | ✅ |
| `ui-text-fontique` adapter（fontique 形状 → 我们的 trait） | ✅ |
| 自写 `cmap` 探测（format 4 / 12） | ✅ |
| 真实字形光栅（`GlyphCache`：skrifa 轮廓 → zeno 覆盖率 → 货架图集） | ✅ |
| 真实度量（`head`/`hhea` 读 ascent/descent/line_gap，`hmtx` 读 advance） | ✅ |

**59 个测试**（默认特性 46 + 系统字体 13），零警告。

## 5.1 adapter 实测（本机，`--features ui-text-fontique/system`）

```text
provider: fontique-system     families: 171
SansSerif -> MiSans VF        Serif -> Noto Serif CJK SC    Monospace -> Maple Mono CN

具名族（精确查）：
  MiSans VF        -> face 107 ✓
  Arial            -> not found      ← 本机没装，不偷换
  NoSuchFamily     -> not found

脚本回退链：
  Latn -> ["MiSans VF"]   Hani -> ["MiSans VF"]
  Arab -> ["Arial"]       Thaa -> ["MV Boli"]

字形覆盖（读真实 cmap）：
  'a' ✓  'Z' ✓  '中' ✓  'م' ✗  '🦀' ✗
```

## 5.2 adapter 阶段发现并修掉的两个问题

**问题 1：静默替换掩盖拼写错误。**
初版 `resolve` 在具名族查不到时落回通用族，于是 `Arial` 和 `NoSuchFamily` 都返回 MiSans VF——
**拼错的族名被伪装成"找到了"**。改成：具名族一个都不匹配就返回 `None`。
通用族才走平台默认。（这与 §2 的 fontconfig 对比一致：精确查名 vs 最佳匹配是两种语义。）

**问题 2：`has_glyph` 查错了对象。**
初版遍历"族内任一字面覆盖该字符"，但 `FaceId` 只是族的代表，不是具体字面。
改成直接探测该字面的字体字节（`face_data` → `cmap`）。
实测生效：`'م'`（阿拉伯文）在 MiSans VF 上正确报 `false`，触发回退到 Arial。

**回退逻辑已用 `StaticProvider` 验证过**：拉丁文本留在首选字面，中文回退到下一个，
空白不触发回退，无覆盖时留在首选。

---

## 5.3 字形管线实测

```text
run "Hello, crab" on Some("MiSans VF")
  metrics: upem=1000 ascent=1.044 descent=-0.282 line_gap=0.000
run "中文字体测试" on Some("MiSans VF")
run "Sans 混排" on Some("MiSans VF")

glyphs rendered: 22  atlas: 1024x1024
cache: CacheStats { hits: 2, misses: 22, rasterized: 20, evictions: 0 }
```

管线四段：`cmap` 取 gid → `skrifa` 取轮廓 → `zeno` 光栅成覆盖率 → 货架算法装图集。
中英文混排、`hmtx` 真实前进宽度、`hhea` 真实行高全部生效。

### 这一段踩到的三个坑（都已修）

| 坑 | 症状 | 根因 |
|---|---|---|
| **双重缩放** | 32px 的 'A' 只有 4 像素宽 | `skrifa` 的 `Size::new(px)` 已经把坐标转成像素，我的 pen 又乘了一次 `px/upem` |
| **硬编码基线** | 两行文字重叠 | 用了 `0.8 × size` 而不是 `hhea.ascender × size`（本机字体是 1.044） |
| **笔位取整** | 字距不均（`s`、`a` 挤在一起） | 我"修"亚像素漂移时把 `pen_x` 每步 `.round()`，反而把小数前进量截断。**去掉取整才是对的** |

第三个坑值得记：**加取整看起来像是修复，实际是引入 bug**。判断依据只能是渲染结果，
不是推理。

## 6. 待决问题

1. **`fontique` 要不要立刻接**：它引入 42 个依赖（含 `windows`/`objc2`/`memmap2`）。
   建议**先接 Linux（fontconfig）**，Windows/macOS 留到真机验证时再接——
   `ui-text-fontique` 的 `system` feature 各平台独立，可以只开一个。
2. **`advance` 的精度**：整形器（parley/harfbuzz）落地前，中文/阿拉伯文的宽度是估算。
   影响布局（会略窄或略宽），不影响正确性。要不要提前接整形器？
3. **字体文件句柄的生命周期**：`face_data` 返回 `Arc<[u8]>`，每次调用都读盘还是缓存？
   建议在 adapter 里缓存，`Arc` 让共享零成本。

---

## 7. 文本整形（swash）

逐字符排版换成真正的整形后，有三件事立刻变对：

| 现象 | 逐字符（旧） | 整形（新） |
|---|---|---|
| `AV` 的字距 | 两个 advance 直接相加，中间空得刺眼 | kerning 生效，收窄 |
| `office` 的 `ffi` | 三个独立字形 | 连字合成一个字形 |
| `中文字体测试` | 每个字按 0.6em 估宽 | 每个字按真实 1.0em，总宽 240 = 6 × 40 |

实测（MiSans VF，40px）：

```text
AVATAR Wave      glyphs=11 width= 251.44
office fjord     glyphs=12 width= 201.24
中文字体测试           glyphs= 6 width= 240.00
Sans 混排 text     glyphs=12 width= 260.76
```

### 7.1 `Shaper` 是消耗性的

`swash` 的 `shaper.shape_with(...)` **消费 `self`**。所以：

- `ShapeContext` 可复用（缓存内部数据）
- `Shaper` 不可复用，每次整形新建一个

这一点在 API 上不明显，第一次写会撞上 `E0382 use of moved value`。

### 7.2 字形带 `x`/`y` 偏移

`ShapedGlyph` 必须携带 `x` 与 `y`：组合标记（`mkmk`）、阿拉伯文的上下标都靠它定位。
只存 `advance` 会把它们全部叠在笔位上。

---

## 8. 图集扩容

初版：单页满了就 `entries.clear()` + 清空覆盖率 + `evictions += 1`。**屏幕上已有的字会瞬间消失。**

改进：先扩页，只有扩到上限后第二次溢出才淘汰。

**踩到的坑**：第一次只加宽不加高。64×64 的页面第二行就溢出，加宽无济于事，
于是 `grow()` 被反复调用直到上限，然后照样淘汰——测试直接抓到了：

```
assertion failed: growth must happen before any eviction  left: 1  right: 0
```

修法：**同时加宽和加高**。每行左对齐，所以加宽不移动已有槽位，加高只在下方补空白行。
两条测试固定该行为：

- `atlas_grows_instead_of_evicting_when_a_page_fills`
- `grow_preserves_existing_glyph_content`（扩容前后同一字形取出的覆盖率必须逐字节相同）

---

## 9. 基线与垂直定位（用户发现的真实 bug）

**用户报告**：「字符的垂直位置没有正确处理（也许是叫做 baseline？）」

### 9.1 现象

渲染结果看起来"对"，但排版细节是错的：

- 大写字母整体下沉，比基线低了约 **30px**（40px 字号）
- 行距因此不匀：`29→82→130→184` 应为等距

用像素扫描量出来的证据（修复前）：

```text
行0: y=58..88      行1: y=104..108   行2: y=111..142
行1 被拆成两段 —— 'fjord' 的 j 落得太低，与下一行几乎相接
```

### 9.2 根因

自写光栅器里 `top` 的语义算错了：

```rust
// 错的：
let top = (y1 + pad).ceil();     // y1 是 y-down 空间的"底部"
```

`bounds` 已在屏幕空间（y 向下增长、基线在 y = 0），所以：

- 字形**顶**是**最小**的 y（`y0`）
- 字形**底**是**最大**的 y（`y1`）

`top` 想让渲染器表达的是「基线到比特图顶边的距离」，于是应该是 **`-(y0 - pad)`**，
而我用了 `y1 + pad` —— 恰好是底部那一侧。

`'A'` 在 40px 下真实 cap height ≈ 29px，实测 `top = 1`：**差了整整一个 cap height**。

### 9.3 修复

```rust
let top = -((y0 - pad).floor());   // 顶: 从基线往上
let bottom = (y1 + pad).ceil();    // 底: 从基线往下
```

修复后：

```text
ch      top      h   bottomRel   expect
A        31     32        1.0       ~0      ← 坐在基线上
H        31     32        1.0       ~0
j        32     42       10.0       >0      ← 降部正确垂下
g        23     33       10.0       >0
中        35     39        4.0       ~0
```

### 9.4 三条测试固定该不变量

| 测试 | 断言 |
|---|---|
| `cap_letters_sit_on_the_baseline` | `H` 的底边距基线 ≤ 2px，且 cap height 在 0.6–0.8 em |
| `descenders_hang_below_the_baseline` | `j` 的底边在基线下方 > 5px |
| `taller_size_gives_larger_cap_height` | 字号翻倍，cap height 也翻倍（±25%） |

**为什么之前没抓到**：我之前的测试只验证"光栅出了非空覆盖率"和"尺寸随字号变化"，
**从不检查字形相对基线的位置**。这是个典型的"测了存在性没测正确性"。

这条也该记进 AGENTS.md §1.5：**渲染正确性必须包含几何位置断言，不只是"画出来了"。**

---

## 10. 亚像素定位（用户报告的抖动）

**用户报告**：「底部字符计数的数字变化时，其文本发生变化，与此同时后面的字符的渲染存在观感问题：
前面字符的宽度发生微小变化时，后面的字符出现明显的位置抖动。当前字体渲染是否没有正确进行亚像素处理？」

**是的，而且比"没做亚像素"更糟：是直接丢弃小数部分。**

### 10.1 先查成熟框架，而不是自己发明

按 AGENTS.md §2，先读源码：

| 框架 | X 变体 | Y 变体 | 出处 |
|---|---|---|---|
| **egui/epaint 0.36.2** | **4** | 1（整数） | `text/font.rs` 的 `SubpixelBin`，注释注明"taken from cosmic-text" |
| **GPUI 0.2.2** | **4** | **1** | `SUBPIXEL_VARIANTS_X = 4`、`SUBPIXEL_VARIANTS_Y = 1`（`text_system.rs:49,52`） |
| cosmic-text | 4 | 1 | egui 的实现来源 |
| swash | API 支持任意 offset | | `Render::offset()`，文档明说"你大概想量化" |

**三家共识**：水平 4 相位，**垂直不做亚像素**。

egui 还多一条我原本想不到的约束（`text/font.rs:622-628`）：

```rust
let (h_pos_round, bin) = if self.subpixel_binning && !is_cjk {
    SubpixelBin::new(h_pos)
} else {
    // CJK scripts contain a lot of characters and could hog the glyph atlas
    // if we stored 4 subpixel offsets per glyph.
    (h_pos.round() as i32, SubpixelBin::Zero)
};
```

**CJK 要排除**——数千汉字 × 4 相位会主导图集。

### 10.2 根因

`draw_glyph` 把分数坐标直接截断：

```rust
let dst = self.to_device(prim.rect);        // x 可能是 123.47
let x0 = dst.min_x().floor() ... as u32;    // → 123，0.47 被丢掉
let off_x = prim.atlas.min_x() as i32 - dst.min_x() as i32;
```

后果：真实 0.95px 的位移被吞掉，累计越过整数边界时突然跳 **1 整像素**。

### 10.3 实现（按共识）

1. `ui-text/src/subpixel.rs`：`SubpixelBin::split(pos) -> (i32, SubpixelBin)`，
   用 `pos * 4` 取整后 `div_euclid` / `rem_euclid` 拆分，负数也正确。
2. `GlyphCache` 键加相位：`(FaceId, gid, size_bucket, phase)`。
3. `ParsedFace::rasterize_subpixel(gid, px, dx)`：等价于 swash 的 `Render::offset`。
4. `place_run_scaled(run, pen_x, baseline_y, use_subpixel, scale)`：统一落位入口。

### 10.4 一个连带发现的隐患：分数缩放下量化必须发生在设备空间

初版在**逻辑**空间量化，渲染器再乘 scale 后 `floor()`。实测：

```text
scale 1.6 时设备坐标的小数部分 = [0.0, 0.6, 0.2, 0.8, 0.4, 0.0]
```

**逐字不同**，所以 `floor()` 会吞掉不同量 —— 即使做了 4 相位，分数缩放下抖动依旧。

修法两条同时需要：

- `place_run_scaled` 接受 `scale`，在**设备**空间量化；
- 渲染器 `draw_glyph` 用 `round()` 而非 `floor()`，
  否则 `device/scale*scale` 的 1-ulp 误差会把整数截掉。

### 10.5 实测证据

`cargo run -p ui-text-fontique --example jitter_probe --features system`：

```text
      text     true x     int x    err int    err sub
   3 chars     29.466        29     -0.466     +0.034
   4 chars     29.453        29     -0.453     +0.047
   5 chars     29.622        30     +0.378     -0.122
   7 chars     28.621        29     +0.379     -0.121

max error: integer 0.466px -> subpixel 0.122px
improvement: 3.8x
phase distribution (0, 1/4, 1/2, 3/4): [5, 6, 7, 7]
```

**关键数字**：相邻状态 `3 chars` → `5 chars`，整数取整误差从 −0.466 跳到 +0.378，
**摆幅 0.844px**（这就是肉眼可见的抖动）；亚像素下只有 +0.034 → −0.122，**摆幅 0.156px**。

0.122px ≈ 1/8 像素，正是 4 相位的理论误差上界（±1/8）。

### 10.6 测试

| 测试 | 断言 |
|---|---|
| `split_rounds_to_the_nearest_quarter` | 0.9 → 下一整像素相位零；0.13 → 相位一 |
| `split_handles_negative_positions` | 负数用 euclidean 拆分，−1.4 → (−2, ¾) |
| `recombine_is_within_a_quarter_pixel_of_the_input` | 800 个采样点，误差 ≤ 1/8 px |
| `accumulated_advances_do_not_jump_a_whole_pixel` | 10.37px 前进量下每次只跳 10 或 11 |
| `different_phases_rasterize_different_bitmaps` | 相邻相位位图必须不同（否则偏移没进光栅器） |
| `each_phase_gets_its_own_cache_entry` | 4 个相位 4 个槽位 |
| `phase_zero_matches_the_unphased_bitmap` | 相位零与无相位逐字节相同 |
| `scaled_placement_lands_on_whole_device_pixels` | 1.0/1.25/1.5/1.6/2.0 下设备原点往返精确 |
| `cjk_scripts_skip_subpixel_phases` | 拉丁/阿拉伯启用，汉字/假名/谚文不启用 |

### 10.7 代价

图集占用最多 ×4（egui 对 CJK 关闭正是为此）。当前 `GlyphCache` 从 1024² 起步、
按需翻倍到 4096²，配合第 8 节的扩容逻辑可以承受。

---

## 11. 1.6 倍下模糊 + 基线错位（用户第二次报告）

**用户报告**：「1.6倍下文字模糊且baseline再次出现问题」

上一节做完亚像素后，1.0 倍是对的，但 1.6 倍同时坏了两个地方。

### 11.1 两个症状，一个根因

| 症状 | 直接原因 |
|---|---|
| **模糊** | `slot.width` 没除以 `scale` 就当作逻辑宽度 → `to_device` 又乘一次 → `dst.size != atlas.size` → 渲染器**掉进插值分支** |
| **基线错位** | 字形按 `size`（逻辑 13px）光栅，但位置按设备坐标（20.8px 空间）算 → `slot.top` 与基线不同量纲 |

共同根因：**光栅字号用了逻辑字号，位置用了设备坐标。**

```text
scale 1.6: 光栅字号=13.0px  应为=20.8px  差距 1.60x
```

字形只有应有高度的 1/1.6，却按 1.6 倍的行高和 advance 排布。

### 11.2 修法：把设备/逻辑换算收进一个函数

调用点原来自己做这套算术，两处都算错了。改成 `GlyphCache::blit_run`：

```rust
pub fn blit_run(&mut self, face, placed, size: f32, scale: f32) -> Vec<GlyphBlit>
```

它保证三件事：

1. **按 `size * scale` 光栅**（设备分辨率）；
2. `dest` 是**设备像素**；
3. `dest.size == atlas.size`，因此渲染器永远走 1:1 贴图路径，不插值。

调用方只需 `blit.dest_logical(scale)` 拿逻辑矩形给图元，换算只此一处。

### 11.3 三条测试固定该不变量

| 测试 | 断言 |
|---|---|
| `blit_run_is_one_to_one_at_any_scale` | 1.0/1.25/1.5/1.6/2.0 下 `dest.size == atlas.size`（不等就是模糊） |
| `blit_run_rasterizes_at_device_resolution` | 2 倍缩放下字形高度是 1 倍的两倍（±20%） |
| `scaled_placement_lands_on_whole_device_pixels` | 设备原点往返精确（上节已有） |

### 11.4 教训

这是同一个盲区的第三次：**"坐标属于哪个空间"没有在类型上区分。**

- 第 9 节：`top` 用错了 y 方向
- 第 10 节：量化发生在逻辑空间而非设备空间
- 本节：光栅字号是逻辑的、位置是设备的

三次都是裸 `f32` 在两种空间之间流动，编译器无从检查。
`GlyphBlit { atlas, dest }` 把"atlas 纹理坐标 / 设备像素"写进字段名与文档，
配合 `dest_logical(scale)` 收口，才让调用点无法再错。

**推论**：凡是涉及两种单位的算术，都应该有一个类型或一个函数承担换算，
而不是散落在调用点。这是 Rust 类型系统能帮上忙的地方，我先前没用。

---

## 12. 双向文本（bidi）

到这一节之前，阿拉伯文和希伯来文会**整体反序显示**——这是唯一还会让文本**读错**的缺口。

### 12.1 先看成熟框架怎么分工

| 来源 | 做了什么 | 没做什么 |
|---|---|---|
| **parley_engine 0.12** `bidi.rs` | **自己实现完整 UAX#9**，915 行 | — |
| **unicode-bidi 0.3.18** | levels（逐字节）、`reorder_visual`（L2） | **明确声明不做 L3/L4**："that should be handled by the engine" |
| **swash 0.2.10** | 按方向选上下文形态 | **不做 bidi**；且 RTL 也返回逻辑序 |

**结论**：UAX#9 有 900 行量级、极多边界情形，**手写不现实**。
用 `unicode-bidi` 拿 levels + L2，`unicode-bidi-mirroring` 补 L4，`swash` 做逐段整形。

### 12.2 一个必须实测才知道的事实

`swash` 的 `ShaperBuilder::direction()` **只影响上下文形态，不重排字形**。实测：

```text
arabic RTL  clusters = [(0,2), (2,4), (4,6), (6,8), (8,10)]   ← source 递增
arabic LTR  clusters = [(0,2), (2,4), (4,6), (6,8), (8,10)]   ← 完全相同
```

两种方向返回**同样的逻辑序**。所以 RTL 段必须由调用方 `reverse()`。

**如果只信 API 名字（`direction`）而不实测，这里会静默出错**——文本会整体反序。

### 12.3 管线

```text
itemize(text, base)          → 逐字节 level → 按字符边界切成 LevelRun
shape_directed(run, dir)     → swash 整形（形态按方向）
if rtl: glyphs.reverse()     → 转成视觉序
if rtl: mirror_brackets()    → UAX#9 L4
visual_order(runs)           → UAX#9 L2，段级重排
```

`Shaper::shape_bidi()` 返回**视觉序**的段，每段字形也是视觉序，
所以调用方只需一个从左到右的笔位累加 `pen_x`，**无需任何 RTL 特判**。

### 12.4 L4 镜像：`unicode-bidi` 明确不管

不做镜像时，`(مرحبا)` 的字形 id 实测为：

```text
修复前 gids=[12, 910, 914, 931, 942, 995, 11]   ← 左端是 ')' 的字形
修复后 gids=[11, 910, 914, 931, 942, 995, 12]   ← 左端是 '(' 的字形
```

括号位置正确交换了，但**字形没换**，视觉上就是 `)文本(`。

修法：对 RTL 段的每个字形，用 cluster 找回源字符，查 `unicode_bidi_mirroring::get_mirrored()`，
命中就换成镜像字符的字形 id。

### 12.5 实测渲染

`cargo run -p ui-platform-wayland --example text_bidi --features system-fonts`

```text
Latin only: hello                  base=LTR  runs=1
Arabic: مرحبا بالعالم              base=None runs=2   ← 拉丁标签 + RTL 段
Hebrew: שלום עולם                  base=None runs=2
Mixed: abc مرحبا xyz               base=LTR  runs=3
Mixed RTL base: abc مرحبا xyz      base=RTL  runs=3   ← 段序反转
Mirroring: (abc) vs (مرحبا)        base=None runs=3
```

阿拉伯文与希伯来文正确从右到左，混排时 LTR/RTL 段位置正确，
RTL 基方向下整行段序反转（`xyz` 出现在最左）。

### 12.6 测试

| 测试 | 断言 |
|---|---|
| `pure_latin_is_a_single_ltr_run` | 单段、level 0 |
| `pure_arabic_is_a_single_rtl_run` | 单段、奇数 level |
| `mixed_text_splits_into_alternating_runs` | 段无缝隙铺满文本 |
| `runs_never_split_a_multibyte_character` | 段边界落在字符边界（levels 是逐字节的） |
| `rtl_base_lifts_ltr_text_to_level_two` | UAX#9 I2：RTL 段落里 LTR 字符升到 level 2 |
| `base_direction_changes_the_visual_run_order` | 同一文本两种基方向，视觉序必须不同 |
| `bidi_rtl_run_has_its_glyphs_reversed` | 视觉序首字形的 cluster 必须大于末字形 |
| `bidi_rtl_brackets_are_mirrored` | RTL 段最左是 `(` 字形，最右是 `)` |
| `bidi_ltr_brackets_are_left_alone` | LTR 段不做镜像 |

### 12.7 仍未做的

- **L3（组合字符重排）**：`unicode-bidi` 同样不管；需要时再加。
- **逐字簇级回退**：一段内混多种脚本时仍取 `Script::dominant`，
  真正的逐簇字体选择属于 itemize 的下一步。
- **换行**：当前 bidi 按整段处理，没有行内重排。

## 字重与斜体：请求曾被完整忽略

`FontRequest` 从第一天起就有 `weight` 与 `italic` 字段，`FontProvider::resolve` 却
**从不读取它们**：每个族只登记一个槽位，返回该族的默认字面。

三处独立缺陷叠在一起，任何一处单独修都不够：

| 层 | 缺陷 | 症状 |
|---|---|---|
| `TextStyle` | 只有 `size` 与 `color` | 调用方无法表达字重 |
| `resolve` | 忽略 `request.weight` / `.italic` | 任何字重都得到同一字面 |
| `face_data` | 永远取 `fonts().first()` | 即使选对了字面，加载的仍是 Regular 字节 |

### 修法

1. **`Weight` 归入 `ui-core`**：`ui-text` 依赖 `ui-core`，字重定义在 `ui-text` 会让
   `TextStyle` 无法携带它。现在只有一个类型（之前两个同名不同类型正是静默忽略的温床）。
2. **`FaceId` 标识字面而非族**：槽位表按 (族, 族内下标) 展开，
   `face_data` 用槽位自己的 `font_index` 加载。
3. **匹配交给 fontique**：`Family::match_index(width, style, weight, synth)`，
   不自己写「取第一个」。通用族路径同样要传请求——不带具名族是最常见的情形。
4. **每样式一条栈**：`TextPass::stack_for(weight, italic)` 按 (字重, 斜体) 解析并缓存；
   覆盖不足时退回基础栈（**丢字形比丢字重更糟**）。
5. **测量也用整个样式**：粗体比常规宽，按常规测量会让换行后的行溢出。

### 不做仿斜

族没有斜体字面时返回正体，不合成倾斜。这是有意的默认，测试据此断言。

### 验证

| 测试 | 钉住的行为 |
|---|---|
| `bold_and_regular_resolve_to_different_faces_and_bytes` | 字重到达 `resolve`，两者不是同一 `FaceId` |
| `italic_resolves_to_an_italic_face_when_the_family_has_one` | 斜体到达 `resolve`；族有斜体时必须取到 |
| `a_bold_run_resolves_a_different_face_than_a_regular_one` | 带样式的栈与基础栈不同，且缓存稳定 |

三条都验证过「退回旧行为则变红」。

**注意**：前两条需要 `--features system`（否则 `system_or_isolated()` 给出空集合，
测试无从判断）。没有字体时它们打印 `skipped: ...` 而不是静默通过——
静默跳过的测试与通过的测试看起来一模一样，这是之前踩过的坑。

### 踩坑记录

- 「字节长度不同」是错的断言：**可变字体**是同一个文件靠下标区分字面，
  长度相同完全正确。断言应落在 `FaceId` 上。
- 斜体测试一开始从 `HashMap` 迭代里挑族，**结果不确定**（同一构建时绿时红）。
  改为排序后取第一个，并去掉 `let Some else return` 这类静默早退。

### 后续修正：还有两个缺陷

第一版修完后标题**视觉上仍不加粗**，追下去发现另外两处：

| 缺陷 | 症状 |
|---|---|
| `script_face` 只按脚本缓存 | 常规 run 先把字面缓存住，粗体 run 查找"成功"并直接复用它——字重被静默丢弃 |
| `Shaper::new(data, 0)` 永远用下标 0 | 对**集合字体**（TTC，多个字面打进一个文件）解析出的永远是第一个字面 |

修法：

- 缓存键改为 `(Script, weight, italic)`。
- `FontProvider::face_index(face)` 新增，`Shaper` 用它解析。

### 「族内下标」与「文件内下标」不是一回事

`face_index` 第一版直接返回槽位里的 `font_index`，结果 Calibri 的粗体又变回常规宽度——
因为槽位记的是字面**在族内的位置**，而解析器要的是它**在文件内的位置**。
TTC 里两者不同，混用会静默加载错误字面。现在通过 `FontInfo::index()` 取后者。

### 可变字体：实例即 `FaceId`

可变字体是**一个**字面配一条连续轴（MiSans VF 是 `wght 150..700 default 330`），
所以「粗体」不是另一个字体，而是同一文件在另一个 `wght` 上。

做法：把实例编码进 `FaceId`——按 100 步长预注册一条字重阶梯，每个档位一个槽位。
这样整形器、光栅器、图集**无需新增键维度**就自动正确；若改为在别处传轴值，
就要给每个缓存都加一维，漏一处就是静默错误。

轴值必须在**两侧**同时应用：

| 侧 | 位置 | 漏掉的后果 |
|---|---|---|
| 整形 | `Shaper` 的 `.variations([("wght", w)])` | 前进宽度来自默认实例 |
| 光栅 | `ParsedFace` 的 `LocationRef` | 画出默认实例的轮廓 |

实测（24px）：

| 族 | regular | bold | 说明 |
|---|---|---|---|
| Tahoma | 180.39 | 208.69 | 独立粗体文件 |
| Sitka | 195.26 | 207.55 | 可变字体，轴生效 |
| MiSans VF | 189.92 | 189.74 | 可变字体，**CJK 等宽所以宽度不变** |
| Maple Mono | 216.00 | 216.00 | 等宽族（正确） |

**CJK 字体的前进宽度在任何字重下都相同**，因此宽度无法判断轴是否生效——
测试必须比较**光栅出的覆盖率**。这条差点又变成「测试绿着但什么也没证明」。

样例程序：拉丁标题用 Tahoma（真实 Bold 文件），中文行用 MiSans VF 的实例。

### 端到端断言

`a_bold_run_is_wider_than_a_regular_one` 测**宽度**而不是 FaceId：
选了不同 `FaceId` 但仍解析同一文件字节 0 时，整形结果完全相同——
这正是本轮的 bug，只有宽度能抓住它。
