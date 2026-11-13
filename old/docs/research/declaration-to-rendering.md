# 声明 → 渲染：主流框架的实际转换路径

> 目的：回答"UI 声明（DSL/声明式/组件式）如何变成实际渲染"，并据此判断我们的
> `DrawList` 设计是否走错了路。
> 证据来源：本机源码（每处都标了 `file:line`），不是二手描述。
> 相关：[`ui-framework.md` §6](../ui-framework.md)、[`ui-framework-review.md`](ui-framework-review.md)

---

## 0. 结论先行

**主流框架没有一家把"渲染"表达成"一个 `enum Cmd` 的命令流"**——包括看起来最像命令流的 egui。
它们共有同一个三段结构：

```text
① 声明        →  持久对象图（元素树 / Item 树 / 控件树）
② 绘制调用    →  「类型化图元」写入带排序信息的批（不是单一枚举流）
③ 后端消费    →  (a) 实例化为 GPU 缓冲，或 (b) 三角化为顶点/索引
```

而**命令流的语义**由"绘制操作日志 / 绘制顺序号"承载，与图元数据**分离存储**。
GPUI 是这个模式的教科书实现：`Vec<PaintOperation>` 记顺序，`Vec<Quad>`/`Vec<Path>`/… 存数据
（`scene.rs:41-58`），最后 `batches()` 用 k 路归并还原画家顺序（`scene.rs:172-185`）。

**我们的 `DrawList`（`enum Cmd { Rect, Path, Text, ... }`）是 v3 的一处设计倒退**：
它把"顺序"和"数据"焊在一个枚举里，直接导致三个后果（详见 §5）：

1. GPU 无法按类型批量上传实例（每条命令都要重新分支）；
2. 叠加 `PushLayer`/`PixelBackdrop` 这类**结构性操作**后，枚举里混着"数据"和"控制流"两类东西；
3. 无法做 §6.4 要求的**按节点分组比较**（damage 需要"这是哪个节点的命令"）。

---

## 1. 四份实测材料

| 框架 | 声明层 | 中间表示 | 后端消费 | 证据 |
|---|---|---|---|---|
| **egui/epaint** | 即时模式：每帧调用 `ui.add()` | 先产生 `Vec<ClippedShape>`（**形状**，不是三角形），再 `tessellate_shapes` 变成 `Vec<ClippedPrimitive{Mesh}>` | 顶点+索引缓冲，按 `(clip_rect, texture_id)` 合并 | `tessellator.rs:2220-2234`、`1385-1414` |
| **GPUI** | 声明式 builder，元素树每帧重建、状态跨帧保留 | `Scene` = **并行的类型化向量** + `Vec<PaintOperation>` 顺序日志 + `BoundsTree` 空间索引 | **实例缓冲**：每种图元一个 `Vec<T>`，`#[repr(C)]` 直接上传；每批一次 `draw(0..4, instances)` | `scene.rs:41-58`、`172-185`、`gpui_wgpu/wgpu_renderer.rs:1620-1650` |
| **Qt Quick** | QML 声明 → `QQuickItem` 树 | **场景图**：节点类型化（`GeometryNode`/`TransformNode`/`ClipNode`/`OpacityNode`），几何+材质挂在节点上 | 渲染线程遍历场景图，提交批次 | `qsgnode.h:37-60`、`155-295` |
| **Flutter** | Widget 树（不可变）→ Element 树（持久）→ RenderObject 树 | `Layer` 树（`ContainerLayer`/`PictureLayer`/…），`Picture` 是**录制**的命令流 | `Picture` 光栅化到 `Layer` 合成的离屏目标 | 三棵树分层；`PictureRecorder` 是录制器 |

**两个关键观察：**

1. **egui 看似"命令流"，其实分了两层**：`Shape` 之上还有 `ClippedPrimitive`（三角形），
   而且三角形是按 `(clip_rect, texture_id)` **合并**过的（`tessellator.rs:1385-1414`）。
   一个 `ClippedPrimitive` 可能对应几十个 `Shape`——**命令边界在消费者那里已经被打破了**。
2. **`Rect` 不等于三角形**。GPUI 把 `Quad` 原样上传成实例，**四个顶点由 vertex shader 生成**
   （`draw(0..4, instances)`），GPU 端没有三角形数据。egui 相反：CPU 端三角化好了再上传。
   这是两种都成立的策略，但**都要求"图元是同质的、可批量"**——单一枚举做不到。

---

## 2. 逐框架分解

### 2.1 egui：Shape → Tessellate → ClippedPrimitive

```text
用户代码 (每帧执行)
   └─ ui.add(Button) → Painter → Vec<ClippedShape>
                                    │  Shape 是封闭枚举：
                                    │  Noop/Vec/Circle/Ellipse/LineSegment/Path/
                                    │  Rect/Text/Mesh/QuadraticBezier/CubicBezier…
                                    ▼
              tessellate_shapes(shapes)          ← 唯一的"转换点"
                                    │            （tessellator.rs:2220）
                                    │  合并规则：clip_rect 与 texture_id 不变就继续往
                                    │  当前 Mesh 里追加顶点（tessellator.rs:1385-1394）
                                    ▼
                     Vec<ClippedPrimitive { clip_rect, Primitive::Mesh(mesh) }>
                                    ▼
                        后端：上传顶点/索引 + 纹理图集
```

**为什么它必须三角化**：egui 的后端契约是"给我顶点和索引"，这样任何后端（wgpu/glow/webgl）
都能用同一段代码。代价是 CPU 每帧都要跑一遍 tessellation（1.5k 行的 tessellator）。

**抗锯齿靠几何**：每条边生成半像素透明羽化带（`tessellator.rs:796-810`），不是 MSAA。
所以三角化必须知道 `pixels_per_point`，缩放变化会重建整个 mesh。

### 2.2 GPUI：Element 树 → Scene（类型化向量 + 操作日志）→ 实例缓冲

```text
Element 树 (每帧 build；状态按 ElementId 跨帧保留)
   └─ paint() 内调用 window.paint_quad(...) 等   ← 绘制调用（不是返回数据）
                                    │
                                    │  window.paint_quad → scene.insert_primitive(Quad)
                                    │  （window.rs:4702、scene.rs:87）
                                    ▼
Scene {
   paint_operations: Vec<PaintOperation>,   ← 顺序日志：StartLayer/EndLayer/Primitive(idx)
   primitive_bounds: BoundsTree,            ← 空间索引（命中裁剪用）
   quads:      Vec<Quad>,                   ← 类型化数据
   shadows:    Vec<Shadow>,
   paths:      Vec<Path<ScaledPixels>>,
   sprites:    Vec<*Sprite>,
}
                                    ▼
   batches()  ← k 路归并：按 paint_operations 的顺序，把类型化向量切成长连续段
                                    ▼
   对每个 batch：draw_indexed(0..4, 0..n_instances)   ← 顶点由 VS 生成
                 数据以 #[repr(C)] 实例数组一次上传
```

**关键设计点（值得直接抄）**：

| 设计 | 位置 | 收益 |
|---|---|---|
| **数据与顺序分离** | `scene.rs:41-58` | 顺序只存一个 `u32` 索引；图元数据可被后端按类型整段上传 |
| **类型化并行向量** | 同上 | 每种图元一个 `Vec<T>` → 一整个 batch 一次 draw call |
| **k 路归并还原顺序** | `scene.rs:172-185` | 画家顺序正确，同时保留"同质长连续段" |
| **`#[repr(C)]` 实例** | `scene.rs:535-544` + `wgpu_renderer.rs:1620` | 直接把 `&[Quad]` 当 `&[u8]` 上传，零转换 |
| **`BoundsTree` 空间索引** | `scene.rs:43` | 裁剪/命中不需要遍历全部图元 |
| **`DrawOrder` 在实例里** | `Quad.order` | 排序键跟着数据走，归并时不需要额外查表 |

### 2.3 Qt Quick：Item 树 → Scene Graph 节点

```text
QML 声明 → QQuickItem 树（逻辑层）
              └─ updatePaintNode() → QSGNode 树（渲染层）
                    ├─ QSGGeometryNode   （几何 + 材质）
                    ├─ QSGTransformNode  （变换）
                    ├─ QSGClipNode       （裁剪）
                    └─ QSGOpacityNode    （分组不透明度）
```

**要点**：Qt 把"结构性操作"做成**独立的节点类型**而不是命令流里的控制项。
`OpacityNode` 的存在直接说明：**分组不透明度是图元之外的第二种东西**，混在同一个枚举里
会导致消费者必须区分"数据命令"与"控制命令"。

（`qsgnode.h:37-60` 的 `NodeType` 枚举只用于 RTTI，节点本身是类层次——不是"一个枚举装所有东西"。）

### 2.4 Flutter：Widget → Element → RenderObject → Layer

```text
Widget 树（不可变，每帧重建）
   └─ Element 树（持久，做 diff —— 与我们的 RetainedTree 同构）
        └─ RenderObject 树（布局 + 绘制）
              └─ Layer 树（合成单元，可缓存为离屏纹理）
                    └─ Picture（录制的绘制命令流）→ 光栅化
```

**要点**：Flutter 的 `Picture` 是**录制**的命令流（类似我们的 DrawList），但它只在
**叶子**出现；上层是 `Layer` 树，负责组合与缓存。也就是说：**命令流不是架构的中心，
合成层树才是**。这与我们的 `PushLayer` 思路一致，但更彻底。

---

## 3. 四种策略的共同点（这才是"主流做法"）

| 共同点 | 说明 |
|---|---|
| **分层** | 声明层 / 中间表示 / 后端，三者从不合并 |
| **中间表示是"类型化的"** | 要么按图元类型分桶（GPUI），要么按裁剪+纹理合并（egui），没有一家是"一个扁平枚举" |
| **顺序独立存储** | 操作日志、`DrawOrder` 字段、场景图层次 |
| **结构性操作与图元分离** | Layer/Clip/Opacity/Transform 是独立概念，不是枚举变体 |
| **空间索引** | GPUI 的 `BoundsTree`；Flutter 的 `Layer` 层次；Qt 的场景图子树 |
| **后端契约尽量窄** | egui 只要顶点+索引；GPUI 只要实例数组；Qt 只要几何+材质 |

---

## 4. 我们的设计哪里对、哪里错

### 4.1 对的

| 设计 | 与谁一致 |
|---|---|
| 声明层 + 持久树（`RetainedTree`） | Flutter 的 Element 树、GPUI 的元素树 |
| 不把布局结果直接喂给渲染（中间隔着绘制阶段） | 全部四家 |
| `PushLayer`/`PopLayer` 做分组不透明度与圆角裁剪 | Qt 的 `QSGOpacityNode`、Flutter 的 `Layer` |
| `Damage` 做区域提交 | egui/GPUI 都没有，这是我们**额外**的正确选择 |
| 效果面作为命令流里的一个位置（读"之前的像素"） | Flutter 的 `BackdropFilter` 用 `Layer` 实现，思路一致 |

### 4.2 错的：`DrawList` 是单一枚举

```rust
// ❌ v3 现状：数据 + 控制流混在一个枚举里
pub enum Cmd {
    Rect { .. }, Path { .. }, Text { .. }, Image { .. }, CanvasRef { .. },
    PixelBackdrop { .. },              // ← 控制：触发一个 pass 序列
    PushLayer { .. }, PopLayer,        // ← 控制：改变后续图元的目标
    Clip { .. }, PopClip,              // ← 控制：改变后续图元的裁剪
}
```

**三处具体代价：**

1. **后端无法批量**：GPU 后端必须 `match` 每一条命令，无法把整段 `Rect` 一次性上传成实例数组。
   GPUI 的 `draw(0..4, instances)` 在当前结构下写不出来。
2. **damage 分组比较失效**：§7 要求"按节点分组比较绘制命令"，但枚举流里没有节点归属
   （只有 effect 带 id）。要么给每条命令加 `NodeId` 字段（浪费 4 字节 × 每条），
   要么另存一份映射（就是 GPUI 的做法）。
3. **结构性操作污染类型**：`PushLayer` 没有 rect 语义、`Clip` 不是图元，
   却与 `Rect` 共享一个 `match`，每加一种控制操作就要改所有后端的 `match` 分支。

---

## 5. 修正方案：三层表示

```text
① DrawList        ← 绘制调用产生的「图元数据 + 操作日志」（对齐 GPUI）
② BatchPlan       ← 归并后的批次计划（每个批次含 clip 段与实例范围）
③ Backend         ← CPU: 三角化/光栅  |  GPU: 实例缓冲上传
```

### 5.1 `DrawList` 的类型化结构

```rust
pub struct DrawList {
    /// 顺序日志：只存"哪种图元、第几个"，4 字节
    /// Paint order log: kind + index, 4 bytes.
    ops: Vec<Op>,
    prims: Primitives,
    /// 每个图元的所属节点，供 damage 按节点分组比较
    /// Owning node per primitive, for per-node damage diffing.
    owners: Vec<NodeId>,
    clips: Vec<ClipEntry>,
}

pub enum Op {
    Primitive { kind: PrimKind, index: u32 },   // kind = 8 种具体图元之一
    PushClip(u32), PopClip,
    PushLayer(u32), PopLayer,
    PixelBackdrop(u32),                          // 效果面：一个"读取之前像素"的图元
}

#[derive(Default)]
pub struct Primitives {
    pub rects:   Vec<RectPrim>,      // #[repr(C)]
    pub paths:   Vec<PathPrim>,
    pub glyphs:  Vec<GlyphPrim>,
    pub images:  Vec<ImagePrim>,
    pub canvases: Vec<CanvasPrim>,
    pub shadows: Vec<ShadowPrim>,
    pub effects: Vec<EffectPrim>,
}
```

### 5.2 `BatchPlan`：归并 + 确定性排序

```text
输入：ops（画家顺序）+ prims（类型化）
输出：Vec<Batch { clip: ClipId, target: LayerId, kind: PrimKind, range: Range<u32> }>

算法：
  1. 扫描 ops，遇到同 kind 且 clip 未变 → 延长当前 batch 的 range
  2. clip 变了 → 结束当前 batch，开新 batch（按 clip 状态分组）
  3. PushLayer → 结束当前 pass，进入离屏目标；PopLayer → 合成回父目标
  4. PixelBackdrop → 结束当前 pass，插入 effect pass 序列（§12.3）
```

**为什么不用 GPUI 的 k 路归并**：它归并的是"类型化向量 → 还原顺序"，前提是**同种图元可以合并**。
我们的 `PushLayer` 会切换渲染目标，跨目标的图元**不能**合并。所以我们的归并必须
**受目标（Layer）与裁剪状态约束**——这是与 GPUI 的实质差异，写清楚避免照抄出错。

### 5.3 与 damage 的协同（顺带解决 P0-3）

类型化 + `owners` 表让 damage 计算变简单：

```text
每帧对比 owners[i] 的图元集合：
  节点 N 本帧的图元切片 ≠ 上帧 → N 的 rect + ink_margin 进 damage
```

不需要"扫描之前的全部命令"（P0-3 的 O(n²)），也不需要给每条命令加节点字段。

---

## 6. 对 v3 文档的具体改动

| 章节 | 现状 | 改为 |
|---|---|---|
| §6.2 `DrawList` | `enum Cmd { Rect, Path, Text, ... }` 单一枚举 | 三层结构（§5.1/5.2），类型化向量 + 操作日志 |
| §6.2 顺序语义 | "命令流" | "操作日志 + 类型化图元；批次由 BatchPlan 生成" |
| §8.4 效果源 | "命令流中在它之前、与 rect 相交的图元" | "效果面在 ops 中的位置之前、已提交到当前目标的图元" |
| §7 damage 来源② | "按节点分组比较命令切片" | 依赖 `owners` 表（§5.3） |
| §12.2 图元→批次→管线 | 表格里"批次"没有定义 | 补 `BatchPlan` 的生成算法 |
| 附录 A | 未与主流中间表示对比 | 增加"中间表示"一行：egui 三角化 / GPUI 实例化 / 我们实例化为主、三角化为辅 |

---

## 7. 一个必须现在定的选择：实例化还是三角化

| | 实例化（GPUI） | 三角化（egui） |
|---|---|---|
| 圆角矩形 | 顶点由 VS 生成，1 个实例 4 顶点 | CPU 生成带圆角的三角带 |
| 抗锯齿 | 需要 MSAA 或 SDF 距离场 | 几何羽化（分辨率无关） |
| 文字 | 图集 quad（也是实例） | 图集 quad（也要三角化） |
| 路径 | 必须三角化（无法实例化） | 三角化 |
| CPU 后端 | 要额外实现一套光栅 | 天然复用同一份三角数据 |

**建议：混合**——矩形/图像/文字走**实例化**（它们是绝大多数命令，且形状规整）；
路径走**CPU 三角化**（路径无规整形状，实例化没有意义）。

理由可推导：实例化的收益在"同质大量"，路径恰恰不同质；而 CPU 后端无论选哪条都要
自己光栅化，所以"三角化数据两条后端共用"只对路径成立。

**这条选择决定了 `Primitives` 里哪些结构带 `#[repr(C)]`**，必须在写 `ui-paint` 前定。

---

## 8. 未验证的部分（标注而非假装知道）

以下内容我**没有**读源码，只基于公开文档与常识，按 AGENTS.md §1.2 标 `[未证实]`：

- **Flutter 的 `Layer` 树细节**：本机无 Flutter 源码。"`Picture` 是录制的命令流、
  `Layer` 负责合成"是公开文档描述，未核实源码。
- **Qt Quick 的批处理策略**：只读了 `qsgnode.h` 的节点类型，没读渲染器
  （`QSGRenderer` 在 Qt Quick 的私有实现里，本机无源码）。
- **Skia 的 `SkCanvas` 内部**：本机无 Skia 头文件，未核实。

**这三处结论在本文档中的作用是"旁证"**，不构成我们决策的依据；
真正的依据是 egui 与 GPUI 的源码（已逐行核对）。

---

## 附：本次核对用到的命令

```bash
# egui 的合并条件
sed -n '1385,1414p' epaint-0.36.2/src/tessellator.rs
# GPUI 的 Scene 结构
sed -n '41,58p'  gpui/src/scene.rs
sed -n '172,185p' gpui/src/scene.rs     # batches()
# GPUI 的实例上传
sed -n '1620,1650p' gpui_wgpu/src/wgpu_renderer.rs
# Qt Quick 的节点类型
sed -n '37,60p' /usr/include/qt6/QtQuick/qsgnode.h
```
