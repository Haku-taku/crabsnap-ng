# 状态背景与涟漪的统一裁切盒

状态：**已采纳**（2026-11-01）。`InkTarget::Rect` + `layer::Background` 已落地，
nav bar / nav rail / nav drawer / toolbar 已迁移。

## 问题

MD3 控件有三种东西要画在同一块「有效背景」上：悬停/聚焦的状态层、选中背景、按压涟漪。
它们此前各自找裁剪目标，于是出现三类不一致：

1. **三种 clip 机制并存**：`InkTarget::Node`（节点矩形 + 声明曲线）、`Shape(n)`（按绘制
   顺序取第 n 个形状）、`Descendant(id)`（后代节点矩形）。同一控件里三者可以混用。
2. **`Shape(n)` 依赖声明顺序**：条件性 push 会让索引漂移（第 4 项已记录这个坑），
   carousel 靠"背景恒为首个形状"的约定维持。
3. **没有背景的项无处可裁**：导航指示片未选中时宽度为 0，`Shape(0)` 取到零宽矩形，
   涟漪半径也是 0 —— 未选中项按下看不到水花。

## 决定

引入「有效背景」（effective background）作为唯一概念，两种表达方式：

| 表达 | 适用 | 机制 |
|---|---|---|
| `InkTarget::Rect { rect, corners }` | 槽位矩形静态（drawer 胶囊、toolbar 圆盘） | 控件每帧声明矩形与曲线 |
| `InkTarget::Descendant(id)` + 子节点 `.corners()` | 槽位位置随布局动态（nav bar/rail 的指示片居中） | 槽位是独立节点，始终存在 |

关键点：**槽位与"本帧是否画了背景"无关**。未选中的行、尚未展开的胶囊都拥有一个固定的
矩形与曲线，涟漪照样有地方裁。

控件侧用 `layer::Background` 统一构造：

```rust
let background = Background::stadium(width, height);   // 或 ::new(rect, corners)
shapes.push(background.fill(layer));                    // 状态层
shapes.push(background.ring(colour, width));            // 焦点环
.press_ink(background.press(ink, InkRadius::Cover))     // 涟漪
```

同一个 `background` 产出的三样东西不可能不一致。

## 为什么

- 状态层、选中背景、涟漪在视觉上是同一块背景的三种表现，让它们共用一份几何是唯一能
  保证一致的构造方式；分别声明再靠约定对齐，迟早会分叉。
- `Shape(n)` 的索引是**绘制顺序**，而绘制顺序会因状态变化而变；把槽位变成显式几何或
  独立节点，就把"第几个形状"这个隐式契约消掉了。
- 槽位始终存在，解决了"没有背景就没有涟漪"这一类结构性问题，而不是给每个控件补分支。

## 迁移状态

| 控件 | 状态 |
|---|---|
| nav bar / nav rail | ✅ `Descendant("indicator")` + 指示片声明 `.corners(胶囊)` |
| nav drawer | ✅ `Background::stadium`（胶囊槽位） |
| toolbar | ✅ `Background::new(disc)`（40dp 圆盘） |
| carousel | ✅ 已有 `Shape(0)`；背景恒为首个形状，暂不迁移 |
| button / chip / card / list / tab / segmented / fab menu / date/time picker | 仍用 `InkTarget::Node`，节点矩形即有效背景，曲线由 `Shape::Box` 或 `.corners()` 推断 |

## 回归检查

- `a_declared_background_clips_the_ink_even_when_the_node_paints_nothing`：声明的矩形
  优先于节点盒子，且节点不画任何东西时仍生效。
- `an_unselected_destination_still_offers_a_capsule_to_cut_the_ripple_to`：未选中目标的
  槽位存在、声明胶囊曲线、且不画形状。
- `a_destinations_ink_clips_to_its_own_capsule`（drawer）：涟漪目标是胶囊槽位。
