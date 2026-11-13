
# AGENTS.md

> **强制约定**，优先级高于个人习惯。本文件是根路由，只保留会改变编辑行为的高信号规则。
> 章节号是外部契约：重排前先 `grep -rn 'AGENTS\.md §' docs/ README.md` 并同步改引用。

## 0. 工作前必读（最高优先级）

**旧项目的一切都是待验证输入，不是结论。** `/home/ollie/git/crabsnap` 的笔记、计划、注释 = 未证实轶事；源码 = 在用但可能带缺陷。不得默认其正确。

可采信只有三条通道，至少满足一条：
1. 新仓库复现（测试或最小例子跑通）；
2. 对上外部权威（协议规范、crates.io/docs.rs、egui/GPUI/Qt/GTK/Flutter）；
3. 能从第一性原理推出（数学、坐标变换、借用检查等）。

三条都不满足还要依赖的，文档标注 `[未证实]`。**这条对新仓库自己的设计文档同样适用。**

**动手改任何子系统前，先扫 `docs/lessons/design-errors.md`**（约 150 条：说法 / 实际情况 / 发现方式）。新错误写进那里，不写这里。

### 进程操作：不用命令行模式找/杀自己刚启动的进程

`pgrep -f` / `pkill -f` 会匹配发起命令的 shell 和包装层，拿错 pid 后会一路错下去。

1. 刚启动的进程用 `$!` 拿 pid；`setsid` 可能 fork，沿 `/proc/<pid>/task/<pid>/children` 下钻，每步核对 `/proc/<pid>/comm`。
2. 杀进程用显式 pid：先 `ps -eo pid,args` 看清楚，再 `kill <pid>`。
3. 必须用模式时加方括号（`'examples/galler[y]'`），且覆盖所有启动路径。
4. 从文件读到 pid 再动手前核对 `/proc/<pid>/comm`，pid 会复用。
5. “进程自己会退出”要先证实：`GALLERY_SELFTEST=1` 打 PASS 后仍跑满 `GALLERY_SECONDS`。

正确写法与自查命令：`docs/lessons/process-ops.md`；参考实现是 `scripts/nested-session.sh` 的 `compositor_pid()` 与 `stop()`。

## 1. 项目事实

`ui/` 是 Rust workspace：从旧项目重写的 UI 引擎（布局 / 文本 / 绘制 / 平台后端）+ MD3、Fluent、Liquid Glass 三套控件。分层见 `docs/crate-map.md`；依赖边界由 `cargo test -p ui-platform --test platform_boundary` 强制。

## 2. 命令

在 `ui/` 下执行：

| 目的 | 命令 |
|---|---|
| 构建 | `cargo build` |
| 全量测试 | `cargo test --workspace --features ui-text-fontique/system` |
| 零 warning | `cargo build --workspace --tests --examples` |
| clippy | `cargo clippy --workspace --all-targets`（存量分类：`docs/research/clippy-baseline.md`） |
| 分层边界 | `cargo test -p ui-platform --test platform_boundary` |
| 观感 / 性能 | 一律 `--release`；debug 只用于快速迭代 |
| 帧剖析 | `cargo build --release --example gallery --features profile`；读法见 `docs/conventions/workflow.md` §6 |

**图形测试只在嵌套会话里跑**（主会话 compositor 可能正被用户使用）：

```bash
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=slider GALLERY_SECONDS=300 ./ui/target/release/examples/gallery &
scripts/nested-session.sh shot /tmp/a.png
scripts/nested-session.sh stop        # 收尾必须执行
```

`GALLERY_PAGE=<tag>` 直接开某页；`GALLERY_SELFTEST=1` 跑自检。`GALLERY_DUMP` 与自检分开跑（自检会切页，导出拿到的是最后一帧）。控件补全的坑：`docs/control-completion-progress.md` §6。

## 3. 硬边界（不可协商）

**边界由可执行检查维持，不靠自觉**：`ui/crates/ui-platform/tests/platform_boundary.rs`。

- 可移植 crate 源码不得出现 `std::os::unix/windows`、`cfg(target_os)`、`libc::`、`OwnedFd`、`std::process`；`Cargo.toml` 不得依赖平台 crate。
- 每个 crate 必须在 `PORTABLE` 或 `PLATFORM_BOUND` 里；新增平台后端加进 `PLATFORM_BOUND` 并写明理由，不放宽 `PORTABLE`。
- 依赖只能向上；facade crate 只允许 `pub use`；跨 crate 不使用编译期 `cfg`（`gl` 运行时探测，ADR-007 在 `docs/architecture.md`）。

## 4. 编辑时的决策规则

### 4.1 参考实现

渲染器 / UI 引擎 / 布局 / 文本 / 效果这类工业界反复做过的部分，读成熟框架的**源码**，参考其解决的问题与不变量，不抄结构。

| 领域 | 优先参考 |
|---|---|
| UI 引擎（树/布局/命中/事件） | GPUI、egui；Qt 6 / GTK 4 的布局与尺寸协商契约 |
| 渲染器（GPU） | GPUI `gpui_wgpu` + shader、egui `epaint` tessellator、Skia 后端 |
| 采样/缩放 | Skia `SkSamplingOptions`、egui tile atlas |
| 文本排版 | parley、Flutter 文本布局 |
| 模糊/效果 | CSS filter 规范、Flutter `BackdropFilter`、Skia image filters |

不要参考旧项目的 `src/ui/`、`src/render/`、`resample.rs`、effect 全家。源码位置：`docs/reference/README.md`。

### 4.2 注释

一段连续注释上限 **2 行英文**。不写文件历史、bug 叙事、重构日志、工单号、表格、列表、堆栈、协议摘录。禁止复述代码、注释掉的代码、无 issue 链接的 `TODO`、超过一行的装饰分隔线。超过两行就不属于代码：写成测试，或按 §6.2 写笔记。

**用户可见字符串一律走 i18n（Fluent），绝不硬编码。**

完整规则与反例：`docs/conventions/comments.md`。

### 4.3 测试

每条新测试必须回答三问：① 钉住哪个不变量（写进名字）？② 什么改动会让它失败？③ 为什么现有测试抓不到？答案是“将来某天能防回归”的，不要加。

不加：覆盖每个方法、默认跨模块集成、平凡输出快照、同一常量断言自己、为覆盖率而写、需要 GPU / Wayland / 真实时钟（那些留在 `examples/`）。

**名字就是规格**；一条测试一个行为；只喂确定输入；断言本体而不是代理。位置：单元在 crate 内 `#[cfg(test)] mod tests`；跨 crate 契约放 `crates/<focused>/tests/`（少用）。

删低价值测试是正常动作，在提交信息里说一声。永远不要为了让重构通过而弱化断言——判断是重构错了还是断言错了，记下来。

完整版（含反模式表与评审清单）：`docs/conventions/tests.md`。

## 5. 流程纪律

- **改文件一律用编辑工具**（`read`/`edit`/`write`），不用 shell heredoc 拼脚本——会因引号、转义、子进程启动失败而静默不落地。整块搬运 Markdown 可用 `sed -n` 复制，搬完必须 `diff` 校验字节相同。
- **改完读回**：`grep -c '<expected-text>' <file>` 确认落地；跑测试（`0 passed` 是空集合上的 ok，不是验证）；断言检查点**先看它红，再看它绿**。
- **三条判据**：① 实物到了 ≠ 接线到了（新增/搬移/重命名后问“接线那一行在哪”）；② 口头合理的机制 ≠ 事实，能跑就跑；③ 探针没触发有两种可能——代码没执行，或探针插错了函数。

细则：`docs/conventions/workflow.md`。

## 6. 笔记与推翻旧决定

### 6.1 笔记预算

超出注释预算且无法写成测试的，每个主题最多一条短笔记（一屏），严格四段：**症状 → 根因 → 修法 → 如何验证**。笔记预算从零开始（当前只有 `docs/notes/decl-cache.md`），过期就删——错的笔记比没有笔记更糟。新增时同批加进 `docs/notes/README.md`。

### 6.2 推翻旧项目的决定

写进 `docs/decisions/<slug>.md`（ADR，半页：旧做法、为什么不行、改用什么、哪条检查抓回归），代码里最多留一行指针。

## 7. 遇到什么情况读什么

| 触发条件 | 读 |
|---|---|
| 改任何子系统前 | `docs/lessons/design-errors.md`（必扫） |
| 引用旧项目材料 | `docs/lessons/legacy-project.md` §1 分类表、§3 不信任案例 |
| 分层 / ADR / 迁移史 | `docs/architecture.md`、`docs/crate-map.md`、`docs/migration.md` |
| 工作流 / 注释 / 测试细则 | `docs/conventions/workflow.md`、`docs/conventions/comments.md`、`docs/conventions/tests.md` |
| 引擎缺口 | `docs/engine-gaps.md` |
| 呈现归属 / 过渡 / 尺寸动画 | `docs/present-target-design.md`、`docs/transition-design.md` |
| 控件补全与坑 | `docs/control-completion-progress.md` |
| 待实现设计 | `docs/layout-engine-design.md`、`docs/popover-tooltip-design.md`、`docs/pointer-model-design.md`、`docs/style-object-design.md`、`docs/icon-text-design.md`、`docs/cursor-system-design.md`、`docs/style-system-design.md`、`docs/ime-text-design.md`、`docs/shortcut-system-design.md` |
| 外部框架源码 / 调研 | `docs/reference/README.md`、`docs/research/` |

**引用文档时说明“为什么/何时读”，不要只甩路径。** 只给路径的引用，agent 常常忽略。
```

**改动依据**：根文件压缩至约 130 行，符合 AGENTS.md 规范建议的 100–150 行区间；完整规则下沉到 `docs/` 参考文档，对应“渐进披露 + 模块化”模式；每个文档指针附加触发条件，避免“盲引用”反模式；进程操作与旧项目信任放在最前，因为它们是该仓库最高频的不可逆错误来源。
