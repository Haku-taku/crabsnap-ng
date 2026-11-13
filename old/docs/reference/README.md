# 参考资料与本地源码位置

> 从 `AGENTS.md` §4 迁出。**选什么参考、怎么读**在 `AGENTS.md` §2：读源码、参考它们
> **解决的问题与不变量**，不是抄代码结构；必要时用 `deepwiki` / `exa` 查官方文档。

## 1. 外部框架源码（本机已下载）

| 材料 | 路径 |
|---|---|
| egui / epaint / ecolor / emath 0.36.2 | `/home/ollie/.cargo/registry/src/index.crates.io-*/egui-0.36.2/src/` 等 |
| GPUI 0.2.2（Zed `79fe6a6`） | `/home/ollie/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/crates/gpui/src/` |
| taffy 0.15 | `/home/ollie/.cargo/registry/src/index.crates.io-*/taffy-0.15.0/src/` |
| GTK 4 布局契约 | `/usr/include/gtk-4.0/gtk/gtklayoutmanager.h` |
| Qt 6 布局契约 | `/usr/include/qt6/QtWidgets/qlayoutitem.h`、`qsizepolicy.h` |

## 2. 参考项目本地克隆

`/home/ollie/git/widget-ref/`：

| 目录 | 内容 |
|---|---|
| `fluent_ui` | Flutter 的 WinUI 风格控件库 |
| `liquid_glass_widgets`、`flutter_liquid_glass` | Apple 液态玻璃的两个实现 |
| `flutter` | sparse 克隆：`painting` / `material` / `animation`（MD3 控件的参考实现） |
| `haze` | Compose 背景模糊库，ADR 集在其 `docs/adr/` |

## 3. 本仓库的调研与设计文档

| 文档 | 内容 |
|---|---|
| `docs/research/egui-0.36.2-analysis.md`、`docs/research/gpui-0.2.2-analysis.md` | 引擎/渲染器调研 |
| `docs/research/flutter-md3-analysis.md` | MD3 令牌、状态层、动效（405 行，带行号） |
| `docs/research/fluent-ui-analysis.md` | WinUI3 资源字典 / acrylic / motion |
| `docs/research/liquid-glass-analysis.md` | Apple 玻璃两 pass 管线、越界 tap、降级阶梯 |
| `docs/research/layout-box-model.md`、`layout-flexbox.md`、`layout-grid.md`、`layout-architecture.md` | 布局引擎重写的四份规则调研（盒模型/定位、flex 八阶段、grid 轨道、引擎架构）；设计见 `docs/layout-engine-design.md` |
| `docs/research/engine-audit.md` | 子系统审计台账：flex 自动最小尺寸（F1 已修）与后续顺序 |
| `docs/research/dirty-rects.md` | GPU 脏矩形：GPUI/egui 的实测做法、`damage_buffer` 的协议约束、持久场景纹理方案 |
| `docs/research/i18n-fluent.md` | i18n 方案：`ui-i18n` crate、`Cx::t`、预格式化、回退链、Fluent 0.16 的能力边界（含实测成本） |
| `docs/research/fluent-reference-implementations.md` | Fluent 控件参考实现：WinUI 3 XAML 为视觉第一权威、`ThemeResource` 表、四个地基（metrics/density、焦点双环、动效槽、图标集）、分阶段计划；含 windows-reactor 的查证结论 |
| `docs/research/control-mapping-three-systems.md` | MD3 / Liquid Glass·SwiftUI / WinUI 3·Fluent 三套体系的控件清单（36 / ~50 / ~50）与逐功能对应表，含各方的独占缺口与同名不同义翻译表 |
| `docs/style-system-design.md` | 多风格体系设计（ui-style / ui-widgets crate、阶段图、三包令牌表、验收纪律） |
| `docs/reference/*.json`、`*.md` | 导出令牌与色彩规范（MD3 / Fluent / HIG） |
