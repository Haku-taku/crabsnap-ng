#!/usr/bin/env bash
# Does the screen stop updating when the gallery exits? A second instance answers, not your eyes.
# 关窗时屏幕会不会停住？由第二个实例回答，不用你的眼睛。
#
#   scripts/freeze-test.sh                        # baseline teardown
#   scripts/freeze-test.sh EXIT_KEEP_BUFFERS=1    # any variant, passed as NAME=value
#
# A witness instance keeps animating and stamps its loop once a second. Close the measured
# window by hand; the script then reports the longest gap between witness stamps and when it
# happened, relative to the close. A gap of seconds is the freeze, measured.
# 见证实例持续动画，每秒给循环盖一次时间戳。手动关掉被测窗口；脚本报告见证戳之间最长的
# 间隔及其相对关闭时刻的位置。几秒的空档就是冻结本身，而且有了数字。
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

app=ui/target/release/examples/gallery
witness_log=/tmp/freeze-witness.log
subject_log=/tmp/freeze-subject.log
[ -x "$app" ] || cargo build --release --example gallery || exit 1

# The witness must keep presenting: an idle window would tick regardless of the output.
# 见证窗口必须持续出帧：空闲窗口无论输出如何都会继续盖戳。
GALLERY_HEARTBEAT=1 GALLERY_PAGE=buttons GALLERY_PRESS=500,300 GALLERY_SECONDS=3600 \
    "$app" >"$witness_log" 2>&1 &
witness=$!
trap 'kill "$witness" 2>/dev/null' EXIT INT TERM
sleep 2

echo "a witness window is already open; a second one is opening now"
echo "close the NEW window (the focused one); Ctrl-C abandons the run"
env "$@" EXITDBG=1 GALLERY_SECONDS=3600 GALLERY_PAGE="${GALLERY_PAGE:-slider}" \
    GALLERY_SWAY="${GALLERY_SWAY:-500,300,200,400}" "$app" >"$subject_log" 2>&1
kill "$witness" 2>/dev/null
trap - EXIT INT TERM
sleep 0.3

close=$(grep -m1 -E 'toplevel-close|requested-close' "$subject_log" | sed -E 's/.*t=([0-9]+).*/\1/')
echo
echo "--- subject (the window you closed) ---"
grep -E '^\[exit\]|^frames |^dmabuf' "$subject_log" | tail -8

echo "--- witness heartbeat gaps (s between stamps, at epoch ms) ---"
awk -v c="${close:-0}" '
    /^\[hb\]/ {
        t = $2; sub("t=", "", t)
        if (last) {
            gap = (t - last) / 1000
            if (gap > worst) { worst = gap; worst_at = last }
            if (gap > 1.5) printf "  %.1f s gap starting at t=%s%s\n", gap, last,
                (last >= c - 1000 && last <= c + 5000 ? "  <- during the exit" : "")
        }
        last = t
    }
    END {
        if (!last) { print "no witness heartbeats: the witness never ran"; exit }
        printf "longest gap: %.1f s%s\n", worst + 0,
            (worst > 1.5 ? "  <- THE SCREEN FROZE" : " (screen kept updating)")
    }
' "$witness_log"
