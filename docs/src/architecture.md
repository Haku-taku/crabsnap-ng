# Architecture

`ui/` is a self-contained workspace: an engine that turns declarations into
pixels, and three complete control sets on top of it. It is not an
application. The gallery exists to exercise the controls, not to be shipped.

This page describes the tiers and the laws that hold them apart. The per-crate
table is in [Crate Map](./crate-map.md).

## Tiers

| Tier | Crates | Knows about |
| --- | --- | --- |
| Vocabulary | `ui-core`, `ui-color-hct`, `ui-i18n`, `ui-text` | Types and contracts; no I/O |
| Paint | `ui-paint`, `ui-gpu-scene`, `ui-paint-gpu`, `ui-gpu-egl` | Primitives to pixels |
| Platform and engine | `ui-platform`, `ui-platform-wayland` | Windows, surfaces, the frame loop |
| Design system | `ui-design`, `ui-style` | Colour roles, materials, motion, capabilities |
| Controls | `ui-widgets`, `ui-widgets-md3`, `ui-widgets-fluent`, `ui-widgets-glass` | Declarations built from the layers below |
| Consumers | `ui-demo`, `ui-gallery` | Everything above |

Dependencies point upward in this table. A lower tier never names a higher one.

## The enforced law

Only one boundary is checked by a test:
**portable crates may not touch the platform**.

`crates/ui-platform/tests/platform_boundary.rs` asserts three things:

| Test | Asserts |
| --- | --- |
| `portable_crates_use_no_platform_specific_apis` | No `std::os::unix/windows`, `cfg(target_os)`, `cfg(unix/windows)`, `libc::`, `OwnedFd`, `FromRawFd`, or `std::process` in a `PORTABLE` crate's source |
| `portable_crates_do_not_depend_on_platform_crates` | No `[dependencies]` on a `PLATFORM_BOUND` crate from a `PORTABLE` one |
| `every_abstraction_crate_is_classified` | Every crate appears in `PORTABLE` or `PLATFORM_BOUND` |

The scan strips `//` comments before looking, so a banned token in a comment is
not a violation. Dev-dependencies are exempt, which is why `ui-gallery` can be
portable while its examples use a platform backend — named per target in its
manifest, since a target can only name the backends that compile there.

`PLATFORM_BOUND` today: `ui-gpu-egl`, `ui-gpu-d3d11`, `ui-platform-wayland`,
`ui-platform-win32`, `ui-host`, `ui-demo`. Everything else is `PORTABLE`, including the
painter: `ui-gpu-scene` decides what to draw, and `ui-paint-gpu` (GL) and
`ui-gpu-d3d11` (Windows) are its devices behind that trait.

## Conventions that no test enforces

These are real rules, enforced by review rather than by cargo:

- **Direction only.** A crate may depend on its own tier or a lower one. A
  same-tier or upward edge needs a reason in the change description; there is
  no `xtask` to catch it.
- **No cross-crate `cfg`.** Choose a backend with a Cargo feature or with
  runtime capability detection. A `#[cfg]` that applies to one crate silently
  evaluates false in another, and the code disappears without a build error.
- **Capabilities are data, not compile-time facts.** What a compositor or
  driver supports is discovered at runtime and passed down. A portable crate
  must not assume it.

> **Wrong path:** adding `libc` or `std::process` to `ui-core` "just for one
> platform". The boundary test fails, and the portable crate stops being
> usable for the CPU-only path and for tests.

## Backends

- **Presentation** has a CPU path (`ui-paint`) and a GPU path (`ui-gpu-scene`
  driving a device: `ui-paint-gpu`'s GL device onto a target from `ui-gpu-egl`,
  or `ui-gpu-d3d11`, offscreen or behind `ui-platform-win32`'s swap chain). The
  Wayland backend selects the GPU path with its `gpu` feature; without it, it
  presents through shared memory. See
  [Rendering Backends](./rendering-backends.md).
- **Text** is split between the contract (`ui-text`: font stacks, shaping
  output, fallback) and a provider crate (`ui-text-fontique`). The system font
  stack is the provider's `system` feature, off by default so unit tests do not
  depend on installed fonts.
- **Localisation** is Fluent. `ui-i18n` generates its key table in `build.rs`
  from `locales/*/*.ftl`, and negotiates the shipped languages against the
  user's choice. The host supplies the choice and the platform's language list;
  a library crate never reads `LANG`. See
  [Language and Locale](./localization.md).

## Where a change goes

| Change | Crate |
| --- | --- |
| A layout, hit, event, damage, or animation contract | `ui-core` |
| A primitive, batch, or CPU raster rule | `ui-paint` |
| A batch walk, layer, damage band, or pass order | `ui-gpu-scene` |
| A GLSL program or GL resource | `ui-paint-gpu` |
| An HLSL program, D3D object, or DXGI resource | `ui-gpu-d3d11` |
| An EGL, gbm, or dma-buf detail | `ui-gpu-egl` |
| Window, surface, frame loop, presentation target | `ui-platform` |
| Wayland protocol or buffer handling | `ui-platform-wayland` |
| A Win32 window, message pump, or DXGI swap chain | `ui-platform-win32` |
| A colour role, material, motion curve, or capability | `ui-design`, `ui-style` |
| A control shared by two or more systems | `ui-widgets` |
| A control of one design system | that system's `ui-widgets-*` crate |

Adding a crate means classifying it in `platform_boundary.rs` in the same
change. A new platform backend goes into `PLATFORM_BOUND` with a written
reason; `PORTABLE` is never widened.

## See also

- [Crate Map](./crate-map.md): what each crate owns, module by module.
- [Platform Boundary](./platform-boundary.md): how to add a backend without
  breaking the law.
- [Frame Pipeline](./engine/frame-pipeline.md): what the engine does with a
  declaration.
