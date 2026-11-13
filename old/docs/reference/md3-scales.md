# MD3 尺寸刻度（shape / spacing / motion）核对（2026-10-28）

来源：`md3-docs` MCP 直读 `m3.material.io`（页面更新时间标注 2026-07-17 / 08-18）。

## 1. 圆角刻度（✅ 官方数值，10 档）

`styles/shape/corner-radius-scale`：「The Material 3 shape system uses a size-based scale with
**ten styles**」：

| 档 | 值 |
|---|---|
| None | 0dp |
| Extra small | 4dp |
| Small | 8dp |
| Medium | 12dp |
| Large | 16dp |
| Large increased | 20dp |
| Extra large | 28dp |
| Extra large increased | 32dp |
| Extra extra large | 48dp |
| Full | 全圆 |

要点（都影响我们的令牌建模）：

- **按「圆度」命名而非组件尺寸**（M2 是按容器尺寸三级，M3 改成了十级）；
- **对称与非对称共用同一条十级刻度**；「inner corner」类组件令牌逐角映射；
- **形状风格族可切换：`rounded` ↔ `cut`**——`cut` 让角变成**直线**而不是曲线。
  我们现有的 `Corners` 只有 squircle/round 家族，**缺 `cut`**；
- **光学圆度公式：`outer − padding = inner`**（HIG 的 `outer = inner + padding` 是同一条，
  两套体系在此收敛 ✓）；嵌套同值被明确判为错误；
- 组件默认映射可重映射（例：按钮默认 `full`，可改 `small`/`medium`）；
- 另有 shape library（非矩形容器），文档建议只用于视觉型元素、避免用在文字多的容器上。

## 2. 间距（✅ 模型，具体数值未公开）

`styles/spacing/overview` + `styles/spacing/tokens`：

- **8dp 基准**，`space100 = 8dp`，其余为**倍率**（`space200 = 16dp` …）；
- 官方明确「**不只定义 8 的倍数**」：还会用到 2/4/6/10dp，Material **只定义常用嵌套单位**；
- 三类用途：`padding`（元素内）、`gap`（元素间）、`margin`（元素外）；
  位置维度：`vertical / top / bottom / horizontal / leading / trailing`（**leading/trailing 随 RTL 交换**）；
- 指引：**优先 padding 与 gap，少用 margin**；间距应随组件尺寸/布局/形态自适应；
- 实例（search 容器）：垂直 padding 8dp、水平 gap 8dp、水平 margin 24dp（聚焦时 12dp）。

> 结构化令牌表（`designSystems/20543ce18892f7d9`）**当前不可用**（与色彩 baseline 同一端点），
> 所以 `space0…space1200` 的逐项数值未取到 → **`[未证实]`**，实现时按「8dp × 倍率 + 少量嵌套单位」建模。

## 3. 动效（⚠️ 表格未公开，数值来自已读源码）

- 文档给出的是**集合与用法**：推荐 **Emphasized** 缓动集用于大多数转场；**Standard** 用于小而快的
  实用转场，并作为「不支持 Emphasized 的平台（iOS、Web）」的回退；
  另有 springs（阻尼/刚度的复合令牌）属于新的 motion physics 系统，文档明确说
  「easing/duration/path 属于 legacy 系统」。
- **具体数值表（"Tokens & specs" 与「Suggested easing and duration pairs」）在本页返回为空**
  （同一不可用的结构化端点）→ **`[未证实]`**。
- 我们目前已核实的数值来自 Flutter 的 `Curves`（已读）：`fastOutSlowIn = cubic(.4,0,.2,1)`、
  `easeOutBack = cubic(.175,.885,.32,1.275)`，以及 `toggleDuration = 300ms`、
  `kRadialReactionDuration = 100ms`、`_kReactionFadeDuration = 50ms`。
  **这些是「参考实现里的官方常数」，不是从 m3.material.io 表格取到的**，来源差异需记录。

## 4. 对统一层的直接影响

| 发现 | 影响 |
|---|---|
| MD3 十级圆角 + `rounded/cut` 两族 | 圆角令牌必须带**曲率族**（round / squircle / cut），不只是数值 |
| `outer − padding = inner`（MD3 与 HIG 同式） | 同心规则可以进**公共层**，不是某风格私有 |
| 间距是**倍率**而非枚举（且含 2/4/6/10） | 公共层用「基准 × 倍率」表达，枚举只是官方推荐子集 |
| Emphasized 是推荐集、Standard 有**平台回退**语义 | 动效令牌需要「集合」概念，不能只有单条曲线 |
| springs 与 easing/duration **并存**（新/旧两套） | 动效令牌要能表达弹簧，未来 MD3 会往这边走 |
