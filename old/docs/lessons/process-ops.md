# 进程操作：为什么不能按命令行模式找/杀进程

> `AGENTS.md` §5.1 的五条规则由此而来。这里放**证据与推导**，规则本身不重复。
> 同类事故的另一面（"kill 掉示例进程就干净了"）记在 `docs/lessons/design-errors.md`。

## 1. 症状

两种，看起来像别的问题：

| 操作 | 表现 | 根因 |
|---|---|---|
| `pgrep -f "Hyprland -c $CONFIG" \| head -1` | 拿到的 pid 读 `/proc/<pid>/fd` 是空的，脚本报"找不到 socket" | 匹配到的是**包装 shell**，不是合成器 |
| `pkill -f "target/release/examples/gallery"` | 自己的后台任务 shell 被杀（exit 144），后续步骤全部失踪 | 该 shell 的命令行里就含有这个字符串 |

## 2. 为什么

`pgrep -f` / `pkill -f` 匹配的是**整条命令行**，而不是进程名。于是：

- 发起这条命令的 shell（`bash -c "…"`、任务运行器、`cargo run` 的包装层）命令行里
  **就含有**那个模式，必然被匹配；
- `| head -1` 取到的往往是包装层而不是目标进程；
- 拿错 pid 后不是"多杀了一个无关进程"，而是**一路错下去**：读错 fd 表、判断错存活状态，
  错误形态还会伪装成"文件没生成""socket 不存在"。

## 3. 正确写法

**启动后立刻拿 pid**（不做事后 `pgrep`）：`setsid` 可能 fork，所以沿
`/proc/<pid>/task/<pid>/children` 下钻，每步用 `/proc/<pid>/comm` 核对进程名。
参考实现：`scripts/nested-session.sh` 的 `compositor_pid()`。

**杀进程**：先 `ps -eo pid,args` 看清楚，再 `kill <pid>`。从文件里读到 pid 时，
动手前核对 `/proc/<pid>/comm`——pid 会复用，杀掉一个恰好复用了该号的无关进程不可逆。
`scripts/nested-session.sh` 的 `stop()` 就是这么挡的。

**必须用模式时**：加方括号让模式不匹配自己（`'examples/galler[y]'`，`grep` 的 `[y]` 技巧同理），
且模式要覆盖所有启动路径——`cargo run` 起的进程命令行是 `target/release/...`，没有 `ui/` 前缀。

## 4. 怎么验证自己没踩

```bash
# 只读：看清有几个进程、各自的 pid 与命令行
ps -eo pid,ppid,args | grep -E 'examples/galler[y]'
# 确认 pid 真的是你以为的那个进程
cat /proc/<pid>/comm
```

清理后**复查一遍**：`GALLERY_SELFTEST=1` 打印 PASS 之后仍会跑满 `GALLERY_SECONDS`
（默认 300s），"测试结束"不等于"进程退出"。

## 5. `set -euo pipefail` 会让探测脚本**静默退出**

`nested-session.sh start` 曾返回 1 且**一行输出都没有**：合成器起得来，脚本却在中途死掉。
根因不是进程，是 shell 语义：

```bash
fds=$(ls -l "/proc/$pid/fd" | grep -oE 'wayland-[0-9]+' | sort -u)
```

socket 还没出现时 `grep` 无匹配、退出码 1，`pipefail` 把整条管道判为失败，
`set -e` 便在**赋值语句**上终止脚本——没有消息，因为它不是 `if`/`||` 的一部分。
同理 `[[ -n "$x" ]] && echo ...` 单独成句时，`$x` 为空也会以同样方式退出。

规则：**轮询里的每条管道都写 `|| true`**，`[[ ]] && cmd` 一律展开成 `if [[ ]]; then … fi`。
自查：脚本"什么都没说就退出"时，先 `bash -x` 跑一遍，看最后一条被执行的语句。

## 6. 新建文件会**静默覆盖**同名文件

写 `src/session.rs` 时没有先看这个路径，于是一个已有的 `session` 模块（Wayland 连接与
全局绑定）被整个替换掉，编译器直到 `window.rs` 找不到 `connect()` 才报错——报错位置与
真正被覆盖的文件不在同一处。

规则：**新建模块前先 `ls`/`grep` 确认同名文件不存在**；名字已被占用就换名。
这里顺带做了一次命名清理：连接/全局那块改叫 `conn.rs`（`conn::connect()`），
`session.rs` 留给「窗口 + 呈现器 + 循环」。
恢复来源：`~/.claude/projects/<proj>/*.jsonl` 里此前的读取结果，而不是重写。
