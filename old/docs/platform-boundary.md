# 平台边界：现状、保证方式与剩余耦合

> 本文回答一个问题：**后续开发能否确保与平台解耦，从而暂缓 Win32 接入？**
> 答案是「大部分能，且已用检查固化」，但有三处已知耦合必须写清楚。

---

## 1. 已实测的解耦程度

最强的证据不是"设计上解耦"，而是**真的交叉编译过去了**：

| crate | 交叉编译到 `x86_64-pc-windows-gnu` | 说明 |
|---|---|---|
| `ui-core` | ✅ | 零依赖 |
| `ui-text` | ✅ | 只依赖 `ui-core` + 纯 Rust 的 unicode crates |
| `ui-paint` | ✅ | 只依赖 `ui-core` + `zeno` |
| `ui-platform` | ✅ | 只依赖可移植 crate |
| `ui-text-fontique` | ✅ | `fontique` 内部用 `cfg` 选 fontconfig/DirectWrite，对本仓库透明 |
| `ui-paint-gpu` | ❌ | **EGL/GBM，Unix 专属**（见 §3.1） |
| `ui-platform-wayland` | ❌ | Wayland，预期内 |

并且在 Wine 下：DirectWrite 枚举到 160 个字体族，文本栈 42 个测试串行全过。

---

## 2. 保证方式：三条会失败的检查

边界**不靠自觉**维持。`ui/crates/ui-platform/tests/platform_boundary.rs`：

| 检查 | 断言 |
|---|---|
| `portable_crates_use_no_platform_specific_apis` | 可移植 crate 源码里不得出现 `std::os::unix/windows`、`cfg(target_os)`、`libc::`、`OwnedFd`、`std::process` |
| `portable_crates_do_not_depend_on_platform_crates` | 可移植 crate 的 `Cargo.toml` 不得依赖平台 crate |
| `every_abstraction_crate_is_classified` | 新 crate 必须在 `PORTABLE` 或 `PLATFORM_BOUND` 里，否则报红 |

**三条都做过"故意污染 → 变红 → 恢复 → 变绿"**：

```text
往 ui-core 塞 #[cfg(unix)]          → crates/ui-core/src/lib.rs:29 uses `cfg(unix`  FAILED
往 ui-core/Cargo.toml 加 wayland 依赖 → ui-core/Cargo.toml depends on ui-platform-wayland  FAILED
新建未分类 crate                     → neither in PORTABLE nor PLATFORM_BOUND: ["ui-future-thing"]  FAILED
恢复                                → 3 passed
```

这比 `arch.toml` 之类的声明式配置更直接：它读的是**真实源码与真实清单**，
而且随 `cargo test` 一起跑，不会被忘记。

> 注：本仓库早期 `AGENTS.md` §3 声称 `arch.toml` + `cargo xtask arch` 是权威，
> §5 引用 `scripts/*.sh`——**这些文件从未存在**。已改为指向这三条检查与真实命令。

---

## 3. 三处已知耦合（暂缓 Win32 前必须知道）

### 3.1 `ui-paint-gpu` 是具体类型，没有 trait

`GpuRenderer` / `GpuTarget` 直接暴露，没有"渲染后端"抽象。
现在加 Windows GPU 后端（D3D12）只有两条路：

- 复制一份 `ui-paint-gpu-windows`，然后在调用处二选一 → 会分叉
- 先抽出 trait（`trait Renderer { fn render(&mut self, list) -> Stats }` + `trait Surface`）→ 干净

**建议**：真正做 Windows GPU 时先抽 trait。现在不做，因为只有一个实现时抽 trait 是
过早抽象（AGENTS.md §2：参考成熟框架解决的问题，不是提前设计）。

### 3.2 `PresentTarget` 里"呈现"与"绘制"的耦合（已部分解除）

`AppLoop` 现在拆成两半：

| 方法 | 职责 | 需要后端吗 |
|---|---|---|
| `build(size, pointer)` | view → apply → layout → paint → **文本解析** → 脏区 → 命中 | 否 |
| `frame(target, pointer)` | `build` + CPU 渲染 + `present` | 是 |

文本解析在 `build` 内完成，所以**两个后端看到同一条字形流**。
GPU 后端可以直接用 `build()` + `draw_list()` + `text_pass()`，不必碰 CPU 渲染器。

剩余耦合仍在 `PresentTarget::pixels_mut()`：GPU 目标没有 CPU 像素，仍需按下面拆 trait。

### 3.2b `PresentTarget` 假定 CPU 像素

```rust
pub trait PresentTarget {
    fn pixels_mut(&mut self) -> &mut Pixels;   // ← GPU 目标没有 CPU 像素
    fn present(&mut self, damage: &Damage) -> Result<(), String>;
}
```

CPU 路径（`OffscreenTarget`、`WaylandWindow` 的 shm 路径）用得很顺，
但 GPU 零拷贝目标套不进去——`window_gpu` 示例只能绕过这个 trait 直接调 `present_dmabuf`。

**这是目前最实质的架构债。** 修法是把 trait 拆成两层：

```rust
trait Surface  { fn size(&self) -> (u32, u32); fn present(&mut self) -> Result<(), String>; }
trait CpuSurface: Surface { fn pixels_mut(&mut self) -> &mut Pixels; }
```

这样 GPU 后端只实现 `Surface`，CPU 后端实现 `CpuSurface`。

### 3.3 `ui-text-fontique` 的测试样本曾是 Linux 专属

已修（`fc-match` → 平台 provider），并加了哨兵测试防止再次空跑。
但要注意：**回退数据本身是平台相关的**，Wine 下阿拉伯文回退是 0 个 face。
所以「文本布局在别的平台上是否一致」只有真机能答。

---

## 4. 结论

**能暂缓 Win32 接入。** 理由：

1. 可移植层（`ui-core`/`ui-text`/`ui-paint`/`ui-platform`/`ui-text-fontique`）
   有交叉编译 + Wine 运行的实证
2. 边界由三条会失败的检查固化，不会因为后续开发悄悄腐化
3. 剩下三处耦合都是**已命名、已定位**的，不会在接入 Win32 时才意外发现

后续开发只要遵守一条规则就不会破坏解耦：
**新代码要么进可移植 crate（不得碰 OS API），要么进 `PLATFORM_BOUND` 并写明理由。**

真正要动 Windows 时，按此顺序：

1. 先按 §3.2 拆 `PresentTarget`
2. 再写 `ui-platform-win32`（Win32 窗口 + DXGI 呈现）
3. GPU 后端视需要按 §3.1 抽 trait
4. 真机复核：字体回退、DirectComposition 行为、并发初始化 DirectWrite（§docs/gpu-backend.md 8.3.4）

---

## 5. 每次提交前的验证

```bash
# 平台边界（快，纯文件扫描）
cargo test -p ui-platform --test platform_boundary

# 全量（含可移植层的行为测试）
cargo test --workspace --features ui-text-fontique/system

# 可移植性实证：交叉编译
export RUSTUP_HOME=~/.rustup-win CARGO_HOME=~/.cargo PATH=~/.cargo-win/bin:$PATH
cargo build -p ui-core -p ui-paint -p ui-text -p ui-text-fontique -p ui-platform \
    --features ui-text-fontique/system --target x86_64-pc-windows-gnu --offline
```

---

## 6. 文本进入主管线（2026-10-25）

在此之前 `AppLoop` **画不出真字**：`CpuRenderer::draw_text` 是占位实现（每字符一条竖条），
GPU 渲染器则直接跳过 `PrimKind::Text`。文本栈只在手写示例里验证过。

现在补上了缺失的中间层：`ui/crates/ui-platform/src/text_pass.rs`。

| 步骤 | 说明 |
|---|---|
| `paint_tree` 产出 `PrimKind::Text` | 与后端无关 |
| `TextPass::resolve(&list, scale)` | `Text` → 已定位 `Glyph` + 图集 |
| 两个后端消费同一条 `Glyph` 流 | CPU 取图集覆盖率，GPU 上传纹理 |

### 6.1 三条真机才发现的事

**① `fallback_chain` 返回的 FaceId 可能没有可加载数据。**
本机 fontique 从 fontconfig 拿到了 "Arial"、"DejaVu Sans" 的族名，但背后没有字体条目：
`face_data()` 返回 `None`，精确查找也失败（DejaVu 根本没装，fontconfig 索引是残留）。
所以字体栈里的名字**只有在字节真能加载时才生效**，不能信族名列表。

**② 单一通用族不够，必须按段选字面。**
拉丁、CJK、阿拉伯很少由同一字体覆盖。用不覆盖的字面整形只会得到 `.notdef`。
现在按 `itemize` 的逻辑段逐个选"整形后不含 `.notdef`"的字面，并把选择按脚本缓存。

**③ 绕过 `shape_bidi` 会丢掉 RTL 后处理。**
为了按段选字面，一开始改用 `visual_runs` + `shape_directed`，
结果阿拉伯语既不连写、顺序也错。原因是 `shape_bidi` 内部还做了两件事：
RTL 反转字形、UAX#9 L4 括号镜像。已把它抽成公共的 `Shaper::shape_run`
（`ui-text-fontique/src/shape.rs`），两处共用，不再各写一遍。

### 6.2 抓到两个 `AppLoop` 的真实缺陷

`AppLoop` 此前从未驱动过会缩放的窗口，所以这两个问题一直没暴露：

| 缺陷 | 症状 | 修法 |
|---|---|---|
| 尺寸变化不丢弃上一帧 | compositor 把窗口改成 1270x1397 后 `copy_from_slice` panic（478400 vs 7096760 字节） | 尺寸变化时 `previous = None` + 全量脏区 |
| 绘制快于合成器消费 | 帧缓冲里有字，屏上却是旧的（只有卡片没文字） | 绘制**之前** `can_draw()` / `wait_for_frame()` 限速 |

### 6.3 已解：文本内容变化现在会产生脏区

原先 `compute_damage` 比的是节点 `ink_rect()`（几何），改文案而尺寸不变时 `damage == None`，
屏上停在旧帧（示例计数器卡在 `frame 3`）。

现在按**已解析图元**取指纹：每帧从 `self.resolved` 汇总每个所属节点的合并墨迹矩形 + 内容哈希
（矩形、圆角、填充、描边、图集槽位、字形位置、文字内容、字号、颜色全参与）。
指纹相同即认为该节点贡献的像素完全相同。

实测：窗口计数器从卡在 `frame 3` 变为正常递增（`frame 1077`）。

### 6.3b 原先的缺口

`compute_damage` 比的是节点 `ink_rect()`（几何），不看文本内容。
所以改文案而尺寸不变时 `damage == None`，屏上停在旧帧（示例里计数器卡在 `frame 3`）。
修法是把内容纳入脏区判据，而不仅是几何。

### 6.4 验证方式

```text
无头：cargo run -p ui-platform --example apploop_text --features system-fonts
      四种脚本各一行，输出 apploop_text.ppm，肉眼确认字形与 bidi
真窗口：cargo run -p ui-platform-wayland --example window_apploop --features system-fonts
      glyphs=79 text=0 unresolved=0；截图确认连写阿拉伯文在屏上
诊断：  DUMP_FRAME=1 时导出 /tmp/window_frame.ppm，用帧缓冲排除"渲染对但没呈现"
```

---

## 7. 两个后端共享同一条字形流（已验证）

`gpu_text_scene` 用 `AppLoop::build()` 产出**同一份**已解析列表，分别交给
`CpuRenderer` 与 `GpuRenderer`，再逐像素比对：

| 不变量 | NVIDIA RTX 5060 Ti | Intel ARL iGPU |
|---|---|---|
| [1] 字形墨量偏差（限 0.5%） | 0.3018% | 0.3018% |
| [2] 溢出像素（限 0.05%） | 0.0000% | 0.0000% |
| [3] 墨迹质心位移（限 0.5px） | 0.0054px | 0.0054px |
| [4] 逐像素差异 >±8（限 0.2%） | **0.0000%** | **0.0000%** |

**逐像素完全一致**。原因不是"两个后端实现得一样"，而是它们根本不做同样的事：
字形由 `TextPass` 光栅一次进图集，两个后端都只是把同一张图集的同一批图块
按同一组已量化位置贴上去。这就是把文本解析放进管线的收益。

`window_gpu` 也改用 `AppLoop` 驱动（不再手搓 `DrawList`），零拷贝窗口里
拉丁/CJK/阿拉伯语与计数器都正常，`releases ≈ frames - 2` 的双缓冲行为不变。

### 7.1 顺带修掉的一个测量错误

第一版 `gpu_text_scene` 的 `to_rgb` 抄错了 `gpu_scene` 的第二个参数含义：
`gpu_scene` 的 bool 是「是否预乘 BGRA」，不是「是否翻转 Y」。
结果 CPU 侧图像被上下颠倒、且把 BGRA 当 RGBA 读，四条不变量全红（质心偏 100px）。

教训：**测量代码本身也会错**，红色的不变量先怀疑测量，再怀疑被测。
`gpu_scene` 的原实现是对的（无翻转，仅换通道序 + 还原预乘）。

---

## 8. 还没做：离屏层

`LayerEntry { bounds, opacity, radius }` 在绘制表示里已经存在，但**没有任何代码产生层**
（`push_layer` 零调用），两个渲染器也都忽略 `PushLayer`。这是双边工作：

| 侧 | 要做的事 |
|---|---|
| 产生侧 | `paint_tree` 对需要离屏合成的节点发 `PushLayer`（opacity < 1、蒙版，之后是模糊） |
| 消费侧 | CPU 用暂存缓冲、GPU 用 FBO 渲染到纹理，再按 opacity 合成回父层 |

这是"效果是类别"落地的入口，但它是独立的一块，不宜与文本管线混在一起做。

---

## 9. 离屏层（opacity）已落地

`LayerEntry` 早在绘制表示里，但此前**没有任何代码产生层，两个渲染器也都忽略 `PushLayer`**。
现在两端都通了。

### 9.1 语义：组不透明度 ≠ 逐图元 alpha

这是层存在的唯一理由，也是唯一的验收判据：

| 做法 | 两个重叠的不透明子节点，层 50% |
|---|---|
| 逐图元 alpha | 单个 50%，**重叠处 75%**（变暗两次） |
| 组不透明度（层） | 处处 50% |

测试 `group_opacity_does_not_double_darken_overlaps` 钉住前者，`per_primitive_alpha_would_double_darken`
作为对照钉住"层不能长成这样"。两者都在，缺一条就看不出区别。

### 9.2 实现

| 侧 | 做法 |
|---|---|
| 产生 | `ContainerParams.opacity`（`Decl::opacity()`）；`paint_node` 对 `< 1.0` 的容器发 `PushLayer`/`PopLayer`，边界取**子树 ink 并集** |
| CPU | 按深度复用暂存缓冲；合成时整层乘 opacity 后 source-over |
| GPU | 按深度复用 FBO+纹理；合成时画全屏四边形采样层纹理乘 opacity |

`Batch.layers` 从 `Option<u32>` 改成 `Vec<u32>`（完整层链）——只记最内层无法区分
"兄弟层切换"与"进入子层"。

### 9.3 抓到的三个 bug

**① GPU 层采样器从未绑定（静默）**

`end_layer` 用 `uniforms.atlas` 去找 `u_atlas`，但层着色器的采样器叫 `u_layer`。
`get_uniform_location` 返回 `None`，代码 `if let Some(u) = ...` 静默跳过 → **纹理从未绑定**，
采样到残留的字形图集。屏上表现为文字横向拖影、叠着历史帧的计数器。

修法不只是改名字：`Program::new` 现在接收 `required_uniforms`，缺任何一项**启动即报错**。
已用"把 `u_layer` 改名"验证过它会红：

```text
renderer: Gl("shader is missing required uniform `u_layer`")
```

这类错误靠肉眼几乎不可能从画面反推，必须在启动期拦。

**② 测量又错了一次（颜色空间）**

第一版对比把 CPU 的**预乘 BGRA** 直接和 GPU 的**非预乘 RGBA** 比，7 条里红 1 条
（12% 像素"不同"）。实际两边完全一致。

这是本项目第二次被"测量代码自己错"耽误（第一次是 `gpu_text_scene` 的翻转参数）。
`docs/` 里那条"红色的不变量先怀疑测量"不是套话。

**③ 用 `padding` 做位移动画（演示代码的错）**

`window_gpu` 想让卡片滑动，写成 `.padding(Insets { left: x, .. })`。
padding 只内缩子节点，**卡片自己的背景与边框留在原地**，于是看起来"内容整体偏右半个窗口"。

先做了对照：把同样的 padding 加到 CPU 路径的 `window_apploop`，**表现完全一样**——
所以不是后端差异（两个后端吃同一份绘制列表，位置差异本就不可能来自后端）。
正确做法是用前置占位块让整张卡片一起移动。

### 9.4 验证

```text
gpu_layer_scene，NVIDIA 与 Intel 结果一致：
[1] cpu overlap uniform      0.0000      [5] cpu outside untouched   0.0000/255
[2] gpu overlap uniform      0.0000      [6] gpu outside untouched   0.0000/255
[3] cpu opacity accuracy     0.0000/255  [7] cpu/gpu differing pixels 0.0000%
[4] gpu opacity accuracy     0.0000/255  [8] stale layer content      0.0000
```

[8] 单独钉"层不得残留上一帧"：同一方块画在两个不同 x，检查腾出的位置已清空。

真窗口：`window_apploop`（CPU）与 `window_gpu`（GPU 零拷贝）里各有一个 0.55 的组，
屏上都是浅色条带 + 深色文字，与层外内容正确区分。

### 9.5 仍未做

- 层内裁剪与圆角裁剪（`LayerEntry.radius` 已存在但未生效）
- 嵌套层已有实现与测试，但真窗口里只用了一层
- 模糊等效果：层的链路通了，效果类别可以按同一模式接

---

## 10. 圆角曲线族（不只一种圆角）

**结论：能，而且不该做成"圆角的种类枚举"。** 它是一条连续轴上的预设。

### 10.1 对齐 CSS `corner-shape`（权威依据）

CSS Borders and Box Decorations Module Level 4 已标准化 `corner-shape`，
把圆角**大小**（`border-radius`）与**曲线族**（`corner-shape`）分成两个正交属性。

`superellipse(K)` 的曲线是**指数为 `2^K`** 的超椭圆（规范原文：*"it defines a
superellipse using an exponent of 2^K"*）：

| 关键词 | `superellipse(K)` | 指数 | 形状 |
|---|---|---|---|
| `square` | ∞ | ∞ | 直角 |
| `squircle` | 2 | 4 | iOS 超圆角 |
| `round` | 1 | 2 | 普通圆角（默认） |
| `bevel` | 0 | 1 | 直线斜切（菱形角） |
| `scoop` | −1 | ½ | 内凹 |

> 注意：有一篇流传较广的博客写成 `x^(2K) + y^(2K) = 1`（即指数 `2K`）。**那是错的**，
> 规范原文是 `2^K`。按博客写法，`bevel = superellipse(0)` 会退化成无意义的方程；
> 而 `2^K` 下 K=0 恰好给出 `|x|+|y|=1`，正是直线斜切。这个差异只有对照规范原文才能发现。

### 10.2 设计：参数，不是枚举

```rust
pub struct CornerShape { k: f32 }          // 私有字段，只有下面这些入口
impl CornerShape {
    pub const ROUND: Self;                  // k = 1
    pub const SQUIRCLE: Self;               // k = 2
    pub const BEVEL: Self;                  // k = 0
    pub const SQUARE: Self;                 // k = ∞
    pub const fn from_k(k: f32) -> Self;    // 任意 k
    pub fn exponent(self) -> f32;           // 2^k，钳制到 [1, 64]
}
```

命名形状只是预设值，任意 `k` 都能用——这正是"效果是类别而不是种类枚举"的同一原则。

形状挂在 `Corners` 上而不是单独的字段：

```rust
Corners::all(12.0).squircle()                 // 半径与曲线一起走
Corners::all(8.0).with_shape(CornerShape::from_k(1.6))
```

这样调用方**无法只设半径而忘记曲线**，也不需要改动任何既有 `RectPrim` 构造点
（26 处零改动，因为形状随 `radius` 一起传递）。

### 10.3 实现：一条公式覆盖全部形状

圆弧与超椭圆只差一个范数。把圆角盒内的欧氏距离换成 Lp 范数即可：

```text
n = 2^K
norm(x, y) = (x^n + y^n)^(1/n)      // n=1 → 斜切；n=2 → 圆；n→∞ → 直角
```

- CPU：`coverage_rounded_rect` 里 `(dx²+dy²)^½` → `norm(|dx|, |dy|)`
- GPU：`sd_round_box` 里 `length(max(q,0))` → `lp_norm(max(q,0), n)`，指数作为第 17 个实例浮点传入

两者是同一个公式，所以结果能逐像素对上。

### 10.4 验证

`gpu_corner_scene`（NVIDIA 与 Intel 结果一致）：

```text
cpu mass: [204040, 320368, 378200, 399204, 408000]   ← bevel/round/squircle/k=3/square
gpu mass: [203960, 320352, 378200, 399204, 408000]
[1] cpu shapes differ            0.0000  OK   ← 各形状确实互不相同
[2] gpu shapes differ            0.0000  OK
[3] per-shape mass              0.0392%  OK   ← 两后端每形状总量一致
[4] cpu/gpu differing pixels    0.0000%  OK   ← 逐像素完全一致
```

[1][2] 是必要的：只验 [4] 的话，"两后端都忽略指数"会自洽通过。
反向验证：把 GPU 的指数写死成 2.0 → `gpu mass` 全变 320352，[2][3][4] 立刻变红。

`ui-paint/tests/corner_shape.rs` 钉住几何（不是分支）：
圆弧是圆、超圆角比圆填得多、斜切是直线、直角不圆、填充量随 k 单调。

`ui-core` 钉住 `2^K` 映射本身（这是最容易读错的一处）。

真窗口：`window_gpu` 与 `window_apploop` 的卡片都改用 `.squircle()`，屏上角部比圆弧更贴合。

### 10.5 未实现

- **`scoop` / `notch`（k < 0，内凹）**：覆盖率公式假定凸形；内凹需要不同的判定。
  当前把 k 钳制到 0（斜切）而不是产出无意义结果，并有测试钉住这个钳制。
- **每角独立曲线族**：CSS 允许 `corner-top-left-shape` 等逐角设置；当前一个矩形一种曲线族。
  因为形状挂在 `Corners` 上，改成四元组是局部的。
- **圆角裁剪层**：`LayerEntry.radius` 仍未生效（与 §9.5 同一条）。

---

## 11. 裁剪：布局算了，绘制没落实

### 11.1 症状

一个 40×40 的容器里放 100×100 的子节点，红色子节点**直接画到父容器外面**。
实测：父容器之外有 2400 个红色像素。

### 11.2 根因

布局期已经算出每个节点的 `mask = 父 mask ∩ 自身矩形`（`layout_engine.rs`），
但 `paint_tree` **从不发出 `PushClip`**——`push_clip` 在整仓库只有两处调用，都在测试里。

于是 `mask` 只被用作"是否跳过该节点"的空判据，从未进入绘制列表：
`batch.clip` 永远是 `None`，两个渲染器的裁剪逻辑形同虚设。

### 11.3 修法

在 `paint_node` 里按需发出裁剪。关键是**不必每个节点都发**：

- 后代的 `mask` 是本节点 `mask` 的子集，所以后代自己会裁；
- 因此只需检查**本节点自身的墨迹**是否超出 `mask`：

```rust
let clipped = !is_root && mask.intersection(ink) != Some(ink);
```

常见情况下（子节点都在父容器内）不产生任何裁剪，批处理不受影响。

### 11.4 顺带发现的布局 bug：`fixed` 被撑满覆盖

修完裁剪后探针仍显示泄漏，追下去发现更早的问题：子节点声明 `fixed(100.0)`，
布局却给了 160。

原因在交叉轴拉伸：

```rust
// 修前
let cross = cross_measured + (cross_avail - cross_measured).max(0.0) * params.cross_align;
```

`cross_align` 默认 1.0（撑满），这段**完全无视子节点的 `max`**。
而 `AxisSize::fixed(v)` 的语义正是 `min == hint == max == v`——契约被违反。

修法是拉伸后按声明钳制：

```rust
let cross = stretched.min(declared.max).max(declared.min);
```

`hug`/`fill` 的 `max` 是 `∞`，所以默认行为不变；`fixed` 与显式 `max` 成为硬上限。

> 这个 bug 一直被"父容器默认 hug 所以大家一起被撑满"掩盖着，只有在
> 父容器**尺寸受限**时才暴露。

### 11.5 验证

`crates/ui-platform/tests/clipping.rs`：

| 测试 | 断言 |
|---|---|
| `a_child_larger_than_its_parent_is_clipped` | 40×40 父容器外无红色像素 |
| `a_fixed_size_child_keeps_its_size` | `fixed(100)` 的子节点实测 100×100 |

两条各自反向验证过，**只会因自己的原因变红**：

```text
去掉 PushClip          → a_child_larger_than_its_parent_is_clipped  FAILED
去掉交叉轴钳制          → a_fixed_size_child_keeps_its_size          FAILED
```

真窗口：CPU 与 GPU 零拷贝窗口在开启裁剪后外观不变（因为示例内容本来就在界内），
说明裁剪没有误伤正常内容。

### 11.6 仍未做

- **圆角裁剪**：当前裁剪是矩形。圆角裁剪需要把 clip 从 `Rect` 升级为带形状的
  clip 条目（`LayerEntry.radius` 已有形状信息，但 clip 没有）。
- **裁剪与层的交互**：层内的裁剪现在按矩形生效，但层的 `radius` 尚未参与。
- 文本溢出现在会被裁掉而不是溢出——正确，但还没有省略号或换行来配合（见 §12）。

### 11.7 裁剪暴露的第二个潜伏 bug：GPU scissor 的 Y 翻转

接上裁剪后 `gpu_text_scene` 立刻 4 条全红，第二行文字整行消失。

根因不在裁剪本身，而在 GPU 的 scissor：

```rust
// 修前：GL 的 scissor 原点在左下，UI 在左上，所以翻转
let gl_y = (viewport.1 - y - h).max(0.0);
gl.scissor(x as i32, gl_y as i32, w as i32, h as i32);
```

这个翻转是**错的**。本渲染器的投影故意不做 Y 翻转（见 §7：GL 第 0 行 == dma-buf 第 0 行
== 图像顶部），所以设备坐标 y 与 scissor 的 y 是同一个方向，直接传即可。

于是裁剪框被镜像到屏幕另一侧：第二行在 y=28，被裁到 y=87..112 的位置，整行消失。

**为什么一直没发现**：`batch.clip` 在此之前永远是 `None`（§11.2），`scissor` 分支从未执行。
这和 §11.2 是同一个盲区——代码在，但从没被走到。

### 11.8 一个"几乎无效"的裁剪也会有害

修好 scissor 后又发现：文字节点会发出一个高度差 **0.000002px** 的裁剪。

`mask` 是多次求交的累积结果（`parent ∩ rect` 一路乘下来），浮点误差让它比节点自身矩形
小一丝。按精确比较就算"溢出"，于是发出裁剪、打断批处理（3 批变 2 批）。

修法是给判据一个 0.01px 的容差：

```rust
fn exceeds(ink: Rect, mask: Rect) -> bool {
    const SLACK: f32 = 0.01;
    ...
}
```

### 11.9 双后端裁剪不变量

`gpu_clip_scene`：4 条 60×60 的裁剪框分布在 y = 8 / 88 / 168 / 248，
每个框里画 120×120 的橙色矩形。裁剪框**刻意放在多个高度**——这正是能抓住 Y 翻转的地方。

```text
[1] gpu content outside clips     0.0000  (limit 0)     OK
[2] cpu/gpu differing pixels      0.0000%  (limit 0.2%) OK
[3] every band has content        0.0000  (limit 0)     OK
  band centre red: [255, 255, 255, 255]
```

反向验证（把 Y 翻转加回去）：

```text
[2] cpu/gpu differing pixels     15.9375%  FAIL
[3] every band has content        1.0000   FAIL
  band centre red: [18, 18, 255, 18]      ← 上两条空、下两条有内容，正是镜像
```

NVIDIA 与 Intel 结果一致。

> 写这个测试时又踩了同一个测量坑第三次：CPU 的 `Pixels` 是预乘 BGRA，
> GPU 的 `read_pixels` 是直通 RGBA，直接比得到 100% 不同。
> 已加 `straight_rgba()` 统一两侧——这类比较**每次都要显式转换**，不能靠记性。

---

## 12. 圆角裁剪

### 12.1 数据模型：裁剪条目携带曲线族

```rust
pub struct ClipEntry {
    pub rect: Rect,
    pub corners: Corners,   // 半径 + 曲线族；ZERO 表示普通矩形
}
```

`Corners` 已经在 §10 里携带了形状，所以裁剪直接复用，不需要新的形状字段。

嵌套裁剪的合成规则是**有损但明确**的：

| 量 | 规则 |
|---|---|
| 矩形 | 所有裁剪矩形求交 |
| 形状 | 取**最内层**非零半径的裁剪 |

两个超椭圆角求交后不再是超椭圆，所以形状不能求交。取最内层是最接近直觉的近似，
并在代码里写明了这一点。真正的通用解需要模板缓冲（见 §12.5）。

### 12.2 两端实现

| 侧 | 做法 |
|---|---|
| CPU | 逐像素把裁剪的覆盖度**乘**到图元覆盖度上（不是只限制范围） |
| GPU | scissor 限制矩形，片元着色器里再乘 `clip_coverage(v_device)` |

GPU 的共享裁剪 GLSL 只写一份（`shader::CLIP_GLSL`），由 `rect_fragment()` / `glyph_fragment()`
**组合**进源串——两个程序各存一份副本迟早会漂移。组合时把裁剪块插在 `precision` 之后，
因为 GLSL ES 的片元着色器没有默认 float 精度，`precision` 必须最先出现。

### 12.3 什么时候发出圆角裁剪

矩形裁剪只需在墨迹超出 `mask` 时发出（§11.3）。圆角裁剪不行：

> 子节点完全在矩形**内**，仍可能盖住圆角。

所以判据变成「半径非零 **且** 有子节点」，即使没有溢出也发。代价是每个带圆角且有子节点的
容器一个裁剪（示例里每帧 1 个），换来角部正确。

### 12.4 又一处潜伏 bug：CPU 的裁剪坐标空间混用

`batch.clip` 是**逻辑**单位（布局产出），而 CPU 的 `region` 是**设备**像素。
原代码直接 `c.rect.intersection(region)`——**只在 scale = 1 时成立**。

GPU 侧一直是对的（`set_scissor` 里乘了 `scale`），所以这是个只在分数缩放下出现的单边错误。
已改为先把裁剪矩形换算到设备空间再求交，半径同样换算。

> 这是本项目第三类"两个空间混用"的 bug（前两类见 §7 的 Y 方向、§10 的预乘通道）。

### 12.5 验证

`gpu_clip_scene` 现在同时覆盖矩形与圆角裁剪（NVIDIA 与 Intel 一致）：

```text
[1] gpu content outside clips     0.0000  (limit 0)     OK
[2] cpu/gpu differing pixels      0.0000%  (limit 0.2%) OK
[3] every band has content        0.0000  (limit 0)     OK
  rounded corner [18, 20, 26, 255] centre [90, 200, 255, 255]
[4] rounded corner is cut         0.0000  (limit 0)     OK
[5] rounded centre is painted     0.0000  (limit 0)     OK
```

[4] 是关键：角落像素是**背景色**。矩形裁剪会把它留成蓝色，所以这条能单独抓住"只做了 scissor"。

两侧各自反向验证过：

```text
GPU 忽略圆角裁剪 → [2] 0.6771% FAIL, [4] FAIL（角落变蓝）
CPU 忽略圆角裁剪 → [2] 0.6771% FAIL
```

`ui-platform/tests/clipping.rs` 另有一条生产者侧测试
`a_rounded_parent_clips_its_child_to_the_curve`：80×80 的 `squircle` 容器里铺一个 80×80 子节点，
角落必须空、中心必须满。把 `push_clip(mask, corners)` 改回 `push_clip(mask, ZERO)` 它立刻变红。

### 12.6 仍未做

- **嵌套圆角裁剪**：外层圆角被降级成矩形（§12.1）。通用解是模板缓冲：
  把裁剪形状画进 stencil，再用 `GL_EQUAL` 测试。这也能顺带解决任意路径裁剪。
- **层与裁剪的交互**：层内部的裁剪按批次的 clip 生效，但 `LayerEntry.radius`
  仍未参与层的合成（层的圆角边框/裁剪还没接）。
- 圆角裁剪目前逐像素计算覆盖度；CPU 侧在大面积圆角裁剪下会比矩形裁剪慢，
  尚未做包围盒优化（先判矩形，再判圆角）。

---

## 13. 文本换行

### 13.1 断行器：纯函数，注入宽度测量

`ui_text::break_lines(text, max_width, width_of) -> Vec<Line>`，按 UAX #14 换行机会贪心断行
（`unicode-linebreak`，已在本地 registry）。

| 情形 | 处理 |
|---|---|
| 行尾空白 | 不计宽，也不进入行区间 |
| 词间空格 | **计宽** |
| 单段超宽（无换行机会） | 按字符紧急切开 |
| `\n` | 强制断行 |
| 空文本 | 返回一个空行（布局仍需一个行高） |

每段只测量一次（`full` 含尾随空白、`ink` 不含），贪心过程是线性的；代价是不计跨换行机会的
字偶间距，而行数不受其影响。

7 条原子测试全部注入「每字符 10 单位」的假宽度，不依赖任何字体。

### 13.2 测量与绘制必须共用同一实例

第一版让布局和绘制各持一个 `TextPass`（同一配置）。结果 CJK 段落布局算 2 行、绘制排 3 行，
文字叠到了下一行上。

原因不是代码不同，而是**状态不同**：`shape_covering` 会缓存 script → face 的选择，
测量实例与绘制实例的缓存处于不同状态，于是选到不同字面、宽度略异、断行位置就变了。

修法是共用一个实例（`Rc<RefCell<TextPass>>`），`PassMeasure` 持有同一个 `Rc`：

```rust
struct PassMeasure(Rc<RefCell<TextPass>>);
```

**教训**：两个"配置相同"的实例不等于两个"行为相同"的实例。凡是有内部缓存的组件，
要么共用一个实例，要么让缓存不参与决策。

### 13.3 换行宽度必须由布局给出

绘制端不能自己从节点矩形推断换行宽度。`AxisSize::fixed(170)` 的节点，布局按 170 测量，
但如果绘制读的是节点最终宽度（被父容器撑满后是 396），断行就完全不同。

所以布局把实际用的值记在节点上：

```rust
let wrap = available.w.min(sc.width.max);
let m = self.measure.measure(text, *size, wrap);
n.wrap_width = wrap;      // 绘制端复用这个值
```

`TextPrim.max_width` 直接取自 `node.wrap_width`，两端不可能用不同的数。

### 13.4 顺带修的：`hug` 的 max 是无穷

`AxisSize::hug(min, hint)` 的 `max` 是 `f32::INFINITY`，所以列容器的默认交叉轴撑满
会把一个「想窄一点」的文本节点撑到父宽。想真正限制宽度必须用 `fixed` 或显式 `max`。
这不是 bug（与 §11.4 的 `fixed` 契约一致），但写示例时容易踩。

### 13.5 一个被错误归因的 GL 错误

改完裁剪后 `window_gpu` 报：

```text
target (1272, 1397) failed: GL: glEGLImageTargetTexture2DOES: 0x502
```

看起来是 dma-buf 导入失败，实际与 dma-buf 无关。

真因：我把裁剪 uniform 的设置放在了 `use_program` **之前**。没有当前程序时
`gl.uniform_*` 报 `GL_INVALID_OPERATION`，而 GL 的错误是**粘滞**的——它一直留到
下一次错误检查，于是被归因到了 `glEGLImageTargetTexture2DOES`。

修法是把 uniform 设置移进 `draw_rects` / `draw_glyphs` 里 `use_program` 之后。

> 这个 bug 还解释了为什么 `gpu_clip_scene` 当时是绿的：第一个批次设 uniform 失败，
> 但后续批次里前一个程序仍绑着，于是"碰巧"生效。**绿不代表对。**

### 13.6 验证

`ui-platform/examples/text_wrap.rs`（需要 `--features system-fonts`）：

```text
latin  wrap 170.0 box_h 79.6 text_h 79.6  OK     ← 4 行，按词断
cjk    wrap 170.0 box_h 59.7 text_h 59.7  OK     ← 3 行，按字断
wide   wrap 380.0 box_h 21.0 text_h 19.9  OK     ← 1 行，不换
glyphs 107 stray 0
```

两条判据：

1. **没有字形落在所有文本框之外**（`stray 0`）——断行或行高不一致时必然出现；
2. **每个文本框不短于它自己的文本高度**——测量低估时必然出现。

### 13.7 仍未做

- **省略号**：超出 `max_lines` 时应截断加 `…`，现在只是继续换行。
- **对齐**：只支持左对齐；居中/右对齐/两端对齐未做。
- **首行缩进、行高倍数、字距**：都没有。
- `MonospaceMeasure` 仍是 `AppLoop::new` 的默认测量器，它不换行（只是按宽度比例估行数）。
  真字体请用 `AppLoop::with_stack`。

### 13.8 省略号与 `max_lines`

`TextMeasure::measure` 多了一个 `max_lines: Option<u32>`——行数上限会改变高度，测量端必须知道。

断行器新增：

```rust
pub const ELLIPSIS: &str = "…";
pub fn wrap(text, max_width, max_lines, width_of) -> Vec<Line>
```

`Line` 增加 `ellipsis: bool`。截断行的 `range` **不含**省略号；绘制端在标志为真时追加
`ELLIPSIS`，所以测量与绘制用的是同一串、同一宽度。

截断规则：

1. 保留前 `max_lines - 1` 行；
2. 从第 `max_lines` 行的起点重新填充，取「前缀 + 省略号」仍能放下的最长前缀。

> 注意第 2 步的起点是**第 n 行原本的起点**，不是第 n-1 行的末尾。写测试时我一开始期望
> `"dd"`，实际是 `"cc d"`——实现是对的，期望值是我按直觉写错的。

3 条新测试（共 10 条，全部注入假宽度）：

| 测试 | 断言 |
|---|---|
| `wrap_keeps_every_line_when_under_the_limit` | 未超限时不截断、无省略号标志 |
| `wrap_marks_the_last_line_when_the_limit_is_exceeded` | 超限时最后一行带标志 |
| `the_ellipsized_line_still_fits_the_width` | 省略号计入行宽，截断后仍放得下 |

端到端（`text_wrap` 例子新增一条 `Some(2)` 的节点）：

```text
capped  wrap 170.0 box_h 39.8 text_h 39.8  OK     ← 两行，末行以 "…" 结束
glyphs 141 stray 0
```

屏上是 `The quick brown fox` / `jumps over the lazy d…`。

### 13.9 仍未做

- **省略号的宽度假设**：`wrap` 用 `width_of(ELLIPSIS)` 单独测量省略号，再与前缀相加；
  若省略号与前一个字符之间存在字偶间距，实际宽度会差一点点。当前字符（拉丁字母、CJK）
  与 `…` 之间没有 kerning，所以实测无差异，但这是**假设**，不是保证。
- **对齐**：仍只有左对齐。
- 首行缩进、行高倍数、字距没有。
- `NodeData::Text` 现在有 4 个字段，`Decl` 还没有对应的构建器；示例里都是手写字面量。
  字段再多就该抽 `TextStyle` 了。

---

## 14. 富文本的准备：段落 = run 列表

### 14.1 为什么现在就改

Markdown / HTML / XML 最终都归约为同一件事：**一段文本由若干带样式的片段（run）组成**。
如果数据模型是「一个 String + 一套样式」，将来接解析器时整条管线都要改；现在改，代价是
一次机械替换。

```rust
pub struct TextStyle { pub size: f32, pub color: Color }
pub struct TextRun { pub text: String, pub style: TextStyle }
pub struct TextBlock {
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
    pub max_lines: Option<u32>,
}
```

`NodeData::Text { block }` 和 `TextPrim { block, origin, max_width }` 都只携带一个 `TextBlock`，
所以**加粗体、斜体、下划线、链接只在 `TextStyle` 上加字段**，构造点不用再动 18 处。

### 14.2 扁平化 + 区间表

断行器与整形器都作用于一个连续字符串，因此进入管线前先扁平化：

```rust
let (text, spans) = block.flatten();   // spans: Vec<Span { range, style }>
```

`spans` 恰好覆盖每个字节一次（零长度的 run 不产生区间），于是：

- **测量**：`width_of(slice, range)` 用 `TextBlock::style_at(spans, range.start)` 取字号；
- **绘制**：逐行按区间边界切段，每段用自己的字号整形、用自己的颜色出图；
- **行高**：取最高 run 的行高，**整段共用一套行网格**（否则一行里会高低不齐）。

这也是为什么断行器的宽度函数签名从 `FnMut(&str) -> f32` 变成
`FnMut(&str, Range<usize>) -> f32`——只有切片时，两个 run 文字相同就无法区分样式。

### 14.3 对齐

`TextAlign::{Start, Center, End}` 只影响行的水平位置：

```rust
let spare = (max_width - line_width).max(0.0);   // max_width 非有限时视为 0
origin_x + match align { Start => 0.0, Center => spare * 0.5, End => spare }
```

注意用的是**布局给出的换行宽度**（`node.wrap_width`），不是节点矩形——理由同 §13.3。

### 14.4 验证

`text_wrap` 例子现在有 7 条文本节点，覆盖：自由换行、CJK 断字、单行不换、省略号、
**跨 run 的两样式段落**、居中、右对齐。

```text
latin    wrap 170.0 box_h 79.6 text_h 79.6  OK
cjk      wrap 170.0 box_h 59.7 text_h 59.7  OK
wide     wrap 380.0 box_h 21.0 text_h 19.9  OK
capped   wrap 170.0 box_h 39.8 text_h 39.8  OK
rich     wrap 300.0 box_h 53.0 text_h 53.0  OK   ← 20px + 13px 同段换行
centred  wrap 240.0 box_h 21.0 text_h 19.9  OK
aligned  wrap 240.0 box_h 21.0 text_h 19.9  OK
glyphs 214 stray 0
```

判据不变：没有字形落在所有文本框之外，且每个文本框不短于它自己的文本。

### 14.5 下一步接解析器时要补的

| 缺口 | 说明 |
|---|---|
| **解析器本身** | `TextBlock` 是目标模型，但 md/html → runs 的转换还没有。放独立 crate，不进 `ui-core` |
| **样式继承** | HTML 的 `<b><i>x</i></b>` 需要可合并的样式（`TextStyle` 现在不能 merge） |
| **字体粗细/斜体** | `TextStyle` 只有 `size`/`color`；加 `weight`/`italic` 后 `TextPass` 需要按样式选字面 |
| **行内盒模型** | `<code>` 背景、链接下划线、图片内联——需要行内矩形而不是字形列表 |
| **段落间距/缩进** | `TextBlock` 还没有 `line_gap`/`indent` |
| **`Decl` 构建器** | 示例里仍是手写 `TextBlock` 字面量；`Decl::text(...).align(...)` 还没做 |

---

## 15. 效果作为类别

### 15.1 只有一个代数，没有 kind 枚举

渲染器**不知道任何效果的名字**。它只会两件事：

```rust
pub struct Effect {
    pub kernel: Kernel,        // 可分离一维卷积核（半径 + 归一化权重）
    pub color: ColorMatrix,    // 预乘 RGBA 上的 4x4 变换
}
```

一个层携带 `Vec<Effect>`，按顺序执行。于是：

| 想要的效果 | 怎么表达 |
|---|---|
| 组透明度 | `Effect::opacity(0.5)` —— 核为恒等，矩阵为全通道乘 0.5 |
| 高斯模糊 | `Effect::blur(radius, sigma)` —— 核为高斯，矩阵为恒等 |
| 去饱和 / 变灰 | `Effect::none().with_color(ColorMatrix::saturate(0.0))` |
| 模糊后淡出 | `vec![Effect::blur(6, 2.5), Effect::opacity(0.6)]` |

**新增效果不需要改渲染器**——只要它能写成「邻域加权采样 + 颜色矩阵」。这也是判断它是否真属
「类别」的标准：如果新效果要求渲染器加一个分支，那它就不是这个类别里的成员。

### 15.2 核必须是归一化的

`Kernel::gaussian` 内部归一化到和为 1。这不是洁癖：不归一化就等于每次模糊都改变画面总亮度，
而人眼对整体变暗比对方块边缘变糊敏感得多。测试
`a_normalised_kernel_redistributes_ink_without_creating_it` 钉的就是这一条——把归一化去掉
（`*w /= sum * 1.05`），总墨迹从 440640 掉到 396240，测试立刻变红。

### 15.3 `EffectReach { guaranteed, actual }`

模糊会读取自身输入之外的像素，所以层边界必须留白。但「需要多少」有两个答案：

- `guaranteed`：解析界 `⌈3σ⌉`，与无限高斯不可区分；
- `actual`：调用方实际给的截断半径。

两者都留着，是为了让「便宜的模糊看起来便宜」，而不是悄悄按解析值宣称精确。这条沿用
`docs/architecture.md` 里 `EffectReach` 的判据（保证 vs 估计）。

### 15.4 后端差异：拒绝而不是跳过

CPU 路径已实现完整效果链（两趟一维卷积 + 颜色矩阵）。GPU 路径目前只做「纯淡出」，
遇到核就返回错误：

```text
GL: layer effect chain is not a pure fade yet: [4]
```

**静默跳过是最坏的结果**：画面看起来合理，没有任何警告，等到发现时已经不知道是渲染器还是
场景的问题。`examples/gpu_blur_unsupported.rs` 专门钉这一条——它断言 GPU **必须报错**，
且错误文本必须说明不支持的是什么。

GPU 侧的双趟 FBO ping-pong 是下一步。

### 15.5 验证

| 层 | 内容 |
|---|---|
| `ui-core::effect` 单元测试（6 条） | 恒等无操作、高斯权重和为 1、高斯对称、透明度矩阵作用于全部预乘通道、矩阵按应用序复合、模糊报告自身触及范围 |
| `ui-paint` 单元测试（3 条） | 质量守恒、峰值下降且向外扩散、超出半径逐字节不变 |
| `ui-paint` 示例 | `effects_cpu` 导出锐利/模糊两张图供肉眼对照 |
| `ui-paint-gpu` 示例 | `gpu_blur_unsupported` 断言 GPU 拒绝 |

一条值得记的教训：第一版质量守恒测试用了 8x8 的色块，u8 量化噪声（1.7%）超过了 1% 的容差，
看起来像「核不守恒」。**样本太小时，量化噪声会冒充算法错误**——改成 24x24 后噪声降到 1% 以下。

### 15.6 仍未做

- ~~**GPU 模糊**~~：已实现（§15.7）。
- ~~**效果之间的裁切**~~：已核实——**原说法是错的**，当时 `bounds` 在两个后端都没被用过，
  层纹理是整个视口，根本没有截断。现已修正（§15.10）。
- **降采样模糊**：大半径时先降采样再卷积才划算，现在按全分辨率算。
- **投影/内阴影**：需要「偏移 + 着色」，超出当前「核 + 矩阵」的代数范围，得先扩类别。
- **`LayerEntry.radius`** 仍未被合成端使用（层圆角裁剪）。
- **效果缓存（按脏区复用上一次输出）**：已调查未实现，结论见
  [`docs/effect-cache-plan.md`](effect-cache-plan.md)。前提是先有持久输出纹理与 GPU 侧 damage 通路。

### 15.7 GPU 侧：一个程序，两趟

`effect_pass.rs` 里没有 `match effect`：只有「一次全屏卷积 + 一个颜色矩阵」这一趟。

```text
horizontal pass: src ──kernel──▶ scratch[0]
vertical   pass: scratch[0] ──kernel + colour──▶ scratch[1]
```

- `u_dir` 选择轴向，同一个程序跑横纵两趟；
- 颜色矩阵叠在纵向那趟里，省一次全屏；
- 纯颜色效果（无核）走单趟，`u_dir = (0,0)`、`u_taps = 1`；
- 两个 scratch 目标乒乓，尺寸随层变化时重新分配。

两个坑值得记：

**坑一：新建的纹理没有存储。** 我在创建 `LayerTarget` 时把 `width/height` 记成了请求值，
于是后面的「尺寸不符则 `tex_image_2d`」分支被跳过——纹理从未分配，FBO 不完整，
每一趟都报 `GL_INVALID_FRAMEBUFFER_OPERATION (1286)`。改成创建时留 `0,0`，让分配分支跑。
**教训**：把「已分配」的记账和「已请求」的记账混在一起，就会跳过真正的分配。

**坑二：混合开着，暂存却没清。** 效果趟写满每个像素，但混合方程
`src + dst*(1-src.a)` 会把 scratch 里未定义的内容混进来。整条链前后 `disable/enable(BLEND)`。
`begin_layer` 会 `clear` 层目标，所以层本身没这个问题——**新加的缓冲要自己负责初始化**。

### 15.8 两个后端的一致性怎么验

`gpu_blur_scene` 用五条判据，前四条在两个后端上各测一遍：

| 判据 | 内容 |
|---|---|
| 1 | 墨迹总量守恒（漂移 < 2%） |
| 2 | **边缘变软**（不是「峰值下降」） |
| 3 | 方块外侧紧邻像素变亮 |
| 4 | 半径之外的列全透明 |
| 5 | CPU 与 GPU 平均通道差 ≤ 3/255 |

实测（NVIDIA RTX 5060 Ti 与 Intel ARL 都通过）：

```text
1 cpu ink drift                3960 of 440640  OK
1 gpu ink drift                  24 of 440640  OK     ← 浮点比 u8 截断更守恒
2 cpu edge softens                 255 -> 147  OK
2 gpu edge softens                 255 -> 148  OK
5 cpu/gpu mean channel delta            0.068/255  OK
```

**判据 2 一开始写的是「中心变暗」，它错了。** 色块（24px）比核（半径 6）大时，中心仍在核的
支撑范围内，本该保持饱和——CPU 的 253 只是截断噪声。模糊的定义性事实是**边缘变软**，
不是峰值下降。判据写错时，GPU 会「失败」而实现是对的。

### 15.9 一处结构性修正

`GpuRenderer::read_pixels` 返回**直通 RGBA**，`ui_paint::Pixels` 是**预乘 BGRA**——通道序和
alpha 约定**都**不同。这已经是第四次在这上面出错（AGENTS.md 里有前三次），所以这次不再写进
示例，而是把它变成 API：

```rust
GpuRenderer::to_premultiplied_bgra(&read_pixels(...))
```

转错一次，墨迹总量会差一倍以上（边缘 alpha=69 的像素在直通表示里 RGB 仍是 255）。

### 15.10 层边界：一条被我写错的「已知缺口」

§15.6 曾经写着「大半径模糊在层边缘会被截断」。核对后发现：

- `begin_layer` 用的是**整个视口**尺寸（`viewport.0/1`），不是 `LayerEntry.bounds`；
- `bounds` 在两个后端**都是 0 处引用**（`grep -rn '\.bounds' crates/ui-paint*` 无命中）。

所以从来没有截断——那条「缺口」是凭直觉写的，**从未验证**。现已修正为：

1. `bounds` 成为有效数据：`view.rs` 按 `EffectReach::of(&chain).actual` 外扩；
2. CPU 端把效果区域收敛到 `bounds`，不再是整个脏区；
3. 两条测试钉住它：外扩后的边界与全画布**逐字节相同**，不外扩的边界**会把模糊切断**。

顺便修了一个真 bug：`EffectReach.guaranteed` 原本用 `radius / 3` 反推 sigma，
`ceil(3 · radius/3) ≡ radius`，所以 `guaranteed` **恒等于** `actual`——
那个「保证 vs 估计」的区分形同虚设。根因是 `Kernel` 不记得自己是哪个 sigma 生成的。
现在 `Kernel` 带上 `sigma`，`gaussian(4, 2.5)` 如实报 `actual = 4`、`guaranteed = 8`。

> 单位陷阱：`reach` 是**设备像素**（卷积在设备像素上做），`bounds` 是**逻辑单位**，
> 所以 `view.rs` 里除以 `scale`，CPU 端再用 `to_device` 换回去。直接混用只在 scale=1 成立。

层纹理仍按整个视口分配；**按 `bounds` 缩尺寸**是性能改动，归入
[`effect-cache-plan.md`](effect-cache-plan.md) 的第 2 步（持久输出纹理）一起做，
因为它需要同样的坐标平移，且与缓存共用同一套边界语义。

## 16. 设备层与绘制层的分界

原先 `ui-paint-gpu` 一个 crate 同时做 EGL 设备管理、gbm 分配、dma-buf 导出和 GL 绘制。
重构后按**「谁拥有资源」**切开，而不是按「谁用到 GPU」切。

| crate | 拥有 | 依赖 |
|---|---|---|
| `ui-gpu-egl` | EGL 上下文、gbm 设备、dma-buf 分配、`DmaBufTarget`、`GpuError` | `khronos-egl`、`glow`、`libc` |
| `ui-paint-gpu` | GL 程序、批处理、效果趟、`OffscreenFbo` | `ui-core`、`ui-gpu-egl`、`glow` |

`GpuSurface` trait 落在 `ui-gpu-egl`，因为两个实现分居两侧：

- `DmaBufTarget`（设备层）：缓冲要交给 compositor，零拷贝
- `OffscreenFbo`（绘制层）：只读回，从不离开 GPU

### 16.1 为什么 gbm 不能单独搬走

最初的计划是「把 gbm FFI 搬到平台侧」，实测不成立：gbm 设备句柄由 `EglContext` 拥有
（同一个 DRM fd）。只搬 FFI 就得在同一 fd 上开两份 gbm device，或者把 `GbmDevice`
类型暴露出去——前者是资源浪费与潜在不一致，后者等于没搬。

所以边界不是「gbm 在哪」，而是**「设备与呈现」 vs 「GL 绘制」**：`EglContext` 整体
搬进 `ui-gpu-egl`，`GpuRenderer` 通过 `GpuSurface` 与 `glow::Context` 与它交互。

### 16.2 兼容再导出让搬运可分段验证

`ui-paint-gpu` 保留：

```rust
pub use ui_gpu_egl::{DmaBufTarget as GpuTarget, GpuSurface};
pub mod context { pub use ui_gpu_egl::context::*; }
```

于是 `GpuTarget::new(...)`、`.desc()`、`.dup_fd()` 和 `crate::context::EglContext`
在所有调用方都不需要改动。**「搬文件」与「改调用点」因此解耦成两个可独立验证的阶段**，
不必一次改动二十多处。

### 16.3 分类守卫拦下过一次

`platform_boundary` 测试在新 crate 出现时立刻失败——它要求每个 crate 显式声明
「为什么允许碰 OS 特定 API」。`ui-paint-gpu` 的分类理由随之改写为
「GL 渲染器；仅通过 `ui-gpu-egl` 才与平台相关」，这就是本次重构的实质。

### 16.4 绘制层已不再引用 EGL

`GpuRenderer` 与 `OffscreenFbo` 的签名只接受 `&glow::Context`：

```rust
GpuRenderer::new(gl: &glow::Context, is_gles: bool)
OffscreenFbo::new(gl: &glow::Context, width, height)
GpuSurface::bind(&self, gl: &glow::Context)
```

`is_gles` 只有 `new` 里用到（编译着色器前缀），所以不存字段，渲染器保持无状态。

**必须区分的边界**：目标对象的释放属于设备层，`DmaBufTarget::release` 仍要
`&EglContext`（它得销毁 EGLImage）；`OffscreenFbo::release` 只要 `&glow::Context`。
把两者统一成 `gl` 会丢掉 EGLImage 的销毁路径。

`ui-paint-gpu/src/lib.rs` 保留 `pub use ui_gpu_egl::EglContext;` 只是为了调用方创建
上下文方便，渲染器本身不再触碰 EGL。

### 16.5 对 Vulkan 后端的意义

绘制层现在只依赖 `glow::Context` 与 `GpuSurface`。接 Vulkan 时新增 `ui-gpu-vulkan`
作为设备层，实现同样的 `GpuSurface`；**真正的工作量在渲染器本身的 API 替换**
（glow → wgpu/ash），而不在分层。分层已经就位，不需要把绘制逻辑写两遍。

## 17. 能力抽象：按功能主题，而不是按协议

### 17.1 原则

Wayland 连接与协议本身是**平台协议**，属于平台层。但协议**提供的功能**不应以协议名
出现在上层。上层的接口按**功能主题**命名：

| 功能主题 | 上层看到 | Wayland 侧可能的协议 | 缺失时 |
|---|---|---|---|
| 背景模糊 | `BackgroundEffect::blur(region, radius)` | `ext_background_effect_v1`、`org_kde_kwin_blur` | 不模糊，其余照常 |
| 输入法 | `PresentTarget::set_ime(ImeState)`（启用 / 插入符矩形 / surrounding）+ `Event::Preedit`、`Event::DeleteSurrounding`（**已落地**） | `zwp_text_input_v3` | 只能收原始按键 |
| 热键 | `Hotkeys::bind(combo) -> BindingId` | `zwp_keyboard_shortcuts_inhibit`、桌面专用全局 | 无全局热键 |

判断标准：**上层代码里不应出现协议名**。若某处写了 `zwp_`、`ext_`、`org_kde_`，
说明抽象漏了。

### 17.2 形状

可移植侧（`ui-core` 或 `ui-platform`）定义 trait 与数据类型，平台侧提供实现：

```rust
// 可移植侧：只有语义
pub trait BackgroundEffect {
    /// Ask the compositor to blur what is behind this surface.
    /// 请求 compositor 模糊此 surface 背后的内容。
    fn set_blur(&mut self, region: Rect, radius: f32);
    fn clear_blur(&mut self);
}

pub trait Hotkeys {
    fn bind(&mut self, combo: KeyCombo) -> Option<BindingId>;
}
```

平台侧：每个功能一个模块，内部自行选择可用协议：

```rust
// ui-platform-wayland/features/blur.rs
pub struct WaylandBlur { /* ext_background_effect_v1 或 org_kde_kwin_blur 或 None */ }
```

### 17.3 两条硬约束

1. **缺失即降级，不报错**。协议不存在时实现为空操作，窗口照常工作。这与 §16 里
   `Capabilities` 的「硬性要求 / 可选」两分类一致。
2. **能力查询与能力使用分离**。上层可以问「支持模糊吗」，但**不允许**据此改变布局或
   语义——模糊是纯视觉增强。IME 不同：它影响文本输入路径，所以「有没有 IME」要能在类型上
   区分。落地的做法是让**事件本身**承担这个区分：没有 IME 时后端不会产出 `Event::Preedit` /
   `Event::DeleteSurrounding`，`set_ime` 保持空实现（`docs/ime-text-design.md` §5.2）。

### 17.4 与 §16 的关系

§16 的 `session.rs` 回答「**这个 compositor 提供什么**」（检测，一处）。
§17 回答「**上层能用什么**」（功能抽象，按主题）。
两者中间隔着一层适配：`session::Capabilities` 里的可选全局对象，被各功能模块取用并
包装成主题接口。

### 17.5 首个落点：背景模糊

三个例子里它最自包含：一次调用、无事件流、无状态机。适合作为验证这套抽象的垂直切片。

顺序：
1. 在 `ui-platform` 定义 `BackgroundEffect` trait（不实现）
2. `ui-platform-wayland` 加 `features/blur.rs`，探测 `ext_background_effect_v1`，
   缺失时为空实现
3. 示例里对卡片背后的区域请求模糊，并与现有的 `Effect::blur`（层内模糊）**明确区分**：
   后者模糊层自身内容，前者模糊背后的 compositor 内容

第 3 点值得单独强调：**这是两个不同的能力**，不要用同一个名字。
