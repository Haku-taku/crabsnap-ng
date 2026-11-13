# External Sources

This engine reimplements problems that mature frameworks have already
solved. Read their source for the invariants they enforce — the freeze rule
in a flex solver, the buffer lifecycle behind a frame callback — and design
from those. Copying their module layout carries their assumptions
without their reasons.

## Which authority to read

| Question | Read |
| --- | --- |
| Layout semantics | CSS Flexbox §9.7, CSS Grid §12, taffy; Qt `QLayoutItem`/`QSizePolicy` and GTK 4 `GtkLayoutManager` for size negotiation |
| Text layout, fallback, bidi | parley, Flutter `painting`, UAX #14 / #29 / #9 |
| Language negotiation and the system locale | `fluent-langneg` 0.14 `negotiate.rs` (six matching levels), `sys-locale` 0.3 `unix.rs`, GLib `g_get_language_names`, Android `LocaleManager`, Windows `GetUserPreferredUILanguages` |
| MD3 visuals and motion | Flutter `material` (the reference implementation behind the tokens) |
| MD3 forms Flutter's `material` does not share (rich tooltip, wavy progress, modal date pickers, modal bottom sheet, split button) | AndroidX Compose Material3 `Tooltip.kt`, `WavyProgressIndicator.kt`, `internal/LinearWavyProgressModifiers.kt`, `DatePicker.kt`, `DateRangePicker.kt`, `ModalBottomSheet.kt`, `SheetDefaults.kt`, `FloatingActionButton.kt`, `SplitButton.kt` (material3 1.4.0) |
| MD3 side sheet (no Compose implementation) | M3 side sheet specs page (24 dp inset, 400 dp maximum); MDC-Android `sidesheet/` sources and `dimens.xml` on `master` (256 dp panel, `corner.large`) |
| Fluent / WinUI visuals | WinUI 3 XAML is the first authority; `fluent_ui` for widget structure |
| Liquid Glass | Apple HIG plus the two local glass implementations |
| GPU presentation, damage, effects | Skia sampling and filters, GPUI's wgpu backend, egui's tessellator, `wayland.xml`, wlroots |
| Hit testing and input routing | egui `Sense`/`interact`, GPUI's per-layer hit test |
| System tray | Win32 `Shell_NotifyIcon` and `NOTIFYICONDATA` (the version 4 callback layout), Qt `QSystemTrayIcon` and its `org.kde.StatusNotifierItem` adaptor, Electron `Tray`, tray-icon's Windows implementation |
| Ownership, sharing, copies | GPUI `SharedString`, egui `Arc<Galley>`, Bevy `RenderAsset`, rowan, cosmic-text `ShapeBuffer`; the rules are in [Ownership and Copies](../conventions/ownership.md) |

## Local copies

On this machine, already downloaded — no network needed:

| Source | Path |
| --- | --- |
| egui / epaint 0.36.2 | `~/.cargo/registry/src/*/egui-0.36.2/src/` |
| taffy 0.15 | `~/.cargo/registry/src/*/taffy-0.15.0/src/` |
| GPUI 0.2.2 (Zed `79fe6a6`) | `~/.cargo/git/checkouts/zed-*/79fe6a6/crates/gpui/src/` |
| cosmic-text 0.19.0 | `~/.cargo/registry/src/*/cosmic-text-0.19.0/src/` |
| GTK 4 layout contract | `/usr/include/gtk-4.0/gtk/gtklayoutmanager.h` |
| Qt 6 layout contract | `/usr/include/qt6/QtWidgets/qlayoutitem.h` |
| Qt 6 tray contract | `/usr/include/qt6/QtWidgets/qsystemtrayicon.h`, `/usr/include/qt6/QtGui/6.12.0/QtGui/private/qstatusnotifieritemadaptor_p.h` |

Widget reference clones live in `~/git/widget-ref/`: `flutter` (sparse:
`painting`, `material`, `animation`), `fluent_ui`, `winui`,
`liquid_glass_widgets`, `flutter_liquid_glass`.

## Token exports

The generated tables are built from the export snapshots in
`docs/reference/`, not from the frameworks themselves:

| Export | Feeds | Taken from |
| --- | --- | --- |
| `md3-color-roles.json` | `ui-design/data/md3_data.rs` | Flutter `material` colour roles |
| `md3-scales.json` | `dimensions_data.rs` | MD3 size, radius and type scales |
| `fluent-tokens.json` | `fluent_data.rs`, `dimensions_data.rs` | WinUI theme resources |
| `hig-tokens.json` | `hig_data.rs`, `dimensions_data.rs` | Apple HIG values; the standard material recipes are our derivation |

Each generator under `ui/crates/ui-design/data/` derives the repository root
from its own path, so it runs from any directory. Regenerating `hig_data`,
`fluent_data`, `md3_data` and `dimensions_data` reproduces the checked-in
files byte for byte. Several `spec/*_tokens.rs` predate their generators: the
checked-in file carries entries the current script does not emit, so reconcile
before overwriting.

## Reading discipline

- Name the invariant you took: "freeze items that reach a bound" is usable;
  "taffy has a `FlexLine` struct" is not.
- A framework's behaviour is evidence only for the question it answers.
  egui's scroll model says nothing about MD3's scrollbar thickness.
- When two authorities disagree, say which one wins and why in the subsystem
  page, not in a comment.

## See also

- [Architecture](../architecture.md): where a change belongs before you
  look outside.
- [Workflow](../development/workflow.md): how to check a claim
  before relying on it.
