# HIG / SwiftUI 色彩与令牌规范通读（2026-10-28）

## 0. 数据源与可信度

| 来源 | 内容 | 状态 |
|---|---|---|
| `hig_mcp` v0.3.0（经 sosumi.ai 读 Apple HIG 原文） | System colors 全表（12 色 × 4 上下文）、iOS gray ramp（5 档 × 4 上下文）、语义角色名、iOS/macOS 排版刻度、命中区、材质、SF Symbols | 上游自报核对时间 2026-10-17 |
| HIG [Color](https://developer.apple.com/design/human-interface-guidelines/color)（本次直读，时间戳 2026-10-28） | 动态色语义、背景层级（system / grouped）、前景角色表、macOS 动态色全表、Liquid Glass 上色规则、色彩管理、对比度要求 | ✅ 与 MCP 表逐值一致 |
| HIG [Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode)（本次直读） | base / elevated 两套暗色背景、vibrancy、强调色被系统覆盖的行为 | ✅ |
| HIG [Motion](https://developer.apple.com/design/human-interface-guidelines/motion)（本次直读） | **只有定性准则**，无数值 | ⚠️ 见 §6 |

**落地数据**：[`hig-tokens.json`](hig-tokens.json)（13 个系统色 + 5 档灰 + 15 个语义角色名 + iOS/macOS 两套排版 + 命中区 + 材质 + 玻璃护栏，每个字段带来源标签）。

> Apple 的原文警告（HIG Color「System colors」一节）：**不要硬编码系统色值**，文档值仅供设计参考，
> 实际值随版本与环境变化。所以下面的 hex 是**参考表**，不是实现依据。

## 1. 色彩模型：两层，且第二层**没有公开数值**

```
系统色（固定）        12 个 vivid 色 + 5 档灰，每色 4 个上下文变体
   └─ 动态语义色      label / secondaryLabel / … / systemBackground / separator / fill …
                      按「用途」定义，由 OS 按 appearance × contrast × vibrancy 解析
                      ——Apple 不公布 hex，也不给公式
```

**这是与 MD3 / Fluent 的根本差别**：

| | 语义值怎么来 |
|---|---|
| MD3 | 规则解析（tone + 对比曲线 + ToneDeltaPair），**可离线复算** |
| Fluent | 查表（别名 → 全局色阶），**表已公开** |
| HIG | **由 OS 运行时解析**，公开表只有固定系统色 |

所以我们的 HIG 风格包必须自己决定语义色取值，并按 HIG 的对比要求自证（§7）。

## 2. 四个上下文维度（HIG 的"主题"不是一个开关）

| 维度 | 取值 |
|---|---|
| appearance | light / dark（visionOS 用 dark 值；watchOS/tvOS 另有约定） |
| contrast | default / **increased contrast**（每色都给独立值，不是简单调对比） |
| 背景层级 | **system** 与 **grouped** 两套，各 primary/secondary/tertiary |
| 暗色深度 | **base** 与 **elevated** 两套背景，随界面是否在前景（sheet/popover/多任务）**自动切换** |

`increased contrast` 的例子（HIG 表）：Blue `#0088FF → #1E6EF4`（light）、Red `#FF383C → #E9152D`；
dark 侧则普遍**提亮降饱和**（Blue `#0091FF → #5CB8FF`）。不是线性变换——**只能查表**。

## 3. 固定系统色（参考值，来自 HIG Specifications）

| 色 | light | dark | IC light | IC dark |
|---|---|---|---|---|
| red | `#FF383C` | `#FF4245` | `#E9152D` | `#FF6165` |
| orange | `#FF8D28` | `#FF9230` | `#C55300` | `#FFA056` |
| yellow | `#FFCC00` | `#FFD600` | `#A16A00` | `#FEDF43` |
| green | `#34C759` | `#30D158` | `#008932` | `#4AD968` |
| mint | `#00C8B3` | `#00DAC3` | `#008575` | `#54DFCB` |
| teal | `#00C3D0` | `#00D2E0` | `#008198` | `#3BDDEC` |
| cyan | `#00C0E8` | `#3CD3FE` | `#007EAE` | `#6DD9FF` |
| blue | `#0088FF` | `#0091FF` | `#1E6EF4` | `#5CB8FF` |
| indigo | `#6155F5` | `#6D7CFF` | `#564ADE` | `#A7AAFF` |
| purple | `#CB30E0` | `#DB34F2` | `#B02FC2` | `#EA8DFF` |
| pink | `#FF2D55` | `#FF375F` | `#E7124D` | `#FF8AC4` |
| brown | `#AC7F5E` | `#B78A66` | `#956D51` | `#DBA679` |
| gray | `#8E8E93` | `#8E8E93` | `#6C6C70` | `#AEAEB2` |

灰阶（iOS/iPadOS）：`gray2 #AEAEB2/#636366`、`gray3 #C7C7CC/#48484A`、`gray4 #D1D1D6/#3A3A3C`、
`gray5 #E5E5EA/#2C2C2E`、`gray6 #F2F2F7/#1C1C1E`（+ 各自的 increased contrast 值）。

**语义角色名**（15，无公开值）：`label, secondaryLabel, tertiaryLabel, quaternaryLabel,
systemBackground, secondarySystemBackground, tertiarySystemBackground, systemGroupedBackground,
separator, opaqueSeparator, systemFill, secondarySystemFill, tertiarySystemFill,
quaternarySystemFill, tintColor`；HIG 前景表另给 `placeholderText`、`link`。

## 4. 排版：是**样式**不是字号表

- iOS 11 档（默认 Large 设置下的值）：`largeTitle 34/41`、`title1 28/34`、`title2 22/28`、
  `title3 20/25`、`headline 17/22 semibold`、`body 17/22`、`callout 16/21`、`subheadline 15/20`、
  `footnote 13/18`、`caption1 12/16`、`caption2 11/13`，并给出每档的 emphasized 字重。
- macOS 另一套（更小、固定）：`largeTitle 26/32 … caption2 10/13 medium`。
- **必须按样式驱动**（`Font.body` 等），因为 Dynamic Type 会缩放；写死字号违反 HIG。
- 光学尺寸规则：**≥20pt 用 SF Pro Display，<20pt 用 SF Pro Text**；代码 SF Mono，衬线 New York。

## 5. 布局与材质

| 项 | 值 | 来源 |
|---|---|---|
| 命中区 | iOS 默认 44×44、**硬下限 28×28**；macOS 28/20；tvOS 66/56；visionOS 60/28；watchOS 44/28 | apple-hig |
| 控件间距 | 有 bezel 约 12pt、无 bezel 约 24pt | apple-hig |
| 布局边距 | compact 16 / regular 20（优先用系统 Layout Margins / safe area） | convention |
| 间距刻度 | 8pt 栅格，4/8/12/16/20/24/32/40/48 | **convention，非 Apple 数值** |
| 圆角 | **continuous（squircle）**；同心公式 `outer = inner + padding`；常见控件 8/12/16 | guidance |
| 材质（内容层） | `.ultraThinMaterial … .ultraThickMaterial`、`.bar`（6 档），厚=更不透明=更利于文字 | apple-hig |
| Liquid Glass（功能层） | `regular` / `clear` / `identity`；**clear 在亮内容后需约 35% 暗化层**；大元素更不透明 | apple-hig |
| 玻璃数值预算 | Apple **不公布**；护栏来自社区/Figma：模糊 ≤40(iPhone)/60(iPad·Mac)px、合成层 ≤4、depth ≤20、frost 10–25、**模糊后**文字对比 ≥4.5:1 | 标注 `community-bestpractice` / `figma-effect` / `wcag-aa` |
| 降级 | Reduce Transparency / Increase Contrast 下必须提供**近实心**变体 | apple-hig |

## 6. 动效：HIG **没有数值**

Motion 页只有定性准则：动效要有目的、可关闭、可取消、避免高频交互加动效、反馈要短促；
游戏 30–60fps；visionOS 避免视野边缘运动与 ~0.2Hz 振荡；watchOS 布局/外观动画的内置缓动**不可关闭**。

→ 我们 HIG 风格包的时长/曲线必须是**明示的自建约定**（或依 SwiftUI 默认行为实测得出），
不能伪装成 Apple 规范。这是三套里唯一"规范本身缺数"的维度。

## 7. 验收集

1. **对比度**：HIG 明确要求前后景对比 **≥4.5:1**，自定义配色建议 **7:1**（小字）。
   我们派生的每对语义色（`label` vs `systemBackground` 等）在 **4 个上下文**都要过。
2. **increased contrast 必须更分化**：同色 IC 值与默认值的差必须朝"更易区分"方向（HIG 举例）。
3. **灰阶单调**：light 侧递增、dark 侧递减（同一亮度轴上不交叉）。
4. **命中区**：任何控件的可命中尺寸 ≥ 该平台 minimum（iOS 28pt），默认为 44pt。
5. **同心圆角**：嵌套形状满足 `outer = inner + padding`，且使用 continuous。
6. **排版可缩放**：所有尺寸从样式取，不出现硬编码 pt（除样式的默认值定义处）。

## 8. 缺口（明示，不猜）

| 项 | 状态 |
|---|---|
| 语义角色（label/background/fill/separator…）的**具体值与公式** | Apple 不公开；需我们自定并按 §7 验收 |
| Dynamic Type 各档（xSmall…AxXLarge）的倍率表 | 未取到，需要另查（影响排版表完整性） |
| vibrancy 的实际合成方式 | 未取到（材质与色彩交互） |
| 动效数值 | HIG 不提供 |
| P3 / 广色域下的取值 | 仅有指引，无数值 |
| macOS「desktop tinting」对窗口底色的影响 | 定性描述，无公式 |
