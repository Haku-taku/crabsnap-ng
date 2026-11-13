# Crate Map

Every crate in `ui/crates/`, what it owns, and what it may not touch. A change
should land in the crate that already owns the contract; if none does, add the
contract to the lowest crate that can express it.

## Portable

| Crate | Owns | Key modules | May not |
| --- | --- | --- | --- |
| `ui-color-hct` | HCT colour space: CAM16 hue and chroma, CIELAB lightness, contrast, palette solving, MD3 role schemes from a seed, a picture's dominant colours | `cam`, `lab`, `contrast`, `palette`, `quantize`, `scheme`, `solve`, `viewing` | Depend on any other crate here |
| `ui-core` | The engine's vocabulary and contracts: ids, tree, layout, hit, events, damage, animation, text input, selection | `tree`, `layout`, `layout_engine`, `flex`, `grid`, `draw`, `hit`, `event`, `damage`, `anim`, `text_input`, `selection`, `menu` | Platform APIs; platform crates |
| `ui-design` | Semantic design layer plus the three native systems' palettes and tokens | `md3/`, `fluent*`, `hig`, `winui`, `caps` | Platform crates |
| `ui-i18n` | Fluent catalogs, the generated key table, and language negotiation; `Catalog`, `Language`, `Cx::t` | `build.rs`, `language`, `locales/{en,zh-CN}` | Read `LANG`; the app supplies the choice and the platform's language list |
| `ui-paint` | Typed primitives, batching, command encoding, the CPU rasterizer (zeno) | `prim`, `batch`, `raster`, `encode` | Platform crates |
| `ui-platform` | Presentation targets, the CPU window/surface app loop, the portable session contract both window backends implement, plus the platform's language list | `driver`, `view`, `present`, `session`, `element`, `text_pass`, `state`, `profile`, `locale` | Platform crates |
| `ui-style` | Style objects, environment, colour roles, materials, motion, control metrics, capability plumbing | `style`, `env`, `roles`, `material`, `motion`, `capability`, `control_metrics` | Platform crates |
| `ui-richtext` | The block document model: blocks with source spans, document-level marks and their clipping per block, the Markdown import that fills them, and the conversion of a leaf into runs | `document`, `markdown`, `inline` | Platform crates; render or lay text out |
| `ui-highlight` | Syntax highlighting over tree-sitter: token classes and byte ranges, one lazily built grammar per language | Platform crates; carry a colour or a theme |
| `ui-math` | Formula layout behind a seam: LaTeX to a model tree, then extents and a glyph/rule/line display list; a math face parsed once | Platform crates; do I/O or carry a theme |
| `ui-text` | Font stack contract, shaping output, bidi and line breaking, platform provider abstraction | `stack`, `shape`, `bidi`, `linebreak` | Platform crates |
| `ui-text-fontique` | The fontique-backed provider and glyph rasterizer; `system` feature turns on the OS stack | provider, raster | Assume a system font exists unless `system` is on |
| `ui-widgets` | Controls shared by two or more systems, and the widget plumbing | `button`, `list`, `math`, `scroll`, `surface`, `icon`, `text`, `scaffold`, `styled`, `state` | Names from a single design system |
| `ui-widgets-md3` | The MD3 control set | one module per control, plus `spec/` and `tests/` | Platform crates |
| `ui-widgets-fluent` | The Fluent (WinUI 3) control set | `button`, `checkbox`, `math`, `radio`, `slider`, `textbox`, `toggle`, `progress`, `tab_view`, `infobar`, `tooltip`, `dialog` | Platform crates |
| `ui-widgets-glass` | The Liquid Glass control set | `card`, `dialog`, `math`, `popover`, `overlay`, `morph`, `search_bar`, `action_sheet`, `modal_sheet`, `stepper` | Platform crates |
| `ui-gpu-scene` | The backend-neutral GPU paint scene: batch walking, the layer stack, damage bands, backdrop ordering, the pass sequence | `device`, `renderer`, `program`, `effect`, `target`, `format`, `layout` | Platform crates; name a graphics API |
| `ui-gallery` | No library: the `gallery` and `list_bench` examples and their pages | `examples/gallery/` | Be depended on by anything |

## Platform-bound

| Crate | Owns | May not |
| --- | --- | --- |
| `ui-gpu-egl` | EGL display and context, GBM buffers, dma-buf export, explicit sync, the render node to use | Be a dependency of a portable crate |
| `ui-paint-gpu` | The GL device the scene draws through: shaders, vertex arrays, texture storage, framebuffers | Be a dependency of a portable crate |
| `ui-gpu-d3d11` | The D3D11 device: COM objects, generated HLSL and its constant-buffer layout, DXGI textures and targets; empty off Windows | Be a dependency of a portable crate |
| `ui-platform-win32` | The Win32 window and its DXGI flip-model swap chain: the `SceneTarget` the engine presents through on Windows, the persistent frame target and the copy into the back buffer, the message pump's input and clipboard, the refresh/scale/theme queries, and the `Session` host | Be a dependency of a portable crate |
| `ui-platform-wayland` | The Wayland XDG target: protocol glue, `wl_shm` buffers, input, clipboard, presenting | Be a dependency of a portable crate |
| `ui-host` | The facade an app depends on: target tables pick the window backend, and two `pub use` lines re-export that `Session` beside the portable `HostSession` trait it implements | Contain code, or be a dependency of a portable crate |
| `ui-demo` | Shared showcase bootstrap for the demo binaries: the faces an app carries, the wallpaper, scripted input and the frame loop — everything but the window | Be a dependency of a portable crate |

## Dependency shape

Read each line as "depends on". There are no other internal edges.

| Crate | Internal dependencies |
| --- | --- |
| `ui-core`, `ui-color-hct`, `ui-highlight`, `ui-i18n` | None |
| `ui-gpu-scene` | `ui-core` |
| `ui-gpu-egl` | `ui-gpu-scene` |
| `ui-richtext` | `ui-core`, `pulldown-cmark` |
| `ui-math` | `ui-core`, `math-core`, `math-syntax`, `math-font`, `math-editor` |
| `ui-text` | `ui-core` |
| `ui-text-fontique` | `ui-core`, `ui-text` |
| `ui-paint` | `ui-core` |
| `ui-design` | `ui-core`, `ui-color-hct` |
| `ui-platform` | `ui-core`, `ui-paint`, `ui-text`, `ui-text-fontique`, `ui-math` |
| `ui-paint-gpu` | `ui-core`, `ui-gpu-scene` |
| `ui-gpu-d3d11` | `ui-gpu-scene`; `ui-core` for its example |
| `ui-host` | `ui-platform`, and `ui-platform-wayland` (unix) or `ui-platform-win32` (windows) |
| `ui-platform-win32` | `ui-core`, `ui-platform`, `ui-gpu-scene`, `ui-gpu-d3d11` |
| `ui-platform-wayland` | `ui-core`, `ui-paint`, `ui-platform`, `ui-text`, `ui-text-fontique`, and `ui-paint-gpu` with `ui-gpu-egl` under `gpu` |
| `ui-style` | `ui-core`, `ui-design` |
| `ui-widgets` | `ui-core`, `ui-design`, `ui-style`, `ui-platform`, `ui-i18n` |
| `ui-widgets-md3` | `ui-core`, `ui-design`, `ui-style`, `ui-widgets`, `ui-i18n` |
| `ui-widgets-fluent` | `ui-core`, `ui-design`, `ui-platform`, `ui-style`, `ui-widgets` |
| `ui-widgets-glass` | `ui-core`, `ui-design`, `ui-style`, `ui-widgets` |
| `ui-demo` | `ui-core`, `ui-platform`, `ui-host` |
| `ui-gallery` | All of the above; through `ui-host` as its only window dependency, examples only |

`ui-core` is the hinge: it depends on nothing else in the workspace, and nearly
everything depends on it. Adding an engine dependency to `ui-core` changes the
shape of the whole tree; prefer a trait defined there and implemented
elsewhere.

The edges above are what the manifests say today. Only the portable/platform
split is enforced by a test; the rest is held by review.

## The application

`apps/crabsnap` is the screenshot and pin tool. It is not a member of this workspace: it has
its own workspace root (`apps/Cargo.toml`) and shares the target directory, so the framework's
build never includes it and the app's build never changes the framework's feature set. The
dependency points one way — the app depends on `ui/crates/*` by path, and no crate here names
it.

## Adding a crate

1. Put it where its tier says, not next to its first caller.
2. Classify it in `crates/ui-platform/tests/platform_boundary.rs`.
3. Give it a `description` in `Cargo.toml`: one sentence naming what it owns.
4. Add the dependency edges you need, and no others. A dependency added "for
   one function" is a missing abstraction.

See [Architecture](./architecture.md) for the tier rules and
[Platform Boundary](./platform-boundary.md) for what the classification
commits you to.
