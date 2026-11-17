# File Choosers

A file chooser is a request ("one PNG, starting in Pictures") and an answer ("this path, or
cancelled"). The framework has two implementations: the drawn picker (see
[File Picker](./file-picker.md)) and the platform's own dialog — `IFileDialog` on Windows,
`org.freedesktop.portal.FileChooser` on the desktops that expose it. Both speak the contract
in `ui-platform::chooser`; `ui_picker::Chooser` is the policy that picks one.

Nothing about a caller's request changes when the implementation changes. Filters, modes,
multi-selection and save names mean the same thing on both sides, and a request the native
dialog cannot honour is answered by the drawn picker (or refused with the feature's name)
rather than silently narrowed.

## Contract

`ChooserRequest` carries:

| Field | Meaning |
| --- | --- |
| `mode` | `OpenFile`, `OpenFiles`, `SelectFolder`, `SelectFolders`, `SaveFile`, `SaveFiles` |
| `start` | `Remembered(path)`, `Folder(path)`, `File(path)` — the last selects an existing item |
| `filters` | `FileTypeFilter { label, patterns, mime_types }`, patterns being the portable spelling |
| `default_filter` | which filter is selected when the dialog opens |
| `title`, `accept_label`, `cancel_label` | the caller's words; a provider without them falls back |
| `show_hidden`, `modal` | settings a platform may not be able to express |
| `extra_places`, `choices` | sidebar entries and extra controls (portal `choices`, `IFileDialogCustomize`) |
| `save` | name, names (SaveFiles), default extension, overwrite policy, strict types |
| `state_key` | a stable name for remembered geometry (Windows `SetClientGuid`) |

The answer is `ChooserSelection { paths, filter, choices }` or `ChooserError`. The error
distinguishes `Cancelled` from `Unsupported(feature)` — the latter is what makes a fallback
possible — from `Unavailable`/`Failed`. Paths are filesystem paths only; a provider that can
also name virtual shell items sets `FOS_FORCEFILESYSTEM` rather than widening the result.

Filters are advice. The portal says so in its specification, a Windows open dialog lets a
typed name through, and the drawn picker treats them as a view concern; a caller validates an
answer it cannot accept and reads `ChooserSelection::filter` to know what the user chose.

## Capabilities and the switch

`ChooserCapabilities` is a list of what a provider can actually do, indexed by mode plus the
request features. A request is given to the native provider only when `missing(&request)`
answers `None`; the first unmet feature drives either the drawn picker or
`ChooserError::Unsupported`.

`ChooserPreference` chooses the policy:

- `Auto` (default): native when it can honour the whole request, drawn otherwise.
- `NativeOnly`: a sandbox or a review that must see the desktop's dialog; unsupported is an
  error.
- `DrawnOnly`: the drawn picker on every platform.

`CRABSNAP_FILE_CHOOSER=native|drawn` overrides the policy for a run; it is a diagnostic
switch, not a configuration surface.

| Feature | Drawn picker | Win32 `IFileDialog` | XDG portal |
| --- | --- | --- | --- |
| `OpenFile` / `OpenFiles` | yes | yes | yes |
| `SelectFolder` | yes | yes (`FOS_PICKFOLDERS`) | yes (`directory`) |
| `SelectFolders` | yes | undocumented | no |
| `SaveFile` | yes | yes | yes |
| `SaveFiles` | yes | no | defined, not implemented here |
| glob filters | yes | yes | yes |
| MIME filters | yes (extension table) | no | yes |
| filters enforced | save with `strict_types` | save only (`FOS_STRICTFILETYPES`) | no |
| custom places | yes | yes (`AddPlace`) | no |
| extra choices | yes | no | yes |
| modeless | yes | no | yes (`modal: false`) |
| force hidden | yes | yes (`FOS_FORCESHOWHIDDEN`) | no |
| initial file | yes | yes | yes |
| default extension | yes | yes | no |
| overwrite prompt | yes (`Prompt`; `Forbid` refuses) | yes | desktop's own |
| remembered state | no | yes (`SetClientGuid`) | no |

## Host seam

A provider hangs off the surface it belongs to, because the surface owns the window the
dialog is parented to:

```rust
// SurfaceHandle
fn chooser_capabilities(&self) -> Option<ChooserCapabilities>;
fn choose(&mut self, id: ChooserId, request: ChooserRequest) -> Result<(), ChooserError>;
fn chooser_events(&mut self) -> Vec<(ChooserId, ChooserAnswer)>;
```

`MessageCtx::choose_native(parent, request, done)` validates the request, checks the
capabilities, records the callback and marks the parent's input held while the chooser
runs — the same rule as a modal dialog surface, but carried on the parent because a native
chooser is not a surface the host steps. `done` runs once on the loop thread. A parent that
closes first cancels the chooser; the callback still runs, exactly once, with `Cancelled`.

The callback gets no context. A drawn picker answers from its own `App`, where no message
door exists, so a callback that could open a surface would give the two implementations
different powers. Work that needs the loop belongs to the app's next turn, which reads the
answer from its own state.

A drawn dialog asks for a 360×240 logical floor (`ui_picker::MIN_SIZE`), which the spec carries
as `SurfaceSpec::with_min_size`; a native dialog brings its own sizing and ignores it. The
backend converts the client floor to its own terms: Win32 adds the frame at the window's DPI,
Wayland hands it to `xdg_toplevel::set_min_size` ([Window](./window.md)).

## Windows

`ui-platform-win32/src/chooser.rs` maps the request to `IFileOpenDialog`/`IFileSaveDialog`
and runs `Show` with the window's `HWND`. `Show` pumps its own message loop, so the caller's
turn blocks until the user answers; the presenter queues the answer and the host delivers it
on the next turn. COM is initialized per call in an STA, tolerating an apartment another
part of the program already opened. Cancellation is the `ERROR_CANCELLED` HRESULT; every
other failure keeps its context.

## Linux

`ui-platform-wayland/src/chooser.rs` calls the portal's `OpenFile`/`SaveFile`, watches the
request object's `Response` on a worker thread, and queues the answer for
`chooser_events`. The answer arrives on a D-Bus signal that the compositor connection knows
nothing about, so the worker bumps the backend's eventfd — the wake the hotkey reader and
the job worker already share. `parent_window` is empty for now: the portal documents it as
optional, and exporting our surface through `xdg-foreign` is a separate protocol addition.
The portal's `uris` are `file://` URIs; a remote authority is refused rather than mapped to
a path this process cannot open.

Filters are serialized in the interface's typed form, `a(sa(us))`: entry type 0 is a glob,
1 is a MIME type, and a filter that names neither becomes `*`. This is not optional — the
portal front-end validates the type and rejects the whole call with `InvalidArgument`
otherwise. A portal answer also carries `current_filter`; the provider maps its label back
to the request's filter index, which is what `ChooserSelection::filter` holds.

### Linux work queue

The provider is cross-compiled and checked from the Windows machine, but no dialog has run
on a Linux desktop. The following is what remains, in the order it should be done; each item
names what verifies it.

1. **Build and test natively.** `cargo check --workspace --all-targets`,
   `cargo build --workspace --tests --examples` (zero warnings), and
   `cargo test --workspace --features ui-text-fontique/system --lib` from `ui/`. The Windows
   cross-check cannot build the examples: the host stack pulls `fontconfig` through
   `pkg-config`, which has no Linux sysroot here.
2. **Run both implementations.** `cargo build --release -p ui-picker --example picker_demo`,
   then `target/release/examples/picker_demo` (drawn and native through
   `PICKER_DEMO_NATIVE=1`), and both `CRABSNAP_FILE_CHOOSER=native|drawn`. Walk every mode the
   demo takes — `PICKER_DEMO_MODE=open|open-files|folder|folders|save|save-files` — and the
   controls with `PICKER_DEMO_EXTRAS=1`: the type menu, the custom place, the select and
   checkbox choices, a save onto a taken name, and cancel. Each answer must reach the callback
   once, and the drawn fallback must produce the same `ChooserSelection` shape.
3. **Filters on the desktop.** With a filter list, confirm the dialog shows it and that the
   chosen entry comes back as a non-`None` `ChooserSelection::filter`. A call rejected with
   "expected type for key filters" means the typed encoding regressed. Test on GNOME and KDE
   at least; `current_filter` is advisory, so a desktop may ignore it.
4. **`parent_window` through `xdg-foreign`.** Bind `xdg_foreign` (v1) or
   `xdg_foreign_unstable_v2` in the Wayland backend, export the parent toplevel when the
   surface is created, pass `wayland:<handle>` as the portal's `parent_window`, and unexport
   when the surface dies. A compositor without the protocol keeps the empty string. Verify
   the dialog is transient and centred over its parent on GNOME and KDE.
5. **Discovery must not stall the loop.** `ensure_connection` runs a blocking session-bus
   connect and property read from `choose`, which the facade calls on the loop's thread.
   Measure the first call with the portal present and absent; if it delays a frame, move
   discovery to the worker thread or warm it before the request.
6. **The answer must wake the loop.** The portal's `Response` arrives while the app is idle;
   check the eventfd wake builds the frame that shows the answer without pointer movement.
7. **Declared work, not implemented.** `SaveFiles` has a portal spelling (`files`, `aay`) and
   the drawn picker now claims it; the portal provider does not, because it is unrun here.
   The request carries the names in `SaveOptions::names`, so enabling it is an option
   mapping and a `SaveFiles` method on the worker. The portal also wants absolute paths in
   `current_folder`/`current_file`, while the request accepts a relative start; decide
   whether to canonicalize or to state the caller's obligation.

## Verified behaviour

- `ui-platform::chooser::tests` — capability gating, each unmet feature naming itself,
  MIME hints not forcing a fallback, save requests without options.
- `ui_picker::chooser::tests` — the drawn provider's mode set.
- `ui_picker::model::tests` — the drawn variants: a multi folder mode marks and still
  navigates, `SaveFiles` joins the request's names, and `strict_types` refuses a name the
  filter rejects. `ui_picker::view::tests` — the type menu and a choice reach the answer, and
  an open never asks to replace a taken name.
- `ui-platform-win32::chooser::tests` — options per mode, filter joining, state GUID.
- The Win32 dialogs ran on the owner's machine: drawn and native through
  `PICKER_DEMO_NATIVE=1`, and both `CRABSNAP_FILE_CHOOSER` overrides. The run is manual, so
  its observations are the owner's report, not a recorded measurement.
- The portal provider was cross-checked with
  `cargo check -p ui-platform-wayland --target x86_64-unknown-linux-gnu --all-targets`; its
  dialogs are unrun here — see the work queue above.

## Corrections

**The portal was out of scope.** The picker page originally said a sandboxed app must use
the portal and that the framework would not grow a native dialog. Both are now one
implementation behind `ui-platform::chooser`; the drawn picker is no longer the only answer,
and the portal path is what makes the framework usable in a sandbox.

**The switch was going to be a compile-time backend choice.** It is per request instead.
Capabilities travel with the surface because a desktop can lose its portal, and an
application can ask for the drawn picker for a review without a rebuild.

**The completion callback was to receive a `MessageCtx`.** The drawn picker cannot supply
one — its answer comes from an `App`, outside any message door — so the callback receives
only the answer and the app acts on it in its next turn. One contract, one power.

**The portal filters were serialized in the old shape.** The first provider wrote each
filter entry as `(glob, mime)` strings (`a(sa(ss))`). `org.freedesktop.portal.FileChooser`
takes `a(sa(us))` — a type number and one string — and its front-end checks the type before
forwarding, so a request with filters is rejected with `InvalidArgument` on every desktop
rather than showing a dialog without them. The interface has used the typed form since at
least xdg-desktop-portal 1.8 (interface version 2), so there is one encoding, not a switch.
Verified against the interface XML and the front-end's `validate_filters`; the desktop run in
the Linux work queue is what still has to confirm the dialog shows the list.
