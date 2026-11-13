# 玻璃透镜实现备份（2026-11-02）

这个目录保存了切换到 LiquidGlassKit 模型之前的玻璃实现，供对比与回退。

## 文件

这五个文件是切换前的最后状态（含本轮前半段对 iOS 27 近轴透镜的尝试）：

| 文件 | 原路径 |
|---|---|
| `shader.rs` | `ui/crates/ui-paint-gpu/src/shader.rs` |
| `renderer.rs` | `ui/crates/ui-paint-gpu/src/renderer.rs` |
| `material.rs` | `ui/crates/ui-widgets-glass/src/material.rs` |
| `shared.rs` | `ui/crates/ui-widgets-glass/src/shared.rs` |
| `draw.rs` | `ui/crates/ui-core/src/draw.rs` |

## 旧透镜的定义（回退时改这三处）

1. `shader.rs` 的 `PREMIUM_GLASS_FRAGMENT`，近轴分支的位移：

   ```glsl
   disp = n2 * sin_i * sin_i * (thickness * 8.0) * max(u_optical.x - 1.0, 0.0) * u_optical.w;
   ```

   方向朝外，量值随 `sin²` 走抛物线，采样距离夹在 `0.5 * min(size.x, size.y)` 内。

2. `material.rs` 的 `glass_recipe`，Switch / Slider 的四个元组：

   ```rust
   (Control::Switch, false) => (Color::rgba(0.88, 0.88, 0.90, 0.12), 1.24, 32.0, 2.0),
   (Control::Switch, true)  => (Color::rgba(1.0, 1.0, 1.0, 0.08), 1.24, 32.0, 2.0),
   (Control::Slider, false) => (Color::rgba(0.88, 0.88, 0.90, 0.12), 1.24, 32.0, 1.8),
   (Control::Slider, true)  => (Color::rgba(1.0, 1.0, 1.0, 0.08), 1.24, 32.0, 1.8),
   ```

3. 删掉本轮新增的 `Glass::magnification`、`u_lens` uniform 与 `GlassRecipe::magnification`。

旧实现的观感：面下画面整体缩小（约 0.73×），边缘是一条干净的压缩带，没有折叠与涂抹。
