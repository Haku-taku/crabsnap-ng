#!/usr/bin/env bash
# Log how long the compositor takes to answer, so a freeze shows up as a number.
# 记录合成器的应答耗时，冻结因此变成一个数字。
#
# Run it in a second terminal next to `exit-check.sh`, close the window, then Ctrl-C here:
# the summary names every stall longer than 100 ms with its wall-clock stamp, which lines up
# with the `[exit] t=<epoch ms>` stamps in the app log.
# 在第二个终端与 `exit-check.sh` 并排运行，关掉窗口后在此 Ctrl-C：摘要列出所有超过 100ms
# 的停顿及其墙钟时间，可与应用日志里的 `[exit] t=<epoch ms>` 对齐。
set -uo pipefail

log=${WATCH_LOG:-/tmp/compositor-watch.log}
interval=${WATCH_INTERVAL:-0.05}
: >"$log"
echo "logging to $log every ${interval}s; Ctrl-C prints the summary"

poll() {
    local t0 t1
    t0=$(date +%s%N)
    timeout 5 hyprctl version >/dev/null 2>&1
    local ok=$?
    t1=$(date +%s%N)
    echo "$((t1 / 1000000)) $(((t1 - t0) / 1000000)) $ok" >>"$log"
}

summary() {
    echo
    echo "--- stalls over 100 ms (epoch ms / round trip ms / exit code) ---"
    awk '$2 > 100' "$log" || true
    echo "--- worst round trips ---"
    sort -k2 -n "$log" | tail -5
}

trap 'summary; exit 0' INT TERM
while true; do
    poll
    sleep "$interval"
done
