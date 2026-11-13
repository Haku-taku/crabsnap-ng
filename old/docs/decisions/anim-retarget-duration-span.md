# ADR: 重定向时长按值域缩放，而不是按上一段距离

状态：**已采纳并实现**（2026-10-29，switch 重写时发现）。代码在
`ui/crates/ui-core/src/anim.rs` 的 `AnimSpec::span` 与 `AnimStore::animate`。

## 旧做法与问题

`animate` 计算重定向时长时，用**上一段的距离**当单位：

```rust
let total = (entry.to[0] - entry.from[0]).abs();
let fraction = (remaining / total).clamp(0.05, 1.0);
```

对逐帧拖拽跟随这是对的：每帧的 `total` 就是上一帧的微小位移，`fraction ≈ 1`，
`DRAG_FOLLOW` 因此保持恒定的 24ms 滞后。但对**状态过渡**是错的：拖动结束时上一段是
一次零时长写入，`total` 很小，`fraction` 被夹到 1，于是松手后从 0.6 走到 1.0 仍走满
300ms；Flutter 的 `AnimationController.forward()` 按 `remaining / (upper - lower)` 缩放，
只走 120ms。中途反向也有同样的偏差。

## 决定

`AnimSpec` 增加 `span: Option<f32>`（构造器 `.span(1.0)`）。有 `span` 时
`fraction = remaining / span`；没有时保留旧的「上一段距离」单位，拖拽跟随的语义不变。
MD3 开关的 `PROGRESS` / `POSITION` / `REACTION` / `REACTION_FADE` 都声明 `span(1.0)`。

## 回归检查

- `a_declared_span_scales_a_partial_retarget_by_the_value_range`：去掉 `span` 分支后，
  0.4→1.0 的重定向在 0.3s 处不再是 0.7，测试变红。
- `a_drag_release_keeps_the_thumb_continuous_until_the_app_flips`：松手帧与下一帧的滑块
  尺寸差超过 1px 即红。
