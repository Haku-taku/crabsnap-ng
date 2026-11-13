# ADR: 命名槽由容器在构建期声明，槽 id 由引擎派生

状态：**已采纳并实现**（2026-10-30）。代码在 `ui/crates/ui-core/src/slot.rs` 与
`ui/crates/ui-core/src/tree.rs` 的 `Decl::{slot_id, slot, slot_with}`；首个采用者是
`ui/crates/ui-widgets-md3/src/list.rs` 的 `ListItemSlot`。

## 旧做法与问题

列表项的前置/后置区是硬编码位置：`id.with("lead")`、`id.with("trail")`；前置内容只能由
`ListLeading` 枚举加 `fn(Id, Color, f32) -> Decl` 指针决定，应用塞不进自己的子树。问题：

1. **位置与内容类型绑死**：想放别的东西就得改控件、加枚举分支。
2. **应用自己编 id**：内容要按 id 参与路由/动画时，必须猜中控件的派生规则。
3. **没有「不给」这个状态**：只能约定尺寸为 0，容易在布局里留下间隙。

## 改用什么

**Model A：容器声明词汇表，应用只填充。**

- `SlotName` 是静态名字（`&'static str`）；容器用枚举 + `ALL` 给出**有序**词汇表。
- `SlotFills` 是应用的填充集合，按名字索引；同名再填是**后者覆盖**（不报错），
  于是「内建默认 + 应用覆盖」是一条自然路径，内建内容只是第一个默认填充。
- `Decl::slot(name, content)` / `Decl::slot_with(name, frame, content)` 把填充挂上容器：
  槽节点 id = `slot_id(容器 id, 槽名)`，**应用不编 id**。`frame` 是容器给槽套的样式
  （内衬、尺寸、类型）；它的 id 被派生 id 替换，内容追加在 frame 自己的子节点之后。
- 未填充的槽不产生节点，因此不占位、不留空洞；槽数量 0..n。
- 布局位置由**容器**决定，不按应用填充顺序：列表项把 leading 放在文字列前、trailing 放在后。
- 同一槽填两次是后者覆盖：`slot_with` 找到同 id 的子节点就地替换，位置不变。

## 与元素缓存、`Decl` 深比较的相互作用

派生 id 只依赖 (容器 id, 槽名)，两者跨帧稳定，所以换内容不会换槽节点：
`Tree::apply` 复用槽节点，只创建/回收内容节点。内容仍是应用自己的 `Rc<Decl>`，
`Decl` 的 `Rc::ptr_eq` 快路径照常命中；槽外壳每帧新建，但字段深比较是常数代价。

槽填充随参数进入 `Widget::decl`，因此它和任何其他参数受同一条缓存规则约束：内容变了而
`element_decl` 的缓存未被作废时，命中返回的是上一帧的整棵声明。应用照常用 `changed` /
`Cx::invalidate_decls` 上报，派生 id 本身不需要额外处理。

## 哪条检查抓回归

- `a_slot_keeps_its_id_when_its_content_changes`（`ui-core`）：内容换 id，槽节点必须复用。
- `filling_one_slot_twice_keeps_the_last_content_where_the_first_landed`：覆盖而非追加。
- `a_name_nobody_filled_reads_as_absent`：缺席不回退到别的填充。
- `a_slot_keeps_the_frames_style_and_puts_the_content_inside_it`：容器样式不被丢弃。
- `a_bare_slot_adds_no_padding_gap_or_displacement`：外壳不加内衬/间隙/位移。
- `an_app_fill_outranks_the_built_in_mark`、`an_unfilled_leading_slot_leaves_no_gap_before_the_text`、
  `slots_lay_out_around_the_text_column_when_filled_backwards`（`ui-widgets-md3`）。

## 未做的

- **dialog / carousel 尚未接入**：`DialogContent` 由 gallery 以结构体字面量构造，新增字段会
  破坏调用方；等那一轮改动时再把各 band 换成命名槽。
- **裸槽的外壳是 `Stack`**：父容器的 `cross_align` 会拉伸外壳，而 Stack 仍按测得尺寸放置
  内容，于是裸槽内容不再随父拉伸。需要几何一致时传 frame（列表 leading 就是这样），
  或后续加一个直通节点。
- 词汇表没有运行时注册表：越界名字由容器的 `debug_assert` 抓，release 下静默忽略。
