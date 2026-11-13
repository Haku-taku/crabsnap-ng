#!/usr/bin/env bash
# A/B the exit path: run the gallery N times per variant, and report both clocks.
# A/B 退出路径：每个变体跑 N 次，同时报告应用侧与合成器侧的时间。
#
#   scripts/exit-ab.sh [runs]        # default 3 runs per variant
#
# The app closes itself (`GALLERY_CLOSE_AFTER`), so this needs no hand on the mouse; the
# teardown it measures is the same one a real close runs. A compositor stall shows up as a
# `worst` far above the app's own `exit`.
# 应用自己关窗（`GALLERY_CLOSE_AFTER`），无需动手；测的拆除与真实关窗同一条路径。
# 合成器停顿会表现为 `worst` 远高于应用自己的 `exit`。
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

runs=${1:-3}
page=${GALLERY_PAGE:-slider}
app=ui/target/release/examples/gallery
watch_log=/tmp/exit-ab-watch.log
: >"$watch_log"

[ -x "$app" ] || cargo build --release --example gallery || exit 1

WATCH_LOG="$watch_log" WATCH_INTERVAL=0.1 scripts/compositor-watch.sh >/dev/null 2>&1 &
watcher=$!
trap 'kill "$watcher" 2>/dev/null' EXIT INT TERM
sleep 0.5

printf '%-13s %-7s %10s %14s\n' variant run app-exit compositor-max
for variant in default keep-egl keep-buffers no-detach; do
    envs=()
    case $variant in
        keep-egl) envs=(EXIT_KEEP_EGL=1) ;;
        keep-buffers) envs=(EXIT_KEEP_BUFFERS=1) ;;
        no-detach) envs=(EXIT_NO_DETACH=1) ;;
    esac
    for run in $(seq 1 "$runs"); do
        log=/tmp/exit-ab-$variant-$run.log
        env "${envs[@]}" EXITDBG=1 GALLERY_CLOSE_AFTER=6 GALLERY_SECONDS=120 \
            GALLERY_PAGE="$page" GALLERY_SWAY=500,300,200,400 \
            "$app" >"$log" 2>&1
        close=$(grep -m1 -E 'toplevel-close|requested-close' "$log" |
            sed -E 's/.*t=([0-9]+).*/\1/')
        total=$(grep -m1 'main-return' "$log" | sed -E 's/.*\+ *([0-9.]+)ms.*/\1/')
        worst=$(awk -v c="${close:-0}" '$1 >= c - 500 && $1 <= c + 4000 { if ($2 > m) m = $2 } END { print m + 0 }' "$watch_log")
        printf '%-13s %-7s %8s ms %11s ms\n' "$variant" "$run" "${total:-?}" "$worst"
    done
done
