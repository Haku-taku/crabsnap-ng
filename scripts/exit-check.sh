#!/usr/bin/env bash
# Close-time probe: run the gallery with exit tracing, close its window by hand, read both clocks.
# 关闭耗时探针：带退出追踪运行 gallery，手动关掉窗口，同时读应用侧与合成器侧的时间。
#
#   scripts/exit-check.sh                       # window stays until you close it
#   GALLERY_SECONDS=20 scripts/exit-check.sh    # or quit by itself after 20 s
#
# Two things are measured at once: the app's teardown stages, and how long the compositor
# takes to answer its control socket (a freeze becomes a number with a wall-clock stamp).
# 同时测两件事：应用拆除的各个阶段，以及合成器回应控制套接字的耗时
# （冻结因此变成一个带墙钟时间戳的数字）。
#
# If the app stalls, the watchdog prints "still in <stage>" plus every thread's kernel wait
# channel once a second; `scripts/exit-stacks.sh` then dumps real stacks.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

bin=ui/target/release/examples/gallery
log=${EXIT_LOG:-/tmp/gallery-exit.log}
pid_file=${EXIT_PID:-/tmp/gallery-exit.pid}
watch_log=${EXIT_WATCH_LOG:-/tmp/gallery-exit-watch.log}

[ -x "$bin" ] || cargo build --release --example gallery || exit 1

: >"$log"
echo "log: $log   pid file: $pid_file"
echo "close the window when you want the measurement; Ctrl-C abandons the run"

watcher=""
if command -v hyprctl >/dev/null 2>&1; then
    WATCH_LOG="$watch_log" WATCH_INTERVAL=0.1 scripts/compositor-watch.sh >/dev/null 2>&1 &
    watcher=$!
fi

EXITDBG=1 GALLERY_SECONDS="${GALLERY_SECONDS:-3600}" "$bin" > >(tee "$log") 2>&1 &
pid=$!
echo "$pid" >"$pid_file"
trap 'kill "$pid" ${watcher:+"$watcher"} 2>/dev/null' INT TERM
wait "$pid"
status=$?
trap - INT TERM
sleep 0.2
if [ -n "$watcher" ]; then
    kill "$watcher" 2>/dev/null
    wait "$watcher" 2>/dev/null
fi

echo
echo "exit status $status"
if grep -q '^\[exit\]' "$log"; then
    echo "--- app timeline (ms since the close event) ---"
    grep '^\[exit\]' "$log" | tail -n 20
    grep '^teardown:' "$log" || true
else
    echo "no [exit] lines: the close event never reached the app"
fi

if [ -n "$watcher" ] && [ -s "$watch_log" ]; then
    close=$(grep -m1 -E 'toplevel-close|requested-close' "$log" | sed -E 's/.*t=([0-9]+).*/\1/')
    echo "--- compositor round trips (epoch ms / ms / exit code) ---"
    echo "samples: $(wc -l <"$watch_log")   worst: $(sort -k2 -n "$watch_log" | tail -1 | awk '{print $2}') ms"
    awk -v c="${close:-0}" '
        $2 > 100 { printf "  stall %s ms at t=%s%s\n", $2, $1, ($1 >= c - 500 && $1 <= c + 4000 ? "  <- during the exit" : "") }
    ' "$watch_log"
    awk -v c="${close:-0}" '$1 >= c - 500 && $1 <= c + 4000 { if ($2 > m) m = $2 } END {
        printf "compositor around the close: worst %d ms%s\n", m + 0,
            (m > 100 ? "  <- IT FROZE" : " (clean)")
    }' "$watch_log"
else
    echo "no compositor samples (hyprctl unavailable?)"
fi
