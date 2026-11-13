# ADR: 模态的目标由引擎在布局后解析，而不是由应用传矩形

状态：**已采纳并实现**（2026-10-30）。代码在 `ui/crates/ui-core/src/tree.rs` 的
`ModalTarget` / `ContainerParams::{modal, modal_host}`、`layout_engine.rs` 的
`modal_target_rect` 与 `place_node`、`view.rs` 的 `modal_root` / `in_key_scope` /
`focusables`。

## 旧做法与问题

`DialogContent::area: Size` 由应用在声明期传入，控件用 `SizeClass::fixed(area.w, area.h)`
做根节点，gallery 直接写 `Size::new(WIN_W, WIN_H)`。问题：

1. **宿主矩形只有布局后才知道。** 宿主被兄弟挤压、滚动、将来还有布局动画时，应用在声明期
   拿不到下一帧的矩形，永远差一帧。
2. **差一帧的矩形会留下脏区残影**：`compute_damage` 按 old ∪ new 的墨迹矩形差分，遮罩晚一帧
   就有一帧宽的条纹。
3. **内联在宿主子树里会被压住**：本引擎的绘制顺序与命中 z 都来自 `Tree::walk()` 的前序下标，
   把模态当作宿主的最后一个子节点，只盖得住宿主子树内部；宿主的后兄弟、宿主祖先的后兄弟
   仍画在它上面并抢走点击。
4. **遮罩只是图片**：`focusables_with_rects` 遍历整棵树，模态打开后 Tab / Enter 仍会到达
   scrim 背后的控件。

## 改用什么

**目标解析 + 渲染提升 + 键盘作用域**三件事分开做，与主流框架一致：

- `ModalTarget::{Global, Proximate}` 是构建期参数（与 `barrier` 同级），默认由调用方选。
  `Global` 取根节点矩形，`Proximate` 取最近的 `modal_host` 祖先，找不到时回退 `Global`。
- 声明了 `modal` 的节点在 `place_node` 里被改放到目标矩形上（顺带把 `parent_mask` 也换成
  目标矩形），所以它的居中/裁剪都相对目标，而不是相对树给它的位置。
- 目标在**布局趟内**解析：目标是祖先，放置时已经定好，因此不需要外部传矩形，同一帧跟上
  窗口与控件的尺寸变化。
- `walk()` 把模态子树推到末尾（后声明的更靠后，因此最新的在最上面），命中 z 由此正确；
  绘制另有一套顺序（`paint_node` 递归子节点列表），`paint_tree` 用同一个推迟规则跟它对齐
  ——等价于 Flutter 把 overlay child 重挂到 `_RenderTheater`、Qt 重挂到窗口 overlay、
  Zed 把 `ModalLayer` 放在根的最后。**两套顺序必须由同一条规则产生**，否则模态能点却画
  在下面（`a_modal_paints_last_as_well_as_hitting_last` 钉住这一点）。
- 模态节点与 `NodeKind::Overlay` 共用「浮起」概念（`Tree::floats`）：不占父的槽位、不参与
  gap 与 `distribute`。
- `modal_root()` 给出最上面那个模态；`focusables` / `focusables_with_rects` 限定在它之内，
  Enter / Space 也走 `in_key_scope`。

## 与成熟做法的对应

| 本仓库 | Flutter | UIKit | libadwaita | Qt Quick | GPUI/Zed |
|---|---|---|---|---|---|
| `Global` | `Overlay.of(rootOverlay: true)`、`showDialog(useRootNavigator: true)`（默认） | 无 `currentContext` 时的 `.sheet` | `adw_dialog_present(d, window)` | 唯一行为 | `ModalLayer` 挂根 |
| `Proximate` | `rootOverlay: false` / `useRootNavigator: false` / `OverlayPortal(overlayLocation: nearestOverlay)`（默认） | `definesPresentationContext` + `.currentContext` | `find_dialog_host()` 沿祖先找 `AdwDialogHost` | 无（`parent:` 只是坐标系） | 无 |
| 找不到宿主时回退 `Global` | 抛清晰错误 | 回退窗口 root VC | 开独立窗口 | — | — |

`Global` 是所有框架的公共子集；`Proximate` 有 Flutter 与 UIKit 两个一等公民先例，
libadwaita 有同样的 API 形状（但宿主集合封闭在窗口，实际仍是 `Global`）。
Qt / Compose / SwiftUI 只有 `Global`——它们把「覆盖某个子区域」留给不带模态语义的
`Stack` / `.overlay`，这正说明**模态语义**（焦点作用域、拦截、居中）必须由引擎提供，
而不是由应用画一个矩形假装。

## 哪条检查抓回归

- `a_modal_covers_its_target_instead_of_the_slot_the_tree_gave_it`（`ui-core`）：
  声明在带内衬窗口里的模态，`Proximate` 的矩形等于宿主、`Global` 等于窗口。
- `the_scrim_covers_its_target_and_the_surface_centres_in_it`（`ui-widgets-md3`）：
  同一件事在控件层，且表面在目标内居中。
- `a_modal_owns_the_keyboard_so_tab_cannot_walk_out_of_it`（`ui-platform`）：Tab 环
  只在模态内环绕。
- `a_modal_paints_last_as_well_as_hitting_last`（`ui-platform`）：声明的模态在绘制顺序里
  排在后声明兄弟之后，与命中 z 同序。
- 浮起不占槽位由既有的 `an_overlay_child_takes_no_space_in_the_flow` /
  `an_overlay_child_does_not_add_a_gap_to_the_parents_size` 覆盖（它们现在走同一个
  `Tree::floats`）。

## 尚未做的

- 模态的进入/退出过渡（`docs/engine-gaps.md` #13）。
- 多个模态同时存在时的显式层序计数（现在按树声明顺序，后声明者在上）。
- 模态打开时，原本聚焦在它之外的节点仍带着焦点环（只是听不到键盘）；缺一次
  「打开即把焦点交进模态」的移交。
- scrim 用毛玻璃/背景模糊时的脏区联动。
