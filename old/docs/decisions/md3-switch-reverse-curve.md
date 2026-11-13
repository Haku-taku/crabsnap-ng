# ADR: MD3 开关反向腿的曲线是 `easeOutBack`，不是翻转后的曲线

状态：**已采纳并实现**（2026-10-29，switch 重写）。代码在
`ui/crates/ui-widgets-md3/src/toggle.rs` 的 `POSITION`。

## 旧做法与问题

旧实现把 Flutter 的 `reverseCurve = Curves.easeOutBack.flipped` 直接当成引擎的
「下行段曲线」：`down(TOGGLE_MS, Easing::Cubic(0.68, -0.275, 0.825, 0.115))`。
两者的自变量不同：

- Flutter 的 `reverseCurve` 作用在**从 1 降到 0 的控制器值**上，所以要用
  `flipped(t) = 1 - f(1 - t)` 才能得到时间反演的形状；
- 本引擎的 `AnimSpec.down.easing` 作用在**从 0 升到 1 的段进度**上，值由
  `from + (to - from) * easing(y)` 给出。要让位置等于 `1 - easeOutBack(y)`，
  `easing` 必须是 `easeOutBack` 本身。

按 `flipped` 写，反向腿在开头会**越过开启端**：段进度 3% 处位置约 1.03，34dp 胶囊
中心到 36.4，右缘 53.4，超出 52dp 轨道 **1.66px**（旧测试测到 1.81px，被误判为
「参考本身的过冲」）。

## 决定

`POSITION.down` 与正向用同一条 `POSITION_CURVE`（`easeOutBack`）。反向的「慢离开远端、
快到达近端」由这条曲线在**段进度**上的形状自然给出，不需要再翻转。

## 回归检查

`the_thumb_stays_inside_the_track_through_both_legs`：把 `down` 换回翻转曲线，t=0.03
立即报 1.66px 越界；换回 `easeOutBack` 后最坏越界 < 0.01px。
