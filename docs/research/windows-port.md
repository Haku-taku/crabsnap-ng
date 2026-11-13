# Windows Port

What a Windows port of the engine and the screenshot app must preserve, which
Win32 APIs replace the Wayland ones, and whether Vulkan is the right first
renderer. This began as a report; what has since become code is marked in
[Status](#status), and where the code chose differently from the plan the reason
is under [Corrections](#corrections).

## Problem

The engine's contracts are portable; its only backend is not.
`platform_boundary.rs` classifies four crates as `PLATFORM_BOUND`, and all four
are Linux-shaped: `ui-platform-wayland`, `ui-demo`, `ui-gpu-egl`, and
`ui-paint-gpu` through `ui-gpu-egl`. Every surface, every pixel path, and every
platform service on Windows is missing.

The application is coupled at the same depth. `WaylandBackend::connect()` is
called directly at six sites: three in `apps/crabsnap/src/main.rs` (`:2374`,
`:2500`, `:2683`), plus `apps/crabsnap/src/daemon.rs:83`,
`apps/crabsnap/src/run.rs:236`, and `apps/crabsnap-settings/src/settings.rs:67`.
The capture, colour, hotkey, and daemon modules import Wayland protocol types by
name. `apps/crabsnap-platform` is a Unix socket client, not the backend
selector the event-loop page assigns it. A Windows build therefore needs a
second backend, a second renderer if the GPU path is to survive, and a second
application shell. It is not a feature flag.

Two properties make the port tractable. The frame contract is already split
into a portable `Host` and a platform `Backend`
(`ui/crates/ui-platform/src/host.rs`), and the presentation contract is already
two traits, `PresentTarget` and `SceneTarget`
(`ui/crates/ui-platform/src/present.rs`). Both carry the right shape for Win32.
The work is in the implementations, not in a redesign of the loop.

## What must not change

| Contract | Where it lives | Why the port must keep it |
| --- | --- | --- |
| One loop, one wait | `ui-platform::host` | Win32 messages are also one thread queue; a per-surface `GetMessage` would starve the other surfaces exactly as a per-surface `poll` would |
| Frame demand, not frame rate | `ui-platform::driver` | The compositor's callback becomes the swapchain's frame-latency waitable; the demand rule stays |
| The submit rule | `ui-platform::present` | Damage, animation, IME state, and pending surface state decide presentation; the app never does |
| Damage is the redraw unit | `ui-core::damage` | `Present1` takes dirty rectangles, so the contract maps without translation |
| Logical layout, device pixels | `ui-core::layout`, `FrameHost` | Per-monitor DPI is a scale per surface, which is what the engine already assumes |
| Premultiplied alpha | `ui-paint`, `ui-paint-gpu` | DirectComposition composites premultiplied; the invariant is unchanged |
| Portable crates never name an OS | `platform_boundary.rs` | The port adds platform crates; it does not widen `PORTABLE` |

## Decisions

### D1 — One HWND per surface, one message pump per process

A surface is an `HWND`. `Backend::wait` becomes
`MsgWaitForMultipleObjectsEx` over the loop's handles plus `QS_ALLINPUT`, and
the message queue is drained with `PeekMessage` before the next turn. A
`WndProc` never dispatches to the engine; it appends to the surface's event
queue and returns. This preserves the one-reader rule: the loop decides when
the process wakes, and one surface cannot hold another's input.

The alternative, one blocking `GetMessage` loop per window, is the same mistake
the event-loop page records for Wayland: the first surface stepped decides when
every other surface runs.

### D2 — Every surface presents through a composition swapchain

`IDXGIFactory2::CreateSwapChainForComposition` plus a DirectComposition visual,
with a `WS_EX_NOREDIRECTIONBITMAP` window. The swapchain format is
`DXGI_FORMAT_B8G8R8A8_UNORM` with `DXGI_ALPHA_MODE_PREMULTIPLIED`; the HDR form
is `DXGI_FORMAT_R16G16B16A16_FLOAT`.

An HWND swapchain cannot be premultiplied. `CreateSwapChainForHwnd` rejects
`DXGI_ALPHA_MODE_PREMULTIPLIED`, which is why the composition path is mandatory
for pins, the toolbar, and the overlay, and it is simpler to use one path for
opaque windows too. The alternative, `WS_EX_LAYERED` with
`UpdateLayeredWindow`, copies the whole surface through the CPU on every frame
and is rejected for that reason.

### D3 — Direct3D 11 first

The renderer has eighteen programs, instanced quads, offscreen passes, and no
compute. Six of them form three sRGB and linear pairs, which D3D11 expresses as
two pixel shaders over one pipeline layout. D3D11 covers that without
descriptor heaps or explicit barriers. It
also has WARP, which gives the port a software device for CI, for RDP sessions,
and for virtual machines. D3D12 buys nothing this renderer can measure; Vulkan
is assessed in [Is Vulkan the right renderer?](#vulkan).

### D4 — Layer surfaces are approximated, not emulated

`SurfaceRole::Layer` has no Win32 counterpart. The backend turns the spec into
a rectangle on a monitor and a set of window styles. The mapping is on the
types already (`ui/crates/ui-platform/src/window.rs`), but the Windows
approximation is larger than that comment admits; see
[Surfaces and layers](#surfaces-and-layers).

### D5 — Platform services stay behind their existing traits

Clipboard, hotkeys, IPC, language, theme, outputs, and the worker waker each
already have a portable interface. The Windows backend implements those
interfaces; no portable crate gains a `cfg(windows)` branch or a Win32 type.

### D6 — The application splits, the engine does not

`crabsnap-core` stays portable: selection geometry, the document, the pin
model, the protocol, and the settings schema do not change. `apps/crabsnap`
becomes portable application state plus a platform shell. Capture, colour
descriptions, hotkey registration, the daemon's transport, and the tray are
per-platform. `apps/crabsnap-platform` becomes the selector the event-loop page
already describes, instead of a Unix socket client that every binary bypasses.

### D7 — Unpackaged, per-user, signed

An installer under the user's profile, no elevation, no service. A service
cannot put a window on the user's desktop, and an AppContainer adds restrictions
on hotkeys, autostart, and shell integration that would each need their own
design. MSIX is a later option, not the first package.

## Surfaces and layers {#surfaces-and-layers}

`SurfaceSpec::window` is a normal overlapped window: `WS_OVERLAPPEDWINDOW`,
`WM_GETMINMAXINFO` for the minimum, and a DWM frame. The settings window is the
only such surface in the app.

`SurfaceSpec::layer` is a popup window on one monitor, placed by the backend:

| Layer field | Win32 form |
| --- | --- |
| `level: Overlay` or `Top` | `WS_EX_TOPMOST` on a `WS_POPUP` window, with `WS_EX_TOOLWINDOW` so the surface stays out of Alt+Tab and the taskbar |
| `level: Bottom` or `Background` | A normal window with no topmost bit. There is no supported way to place it under other applications' windows; the field degrades to "not topmost" |
| `anchor` plus `size` plus `margin` | Rectangle arithmetic on `MONITORINFO::rcMonitor`, in physical pixels; a single-edge anchor makes the margin the position, as on Wayland |
| `exclusive: Reserve` | Ignored. Reserving desktop work area is an AppBar, which changes the shell's layout and is out of scope |
| `keyboard: None` | `WS_EX_NOACTIVATE`; the surface never takes focus |
| `keyboard: OnDemand` | No `WS_EX_NOACTIVATE`; `WM_MOUSEACTIVATE` returns `MA_ACTIVATE` inside a consumed area and `MA_NOACTIVATE` elsewhere, so a click on a pin focuses the canvas and a click on the desktop does not |
| `keyboard: Exclusive` | `SetForegroundWindow` on map, and `WM_MOUSEACTIVATE` answers `MA_ACTIVATE`. The hotkey press usually satisfies the foreground rules; a refusal leaves the surface unactivated until it is clicked. A low-level keyboard hook for the keys it owns is not implemented |
| `keyboard: FollowPointer` | Degrades to `OnDemand`: `WM_MOUSEACTIVATE` returns `MA_ACTIVATE` over an area the surface consumes. Windows has no "focus follows pointer" contract, so the previous foreground window cannot be restored when the pointer leaves without a click |

`SurfaceRole::Layer` drives the input region: the engine publishes the hit index
through `set_input_region`, and the backend answers `WM_NCHITTEST` with `HTCLIENT`
inside one of those areas and `HTTRANSPARENT` outside, so the rest of a full-screen
overlay passes the pointer to whatever is beneath it. `examples/layers.rs` checks
this with `WindowFromPoint` over a window of the same process sitting below the
layer.

## The frame loop on Win32

`FrameHost` maps without a new portable concept:

| `FrameHost` method | Win32 source |
| --- | --- |
| `size` | `GetClientRect`, physical pixels under per-monitor DPI awareness v2 |
| `scale` | `GetDpiForWindow(hwnd) / 96.0`; a pin canvas uses the monitor's DPI |
| `pointer` | Last `WM_MOUSEMOVE` or `WM_POINTERUPDATE`, in client physical pixels, divided by scale |
| `can_draw` | The swapchain's frame-latency waitable is signalled, or the surface has not presented since the last turn |
| `wait_until` | `MsgWaitForMultipleObjectsEx` on the wake event, pipe, swapchain waitables, and timer; then `PeekMessage` |
| `wait_for_frame` | `WaitForSingleObject` on `IDXGISwapChain2::GetFrameLatencyWaitableObject`, capped as the Wayland wait is |
| `should_close` | `WM_CLOSE` or `DestroyWindow` recorded by the surface |
| `take_events` | The surface's queue, filled by `WndProc` |
| `take_silent_change` | `WM_SETTINGCHANGE`, `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, or a device-lost flag |

Pacing uses the waitable object, not `DwmFlush`. `DwmFlush` waits for the
desktop composition, which is a different clock from the swapchain's frame
window; a loop paced on it presents into a closed window and drops frames.

`DXGI_STATUS_OCCLUDED` from `Present` is a normal state, not an error. The
surface stops presenting, waits for the next message, and presents a full frame
when it is visible again. A minimized window, a locked session, and a virtual
desktop switch all take this path.

## Presentation

### Damage and buffer rotation

The engine's `Damage` maps to `IDXGISwapChain1::Present1` dirty rectangles,
but only after two things hold:

- The swapchain uses `DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL`. `FLIP_DISCARD`
  leaves the back buffer undefined, so a partial redraw would show whatever
  the previous frame left. It is created with
  `DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT` and a maximum frame
  latency of one, which is what makes the waitable in the loop meaningful.
- A resize releases every view, calls `ResizeBuffers(0, 0)`, and rebuilds the
  targets. The engine adds full damage on a size change for the same reason it
  does on Wayland.

The first version draws the whole surface every frame. Damage is enabled only
after the renderer's band logic matches the Wayland path's carry-forward
result; a partial frame that is cheaper but wrong is not a port.

### HDR and colour

`DXGI_OUTPUT_DESC1` supplies the primaries, the minimum and maximum luminance,
and the space the desktop is composing in. It carries no SDR white level — an
earlier version of this note said it did. That value belongs to the display
configuration, as `DISPLAYCONFIG_SDR_WHITE_LEVEL` in units of 1/1000 of 80 nits,
and is reachable only by matching the path's `DISPLAYCONFIG_SOURCE_DEVICE_NAME`
to the monitor's `szDevice`. `IDXGISwapChain3::SetColorSpace1` selects the output
space.

| Engine value | Windows form |
| --- | --- |
| `SurfaceColour::Sdr` | `DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709`, BGRA8 unorm |
| `SurfaceColour::Hdr` | `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020`, RGBA16F, values encoded by the existing `encode_for_pq_output` |
| `ContentColour` from a capture | `IDXGIOutput5::DuplicateOutput1` hands back whichever of its format list the desktop is composed in: `R16G16B16A16_FLOAT` scRGB (linear, 1.0 is 80 nits) on an advanced colour desktop, BGRA8 otherwise. `DuplicateOutput` alone always converts to BGRA8, by clipping |
| scRGB composition | `DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709`, where 1.0 is 80 nits |

Presenting f16 PQ keeps the engine's single output transform: the shader that
serves Wayland serves Windows, and only the swapchain's colour space changes.
The alternative, scRGB, would need a second present variant and a scale by
`reference_nits / 80` that no portable code states today.

Implemented in `crates/ui-platform-win32/src/presenter.rs`, which decides from
the output before it creates a chain. An output whose `ColorSpace` is
`DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` gets the half-float chain and the
PQ space; the renderer is then given a linear destination from the output's own
description — SDR white as the reference, `MinLuminance` and `MaxLuminance` as
the destination's floor and peak — plus the sRGB-to-BT.2020 gamut matrix. Every
other output keeps the 8-bit chain, and an output that refuses the PQ space
(`CheckColorSpaceSupport` without `DXGI_COLOR_SPACE_SUPPORT_FLAG_PRESENT`, or a
failing `SetColorSpace1`) is dropped and replaced by the SDR one, so a machine
with HDR switched off never sees the half-float path.

`examples/hdr.rs` is the check. When it was run this machine's panel composed
SDR, so what it pins is the fallback: eight-bit chain, all five grey codes
unchanged. Aiming the renderer at the output's description anyway — 80 nits SDR
white, 417.7 nits peak — put white at 0.486 against `encode_for_pq_output`'s
0.4859 and every other code within 0.0007, through the half-float frame and the
copy into an SDR back buffer. The desktop on this machine now reports PQ
composition — captures come back as half floats — so the presenter takes the
half-float chain here, but that chain has not been re-checked since the display
changed and stays `[to verify]`.

The SDR white level is not a constant. It comes from the display configuration,
as the Wayland path takes it from `wp_color_manager_v1`. A value copied from
the developer's own display is the same mistake the colour page records for
PQ placement.

### Transparency and input

A composition swapchain carries premultiplied alpha, but Win32 input does not
know the client area is transparent. The window still owns its whole
rectangle. Three mechanisms exist:

| Mechanism | What it gives | Cost |
| --- | --- | --- |
| `WM_NCHITTEST` against the published input areas | Per-area hit testing, exact to the engine's hit index | `HTTRANSPARENT` is documented for windows in the same thread; cross-process pass-through must be measured before it is trusted |
| `SetWindowRgn` from the same areas | Cross-process click-through | Integer regions, rebuilt when a pin moves, and no antialiased edge |
| `WS_EX_LAYERED` with `UpdateLayeredWindow` | Per-pixel alpha hit testing, which the shell already implements | A CPU copy of the whole surface per frame |

The pin canvas is one window the size of a monitor. It is the surface where
this choice matters: a click between pins must reach the desktop, and a click
on a pin must not. The first implementation uses `WM_NCHITTEST` and a probe
that clicks through to a window of another process. If the probe fails, the
canvas moves to a window region, and the GPU path keeps a separate small
"input window" per pin as the last resort. That last form reintroduces the
per-pin surface the Wayland design rejected, so it is a fallback, not a plan.

## Rendering

### Stage 0: a backend-neutral painter

`ui-paint-gpu` cannot be reused as it stands. It depends on `ui-gpu-egl`
unconditionally (`ui/crates/ui-paint-gpu/Cargo.toml`), and the target contract
is GL-shaped: `GpuSurface::bind(&self, gl: &glow::Context)`
(`ui/crates/ui-gpu-egl/src/target.rs:14`). A D3D11 backend that starts from
this tree would rewrite the scene planner as well as the device layer.

The first rendering task is therefore extraction, not D3D:

- Move batch walking, the layer stack, backdrop ordering, scissor state, the
  uniform layout, and the canvas pass into a backend-neutral module.
- Replace `GpuSurface` with a target trait that names size, format, and
  attachment, not `glow`.
- Keep the GL implementation as the first consumer, and prove the extraction
  changed nothing: the existing `gpu_*_scene` examples must produce identical
  probes and byte-identical PPM dumps where they exist.

This is the same Stage 0 the legacy Vulkan survey proposed
(`old/docs/research/vulkan-backend.md`, `[unverified]`). It is a prerequisite
for any second renderer, D3D or Vulkan, and it is cheap compared with either.

### The D3D11 mapping

| Engine primitive | D3D11 form |
| --- | --- |
| Program | Vertex and pixel shader pair, input layout, blend state, rasterizer state |
| FBO plus texture | Texture2D with a render-target view; no framebuffer object |
| Scissor | `RSSetScissorRects`, one rectangle; the shader still rounds the corners |
| Premultiplied blend | `SrcBlend = ONE`, `DestBlend = INV_SRC_ALPHA` |
| Instanced quads | Per-instance vertex buffer, input layout with `D3D11_INPUT_PER_INSTANCE_DATA`, step rate 1 |
| Uniforms | One constant buffer per program, 16-byte aligned, updated with `Map(WRITE_DISCARD)` |
| Texture and atlas upload | `UpdateSubresource` or a staging texture; R8 for coverage, BGRA8 for images |
| Offscreen effect pass | Draw a fullscreen triangle into the destination texture |
| Layer composite | The same shader, sampled from the layer texture, with opacity and corner coverage |
| Readback | `CopyResource` to a staging texture, `Map(READ)`, then unpremultiply and swap channels |
| Present | `Present1(1, 0, dirty_rects)` on the composition swapchain |

D3D11's viewport origin is the top-left, the same convention as the engine's
device rows. There is no Y flip, unlike the Vulkan port, which has to reverse
its projection.

D3D11 hazards are handled by unbinding a texture's shader-resource view before
binding the same texture as a render target. The effect chain already avoids
reading and writing one texture in one pass; that invariant becomes a device
rule, not an optimization.

### Shaders and pipelines

The GLSL sources are twenty-seven files embedded with `include_str!` and
assembled into eighteen programs (`ui/crates/ui-paint-gpu/src/shader.rs`); four
of the files are shared chunks, and the rest are vertex and fragment stages.
The D3D port translates them to HLSL once and keeps the same assembly points,
so a change to the SDF or coverage code lands in one place. Compilation uses
`D3DCompile` from `d3dcompiler_47.dll`, which ships with Windows 10 and 11; the
bytecode is cached beside the configuration. This avoids adding a shader
compiler to the build machine, which the repository does not have today.

Pipelines are cached by `(program, attachment format)`. The internal formats
are BGRA8 unorm for SDR, RGBA16F for HDR and effects; the present target adds
the swapchain format. There are eighteen programs times three formats, and a
pipeline is built on first use, not at startup.

### Canvases, effects, readback

`SceneTarget::present_scene` receives `CanvasFrame` entries. A canvas is an
offscreen texture with its own size and version; the renderer keeps a pool
keyed by canvas id and rebuilds on a size or version change. This is the same
contract the GPU path already serves, so the markdown editor and the
annotation preview need no change.

Effects use two scratch textures and ping-pong, as the GL path does. The
`EffectReach` band padding is portable and stays; only the pass execution is
new. The CPU and D3D renderers must be compared on the same scenes, because
the CPU path is the reference the repository already trusts.

## Is Vulkan the right renderer? {#vulkan}

Vulkan is technically feasible on Windows and is the wrong first choice. The
port should reach a working D3D11 backend before a Vulkan backend is considered,
and the case for Vulkan at all is weak while the composition and capture stack
is D3D-native.

The reason is not that Vulkan is slow or immature. The reason is that every
Windows service this application depends on hands out D3D resources or asks for
a D3D swapchain:

| Requirement | Native Windows answer | Vulkan answer |
| --- | --- | --- |
| Premultiplied-alpha surface on the desktop | Composition swapchain, DComp visual | No Vulkan equivalent of `CreateSwapChainForComposition`. Transparency needs a D3D11 or D3D12 texture shared through `VK_KHR_external_memory_win32`, a Vulkan render into it, and a DComp visual on the D3D side |
| Screen capture | Windows Graphics Capture and Desktop Duplication produce D3D11 textures | Import each frame through external memory, with a keyed mutex or an external semaphore, before sampling it |
| HDR signalling | `SetColorSpace1` plus `DXGI_OUTPUT_DESC1` | WSI colour-space support is driver-dependent; `VK_EXT_hdr_metadata` is not a swapchain colour-space contract |
| Software fallback | WARP, a D3D11 device | None. A machine without a Vulkan driver has no path |
| Remote sessions | WARP or the host GPU; a D3D11 device either way | Driver-dependent; some RDP stacks expose no Vulkan device |

Each row is solvable. Together they mean that a Vulkan backend on Windows is
not a renderer with a Windows surface; it is a renderer plus a D3D interop
layer, and it still needs a D3D11 device for composition and for capture even
when the pixels came from Vulkan.

### What a Vulkan backend would take

The legacy survey counted the renderer as it stood when it was written
(`old/docs/research/vulkan-backend.md`, `[unverified]`): eleven programs,
sixteen shader chunks, fifty-four uniforms, and about two hundred `gl.*` call
sites. The current tree has eighteen programs and twenty-seven GLSL files, so
the counts moved and the shape of the work did not. The survey's five stages
still describe it:

1. Extract the backend-neutral painter. This report requires it anyway, for
   D3D11.
2. Add a device crate over `ash`, with instance, device, queue, allocator, and
   swapchain.
3. Translate the shader chunks to SPIR-V. The build gains a `glslc` or
   `shaderc` step, or a checked-in SPIR-V directory with a regeneration script.
   The repository has no shader compiler today.
4. Rebuild the pass structure with explicit barriers, render passes, and
   per-format pipeline variants. The GL driver did this implicitly.
5. Port the readback, the canvas pool, and the present path, then run the
   scene comparisons.

On Windows, add a sixth stage: external memory and semaphore import for
composition and capture, plus a D3D11 device that owns the visual. That is
strictly more work than D3D11 alone, for a renderer that draws eighteen 2D
programs.

### Why it is not first

D3D11 reaches the same pixels with less machinery: no barriers, no descriptor
sets, no SPIR-V toolchain, no interop for composition or capture, and WARP for
CI. The engine's rendering invariants are already expressed in terms D3D11
shares: premultiplied blending, one scissor, ping-pong effect textures, and a
full-frame pass. Vulkan shares those terms too, but it charges for them in
setup that D3D11 does not require.

The legacy survey reached the same conclusion for a different reason: if the
real target is Windows, Vulkan is the wrong answer, because the platform's
composition, capture, and colour services are D3D. This report agrees, and adds
the app-specific argument: the pin canvas needs per-pixel transparency, the
capture path needs D3D11 textures, and the overlay needs both.

### ANGLE as a spike, not an architecture

`ui-gpu-egl` already speaks EGL. ANGLE would run the existing GLES renderer on
Windows over D3D11, which could validate the shader translation and the scene
code before a D3D11 renderer is written. A time-boxed spike is justified
because it tests the extraction and the shaders with the code that exists.

It is not the port. ANGLE adds a large binary, its window surfaces are opaque,
so transparency still needs DComp interop, and it leaves the engine with two
GPU abstractions and a translation layer between them. If the spike happens,
its output is evidence about the shaders, not a shippable backend.

### When to revisit Vulkan

Vulkan becomes the right answer if one of these becomes true, and not before:

- The renderer must be bit-identical across Linux and Windows for a reason the
  product states, and the D3D11 backend cannot meet it.
- A measured D3D11 driver defect on a target GPU cannot be worked around.
- The engine gains GPU work that D3D11 cannot express well, such as compute
  passes that dominate frame time.

"Vulkan is more modern" is not on the list.

## Input and text

### Keyboard

`WM_KEYDOWN` and `WM_SYSKEYDOWN` become `KeyPressed`; `WM_KEYUP` and
`WM_SYSKEYUP` become `KeyReleased`. The virtual-key code is not the engine's
`Key`, and a layout can map several keys to one character or one key to several
characters. The backend uses `MapVirtualKeyEx` with the active layout for the
physical part of the mapping and `VkKeyScanEx` for the character part, and it
stores the unshifted character as the shortcut model expects. Dead keys and
AltGr need a probe per layout; the engine's shortcut rules were written against
a Linux layout, so this is a table to verify, not to assume.

Text arrives through `WM_CHAR`, which is already UTF-16. A surrogate pair
arrives as two messages; the backend joins them before it emits `Event::Text`.
Keys that the engine consumes for shortcuts still produce `WM_CHAR` unless the
window eats the message, so the backend suppresses `WM_CHAR` for a key it
already reported as consumed.

Windows repeats keys at the OS level. The engine's repeat logic must be disabled
for this backend, or a held key repeats at double rate. Focus loss
(`WM_KILLFOCUS`) releases every key the engine thinks is down, or a shortcut
stays stuck after Alt+Tab.

### Wheel and pointer

`WM_MOUSEWHEEL` carries 120-unit detents and screen coordinates, not client
coordinates; the backend converts with `ScreenToClient`. A precision touchpad
sends smaller deltas, which the engine accumulates. The engine's forty pixels
per detent is a fixed step; Windows has `SPI_GETWHEELSCROLLLINES`, and a
Fluent-style list is expected to honour it. The first version keeps forty for
cross-platform parity and the settings page gains the scroll-lines option when
the divergence is measured.

Pointer input uses `WM_MOUSEMOVE`, `WM_LBUTTONDOWN`, `WM_RBUTTONDOWN`, and
`WM_MBUTTONDOWN` for the mouse, and `WM_POINTER*` for pen and touch where the
device reports it. `WM_MOUSEMOVE` with `SetCapture` reports coordinates outside
the client area; they are clamped to the surface, as `PointerWarp` clamps on
Wayland. `TrackMouseEvent` supplies `WM_MOUSELEAVE`, which is the only leave
notification Win32 gives.

The cursor is set with `SetCursor` in response to `WM_SETCURSOR`, not
unconditionally; the message tells the backend whether the pointer is over the
client area or the frame. Hiding is `SetCursor(NULL)` plus a `WM_SETCURSOR`
handler that returns `TRUE`. `PointerWarp` is `SetCursorPos` after converting
client to screen coordinates, and it may refuse when the calling process is not
in the foreground; the engine already treats a refused warp as legal.

### IME

Implemented with IMM32. `set_ime` opens or closes the window's input context
with `ImmSetOpenStatus` and places the composition and candidate windows from
the caret rectangle the engine publishes through `ImeState`;
`WM_IME_STARTCOMPOSITION`, `WM_IME_COMPOSITION` and `WM_IME_ENDCOMPOSITION`
fill `Event::Preedit` and `Event::Text`. The IME's own composition window is
suppressed in `WM_IME_SETCONTEXT`, because the engine draws the preedit, and a
secret buffer suppresses the candidate list as well.

Four traps. `WM_IME_CHAR` duplicates characters that also arrive through
`GCS_RESULTSTR`; the backend reads the result string and ignores `WM_IME_CHAR`,
which is also the only path that keeps a surrogate pair whole. The composition
string is UTF-16 with its cursor in code units, so the preedit's byte cursor is
a separate conversion, and a position inside a surrogate pair snaps forward.
Both forms' `ptCurrentPos` is relative to the client area, the candidate form
included, so a point that went through `ClientToScreen` put the list exactly one
window offset down and right of the caret. And a context change sends messages
back to the same window procedure, so no state may be borrowed while the IMM
calls run.

Surrounding text has no IMM32 equivalent, and neither has
`Event::DeleteSurrounding`; both stay unserved until TSF. TSF provides them
through `ITfContext::GetSelection` and the surrounding-text interfaces, which
is why the work is split: IMM32 for composition and placement, TSF later for
surrounding text and for input scopes on the hidden text fields the magnifier
uses. The engine's four-thousand-byte cap stays on the engine side.

`examples/ime.rs` checks what a machine without a language IME can still show:
the context opens with a caret and closes without one, the composition point
follows the caret, the candidate form excludes the caret's rectangle, and an
injected composition string arrives as a preedit whose cursor is a byte offset.

## Services

### Clipboard and drag-and-drop

The engine's clipboard model is MIME-typed content with an owner that serves
it on demand. Windows is format-typed and the owner is the clipboard itself
after `SetClipboardData`, so a copy is written whole, in every format its
content has.

| Engine content | Windows formats |
| --- | --- |
| `text/plain` | `CF_UNICODETEXT` |
| `image/png` | `"PNG"` registered format, plus `CF_DIBV5` and a `CF_DIB` fallback |
| `text/uri-list` | `CF_HDROP` with a `DROPFILES` block of wide, double-null-terminated paths |
| `text/html` | `"HTML Format"` with the header Microsoft applications expect |

Implemented for all four, in `crates/ui-platform-win32/src/clipboard.rs`. The
`"PNG"` format alone is not enough: Explorer, Word, and Paint read `CF_DIB`, so
a picture goes on as all three, and the DIB is built by decoding the PNG — a DIB
is pixels, and the `image` crate under its `png` feature is the decoder. A DIB
stores its rows bottom-up and each pixel blue, green, red, alpha; `CF_DIBV5`
carries the masks and an sRGB colour space, and `CF_DIB` is the older 40-byte
header as the fallback. The HTML header's four offsets count bytes from the
block's first byte and straddle the fragment, and the uri-list's `file:` URIs
are percent-decoded into wide paths, with a non-file URI dropped because the
format holds no place for it. A MIME type with no Windows equivalent is refused
rather than given an invented format.

A paste is still text only. `Event::Paste` carries a string, so reading
`CF_DIBV5`, `CF_DIB`, `"PNG"` and `CF_HDROP` into a pin needs a paste event that
carries content — the app's import path, not the backend's. The conversion each
of those four needs is the one above, run backwards.

Nothing here is delayed: `SetClipboardData` hands over the whole block at once,
so a copy costs its size until something replaces it. Delayed rendering — a null
handle plus `WM_RENDERFORMAT` — is how the daemon would keep a large capture
cheap, and `OleFlushClipboard` after `OleInitialize` is how the data would
survive the daemon on exit; both are planned, and both need the clipboard to be
owned by a window that outlives the command.
`AddClipboardFormatListener` and `WM_CLIPBOARDUPDATE` replace the Wayland
selection event; a message-only window is enough to receive them, and nothing
listens yet.

`examples/clipboard.rs` reads every format back through the shell rather than
through the encoder that wrote it: `DragQueryFileW` returns the two paths, the
HTML offsets are parsed out of the header, and the DIB headers are read as raw
little-endian fields. Its four-pixel picture pins the row order and the channel
order at once — the first pixel in the block is the picture's bottom-left one,
blue before red.

Drag-and-drop uses `RegisterDragDrop` and `IDropTarget` on the surfaces that
accept drops. `CF_HDROP` covers files dropped from Explorer; an image dragged
from a browser arrives as `CF_DIB` or `"PNG"` and goes through the same
conversion as the clipboard. `OleInitialize` is per thread, and the thread must
pump messages, which the `Host` loop already does.

### Global hotkeys

`RegisterHotKey` with `MOD_NOREPEAT` and a thread that pumps messages. The
`WM_HOTKEY` message goes to the registering thread's queue, so the daemon
registers on its own loop thread and never on a worker.

The engine's `GlobalHotkeyError` maps cleanly: `ERROR_HOTKEY_ALREADY_REGISTERED`
is `Taken`, and any other failure is `Failed`. `Unsupported` does not occur,
because every Windows desktop has the facility, and `Refused` has no equivalent,
because there is no prompt to refuse. A registration belongs to the thread
that made it and lasts until it is unregistered or the thread ends, so the
daemon registers once on its loop thread. An elevated foreground window does
not stop a lower-integrity process from receiving `WM_HOTKEY`; it does stop the
overlay from appearing above that window, which is the real constraint.

Print Screen is the key the configuration examples bind for region capture, and
Windows 11's Snipping Tool also claims it. The app must treat a taken key as a
normal state: `info status` reports it, and the settings page offers
alternatives. The
low-level `WH_KEYBOARD_LL` hook is a fallback for keys `RegisterHotKey` cannot
express, not the default, because it needs a message loop that never stalls and
it is the first thing anti-cheat and accessibility software interferes with.

### IPC and the daemon

The Unix `SOCK_SEQPACKET` socket becomes a named pipe:

- The name is `\\.\pipe\crabsnap-<session>-<user>`, where the session comes from
  `ProcessIdToSessionId` and the user from a hash of the SID. Two logged-in
  users must not share a daemon, and the global pipe namespace is machine-wide.
- `PIPE_TYPE_MESSAGE` with `PIPE_READMODE_MESSAGE` preserves the message
  boundaries the protocol relies on. A byte-mode pipe would let two replies
  interleave.
- `PIPE_REJECT_REMOTE_CLIENTS` and a security descriptor that grants the user
  and denies everyone else. The default descriptor is not sufficient for a
  service that can capture the screen.
- Overlapped I/O for the accept loop and for each connection; the `Host` loop
  waits on the pipe's event alongside the wake event.
- `Reply::deferred` needs the request's handle kept alive. The daemon duplicates
  it with `DuplicateHandle` into its own process handle table, or keeps the
  connection object; there is no descriptor passing to rely on.
- A pipe disappears when its last handle closes, so the stale-socket case in
  the CLI page cannot happen. A busy pipe means another daemon is running and
  the client should connect, not delete the name.

Bulk image transfer uses a named file mapping, as the CLI page already
describes for Wayland. The name needs the same session and user scoping, and
the mapping must be created with a security descriptor for the same reason.

### Outputs, language, theme, tray

Outputs come from `EnumDisplayMonitors`, `GetMonitorInfoW`, and
`GetDpiForMonitor`, with the friendly name and the HDR description from
`QueryDisplayConfig` and `IDXGIOutput::GetDesc1`. `\\.\DISPLAY1` is not a stable
identifier across reboots and dock changes, so the persisted output id is a
hash of the monitor device path, not the adapter's display name. Hotplug is
`WM_DISPLAYCHANGE`; a monitor that appears while the app runs adds an output
and a pin canvas, which is the same silent change the Wayland output listener
reports.

Language comes from `GetUserPreferredUILanguages(MUI_LANGUAGE_NAME)`, ordered,
not from the locale API. A per-app language override is a Windows 11 setting;
the engine's language list is the process's preference, and the override can be
ignored until the app has a UI for it.

Theme is two registry values under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` plus
`WM_SETTINGCHANGE` with `ImmersiveColorSet`. High contrast is a separate
`SPI_GETHIGHCONTRAST` query and must force the accessible colour set even when
the app theme is dark. The DWM dark title bar and the rounded corners are
`DwmSetWindowAttribute` attributes; they apply to the settings window only,
since every other surface has no frame.

The tray icon is `Shell_NotifyIconW` on a message-only window with an
application-defined callback message. Explorer restarts and broadcasts
`TaskbarCreated`; the icon must be re-added on that message or it disappears
until the next login.

## Capture

The capture chain keeps its shape: permission and capability probe at start,
then a per-output capture object, then frozen frames. Only the rung
implementation changes.

| Rung | API | Limits |
| --- | --- | --- |
| Preferred | Windows Graphics Capture, monitor or window item | Windows 10 1803 for the session, 1903 for item creation from a monitor or window, 22H2 for the border API and secondary windows |
| Fallback | Desktop Duplication per output | Duplication is a scarce per-output resource; another active duplicator can make it fail with `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`; access is lost on a mode change |
| Last resort | GDI `BitBlt` with `CAPTUREBLT` | No HDR, no protected content, slow, and it can miss hardware overlays |

Each rung is one file under `apps/crabsnap-platform/win32/src/capture/` answering the app's
`Backend` trait: `wgc.rs`, `duplication.rs`, `gdi.rs`. The frame type, its packings and the
trait are the portable core's, so the Unix rungs use the same three-way split. A fourth API
sometimes named for this job, `DwmGetDxSharedSurface`, is exported by `dwmapi` ordinal only and
has no header; Duplication and Graphics Capture supersede it, and the port does not implement
it.

The app must capture before it opens any overlay, and it must mark every own
window with `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Otherwise a
monitor capture includes the pins and the toolbar, which is the equivalent of
the Wayland trap where a layer surface appears in its own screenshot.
`WDA_EXCLUDEFROMCAPTURE` needs Windows 10 2004; on older builds the window is
black in the capture instead of absent, which is visible and acceptable for a
fallback.

Desktop Duplication must duplicate the output from a device on the adapter that
drives it. On a hybrid laptop, the iGPU drives the panel and the dGPU renders;
a device created on the wrong adapter returns `DXGI_ERROR_UNSUPPORTED`. The
backend enumerates the output's adapter with `IDXGIOutput::GetParent` and
creates or shares a device accordingly. Cross-adapter sharing through a shared
NT handle and a keyed mutex is the fallback, and it costs a copy.

HDR capture yields `R16G16B16A16_FLOAT` from Duplication and f16 from Graphics
Capture, in scRGB: linear, 1.0 is 80 nits. Neither path converts what it reads.
The frame carries `PixelStorage::Float16` and its peak, the pin uploads it as an
`Rgba16F` texture, and only an SDR destination — a file's rendition — divides
and encodes, through the same code the Wayland path uses.

Both DXGI rungs are implemented in `ui-platform-win32` and hand back the same `ScreenFrame`:
straight and upright, in whatever packing the desktop composed in — RGBA8, or
scRGB half floats at eight bytes a texel — with the output's name and DPI scale. `ScreenCapture` duplicates one
output from a device on that output's adapter. It asks `DuplicateOutput1` for f16 first, and
the same clipping trap applies on an auto colour managed SDR desktop as on an HDR one:
`DuplicateOutput` alone converts the composed surface to BGRA8 and everything above roughly
158 in sRGB lands on white. Measured here — an SDR-composed ACM panel whose white level reads
raw 3000 (240 nits) — a chart of eight known codes read back exactly through the float path
and came back as 59 for 32 and 210 for 128 through the 8-bit one. The float frame is kept as
it arrives, so a screenshot holds the light the desktop composed; only the file rendition
divides by the white level and encodes.

`GraphicsCapture` runs a capture session over the monitor instead: an item from
`IGraphicsCaptureItemInterop::CreateForMonitor`, a free-threaded frame pool, since a caller
with no dispatcher queue drains the pool itself, and one frame per grab. The pool is asked for
`R16G16B16A16Float` when the output reports PQ compositing and for `B8G8R8A8UIntNormalized`
otherwise, and a null `TryGetNextFrame` — which windows-rs reports as an error carrying `S_OK`
— is the "nothing yet" answer, not a failure. A fresh session hands over the current desktop
on a still screen, so one session per grab is enough. The border and pointer are switched off
through the session's own settings where Windows offers them; the rung's frames agree with the
duplication's to a mean of 0.04 codes, while the blit's differ by 3.6, because the compositor
tone-maps the float desktop when it hands GDI an SDR surface.

Measured on this machine at 2560×1440 and 125% for one grab: 331 ms through Graphics Capture,
142 ms through Duplication, 41 ms through BitBlt. Graphics Capture still leads the chain,
because a duplication holds the output exclusively for the length of the grab and makes any
other capture tool lose it; a screenshot that costs 190 ms more must not interrupt someone
else's recording. With the desktop composing in PQ, both DXGI rungs returned `Float16` frames
and the half-float pool is verified end to end; the engine carries them without converting.
Still `[unverified]`: the frame's orientation on a rotated display, because the session path
rotates nothing while the duplication path turns its image upright.

Window capture needs an item, not the system picker. Graphics Capture creates
one from an `HWND` through `IGraphicsCaptureItemInterop`, which is what the
selection overlay uses; the picker UI is only for a future "capture this window
from the tray" command. Protected content, UWP windows that opt out, and some
games return a black frame; the UI reports a failed capture rather than a black
screenshot.

There is no portal and no permission prompt. Windows grants screen capture to
any desktop process, which removes a whole class of Wayland failure from the
port, and it also means the app must be careful about what it captures and
where it writes it.

## The application

`crabsnap-core` keeps the selection geometry, the document, the pin model, the
protocol types, the settings schema, and the parts of the save path that name no
platform: where a shot goes given the platform's pictures folder, its file name,
and the calendar arithmetic its stamp needs. The rest of the shell is
`crabsnap-platform`, a facade over `crabsnap-platform-unix` and
`crabsnap-platform-win32` chosen by target tables exactly as `ui-host` chooses a
backend. Each sub-crate answers what the app asks a session for — the daemon
channel, the user's directories and local clock, and the capture-backend names
its build can offer — and no app code picks between them with a `cfg`.

An app opens its backend through `ui_host::connect`, which is the one name for
the per-target choice, and reads the system's language list from
`Host::languages`: Windows keeps that list in its own settings, while the POSIX
environment is only the Unix answer. The settings window runs on that today — it
was built and run on this machine, and the form came up with the system's
Chinese catalogue. Its capture-backend row asks the platform crate, so a Windows
window offers only `auto` where a Wayland one offers the three rungs. The Unix
client type-checks for Linux and the Windows one is a message-mode named pipe
scoped to the logon session, where `WaitNamedPipe` with a zero timeout answers
whether a daemon exists and `CallNamedPipe` carries one request and one answer.
`crabsnap` no longer names a backend: it opens through `ui_host::connect` and its manifest
carries no Wayland dependency, so which backend answers is `ui-host`'s target table alone.
One thing the app must still take from the backend is the display connection itself: a
Wayland grab has to borrow the loop's own, because the loop waits on that connection and a
grab on a connection of its own would only be read when the loop's next deadline expired.
That borrow is a per-platform `capture::Session` — a cloned `WaylandLink` on Unix, an empty
witness on Windows — so both platforms open a rung the same way. The frame, its packings,
the crop and the rung trait live in `crabsnap-core`, and the platform crates own the rungs:
the Unix ones are `ext.rs`, `screencopy.rs` and `portal.rs` plus the
`wp_color_manager_v1` description, and its 2000-line capture file is gone.

Built and run on Windows: `info status` reports `capture: ready` and the monitor with its
scale, `-D shot output -f out.png` wrote a 2560x1440 frame of the real desktop and the file
was looked at, `-D shot region --demo` drew the frozen screen, the selection and its readout
and toolbar, and the settings window came up with the system's Chinese catalogue. The
toolbar of a bare `crabsnap` run renders too, read back from its own window. The icons
first drew as empty boxes: Material Symbols is a system font on a Linux desktop and
nothing on Windows. `crabsnap` now ships the variable Outlined face and hands it to each
surface through `OpenOptions::fonts`, so the toolbar's glyphs draw on either platform.
What stays open is the tray icon.

The daemon runs as the user, not as a service. It owns the pipe, the clipboard,
the hotkeys, the tray icon, and the output list, and it creates surfaces on
demand. The CLI is a console program that connects to the pipe and prints; it
never creates a window. A `--local` run of the CLI starts a private daemon in
the same process, as on Wayland, and exits when the command finishes.

### Commands, settings, paths

The command set does not change. `info status` gains Windows rows: the monitor
device path, the adapter name, the capture rung in use, the hotkey registration
result, and the theme source. The settings window is the one normal window and
uses the Fluent controls the app already has.

Paths come from known folders, not environment variables: `FOLDERID_Pictures`
for the default save directory, `FOLDERID_RoamingAppData` for the config, and
`FOLDERID_LocalAppData` for the log. The first two are wired through
`SHGetKnownFolderPath`, because the user may have moved either to another drive;
the pictures module keeps `Screenshots/` under its folder, as the Wayland build
does, and `info config` prints
`C:\Users\<user>\AppData\Roaming\crabsnap\config.toml`, with `%APPDATA%` as the
answer when the known folder cannot be read. The log's folder is not wired yet.
Which folder is the base and what hangs off it are two answers, not one:
`crabsnap-core` owns the policy — `config_path(base)` — and the platform crate
answers the base.
The stamp is the other platform answer: epoch seconds carry no offset, so
`SystemTimeToTzSpecificLocalTime` supplies the local calendar fields, and a
machine with no usable zone files the shot in UTC rather than failing. The
Wayland `user-dirs.dirs` parser has no counterpart and lives only in the Unix
crate. Output names and pin titles can contain characters that are legal in a
title but not in a file name, so the
existing sanitizer needs the Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`,
`COM1` through `COM9`, `LPT1` through `LPT9`), trailing dots and spaces, and the
long-path limit. The manifest opts into `longPathAware`; internal paths still
use the `\\?\` prefix where a user-controlled name reaches a Win32 call.

### ffmpeg

The shim is C and stays C, but it is also the one piece of the app that cannot be
written in portable Rust: it `dlopen`s libavcodec, and `dlopen` names a Unix loader. So
the split follows the loader, not the feature. `crabsnap-core::ffmpeg` holds the data
types both sides hand across — muxers, encoders, encoder options, the colour record and a
decoded picture — as plain structs with `&str` accessors, so nothing in the portable crate
names C or a platform. `crabsnap-platform-unix` compiles `ffi/ffmpeg_shim.c` with `cc`,
links `-pthread`, finds its headers through pkg-config, and maps the C records onto those
types. `crabsnap-platform-win32` answers the same functions in Rust and answers "absent":
no library loader, no muxers, no encoders, and an encode or decode that says why. The
build script of the Unix crate returns before it looks for anything when the target is not
Unix, so the Windows workspace builds the crate empty rather than failing.

What Windows does instead is not started. `dlopen` becomes `LoadLibraryExW` with
`LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, `dlsym` becomes `GetProcAddress`, and the library names
become `avcodec-61.dll`, `avformat-61.dll`, `avutil-59.dll`, and `swscale-8.dll`; the app
would call `SetDefaultDllDirectories` at startup and add its own `ffmpeg` directory with
`AddDllDirectory`, never `SetDllDirectory("")`, which is the DLL-hijack path the
repository's other platform code already avoids. Until then a Windows build encodes PNG
and nothing else, and decodes what its own reader can read.

That reader is the one format the build writes itself: PNG. With no ffmpeg library and no
`ffmpeg` on `PATH`, a decode falls back to it instead of failing, so a read path —
`pin add` — still opens the pictures this app saves. It could not before, and a test that
opens a list of files and names the bad one caught it.

The executable fallback spawns `ffmpeg.exe` with `CREATE_NO_WINDOW` and puts it
in a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so a crash does not
leave an encoder orphaned. The bundled build is the same LGPL build the Wayland
package uses. If the DLLs are missing, the same degradation applies: encoding
features disappear and the rest of the app works.

### Packaging

An unpackaged per-user install under `%LOCALAPPDATA%\Programs\crabsnap`, with
the binaries, the ffmpeg DLLs, and a start-menu shortcut. Autostart is a
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value pointing at the
daemon; the settings page owns that value, and uninstall removes it. The
installer and every executable are Authenticode signed.

The executable manifest declares per-monitor DPI awareness v2, `longPathAware`,
the supported OS GUIDs, common controls v6, and `asInvoker`. Without the DPI
declaration the app is virtualized to 96 DPI and every coordinate in the
selection geometry is wrong. `asInvoker` keeps the app at the user's integrity
level; elevation would add a UAC prompt and gain nothing.

MSIX is a later option. It would give clean updates and a package identity, and
it would also restrict the autostart task and the global hotkey registration in
ways this design does not yet answer.

## Traps

| Trap | Symptom | Rule |
| --- | --- | --- |
| `CreateSwapChainForHwnd` with premultiplied alpha | `E_INVALIDARG` at startup | Use a composition swapchain for every GPU surface |
| `FLIP_DISCARD` with partial damage | Flicker and stale pixels between frames | `FLIP_SEQUENTIAL`, and no partial damage until the band logic is verified |
| Presenting without the frame-latency waitable | Dropped frames or a spinning CPU | Wait on `GetFrameLatencyWaitableObject`, as the Wayland path waits on the frame callback |
| Per-monitor v2 without handling `WM_DPICHANGED` | Blurry or wrongly sized surface after a drag between monitors | Rebuild the swapchain and add full damage |
| Mixing logical and physical coordinates for pin placement | Pins drift by a few pixels at 125% and 150% | One conversion at the surface boundary; the canvas and the capture use the same monitor DPI |
| Handling `WM_IME_CHAR` and `WM_CHAR` | Doubled characters with an IME | Read `GCS_RESULTSTR` and ignore `WM_IME_CHAR`, or the text arrives twice |
| `ImmSetCandidateWindow` with a screen-space point | The candidate list sits one window offset down and right of the caret, and follows the window | Both IMM forms take client-area coordinates, and `CFS_EXCLUDE` places the list off the caret |
| `RegisterHotKey` on a thread without a message loop | The hotkey never fires | Register on the loop thread and pump |
| Duplicating an output from the wrong adapter | Black frames on hybrid laptops | Duplicate from the output's own adapter |
| Not excluding own windows from capture | The overlay appears in the screenshot | `WDA_EXCLUDEFROMCAPTURE` on every own HWND, and capture before opening |
| `WS_EX_TRANSPARENT` for click-through | The window also loses the keyboard | Hit testing through `WM_NCHITTEST` or a window region |
| `SetForegroundWindow` from a background process | The overlay opens without focus and Escape does nothing | Register the hotkey on the loop thread, and keep the low-level hook fallback |
| Copying an image as `"PNG"` only | Explorer and Word paste nothing | Add `CF_DIBV5` and `CF_DIB` |
| `CF_DIB` with straight alpha | White or black fringes around the selection | `CF_DIBV5` with an alpha mask, or PNG |
| Named pipe without an ACL or `PIPE_REJECT_REMOTE_CLIENTS` | Another user or a remote client connects | Explicit descriptor, session and user in the name |
| `LoadLibrary("avcodec-61.dll")` from the working directory | DLL hijack | `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` and `AddDllDirectory` |
| `UpdateLayeredWindow` for a full-screen canvas every frame | A CPU copy of the monitor per frame | Layered windows carry no surface here; the composition swapchain does |
| Pacing on `DwmFlush` | Frames dropped into a closed window | Pace on the swapchain waitable |
| `Shell_NotifyIcon` without `TaskbarCreated` | The tray icon disappears after an Explorer restart | Re-add on the broadcast |
| Using `\\.\DISPLAY1` as a saved output id | The setting attaches to the wrong monitor after a dock change | Hash the monitor device path |
| A capture of a window that excludes itself | The window is missing from the capture, or black on older builds | Expected; the UI says so instead of showing a black image |
| Running the overlay over an elevated window | The overlay stays behind the elevated window and cannot take focus | Document the UIPI boundary; do not try to bypass it |
| Long paths or reserved names in a shot title | Save fails with a path error | Sanitize, prefix `\\?\`, and check the length before the call |

## Work order

Each stage ends with a working application, not a layer. The order follows the
dependency direction in `crate-map.md`: platform, then paint, then app.

| Stage | Content | Exit criterion |
| --- | --- | --- |
| 0 | `ui-platform-win32`: HWND surfaces, message pump, DPI, the D3D11 device and its composition swapchain for opaque surfaces — solids, glyphs, images, readback — outputs, language, theme, waker; the crate is classified `PLATFORM_BOUND` with a reason | `ui-gallery` runs on Windows at 100% and 150%, on two monitors, and the `d3d_*_scene` probes match the CPU reference on WARP and on a GPU |
| 1 | Services: clipboard, drag-and-drop, hotkeys, named pipe, tray, single instance | `crabsnap shot --clipboard` pastes into Explorer and Word; `pin list` answers over the pipe; a taken hotkey is reported |
| 2 | Transparency, layers, effects, canvases, HDR, input regions, activation policy | A pin canvas is click-through between pins and focused on a pin; the overlay is absent from its own capture; HDR is correct on a calibrated display |
| 3 | Capture: WGC, Desktop Duplication, GDI fallback, `ContentColour`, the app shell, the daemon, settings, paths, ffmpeg | The Wayland verification list for the screenshot app passes on Windows, including HDR and window capture |
| 5 | Packaging: installer, signing, autostart, tray, uninstall, first-run | A clean VM with no developer tools installs, captures, pins, and uninstalls |

Stage 0 is the largest single item and the one that de-risks the rest. Stage 2
cannot start before the extraction, which is why the extraction is inside the
stage and not a separate research task.

## Verification

Layering is checked the same way it is now:
`cargo test -p ui-platform --test platform_boundary` after adding the new
crate, with the reason written in the array. No portable crate may grow a
`cfg(windows)` branch, and no platform crate may be depended on by a portable
one.

The parts that can be tested without a desktop are the valuable ones, and they
are ordinary unit tests: virtual-key to `Key` tables per layout, unshifted
character rules, surrogate assembly, DPI conversion, rectangle placement for
every anchor, clipboard format conversion in both directions, pipe framing,
path sanitization, and the output-id hash. They run on the development machine,
which is Windows, and in CI, which may be Linux: the tables are data, so they
compile on both.

The renderer is compared against the CPU path, not against a golden image from
a GPU. The `d3d_*_scene` examples mirror the existing `gpu_*_scene` ones,
render the same scenes offscreen, and compare region means within the tolerance
the CPU comparisons already use. WARP makes this runnable on a machine with no
GPU, which is the only way the test is honest in CI; a WARP-versus-hardware
comparison is a separate, optional check.

Live behaviour needs a real desktop and a real user: the click-through probe,
the activation probe, the DPI drag between monitors, the HDR check, the
clipboard round trip with Explorer, Word, and a browser, the tray restart, and
the elevated-window boundary. Each becomes a line in the Windows section of the
verification page, with the same evidence style as the Wayland entries: the
command, the display or app, and the observed result.

No test sleeps or reads the wall clock. The frame tests drive the loop with the
existing test clock; the input tests feed messages directly. The Windows
message queue is not a test clock, and a test that waits for `WM_TIMER` is a
flaky test.

## Status

| Area | Status |
| --- | --- |
| Win32 backend, D3D11 presenter | Done: window, input, DPI, clipboard text, theme, `HostSession` trait, and the process `Backend` (one pump, several windows, outputs, languages, waker); see `docs/src/rendering-backends.md` |
| HDR swap chain | Implemented: half-float PQ chain on an output the desktop composes in HDR, 8-bit sRGB otherwise, SDR white and the luminance range from the display configuration. Measured only on an SDR panel, where the fallback and the PQ encode were checked; the HDR chain has not been presented on a display showing PQ |
| D3D11 renderer | Done |
| Layer surfaces | Approximated: a `WS_POPUP` tool window on the primary monitor, topmost for `Overlay`/`Top`, placed by anchor, size and margin arithmetic, `WS_EX_NOACTIVATE` for `KeyboardMode::None`, and click-through outside the published input region; `Bottom`/`Background` are not topmost, `Reserve` is ignored, and `FollowPointer` is click-to-focus |
| Transparent surfaces and input regions | Done for a layer: `HTTRANSPARENT` is verified with `WindowFromPoint` landing on the window below. A translucent capture window is still planned |
| Clipboard, drag-and-drop, hotkeys | A copy carries text, HTML, a file list and a picture, each as every Windows format it needs; a paste is text only; hotkeys done through `RegisterHotKey` against a message-only window; drag-and-drop and delayed rendering planned |
| Named-pipe IPC | Server done: message mode verified by `host_services`, where a client connects, sends one message and reads the answer; a worker's deferred reply is implemented but unexercised |
| Capture via WGC, Desktop Duplication and BitBlt | Done for a full output, one file per rung behind one trait: Graphics Capture through a free-threaded session, Duplication from a device on the output's adapter, and a GDI blit as the last resort. A float desktop travels as scRGB half floats under `PixelStorage::Float16`, unconverted, so a pin keeps its light above white; only a file's rendition encodes. Rotation and DPI scale are carried on the frame, and the two DXGI rungs agree on one desktop to a mean of 0.04 codes. Rotated displays and window items are unverified, and `WDA_EXCLUDEFROMCAPTURE` is planned |
| The app on Windows | Builds and runs: `crabsnap` and `crabsnap-settings` compile for Windows with no Wayland dependency, and `ui_host::connect` is the one way in. Verified here: `info status` reports the capture rung and the monitor, `-D shot output` wrote a real 2560x1440 frame, `-D shot region --demo` drew the frozen screen with its selection, readout and toolbar, and the settings window came up. `crabsnap-platform` is a facade over a Unix socket client and a Windows named-pipe client, the save path's known folders and local clock, and the ffmpeg shim — C on Unix, "absent" in Rust on Windows, with a PNG reader as the decode fallback. Not done: the tray icon and the Windows ffmpeg loader |
| IME | Composition done through IMM32: open status from the frame's `ImeState`, composition and candidate windows at the caret, preedit and commit as engine events; surrounding text and `DeleteSurrounding` need TSF |
| Packaging and signing | Planned |
| Vulkan | Not planned; see [Is Vulkan the right renderer?](#vulkan) |
| Windows 10 support | Unverified; the design targets Windows 11 22H2 and names the Windows 10 2004 API floor where one exists |

The `[to verify]` markers above are the items that need a spike before they
become commitments: cross-process hit testing through a composition window, the
colour space WGC actually delivers on an HDR display, and whether Desktop
Duplication can coexist with the system's own capture tools on the target
machines. The focus-return path for `FollowPointer` is settled: the contract
degrades to click-to-focus, because no unrequested foreground change survives
the shell's rules.

## Documentation gaps this port exposes

Five statements in the book are now known to be incomplete. They are listed
here because the port is where they stop being harmless.

- `platform-boundary.md` and `rendering-backends.md` present "implement
  `PresentTarget` or `SceneTarget`" as the whole of adding a platform. That is
  true for the loop and false for the renderer: `ui-paint-gpu` depends on
  `ui-gpu-egl` unconditionally, and `GpuSurface::bind` takes a `glow::Context`.
  A second GPU API needs the extraction described above.
- `event-loop.md` says the app's platform layer obtains the backend. Every binary
  now goes through `ui_host::connect`, and the one thing they take from the
  backend is the `capture::Session` a grab borrows the loop's connection with.
  The page should say that rather than describe the seam as planned.
- `window.md` says a platform without layer surfaces approximates the role with
  `WS_EX_TOPMOST` and `WS_EX_TOOLWINDOW` and ignores `keyboard`. The real
  approximation also covers placement from anchor, size and margin, activation,
  and per-area hit testing; only `keyboard: FollowPointer` has no counterpart,
  and it degrades to click-to-focus.
- `fonts.md` says font matching on Windows is not implemented. The dependency
  comment on fontique says Windows uses DirectWrite, so a Windows font source
  exists; the gap is which face `SystemUi` resolves to and what the default
  family is, not whether the platform can supply fonts.
- `cli.md` says Windows named pipes are not written. This report defines the
  transport; the page should link here instead of restating the gap.

The Status section on `rendering-backends.md` now names the Win32 window, the
D3D11 device, the process `Backend`, the layer approximation, the input method's
composition, the clipboard's formats and the HDR swap chain as implemented.
Vulkan is still absent. What remains of stage 2 is unmeasured rather than
unwritten: no display here composes in HDR, so the half-float chain has only been
exercised with the renderer forced and the back buffer read back, not on a panel
showing PQ.

## Corrections

**The grab owned its own display connection.** The first cut of the app's platform
seam opened a second Wayland connection for a capture. That connection is waited on
by nobody: the loop sleeps on its own connection, and a pending grab is polled once
per turn, so a frame arriving on a private one is read only when the loop's next
deadline expires — two seconds of latency on every capture, and none at all if the
loop is idle with no deadline. A `capture::Session` now carries the backend's own
connection (a clone of the `WaylandLink`), which is what the rung polls. What
catches it now: the Wayland rung's `Pending::poll`/`deadline` pair, and the app's
`Session::of(&backend)` call before the backend moves into the host.

**The app borrowed a Windows answer for a Unix question.** `Config::path()` lived in
`crabsnap-core` and built `$HOME/.config/crabsnap/config.toml` on every target, so a
Windows build filed its config under whatever `HOME` said. Which folder is the base
is now a platform answer — `XDG_CONFIG_HOME` or `~/.config` on Unix,
`FOLDERID_RoamingAppData` on Windows — and `crabsnap-core::paths::config_path(base)`
keeps the policy of what hangs off it. What catches it now: `info config` prints the
resolved path, and the two platform crates answer it separately.

**A platform crate that does not compile off its target takes the workspace with it.**
`cargo check --workspace` on Windows failed inside `wayland-sys`, pulled in by the
Unix platform crate's dependencies. The crates are now empty off their own target
(`#![cfg(unix)]`, `#![cfg(windows)]`, as `ui-platform-win32` already was), the Unix
build script returns before it looks for headers, and the ffmpeg bindings follow the
C compiler that only Unix has rather than the feature. What catches it now:
`cargo build --workspace` on Windows, and the app's own build.

**Presentation went through an HWND swapchain, not composition.** D2 argues for
`CreateSwapChainForComposition` because an HWND swapchain cannot be premultiplied,
and that is still where a translucent surface has to go. What was built presents
through `CreateSwapChainForHwnd` with `DXGI_ALPHA_MODE_UNSPECIFIED`, which is opaque:
a window, the overlay and the toolbar all draw correctly, and a surface whose pixels
are partly transparent composites as if they were not. What catches it now: the
Status row on transparent surfaces, which still names them as planned, and the app's
toolbar, whose 240-alpha bar renders opaque.

## See also

- `docs/src/architecture.md` for the tiers this port keeps.
- `docs/src/crate-map.md` for where the new crates belong.
- `docs/src/engine/event-loop.md` for the `Host`, `Backend`, and
  `SurfaceHandle` contracts.
- `docs/src/engine/window.md` for the surface and layer fields mapped here.
- `docs/src/engine/present-targets.md` for `PresentTarget` and `SceneTarget`.
- `docs/src/apps/screenshot-app.md` for the capture chain and decisions D1-D10.
- `docs/src/apps/cli.md` for the commands and the IPC protocol.
- `docs/src/engine/colour.md` for `SurfaceColour`, `ContentColour`, and the
  output transforms.
- `docs/src/reference/engine-gaps.md` for the gaps this port does not close.

## Sources

- [CreateSwapChainForComposition](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgifactory2-createswapchainforcomposition)
- [Layered windows](https://learn.microsoft.com/en-us/windows/win32/gdi/layered-windows)
- [High dynamic range and wide colour](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range)
- [DXGI_OUTPUT_DESC1](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_6/ns-dxgi1_6-dxgi_output_desc1)
- [Screen capture](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture)
- [Desktop Duplication API](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nn-dxgi1_2-idxgioutputduplication)
- [SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)
- [WM_NCHITTEST](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-nchittest)
- [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)
- [Named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes)
- [GetUserPreferredUILanguages](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getuserpreferreduilanguagesw)
- [Text Services Framework](https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework)
- [DWM window attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
- [VK_KHR_win32_surface](https://registry.khronos.org/vulkan/specs/latest/man/html/VK_KHR_win32_surface.html)
- [VK_KHR_external_memory_win32](https://registry.khronos.org/vulkan/specs/latest/man/html/VK_KHR_external_memory_win32.html)

