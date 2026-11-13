# ADR: 捕获按位置寻址，多出的面积是余量而不是缩放

状态：**已采纳并实现**（2026-11-04）。代码在 `ui/crates/ui-paint-gpu/src/renderer.rs` 的
`lens_capture`、`ui/crates/ui-paint-gpu/src/shaders/liquid_glass.frag.glsl` 的 `capture_uv`、
`ui/crates/ui-core/src/draw.rs` 的 `LiquidGlass.magnification`。

## 旧做法与问题

`capture_uv(device) = (device − 节点原点) / 节点尺寸`，而纹理里装的是节点 × `capture_size`
的区域 —— 等于把**节点铺满整张纹理**。于是 `capture_size: 1.1` 不再是余量，而是把玻璃下的
内容缩小约 9%：玻璃最外圈读到的是**节点之外**的内容。

后果落在标签栏上：强调色副本裁在透镜矩形内，玻璃边缘的那一圈便从外面取到中性副本，
在强调色与中性墨色之间混出一条白蓝带（用户报「玻璃块的边缘看到了白色和蓝色的融合带」）。
把副本外扩到「玻璃读到的范围」又会让强调色露在玻璃之外 —— 副本是透镜的**输入**，
不该在透镜之外可见。

## 参考怎么做

参考的捕获按**位置**寻址，不把节点铺满纹理：`screenUV = (fragCoord + uCaptureOffset) *
invTexSize`（`liquid_glass_render.frag:323`），捕获图像比玻璃大，多出的部分只是边缘可读的
余量。标签栏里那点放大是**显式的内容缩放**（`tab_bar_bottom_layout.dart:491` 的
`lerpDouble(1.0, magnification, intensity)`），不是透镜的光学缩放。

## 改用什么

- `u_capture` 传**捕获区域**（节点 × `capture_size`，居中），纹理尺寸 = 区域 × `capture_scale`：
  一个纹素对一个设备像素，节点与其内容 1:1 对应。
- 新增 `LiquidGlass.magnification`（`1` = 1:1，`< 1` 显示更宽的范围、内容缩小），经
  `u_optical.w` 送入着色器，绕节点中心缩放。透镜取 `1.0`（不再缩放，边缘只做棱镜弯折），
  开关/滑块保留原有的 0.75 / 1.0 观感。
- 透镜的 `capture_size` 回到 `1.0`：1:1 显示时边缘不需要读节点之外。
- 强调色副本的裁切回到透镜矩形本身（撤掉上一版的外扩），它只作为透镜的输入。

## 哪条检查抓回归

- **没有自动化检查**。手工判据：`GALLERY_PAGE=nav-bar` 配 `GALLERY_SWAY`，透镜扫过标签时
  玻璃外只有中性墨色、玻璃内是强调色，边界处没有白蓝混合带，也没有强调色溢出到玻璃外；
  16× 放大截图看得到「中性字形 → 玻璃边缘 → 强调色字形」的硬边界。
- 开关/滑块观感不变（`GALLERY_PAGE=switch`、`slider`），分段控件的透镜同样取 `1.0`。
