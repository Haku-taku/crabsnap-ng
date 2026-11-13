# Command Line, Daemon and Settings

The command surface of the screenshot app, the processes behind it, and the contract they speak.

## Processes

| Binary | Role | Links |
| --- | --- | --- |
| `crabsnap` | Command line; runs a command locally, or forwards it to the daemon; `crabsnap settings` opens the settings window | portable app logic, `crabsnap-platform`, UI crates |
| the daemon | `crabsnap daemon`; holds the connection, surfaces, clipboard and hotkeys | `crabsnap-platform` |

There is one binary. The daemon and the settings window are subcommands of it, not programs of
their own, because both share the app's loop, its catalogs and most of its crates — a second
binary would link the UI framework twice to save nothing. `crabsnap daemon` hosts the service as
a local command, with an IPC listener and an idle policy added. A local command never writes its
own loop — it uses the host from [the event loop](../engine/event-loop.md).

The settings window is the only **writer** of the configuration file; the daemon reads it and is
told to reload.

It edits the capture backend, the save format and the language as choice rows, the fourteen
shortcut actions as one text field each, and two free-form values: the shot directory and the
ffmpeg program. An emptied field removes its key, so the default takes over again; a shortcut
value holds one chord or several separated by commas, and one the parser cannot read is refused
and nothing is written. Writing is surgical: only keys this build knows are touched, so the rest
of a file that also carries `[ui]`, `[annotate.*]` or shortcut names from a newer build keeps
them. The window paints itself with the MD3 dark scheme its controls resolve against, rather than
with colours of its own.

The language item is a choice, not a tag to type: `Auto` follows the system's
preferred languages, and every other button names a shipped language in its own
script. Selecting one switches the window at once and writes `[ui] language`;
saving sends `reload`, which is how the daemon learns the choice. A tag someone
typed into the file by hand is read at start-up; one that does not parse is
reported by `crabsnap info config` and treated as `auto`, so a typo costs the
language and not the tool. The command line keeps its own precedence: `--lang`,
then `[ui] language`, then the system list, then English.

A window needs input, so the scripted path exists too:

```sh
crabsnap settings --save --backend ext --format webp --ui-language zh-CN \
    --shortcut region=Print --directory /tmp/shots --ffmpeg /usr/bin/ffmpeg
```

That applies the values, writes the file, sends `reload` to a running daemon and exits without
a window — the same code the Save button runs. A flag that is not given leaves the file's value
alone; an empty one (`--directory ''`) is the same as clearing the field, and `auto` on
`--backend`, `--format` and `--ui-language` removes the override. `--ui-language` takes `auto`
or a BCP-47 tag and is refused, before the file is touched, when it is neither. It is not the
top-level `--lang`: that one only chooses the words of this run, while this one is written into
the file.

## Starting it

With no argument the launch decides what the process is. Started from a terminal it prints the
command tree — the same text `crabsnap --help` prints — because someone who typed the bare name
is asking what the tool does; started with no terminal at all — a desktop icon, a file manager, a
session's login entry — it starts the daemon, because a person who double-clicked asked for the
tool to be there, not for a window. "No terminal" is stdin, stdout and stderr all not being a
terminal, which is exactly how those launches arrive; a shell that redirected one of them still
counts.

The daemon puts its icon in the [system tray](../engine/tray.md), which is where a desktop
start can be seen and stopped: the tooltip names the tool, and the menu offers Capture Region,
Capture Output, Settings… and Quit. A primary click is Capture Region, the tool's main action;
a right-click opens the menu, as the desktop's own. On a platform with no tray — Linux today —
the daemon starts anyway and loses only the icon, because the hotkeys are what it is for.

A second desktop start while the daemon runs exits at once with success: the user asked for
the tool to be running, and it is. `crabsnap daemon` keeps the strict answer and refuses with
"a daemon is already running", because a script that starts one wants to hear about it.
Settings… starts this binary again as `crabsnap settings`: that window owns its own connection
and its own loop, so it cannot be a surface of the daemon.

Windows builds are GUI-subsystem binaries, so a desktop start opens no console window. A shell
hands a GUI child no standard handles, though, so a terminal start attaches to the parent's
console and reopens `CONOUT$` and `CONIN$` itself; that attach is what makes a command's output
and `--help` work at all, and it happens on every invocation because every invocation may
print. Only the bare launch turns "there is a console" into a decision. The test harness is the
exception and keeps the console subsystem, since a GUI one has nowhere to print its results. A
failure on the desktop path has no console to land in: Windows shows a message box, Unix writes
to stderr, which such a launch discards — which is what the [log](#logs) is for.

## Logs

Every process files its records under the local application data, and prints them to stderr as
well when it has one: `%LOCALAPPDATA%\crabsnap\logs\` on Windows, `$XDG_STATE_HOME/crabsnap/logs/`
(`~/.local/state`) on Linux. The file is named for what the process is — `daemon.log`,
`settings.log`, and `command.log` for everything else — so a report from a desktop launch says
which part of the tool wrote it.

Records are the `log` crate's, which the engine already reports through; installing a logger in
the app is what stops the engine's warnings being dropped, and the host's retired-surface and
unserved-clipboard lines take the same road. A panic goes through it too, because a thread that
dies on a desktop launch would otherwise leave nothing behind. A file that reaches 1 MiB is
moved to `.1` before the next record, older generations shift up, and three are kept, so a
daemon left running for weeks cannot fill the disk.

## Commands

Names follow the legacy project; the mapping is deliberate, so muscle memory and scripted
recipes survive.

| Command | Meaning |
| --- | --- |
| `shot region` | Interactive selection over a frozen frame; the overlay |
| `shot output` | Capture one output, no overlay |
| `shot window` | Capture a window (later; needs a picker) |
| `shot desktop` | All outputs stitched (later) |
| `shot longshot` | Scrolling stitch (later, D8) |
| `pin add --file PATH... [--as KIND] [-o N]` | Put each file on the screen as its own pin, cascaded on output `N` (the first by default); `--as` overrides detection, a file that fails is named and the rest still pin |
| `pin add --color CODE [-o N]` | A palette pin from one CSS colour: hex, `rgb()` or `hsl()`; `--file` and `--color` are mutually exclusive |
| `pin close|show|hide|toggle [--id N]` | Operate the on-screen pins; the daemon cannot see a client's pointer, so `--id` is required |
| `pin list` | The pins the daemon holds: id, kind, size, position, source, visibility |
| `pin export --id N --file PATH [--format F]` | Write an on-screen pin to a file; the pixels live in the daemon |
| `info config|formats|status` | Read-only answers; `config` prints the file path, effective values and any ignored keys, `formats` the encodings this machine can write and where each comes from, `status` the outputs, the chosen backend, the hotkeys the daemon bound and the application identity the process took |
| `daemon [--idle SECS]` | Host the service |
| `settings [--save] [--backend RUNG] [--format F] [--directory PATH] [--ffmpeg PATH] [--ui-language TAG] [--shortcut NAME=CHORD]...` | The settings window; `--save` applies the flags to the file instead of opening it |
| `autostart [on\|off]` | Start the daemon at login; no argument prints the state |
| `quit` | Ask the daemon to exit |
| `reload` | Re-read the configuration and re-probe affected capabilities |
| `completions --shell SHELL` | Completion script |

`shot` takes the output targets as composable flags — `--file`, `--clipboard`, `--pin` — with
`--file` the default when none is given, a `--delay`, and `--format`. `--clipboard` is on
`shot output`: the capture goes to the daemon over the channel as a file path, and the daemon
holds the selection so it outlives the command that asked. A region shot copies through the
overlay's own Copy action instead. An encoding is chosen by
the flag first, then by the file name's extension, then by the configured default, then PNG; so
`shot -f a.jpg` is a JPEG with no flag, and `--format webp` wins over a `.png` name.

PNG is built in, in both bit depths: 8-bit sRGB for an SDR picture, and 16-bit BT.2020 PQ when
the picture holds light above the display's white, with `cICP` and `cLLI` written beside the
pixels. Reading a PNG — including that 16-bit one — goes through the same crate, so an HDR file
this tool saved is read back as the light it holds even with no ffmpeg anywhere.
The other offered formats — `jpg`, `webp`, `avif`, `jxl`, `tiff`, `bmp`, `gif`,
`qoi` — are ffmpeg's: the container, the encoder, its pixel formats and its parameter list all
come from libavformat and libavcodec at startup, and the picture is encoded in this process
through a C shim compiled against the ffmpeg headers on the build machine
(`apps/crabsnap-platform/ffmpeg/ffi/ffmpeg_shim.c`). One crate serves both targets, because the
loader has a branch per platform: `dlopen` by versioned soname on Unix, `LoadLibraryExW` by
versioned file name on Windows, which also opens avcodec and avutil beside avformat — on ELF a
name reaches libavformat's dependencies, and `GetProcAddress` reaches nothing but its own module.
Both branches refuse a major that differs from the headers, so the binary neither links the
libraries nor fails to start without them; a build with no headers compiles a stub with the same
symbols. Where the headers are is the platform's question, and Windows has no registry to ask:
`CRABSNAP_FFMPEG_INCLUDE` names a directory holding `libavcodec/avcodec.h`, e.g. `mingw64\include`
of an MSYS2 install, and the DLLs of the same major — `avformat-<major>.dll` and its companions
in `mingw64\bin` — are what is loaded at run time. When the
libraries cannot be used, the `ffmpeg` program — on `PATH`, or at `[codec] program` — is the
fallback. It and the tray's Settings row are the only processes this tool starts: one encodes a
format the libraries cannot, the other is this binary again as `crabsnap settings`. The
libraries are looked for in
`[codec] library_directory` first and then along the dynamic loader's own search path. A
configured `program` or `library_directory` that does not exist is reported by `info formats`,
not quietly replaced by another path. `info formats` prints each format with the encoder and
muxer that write it and where they came from; `--verbose` adds the encoder's own options.
Asking for a format the machine lacks is an error that names what is missing, never a silent
PNG.

Each format's encoder is tuned from `[codec.options]`: the key is the extension, a dot, and a
parameter of the encoder that writes it (`webp.lossless`, `avif.crf`, `jxl.distance`). Those are
the encoder's own parameters, so `info formats --verbose` is the reference: it prints each one
with its default. JPEG is the case worth knowing — the mjpeg encoder has no quality option, so
its knob is `global_quality`, the old qscale counted in `FF_QP2LAMBDA` (236 is q=2, the default).
Values are text and the encoder parses them. A key that names an unknown format, a format this
machine cannot write, or a parameter its encoder does not have is named by `info formats`, not
silently ignored.

`shot region --demo`
paints a fixed selection instead of waiting for a drag; the scripted probes that stood beside it
(`--script`, `--annotate` and the `--pin-*` drivers) were removed, and no capture mode is
selected by an environment variable.

`pin list` exists because `pin export` is meaningless without it: the daemon owns the pixels, and
a client can only name one by the id the daemon gave it.

`autostart` writes the platform's own login entry rather than a mechanism of ours: a `REG_SZ`
value named `crabsnap` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` on Windows, and
`crabsnap.desktop` under `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`) on a desktop.
The command carries this executable's path and `daemon`, and the state it prints is read back
from that entry, so an executable that has since moved reads as off and `on` rewrites the entry.
The system's startup settings list the same entry, so a user who prefers their own tool has a
place to say so.

## Local mode and the daemon

A command first tries the daemon's socket. No socket, or a refused connection, means no daemon:
the command runs the same service in its own process and exits when it has no surface left. A
connection that is accepted but never answered is an error, not "no daemon" — starting a second
daemon would split the pins, the clipboard and the hotkeys between two processes that cannot see
each other.

Single instance is enforced by the channel itself: a second `crabsnap daemon` connects first and
refuses when something answers. A stale socket — a file with no listener — is detected by the
refused connection, read as "no daemon", and replaced by the next daemon start.

`pin add --clipboard` is the exception: the selection must be read by the
process that owns the pins, so with no daemon the command says so instead of
pinning locally.

`--no-daemon` skips discovery entirely and always runs locally, which is the debugging path.

Pins end with the daemon that holds them: Ctrl-C on a foreground
`crabsnap daemon` closes every canvas it opened, so a session meant to keep
pins runs it detached or in a terminal of its own.

## IPC

The transport is a framework capability, the messages are the application's. The split keeps
platform code out of the app and protocol code out of the framework.

| Layer | Where | What |
| --- | --- | --- |
| Transport | `ui-platform::ipc` traits; backend per OS | A message-oriented local channel. Implemented on Linux: `SOCK_SEQPACKET` at `$XDG_RUNTIME_DIR/crabsnap/daemon-<display>.sock`, because a daemon is bound to the compositor it connected to. Windows named pipes are the other backend, not written yet |
| Bulk data | `ui-platform` | A shared buffer: memfd on Linux, `shm_open` on macOS, named file mapping on Windows; the owner passes a token, the reader maps it. Not needed yet: no command returns pixels |
| Messages | `crabsnap-core` | Versioned op codes and request/response types, encoded with a fixed header plus `bincode` |

The header is the magic, the protocol version, the operation and the payload length; a peer
framing a stream needs the length, and a version mismatch is reported instead of misparsed. The
daemon's channel joins the backend's poll set, so a request wakes the one loop exactly as input
does; the application's handler answers through the reply path it is given.

An answer that needs background work keeps its reply: `Reply::deferred` duplicates the peer's
descriptor, and the loop sends the answer from a later turn. `copy` uses it — the daemon reads and
re-encodes the image on the worker, then publishes the selection and answers `Ok` only once the
bytes exist, so a client that reads the clipboard after the reply finds it already set.

Pixels never travel inline. The daemon writes a file itself when the answer is a path, holds the
clipboard when the answer is a copy, and hands a shared-buffer token when the caller asked for the
raw frame. On Unix the shared buffer's descriptor may be passed over the socket instead of the
token; that is an optimisation, not part of the contract, because Windows has no descriptor
passing.

The protocol is binary on purpose. A control message is small, so the codec is not where latency
lives; the reasons are no text escaping, no allocation per field, and a versioned fixed header
that cannot be confused with payload. The measurable wins are elsewhere: a long-lived daemon does
not probe backends per invocation, and no frame is copied through a socket.

## Capability probing

Backends are resolved **once**, when the service starts:

- capture rungs in order: `ext-image-copy-capture-v1`, `wlr-screencopy`, the portal;
- hotkeys: the portal's `GlobalShortcuts`, then a compositor protocol;
- clipboard, shm formats, and the output list.

Resolution is repeated only on a trigger: a configuration reload, an output appearing or
disappearing, a scale change, an explicit `info --refresh`, or a rung failing repeatedly with a
backoff. A failed grab falls through the already-resolved chain and marks the rung degraded; it
does not re-probe everything.

A daemon therefore pays the probe once, not per command. A local invocation pays it once, because
it has no earlier lifetime to inherit.

## Configuration and hot reload

One schema, one writer. The settings window edits it; `crabsnap info config` reads it. The file is
`$XDG_CONFIG_HOME/crabsnap/config.toml` on a Unix desktop and
`%APPDATA%\crabsnap\config.toml` on Windows, and a missing file is simply the defaults:

```toml
[capture]
backend = "ext"            # ext, screencopy or portal; omit to take the fallback chain

[shots]
directory = "/home/me/Pictures/Screenshots"   # omit for the pictures directory
format = "png"                                # png, jpg, webp, avif, jxl, tiff, bmp, gif, qoi; omit for PNG

[codec]
library_directory = "/opt/ffmpeg/lib"         # searched for the ffmpeg libraries before the loader's path
program = "/usr/local/bin/ffmpeg"             # used when the libraries are missing or too old

[codec.options]
"webp.lossless" = "0"                         # lossy WebP; the default is lossless
"webp.quality" = "90"
"avif.crf" = "30"

[ui]
language = "zh-CN"         # "auto" (the default) or a BCP-47 tag

[pin]
radius = 12                # squircle corner radius in logical pixels; 0 is square

[pin.idle]                 # what a pin looks like with the pointer away
border_width = 0           # logical pixels, drawn outside the card; 0 hides the border
border_colour = "#00000000"
shadow_blur = 16           # pixels; 0 turns the shadow off
shadow_spread = 0
shadow_alpha = 0.47        # 0..=1
shadow_colour = "#000000"

[pin.active]               # what it looks like with the pointer on it
border_width = 1
border_colour = "#00000040"
shadow_blur = 24
shadow_spread = 2
shadow_alpha = 0.55
shadow_colour = "#000000"

[shortcuts]
region = "Print"           # omit an action for its default; "" clears it
```

Saving sends `reload` over the same IPC contract; the daemon re-reads the file and re-resolves the
capture rung and the encoders. Each probe starts from nothing loaded, so a `library_directory`
that only became right, or an ffmpeg installed, since the process started is picked up by the
next `reload` rather than the next launch. A file that is not TOML is refused with its path and
line, and a failed reload keeps what the daemon was already serving — a typo must not take the
tool down.
Only keys that a binary actually reads are in this schema: a key with no consumer is a setting
that changes nothing, which reads as a broken tool. `[ui] language`, `[shortcuts]` and `[pin]` are
adopted because something reads them; the rest of the legacy `[ui]` table and `[annotate.*]` wait
for their features (recording, annotation tools). Until then they are ignored and named by
`info config` rather than failing the command, because a user may already have that file.

`[pin]` is the look of a pin on screen: the corner radius every card shares, and two skins — one
for the pointer away, one for the pointer on the pin. A skin states the border's width and colour
and the shadow's blur, spread, opacity and colour, so the active skin can be a lift, a glow or a
ring without touching the code. Colours accept `#RGB`, `#RRGGBB` and `#RRGGBBAA`, with or without
the `#`. The two skins are read together with the radius because they describe one object, and
the pointer's arrival is one transition: every number and every channel moves between the skins
on the same clock (`0.16 s`, ease-out), so a border cannot snap on before the shadow that lifts
it. A value outside its range (a negative radius, a blur past 128, an alpha past 1, a colour that
does not parse) is named by `info config` and the built-in value is used for that key alone, so
one bad number does not take the pin away. `shadow_blur` and `shadow_alpha` directly under
`[pin]` are the older spelling of `[pin.idle]`'s two keys and still apply. The look is resolved
when a canvas opens: changing the file affects the next pin canvas, not the pins already on
screen.

`[shortcuts]` maps an action name to the chord or chords that run it. A missing key means the
built-in default, and an empty value clears the action — the question is whether the key is
there, not whether its value is empty. A value may hold several chords separated by commas,
which is how each cursor direction answers both a letter and an arrow. The actions this build
knows, and where each one is bound:

| Action | Default | Bound on |
| --- | --- | --- |
| `region` | `Ctrl+Shift+4` | the desktop, through the global-hotkey service |
| `output` | `Ctrl+Shift+3` | the desktop |
| `cancel` | `Escape` | overlay, pin |
| `undo` | `Ctrl+Z` | overlay, pin |
| `redo` | `Ctrl+Shift+Z` | overlay, pin |
| `save` | `Ctrl+S` | overlay, pin |
| `copy` | `Ctrl+C` | overlay, pin |
| `pin` | `Ctrl+P` | overlay |
| `cursor` | `V` | overlay, pin |
| `clear` | `Ctrl+D` | overlay, pin |
| `confirm` | `Enter` | pin |
| `cursor-left` | `A, Left` | overlay, pin |
| `cursor-right` | `D, Right` | overlay, pin |
| `cursor-up` | `W, Up` | overlay, pin |
| `cursor-down` | `S, Down` | overlay, pin |

A cursor direction is also answered with `Shift` held, which asks for the coarse
step (ten picture pixels instead of one). Those spellings are derived from the
written chord and never appear in the file.

"Bound on the desktop" means the compositor's global-hotkey service; the other three are the
surface's own key tables, so a chord there only reaches the surface that has the keyboard. One
spelling reaches every place an action names: a chord written for `cancel` works on the overlay
and on a pin without a second entry. A chord is modifiers joined to one key: `Ctrl`, `Shift`,
`Alt`, `Super`/`Logo`, then a letter, digit, `Print`, `F1`..`F24` or one of `Escape`, `Enter`,
`Space`, `Tab`, `Backspace`, `Delete`, `Home`, `End` and the arrows. A modifier written with a
trailing `?` is optional, so one line answers a command whose effect depends on it:
`Shift?+A` is both `A` and `Shift+A`. A global action cannot take a `?`, because the desktop is
given one literal trigger. A name this build does not know, or a chord it cannot read, is
printed by `info config` and binds nothing rather than failing the command.

The settings window edits the same keys, one field per action, and spells a default under an
empty field, since a placeholder only appears while the field is focused. An emptied field
removes the key, so the default takes over again; an action the file cleared stays cleared —
the window says so under its field, and a save writes it back rather than rebinding it. Clearing
is therefore a hand edit: neither the window nor `--shortcut` writes an empty value. What the
window writes is the schema's own name and spelling, so the file, the window and `info config`
cannot disagree.

The daemon binds the plan at startup and again on `reload`, printing each outcome once and
reporting the same lines through `info status`: a desktop without global hotkeys says so, another
application's chord is named, and the portal's own dialog is answered by the user. A press acts
in the daemon's own process: `region` grabs one frame and opens the selection overlay as a
surface of the daemon's loop, `output` grabs and saves without a surface. No command is started
and no second connection is opened. The settings
window stays usable with no daemon running: the next daemon start reads the file, and so does every
local `shot`, where `--backend` wins over the file. `CRABSNAP_SAVE_DIR` still wins over the
configured directory, so a scripted run never writes into the user's own folder.

## Status

The command surface is in, on the framework host ([event loop](../engine/event-loop.md)). Bare
`crabsnap` prints the command tree from a terminal and starts the daemon without one; every other
invocation parses a command.

| Works today | Notes |
| --- | --- |
| `shot output [-o N] [--backend RUNG] [-f PATH] [--pin] [--clipboard] [--delay SECS]` | `-o` names the output by its place in the compositor's list and is refused, with the count, when there is no such output; `--pin` attaches the frame to a canvas the size of that output |
| `shot region [--demo] [--script] [--annotate] [--pin-drag] [--pin-zoom] [--pin-menu] [--pin-raise] [-f PATH] [--delay SECS]` | the probes that used to be environment variables |
| `info config|formats|status` | `status` reports the capture backend and every output: name, logical size, position, scale and refresh |
| `settings [--save] [...]` | the window, or with `--save` the same values written to the file and announced to the daemon |
| `autostart [on\|off]` | the platform's login entry: the `Run` value on Windows, a `.desktop` file on the desktop; the report is read back from it |
| `completions --shell SHELL` | generated from the same tree the parser uses |
| `daemon [--idle SECS]` | the service: one connection, the local channel, the output list, capability probing once at start, and the tray icon where the platform has one |
| `quit` / `reload` | answered over the channel; either prints "no daemon is running" when none is |
| `pin add --file PATH... [--as KIND] [-o N]` | the daemon reads, classifies and places each file on that output's canvas |
| `pin add --color CODE [-o N]` | the daemon parses the code and opens a one-colour palette on that output |
| `pin list` | the daemon's answer: every held pin, with `(hidden)` when it is not painted |

The daemon probes the capture rung once, answers the channel, exits on `quit` or after `--idle`
seconds with no request, and refuses to start twice. Pins belong to its connection: the first
`pin add` for an output opens that output's canvas — one surface carrying every pin of that
output, so pressing a pin can raise it above the others — and later ones add a texture to the
surface already there. `Escape` with the pointer over a pin closes it, and a canvas whose last
pin is gone closes itself, so a local `pin add` returns as soon as someone dismisses the pin.
Without a daemon the same command runs locally and blocks until then; `pin list`, `export`,
`close`, `show`, `hide` and `toggle` have no local form, because the pixels and the canvas are
the daemon's. `--format` waits for the encode layer. Scripted runs still read
`CRABSNAP_SECONDS` as a deadline, a test knob rather than a mode.

Help text lives in the app's Fluent catalogs: clap declares the structure, and `cli-<path>-<arg>`
keys supply every about and help string, so `--lang` switches the whole tree before parsing.

## Corrections

**A daemon started from the desktop left no trace.** Everything the app reported went to
`eprintln!`, and a GUI-subsystem binary started by an icon has no stderr handle at all; the
engine's own `log::warn!` records were worse off, because no logger was installed anywhere, so
they were dropped even where a console existed. A daemon that exited silently — the failure
[the event loop](../engine/event-loop.md) now retires a surface for instead of dying — left
nothing to read afterwards. `diagnostics::install` runs first in `main` and writes each record to
`%LOCALAPPDATA%\crabsnap\logs\<role>.log` as well as stderr, rotating at 1 MiB with three
generations kept; the app's stderr calls are `log` records now, so nothing depends on a console
being present. Caught by `a_full_log_moves_aside_and_keeps_three_generations` for the rotation
and, for the rest, by reading the file after a run: `crabsnap daemon` followed by a capture
leaves the start line with its hotkeys in `daemon.log`.

**The encoders do not hand-declare libavcodec's types, and no longer avoid it either.** The
first version kept PNG in-process and reached JPEG and WebP through libjpeg-turbo and libwebp,
each with a one-call whole-image API, on the grounds that any other route meant declaring
`AVCodecContext`, `AVFrame` and `AVPacket` by hand with no guarantee the layout matches the
library at run time. That held while the tool offered three formats. Covering what ffmpeg can
write — avif, jxl, tiff, gif, qoi — needs the encoder table, its pixel formats and its options,
which only libavcodec knows, so the app now builds
`apps/crabsnap-platform/ffmpeg/ffi/ffmpeg_shim.c` against the headers installed on the build
machine. Every layout is correct by construction, the library's
major is checked against those headers when it loads, and a build without headers compiles
`ffi/ffmpeg_shim_stub.c`, which answers "unavailable" so the program fallback takes over.
`info formats` reports which path is live; `CRABSNAP_NO_FFMPEG_SHIM=1` builds the stub on
purpose, and every format then encodes through the program with the same results.

**`dlopen(NULL)` is the main program, not a failure.** The shim opens libavformat from the
configured `library_directory` when there is one and by soname otherwise. The first form passed
a null path when no directory was configured, which returns a handle to the running binary: the
libswscale step then "found" a handle, failed to resolve `sws_getContext`, and every extended
format fell back to the program without saying why. `info formats` printing the empty reason is
how it surfaced; the call is now made only with a path.

**A still image in a container is not a stream.** Two assumptions of the first encode loop were
wrong for avif. The codec configuration goes into the file's `av1C` box only when the encoder
opens with `AV_CODEC_FLAG_GLOBAL_HEADER`, so the container has to be chosen before the encoder
opens rather than after the picture is encoded. And the muxer rewrites its header and refuses a
non-seekable output, so the memory sink tracks a cursor and a length instead of appending, and
the encoder is flushed before the packets are collected. `magick identify` rejected the first
attempts as truncated files; it opens every format the shim writes now.

**A probe could only be made once, and not from two threads at all.** The shim kept its loaded
state for the life of the process, and `cs_library_directory` ignored a later directory after any
attempt, including a failed one. A daemon whose `library_directory` was wrong at startup went on
answering "this machine cannot write jpg" after the file was fixed and `reload` sent; only a
restart helped. The probe is now the unit of that state: `cs_reopen(directory)` retires it, and
the shim builds a candidate from the directory and publishes it only when it is complete, so a
retry that fails leaves the working libraries in place while a fixed directory takes over.
Verified by poisoning the loader with `LD_LIBRARY_PATH` and pointing the config at a directory of
symlinks: a daemon that failed its first probe exported a JPEG after `reload`, and the same
sequence failed with the call removed.

The first version of that fix unloaded the handles, which is only safe when nobody else is inside
them. The test suite runs its tests in parallel, two of them probe, and one thread was calling
`avcodec_version` in a library the other had just `dlclose`d: SIGSEGV four runs in four. Loaded
state is now never unmapped — a re-probe keeps its own reference and the published pointers stay
callable — and publishing sits behind a mutex. A crash-free parallel suite is what notices a
return to unloading.

**Windows resolves one module at a time, and its search flag takes no forward slash.**
`GetProcAddress` reads one module and stops, so `avcodec_version` and the pixel-format table are
invisible through the `avformat` handle that `dlsym` reaches on ELF; the shim opens avcodec and
avutil beside it and looks through all three. The configured directory then failed with "module
not found" for a file that was there: `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR` takes only a Windows
path, and a forward slash turns the whole string into a module name, where the plain flags
normalise it. The join uses the target's separator now, and a failed load names the path it tried
rather than the bare library name. `info formats` with `library_directory` set and the DLL
directory off `PATH` is what catches it.

**The PNG crate reads cICP and cLLI and writes neither.** `Info` carries both and `Decoder`
fills them, so a 16-bit PNG written through `Encoder::with_info` looked complete until the
round-trip test decoded it: no tag, and the light came back as sRGB. The writer emits IHDR, pHYs,
the colour chunks it knows and IDAT, and drops the rest without a word. Both chunks are written
by hand through `Writer::write_chunk` before IDAT, which is the order PNG 3.0 requires; the
round-trip test is what notices if that changes.

**A shortcut field hid the default it would use.** The settings window put the default chord in
the field's placeholder, so an untouched action showed an empty box: MD3 draws a placeholder only
while the field is focused and empty, and the moment a user reads the form is the moment the
field is not. An empty field now carries the default as supporting text, which is drawn whether
or not the field has focus; the placeholder stays for the focused, empty case. Verified in a
nested session: every untouched field shows its default under it (`Default
Ctrl+Shift+4; leave it empty for that, commas separate several`).

**The settings window was a second binary, and the split bought nothing.** It was its own crate
(`apps/crabsnap-settings`) so the main command would not carry its resources, but the window links
the UI framework, the widget kit and the same engine the app already links: "separate" bought a
second copy of nearly the same dependency tree, plus a second language resolution and a second
Fluent catalog that had to agree with the app's. It is `crabsnap settings` now, in
`apps/crabsnap/src/settings_window.rs`, taking the `Arc<Language>` the app already resolved and
that one catalog — which is why its keys carry a `settings-` prefix beside the app's `toolbar-`,
`pin-` and `cli-` ones. The scripted path moved from a private argument parser to clap, and its
language flag became `--ui-language`: the top-level `--lang` already means "the words of this
run", and the two are not the same choice. Verified by `--save` writing a file and refusing a bad
tag before touching it, and by the window opening through the same `Host` every other surface
uses.

**A GUI-subsystem process has no terminal until it attaches to one.** The first version asked
`std::io::IsTerminal` on the standard handles and nowhere else. Started from Windows Terminal's
PowerShell, that answered "no terminal" — the launch started the daemon, and `crabsnap -h`
printed nothing, because a shell hands a GUI child no standard handles at all. The process now
attaches to its parent's console with `AttachConsole(ATTACH_PARENT_PROCESS)` and reopens
`CONOUT$`/`CONIN$`, which is the documented route and the only one that gives `println!` a
stream. Whether it has a console is `GetConsoleCP`, not `GetConsoleWindow`: a pseudoconsole,
which is what Windows Terminal is, has no window, so the handle answers zero for an attached
process too. Checked by launching from a console-attached shell — the command tree is printed
and the process exits — and from `explorer.exe`, whose process has no console: that one starts
the daemon and puts its icon in the tray.

**The bare launch opened a stub bar.** A terminal start with no argument used to open a
layer-surface bar reading "Capture is not wired up yet", left from before the overlay existed. It
answered a question nobody asked — the bar's own subject was that it did nothing — while the one
thing a person typing a bare program name wants is what it can be asked to do. The launch now
prints the command tree, taking the parser's no-subcommand path, and the stub, its `Toolbar` app
and its two strings are gone.

## See also

- [The Screenshot App](./screenshot-app.md): what the commands do to the screen.
- [The Event Loop](../engine/event-loop.md): the one loop each process runs.
- [Window Backend](../engine/window.md): surfaces and layer roles.
