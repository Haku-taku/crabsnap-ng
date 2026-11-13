#!/usr/bin/env bash
# Dump every thread's stack of the running gallery started by `exit-check.sh`.
# 导出 `exit-check.sh` 启动的 gallery 各线程栈。
#
# Works because the app calls prctl(PR_SET_PTRACER, ANY) when EXITDBG is set, which is what
# `yama.ptrace_scope=1` otherwise refuses. Names stay in Rust v0 mangling: readable enough.
# 之所以可用，是应用在 EXITDBG 下设了 PR_SET_PTRACER，否则 yama 会拒绝附加；
# 名字保持 Rust v0 修饰，仍可读。
set -uo pipefail

pid_file=${EXIT_PID:-/tmp/gallery-exit.pid}
pid=$(cat "$pid_file" 2>/dev/null) || { echo "no pid file: $pid_file"; exit 1; }
if ! kill -0 "$pid" 2>/dev/null; then
    echo "pid $pid is gone — the run already exited"
    exit 1
fi
eu-stack -p "$pid" || echo "eu-stack failed (is elfutils installed?)"
