# Fluent 参考实现调研：控件、令牌与 Rust 侧 API 形状

> 调研日期：2026-10-31。范围：为 `ui-widgets-fluent` 补控件，回答“读谁、令牌从哪来、每个控件看哪一行、与 MD3 差在哪、按什么顺序做”。
>
> 证据约定（`AGENTS.md` §1.2）：外部结论给 `repo path:line` 或官方 URL；本仓库结论给 `path:line`；能从第一性原理推出的单独标注。做不到的标 `[未证实]` 并写风险。本文不修改任何代码。
>
> 外部源码的提交 SHA 已在文中固定，避免行号漂移：
>
> - `microsoft/microsoft-ui-xaml`：`868fef9ac230e79ab54a90a79d2048c939299c70`
> - `microsoft/WindowsAppSDK`：`89d93ef67cfb515b6faf6cc9adb803f935b4352a`
> - `microsoft/fluentui`：`1d2e3b10bdfc230051dbcc35d0c56d2ba78001cc`
> - `microsoft/microsoft-ui-reactor`：`74791562c11792f7c0e2dd875f1e82b94d70b397`
> - `microsoft/windows-rs`：`3bae60c742d232c101c1c699d04268c42ff2fb1c`
> - `microsoft/fluentui-blazor`：`f8af8e010001336012b3b681fa0f6b897c17340d`
> - `microsoft/WinUI-Gallery`：`7360ac322414e118eadfb0c7c2f4ecc31a218f59`
> - `dotnet/wpf`：`dbd3026982f09a279e4595666c07bb6136f579c3`
> - `unoplatform/uno`：`3c1b666bcebff3967e938b5be14a36d6a77e0fba`
>
> 本仓库 `windows-reactor` 源码取自 crates.io 的 `windows-reactor-0.100.0` 包（`.cargo_vcs_info.json` 记录 `path_in_vcs = "crates/libs/reactor"`，`sha1 = a57cc321bfdbac6bb98c488ba49f399da8fcb98a`）。

---

## §0 结论（可执行）

1. **控件视觉规格以 WinUI 3 的 XAML 为第一权威，Fluent UI React v9 只做交叉验证。** WinUI 的 `*_themeresources.xaml` 是实际控件模板、状态和 `ThemeResource` 值的来源；React v9 是 Web 实现，尺寸、焦点环和圆角与 WinUI 不同（§2、§3、§4）。实现每个控件前先读对应的 WinUI 文件，再用 React v9 对照状态命名。

2. **先补一张 WinUI `ThemeResource` 表，再写控件；不要继续用 7 条 `NATIVE_ALIAS` 硬编码。** 当前 `FluentSystem` 只有 `@fluentui/tokens` 的 459 个 Web 主题令牌（`ui/crates/ui-design/data/fluent_data.rs:1-5`），而 WinUI 控件用的是 `ControlFillColor*`、`TextFillColor*`、`FocusStrokeColor*`、`SolidBackgroundFillColor*` 等另一套词汇（`Common_themeresources_any.xaml:5-87`）。这两套不能静默互相别名（§2.3）。

3. **把四个地基放进 P0，并在写第一个新控件之前完成：per-control metrics + density、焦点双环、WinUI 动效槽位、Fluent 图标集。** 当前 `MetricsSet` 只有 control/surface/overlay 三层（`ui/crates/ui-style/src/types.rs:66-95`），`MotionTable::fluent()` 的 358 ms 与 `EaseInOut` 都不是 WinUI 值（`ui/crates/ui-style/src/motion.rs:81-91`），`CommonColor::FocusRing` 只有单环（`ui/crates/ui-design/src/common.rs:78`），图标只有 Material Symbols（`ui/crates/ui-widgets/src/icon.rs:1-10`）。不补这四项，每个控件都会重复硬编码。

4. **“Windows Reactor”不是 WinUI 的 Rust 重写或“标准实现”，而是微软官方的实验性声明式 WinUI 3 封装。** C# 侧 `microsoft/microsoft-ui-reactor` 自述 “not a new UI platform”，每个控件都是真实 WinUI 控件（`README.md:17-19`）；Rust 侧 `windows-reactor` 0.100.0 自述 “declarative WinUI 3 library for Rust”，把新 `View` 与原生 UI 树做 reconcile（`readme.md:3-5`），源码直接调用 WinUI COM（`src/native/winui/bindings.rs:1-11`）。它的价值在 Rust API 形状与 `ThemeBrush`/`ResourceOverrides`/`ButtonStyle` 三种模式，不在视觉规格；状态层、焦点视觉、动效、密度都由原生 WinUI 模板负责，Reactor 没有实现（§1.1、§4.9）。

5. **按 P0 → P1 → P2 → P3 → P4 推进，每期用“gallery 页 + 状态矩阵 + nested-session 截图 + 零 warning 构建”验收。** P1 先做 Button/CheckBox/Radio/Progress，并顺手把已有 Switch/Slider 拉回 WinUI 规格；P2 做 TextField/ListView/TabView/Breadcrumb/InfoBar/Tooltip；P3 做 Dialog/Menu/ComboBox/NavigationView；P4 做变体与高级状态（§5）。

---

## §1 参考实现清单与查证

### 1.1 先回答“Windows Reactor”是什么

#### 1.1.1 结论

`windows-reactor` 是 **微软官方、Rust、实验性（preview）的声明式 WinUI 3 库**，位于 `microsoft/windows-rs` 仓库的 `crates/libs/reactor`，crates.io 包名 `windows-reactor`。它**不是** WinUI 3 的 Rust 实现，也**不是** WinUI 的“标准实现”；它和 C# 的 `Microsoft.UI.Reactor` 是同一实验的两种语言版本，底层都渲染真实 WinUI 控件。

“标准实现”这个说法不成立，证据如下：

- 包自述：“Windows Reactor is a declarative WinUI 3 library for Rust. A `Component` owns state, receives parent-owned input, handles typed messages, and returns a `View`. Reactor reconciles each new view with the native UI tree and applies the required WinUI changes.”（`windows-reactor-0.100.0/readme.md:3-5`）
- 仓库 HEAD 自述：“Build native WinUI 3 applications with Rust components, state, and typed messages. Reactor is a preview; public APIs may change.”（`microsoft/windows-rs` `crates/libs/reactor/readme.md:3-4`）
- C# 版自述：“Reactor is **not** a new UI platform. It is a new way to *describe* WinUI content. Every control you render is a real WinUI control.”（`microsoft/microsoft-ui-reactor` `README.md:17-19`）
- C# README 的 “Prefer Rust?” 一节明确说：Rust 侧是“the same experiment ... in Rust”，是“a separate library with its own model”，但“Different language, same native controls underneath.”（`README.md:257-264`）
- 源码里是 `windows_core::link!` 到 `user32.dll`、`ole32.dll` 等，以及 `Microsoft.UI.Xaml` 的 COM 接口（`windows-reactor-0.100.0/src/native/winui/bindings.rs:1-11`、`:14-33`）；它调用原生 WinUI，不重新实现控件。

#### 1.1.2 仓库、版本、许可证、活跃度

| 项 | 值 | 证据 |
|---|---|---|
| 仓库 | `microsoft/windows-rs`，子目录 `crates/libs/reactor` | crates.io `.cargo_vcs_info.json` 的 `path_in_vcs`；`https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor` |
| 包名/版本 | `windows-reactor` 0.100.0 | crates.io API `/api/v1/crates/windows-reactor/0.100.0` |
| 许可证 | `MIT OR Apache-2.0` | `Cargo.toml:34`；包内 `license-mit`、`license-apache-2.0` 两份文件 |
| 语言/工具链 | Rust 2024 edition，`rust-version = "1.95"` | `Cargo.toml:13-16` |
| 平台 | Windows-only；docs.rs 默认 target 为 `x86_64-pc-windows-msvc` | `Cargo.toml:37-39` |
| 首次发布 | 2026-04-27 | crates.io API 的 `created_at` |
| 最近发布 | 2026-09-03 | crates.io API 的 `updated_at` |
| 仓库活跃度 | `microsoft/windows-rs` 默认分支最近提交 2026-10-30 | GitHub `commits/master.atom` |
| 独立仓库 | **不存在** `microsoft/windows-reactor` | `git ls-remote https://github.com/microsoft/windows-reactor` 返回 “Repository not found” |
| C# 版状态 | Experimental · April 2026；Theming & styling 为 Draft | `microsoft/microsoft-ui-reactor` `README.md:12`、`:145` |
| C# 版许可证 | MIT | `LICENSE:1` |

补充：C# 版的 `ThemeRef` 与三层值模型（Unset / ThemeRef / local concrete）写在 `docs/specs/001-theming-design.md:14-39`、`:45-56`，但 C# 侧仍明确说 “Only 3 brush properties accept `ThemeRef`” 且 “No lightweight styling”（`docs/specs/015-styling-design.md:14-23`）。Rust 0.100.0 没有 `ThemeRef` 类型；它只有 8 个 `ThemeBrush` 变体和 `ResourceOverrides`（§4.9）。不要把 C# 设计文档当成 Rust 已实现的能力。

#### 1.1.3 源码怎么组织控件与样式

Rust crate 的关键结构：

- `Element`/`View`：控件是具体结构体，统一收进 `enum Element`（`src/generated.rs:5440-5480`）；`View` 再包一层 `ViewKind`，支持原生元素、组件、片段、Provider、内容、子节点、槽位、Tooltip、Flyout（`src/element.rs:434-464`）。这是 retained-mode 的声明式树，不是我们的 immediate-mode `Widget::decl`。
- 控件属性：每个控件的公开 builder 以**语义属性**为主。例如 `Button` 主要是 `is_enabled`、两个 content alignment、`resource_overrides`、`style`、`key_accelerators`、`content`（`src/generated.rs:490-537`，另有回调和内部引用字段）；`CheckBox` 主要是 `is_checked`、`is_enabled`、回调、内容（`:2241-2248`）；`Slider` 主要是 min/max/value/enabled/step/orientation/回调（`:1652-1662`）；`InfoBar` 主要是 title/message/severity/open/closable/回调（`:2568-2576`）。视觉属性不由 Reactor 暴露，由 WinUI 模板决定。
- 令牌：`ThemeBrush` 只有 8 个变体：`Accent`、`AccentText`、`PrimaryText`、`SolidBackground`、`CardBackground`、`CardStroke`、`SystemCritical`、`SystemCriticalBackground`（`src/element.rs:158-236`）。每个变体映射到一个 WinUI `*Brush` 资源键（`:224-236`）。`Brush` 只有 `Theme(ThemeBrush)` 与 `Solid(Color)` 两种（`:266-310`）。
- 样式：`ThemeStyle` 是 4 个 `Option<ThemeBrush>` 的定长数组（`src/core/runtime.rs:9-27`）；只有 `TextBlock`、`Border`、`Grid`、`TextBox`、`Rectangle`、`Ellipse`、`Line` 会填它（`src/generated.rs:11201-11280`），所有交互控件（Button/CheckBox/ToggleSwitch/Slider/...）都是 `ThemeStyle::default()`。`apply_theme_style` 把非空槽位编译成一段 XAML `Style`，用 `{ThemeResource ...}` setter 设到 `FrameworkElement.Style`（`src/native/winui/mod.rs:998-1030`、`:325-335`）。
- 资源覆盖：`ResourceOverrides` 是 `BTreeMap<String, ResourceValue>`，值是 `Color` / `Thickness` / `CornerRadius`（`src/element.rs:1508-1576`）。它被写到节点的 `FrameworkElement.Resources`，支持增删 key（`src/native/winui/mod.rs:1055-1120`、`:1887-1895`、`:1985-1990`）。**0.100.0 只有 `Button` 暴露 `resource_overrides()`**（`src/generated.rs:531-535`），其他控件没有这个 builder 方法。
- 变体：`ButtonStyle` 是 `Default` / `Accent` / `Subtle` / `TextLink`（`src/generated.rs:75-80`），映射到 WinUI 的 `AccentButtonStyle`、`SubtleButtonStyle`、`TextBlockButtonStyle`（`src/native/winui/generated.rs:928-939`）。
- 主题：`WindowTheme` 是 `System` / `Light` / `Dark`（`src/element.rs:1578-1592`）；`ColorScheme` 只有 `Light` / `Dark`（`:1594-1601`）。高对比由原生 WinUI 处理，Reactor 没有高对比 token 表。
- 动效：`ExitTransition` 只有 fade，持有一个 `Duration`（`src/element.rs:2152-2168`）；`Border` 另有 `opacity_transition` 与 `scale_transition`，只是把 duration 交给 WinUI 的 `ScalarTransition` / `Vector3Transition`（`src/native/winui/generated.rs:1059-1105`、`src/native/winui/mod.rs:3905-3918`）。没有曲线、没有统一的 motion token 表、没有 enter transition。
- 状态层与焦点：源码里没有 `VisualStateManager` 操作，也没有 hover/pressed 的绘制；`Focus` 只是程序化请求 `FocusState::Programmatic`（`src/native/winui/mod.rs:1489-1492`）。Normal/PointerOver/Pressed/Disabled 和焦点环都由原生 WinUI 模板负责。
- 密度：没有 density API；`Compact` 只出现在 `NavigationViewPaneDisplayMode`、`SplitViewDisplayMode` 这类显示模式里，不是 WinUI 的 Compact 资源覆盖。

一句话：Reactor 的组织方式是“**语义元素树 + 少量主题资源选择 + 原生控件模板**”。它不拥有视觉层，所以不能当控件视觉规格的来源；它拥有的是 Rust 侧的 API 形状和资源覆盖的接线方式。

### 1.2 参考实现总表

| 参考 | 许可证 | 语言 | 最近提交（2026-10） | 读什么 | 可信度/用途 |
|---|---|---|---|---|---|
| **`microsoft/microsoft-ui-xaml`**（WinUI 3 XAML） | MIT | C++/XAML | 2026-10-30 | `controls/dev/CommonStyles/*_themeresources.xaml`、`Common_themeresources_any.xaml`、`DensityStyles/Compact.xaml`、`generic.xaml` | **第一权威**：控件模板、状态、尺寸、ThemeResource 值 |
| `microsoft/WindowsAppSDK` | MIT | C++/C# | 2026-10-29 | `README.md`、release notes、samples | 分发与 API 文档；WinUI 源码不在此仓库，不能当规格源（`README.md:24-27`） |
| **`microsoft/windows-rs` / `windows-reactor`** | MIT OR Apache-2.0 | Rust | 2026-10-30 | `crates/libs/reactor/src/{element,generated,core,native/winui}` | **Rust API 形状第一参考**；视觉规格不可用（委托 WinUI） |
| `microsoft/fluentui`（React v9） | MIT | TypeScript | 2026-10-30 | `packages/tokens/src/**`、`packages/react-components/react-*/library/src/**/use*Styles.styles.ts`、`react-tabster/src/focus/*` | **Web 令牌/状态命名交叉验证**；尺寸与焦点环不能覆盖 WinUI |
| `microsoft/microsoft-ui-reactor`（C#） | MIT | C# | 2026-10-30 | `README.md`、`docs/specs/001-theming-design.md`、`docs/specs/015-styling-design.md` | 实验性；只看主题/轻量样式设计，不看作视觉规格 |
| `microsoft/WinUI-Gallery` | MIT | C#/XAML | 2026-10-27 | `WinUIGallery/Samples/<Control>/<Control>Page.xaml` | 变体与用法清单；不是规格，值仍回查 XAML |
| `microsoft/fluentui-blazor` | MIT | C# | 2026-07-29 | `README.md`、组件实现 | 包装 **Fluent UI Web Components**，不是 WinUI；只看组件 API 命名 |
| `dotnet/wpf` Fluent theme | MIT | C#/XAML | 2026-10-29 | `Documentation/docs/using-fluent.md`、`Themes/Fluent*.xaml` | 实验性；XAML 资源结构可作二次对照 |
| `unoplatform/uno` | Apache-2.0（core） | C# | 2026-10-30 | `README.md`、WinUI API 移植 | WinUI API 的跨平台移植；不是第一权威 |
| `microsoft/fluentui-system-icons` | MIT | 多平台资源 | 2026-10-30 | 图标字体/ SVG 与 glyph 映射 | Fluent 图标来源；本仓库图标目前只有 Material Symbols |
| 本地 `fluent_ui`（Flutter） | BSD-3-Clause | Dart | 上游 HEAD 2026-10-21 | `/home/ollie/git/widget-ref/fluent_ui` | 二次对照；已知缺口见 `docs/research/fluent-ui-analysis.md:544-617` |

对应 URL：

- `https://github.com/microsoft/microsoft-ui-xaml`
- `https://github.com/microsoft/WindowsAppSDK`
- `https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor`
- `https://github.com/microsoft/fluentui`
- `https://github.com/microsoft/microsoft-ui-reactor`
- `https://github.com/microsoft/WinUI-Gallery`
- `https://github.com/microsoft/fluentui-blazor`
- `https://github.com/dotnet/wpf`
- `https://github.com/unoplatform/uno`
- `https://github.com/microsoft/fluentui-system-icons`
- `https://github.com/bdlukaa/fluent_ui`（本地 `fluent_ui` 克隆的上游，HEAD `f806a8641f245d919176f38af0bc56f9391972a0`，2026-10-21；`docs/reference/README.md:22` 只记本地路径）

### 1.3 阅读顺序

分两条线，不要混：

- **视觉规格线**：WinUI XAML → WinUI Gallery 变体 → React v9 状态命名/动效用法 → Flutter `fluent_ui` 二次对照。视觉值以第一条为准。
- **Rust API 线**：`windows-reactor` 的 `Element`/`View`、`ThemeBrush`/`Brush`、`ResourceOverrides`、`ButtonStyle`、`ExitTransition` → 我们的 `Widget`/`Cx`/`StyleKit`/`StyleScope`。只借形状，不借视觉。

---

## §2 令牌从哪来

### 2.1 Fluent 2 的两层模型与本地导出

Fluent 2 的官方模型是 **global tokens + alias tokens**：global 存原始值（调色板、字族、圆角、间距、描边、动效），alias 加语义（`colorNeutralForeground1`、`colorBrandBackground`、`shadow16`），theme 再把两层拍平成一套令牌。本仓库的导出选的是 `@fluentui/tokens` 1.0.0-alpha.24（MIT），459 令牌 × 3 套主题（web light / web dark / teams high contrast），另有品牌色阶、65 条全局调色板与全部刻度。证据：

- `docs/reference/fluent-color-spec.md:5` 记录数据源选择：`@fluentui/tokens` 1.0.0-alpha.24，MIT，459 × 3。
- `docs/reference/fluent-tokens.json` 的 `$source` 记录 `project`、`version`、`license`、`url`、`files`、`fetchedAt`、`note`；`themes.webLight` / `webDark` / `teamsHighContrast` 各 459 项。
- 本地生成物：`ui/crates/ui-design/data/fluent_data.rs:1-5` 写明 “Generated from docs/reference/fluent-tokens.json”；`ui/crates/ui-design/src/fluent.rs:19` 的 `TOKEN_SPEC` 是 `Source::Spec("@fluentui/tokens 1.0.0-alpha.24 (MIT)")`。
- 刻度：`ui/crates/ui-design/data/dimensions_data.rs:108-200` 是 Fluent 的 radius/spacing/type/duration/curve/hit 数据；其中 radius 11 档（medium 4、circular -1），spacing 0…32，type 24/20/16/14/12，duration 50/100/150/200/250/300/400/500，curve 9 条，hit 32/24。

这套数据适合 **Web 端 Fluent 2**，但不覆盖 WinUI 控件的 `ThemeResource` 词汇。

### 2.2 WinUI 控件实际使用的 ThemeResource 词汇

WinUI 的通用颜色/画刷在 `Common_themeresources_any.xaml`。文件结构是三个 `ResourceDictionary`：`Default`（暗）、`Light`、`HighContrast`。关键块：

| 类别 | 暗（Default） | 明（Light） |
|---|---|---|
| 文本填充 | `TextFillColorPrimary #FFFFFF`、`Secondary #C5FFFFFF`、`Tertiary #87FFFFFF`、`Disabled #5DFFFFFF`（`:5-8`） | `Primary #E4000000`、`Secondary #9E000000`、`Tertiary #72000000`、`Disabled #5C000000`（`:209-212`） |
| 控件填充 | `ControlFillColorDefault #0FFFFFFF`、`Secondary #15FFFFFF`、`Tertiary #08FFFFFF`、`Quarternary #0FFFFFFF`、`Disabled #0BFFFFFF`（`:15-19`） | `ControlFillColorDefault #B3FFFFFF`、`Secondary #80F9F9F9`、`Tertiary #4DF9F9F9`、`Quarternary #C2F3F3F3`、`Disabled #4DF9F9F9`（`:219-223`） |
| 控件实底/强填充 | `ControlSolidFillColorDefault #454545`、`ControlStrongFillColorDefault #8BFFFFFF`（`:22-24`） | `ControlSolidFillColorDefault #FFFFFF`、`ControlStrongFillColorDefault #72000000`（`:226-228`） |
| Alt 填充 | `ControlAltFillColorSecondary #19000000`、`Tertiary #0BFFFFFF`、`Quarternary #12FFFFFF`（`:30-32`） | `Secondary #06000000`、`Tertiary #0F000000`、`Quarternary #18000000`（`:234-236`） |
| 焦点 | `FocusStrokeColorOuter #FFFFFF`、`Inner #B3000000`（`:54-55`） | `Outer #E4000000`、`Inner #B3FFFFFF`（`:258-259`） |
| 卡片/层/实底背景 | `CardBackgroundFillColorDefault #0DFFFFFF`、`LayerFillColorDefault #4C3A3A3A`、`SolidBackgroundFillColorBase #202020`（`:56-75`） | `CardBackgroundFillColorDefault #B3FFFFFF`、`LayerFillColorDefault #80FFFFFF`、`SolidBackgroundFillColorBase #F3F3F3`（`:260-280`） |
| 系统语义 | `SystemFillColorSuccess #6CCB5F`、`Caution #FCE100`、`Critical #FF99A4`（`:76-87`） | `Success #0F7B0F`、`Caution #9D5D00`、`Critical #C42B1C`（`:281-291`） |

圆角在单独文件：`CornerRadius_themeresources.xaml:5-6` 给出 `ControlCornerRadius 4`、`OverlayCornerRadius 8`，三套主题相同（`:8-15`）。控件高度/内衬在 `generic.xaml` 与各控件文件：`ControlContentThemeFontSize = 14`（`generic.xaml:36`），`TextControlThemeMinHeight = 32`（`:96`），`FlyoutThemeMinWidth 96` / `MaxWidth 456` / `MinHeight 40` / `MaxHeight 758`（`:50-53`）。

动效令牌在 `Common_themeresources_any.xaml:602-606`：

```
ControlFastOutSlowInKeySpline       0,0,0,1
ControlNormalAnimationDuration      00:00:00.250
ControlFastAnimationDuration        00:00:00.167
ControlFastAnimationAfterDuration   00:00:00.168
ControlFasterAnimationDuration      00:00:00.083
```

注意：`ControlFastOutSlowInKeySpline = 0,0,0,1` 对应 Fluent 2 的 `curveDecelerateMid`（`cubic-bezier(0,0,0,1)`），**不是** `easeInOut`。这直接推翻当前 `MotionTable::fluent()` 的曲线选择（§4.4）。

高对比：`Common_themeresources_any.xaml` 的 `HighContrast` 字典主要用 `SystemColor*` 和 `SystemColor*Brush`（例如 `:421-479`），不是固定色值；默认/浅色字典里的固定值不能直接当高对比值用。`[未证实]`：WinUI 在运行时如何把 `SystemColor*` 映射到具体 RGB，取决于系统主题与高对比设置，本次没有 Windows 环境实测。

#### 2.2.1 阴影：Fluent 2 有具名 shadow，WinUI 用 ThemeShadow

Fluent 2 的阴影是具名令牌：`shadow2`、`shadow4`、`shadow8`、`shadow16`、`shadow28`、`shadow64`，以及对应的 `*Brand` 变体；每条是两层（ambient + key），例如 `shadow16 = 0 0 2px rgba(0,0,0,0.12), 0 8px 16px rgba(0,0,0,0.14)`（`docs/reference/fluent-tokens.json` 的 `themes.webLight`）。React v9 的 Menu 用 `shadow16`（`react-menu/.../useMenuPopoverStyles.styles.ts:22`），Dialog 用 `shadow64`（`react-dialog/.../useDialogSurfaceStyles.styles.ts:46`）。

WinUI 不用这套名字：它用 `ThemeShadow` / `IsDefaultShadowEnabled`（`generic.xaml:5957`），阴影由合成器按 elevation 生成。本仓库已有的 `ShadowSpec::elevation(level)` 是 WinUI 风格的两条线性近似（`ui/crates/ui-core/src/tree.rs:157-182`），不是 Fluent 2 的两层 shadow 令牌。当前 `WidgetMetrics.elevation: u8`（`ui/crates/ui-style/src/types.rs:66-74`）只能选一个层级，不能选 `shadow16`/`shadow64`，也不能表达 ambient/key 两层。需要补一个 `ShadowToken` 或直接在 overlay metrics 里放 `ShadowSpec`，至少覆盖 Menu（shadow16）、Dialog（shadow64）、Tooltip（drop-shadow 近似）。

### 2.3 两套词汇的差异

| 语义 | WinUI 名（light / dark） | Fluent 2 Web 最近名（light / dark） | 差异 |
|---|---|---|---|
| 控件默认底 | `ControlFillColorDefault` `#B3FFFFFF` / `#0FFFFFFF` | `colorNeutralBackground1` `#FFFFFF` / `#292929`（`fluent_data.rs:55`） | WinUI 是半透明洗色，叠在父表面；Fluent 2 是不透明表面。不能互换 |
| 控件悬停底 | `ControlFillColorSecondary` `#80F9F9F9` / `#15FFFFFF` | `colorNeutralBackground1Hover` `#F5F5F5` / `#3D3D3D`（`:56`） | 同上：alpha 洗色 vs 实色 |
| 主文本 | `TextFillColorPrimary` `#E4000000` / `#FFFFFF` | `colorNeutralForeground1` `#242424` / `#FFFFFF`（`:91`） | WinUI 带 alpha；Fluent 2 是实色 |
| 焦点 | `FocusStrokeColorOuter` + `FocusStrokeColorInner` 双环 | `colorStrokeFocus2` 单环 `#000000` / `#FFFFFF`（`:338`） | 数量与语义都不同；不能把 `FocusRing` 一条角色当双环 |
| 浮层底 | `MenuFlyoutPresenterBackground = DesktopAcrylicTransparentBrush`（`MenuFlyout_themeresources.xaml:40`） | `colorNeutralBackground1` / `colorNeutralBackground2` | WinUI 用 acrylic；Fluent 2 Web 的菜单用实色 + shadow |
| 强调色 | `AccentFillColorDefaultBrush = SystemAccentColorLight2`（暗）/ `Dark1`（明），`Secondary`/`Tertiary` 是 0.9/0.8 透明度（`Common_themeresources_any.xaml:125-127`、`:329-331`） | `colorBrandBackground` = 品牌色阶 80（`fluent_data.rs:12`） | WinUI 的 accent 是运行时系统色；Fluent 2 的品牌阶是输入 |

结论：**两套词汇必须同时可达，不能做一对一别名。** 现有 `NATIVE_ALIAS` 的 7 条（`ui/crates/ui-design/src/fluent.rs:110-118`）只能覆盖少数名字，且 `controlSolidFillColorDefault -> colorNeutralBackground1` 这种映射会把 WinUI 的 `#FFFFFF` 实底换成 Fluent 2 的 `#FFFFFF`（浅色碰巧相同，深色 `#454545` vs `#292929` 就不同），属于“看起来对、实际错”。

### 2.4 现有接线与缺口

- `pack_of` 把上下文硬编码为 `Platform::Web`（`ui/crates/ui-widgets-fluent/src/pack.rs:26`）。`ui-design` 的 `Platform` 只有 `Ios/Ipados/Macos/Android/Web/Desktop`（`ui/crates/ui-design/src/context.rs:71-81`），没有 `Windows`，所以当前连“WinUI 上下文”都表达不了。
- `pack_of` 只从 `FluentSystem` 解析 6 个角色名（`pack.rs:47-58`），另外 3 个 wash 是字面量（`:40-42`）。这三个 wash 恰好是 WinUI 的 `ControlAltFillColor{Secondary,Tertiary,Quarternary}` 浅色值，但深色值 `#19000000/#0BFFFFFF/#12FFFFFF` 与当前“白/黑 alpha”公式不一致（当前深色用白 alpha `0x06/0x0f/0x18`，WinUI 暗色是 `0x19/0x0B/0x12`，且 `Secondary` 是黑 alpha）。这是一个已存在的错误，`pack.rs:92-100` 的单调性测试只检查 alpha 递增，抓不到它。
- `FluentSystem::native_colour` 先查 459 个 Web 主题令牌，再查 7 条别名（`fluent.rs:149-172`）；WinUI 名如 `ControlFillColorDefault` 会落到 `Unmapped`，而 `paint.rs` 的 `role_of` 会 debug-assert 并回退洋红。
- `StyleCaps` 有 `named_state_tokens`、`radius_scale`、`motion_tokens` 等 11 个布尔（`ui/crates/ui-design/src/caps.rs:7-60`），但没有 `focus_visuals`、`density`、`icon_set`、`per_control_metrics` 这些 Fluent 真正需要的能力位。

### 2.5 建议：生成第二张表，保留两个命名空间

最小可行方案：

1. 新增 `ui/crates/ui-design/data/winui_theme_resources.rs`，从 `Common_themeresources_any.xaml` + 每个控件的 `*_themeresources.xaml` 生成。每条记录 `(name, [light, dark, high_contrast])`，`Source::Spec("microsoft-ui-xaml <file> @<sha>")`。高对比列先只收 `SystemColor*` 的引用名，不猜 RGB，标 `[未证实]`。
2. 新增 `WinUISystem`（或让 `FluentSystem` 接受 `Platform::Windows` 并同时查两张表）。`native()` 先查 WinUI 表，再查 Fluent 2 Web 表；两个命名空间都保留，不做静默别名。
3. `StyleBuilder::role_from` 允许指定命名空间，例如 `role_from_winui("buttonBackground", "ControlFillColorDefault")` 与 `role_from_web("accent", "colorBrandBackground")`。禁止再用 `NATIVE_ALIAS` 的 7 条兜底。
4. 对确实需要跨词汇映射的名字，写一张**显式映射表**，每条带 `Source::Derived("...")` 与理由；映射不上的返回 `Unmapped`，让测试红，而不是画洋红。
5. 品牌色：WinUI 的 accent 是运行时 `SystemAccentColor*`，Fluent 2 的 brand 是 16 级色阶。短期保留两套：`FluentSystem` 用 Web brand ramp，`WinUISystem` 用注入的 `SystemAccentColor`；不要在 `pack.rs` 里用一套冒充另一套。

---

## §3 逐控件规格来源

下表每个控件给三列：WinUI 主规格（文件:行）、React v9 交叉验证、WinUI Gallery 示例。Gallery 路径统一在 `microsoft/WinUI-Gallery` 的 `WinUIGallery/Samples/<Control>/<Control>Page.xaml`（已逐个 HTTP 200 验证）。React v9 的路径省略前缀 `packages/react-components/`。

| 控件 | WinUI 主规格（file:line） | 关键数值 | React v9 交叉验证 | Gallery |
|---|---|---|---|---|
| **Button** | `Button_themeresources.xaml:152`、`:154-168`、`:176-227`；`:30-41` | padding `11,5,11,6`；font 14 / normal；focus margin `-3`；corner 4；背景 `ControlFillColorDefault`，hover `Secondary`，pressed `Tertiary`，disabled `ControlFillColorDisabled`；前景 `TextFillColorPrimary` | `react-button/library/src/components/Button/useButtonStyles.styles.ts:63-75`：padding medium `5px 12px`，minWidth 96，radius 4，font 14 semibold，lineHeight 20，transition 100 ms `curveEasyEase`；`:292-318` small/large | `Samples/Button/ButtonPage.xaml` |
| **CheckBox** | `CheckBox_themeresources.xaml:270-277`、`:283-294`、`:45-64`、`:602-607` | 盒 20，glyph 12，height 32，minWidth 120，padding `8,5,0,0`，focus margin `-7,-3,-7,-3`，corner 4；未选中 fill `ControlAltFillColorSecondary`，checked/indeterminate fill `AccentFillColorDefault`；glyph `TextOnAccentFillColorPrimary` | `react-checkbox/.../useCheckboxStyles.styles.ts:24-25`：indicator 16/20；`:138-152` radius small，font 12/16；`:164-182` padding 8/4 | `Samples/CheckBox/CheckBoxPage.xaml` |
| **RadioButton** | `RadioButton_themeresources.xaml:179-181`、`:187-197`、`:367-378` | glyph 12 / hover 14 / press 10；padding `8,6,0,0`；minWidth 120；focus margin `-7,-3,-7,-3`；corner 4；外圈 20，行高 32；按下内点 4 | `react-radio` 包；未逐行核对 `[未证实]` | `Samples/RadioButton/RadioButtonPage.xaml` |
| **ToggleSwitch** | `ToggleSwitch_themeresources.xaml:186-187`、`:188-201`、`:15-39`、`:507-515`、`:232-275` | minWidth 154；focus margin `-7,-3,-7,-3`；corner 4；track 40×20，radius 10；knob 容器 20×20，内圆 12×12；hover/press 动画 83 ms；off fill `ControlAltFillColorSecondary`，on fill `AccentFillColorDefault`，on knob `TextOnAccentFillColorPrimary` | `react-switch/.../useSwitchStyles.styles.ts:24-30`：track 40×20 / 32×16，thumb 18/14，gap 2；`:48-77` radius circular，durationNormal 200 ms `curveEasyEase` | `Samples/ToggleSwitch/ToggleSwitchPage.xaml` |
| **Slider** | `Slider_themeresources.xaml:162-173`、`:14-27`、`:195-255`、`:393-416` | track 高 4，thumb 18，inner 12，height 32，pre/post 14，track corner 2，thumb corner 10；rest inner 12（scale 0.86），hover 14（1.167），press 10（0.71）；动画 167/250 ms | `react-slider/.../useSliderStyles.styles.ts:44-59`：medium thumb 20 / inner 6 / rail 4 / minHeight 32；small 16/5/2/24；rail radius xLarge `:122` | `Samples/Slider/SliderPage.xaml` |
| **TextField** | `TextBox_themeresources.xaml:175-198`、`:302`、`:336-339`；`Common_themeresources.xaml:10-12` | header margin `0,0,0,8`；icon 12；minHeight 32；padding `10,5,6,6`；corner 4；border 1，focused `1,1,1,2`；focused 边框切换是离散状态 | `react-input/.../useInputStyles.styles.ts:47` radius 4；`:75-77` focus underline `2px`；`:24` horizontal padding；高度 24/32/40 在 size 变体（同文件，`[未证实]` 行号） | `Samples/TextBox/TextBoxPage.xaml` |
| **ComboBox** | `ComboBox_themeresources.xaml:321-347`、`:32-64`、`:570-590` | minWidth 64，minHeight 32，padding `12,5,0,7`；item padding `11,5,11,7`；highlight radius 7，drop-down button radius 4，item radius 3，pill radius 1.5；popup border 2，popup corner 8；popup 背景 `AcrylicInAppFillColorDefaultBrush`；glyph 12 | `react-combobox` 包；样式文件未逐行核对 `[未证实]` | `Samples/ComboBox/ComboBoxPage.xaml` |
| **ListView / ListViewItem** | `ListViewItem_themeresources.xaml:13-14`、`:29-30`、`:58-60`、`:244-256` | minWidth 88，minHeight 40，corner 4；selection indicator radius 1.5；focus primary 2 / secondary 1；padding `16,0,12,0`；hover `SubtleFillColorSecondary`，pressed `Tertiary`，selected `Secondary` | `react-list` 包；未逐行核对 `[未证实]` | `Samples/ListView/ListViewPage.xaml` |
| **Menu / MenuFlyout** | `MenuFlyout_themeresources.xaml:255-265`、`:298-313` | presenter padding `0,2,0,2`，minHeight 32，minWidth 96，corner 8，背景 `DesktopAcrylicTransparentBrush`；item margin `4,2,4,2`，item padding `11,8,11,9`（窄 `11,4,11,5`），corner 4；separator 1 | `react-menu/.../MenuPopover/useMenuPopoverStyles.styles.ts:14-23`：radius 4，minWidth 138，maxWidth 300，padding 4，shadow16；`MenuItem/useMenuItemStyles.styles.ts:25-39`：radius 4，padding 6，font 14，maxWidth 290 | `Samples/MenuFlyout/MenuFlyoutPage.xaml` |
| **Dialog / ContentDialog** | `ContentDialog_themeresources.xaml:12-19`、`:56-66`、`:222-245` | min/max width 320/548；min/max height 184/756；button spacing 8；title margin `0,0,0,12`；padding 24；separator 1；corner 8；title 20 semibold | `react-dialog/.../DialogSurface/useDialogSurfaceStyles.styles.ts:27-46`：padding 24，radius 8，maxWidth 600，shadow64 | `Samples/ContentDialog/ContentDialogPage.xaml` |
| **TabView** | `TabView_themeresources.xaml:239-264` | header padding `0,8,0,0`；item padding `8,3,4,3`；selected `9,3,5,4`；minHeight 32；minWidth 100；maxWidth 240；font 12；icon 16；close 24×32，glyph 16；separator `0,8,0,8` | `react-tabs` 包；样式文件未逐行核对 `[未证实]` | `Samples/TabView/TabViewPage.xaml` |
| **ProgressBar** | `ProgressBar_themeresources.xaml:5-32` | border 0（高对比 1）；foreground `AccentFillColorDefault`；background `ControlStrongStrokeColorDefault`；minHeight 3；track 1；corner 1.5 / track 0.5 | `react-progress/.../useProgressBarStyles.styles.ts:17-38`：background `colorNeutralBackground6`，thickness 2/4，radius medium | `Samples/ProgressBar/ProgressBarPage.xaml` |
| **ToolTip** | `ToolTip_themeresources.xaml:5-6`、`:47-52`、`:75-77` | font 12；border 1；foreground `TextFillColorPrimary`；background `AcrylicInAppFillColorDefaultBrush`（`:14`）；padding `9,6,9,8`；maxWidth 320；corner 4 | `react-tooltip/.../useTooltipStyles.styles.ts:21-29`：maxWidth 240，font 12，radius 4，padding `4px 11px 6px 11px` | `Samples/ToolTip/ToolTipPage.xaml` |
| **InfoBar** | `InfoBar_themeresources.xaml:62-78`、`:21`/`:40`/`:59` | title/message 14（title semibold）；minHeight 48；close 38，glyph 16；content padding `16,0,0,0`；icon margin `0,16,14,16`，icon 16；panel margin `0,0,16,0`；border 1（高对比 2） | React v9 没有对应 InfoBar 包（检索未见 `react-info-bar`）；以 WinUI 为准 `[未证实]` | `Samples/InfoBar/InfoBarPage.xaml` |
| **BreadcrumbBar** | `BreadcrumbBar_themeresources.xaml:5-33` | chevron padding `2,0`；font weight normal；font size 14；chevron 12；normal/hover/pressed/disabled 分别 `TextFillColorPrimary/Secondary/Tertiary/Disabled`；ellipsis flyout acrylic | `react-breadcrumb` 包；未逐行核对 `[未证实]` | `Samples/BreadcrumbBar/BreadcrumbBarPage.xaml` |
| **NavigationView** | `NavigationView_themeresources.xaml:208`、`:217`、`:219-223`、`:228`；`NavigationView.idl:159-160`；`NavigationView.h:528` | compact pane 48；open pane 320；item minHeight 36；icon box 16；selection indicator 3×16，radius 2；separator 1；item button margin `4,2` | `react-nav` 包；未逐行核对 `[未证实]` | `Samples/NavigationView/NavigationViewPage.xaml` |

### 3.1 几个容易读错的点

- **Button 的 padding 与高度**：WinUI `ButtonPadding = 11,5,11,6`（`Button_themeresources.xaml:152`），没有显式 `MinHeight`；高度由内容 + padding 决定，常见 32。React v9 的 `minWidth: 96`、`padding: 5px 12px` 是 Web 值，不要拿来覆盖 WinUI。
- **CheckBox 的盒与行高**：盒 20、行高 32、minWidth 120（`CheckBox_themeresources.xaml:270-277`）。React v9 的 indicator 是 16/20，且没有 32 行高的强约束；WinUI 的 20 + 32 是硬值。
- **ToggleSwitch 的 knob**：WinUI 的 `SwitchKnob` 容器是 20×20，内圆 `SwitchKnobOn/Off` 是 12×12（`ToggleSwitch_themeresources.xaml:509-515`）；当前实现用 `KNOB_H=18, KNOB_W=12`（`ui/crates/ui-widgets-fluent/src/toggle.rs:198-208`），视觉上比 WinUI 矮 2 px。这不是等价近似，应改。
- **Slider 的 inset 与 thumb**：WinUI 的 pre/post margin 是 14，thumb 18，track 4（`Slider_themeresources.xaml:162-173`）；当前实现是 `SLIDER_INSET=10`、`SLIDER_THUMB_R=10`（直径 20）、`SLIDER_TRACK_H=3.75`（`ui/crates/ui-widgets-fluent/src/slider.rs:162-172`）。当前值来自 Material 习惯（源码注释自己写了 “Material's enabled 10”），不是 WinUI。
- **ComboBox 的 popup 背景**：是 `AcrylicInAppFillColorDefaultBrush`（`ComboBox_themeresources.xaml:34`），不是 `AcrylicBackgroundFillColorDefaultBrush`；Menu 用的是 `DesktopAcrylicTransparentBrush`（`MenuFlyout_themeresources.xaml:40`）。两种 acrylic 的用途不同，不要合并成一个 `Material::Acrylic` 就完事。
- **Dialog 的宽度**：WinUI 是 320/548（`ContentDialog_themeresources.xaml:12-13`），React v9 是 maxWidth 600（`useDialogSurfaceStyles.styles.ts:39`）。以 WinUI 为准。
- **TabView 的选中 padding 会变**：未选中 `8,3,4,3`，选中 `9,3,5,4`（`TabView_themeresources.xaml:240-241`）。这意味着选中态不是纯换色，几何会动 1 px；实现时不能只换背景。
- **NavigationView 的两个 pane 宽度**：compact 48（`NavigationView_themeresources.xaml:208`），open 320（`NavigationView.idl:159-160`、`NavigationView.h:528`）。当前 `FluentScaffold` 的 `SCAFFOLD_PANE_W=240`（`ui/crates/ui-widgets-fluent/src/scaffold.rs`）不是 WinUI 的 320；如果 scaffold 要冒充 NavigationView，需要区分这两个宽度。

---

## §4 与 MD3 包的差异，以及 StyleKit/令牌层要补什么

### 4.1 状态层：MD3 计算叠加，Fluent 查具名令牌

MD3 的状态层是“基色 + 固定透明度的 on-surface/primary 叠加”，在控件里现场算；Fluent 的状态是**每个状态一个具名令牌**，例如 Button 的 `ButtonBackground` / `PointerOver` / `Pressed` / `Disabled` 分别指向 `ControlFillColorDefault` / `Secondary` / `Tertiary` / `Disabled`（`Button_themeresources.xaml:30-37`），CheckBox 的未选中 fill 也是 `ControlAltFillColorSecondary/Tertiary/Quarternary`（`CheckBox_themeresources.xaml:54-57`）。

当前 `StyleCaps.named_state_tokens = true`（`ui/crates/ui-design/src/fluent.rs:138-145`）只表示“Fluent 有具名状态令牌”，但 `Style` 只提供 `roles: RoleDict`（`ui/crates/ui-style/src/pack.rs:45-77`），没有“按状态查角色”的接口。结果是每个控件自己拼字符串 `role_of(pack, "controlAltFillColorTertiary")`（`ui/crates/ui-widgets-fluent/src/toggle.rs:57-64`），没有编译期保证，也没有状态矩阵测试。

需要补：

- 一个 `StateStyle` / `InteractionRoles` 形状，按控件声明 `normal / hover / pressed / disabled / selected / checked / indeterminate / focus` 各自用哪些角色；角色名仍是字符串，但解析集中在 pack 构建期，控件只拿 `Color`。
- 一个状态矩阵测试工具：给定控件 spec 和状态枚举，断言每个状态解析到预期角色，而不是只断言“alpha 递增”。
- 如果继续用字符串角色，至少在 `role_of` 里 panic 而不是回退洋红；洋红只应出现在 debug 的未知角色，不应成为 gallery 的常态。

### 4.2 焦点视觉：WinUI 双环 + 每控件 margin，当前只有单环

WinUI 的焦点是双环：外环 `FocusStrokeColorOuter`、内环 `FocusStrokeColorInner`（`Common_themeresources_any.xaml:54-55` 暗、`:258-259` 明）。每个控件还有自己的 `FocusVisualMargin`：Button `-3`（`Button_themeresources.xaml:167`），CheckBox/Radio/ToggleSwitch `-7,-3,-7,-3`（`CheckBox:275`、`RadioButton:196`、`ToggleSwitch:200`），ListViewItem `1` 且 primary thickness 2 / secondary 1（`ListViewItem_themeresources.xaml:252-256`）。

当前 `CommonColor::FocusRing` 只映射到 `colorStrokeFocus2`（`ui/crates/ui-design/src/fluent.rs:73`），是一个单环颜色；`MetricsSet` 没有焦点环厚度或 margin 字段（`ui/crates/ui-style/src/types.rs:66-95`）。React v9 的焦点也是单环 2 px `colorStrokeFocus2`（`react-tabster/src/focus/createFocusOutlineStyle.ts:95-112`），所以不能拿 React 当双环依据。

需要补：

- `FocusVisual { outer: Color, inner: Color, outer_width: f32, inner_width: f32, margin: Insets }`，放进 `Style` 或 `MetricsSet` 的 per-control 部分。
- 每个控件一个 `focus_margin`，默认 `-3` 或 `-7,-3,-7,-3`，由 WinUI 文件决定。
- 焦点状态必须 key 在 `focus_visible` 上，而不是指针焦点；这是 `docs/lessons/design-errors.md` 已有的条目，Fluent 同样适用。
- 测试：外环/内环颜色在 light/dark 下分别等于 XAML 值；ListViewItem 的 2/1 厚度；Button 的 -3 margin。

### 4.3 圆角与层级：WinUI 只有 control 4 / overlay 8，当前三层同半径

WinUI 的通用圆角只有两个：`ControlCornerRadius 4`、`OverlayCornerRadius 8`（`CornerRadius_themeresources.xaml:5-6`），三套主题相同。控件可以在其上覆写：ComboBox highlight 7、drop-down button 4、item 3、pill 1.5（`ComboBox_themeresources.xaml:344-347`），ListViewItem 4 / selection indicator 1.5（`ListViewItem_themeresources.xaml:58-60`），Menu item 4（`MenuFlyout_themeresources.xaml:309`），Dialog 8（`ContentDialog_themeresources.xaml:66`）。

当前 `metrics_from_scale("medium")` 把 control/surface/overlay 三层都设成同一个 radius 4、border 1、padding `8,4`，只给 overlay 加了 elevation 3（`ui/crates/ui-style/src/pack.rs:223-239`）。这既没有 overlay 的 8，也没有 per-control 覆写。

需要补：

- `MetricsSet` 至少区分 control 4 / overlay 8 / per-control override；不要用一个 `medium` 步长推所有层。
- `WidgetMetrics` 增加 `min_width`、`min_height`、`focus_margin`、`icon_size`、`font_role`（`ui/crates/ui-style/src/types.rs:66-74` 目前只有 radius/corner/border/padding/elevation）。
- 控件 spec 里允许 `radius_override`，但默认从 control/overlay 层取；ComboBox 的 7/4/3 这种多级圆角要能在同一个控件里表达。
- 阴影单独处理：Fluent 2 的 `shadow16`/`shadow64` 是两层 ambient/key（§2.2.1），`WidgetMetrics.elevation: u8` 只能选层级，不能选令牌；Menu/Dialog 需要 `ShadowToken` 或 `ShadowSpec`。

### 4.4 动效：当前 `MotionTable::fluent()` 是 Flutter 值，不是 WinUI 值

WinUI 的动效令牌是 250/167/168/83 ms，曲线 `0,0,0,1`（`Common_themeresources_any.xaml:602-606`）。当前 `MotionTable::fluent()` 是 83/167/250/358 ms，曲线 `EaseInOut`（`ui/crates/ui-style/src/motion.rs:81-91`）。其中 358 ms 是 Flutter `fluent_ui` 的 slow，不是 WinUI；`EaseInOut` 也不是 `cubic-bezier(0,0,0,1)`。注意 `docs/research/fluent-ui-analysis.md:222-237` 把 358 对到 WinUI 的 350 ms、并说 WinUI 最快约 150 ms；那是另一套 motion 文档/Reveal 的映射，不是控件 XAML 的 `ControlFasterAnimationDuration 83`。以 XAML 为准。具体控件里：

- ToggleSwitch 的 knob 动画用 83 ms + `Cubic(0.4,0,0.2,1)`（`ui/crates/ui-widgets-fluent/src/toggle.rs:230`），这是 Material standard curve，不是 WinUI。
- Slider 的 thumb 动画用 167 ms + `EaseInOut`（`ui/crates/ui-widgets-fluent/src/slider.rs:191`），也不是 WinUI。
- WinUI 的 Slider inner thumb 用 167 ms（rest/hover 的 0.86/1.167）和 250 ms（press 的 0.71），曲线都是 `ControlFastOutSlowInKeySpline`（`Slider_themeresources.xaml:208-250`）。

需要补：

- `MotionTable` 增加 WinUI 槽位：`Faster 83`、`Fast 167`、`After 168`、`Normal 250`；曲线用 `Easing::Cubic(0.0, 0.0, 0.0, 1.0)`（`curveDecelerateMid`，本地数据已在 `dimensions_data.rs:182`）。
- 如果保留 4 个槽位，至少把 `Slow` 从 358 改成 250，并把 `StateChange` 的曲线改成 `Cubic(0,0,0,1)`；但 Fluent 2 的 50–500 与 WinUI 的 83–250 是两套刻度，长期应允许 `MotionScale` 按名字取，而不是只有 4 个槽。
- `ControlFastAnimationAfterDuration 168` 是 WinUI 单独给“动画结束后”的时长，当前没有对应槽；如果暂时不用，要在文档里标 `[未证实]` 其用途，不要假装已覆盖。
- 测试：`MotionTable::winui()` 的四个值、曲线控制点；ToggleSwitch 83 ms；Slider 167/250 ms；不能只测 `reverse_spec`。

### 4.5 密度：WinUI 用 Compact.xaml 覆盖全局资源，当前没有密度轴

WinUI 的 Compact 密度在 `controls/dev/dll/DensityStyles/Compact.xaml` 里覆盖全局资源：`TextControlThemeMinHeight 24`、`TextControlThemePadding 2,2,6,1`、`ListViewItemMinHeight 32`、`TreeViewItemMinHeight 24`、`ComboBoxMinHeight 24`、`ComboBoxPadding 12,1,0,3`、`NavigationViewItemOnLeftMinHeight 32`、字体仍 14（`:5-23`）。

当前 `MetricsSet` 只有三个 depth tier（`ui/crates/ui-style/src/types.rs:88-95`），没有 density 维度；`FluentOptions` 只有一个 `slider_track_h`（`ui/crates/ui-widgets-fluent/src/style.rs:11-24`）。`StyleScope` 只有 appearance/transition（`ui/crates/ui-style/src/env.rs:113-121`），也没有 density。

需要补：

- `Density { Standard, Compact }`，放在 `StyleScope` 或一个 scoped value（像 `FluentOptions` 那样通过 `Env::find` 取），不要塞进 `Appearance`。
- 一张 per-control × density 的 metric 表，至少覆盖 TextField、ListViewItem、ComboBox、NavigationViewItem；值直接来自 `Compact.xaml:10-23`。
- 测试：同一控件在 Standard/Compact 下的 minHeight/padding 分别等于 32/24、40/32、32/24、36/32；字体不变（14）。

### 4.6 尺寸、内衬、文本：当前只有三层 padding，没有 per-control min size

WinUI 每个控件都有自己的 minWidth/minHeight/padding，且 padding 的四个方向常常不等：Button `11,5,11,6`，CheckBox `8,5,0,0`，Radio `8,6,0,0`，TextField `10,5,6,6`，ComboBox `12,5,0,7`，Menu item `11,8,11,9`，Dialog 24，TabView `8,3,4,3`。当前 `WidgetMetrics.padding` 是 `Insets`，可以表达四边不等（`ui/crates/ui-style/src/types.rs:66-74`），但 `metrics_from_scale` 只给 `8,4`（`pack.rs:232`），没有 per-control 值，也没有 min size 字段。

文本：当前 `TextRole` 只有 5 个（`ui/crates/ui-style/src/types.rs:11-27`），`TypeTable` 是 5 项定长（`:46-58`），Fluent pack 是 20/16/14/12/12（`ui/crates/ui-widgets-fluent/src/pack.rs:62-68`）。WinUI 控件正文是 14（`generic.xaml:36`），Dialog title 是 20 semibold，InfoBar title/message 14，TabView header 12，ToolTip 12。Fluent 2 的 `FLUENT_TYPE` 又是 24/20/16/14/12（`ui/crates/ui-design/data/dimensions_data.rs:152-165`），与 pack 的 20/16/14/12/12 不一致。

需要补：

- `WidgetMetrics` 增加 `min_width`、`min_height`；`MetricsSet` 允许 per-control 覆盖，而不是只有三层。
- `TextRole` 至少拆出 `Control`（14，普通）与 `Caption`（12），并允许控件指定 `font_size/weight` 覆写；长期再对齐 Fluent 2 的 17 个 `typographyStyles`，不要现在就把 5 个角色硬扩成 17 个。
- `TypeTable` 的 5 项定长是公开 API 的硬限制；如果要加角色，先设计好迁移，不要在每个控件里写 `size: 14.0` 字面量。
- 文本测量是 engine gap #12（`docs/engine-gaps.md:158-171`）：TextField 的 header/placeholder、Dialog 的按钮排布、TabView 的标签宽度都需要 `cx.measure_text`；没有它，只能像 MD3 dialog 那样用短标签和固定宽度绕过。

### 4.7 材质与浮层：`Material` 够用，但缺 placement 与 enter/exit

`Material` 已支持 `Solid`、`Frost`、`Acrylic`、`Mica`、`Glass` 与 fallback（`ui/crates/ui-style/src/material.rs:9-80`），Acrylic 配方也有 `ui-widgets-fluent/src/material.rs`。WinUI 的用法：

- Menu presenter：`DesktopAcrylicTransparentBrush`（`MenuFlyout_themeresources.xaml:40`）。
- ComboBox popup：`AcrylicInAppFillColorDefaultBrush`（`ComboBox_themeresources.xaml:34`）。
- ToolTip：`AcrylicInAppFillColorDefaultBrush`（`ToolTip_themeresources.xaml:14` 的 `ToolTipBackgroundBrush`；样式 setter 在 `:44`）。
- Dialog：`SmokeFillColorDefault` 遮罩 + `SolidBackgroundFillColorBase`/`LayerFillColorAlt` 表面（`Common_themeresources_any.xaml:59`、`:61`、`:68`）。

当前 `Material` 能表达这些填充，但没有：

- 浮层的 anchor/placement（ComboBox 下拉、Menu 子菜单、ToolTip 四向、Flyout）。
- enter/exit 过渡（engine gap #13，`docs/engine-gaps.md:173-184`）。
- 浮层的焦点陷阱/模态目标解析（`docs/lessons/design-errors.md` 有相关条目）。
- 主轴的居中/末端定位（engine gap #11，`docs/engine-gaps.md:144-157`），Dialog 现在靠两个 `Grow::Yes` spacer 绕过。

需要补：

- 一个 `PopupSpec`/`OverlaySpec`：anchor、placement、offset、modal、focus trap、enter/exit motion；短期可以只支持 `BottomStart`/`TopStart`/`Center` 三种，覆盖 ComboBox/Menu/Dialog/ToolTip。
- 浮层节点用 `Sense::PASS_THROUGH` 或显式 hit 区域，避免遮罩吃掉下层点击；这是 `docs/lessons/design-errors.md` 已有的教训。
- Dialog 的居中在 gap #11 修好前继续用 spacer 绕过，但要在代码注释里标“临时绕过”，不要把它当成设计。

### 4.8 图标：当前只有 Material Symbols，Fluent 需要另一套

`ui/crates/ui-widgets/src/icon.rs:1-10` 写的是 “Material Symbols icons: a name resolved to a PUA codepoint and drawn as one glyph”，`IconFamily` 只有 Outlined/Rounded/Sharp（`:9-26`），`lookup` 只认 Material 名字（`:136` panic “unknown Material Symbols icon”）。WinUI 的 CheckBox/Radio/ComboBox/TabView/InfoBar/NavigationView 都需要 Fluent 的 check、chevron、close、severity 图标；`InfoBar_themeresources.xaml:69-74` 直接给了 `&#xF136;` 等 glyph，`TabView` 的 close 是 16 px，ComboBox 的 chevron 是 12 px。

需要补：

- 一个 `IconSet`/`IconFamily` 抽象，至少允许 `Fluent` 与 `MaterialSymbols` 两套；`Icon` 控件按 style 选择 family。
- Fluent System Icons 是 MIT（`microsoft/fluentui-system-icons`），可作为 glyph 来源；但 glyph 码点/字体文件要单独核对，不能凭记忆写。`[未证实]`：哪些 Fluent 图标与 WinUI 模板里的 `Segoe Fluent Icons` 码点一一对应，需要逐个比对。
- 短期可以只映射控件必需的十几个 glyph（check、indeterminate、chevron down/right、close、info/warning/error/success、settings、back），不要一次引入整套。

### 4.9 Reactor 的样式组织与可映射性

先重申：`windows-reactor` 是 Windows-only 的 retained-mode 封装，不能作为依赖进入我们的 portable crate（AGENTS.md §3 的边界测试会红）。下面是**概念映射**，不是代码复用。

| Reactor 机制 | 源码 | 与我们的对应 | 判断 |
|---|---|---|---|
| `Element`/`View` + `Component`/`Message` | `src/generated.rs:5440-5480`、`src/element.rs:434-464` | 我们的 `Widget::decl` + `Cx`（`ui/crates/ui-widgets/src/widget.rs:20-44`） | 形状可参考（builder、语义属性），生命周期不可照搬：我们是 immediate-mode，每帧 `decl`；它是 retained + reconciler |
| `ThemeBrush` 8 变体 + `Brush::Theme/Solid` | `src/element.rs:158-310` | `CommonColor` 23 角色 + `RoleName`/`Color`（`ui/crates/ui-design/src/common.rs:12-115`、`ui/crates/ui-style/src/pack.rs:45-77`） | 我们已有更丰富的角色；只借“主题引用 vs 字面色”的二分，不要把 23 缩成 8 |
| `ThemeStyle` 4 槽 | `src/core/runtime.rs:9-27`、`src/generated.rs:11201-11280` | `MetricsSet` 3 层 + per-control metrics（`ui/crates/ui-style/src/types.rs:66-95`） | 4 槽太窄，不能照搬；借“每控件固定属性表”的思路，但字段要按控件扩展 |
| `ResourceOverrides` 按 WinUI key 覆盖 | `src/element.rs:1508-1576`、`src/native/winui/mod.rs:1055-1120` | `StyleScope`/`Env` 的 scoped override（`ui/crates/ui-style/src/env.rs:113-151`、`ui/crates/ui-widgets-fluent/src/style.rs:59-61`） | **最值得借**：按 key 覆盖、增删同步、值类型受控；但我们的 `StyleScope` 是 `Copy`，应像 `FluentOptions` 一样放进独立的 scoped value，或新增 `TokenOverrides` |
| `ButtonStyle` 4 变体 | `src/generated.rs:75-80`、`src/native/winui/generated.rs:928-939` | Button spec 的 `variant` 枚举 | 直接借：`Default/Accent/Subtle/TextLink` 比 boolean 标志清晰 |
| `Property<T> { Inherited, Set }` | `src/element.rs:1919-1932` | `Option`/scope 继承 | 借三态语义：覆盖要能区分“未设置”与“设成默认值”；我们现在的 `Option` 已经接近，但 `StyleScope` 没有 per-token 的“未设置” |
| `WindowTheme { System, Light, Dark }` | `src/element.rs:1578-1592` | `Appearance { Light, Dark }`（`ui/crates/ui-design/src/context.rs:13-16`） | 借 `System` 到 app 层；`StyleScope` 仍只保留 Light/Dark，避免把系统解析塞进样式 |
| `ExitTransition::fade(Duration)` | `src/element.rs:2152-2168` | `MotionTable` + 未来的 enter/exit | 借“每个元素带退出过渡”的位置；它只有 fade、无曲线，不能当动效系统 |
| `ScalarTransition`/`Vector3Transition` | `src/native/winui/generated.rs:1059-1105` | 我们的 `AnimStore` | 不借：这是 WinUI 组合动画，曲线/语义不可见；我们需要自己的 `MotionTable` |
| 焦点 `FocusState::Programmatic` | `src/native/winui/mod.rs:1489-1492` | 我们的 focus state / `FocusVisual` | 不借：Reactor 不做焦点视觉；我们必须在控件里画双环 |
| 状态层 | 源码无 `VisualStateManager` 操作 | 我们的 `StateStyle`/`InteractionRoles` | 不借：Reactor 交给原生模板；我们必须自己实现 |
| 密度 | 无 density API | `Density` + per-control metrics | 不借：Reactor 没有；值回 WinUI `Compact.xaml` |

**可直接映射到 `StyleKit`/`Widget` 形状的**：

- `ButtonStyle` → `ButtonVariant`。
- `ResourceOverrides` → scope 级 `TokenOverrides`（按 `RoleName` 或 WinUI key）。
- `ThemeBrush`/`Brush` 的二分 → `Style::colour`/`native()` 的解析结果 + 字面 `Color`。
- `WindowTheme::System` → app 层 `Appearance::System`。
- `ExitTransition` 的挂载位置 → widget spec 的 `exit` 字段（但实现要等引擎 #13）。

**需要改造才能用的**：

- `ThemeStyle` 4 槽 → per-control `ControlMetrics` + `StateStyle`；4 槽不够。
- `Element`/`View`/`Component` → 我们的 `Widget`/`Cx`；只借 builder 风格，不借 retained 生命周期。
- `ResourceOverrides` 只挂在 Button → 我们的 scope 级覆盖要覆盖所有控件。

**不能照搬的**：

- 原生 WinUI 控件模板、VisualState、焦点视觉、密度资源；这些是 Reactor 委托给 WinUI 的，我们全部要自己画。
- Windows-only 的 COM 绑定与 `windows-reactor` 依赖。
- Retained component/reconciler 模型。
- 它的 8 个 `ThemeBrush`、4 个 `ThemeStyle` 槽、fade-only `ExitTransition` 作为“完整样式系统”。

### 4.10 汇总：需要新增/修改的类型（不动代码，只列清单）

按优先级：

1. `WinUISystem` / `winui_theme_resources.rs`：WinUI ThemeResource 表 + `Source::Spec`；与 `FluentSystem` 并存。
2. `ControlMetrics`（per control, per density）：`min_width`、`min_height`、`padding`、`border`、`radius`、`focus_margin`、`icon_size`、`font_role`。
3. `FocusVisual`：`outer`、`inner`、`outer_width`、`inner_width`、`margin`。
4. `MotionTable` 扩展：WinUI 83/167/168/250 + `Cubic(0,0,0,1)`；长期改成按名字取的 `MotionScale`。
5. `Density { Standard, Compact }`：scoped value，per-control metric 表。
6. `StateStyle` / `InteractionRoles`：normal/hover/pressed/disabled/selected/checked/indeterminate/focus 的角色表。
7. `TokenOverrides`：scope 级、按 key 覆盖，参考 `ResourceOverrides` 的增删同步；值类型只允许 `Color`/`Insets`/`f32`。
8. `PopupSpec`/`OverlaySpec`：anchor、placement、modal、focus trap、enter/exit；依赖 engine gaps #11/#13。
9. `IconSet`：Fluent vs Material Symbols；先映射控件必需的十几个 glyph。
10. `TextRole` 扩展：至少 `Control`(14) 与 `Caption`(12)；`TypeTable` 的 5 项定长要设计迁移。
11. `ShadowToken`/`ShadowSpec`：Menu `shadow16`、Dialog `shadow64`、Tooltip 近似；`elevation: u8` 不够表达两层 shadow。

---

## §5 分期实施顺序与验收

### P0：地基（不新增控件）

目标：让“写一个 Fluent 控件”不再需要重复硬编码。

工作项：

1. 生成 `winui_theme_resources.rs`，覆盖 `Common_themeresources_any.xaml` 的通用颜色/画刷 + 16 个目标控件的 `*_themeresources.xaml` 中实际使用的 key。每条带 `Source::Spec`。高对比先收 `SystemColor*` 引用，不猜 RGB。
2. `WinUISystem`（或 `FluentSystem` 的 Windows 上下文）接入 `native()`；保留 `FluentSystem` 的 459 个 Web 主题令牌，两个命名空间都可查。
3. `ControlMetrics` + `Density`：先把 16 个控件的 Standard 值录入；Compact 至少录入 TextField/ListViewItem/ComboBox/NavigationViewItem。
4. `FocusVisual`：双环颜色/厚度/margin；Button、CheckBox、Radio、ToggleSwitch、ListViewItem 先接。
5. `MotionTable`：WinUI 83/167/168/250 + `Cubic(0,0,0,1)`；`MotionTable::fluent()` 改为 WinUI 值（或新增 `winui()`，旧 `fluent()` 标记为 Flutter 兼容）。
6. `IconSet`：先做 Fluent 的 check/indeterminate/chevron/close/severity/settings/back 映射。
7. 文本测量入口：给 `Cx` 加只读的 `measure_text(&TextBlock, max_width) -> Size`（engine gap #12），至少让 TextField header、TabView 标签、Dialog 动作排布能问“放不放得下”。
8. Gallery：`FluentSystem` 页展示 WinUI 表与 Web 表的差异；每个目标控件页保留 Fluent 列，但先把 “not implemented yet” 换成“地基中”。

验收：

- `cargo test -p ui-design`：WinUI 表覆盖率测试；每个目标控件使用的 key 都能解析；焦点双环值等于 XAML；高对比不返回 `Unmapped`（或显式 `Absent(SystemColor)`）。
- `cargo test -p ui-style`：`ControlMetrics` 的 Standard/Compact 值；`FocusVisual`；`MotionTable` 四个时长与曲线控制点。
- `cargo build --workspace --tests --examples` 零 warning。
- `cargo test -p ui-platform --test platform_boundary` 绿。
- Gallery 的 Fluent 页不再出现洋红；`GALLERY_PAGE=fluent-system` 截图。

### P1：Button / CheckBox / Radio / Progress（+ 修 Switch / Slider）

为什么先做这四个：最常用、几何简单、gallery 已有 Fluent 占位列（`buttons.rs:212-220`、`checkbox.rs:109-116`、`radio.rs:79-86`），且不依赖文本测量、弹层、焦点陷阱。Progress 没有现成页，需要新建；Switch/Slider 已有实现，但几何/动效不是 WinUI（§4.4、§3.1），必须同批修。

工作项：

- Button：`ButtonVariant { Default, Accent, Subtle, TextLink }`；padding 11,5,11,6；corner 4；focus margin -3；状态角色表；最小宽度 96（WinUI 的 Button 没有显式 minWidth，但 96 是常见值；`[未证实]` 是否应作为硬约束，先按内容 + padding 实现，96 只做 gallery 对齐）。
- CheckBox：20 盒 / 12 glyph / 32 行高 / 120 minWidth / padding 8,5,0,0；checked/indeterminate 两态；三态 `Change` 是 engine gap #4（`docs/engine-gaps.md:57-65`），先只做 checked/unchecked，indeterminate 用 spec 字段 + 视觉，上报仍用 bool 或标 `[未证实]`。
- Radio：20 外圈 / 12 glyph（hover 14 / press 10）/ 32 行高 / 120 minWidth / padding 8,6,0,0。
- Progress：3 px minHeight / 1 px track / 1.5 / 0.5 corner；determinate/indeterminate/error/paused；indeterminate 需要动画，先用 `AnimStore` 驱动一个 0→1 的相位。
- Switch：knob 容器 20×20、内圆 12×12、margin 0/1；83 ms + `Cubic(0,0,0,1)`；hover/press 的 knob 12→14/17 按 WinUI `:232-275`。
- Slider：track 4、thumb 18、inner 12、inset 14、pre/post 14；rest/hover/press inner 12/14/10；167/250 ms + WinUI 曲线。

验收：

- Gallery 页 `buttons`、`checkbox`、`radio`、`progress`、`switch`、`slider` 的 Fluent 列不再是 “not implemented yet”。
- 状态矩阵：normal/hover/pressed/disabled/focus-visible；checked/unchecked（indeterminate 若做）；progress determinate/indeterminate/error/paused。
- 截图里焦点双环可见；Button 的 focus margin -3，CheckBox/Radio/Switch 的 -7,-3,-7,-3。
- 单元测试：几何常量（20/32/120、40×20、4/18/14）、状态角色选择、动效时长；不要给每个绘制调用写快照。
- `GALLERY_SELFTEST=1` 在 nested session 通过；截图 `/tmp/fluent-p1-*.png`。

### P2：TextField / ListView / TabView / Breadcrumb / InfoBar / Tooltip

为什么这六个：覆盖文本、列表、页签、导航路径、状态提示、浮层提示；除 Tooltip 外都不需要复杂弹层；是 P3 的前置（Dialog 的动作排布、Menu 的 item 样式会复用 TextField/ListView 的文本与行高）。

工作项：

- TextField：header margin 0,0,0,8；icon 12；minHeight 32；padding 10,5,6,6；border 1 / focused 1,1,1,2；placeholder/clear/validation；文本测量用 P0 的 `cx.measure_text`（gap #12），不再用固定宽度硬编码。
- ListView：row minHeight 40 / minWidth 88；padding 16,0,12,0；selection indicator 3×16 radius 2；hover/pressed/selected 角色；focus 2/1；滚动/虚拟化先不做，gallery 用固定行。
- TabView：header padding 0,8,0,0；item padding 8,3,4,3；selected 9,3,5,4；minHeight 32；minWidth 100；font 12；icon 16；close 24×32；选中几何变化要画出来。
- Breadcrumb：chevron padding 2,0；font 14；chevron 12；normal/hover/pressed/disabled 角色；ellipsis 先不做。
- InfoBar：minHeight 48；title/message 14；close 38/glyph 16；content padding 16,0,0,0；icon margin 0,16,14,16；四种 severity 背景/图标。
- Tooltip：font 12；padding 9,6,9,8；maxWidth 320；corner 4；acrylic 背景；placement 先做 top/bottom，依赖 gap #11 的定位或临时 offset。

验收：

- Gallery 页 `text-field`、`lists`、`tabs`、`breadcrumb`（新建）、`info-bar`（新建）、`tooltip`（新建）的 Fluent 列可用。
- TextField 的 focused 边框是 1,1,1,2；ListView 选中指示条 3×16；TabView 选中 padding 变 9,3,5,4；Breadcrumb chevron 12；InfoBar 四种 severity；Tooltip maxWidth 320。
- 文本测量/布局测试：TextField header 不压正文；TabView 标签不溢出；InfoBar 在 320 宽度下 title/message 不裁切。
- 截图：`/tmp/fluent-p2-*.png`；`GALLERY_SELFTEST=1` 通过。

### P3：Dialog / Menu / ComboBox / NavigationView

为什么最后：Dialog/Menu/ComboBox 需要浮层/模态/焦点陷阱/placement，依赖 P0 的 `PopupSpec` 与 engine gaps #11/#13；Menu/ComboBox 还依赖 acrylic 与 item 样式，Dialog 依赖按钮排布（P1）与文本测量（P2）。NavigationView 本身不是浮层，但它依赖 P2 的 ListView 行样式、P1 的按钮/焦点基础和 pane 布局，因此放在同一期收口。

工作项：

- Dialog：min/max width 320/548；min/max height 184/756；padding 24；corner 8；title 20 semibold；button spacing 8；separator 1；smoke 遮罩；焦点陷阱；enter/exit 过渡（gap #13 修好前先无动画）。
- Menu：presenter padding 0,2,0,2；minHeight 32；minWidth 96；corner 8；acrylic；item margin 4,2,4,2；item padding 11,8,11,9（窄 11,4,11,5）；corner 4；separator 1；键盘导航。
- ComboBox：minWidth 64；minHeight 32；padding 12,5,0,7；item padding 11,5,11,7；highlight 7 / drop-down 4 / item 3 / pill 1.5；popup border 2 / corner 8；popup acrylic；chevron 12；editable 变体先不做。
- NavigationView：compact pane 48；open pane 320；item minHeight 36；icon box 16；selection indicator 3×16 radius 2；separator 1；item margin 4,2；pane toggle 40×36（`NavigationView_themeresources.xaml:205-206`）。

验收：

- Gallery 页 `dialogs`、`menu`（新建）、`combo-box`（新建）、`navigation-view`（新建）的 Fluent 列可用。
- Dialog：320/548 宽度、24 padding、8 corner、title 20 semibold、焦点陷阱（Tab 不逃出）、遮罩点击关闭。
- Menu：corner 8、item padding 11,8,11,9、键盘上下/Enter、子菜单 placement。
- ComboBox：64/32、padding 12,5,0,7、popup acrylic、item radius 3、chevron 12。
- NavigationView：pane 48/320、item 36、icon 16、selection indicator 3×16。
- 截图：`/tmp/fluent-p3-*.png`；`GALLERY_SELFTEST=1` 通过；模态/浮层的 hit 测试。

### P4：变体与高级状态

工作项（3–6 个）：

- Button 变体：Accent / Subtle / TextLink 的完整状态（WinUI `Button_themeresources.xaml:30-41` 的 `AccentButtonStyle`/`SubtleButtonStyle`/`TextBlockButtonStyle`）。
- CheckBox indeterminate：三态 `Change` 需要 engine gap #4；如果 #4 未修，先只做视觉 + app 侧状态。
- TabView close/add：close 24×32 / glyph 16；add 32×24 / font 12。
- NavigationView top/left/compact：`NavigationViewDisplayMode` 的 Minimal/Compact/Expanded；top pane 48。
- Progress indeterminate/error/paused：`ProgressBar` 的 `ShowError`/`ShowPaused`/`IsIndeterminate`（Reactor `src/generated.rs:2116-2124` 可作语义清单）。
- InfoBar 四种 severity + closable：WinUI `InfoBarSeverity` 四值；close 38。

验收：

- 与 WinUI Gallery 对应页的变体逐项对照（截图并排）；至少覆盖 P4 列表中的 6 个变体。
- 不回归 P1–P3 的截图；`cargo test --workspace --features ui-text-fontique/system` 绿；`cargo build --workspace --tests --examples` 零 warning。

### 每期通用验收命令

在 `ui/` 下：

```bash
cargo test --workspace --features ui-text-fontique/system
cargo build --workspace --tests --examples
cargo test -p ui-platform --test platform_boundary
```

图形验收只在嵌套会话：

```bash
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=buttons GALLERY_SECONDS=300 ./ui/target/release/examples/gallery &
scripts/nested-session.sh shot /tmp/fluent-buttons.png
scripts/nested-session.sh stop
```

注意：

- `GALLERY_SELFTEST=1` 与 `GALLERY_DUMP` 分开跑；自检会切页，导出拿到的是最后一帧。
- 每个断言检查点先看红、再看绿；`0 passed` 是空集合，不是验证。
- 一个任务内 nested session 只 start/stop 各一次；不要每轮验证都启停。
- 不加“覆盖每个方法”的测试；只钉不变量（几何、状态矩阵、令牌覆盖、焦点双环）。

---

## §6 未证实项与风险

1. **WinUI 运行时行为未实测。** 本次只读 XAML 源码，没有 Windows 构建/运行环境。焦点双环的实际绘制、Reveal、高对比、acrylic 的真实采样、Compact 密度的运行时覆盖，都标 `[未证实]`。风险：源码值与运行时可能有系统主题/DPI/版本差异。
2. **高对比映射未证实。** `Common_themeresources_any.xaml` 的 `HighContrast` 字典用 `SystemColor*`（`:421-479`），不是固定 RGB。我们如果要把高对比列生成成 `Color`，必须决定是运行时解析系统色还是用占位；本次没有做这个决定。风险：高对比下颜色错误或不可读。
3. **`@fluentui/tokens` 版本漂移。** 本地是 1.0.0-alpha.24（`docs/reference/fluent-tokens.json` 的 `$source`）。alpha 版本可能改令牌名/值；生成表时要记录版本，升级时重新生成并跑覆盖率测试。
4. **React v9 是 Web 实现。** 它的尺寸、焦点环（单环 2 px）、圆角、菜单宽度都与 WinUI 不同（§3）。只能用于状态命名、动效用法、令牌结构的交叉验证，不能覆盖 WinUI 值。
5. **Reactor 是 preview，且 C# 与 Rust 能力不一致。** Rust 0.100.0 没有 `ThemeRef`，`ThemeBrush` 只有 8 个，`ResourceOverrides` 只有 Button 暴露；C# 设计文档里的三层值模型/轻量样式仍是 Draft（`README.md:145`、`docs/specs/015-styling-design.md:14-23`）。不要把 C# 设计当成 Rust 已实现。
6. **Reactor 源码未编译/运行。** 它是 Windows-only（`Cargo.toml:37-39`），本次只做静态阅读。`ResourceOverrides` 的运行时行为（`FrameworkElement.Resources` 的增删、主题切换后的重解析）没有实测。`[未证实]`。
7. **“Windows Reactor” 名称歧义。** 已核实：没有 `microsoft/windows-reactor` 独立仓库；Rust 包在 `microsoft/windows-rs` 的 `crates/libs/reactor`，C# 包在 `microsoft/microsoft-ui-reactor`。两者是同一实验的不同语言版本，不是 WinUI 的“标准实现”。如果用户指的是别的项目，需要新的 URL/证据。
8. **Fluent UI Blazor 不是 WinUI。** 它包装官方 Fluent UI Web Components（`README.md:10`），组件 API 可参考，视觉/尺寸不可直接搬到 WinUI。
9. **WPF Fluent 仍是 experimental。** `dotnet/wpf` 的文档明确说 “Fluent theme is still in experimental mode”（`Documentation/docs/using-fluent.md:4-5`），只能作 XAML 资源结构的二次对照。
10. **Uno 是移植，不是第一权威。** Uno 使用 WinUI 3 API surface（`README.md:15`），但跨平台渲染/主题可能有偏差；不要用 Uno 的值覆盖 WinUI XAML。
11. **图标码点未逐一核对。** Fluent System Icons 是 MIT，但 WinUI 模板里的 `Segoe Fluent Icons` 码点与 Fluent System Icons 的 glyph 是否一一对应，本次没有逐个比对。`InfoBar_themeresources.xaml:69-74` 的 `&#xF136;` 等是 WinUI 的码点，不能直接当 Fluent System Icons 的码点。
12. **引擎缺口会阻塞部分控件。** `docs/engine-gaps.md` 的相关项：
    - #3 `decl()` 不知道最终尺寸（`:44-56`）：Slider/TabView/Progress 的宽度、Dialog 的居中。
    - #11 没有主轴定位（`:144-157`）：Dialog 居中、Tooltip/ComboBox 的 placement。
    - #12 声明阶段拿不到文字尺寸（`:158-171`）：TextField header、Dialog 按钮排布、TabView 标签。
    - #13 没有进入/退出过渡（`:173-184`）：Dialog、Menu、Tooltip、ComboBox popup。
    - #16 控件拿不到按下点（`:235-249`）：Slider/范围滑块的拖拽归属。
    - #17 `decl()` 的 `local` 是拖拽所有者空间（`:250-264`）：Fluent 开关/滑块的指针换算。
    风险：在缺口修好前，这些控件只能做“静态外观 + 简化交互”，不能宣称与 WinUI 等价。
13. **没有性能数据。** 本次没有做 release 构建的帧剖析；Fluent 控件的 acrylic/模糊、焦点环、状态动画对帧时间的影响未测。`[未证实]`。
14. **没有 Windows 参考截图。** 所有数值来自 XAML；没有在 Windows 上逐控件截图对照。视觉验收只能靠 gallery 截图 + 人眼，风险是“数值对、观感不对”。
15. **Flutter `fluent_ui` 是二次参考。** 它的资源字典/acrylic/焦点双环已在 `docs/research/fluent-ui-analysis.md` 分析，但它已知缺口：没有 Reveal、没有 per-control compact density、动效未集中（`:544-617`）。不要把它当 WinUI 的替代证据。

---

## 附：本文用到的本仓库关键位置

| 主题 | 位置 |
|---|---|
| `Style` 字段 | `ui/crates/ui-style/src/pack.rs:45-77` |
| `StyleBuilder` / `metrics_from_scale` | `ui/crates/ui-style/src/pack.rs:154-240` |
| `MetricsSet` / `WidgetMetrics` | `ui/crates/ui-style/src/types.rs:66-95` |
| `TextRole` / `TypeTable` | `ui/crates/ui-style/src/types.rs:11-58` |
| `MotionName` / `MotionTable::fluent` | `ui/crates/ui-style/src/motion.rs:9-91` |
| `Material` | `ui/crates/ui-style/src/material.rs:9-80` |
| `StyleScope` | `ui/crates/ui-style/src/env.rs:113-151` |
| `CommonColor` | `ui/crates/ui-design/src/common.rs:12-115` |
| `CommonMetric` | `ui/crates/ui-design/src/metric.rs:10-30` |
| `StyleCaps` | `ui/crates/ui-design/src/caps.rs:7-60` |
| `FluentSystem` / `ADAPTER` / `NATIVE_ALIAS` | `ui/crates/ui-design/src/fluent.rs:34-172` |
| Fluent 生成数据 | `ui/crates/ui-design/data/fluent_data.rs:1-5` |
| Fluent 刻度 | `ui/crates/ui-design/data/dimensions_data.rs:108-200` |
| Fluent pack | `ui/crates/ui-widgets-fluent/src/pack.rs:18-71` |
| Fluent kit / `StyleKit` | `ui/crates/ui-widgets-fluent/src/style.rs:11-121` |
| Fluent toggle | `ui/crates/ui-widgets-fluent/src/toggle.rs:198-233` |
| Fluent slider | `ui/crates/ui-widgets-fluent/src/slider.rs:162-191` |
| `Widget` / `StyleKit` trait | `ui/crates/ui-widgets/src/widget.rs:20-50`、`ui/crates/ui-widgets/src/style.rs:18-30` |
| 图标 | `ui/crates/ui-widgets/src/icon.rs:1-136` |
| Gallery 页面注册 | `ui/crates/ui-gallery/examples/gallery/pages/mod.rs:68-102` |
| Gallery Fluent 占位 | `buttons.rs:212-220`、`checkbox.rs:109-116`、`radio.rs:79-86`、`tabs.rs:85-92`、`lists.rs:149-156`、`text_field.rs:114-120`、`dialogs.rs:238-245` |
| 引擎缺口 | `docs/engine-gaps.md:44-56`、`:144-184`、`:235-264` |
| 设计错误 | `docs/lessons/design-errors.md`（焦点/浮层/模态相关条目） |
| 已有 Flutter Fluent 分析 | `docs/research/fluent-ui-analysis.md:1-670` |
