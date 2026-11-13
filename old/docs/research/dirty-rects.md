# GPU 下的脏矩形：参考实现与我们的落点

> 主题 3 的调研（C1）。结论先行：**双缓冲轮转的 dma-buf 上不能只做"局部渲染"，
> 必须引入一张持久的场景纹理**；这张纹理同时也是离屏画布的底座。

## 1. 各家怎么做

| 实现 | 做法 | 依据 |
|---|---|---|
| **GPUI 0.2.2** | 每帧整面 `LoadOp::Clear`，再重画整个 scene。**没有脏矩形**；它省的是"这一帧要不要画"（element 级 invalidation） | 本机源码 `crates/gpui_wgpu/src/wgpu_renderer.rs:1509`（`load: LoadOp::Clear`）、`:1117 draw(scene)` |
| **egui / epaint 0.36.2** | 立即模式，全量重画。`ClippedPrimitive` 只带裁剪盒，渲染端据此设 scissor；tessellator 会丢弃完全在盒外的图元 | 本机源码 `epaint-0.36.2/src/tessellator.rs:1359 tessellate_clipped_shape` |
| **Flutter** | `RepaintBoundary` 把子树切到独立 layer，只有脏 layer 重绘 | `[未证实]` 本地 clone 只有 `painting`/`material`/`animation`，缺 `rendering/layer.dart` |
| **Qt Widgets** | backing store + `QRegion` 脏区，只把脏区从 backing store 推到窗口 | `[未证实]` 本机只有布局头文件，没有 widget 后端源码 |
| **Wayland** | `wl_surface.damage_buffer`（v4）声明"新缓冲与当前表面内容的差异"，compositor 只重画这些区域 | `/usr/share/wayland/wayland.xml:1876`（请求描述含"compositor ignores parts outside the surface"） |

**关键约束**：协议要求**未声明为脏的区域在新缓冲里也必须与当前表面内容一致**。
双缓冲轮转时，第 N 帧写缓冲 A、第 N+1 帧写缓冲 B——B 里那块是第 N-1 帧的内容，
所以"只画脏区"在轮转缓冲上直接做不到（除非每帧整面拷贝一次旧内容，那不比全画便宜）。

## 2. 我们的现状

- `ui-platform-wayland/src/gpu_window.rs`：每帧 `win.present_dmabuf(&image, &Damage::Full, ..)`，
  渲染器每次整面 `clear` + 画完整 `DrawList`。
- 引擎其实**已经算出了脏区**（`compute_damage`，按节点墨迹指纹），只是 `present` 把它丢了。

## 3. 两档方案与取舍

**A. 只上报真实脏区**（compositor 端省重绘、省带宽）。
成本最低，但受上面的协议约束：缓冲轮转下未脏区域会是旧帧内容 → 会看到鬼影。
所以 A 只能配合 B 使用。

**B. 持久场景纹理（scene texture）**：
每帧只把脏区渲染进这张常驻纹理，再把它 blit 到当前交换缓冲。
- 双缓冲轮转不再是问题：纹理总是"完整的最新画面"，blit 可以整面也可以分块。
- 局部渲染真正省下的是 GPU 填充与顶点处理；blit 的代价与脏区面积成正比。
- 这张纹理同时也是**离屏画布的底座**（主题 3 的"为离屏画布留好准备"）。

结论：**先做 B，A 作为 B 的产出**（B 的 damage 集合直接喂给 `damage_buffer`）。

## 4. 落地步骤

1. `ui-paint-gpu` 新增 `SceneTarget`：窗口尺寸的常驻纹理 + 一个 damage 矩形集合，
   提供 `render(gl, list, damage)`——只对脏区 `scissor` + `clear`，并跳过完全在脏区外的批次。
2. `blit_to(target, damage)`：`glBlitFramebuffer` 按脏区逐块搬运（或整面，当脏区超过阈值时）。
3. `GpuWindow::present` 上报本帧的带（`Damage::Rects`），整帧时仍报 `Full`。**2026-11-04 已做**：
   前提是 §6 的两个缺陷都修掉了——交出的缓冲必须处处为真，报文才敢说"只改了这儿"。
4. 离屏画布 = `SceneTarget` + 采样参数（nearest / linear）+ 自己的 damage 缓存；
   控件声明"画布 + 采样方式"，引擎负责只重绘脏区。**见 §4.1 / §4.2。**

### 4.1 已落地：缓存子树（2026-11-04）

`Decl::offscreen()` 把一棵子树声明成自有目标：`LayerEntry::cache_key` 携带节点的 `Id`，
`GpuRenderer` 以它索引跨帧保留的目标（`cached_targets`），只在两种情况重画这棵子树——

- 本帧 `damage` 与画布矩形相交；或
- 目标不存在 / 尺寸或格式变了（窗口改尺寸、HDR 开关）。

其余帧整棵子树的批次被跳过（`RenderStats::cached_skips`）；父级若清空过该处，就由缓存的
合成图补回（`composite_layer` 的 `cleared` 参数）。

两个前提写在代码里，改动前先读：

1. **含磨砂的子树不缓存**：磨砂读父级，而跳过的帧不提供父级像素。绘制器发现子树里出现了
   `Op::Backdrop` 就清掉 `cache_key`。
2. **带效果的层仍强制整帧**（`damage_band` 的既有规则），因此部分帧里出现的层只可能是
   无效果层——也就是画布。合成必须限制在 `cleared` 内，否则带外已有的像素会被再合成一遍，
   半透明内容逐帧变暗。

验证：`ui/crates/ui-paint-gpu/examples/gpu_cached_scene.rs`——冷缓存不跳过、脏区跨过画布时
复用并从缓存补回、画布内脏区重画且抹掉腾出的像素；跳过与补回两处各故意破坏一次，确认红。

### 4.2 仍未做：尺寸 / 缩放 / 采样

`QGraphicsItem::ItemCoordinateCache(QSize)` 那一档（画布按固定像素尺寸渲染、视图缩放时
放大采样）需要给六个着色器加一组平移 + 缩放 uniform：目前几何一律按 `self.scale` 生成，
`viewport` 只是归一化分母，没有把整批图元搬进更小目标的手段。目标目前一律与帧同尺寸，
`cache_key` 也就只表达"缓存"，不表达"密度"。

## 6. 残留与闪烁：三个缺陷，都已修（2026-11-04，用户报告）

### 6.1 症状与度量

用户在真机上看到残留像素，且残留的部分会闪。加了一个探针：`CRABSNAP_TARGET_CHECK=1`
每帧把**刚交给 compositor 的目标**与**持久帧**逐像素比对（`GpuRenderer::diff_frame`），
`Damage::Full` 承诺的正是"二者处处相同"。

| 状态 | 每帧不一致像素 |
|---|---|
| 修复前 | 交替 **3 345 000 / 6 143**（3.34M ≈ 整屏，3.69M 总像素） |
| 只修缺陷 ①（轮转） | 交替 6 143 / 379 |
| 两个都修 | **0**（selftest 全程 0 帧有差异） |

### 6.2 缺陷 ①：轮转缓冲只收到本帧的重绘带

`TARGETS = 2`：第 N 帧写 A、第 N+1 帧写 B，B 里带外是第 N−1 帧的内容。谁在 N 帧变过而
N+1 帧没再变，B 就会把它显示成旧值——一个缓冲周期的"闪回去"。若那次变化是整页内容
（换页、首帧），这个目标**永远**补不回来，那就是残留。
修法：`render_frame_into(..., carry)`——交付时把"上次轮到它之后画过的带"一起搬过去
（`GpuWindow::recent_bands` 保存最近 `TARGETS−1` 个带，整帧记成占满整面的矩形）。

### 6.3 缺陷 ②：裁剪框与重绘带不相交时，裁剪被整个关掉

`set_scissor` 把 `clip ∩ band` 的**空交集**和"两者都没有"混成了同一个 `None`，于是一个
裁剪框落在带外的批次得到的是 `glDisable(GL_SCISSOR_TEST)`——**整批画满持久帧**。带外像素
因此被改写，而目标只搬带内的像素：残留就从这里来。这就是"带外写者"，它不是某个特殊的
pass，而是每一条普通绘制。

修法：把判定提成纯函数 `batch_scissor(clip, band) -> Scissor{Off, Box, Empty}`，空交集给
`Empty`（`glScissor(0,0,0,0)`，什么也不画）。回归测试
`a_clip_that_misses_the_band_draws_nothing_instead_of_everything`：把空交集改回 `Off`
时红，改回来绿。

排查路径留档：先排除 `apply_backdrop`（`CRABSNAP_NO_BACKDROP=1` 复测差值不变）、核对
`gl.clear`/`composite_layer` 都按 band 裁剪，再把"两帧持久帧相减、只看带外"的变化点画出来
——它们是散落在带外的字形与弧线，说明写者就是普通批次，而不是某个特殊 pass。

### 6.4 现状与验证

局部渲染照旧默认开启。探针在 progress / lists / cards 三页各跑 4 秒，以及**整轮 selftest**
（走遍所有页面、开对话框与菜单、含磨砂与动画）全程 `differing=0`，`219 ✓ / 0 ✗`。
`CRABSNAP_NO_PARTIAL=1` 仍可强制整帧做对照。

**已做**：`present` 现在按 `RenderStats::band` 上报 `Damage::Rects`（整帧时仍 `Full`），
合成器可以跳过带外的重绘。探针在带内外的差值仍为 0。第三个缺陷见 §6.5。

### 6.5 缺陷 ③：部分帧把磨砂又合成了一遍（2026-11-04，用户报告）

用户补充：几乎**所有**玻璃块都在闪，只是玻璃块大多长在第三列卡片上，看起来像"只有那一列"；
`CRABSNAP_NO_PARTIAL=1` 就不闪。

`composite_backdrop` 用预乘 `ONE, ONE_MINUS_SRC_ALPHA` **混合**到帧上。玻璃与亚克力的输出
alpha < 1，所以在部分帧里对同一块玻璃再跑一次，结果不是"什么都不变"，而是
`src + (src + dst(1−a))(1−a) ≠ src + dst(1−a)`——逐帧偏离整帧路径。`damage_band` 当时
只保证"带不碰磨砂盒"，却没保证"磨砂不重跑"，于是**带远在别处时玻璃照样被重算**。
这与 §4.1 里层合成踩过的坑是同一个（`cleared` 参数就是为它加的），只是换到了 backdrop 上。

同时 `damage_band` 的判据用错了区域：它拿 `e.rect` 判断，而核实际读取的是盒子外扩
`reach = kernel.radius*3 + 透镜余量 + 2*scale` 的范围。带落在那个环里时，玻璃的输出**会**
随本帧像素改变，却仍被当成"碰不到"。

修法：

- `backdrop_read()` 统一算出读取范围，`damage_band` 与 `apply_backdrop` 共用一份；
  带碰到它就退回整帧（输出确实会变，而带是单个矩形，装不下它）。
- `apply_backdrop` 在部分帧里直接返回：带外的像素既不该重算，重算也不是幂等的。

验证（`ui/crates/ui-paint-gpu/examples/gpu_glass_partial.rs`，先红后绿两处各做一次）：

| 观测 | 修复前 | 修复后 |
|---|---|---|
| 带在左上角、玻璃在中央：玻璃区域漂移 | **13 528 px**（每帧） | 0 px |
| 同上，`CRABSNAP_NO_PARTIAL=1` | 0 px | 0 px |
| 带在玻璃盒外 20dp、在 `reach` 内 | `band=Some(..)`，玻璃不重算 | `band=None`（整帧） |
| gallery cards 页扫动指针：部分帧里重合成的玻璃块数 | **49/49 帧 × 2 块** | 0/47 帧 |

gallery 上的度量同时显示 `[frame-outside]` 与 `[target-check]` 全为 0：帧在带外稳定、交付
缓冲与帧一致。此前 selftest 里测不到这条，是因为**整轮 selftest 一帧都没走局部路径**
（页面切换与动画让带总是超过 70%），"探针没触发"不等于"没有问题"。

### 6.6 磨砂参与部分刷新：把带长到盖住它（2026-11-04，用户要求）

§6.5 的修法让磨砂在部分帧里"要么整帧、要么完全不碰"——只要带碰到它的读取范围就退回整帧，
于是有玻璃的页面上局部刷新基本失效。正确的做法不是跳过它，而是**让带长大**：

> 磨砂会重读盒子四周的画面，所以碰到它的带必须把它也重画；而重画只有在本帧清空过的地方
> 才不会被混合两遍。于是把 `device ∩ clip` 并进带，带 = 清空区 = 重画区 = 合成区。

`damage_band` 因此改成一个**闭包循环**：带碰到某块磨砂的读取范围，就把它的可见矩形并进来；
长出来的带可能又碰到下一块，循环到带不再变大。`apply_backdrop` 不再"部分帧直接返回"，
而是把合成裁剪到 `visible ∩ band`——落在带外的磨砂自然什么都不画。

验证（`gpu_glass_partial.rs` 重写成一个更强的不变量：**部分帧必须与同一场景的整帧渲染
逐像素相同**）：

| 场景 | 带 | 重合成 | 与整帧的差 |
|---|---|---|---|
| 脏区远离玻璃 | 只有脏区本身 | 0 | 0 px |
| 脏区在玻璃盒内 | 长到玻璃可见矩形 | 1 | 0 px |
| 脏区在玻璃盒外 24dp（`reach` 内） | 长到盖住玻璃 | 1 | 0 px |
| 连续 5 个部分帧（来回跨过玻璃） | — | — | 0 px（不累积） |
| 改色一条玻璃底下的条纹 | 长到盖住玻璃 | 1 | 0 px（模糊读到了新像素） |

先红后绿各做一次：关掉长带循环 → "脏区在玻璃盒内"那一行差 13 528 px 且带不再覆盖玻璃；
去掉合成的 `∩ band` → "远离玻璃"那一行差 13 528 px（重复混合）。

真机（gallery `switch` 页，指针按住玻璃开关）：**改前 0 个部分帧**（带碰到玻璃就整帧），
**改后 1147 个部分帧全部重合成 1 块玻璃**，带只有 52×45 设备像素，`[frame-outside]` 与
`[target-check]` 全 0。cards 页扫第一列时 768 个部分帧仍为 `frosts=0`（带离玻璃远），说明
`reach` 判据确实让远处的磨砂不被白算。

**代价**：带会被玻璃撑大。cards 页实测脏区本身只占整面 0.5%–2.3%，而"无脑把每块玻璃都并进
带"是 12.6%——所以按 `reach` 判相交而不是无条件并入，是这个方案的关键。

**仍存在的边界**：带内变化会被带外玻璃像素的核读到，而带外玻璃像素保持上一帧输出。这会让
玻璃边缘的模糊滞后一个核半径，是所有按带渲染的系统共有的性质，不是这里独有的缺陷。

### 6.7 脏区按名义矩形算，阴影画到框外（2026-11-04，用户提问）

阴影的顶点着色器会先把四边形撑开：

```glsl
vec2 pad = vec2(a_ring * 0.5 + a_soft.x * 1.5 + abs(a_soft.y));  // a_soft.x = blur * scale
```

也就是实际覆盖 `rect` 外扩 `ring*0.5 + blur*1.5 + |dy|`（再加半个像素的 AA）。而
`prim_fingerprints` 里 `PrimKind::Shadow` 用的是裸的 `p.rect`——阴影的整圈衰减**一个像素
都没进脏区**。节点移动、出现、消失或改阴影参数时，旧的外圈不会被清空、新画的外圈落在带外
被裁掉，于是残留与切边。静止帧看不出来，因为没人重画它。

`ui-core` 里其实早有一条正确的答案：`Shape::bounds()` 的 `Shape::Shadow` 分支，注释就写着
"A shape may paint outside the node's own box; damage must cover it too"。但它只被
`Node::ink_rect()` 用（裁剪判定与层边界），**脏区那条路没走它**；而且 `Shape::Box` 的阴影
分支算得还偏小（`blur` 而非 `blur*1.5 + |dy|`）——两条路各写一遍公式，注定要分叉。

修法：把"阴影画到多大"收敛成 `draw.rs` 里一个私有函数 `shadow_painted()`，
`ShadowPrim::painted()`（脏区用）与 `Shape::bounds()` 的两条阴影分支都调它，公式只有一个
来源，且与着色器逐项对应。

验证：`ui-platform` 的 `a_moved_shadow_damages_every_pixel_it_paints`——一个同时带
`Shape::Box { shadow }` 与 `Shape::Shadow` 的节点平移 4dp，脏区必须等于
`(-17,-17,98,74)`。改回 `p.rect` 时它报 `(-2,-2,68,44)`（只有 spread 进了矩形，衰减全丢），
改回 `painted()` 后绿。全量测试与 gallery selftest 219 ✓ / 0 ✗ 不受影响。

**顺带**：`Shape::Box` 的 `bounds` 变大后，任何"阴影溢出祖先 clip"的节点如今会被真正裁掉
（以前是漏画的），`subtree_ink` 给出的层边界也相应更保守——两处都是往正确方向变。

## 5. 判据

- 动画帧的脏区面积应远小于整面（例如滑块移动只脏拇指周围）；
- `present` 的 GPU 时间随脏区面积下降，而不是恒定；
- 整面重绘（切页）时不得慢于现在——blit 一次整面是可接受的代价上限。
