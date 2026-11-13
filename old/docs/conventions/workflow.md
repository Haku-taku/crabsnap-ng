# 工作流程纪律

> `AGENTS.md` §5 的细则。规则本身在 `AGENTS.md` §5 / §5.1，这里放**怎么做**与**为什么**。

## 1. 改文件一律用编辑工具，不要用 shell heredoc 拼脚本

用 `read` / `edit` / `write`，不要 `cat > file <<EOF`。

heredoc 会因引号、反引号、转义、子进程启动失败而**静默不落地**，且失败形态不统一；
编辑工具失败时会明确报错并回显当前内容。历史上几乎每一轮都在这上面浪费一次往返。

> 例外：**整块搬运**已有的 Markdown 段落（如把一张表从 `AGENTS.md` 移到 `docs/`）可以用
> `sed -n 'A,Bp'` 直接复制，前提是搬完 `diff` 校验字节相同——重打一遍的风险比复制更高。

## 2. 改完读回：确认改动真的落地

```bash
# 最重要的一条：edit 静默不落地已浪费过数小时
grep -c '<expected-text>' <file>
# 跑测试：`0 passed` 是空集合上的 ok，不是验证
cargo test --workspace --features ui-text-fontique/system
# 断言检查点**确实变红**：临时破坏 → 看它红 → 恢复 → 看它绿
```

## 3. 三条判据（人的失误模式，不是代码事实）

1. **实物到了 ≠ 接线到了**：文件存在 ≠ 被 `mod` 引用；`git add` ≠ 已提交。
   任何"搬移/新增/重命名"之后，问"接线的那一行在哪"。
2. **口头合理的机制 ≠ 事实**。能跑的就跑一遍，跑不了的说"我推测"。
3. **探针没触发有两种可能**：代码没执行，或者**探针插错了函数**。插之前先 `grep -c '<函数名>'`。

## 4. 进程操作

按命令行模式 `pgrep -f` / `pkill -f` 会匹配到发起命令的 shell 自身。证据、正确写法与
自查命令见 [`../lessons/process-ops.md`](../lessons/process-ops.md)。

## 5. 本文件自身也曾"文档与事实不符"

早期版本引用了 `scripts/check-edit-landed.sh`、`scripts/run-tests.sh`、`arch.toml`、
`cargo xtask arch`——**这些文件从未存在过**。已改为上面的真实命令与 `AGENTS.md` §3 的
可执行检查。写文档时凡提到脚本或命令，先确认它真的存在。

## 6. 性能剖析：hotpath，只在 `profile` 特性下存在

帧的每个阶段都有 `timed!` 块（`ui-platform/src/profile.rs`），`hotpath` 是可选依赖，
不开特性时它一个字节也不进构建：

```bash
cargo build --release --example gallery --features profile
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=slider GALLERY_SECONDS=6 ./ui/target/release/examples/gallery
scripts/nested-session.sh stop
```

退出时按标签打印一张表（calls / avg / p95 / total）。**必须用 `--release`**：debug 下
这些数字没有意义。要测某一页的构建成本，用 `GALLERY_SELFTEST=1` 先跑一遍自检——它会把
每页各走若干帧，`build_scripted` 就是那批帧的合计。

读表的三条经验：

1. 先看 **calls**：一个每帧一次的阶段却有上万次调用，说明它在内层循环里；
2. 时间按标签**嵌套**统计，`text ⊃ linebreak ⊃ slice ⊃ shape`，子项占比高就往下钻；
3. 计时只覆盖被包住的块——`paint` 只收集图元（几十微秒），真正光栅在 GPU 进程里，
   别把 `paint` 很小当成「绘制不花钱」。

要量引擎的**吞吐上限**而不是按刷新率的成本，加 `GALLERY_UNCAPPED=1`：它只去掉
「每个 vblank 一帧」的门，帧需求判断照旧。代价是跳过的帧 compositor 既不显示也不释放，
所以这个模式只用来压测，不能用来看观感。

**嵌套会话缺一个协议，present 侧的测量在那里不可信**：主会话 Hyprland 提供
`wp_linux_drm_syncobj_manager_v1`，嵌套里没有（`wayland-info` 对一下就知道），于是
`present` 走的是 `glFinish` 回退分支。要在嵌套里比 present，先确认比的是哪条分支。

## 7. 退出耗时：`EXITDBG=1` 与两个脚本

关闭窗口到进程结束之间的每一步都打点（`ui_platform::exit_probe`）：时间从
`toplevel-close` 起算，卡住时看门狗每秒重报当前阶段，并列出每个线程的 `comm/wchan`。

```bash
scripts/exit-check.sh          # 关窗即测；结束打印时间线
scripts/exit-stacks.sh         # 卡住时另开一次，导出各线程真实栈
scripts/freeze-test.sh         # 见证窗口 + 被测窗口；关被测窗口，报告停摆与心跳
scripts/compositor-watch.sh    # 另开一个终端：合成器应答耗时，冻结会变成 >1000ms 的数字
```

三条用于二分退出路径的诊断开关（正常构建里都是关闭的）：

| 开关 | 跳过什么 | 用来排除 |
|---|---|---|
| `EXIT_NO_DETACH=1` | 关窗前的 `attach(null)+commit` | 「表面还挂着 dma-buf 让 compositor 同步」 |
| `EXIT_KEEP_BUFFERS=1` | 销毁 GBM/dma-buf | 「销毁缓冲触发驱动级等待」 |
| `EXIT_KEEP_EGL=1` | `eglTerminate` | 「拆 EGL display 拖住共享 GPU」 |
| `EXIT_KEEP_GL=1` | 整个 EGL 上下文的拆除（含 `make_current(None)`/`destroy_context`） | 把停顿进一步钉到某一次 EGL 调用上 |
| `EXIT_FINISH=1` | 反过来：拆除**之前**加一次 `glFinish` | 「退出时 GPU 队列还有积压，compositor 排在它后面」——`gpu: finish` 的耗时就是积压量 |

`EXITDBG` 同时让应用调 `prctl(PR_SET_PTRACER, ANY)`，否则 `yama.ptrace_scope=1` 会拒绝
`eu-stack` 附加。栈名保持 Rust v0 修饰，够读即可。启动噪音（configure、缩放、色彩管理）
只在 `WAYLAND_TRACE=1` 时输出。
