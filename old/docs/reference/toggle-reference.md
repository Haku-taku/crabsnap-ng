# 三种风格的开关：参考实现原文摘录

本文件只做**摘录与标注**，不含结论性设计。所有代码逐字取自本地参考实现，路径与行号可核对。

| 风格 | 参考文件 | 行数 |
|---|---|---|
| MD3 | `widget-ref/flutter/packages/flutter/lib/src/material/switch.dart` | 2398 |
| Fluent (WinUI 3) | `widget-ref/fluent_ui/lib/src/controls/inputs/toggle_switch.dart` | 610 |
| Glass | `widget-ref/liquid_glass_widgets/lib/widgets/interactive/glass_switch.dart` | 753 |

**注意**：Flutter 的 `ToggleableStateMixin`（提供 `position` / `positionController` /
`reactionController` / `animateToValue` / `_handleTap`）位于 `material/toggleable.dart`，
**本地参考中缺失**，因此点击路径只能看到 `Switch` 侧的调用点。

---

## 1. MD3（Flutter Material 3）

### 1.1 几何 — `_SwitchConfigM3`（`switch.dart:2308-2398`）

```dart
@override double get activeThumbRadius   => 24.0 / 2;
@override double get inactiveThumbRadius => 16.0 / 2;
@override double get pressedThumbRadius  => 28.0 / 2;
@override double get switchHeight        => switchMinSize.height + 8.0;
@override double get switchWidth         => 52.0;
@override double get thumbRadiusWithIcon => 24.0 / 2;
@override List<BoxShadow>? get thumbShadow => kElevationToShadow[0];
@override double get trackHeight         => 32.0;
@override double get trackWidth          => 52.0;
// The thumb size at the middle of the track. Hand coded default based on the animation specs.
@override Size get transitionalThumbSize => const Size(34, 22);
// Hand coded default based on the animation specs.
@override int get toggleDuration         => 300;
@override double? get thumbOffset        => null;
@override Size get switchMinSize         => const Size(kMinInteractiveDimension, kMinInteractiveDimension - 8.0);
```

**轨道内侧长度**（`switch.dart:829-859`）：

```dart
final double trackInnerStart = config.trackHeight / 2.0;
final double trackInnerEnd   = config.trackWidth - trackInnerStart;
final double trackInnerLength = trackInnerEnd - trackInnerStart;
```

### 1.2 配色 — 逐状态表（`switch.dart:2185-2260`）

**thumbColor**：

```dart
if (states.contains(WidgetState.disabled)) {
  if (states.contains(WidgetState.selected)) return _colors.surface.withOpacity(1.0);
  return _colors.onSurface.withOpacity(0.38);
}
if (states.contains(WidgetState.selected)) {
  if (states.contains(WidgetState.pressed)) return _colors.primaryContainer;
  if (states.contains(WidgetState.hovered)) return _colors.primaryContainer;
  if (states.contains(WidgetState.focused)) return _colors.primaryContainer;
  return _colors.onPrimary;
}
if (states.contains(WidgetState.pressed)) return _colors.onSurfaceVariant;
if (states.contains(WidgetState.hovered)) return _colors.onSurfaceVariant;
if (states.contains(WidgetState.focused)) return _colors.onSurfaceVariant;
return _colors.outline;
```

**trackColor**：

```dart
if (states.contains(WidgetState.disabled)) {
  if (states.contains(WidgetState.selected)) return _colors.onSurface.withOpacity(0.12);
  return _colors.surfaceContainerHighest.withOpacity(0.12);
}
if (states.contains(WidgetState.selected)) {
  if (pressed) return _colors.primary;
  if (hovered) return _colors.primary;
  if (focused) return _colors.primary;
  return _colors.primary;
}
if (pressed) return _colors.surfaceContainerHighest;
if (hovered) return _colors.surfaceContainerHighest;
if (focused) return _colors.surfaceContainerHighest;
return _colors.surfaceContainerHighest;
```

**trackOutlineColor**（`switch.dart:2252-2260`）：

```dart
if (states.contains(WidgetState.selected)) return Colors.transparent;
if (states.contains(WidgetState.disabled)) return _colors.onSurface.withOpacity(0.12);
return _colors.outline;
```

**iconColor**（`switch.dart:2321-2355`）：禁用时选中 `onSurface@0.38` / 未选 `surfaceContainerHighest@0.38`；
否则选中一律 `onPrimaryContainer`，未选一律 `surfaceContainerHighest`。

### 1.3 滑块尺寸序列（`switch.dart:1540-1600`）

两端尺寸：

```dart
final inactiveThumbSize = Size.fromRadius(inactiveThumbRadius);   // 16
final activeThumbSize   = Size.fromRadius(activeThumbRadius);     // 24
final pressedThumbSize  = Size.fromRadius(pressedThumbRadius);    // 28
```

正向序列（`isForward == true`）：

```dart
TweenSequenceItem(tween: Tween<Size>(begin: inactiveThumbSize, end: transitionalThumbSize)
    .chain(CurveTween(curve: const Cubic(0.31, 0.00, 0.56, 1.00))), weight: 11),
TweenSequenceItem(tween: Tween<Size>(begin: transitionalThumbSize, end: activeThumbSize)
    .chain(CurveTween(curve: const Cubic(0.20, 0.00, 0.00, 1.00))), weight: 72),
TweenSequenceItem(tween: ConstantTween<Size>(activeThumbSize), weight: 17),
```

反向序列（逐项镜像，权重顺序 17 / 72 / 11，曲线取 `.flipped`）：

```dart
TweenSequenceItem(tween: ConstantTween<Size>(inactiveThumbSize), weight: 17),
TweenSequenceItem(tween: Tween<Size>(begin: inactiveThumbSize, end: transitionalThumbSize)
    .chain(CurveTween(curve: const Cubic(0.20, 0.00, 0.00, 1.00).flipped)), weight: 72),
TweenSequenceItem(tween: Tween<Size>(begin: transitionalThumbSize, end: activeThumbSize)
    .chain(CurveTween(curve: const Cubic(0.31, 0.00, 0.56, 1.00).flipped)), weight: 11),
```

**尺寸的取值路径**（`switch.dart:1622-1648`）：

```dart
if (reaction.isCompleted) {
  thumbSize = Size.fromRadius(pressedThumbRadius);           // 28，按住满
} else {
  if (position.isDismissed || position.status == AnimationStatus.forward) {
    thumbSize = thumbSizeAnimation(true).value;              // 正向序列
  } else {
    thumbSize = thumbSizeAnimation(false).value;             // 反向序列
  }
}
// Material 2 的轻微收缩；M3 的 thumbOffset 为 null，因此 inset 恒为 0
final double inset = thumbOffset == null ? 0 : 1.0 - (currentValue - thumbOffset!).abs() * 2.0;
thumbSize = Size(thumbSize!.width - inset, thumbSize.height - inset);
```

### 1.4 滑块颜色：**四分支**（`switch.dart:1650-1672`）

```dart
final double colorValue = _colorAnimation!.value;
final Color trackColor = Color.lerp(inactiveTrackColor, activeTrackColor, colorValue)!;
final Color? trackOutlineColor = /* 同上按 colorValue 插值 */;
final double? trackOutlineWidth = lerpDouble(inactiveTrackOutlineWidth, activeTrackOutlineWidth, colorValue);

Color lerpedThumbColor;
if (!reaction.isDismissed) {
  lerpedThumbColor = Color.lerp(inactivePressedColor, activePressedColor, colorValue)!;
} else if (positionController.status == AnimationStatus.forward) {
  lerpedThumbColor = Color.lerp(inactivePressedColor, activeColor, colorValue)!;
} else if (positionController.status == AnimationStatus.reverse) {
  lerpedThumbColor = Color.lerp(inactiveColor, activePressedColor, colorValue)!;
} else {
  lerpedThumbColor = Color.lerp(inactiveColor, activeColor, colorValue)!;
}
final Color thumbColor = Color.alphaBlend(lerpedThumbColor, surfaceColor);
```

### 1.5 滑块绘制偏移（`switch.dart:1716-1731`）

```dart
final double trackRadius = trackHeight / 2;
final double additionalThumbRadius = thumbSize.height / 2 - trackRadius;

final double horizontalProgress = visualPosition * (trackInnerLength - _pressedThumbExtension!);
final double thumbHorizontalOffset =
    trackPaintOffset.dx + trackRadius + (_pressedThumbExtension! / 2)
    - thumbSize.width / 2 + horizontalProgress;
final double thumbVerticalOffset = trackPaintOffset.dy - additionalThumbRadius;
```

### 1.6 交互

**拖拽开始**（`switch.dart:864-868`）：

```dart
void _handleDragStart(DragStartDetails details) {
  if (isInteractive) {
    reactionController.forward();
  }
}
```

**拖拽更新**（`switch.dart:870-883`）——位置直接由位移累加，曲线改线性：

```dart
void _handleDragUpdate(DragUpdateDetails details) {
  if (isInteractive) {
    position
      ..curve = Curves.linear
      ..reverseCurve = null;
    final double delta = details.primaryDelta! / _trackInnerLength;
    positionController.value += switch (Directionality.of(context)) {
      TextDirection.rtl => -delta,
      TextDirection.ltr => delta,
    };
  }
}
```

**拖拽结束**（`switch.dart:887-901`）——**按位置值是否越过 0.5 与当前值不同来提交**：

```dart
void _handleDragEnd(DragEndDetails details) {
  if (position.value >= 0.5 != widget.value) {
    widget.onChanged?.call(!widget.value);
    setState(() { _needsPositionAnimation = true; });
  } else {
    animateToValue();
  }
  reactionController.reverse();
}
```

**点击**：`onChanged` 由 `ToggleableStateMixin` 的 `_handleTap` 触发，本地参考缺失该文件；
`Switch` 侧只暴露 `ValueChanged<bool?>? get onChanged => ... _handleChanged`（`switch.dart:787`），
`_handleChanged` 直接 `widget.onChanged?.call(value!)`。

**位置曲线**（`switch.dart:797-805`）：

```dart
void updateCurve() {
  if (Theme.of(context).useMaterial3) {
    position
      ..curve = Curves.easeOutBack
      ..reverseCurve = Curves.easeOutBack;
  } else { ... }
}
```

`didUpdateWidget` 里只在 `position.value == 0.0 || position.value == 1.0` 时重置曲线
（`switch.dart:740-763`），避免拖拽中被打断。

### 1.7 光标（`switch.dart:2142-2144`）

```dart
@override
WidgetStateProperty<MouseCursor> get mouseCursor => WidgetStateProperty.resolveWith(
  (Set<WidgetState> states) => WidgetStateMouseCursor.clickable.resolve(states),
);
```

Material 2 的默认明确是 `SystemMouseCursors.basic`（箭头），仅 Web 上是手型（`switch.dart:1935-1942`）。

`WidgetStateMouseCursor.clickable` 的实际取值 **[未证实]**：它定义在
`packages/flutter/lib/src/widgets/mouse_cursor.dart`，而本地 sparse checkout **未包含该文件**。
按 Flutter 的通行约定它应对可用控件给出 `SystemMouseCursors.click`（手型）、禁用给出 `basic`，
但这条是**推论而非本仓库可核对的事实**。若要把光标写死，需先补该文件再确认。

---

## 2. Fluent（WinUI 3）

### 2.1 几何

**轨道**（`toggle_switch.dart:236-241`）：`height: 20`、`width: 40`，圆角 `BorderRadius.circular(100)`。

**滑块**（`toggle_switch.dart:308-325`）：

```dart
margin: states.isHovered
    ? const EdgeInsetsDirectional.all(1.0 + checkedFactor)              // = all(2)
    : const EdgeInsetsDirectional.symmetric(
        horizontal: 2.0 + checkedFactor,                                 // = 3
        vertical: 2.0 + checkedFactor,                                   // = 3
      ),
height: 18,
width: 12.0 + (states.isHovered ? 2.0 : 0.0) + (states.isPressed ? 5.0 : 0),
```

`const checkedFactor = 1;`（`toggle_switch.dart:307`）。

### 2.2 配色 — `ToggleSwitchThemeData.standard`（`toggle_switch.dart:450-503`）

**选中轨道**：`ButtonThemeData.checkedInputColor(theme, states)` → `FilledButton.backgroundColor`
（`buttons/theme.dart:375-380`、`buttons/filled_button.dart:100-110`）：

```dart
if (states.isDisabled)      return theme.resources.accentFillColorDisabled;
else if (states.isPressed)  return theme.accentColor.tertiaryBrushFor(theme.brightness);
else if (states.isHovered)  return theme.accentColor.secondaryBrushFor(theme.brightness);
else                        return theme.accentColor.defaultBrushFor(theme.brightness);
```

边框色与填充同色。

**未选中轨道填充**（`toggle_switch.dart:470-478`）：

```dart
color: WidgetStateExtension.forStates<Color>(
  states,
  disabled: theme.resources.controlAltFillColorDisabled,
  pressed:  theme.resources.controlAltFillColorQuarternary,
  hovering: theme.resources.controlAltFillColorTertiary,
  none:     theme.resources.controlAltFillColorSecondary,
),
```

**未选中轨道边框**（`toggle_switch.dart:479-486`）：

```dart
disabled: theme.resources.controlStrongFillColorDisabled,
none:     theme.resources.controlStrongFillColorDefault,
```

**选中滑块**：`disabled ? textOnAccentFillColorDisabled : textOnAccentFillColorPrimary`
**未选中滑块**：`disabled ? textFillColorDisabled : textFillColorSecondary`
**内容前景**：`disabled ? textFillColorDisabled : null`（`toggle_switch.dart:457-461`）

### 2.3 动画

```dart
animationDuration: theme.fasterAnimationDuration,   // 83 ms
animationCurve:    Curves.fastOutSlowIn,
```

轨道与滑块各是一个 `AnimatedContainer`，都吃这两个值（`toggle_switch.dart:236-241`、`310-314`）。
**位置动画**由 `AnimatedContainer.alignment` 在 `centerStart` / `centerEnd` 之间过渡实现，
不是单独的位置控制器。

### 2.4 交互（`toggle_switch.dart:174-235`）

```dart
final sliderGestureWidth = 45.0 + (style.padding?.horizontal ?? 0.0);

HoverButton(
  onPressed: isDisabled ? null : () => widget.onChanged!(!widget.checked),
  onHorizontalDragStart: isDisabled ? null : (e) {
    _handleAlignmentChanged(e.localPosition, sliderGestureWidth);
    setState(() => _dragging = true);
  },
  onHorizontalDragUpdate: isDisabled ? null : (e) {
    _handleAlignmentChanged(e.localPosition, sliderGestureWidth);
    if (!_dragging) setState(() => _dragging = true);
  },
  onHorizontalDragEnd: isDisabled ? null : (e) {
    if (_alignment != null) {
      if (_alignment!.x >= 0.5) {
        widget.onChanged!(true);
      } else {
        widget.onChanged!(false);
      }
      setState(() { _alignment = null; _dragging = false; });
    }
  },
  ...
)
```

对齐值（`toggle_switch.dart:182-191`）：

```dart
void _handleAlignmentChanged(Offset localPosition, double sliderGestureWidth) {
  setState(() {
    _alignment = Alignment(clampDouble(localPosition.dx / sliderGestureWidth, -1, 1), 0);
  });
}
```

**要点**：`_alignment` 范围是 **-1..1**，提交阈值是 `>= 0.5`，即**手势宽度的 1/4 处**，
不是轨道中点。拖拽中滑块由 `alignment` 直接定位（不插值）。

拖拽期间状态被强制为 `pressed`（`toggle_switch.dart:246-250`）：

```dart
DefaultToggleSwitchKnob(
  checked: widget.checked,
  style: style,
  states: _dragging ? {WidgetState.pressed} : states,
)
```

### 2.5 光标

`HoverButton` 的默认是 `MouseCursor.defer`（`utils/hover_button.dart:394`），
`ToggleSwitch` 不传 `cursor`，因此**光标不变**（箭头）。

---

## 3. Glass（liquid_glass_widgets）

### 3.1 几何

```dart
final double width;    // 默认 58.0
final double height;   // 默认 26.0
final Color thumbColor = CupertinoColors.white;
static const _defaultThumbShadowColor = Color(0x33000000);
```

滑块行程（`glass_switch.dart:350-355`）：

```dart
double get _thumbTravelDistance {
  final thumbSize = widget.height - 4.0;
  final thumbWidth = thumbSize * 1.6;
  return widget.width - thumbWidth - 4.0;
}
```

### 3.2 动画（`glass_switch.dart:206-241`）

```dart
_positionController  = AnimationController(duration: const Duration(milliseconds: 380), vsync: this);
_thicknessController = AnimationController(duration: const Duration(milliseconds: 380), vsync: this);

_positionCurve = CurvedAnimation(
  parent: _positionController,
  curve: Curves.easeInOutCubic,
  reverseCurve: Curves.easeInOutCubic,
);

// Pulse animation (0 -> 1 -> 0)，与位置同步
_thicknessAnimation = TweenSequence<double>([
  TweenSequenceItem(
    tween: Tween<double>(begin: 0.0, end: 1.0).chain(CurveTween(curve: Curves.easeInOutCubic)),
    weight: 45,   // Grow up as it gains speed
  ),
  TweenSequenceItem(
    tween: Tween<double>(begin: 1.0, end: 0.0).chain(CurveTween(curve: Curves.easeInOutQuad)),
    weight: 55,   // Settle down as it lands
  ),
]).animate(_thicknessController);
```

外部改值时把脉冲时长按剩余行程缩短（`glass_switch.dart:276-297`）：

```dart
final double posDist = widget.value
    ? (1.0 - _positionController.value)
    : _positionController.value;
final int ms = (380 * posDist).round();
if (ms > 0) {
  unawaited(_thicknessController.animateTo(1.0,
      duration: Duration(milliseconds: ms),
      curve: Curves.easeIn));   // easeIn：按住时保持拉伸，落地才收回
} else {
  _thicknessController.value = 1.0;
}
```

### 3.3 交互

**点击**（`glass_switch.dart:340-342`、`373-381`）：

```dart
void _handleTap() { widget.onChanged(!widget.value); }

void _onTapUp(TapUpDetails details) {
  if (!_isDragging) {                       // 只有没发生横向拖拽才算点击
    if (widget.enableHaptics) unawaited(HapticFeedback.lightImpact());
    unawaited(_thicknessController.forward(from: _thicknessController.value));
    _handleTap();
  }
}
```

**按下即开始膨胀**（`glass_switch.dart:360-371`）：

```dart
void _onTapDown(TapDownDetails details) {
  if (_thicknessController.value >= 0.99) _thicknessController.value = 0.0;
  _dragAbandonedExternally = false;
  unawaited(_thicknessController.animateTo(0.45,
      duration: const Duration(milliseconds: 120), curve: Curves.easeOut));
}
```

**拖拽开始**（`glass_switch.dart:386-418`）：记录 `_dragStartX = details.localPosition.dx` 与
`_dragStartPosition = _positionController.value`，把 `_thicknessController` 推到 0.45（80 ms）。

**拖拽更新**（`glass_switch.dart:428-450`）——位置直接写入控制器：

```dart
final travel = _thumbTravelDistance;
final dragDelta = details.localPosition.dx - _dragStartX;
_positionController.value = (_dragStartPosition + dragDelta / travel).clamp(0.0, 1.0);
```

**拖拽结束**（`glass_switch.dart:452-470`）——**速度优先**：

```dart
final position = _positionController.value;
final velocity = details.primaryVelocity ?? 0.0;

// A fast flick (> 200 px/s) wins over position; otherwise snap at 50 %.
final bool shouldBeOn = velocity.abs() > 200.0 ? velocity > 0 : position >= 0.5;

if (shouldBeOn != widget.value) {
  widget.onChanged(shouldBeOn);
}
```

### 3.4 光标

文件内**没有 `MouseRegion`**，因此光标保持默认（箭头）。

---

## 4. 三者交互规则对照

| | 点击提交 | 拖拽位移映射 | 拖拽提交判据 | 光标 |
|---|---|---|---|---|
| **MD3** | `onChanged(!value)`（mixin 的 tap） | `position += delta / trackInnerLength`，曲线改线性 | `position.value >= 0.5 != value` | 手型；禁用时箭头 |
| **Fluent** | `onChanged(!checked)` | `alignment = clamp(dx / (45 + pad), -1, 1)` | `alignment.x >= 0.5`（即 **1/4** 手势宽度） | 不变（箭头） |
| **Glass** | 未发生拖拽时 `onChanged(!value)` | `position = start + (dx - startX) / travel` | 速度 > 200 px/s 优先，否则 `>= 0.5` | 不变（箭头） |

## 5. 本仓库当前实现与参考的已知差异

以下是按上表核对后确认不一致的地方，供逐条修正：

1. **`Sense`**：三个开关原先没有 `DRAG`，拖拽根本不会开始。已补（本轮改动）。
2. **点击不生效**：`WidgetState.clicked` 原先用等值比较，而点击落在最深层子节点（`track` / `thumb`）
   上，控件根节点收不到。已改为祖先链判断（本轮改动）。
3. **拖拽不跟随手指**：MD3 原先在拖拽时读 `ctx.value` 而不重定向，滑块不动。
   Fluent/Glass 同样。已改为「拖拽时目标 = 手指位置 + `IMMEDIATE` 规格」（本轮改动）。
4. **拖拽提交判据是猜的**：我当前用 `x > bounds.width * 0.5`，与三者都不同：
   - MD3 应比 **position 值**（0..1）与 0.5；
   - Fluent 应比 **alignment.x**（-1..1）与 0.5，即 1/4 手势宽度；
   - Glass 应**先看速度**（>200 px/s）再看 0.5。
5. **光标**：我给三个都加了手型，但只有 MD3 是手型；Fluent 与 Glass 应保持箭头。
   且 MD3 在禁用态应为箭头。
6. **MD3 滑块颜色**：参考有**四分支**（`reaction` 未结束 / 正向 / 反向 / 静止），
   当前实现只有 `hovered || pressed` 两分支。
7. **MD3 位置曲线**：参考在 `didUpdateWidget` 里用 `Curves.easeOutBack`，拖拽中改 `Curves.linear`；
   当前实现固定用 `POSITION` 规格。
8. **Fluent 轨道尺寸**：参考 40×20、滑块 18 高；当前实现的 `TRACK` 是 40×20（一致），
   但滑块的 margin/width 公式需按 §2.1 逐项核对。
9. **Glass 脉冲**：参考是 380 ms 的 0→1→0 序列（权重 45/55，`easeInOutCubic` / `easeInOutQuad`），
   且外部改值时按时长 `380 * 剩余行程` 缩短；当前实现用单一 `THICKNESS` 规格。
