# Graphical Tests

A screenshot is evidence only if it came from the nested session. Your own
compositor is in use, and a window often maps after scripted input has already
fired: the process lives, `hyprctl clients` is empty, and the capture shows the
desktop.

## Recipe

```sh
scripts/nested-session.sh start
eval "$(scripts/nested-session.sh env)"
GALLERY_PAGE=slider GALLERY_SECONDS=300 ./ui/target/release/examples/gallery &
scripts/nested-session.sh shot /tmp/a.png
scripts/nested-session.sh stop
```

`stop` is mandatory and belongs in the same command sequence as `start`. The
script records the compositor's pid and instance signature; `stop` checks
`/proc/<pid>/comm` before killing.

Other subcommands: `clients` lists the nested windows, `cursor X Y` moves the
pointer.

## Knobs

| Variable | Form | Does |
| --- | --- | --- |
| `GALLERY_PAGE` | `<tag>` | Opens that page directly |
| `GALLERY_SECONDS` | seconds (default 300) | Lifetime of the run |
| `GALLERY_CLOSE_AFTER` | seconds | Closes the window itself, for exit-path tests |
| `GALLERY_SWITCH` | `<frame>:<tag>,...` | Switches pages mid-run; times cold and warm switches |
| `GALLERY_SELFTEST` | `1` | Runs the self-test, then keeps running |
| `GALLERY_DUMP` | `1` | Writes the frame to disk; needs the GPU presenter |
| `GALLERY_SS` | `1..4` | Renders at n× and box-averages down |
| `GALLERY_CAPTURE_SCALE` | float | Pins glass capture density instead of downsampling |
| `GALLERY_DARK` | `0` or `1` | Seeds the appearance |
| `GALLERY_CPU` | set | Uses the CPU presenter |
| `GALLERY_WALLPAPER` | path | Replaces the gallery backdrop |
| `GALLERY_STALL` | seconds | Captures stacks and exits if the loop stops turning |
| `GALLERY_PERF` | `<wait>:<samples>` | Samples the loop and prints a report |
| `GALLERY_FRAMELOG` | set | Prints each frame's CPU time and page |
| `GALLERY_HEARTBEAT` | set | Prints a periodic liveness line |
| `GALLERY_UNCAPPED` | set | Removes the one-frame-per-vblank gate |

The gallery's language is a command-line argument, not a knob: pass
`--locale auto` or `--locale <tag>`, and an unknown tag falls back to English.
It seeds the `Language` every page builds under; a locale is not a frame knob
because it changes the catalogue the whole window is built with.

## Scripted input

All coordinates are in window pixels.

| Variable | Form | Does |
| --- | --- | --- |
| `GALLERY_FOCUS` | `x,y` | A plain click: press and release, no drag. This is the one that clicks. |
| `GALLERY_CLICK` | `x,y` | Press, drag 300px away, release. It is a drag; it never produces a click. |
| `GALLERY_PRESS` | `x,y` | Holds a synthetic press, no release: captures press-only ink |
| `GALLERY_HOLD` | `x,y,dx,dy` | Presses, moves by `dx,dy`, never releases |
| `GALLERY_SWAY` | `x,y,dx,ms` | Presses, sweeps to `x+dx` every `ms`, never releases |
| `GALLERY_DRAG` | `x1,y1,x2,y2` | Press, drag, release |
| `GALLERY_DRAGS` | `x1,y1,x2,y2;...` | Several drags a few frames apart |
| `GALLERY_WHEEL` | `x,y,delta` | One wheel notch |
| `GALLERY_KEY` | `ctrl+a,ctrl+c` | Chords in order; the last part of each names the key |
| `GALLERY_TYPE` | text | Types the text into whatever holds the caret |
| `GALLERY_RIGHT` | `x,y` | Opens the context menu a right click would |
| `GALLERY_CLIPBOARD` | `copy:<text>` or `paste` | Seeds the clipboard or asks for it |

## Traps

**Scripted input bypasses the driver gate.** The gallery's scripted path builds
frames with `build_with_dt` directly, so a bug in frame demand
(`pointer_affects_frame`, `take_dispatched`) does not reproduce here. A drag
that shows live selection on screen through the script can still be dropped
for real users. Pin that class with a unit test that asserts
`take_dispatched()`.

**A click is not `GALLERY_CLICK`.** Use `GALLERY_FOCUS` when you need a click;
use `GALLERY_CLICK` or `GALLERY_DRAG` only when you need a drag.

**Coordinates come from the screenshot, not from the layout.** Read the page,
capture it, and pick the pixel. Then confirm the hit: `GALLERY_SELFTEST` probes
the tree and prints rectangles for the same reason.

**The nested compositor runs at about 4fps.** Wait longer than feels necessary
before the capture, and crop plus magnify the region you are judging. A
whole-window screenshot at 1× hides a 3px caret.

**The self-test and a dump do not mix.** The self-test switches pages; a dump
taken in the same run captures the last frame of the wrong page.

**The self-test does not exit.** It prints `PASS` and keeps running for
`GALLERY_SECONDS`. See [Process Operations](./process-operations.md).

**A press placed from a rect read while the page moves lands elsewhere.** A caret
reveal moves the engine's scroll without touching the app's scroll request, and a
rebuilt document takes a frame or two to reach the tree the hit test reads, so a
point built from a rect read too early names a byte that has since slid away.
Settle frames before reading the rect, and correct a reveal by the content's own
visual movement rather than by the request map: `click_id` and `reveal_page_point`
in `selftest/mod.rs` do both.

**Present timings are invalid here.** The nested compositor does not advertise
`wp_linux_drm_syncobj_manager_v1`, so the present path falls back to
`glFinish`. Measure present on the host session, or state which branch you
measured.
