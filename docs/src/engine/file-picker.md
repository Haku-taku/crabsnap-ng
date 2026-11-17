# File Picker

A file picker lets an app choose a path without the app listing directories itself: the
framework draws the dialog, and a platform service answers what a directory holds and what
places exist. This page fixes that split and the drawn implementation; the contract that
lets an app switch between this picker and the platform's own dialog is
[File Choosers](./file-choosers.md).

`ui-platform` answers the listing and the places, `ui-picker` draws the dialog, and the
`picker_demo` example opens it on a dialog surface with the in-window modal as its fallback.

## Problem

The picker is the reason the thumbnail and icon layers exist. It must answer, per row,
before any decode finishes: what kind of item this is, what picture fills the slot, and
whether a thumbnail may replace that picture. The row itself is cheap; what costs design is
everything around it:

- listing a directory without blocking the loop, and discarding an answer that arrived after
  the user navigated away;
- ordering (directories first, natural name order) and filtering, which are view concerns
  the lister must not bake in;
- places — home, the XDG user directories — and mounted volumes, with bookmarks and recent
  files later;
- open, open-many, select-folder, select-folders, save and save-many modes, which differ only
  in the naming field, the filters and what the accept button returns;
- keyboard, focus and internationalisation, which every dialog in the framework already has
  to respect.

This is a **framework-drawn** picker for apps that may read the filesystem directly. A
sandboxed app can ask for the desktop's portal dialog instead through
[`ChooserPreference::NativeOnly`](./file-choosers.md); the two implementations share one
request and one result, so moving between them changes no caller code.

## Comparison

| Framework | Listing | Ordering / filtering | Places | Notes |
| --- | --- | --- | --- | --- |
| GTK 4 `GtkFileChooserWidget` | `GtkFileSystemModel` enumerates asynchronously through GIO, in batches, with a `GCancellable` owned by the model | `GtkSortListModel` sorts downstream, directories first; `GtkFileFilter` hides entries and folders bypass it by default | `GtkPlacesSidebar` from bookmarks, XDG directories, mounts and recent | filtering and sorting never touch the model's storage; a cancelled enumeration emits no `finished-loading` |
| KDE `KFileWidget` / Dolphin | `KDirOperator` over `KCoreDirLister`, which runs a KIO job; `KFileItemModel` owns the items | natural sort, directories first, hidden last; name and MIME filters; grouping by role | `KFilePlacesModel` from bookmarks, Solar devices, tags | `KFileItemModelRolesUpdater` resolves expensive roles and previews for the **visible range first** |
| Windows `IFileOpenDialog` | the shell namespace; the dialog browses it, not the app | `SetFileTypes` with `COMDLG_FILTERSPEC` patterns; folder mode is `FOS_PICKFOLDERS` | the shell's own places (Quick Access, This PC, drives) | customisation is `IFileDialogCustomize`; `GetResults` returns shell items |
| macOS `NSOpenPanel` | the panel browses through Launch Services | `allowedContentTypes`, `allowsMultipleSelection`, `canChooseDirectories` | the panel's own sidebar | presented modally or as a sheet; a delegate can validate a path as it is typed |
| XDG portal `FileChooser` v4 | the portal backend lists; the app asks with `OpenFile`/`SaveFile` | `filters` as `(label, [(0=glob, 1=mime)])`; filters are advisory | the backend's | sandboxed apps only: it needs `parent_window` and cannot be embedded |

> **Sources.** GTK 4 `gtk/gtkfilechooserwidget.c`, `gtk/gtkfilesystemmodel.c`,
> `gtk/gtkplacessidebar.c`. KIO `src/widgets/kfilewidget.cpp`, `src/core/kcoredirlister.cpp`;
> Dolphin `src/kitemviews/kfileitemmodel.cpp` and `kfileitemmodelrolesupdater.cpp`. Win32
> `IFileDialog::SetFileTypes`, `IFileOpenDialog`, `FOS_PICKFOLDERS` (Microsoft Learn). AppKit
> `NSOpenPanel` / `NSSavePanel` (`allowedContentTypes`, `canChooseDirectories`). Portal
> `org.freedesktop.portal.FileChooser` documentation, version 4.

What all of them share, and what this design keeps:

1. **Listing is asynchronous and cancellable.** A superseded answer is dropped, not merged.
2. **Ordering is the view's.** The lister answers what the directory holds; the model sorts
   and filters, so a filter change costs no disk read.
3. **Directories survive a file filter.** Only an explicit folder-only mode changes that.
4. **The cost of a preview follows the viewport.** Dolphin resolves the visible range first;
   the picker should ask for thumbnails the same way.
5. **The mode changes little.** Save adds a name field and an overwrite question; it does not
   need a second dialog.

## Contract

### Layering

| Piece | Home | Why |
| --- | --- | --- |
| `Entry`, `ListingRequest`, `Place`, `Device`, and the `Listings`/places services | `ui-platform` | they describe what the platform answers, and the row and the request travel together |
| `SearchRequest`, `Searches`, the `DirectoryWatcher`/`WatchGuard` traits and the `Watch` handle | `ui-platform` | the request and the answer are portable; only the walk's platform rules and the native watch are not |
| `ChooserRequest`, `ChooserSelection`, `ChooserCapabilities` | `ui-platform::chooser` | the portable contract; a caller configures one request whichever implementation answers |
| `FileTypeFilter`, `ThumbnailPolicy`, model, `Picker`, `PickerState` and `Chooser` | new `ui-picker` crate (portable) | depends on `ui-widgets` and `ui-platform`, depends on no backend |
| native chooser | `SurfaceHandle::chooser_capabilities`/`choose`/`chooser_events`, implemented by the backend | the dialog must be parented to the window that asked for it |
| listing, search, watch and places implementation | `Backend::file_listing()` / `Backend::file_search()` / `Backend::file_watch()` / `Backend::places()` / `Backend::devices()`, like `thumbnails()` | hidden-file rules, `user-dirs.dirs`, the mount table and the native change notification are the desktop's |
| `Volumes`, `VolumesState`, `VolumeMounter` and the "first free letter" rule | `ui-platform` | the state machine, the sidebar contract and the drive-letter search are portable; only the enumeration and the mount call are not |
| the volume enumeration and mount call | `Backend::volume_mounter()`, implemented by `ui-platform-win32` | `FindFirstVolumeW`, `GetVolumePathNamesForVolumeNameW` and `SetVolumeMountPointW` are the shell's |
| Linux implementation | `ui-platform-wayland` | reads real directories and `user-dirs.dirs`, walks with the dotfile rule, watches with inotify |
| Windows implementation | `ui-platform-win32` | the shell's known folders and image list; directories through the shared `std::fs` lister with the attribute rule; `ReadDirectoryChangesW` for changes |
| the dialog window | `ui-picker`'s `Picker` through `Host::open_dialog` | the window is the engine's dialog-surface capability, not a picker concern |

The picker never lists a directory itself. That keeps the portable crate free of filesystem
policy and lets Windows answer with reparse points and drives while Linux answers with
dotfiles and mounts. A typed path is the one thing it resolves on its own: a `metadata` call
tells a file from a folder and nothing visible is built from it. `Entry` and `Place` sit
beside the services that produce them because a value type whose fields are the platform's
answer belongs to the platform, not to one of its consumers: the row, the listing cache and
the thumbnail version all read the same record.

### Entries

```rust
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: FileKind,
    pub directory: bool,
    /// A symbolic link or a reparse point: the type follows the target, the row gets a badge.
    pub link: bool,
    pub len: u64,
    pub modified: Option<Duration>,
    pub hidden: bool,
}
```

`kind` comes from the path's name, as in `files.rs`; a directory is `Folder` unless the
platform can tell it is a mount point. The listing carries `len` and `modified` because the
thumbnail request needs them as its version, and the row's size column shows `len`.
`Entry::version()` is that pair, so a rewritten file is a new thumbnail request.

The type lives in `ui-platform` (`ui_platform::listing::Entry`), not in `ui-picker`: the
listing cache hands out `Arc<Vec<Entry>>`, and a consumer-side copy would have to be rebuilt
on every answer.

### Listing service

```rust
pub struct ListingRequest { pub path: PathBuf, pub show_hidden: bool }
pub trait DirectoryLister: Send + Sync {
    fn entries(&self, request: &ListingRequest) -> Result<Vec<Entry>, String>;
}

pub struct Listings;                       // the portable handle, cloned per surface
pub enum ListingState {
    Loading,
    Ready(Arc<Vec<Entry>>),
    Failed(Arc<str>),
}
```

The provider is synchronous and the portable handle runs it on its own worker, the same
shape as `Thumbnails`: the picker asks with `Listings::get(&request)`, the model keeps its
previous rows, and the answer arrives as a completion the host turns into a frame. The
listing answers the entries in the order the platform returned them; ordering and filtering
happen above it.

The walk itself is the same `std::fs` read on both desktops, so it is one portable type,
`FsDirectoryLister` in `ui-platform/src/listing.rs`, and the platform supplies only its
hidden rule: the dot prefix on unix, the dot prefix or a hidden/system attribute on Windows.
That keeps the two backends from drifting over link handling, the modified time or the kind,
which are not platform decisions.

Superseded answers are dropped structurally rather than by a generation counter: the model
holds the current `ListingRequest` and only ever asks `Listings` about that request, so an
answer to a directory the user has already left is never the answer to the question being
asked. A destination is invalidated before it is read, so a picker opened twice does not
show yesterday's files, while the directory being left keeps its cached answer for a quick
step back. Killing a `readdir` mid-directory is an optimisation KIO and GIO make and this
design does not need: the worker reads one directory at a time and a late answer lands in
the cache for a request nobody asks for again. The cache is bounded both ways: at most 32
directories, and none kept past the idle window, which is the same rule the thumbnail and
icon caches follow because what a closed picker read is the app's memory, not the run's.
`PickerState` claims each service it was handed (`Listings`, `Thumbnails`, `FileIcons`) for
the dialog's lifetime, so the idle sweep only applies once the dialog is gone: an open
dialog that merely sits untouched keeps its listing and its icons instead of re-reading and
re-querying them on the first frame after.

Batches are deferred. GTK adds entries as they arrive so a huge directory paints early; the
prototype answers once, and a directory of 100k entries is one completion of a few hundred
milliseconds rather than a stream. The model is a vector, so adding batches later does not
change its interface.

### Places and devices

```rust
pub struct Place { pub kind: PlaceKind, pub path: PathBuf }
pub struct Device { pub path: PathBuf, pub label: Option<String> }
```

`Place` is a fixed vocabulary — home, desktop, documents, … — so the picker draws the label
from its own catalogue. `Device` is not: a volume carries the name its desktop gives it, and
`None` is a volume with no name of its own, which the picker names ("Root").

`Backend::places()` and `Backend::devices()` are read once when the host is built; a mount
table is a small synchronous read, like `user-dirs.dirs`. The picker draws both in one
scrolling sidebar, with a "Devices" heading between them, and a click navigates to the
volume's mount point.

The selection is Dolphin's rather than `mount`'s: a volume is a block device the user can
browse, not every mount point the kernel holds. `ui-platform` owns the rule as the pure
`places::linux_mounts`, fed `/proc/self/mountinfo` by the Wayland backend, because one
text-parsing question deserves one answer and a test that needs no machine that mounts
anything:

- a source outside `/dev/` is not a volume, which drops tmpfs, proc, overlay, gvfs and
  network shares;
- a `/dev/loop*` source is a snap package or a mounted image;
- a device mounted more than once — a bind mount, a container's view, a filesystem at two
  mount points — is one row, at its shallowest mount point, and a tie between two mount points
  of equal depth goes to the row the mount table lists first;
- the label comes from udev's `/dev/disk/by-label` links, else from the mount point's name.

Windows enumerates logical drives (`GetLogicalDrives`) and keeps the fixed, removable,
network and optical types, dropping a letter whose volume query fails, so an empty optical
drive or a disconnected share is not a row that cannot open. Its label is the volume name
with the letter — `Windows (C:)` — because two disks may share a name and the letter is what
a Windows user navigates by.

Unmounted removable media is the sidebar's second list. `Volumes` asks the backend's
`VolumeMounter` for the volumes that exist and no mount point claims, on its own worker;
`Win32Volumes` finds them through `FindFirstVolumeW` and prices a volume as unmounted when
`GetVolumePathNamesForVolumeNameW` fails with `ERROR_MORE_DATA`, listing only `DRIVE_REMOVABLE`
ones. A click calls `Volumes::mount`, and the worker mounts at the first free letter `D:`
through `Z:` with `SetVolumeMountPointW`, then answers the mount point as a completion the
picker turns into navigation; the failure reaches the footer as one localized line, because
the sidebar row has nowhere else to say it. The list is refreshed after every mount, so the
volume leaves the section it was clicked in. The portable half — the state machine, the
letter search, the outcomes queue — is in `ui-platform`; `Backend::volume_mounter` is the
platform half, `None` on a backend with no such call, and a picker handed no `Volumes` draws
no section.

### Ordering and filtering

- **Order**: directories first, then a natural case-insensitive name order — a run of ASCII
  digits compares as a number, everything else by lowercased bytes. Locale collation
  (`g_utf8_collate_key_for_filename` in GTK) is deferred; the deviation is visible only for
  names that differ by case or by a non-ASCII character at the same position.
- **Filters**: a `FileTypeFilter { label, patterns, mime_types }`, matching a name
  case-insensitively against glob patterns or its extension's MIME type against the MIME
  patterns (`image/*` included). Directories bypass a file filter unless the mode selects
  folders, the GTK default. A filter change costs no disk read: the model re-filters the
  rows it holds.
- **Hidden**: dotfiles are listed only when the toggle is on; the `.hidden` file KDE and
  GTK consult is deferred.

### Thumbnails and icons

The row composes the two existing layers: a thumbnail when it is `Ready`, otherwise the
platform type icon, otherwise the glyph from `ui_widgets::file_icon`. Both platform answers
arrive on a worker, so a `Loading` thumbnail or icon draws the layer under it — which is why
a fast scroll through a large directory costs row building, not shell queries. The policy is
the picker's, not the module's:

```rust
pub struct ThumbnailPolicy { pub show: bool, pub max_file_size: u64 }
```

It mirrors Nautilus' `show-image-thumbnails` (`always` / `local-only` / `never`) plus a size
limit. A row outside the policy sends no request, so a picker with previews off costs
nothing. The default asks at 64 MiB. The requested size follows the mark it fills: 20 pixels
in a row, the grid's 80-pixel cell picture. `local_only` is not in the type yet: it needs the
filesystem type of the row's own directory, which the list of mounted volumes a sidebar
shows does not answer, and a variant that always answered `local-only` for a network path
would be a lie with nothing behind it. Requests are made in viewport order and only for
visible rows, which is Dolphin's visible-range rule.

### Interaction

- `Up`/`Down`/`Home`/`End` move the selection, `Enter` activates (a directory navigates, a
  file accepts), `Backspace` goes up, `Escape` cancels. `Up`/`Down` move one screen row, which
  in the grid is a whole row of cells; `Left`/`Right` move one cell there and move nothing in
  the list, which has one column. The arrow bindings allow Shift and Ctrl:
  Shift grows the marks from the anchor and Ctrl moves the cursor without touching them, as
  Explorer and Dolphin do; a grid's Shift range is the rectangle between the anchor and the
  cursor. `Ctrl+A` marks every row the mode accepts. The bindings are app-level `ShortcutId`s
  the surface installs, so a host that reroutes keys can offer them elsewhere. While the name
  field has focus the engine gives it the plain arrows, Shift+arrows, letters and `Ctrl+A`
  first, so typing a name never marks rows or jumps the list.
- A letter moves to the next name whose first letter it is, starting after the cursor, and
  wraps at the end: pressing `r` again advances past the name it is already on, and a letter
  that matches nothing leaves the selection alone. Shift is allowed, so a capital finds the
  same names. The binding is per letter — one `ShortcutId` for each of the 26 — because a
  chord's id is a `&'static str` and the table has no "any character" wildcard; the press
  itself names which letter fired. A menu or a modal question swallows the letter instead of
  letting it move the list behind the overlay.
- Focus stays inside the dialog while it is open; the accept button is reachable by `Tab`,
  and `Escape` is never the only way out.
- A single click selects; a double click — or `Enter`, or the accept button — opens. A folder
  opens by navigating into it; a file opens by accepting in the modes that open one, and in
  `SaveFile` a double click on a file fills the name field with its name. The count comes from
  the engine's per-button burst, on the desktop's double-click time and rectangle, and reaches
  the row as `RowGesture::trigger`; a drag never carries a click count, so marking a range of
  rows cannot open one. A single click on an already-selected row no longer activates: without
  a second event a hesitant click and a double click's first half are the same press, and the
  engine now says which is which.
- The marks, the pointer's hover wash and the keyboard's cursor are painted independently.
  A marked row still answers the pointer, because the wash composites over the selection
  fill rather than replacing it, and the cursor draws a `FocusRing` stroke whether or not
  its row is marked. In a multi mode that is what makes keyboard navigation legible: the
  arrows move the cursor, which may sit between or beside marked rows, and `Ctrl+Arrow`
  browses with the marks untouched.
- A press that travels past the engine's click threshold is a selection drag: the row it
  began on is the anchor, the row under the pointer is computed from the pointer's own
  coordinates in the anchor row, and the marks follow the swept range while the pointer
  moves. In the grid the coordinates are divided by the cell and row pitches, and the swept
  range is the rectangle between the two cells. A plain drag replaces the marks; a Ctrl drag
  adds the swept cells to the marks the gesture began with. The rows and cells declare
  `Sense::DRAG`, which is also what tells the list not to pan under a drag.
- The two halves of a crumb capsule are two targets: the label jumps to that ancestor, the
  chevron opens its subfolders. A menu row navigates and closes the menu; `Escape` or a press
  outside closes the menu alone, without closing the dialog.
- The footer's type menu lists the request's filters and the check names the active one. A
  toggle in the footer's right group unfolds the band above it; the band holds the request's
  extra controls: a select opens a menu of its options, a boolean is a checkbox whose click
  is the whole interaction. A caller's extra places follow the platform's in the sidebar.
- Every user-visible string is a Fluent message; the picker ships its own catalog rather
  than borrowing the app's.

### Modes

| Mode | Accepts | Extra |
| --- | --- | --- |
| `OpenFile` | one file | filters |
| `OpenFiles` | several files | filters, multi-select |
| `SelectFolder` | one directory | directories are the rows that accept |
| `SelectFolders` | several directories | multi-select over folders |
| `SaveFile` | a name in the current directory | name field, overwrite question |
| `SaveFiles` | the request's names in the current directory | name-list caption, overwrite question |

All six are drawn. A multi mode shapes a click by the modifiers it was made with: a plain
click replaces the marks with the accepted row, `Ctrl` toggles the row it hits, and `Shift`
extends the marks from the anchor. The engine reports the modifiers with the row's gesture,
so the picker's row control reports its own `RowGesture` rather than `Change::Activated`,
which carries none. A plain click on a row the mode cannot answer leaves the assembled marks
alone — it is a step of browsing, not an answer. `Space` marks the cursor's row. In
`SelectFolders` a double click on a folder enters it, which drops the marks: browsing past an
assembled answer has to stay possible. A click on a file in that mode marks nothing, because a
file cannot be the answer.

`SelectFolder` accepts the folder on screen, as native folder pickers do; a double click on a
subfolder enters it, and a single click selects it without answering. Save adds the name field
to the footer, and an empty or path-bearing name disables accept. `SaveFiles` shows how many
names the request will write instead of a field: the folder is the only thing the user
chooses, and the answer is that folder joined with each name. A save's `strict_types` makes
accept refuse a name the active filter does not match, and `OverwritePolicy::Forbid` refuses
one that already exists. `Prompt` asks first: accepting a taken name opens the Replace
question as a modal over the dialog, and only its Replace answer hands the paths to the
caller. No open mode asks it — the policy belongs to a
save, and an open of a taken name is the answer the caller asked for.

The contract's `ChooserRequest` is what a caller builds; `PickerOptions` is the drawn
implementation's own configuration (listing source, pack, places, thumbnails). `Chooser`
maps one onto the other and falls back according to the provider's capabilities.

## UI and layout

### Form

The picker is a **dialog window owned by the surface that opened it**, with the in-window
modal as its fallback. Both present the same content; which one is used is the host's
answer, not a branch in the picker's build.

The host path asks for a dialog of the opening surface and gets back a surface id:

```rust
let options = PickerOptions::new(listings.clone(), start, ui_widgets_md3::pack_of, env)
    .places(places.to_vec())
    .devices(devices.to_vec())
    .thumbnails(thumbnails)
    .file_icons(file_icons);
let picker = Picker::new(options, move |outcome| {
    // PickerOutcome::Selected(path) | PickerOutcome::Cancelled
});
ctx.open_dialog(
    parent,
    SurfaceSpec::dialog(APP_ID, title, (900, 600)).with_modal(true),
    Box::new(picker),
    OpenOptions {
        shortcuts: Picker::shortcuts(),
        ..OpenOptions::default()
    },
)
```

The pack argument is a family, not one resolved snapshot: the caller passes the kit's
`pack_of`, and the picker takes the appearance from the `StyleScope` on the `env` it is
given, exactly as a page takes it from the frame's root env. A dialog is its own surface
and cannot read its opener's env, so the opening app states the appearance once when it
builds the options; an unscoped env means light. The demo follows the system preference
through `ThemeWatch` and offers a light/dark switch, because both are worth looking at in
each pack.

The bindings are the caller's to install: a surface that opens the picker without
`Picker::shortcuts()` gets a dialog whose mouse works and whose `Enter` and `Escape` do
nothing. They are not a default the picker can set for itself — the engine installs no
bindings on an app's behalf — so the demo's two `OpenOptions` literals are the reference a
caller copies.

The dialog stacks above the parent, holds its input while open, and closes with it; those
rules are [Dialog Surfaces](./dialogs.md)'s contract, and the host enforces them itself
because Wayland's `xdg-dialog` modality is a hint. The engine inside the dialog owns its
focus, its keyboard and its `Enter`/`Escape` routing, which is what a picker needs.

When `open_dialog` refuses — a layer surface cannot own a dialog on Wayland, and a
presentation may have no dialog surfaces at all — the same content is drawn as an
in-window modal: it declares `Decl::modal`, draws a scrim over the window and traps focus,
which is what the MD3, Fluent and Glass dialogs already do with `ModalTarget`. The engine
resolves a modal's rect after layout, moves focus into the topmost `modal_root()` when it
opens, and keeps the keyboard inside that subtree, so the focus trap and `Enter`/`Escape`
routing are the engine's, not a platform call.

The two other surface shapes are set aside for reasons the engine states:

- **Not a popup.** `PopupSpec` is the presentation of a `Decl::overlay` panel — a menu, a
  tooltip, a picker popover — anchored to another node because it must escape its window's
  clip. The Wayland backend turns it into an `xdg_popup` against the parent surface, and the
  Win32 backend into an owned popup window; both are dismissed by a press outside and carry
  no modality. A picker is anchored to nothing, covers the window and must not vanish on a
  stray click: that is a modal, not an overlay.
- **Not a sheet.** macOS presents a modal panel as a sheet attached to its window's title
  bar. This framework has no sheet role, so the dialog window is the form on every platform
  it runs on.

The picker asks for 900×600 logical pixels. As a dialog that is the opening size, which the
compositor may answer with its own; as an in-window modal the app measures it inside an
inset of its own (the demo leaves 48 px a side). Either way the layout shrinks it on a
smaller surface rather than letting it overflow, and the rows area scrolls what the height
cannot hold. Below 720 px the sidebar hides. The picker animates its hover states with the
pack's state motion and nothing else: opening and closing are the surface's, and a host that
wants a dialog's enter/leave transition puts it on the window, as the modal dialogs do.

### Layout

```
┌──────────────────────────────────────────────────────┐
│  Home › Documents › Drafts                    ⋯ hidden│  toolbar
├──────────────────────────────────────────────────────┤
│  Places   │  Name              Size      Modified    │  header row
│  Home     │  ▣ figures         —         today       │
│  Documents│  ▣ notes.md        4 KB      yesterday   │  rows (RowWindow)
│  Downloads│  ▣ report.pdf      2 MB      2 Mar       │
│  Pictures │  …                                       │
├───────────┴──────────────────────────────────────────┤
│  All files ▾                          Cancel   Open  │  footer
└──────────────────────────────────────────────────────┘
```

| Band | Content | v0 |
| --- | --- | --- |
| toolbar | one address slot (breadcrumb, typed path or search field), a search button, a list/grid toggle, a hidden-files toggle | yes |
| sidebar | places, mounted volumes and mountable removable media, 180 px, hidden below 720 px wide, scrolling | home and XDG user directories; Windows drives and Linux mount points; the caller's extra places |
| header | Name, Size, Modified, right-aligned figures; scrolls sideways and carries the rows' horizontal offset | yes |
| rows | `row_height` (32 by default), 20 px icon slot, selection fill; or the grid's cells, an 80 px picture over a two-line name | details list and grid; the marks, the pointer's wash and the keyboard's focus ring are three layers, and engine focus stays on the dialog's buttons |
| choices | the request's extra controls, 44 px, scrolling sideways, unfolded by the footer's toggle | select menus and checkboxes |
| footer | left half: the save name field or the every-name status; right group: the options toggle, the type menu, Cancel and the accept button | every request filter switchable |

The footer is two groups. The left group grows, so a field takes all the room a dialog has
instead of sharing it with the controls; the right group hugs — the filter menu and both
buttons move as one block against the trailing edge. A save's name field takes focus when the
dialog opens, as a platform save dialog's does, and draws the engine's insertion caret,
selection and composition from `caret_for`/`selection_for`/`preedit_for`; a field that only
declared `text_input` would edit a buffer with no visible insertion point.

Save mode puts the editable name field in the footer's left group; the type menu stays in the
right group when the request carries filters. It is a container with the engine's
`text_input` buffer and one `ui_widgets::text::LineEdit` line: the engine owns the caret,
selection, composition and editing keys and reports the whole buffer as `Change::Text` on the
field's id, and the model keeps it until accept joins it to the folder on screen. The shared
line does what a hand-painted `Text` did not — it scrolls a value longer than the field under
the caret, draws a placeholder, and the field carries the text cursor — and it asks the engine
for the caret and selection rather than tracking an offset. The same line draws the path and
search fields, so all three behave alike. `SaveFiles` replaces the field with a caption naming
the count, because its names come from the request.

The name column has a floor of 60 units. A panel narrower than the three columns is not
allowed to squeeze a name below it: the header and the rows are declared at the content
width (the columns at their floors), and the header is one horizontal `Scroll` whose
viewport is the panel's width. The header owns the horizontal offset: the rows are a
vertical-only scroller and each row is displaced by the header's `offset_x`, so the header
labels and the columns under them move together while the list itself cannot be dragged
sideways. Dragging the rows sweeps the selection instead.

The header's band is set apart by a one-pixel rule inset from both ends and faded against the
pack's `Divider` role — fainter than the toolbar's full-width rule — and the boundaries
between Name, Size and Modified carry short vertical rules that meet neither the header's
top nor its bottom. Those short rules sit in a `Stack` over the header row at the columns'
computed `x`, so they scroll with the labels rather than staying pinned.

The two rules — under the toolbar and between sidebar and panel — are one-pixel boxes filled
with the pack's `Divider` role, declared as the body's own children rather than as borders on
either region, because the border shape strokes all four sides. The body's height subtracts
both rules, so the column's fixed bands add up to the dialog's height exactly.

Every scrolling region — the rows, the grid, the header, the sidebar, the crumb trail, the
choices band and both menus — carries its inset as padding on the scrolled content, never as a margin
or a padding around the scroller. The viewport then reaches the region's edge, so the
scrollbar and the cut sit over empty padding instead of directly on a column's last
character. The header's and the rows' insets are equal, so a row's text starts exactly under
its label whatever the horizontal offset.

The rows are the picker's own declarations inside a `RowWindow`, not `List`: that widget is a
column of one-line strings, and a file row carries an icon slot, a name that may wrap, two
right-aligned columns and a selected state. Each row's declaration is wrapped in the picker's
`RowControl`, an element because a declaration alone has nowhere to report a click and because
the report is a `RowGesture` carrying the click's modifiers and, during a drag, the row under
the pointer. Row height is a parameter with a default, the way `List::row_height` is, because
a details row is denser in WinUI than in MD3; insets, radii, borders and elevation come from
the pack's `metrics.surface` / `metrics.overlay`, so the picker names no colour, radius or
duration of its own.

Hidden entries are drawn dimmed rather than suppressed when the toggle shows them: the name,
the thumbnail or platform icon and the fallback glyph take `HIDDEN_OPACITY` (0.55), while
the row keeps its full-size mark and its columns. That is what Dolphin and every platform
dialog do with a shown dotfile, and it keeps the columns aligned — a hidden entry that
dropped its mark would move its name out from under the header. The dim is paint only: a
hidden row is as clickable and as markable as any other, because "listed because you asked"
is not the same as "disabled".

A name too long for its column is cut with `…`, and its full text belongs to the pointer:
after 600 ms over a row or a cell whose label `Text::truncated_in` reports cut, the picker
draws the whole name in a small panel above the label. The panel is `Presentation::Parent`,
not a surface of its own, because a popup would take the pointer capture from the row it
describes and end the hover that keeps it up. The delay is the row's own `cx.age`, forgotten
when the pointer leaves, so a second visit waits again; and nothing shows it while a menu, a
question, a reading window or a drag stands over the list. The list and the grid share the
rule and the panel, so a cut name behaves the same in either geometry.

The grid is the same entries in another geometry, chosen by the toolbar toggle whose glyph
names the view the click reaches. Its cells are a square 80-unit picture over a two-line
centred name; the column count is the panel's width divided by a cell and its gap, and the
cells share the remainder so the last column meets the edge. There is no header: a grid has
no named columns to label. The scroller measures its window in grid rows rather than entries,
so a thousand files still build only the rows on screen. A cell is a `GridControl` reporting
the same `RowGesture`, and it stands on its own node id rather than the list row's: the list
learns a row's height from the rect the row id left in the previous frame, and a cell's rect
must never be read as a row height. During a drag the pointer's offset from the pressed cell is divided by
the cell and row pitches to name the cell under it. The model learns the shape, not the
pixels: `RangeShape::Grid { columns }` makes a Shift extension or a drag mark the rectangle
between the two corners, where `RangeShape::Column` marks the linear range between them.
`Up`/`Down` step one screen row — a whole row of cells in the grid — and `Left`/`Right` step
one cell, each carrying the same Shift and Ctrl meanings as in the list. The two views keep
separate scrollers, keyed by directory like the list's, so switching views returns to the
offset the user left in that view instead of translating an offset between geometries.

The choices band is declared at its animated height and clipped, with its content aligned to
the band's bottom: opening grows the band upward from the footer and the clip reveals it as
the top edge rises, while the body gives back the band's height so nothing below moves.
Closing runs the same thread backward. The animation is a declared `animate` on the band's id
with the pack's `Standard` motion; the band's group opacity follows the same value, so the
content fades rather than being cut off at a hard line. A dialog with no extra controls
declares no toggle and no band.

The breadcrumb follows Explorer's: each segment is a capsule of its label and a chevron, and
the chevron opens a menu of that folder's subfolders. The whole trail sits inside one rounded
field filled with the pack's `Surface`, inset two units from capsules 24 units high, so the
toolbar has a single filled region and the trail reads as a compact address field rather than
a row of buttons; the field's radius comes from the pack, the capsules' half of their height.
A segment carries the chevron when it is not the last one — the segment after it proves a
child exists — or when the current folder's listing already names a subfolder, so a visible
chevron and a non-empty menu cannot disagree. The chevron points down while its menu is open;
the engine paints no rotated glyph, so the pair is Material's `chevron_right` and `expand_more`
rather than one icon turned.

A capsule hovers in two levels, the way Explorer's does. The label and the chevron are two
targets — clicking the name navigates and clicking the chevron opens the menu — so each half
fills on its own hover. The capsule under them fills faintly whenever the pointer is anywhere
inside it, and says which crumb the pointer is in while the half says which action a click
takes. The engine has no rounded clip, so the halves carry only their outer corners and the
capsule carries the pill: the union of the three fills reads as one rounded crumb, and no half
can paint a corner inside it. Both fades read `SurfaceHover` and run the pack's
`StateChange` token, so Fluent's 167 ms and MD3's own timing stay the packs' decisions.

The menu asks for `Presentation::Platform`: Explorer's subfolder menu may leave the dialog's
window, and a dialog near the top of the output would otherwise cut a long list at its own
edge. A loop without a host to present the panel draws it in the window instead, so the menu
is the picker's first user of that path rather than of a requirement on its embedder.

### Questions and reading windows

A window the picker opens is one of two kinds, and the kind decides its form:

- **A confirmation is an in-dialog modal.** The save's Replace question, the conflict
  question, the permanent-delete and undo-copy confirmations, and the Move-to/Copy-to
  chooser are drawn in the dialog's own surface: a node declared
  `Decl::modal(ModalTarget::Global)` covers the window with the pack's `Scrim` and takes
  every press, and two zero-basis `grow_y` growers centre the raised surface, because a
  column has no main-axis position. `Enter` answers with the question's default — Replace,
  Delete, Undo, or the chooser's "here" — and `Escape` cancels, both through the picker's
  `OPEN` and `CANCEL` bindings. While one is open, every other list key is dropped: a key
  that is not an answer cannot move the list behind the scrim. A focused button still
  answers itself, because keyboard activation is the engine's and resolves before the app's
  binding.
- **A reading window is a window, not a panel above the dialog.** Properties and the
  failure list declare `Presentation::Window` and name themselves with
  `Decl::window_title`; the platform opens a modeless window owned by the dialog — an owned
  dialog window on Win32, a floating `xdg_toplevel` parented to the dialog on Wayland — so
  the compositor floats it rather than tiling it, and a press outside neither dismisses it
  nor blocks the list behind it. They report and do not question, so they take no scrim.
  `Enter` and `Escape` both dismiss them — a reading window has nothing to confirm, so the
  next `Enter` is the dialog's own answer — and its close box dismisses it the same way the
  footer's Close button does. The failure list caps its height and scrolls, so a job that
  failed on a hundred names cannot grow a window past the output. A platform that cannot
  open windows shows the same panel as a popup under the toolbar instead, and one that
  refuses that too paints it in the dialog, so a report never disappears.

  The body draws no card: the pack's raised surface fills the client area, and the border,
  rounded corners and shadow that enclosed the question panels are gone, because a card
  inside a framed window draws a second frame. The window is the panel's size and offers no
  resize handle, so a user cannot drag it to a size the panel does not fill; a change of
  content is what makes the window grow or shrink.

The footer menus, the crumb submenus and the right-drag menu stay anchored popups: they are
menus, and the press that opens one names the row or point it belongs to.

### Typed paths, search and watching

The toolbar has one address slot with three tenants, and exactly one is drawn: the breadcrumb
trail, the path field after `Ctrl+L`, the search field after `Ctrl+F`. The search button takes
the slot for the field and gives it back on a second click, and both fields dismiss with
`Escape` without answering the dialog. This is the slot a native picker reserves for its own
search box; showing a third band would have cost the rows their height for a control that is
empty most of the time.

A typed path is resolved against the directory on screen: relative input is joined to it and
`.`/`..` fold in the picker rather than being handed to the filesystem to interpret. `Enter`
navigates and `Escape` returns to the trail, and a press anywhere but the field ends the edit
the same way `Escape` does — without committing — which is how a native address bar lets go of
a half-typed path. The press that opens the field is the one press it ignores. A crumb that
names an ancestor jumps to it; the crumb naming the folder already on screen has nowhere to
go, and the empty space after the last crumb is no target either — a click on either turns the
trail into the field, the way Explorer's address bar becomes editable. A path that cannot be
read leaves the field open and names the failure in the footer, where the type label sits, so
the retry is the typed text itself rather than a dialog on top of a dialog.

Search is recursive under the directory on screen: case-insensitive name containment, the same
hidden rule as the listing, no descent through links, and canonical paths guard against a
junction loop. Matches stream into the rows in batches of 64, capped at 2000, so a wide tree
shows its first hits long before the walk ends; a newer query cancels the walk in progress by
generation, and only its answer may reach the rows. A row found below the root shows its path
relative to the root, since a bare name would not say where it is. Navigating leaves the search
— the trail returns and the rows are the new directory — while `SelectFolder` accepts nothing
until the search ends, because a folder found somewhere below is not the folder the caller
asked to select. `Escape` is scoped in the order overlays, path field, search, dialog, so one
key never discards two layers.

`Enter` and `Escape` are declared as node shortcuts on the fields, not left to the engine's
editing keys: a focused caret swallows them unless a shortcut is bound to the node. The picker's
routing reads `cx.shortcut()` in `PickerState::field_shortcut`, and an accept reported from
there is staged for the frame that built the field to answer.

The picker watches the directory it shows through `Host::watches()`, whose implementation is
the platform's: `ReadDirectoryChangesW` on Windows, inotify on Wayland. The handle reports a
change once per burst — the platform coalesces a save's rename storm into a 250 ms quiet
window — and the picker refreshes the listing on the next frame with the selection still keyed
by path. Watching follows navigation and stops with the dialog; a running search is not
disturbed, because its root is the directory it was started in rather than the rows' source.
A host that answers no watch service simply gets a picker that changes only when the user does.

### One implementation, three design systems

`ui-picker` is written against `ui-widgets` and role names, the same way `List` and `Card`
are: no colour, radius or duration literal appears in it. The app's style pack resolves the
roles; a role a pack does not define falls back to the common plane's token, which is the
pattern `Card` already uses. There is therefore no MD3, Fluent and Glass picker — only role
values, so a pack can restyle the picker without touching it.

The picker adds no roles of its own. It reads the common plane directly —
`SurfaceRaised`/`OnSurfaceRaised` for the dialog, `Surface`/`OnSurface` for the sidebar and the
breadcrumb field, `SurfaceHover`, `SurfaceSelected`, `Divider`, `TextMuted` and `Scrim` — each through
`Style::colour_or` with a literal fallback for a pack that answers no common plane, and the two
surfaces that cover content (the dialog and the crumb menu) through `Style::opaque_or`, since
a raised surface may be a tint meant to composite over the page. A pack
restyles the picker by answering those tokens, and a missing answer keeps the dialog legible
instead of painting transparent.

The chrome icons are names, not parameters yet: the picker names Material Symbols glyphs
(`arrow_upward`, `visibility_off`, and the row's kind marks) and falls back to them when the
platform has no type icon. Parameterising the face per pack is a `PickerIcons`-shaped
addition that nothing needs until a pack ships its own icon font; Fluent and Glass currently
draw the same glyphs, which the screenshots in the verification pass show.

Verification uses the `picker_demo` example, not a gallery page: a gallery page cannot open
a dialog surface — its `App` has no `Host` — and the point of the check is the dialog window
holding its opener's input. The example puts a pack selector on its own window and opens the
picker over it, so the same three packs are checked against the same page.

### States

- **Loading**: the previous directory's rows stay until the answer lands, with the accept
  button disabled while the listing is in flight.
- **Empty**: "This folder is empty" in place of the rows.
- **Unreadable**: the error text, a retry action, and the toolbar still navigating, so a
  permission error never strands the dialog.
- **No selection**: the accept button is disabled; `Enter` on a selected directory still
  navigates.
- **Searching**: the search field holds the address slot, the rows fill in as the walk finds
  matches, "Searching…" stands in the empty body until the first batch, and "No matches" takes
  over once the walk ends with nothing found.
- **Path rejected**: the path field stays open and the footer names the failure.

### What this rules out

- Reusing `Md3Dialog`/`FluentDialog`/`GlassDialog` shells: they are alert dialogs 280–560 px
  wide with fixed padding. The picker declares its own panel from a column and the common
  plane's surface tokens.
- Reusing `List`: see above.
- An OS-level sheet; the dialog window covers that case on the platforms this framework runs
  on.

## Prototype cut line

| In v0 | Out of v0 |
| --- | --- |
| all six modes and both views — details list and grid — opened by the `picker_demo` example | column views |
| real directory listing through `Backend::file_listing()` | batching, cancellation of a listing read |
| thumbnails, platform icons and glyph fallback in the row | directory 2×2 previews |
| keyboard navigation, `Enter`, `Backspace`, `Escape`, focus trap, type-ahead, double-click activation | bookmarks, recent files |
| `Ctrl+L` path entry and `Ctrl+F` search, the search button, the path-failure line | per-row `.hidden`, a MIME table beyond extensions |
| recursive search with streamed batches and cancellation through `Host::searches()` | choices beyond the portal's two shapes |
| the type menu over the request's filters, globs and MIME by extension; the Replace question | dismissing a picker by a click outside |
| the request's extra controls: select menus and checkboxes | |
| hidden entries dimmed in place, their icons and glyphs faded with the name | |
| places and mounted volumes, with the caller's extra places beside them | |
| mountable removable media: enumeration on a worker, a click mounts at the first free letter and navigates | |
| dotfile toggle setting, live directory watching through `Host::watches()` | |
| the in-window modal fallback, with its own retry on a failed read | |
| the dialog's claim on its services' idle caches | |

## Verification

The prototype is done when these hold:

| Check | What it pins |
| --- | --- |
| `directories_sort_before_files` | the comparator's first key, which every file manager shares |
| `a_natural_name_order_compares_digit_runs` | `file2` before `file10`, the reason the comparator is not a plain byte order |
| `a_filter_never_hides_a_directory` | GTK's default, and the mistake that makes navigation impossible |
| `a_stale_listing_is_dropped` | navigating twice quickly shows the second directory, never the first |
| `a_multi_folder_mode_answers_the_marked_folders_and_still_navigates` | a folder mode marks folders, and a double click still enters one instead of answering |
| `a_double_click_opens_a_file_while_one_click_only_selects_it` | the engine's click count, not the row's selection state, decides whether a file answers; a folder opens by navigating |
| `a_save_of_several_names_joins_each_to_the_folder` | `SaveFiles` answers the folder joined with each request name, with the default extension |
| `a_save_of_several_files_must_name_each_of_them` | the contract refuses a SaveFiles request that names nothing, or names a path |
| `the_type_menu_and_every_choice_reach_the_answer` | a filter picked in the menu and a checkbox toggle are both in the answer, so a painted band is not mistaken for a wired one |
| `an_open_answers_a_taken_name_without_asking_to_replace_it` | the overwrite question is gated on a save; an open of an existing file answers at once |
| `an_in_flight_listing_accepts_nothing` | the previous directory's rows stay clickable while the next read runs; accepting one would name a path the dialog is not showing |
| `a_row_outside_the_thumbnail_policy_sends_no_request` | previews-off costs nothing and no request escapes the policy |
| `every_clickable_region_the_picker_builds_reports_its_own_click` | a click on a toolbar toggle, a place, a row or a footer button reaches the app; the tree alone is not paint-only |
| `a_grid_range_is_the_rectangle_between_its_corners` | a grid drag and a Shift extension mark the rectangle, swept in either direction, instead of the linear range a list would take |
| `type_ahead_advances_through_names_with_the_same_initial_and_wraps` | a letter jumps to the first matching name, repeats advance past it, capitals match the same names, and the search wraps |
| `a_typed_path_folds_dots_and_joins_a_relative_one` | a typed relative address is joined to the folder on screen, and `.`/`..` fold rather than reaching the filesystem as characters |
| `a_new_query_stops_the_previous_walk_and_only_its_answer_survives` | a newer query cuts the running walk at its next entry, and a stopped walk's matches never reach the rows |
| `closing_a_search_puts_the_directory_back`, `going_up_at_a_root_leaves_the_search` | leaving a search by its close button or by Up restores the folder's own rows, not the matches |
| `the_view_toggle_builds_a_grid_and_arrows_step_by_its_columns` | the toggle reaches the state and builds cells that report their own click; a vertical arrow moves a whole grid row; a cell does not stand on the list row's node id, which the list's height table would read as a row height |
| `a_short_list_fills_the_panel_below_its_header` | the list's viewport and clip are the panel's, not the content's: a one-row folder in a tall dialog scrolls and clips to the dialog's edge |
| `a_mount_reports_its_point_and_the_next_list_drops_the_volume` | a mount answers its mount point once, and the refreshed list no longer offers the volume |
| `enter_answers_a_question_and_other_keys_wait` | `Enter` answers a modal question with its default instead of accepting the dialog, and a list key pressed over the question changes nothing |
| `a_free_name_is_kept_and_a_taken_one_steps_aside` | a requested name is kept while it is free, and only a taken one steps to `(2)` |
| `a_window_panel_is_recorded_with_its_title_and_is_the_parents_without_a_host` (in `crates/ui-core/src/layout_engine/tests.rs`) | a `Presentation::Window` panel reaches the host with its title, and a host that presents nothing gets it painted and clamped in the dialog instead |
| `a_pointer_leaving_a_level_drops_its_hover_but_never_a_keyboards` (in `crates/ui-platform/src/menu.rs`) | a pointer's departure unlights the context menu's row, while a level the pointer never entered keeps the keyboard's row and an open flyout keeps its header |
| `a_claimed_cache_survives_the_idle_window` | a service a dialog claims keeps its listings, thumbnails and icons through an idle window; released, the sweep applies again |
| `picker_demo` against `$HOME` and fixture directories | platform icons and decoded thumbnails in the rows; scrolling and keyboard (`Down`, `End`, `Enter`, `Escape`) in a dialog window of its own with the parent's input held, under each of the three packs; pointer clicks on the pack buttons, a row, a place, the view toggle, the hidden toggle and Cancel/Open; a single click selecting a row and a double click opening it; hidden entries dimmed in place; the removable section mounting an inserted stick; the grid's cells and its rectangle drag; then the same picker drawn as an in-window modal; an unreadable start directory shows the error and the retry action; a name too long for its column showing its full text in a tooltip after a hover, in both views; the row menu dropping its highlight when the pointer leaves it |

The pointer half was driven through `zwlr_virtual_pointer_v1` in the nested session, which has
no physical input device. On a 2560×1440 output, a click at (41, 105) opened the MD3 dialog; a
click on the `sub` row selected it and a double click navigated into it (single click at the
time; the engine now counts the burst, and the test above pins the split), the breadcrumb's
root returned, and Cancel and Open each delivered the expected answer to the parent's readout.
The same clicks worked in the Fluent and Glass dialogs and in the in-window fallback at a 48 px
inset.

On Windows 11 at 125% scaling, the same example was driven with synthesized pointer input
after `SetWindowPos` raised the dialog above the other windows. The MD3 and Fluent dialogs
listed `C:\Windows\Web\Wallpaper\Windows` with the shell's thumbnails in the rows and the
nine shell places in the sidebar — `Home`, `Desktop`, `Documents`, `Downloads`, `Music`,
`Pictures`, `Videos`, `Templates`, `Public`, the last being `FOLDERID_Public` and `Templates`
resolving to the profile's roaming folder as the shell reports it. A click on a sidebar place
navigated, a click on `img19.jpg` selected the row and enabled Open, and a click on Open
closed the dialog with `C:\Windows\Web\Wallpaper\Windows\img19.jpg` on the parent's readout.
The rows of the repository root drew Windows' own folder, text and generic-page icons.

At the same 125% scale the dialog opened on `C:\Windows\Web` showed the breadcrumb as
`C:` › `Windows` › `Web`, the roots being the drive's prefix rather than an empty name, and a
"Devices" section under the nine places listing `系统 (C:)`, `数据 (D:)`, `Portable Disk (E:)`
and `RamDisk (R:)` with the drive glyph the platform type-icon fallback draws. The two clicks
that need a pointer are tests instead of screen evidence, because each is the id the view builds
fed back to `PickerState::changed`: `a_breadcrumb_click_jumps_to_its_ancestor` and
`a_device_click_opens_its_mount_point`. `a_mount_table_yields_one_row_per_volume` pins the
Dolphin-shaped selection of `linux_mounts` against a `/proc/self/mountinfo` fixture that holds a
btrfs subvolume, a container bind mount, two snap loops, tmpfs and an escaped label.

## Status

Implemented at the v0 cut. `ui-platform` carries `listing` (`Entry`, `ListingRequest`,
`Listings`, the shared `FsDirectoryLister`), `search` (`SearchRequest`, `Searches`,
`FsDirectorySearcher`, which shares `read_entry` and the hidden rule with the lister), `watch`
(`Watches`, the portable `Watch` handle and the `DirectoryWatcher`/`WatchGuard` traits),
`volumes` (`Mountable`, `VolumesState`, `VolumeMounter`, `Volumes`) and
`places` (`Place`, `PlaceKind`, `Device`, `linux_mounts`); `ui-picker` is the portable crate with
the model, the row view, `Picker` (the dialog `App`) and `PickerState` (the same picker for an
in-window modal); the `picker_demo` example is the harness. Both backends answer the listing,
the search, the watch, the places and the devices: Wayland reads `user-dirs.dirs`,
`/proc/self/mountinfo` and dotfiles, walks through `FsDirectorySearcher::dotfiles()` and watches
with inotify; Windows asks the shell for places and icons, reads logical drives from
`GetLogicalDrives`, enumerates unmounted removable volumes and mounts them (`Win32Volumes`),
reads directories through the shared lister with the attribute rule, and
watches with overlapped `ReadDirectoryChangesW` threads. The watcher and volume implementations
live in the platform crates because the layering test forbids `std::os::windows`/`libc::` in
`ui-platform`; the traits and the handles stay there, and the `Host` owns the services and
forces a frame from their completion counters, the same way it does for listings and
thumbnails. The gallery page is not part of it: a gallery page has no
`Host` and so cannot ask for a dialog surface, which is the one thing the check must exercise.

The picker's own list, grid and sidebar are fill-height scrollers (`Scroll::fill`), so their
viewport, clip and bar reach the panel's edge over a short folder; `PickerState` holds a
`KeepAlive` claim on each of its services, so an open dialog's caches survive the idle window
and a closed one's do not.

What the cut leaves for later is the table above plus `local_only` thumbnails, a per-pack
icon face, and remembering a folder across runs (`state_persistence`, the one capability the
drawn provider does not raise).

## Corrections

**`Entry` and `Place` were to live in `ui-picker`.** They are the platform's answer, and the
listing cache hands `Arc<Vec<Entry>>` around while `Entry::version()` keys a thumbnail
request; a consumer-side copy would be rebuilt on every answer. They are in `ui-platform`
now, and `ui-picker` holds only what is the view's: `FileFilter`, `ThumbnailPolicy`, the
model and the two picker types.

**A generation counter was to drop stale answers.** The model never asks about a directory
it has left: it holds one `ListingRequest` and only ever queries `Listings` about that, so a
late answer to another request cannot be mistaken for this one. The counter was a way to
mechanically prevent a mistake that the query shape already makes impossible.

**The listing was to answer sorted entries.** It answers them in the platform's order. The
page's own rule is that ordering is the view's; sorting in the lister would also make the
claim that a filter change costs no disk read false to read.

**`ThumbnailPolicy` carried `local_only`.** The flag needs the filesystem type of the row's
own directory; the mounted-volume list answers whether a volume is a block device, not what
filesystem it holds, so every answer would have been a guess. The type is `show` and a size
limit until that data exists.

**Double-click was to activate a row, because the engine had no click count.** A click
selected, and a click on the already-selected row, `Enter`, or the accept button activated.
That stayed defensible only while the engine could not tell a second press from a hesitant
first one; the engine now counts each button's burst (see
[Pointer Model](./pointer-model.md#clicks)), so the row opens on a double click and a single
click only selects, as the native dialogs do. `Enter` and the accept button still activate.

**The picker added its own colour roles.** It names none. The common plane's
`SurfaceRaised`/`OnSurfaceRaised`, `Surface`, `SurfaceHover`, `SurfaceSelected`, `Divider`,
`TextMuted` and `Scrim` already resolve in all three packs, and a parallel `pickerSurface`
vocabulary would have needed each pack to map it for no gain.

**`PickerIcons` was to parameterise the icon face.** The picker names Material Symbols
glyphs. No pack ships another icon font yet, so the parameter would have had no caller; it
returns when one does.

**A gallery page was the verification harness.** A gallery page's `App` has no `Host` and
cannot open a dialog surface, which is exactly what the check must exercise. The
`picker_demo` example owns the `Host`, asks for the dialog from `Host::on_turn`, and keeps
the same picker as a `PickerState` for the in-window fallback — so one harness covers both
forms. Its light/dark switch puts the same three packs through both appearances.

**The accept button was enabled whenever a row was selected.** It is also gated on the
current read being ready. Navigating leaves the previous directory's rows on screen and
clickable; accepting one would hand the app a path the dialog is no longer showing, so the
button stays disabled and `Enter` does nothing until the answer lands.

**The picker was built from declarations alone, and its pointer did nothing.** The first
view returned one `Decl` tree, and `Picker::view` handed that tree to `from_decl`. The
keyboard shortcuts and the demo's scripted `PICKER_DEMO_OPEN` worked, so the dialog looked
finished, but a click on a pack button, a row, a place, a breadcrumb or a footer button
reached no `change()`: a declaration carries none, and the engine's pointer activation is
read only by the control that owns the node. The checklist rule — a live control is placed
through `Cx::element`, or `change()` is silently never called — was already written down;
the view ignored it. Every clickable region is now a `Pressable`, a widget that turns
`state.activation()` into `Change::Activated`, placed through `Container` elements, and
`PickerState::view` returns the element tree so the in-window fallback keeps its controls
live. `every_clickable_region_the_picker_builds_reports_its_own_click` walks the built tree
and clicks one region in each band; it goes red when any of them is rebuilt as a bare
declaration.

**The picker owned its own mode and filter types.** `Mode` and `FileFilter` were this
crate's, which made the picker the definition of what a file dialog could be asked for.
They are `ui_platform::chooser::ChooserMode` and `FileTypeFilter` now: the platform's
dialog and the drawn picker take the same request, and the fallback compares one
capability list instead of translating between two vocabularies.

**The name field was going to need a new widget.** The base set has no style-neutral text
field, so the page expected the picker to add one. The engine already owns text input on a
container (`Decl::text_input`) and reports the whole buffer as `Change::Text`; drawing a
background and one `Text` view around it was enough.

**The portal was a separate backend "if it is ever wanted".** It is here, behind the same
contract as the drawn picker; see [File Choosers](./file-choosers.md).

**`SaveFiles` had no names to write.** The mode was defined from the portal's `SaveFiles`,
whose `files` option *is* the list of names, but `SaveOptions` carried only one
`suggested_name`; a drawn dialog could only have answered a folder, which is not a path the
caller asked it to write. `SaveOptions::names` carries the list now, `ChooserRequest::save_files`
takes it, and `validate` refuses an empty list or a name that carries a path. Pinned by
`a_save_of_several_files_must_name_each_of_them`.

**The type menu was a caption.** The picker honoured `default_filter` when it opened and
then drew the active filter's label as text, so the filters a request offered could never be
switched. The footer's chip is a menu now, its rows come from the request, and a check names
the active one. Filters also match MIME types through the framework's extension table, which
is what `mime_filters` claims; `a_mime_filter_names_an_exact_type_or_a_kind` pins the
wildcard rule, and `the_type_menu_and_every_choice_reach_the_answer` pins the view's ids to
the state's handlers.

**The overwrite question was the caller's alone.** A drawn save answered a taken name without
asking, while `OverwritePolicy::Prompt` and the contract's `overwrite_prompt` capability said
a provider could ask. Accept now opens a Replace panel over the accept button, `Forbid`
disables accept for an existing name, and `strict_types` does the same for a name the active
filter rejects. The first cut asked in every mode, so an open of an existing file drew the
Replace panel; the question is gated on `is_save` now, and
`an_open_answers_a_taken_name_without_asking_to_replace_it` pins it. The capability table and
`drawn_capabilities` say so.

**`SelectFolders` and `SaveFiles` were in no provider.** The modes were defined in the
contract and reported false everywhere, so a caller asking for either fell back or failed.
Both are drawn now — the first as a multi mode over folders, the second as a folder choice
whose answer is the request's names inside it. Clicking a directory in any mode is a step
into it on a double click, which is also what keeps a multi mode browsable while its marks
stand (`a_multi_folder_mode_answers_the_marked_folders_and_still_navigates`).

**The accept button kept its disabled colours for a frame after a selection.** The button's
look came from `cx.state(id).disabled`, and that bit is the committed node's — last frame's
declaration. Selecting a row enabled the model in the same turn, but the first rebuilt frame
still resolved from the stale bit, so the colour changed only when an unrelated frame
happened to arrive. Resolving the helper from this build's answer was not enough: the
`Styled` adapter then faded the whole button from the same stale routing, and the picker's
`Pressable` never reported that the declaration already paints its disabled colours, so the
button was dimmed twice and that second fade still lagged. The adapter now folds this
build's declaration together with the inertness its containers declare while building
(`Container::decl` opens the scope), and the footer's buttons are `Pressable::painted_disabled`
elements. The frame that receives the click is the frame that shows the enabled button;
`a_control_that_handles_disabled_is_not_faded_a_second_time` pins both fade sources and the
stale-routing case.

**The footer packed every control after the name field.** One row meant the type menu sat
wherever the growing field ended, and with no field the chip and both buttons drifted left.
The footer is two groups now: a growing left slot for the name field or the every-name
status, and a hugging right group holding the type menu, Cancel and the accept button.

**The name field had no caret and did not open focused.** The field declared `text_input`
and one plain `Text`, which edits a buffer but draws no insertion point: the engine's caret,
selection and composition are facts the app has to read back and put on the text decl. The
field now asks `caret_for`/`selection_for`/`preedit_for`, draws the empty buffer's caret box,
and a save asks for focus once when it opens, as a platform save dialog does.

**Scroll insets were margins around the scroller.** The rows, sidebar, crumb trail, choices
band and the two menus each padded a container outside their `Scroll`, so the viewport and
its scrollbar stopped at the margin: the bar sat on the edge of the content and the clip cut
exactly where a column's text ended. The insets are padding on the scrolled content now; the
viewport reaches the region's edge and the bar and cut land on empty space. The rule is
general — every scrollable container uses content padding — and is recorded in the agent's
memory.

**The whole panel scrolled sideways.** The rows were a `Scroll` over both axes, so dragging
anywhere in the list moved every column under a header that followed it from a frame behind;
the user had no way to move the columns deliberately and no place to grab. The header owns
the horizontal offset now: it is its own horizontal scroller at a stable id, the rows are a
vertical-only scroller displaced by the header's `offset_x`, and the header is also where a
sideways drag goes. The columns' boundaries carry short rules inside the header's `Stack`,
inset from its top and bottom, and the header's own underline is one faded pixel inset from
both ends so it reads apart from the toolbar's full-width rule.

**A click had no modifiers, so a multi mode could only mark one row.** The first multi mode
marked the clicked row and left `Ctrl` and `Shift` unanswered: `Change::Activated` carries no
modifier set, and the view had no drag gesture at all. Rows are the picker's `RowControl` now,
reporting a `RowGesture` with the modifiers the engine saw and, while a `Sense::DRAG` press is
past the threshold, the row under the pointer; the model shapes a click into replace, toggle
or extend and a drag into a swept or additive range, with `Shift`/`Ctrl` arrows and `Ctrl+A`
on the same model. `clicks_and_a_drag_select_the_way_a_file_manager_does` pins the
model's four gestures, and `every_clickable_region_the_picker_builds_reports_its_own_click`
pins that a row still reports rather than going silent.

**The extra controls were a permanently visible band.** Every request with `choices` paid
44 px whether or not the user touched them, and the band arrived and vanished in one frame
when the dialog was rebuilt. The footer now carries a toggle, and the band's height is an
`animate` on its own id: the body's height follows the same value, the content stays at its
full height aligned to the band's bottom and is clipped, so opening reveals it upward and
closing runs that backward, with group opacity following the height. The band is declared
even at rest so its scroller keeps its offset and its controls keep their ids.

**The picker was pinned to one appearance.** `PickerOptions` took a resolved `&'static Style`,
so a dialog could only ever be the appearance its opener happened to pass — in practice the
demo's dark pack — and there was no way to follow the host's light/dark preference. It takes
a pack family (`PackOf`, the kits' own `pack_of`) now, and resolves the family for the
appearance stated by the `StyleScope` on its `env`, which is the same scope every other control
reads. The demo states the scope from the system preference (`ThemeWatch`) and offers a
light/dark switch, and `PICKER_DEMO_DARK=0|1` seeds it for a screenshot; the picker itself
names no appearance-specific colour.

**The grid's cells stood on the list's row ids.** `grid_cell` built its node on
`row_id(entry)`, the same id the list uses, which looked harmless because both name one
entry. `RowHeights` learns a row's height by reading the rect that id left in the previous
frame's tree: after a visit to the grid, the first list build read 136-tall cell rects as
row heights and predicted every unseen row at that average, so most of the list collapsed
into one spacer — switching from grid to list showed a handful of rows until interaction
scrolled and the table slowly re-measured. Cells have their own `grid_cell_id(entry)` now;
the two views never share a node, and `PickerState::row_index` accepts either so a grid
drag still names its entry.
`the_view_toggle_builds_a_grid_and_arrows_step_by_its_columns` asserts that a cell does not
declare the row's id.

**The selection fill was the only highlight.** Hover and focus were derived from selection:
a marked row dropped its hover wash, and the cursor had no ink of its own, so in a multi
mode the cursor could sit on an unmarked row (`Ctrl+Arrow`) or inside a block of marks and
the screen would not show where the keyboard was. Rows composite the wash over the
selection fill and stroke the cursor's row with the pack's `FocusRing`, and the sidebar's
rows take the same layering; the demo shows hover on a marked row and the ring moving with
the arrows in `OpenFiles`.

**A closed picker's listings stayed in memory for the run.** The shared `Listings` cache
kept up to 32 directories and never let one go: every `Entry` carries its name and path, so
browsing folders of thousands of files pinned several megabytes after the dialog was gone.
Three open/browse/close cycles of `picker_demo`, each closed and left idle past the picture
caches' window, measured 40.4 → 59.8 → 62.7 → 64.9 MB — the first cycle paying the cache's
fill, each later one a few more directories. The cache follows the picture caches' idle rule
now: a `get` or an `invalidate` counts as use, and the worker drops every listing once 60 s
pass without one. `an_idle_cache_is_dropped_and_the_next_look_asks_again` pins it and was
watched red with the drop removed.

**A field's `Enter` never reached the picker.** The save name field declared `text_input` and
let the dialog's bound `Enter` do the accept, but a focused caret consumes the key before the
surface's bindings see it, so typing a name and pressing `Enter` did nothing. A field that needs
keys of its own declares them as node shortcuts (`Decl::shortcut`), and every input field now
binds `Enter` and, when it dismisses, `Escape`; `PickerState::field_shortcut` reads
`cx.shortcut()` to tell a field's key from the dialog's. An accept that arrives while the modal
is being built is staged and drained after the build, because the state cannot answer from
inside its own `view`.

**A Win32 watch re-armed its overlapped read as soon as cancellation returned.** `CancelIoEx`
only asks the kernel to cancel; it may still be writing into the `OVERLAPPED` the read was
started with. The old code re-used the stack struct for the next `ReadDirectoryChangesW`, so a
cancelled read could complete into a struct the next read owned. Cancellation now follows with
`GetOverlappedResult(..., true)`, which returns only once the kernel has let go of the
`OVERLAPPED` and the receive buffer.

**Search was going to answer with names alone.** A match three levels below the root drew a
bare file name, which does not say where the file is; the row shows the path relative to the
search root while a search is running. `SelectFolder` accepts nothing during a search for the
mirror reason: the only folder on screen is the root, and the caller asked to select what the
user found, not the directory the walk started in.

**A search's walk could outlive its query.** The first worker kept walking after a newer query
arrived and only guarded the answer at the end, so an old walk could hold the disk for seconds
and its buffered matches could land in the newer state's rows. `Searches` checks the generation
in the walk's `stop` closure, in every batch flush and before the final answer; the walk stops at
its next entry once the query is replaced. Pinned by
`a_new_query_stops_the_previous_walk_and_only_its_answer_survives`, watched red with the
generation test removed from `stop`.

**Closing a search left its matches on screen.** `close_search` stopped the walk first, and
stopping gave the model up as a search (`searching = false`); `end_search` then saw no search to
restore and returned, so the rows stayed matches while the toolbar said the search had closed.
The restore now runs before the service is cancelled, and the model's own way out — a
navigation that lands on the folder already on screen, or Up at a root — ends the search rather
than returning early. `closing_a_search_puts_the_directory_back` and
`going_up_at_a_root_leaves_the_search` pin both, watched red with the old order.

**The address bar could only be reached by `Ctrl+L`.** A click on the crumb naming the folder
on screen did nothing: it navigated to the path already shown, and `navigate` returned before
touching anything. That crumb is the one an address bar's click means, so it now turns the
trail into the path field, focused and holding the current path; a click on the empty space
after the last crumb does the same, and a crumb naming an ancestor still jumps to it. The
empty-space target is a sibling of the crumbs rather than their container, so a click on a
crumb never reaches both.

**The editable line was painted by the picker itself.** The field drew a `Text` and a caret box
of its own instead of the shared line the packs' text fields draw, and was missing what that
line provides: the pointer showed an arrow instead of a text cursor, and a value longer than
the field ran under the edge rather than scrolling with the caret.
`ui_widgets::text::LineEdit` already draws exactly this line — caret scroll, selection,
composition, placeholder — and all three fields use it now. A field that fills its parent takes
its width from the rect its last frame left, since no width exists before layout.

**A hidden entry looked exactly like a visible one.** The toggle listed dotfiles, but the row
drew no distinction, so the only way to tell why a name was on screen was to recognize its
dot. Explorer and Dolphin dim a shown hidden entry, and the row now fades its mark and name to
55% while keeping the row's geometry, so the columns stay aligned and the row stays fully
clickable; the styling is paint, not state.

**A single click on a row had to select before anything could open.** The engine reported no
click count, so the picker made the second click — a click on the already-selected row —
activate. That rule cannot be told apart from a hesitant click on a selected row, and it made
a double click open only because its first half had asserted the selection. The engine now
counts the burst per button, and the row opens on a double click (or `Enter`), while one click
only moves the selection; in `SaveFile` a double click on a file fills the name field.

**Unmounted removable media was in the cut.** The sidebar listed mounted volumes only, so a
USB stick the desktop had not auto-mounted had no row at all and was unreachable from the
picker. The platform enumerates mountable, unmounted removable volumes (`Win32Volumes`) and
mounts one on a click at the first free drive letter; the picker draws them under a
"Removable" heading below "Devices" and navigates to the mount point the worker answers.
`a_mount_reports_its_point_and_the_next_list_drops_the_volume` pins the worker's contract.

**An open dialog's caches were evicted by the idle window.** The listing, thumbnail and icon
caches each drop after 60 s with no `get`, which is right for a closed dialog and wrong for an
open one: an untouched dialog builds no frames, so nothing refreshes `asked_at`, and the first
pointer move after a minute re-read the directory and re-ran the shell query for every visible
icon. A `KeepAlive` claim now states "in use, merely idle": `PickerState` holds one per service
for the dialog's lifetime, and the sweep waits while any claim lives. The size bounds still
evict. `a_claimed_cache_survives_the_idle_window` pins both the listing and the picture workers
(red: "the claimed cache was dropped").

**A question was a popup above the accept button.** The Replace question, and the conflict,
delete and undo questions that followed its shape, were `Presentation::Platform` popups
anchored above the footer. That put them in the window's bottom-right corner, and it left
their keys in the popup's own surface, so `Enter` never reached them: it resolved against the
dialog's binding and accepted the dialog under the question. A confirmation is an in-dialog
modal now — `Decl::modal(ModalTarget::Global)`, the pack's `Scrim`, two `grow_y` growers for
the centred surface — and `PickerState::shortcut` answers it on `OPEN` and `CANCEL` while
dropping every other key. Properties and the failure list moved the other way: they report,
so they take no scrim, and `Enter` or `Escape` closes them.
`enter_answers_a_question_and_other_keys_wait` pins the routing.

**A reading window was still a popup above the dialog.** The first shape of the redesign moved
Properties and the failure list to `Presentation::Platform` panels under the toolbar: a surface
of their own, but a popup all the same — a press outside dismissed it, a `WS_EX_NOACTIVATE`
popup never takes the foreground on Win32, and a compositor has no reason to float one. A
report read while the desktop is used elsewhere belongs in a window. The engine grew
`Presentation::Window`: the same declared panel, presented into a modeless window owned by the
surface it is declared in — an owned dialog window on Win32, a floating `xdg_toplevel` on
Wayland — and named by `Decl::window_title`. The fallback chain stays whole: a backend with no
window surfaces offers the panel as a popup, and a refused popup paints it in the parent.
`a_window_panel_is_recorded_with_its_title_and_is_the_parents_without_a_host` pins the
recording and the fallback; `picker_demo` is the visual check.

The first cut of that window kept the question's card and let the platform's frame resize it.
Both were wrong for the same reason: a reading panel has a frame now, so its card's border,
corners and shadow drew a second one, and the panel does not reflow, so a window dragged larger
left a blank strip beside the content. The reading body is a plain fill of the pack's raised
surface, and the window is pinned to the panel's size — a dialog frame with no sizing border on
Win32, the toplevel's minimum and maximum on Wayland — so only a change of content changes the
size.

**New Folder always started at "(2)".** `unique_name` means "a name that is free", and its
first candidate is always the numbered one — right for a copy, which must never reuse the
source's name, and wrong for a create that was given a free name of its own. The create path
asks `available_name`, which keeps the plain name until something holds it;
`a_free_name_is_kept_and_a_taken_one_steps_aside` covers both halves.

## See also

- [File Operations](./file-operations.md) — the copy, move, link and trash verbs a manager adds.
- [File Thumbnails](./thumbnails.md) — the decode layer a row may show instead of an icon.
- [File Icons](./file-icons.md) — the kind vocabulary and the icon a row falls back to.
- [File Choosers](./file-choosers.md) — the portable request and the platform dialogs.
- [Dialog Surfaces](./dialogs.md) — the window the picker opens in.
- [Event loop](./event-loop.md) — how a completion becomes a frame.
