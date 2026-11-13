# Platform Boundary

A crate is either portable or platform-bound, and the split is enforced by a
test rather than by review. This page states what each side commits to, how to
add a backend, and where the test does not look.

## The two lists

`crates/ui-platform/tests/platform_boundary.rs` holds them.

`PLATFORM_BOUND` — six crates, each with a written reason:

| Crate | Reason in the test |
| --- | --- |
| `ui-platform-wayland` | Wayland window backend: `wayland-client`, `libc`, `xkbcommon` |
| `ui-demo` | Showcase bootstrap: opens a Wayland window and exits the process |
| `ui-gpu-egl` | EGL device + GBM + dma-buf: unix fds, `libc`, `libgbm` |
| `ui-gpu-d3d11` | D3D11 device: COM, DXGI and d3dcompiler are Win32 APIs, and the crate is empty off Windows |
| `ui-platform-win32` | Win32 window, input and DXGI swap chain: user32 and dxgi are Windows APIs, and the crate is empty off Windows |
| `ui-host` | Host facade: re-exports this target's window backend, so it depends on a platform crate by design |

A backend whose platform is absent builds as an *empty library*, not as a compile error:
`ui-gpu-egl` and `ui-platform-wayland` gate their bodies with `#![cfg(unix)]` and declare
their bindings in a `[target.'cfg(unix)'.dependencies]` table, exactly as `ui-gpu-d3d11` and
`ui-platform-win32` do for Windows. Gating the body alone is not enough — an empty crate
still builds whatever it declares — and a body without the gate does not build at all.

An application depends on `ui-host`, never on a backend. Its target tables choose the
backend, its `src/lib.rs` re-exports the choice, and nothing in app code or app manifests
mentions an OS. That the choice is compile-time is unavoidable: a crate that names Wayland's
protocols cannot compile on Windows. Capabilities are still decided at runtime.

Everything else is `PORTABLE`: `ui-core`, `ui-color-hct`, `ui-design`,
`ui-gallery`, `ui-gpu-scene`, `ui-highlight`, `ui-i18n`, `ui-math`, `ui-paint`,
`ui-paint-gpu`, `ui-platform`, `ui-richtext`, `ui-style`, `ui-text`,
`ui-text-fontique`, `ui-widgets`, `ui-widgets-md3`, `ui-widgets-fluent`,
`ui-widgets-glass`.

## What the checks scan

| Test | Scope | Fails on |
| --- | --- | --- |
| `portable_crates_use_no_platform_specific_apis` | `<crate>/src/**/*.rs`, `//` comments stripped | `std::os::unix`, `std::os::windows`, `cfg(target_os`, `cfg(unix`, `cfg(windows`, `libc::`, `OwnedFd`, `FromRawFd`, `std::process` |
| `portable_crates_do_not_depend_on_platform_crates` | The `[dependencies]` section of a portable `Cargo.toml` | Any platform-bound crate name starting a dependency line |
| `every_abstraction_crate_is_classified` | Every `crates/*/Cargo.toml` | A crate in neither list |

Two consequences worth knowing:

- `[dev-dependencies]` is deliberately exempt. `ui-gallery` is portable and
  still opens a Wayland window in its examples.
- The API scan covers `src/` only. A `build.rs` is not scanned.

## Known holes

The dependency check reads one section literally. These pass the test and are
still violations:

- Putting the dependency under `[target.'cfg(...)'.dependencies]`.
- Renaming the dependency
  (`wayland = { package = "ui-platform-wayland", ... }`).
- Doing platform work in `build.rs`.

Review has to catch these. The test is a floor, not a proof.

## Adding a platform backend

1. Create `crates/<name>/`. The root manifest's `members = ["crates/*"]`
   picks it up; `crates/assets` is the only exclusion.
2. Register it in `PLATFORM_BOUND` with a reason. Never move a platform crate
   into `PORTABLE` to silence the classification check: the other two checks
   then apply and fail on its OS code.
3. Keep edges downward: the backend depends on `ui-core`, `ui-paint`,
   `ui-platform`; no portable crate may depend on it.
4. Implement one of the presentation contracts:
   - `PresentTarget` for CPU pixels: `kind`, `size`, `pixels_mut`,
     `present(&Damage)`, optionally `set_cursor` and `set_ime`.
   - `SceneTarget` for an engine-owned scene (the GPU path): `size`,
     `present_scene(&DrawList, &TextPass, &Damage)`, optionally `set_cursor`,
     `set_ime`, `commit_pending`.
   - Reuse the GPU painter by implementing `ui_gpu_scene::RenderTarget` (`size`,
     `target_id`) for the target it draws into. A different graphics API needs
     its own `ui_gpu_scene::Device` and a device crate beside `ui-paint-gpu`
     and `ui-gpu-d3d11`, not just this trait.
5. Detect capabilities at runtime and pass them down. Gate the backend with a
   Cargo feature if it has an optional dependency;
   `ui-platform-wayland/gpu` is the model.
6. Test it from a platform-bound example, or from `[dev-dependencies]` of a
   portable example.

## Wrong paths and their symptoms

| Change | Symptom |
| --- | --- |
| `libc::` or `std::process` in `ui-core` | The API check fails with `path:line uses ...` |
| A platform crate under `[dependencies]` of a portable crate | The dependency check fails |
| Moving a platform crate into `PORTABLE` | The API check now applies to its OS code |
| A new crate with no classification | `neither in PORTABLE nor PLATFORM_BOUND: [...]` |
| Resolving a capability with `#[cfg]` in a second crate | The code vanishes and the build still succeeds |

## See also

- [Architecture](./architecture.md): the tiers and the conventions no test
  enforces.
- [Rendering Backends](./rendering-backends.md): what a GPU backend has to
  guarantee.
- [Crate Map](./crate-map.md): what each portable crate may depend on.
