# Rendering Backends

One engine builds one `DrawList`; two paths turn it into pixels. The CPU path
rasterises into memory, the GPU path renders into a dma-buf that the compositor
scans out directly — or, on Windows, into a DXGI swap chain's back buffer.

## Shared by both

- The frame's `build_step`: view and declarations, layout, paint,
  `TextPass` resolution, damage, hit.
- `DrawList`, resolved primitives, clips, layers, and the glyph atlas.
- One `TextPass` instance for layout and painting. Two instances keep separate
  shaping caches and wrap the same paragraph differently.

## GPU path

The path splits at a `Device` trait. `ui-gpu-scene` decides what to draw —
batches, the layer stack, damage bands, pass order — and names no graphics API.
`ui-paint-gpu` implements that trait on GL: shaders, vertex arrays, texture
storage, framebuffers. `ui-gpu-egl` owns the EGL context and the dma-buf target;
`ui-gpu-d3d11` is the same trait on D3D11, described below. The Wayland chain
is:

1. `gbm_bo_create(..., GBM_FORMAT_ARGB8888, GBM_BO_USE_RENDERING)`.
2. Export fd, stride, offset, modifier; import as `EGLImage` with
   `EGL_LINUX_DMA_BUF_EXT`.
3. Bind the image to a texture, render into an FBO.
4. Wrap the same fd in a `wl_buffer` through `zwp_linux_dmabuf_v1` and commit.

Two targets rotate (`TARGETS = 2`). Each carries its recent bands forward, so
a reused buffer does not show a frame of stale content. A resize rebuilds the
pool.

### Explicit sync

With `wp_linux_drm_syncobj_manager_v1`, two DRM timelines carry an acquire
point (exported by EGL and imported as a fence) and a release point (signalled
by the compositor). Both ride the same `wl_surface.commit`. Points must
strictly increase. Before reusing a buffer the old release point is waited on
for 50 ms, then drawing proceeds anyway rather than blocking the UI.

Without the protocol, `glFinish` runs before attach and commit. The nested test
session does not advertise the protocol, so present timings measured there come
from the fallback.

### The Windows device

`ui-gpu-d3d11` implements the same `Device` trait on D3D11 and is empty off
Windows: a device that cannot exist without COM and DXGI has nothing to fall
back to. Both stages' constant buffer is generated from one table with explicit
`packoffset`, and the byte offsets the device writes into come back from the
same function, so the shader's layout and the upload cannot describe different
buffers. Instance layouts and primitive semantics stay the scene's; only the API
calls are the backend's.

Four API facts shape the code:

- A sampler array takes a literal index only, so a unit chosen by a uniform
  becomes a two-way `if` (`sample_unit`) rather than `u_samplers[unit]`.
- A scalar array takes one constant register per element, so the tap weights are
  `float4[9]` read through `tap_weight`, not `float[33]`.
- `ClearRenderTargetView` ignores the scissor, so a damage band clears through
  `ID3D11DeviceContext1::ClearView` and falls back to the plain clear on a
  context without it.
- Device rows grow downward and D3D's viewport grows upward, so the flip lives
  once, in `device_to_ndc`.

`GpuRenderer` holds a `HashMap<ProgramKey, Program>` and builds a program the
first time a frame draws it, because a surface pays for what it draws and not
for the rest. The device compiles each program's HLSL once per process into a
`OnceLock` per `ProgramKey`, so a second surface reuses the bytecode, and the
presenter starts a worker for the programs its first frame did not draw as soon
as that frame is on screen (`precompile_remaining`).

`cargo run -p ui-gpu-d3d11 --example d3d11_scene` draws one 160×120 scene
through d3dcompiler — a rounded rect, an ellipse, a line, a sector and a blurred
layer over an opaque backdrop, so the programs it needs compile on first use —
and reads it back, checking
eight invariants: shape edges land where the primitive says, off-shape pixels
keep the backdrop, and the layer composite keeps its orientation. It does not
compare against the GL backend; that needs a Linux run.

### The Windows window

`ui-platform-win32` is the D3D11 device's window: `CreateWindowExW` under
per-monitor DPI awareness, an `IDXGISwapChain1` from `CreateSwapChainForHwnd`
(`B8G8R8A8_UNORM`, two buffers, `DXGI_SWAP_EFFECT_FLIP_DISCARD`), and
`Present(1, 0)` for vsync pacing. It implements `SceneTarget`, so the app loop
drives it exactly as it drives the Wayland GPU window; the opening size in a
`SurfaceSpec` is logical and becomes a physical client area through
`AdjustWindowRectExForDpi`.

The painter never draws into the back buffer. `ui-gpu-scene` keeps a frame
target of its own and, at the end of a frame, blits only the damage band into
the target it was handed; the presenter hands it a second persistent `Rgba8`
target, then runs `D3d11Device::present_copy`, one fullscreen-triangle pass
that samples all of that target into the back buffer. Two reasons for the
extra target: a flip-discard back buffer holds no previous frame, which the
band's untouched pixels depend on, and a plain `CopySubresourceRegion` does not
convert RGBA to BGRA, while the destination view's format orders the channels
during the sample. A resize releases the adopted back buffer, calls
`ResizeBuffers`, adopts buffer 0 again and resizes the frame target.

D3D11 hides a flip chain's rotation — `GetBuffer(0)` is always the buffer to
draw into, and `GetBuffer(1)` returns `DXGI_ERROR_INVALID_CALL` — so the
presenter holds one back-buffer target. A capture therefore reads buffer 0
inside `present_scene`, after the copy and before `Present`: after the flip,
that index holds the next, still-blank storage.

`cargo run -p ui-platform-win32 --example window_scene --release` draws a scene
of three primaries, a rounded bar animated from `cx.now()` and a line of text;
`WINDOW_CHECK=1` reads the presented buffer back at the opening extent, resizes
the window to 720×480, and reads it back again. Verified on Windows 11 at 1.25
device pixels per logical unit (1200×800 and 720×480 client areas): all four
probes passed at both extents. `WINDOW_DUMP=path` writes the frame as a PPM.

### The Windows window as a frame host

The window is also the driver's `FrameHost`, so the engine decides when to turn
and no app pumps messages itself. A Win32 window has no frame callback:
`wait_for_frame` calls `DwmFlush`, which returns once the composition pass that
will show the presented frame has been submitted — the pacing point a
`wl_surface` frame callback marks — and sleeps 8 ms instead when composition is
off and there is nothing to flush. `can_draw` is always true, because unlike a
Wayland surface there is no configure round trip that can withhold a buffer.

Input leaves the window procedure as `ui_core::Event`: cursor motion plus a
`TrackMouseEvent`-armed leave, the three buttons with `SetCapture` as the
implicit grab a compositor would give, `WM_MOUSEWHEEL` and `WM_MOUSEHWHEEL` in
notches of 48 logical units, named keys and command chords from `WM_KEYDOWN`,
text from `WM_CHAR`, and `Modifiers` whenever `GetKeyState` disagrees with the
last report. A numeric keypad needs no map of its own: with Num Lock off it
sends the navigation cluster's `VK_INSERT`…`VK_DELETE`, which those names cover,
and with it on `VK_NUMPAD0`…`VK_NUMPAD9`, which arrive as text or as a command
character. The media keys stay unnamed on both backends: they are the desktop's
transport controls, and no engine command binds one.

The two wheel messages do not share a sign: a positive vertical
notch is away from the user, which scrolls content up, while a positive
horizontal one is a wheel tilted right, which reveals content to the right — the
sign Wayland sends. Positions are divided by the window's DPI so input and
layout share logical units. Focus, extent and scale changes carry no event of
their own and are reported as a silent change, which still owes a frame.

A finger and a pen need no code of their own: Windows promotes a contact to the
mouse messages above, at the contact's own position, and `examples/pointer_probe`
injects one of each to watch the press, the drag and the release arrive that way.
Pressure, the eraser end and a second contact have no engine event to arrive in;
a pressure channel is a change to `Event` and to every annotation tool, not a
backend gap.
`WM_PAINT` is validated at once and reported the same way, because the frame
loop is what paints. A clipboard copy publishes every Windows format its content
kind has — `CF_UNICODETEXT`, `"HTML Format"`, `CF_HDROP`, and `"PNG"` beside
`CF_DIBV5` and `CF_DIB` — and a read turns those back into text, a file list or
a PNG; the app colour scheme comes from
`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`,
read on every poll.

`ui-platform-win32::Session` owns window, presenter and driver in that teardown
order and offers the same `open`/`tick`/`finish` shape as the Wayland session.

An app is written against `ui_platform::session::HostSession`, the portable
contract both sessions implement, and against the two DTOs beside it,
`SessionOptions` and `Tick`: a backend may carry any number of extra methods, but
what app code calls is the trait. `ui-host` re-exports those three from
`ui-platform` and only the backend itself — `Session`, `ThemeWatch` — from the
target tables, so the gallery's manifest and source mention no OS and its run
loop is generic over `H: HostSession<Gallery>`. Nothing restates a DTO on the
platform side: two copies of one struct are two contracts that drift apart.

What a host can answer and another cannot stays a runtime question, and the trait
is where the question is asked — a refresh rate, a frame-callback count, a
dma-buf count, a process CPU clock. A host without the number returns `None`
and the report leaves the field out instead of printing a zero; a Win32 session
answers `true` to `gpu_present` because D3D11 is its only path, and `None` to
`dmabuf_stats` because a flip-model swap chain holds no dma-buf. Measured on
Windows 11 at 1.25 device pixels
per logical unit: the gallery draws its three control sets, a posted
`WM_LBUTTONDOWN`/`WM_LBUTTONUP` on the MD3 switch flipped it and built the next
frame, an animated page ran at 184 frames per second against a 275 Hz display,
and an idle page settles to four to six turns a second.

### The Windows process loop

`Win32Backend` implements `ui_platform::host::Backend`, so a multi-surface
application runs on Windows through the same `Host` as on Wayland. One message
pump serves the process: a window procedure queues the events of its own window,
and a surface's turn takes only those, so two windows never read each other's
input. `wait` is `MsgWaitForMultipleObjectsEx` over an auto-reset event the
backend hands out as the `Waker`, with the loop's deadline bounded the way the
window's own wait is. `flush` calls `DwmFlush` once per turn when a surface
presented, which is the process's single pacing point.

The remaining services are answered from Win32 directly: `outputs` enumerates
monitors and reports each in logical units with its effective DPI, refresh rate
and quarter-turn orientation; `system_languages` reads
`GetUserPreferredUILanguages` under `MUI_LANGUAGE_NAME`; a clipboard copy is
written whole in every format its content has, since Windows has no selection
protocol to serve under, and a paste request goes back to the asking window.

A surface must say when it owes the next frame. A turn that drew clears the
driver's own deadline, so `next_deadline` also reports "now" while the engine
is animating or the last build read the clock; without that the loop slept out
its poll timeout between frames. Measured with
`cargo run -p ui-platform-win32 --example two_windows --release`: two windows
of different sizes, both animating from `cx.now()`, ran 408 frames in 204 turns
over 3 s, each surface drawing in every turn.

The local channel is a named pipe under `\\.\pipe\`, served in message mode so a
read is one whole message. An accept thread hands each client to a reader thread
that wakes the loop through the same event a worker uses, and `poll` only takes
what has already arrived.

Every handle is overlapped: a synchronous handle allows one operation at a time,
so a reader parked in `ReadFile` would block the reply the loop writes on the
same pipe. The pipe takes the process token's default descriptor and sets
`PIPE_REJECT_REMOTE_CLIENTS`, so neither another user nor the network can
connect. A copy of the peer's handle travels with the reply path, which is how
`Reply::deferred` answers from a worker.

Global hotkeys are `RegisterHotKey` against a message-only window of our own,
whose procedure only records the id; `poll` maps it back to the application's
`ShortcutId`. Registering against a window rather than the thread keeps
`WM_HOTKEY` out of every surface's queue, and `Backend::wait` dispatches the
thread's messages after it wakes, so a daemon with no window still receives one.
A chord's required modifiers are what the registry binds; one the chord leaves
optional cannot be expressed.

Not implemented on the process loop: drag-and-drop onto a window. A paste
request is read from the system clipboard instead (`ClipboardReader` is
`Backend::selection_reader`), and a window receives no drop.

A border drag belongs to the window, not to `DefWindowProcW`. A non-client press
on an edge or a corner starts a drag here: the press position and the window's
rectangle are kept, each mouse move resizes through `SetWindowPos`, Escape puts
the rectangle back, and the release ends it. The app loop therefore keeps
running through the drag, and every move is followed by a frame built for the
new size. `DefWindowProcW`'s sizing loop would instead block this thread until
the button came up, which is why the last frame used to be stretched across the
growing window by DWM. A title-bar drag, `SC_SIZE` from the system menu, and
Alt+Space still use that loop, so nothing else in the process draws while a
window is being moved, and a resize that this window performs loses the edge
snapping the shell's own loop gives a dragged border.

A window that declares a minimum has that floor on both routes: `WM_GETMINMAXINFO`
answers `tracked_minimum`, which converts the spec's logical client area into a
window size through the window's DPI and frame, and `sizing_step` clamps the drag
it performs itself to the same value, taking the larger of the spec's floor and
`SM_CXMINTRACK`/`SM_CYMINTRACK`.

A layer surface opens as a `WS_POPUP` window with `WS_EX_TOOLWINDOW`, so it stays
out of Alt+Tab and the taskbar. `Overlay` and `Top` add `WS_EX_TOPMOST`; `Bottom`
and `Background` open as ordinary windows, since Win32 has no supported way to sit
beneath another application. The rectangle comes from the spec: an anchored edge
pins that side, opposite edges together stretch the surface, `margin` is the inset
(and with a single edge the position), and `size` is honoured only where the
anchors leave room; a rect that reaches the monitor edge an auto-hidden taskbar
hides behind gives that two-pixel strip back, or the bar stops coming up, unless
the spec asks for `AutoHide::Cover` instead of the default `Yield` — a surface
whose picture is the screen cannot be shorter than the screen.
`KeyboardMode::None` adds `WS_EX_NOACTIVATE`, `Exclusive` calls
`SetForegroundWindow` once the popup is shown, and the other two modes activate
through `WM_MOUSEACTIVATE` only when the click lands on an area the surface
consumes. `FollowPointer` adds the other half of that rule: the window notes
which window held the keyboard as the pointer arrived, and returns it when the
pointer leaves, since the shell refuses a foreground change from a process that
does not already hold it. Hover alone cannot take the keyboard — Windows has no
focus-follows-pointer contract — so the pointer decides only who gets it back.
A layer's `output` names the monitor it belongs on, as the name
`Backend::outputs` reports it, and an empty or unknown name lands on the primary
one. `ExclusiveZone::Reserve` is still ignored: its Windows counterpart is an
appbar registration, which moves the work area of the whole monitor, and no
surface of this app asks to reserve anything.

The engine publishes the frame's consumed areas through `set_input_region`, and
the shaped window takes a hit test only inside them, which is what lets a
full-screen overlay pass the pointer to whatever is beneath it. Two windows do
it: the input window answers the messages and is shaped with `SetWindowRgn`, and
the window that holds the swap chain is `WS_EX_LAYERED | WS_EX_TRANSPARENT` so
the pointer skips it. Measured with
`cargo run -p ui-platform-win32 --example layers`: on a 1.25-scale monitor a
320x180 layer with a 40,24 margin landed 400x225 at (50, 30), `WindowFromPoint`
returned the layer over its one published patch and the window beneath it
everywhere else in the layer's rectangle, an all-edge anchor covered the monitor
less the auto-hide strip the shell reports, moving off a `FollowPointer` layer
returned the keyboard to the window beneath, and a click on the layer activated
it. The pass to another process was measured with a form of its own, which
logged the press only once the painted window carried both styles.

A copy is published as every format its content kind has, because Windows pastes formats
and not MIME types: text is `CF_UNICODETEXT`, `text/html` is `"HTML Format"` with four byte
offsets around a marked fragment, `text/uri-list` is `CF_HDROP` with the `file:` URIs
percent-decoded into wide paths, and `image/png` is `"PNG"` beside `CF_DIBV5` and `CF_DIB` —
the formats Explorer, Word and Paint read, built by decoding the PNG, since a DIB is pixels.
A MIME type with no Windows equivalent is refused rather than invented. A paste into an
editor is still text only, because `Event::Paste` carries a string.

A command reads the clipboard through the process `Backend`, so `pin add --clipboard` asks
the backend's `SelectionReader` what the copy offers and for one of those MIME types. The
formats present become MIME types, most specific first: `CF_HDROP` is `text/uri-list`, one
`file:` URI a line, written from the wide paths the shell hands over — a drive path under an
empty authority, another machine's share as the URI's host, separators forward and every byte
a URI does not carry escaped from UTF-8; `"PNG"` and a DIB are both `image/png`, the PNG
bytes unchanged and the DIB decoded and re-encoded as PNG so one picture has one MIME; and
`CF_UNICODETEXT` is `text/plain`. Windows copies a format in whole rather than holding it for
a peer to pull, so a read finishes inside the call that starts it and `poll` only hands over
what was copied. The app turns a URI list back into paths and a blob into a pin by its bytes.

Measured with `cargo run -p ui-platform-win32 --example clipboard`, which reads each block
back through the shell rather than through the encoder that wrote it: `DragQueryFileW`
returned `C:\Windows\win.ini` and `C:\a b.txt` from two URIs, the HTML header's offsets
delimited the fragment and ended at the block's last byte, and a 2x2 picture's `CF_DIBV5`
and `CF_DIB` both began with its bottom-left pixel as blue, green, red, alpha. The read side
was measured live: with `[System.Windows.Forms.Clipboard]::SetImage` holding a 126x56
picture — `CF_DIB` and nothing else — `crabsnap pin add --clipboard` pinned a 126x56 bitmap;
`Set-Clipboard -Path` on a 156x80 PNG pinned the file, whose origin read
`file:X:\crabsnap-ng\old\res\capture_pressed.png`; and a copied line of text became a text
pin. Both conversions have a test that pins them:
`a_written_dib_reads_back_as_the_same_picture` reads the DIBs the write side produces back
through the read side, and `a_copied_path_becomes_a_file_uri` in
`crates/ui-platform-win32/src/clipboard.rs`, with `a_windows_file_uri_names_a_drive_or_a_share`
in `apps/crabsnap-core/src/uri_list.rs` for the path shapes the URI list is read back into.

Input methods take one of two paths. A focused field that is not secret is a TSF
document when the active input method is a service this process can host: the window
hands TSF an `ITextStoreACP` holding the field's surrounding text. Every other state —
no field, a secret one, or a service that runs out of process — is IMM32. The split is not a
preference: an input method registered as a `LocalServer32` service answers keys only
through the window's IMM32 emulation, and associating a TSF document with the window
switches that emulation off, so the service then takes no key and composes nothing.
The backend asks the registry which kind the active profile is (`tsf::serves_input`)
before the first attach and keeps IMM32 when the answer is out of process.

TSF is the only Windows path that carries the surrounding text: the store publishes
`ImeState::surrounding` as the document, holds the input method's composition as a
splice at the caret, and maps the composition to `Event::Preedit` and its commit to
`Event::Text`. An edit that removes text beside the caret becomes
`Event::DeleteSurrounding`, with the counts in the engine's UTF-8 bytes although the
store indexes by UTF-16 code units. The document is created and associated with the
window the first time a field asks for it, and if TSF refuses to start, or the input
method cannot be served, the window keeps IMM32 for the rest of its life. While the
document is attached it owns the keyboard: each key is offered through
`ITfKeystrokeMgr::TestKeyDown`/`KeyDown`, `WM_IME_*` is left unanswered, and the
`WM_CHAR` that `TranslateMessage` still makes from a key an input method took is
dropped rather than typed a second time. The store reports its layout
(`OnLayoutChange`) whenever its text or caret moved, because an input method composes
only where it has a rectangle for its candidate window; a frame that changed neither
says nothing.

The IMM32 path carries the rest. A frame's `ImeState` opens or closes the input
context and puts the composition window at the caret, with the candidate list told
to keep off the caret's own rectangle; both forms take window coordinates.
`WM_IME_COMPOSITION` is read for two strings: the composition becomes
`Event::Preedit` and the result becomes `Event::Text`. The IME's own composition
window is suppressed, because the engine draws the preedit, and a secret buffer
suppresses the candidate list too. `WM_IME_CHAR` is ignored: the result string is the
same text in one piece, and the only path that keeps a surrogate pair whole.

Measured with `cargo run -p ui-platform-win32 --example ime`: an unfocused
window left the context closed and a caret opened it, a caret 20 by 40 logical
units placed the composition point at (25, 50) physical on a 1.25-scale monitor
with the candidate form excluding the caret's rectangle, and an injected `a😀b`
composition arrived as a preedit of 6 bytes with the caret at its end, which is
what an injected composition reports.

Measured with `cargo run -p ui-platform-win32 --example tsf_probe`: on this machine
the probe reports `the input method can be served through TSF  no (out of process,
so the window keeps IMM32)` and skips the document checks. What it still checks
passes: a posted `a` arrives as `Event::Text`, and a posted space reaches the engine
as a key once the field is gone.

The store's contract was measured before the gate existed, on the same machine: a
field whose text was `hello world` with the caret at 5 gave TSF a focused document,
and an edit session run under the store's lock read `hello world` back out of it
with the caret at 5. Live composition into that store was never observed here — the
input method this machine ships is out of process, so no window may use TSF for it;
when it was still associated, every key came back from
`ITfKeystrokeMgr::TestKeyDown` as not taken, the context's open/close and conversion
compartments stayed empty, `GetText` was never called, and the typed letters reached
the engine as plain text. Composition through IMM32 is measured by the `ime` example
above. The store's derivation is what the tests in
`crates/ui-platform-win32/src/tsf.rs` pin:
`a_composition_is_one_preedit_and_its_end_one_commit`,
`a_removal_before_the_caret_asks_the_engine_for_surrounding_text`,
`a_removal_counts_the_engines_bytes_not_tsfs_units`, and
`a_range_inside_a_surrogate_pair_snaps_to_a_character_boundary`.

The swap chain follows the output it lands on into HDR. An output the desktop is
composing in `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` gets a
`R16G16B16A16_FLOAT` chain, and the renderer takes a linear destination, so the
output pass writes light rather than sRGB codes. The space the chain declares is
the first one it will present: PQ in BT.2020, or scRGB
(`DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709`) — linear light in sRGB primaries
whose 1.0 is 80 nits, which is what the desktop composites in and what Chromium
declares for windowed HDR. PQ takes BT.2020 through the renderer's gamut matrix;
scRGB shares the working space's primaries and takes the identity. Every other
output keeps the 8-bit sRGB chain.

SDR white is what linear 1.0 is shown at, and DXGI does not report it:
`DXGI_OUTPUT_DESC1` has no field for the value, which belongs to the display
configuration under `DISPLAYCONFIG_SDR_WHITE_LEVEL` and is found by matching that
path's source GDI device name to the monitor's (thousandths of 80 nits: 3000, or
240 nits, on this machine). The panel's own minimum and peak bound the destination's tone
curve.

An output that will not present either space takes the SDR chain instead. The
chain asks first — `DXGI_COLOR_SPACE_SUPPORT_FLAG_PRESENT` from
`CheckColorSpaceSupport`, then `SetColorSpace1` — and a display whose desktop is
not composing in HDR refuses both, which is the branch the earlier probe measured
here. The output is read once, when the window opens: a window dragged from an
SDR monitor to an HDR one keeps the chain it was built with until it is reopened.

Measured with `cargo run -p ui-platform-win32 --example hdr --release` on this
machine's panel (SDR white 240 nits, black 0.035 nit, peak 470, with the desktop
composing in HDR). `CheckColorSpaceSupport` answered 0 for PQ, 3 for scRGB and 0 for the
8-bit space, on a window chain and a composition chain alike, so the probe pins
the scRGB path: the buffer holds linear light in 80-nit units, where sRGB 128
reads back as 0.6471 against `decode(0.502) × 240/80 = 0.6476`, and white — 240
nits, 3.0 — clamps at 1.0 in the 8-bit readback. The probe also refuses a
destination whose transform would leave the codes where sRGB put them.
`window_scene`'s `WINDOW_CHECK` expectations follow whichever branch the
presenter took.

### Invariants

- Premultiplied alpha everywhere; blending is `ONE, ONE_MINUS_SRC_ALPHA`.
- No Y flip for dma-buf targets: GL row 0 is the top of the image.
- `read_pixels` returns straight RGBA; convert to premultiplied BGRA before
  comparing with `ui_paint::Pixels`.
- Required uniforms fail at startup. An unbound sampler does not error; it
  ghosts.
- Partial bands are disabled under supersampling or HDR: the full frame is drawn.
- A band grows over every reader it touches (a frost's box, a layer whose chain
  reads neighbours), and a layer it stays clear of is skipped whole.
- A capture reads the frame before the output pass, so it holds the scene's own
  rendition — sRGB codes on an SDR destination, linear light on an HDR one — never
  the encoded surface; `read_pixels` on the surface returns the bytes the
  compositor got.
- A size change adds full damage. Without it, bands outside the reported
  region keep old pixels.

### Screen capture

`ScreenCapture` duplicates one output and hands back a `ScreenFrame`: straight
RGBA8, top-down, already rotated upright, with the output's GDI device name and
its DPI scale. The duplication returns the desktop's own texture, so the D3D11
device has to sit on the adapter that drives that output; on a hybrid laptop a
device on the other adapter duplicates a black frame.

The duplication asks for `R16G16B16A16_FLOAT` first and `B8G8R8A8_UNORM` second
through `IDXGIOutput5::DuplicateOutput1`. DWM composes an advanced colour
desktop — HDR, or auto colour managed SDR — in linear light, sRGB primaries,
where 1.0 is 80 nits and SDR white sits at the display's own white level;
`IDXGIOutput1::DuplicateOutput` converts that surface to BGRA8 by clipping,
which is where an over-exposed screenshot comes from. The float frame instead
divides by the display's white from `DISPLAYCONFIG_SDR_WHITE_LEVEL` and encodes
sRGB, which returns the codes a window drew. Light past SDR white clips: the
frame is an SDR picture, and a genuinely HDR scene loses its highlights until
the engine can carry half-floats through to a sink.

Measured with `cargo run -p ui-platform-win32 --example capture --release`: a
chart of eight known codes (black, two greys, white, the three primaries, one
mixed colour) drawn into a window and read back out of the duplicated desktop
matched exactly, which pins channel order, position and colour path at once.
The same chart through `DuplicateOutput` read 32 as 59 and 128 as 210 — linear
light times the display's 240-nit white, clipped — and white and every primary
at 255, which is what asking for floats removes.

Two frames are not the desktop: one whose `LastPresentTime` is zero carries the
pointer and nothing else, and the first frame after `DuplicateOutput` is empty
in the desktop's own packing. `grab` skips both until its deadline. The packing
comes from each frame's own texture, not from `GetDesc` at duplication time,
which describes the mode rather than what `AcquireNextFrame` hands over.

The app picks a Windows rung at runtime: `apps/crabsnap-platform-win32/src/capture.rs`
tries Windows Graphics Capture, then Desktop Duplication, then GDI, and the
`capture.backend` setting pins one of the three. Graphics Capture leads because a
duplication holds the output exclusively for the length of the grab, which takes
the desktop away from anything else recording it, while a capture session holds
nothing and still answers on a still desktop. The session is free-threaded
(`CreateFreeThreaded`), so it needs no dispatcher queue, sizes its pool for half
floats when the output composes in advanced colour, and shares the duplication's
staging readback, so both rungs hand back the same `ScreenFrame`. Opening one in
release on this machine: 153 ms for the process's first D3D11 device, 59 ms for
the item, pool and session, and 11 ms more until the first frame is read.

A session is also closed, not merely released: Windows holds its frames, its
plumbing and the device behind its pool until `Close()`, and one grab is one
session. `Drop for GraphicsCapture` closes the session, then the pool.

## CPU path

`ui-paint` uses zeno for coverage and reads the same glyph atlas. It repaints
the whole surface and reports `Damage::Full`. It is the fallback on machines
without dma-buf or a usable GPU, and the reference for every primitive in the
`DrawList`.

A texture and a shadow are where the two paths agree closest, because their
picture is a function of a distance rather than of edge coverage. A texture is
stretched bilinearly over premultiplied texels, sampling at `u * src - 0.5` so
texel centres sit half a texel in. A texture whose entry asks for
`Sampling::Pixelated` takes the nearest texel on both paths instead — the GPU
sets `GL_NEAREST` on that texture object, the CPU reads the texel the sample
lands in — and because the filter belongs to the texture, the GPU cache is keyed
by picture and sampling and one file drawn both ways holds two textures. A shadow is the silhouette's signed distance
through the normal CDF, with `blur` as two sigma. `ui-paint` evaluates the same
`sd_round_box` the shaders do — it is what feathers every fill too — so
`gpu_image_scene` and `gpu_shadow_scene` compare the two renders and find no
difference at their probes.

A shadow stops at the surface it belongs to. `ShadowPrim.bound` names that box
and which side of its edge the cast lives on: an outer cast
(`ShadowSide::Outer`) paints only beyond the edge, an inset one
(`ShadowSide::Inset`) only within it, each feathered over one pixel like a
fill's own edge. That is how CSS clips a `box-shadow` inside the border box and
an inset one outside the padding box (CSS Backgrounds 3 §7.1). A cast with no
bound paints freely, as the glass rim does. An inset cast's falloff is the same
CDF mirrored, so the surface edge carries half the peak on both sides.
`Shape::Box` bounds every shadow at its own box and paints an inset one after
its fill, above the background.

The glyph atlases are sampled with `LINEAR` on both sides of that rule: a glyph's destination
carries the fraction its subpixel phases did not take (up to a quarter pixel vertically), and a
1:1 tile still returns exact texels because fragment centres land on texel centres. The CPU
rasteriser rounds a glyph's destination instead and drops that fraction, so its text steps
where the GPU's glides; the tiles themselves are already padded, so a blend at a tile's edge
reaches only transparency.

There is no automatic GPU-to-CPU fallback: if `zwp_linux_dmabuf_v1` is absent,
session creation fails with `NoDmaBuf` unless the CPU presenter was requested
(`GALLERY_CPU=1` in the gallery).

## What the two paths must not share

Anti-aliasing is implementation-defined. The CPU rasteriser computes analytic
coverage per pixel; the GPU feathers an SDF or a distance to the edge. Their
edge pixels differ by a few levels, and demanding equality would force one
backend to imitate the other's coverage rather than its result.

The shared contract is therefore the plan, not the pixels:

| Shared | Separate |
| --- | --- |
| Declarations, tree, layout, hit, events, animation, damage | Coverage at an edge |
| Primitives and batch order | The rasterisation mechanism |
| Effect parameters and semantics | The filter implementation |
| The `PresentTarget` contract | Buffer allocation and format details |

The invariants that replace pixel equality are measurable: an opaque 100×100
square covers the same area on both paths (within 0.5%), alpha along a slanted
edge is monotone, and compositing never overflows or goes negative. The GPU
examples assert these against the CPU renderer for the primitives the CPU
draws; a new primitive must extend both the CPU rasteriser and those checks,
because two backends that are never compared drift within months — the image and
shadow scenes each found defects on their first comparison run.

## Why not the obvious alternatives

- **wgpu**: no dma-buf export under our control, so the zero-copy chain
  breaks. The rejection was made against a specific version; re-check before
  reopening it.
- **Sharing triangles between CPU and GPU**: loses SDF coverage and
  instancing, and needs a second triangulation path.
- **Two independent renderers**: they drift. The shared contract is the
  `DrawList`, not an implementation.

## Traps

| Change | Symptom |
| --- | --- |
| One shm pool or offset 0 for both slots | Compositor samples memory being rewritten: tearing, stale rows |
| Report only the diff while the CPU path repaints everything | Kept pixels in unreported areas |
| Resize without full damage | Stale bands, or garbage at the edges |
| Flip the projection Y for dma-buf | The whole frame is upside down |
| Compare `read_pixels` with `Pixels` directly | A near-100% difference from alpha convention |
| Use only `drmSyncobjImportSyncFile` for a release point | The point never advances; reuse never unblocks |
| Skip `glFinish` when explicit sync is absent | A black window with a correct readback |
| Set uniforms before `use_program` | A sticky `GL_INVALID_OPERATION` blamed on a later call |
| Render on a `renderD*` that does not drive the connected output | Import and release succeed; the output is black |
| Keep cached `wl_buffer`s across reallocation | A reused inode shows the previous allocation's content |
| Draw a new primitive on the GPU only | The CPU presenter silently omits it, and nothing counts the omission |
| Filter straight-alpha texels | A transparent texel's colour tints the edge |
| Map the destination box onto texel indices | Every texel boundary sits half a texel early |
| Mix a logical rect with a device-pixel band | The blur covers a fraction of the box at fractional scaling |
| Square every corner the intersection matches no clip | A scrolled rounded card leaks through its own corner |
| Index a sampler array by a uniform, as GLSL does | `X3512: sampler array index must be a literal expression` |
| Declare a convolution's weights as `float weights[33]` | One register per element: the next member's `packoffset` collides |
| Pass a mip-chained target to `CopyResource` | The copy fails, silently, and the readback is zeros |
| Clear a damage band with `ClearRenderTargetView` | The whole frame is cleared and the band's promise is gone |
| Leave `SV_POSITION` out of the pixel shader's struct | Every input reads its neighbouring register |
| Draw the painter straight into a flip-model back buffer | The damage bands assume a previous frame the discarded buffer never held |
| `CopySubresourceRegion` a frame into a `B8G8R8A8` view | Channels are copied, not converted: red and blue swap |
| Read a flip chain's buffer 0 after `Present` | After the flip it is the next, blank storage |
| `ResizeBuffers` while a target still holds the back buffer | The call fails on the live reference |
| Give a `GENERATE_MIPS` texture only one bind flag | `CreateTexture2D` returns `E_INVALIDARG` |
| Store the window state behind a null check in the procedure | `WM_NCCREATE` is the message that hands it over, so it takes the same early return: no input, no redraw |
| Wait for messages with `INFINITE` | An app's own deadline is only noticed between turns, and the run overshoots it |
| Let a `WM_PAINT` reach `DefWindowProcW` unpainted | It is reposted until the thread starves; validate it and let the frame loop paint |
| Declare PQ on a chain without asking the output | An SDR-composed output refuses `SetColorSpace1`; nothing is presented into a chain that cannot show |
| Look for SDR white in `DXGI_OUTPUT_DESC1` | The struct has no such field; the value is in the display configuration, on the path whose source names the monitor |
| Capture through `IDXGIOutput1::DuplicateOutput` on an advanced colour desktop | It converts the composed float surface to BGRA8 by clipping: SDR windows arrive over-exposed (32 reads 59, 128 reads 210) |
| Trust the format `GetDesc` reports when the duplication opens | It describes the mode, not the texture `AcquireNextFrame` hands over |
| Take the first frame after `DuplicateOutput` as the desktop | It is empty; a pointer-only frame has `LastPresentTime` zero and no image |
| Duplicate on a device from another adapter | A hybrid laptop gets a black frame |

## Corrections

**The CPU path drew no texture at all.** `draw_batch` counted every `Image` into
`stats.skipped_primitives`, so under `GALLERY_CPU=1` a wallpaper page was blank
while the same `DrawList` rendered on the GPU — the primitive was implemented
twice only in name. `draw_image` now converts the destination box to device
pixels, feathers it through the same `coverage_rounded_rect` the fills use, and
samples `prim.src` bilinearly with edge clamping. The sample interpolates
premultiplied channels and unpremultiplies the result, because a straight-alpha
lerp lets a transparent texel's colour reach the edge, which the GPU sampler does
not do. Caught by `a_texture_is_sampled_the_way_the_sampler_does` in
`crates/ui-paint/src/tests.rs`, which was watched red three ways: with the draw
removed, with nearest sampling, and with straight-alpha lerp.

**A shadow had no CPU implementation either, and the shape existed twice.**
`Shadow` shared `Image`'s skip arm, so `GALLERY_CPU=1` drew no elevation at all.
Filling the gap exposed the second half: `coverage_rounded_rect` tested each
corner separately while the shaders evaluated `sd_round_box`, two derivations of
one silhouette that a falloff would have had to guess between. The renderer now
has one `sd_round_box` and the fill's coverage is its `(0.5 - d)` feather — the
shader's `AA` constant is 0.5, so the fill pixels are unchanged — and `draw_shadow`
puts that same distance through the same Abramowitz & Stegun CDF the shader
carries, scaled into device pixels. Caught by
`a_shadow_fades_from_its_edge_like_a_gaussian_cast` and
`a_shadow_covers_the_device_pixels_of_its_caster` in `crates/ui-paint/src/tests.rs`,
whose expected alphas (102 and 115 at the edge, 27 one sigma out, 218 on a ring)
are the CDF evaluated by hand; each half was watched red separately, with sigma
unscaled and with the ring's band dropped. `gpu_shadow_scene` compares the two
renders at six probes and finds them identical. The widget/literal parity test
also failed, which is how the gap showed: its hand-written scene had never
expressed the pack's overlay elevation, and a skipped primitive hid the omission.
The literal now states `.elevation(3)`, so that test covers a shadow too.

**`gpu_shadow_scene` had been failing on two stale thresholds.** The scene
demanded a cast of 0.10 six pixels outside the edge, which was right when `blur`
was read as sigma; since `blur` became two sigma (CSS and Skia) the correct value
there is `0.5 · CDF(-1.1) = 0.068`, and the last probe sat 11.5 pixels out, where
the tail is still 0.005 rather than the asserted 0.002. Both probes now state
the model: the edge pixel carries half the peak, and the zero is measured at
three sigma.

**The cross-backend check found two more image defects.** `gpu_image_scene` now
rasterises each scene on both paths and compares region means. The first run
disagreed by 157 levels on a seam between a transparent and an opaque texel,
because the GPU uploaded straight-alpha texels and let `GL_LINEAR` filter them —
a transparent texel bled its colour into the edge, against the premultiplied
invariant the frame's blend equation assumes. The upload now premultiplies and
`image.frag.glsl` applies one coverage factor. The same run showed an opaque
scene off by 2.2: the CPU mapped the destination box straight onto texel
indices, putting every texel boundary half a texel early. Both paths now sample
at `u * src - 0.5`; the measured gaps fell to 0.02 and 0.5. Caught by
`[12]`–`[15]` in `crates/ui-paint-gpu/examples/gpu_image_scene.rs`.

**A rounded clip lost every corner when another clip cut it.** `plan_batches`
intersected the clip stack's rects and then looked for a clip whose rect equalled
the intersection; when none did — a card scrolled halfway out of its viewport —
it fell back to square corners, so a child's square fill bled past the card's
visible curve. It also squared the card when a *square* clip merely coincided
with the rounded one, whose region contains the square's. The batch now takes
its corner shape from the innermost rounded clip and keeps each corner only
where that clip still owns it: a corner the intersection cut through is
straight, which is what the cut is. Caught by
`a_rounded_clip_keeps_the_corners_a_square_clip_did_not_cut` in
`crates/ui-core/src/draw.rs`. Two *rounded* clips intersecting still resolve to
the innermost one's arcs; a complete answer needs the clip stack in the
backends instead of one `ClipEntry` per batch.

**The CPU frost ignored the box's shape and its scale.** `Band::write_into`
copied a rectangle, so under `GALLERY_CPU=1` a rounded panel's blur showed
square corners outside the arc; and the band was built from the logical rect on
a device-pixel buffer, so at 2x it covered the top-left quarter of the panel
with a blur half as wide. The frost now converts its area, sigma and corner
shape into device pixels and feathers the write through the same
`coverage_rounded_rect` the fills use. Caught by
`a_rounded_frost_keeps_the_pixels_outside_its_corners` and
`a_scaled_frost_covers_the_boxes_device_pixels` in `crates/ui-paint/src/tests.rs`.
The group-effect half of the same defect was the next one: `composite` passed
the authored kernels straight to `apply_effects`, so a `.blur(4)` on a 2x grid
blurred 4 device pixels. Both backends now convert through
`ui_core::effect::on_grid`, which moved out of the GPU crate so there is one
definition; pinned by `a_group_blur_widens_with_the_render_grid` in
`crates/ui-paint/src/tests.rs`.

**A shadow painted through its own surface, and no inset form existed.**
Neither `draw_shadow` nor `shadow.frag.glsl` cut the cast at the caster's edge,
so a `Shape::Box` shadow carried its peak alpha across the whole box and only an
opaque fill hid it. Any translucent surface showed the cast as a rim inside its
own edge: the gallery's markdown code card tints its body at 6 %, and on the
markdown page (release build, wayland-2, 2x) its fill read 179 inside the card,
ramping to 196 at the card's edge, against 251.7 on the page outside. CSS clips
an outer `box-shadow` inside the border box and an inset one outside the padding
box (CSS Backgrounds 3 §7.1), and Skia culls the geometry behind an opaque
occluder unless told the occluder is transparent
(`kTransparentOccluder_ShadowFlag`, `SkShadowUtils::DrawShadow`). `ShadowPrim`
now carries `bound: Option<ShadowBound>`, a box plus a `ShadowSide`; both
renderers zero the cast on the wrong side of that edge over one pixel of
feather. `Shape::Box` bounds every shadow at its own box, and `ShadowSpec.inset`
selects the mirrored falloff painted after the fill. The same row measured again
after the change holds 218.3 flat to the card's edge, with the shadow ramping
only outside it (213.7 at the edge, 232 six logical pixels out). Caught by
`a_shadow_stops_at_its_surfaces_edge` in `crates/ui-paint/src/tests.rs`, watched
red with the cut removed (an outer cast read 153 inside the surface) and with
the inset falloff unmirrored (197 where the inner edge reads 58);
`gpu_shadow_scene` now renders a translucent surface and an inset one and
compares both paths at eleven probes.

**A node's own clip cut its own shadow.** `paint_node` pushed the node's
declared clip before `paint_self`, so a clipped container — a scrolled card, a
popover surface — lost its own `box-shadow` at its padding box, and a leaf with
a clip lost it to the overflow branch as well. In CSS a clip applies to the
box's content (CSS 2.1 §11.1.1), and a `box-shadow` is not content: it is drawn
outside the border box (CSS Backgrounds 3 §7.1). Every ancestor's clip is also
already on the stack when a node paints here: the mask only narrows where a node
declared a clip, and that node pushed it. The clip now goes on after
`paint_self` and before the children, and the branch that re-pushed an ancestor
rect for the node's own ink is gone with it — redundant where an ancestor owned
the clip, harmful where the mask reflected the node's own. Caught by
`a_nodes_own_clip_cuts_its_children_but_not_its_own_shadow` in
`crates/ui-platform/tests/clipping.rs`, watched red with the push moved back
before `paint_self` (the page half a pixel outside the box read 255 instead of
153); `a_rounded_parent_clips_its_child_to_the_curve` pins the child side. Press
ink, the scrollbar and the focus adorner still paint inside the clip.

**The CPU path cannot be the reference for a second backend.** The Windows port
began from the assumption that `ui-paint`'s output defines correct pixels. It
does not: the CPU path skips canvas compositing, layer effects and backdrop
materials, so a backend that agreed with it would have baked those gaps in. The
GPU path is the reference instead, and it had to be made portable before a
second API could implement it: the painter moved from `ui-paint-gpu` into
`ui-gpu-scene`, which reaches the driver only through the `Device` trait, and
`ProgramId`, `TextureId` and `TargetId` are opaque handles a backend decodes its
own way. `ui-paint-gpu` is now just the GL implementation of that trait.
Verified as far as this machine allows: `ui-gpu-scene` builds on its own and its
unit tests pass, which covers the batch and scissor geometry and not the device.
The GL backend, the Wayland wiring and every example still need a Linux run
before this paragraph can claim more.

**The D3D11 readback returned zeros from a frame that had rendered.** `read_pixels`
copied the target with `CopyResource` into a one-level staging texture, and the
targets carry a full mip chain so that `set_sampling(.., mips)` works on any of
them; D3D11 wants both sides of `CopyResource` to agree on dimensions, format
*and* mip count, so the call failed and the example read the staging buffer's
zeros. The device's own copy has no error return to check, which made the
failure look like a broken clear or a broken draw: a probe that cleared a target
red and blitted it into a second one showed both working before the readback
did. The readback now copies subresource 0 with `CopySubresourceRegion`, which
does not care how many levels the source has. Caught by `[1]` of `d3d11_scene`,
which failed with an all-zero readback; the example is what makes the silent
path visible at all.

**Every D3D11 varying arrived as zero.** The pixel shader's input struct did not
declare `SV_POSITION`, so its inputs were numbered from register 0 while the
vertex shader's outputs start at register 1, and the rasteriser fed each input
from the neighbouring register instead of renumbering the pair. The symptoms
pointed elsewhere: every shape drew as a full quad in its instance colour, in the
right place, and a vertex shader that wrote a constant into a varying still lost
it. It took a fragment shader that returned `v_local` and printed the same value
at every pixel to show the inputs were not the ones the vertex stage wrote. `PsIn` now opens with `SV_POSITION` like
`VsOut`, so the two signatures describe one register layout. Caught by `[2]` and
`[7]` of `d3d11_scene` — a rounded corner read as filled and the sector's arc
read as empty — which are the two checks that only pass when a *shape*, not a
rectangle, reaches the target.

**No D3D11 texture bound both `SHADER_RESOURCE` and `RENDER_TARGET`, so the
glyph atlas could not be created.** The device's texture helper added the
render-target binding only for textures that were targets, while every texture
carried `D3D11_RESOURCE_MISC_GENERATE_MIPS`, because `set_sampling(.., mips)`
may ask for a chain on any of them. D3D11 refuses that misc flag unless both
bindings are present, so `CreateTexture2D` returned `E_INVALIDARG` — reported
as `atlas upload: texture: 参数错误` on the first frame that drew text.
`d3d11_scene` draws shapes only, so no example had exercised the path. Every
texture now binds both, which is also what the chain needs: `GenerateMips`
reads a shader resource view and writes a render target view of one resource.
Caught by `window_scene`'s first frame, the first thing in the tree to draw
text through the device.

**The Win32 window took no input and never redrew.** `window_proc` read
`GWLP_USERDATA`, and when it was null it returned to `DefWindowProcW`.
`WM_NCCREATE` is the message that carries the state pointer in
`CREATESTRUCTW.lpCreateParams`, so it took that same early return and the
pointer was never stored: every message after it went to the default procedure,
which is why the window appeared, painted once, and then only changed when DWM
stretched a frame across a resize. `WM_NCCREATE` is now answered before the
lookup, and the extra strong count taken for the procedure is released in
`WM_NCDESTROY`, which runs even when a later step of creation fails. Caught by
`cargo run -p ui-platform-win32 --example input_probe`, which posts a move, a
click, a notch, a character and a key and requires all seven translations, and
by posting a real click at the gallery's switch, which flipped it and built the
next frame.

**The Win32 message wait had no deadline.** `wait_until(None)` passed `INFINITE`
to `MsgWaitForMultipleObjectsEx`, so a run with `GALLERY_SECONDS=2` took 8.6 s:
the gallery's lifetime, like a scripted probe's next step, can only be noticed
between turns, and the loop was asleep until unrelated input woke it. The wait
is now the caller's timeout capped at one second, with 250 ms when there is
none — the same bound the Wayland backend carries.

**The Windows host loop slept through an animating surface.** The surface's
`next_deadline` reported only the driver's deadline, and a turn that drew clears
that deadline, so a build driven by `cx.now()` left the loop with nothing to
wake for: it slept out the poll bound between frames. The Wayland path hides
this, because there the compositor's frame callback is an event on the waited
descriptor; on Windows nothing arrives after `DwmFlush` returns and the loop
must be told to come straight back. `Session::next_deadline` now also reports
"now" while the engine is animating, which is the same flag that makes a
time-reading control owe the next frame. Caught by
`cargo run -p ui-platform-win32 --example two_windows --release`: two
`cx.now()`-driven windows went from 5 turns and 10 frames in 3 s to 204 turns
and 408 frames, each surface drawing in every turn.

**The named-pipe server deadlocked on its own reply.** The first version created
the pipe without `FILE_FLAG_OVERLAPPED` and gave each connection a reader thread
parked in `ReadFile`. A synchronous handle allows one operation at a time, so
the `WriteFile` the handler made on the same handle waited behind that pending
read, and the client waited for the write: the probe printed the message
reaching the handler and then nothing, with no error anywhere. Every pipe handle
is now overlapped and each operation completes through `GetOverlappedResult`
with its own event, so a read and a write can be in flight together. Caught by
`host_services`, which sends a message from a client thread and requires the
answer back by the time the run ends.

**`FollowPointer` could not hand the keyboard back, because the report it waited
for never came.** The port plan answered a click over a consumed area with
`WM_MOUSEACTIVATE` and restored the previous foreground window on
`WM_MOUSELEAVE`, triggered by the engine's per-frame "the pointer is on what
this surface consumes" report. Two things were wrong with that. A still pointer
builds no frame, so a surface that had gone idle — the normal state between
touches — never received the report the hand-back hung on: measured in the app,
the foreground stayed on the canvas indefinitely after the pointer left a pin,
because the leave had been seen but no frame ever carried it to the target. And
the window to restore cannot be read at `WM_MOUSEACTIVATE`: the activation has
already cleared the foreground by then and `GetForegroundWindow` answers
`HWND(0)`, so nothing was ever stored. The window now records the foreground on
each movement over a consumed area and hands it back in the leave handler, where
the event actually arrives; the engine's per-frame report is not consulted on
Windows at all. Measured in the app: clicking a pin takes the foreground and
moving off returns it within 15 ms. Caught by `examples/layers.rs`, which clicks
a plain window, clicks a `FollowPointer` layer over it, and requires the
foreground back on the plain window after the pointer leaves — pumping the queue
between a click and its answer, because the activation is dispatched with the
message rather than with the call to `mouse_event`. The same probe pins the
placement arithmetic (a 320x180 layer with a 40,24 margin at (50, 30) on a
1.25-scale monitor), the `MA_ACTIVATE` and `MA_NOACTIVATE` answers for each
mode, and the input region (`WindowFromPoint` returns the layer over its
published patch and the window below it elsewhere).

**A composed layer could be painted or clicked, not both.** A swap chain shown
through an HWND is opaque, so a layer's transparency needs
`CreateSwapChainForComposition` and a visual; but the window that carries that
visual still takes every click over its whole rectangle. `SetWindowRgn` shapes
hit testing, and it also clips composed content, so shaping the window that
presents the frame turned the pins' surroundings transparent, edges and all,
along with their clicks. Passing a click to another process needs
`WS_EX_LAYERED | WS_EX_TRANSPARENT`, not `WS_EX_TRANSPARENT` alone: a form in a
second process logged nothing at the point until both styles were set, and with
both it logged the press. A layer therefore opens as two windows — an input
window, `WS_EX_NOREDIRECTIONBITMAP`, shaped with `SetWindowRgn` to the areas its
frame publishes, and a visual window owned by it, carrying
`WS_EX_LAYERED | WS_EX_TRANSPARENT` and holding the swap chain — so what is
painted and what is clicked can differ. `WM_NCHITTEST` answering `HTTRANSPARENT`
was tried first and cannot work across processes: the answer only routes within
the thread that gives it. Leaving the region does produce a `WM_MOUSELEAVE` for
a shaped window on this build, so the departure needs no polling timer.
`examples/layers.rs` requires the leave, the click-through, and the style on the
painted window.

**The candidate list was placed in screen coordinates and read as window ones.**
`ImmSetCandidateWindow`'s `ptCurrentPos` is relative to the client area's
upper-left corner, for `CFS_CANDIDATEPOS` as much as for the composition form,
and the first implementation converted the caret with `ClientToScreen`. The list
therefore appeared one window offset down and right of the caret and followed
the window anywhere it was dragged. Measured with a throwaway probe that opened
a window at (400, 300), put the caret at screen (459, 413), typed a real Pinyin
composition and captured the screen: the list sat at (870, 805), and at a
different place again on a second run, because it was never anchored to the
caret at all. Both forms now take window coordinates and the candidate form uses
`CFS_EXCLUDE` with the caret's rectangle, so the system places the list where the
caret is not; the same probe then found it at (460, 438), directly below the
caret. `examples/ime.rs` asserts the exclusion style, the point and the rectangle
it reads back.

**The HDR design took SDR white from DXGI, which does not carry it.** The
research note listed `DXGI_OUTPUT_DESC1::SDRWhiteLevel`; the struct has
`MinLuminance` and `MaxLuminance` and no white level, and `IDXGIOutput6` offers
no other query for one. The value lives in the display configuration:
`DISPLAYCONFIG_SDR_WHITE_LEVEL` on the path whose
`DISPLAYCONFIG_SOURCE_DEVICE_NAME` equals the monitor's `szDevice`, in
thousandths of 80 nits. With it, linear 1.0
in the scene is the desktop's own SDR white rather than a guess. The same pass
found the other half: an output whose desktop is not composing in HDR refuses the
PQ colour space, so the chain asks `CheckColorSpaceSupport` first and keeps the
8-bit chain when the answer is no — forced through anyway, the D3D11 encode
still matched `encode_for_pq_output` to 0.0007 in an SDR buffer.

**The HDR chain was assumed to be PQ, and this panel presents scRGB.** The float
chain declared `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` and dropped to the
8-bit chain whenever `CheckColorSpaceSupport` said no, which was read as "this
display is not showing HDR". On an HDR desktop this machine answers 0 for PQ, 3
for scRGB and 0 for the 8-bit space — the same on a window chain and on a
composition chain — so every surface presented SDR while the captures already held
float light, and the HDR went missing at the last step: a picture that captured
240-nit white looks like a program that did not capture HDR at all. scRGB
(`DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709`) is the space DWM composites in and the
one Chromium declares for windowed HDR, so the chain now takes the first space it
will present and the renderer carries that choice into a new `OutputScRgb` pass,
which scales the scene's white into 80-nit units and writes the light unencoded.
Caught by `examples/hdr.rs` on both chain kinds: sRGB 128 reads back 0.6471
against `decode(0.502) × 240/80 = 0.6476`, and the probe fails a destination whose
transform would leave the codes where sRGB put them.

**Capture took the desktop's 8-bit surface at face value.** `IDXGIOutput1::DuplicateOutput`
always hands back BGRA8, and on an auto colour managed desktop that is the composed float
surface clipped: the first probe compared one drawn colour with the captured one and read
[255, 56, 150] where [220, 30, 90] had been drawn. A chart of eight known codes pinned the
rule — 32 read as 59, 128 as 210, white and every primary at 255 — which is linear light
multiplied by the display's SDR white and clipped, so nothing above roughly 158 in sRGB
survived. The duplication now asks `IDXGIOutput5::DuplicateOutput1` for
`R16G16B16A16_FLOAT` first, divides the frame by the white level from
`DISPLAYCONFIG_SDR_WHITE_LEVEL` (raw 3000 here, 240 nits — the earlier claim of 1000 in this
document had no measurement behind it) and encodes sRGB; the same chart then read back
exactly, and `examples/capture.rs` checks all eight codes. The same pass found that the frame
right after `DuplicateOutput` is empty and arrives in the mode's packing, and that a
pointer-only frame carries a zero `LastPresentTime`; neither is the desktop.

**A full-monitor layer made an auto-hidden taskbar unreachable.** With "hide the
taskbar automatically" on, the shell stops revealing it on edge hover for as long
as a window covers the monitor edge the taskbar hides behind — its full-screen
detection. An overlay surface is exactly that: the monitor's rectangle, topmost,
present the whole time. Every toolkit that fills the screen answers the same
way: Chromium leaves `kAutoHideTaskbarThicknessPx` (2 px) of the auto-hide edge
to the desktop, Windows Terminal does the same for a maximized or fullscreen
window, and `DefWindowProc`'s own maximize path leaves a one-pixel sliver for
this reason. A layer whose rect reaches the edge an auto-hidden taskbar hides
against now gives that strip back, asked of the shell through `ABM_GETSTATE` and
`ABM_GETTASKBARPOS` so a visible or differently placed taskbar costs nothing.
Caught by `examples/layers.rs`, which fills the monitor and requires the
resulting rect to be the monitor less the strip the shell itself reports; the
reveal was watched by hand — with the strip the bar comes up at the edge, and
without it the edge stays dead.

**Every D3D11 image was sampled from a mip level nobody had written.** All
textures in the device carry a full mip chain, because `set_sampling(.., mips)`
may ask for one, but an image's chain is never generated: the upload passes
`mips: false`. The sampler asked for a mip-filtering minimum with
`MaxLOD: f32::MAX` anyway, so as soon as minification crossed LOD 0.5 — a scale
below 1/√2, the first zoom stop past 70.7 % — it read level 1, which held
whatever `CreateTexture2D` left there. A pin then drew as its own background:
transparent except a one-pixel band at the left edge, which the filter still
took from level 0. The sampler's `MaxLOD` is now 0 for a texture without mips,
which pins minification to the level that was written. Caught by zooming a pin
out in the app: at 0.707 an unpatched build left a 786-pixel band where the
400x300 image had been, and the patched one drew the whole face at 0.707, 0.59
and below.

**Every surface compiled nineteen programs before it could show anything.**
`GpuRenderer::new` built all of them through `D3DCompile` while the presenter
was opening, though a window that draws shapes, one image and the output pass
needs none of the glass. Measured in release on Windows 11, 2560x1440 output:
the presenter took 916 ms to open, of which `GlassKit` cost 346 ms and
`GlassKitLinear` 367 ms, against 3 to 20 ms for everything else; the screenshot
overlay's first frame then drew three programs in 26 ms. Programs are now built
on first use and compiled once per process into a per-key `OnceLock`, so that
open takes about 24 ms, and the presenter warms the rest on a worker after its
first frame is presented. The same command end to end — `crabsnap shot region`
with `CRABSNAP_SECONDS=1`, three runs each — went from 2215-2252 ms to
1341-1435 ms, and the window's content is on screen about 390 ms after launch
instead of about 1.3 s. What the remaining time is: the process's first D3D11
device costs about 153 ms — the runtime and the display driver loading — and the
capture rung creates it, which is why the presenter's own device then costs 8 ms;
the capture item, pool and session add 59 ms, and the first frame 11 ms. Nothing
pins the timing; what covers the lazy path is that `d3d11_scene` and
`window_scene` build the programs they draw, so a program that stopped building
fails those examples, and a key a frame never draws is a cost deferred, not a
program lost.

**The first frame converted a megapixel picture one allocation at a time.**
`upload_image`'s conversion branch allocated a `Vec` per texel while premultiplying
and encoding, so a 2560x1440 capture spent 303 ms preparing a texture whose
actual upload was 7 ms — the dominant cost of the frame that followed it. It now
writes into one pre-sized buffer, which puts the same conversion at 82 ms; the
rest is the conversion itself, table lookup and half-float encode per texel.
Measured in release on the same output. The path stays a full pass because the
destination's encoding and the source's colour space can differ, and doing it on
the GPU needs a shader per source/destination pair the scene does not have yet.

**The strip an auto-hidden taskbar keeps squeezed the frozen picture.** The
give-back above was written for every layer that reaches the edge, and the
capture overlay is one: on a 2560x1440 output with the taskbar hiding at the
bottom, the overlay opened 1438 rows tall and drew the frozen frame into it with
`ImageFit::Fill`, so 1440 picture rows landed in 1438 surface rows. Every mark
and every crop sat up to two pixels off from the pixel it was drawn over, and the
screen's last two rows could not be selected at all. `LayerSpec` now says which
it wants: `AutoHide::Yield` — the default, which keeps the bar reachable — or
`AutoHide::Cover`. The overlay covers the edge, because a picture that is the
screen has to be the screen; the auto-hidden taskbar stays down while the overlay
is up, which is the same trade a modal selection makes, since the overlay is
opaque and would swallow the click the bar was brought up for. A pin canvas keeps
yielding: pins outlive the capture they came from, and a taskbar that could not
be brought up for as long as one is on screen would be the worse bargain. Caught
by the overlay's own window: `EnumWindows` and `GetWindowRect` report 0,0
2560x1440, and `examples/layers.rs` now opens a `Cover` layer and requires the
monitor rect; that check printed OK on this machine.

**Every screenshot left its capture session behind.** `GraphicsCapture` dropped
its pool, session and item the way one drops a DXGI object, and a WinRT capture
session keeps the compositor's frames, its share of the desktop's capture
plumbing and the device behind the pool alive until `Close()` is called. Because
the daemon opens one rung per shot, each region capture after the first cost
about 33 MB of working set and 152 handles: a probe that opened, grabbed and
dropped five sessions in one process climbed 133 to 1075 handles, 153 per
session, while the daemon itself read 146 MB before the first shot and 180, 213,
246 MB after the next three. `Drop for GraphicsCapture` now closes the session
and then the pool, which is the root fix rather than a cap on the leak: the
session is per grab by design, since a pool only hands over a frame when the
desktop changes. Caught by the same probe, since removed — 30 sessions hold one
working set of about 31 MB and end 3 handles apiece above the start, and the
duplication rung leaves the same residue, so those belong to the per-grab D3D11
device and not to the session. Nothing in the suite pins it: a capture session
needs a display, and the check that catches a regression is cycling sessions in
one process and reading its handle count.

**A lost device took the window with it.** A driver reset, an adapter removal or a TDR left
every D3D11 call failing and the presenter reported the failure as final, so the surface closed
— and before the loop's error policy changed, the process with it. `Win32Presenter::device_loss`
now asks `ID3D11Device::GetDeviceRemovedReason`, which keeps answering after the device is
gone, and `Session::draw` rebuilds the presenter on that answer: a new device, renderer, swap
chain and composition, which is what a flip-model window needs since it holds one chain at a
time and the old one has to leave its visual first. What is not GPU state survives — the
supersampling factor is re-applied, and the glyph atlases and registered images re-upload
because a fresh presenter and renderer start with empty caches. A failure with a healthy device
is still reported unchanged rather than retried. `WM_DISPLAYCHANGE` asks for the same rebuild,
because a chain's format and colour space are decided when it is created and the output under
the window may now present HDR where it did not, or the reverse. The removal path was
observed once, on the report that a graphics driver update ran while the daemon and its
windows were on screen and the program kept working: the process survived the removal a
driver update causes and kept drawing afterwards. What that observation does not say is
which trigger answered — the device-removed reason or a `WM_DISPLAYCHANGE` — or how many
rebuilds ran, since nothing counted them; if both miss, the failure reaches the loop, which
retires just that surface.
The rebuild itself was exercised by posting `WM_DISPLAYCHANGE` twice at a live settings window:
each rebuild re-made the chain and the window kept painting, which is what proves the old chain
leaves the window before the new one attaches — a second flip chain on one window is refused.

**The clipboard was write-only on Windows.** The backend published every Windows format a
content kind has, but `Backend::selection_reader` kept its default `None`, so `pin add --clipboard`
— the one command whose whole purpose is another application's copy — reported "the clipboard
offers nothing a pin can show ()" whatever had been copied. The backend now owns a
`ClipboardReader` and the formats present become MIME types. Two traps surfaced only by running
it: `image`'s `BmpDecoder::new_without_file_header` skips twelve bytes after a V4 or V5 header,
believing the bit masks follow it, so a `CF_DIBV5` read through it ran past the pixels and
failed with "failed to fill whole buffer"; a `BITMAPFILEHEADER` is now prepended instead, and
the offset it carries is where the decoder reads from, which also settles a 40-byte
`BI_BITFIELDS` header whose masks really do follow it. And `uri_list` read `file:///C:/x` as
`/C:/x` on every host, a path Win32 cannot open and one the `CF_HDROP` reader would have
produced for every copied file; the parser now maps a drive path under an empty authority and a
foreign host to the local and UNC paths Windows uses. Caught by
`a_written_dib_reads_back_as_the_same_picture`, watched red on the missing twelve bytes before
it passed, `a_copied_path_becomes_a_file_uri`, `a_windows_file_uri_names_a_drive_or_a_share`,
and three live pins from a DIB-only picture, a copied file and copied text.

**The status page said a Windows capture gave SDR codes.** The duplication asked for the float
surface, and the capture session carried it unconverted, since "Carry light above SDR white as
half floats" — but this page's status still described a capture that returned "SDR sRGB codes"
and listed HDR content as not implemented. A reader would have looked for a conversion that does
not exist, or tonemapped a frame twice. The paragraph now says what arrives: half-float scRGB
with the display's peak beside it on an advanced-colour desktop, 8-bit sRGB otherwise. Code
cannot drift from this again quietly, because a frame carries its `Colour` and every consumer
switches on it; the file path through that switch is pinned by
`a_measured_picture_rolls_its_highlights_into_the_file_white`.

**A per-output surface landed on the primary monitor.** The app opens one canvas per output and
one overlay over the output it just captured, and sized each for that output's rectangle, but the
portable spec named no output at all, so every layer opened on the primary monitor: a pin taken
from a second monitor appeared on the first, at coordinates from the wrong screen. `LayerSpec`
now carries the output's name — the same string `Backend::outputs` reports and the app already
kept for a pin's source — and `opening_layer` resolves it to a monitor, falling back to the
primary one for a name no monitor has. The Wayland backend does not act on the name yet, so a
layer there still lands where the compositor puts it; that half is unverified and unimplemented.
Caught by `layers`, which now opens a layer naming the enumerated output and one naming
`\\.\DISPLAY999` and checks both against the monitor's rectangle — a single-monitor machine
cannot tell the two apart, so the second-monitor case is unobserved here.

**A tilted wheel scrolled nothing on Windows.** Only `WM_MOUSEWHEEL` was translated, so a mouse
with a horizontal wheel, and the two-finger sideways gesture every precision touchpad sends
through it, produced no event at all and a strip that scrolls sideways could not be moved
without Shift. Both messages now share one arm of the window procedure and differ only in the
axis and the sign, which they have to: Microsoft's documentation gives the vertical high word
"away from the user" and the horizontal one "to the right", and the engine's positive step means
content moves left or up, so the vertical notch is negated and the horizontal one is not.
`input_probe` now asks for both steps and names the horizontal one; it printed FAIL with the
message posted and no arm to take it, and PASS with the arm.

**TSF would not take the store as it was first written.** Three calls failed with an HRESULT
while `tsf_probe` was being made to read a field, each from one wrong assumption. A context
created under `TF_CLIENTID_NULL` is refused with `E_INVALIDARG`: the client id must be the one
`ITfThreadMgr::Activate` returned for this thread, which is also the id an edit session is
requested under. A composition sink cannot be advised on a document, because
`ITfSource::AdviseSink` for `ITfContextOwnerCompositionSink` answers `TS_E_NOOBJECT`: the
manager queries the object handed to `CreateContext` for that interface and installs it itself,
which is why the store now implements the sink rather than an advisor beside it. And
`ITextStoreACP::GetText` must read `acpEnd == -1` as "to the end of the document": TSF reads
the whole document that way, and the probe's edit session, which widens the range to the
document's end before reading, came back empty until the store honoured it. That last one is
what the probe pins today; the two refusals are what they were at the call, since the input
method this machine has never notifies the sink — it runs out of process, and no window here
associates a document with it at all.

**TSF took the keyboard away from the input method that worked.** The first cut gave a
focused field's window a TSF document whenever the field was enabled and not secret. On
this machine the gallery then showed no preedit at all and could not switch input methods:
Microsoft Pinyin registers only as a `LocalServer32` service, and it serves keys through
the window's IMM32 emulation — `ITfThreadMgr::AssociateFocus` switches that emulation off,
so the service received nothing. Measured with a probe that typed `nihao` into a real
window: the service reported itself active (`ITfActiveLanguageProfileNotifySink`),
`ITfKeystrokeMgr::TestKeyDown` answered `S_OK` with *not taken* for every key, the
document's open/close and conversion compartments stayed `VT_EMPTY`, `GetText` was never
called, and the letters arrived as plain `Event::Text`. The same probe with
`AssociateFocus` skipped, and the window refocused, composed `nihao` through
`WM_IME_COMPOSITION`. The fix is the rule above it: `tsf::serves_input` reads the active
keyboard profile and uses TSF only when its CLSID has an `InprocServer32` entry, so a
window never swaps a working input method for one that cannot serve it. `tsf_probe` now
reports which path the machine takes and skips its document checks on the out-of-process
one; `ime` pins that the IMM32 path still opens, places and reads a composition.

**A sizing drag froze the window it was resizing.** `WM_NCLBUTTONDOWN` went to
`DefWindowProcW`, which runs the sizing loop on this thread: the process loop stopped
until the button came up, every other surface with it, and DWM stretched the last frame
over the growing window. The window performs that drag itself now. The press keeps its
screen position and the window's rectangle, a mouse move resizes through `SetWindowPos`,
Escape restores the rectangle, and the release or a lost capture ends it, so the loop
runs between messages and a frame follows each move. What that gives up is in the
sizing paragraph above: the shell's loop is where a dragged edge snaps to a monitor
edge, and a title-bar drag still blocks the process. `resize_probe` drags the border
with posted messages and reads the presented frame back: a client area of 525 by 375
pixels became 585 by 415 after a 60-by-40 move, and the frame read back was 585 by 415,
not the old size stretched.

**The caption stayed outside the scheme the window drew with.** DWM gives a window a light
caption until it asks for anything else, and this backend never called
`DwmSetWindowAttribute`; `WM_SETTINGCHANGE` only set a flag that nothing read. A dark client
area could therefore sit under a light title bar — hidden here only because a third-party
title-bar hook repainted the caption, which made the split visible once an app kept a fixed
scheme. The window now sets `DWMWA_USE_IMMERSIVE_DARK_MODE` from the registry value the app's
theme watcher reads, at creation and again on every `WM_SETTINGCHANGE`, and that message also
marks a frame owed so an idle window rebuilds its appearance. Checked by captures of one
running settings window: with `AppsUseLightTheme` set to 1 the form and its caption were
light; setting the value to 0 and broadcasting `WM_SETTINGCHANGE` turned the same window
dark without a restart. The value was restored to 0 afterwards.

**A border drag could shrink a window past what its content could lay out.** The Win32
backend performed the drag itself but knew no minimum, so a window could be dragged narrower
than a row of its form: the settings window's field is 592 logical pixels inside 24 of
padding, and below 640 the rows ran out of width and overflowed the frame. The drag now
clamps through `tracked_minimum`, the same helper that answers `WM_GETMINMAXINFO`, and both
take the larger of the declared floor and the system's `SM_CXMINTRACK`/`SM_CYMINTRACK`.
Checked by `resize_probe`: a surface that declares its opening size as its minimum is told
`ptMinTrackSize` equal to the window rectangle it opened with, and a 240-by-160 inward drag
from the bottom-right corner leaves the client area at that opening size.

**The status still said the Windows process loop could read no selection.** It reported a
selection reader for drag-and-drop formats falling back to the trait's default, which stopped
being true when `ClipboardReader` became `Backend::selection_reader` for `pin add --clipboard`.
The real gap is drag-and-drop itself: no window handles `WM_DROPFILES` or registers an
`IDropTarget`, on any backend. Checked by reading
`ui-platform-win32/src/backend.rs`, where `selection_reader` returns `Some(&mut self.reader)`,
and by searching the workspace for `WM_DROPFILES`, `DragAcceptFiles` and `IDropTarget`, which
have no hits.

**Drag and drop reached the pointer model and both windows.** The engine now has a payload
(`ui_core::dnd`), `Sense::DROP` targets that match offered MIME patterns, and one
`DragSession` for both an in-app gesture and a foreign drag; see
[Pointer Model § Drag and drop](./engine/pointer-model.md#drag-and-drop). The Win32 window
registers an `IDropTarget` and answers every callback from the clipboard format table, and
starts an outgoing drag on a worker STA thread because `DoDragDrop` is modal; the Wayland
window handles `wl_data_device.enter`/`motion`/`drop`/`leave` and starts its own drags with
`wl_data_source.start_drag`, reading the dropped payload from every announced MIME down its
own pipe. Checked by four `ui-platform` contract tests over the engine's DnD events, the
`ui-platform` suite (159 passed; the 11 failures are the pre-existing text-selection ones),
and a `cargo check --target x86_64-unknown-linux-gnu` for the Wayland crate. The live
gestures — a file from Explorer into the gallery, a card out into another application, and
the Wayland wire check — have not been run yet.

## Status

Implemented: both presenters, dma-buf zero-copy, explicit sync, HDR targets,
offscreen layers, text on both paths, every `DrawList` primitive on both paths,
fractional scaling, a D3D11 device that renders the whole `DrawList` on
Windows, and a Win32 window that presents it through a DXGI flip-model swap
chain that follows its output into HDR in whichever of PQ and scRGB that output
presents and falls back to 8-bit sRGB, feeds
pointer, keyboard, clipboard and drag-and-drop events back to the loop, takes drops from
other applications and starts its own drags into them, paints its caption in the system's
light or dark scheme, resizes itself on a border drag so a frame follows every move and stops
at the minimum the spec declares on both that drag and the shell's own sizing, and hosts the
gallery.
Both window backends implement the portable `HostSession` trait, so an app's
loop is written once and names no backend; the Win32 implementation is
built and exercised on this machine, the Wayland one is checked by reading each
delegated signature until it is next built on Linux. The Win32 backend also
implements the process `Backend`, so a `Host` on Windows runs several windows
from one message pump, answers outputs, languages and the system clipboard, and
serves a named-pipe channel and global hotkeys. A layer surface opens as a
topmost popup on the primary monitor, placed by the spec's anchors, size and
margins, painted by one window and shaped to the areas its frame publishes by
another, so a click leaves it wherever it claims nothing; the strip an
auto-hidden taskbar hides behind stays with the desktop unless the spec covers
it, and a `FollowPointer` layer takes the keyboard on a click and hands it back
when the pointer leaves.

A focused field's input method composes through TSF when the active text service is
in process, which also carries the field's surrounding text and a request to delete
it; an out-of-process service, and every other state, composes through IMM32, with
the engine's preedit and commit events and the IME windows at the caret. A screen capture runs a Windows
Graphics Capture session over one output, or duplicates it through Desktop
Duplication, and an advanced-colour desktop arrives as the half-float scRGB
surface it composes, unconverted, with the display's peak beside it; an 8-bit
desktop arrives as sRGB codes. A surface whose device DXGI reports as removed,
or whose window hears that the display changed, rebuilds its device, chain and
renderer and redraws; the process loop retires a surface that fails rather than
ending.

Not implemented: on the Windows window, a paste that carries more than text
into an editor (the event carries a string, though the process can read a file
list or a picture), drawing while a window is moved by its title bar (that drag
is still the shell's modal loop), and `ExclusiveZone::Reserve`; in TSF, an
observed live composition, because the only input method this machine has runs out
of process and no window here may use TSF for it, so the store's preedit and
delete-surrounding paths are pinned by unit tests and its document by a probe run
before the gate existed; in a capture, a frame larger than one output assembled into one
picture, and the GDI rung's colours, which are 8-bit by nature; Vulkan. The
Windows and Vulkan notes in `old/` are surveys, not code.

## See also

- [Platform Boundary](./platform-boundary.md): how a backend is registered.
- [Frame Pipeline](./engine/frame-pipeline.md): what the `DrawList` contains.
- [Profiling](./development/profiling.md): measuring present, and why the
  nested session lies.
