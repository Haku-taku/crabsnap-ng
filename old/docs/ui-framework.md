# UI 框架设计与实现方案（`crabsnap-ui` 引擎）

> 状态：**设计定稿待评审**（v3，2026-10-25）
> 版本演进：**v1** 纯即时模式 → **v2** 允许控件树（"实现接近 egui"是笔误），改为
> 「取两家之长」：有持久控件树，但每帧重新声明、状态按 `Id` 认领 → **v3** 框架独立成项目（§3.1）、
> 布局模型定型（§5.2）。
> 相关：[`architecture.md`](architecture.md)（crate 分层）、[`crate-map.md`](crate-map.md)
> 底层调研报告（逐条带 `file:line`，均已核实）：
> [`egui-0.36.2-analysis.md`](research/egui-0.36.2-analysis.md)、
> [`gpui-0.2.2-analysis.md`](research/gpui-0.2.2-analysis.md)

---

## 0. 结论先行

需求收敛成三条力，v2 重新配平了它们：

| 需求 | 含义 | 本文档的落点 |
|---|---|---|
| 语法接近 GPUI | 声明式、链式、组件即函数 | `ui-el` 的 builder + `Element` trait（§4） |
| **允许控件树**（v2 更正） | 跨帧存在的节点、可持有自己的状态、可做结构复用与局部重建 | `RetainedTree`：节点跨帧存活，但**内容每帧重新声明**（§3.2） |
| 取两者优点 | egui 的"声明简单、状态免维护" + GPUI 的"结构稳定、可局部更新、有真焦点树" | 混合模型：**结构持久 + 声明即时**（§3.2、§3.3） |

### 0.1 为什么"有控件树"反而更好

纯即时模式在以下四点上要吃亏，而这四点我们**全都用得上**：

| 场景 | 纯即时模式的问题 | 有控件树的好处 |
|---|---|---|
| 工具栏换边变形动画 | 每帧重算几何，动画中布局每帧都变 | 节点保留上一帧矩形，**布局只在声明变化时重算**，动画走 `paint` 偏移 |
| 标注列表（几百条） | 每帧重建全部控件 | 节点按 `Id` 复用，**只重建可见区**（GPUI `uniform_list` 的形态） |
| 焦点与 IME | 焦点只能按 `Id` 现算路径 | 焦点是树上一条**路径**；`FocusIn/Out` 由结构差分直接得出 |
| 像素效果依赖 | 要额外维护"效果 ↔ 源区域"依赖表 | 效果面是树节点，**源 = 前序图元**，依赖从结构直接读出 |

代价是"状态可能不干净"（egui 的优势：没出现的控件自动消失）。v2 用**结构差分 + 帧末回收**补回来（§3.2 规则 4），所以不会有僵尸控件。

### 0.2 三条柱子

1. **声明层（每帧重建）**：`fn view(&self) -> impl Element`，用户每帧重新描述界面。
2. **保留层（跨帧存活）**：`RetainedTree::apply(decl)` 把声明差分到树上——`Id` 相同的节点复用，只更新变化的属性；`Id` 消失的节点进回收列表。
3. **执行层**：`layout+prepaint`（融合趟）→ `dispatch` → `paint` 由引擎驱动，`DrawList` + `Damage` 是纯数据。

一句话：**GPUI 的结构（树、两趟）+ egui 的写法（每帧重新声明、状态按 Id 认领）+ 我们自己的脏区与效果级联。**

### 0.3 v3 增补：框架独立成项目 + 布局模型定型

两条结构性调整（回应评审）：

| 项 | 决定 |
|---|---|
| **crate 树** | UI 框架成为**独立的 workspace**（`crabsnap-ng/ui/`，6 个 crate），配 7 个 examples 做原型验证；第一批验证 **普通窗口 + Wayland layer surface** 两种呈现（§3.1） |
| **离屏画布** | 引擎提供 `Canvas` + `SamplingEngine` + `CanvasMapping`：内容网格 / 显示尺寸 / 屏幕位置三层分离，编辑永远在原生像素网格（§6.4） |
| **布局** | **不跑两遍**。用 Qt 式的「尺寸类四元组 `(min, hint, max, grow)`」+ 一次自上而下的确定性 measure，四种宽度需求（固定/内容/跟随父/flex）只是四个字段的值（§5.2）。布局引擎抽成 `LayoutEngine` trait，自研为默认，**taffy 作为可选后端** |

依据是四个真实框架的核对结果：**Qt 与 GTK 都不做两遍完整布局**（Qt 用 `(sizeHint, min, max, expandingDirections)`，
GTK 用 `measure(orientation, for_size, min, natural)` + 至多 3 次调用）；只有 CSS flexbox 那一类算法（taffy）才多轮测量。

---

## 1. 两份参考实现的实测结论

调研基于本地真实源码（读完并核对行号）：egui/epaint/ecolor/emath **0.36.2**（registry 源码）、
GPUI **0.2.2**（Zed `79fe6a6` checkout，`crates/gpui/src/` 共 90,946 行）。

### 1.1 GPUI：值得抄的

| 机制 | 位置 | 要点 |
|---|---|---|
| 三阶段元素生命周期 | `element.rs:52-105` | `request_layout` → `prepaint` → `paint`。布局/命中/绘制被强制分开——**分开是对的，三趟是另一回事**（后者我们不做，见 §1.6.1） |
| 元素树持久化 | `element.rs:52-70` | `ElementId` + `GlobalElementId` 让元素跨帧认领状态；三阶段都收到 `id` |
| 命中盒与绘制同源 | `window.rs:841-851, 5313-5327` | `insert_hitbox(bounds, behavior)` 在 prepaint 阶段登记，附带当时的 `content_mask` |
| 捕获/冒泡双阶段 | `window.rs:5974-6029` | 先正序 Capture 再逆序 Bubble，`cx.propagate_event=false` 中断 |
| 指针捕获 | `window.rs:6027-6029` | `captured_hitbox`，mouse up 自动释放 |
| 焦点路径 | `window.rs:2431-2447, 5561-5566` | `focus()`/`blur()`，`previous_focus_path` 用于派发 `FocusEvent` |
| 副作用隔离 | `element.rs:180-185` | `RenderOnce::render(self)` 取所有权，组件参数构造时消耗 |

### 1.2 GPUI：不能抄的

1. **没有真正的背景模糊**。`blur_radius` 只出现在 `Shadow`（`scene.rs:576`、`style.rs:355`），GPU 后端 `gpui_wgpu` 里 `grep blur` 零命中。→ 我们的效果抽象必须比它强（§8）。
2. **实体系统过重**：`entity_map.rs` 1308 行 + `app.rs` 3911 行 + `window.rs` 9979 行，绝大多数服务于 `Entity<T>` / `cx.notify()` / 多窗口调度。
3. **taffy 全量 flexbox**（`taffy.rs` 787 行）：我们不需要完整 flex 语义。
4. **泛型层叠**：`IntoElement`/`AnyElement`/`ParentElement`/`InteractiveElement` 四层 trait，编译时间与错误信息都很难受。
5. **无脏区**：整帧重绘。

### 1.3 egui：值得抄的

| 机制 | 位置 | 要点 |
|---|---|---|
| 每帧重新声明 | `context.rs` 的 `begin_pass`/`end_pass` | 界面由代码重新生成，没有"视图对象要同步"的问题 |
| 状态外置 | `memory/mod.rs` | `Id → WidgetRect/InteractionState`；这帧没出现的控件自然被清理 |
| 动画按 Id 驱动 | `context.rs:3191-3264` | `animate_bool/value(id, target)`，不用持有动画对象 |
| `Response` | `response.rs` 1172 行 | 把交互结果聚合成一个值，控件只读它 |
| 纹理图集 | `epaint/texture_atlas.rs` | 字形/图标一次 draw call |

### 1.4 egui：问题（逐条已核对源码）

1. **完全没有脏区**（全仓 `grep -i damage` 零命中，已核实）。调度只有"整帧重绘 + 延时重绘"（`request_repaint_after`，`context.rs:130-170`）。
2. **命中判定跑在用户代码之前，用的是上一帧的矩形** —— 这是它最贵的结构性代价。
   `Context::run_dyn` 里 `begin_pass` → 用户闭包 → `end_pass` 循环（`context.rs:836-876`）；
   而 `hit_test(&viewport.prev_pass.widgets, …)` 在 `begin_pass` 内部、用户代码之前执行
   （`context.rs:483-506`）。**后果**：新弹出的菜单有一帧不可悬停；`Response` 读到的是本帧分配前的几何
   （`context.rs:1348-1355`）。对我们的右键标注菜单是硬伤——菜单弹出那一帧就点不到。
3. **Id 不稳定**：`Id::with(salt)` 把父 id 与 salt 混成新哈希（`id.rs:83-96`）；`Ui::unique_id` 文档原话是
   **"not stable"**，增删前面的控件会让它变（`ui.rs:349-356`）。冲突检测 `check_for_id_clash`（`context.rs:1163`）
   只在 debug 打警告并画红框——所以过滤/重排的菜单项**必须**自带显式 id salt，否则状态串台。
4. **布局有"估算 + 事后修正"**：`allocate_ui_with_layout` 先按 `desired_size` 占位再跑子内容
   （`ui.rs:1337-1348`）；`Area`/`Grid` 用"尺寸试探 + `request_discard` 重跑一帧"规避，首帧可见抖动。
5. **命中是线性的**：`hit_test` 对全部 WidgetRect 做一次过滤扫描（`hit_test.rs:63-85`），没有空间索引。
6. **`blur_width` 不是背景模糊**：它是"加大羽化宽度"来实现字形/阴影发虚
   （`tessellator.rs:1864-1877` 的注释原话 "We accomplish the blur by using a larger-than-normal feathering"），
   不能糊它下面的内容。
7. **没有分组不透明度与合成层**：`Shape` 是封闭枚举（`shape.rs:27-71`），没有"渲染到纹理再整体混合"的图元，
   半透明子元素重叠时逐形状相乘会合成错。
8. **IME 是每帧的瞬时状态**：`IMEOutput`（`data/output.rs:95-114`）挂在 `platform_output` 上，
   控件这帧没重建就消失（by omission）。
9. **没有捕获/冒泡、没有焦点路径**：`FocusWidget` 单值 + 4 个 bool 的 `EventFilter`。

---

### 1.5 egui 分析报告的 8 条可取 + 6 条要避，及其在本设计中的落点

完整报告见 [`docs/research/egui-0.36.2-analysis.md`](research/egui-0.36.2-analysis.md)。

| 教训 | 落点 |
|---|---|
| ① **输入先于用户代码用两个槽解析**（`potential_click_id`/`potential_drag_id`，`memory/mod.rs:486-498`）——"可拖窗口上的按钮、按钮在滚动区里"这种组合无需控件配合就能判对 | §10.2 状态机（引擎仲裁 click vs drag，控件只读 `Response`） |
| ② **`frame_rect` 与 `widget_rect` 分离**（消耗的空间 vs 画出的矩形，`ui.rs:1239-1251`） | §5.2 布局：`measure` 返回 `(Size, Rect)`，对齐/间距从同一处推出 |
| ③ **软 `max_rect` + 一侧无限游标**（`Region`，`layout.rs:13-48`）——尺寸猜错只会轻微错位，不会被裁掉 | §5.2：约束用 `Constraints { min, max, soft_max }`，`Column` 的游标主轴为 `INFINITY` |
| ④ **Id 是确定性哈希；状态按 `(Id, Type)` 存**（`id.rs:67-88`、`id_type_map.rs`）——免费持久化 + 快速探测 | §5.1 + `cx.state::<T>(id)`；同时把 `check_for_id_clash` 的调试红框也拿来（§14 测试） |
| ⑤ **羽化优于 MSAA**：沿法线的半像素透明边（`tessellator.rs:667-674, 796-810`），分辨率无关、一个 shader 搞定、合成在未知像素上也对 | `ui-backend` 的 GPU 光栅策略（§12.2） |
| ⑥ **按 `(clip_rect, texture_id)` 合并绘制**（`tessellator.rs:1385-1403`）——同类图元连续存放才能合并 | §6.2 的 `BatchPlan`：同 kind 且 `(clip, layer)` 未变就延长同一批 |
| ⑦ **重绘 = 延时 + 原因**（`context.rs:130-170, 255-304`），空闲零成本 | §7 提交策略 + `Damage` **叠加**在它之上，不替代 |
| ⑧ **图集里预光栅化的图元**（圆盘当纹理 quad，`tessellator.rs:1507-1529`）可以推广成阴影 LUT / 模糊核 / 笔刷 | `ui-backend` 纹理图集：**模糊核也进图集**，与 §8 的效果实现共用 |

| 要避的坑 | 本设计怎么避 |
|---|---|
| ① **用上一帧矩形做命中** → 新弹窗一帧点不到 | §3.2.1：layout 在 hit 之前，**用本帧矩形** |
| ② **`request_discard` 当布局修复** → 每帧跑两遍，首帧仍抖 | 不设多趟；要求 `measure` 是纯函数（§5.2），没有"猜错再重跑" |
| ③ **单 Id 焦点 + 层序比较式模态** → id 不在 used_ids 里焦点静默丢失 | §5.6 焦点路径 + 回收时回落到最近存活祖先；§6.1 层是枚举 |
| ④ **preedit 当真实文本、IME 当每帧状态** → undo/校验含混，"关 IME"靠缺省 | §5.7：**合成层**（preedit 不进文档）+ 显式 `ImeOutput` 命令 |
| ⑤ **per-frame `Response` 是唯一信号** → click-vs-drag 的判定窗口泄漏给业务 | §5.4：引擎仲裁完再递 `Response`；press/drag/click 各递一次 |
| ⑥ **扁平 `Order` + 一层子层 + 逐形状不透明度** → 毛玻璃遮罩要渲染到纹理 | §12.0 `PushLayer/PopLayer`（分组不透明度 + 圆角裁剪）从第一天就有 |

### 1.6 GPUI 分析报告的 5 条可取 + 5 条要避，及其在本设计中的落点

| 可取 | 落点 |
|---|---|
| ① **`AnyElement` 用每帧 bump arena**（`element.rs:649-655`、`arena.rs:81-218`）：异构子节点零堆分配、一次批量释放 | `ui-el` 的 `Decl.children` 用 `SmallVec`（§4.3），每帧重建的 arena 由引擎持有；**不在树里存 `Box<dyn Element>`** |
| ② **`GlobalElementId` = 增量哈希的路径**（`element.rs:218-300`）：任意深度 O(1)、便宜 `Eq`/`Hash`，还兼作 accesskit `NodeId` | 我们更进一步：用户直接给**确定性 `Id`**（§5.1），不需要前缀哈希——但把"同一哈希兼作无障碍 node id"这条拿来（无障碍导出时用 `Id.0`） |
| ③ **阶段有类型的绘制状态**（`ElementDrawPhase`，`element.rs:322-347`）：把"测量一次、绘制时复用"编码进类型而不是 debug 断言 | §4.3 的 `BuildOutcome` 把测量结果带进 `prepaint`/`paint`；三个 `Ctx` 类型本来就隔离阶段 |
| ④ **排序切片 + k 路归并的 `Scene`**（`scene.rs:41-185`）：画家顺序保证正确性，同质长连续段给 GPU | §6.2 的**类型化图元 + 操作日志**：数据按类型存、顺序单独存，归并时受 `(clip, layer)` 约束（**我们的 layer 会切渲染目标，不能像 GPUI 那样无条件归并**） |
| ⑤ **`DispatchPhase` + 一个传播布尔**（`window.rs:93-122, 5974-6030`）：捕获/冒泡约 40 行，无需按事件解析目标，能处理"按钮在滚动区在面板里" | §5.5 直接采用，并保留 `default_prevented` 语义 |

| 要避 | 本设计怎么避 |
|---|---|
| ① **每帧两次完整树遍历 + 一次为 layout 请求**（`window.rs:3719-3760`） | 见下方 §1.6.1：**融合布局与 prepaint 的遍历** |
| ② **每个监听器一个全局 `'static` 闭包，过滤写在闭包内部**（`div.rs:2810-2830`）——每次鼠标事件跑全窗口所有监听器，各自再查 `is_hovered` | §5.5：**先算出唯一命中路径，再只派发给路径上的节点**。引擎持有帧，处理器直接借 `&mut State`，不需要 `'static + Fn` |
| ③ **`cx.listener()` / `Rc<RefCell<>>` 存处理器状态** | 同上：处理器是闭包但状态在外（`&mut AppState`），不需要 `Rc<RefCell<>>` |
| ④ **`Entity`/观察者层做元素局部状态** | 已有的按 `Id` 持久化（§5.1 + `cx.state::<T>`）就够 |
| ⑤ **`RequestLayoutState` + `PrepaintState` 两个关联类型**——逼每个自定义元素命名两个类型并贯穿三个签名，还逼 `AnyElement` 擦除它们 | §4.3 只有一个 `Element` trait + 一个 `Decl`；阶段状态存在 `Node` 里，用户不声明类型 |

#### 1.6.1 采纳的核心简化：融合布局与 prepaint 的遍历

GPUI 的三趟（`request_layout` → `prepaint` → `paint`）之所以存在，是因为它用 taffy 做布局，
必须先"请求布局"拿到 `LayoutId`，再在 `prepaint` 里拿 `Bounds`。那份双关联类型的状态机几乎
全部是为这三趟服务的（报告原话：*"that single decision is the root cause of the typestate machine,
both associated state types, and most of the cross-frame state machinery"*）。

我们不用 taffy，`measure` 是纯函数（§5.2），所以**一趟就能拿到底**：

```text
融合趟（自上而下再自下而上，一次遍历）：
  · 递归 measure（父给约束 → 子返回需求）
  · 回程时对每个节点：定 rect、算 mask、登记 hitbox
```

于是每帧的遍历从 **3 趟降到 2 趟**（融合趟 + paint 趟），而且省掉：
- `RequestLayoutState`/`PrepaintState` 两个关联类型；
- "布局请求 → 得到 id → 再查 bounds"的间接层；
- 用户自己实现 `Element` 时要线程化三个签名的心智负担。

**代价**：hitbox 在布局回程登记，而 paint 在之后——这与 egui 相反（它在用户代码前用旧矩形），
与 GPUI 相同（`hit_test` 在 draw 之后）。但因为我们**没有多趟**，不需要 GPUI 那套
"绘制的边界值在 prepaint 就冻结"的 `ContentMask` 快照机制：paint 读到的就是本帧最终 rect。

一句话：**GPUI 的结构复杂度几乎全部绕开，只保留它的排序切片场景与捕获/冒泡机制。**

---

## 2. 目标与非目标

### 2.1 目标

| # | 目标 | 判据 |
|---|---|---|
| G1 | 语法像 GPUI | 一个工具条 = 一个函数 + 链式调用；无 `&mut self` 视图对象 |
| G2 | **有控件树**，但声明仍即时 | 树跨帧存活；用户每帧重新声明；未声明节点帧末回收 |
| G3 | 阶段由引擎驱动（两趟 + 三语义阶段） | `build → [layout+prepaint 融合趟] → dispatch → paint`，顺序不可被用户代码打乱 |
| G4 | 引擎级焦点 | 焦点路径 + Tab 顺序 + 焦点环 + IME 绑定 |
| G5 | 引擎级命中 + 捕获/冒泡 | Capture 正序、Bubble 逆序、可中断、指针捕获 |
| G6 | 区域级脏区 | 每帧产出 `Damage`；`None` 且无动画 → 不提交帧 |
| G7 | **像素后端效果是一等类别** | 马赛克/模糊/液态玻璃及一切后续效果共用一套元数据 + 级联 + 降级链；**新增效果不改引擎** |
| G8 | 后端无关 | 同一份 UI 在 CPU/GPU × Wayland 覆盖层/普通窗口上跑；加 Windows 不改 UI 代码 |
| G9 | IME 完整 | preedit / commit / delete-surrounding / 光标矩形；`done` 批次语义由引擎处理 |
| G10 | 可测 | 布局、命中、脏区、效果级联可在无 GPU 无 Wayland 环境单测 |
| G11 | 离屏画布 + 统一采样 | 内容像素网格与显示解耦；`Nearest`/`Linear` 由引擎按规则选，编辑坐标恒落整数网格（§6.4） |
| G12 | **HDR + 色彩管理** | **存储域与运算域分离**：运算恒在线性光，存储按环境选（SDR 用 `Srgb8`，**无额外转换 pass**）；格式/色彩是 buffer 身份；编码只有一个落点（[`hdr-color-management.md`](hdr-color-management.md)） |

### 2.2 非目标

- 不做通用桌面框架（不做多窗口管理、托盘；无障碍只导出选区与按钮角色）。
- 不做完整 flexbox（只做盒 + 流 + 叠放 + 绝对定位 + 栅格）。
- 不移植 egui 的自绘 UI（旧 `ui/` 33k 行按新接口重写，§11）。
- 不做热重载与可视化 inspector（调试面板够用）。

---

## 3. 总体架构

### 3.1 crate 划分：**独立 UI workspace（6 个 crate）+ 两级 workspace**

评审意见：这个框架本身偏重，应当成为一个**几乎独立的项目**，在新项目里做原型验证并配 examples；
第一批验证目标是 **普通窗口 + Wayland surface** 两种呈现。据此把它拆成**独立 workspace**：

```text
crabsnap-ng/
├── Cargo.toml                 # 主 workspace（app、领域 crate…）
├── crates/…                   # 业务 crate
└── ui/                        # ★ 独立 workspace（自成一个项目，将来可单独发布/复用）
    ├── Cargo.toml             # [workspace] members = {ui-el, ui-engine, ui-paint, …}
    ├── README.md              # 这个项目自己的说明（不含业务）
    ├── examples/              # ★ 原型验证：每个 example 是一个可跑的最小场景
    │   ├── window_hello.rs        # 普通窗口 + 3 个控件（最先做）
    │   ├── window_frost.rs        # 普通窗口 + 毛玻璃 + 分组不透明度
    │   ├── layer_wayland.rs       # Wayland layer surface 全屏覆盖层
    │   ├── effects_zoo.rs         # 各种 PixelBackdrop 程序并排对比
    │   ├── canvas_zoom.rs         # ★ 离屏画布 + 采样：同一张贴图在编辑态(nearest)/呈现态(linear)切换
    │   ├── layout_matrix.rs       # 四种宽度需求 + 增量布局对照
    │   ├── ime_cjk.rs             # 中文输入法全链路
    │   └── drag_drop.rs           # 拖拽 + 放下目标
    └── crates/
        ├── ui-el/        声明层：Element trait、尺寸类、命中区域、布局算法
        │                 **零依赖**（只 std + 自带的几何类型），可单独发布
        ├── ui-engine/    树与执行：RetainedTree、Id、事件、焦点、动画、脏区、效果解算
        │                 依赖 ui-el / ui-paint / ui-text（都是 trait 或纯数据）
        ├── ui-paint/     绘制与光栅：Primitives/Op/BatchPlan、Clip/Layer 栈、CPU 光栅
        ├── ui-text/      TextMeasure trait + 基于 crabsnap-text 的默认实现（可替换）
        ├── ui-backend-gl/    GPU 后端（glow + EGL/GBM）
        └── ui-platform/  呈现与输入：
                        ├─ window/       ×dg-shell 普通窗口（x11/wayland 由 winit 或自写）
                        └─ layer-surface/  wl_surface + layer-shell（覆盖层）
```

**为什么这样切（而不是原来的 4 个 crate）**：

| 切点 | 理由 |
|---|---|
| `ui-el` **零依赖** | 声明层只有类型与算法，能脱离一切跑单测；将来别人要复用"GPUI 风格语法 + Qt 尺寸模型"可以直接拿走 |
| `ui-paint` 独立 | `DrawList` 是**引擎与后端的唯一契约**，它不该藏在引擎里；CPU 光栅也要能单独测 |
| `ui-text` 是 trait + 默认实现 | 引擎只认 `TextMeasure`；`crabsnap-text` 只是默认实现，换成别的排版栈不动引擎 |
| `ui-platform` 里 window 与 layer-surface **并列** | 这是评审要求的"先验证两种情况"：两者**共用同一套 `ui-engine`**，证明引擎真的与呈现目标解耦 |
| `ui-backend-gl` 独立 | GPU 是可选能力（ADR-004）；CPU 后端在 `ui-paint` 里 |

依赖方向：`examples → {ui-platform, ui-backend-gl} → ui-engine → {ui-el, ui-paint, ui-text}`。
**`ui-engine` 不知道 `ui-platform` 存在**——它只产出 `DrawList` 与 `Damage`。

### 3.1.1 与主项目的接线

UI workspace 是独立项目，主项目通过**路径依赖**引用它：

```toml
# crabsnap-ng/Cargo.toml（主 workspace）
[workspace.dependencies]
crabsnap-ui-engine   = { path = "ui/crates/ui-engine",   version = "0.1.0" }
crabsnap-ui-el       = { path = "ui/crates/ui-el",       version = "0.1.0" }
crabsnap-ui-paint    = { path = "ui/crates/ui-paint",    version = "0.1.0" }
crabsnap-ui-platform = { path = "ui/crates/ui-platform", version = "0.1.0" }
```

两件事必须明确（否则"独立项目"是空话）：

1. **主项目的 crate 不允许 `use` `ui-paint`/`ui-platform` 的内部模块**，只允许
   `crabsnap_ui_engine` 的公开 API。由 `xtask arch` 检查（`arch.toml` 里把 `ui/*` 作为**外部项目**处理，
   只允许白名单接口）。
2. **UI workspace 有自己的 profile 吗？** 不。Cargo 的 profile 只从**根 workspace** 取；
   作为路径依赖被主项目构建时，用的是主项目的 profile（`debug=false, opt-level=1`）。
   但**单独开发 UI 时**（`cd ui && cargo run --example window_hello`），
   它用自己的根 manifest —— **必须在那里也写上同样的 profile**，否则开发时的构建产物与最终不一致。
   ```toml
   # ui/Cargo.toml —— 与主项目逐字一致的 profile（§7.1 of architecture.md）
   [profile.dev]
   debug = false
   opt-level = 1
   [profile.test]
   debug-assertions = true
   ```
   `xtask` 增加一条检查：**两个 manifest 的 profile 段必须字节相同**。

3. **验证顺序**（评审要求"先窗口 + wayland surface"）：
   ```text
   ① examples/window_hello      —— 普通窗口 + 布局 + 绘制 + 输入（最小闭环）
   ② examples/window_frost      —— 效果面 + 分组不透明度（验证 §8/§12.0）
   ③ examples/layer_wayland     —— 同一套 UI 代码跑在 layer surface（验证解耦）
   ④ 其余 examples 逐项验证 IME/拖拽/布局策略/效果种类
   ```

### 3.2 树模型：结构持久 + 声明即时

这是 v2 的核心。三层职责严格分开：

```rust
// ① 声明（每帧重建，用户写）
pub trait Element {
    fn build(&self, cx: &mut BuildCx<'_>) -> Decl;   // 本节点的 kind + props + 子声明
}

// ② 树（跨帧存活，引擎维护）
pub struct RetainedTree { nodes: Arena<Node> }

pub struct Node {
    pub id: Id,                 // 稳定标识（用户给）
    pub parent: Option<NodeId>,
    pub kind: NodeKind,         // Row/Column/Stack/Overlay/Text/Image/Effect/…
    pub props: Props,           // 本帧属性（diff 后替换）
    pub rect: Rect,             // 上一帧布局结果（布局只在声明变化时重算）
    pub mask: Rect,             // 上一帧裁剪矩形
    pub layer: Layer,
    pub flags: NodeFlags,       // focusable / hoverable / hit_behavior / …
    pub state: NodeState,       // 跨帧状态（动画值、滚动偏移、文本编辑状态）
    pub generation: u32,        // 回收代数（防 use-after-free）
}

// ③ 执行（每帧，引擎）
impl RetainedTree {
    fn apply(&mut self, decl: Decl);                            // 声明 → 差分 → 更新树
    fn layout_incremental(&mut self);                           // 只对 dirty 子树重算
    fn prepaint(&mut self, events: &[Event]) -> InteractionState;
    fn paint(&mut self, out: &mut DrawList) -> Damage;
}
```

**关键规则**：

1. **用户不持有 `NodeId`**。声明里只给 `Id`（稳定字符串/元组），引擎维护 `Id ↔ NodeId` 映射。树可内部重排、回收、重分配，用户代码不受影响。
2. **差分按 `Id` 对齐**：同一父下 `Id` 相同 → 复用（保留 `state`、`rect`）；`kind` 变了 → 整棵子树重建；`props` 变了 → 标记该节点 `dirty`。
3. **布局增量**：只有 `dirty` 节点及其祖先链上的容器需要重新 `measure`。工具条 hover 高亮只改 `props.color` → 不触发布局，只产生该节点的 damage。
4. **帧末回收**：本帧未被声明的节点进 `retired`，**延后一帧销毁**（让退场动画播完，§9.4）。

### 3.2.1 树带来的第一个硬收益：**本帧几何命中**

egui 的 `hit_test` 用的是 **上一帧** 的 `WidgetRect`（`context.rs:483-506`），因为它在 `begin_pass`
里、用户代码之前就跑完了。这导致"新弹出的一帧点不到"。

有了树，我们把顺序反过来，并且**不付额外代价**：

```text
① build + apply   →  树的形状确定（谁是谁的孩子）
② layout          →  **本帧** 矩形全部算完
③ prepaint+hit    →  用本帧矩形建 HitIndex，再派发事件
```

代价是"事件处理晚了一帧的绘制"，但**交互的视觉反馈在同帧的 paint 里就能体现**（因为 paint 在 hit
之后）。这正是我们右键标注菜单需要的：菜单出现的那一帧就能点。

对照 GPUI：它在 `draw()` 里做完 layout/prepaint 后才 `hit_test`（`window.rs:5975`），与我们同序；
egui 是唯一"用旧几何"的。**这条是三者里我们唯一明确不跟 egui 的地方。**

### 3.3 与两个参考实现的对照

| 维度 | egui | GPUI | 本项目（v2） |
|---|---|---|---|
| 用户怎么写 | 每帧 `ui.add()` | `render()` 返回元素树 | **每帧 `view() -> impl Element`** |
| 结构持久化 | 无（全部重建） | 有（`ElementId` + 状态） | **有（`RetainedTree`），按声明差分** |
| 用户持有节点句柄 | `Id` | `ElementId`/`Entity` | **不持有**（只给 `Id`） |
| 状态存哪 | `Memory` | `Entity` + `ElementState` | **`Node.state`（树内）** |
| 布局 | 估算修正（猜错再重跑） | taffy（CSS flex，多轮测量） | **`SizeClass` 四元组 + 单趟 measure**；可换 taffy（§5.2） |
| 命中 | 后写赢 | hitbox + mask | **HitIndex（层 + mask + 分桶）** |
| 事件 | 无冒泡 | capture + bubble | **capture + bubble** |
| 脏区 | 无 | 无 | **`Damage`** |
| 效果 | 无 | 仅阴影 blur | **`EffectProgram` 类别 + 级联** |

### 3.4 一帧的生命周期

```text
① begin_frame(input)
   · 时钟推进（§9）：Anim 目标 → 当前值，产出 animated damage
   · 读取 IME 批次（§5.7），应用 preedit/commit

② build（用户代码）
   root.view() → Decl 树
   tree.apply(decl)：按 Id 差分 → 复用 / 更新 / 新建 / 标记 retired

③ layout+prepaint 融合趟（引擎，只跑 dirty 子树）—— 见 §1.6.1
   下行：measure（父给约束 → 子返回需求）
   回程：定 rect → content_mask = parent_mask ∩ self.clip → 登记 Hitbox
   → 新矩形≠旧矩形的节点：old ∪ new 进 damage
   → 标记"与其它节点矩形相交"的区域（§7）

④ 交互（引擎，本帧矩形已全部就绪）
   · HitIndex 建好 → 算唯一命中路径 target 与焦点路径
   · 计算 hovered / active / captured / focused（结构差分给 FocusIn/Out）
   · 派发事件：Capture(root→target) → Bubble(target→root)
   · 产出每节点 Response

⑤ paint（引擎，树前序 + 层序）
   · 用户 paint 写 DrawList
   · Effect 节点在此登记：源 = 它之前、同层或更低层的图元（§8.2）
   · 与上一帧按 `owners` 比对 → 每节点图元差异进 damage

⑥ effects（引擎）
   按画家顺序解算每条效果：Unchanged / Fresh / Partial（级联，§8.4）

⑦ end_frame
   Damage::None 且无动画 → 不提交；否则交给 ui-backend
```

**为什么命中登记与绘制分开**（可推导）：如果命中在 paint 里登记，那么任何导致 paint 提前返回的路径（`?`、早退、条件判断）都会让**命中盒缺失**而不是**布局缺失**——前者表现为"看得见点不着"，且只在特定交互序列下复现。分开后：布局完成即确定，命中登记只读布局，paint 只读布局 + 交互结果。**注意"分开"指语义阶段，不是指多跑一趟**——见 §1.6.1。

---

## 4. 声明层：`ui-el`

### 4.1 一个真实例子

```rust
fn tool_button(tool: Tool, active: bool) -> impl Element {
    Button::new(Id::of(("tool", tool.id())))     // 稳定 Id，全树唯一
        .icon(tool.icon())
        .selected(active)
        .hoverable(true)
        .on_click(move |cx, _| cx.action(UiAction::SelectTool(tool)))
        .animate(HeightIf(active, 28.0, 24.0).ms(160))
}
```

`Id::of` 就够——用户不需要知道节点在树里的位置，也不需要持有任何句柄。

### 4.2 三件事的分离

| 概念 | 类型 | 生命周期 | 谁写 |
|---|---|---|---|
| **Element（声明）** | `impl Element` | 构造 → `build` → 丢弃 | 用户 |
| **Node（树节点）** | `ui_core::Node` | 跨帧（按 `Id` 复用） | 引擎写 `rect/mask/flags`；用户写 `props`/`state` |
| **Response（交互结果）** | `ui_core::Response` | 本帧 | 引擎写，控件读 |

**状态归 Node**：`Node.state` 放跨帧数据（动画当前值、滚动偏移、文本编辑状态、拖拽临时值）。用户通过 `cx.state::<T>(id)` 借出来用，不自己存。

### 4.3 核心 trait

```rust
pub trait Element {
    /// 产出本节点的声明（属性 + 子节点）。**必须纯**：不画、不派发事件。
    fn build(&self, cx: &mut BuildCx<'_>) -> Decl;

    /// 融合趟的回程（可选）：拿到本帧 rect 后登记互动。默认由 props 推导。
    /// **没有 request_layout/prepaint 两个关联类型**——见 §1.6.1
    fn prepaint(&self, cx: &mut PrepaintCx<'_>, node: NodeRef<'_>) {}

    /// 绘制（可选，默认按 props 画）。
    fn paint(&self, cx: &mut PaintCx<'_>, node: NodeRef<'_>) {}
}

pub struct Decl {
    pub key: Id,
    pub kind: NodeKind,
    pub props: Props,
    pub children: SmallVec<[Decl; 8]>,
}
```

用户几乎不直接实现 `Element`——组合 `ui-el` 的容器（`Row`/`Column`/`Stack`/`Overlay`/`Pad`/`Sized`/`Grid`）和控件即可；只有自绘控件才实现 `paint`。

### 4.4 两种声明形态

```rust
// ① 组合式（最常见）
fn toolbar() -> impl Element { HStack::new().children([...]) }

// ② 结构体式（有内部参数、要自绘）
struct Loupe { aim: Point }
impl Element for Loupe { /* prepaint + paint 自己实现 */ }
```

### 4.5 为什么仍不做 GPUI 的 `Entity<T>`

保留树之后仍然不需要实体系统：

1. **业务状态归业务**：工具选择、文档内容、会话状态住在 `AppState`，不需要被 UI 框架接管。`Entity<T>` 的价值是"跨视图共享 + 自动重绘通知"，一个 `dirty` 标记位就够。
2. **每帧重新声明**已经保证"视图与世界同步"，`cx.notify()` 那套订阅机制是冗余的。

**结论**：树是引擎的，不是用户对象图的一部分。

---

## 5. 内核：Id、布局、命中、焦点、事件、IME

### 5.1 Id

```rust
pub struct Id(u64);
impl Id { pub fn of(parts: impl Hash) -> Id; }

pub struct NodeId(u32 /* index */, u32 /* generation */);   // 引擎内部，用户不可见
```

- **用户给 `Id`，引擎给 `NodeId`**：树可重排/回收，用户代码不受影响。
- **不做父 id 加盐**（egui 的做法会让 id 随兄弟增删漂移，`ui.rs:349-356` 原文 "not stable"）。我们的规则是"显式路径"，全局唯一。
- 重复 `Id`：`debug_assert` 报错并打印两个构造点的 `Location`（生产环境合并为一个节点并打 WARN）。
- `NodeId` 带 `generation`：节点回收后旧 `NodeId` 变成 `None`，而不是指向新节点（防 use-after-free）。

### 5.2 布局：可插拔策略 + **一次确定性的 measure**

> 这一节回答两个问题：① 布局引擎要不要跑两遍？② 怎样同时支持"跟随父宽/适应内容/固定/flex"四种宽度需求？
> 结论：**不需要跑两遍**，用「尺寸类（SizingClass）+ 一次自上而下的确定性 measure」；
> 布局引擎做成 trait，默认实现自研，**`taffy` 作为可选后端**后期可插入。

#### 5.2.1 先看四个真实框架怎么做（已核对源码/头文件）

| 框架 | 尺寸协商 API | 是否多趟 | 怎么表达"跟随父宽/flex" |
|---|---|---|---|
| **Qt 6** | `QLayoutItem::sizeHint() / minimumSize() / maximumSize() / expandingDirections()`（`qlayoutitem.h:30-33`）+ `QSizePolicy{Fixed,Minimum,Maximum,Preferred,Expanding,Ignored}`（`qsizepolicy.h:23-38`） | **否**。父布局挨个问子项三个尺寸，然后 `setGeometry` 分配 | 策略位：`GrowFlag`（可长到 hint 以上）、`ShrinkFlag`（可缩到 hint 以下）、`ExpandFlag`（**吃剩余空间**）。**"跟随父宽"= 父布局把宽给满；"flex 权重"= Expanding 项按 hint 比例分剩余** |
| **GTK 4** | `measure(orientation, for_size, *minimum, *natural, *baseline…)`（`gtklayoutmanager.h:60-70`）+ `GtkSizeRequestMode{MIN,NAT,HFW}` | **是，但只有 3 次**（宽度 MIN、宽度 NAT、给定宽度求高度），由框架统一调度 | 每个控件只报 min/natural 一对数，**吃不吃剩余空间由父布局决定**（`GtkBoxLayout` 的 expand 挂在**子属性**上，不在子身上） |
| **egui** | 无协商：父给 `desired_size`，子跑完再"修正游标"（`ui.rs:2203-2215`） | 靠"猜错就 `request_discard` 重跑"（`grid.rs:468`） | 只有一个 `max_rect` 上界 |
| **GPUI** | 交给 **taffy**（`taffy.rs:44-45, 236`），自己只提供 `measure` 回调 | taffy 内部多轮（`flexbox.rs:1086,1129,1574` 多次 `measure_child_size`） | CSS flex：`flex_grow/basis/width` |

**关键结论：Qt 与 GTK 都不需要"两遍完整布局"。** 它们用的是同一招——
**子节点把"尺寸空间"报成一组区间 + 策略位，父节点在一个 pass 里完成分配**。
Qt 报 `(min, hint, max, expanding)` 四元组，GTK 报 `(minimum, natural)` 二元组 + 父侧的 expand 属性。
两者都**没有**"先布局一遍看看多宽、再布局一遍按这个宽"这种模式。
真正需要多趟（或多轮）的是 CSS flexbox 的那一类算法（taffy 就是），代价是每次测量都要访问子节点。

#### 5.2.2 我们的模型：`SizeClass` 四元组，一次自上而下

```rust
/// 一个节点在**主轴上**的尺寸意愿。刻意对齐 Qt 的 (min, hint, max, expanding)，
/// 因为它用 4 个数就覆盖了需求里的四种宽度场景。
pub struct SizeClass {
    pub min: f64,              // 再小就坏（文字会被截断的下限）
    pub hint: f64,             // 最舒适的尺寸（"适应内容"就是它）
    pub max: f64,              // 再大也无益（"固定宽度"= min == hint == max）
    pub grow: Grow,            // 主轴是否吃剩余空间；以及权重
    pub shrink: bool,          // 空间不够时能不能压到 hint 以下
}

pub enum Grow { No, Yes, Flex(u16) }   // Flex(weight) 用于按权重分剩余空间
```

四种宽度需求一一对应（**不需要任何特殊分支**）：

| 需求 | `SizeClass` | 谁决定最终宽度 |
|---|---|---|
| **固定宽度** | `min = hint = max = 200, grow = No` | 自己 |
| **适应内容** | `min = 文字下限, hint = 测得宽度, max = INF, grow = No` | 自己 |
| **跟随父宽** | `min = 文字下限, hint = 测得宽度, max = INF, grow = Yes` | 父（把剩余全给它） |
| **flex 权重** | 同上但 `grow = Flex(w)` | 父（按权重分剩余） |

`Column`/`Row` 的分配算法（**一趟**，没有迭代）：

```text
1. 已确定的固定宽度先占位；
2. 按 hint 给其余项分配，累计 sum(hint)；
3. 剩余 = 可用 - sum(已分配)：
     剩余 > 0 且有人 grow=Yes/Flex → 按 (grow 权重) 分掉；
     剩余 < 0 且有人 shrink      → 从 hint 往 min 压（按可压缩量比例）；
     都不满足                     → 溢出（由 §5.2.3 的 bounds 规则决定裁还是撑大）；
4. 每个子节点拿到**确定宽度**后再定高度（自下而上汇总）。
```

对比 Qt：Qt 的 `QLayout` 也是"先问 hint，再按 expanding 分配"，逻辑同构；
对比 GTK：GTK 在 `GtkBoxLayout` 里做同样的事，只是把 expand 放在子属性上。
**这个模型没有任何"再跑一遍"的环节**，所以不存在 egui 那种首帧抖动。

#### 5.2.3 为什么 GTK 还需要 `for_size`（我们要不要）

GTK 的 `measure(orientation, for_size, …)` 里，`for_size` 只在一个场景下非 -1：
**`GtkSizeRequestMode::HEIGHT_FOR_WIDTH`** 的控件——典型是**自动换行的多行标签**
（给定宽度才知道折成几行、因此多高）。这种依赖是**单向**的：高度依赖宽度，宽度不依赖高度。
所以 GTK 只是一次深度优先、**高度依赖宽度**，不存在环。

我们的情况更简单：**文字排版已知宽度就确定高度**（`crabsnap-text` 的 `TextEngine` 正是这个接口）。
所以：

```text
measure(node, axis, for_size) -> (min, hint)
```

- `axis = Width` 时 `for_size` 省略（宽度不依赖高度）；
- `axis = Height` 时 `for_size = 已分配的宽度`，文字节点据此折行。

**结论：仍然是单趟，不需要二次布局。** 唯一的例外是"高度反过来影响宽度"的控件（我们没有：
没有等宽网格里随高度变宽的项），出现时按 GTK 的办法标 `RequestMode::WidthForHeight` 并允许一次重算——
这是个**后门**，不是默认路径。

#### 5.2.4 可插拔：布局引擎是 trait

```rust
pub trait LayoutEngine {
    /// 完整解算：给定根约束 → 每节点 rect。
    fn solve(&mut self, tree: &mut RetainedTree, root: NodeId, constraints: Constraints) -> LayoutStats;

    /// 增量能力：引擎能不能只解算 dirty 子树。默认 false → 引擎每次都全量。
    fn supports_incremental(&self) -> bool { false }
}

pub struct BuiltinLayout;      // 自研：SizeClass + 单趟分配（默认）
pub struct TaffyLayout;        // 可选 feature：把 Style 映射成 taffy Style，借它的 flex/grid
```

**这样做的理由**：

1. 自研实现覆盖我们**全部**现有界面（工具栏/菜单/面板/对话框/设置页都是盒 + 流 + 叠放），
   它可预测、可单测、无 C 风格复杂度；
2. 但布局语义是**最容易变**的部分（将来设置页可能要真正的 flex/grid），
   所以把 `LayoutEngine` 做成 trait，`taffy` 挂在 feature 后面：
   ```toml
   [features]
   default = ["layout-builtin"]
   layout-builtin = []                      # 无依赖
   layout-taffy   = ["dep:taffy"]           # 换成 taffy 后端
   ```
3. **两套引擎共用同一个测试套**：同一棵 `RetainedTree` + 同一组约束，
   断言两者产出的 rect 在容差内一致（只对两者都支持的原语）。
   这样"换引擎"不是重写，而是回归测试换一组期望值。

**代价（必须说清）**：`taffy` 用 CSS 语义（`AvailableSpace::Definite/MinContent/MaxContent`），
而我们的 `SizeClass` 是 Qt 语义。映射不是一一对应——`grow` 要翻成 `flex_grow`，
`min/hint/max` 要翻成 `min_width/width/flex_basis`。**所以 `TaffyLayout` 只保证"盒 + 流"这一子集等价**，
超出部分（真正的 flex 换行、grid）只在 taffy 后端可用。这与"引入 taffy"的初衷一致：
它提供的是**我们自研没实现的那部分**，而不是替代整个布局。

#### 5.2.5 增量规则

```text
dirty 来源：
  · props 变了（颜色/文字/图标/尺寸约束——影响 measure 的才算）
  · 子节点集合变了
  · 祖先的约束变了（父变宽 → 子树要重测）

重算范围 = 最上层 dirty 节点的子树；非 dirty 节点直接复用 rect
```

**注意**：增量是**引擎的能力**而不是框架的假设。`supports_incremental() == false` 时（如 `TaffyLayout`）
每帧全量解算——布局全量的代价在几百节点上是微秒级，不值得为它牺牲引擎可替换性。

#### 5.2.6 约束与布局原语

```rust
pub struct Constraints { pub min: Size, pub max: Size, pub soft_max: Option<Size> }
```

`soft_max` 学 egui 的"软上界 + 一侧无限游标"（`layout.rs:13-48`）：尺寸猜小只表现为轻微错位，不裁内容。

| 原语 | `SizeClass` 主轴 | 语义 | 参考（成熟框架） |
|---|---|---|---|
| `Row`/`Column` | 由子项决定 | 主轴顺序 + 间距 + 交叉轴对齐 | GTK `GtkBoxLayout`、Qt `QBoxLayout` |
| `Stack` | 取子项最大 | 子节点互相叠放 | Qt `QStackedLayout`（尺寸语义） |
| `Overlay` | 脱离主轴 | 相对父矩形绝对定位 | Qt `setGeometry` + `QWidget::move/resize` |
| `Pad`/`Sized` | 叠加 min/hint/max | 内边距 / 显式尺寸 | Qt `contentsMargins` + `sizePolicy` |
| `Grid` | 列宽固定或 `Flex` | 等宽列 / 按权重分列 | Qt `QGridLayout`、CSS `grid-template-columns` |

**文本测量**走 `ui_core::TextMeasure` trait（`crabsnap-text` 实现），
按 `(text, style, max_width)` 缓存——这是唯一"高度依赖宽度"的测量。

### 5.3 命中索引

```rust
pub struct HitEntry { node: NodeId, bounds: Rect, mask: Rect, behavior: HitBehavior, z: (Layer, u32) }

pub struct HitIndex {
    entries: Vec<HitEntry>,
    grid: HashMap<(i32, i32), SmallVec<[u32; 4]>>,   // 16×16 分桶
}
```

判定：按 `z` 降序 → `bounds` 与 `mask` 都包含点 → 第一个命中（`PassThrough` 继续往下）。

### 5.4 `Response`

```rust
pub struct Response {
    pub hovered: bool, pub pressed: bool, pub clicked: bool, pub double_clicked: bool,
    pub right_clicked: bool, pub dragging: bool, pub drag_started: bool,
    pub drag_delta: Vec2, pub drag_ended: bool, pub has_focus: bool,
    pub edited: Option<TextEdit>,
}
```

引擎负责去重：同一帧多个 `on_click` 只有最上层那个拿到 `clicked`。

### 5.5 事件分发：捕获 → 冒泡

```rust
target = captured.unwrap_or_else(|| hit_index.topmost(pos));
path   = ancestors(target) + target;        // 键盘事件用 focus.path
for n in path        { handler(n, Capture); if stopped { break } }
for n in path.rev()  { handler(n, Bubble);  if stopped { break } }
```

语义表（必须写死，否则每个控件自己猜）：

| 规则 | 说明 | 为什么 |
|---|---|---|
| 路径 = 祖先链 | 不是几何重叠链 | 菜单与工具栏几何重叠但不在同一路径 |
| Capture 正序 / Bubble 逆序 | root→target / target→root | 与 GPUI `window.rs:5990-6009` 一致 |
| `stop_propagation` 跨阶段生效 | 捕获阶段停掉，冒泡整个不跑 | 同 GPUI `cx.propagate_event` |
| 滚轮先 Bubble | 最内层滚动容器优先消费，未消费再往上 | 唯一"内层优先"的常规事件 |
| 谁算消费 | 返回 `Handled::Yes`；Bubble 末尾无人消费才给全局快捷键表 | 否则快捷键抢走文本框按键 |
| `FocusIn/Out` 是结构差分产物 | 两帧焦点路径不同 → 合成事件 | 控件靠它提交编辑 |
| 不合并同类事件 | 除 `PointerMove` 只保留最后位置 | 合并会把"快速双击"变单击 |

**右键菜单的例子**：

```text
PointerDown(左键)，位置在菜单外
命中 topmost = 菜单遮罩（Popup 层，Block）
路径 root → overlay → menu_mask
Capture: overlay(记录) → menu_mask(关闭菜单, stop) → Bubble 不执行
工具栏收不到 → "点外面关菜单且不触发下面的按钮" ✓
```

### 5.6 焦点与键盘

```rust
pub struct FocusState {
    focused: Option<NodeId>,
    path: SmallVec<NodeId>,       // 根 → focused
    ring: FocusRing,              // Always / KeyboardOnly（默认）/ Never
}
```

1. Tab / Shift-Tab 按**树前序**遍历 `focusable` 节点（不做几何排序——浮层场景会乱跳）；
2. 鼠标按下可聚焦节点 → 焦点转移；
3. **节点被回收 → 焦点回落到 `path` 中最近的存活祖先**（有树才做得到；egui 只能丢弃）；
4. 键盘事件先给 `focused`，冒泡到根，最后才是全局快捷键表；
5. `Escape` 由 `scope` 栈决定给谁：嵌套编辑 → 菜单 → 覆盖层；
6. 焦点环默认只在键盘输入后显示（`FocusRing::KeyboardOnly`）：指针点击时显示焦点环等于把"键盘焦点"与"点击位置"混为一谈。

### 5.7 IME

```rust
pub struct ImeOutput { area: Option<Rect>, cursor_rect: Option<Rect>, surround: Option<String> }

pub enum ImeEvent {
    Batch { preedit: Option<Preedit>, commit: Option<String>, delete: Option<DeleteSurrounding> },
}
```

规则：

- **焦点节点拥有 IME**（`ime.owner == focus.focused`），不是全局单值（egui 的问题）。
- **`done` 之前不应用任何子事件**——`zwp_text_input_v3` 规范规定 preedit/commit/delete 都只写 pending 状态，`done` 才是这一批的提交标记（**规范事实，见 wayland-protocols 的 XML**）。
- **preedit 由引擎绘制**：在 owner 的 `cursor_rect` 处叠一层带下划线的文本命令，控件只画已提交文本。中文输入法在任何文本框里表现一致。

---

## 6. 层、裁剪、绘制列表

### 6.1 层

```rust
pub enum Layer { Base, Chrome, Popup, Tooltip, Debug }
```

层决定三件事：绘制顺序（层序优先于树序）、命中顺序（逆层序）、**效果面的作用域**（只影响同层及以下）。

层是**枚举**而不是整数：把层写成 `u32` 则"Tooltip 画在 Popup 下面"这种错误只能运行期发现，写成枚举则编译期就排除。

### 6.2 绘制表示：三层（图元数据 / 操作日志 / 批次计划）

> 本节结论来自对 egui、GPUI、Qt Quick 的源码核对，详见
> [`research/declaration-to-rendering.md`](research/declaration-to-rendering.md)。

**不做单一 `enum Cmd`。** 主流框架都把"顺序"与"图元数据"分开存
（GPUI：`Vec<PaintOperation>` + 并行的 `Vec<Quad>`/`Vec<Path>`…，`scene.rs:41-58`），
因为只有类型化才能被后端整段批量上传。

```rust
/// ① 图元数据：每种一个 `Vec<T>`，`#[repr(C)]`，可直接当实例数组上传。
/// Primitive data: one typed vec per kind, `#[repr(C)]`, uploaded as instance arrays.
#[derive(Default)]
pub struct Primitives {
    pub rects:    Vec<RectPrim>,
    pub paths:    Vec<PathPrim>,
    pub glyphs:   Vec<GlyphPrim>,
    pub images:   Vec<ImagePrim>,
    pub canvases: Vec<CanvasPrim>,
    pub shadows:  Vec<ShadowPrim>,
    pub effects:  Vec<EffectPrim>,      // §8 的像素后端效果面
}

/// ② 顺序日志：只存"哪种图元、第几个"，4 字节一条。
/// Paint-order log: kind + index, 4 bytes per entry.
pub enum Op {
    Prim   { kind: PrimKind, index: u32 },
    Clip    { id: ClipId },  PopClip,
    Layer   { id: LayerId }, PopLayer,   // §12.6 分组不透明度 / 圆角裁剪
    Effect  { index: u32 },              // 触发 §12.3 的 pass 序列
}

pub struct DrawList {
    ops: Vec<Op>,
    prims: Primitives,
    /// 每个图元所属的节点，供 §7 的按节点差分用。
    /// Owning node per primitive, for per-node damage diffing.
    owners: Vec<NodeId>,
    clips: Vec<ClipEntry>,
}
```

**③ `BatchPlan`：归并成可提交的批次**

```text
扫描 ops：
  同 kind 且 clip / layer 未变  → 延长当前 batch 的 range
  clip 或 layer 变了            → 结束当前 batch，开新 batch
  Layer(id)                     → 结束当前 pass，切到离屏目标
  Effect(index)                 → 结束当前 pass，插入效果 pass 序列（§12.3）
```

**与 GPUI 的实质差异（不要照抄）**：GPUI 的 `batches()` 是纯粹的 k 路归并
（`scene.rs:172-185`），前提是同种图元可以跨任意顺序合并。我们的 `Layer` 会切换**渲染目标**，
跨目标的图元**不能**合并，所以归并必须受 `(clip, layer)` 段约束。

**`Op` 里的控制项与图元分开的原因**：`Clip`/`Layer`/`Effect` 没有"矩形"语义，
如果与 `Rect` 放进同一个枚举，每加一种控制操作都要改所有后端的 `match`——
Qt Quick 把它们做成独立节点类型（`QSGClipNode`/`QSGOpacityNode`，`qsgnode.h:155-295`），同一判断。

### 6.2.1 实例化还是三角化

| 图元 | 策略 | 理由 |
|---|---|---|
| `Rect`/`Image`/`Glyph` | **实例化**（4 顶点由 VS 生成） | 形状规整、数量最多；`draw(0..4, instances)` 一批一次调用 |
| `Path` | **CPU 三角化** | 路径不同质，实例化无收益；CPU 后端也要同一份三角数据 |
| `Shadow` | 实例化（SDF + 解析积分） | 与 `Rect` 同构 |

跨后端一致性由"同一份图元数据 + 两条等价实现"保证（§12.4）。

### 6.3 content mask

`content_mask = parent_mask ∩ self.clip`，在**布局回程**确定，paint 不能修改。否则命中与绘制会用到不同的裁剪，表现为"看着在框里却点不到"——这类不一致只有在裁剪动态变化时才暴露。

---

### 6.4 离屏画布与采样引擎（贴图工具的关键路径）

> 需求（v3 补充）：*"贴图可能会需要进行缩放，但是对它的编辑都需要在图像本身的像素网格内进行，
> 所以 UI 框架应有能力处理离屏画布在渲染后通过统一的采样引擎渲染到屏幕上（最邻近、插值，
> 分别对应图像标注编辑状态下的保留像素感，和作为贴图非编辑只呈现状态下的平滑化处理）"*
>
> **v3.0 的图元里只有 `ImagePrim { sampling }`，缺"离屏画布"这个源的身份**——
> 这一节补上。结论：架构允许，但需要三个明确的部件。

#### 6.4.1 三层分离：内容网格 → 显示尺寸 → 屏幕

**三层分离是可推出的**：内容网格是离散的（像素有边界），显示是连续的（屏幕坐标），
两者之间是任意比例的重采样；只要把它们当一个坐标系用，任何非整数倍缩放都会让"编辑坐标"
与"屏幕坐标"无法互相还原。新设计把这三层做成**不同类型**，让混用编译不过：

```text
① 内容网格（Canvas native）   贴图的**像素网格**：W×H 个像素，每个像素有明确边界。
                              所有编辑（画标注、打码）都在这个网格里进行，坐标以像素为单位。
② 显示尺寸（display size）    = native × scale × device_scale（四舍五入）。
                              这是"这块内容在屏幕上占多少物理像素"。
③ 屏幕位置（screen rect）     显示尺寸落在视口中的位置。
```

**关键约束**：`①②③` 三者分离，且**编辑坐标永远在 ①**。
反例（可推导）：若预览用逻辑坐标、导出用物理坐标，同一份标注在两边会差一个 `scale` 倍——
预览里在 A、导出后落到 B。这不是"某个项目踩过的坑"，是坐标系混用的**必然结果**。

```rust
pub struct CanvasId(u64);

pub struct Canvas {
    /// 内容网格：所有编辑坐标系的原点与单位。
    pub native: PictureRect,          // 像素整数矩形
    /// 缓冲格式与色彩描述是身份的一部分，构造时**必须同时给出**（见 hdr-color-management.md §1）。
    /// Format and colour are part of identity; both are required at construction.
    pub format: PixelFormat,
    pub colour: ColorDescription,
    /// 内容本身（后端持有：CPU = Pixels，GPU = 纹理）
    handle: BackendTexture,
    /// 内容代数：内容变了才递增。**不能用 `Arc::as_ptr`**——那是指针地址，分配器会复用
    generation: u32,
    /// 脏区（相对 ① 的像素坐标）
    dirty: Region,
}
```

#### 6.4.2 统一采样引擎

**采样是引擎的一个部件，不是每个控件自己调 `draw_image`。**

```rust
pub struct SamplingEngine { kernels: KernelSet, cache: SampleCache }

pub enum Sampling {
    /// 最近邻：保留像素感。编辑态放大必须用它——见 §6.4.4 的三条规则。
    Nearest,
    /// 双线性/更高质量：非编辑态呈现用，缩放平滑。
    Linear,
    /// 自适应（默认）：缩小恒平滑（nearest 缩小会丢像素），放大按显式意图。
    Auto(SampleIntent),
}

pub enum SampleIntent { PreservePixels, Smooth }
```

**缓存键必须含运算域**（这是 HDR 需求引入的真 bug，见 review A3）：

```rust
pub struct SampleKey {
    canvas: CanvasId, generation: u32,
    display_size: (u32, u32), visible: Region,
    sampling: Sampling,
    /// 插值在哪个空间做。缺了它，SDR 的缓存会被 HDR 路径命中。
    domain: StorageDomain,
}
```

同一份内容在 `LinearF16` 与 `Srgb8` 域下线性插值结果不同（编码域插值暗部偏暗，
实测黑白棋盘差 61/255）。缓存键不含域 → "切到 HDR 后边缘偏暗，且只在被缓存过的区域出现"。

**规则**：`Linear` 插值**必须在线性光域**做（目标为 `Srgb8` 时用 LUT 往返，§5.3.3）；
`Nearest` 可跳过线性化（逐像素复制，两域等价）。

**一致性判据不是"逐像素相等"**（原文档写错了，与 §12.4 的矛盾同理）：

| 不变量 | 测法 |
|---|---|
| `Nearest` 是恒等 | 1:1 时逐像素等于源（这个真的能相等） |
| 线性是凸组合 | 任意点结果落在两邻点 [min,max] 内（无过冲） |
| 线性可复现 | 同一后端重复采样结果完全相同 |
| 位置误差 | CPU vs GPU 的 50% 等值线位置差 < 0.5px |

引擎按上述键缓存，**可见区之外不采样**；拖动/缩放时只重采样新暴露的条带。

#### 6.4.3 离屏画布作为图元源

离屏画布（贴图、标注编辑中的合成结果、`Layer` 的离屏结果）都能作为 `CanvasPrim`
被采样上屏：

```text
① 业务往 CanvasId 对应的离屏目标里画（坐标 = 内容网格像素）
② 引擎在 paint 阶段遇到 `CanvasPrim { id, rect, sampling, native }`
③ 采样引擎按 (native → rect) 做重采样 → 写入 DrawList 的目标位置
```

`native` 必须显式给出，**不能让引擎从纹理尺寸推**——因为编辑态的画布可能比内容网格大
（含临时边距/吸附余量），从纹理尺寸推会把边距算进网格，所有像素坐标整体偏移。

#### 6.4.4 三条采样规则（**从几何推出**，不是从旧项目继承）

> 证据等级：**可推导**（AGENTS.md §1.2 通道 3）。下面每条都能自己推一遍，不依赖旧项目的任何说法。
> 旧项目的笔记只是**碰巧也提到了**相似结论——按 AGENTS.md §1，那不算证据。

| 规则 | 内容 | 为什么（第一性原理） |
|---|---|---|
| **R1 编辑态判据是几何事实，不是用户偏好** | `nearest = (scale > 1.0)`，**不看**任何"显示态是否像素化"的设置开关 | 编辑态的需求是"看得见内容像素的边界"；放大时最近邻才把每个内容像素映射成清晰的方块，双线性会产生内容网格里不存在的过渡色。这与用户是否喜欢像素风**无关**——混在一起会让几何约束依赖一个可关的偏好 |
| **R2 缩小时恒平滑** | `scale ≤ 1` 时即便编辑态也用 `Linear` | 缩小是**多对一**映射：最近邻会直接丢弃未被采样的内容像素（抽样混叠）。此时平滑映射的是"平均"，恰好保留更多信息 |
| **R3 缩放与 DPI 分开算** | `editing_nearest = scale × device_scale > 1.0` 只回答"放大没放大"；显示态像素化是独立开关 | 决定"内容像素是否被放大成多个屏幕像素"的是**总倍率**，不是用户缩放值。DPI=2 时 `scale=1` 已经是 1 内容像素 → 2 屏幕像素，此时不放大就会糊 |

因为采样是引擎的部件，**这些规则写在引擎里一处**，业务代码只声明 `SampleIntent`。
把规则分散到调用点的害处是可推导的：N 个绘制点就有 2^N 种"谁记错"的可能，
而其中一种错（编辑态用了平滑）在缩放下肉眼不可见、放大后才暴露，是最难发现的一类。
**这不是"旧项目怎么错"，而是"任何把规则交给调用方的设计都会错"**——所以收进引擎。

#### 6.4.5 编辑坐标与显示坐标的转换

```rust
pub struct CanvasMapping { native: PictureRect, screen: Rect }
impl CanvasMapping {
    /// 屏幕坐标 → 内容像素坐标（**编辑用，结果是整数像素**）。
    pub fn to_native_pixel(&self, screen: Point) -> PicturePoint;
    /// 内容像素 → 屏幕坐标（绘制用）。
    pub fn to_screen(&self, pixel: PicturePoint) -> Point;
    /// 显示态像素化：只影响采样方式，不影响坐标。
    pub fn magnified(&self) -> bool;
}
```

**不变量**：`to_native_pixel` 的结果**永远落在 ① 的整数网格上**，不管当前是 nearest 还是
linear、不管 scale 是多少。这就是需求原话"编辑都需要在图像本身的像素网格内进行"的落点——
它由类型保证，不是由调用方自觉。

### 6.5 为什么这些放引擎而不是放业务

| 能力 | 放在业务侧会怎样 | 放在引擎（新设计） |
|---|---|---|
| 采样方式选择 | 每个绘制点手写 `nearest` | `Sampling` 一个枚举 + 三条规则在 §6.4.4 |
| 重采样缓存 | 各自的 `Rescaled` | `SamplingEngine` 统一缓存与可见区裁剪 |
| 坐标换算 | 各处 `× scale × device_scale` | `CanvasMapping` 唯一实现 |
| 编辑网格约束 | 靠调用方传对整数坐标 | `to_native_pixel` 返回整数类型 |
| 内容代次 | `Arc::as_ptr` 判变化（地址会被复用！） | `generation` 显式递增 |

最后一行可推导：`Arc::as_ptr` 返回的是**分配器地址**，不是内容身份；分配器复用地址后
"内容变了"与"内容没变"无法区分。新设计用显式 `generation`，从类型上堵死。

---

## 7. 脏区：`Damage`

```rust
pub enum Damage { None, Rects(SmallVec<[Rect; 8]>), Full }
```

来源四处：

1. **布局变化**：`old ∪ new`（只报新矩形会在旧位置留残影——新矩形不覆盖旧矩形，那条边没人清）；
2. **绘制命令变化**：按 `DrawList.owners` 分组比较（每个节点的图元集合与上帧同节点比较），不比像素。
   这一点必须从第一天就做对——egui 把所有形状丢进一个扁平 `Vec` 再合并（`tessellator.rs:1385-1414`），
   一旦扁平化，**某个控件没变这件事就不可恢复**，所以它永远做不了区域脏区；
3. **动画推进**：变化中的节点矩形 + 落定帧（§9.3）；
4. **效果级联**：效果面重算区域（§8.4）。

**相交修正**：某节点的矩形变化会盖到别的节点上（尺寸动画推动邻居），所以 damage 必须扩到"所有与本帧/上帧变化矩形相交的节点矩形"。这一条靠树才做得准——树上知道谁和谁相交，纯即时模式只能保守地整窗重绘。

提交策略：`None` 且无动画 → **不提交**（daemon 空闲时 CPU 占用为 0）；`Rects` → 按 `PresentTarget` 能力选 `damage_buffer` 或整帧；`Full` → 整帧。

---

## 8. **像素后端效果**（Pixel-Backdrop Effects）——本设计的核心抽象

> 需求原话（v3 澄清）：*"不是液态玻璃，而是一切依赖底层像素并通过处理得到一层处理后纹理的效果——
> 高斯模糊、马赛克、液态玻璃等等"*。
>
> 这句话改变了这一节的性质：**我们要建的不是"几个效果"，是一个效果类别**。
> 类别成立的条件是：**新效果不该要求改引擎**。下面先证明这一点，再谈实现。

### 8.1 判定标准：什么样的效果属于这一类

一个效果属于"像素后端效果"当且仅当它满足四条：

| # | 条件 | 反例（不属此类） |
|---|---|---|
| B1 | **读取一块已渲染的像素**（不是凭空生成） | 纯色填充、渐变、阴影（它们不需要读下方） |
| B2 | **输出一块同尺寸的处理后纹理** | 模糊是；但"画一个圆"不是（输出形状取决于自己而非输入像素） |
| B3 | **只读，不写回源** | 所有此类效果；写回的那是"标注"（打码工具移动马赛克块） |
| B4 | **有可保证的有限采样半径**（`reach.guaranteed`） | 半径可 ∞ 的效果（如"整屏对比度拉伸"）不属此类，得走全帧后处理 |

四条合起来给出一个**统一的元数据四元组**，这就是整个抽象的落点：

```rust
/// 效果的影响范围。**保证与估计分开**——见 [`review-semantics-vs-means.md`](review-semantics-vs-means.md) A1。
/// Guarantee vs estimate are separated: `guaranteed` is contract, `actual` is efficiency.
pub struct EffectReach {
    /// 算法**保证**的半径：引擎据此扩 damage。超出即错误，不是"取整选择"。
    pub guaranteed: u32,
    /// 算法**实际**用到的半径（可小于保证值，避免过度重算）。
    pub actual: u32,
}

pub struct PixelBackdrop {
    /// 影响范围。**引擎只读字段，不认识算法。**
    pub reach: EffectReach,
    /// 平移复用约束：`None` = 无网格要求（可亚像素平移）；
    /// `Some(n)` = 平移必须是 n 的整数倍，否则整块重算（马赛克）。
    pub aligned_translation: Option<u32>,
    /// 参数指纹（进缓存键）。
    pub param_key: u64,
}
```

**引擎只认这三个字段**：用 `reach.guaranteed` 算级联与裁边、用 `aligned_translation` 判断
能不能复用、用 `param_key` 判断缓存对不对。**它完全不知道什么是模糊、什么是玻璃。**

两处修正（原设计把实现手段当成了契约）：

| 原来 | 问题 | 现在 |
|---|---|---|
| `pad: u32`（算法自己算好交上来） | `⌈3σ⌉` 是**当前实现的选择**，不是高斯模糊的语义。改成 5σ 会让所有 damage 计算悄悄变化，引擎无从校验 | `EffectReach { guaranteed, actual }`，且必须有一条**脉冲输入实测半径 ≤ guaranteed** 的测试 |
| `extends: EdgeMode` | 引擎**不读它**——它只在算法的卷积里用。放进引擎元数据违反了"三个正交点" | 移到**源描述**（`EffectSource.outside`），由"这是贴图还是屏幕覆盖层"决定，不是算法决定 |

### 8.2 算法从哪里来：三条注册通道

```rust
pub trait EffectProgram: 'static {
    fn metadata(&self, params: &EffectParams) -> PixelBackdrop;
    /// CPU 执行器（必需）：语义在线性光域，存储随目标域（LUT 往返，无需 f16 中间缓冲）。
    /// Linear-light semantics; storage follows the target domain (LUT round-trip).
    fn cook_cpu(&self, src: &Surface, dst: &mut Surface, plan: &EffectPlan);
    /// GPU 执行器（可选）：没有就由引擎调度到 CPU → 纹理上传
    fn cook_gpu(&self) -> Option<ShaderProgram>;
}
```

三条通道，面向不同作者：

| 通道 | 形态 | 适合 | 是否需要重编译 |
|---|---|---|---|
| **内置**（Rust trait） | 引擎自带 `Blur` / `Mosaic` / `Refraction` 等 | 我们自己的常用效果 | 否（随 crate 编译） |
| **注册**（`register(EffectId, Box<dyn EffectProgram>)`） | 任何 crate 实现 trait 后注册 | 业务侧一次性效果（调试浮层专用） | 否 |
| **热载 shader**（只 GPU） | `include_str!("my.frag")` 或运行时载入 → `EffectProgram` 的 GPU 部分 | 设计/调参阶段快速迭代液态玻璃 | 否（编译期字符串） |

**关键**：三条通道产出的是**同一个** `EffectProgram`，因此它们的级联、缓存、降级行为完全一致。
不存在"内置效果好、自定义效果差一截"的情况。

### 8.3 为什么"新增效果不改引擎"能成立（三个正交点）

前几版把效果分成 `Mosaic / Blur / LiquidGlass / Shader` 四个枚举变体——**那是错的**，
因为它把"算法"和"引擎关心的元数据"焊在一起，加一个效果就要改匹配分支。v3 拆成三个**正交**点：

| 正交点 | 谁负责 | 引擎是否需要知道 |
|---|---|---|
| **需要多大的源**（`reach.guaranteed`） | 算法声明**保证值**与**实际值** | 只知道数字，且在 `debug` 下断言 `actual ≤ guaranteed` |
| **网格相位**（`grid`） | 算法自己声明 | 只知道数字 |
| **怎么算**（`cook_cpu` / `cook_gpu`） | 算法自己实现 | **完全不知道** |

于是 `Blur` 与 `LiquidGlass` 的差别只体现在三个数和一个函数体上，引擎的级联解算（§8.5）对两者
跑的是**同一段代码**。这就是类别的意义。

### 8.4 效果面在命令流里的位置

```rust
ui.pixel_backdrop(Id::of(("frost","toolbar")), rect, radius).program(Blur::gaussian(16.0));
// 引擎内等价于往 DrawList 推一条
EffectPrim { id: EffectId, rect: Rect, radius: Corners, param_key: u64 }   // + Op::Effect{index}
```

**作用域**：源 = `Op` 序列中它**之前**、已提交到当前渲染目标、与 `rect` 相交的图元
（依赖顺序 = 画家顺序，用户不必描述"谁在谁下面"）。这也是顺序日志与图元数据
必须分开、且日志保持全局有序的原因——效果的语义就是"这个位置之前已画出的像素"。

**采样源必须扩 `reach.guaranteed`**：外扩后的矩形才是真正要读的源区域；输出只写回 `rect` 内。
这两件事**必须绑在一起**（分开给就会有人只拿区域忘了外扩，边界那圈像素不重算），
这里**收进一个类型**，调用方拿不到"只给区域忘了外扩"的组合。

#### 8.4.1 源的边缘策略（原 `extends` 移到这里）

`EdgeMode` **不属于引擎元数据**——引擎不读它，它只在算法的卷积实现里用。
它属于**源描述**，由"这是贴图还是屏幕覆盖层"决定：

```rust
pub struct EffectSource {
    /// 边界内是有效内容；边界外按 `outside` 扩展。
    /// Inside `bounds` is valid content; outside extends per this policy.
    pub bounds: Rect,
    pub outside: EdgeMode,   // 由 canvas/屏幕语义定，不是算法定
}
```

- 贴图：`Clamp`（内容在边缘之外确实"延续"）；
- 全屏覆盖层：`Clamp` 会把屏幕外的假色拉进来 → 必须 `Transparent` 或 `Reflect`。

算法读它做卷积，引擎只负责**把它设对**。

### 8.5 能力协商：三种降级，绝不留白

效果不是"支持或不支持"的二值，而是一条**降级链**：

```rust
pub trait EffectBackend {
    fn capabilities(&self) -> EffectCapabilities;    // { max_reach, fp16, shader_programs, max_texture }
    /// 这个程序在本后端能不能跑、跑什么。不能则给出替身程序。
    fn resolve(&self, prog: &dyn EffectProgram) -> Resolution;
}

pub enum Resolution {
    Native,                                   // 原样
    Substitute(&'static dyn EffectProgram),   // 用等价/近似效果顶替（液态玻璃 → 高斯模糊）
    CpuThenUpload,                            // GPU 后端没有该 shader：CPU 算完再上传纹理
    Unsupported,                              // 彻底不行 → 引擎记录 + 调试面板可见
}
```

**`CpuThenUpload` 是这一类的关键保底能力**：只要效果有 `cook_cpu`（必需），它就能在任何后端上
出结果——GPU 只是"更快"，不是"能不能"。这样"新写一个 shader 效果"不会因为某个后端没实现
就消失。

`Unsupported` 时**必须有声音**（静默跳过意味着"效果消失"和"效果本来就这样"无法区分）：
引擎记 `EffectDiag { id, reason }`，调试面板列出来，`--diag` 模式打 WARN。

### 8.6 级联解算（核心技术点，对**所有**效果同跑一段代码）

```rust
pub struct EffectEntry {
    last_rect: Option<PixelRect>,   // 上帧实际计算过的矩形（不含采样边）
    last_params: u64,               // param_key（圆角不参与：它是后处理）
    cache: Option<BackendCache>,    // CPU 像素块 / GPU 纹理
}
```

每帧三步（纯逻辑，可单测）：

```text
① new = clip(effect.rect, canvas)      —— floor/ceil 后夹回画布
② reusable: param_key 相同 && (aligned_translation 为 None 或平移是其整数倍)
   （不要求覆盖：平移时 old∩new 恰恰能复用，要求覆盖会堵死"移动"这条路）
③ solve（用该效果自己的 reach.guaranteed 与 aligned_translation）:
     B       = new \ old                                   新暴露区
     A       = (damage ± reach.guaranteed) ∩ old ∩ new     缓存有、源变过
     compute = ((A ∪ B) ± reach.guaranteed) ∩ new          有采样边再外扩一圈
     reuse   = (old ∩ new) \ compute
```

```rust
pub enum Plan { Unchanged, Fresh, Partial { compute: Vec<PixelRect>, reuse: Vec<PixelRect> } }
```

**级联**：按画家顺序依次解算。**累计半径是相加的，不是取 max**（本机脉冲实测确认）：

```text
第 k 个效果的累计半径  R_k = Σ reach.guaranteed(0..k)
它对 damage 的重算区   = damage ± R_k
```

实测依据（两级 box blur，一维脉冲）：

| `p1` | `p2` | 实际传播 | `p1+p2` | `max` |
|---|---|---|---|---|
| 1 | 1 | 2 | 2 ✓ | 1 ✗ |
| 2 | 3 | 5 | 5 ✓ | 3 ✗ |
| 10 | 4 | 14 | 14 ✓ | 10 ✗ |

写成"追加进 source_dirty"是含糊的；写成累加 `R_k` 才是可直接实现、可直接测试的。
用户什么都不写。

#### 这套解算的推导（不依赖任何既有实现）

`A/B/compute/reuse` 三分区是**从"效果的输出在 p 处依赖源 p±R 对某个 R（`reach.guaranteed`）成立"这一条定义推出来的**，
不是从哪份代码抄来的：

- `B = new \ old`：新暴露的像素，缓存里根本没有它们 → 必须算；
- `A = (damage ± R) ∩ old ∩ new`：源变过的地方，它们的输出在 `p ± R` 范围内 → 必须重算；
- `compute = ((A ∪ B) ± R) ∩ new`：**为什么要再外扩一次 `R`** —— `B` 里那些像素
  有一部分源落在 `old` 里，而 `old` 此刻存的是**上一帧的输出、不是源**，所以 `B ± R`
  这圈也要重算，尽管它们的源没变。这是"继承"的必然代价：`reuse` 因此比 `old ∩ new` 小一圈；
- `reuse = (old ∩ new) \ compute`：剩下的才能按偏移继承；
- `A` 必须收进 `old`：否则 damage 落在新暴露区时 `A ∩ B ≠ ∅`，两笔账重复计数；
- `grid` 相位变化 → 整块 `Fresh`：输出在 `p` 处依赖**源的绝对网格**，平移非整数格整块都变。

这套推导的每一条都应该有一个单测（`ui-engine` 的锚点测试清单见 §14），
**测试要能证明"改坏了会红"**，而不是"照着旧项目抄了一份期望值"。

#### 一个必须避免的设计错误（新仓库自己发现的）

解算算法**不能**对效果类型做 `match`。任何形如"有网格的效果才看 grid、其它恒 0"的写法，
都会让"新增一种有网格的效果"必须改引擎——这正好否定了 §8 的整个前提。
所以 `reach.guaranteed`/`aligned_translation` 来自 `PixelBackdrop` **数据字段**，引擎只读字段、不认识算法。

**CPU / GPU 分工**：引擎只做决策；后端只实现
`cook`（执行器）/ `paste`（预处理器）/ `capture`（后处理器）三个动作。
划分原则可推导：**只有"像素运算"与"像素拷贝"是后端特有的**，
"算哪块、能不能复用、按什么顺序"必须只有一份实现，否则两个后端会走出不同分支。

| 动作 | CPU | GPU |
|---|---|---|
| cook | `cook_cpu()`（卷积/块平均/折射采样…） | `cook_gpu()` fragment shader |
| paste | `copy_within` | blit / textured quad |
| capture | 复制到 `Pixels` | 渲染到纹理 |

### 8.7 三个内置程序（只是三条注册项，没有特殊待遇）

| 程序 | `reach.guaranteed` | `aligned_translation` | CPU 实现 | GPU 实现 |
|---|---|---|---|---|
| `Blur { σ }` | `⌈3σ⌉` | 0 | 可分离卷积（两趟） | 两趟 fragment |
| `Mosaic { block }` | `block` | `block` | 块平均（线性光域） | 块平均 shader |
| `LiquidGlass { .. }` | 折射位移上限 | 0 | **近似**（降级为 blur + 微高光） | 折射采样 + 模糊 + 高光 |

三个程序都实现同一个 `EffectProgram`，走同一条注册表、同一段级联解算、同一套降级链。
**液态玻璃没有任何"一等公民"待遇**——这正是这次澄清要的结果。

### 8.8 与层的关系

效果只作用于同层及以下。工具栏毛玻璃（Chrome）糊桌面 + 标注但不糊菜单；菜单毛玻璃（Popup）能糊工具栏。
想"糊住上面已有的东西"物理上不成立，引擎 `debug_assert` 检查。

**协议侧与像素侧的一体化**：`EffectPrim` 在 Wayland 上可以同时做两件事——
① 在 `DrawList` 里做像素处理（就地/离屏）；② 由 `PresentTarget` 收集矩形设置
`ext_background_effect_v1`（compositor 糊 surface 后方）。同一份 UI 代码，两种"毛玻璃"自动分别落地，
业务侧不需要知道当前后端有没有协议支持。

---

## 9. 动画

### 9.1 目标驱动

```rust
let hover = self.anim.animate(Id::str("card"), target, AnimSpec::quick());
let bg = BASE.lerp(LIFTED, hover);
```

`AnimStore { entries: HashMap<Id, Entry> }`，`dt` 由外部喂（不读墙钟 → 同一份输入永远得到同一帧，可离线单测）。

`App::anim()` 返回 `Option<&mut AnimStore>`，框架在 `view` **之前**推进它。顺序不能反：
重定向发生在 `view` 内，若先 `view` 后推进，值要到下一帧才第一次移动。

### 9.2 三条契约

1. **重定向从当前值出发**：从旧的 `to` 重新开始会让值在指针一动的瞬间跳变。
2. **反向重定向镜像缓动**：`ease-out` 中途掉头用 `ease-in`，否则像卡了一下。
   第二次掉头要**恢复原曲线**，否则每次转向都再镜像一次，`EaseOut` 会一路漂成 `EaseIn`。
3. **时钟外部喂**：同一份输入永远得到同一帧；非正/非有限 `dt` 忽略而非回退。

### 9.3 与 Damage 联动：不需要特殊处理

原设计说「动画推进的节点自动进 `damage.rects`」，**实现后发现这是多余的**：
值变化会改变图元，而 `compute_damage` 本就按节点为图元取指纹，脏区自动产生。

这条是写测试时发现的——把「按动画 id 加脏区」的代码整段删掉，测试**仍然全绿**。
保留它只会多一条永远无法失败的保护路径，正是 AGENTS.md 里「两条路径做同一件事」的错误。

同理 `AnimStore::advanced()` 与 `ids()` 随之删除：没有调用方，只有测试在测它自己。

**保留的**是 `FrameOutcome::animating`（`is_animating()`）：它回答的是另一个问题——
「平台要不要安排下一帧」，而脏区回答的是「这一帧要画什么」。两者不能互相替代。

### 9.4 退场动画（有树才做得到）

节点从声明中消失时**不立即销毁**：标记 `retired`，保留一帧让退场动画播完（§3.2 规则 4）。纯即时模式做不到——节点消失那帧它就没有矩形了。

---

## 10. 拖拽

### 10.1 状态机（已实现：`ui-core/src/drag.rs`）

```
Idle ─按下→ Pressed{on,at} ─移动≥4px→ Dragging{on,from,current}
                                        ├─ 松手 → Dropped{on,target}
                                        └─ Esc  → Cancelled{on}
```

`DragTracker` 是纯状态机，不碰平台、不读时钟，因此可以完全确定性地测。

| 契约 | 为什么 |
|---|---|
| 低于阈值不算拖拽 | 否则每次点击都顺带拖动它命中的东西 |
| `Started` 每次拖拽恰好一次 | 第二次会让 app 记录两次起始状态，丢掉原位置 |
| `delta` 从**按下点**量 | 从上次移动量起会累积漂移 |
| 进入 `Dragging` 才捕获指针 | 按下就捕获会让普通点击也抢占路由 |
| `Cancelled` 不伴随 `Dropped` | 否则 app 会应用它刚被告知放弃的移动 |
| `Escape` 取消拖拽时**不清焦点** | 两件事，混在一起用户会丢失位置 |

### 10.2 手势归属：又一次「最上层」陷阱

和悬停完全同一个坑：按下命中的是**最上层**节点，而可拖拽的通常是容器。
`App::draggable(id)` 标记哪些节点可拖，引擎从按下节点沿祖先链上找最近的一个：

```rust
fn draggable(&self, id: Id) -> bool { id == Id::str("card") }
```

不做上溯的话，手势会属于一个 app 从未标记的子节点，`drag` 回调里的
`on == card` 永远不成立——表现就是「拖不动」。

放下目标同理：`App::accepts_drop(id)` + 祖先上溯。否则「拖到放置区上方」
会命中放置区自己的子节点，拖拽丢失。

### 10.3 绘制偏移：`Decl::offset`

拖拽中不重排布局，视觉位移走 paint 偏移。`Node::offset` 是**绝对值**：

```rust
let card = Decl::container(Id::str("card"), NodeKind::Column)
    .offset(self.drag_offset, 0.0)
    ...
```

一趟布局之后再做一次 `accumulate_offsets`，把祖先偏移折进每个节点。三点必须一致，
否则「移动了但点不到」：

| 跟着 offset 走 | 不跟的后果 |
|---|---|
| 绘制 `visual_rect()` | 背景动了、子节点不动 |
| `ink_rect()` | 脏区漏掉移动后的像素 |
| `mask`（与 offset 一起平移）| 裁掉节点自己的墨迹（布局空间的 mask 外） |
| 命中 `visual_rect()` | 节点在屏上移动了，却要点击原位置，成死区 |

`mask` 在布局趟里与 offset 一起平移，因此命中处**只 intersect 不再平移**——
平移两次会让位移翻倍。`apply_decl` 复用节点时从声明重置 `offset`，
因为 `accumulate_offsets` 每帧都折祖先，保留上帧值会让偏移逐帧累积。

样例的滑动 + 拖拽现在都走偏移，不再用前置 spacer，因此不触发重排。

实测（拖到新位置 x 614..1077，光标停在 (700,900) 即新视觉位置、旧布局位置之外）：
卡片由焦点红 (220,40,40) 淡入悬停蓝 (110,175,245)；旧布局位置 (20,900) 是背景死区。
即命中确实跟随绘制位置。

### 10.4 实测（嵌套会话，2026-10-26）

脚本化指针手势走真实 `dispatch`（`CRABSNAP_AUTODRAG=1`），只跳过合成器投递这一层：

```
drag Started { from: (300,400) }
drag Moved   { delta: 36 }   ... delta 612
drag Dropped { target: None } -> offset 612
```

卡片横向位置：无拖拽 `x 7..470`，拖拽后 `x 619..1082`，差值 **612** 与 `delta` 一致。

## 10.4 指针形状：`Decl::cursor`

节点声明 `cursor(CursorShape)`，框架每帧沿**祖先链**解析：从悬停节点向外找最近的声明，
无人声明则 `Arrow`。名字取自 CSS 关键字集——`wp_cursor_shape_v1` 本身就源自它，
因此后端直接映射，不需要比喻翻译表。

```rust
Decl::container(Id::str("card"), NodeKind::Column)
    .cursor(CursorShape::Grab)
```

**为什么是 `Option` 而不是默认变体。** `None` 表示「交给祖先」，而显式的 `Arrow`
必须能表达「到此为止」。若 `Arrow` 同时兼任「未设置」，父节点的 `Grab` 会泄漏到
本应显示箭头的子节点上，且没有写法能阻断它。

**为什么形状是确定值而非差值。** `FrameOutcome::cursor` 永远给出本帧答案，不是
「有变化才带值」。若用「无变化」表示不变，节点不再声明形状时旧形状会卡在指针下——
这正是 §1.5 反复出现的静默失效类。

与 hover/draggable/drop 是同一族问题（祖先归属），因此同样沿 `hovered_path` 解析，
不新增 `App` 钩子。

### 10.4.1 平台侧：`wp_cursor_shape_v1`

三个协议陷阱，任一处漏掉都是「请求被静默忽略」：

| 陷阱 | 后果 | 我们的做法 |
|---|---|---|
| `set_shape` 的 `serial` 必须等于最近一次 `wl_pointer.enter` | 不匹配则 compositor **直接丢弃**，无错误 | 在 `Enter` 事件里记 `pointer_enter_serial` |
| 指针静止时没有后续事件捎带请求 | 等下次派发，形状慢一步 | `set_shape` 后立即 `flush` |
| 重复发送同一形状无意义 | 每帧一次请求 | 记住上次形状，相同则跳过 |

协议缺失（老 compositor）时 `cursor_device` 为 `None`，`set_cursor` 返回 `false`，
指针保持默认外观——降级是**可见的**（返回 `false`），不是假装成功。

### 10.4.2 实测

本 compositor 提供 `wp_cursor_shape_manager_v1` v2。加临时 trace 后，真实会话中记录到：

```
cursor: attempt shape=Arrow device=true serial=None  last=None
cursor: attempt shape=Grab  device=true serial=Some(192) last=None
cursor: set_shape serial=192 Grab
cursor: attempt shape=Grab  device=true serial=Some(192) last=Some(Grab)   ← 去重生效
cursor: set_shape serial=192 Arrow                                          ← 离开卡片
```

第一条证明 enter 之前 serial 缺失、请求被挡；第二条证明拿到 serial 后请求真的发出；
第四条证明静止指针不重复发送。指针移出卡片后正确回退。

**未验证**：光标位图本身由 compositor 绘制，客户端截图看不到形状，
因此「用户眼睛看到的箭头/手型确实变了」这一环只能在真实交互中主观确认。
协议层证据（上面四行）是客户端能提供的最强证据。

## 11. 与旧项目 `ui/` 的关系（重写）

旧 `ui/` 共 33,000 行。**重写，不搬运。**

### 11.1 保留（经验与纯函数）

| 保留 | 位置 |
|---|---|
| 布局纯函数（对齐/栅格/内边距） | `ui/base/layout.rs` → `ui-el::layout` |
| 动画曲线与补间数值 | `ui/engine/anim.rs` |
| 命中几何算法（把手容差、缩放命中） | `ui/panel/hit.rs` |
| **像素效果解算的纯逻辑** | 需要重新实现（按 §8.6 推导，不参考旧实现） |
| 图标绘制（纯 DrawList 命令） | `ui/chrome/icons.rs` |

### 11.2 重写（架构债）

| 旧 | 问题 | 新 |
|---|---|---|
| `ui/engine/widget/mod.rs`（2665 行） | 状态/层/动画/绘制混在一个 `Ctx` | `BuildCx`/`PrepaintCx`/`PaintCx` + `Node.state` |
| `ui/panel/overlay/mod.rs`（1798）+ `input.rs`（1679） | 事件分派与面板状态混写 | 路径分发（§5.5）+ 面板状态各自独立 |
| `ui/control/*` | 每控件自带命中+布局+绘制 | builder + 引擎默认实现 |
| `ui/base/hit.rs` | `HitTarget` 枚举硬编码所有面板 | `HitBehavior` + 回调 |
| 设置窗口的 egui（9148 行） | 两套 UI 框架并存 | 也移植到本引擎（§11.3 第 5 步） |

### 11.3 迁移顺序

1. `ui-el` + `ui-engine` + `ui-paint` + `ui-platform/window`：3 个控件（button/slider/text field），
   **普通窗口跑通**（`examples/window_hello.rs`）；
2. **离屏画布 + 采样引擎**（§6.4）：`canvas_zoom` example 跑通再上控件——
   贴图的编辑/呈现两种采样状态是这条路径的第一批用户； 
3. **工具栏**（旧 2789 行 → 预计 1200 行）：效果面与换边动画的主战场；
4. **菜单与弹层**：捕获/冒泡的第一批用户；
5. **保存对话框与调色板**：文本框 + IME + 滚动 + 列表复用；
6. **设置窗口**，完成后删掉 egui。

预计 33k → 15–18k（减掉重复的命中/布局/绘制样板与两套主题）。

> 步骤 1 之前还有一步：**UI workspace 自身先立起来**（§3.1）——
> 6 个 crate 的空壳 + `window_hello` 跑通，才轮到往它上面搬控件。
> 这一步与主项目的 Phase 划分无关，可以并行做（它不依赖业务代码）。

---

## 12. 渲染后端：**按成熟 GPU 渲染器的结构设计**

> 依据 AGENTS.md §2：渲染器参考成熟框架，**不参考旧项目的手写 GL 路径**。
> 主要参照：**GPUI 的 `gpui_wgpu` 渲染器**（已读源码）、**egui 的 `epaint` tessellator**、
> Skia 的采样/滤镜模型。

### 12.0 从 GPUI wgpu 渲染器抄到的三个结构（已核实源码）

| 结构 | 位置 | 为什么值得抄 |
|---|---|---|
| **两阶段帧**：先把顺序无关的图元分组渲染到中间纹理，再合成 | `wgpu_renderer.rs:1540-1571` | 当某一批（GPUI 里是 path）需要"先光栅到离屏、再当纹理画回来"时，它在 **pass 中间插入一个离屏 pass**，然后 `Load` 继续主 pass。这正是我们要的"效果面 / 离屏画布"机制 |
| **实例缓冲 + 按批次 draw** | `wgpu_renderer.rs:1117-1620`（`write_instances` + `batches()` 循环） | 每个图元类型一个 `Vec<Instance>`，`#[repr(C)]` 直接上传；一次 `draw_indexed(0..4, instances)` 画完一批。**"一条命令一次 draw call"因此不成立，也不必要** |
| **管线按图元类型预建** | `wgpu_renderer.rs:664-930`（quads/shadows/paths/underlines/sprites…） | 管线在建时一次创建，帧内只切 bind group；圆角/描边/阴影全在 fragment shader 里用 SDF，不需要 CPU 细分 |

### 12.1 我们的帧结构

```text
帧 = 一串 pass，pass 之间靠离屏纹理传递
  ┌ pass 0：底图/内容（可选）
  │ pass 1..n：按绘制顺序的图元批次
  │   ├─ 遇到 PushLayer        → 起一个离屏 pass（目标 = 层纹理），画完再当一个 quad 合成回来
  │   ├─ 遇到 Op::Effect        → 起"采样-处理-写回"三个动作（§8 / §12.3）
  │   └─ 遇到 Prim::Canvas 图元  → 直接以采样纹理为源画一个 quad
  └ 最终 pass：写呈现目标
```

**关键**：pass 是按**命令流的需要**动态插入的，不是固定的几个。GPUI 已经证明了这种结构的可行性
（它按 path 批次插 pass）；我们只是把"插入条件"从"图元类型"换成"命令语义"。

### 12.2 图元 → 批次 → 管线

| `Cmd` | 批次 | 管线 | 说明 |
|---|---|---|---|
| `Rect`（纯色/圆角/描边） | `quads` | SDF 圆角盒 | 与 GPUI `fs_quad` 同构：圆角、边框宽度都在 shader 里算 |
| `Rect`（带阴影） | `shadows` | SDF + 高斯积分 | GPUI 用解析 erf（`shaders.wgsl:325, 336`），**不是卷积**——只够画阴影，不够做 backdrop 模糊 |
| `Path` | `paths` | stencil + cover 两趟，或 MSAA | GPUI 走 `draw_paths_to_intermediate`（`1540`）；egui 走 CPU tessellation + feathering（`tessellator.rs:796-810`）。**我们选 CPU tessellation + feathering**：实现简单、分辨率无关、与字体蒙版同一套（§12.4） |
| `Text` | `glyphs` | 图集纹理 quad | 同 GPUI `monochrome_sprites` |
| `CanvasRef` | `images` | 纹理 quad + 采样器 | 采样器由 `Sampling` 决定（§6.4） |
| `PixelBackdrop` | 独立 pass | 见 §12.3 | 不是"一个批次"，是 pass 序列（由 §6.2 的 `BatchPlan` 切出） |
| `PushLayer` | 离屏 pass | 见 §12.0 | 层纹理复用池 |

### 12.3 像素后端效果的 GPU 执行

一条 `PixelBackdrop` 在 GPU 上的执行是**三个 pass 动作**，与 CPU 侧三个动作一一对应（§8.6）：

```text
① capture：把效果矩形（含 `reach.guaranteed` 外扩）从当前目标拷到"源纹理"
② cook   ：源纹理 → cook_gpu() 的 shader → 目标纹理
③ paste  ：只把 compute 区写回当前目标（reuse 区已由上一帧的结果贴回）
```

**这直接对应 GPUI 的中间纹理模式**：`path_intermediate_texture`（`wgpu_renderer.rs:1377-1395`）
就是"离屏渲染 → 当纹理用"的现成机制，我们把它扩展成"可复用的效果源/目标纹理池"。

CPU 侧同一套动作：`capture` = 从画布拷块、`cook` = `cook_cpu()`、`paste` = `copy_within`。
**决策只有一份（引擎），执行各一套（后端）**——这条划分是 §8.6 的结论。

### 12.3.1 CPU 光栅核心：改用 `zeno`（不沿用 tiny-skia）

> 依据：[`research/cpu-rasterizer-survey.md`](research/cpu-rasterizer-survey.md)（含本机实测）。

旧项目用 tiny-skia；按 AGENTS.md §1 这不算依据。实测与调研后的结论：

| 场景（本机实测） | zeno 0.3 | tiny-skia 0.11 |
|---|---|---|
| 批量 900 个矩形 → 1920×1080 | 2.12 µs/个 | 2.42 µs/个 |
| **局部更新 64×64 tile**（UI 常态） | **0.62 µs/次** | 1.14 µs/次 |

架构以 `Damage` 区域更新为主导（§7），**局部更新是主战场**，因此选 zeno。

分工（与 §12.5 的 trait 对应）：

```text
OurPath → zeno::Mask（alpha 覆盖率）→ 我们的 blend_into(&mut Pixels) → 屏幕
```

**合成、像素格式、效果、采样都留在 `ui-paint`**——zeno 只负责"路径 → 覆盖率"这一件事。
这样两条后端路径的混合语义只有一份实现（`dual-backend.md` §3）。

额外收益：zeno 还提供**像素级命中测试**与**路径遍历**，前者可作为 §5.3 几何命中的精确补充
（贝塞尔/箭头这类非矩形元素），后者用于沿路径的标注动画。

### 12.4 抗锯齿：CPU 与 GPU **机制不同**，不要求像素一致

| | CPU（zeno） | GPU（SDF / 羽化） |
|---|---|---|
| 机制 | **256× 超采样**，输出 alpha 覆盖率 | 距离场或几何羽化 |
| 证据 | zeno 文档："256x anti-aliased rasterization" | egui `tessellator.rs:796-810`、GPUI `fs_quad` |

**不要试图让两者输出相同 alpha**：超采样与距离场近似在边缘像素上天然差 1–3/255。
用可测的不变量替代逐像素相等（见 `dual-backend.md` §2.2）：
面积守恒（<0.5%）、边缘单调、无溢出、50% 等值线位置误差 <0.5px。

### 12.4.1 另一条路：让 GPU 吃同一份三角

CPU 三角化 + 羽化，GPU 上传统一顶点（egui 的做法）。好处是两条路完全一致；
代价是 GPU 失去实例化与 SDF（`draw(0..4, instances)` 不再可能），4K 全屏场景性能下降。
**默认不选**，除非"两条路完全一致"的优先级高于 GPU 性能。

egui 的做法（`tessellator.rs:667-674, 796-810`）：在几何边缘生成**半像素宽的透明羽化边**
（沿法线复制顶点），而不是开 MSAA。优点：

- 分辨率无关（在 1×/2×/3× 上都对，MSAA 的采样数固定）；
- 一个 shader 搞定，不需要多采样渲染目标与 resolve；
- 合成到未知背景上时结果正确（MSAA 的 resolve 需要知道背景）。

代价是 CPU 侧要把形状三角化（`libtess2`/自写），但**我们已经要为文字光栅做蒙版**，
同一套即可。**结论**：GPU 后端也走"CPU tessellate + 上传三角"，而不是把路径细分丢给 GPU。

### 12.5 与呈现目标的接口（HDR 感知）

> 完整设计见 [`hdr-color-management.md`](hdr-color-management.md)。要点：
> **格式与色彩描述是 buffer 身份的一部分**，且呈现层只暴露纯数据能力集，不暴露平台类型。

```rust
pub trait PresentTarget {
    fn size(&self) -> (u32, u32);
    fn scale(&self) -> f64;
    /// **实际**格式（请求 f16 被降级成 8-bit 时会不同）——编码前必须读它。
    fn format(&self) -> PixelFormat;
    fn colour(&self) -> ColorDescription;
    fn present(&mut self, damage: Damage) -> Result<()>;
    fn set_ime_cursor(&mut self, rect: Option<Rect>);
    /// 协议侧背景模糊（可选能力）：由引擎收集 `EffectPrim` 的矩形后调用。
    fn set_backdrop_regions(&mut self, rects: &[Rect]) {}
}

/// 平台能力只暴露"支持哪些 (传输, 格式, 峰值亮度)"，没有平台类型。
/// Platform abilities as pure data: no platform types leak upward.
pub struct PresentPath {
    pub transport: Transport,     // SharedMemory | DmaBuf | Native
    pub format: PixelFormat,
    pub peak_nits: Option<f32>,
}
```

`ui-engine` 不依赖这两个 trait，它们住在 `ui-platform` / `ui-backend-gl`，由 app 组装。
分工可推导：引擎的产出必须是**纯数据**（`Primitives` + `Damage`），否则"能不能渲染"
会变成编译期问题而不是运行期探测。

| 目标 | 特殊之处 |
|---|---|
| Wayland layer surface | `input_region`；`ext_background_effect_v1` 的协议侧模糊由 `PresentTarget` 从 `EffectPrim` 收集矩形设置（不耗像素）；HDR 走 `ABGR16161616F` + `color-management-v1` |
| 普通 XDG 窗口 | 同上，无 layer shell、整窗可点 |
| Windows HWND | DPI 变化要整树重排；HDR 走 flip model + `R10G10B10A2`/`R16G16B16A16_FLOAT` `[未证实]` |
| macOS CAMetalLayer | EDR（`wantsExtendedDynamicRangeContent`）`[未证实]` |

### 12.6 合成层（`PushLayer` / `PopLayer`）

```rust
Op::Layer { id: LayerId }   // LayerId → { bounds, opacity, blend, radius }，存环境表
Op::PopLayer,
```

`PushLayer…PopLayer` 之间的命令**先渲染到离屏目标再整体混合**。为什么必须要它：

- **分组不透明度**（可推导）：逐形状乘 alpha 时，两个半透明子元素**重叠处**会叠两次，
  整体比"先合成再整体乘一次"更暗。对话框淡入、菜单阴影、气泡都受此影响；
- **圆角裁剪**：层内内容按圆角裁剪（菜单内容不溢出）；
- **效果载体**：`EffectPrim` 的源可以是一个层的离屏结果。

CPU 后端：渲到临时 `Pixels` 再 `blend`；GPU 后端：渲到层纹理再整体画一个 quad。
**两边数值必须等价**：一条确定性测试（两个半透明白块叠在灰底上）钉住，输入与期望值都是常量。

### 12.7 输出编码：唯一落点

`Surface`（存储域见 §5.3.4 of hdr 文档）→ 目标格式的字节，**全项目只此一处**：

```rust
// Single conversion from linear light to a target format. No-op when domains match.
pub fn encode_for_output(src: &Surface, target: &OutputTarget, dst: &mut [u8]);
```

**同域时退化为无操作**——SDR 渲染在 `Srgb8` 上、HDR 渲染在 `LinearF16` 上，都不触发转换。

`OutputTarget = { format, primaries, tf, reference_nits, peak_nits }`，四个维度都来自
协议/显示描述而非硬编码。色调映射**只在内容峰值 > 目标峰值时**启用。

---

## 13. 性能与可观测性

| 项 | 做法 |
|---|---|
| 帧预算 | 记录 `build / layout / prepaint / paint / effects / damage` 六段耗时 |
| 增量布局 | 只跑 dirty 子树；记录"本帧 measure 了多少节点"（回归时能看出是否退化成全量） |
| 效果 | 每帧记录 `[{id, plan, computed_px, why}]`。`why` 必须能区分 `Fresh` 的四种来源（无缓存/键不同/相位变/源全覆盖），否则"为什么没用缓存"只能靠猜 |
| 零帧 | `Damage::None` 且无动画 → 不提交、不渲染 |
| 命中 | 16×16 分桶 |
| 批处理 | `DrawList` 按 `(Clip, Texture)` 分组交 GPU |

---

## 14. 测试策略（遵循 AGENTS.md §7）

### 14.1 分层原则

| 层 | 位置 | 覆盖什么 |
|---|---|---|
| 单元 | 各 crate 内 `#[cfg(test)] mod tests` | 一个函数 / 一个类型的契约 |
| 属性 / 差分 | 同 crate | "增量 == 全量"这类不变量（固定种子） |
| 跨 crate 契约 | 罕有；只在契约本身是主题时 | 两 crate 之间的接口不变量 |
| 视觉 / 人工 | `examples/*` | 需要 GPU、compositor、人眼判断的 |

**引擎的单测不连 Wayland、不开 GPU**——`ui-engine` 的依赖里没有 `wayland-*`/`glow`/`khronos-egl`，编译期保证。

### 14.2 每个子系统必须有的一条测试（其余按需）

下表是**候选清单**，不是待办列表。每条进仓库前要过 AGENTS.md §7.6：

| 子系统 | 该钉住的不变量（一条测试一条） |
|---|---|
| 布局 | 给定约束 → 矩形精确匹配 |
| 布局增量 | 改一个叶子 → 只有祖先链被重新 measure |
| 树差分 | 同 `Id` 二次声明 → 节点复用且状态保留 |
| 树回收 | 消失节点进 retired，且 `debug` 断言节点数守恒 |
| 命中 | `topmost()` 在层序 / mask / PassThrough 三种情形下各自正确（**三个测试，不是三个断言**） |
| 事件 | 捕获→冒泡顺序；`stop_propagation` 跨阶段生效 |
| 焦点 | Tab 前序；被回收节点的焦点回落到最近存活祖先 |
| 脏区 | 矩形变化产出 `old ∪ new`（残影用例） |
| 动画 | 固定 `dt` 序列 → 值；落定帧 `advanced == true` |
| 采样 | 2×2 棋盘 2× 缩放：`Nearest` 硬边 / `Linear` 中间值 |
| 编辑网格 | `to_native_pixel` 在任意倍率下落在整数网格 |
| 效果解算 | 新暴露区 / 源变区 / 外扩二次 / A∩B 不重复 / 相位 / 平移条带 / 级联 —— **七条各一个测试**，外加每个效果「脉冲衰减半径 ≤ guaranteed」 |
| 色彩编码 | 同一 `LinearImage` 编码到 5 种格式：可逆者往返一致；超白值在 `Sdr8*` 上被 tonemap 而非硬裁 |

**效果那七条为什么不能合并**：合并后一条红，要读实现才能判断是"外扩错了"还是"相位错了"，
违反"失败只有一个原因"（AGENTS.md §7.4）。

### 14.3 明确不做的测试

- 每个公开方法都测一遍（断言实现形状）；
- 快照测试（没人读的快照等于没测）；
- 需要真机才能跑的"回归测试"——那些放 `examples/` 当诊断工具；
- 同一个不变量在两层的重复测试。

## 15. 风险与取舍

| 风险 | 影响 | 缓解 |
|---|---|---|
| 保留树 + 声明差分 = 两套状态可能不一致 | "树里有的、声明里没有"的僵尸节点 | 帧末强制回收 + `debug` 断言"树上节点数 == 本帧声明数 + retired" |
| 增量布局的脏标记漏标 | 布局不更新（控件位置错） | 所有 props 修改走统一 setter 自动标 dirty；差分测试：全量布局 vs 增量布局结果逐节点相等 |
| `ui-engine` 可能到 10k 行 | 新的"大文件" | 按 §5–§10 十个主题分模块，每模块 < 1200 行；`xtask arch` 限制内部模块依赖 |
| 效果级联边界 bug | 撕裂/残留 | 每个不变量一条原子测试（§7.3/§14）+ 一条固定种子的差分测试（增量 vs 全量逐像素） |
| 液态玻璃 CPU 无原生实现 | 观感降级 | `fallback_for` 显式降级 + 调试面板可见 |
| 与 egui 设置窗口并存期 | 两套主题/快捷键 | 设置窗口移植排最后，期间两进程互不影响 |

---

## 16. 评审需要拍板的事

### 16.1 已确认（v2/v3）

- ✅ UI 框架**独立成项目**（独立 workspace + examples 原型验证），先在普通窗口与 Wayland layer surface 两种呈现上验证（§3.1）。
- ✅ 布局**不跑两遍**：`SizeClass` 四元组 + 单趟确定性 measure；`LayoutEngine` 可插拔，taffy 为可选后端（§5.2）。
- ✅ 允许控件树（v2）；取两家之长而非照搬任一方（§0.1）。

### 16.2 仍然开放

1. **`RetainedTree` 回收策略**：本文档选"消失后保留一帧"（让退场动画播完）。不要退场动画可改立即回收，省一帧树遍历。
2. **`Node.state` 是否允许用户自定义数据**：本文档选"只放引擎定义的 `NodeState`"，用户数据走 `cx.state::<T>(id)` 的类型化侧表。若允许任意 `Any` 更灵活，但类型安全靠约定。
3. **taffy 后端什么时候真正接**：本方案先只做 `BuiltinLayout`（覆盖现有全部界面），
   `TaffyLayout` 留成 feature 口子。触发条件：设置页真的需要 flex/grid 换行时，
   或"两套引擎跑同一组测试"暴露出自研实现的语义缺口时。
4. **离屏画布的"编辑网格"边界**：本方案约定 `to_native_pixel` 恒落整数网格。
   但"标注工具的笔宽是屏幕像素还是内容像素"是产品决定——两种都要支持（框选工具用屏幕空间，
   画笔用内容空间）。待与标注子系统对接时定。
5. ~~液态玻璃是否一等效果~~ —— **已被 v3 澄清取代**：不做「某个效果是一等公民」，
   而是做「**像素后端效果是一类**」。任何满足 §8.1 四条判定标准的算法都能注册进来（§8.2），
   引擎只看 `PixelBackdrop` 元数据，不认识具体算法。

---

## 附录 A：与两份参考实现的逐条对照

| 能力 | egui 0.36 | GPUI 0.2.2 | 本项目 v3 |
|---|---|---|---|
| 语法 | 即时方法调用 | 链式 builder + 实体 | **链式 builder（无实体）** |
| 控件树 | 无 | 有（`ElementId`） | **有（`RetainedTree`，按 Id 差分）** |
| 用户句柄 | `Id` | `ElementId`/`Entity` | **只给 `Id`** |
| 布局 | 估算修正（猜错再重跑） | taffy（CSS flex，多轮测量） | **`SizeClass` 四元组 + 单趟 measure**（Qt 式）；可换 taffy |
| 命中 | 后写赢 | hitbox + mask | **HitIndex（层 + mask + 分桶）** |
| 事件 | 无冒泡 | capture + bubble | **capture + bubble** |
| 焦点 | `Id` 单值 | `FocusHandle` + 路径 | **路径 + Tab + 回收回落** |
| IME | 全局单值 | `set_ime_position` | **焦点绑定 + `done` 批次 + 引擎画 preedit** |
| 脏区 | 无（整窗） | 无（整帧） | **`Damage` 矩形集合** |
| 动画 | `animate_bool` | `Animation` trait | **`AnimStore`（目标驱动 + 落定帧脏区 + 退场）** |
| 拖拽 | `Response::dragged` | `DragMove` + 全局 drag | **状态机 + 捕获 + payload** |
| 像素后端效果 | 无 | 仅阴影 blur | **`EffectProgram` 可扩展类别 + 级联 + 降级链** |
| 后端 | 外部 integration | `Platform`/`Window` trait | **`RenderUi` + `PresentTarget`** |
| 合成层 | 无（逐形状 alpha） | 有 `StackingContext`（`backdrop_filter` 无实现） | **`PushLayer/PopLayer`（分组不透明度 + 圆角裁剪）** |
| 命中用的几何 | **上一帧** | 本帧（draw 之后） | **本帧（layout 之后）** |
| 中间表示 | `Shape` → 三角化 → `ClippedPrimitive` | 类型化 `Vec<T>` + `Vec<PaintOperation>` | **类型化 `Primitives` + `Op` 日志 + `BatchPlan`（§6.2）** |
| 结构性操作（clip/layer/effect） | 是 `Shape` 的变体或 Area 排序 | `defer_draw` + `DrawOrder` | **独立于图元（`Op` 变体），不进后端 match（§6.2）** |
| 离屏画布 | 无概念（外部 integration） | `paint_surface` 仅 macOS/iOS | **`Canvas` + `SamplingEngine` + `CanvasMapping`（§6.4）** |
| GPU 渲染器结构 | tessellator + 图集，无离屏 pass | **按需插入离屏 pass**（`wgpu_renderer.rs:1540`）+ 实例缓冲 + 预建管线 | **同 GPUI 的帧结构（§12.0-12.3）**，抗锯齿取 egui 的 feathering |
| 每帧遍历趟数 | 1（但布局要猜 + 可能重跑） | **3**（request_layout/prepaint/paint） | **2**（融合趟 + paint） |
| 事件派发 | 每次 `interact` 覆盖前一次 | **全窗口监听器广播**，过滤在闭包内 | **先算唯一命中路径，只派发给路径上的节点** |
| 元素状态键 | `(Id, TypeId)` | `(GlobalElementId, TypeId)` | **`(Id, TypeId)`**（用户给确定性 Id） |

## 附录 B：一帧的伪代码

```rust
pub fn run_frame(tree: &mut RetainedTree, root: impl Element, input: FrameInput) -> FrameOutput {
    tree.begin_frame(input);                    // ① 时钟推进 + IME 批次

    let decl = root.build(&mut BuildCx::new()); // ② 用户声明
    tree.apply(decl);                           //    差分：复用/更新/新建/retired

    tree.layout_and_prepaint();                 // ③④ 融合趟：measure → rect → mask → hitbox
    tree.dispatch(&input.events);               //    本帧矩形就绪后才派发（§1.6.1）

    let mut list = DrawList::new();
    tree.paint(&mut list);                      // ⑤ 前序 + 层序

    let effects = tree.effects_pass(&list);     // ⑥ 级联解算
    tree.end_frame(&list, effects)              // ⑦ Damage
}
```

用户只写 `root`——其余六步由引擎编排。

## 键盘焦点遍历（Tab / Shift+Tab）

### 修饰键是环境状态，不是按键属性

`Event::Modifiers(Modifiers)` 单独上报按住的修饰键，而不是给每个 `KeyPressed` 挂字段。
理由：平台分两条通道上报（`wl_keyboard.key` 与 `wl_keyboard.modifiers`），
按键事件里没有修饰键信息；挂字段会迫使实现方在按键到达时猜测。

### 焦点环

`AppLoop::focus_step(forward)` 在**文档顺序**上移动焦点，两端环绕：

| 情形 | 行为 |
|---|---|
| 未聚焦时 Tab | 第一个可聚焦节点 |
| 未聚焦时 Shift+Tab | 最后一个 |
| 已聚焦时 Tab | 下一个（末尾回到第一个） |
| 已聚焦时 Shift+Tab | 上一个（开头回到最后一个） |
| 没有可聚焦节点 | 不做任何事，返回 false |

### 可聚焦的判据

与命中测试**不同**，这是有意的：

```rust
hit_behavior(kind) != Ignore
  && (有背景盒 || 是 Text 节点)
  && 可见区域非空
```

纯布局容器（Row/Column）是命中目标但没有可聚焦内容，因此被排除。
若直接复用命中判据，Tab 会停在根容器上——有一条测试专门钉住这一点。

### 验证

两条测试，且都做过「故意破坏 → 变红 → 恢复 → 变绿」：

- `tab_walks_the_focus_ring_and_wraps_at_both_ends`
- `tab_skips_a_layout_container_that_paints_nothing`

### 层的圆角语义（决定记录）

`LayerEntry.radius` 现在两个后端都实现了，但 UI 层**不设置它**（仍传 `Corners::ZERO`），
原因是一个语义选择：

| 语义 | 效果 |
|---|---|
| 层半径裁掉**合成结果**（含效果） | 模糊光晕会被切掉，圆角卡片不会外溢 |
| 层半径只作用于内容 | 光晕按自然扩散 |

Flutter 的 `BackdropFilter` 默认是后者（不裁），只有套上 `ClipRRect` 才裁。
我们选择同样的默认：**半径 0 = 不裁**，需要裁时由使用方显式设置。
所以这不是缺口，而是一个已经确定的默认值。

### 键盘激活

`App::activate(&mut self, id: Id) -> bool`：焦点节点被 Enter 或空格激活时调用。

为什么是独立的门而不是复用 `event`：点击本来就带着坐标进 `event`，而激活携带的是
**身份**而不是坐标。把两者挤进同一个签名，要么让 `event` 的坐标变成 `Option`，
要么让调用方从坐标反推是哪个节点——两者都在猜。

| 键 | 行为 |
|---|---|
| Enter | 激活焦点节点 |
| Space | 同上（多数工具包的惯例） |
| Tab / Shift+Tab | 焦点环前进/后退 |
| Escape | 释放焦点 |

`Key::Space` 与 evdev `57` 一并加入；无焦点时 Enter 什么都不做（不凭空产生焦点），
有测试钉住这一点。

### 端到端

样例程序里焦点在药丸上按 Enter，计数标签从 `idle unless input` 变成 `activated N`——
这条路径同时验证了键盘事件、焦点路由与文本重排。

### 方向键：空间导航

#### 依据几何，不依据树序

布局引擎把**绝对矩形**写进 `Node.rect`（命中测试直接拿它与绝对指针坐标比较，
绘制也直接用它），因此空间导航只需要矩形，不需要树。

树序只回答顺序问题（Tab）。用树做空间导航会在「视觉顺序 ≠ 结构顺序」时给出错答案
——那正是 `Stack` / `Overlay` / 绝对定位存在的场景。

#### 与成熟框架一致

| 框架 | 依据 |
|---|---|
| **Flutter** | `DirectionalFocusTraversalPolicyMixin.inDirection` 用布局后的 `Rect` |
| **GTK 4** | 默认实现按 `GtkAllocation` 矩形挑该方向的邻居 |
| **Web** | WICG Spatial Navigation 规范，基于 bounding box |
| Qt Quick | `KeyNavigation` 需作者手写 id（不自动） |
| egui / GPUI | 只有顺序 Next/Previous |

#### 打分规则

`spatial_score(from, to, dir) -> Option<(band, gap, misalign)>`，字典序比较：

1. **同带优先**（`band` 为 0/1）：候选与当前矩形在垂直轴上是否有重叠
2. **间隙最小**（`gap`）：该方向上的距离，相交时按 0 计
3. **偏轴最小**（`misalign`）：中心点在垂直轴上的偏差

第 1 条是 GTK 与 Flutter 共同的规则：**同排的邻居胜过更近但在别的带里的候选**。
有单测钉住，并验证过去掉该条会变红。

#### 两个边界情形

| 情形 | 行为 | 理由 |
|---|---|---|
| 该方向没有候选 | **不移动**，返回 false | 环绕会跳到屏幕另一端，与「答案错了」无法区分 |
| 尚无焦点 | 落到第一个可聚焦节点 | 与 Tab 一致，可预期 |
| 矩形相交（gap < 0） | 改用中心点比较 | 否则叠放布局完全无法移动 |

#### 测试分层

- `spatial_score_*`：纯函数，直接钉住三条规则（含拒绝「不在该方向」的候选）
- `arrows_move_within_the_visual_grid`：真实布局上的方向语义

第二条原本命名为「按几何而非树序移动」，但流式布局下树序与几何一致，**这个测试无法区分两者**
——名字夸大了它证明的东西。已改名，把「几何 vs 树序」交给纯函数测试。

## 滚动

### 偏移是视图值，不是状态

`ContainerParams::scroll_y` 是**每帧传入的值**，不是可变状态：偏移属于 app，
框架按它平移子节点。这与 `Decl` 的其余部分一致——树是声明，状态在 app 里。

### 裁剪不需要额外工作

这是最值得记的一点：`mask` 本来就沿树逐层求交（`child.mask = parent.mask ∩ child.rect`）。
容器自身的遮罩是它的盒，因此滚出盒子的子节点**自动**被裁掉：

```
a.rect.origin.y = -20   （滚出上方）
b.rect.origin.y = 10
b.mask.max_y()  = 40    （容器下边缘，不是子节点自己的底边）
```

所以滚动**不需要新增裁剪路径**——已有的不变量直接承担了它。

### 但只有平移是不够的：还必须允许溢出

这是本功能最初的错误。引擎的分配规则是「剩余给可增长项，**亏空则压向各自下限**」，
所以内容装不下时会被**压缩塞进**盒子，永远不溢出——于是没有可滚动的东西。
实测症状：六行 22px 的列表被压进 66px 视口，滚动毫无效果。

因此容器有两个独立的概念：

| 字段 | 含义 |
|---|---|
| `scrollable: bool` | 内容**允许**溢出盒子（否则被压缩） |
| `scroll_y: f32` | 当前偏移多少 |

布局期可滚动容器的可用空间取「子节点测得总量 + 间隔」：既不压缩，也不留可增长空间，
子节点保持自然尺寸并溢出到遮罩外。平移仍是一行 `content_origin.y -= scroll_y`。

### 事件路由

| 环节 | 行为 |
|---|---|
| Wayland | `wl_pointer.Axis` 垂直轴 → `Event::Scroll { pos, delta }`（正值向下，取负） |
| 平台 | 从指针下的节点沿 `parent` 链**上溯到最近的可滚动祖先**，调用 `App::scroll(id, delta)` |
| App | 改自己的偏移并返回 true；框架不夹取范围 |

`App::scroll(id, delta)` 与 `App::activate(id)` 同一模式：携带身份，不携带坐标。

**为什么必须上溯**：滚轮落在最上面的东西上——一行、一个标签——而不会落在真正该移动的
视口上。按原始命中目标路由，app 收到的是 `row`，判断 `id != "scroller"` 后什么都不做。
实测症状：列表铺满视口，指针永远不可能落在"空白处"，于是**完全滚不动**。

### 回调的返回值就是重绘请求

`scroll`/`activate` 的返回值必须并入 `dispatch` 的返回值，否则 app 说"要重绘"没人听，
画面只能等下一个无关事件顺带刷新。实测症状：激活计数与滚动都要等下一次刷新才更新。

### 已知缺口

框架**不提供**内容高度，因此 app 无法自己夹取滚动范围。两条路：

1. app 自己知道内容高度（它的状态里就有）
2. 将来让容器报告 `scroll_extent`（子树测得高度），由 app 用它与自身高度夹取

当前选 1：框架不猜 app 的内容。

### 测试

- `scrolling_shifts_children_and_clips_them_without_extra_work`（布局层）：
  子节点按偏移上移，且 `mask` 在容器边缘截断
- `the_wheel_reports_the_node_under_the_pointer`（路由层）：
  滚轮到达指针下的节点，指针移开后到达**另一个**节点
- `the_wheel_walks_up_to_the_nearest_scrollable_ancestor`（路由层）：
  指针打在行**内部**时上报的是容器。第一版打在 (30,30)，而行只占 y 0..22——
  那里本来就命中容器，新旧代码返回相同结果，**测试没有区分力**

## 重复 `Id`：debug 构建下直接失败

树的节点按 `Id` 索引。同一父节点下重复的 `Id` 曾经**静默**塌成一个节点，
只有最后写入的内容存活。

实测代价：列表六行都用了 `Id::str("row")`，画面只剩一行 `scroll row 5`，
看起来像"布局只放了一行"。真正的定位线索是打印子节点坐标时**只输出一行**。

现在 `apply_decl` 在 debug 构建下断言同层无重复 `Id`：

```rust
#[cfg(debug_assertions)]
{ /* 同层 HashSet，重复即 panic，报出「用派生 id」 */ }
```

**为什么选断言而不是报错返回值**：重复 `Id` 没有任何合法用途——树只能容纳一个节点。
把它变成"少画几个节点"是最难反推的失效方式，而 panic 直接指出位置。

重复项的正确写法是派生 id：`Id::str("row").with(i)` 或 `Id::of(("row", i))`。

测试 `duplicate_sibling_ids_are_rejected_in_debug`（`#[should_panic]`），
并验证过：把 `cfg(debug_assertions)` 关掉后它变红。

## 段落排版：首行缩进与行距

`TextBlock` 增加两个字段，都属于**段落**而非字面：

| 字段 | 含义 | 与字体度量的关系 |
|---|---|---|
| `line_gap` | 行间额外空隙 | 与 `hhea.line_gap` **分开**：那个属于字面，这个属于段落 |
| `indent` | 首行缩进 | 只收窄首行，后续行从块左边缘开始 |

分开的理由：想调整疏密的调用方不该被迫伪造字体度量。

### 两个必须同时成立的后果

**`line_gap`**：n 行加 **n-1** 个空隙（最后一行之后不加，否则盒子多出一个空隙高，
后面每个兄弟节点都被推下去）。

**`indent`**：不只是偏移，还要**参与换行**——

- 块宽增加 `indent`
- 首行可用宽度减少 `indent`，因此可能更早断行

若只做偏移不参与换行，首行会溢出到缩进区，看起来像段落意外右对齐。

### 实现时踩到的两个坑

1. **`first == 0` 是段索引，不是行号**。文本没有断行机会时（一长串无空格的词）
   它恒为 0，缩进于是被施加到每一行。改为显式计数行。
2. **按字符切分的长词会把首行限制套到它切出的每一行**。`split_by_char` 需要
   「首片更窄、其后正常」两个限制，而不是一个。

### 测量与绘制必须同一套规则

最典型的漂移是「测量加了空隙、绘制忘了加」，结果最后一行被画到盒子外。

测试 `painted_lines_follow_the_same_spacing_the_measurer_used` 的做法值得记：
**不能只跟自身比**——那样绘制偏低也能通过。要比较**松紧两种行距的墨迹底部差**，
缺了空隙时差值为 0，立刻变红。

### 测试

| 测试 | 层 | 钉住的行为 |
|---|---|---|
| `an_indent_narrows_only_the_first_line` | `ui-text` | 缩进参与换行（无缩进 1 行，有缩进 2 行） |
| `an_indent_shortens_the_first_line_only` | `ui-text` | 断点前移、其后行变长 |
| `an_indent_wider_than_the_line_still_makes_progress` | `ui-text` | 极端缩进不丢字、不死循环 |
| `painted_lines_follow_the_same_spacing_the_measurer_used` | `ui-platform` | 绘制与测量同一套间距 |

前三条在 `ui-text` 层用纯函数测（每个字符宽 10 的假测量），不需要真实字体，
因此**确定性**；第四条需要字体，无字体时显式 `skipped`。

**注意**：`an_indent_narrows_only_the_first_line` 第一版用无空格文本，
走的是按字符切分路径，测不到按段换行——换成 `"aaa bbb ccc"` 才有区分力。

## 悬停：最上层 vs 子树内

`FrameInfo::hovered` 是**最上层**节点——点击语义要求如此，命中的是真正接住事件的那个。

但容器问「我是否被悬停」时不能用它：卡片里铺满不透明子节点，`hovered` 永远是子节点，
容器自己一次也不会被报告。表现为「样式写了、代码也接线了，但看不到效果」。

因此另给一条**祖先路径**：

```rust
let card_hovered = info.hovered_within(Id::str("card"));
```

`hovered_path` 从根到悬停节点，`hovered_within(id)` 即「该 id 在这条链上」。
两条信息各答各的问题，不能合并：

| 问题 | 用哪个 | 为什么不能用另一个 |
|---|---|---|
| 点击该给谁 | `hovered`（最上层） | 祖先链会让父容器抢走子节点的点击 |
| 我的样式该不该亮 | `hovered_within`（含子树） | 最上层在子节点铺满时永不命中容器 |

路径在 `build` 与 `dispatch` 两处刷新——只在一处刷新会让「指针移动」与「布局重算」
两个时机看到不同的路径。

**样例程序的两条循环**：`window_gpu` 有 CPU 与 GPU 两条事件循环，改动画接线时
两条都要喂 `dt`。只改一条会让另一条上的动画永远停在第 0 帧。

## 手工视觉测试：嵌套 Hyprland 会话

真实会话里有其他窗口与装饰插件，截图无法判断「哪块像素是我的」。改为在**嵌套会话**里测：

```bash
scripts/nested-session.sh start          # 启动，把 instance 指纹写进文件
scripts/nested-session.sh env            # 打印 export 行
scripts/nested-session.sh shot out.png   # 只截嵌套会话
scripts/nested-session.sh cursor 800 400 # 在会话内移动指针
scripts/nested-session.sh stop
```

状态目录默认 `/tmp/crabsnap-nested`，里面 `instance.txt` 是 `hyprctl` 能用的 instance 指纹，
`wayland_display.txt` 是它的 socket 名。`CRABSNAP_NESTED_DIR` 可改位置。

**要点**：

| 坑 | 做法 |
|---|---|
| 新实例与旧实例混淆 | 启动前快照 `$XDG_RUNTIME_DIR/hypr/` 与 `wayland-*`，取差集 |
| 截图截到真实会话 | 客户端与 `grim` 都必须带 `WAYLAND_DISPLAY=<嵌套>` |
| demo 25 秒就退出 | `CRABSNAP_RUN_SECS` 可延长 |

**这个 Hyprland 的 IPC 是 Lua 语法**，`hyprctl dispatch movecursor 800 400` 会报
`')' expected near '800'`。正确写法：

```bash
hyprctl -i "$SIG" dispatch 'hl.dsp.cursor.move({x=800, y=400})'
```

### 实测记录（2026-10-26）

卡片悬停淡入，指针在**列表行**上（子节点，不是卡片本身）：

| 时刻 | 卡片颜色 |
|---|---|
| 离开 | `(60,130,220)` |
| 进入中 | `(95,162,238)` |
| 进入中 | `(109,174,244)` |
| 落定 | `(110,175,245)` |

淡出同理反向。四帧递增说明是**渐变**而非瞬变，也说明
`hovered_within` 确实让子节点上的悬停点亮了容器。

## 背景模糊：三趟箱式近似

**症状**：模糊「看起来没生效」，画面反而整体变暗。

**根因**：`Kernel::gaussian` 按请求半径生成任意多 taps，但 GLSL 侧只有
`uniform float u_weights[33]`，超出部分被静默丢弃且核不再归一。σ=14 需要 85 taps，
只保留 76.3% 权重，有效 σ 实为 **5.3**。`shader::MAX_TAPS` 常量定义了却从未被读取。

**做法**：改用三趟箱式平均近似高斯（W3C filter spec 与 Skia `SkBlurMask` 的同一构造）。
宽 `w` 的三次箱式方差为 `3(w²-1)/12`，故 `radius ≈ σ`；滑动窗口每像素 O(1)，
与半径无关，因此不再受核数组长度约束。

| 方案 | σ=14，260×120 面板 | 半径上限 |
|---|---|---|
| 直接卷积 85 taps | 4.66 ms（单趟） | 33 taps |
| 三次箱式 | 0.90 ms（h+v 全 6 趟） | 无 |

**注意**：`target-cpu=native` 对该卷积只快 3%，瓶颈是每像素的串行累加而非指令宽度。
优化应优先改算法。

**验证**：
- `boxblur::tests::a_hard_edge_becomes_a_ramp` —— 硬边必须变成斜坡
- `boxblur::tests::a_ramp_keeps_its_slope_right_out_to_the_edges` —— 子矩形首末像素等于夹取窗口的精确整数均值
- `gpu_backdrop_scene`「a sigma past the kernel cap still flattens the region」——
  σ=5 时区域内亮度跨度 120，σ=30 时降到 6；被钳到 16 的旧核仍留 54
- `ui-paint`「a backdrop blurs what was painted before it」—— CPU 路径同样柔化

## 背景模糊的脏区契约

模糊节点自身图元不变，像素却随下方内容改变。两条规则缺一不可：

1. **不无条件标脏**：静止帧不重绘，否则循环空转。
2. **继承式标脏**：其它脏区外延该节点自己的 `radius` 形成**探针**，与磨砂矩形相交则标脏；
   标脏范围仍夹回磨砂矩形本身。

只做第一条会让面板在陈旧背景上反复混合，表现为向背景色褪去（实测标题 `246 → 135`）。
只做第二条会让静止帧也重绘。

**GPU 侧配套**：这一整套（按精确脏矩形清空、Op 级跳过 `backdrop_is_dirty`）已随跨帧复用
一并撤除——每帧整帧清空、每个磨砂都重算，上述症状从结构上不再可能发生
（`docs/effect-cache-plan.md` §11）。

## 背景模糊：执行顺序与两个外延量

### 执行时机

模糊必须在**批次之间**、磨砂节点自身填充之前执行。

磨砂矩形内的内容由更早的批次画好，因此那里的输入本来就完整；而放到帧末统一执行会让
模糊跑到面板自身文字之后，把标题一起抹掉（实测 `246 → 130`）。

**末尾的 backdrop 必须单独排空。** 批次循环用 `backdrops[done].0 <= batch.epoch` 推进
队列，只有遇到更晚的世代才会前进；如果磨砂面板是最后一个 op、后面没有批次，条件永远
不成立，该面板从此不再执行。循环结束后需要再排空一次队列。

验证：`gpu_backdrop_scene`「a frost touching the bottom edge is blurred there too」——
把磨砂放到画面最底部（其后没有批次）时，那里的棋盘格仍必须被压平。

### 就地把源行一起模糊会退化成递归

`box_h` / `box_v` 的窗口要减去「离开」位置的像素，而那个位置**已被本趟写过**。
直接读缓冲会减掉本趟的输出，求和不再是窗口平均，而是一个 IIR 递归：从第三个像素
起就偏离真值（实测 24 像素的行，`got` 与直接窗口平均在索引 2 处开始分叉）。

修法是保留源行 / 源列：每行（列）先拷出 `w * 4`（`rows * 4`）字节，求和全部从副本读取。
代价约 15%（33 → 36 ns/px），换来与参考实现逐像素相等。

守门测试：`a_pass_matches_a_direct_window_average`、
`a_vertical_pass_matches_a_direct_window_average`——与直接窗口平均逐像素比对。

注意 `a_ramp_keeps_its_slope_right_out_to_the_edges` 里的 `last == 161` 原本记录的是
递归漂移后的结果；闭式值是 `3088/19 = 162.5 → 162`，已按闭式改正。

### 四通道 SIMD（`simd` feature）

RGBA 正好是一个四通道向量，因此运行和、加入、减去各用一条指令。开关是**编译期**的
（`#[cfg(feature = "simd")]` 选择实现），关闭时标量路径没有运行时判断。

实测（release，六趟，`crates/ui-core/examples/box_blur_bench.rs`）：

| 区域 | 半径 | 标量 | SIMD |
|---|---|---|---|
| 520×240 | 6 / 14 / 24 | 33.2 / 34.2 / 35.6 ns/px | 27.1 / 27.5 / 27.7 ns/px |
| 1040×480 | 6 / 14 / 24 | 35.9 / 35.1 / 36.0 ns/px | 27.7 / 27.6 / 27.9 ns/px |

约 23% 提升。两条参考测试在开启 feature 后同样通过，因此向量路径与标量路径受同一套
断言约束。除法仍在数组上标量执行——`wide` 没有 u32 除法，而 f32 除法会让逐像素比对
出现 ±1 LSB，取舍是先保精确。

### reach 与 band 不是同一个数

| 方法 | 语义 | 使用者 |
|---|---|---|
| `Effect::reach()` | 结果依赖的原始像素范围 | 视图层算脏区（`EffectReach`）与 CPU `backdrop_blur` 的外延 |
| `Effect::band()` | 各趟累积所需的工作范围 | CPU `backdrop_blur` 的邻域外延（每帧整帧重算后，GPU 侧不再需要它） |

箱式链每轴跑 `boxes` 趟，每趟多读一个半径，故 `band() = radius × boxes`。
把两者混用会让探针撑到 3 倍，任何微小脏区都判定「触及磨砂」，整块重算并叠加，
面板整体发灰。

外延量由效果自己声明，不在通用 API 里写死倍数——新增马赛克、液态玻璃等背景效果时，
各自给出自己的 `boxes`/`band` 即可，`Effect` 无需新增分支。

### 滤波输入必须是当帧场景，且不能裁到磨砂矩形

帧在被模糊时是**就地覆写**的，所以任何"跨帧复用输入"的做法都会让下一次模糊读到自己的
旧输出（二次模糊）。现行做法是每帧从**当帧场景整帧**读取，不存在快照。

外延问题随之消失但仍要记住：滤波器的边缘 tap 会读到磨砂矩形之外。若把输入裁到矩形边界，
`CLAMP_TO_EDGE` 会把边界值抹进边缘一带。一维模型实测（r=24）：正确值 124.9，只裁到边界内
得到 0.0。整帧读取天然满足这条；将来若做降采样采集，采集矩形必须外延到 `band()`。

### 渲染器上报它真正写过的区域

app 的脏区不等于「本帧所有变化的像素」，这个观察曾经成立过：当时磨砂只重写自己那一块，
而平台只把「脏区与磨砂矩形求交」的部分标脏，未被上报的部分继续显示上一帧内容。
`GpuRenderer::written()` 就是为这件事加的。

**2026-10-27 起这条不再需要**：渲染器每帧重画整个表面，呈现恒为全量脏区，
`written()` 已删除（`repaints_everything()` 显式报告这一事实）。整个「谁写过哪块」的记账
连同跨帧复用一起撤除，见 `docs/effect-cache-plan.md` §11。

验证：`gpu_backdrop_scene`「a repeated frame is byte-identical」——同一帧重复渲染必须逐字节
相同，覆盖了「磨砂把结果混合到自己之上而漂移」这一类问题。

### 圆角写回必须与原始背景解算

磨砂节点的圆角要抗锯齿，因此边缘像素的覆盖率为小数。写回若用预乘混合：

```
out = frost × cov + frame × (1 - cov)
```

结果依赖 `frame` 里原有的内容。首次写入 `frame=0` 得 100，第二次把 100 当背景得 150，
第三次 175——每次复用都变亮，直到边缘与内部一样，圆角看起来消失了。

正确做法是让着色器用**未模糊的当帧场景**解算覆盖率，然后覆盖写入：

```
out = frost × cov + sharp × (1 - cov)
```

等号右边三项都不受上次输出影响，因此重复执行结果不变。每帧整帧重画之后，"未模糊的
当帧场景"就是绘制到该点为止的帧缓冲，不需要额外保存副本。

配套的必要条件：着色器在 `cov <= 0`（形状之外）的分支里，必须写成 `sharp` 而不是透明。
覆盖模式下写透明会在节点外挖出一圈洞——这是「角点 alpha 归零」的真正原因。
