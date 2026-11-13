# ADR: 玻璃拆成中性本体与语义染色，内容层退回系统填充

状态：**已采纳并实现**（2026-11-03）。代码在 `ui/crates/ui-core/src/draw.rs` 的
`GlassBody`/`LiquidGlass`、`ui/crates/ui-paint-gpu/src/shaders/liquid_glass.frag.glsl`
的 `body_transfer`/`shade_sample`、`ui/crates/ui-widgets-glass/src/material.rs` 与 `pack.rs`。

## 旧做法与问题

1. **一个 `tint` 同时扮演本体与语义色。** 参考实现用 `#E6F2FF`、α0.80 的淡蓝洗色去
   近似 Apple regular 玻璃的亮度抬升，我们的着色器再 ×0.8 混合——等于每块玻璃都被
   64% 的淡蓝盖住。品牌色与"玻璃本身看起来什么样"因此无法分开调，所有控件一起偏蓝。
2. **玻璃铺到了内容层。** 列表行、文本框、芯片也走 `glass_recipe(Control::Container)`。
   HIG 把 Liquid Glass 限定在功能层：内容层用系统填充色，玻璃只给标签栏、分段、
   开关、滑块、按钮、菜单、弹层、对话框。铺错层的结果是行表面等于背景、文字压在
   模糊上。

## 改用什么

- `LiquidGlass.body: GlassBody { mix, pole, chroma, tone }` 承担**中性**本体变换：
  先按 `tone` 走深色凹曲线，再按 `mix` 把亮度推向 `pole`，最后围绕背景自身亮度缩放
  色度。浅色取 Apple 实测的 `mix 0.53 / pole 0.99 / chroma 0.937`，深色取
  `pole 0.1125 / chroma 1.043 / tone 1`；拇指只抬 0.10，透镜保持 `CLEAR`。
- `LiquidGlass.tint` 退回语义色，默认透明；只有 prominent 按钮带品牌色。
- 内容层三个控件改用 Apple 系统填充：分组单元格白 / `#1C1C1E`（`glassRowFill`）、
  字段 `tertiarySystemFill`（`glassFieldFill`）、芯片 `secondarySystemFill`
  （`glassChipFill`）。三个值都从 `ui-design/data/hig_data.rs` 的 HIG 表取。

## 哪条检查抓回归

- 截图取样：独立行浅色 `srgb(255,255,255)`、深色 `srgb(28,28,30)`；把
  `glassRowFill` 换回玻璃时行色会等于卡片色。
- `pack.rs` 的角色测试断言 `glassTrackInactive` 等随外观切换；新角色与它们同批。
- 材质页（`GALLERY_PAGE=materials`）的 body mix 滑杆把 `mix` 从 0 拉到 0.8，本体
  与染色可以分别观察。
