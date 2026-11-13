# ADR: 玻璃的磨砂用真模糊，不用粗捕获；边缘高光收成方向性波瓣

状态：**已采纳并实现**（2026-11-03）。代码在 `ui/crates/ui-core/src/draw.rs` 的
`LiquidGlass::frost`、`ui/crates/ui-widgets-glass/src/material.rs` 与 `shared.rs::material()`、
`ui/crates/ui-paint-gpu/src/shaders/liquid_glass.frag.glsl`。

## 旧做法与问题

1. **磨砂靠 0.2 密度的捕获**：把节点面积的五分之一采进纹理，再用双线性放大回节点大小。
   放大倍数一大，背景就成了块状锯齿（用户报「nav bar 的主体部分、玻璃的卡片…仍然有明显的
   锯齿感」）。
2. **边缘高光是一圈白描边**：`angular = 0.5 + sin·0.5` 从不为零，再经 `pow(·, 0.3)` 压平、
   乘 `RIM_GAIN = 8` 后整圈饱和到 1，最后 `mix(白, 0.85)` —— 每块玻璃都有一圈白边
   （用户报「所有玻璃的外圈都是一圈白色」）。它不是画上去的描边：控件画的发丝线是黑色
   25%（`shared.rs::rim_border`），`glassRim` 角色也没有任何调用者。

## 改用什么

- `LiquidGlass.frost`（dp）：捕获之前对背景做**真正的**箱式模糊，捕获密度全部回到 1.0。
  `regular` 取 10dp，透镜与拇指为 0（透镜下的内容保持清晰）。旋钮 `Knob::FrostBlur`
  可在 materials 页实时调，`GALLERY_CAPTURE_SCALE` 仍可覆盖捕获密度做对比。
- 眩光改成方向性波瓣：`max(sin(angle), 0)` 按收敛度取幂，再用 `x/(1+x)` 软钳制。
  背光的一侧为 0，整圈白边消失，只剩朝向光源的一段高光。

## 哪条检查抓回归

- 手工：`GALLERY_PAGE=nav-bar` 与 `cards` 放大截图 —— 胶囊与卡片边缘没有白圈、背景平滑
  无块。`Knob::FrostBlur` 拉到 0 是清晰玻璃、拉大磨砂加深，磨砂不再是捕获密度的副产品。
- 效果趟的边界收在「节点矩形 + 3×核半径 + 捕获外扩」，不再整帧重算（`renderer.rs` 的
  `apply_backdrop`）：nav-bar 与 cards 页前后截图**逐像素相同**，materials 页 69 → 100 fps
  （2560×1440，嵌套会话，RTX 5060 Ti）。
