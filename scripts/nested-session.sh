#!/usr/bin/env bash
# Nested Hyprland for manual UI tests: an isolated compositor with no other windows,
# no decoration plugins, and a recorded instance signature.
# 用于手工 UI 测试的嵌套 Hyprland：没有其他窗口、没有装饰插件的隔离 compositor，
# 并记录 instance 指纹。
#
#   scripts/nested-session.sh start     # start it, write the signature to a file
#   scripts/nested-session.sh env       # print export lines for running a client inside
#   scripts/nested-session.sh shot F    # screenshot the nested session to F
#   scripts/nested-session.sh cursor X Y
#   scripts/nested-session.sh stop
#
# The nested compositor is sessionless, so it does not advertise
# wp_linux_drm_syncobj_manager_v1: explicit sync only runs on the host session.
# 嵌套 compositor 是无会话后端，不广播 wp_linux_drm_syncobj_manager_v1：
# 显式同步只能在宿主会话里跑到。
set -euo pipefail

STATE="${CRABSNAP_NESTED_DIR:-/tmp/crabsnap-nested}"
CONFIG="${CRABSNAP_NESTED_CONFIG:-$HOME/.config/hypr/hyprlandd.lua}"
SIG_FILE="$STATE/instance.txt"
DISPLAY_FILE="$STATE/wayland_display.txt"
PID_FILE="$STATE/hyprland.pid"
LOG="$STATE/hyprland.log"

mkdir -p "$STATE"

running() {
    [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

# The compositor's own pid. `$!` is the `setsid` we launched: it execs in place when it can
# and forks when it is already a group leader, so follow it down to the process named Hyprland.
# 合成器自身的 pid。`$!` 是我们启动的 `setsid`：能 exec 就原地替换，已是组长时才 fork，
# 因此沿子进程走到名为 Hyprland 的那个。
#
# Never resolve it with `pgrep -f "<cmdline>": that pattern also matches the shell that ran
# this script (and any wrapper), and `head -1` then reads the wrong process's fd table.
# 绝不要用 `pgrep -f "<命令行>"` 解析：该模式同样匹配跑本脚本的 shell（以及任何包装层），
# `head -1` 于是读到别的进程的 fd 表。
compositor_pid() {
    local pid=$1 comm child i
    for i in $(seq 1 50); do
        comm=$(cat "/proc/$pid/comm" 2>/dev/null || true)
        if [[ "$comm" == "Hyprland" ]]; then
            echo "$pid"
            return 0
        fi
        child=$(awk '{print $1}' "/proc/$pid/task/$pid/children" 2>/dev/null || true)
        if [[ -n "$child" ]]; then
            pid=$child
        else
            sleep 0.1
        fi
    done
    return 1
}

start() {
    if running; then
        echo "already running: $(cat "$SIG_FILE")"
        return 0
    fi
    # Snapshot the runtime dir so the new signature and socket can be identified.
    # 先记录运行时目录，才能认出新的指纹与 socket。
    local before after
    before=$(ls "${XDG_RUNTIME_DIR:?}/hypr/" 2>/dev/null | sort || true)
    local sockets_before
    sockets_before=$(ls "${XDG_RUNTIME_DIR:?}" 2>/dev/null | grep -E '^wayland-[0-9]+$' | sort || true)

    setsid env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        Hyprland -c "$CONFIG" >"$LOG" 2>&1 &
    local pid
    pid=$(compositor_pid "$!") || {
        echo "the compositor did not come up; see $LOG" >&2
        return 1
    }
    echo "$pid" > "$PID_FILE"

    local i new_sig=""
    for i in $(seq 1 150); do
        sleep 0.2
        if [[ -z "$new_sig" ]]; then
            after=$(ls "$XDG_RUNTIME_DIR/hypr/" 2>/dev/null | sort || true)
            # `|| true` on every pipeline: with `pipefail` a no-match `grep`/`head` exit
            # would trip `set -e` and kill this script silently.
            # 每条管道都要 `|| true`：`pipefail` 下 grep/head 无匹配的退出码会触发
            # `set -e`，让脚本静默退出。
            new_sig=$(comm -13 <(echo "$before") <(echo "$after") | head -1 || true)
            if [[ -n "$new_sig" ]]; then
                echo "$new_sig" > "$SIG_FILE"
            fi
        fi
        # The signature appears before the socket is open, so keep polling for both.
        # 指纹比 socket 先出现，因此两者都要轮询。
        [[ -n "$new_sig" ]] || continue
        # Ask the compositor's own fd table: socket names get reused, so a
        # before/after diff of the runtime dir silently finds nothing.
        # 问合成器自己的 fd 表：socket 名会被复用，
        # 对运行时目录做前后差集会静默地找不到东西。
        local fds new_disp
        fds=$(ls -l "/proc/$pid/fd" 2>/dev/null | grep -oE 'wayland-[0-9]+' | sort -u || true)
        # Prefer a socket that did not exist before this launch, so a nested compositor
        # that also holds the host's socket cannot be mistaken for ours.
        # 优先取本次启动前不存在的 socket，免得嵌套合成器同时持有宿主的 socket 时认错。
        new_disp=$(comm -13 <(echo "$sockets_before") <(echo "$fds") 2>/dev/null | head -1 || true)
        [[ -n "$new_disp" ]] || new_disp=$(echo "$fds" | head -1)
        if [[ -n "$new_disp" ]]; then
            echo "$new_disp" > "$DISPLAY_FILE"
            echo "started: signature $(cat "$SIG_FILE"), display $(cat "$DISPLAY_FILE")"
            return 0
        fi
    done
    echo "failed to start; see $LOG" >&2
    return 1
}

need() {
    [[ -f "$SIG_FILE" ]] || { echo "not started" >&2; exit 1; }
    SIG=$(cat "$SIG_FILE")
    export WAYLAND_DISPLAY="$(cat "$DISPLAY_FILE")"
    export HYPRLAND_INSTANCE_SIGNATURE="$SIG"
}

stop() {
    if running; then
        local pid
        pid=$(cat "$PID_FILE")
        # A recycled pid must not be killed: confirm it is still the compositor we started.
        # 复用的 pid 不能杀：先确认它仍是当初启动的那个合成器。
        if [[ "$(cat "/proc/$pid/comm" 2>/dev/null || true)" == "Hyprland" ]]; then
            kill "$pid" 2>/dev/null || true
        fi
    fi
    rm -f "$PID_FILE" "$SIG_FILE" "$DISPLAY_FILE"
    echo stopped
}

case "${1:-}" in
    start) start ;;
    env)
        need
        echo "export WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
        echo "export HYPRLAND_INSTANCE_SIGNATURE=$SIG"
        ;;
    shot)
        need
        grim "${2:?usage: shot FILE}"
        ;;
    cursor)
        need
        hyprctl -i "$SIG" dispatch "hl.dsp.cursor.move({x=${2:?x}, y=${3:?y}})" >/dev/null
        ;;
    clients)
        need
        hyprctl -i "$SIG" clients
        ;;
    stop) stop ;;
    *)
        echo "usage: $0 {start|env|shot FILE|cursor X Y|clients|stop}" >&2
        exit 2
        ;;
esac
