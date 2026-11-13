# Fluent 2 色彩与令牌规范通读（2026-10-28）

## 0. 数据源比较与选择

用户提供的线索 + 我自己的补充，逐条实测后的结论：

| 来源 | 覆盖 | 权威性 | 许可 | 结论 |
|---|---|---|---|---|
| `fluentui` MCP（已挂载） | **只有 7 个示例令牌**（`colorNeutralForeground1` 等），宣称 45+ | 值正确，但覆盖太薄 | — | **不足以建体系**，仅作交叉验证 |
| **`@fluentui/tokens` 1.0.0-alpha.24（npm）** | **459 令牌 × 3 套主题**（web light / web dark / teams high-contrast）+ **65 条全局调色板** + 品牌色阶 + 全部刻度 | 官方仓库 `packages/tokens/src` 的**编译产物**，同源 | **MIT** | ✅ **选它作为主数据源** |
| `microsoft/fluentui` `packages/tokens/src/**/*.ts` | 与 npm 包同源（`global/brandColors.ts`、`alias/lightColor.ts` 均存在，raw 200） | 官方源码 | MIT | 只用来核对结构，不重复下载 |
| [fluent2.microsoft.design/design-tokens](https://fluent2.microsoft.design/design-tokens) | **两层令牌模型**的文字定义 | 官方文档 | 引用 | 作为模型依据 |
| [fluent2.microsoft.design/color](https://fluent2.microsoft.design/color) | 三条调色板分类：**neutral / shared / brand**（含 semantic 状态色） | 官方文档 | 引用 | 作为分类依据 |
| Fluent UI Apple / Android | 同一套令牌的 Swift/Kotlin 副本 | 官方 | MIT | 待查（路径未命中，需补） |
| WinUI 3 主题字典 | `SystemAccentColor` 等运行时资源 | 官方 | MIT | 后续用于 Win32 侧对照 |

**落地数据**：[`fluent-tokens.json`](fluent-tokens.json)（90KB）——品牌色阶、65 条全局调色板、
三套主题各 459 个令牌、以及全部刻度（排版/字号/行高/圆角/间距/描边/阴影/动效）。

## 1. 两层令牌模型（官方定义）

> Global tokens are context-agnostic and store raw values… Alias tokens are Fluent's second
> token layer and add semantic meaning to the stored values.

```
全局层（global）    原始值：调色板色阶、字族、圆角、间距、描边、动效
   └─ 别名层（alias）  语义：colorNeutralForeground1 / colorBrandBackground / shadow16 …
        └─ 复合层（theme）createLightTheme(brand) 把两层拍平成 459 个令牌
```

**与我们此前的 MD3 包的关键差异**：MD3 的语义值是**规则解析**出来的（tone + 对比曲线 +
ToneDeltaPair 现场计算）；Fluent 的语义值是**查表**（别名 → 全局，最多一层间接）。
统一令牌层必须同时容纳「规则」与「查表」两种形态。

## 2. 调色板分类（官方）

| 调色板 | 用途 |
|---|---|
| **neutral** | 黑/白/灰，承载表面、文本、布局与状态 |
| **shared** | 跨 M365 对齐（头像、日历、徽章），并承载 **semantic**（status）色：red/yellow/green |
| **brand** | 产品品牌色阶（Communication Blue、Teams、Outlook…） |

数据里的体现：`globalColors` 共 **65 条调色板**（`grey`、`grey10…grey90`、`whiteAlpha/blackAlpha` 系列、
`red/green/darkOrange/yellow/berry/lightGreen/marigold` 等 status、30 条 persona 色）。

色阶步长（两者**不同**，这是 Fluent 的特点）：

| 阶 | 步长 | 例 |
|---|---|---|
| neutral（grey） | **2** | `grey2, grey4, grey6 … grey100` |
| brand | **10** | `10, 20, 30 … 160`（16 级） |
| 单色（status/persona） | primary + shade10…50 + tint10…60 | 每个色相 12 个色 |

默认品牌阶 `brandWeb` 实测值：

```
10 #061724  20 #082338  30 #0a2e4a  40 #0c3b5e  50 #0e4775  60 #0f548c
70 #115ea3  80 #0f6cbd  90 #2886de 100 #479ef5 110 #62abf5 120 #77b7f7
130 #96c6fa 140 #b4d6fa 150 #cfe4fa 160 #ebf3fc
```

## 3. 别名层是品牌色阶的**纯函数**

`lib/alias/lightColor.js` 导出的是

```js
export const generateColorTokens = (brand) => ({ /* 216 个 color* 令牌 */ });
```

`createLightTheme(brandWeb)` 把别名层 + 调色板令牌 + 全部刻度拍平成 **459 个令牌**。也就是说：

- **换品牌 = 换 16 级色阶**，别名层与调色板层完全由它派生（无额外算法、无色彩科学）；
- 因此 Fluent 的「从单一颜色生成整套」**卡在色阶生成这一步**——而这一步在官方代码里是**输入**，
  不是算法（详见 §6）。

## 4. 459 个令牌的构成（webLight，逐类实测）

| 类别 | 数量 |
|---|---|
| `color*` 别名色 | **216** |
| `colorPalette*`（status + persona 色板令牌） | **150** |
| 排版复合样式（`typographyStyles` 的 17 个） | 17 |
| 间距 `spacing*` | 22 |
| 动效（8 时长 + 9 曲线） | 17 |
| 阴影 `shadow*` | 12 |
| 圆角 `borderRadius*` | 11 |
| 描边 `strokeWidth*` | 4 |
| 行高原语 `lineHeightBase100…Hero1000` | 10 |
| **合计** | **459** |

明暗与高对比三套**令牌名完全相同**，只有值不同——与 MD3「一套角色名、两套值」同构。

### 4.1 组合规则（`createLightTheme` 逐行）

```js
createLightTheme(brand) = { ...圆角, ...字号, ...行高, ...字族, ...字重, ...描边,
  ...水平间距, ...垂直间距, ...时长, ...曲线,
  ...generateColorTokens(brand),      // 216 个别名色（含状态变体）
  ...colorPaletteTokens, ...colorStatusTokens,   // 150
  ...createShadowTokens(neutralAmbient, neutralKey),
  ...createShadowTokens(brandAmbient, brandKey, 'Brand') }
```

即：**刻度是常量、颜色全部由品牌阶派生、阴影由 4 个颜色令牌派生**（`shadow2/4/8/16/28/64`
× ambient/key，品牌侧另有 `Brand` 变体）。

### 4.2 状态层：Fluent 用**具名令牌**，不是叠加色

216 个别名色里有 **103 个是状态变体**（主题内实测；别名层去重后 101 个不同名）（`colorXHover` / `colorXPressed` / `colorXSelected`，
例如 `colorBrandBackgroundHover`、`colorNeutralBackground1Pressed`）。

> 这是三套体系最根本的差异之一：**MD3 用叠加透明度（state layer）现场合成**；
> **Fluent 把状态色写进令牌表**；HIG 由系统按材质/外观自动解析。
> 统一令牌层必须能表达这三种形态，而不能只留一种。

## 5. 各刻度的实测值

| 刻度 | 值 |
|---|---|
| 圆角（11） | `0, 2px, 4px, 6px, 8px, 12px, 16px, 24px, 32px, 40px, 10000px(circular)` |
| 间距（22） | 水平/垂直各 11 档：`0, 2, 4, 6(SNudge), 8(S), 10(MNudge), 12(M), 16(L), 20(XL), 24(XXL), 32(XXXL)` |
| 描边（4） | `thin 1, thick 2, thicker 3, thickest 4` |
| 时长（8） | `50, 100, 150, 200, 250, 300, 400, 500 ms`（UltraFast…UltraSlow） |
| 曲线（9） | accelerate/decelerate × Max/Mid/Min + `easyEase` 等，均为 `cubic-bezier(...)` |
| 字族（4） | Base（Segoe UI Web + 系统回退）、Monospace、Numeric（Bahnschrift）、CJK |
| 排版（17 样式） | `caption2 … display`，引用原语（`--fontSizeBase300` 等）而非硬编码 |
| 阴影（12） | `shadow2/4/8/16/28/64` × ambient/key |

## 6. 缺口：品牌色 → 16 级色阶的**生成算法**

官方文档只说明品牌调色板的存在与用途，**没有给出从单一品牌色推导 16 级的算法**；代码里
`brandWeb` 是写死的常量，`createLightTheme(brand)` 只做派生。可用的三条路：

1. **Theme Designer 工具**（交互式，算法未公开）；
2. **WinUI 的 accent palette 生成**（Windows 从系统强调色生成 10 级明暗，源码在
   `microsoft-ui-xaml`，可作为「平台官方做法」的对照）；
3. **我们自定规则 + 用官方两套色阶验收**：已知 `brandWeb`（16 级）与 Teams 品牌阶，
   两条官方曲线足够校准（例如「保持色相、按 L\* 均匀分布、高亮端降彩度」这类规则可反推验证）。

**结论（写进计划的决策）**：Fluent 包的接口**接受一条品牌色阶**（与官方一致）；
我们另外提供一个色阶生成器，其输出必须能复现 `brandWeb` 与 Teams 色阶的合格近似，
并按官方文档的对比度要求自检。

## 7. 验收集

1. **交叉源一致**：MCP 的 `colorNeutralForeground1 = #242424` == npm 包 light 主题值 ✓（已验证）。
2. **同构**：三套主题令牌名集合完全相等（diff 为空）。
3. **别名可解析**：216 个别名色中，除 `rgba(...)` 阴影/叠加外，其余必须等于全局调色板中的某个值。
4. **语义配对**：`colorNeutralBackground1` 与 `colorNeutralForeground1` 的对比度 ≥ 4.5（light/dark 各自）。
5. **刻度单调**：圆角、间距、时长序列严格递增。

## 8. 待补

| 项 | 状态 |
|---|---|
| Fluent UI Apple / Android 的令牌副本（跨平台一致性） | 路径未命中，待用 `deepwiki` 或仓库搜索定位 |
| WinUI 3 的 accent palette 生成算法 | 未读 |
| Theme Designer 的色阶算法 | 未找到源码位置 |
| `createLightTheme` 的组合规则 | ✅ 已读通（§4.1） |
| Fluent 的状态层 | ✅ 已确认：103 个具名状态令牌（§4.2），非叠加色 |
