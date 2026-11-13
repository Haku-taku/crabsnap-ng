# 引擎子系统审计（进行中）

> 主题 2 的台账：逐子系统拿实现对照**规范或有源码的参考实现**，只记录**会导致行为错误**的
> 偏差（不是风格差异）。每条要么当场修（附先红后绿的测试），要么写进这里排队。

## 1. 子系统清单与顺序

按「出错代价 × 被依赖广度」排序，先审底层被复用最多的：

| # | 子系统 | 代码 | 对照对象 | 状态 |
|---|---|---|---|---|
| 1 | 布局：flex 主轴分配 | `ui-core/src/flex.rs` | CSS Flexbox §9.7、taffy 0.15 `compute/flexbox.rs` | ✅ 已审（1 项偏差，已修） |
| 2 | 布局：盒模型/尺寸协商/测量 | `ui-core/src/layout_engine.rs` | CSS Box Sizing、taffy | ✅ 已审（F2、F3 已修） |
| 3 | 布局：网格 | `ui-core/src/grid.rs` | CSS Grid §12、taffy | ✅ 已审（F4 已修；F5 记录未修） |
| 4 | 文本：整形/断行/簇 | `ui-text/*`、`ui-text-fontique/*` | parley、UAX #14/#29、swash/skrifa | ✅ 已审（F6 已修） |
| 5 | 命中与事件 | `ui-core/src/hit.rs`、`ui-platform/src/view.rs` | GPUI、egui 的命中/事件模型 | ✅ 已审（F7 已修，F8 记录） |
| 6 | 绘制与效果 | `ui-paint/*`、`ui-paint-gpu/*` | Skia（采样/模糊）、Flutter `BackdropFilter` | ✅ 已审（F9 已修） |
| 7 | 平台与呈现 | `ui-platform-wayland/*` | wayland.xml、wlroots 的 buffer 生命周期 | ✅ 已审（F10 已修） |
| 8 | 控件层 | `ui-widgets-*/*` | WinUI XAML、MD3、HIG（各自的参考实现） | ✅ 已审（F11 已修，F12 记录） |

## 2. 发现

### F1（已修）`overflow: hidden` 的项不肯收缩

- **规范**：CSS Flexbox §4.5「automatic minimum size」——某项在主轴上的 `overflow` **不是
  `visible`** 时，其自动最小尺寸为 0。`hidden` 与 `scroll` 同等。
- **实现**：`layout_engine.rs` 的测量趟只对 `Overflow::Scroll` 把主轴的 min-content 置零
  （`main_scrolls`），`Hidden` 仍按内容取最小值。于是**被裁切但不可滚动**的容器在 flex 行里
  无法让步，会把整行顶出容器宽度（实测：200 宽的行里放一个装了 300 宽内容的 `hidden` 容器
  + 100 宽的兄弟 → 行宽 400）。
- **修法**：新增 `main_clips`（`overflow != Visible`）只用于自动最小尺寸这条规则；
  `main_scrolls` 保留给「滚动视口不给子项内容约束」那几处（那是另一条语义）。
- **验证**：`an_overflow_hidden_item_may_shrink_below_its_content` 先红（300）后绿（100）；
  全量测试 41 个 binary 与 gallery selftest 210 项均未回归。

### F2（已修）盒模型声明了两个无人读的长度：`border` 与 `box_sizing`

- **事实**：`BoxModel::border` 的文档写着「占布局空间，不只是墨迹」，`BoxSizing` 声称可以选择
  `ContentBox`；实测 `grep -rn "box_model.border\|box_sizing"` 在整仓只命中**它们自己的定义**
  与导出——没有 builder 能写（`Decl::border()` 写的是 `Shape::Box { border }`，一笔盒内描边），
  布局里也没有任何读取点。于是两者是纯装饰：写进去也没人理。
- **规范对照**：CSS 的 `border` 确实占布局空间（`box-sizing: border-box` 时吃内容空间），
  `box-sizing: content-box` 也确实让声明尺寸只覆盖内容盒。但我们**已经**选了另一条路：
  边框是盒内描边（`docs/engine-gaps.md:570` 的缺口正是靠这一点才能切开），声明尺寸是边框盒
  （`Decl::size(fixed)` 含 padding，探针 `padded=100x40 kid_at=10,10` 证实）。两套模型并存
  只会让人以为 `border` 生效。
- **修法**：删掉 `BoxModel::border`、`BoxModel::box_sizing` 与 `BoxSizing` 枚举，
  把「声明尺寸是边框盒」写进 `BoxModel` 的文档，并用 `a_declared_size_is_a_border_box` 钉住。
  设计文档 §3.2/§4.1 同步改正（设计也要被验证，AGENTS.md §1.2）。
- **验证**：`cargo test -p ui-core` 210 项通过；`cargo build --workspace --tests --examples`
  零 warning（删除后无任何调用点需要改，反证了「无生产者」的判断）。

### F3（已修）`Text` 叶子自己的 padding 被忽略

- **事实**：`Decl::padding()` 对 `Text` 节点照常写入 `box_model`，但测量把 `available.w` 原样
  交给换行（不减 padding），`used`/`min_content`/`max_content` 也不加回 padding；绘制端
  `text_prim_of` 与 `push_text` 的 `origin` 就是 `rect.origin`。于是 padded 文本按「含 padding
  的宽度」换行、从盒角开始画，命中又与绘制同错一个角——三处各自差一个 insets。
- **为什么不与 F2 一起修**：这不是死字段，而是**活的**传参链缺口；只改换行会让文本压到
  padding 上，比现状更糟，所以先排队、整条链一次改。
- **修法**（一次改完整条链）：
  1. 测量：`wrap = min(available, decl_max) − (left+right)`；`nowrap` 的上界改夹
     「盒宽 − padding」；`used`/`min_content`/`max_content` 的宽高都加回 padding；
     `baseline` 加 `pad.top`（内联对齐读的是盒内的基线）。
  2. 绘制与命中共用 `text_origin(node)`（`view.rs`）：`rect.origin + padding` 是**唯一**答案；
     插入符定位、选区几何、IME 候选窗都从 `TextPrim` 的原点推，于是一并正确。
  3. 控件层不受影响：`ui-widgets` 的标签本来就把内边距放在外层容器上（`text.rs` 的
     `content` 节点），没有直接给文本节点写 padding 的地方。
- **验证**：
  - `a_padded_text_wraps_inside_its_padding_and_sizes_with_it`（ui-core）：10pt 四边、
    `abc def` 在 100 宽盒里 → 换行宽 80、盒 62×33、min-content 38；把 padding 当 0 时
    红（`left: 100.0`）。
  - `a_padded_text_paints_and_takes_clicks_from_its_content_origin`（ui-platform）：
    `TextPrim.origin == (10, 10)`，按在 (12,12) 得到偏移 0；`text_origin` 退回 `rect.origin`
    时红（`left: (0.0, 0.0)`）。
  - 全量 41 个 test binary、零 warning、clippy 0 error、gallery selftest 210 ✓ / 0 ✗。

### F4（已修）`fr` 轨道把 `minmax` 的下限又加了一遍

- **规范**：CSS Grid §12.7.1「find the size of an fr」——先用刚性轨道（及其基准）让出的空间
  求出 `fr` 尺寸，再令每条柔性轨道 = `max(基准, 份数 × fr)`；份数之和小于 1 时按 1 计，
  于是 `0.5fr` 只占一半、其余空着。taffy 0.15 的实现见
  `track_sizing.rs:1350 find_size_of_fr`（含「基准超过自己那份就转为刚性」的重启循环）。
- **实现**：旧代码是「free = 可用 − 所有轨道尺寸（含 fr 自己的基准）；每条 fr 再加 free×份数/总份数」，
  即**下限之上再加一份**。实测 `minmax(200px, 1fr) 1fr` 在 500 宽的网格里得到 350/150，
  按规范应是 250/250：`minmax` 的 200 只是下限，不该让这一列比同份数的兄弟更宽。
  `0.5fr` 独占 300 宽时旧代码给 300（整盒），规范给 150。
- **修法**：新增 `grid::fr_size`（§12.7.1 的重启循环，逐趟把「基准 > 自己那份」的轨道转刚性，
  `n + 1` 趟收敛），轨道取值改为 `max(基准, 份数 × fr)`。
- **验证**：`a_minmax_track_takes_its_share_rather_than_the_share_on_top_of_its_floor`
  （红 350/150 → 绿 250/250）与 `a_flex_sum_below_one_leaves_the_rest_of_the_space_alone`
  （红 300 → 绿 150）；`grid_tracks_sum_to_the_content_box` 等既有网格测试不变；
  全量 41 个 test binary 通过、gallery selftest 210 ✓ / 0 ✗。

### F5（已发现，未修）两条网格简化：auto 轨道不拉伸、fr 轨道不吃内容

- **auto 轨道不拉伸**：CSS §12.8「stretch auto tracks」把剩余空间平分给 auto 轨道，
  于是 `auto auto` 在确定宽度里两列等宽。我们的设计明确取「auto = 最大测量尺寸」
  （`docs/layout-engine-design.md:473`），即 CSS 的 `max-content` 行为。**这是有意偏离**，
  改它会重排所有用 auto 轨道的控件页，等有真实需求时再动。
- **fr 轨道的基准不含内容**：CSS 里 `1fr` 的 min 是 `auto`（内在），所以一条 fr 轨道
  **至少**是它内容的 min-content，放不下时网格溢出。我们对确定尺寸的容器直接跳过
  内容分摊（`track_sizes` 里 `definite && fr_in(..) => continue`），于是超宽内容只在自己的
  单元格里溢出，而不是撑宽轨道。两种行为都"能看"，但规范结果在不可断长词下很怪
  （taffy 实测会给出 300/0 这样的两列），暂按现状保留，记在这里备查。

### F6（已修）截断行的省略号被量成 0 宽

- **事实**：`ui_text::wrap_indented` 用 `width_of(ELLIPSIS, start..start)` 求省略号宽度，
  而两个真实测量器（`TextPass::measure_range`、`ui_text::range_width`）**只读区间、不读切片**
  ——区间是空的，于是量得 0。截断行因此把最后一簇排进去、再画上省略号，整行比盒子宽一个
  `…`（10pt 字约 5–6px）：居中的截断行会偏右，左对齐会溢出。
  既有测试 `the_ellipsized_line_still_fits_the_width` 没抓到，是因为它用的测试闭包
  `ten(s, _)` 读的是切片，恰好替生产代码量到了省略号。
- **修法**：省略号宽度改由调用方给出（`wrap`/`wrap_indented` 新增 `ellipsis_width` 参数）：
  只有测量器知道本块的样式与字面，源文本里根本没有可量的省略号。`ui-text` 用
  最后一个 span 的字号经 `FontStack` 量，`ui-platform` 用 `measure_slice(ELLIPSIS, style)`。
  顺带修正截断行用 `max_width` 而不是「首行缩进后的宽度」做上限的问题。
- **验证**：`a_truncated_line_reserves_room_for_its_ellipsis`（闭包按**区间**读，与生产
  测量器一致）在省略号宽度被置 0 时红（切到 `"aaaa"`），修后绿（`"aaa"` + 15 = 45 ≤ 49）；
  ui-text 46 项、全量 41 个 test binary、零 warning、gallery selftest 210 ✓ / 0 ✗。

### 文本子系统其余部分的核对结论（无偏差）

- 断行机会来自 `unicode-linebreak`（UAX #14），字簇来自 `ui_core::grapheme`（UAX #29）；
  贪心循环里「行尾空白悬挂、不参与行宽、不参与 min-content」与 CSS/parley 一致。
- 逐段测量意味着**跨断行机会不计字偶间距**（代码注释已声明）：断点落在空格或 CJK 字间，
  而我们整形的文字系统没有跨空格的连写，故无可观测差异。
- 超长单词按字簇切分（`split_by_cluster_with`）——与 WinUI `TextWrapping="Wrap"`
  的默认行为相同（`WrapWholeWords` 才要求整词），不是 CSS `overflow-wrap: normal` 的溢出。
- 双向文本有 `bidi.rs` 的视觉重排，emoji 呈现形式按 UTS #51 逐簇判定（此前的设计错误条目
  已记录过一次）。

### F7（已修）超出分桶上限的命中目标只在原点格子里可命中

- **事实**：`HitIndex::insert` 按 64pt 分桶；覆盖格子超过 4096 的目标「退化为只放原点桶」，
  而 `topmost`/`topmost_consumer`/`stack_at` 只查 `p` 所在的那一个桶。于是这类目标
  **在原点那 64pt 之外完全命中不到**——点击落在空白处，事件丢给下面的节点。
  注释写的「精确判定仍会过滤」只说明了多注册的代价，没说漏注册会丢命中。
- **参考模型**：egui 每帧线性遍历全部 layer 的命中矩形，没有索引也就没有这个悬崖；
  GPUI 同样按树逐层测试。索引是性能选择，不该改变语义。
- **修法**：超限目标放进 `wide: Vec<u32>`，三个查询统一走 `covering(p)`（本格 ∪ wide）。
  顺带把 `topmost` 的文档改成它真正的来源（`build` 按绘制顺序编号，层已计入）。
- **验证**：`a_target_larger_than_the_bucket_cap_is_hit_across_its_whole_extent`
  在旧「只放原点桶」写法下红（`None`），改为 wide 表后三个查询全部命中；
  全量 41 个 test binary 通过、零 warning、gallery selftest 210 ✓ / 0 ✗。

### F8（记录未修）圆角裁剪只作用于绘制，不作用于命中

- 绘制有 `plan_batches` 的圆角裁剪（`clip_shaped`），命中只有 `Node::mask: Rect`。
  于是圆角容器角落之外、矩形之内的那一点点仍会命中容器。
- **这是有分歧的约定**：浏览器与 Flutter 的 `ClipRRect` 会裁命中；WinUI 用布局矩形命中、
  不看圆角。我们三套控件的圆角都不大（≤ 12dp），暂按 WinUI 的行为保留。
- 要改的话是设计级改动：`mask` 需要携带圆角/形状，`HitTarget` 的包含判定跟着换。

### 事件模型的核对结论（无偏差）

- 指针路由是「最上层**消费**者」：`PassThrough` 留在索引里但不终结查找，
  惰性（`disabled`）节点根本不进索引；`Ignore` 的文本标签不会抢走所在控件的悬停——
  与 egui 的 `interact`/`Sense` 语义一致，也与 WinUI 的 `IsHitTestVisible=false` 相同。
- 滚轮走「指针下节点的最近可滚动祖先」（`scroll_target`），正是 CSS 的命中后向上找滚动容器；
  `Sense::SCROLL_OWNED` 的容器由引擎自己累计偏移，其余交给应用。
- 按下/拖动/释放有归属与阈值（`a_press_is_not_a_drag_until_the_pointer_travels` 等），
  按下即取焦点、释放保留焦点，与 WinUI 一致。
- 不向上冒泡点击（只有命中节点收到 `Change::Click`）。这与 Flutter 的手势竞技场一致，
  与浏览器/WinUI 的路由冒泡不同；控件在容器上声明交互位，因此不构成缺陷，记录在此备查。

### F9（已修）效果核不随渲染网格放大：HiDPI 下模糊偏窄，层的边界还会裁掉模糊的尾巴

- **事实**：核（`Kernel::radius`/`sigma`）按**逻辑**单位撰写，而渲染在设备网格上进行
  （`render_frame` 把 `self.scale` 乘上 `supersample`，几何、scissor、文本都按它换算）。
  三处没有换算：
  1. 层效果（`end_layer`）把应用的 `Effect` 原样交给卷积，既不含超采样也不含显示倍率；
  2. 磨砂（`apply_backdrop`）只乘了 `self.supersample`，丢掉了显示倍率；
  3. 绘制端给层边界留的余量 `EffectReach::actual / self.scale`——核既然是逻辑单位，
     除以倍率反而把余量缩小，于是模糊的尾巴在层边界被硬切。
  后果：2× 屏上声明的模糊只剩一半视觉半径（磨砂同比例偏窄），且层边界出现硬边。
  1× 屏上三处都恰好是无操作，所以一直没被发现。
- **参考模型**：Skia 的 `SkImageFilter`、Flutter 的 `BackdropFilter` 都以逻辑像素声明模糊，
  落到设备像素时按 `devicePixelRatio` 换算；文本路径我们本来就是这么做的（`text_scale`）。
- **修法**：新增 `effect_pass::on_grid(effect, scale)`——箱式核按 `sigma × scale` 重建、
  tap 核按半径与 sigma 同比放大，`scale == 1.0` 时原样返回（保证 1× 屏像素逐位不变）；
  `end_layer` 与 `apply_backdrop` 都走它，磨砂的余量改从换算后的核取；
  绘制端的层余量不再除以倍率。
- **验证**：`a_blur_widens_with_the_render_grid`（`scale` 被忽略时红：`sigma tracks the grid:
  2.4494898 vs 2.4494898`；修后箱式 σ 恰好翻倍、tap 核半径 4→8）；
  全量 41 个 test binary 通过、零 warning、gallery selftest 210 ✓ / 0 ✗。
  **限制**：嵌套会话是 1× 屏，像素级验证只能在 1× 下做（此时三处都是无操作）；
  2× 屏的观感验证待有 HiDPI 输出时补做，代数推导见上面第 3 条。

### 绘制与效果其余部分的核对结论（无偏差）

- 模糊是**三趟箱式**近似（`boxblur::radius_for_sigma` 由 W3C 滤镜规范/ Skia 的方差关系推出，
  `3(w²−1)/12`），任意半径 O(1)；tap 核有 `MAX_KERNEL_RADIUS` 截断并拒绝非归一化（避免"变暗"）。
- 组透明度走 `push_layer` + `ColorMatrix::opacity`：子树合成一次再按组透明混合，
  重叠子节点不会叠加变暗——与 CSS `opacity` 的合成语义一致，不是逐图元乘 alpha。
- 磨砂在**填充之前**发出（读的是背景而非本节点），裁剪与盒子同受一道 clip 管；
  结果写回按覆盖率解算且幂等（此前的设计错误条目已记录过一次）。
- 滤波像素从不跨帧复用（`Scratch` 每次全量重算），与 Flutter/Skia/Haze 的做法一致；
  曾经的跨帧复用因「边缘显示旧帧」被撤除（`docs/effect-cache-plan.md` §11）。

### F10（已修）release 点只发不等：写回仍可能撞上屏幕正在读的那一帧

- **协议**：`wp_linux_drm_syncobj_surface_v1` 的显式同步是**双向**的——acquire 点让
  compositor 等客户端的写入；release 点让客户端知道 compositor **已读完**某个缓冲。
  wayland 的 `wl_buffer.release` 表达同一件事，只是粒度在缓冲而非时间点。
- **事实**：`GpuWindow` 每帧都 `set_release_point`（递增），但 `sync_release_timeline`
  的注释写着「仅用于保持存活」——没有任何地方等过它。目标池只有 **2** 张
  （`TARGETS = 2`），第 N+2 帧写回的正是第 N 帧那张 dma-buf；若 compositor 仍在扫描它，
  画面会采到画了一半的帧。无 syncobj 的 compositor 上 `wl_buffer.release` 事件也只被
  记成计数（`dmabuf_inflight()`），同样没有闸门。
- **内核实测（`examples/syncobj_probe.rs`，本机 /dev/dri/renderD128）**：
  1. `drmSyncobjTimelineWait` **不带** `DRM_SYNCOBJ_WAIT_FLAGS_WAIT_FOR_SUBMIT`
     去等一个尚未提交的点，直接 `EINVAL`（-22）——这正是第一版探针拿到的结果；
  2. `timeout_nsec` 是**绝对的 CLOCK_MONOTONIC 期限**，不是时长；
  3. 带上该标志后：已到达的点 `wait(1)` **1.7 µs 返回 true**，未到达的点
     `wait(2)` **~51 ms 返回 false**。
- **修法**：`SyncTimeline::wait(point, Duration)`（绝对期限 + WAIT_FOR_SUBMIT，任何
  非零返回都算「还没到」，绝不因等待失败而丢帧）；`GpuWindow` 记住每个目标上次的
  release 点，复用它之前先等（50 ms 上限），重建目标池时清空；`present_dmabuf` 改为
  接收该点而不是自己计数。
- **验证**：`cargo run -p ui-gpu-egl --example syncobj_probe` 输出 `point 1 -> true in
  1.693µs` / `point 2 -> false in 51.403158ms`；全量 41 个 test binary、零 warning、
  gallery selftest 210 ✓ / 0 ✗。**限制**：嵌套 Hyprland 不提供
  `wp_linux_drm_syncobj_manager_v1`（日志 `explicit sync unavailable`），所以这条路径在
  本机运行时是惰性的，验证只能做到设备级探针；无 syncobj 的 compositor 仍只有
  「2 张缓冲 + frame callback」这道弱保证，记为已知缺口。

### F11（已修）键盘激活有两条送达路径：MD3 控件收不到，Fluent/Glass 收到两次

- **事实**：引擎在 Enter/Space 时自己 `app_changed(id, Change::Activated)`，**同时**把
  `activated` 作为一帧事实交给控件；而控件是否把它折进 `change()` 各写各的：
  - `ui-widgets-md3` 的 21 处 `change()`/`decl` 全部只读 `state.clicked`。于是按钮、卡片、
    列表行只靠引擎那条「裸 Activated」工作，而语义更丰富的控件彻底失灵——
    复选框/单选框/开关/筛选芯片/分段的 `change()` 报的是 `Toggled`/`Selected`，键盘却
    永远走不到它们，应用收到的是读不成新值的 `Activated`。键盘用户无法勾选、切换、选择。
  - `ui-widgets-fluent`、`ui-widgets-glass` 有 8 处写成 `clicked || activated`。这些控件
    一次按键会**送达两次**：引擎的裸 `Activated`，加上下一帧控件自己的语义变化
    （玻璃开关 = 翻两次、菜单行 = 执行两次、列表项 = 打开两次）。
- **参考模型**：激活的语义由控件拥有，引擎只负责路由事实。Flutter 把 Enter/Space 经焦点
  系统变成 `ActivateIntent` → `ActivateAction` → 控件的 `onPressed`；WinUI 的 `ButtonBase`
  在自己的按键处理里 `RaiseClick`；egui 的 `Button` 自己查 `key_pressed(Enter/Space)`。
  没有一家是引擎绕过控件直接向应用报语义变化的。
- **修法**：
  1. 引擎只保留事实：`activated` 不再直接变成应用可见的 `Change`，并且新增
     `activated_path`（与 `clicked_path` 同构）——激活落在聚焦节点上，而控件根常是它的
     祖先（snackbar 的动作是条的子节点），`Cx::activated_within` 与 `Dep::ActivatedWithin`
     让复合控件照旧看得见。
  2. `WidgetState::activation()` = `clicked || activated` 成为**唯一定义**，MD3 全部
     控件、Fluent/Glass 原有的内联 `clicked || activated` 一律改用它。复合控件里需要
     按下点的映射（导航栏、页签栏、工具栏、FAB 菜单的行）显式处理「键盘没有按下点」。
  3. 裸节点（只有 `Decl`、没有控件 `change()`）仍有兜底：那一帧若激活路径上没有任何
     控件上报，引擎补一条 `Change::Activated`——历史行为不丢，控件在场时不重复。
- **验证**：
  - `enter_activates_the_focused_node_and_nothing_when_unfocused`：恢复引擎那条裸上报时
    红（`left: [bar, bar]`）、改后绿（`[bar]` 每帧一条）。
  - `enter_toggles_like_a_click_rather_than_reporting_a_bare_activation`（MD3 复选框）：
    `change()` 读回 `clicked` 时红、改后绿（`Toggled { on: Some(true) }`）。
  - 全量 41 个 test binary 通过、零 warning、gallery selftest 210 ✓ / 0 ✗。

### F12（已记录，未修）单节点复合栏没有键盘入口

- **事实**：MD3 的 `NavBar`、`TabBar`、`NavRail`、`NavDrawer`、`Toolbar` 把整条做成**一个**
  可聚焦节点（每处只有一个 `sense(FOCUSABLE)`），`change()` 靠 `state.local`（按下点）
  判断选中了哪一项。键盘激活时 `state.local` 是 `None`，于是 Enter/Space 在整条上
  什么也不做：键盘用户切不了页签、换不了目的地、按不了工具栏按钮。
- **参考模型**：Fluent 的 NavigationView 与 Flutter 的 `NavigationBar` 都把**每个目标**
  做成独立焦点停靠点（Tab 逐个走，方向键在目标间移动，Enter/Space 激活当前目标）。
- **为什么不一起修**：这五处要改成「每项一个可聚焦节点 + 方向键在项间漫游 + 焦点环画在
  项上」，牵动布局（等分/内缩）、涟漪目标（`InkTarget::Rect` 换成项矩形）、选中动画与
  命中，属于设计级改动；按 F8 的先例记录在案，等有明确需求时统一设计一套「条目栏」。

### 控件层其余部分的核对结论（无偏差）

- 禁用：引擎把惰性节点挡在命中索引与 Tab 环之外（`focusables_within` 与
  `hit_behavior_of` 同一道门），控件只需按各自的淡化规范绘制（MD3 38%/12%，
  Fluent 画刷，玻璃）——三套都与各自的导出/主题一致。
- 焦点外观：MD3 的 10% 聚焦层与 Fluent 的焦点环都只随 `focus_visible`（键盘带来）出现，
  指针焦点不画；与 Flutter 的 `highlightMode`、WinUI 的 `ShowFocusVisualsOnPointer` 一致。
- 关闭/取消：浮层的 `Dismissed` 归到浮层根、Escape 走快捷键，与三家的「轻关闭」一致
  （此前两条相关设计错误已记录）。
