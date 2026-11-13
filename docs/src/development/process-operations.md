# Process Operations

Starting, finding, and killing your own processes. Every rule here comes from
a failure that looked like something else: a missing socket, a missing file, a
task runner that silently stopped.

## Do not find your process by command line

`pgrep -f` and `pkill -f` match the **entire command line**, not the process
name. The shell, task runner, or `cargo run` wrapper that issued the command
contains the pattern, so it matches too. `... | head -1` then hands you the
wrapper.

Two observed failures:

| Command | Symptom | What actually happened |
| --- | --- | --- |
| `pgrep -f "Hyprland -c $CONFIG" \| head -1` | `/proc/<pid>/fd` is empty; "socket not found" | The pid was the wrapping shell |
| `pkill -f "target/release/examples/gallery"` | Exit 144; later steps never ran | The task's own shell was killed |

The damage is not "one extra process died". Every later step reads the wrong
`/proc` entry and the error surfaces as a missing file or a missing socket.

### Correct order

1. Take the pid from `$!` at launch. Do not look it up later.
2. `setsid` may fork. Walk `/proc/<pid>/task/<pid>/children`, checking
   `/proc/<pid>/comm` at each step. `scripts/nested-session.sh` implements this
   in `compositor_pid()`.
3. Kill by explicit pid after reading `ps -eo pid,args`. If the pid came from a
   file, check `/proc/<pid>/comm` immediately before killing: pids are reused,
   and killing the new owner is not reversible. `stop()` in
   `nested-session.sh` does this check.
4. If a pattern is unavoidable, bracket one letter so it cannot match itself
   (`'examples/galler[y]'`), and make it cover every launch path: a `cargo run`
   process is `target/release/...` with no `ui/` prefix.

### Cleanup is not done when the test prints PASS

`GALLERY_SELFTEST=1` prints `PASS` and keeps running for `GALLERY_SECONDS`
(default 300). Verify the process is gone before starting the next one:

```sh
ps -eo pid,ppid,args | grep -E 'examples/galler[y]'
cat /proc/<pid>/comm
```

## Shell scripts fail silently in two ways

**`set -euo pipefail` plus a poll loop.** A pipeline inside an assignment exits
the whole script on the first non-zero status, with no message, because it is
not part of an `if`:

```sh
fds=$(ls -l "/proc/$pid/fd" | grep -oE 'wayland-[0-9]+' | sort -u)
```

When the socket does not exist yet, `grep` exits 1, `pipefail` fails the
pipeline, and `set -e` ends the script at the assignment. Write `|| true` on
every pipe in a poll loop, and expand `[[ -n "$x" ]] && cmd` into
`if [[ -n "$x" ]]; then cmd; fi`.

**A script that exits silently.** Run it with `bash -x` and read the last
statement executed; it is usually one of the two shapes above.

## Do not create a file over an existing one

Creating `src/session.rs` without checking the path replaced an existing
`session` module entirely. The compiler complained in `window.rs`, where
`connect()` was missing, not in the file that was overwritten.

Check before you name a new module:

```sh
ls crates/<crate>/src
grep -rn 'mod <name>' crates/<crate>/src
```

If the name is taken, choose another. A rename is cheaper than recovering the
file from a transcript.
