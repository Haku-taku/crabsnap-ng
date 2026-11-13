# MD3 `Switch` 参数总表（逐条对照源码）

来源（本机 sparse checkout `/home/ollie/git/widget-ref/flutter`）：

| 文件 | 行 |
|---|---|
| `packages/flutter/lib/src/material/switch.dart` | `_SwitchConfigM3` 2310-2400、`_SwitchDefaultsM3` 2180-2310、`_SwitchPainter.paint` 1509-1780 |
| `packages/flutter/lib/src/widgets/toggleable.dart` | 全文（四个动画控制器 + `paintRadialReaction`） |

在线 demo `chayanforyou/flutter_material_3_demo` 是官方 `material_3_demo` 样例副本
（`pubspec.yaml` 只依赖 Flutter SDK），用的就是这两份源码。

---

## 1. 四个动画控制器

| 控制器 | 时长 | 曲线 | 谁启动 |
|---|---|---|---|
| `positionController` | `switchConfig.toggleDuration` = **300ms**（M3；M2 140、adaptive 200） | 见下 | 语义值变化 → `animateToValue()` |
| `reactionController` | `kRadialReactionDuration` = **100ms** | `Curves.fastOutSlowIn` | `onTapDown` → `forward()`；`onTapUp`/`onTapCancel` → `reverse()` |
| `reactionHoverFadeController` | `_kReactionFadeDuration` = **50ms** | `Curves.fastOutSlowIn` | `onShowHoverHighlight` |
| `reactionFocusFadeController` | **50ms** | `Curves.fastOutSlowIn` | `onShowFocusHighlight`（**不是** `focused`） |

**position 的曲线**（`updateCurve()`，仅在 `position.value == 0 || 1` 时被调用）：

```dart
if (Theme.of(context).useMaterial3) {
  position..curve = Curves.easeOutBack
          ..reverseCurve = Curves.easeOutBack.flipped;
} else {
  position..curve = Curves.easeIn
          ..reverseCurve = Curves.easeOut;
}
```

**派生曲线**：`_colorAnimation = CurvedAnimation(parent: positionController, curve: Curves.easeOut)`
（注意挂在**原始** controller 上，不是 `position`）。

---

## 2. 几何（`_SwitchConfigM3`）

| 常量 | 值 |
|---|---|
| `switchWidth` / `trackWidth` / `trackHeight` | 52 / 52 / 32 |
| `switchMinSize` | (48, 40) |
| `switchHeight` | `switchMinSize.height + 8` = **48** |
| `padding` | `EdgeInsets.symmetric(horizontal: 4)` → 控件 **60×48** |
| 轨道半径 | `trackHeight / 2` = 16 |
| 轨道内行程 `trackInnerLength` | `52 − 32` = **20** |
| `inactiveThumbRadius` | 8 → 16×16 |
| `activeThumbRadius` | 12 → 24×24 |
| `pressedThumbRadius` | 14 → 28×28 |
| `transitionalThumbSize` | **34×22** |
| `thumbRadiusWithIcon` | 12（有图标时两端都用 24） |
| `thumbOffset` | `null` → M3 的 `inset` 恒为 0（收缩是 M2 的） |
| `thumbShadow` | `kElevationToShadow[0]` = **空列表**（无阴影） |
| `splashRadius` | `40.0 / 2` = **20** |
| `iconSize` | 16 |

**位置公式**：

```
thumbCentreX = trackRadius + progress × trackInnerLength        = 16 + p×20   (16 … 36)
thumbLeft    = trackRadius + extension/2 − w/2 + progress×inner  = 16 − w/2 + p×20
thumbTop     = trackTop − (h/2 − trackRadius)                   = 8 + (32 − h)/2
```

---

## 3. 尺寸（形状）

### 3.1 端点：`inactiveThumbSize` / `activeThumbSize` 是**锁存**的

`paint()` 每帧按下面的状态机写两个字段，**写不到就保持上一帧的值**（这就是「锁存」）：

```dart
if (reaction.status == reverse && !_stopPressAnimation) _stopPressAnimation = true;
else                                                    _stopPressAnimation = false;

if (!_stopPressAnimation) {
  if (reaction.isCompleted) {                    // 拖动（而不是点击）
    _pressedInactiveThumbRadius = lerp(8,  14, reaction.value);
    _pressedActiveThumbRadius   = lerp(12, 14, reaction.value);
  }
  if (currentValue == 0) {                       // 滑块停在关闭端
    _pressedInactiveThumbRadius = lerp(8,  14, reaction.value);
    _pressedActiveThumbRadius   = 12;
  }
  if (currentValue == 1) {                       // 滑块停在开启端
    _pressedActiveThumbRadius   = lerp(12, 14, reaction.value);
    _pressedInactiveThumbRadius = 8;
  }
}
inactiveThumbSize = Size.fromRadius(_pressedInactiveThumbRadius ?? 8);
activeThumbSize   = Size.fromRadius(_pressedActiveThumbRadius   ?? 12);
```

`currentValue = position.value`（**曲线之后**的值）。

| 情形 | inactive 端点 | active 端点 |
|---|---|---|
| 无按压 | 16 | 24 |
| 按住在关闭端 | 8→14 按 reaction 插值 → 16…28 | 24 |
| 按住在开启端 | 16 | 12→14 → 24…28 |
| 拖动中（`reaction.isCompleted`） | 8→14 → 16…28 | 12→14 → 24…28 |
| 松手回缩（`_stopPressAnimation`） | 冻结在上一帧的值 | 冻结 |

### 3.2 两条序列（`TweenSequence.animate(positionController)`，自变量是**原始**进度）

正向（权重 11 / 72 / 17）：

| 区间 | 值 | 段曲线 |
|---|---|---|
| 0 … 0.11 | `inactiveThumbSize → 34×22` | `Cubic(0.31, 0.00, 0.56, 1.00)` |
| 0.11 … 0.83 | `34×22 → activeThumbSize` | `Cubic(0.20, 0.00, 0.00, 1.00)` |
| 0.83 … 1 | 常数 `activeThumbSize` | — |

反向（权重 17 / 72 / 11）：

| 区间 | 值 | 段曲线 |
|---|---|---|
| 0 … 0.17 | 常数 `inactiveThumbSize` | — |
| 0.17 … 0.89 | `inactiveThumbSize → 34×22` | `Cubic(0.20, 0.00, 0.00, 1.00).flipped` |
| 0.89 … 1 | `34×22 → activeThumbSize` | `Cubic(0.31, 0.00, 0.56, 1.00).flipped` |

### 3.3 短路与方向

```dart
if (reaction.isCompleted) thumbSize = Size.fromRadius(pressedThumbRadius);   // 28×28
else if (position.isDismissed || position.status == forward)
       thumbSize = thumbSizeAnimation(true).value;
else   thumbSize = thumbSizeAnimation(false).value;
```

- **胶囊永远在「离开的那一端」往外 11%**：正向 0.11，反向 0.89。
- 按下/拖动期间是 **28 圆**，不参与序列。
- 反向腿的自变量仍是 `position.value`（1 → 0），所以反向的 0.89 就是「刚离开开启端」。

---

## 4. 颜色

全部按 `colorValue = easeOut(positionController)` 插值：

```dart
final Color  trackColor        = lerp(inactiveTrackColor,  activeTrackColor,  colorValue);
final Color? trackOutlineColor = lerp(inactiveOutlineColor, activeOutlineColor, colorValue);
final double trackOutlineWidth = lerp(inactiveWidth, activeWidth, colorValue);   // 2 → 2
```

### 4.1 轨道填充 `trackColor`

| 状态 | 关闭 | 开启 |
|---|---|---|
| 常态 / 悬停 / 聚焦 / 按下 | `surfaceContainerHighest` | `primary` |
| 禁用 | `surfaceContainerHighest @ 0.12` | `onSurface @ 0.12` |

### 4.2 描边

| 项 | 关闭 | 开启 | 禁用 |
|---|---|---|---|
| 颜色 | `outline` | `transparent` | `onSurface @ 0.12` |
| 宽度 | **2** | **2** | **2** |

宽度恒为 2（`MaterialStatePropertyAll(2.0)`），只有颜色在淡出。

### 4.3 滑块 `thumbColor`

先按状态选一对端点，再用同一个 `colorValue` 插值，最后 `alphaBlend(lerped, surfaceColor)`：

| 条件 | 起点 | 终点 |
|---|---|---|
| `!reaction.isDismissed`（按下/拖动中） | `onSurfaceVariant` | `primaryContainer` |
| `status == forward` | `onSurfaceVariant` | `onPrimary` |
| `status == reverse` | `outline` | `primaryContainer` |
| 静止 | `outline` | `onPrimary` |
| 禁用（选中 / 未选中） | `surface` / `onSurface @ 0.38` | 同左（不插值） |

### 4.4 状态层 `overlayColor`（只被涟漪用作四种颜色）

| 状态 | 值 |
|---|---|
| 选中 + 按下 | `primary @ 0.10` |
| 选中 + 悬停 | `primary @ 0.08` |
| 选中 + 聚焦 | `primary @ 0.10` |
| 未选中 + 按下 | `onSurface @ 0.10` |
| 未选中 + 聚焦 | `onSurface @ 0.10` |
| 未选中 + 悬停 | `onSurface @ 0.08` |
| 其它 | `null` |

---

## 5. 涟漪

```dart
if (!reaction.isDismissed || !reactionFocusFade.isDismissed || !reactionHoverFade.isDismissed) {
  color = lerp( lerp( lerp(inactiveReactionColor, reactionColor, position.value),
                      hoverColor,  reactionHoverFade.value),
                focusColor, reactionFocusFade.value);
  radius = (isFocused || isHovered) ? splashRadius : reaction.value * splashRadius;
  if (radius > 0) canvas.drawCircle(origin, radius, color);
}
```

| 项 | 值 |
|---|---|
| 半径 | 悬停/聚焦 → 20；否则 `reaction.value × 20` |
| 圆心 | `(thumbLeft + thumbSize.height/2, size.height/2)` = 滑块**左端帽**圆心、控件竖向中心 |
| 颜色 | 三层嵌套插值，**位置用 `position.value`（曲线后）** |
| 出现 | `reaction` 100ms 0→20（按下）；悬停/聚焦直接 20 |
| 消失 | `reaction` 100ms 20→0；悬停/聚焦只淡颜色（50ms），半径立刻归零 |
| 裁剪 | **无**（`CustomPaint` 不裁剪） |

注意：涟漪的颜色用**曲线后**的 `position.value`，而轨道/滑块颜色用**原始** controller
（`_colorAnimation`）。两者不同步是参考的真实行为。

---

## 6. 绘制顺序与命中

```
track fill → track outline → radial reaction → thumb
```

命中：`GestureDetector` 包住整个 60×48，`onTapDown/onTap/onTapUp/onTapCancel` 与
`onHorizontalDragStart/Update/End` 都在这一层；滑块**没有**自己的命中区。
`downPosition` 只用于记录起点。

`states` 集合：`disabled`（`onChanged == null`）、`hovered`、`focused`、`selected`。
`onShowFocusHighlight` 与 `focused` **不同**：指针按下会给焦点但不请求焦点环。

---

## 7. 一次点击的时序（关闭 → 开启，300ms）

| t | progress | reaction | 轨道色 | 滑块尺寸 | 滑块中心 |
|---|---|---|---|---|---|
| 0 | 0 | 0 | 关闭色 | 16 | 16 |
| 按下 | 0 | 0→1 (100ms) | 不变 | 16→28（端点锁存） | 16 |
| 松开 | 0→1 (300ms) | 1→0 (100ms) | `easeOut(p)` 插值 | 100ms 后进入序列：0.11 处 34×22 → 0.83 处 24 | 16 + p×20 |
| 300ms | 1 | 0 | 开启色 | 24 | 36 |

---

## 8. 与当前实现（`ui-widgets-md3/src/toggle.rs`）的差异清单

| 项 | 参考 | 现在 |
|---|---|---|
| 尺寸端点锁存 | 有（见 §3.1） | **无**（常数 16/24） |
| 尺寸序列 | `TweenSequence`（原始进度） | 纯函数 `size_at`（原始进度）✓ |
| 颜色 | `easeOut(原始进度)` | ✓ |
| 滑块色端点对 | 四选一（见 §4.3） | ✓ |
| 涟漪圆心 | 左端帽 | ✓ |
| 涟漪半径 | 悬停/聚焦 20，否则 `reaction×20` | ✓ |
| 涟漪颜色位置项 | `position.value`（曲线后） | ✓ |
| 涟漪裁剪 | 不裁剪 | ✓（不裁剪） |
| 悬停/聚焦淡变 | 50ms | ✓ |
| 焦点环 | `onShowFocusHighlight` | ✓（`focus_visible`） |
| 描边宽度 | 恒 2 | ✓ |
| 禁用态 | 四套颜色 | **缺** |
| 图标 | 可选 16px | 未用 |
