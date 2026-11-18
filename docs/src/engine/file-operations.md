# File Operations

A file chooser answers a question about existing files; a file manager also copies, moves,
renames, links and deletes them. This page designs that step for the picker (the operation
model, path transfer, the drag and menu vocabulary, and the platform machinery worth
reusing); it is **Planned**, designed but not built.

## Problem

The picker reads directories it cannot modify. Dragging a row sweeps the selection and
carries nothing; a right-click on a row does nothing; Ctrl+C on a marked file copies
nothing; a file dropped on the dialog is ignored. The engine has an OS drag source and
target, a clipboard and a context-menu renderer, but not the vocabulary between them.

| Concern | Today | Consequence |
| --- | --- | --- |
| Drag payload | `DragData` is MIME bytes plus one fixed `DropEffect` | No modifier negotiation, no "ask" |
| Drop report | `Change::DroppedOn(DragData)` has no point, modifiers or button | A target cannot tell a right-drag from a left-drag, or which row was hit |
| Source outcome | `Finished { dropped: bool }` | A source cannot learn move from copy or link |
| Clipboard | `ClipboardCommand::{Copy, RequestPaste}`; paste is text | No cut marker; a path list cannot be pasted in or out |
| Paste | `Event::Paste(String)` | A pasted or dropped file list has nowhere to land |
| Menus | Rows are declared on a node; opening is engine-private | No row menu, no background menu, no Menu key |
| Files | No write operation at all | No copy, move, rename, delete, trash or link |

## Comparison

| Manager | Plain left-drag | Forced effect | Link by drag | Right-drag menu |
| --- | --- | --- | --- | --- |
| Explorer | move on the same volume, copy across | Ctrl copy, Shift move | Alt or Ctrl+Shift `.lnk` | Move here / Copy here / Create shortcuts here / Cancel |
| Dolphin (KIO) | **asks**: Move Here / Copy Here / Link Here / Cancel | Ctrl copy, Shift move (or the menu) | Ctrl+Shift symlink | same menu |
| Nautilus | move on the same device, copy across | Ctrl copy, Shift move | Ctrl+Shift symlink | none since GTK 4 |
| Finder | move on the same volume, copy across | Option copy, Command move | Cmd+Option alias | none |
| Thunar, PCManFM-Qt | Thunar asks since 4.21; PCManFM-Qt always asks | Ctrl copy, Shift move | Ctrl+Shift symlink | Thunar yes; PCManFM-Qt via the left-drop menu |

KDE's reason for asking is the one that matters here: the same-device test is wrong for
removable media, network shares and virtual locations, and a wrong move is destructive.
Three managers ask on every plain drag; the others use the device heuristic with modifier
overrides and ask only when a collision appears.

Cut/Copy/Paste, Rename, Delete, Move to Trash and New Folder appear in every menu.
Accelerators are stable: Ctrl+X/C/V, F2, Delete, Shift+Delete, Ctrl+Shift+N, Alt+Enter.
Conflicts offer Replace, Skip and Keep Both or Rename; Explorer, Dolphin and Nautilus add
an apply-to-all answer, and folders merge instead of conflicting. Undo exists in Explorer,
Dolphin, Finder and Thunar, and not in Nautilus. No manager creates hard links from the UI
without an extension.

> **Sources.** Explorer: shell data transfer and right-drag guidance, `CF_HDROP`,
> `Preferred DropEffect` (Microsoft Learn); MMC right-button drag documentation. KIO
> `src/widgets/dropjob.cpp`, `renamedialog.cpp`, `paste.cpp` (`application/x-kde-cutselection`
> is the byte `1` for cut, `0` for copy) and `FileUndoManager`. Nautilus' conflict dialog
> and clipboard source (`x-special/gnome-copied-files`). Thunar 4.20/4.21 release notes and
> hidden settings. Wayland `wayland.xml`, `wl_data_source`/`wl_data_offer` (actions
> `copy`/`move`/`ask`; the source deletes after `dnd_finished` when the final action is
> move; an `ask` is resolved by the destination before `finish`). freedesktop Trash
> Specification 1.0.

What this design keeps:

1. **The device heuristic for the plain drag**, because it is what Explorer and Nautilus
   users already do.
2. **An explicit question on the right button**, because the dialog's rows are where
   ambiguity costs data.
3. **Keep Both with a numeric suffix** as the safe default for a collision.
4. **Trash by default**, permanent delete only on explicit intent.
5. **A per-item error list**, never transactional rollback: a cross-volume move cannot be
   rolled back once bytes have landed.

## Contract

### Operations

```text
OperationKind  Copy | Move | Rename | Delete{Trash|Permanent} | Link{Symbolic|Hard}
Request        kind, sources, destination, new_name, policy
```

Copy, Move and Link take a destination folder; Rename takes a new name and one source;
Delete takes no destination. An operation is a list of independent per-source jobs; one
failure never aborts the batch.

```text
FileOperations::start(request) -> OperationId
FileOperations::cancel(id)
FileOperations::take() -> Vec<OperationEvent>
FileOperations::answer(id, decision)
```

`OperationEvent` is `Progress { done_items, total_items, bytes_done, bytes_total,
current }`, `Conflict { … }` or `Finished(OperationReport)`. `current` is the source being
worked on, as a path, so a view can point at it in the batch it started rather than match a
name. The report carries
`succeeded`, `skipped`, the items still undone after a cancel, and `failed:
Vec<(PathBuf, OpError)>` with a portable error vocabulary: `NotFound`,
`PermissionDenied`, `ReadOnly`, `AlreadyExists`, `CrossDevice`, `Recursive`,
`Unsupported`, `Io`.

### Planning and safety

A request becomes a plan before any byte moves, so every refusal happens while the user is
still looking at the menu:

- The destination is canonicalized; a destination inside a source is refused as
  `Recursive`, as all three file managers in the table above refuse it.
- Move onto the same folder is `AlreadyThere`; Copy onto the same folder duplicates.
- A file landing on a directory, or a directory on a file, is a conflict; a directory on a
  directory is a merge.
- An unwritable destination is refused before the batch starts (KIO's `isWritable` rule).
- Symbolic links need `SeCreateSymbolicLinkPrivilege` or Developer Mode on Windows
  (`SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`). When the privilege is absent the row is
  disabled with the reason; it never starts and fails. Hard links are files-only and
  same-volume.

The device heuristic asks the operator, not a name prefix or the mount table:
`same_volume(a, b)` is `GetVolumePathName` plus volume serial on Windows and `st_dev` on
Linux. Folders come from removable volumes, network shares and virtual places, and a guess
is how the wrong move happens.

### Conflicts

```text
ConflictPolicy  Ask | Replace | Skip | KeepBoth | Rename(name)
```

`Ask` suspends the job and hands the question to the view: the item, the clash, whether it
is a file or a folder, and an offered unique name. The answer applies to one item or to
every remaining item (Apply to All), the Dolphin and Nautilus model. Keep Both derives
`name (2).ext`, `name (3).ext`; the same helper names a same-folder copy, so one rule
explains both.

### Execution

Cancellation is cooperative, checked between items and between copy chunks. What has
already moved stays moved and the report says where the batch stopped. That is the same
model as Explorer, KIO and Nautilus.

The worker follows the shape every service already has: a `Clone` handle over `Arc<Shared>`,
one named thread, an `mpsc` queue, a mutexed state, and a completion counter the `Host`
compares each turn so a finished job forces a frame. It differs in one way that must not
be missed: the exit policy only counts `Jobs`. A transfer is submitted through `Jobs`, or
the exit policy gains the transfer service; a running operation must not be abandoned
when the last window closes. No `KeepAlive` belongs here: claims protect a cache from the
idle sweep, and an operation owns its state and ends.

### Portable semantics

| Operation | Same volume | Across volumes |
| --- | --- | --- |
| Move | one `rename` per item, atomic | recursive copy, verify, then remove the source |
| Copy | write `name.part` beside the destination, sync, `rename` into place | same |
| Rename | same-directory `rename`; a case-only rename goes through a temporary name | — |
| Delete | trash by default; permanent only on explicit intent | the platform's trash fallback |
| Link | symbolic or hard | symbolic only; hard link is refused `CrossDevice` |

Three rules keep the engine honest:

- **Symlinks are copied as links.** `std::fs::copy` follows them, silently turning a link
  into a duplicate of its target. The walker reads `symlink_metadata`, recreates the link
  through the operator and never descends through it.
- **Directory copies merge.** An existing destination directory is written into, not
  replaced; collisions inside it use the same conflict policy.
- **Times and permissions are carried over** where the platform permits it. A read-only
  file copies read-only; overwriting a read-only destination is `ReadOnly`, not ignored.

Special files (FIFOs, sockets, devices) are refused `Unsupported`: copying them can block
and dereferencing them is not what the user asked for.

### Undo

An undo journal records inverse actions: move back, rename back, restore from trash,
remove the copy. It is planned after the verbs, because two of the four inverses are not
universally available. Windows restores from the Recycle Bin through the shell, not
through a format we can write, and a copy the user has since modified must not be deleted
without a question. The report already records source, destination and outcome, which is
what a journal needs.

## Clipboard and paths

A path list is `text/uri-list`: CRLF-separated, percent-encoded `file://` URIs, `#`
comments skipped, one path per line. The Win32 clipboard already maps it to and from
`CF_HDROP`, but there is no portable builder or parser and `transfer.rs` leaves parsing to
the app. That is the wrong layer; every consumer writes the same decoder.
`ui-platform::uri_list::{encode, decode}` becomes the one implementation, accepting only
`file:` URIs and reporting a malformed line instead of guessing.

The clipboard grows a file representation with an effect:

```text
ClipboardContent::Files { paths: Vec<PathBuf>, effect: DropEffect }
```

Writing it writes each platform's marker:

| Platform | Format | Cut marker |
| --- | --- | --- |
| Windows | `CF_HDROP` | registered `Preferred DropEffect` = `DROPEFFECT_MOVE` |
| Wayland | `text/uri-list` | `x-special/gnome-copied-files` (`cut`/`copy` first line) and `application/x-kde-cutselection` (`1`/`0`) |

Both Linux markers are written because neither is standard: GNOME's is undocumented and
Nautilus-only, KDE's is KIO-private. Reading accepts either, requires them to agree when
both exist, and defaults to copy.

Paste reads the path list and the markers in one round trip and delivers
`Event::Paste(ClipboardContent)`, replacing the text-only `Event::Paste(String)`. Text
fields keep matching the text variant; a file list pasted into a text field is ignored,
not stringified. A cut is consumed by the first successful paste, so a cancelled conflict
question does not lose it; items that failed keep their cut until a later paste moves them.
KIO goes one step further: the clipboard is then rewritten to the destination paths, so a
second paste copies instead of moving files that are no longer there.

## Drag and drop

### Effects

`DragData.effect` becomes a set of permitted effects; the payload states what it can
mean, not what every target must do. The engine derives the offered effect from the
modifiers held during the drag:

| Modifiers | Effect |
| --- | --- |
| none | the payload's preferred effect |
| Ctrl | Copy |
| Shift | Move |
| Ctrl+Shift | Link |

This is the Wayland convention, recommended by the protocol but not guaranteed by every
compositor, and it matches Windows apart from the Command key. The picker's preferred
effect is the device heuristic, so a plain drag matches Explorer and an override is always
available.

The work is in the engine: the Win32 target ignores `MODIFIERKEYS_FLAGS`, the Wayland
target advertises Copy only, and `drop_effect()` returns the payload's constant.
`Change::DroppedOn` must carry the point, the modifiers, the button and the resolved
effect, because a right-drag and a left-drag differ only in those fields.

### Who performs a move

The asymmetry is easy to get wrong and destructive when wrong:

| Where | Who moves the file | What the source does |
| --- | --- | --- |
| In-app drop | the target, through `FileOperations` | nothing |
| Windows, out of the app | the target (optimized move) or the source | delete only when the logical effect is Move and the target did not perform it |
| Windows, into the app | the target, through `FileOperations` | nothing; the Windows source does not delete |
| Wayland, out of the app | the source, after `dnd_finished` | delete when the final action was Move; never on `cancelled` |
| Wayland, into the app | the source, after `dnd_finished`; the destination copies and does not delete | delete when the final action was Move |

`Finished { dropped: bool }` is therefore not enough. The source needs the final action
and whether the target performed it; the engine normalizes both platforms into one
`DropOutcome { action, performed_by }`, and the picker deletes only in the Wayland-source
case. A Wayland destination copies even when the action is Move: renaming on the
destination would race the source's delete for the same files, and the protocol gives the
deletion to the source.

### Right drag

The decision menu belongs to the drop target:

- **In-app.** The engine reports `button: Secondary` and `effect: Ask`; the picker draws
  "Move Here / Copy Here / Create Link Here / Create Hard Link Here / Cancel" at the drop
  point. Nothing has moved, so Cancel is free.
- **Into this app, Windows.** `IDropTarget::Drop` sees `MK_RBUTTON`; the picker accepts and
  then shows the menu. A Windows source does not move anything itself, so accepting
  without acting is safe.
- **Into this app, Wayland.** The destination is the side allowed to ask. The target
  advertises `ask`, shows the menu, and sends the final action before `finish`; dismissing
  the menu destroys the offer. This needs the backend to hold `finish` until the app
  answers, so an `Ask` drop is answered through `Host` rather than implied by returning.
- **Out of this app.** The external target shows its own menu (Explorer) or consumes a
  compositor-selected action. A source cannot put a menu into another application; a
  pre-drag menu is the only alternative and is worse than the modifier set every manager
  honours.

Hard links are this design's addition. They are files-only, same-volume and invisible to
external targets, so "Create Hard Link Here" appears only in the in-app and Windows-target
menus, disabled with the reason where the volume cannot hold one.

### Targets and gestures in the picker

A directory row, a breadcrumb segment, a sidebar place or drive and the empty area accept a
file drop; the empty area maps to the current folder. The row under the pointer highlights;
the cursor badge and the destination wording follow the resolved effect. A folder row, a
breadcrumb segment or a sidebar row opens after a one-second hover (Finder's spring-loading)
and Space opens the folder row immediately.

Every folder target takes the drag, because entering it must stay possible; the highlight
and the release then apply one rule: a drag whose sources already sit in the destination
folder, or that would drop a folder inside itself, lights nothing up and the release moves
nothing. Entering is navigation, so it ignores that rule — a drag taken up a crumb and back
down returns to the folder it started in. The release applies the rule to the paths a
foreign payload names, which is the only point their sources are known. A drop into a child
of the dragged folder is refused `Recursive` and never starts.

A right press on the empty area is the folder's own menu, as Explorer's background menu
is: the shell reads the folder itself, so "Properties" and the shell's rows act on the
folder rather than on whatever the list has marked, and the item's Properties panel
describes the folder on screen. A right press on a sidebar row is that row's folder's menu
on the same terms: Open, the shell's rows and Properties speak for the folder it names, not
for the list beside it.

A drag that rests within a band of the list's or the sidebar's top or bottom edge scrolls
that container, faster as the pointer closes on the edge; the list windows its rows from
the offset it asks for before it builds, so the frame shows them where they are about to
land, and reading the clock while it moves keeps the frames coming although the pointer is
still. The wheel works during the gesture too: on Windows a system drag runs its own modal
loop, so the drag's frame pump drains the wheel messages that loop leaves in the window's
queue instead of waiting for the turn after the drop.

A drag that starts on a row now carries the marked files, or the row under the press if it
is not marked. A sweep starts only from the empty area, the rule every manager in the
comparison follows. A drag that starts in the marked set and ends in the same list stays a
sweep, so the old gesture survives where it is useful.

### Engine additions

1. An effect set on `DragData` and modifier negotiation in the session; Win32 reads
   `grfKeyState`, Wayland negotiates actions instead of claiming Copy.
2. A drag started by the secondary button, with the threshold separating it from the menu
   tap; `IDropSource::QueryContinueDrag` accepts `MK_RBUTTON` and finishes on release, and
   Wayland passes the right-button serial.
3. A drop report with point, modifiers, button and effect, and a source-side `DropOutcome`.
4. A deferred `Ask` decision answered through `Host`, so Wayland can hold `finish`.
5. An in-app drag ghost. Native drags rasterize one; an in-app file drag without a ghost
   feels broken.
6. `text/uri-list` helpers and a `Files` clipboard payload with an effect.

The source side is closer than it looks: Win32 already maps a `text/uri-list` item to
`CF_HDROP` and back (`crates/ui-platform-win32/src/clipboard.rs:328`), and the Wayland
source offers whatever items the payload declares. A row payload plus the effect fields is
therefore most of the cross-application drag; what is new is negotiation, the secondary
button and the report.

## Menus and commands

### Row and background items

| Item | Selection | Notes |
| --- | --- | --- |
| Open, Open With | one or many | Open With is planned separately: it needs per-platform application discovery |
| Cut, Copy | one or many | writes `Files` with the matching effect |
| Paste | background and folder rows | disabled by the clipboard's content; worded "Paste *n* Items" |
| Move to, Copy to | one or many | opens a small folder chooser; the answer starts the job with the usual conflict question |
| Rename | exactly one | F2; disabled for multiple |
| Move to Trash | one or many | Delete; no confirmation |
| Delete Permanently | one or many | Shift+Delete; confirmation |
| Create Link | one or many | symbolic; disabled with the privilege reason on Windows |
| Create Hard Link | one or many files | this design's addition; disabled across volumes |
| Copy Path | one or many | plain text |
| Properties | one or many | a read-only panel of the selection's facts; the picker shows the item's name, type, location, size and modified time, and a count for many |
| New Folder | background | Ctrl+Shift+N |
| Select All, Invert Selection | both | |
| Show Hidden, Sort By, Refresh | background | joins the existing view controls |

Cut items dim until the paste succeeds or the clipboard is overwritten. The background
Paste targets the current folder; a file row has no Paste.

A row menu opens with Cut, Copy, Paste (a folder row), Rename and Move to Trash in one strip
of icon cells above the list, Win11's shape: the engine's `MenuItem::compact` declares a run of
cells, each `Menu::row_id` unchanged, so the same rows stay reachable by pointer, keys and their
chords. The list below keeps the less frequent verbs, and the permanent `Delete` stays out of the
strip because it is the destructive one.

### Accelerators

| Action | Chord |
| --- | --- |
| Cut, Copy, Paste | Ctrl+X, Ctrl+C, Ctrl+V |
| Rename | F2 |
| Trash, Permanent delete | Delete, Shift+Delete |
| New folder | Ctrl+Shift+N |
| Select all | Ctrl+A |
| Open context menu | Menu key, Shift+F10 |

The text editor already owns Ctrl+X/C/V/A while a caret is present, and the routing gives
the caret priority over app bindings. That stays: the row verbs are unreachable while the
rename or save-name field has focus, and no new exemption is needed.

### Rename

The row becomes an in-place field with the base name selected and the extension unselected,
the convention Nautilus computes explicitly and every other manager follows. Enter commits,
Escape cancels, moving focus elsewhere commits. Validation is platform-aware: `/` and NUL
are refused everywhere, Windows adds its reserved names and trailing dot or space, and a
collision is shown inline instead of replacing silently. The validator sits behind the
platform operator so the picker does not hardcode one system's rules.

### Delete and trash

Trash is the default; permanent delete needs Shift or the explicit row. Trashing is not
confirmed. Windows recycles through `IFileOperation` with `FOFX_RECYCLEONDELETE`; writing
`$I`/`$R` records by hand is how a Recycle Bin gets corrupted. Wayland follows the
freedesktop Trash Specification: the home trash under `$XDG_DATA_HOME/Trash`, a volume's
`.Trash/$uid` when it is sticky and not a symlink, otherwise `.Trash-$uid`; the
`.trashinfo` is created exclusively before the move; a volume with no usable trash falls
back to a copy into the home trash, which is also what a network mount gets.

### Engine additions

1. `open_menu(anchor, point, items)` and a `MenuClosed` report. Node-declared menus cover
   a right-click on a node, not a background menu, a drop menu or a keyboard invocation.
2. Submenus, separators, checked rows, disabled rows with a reason, and a default row drawn
   boldly, as Explorer's right-drag menu draws it.
3. The Menu key and Shift+F10, and a `Key` variant to carry it.
4. Shortcut ids for the file verbs, so a pack can rebind them.

The rows are declared rather than painted: opening a menu declares a platform panel, so a
submenu is a nested popup and a host that offers no surface paints the same tree in the window
([Popovers](./popovers.md#the-menu-is-a-panel)). `MenuStyle` still owns the look; the engine owns
which rows exist and which of them are enabled.

## Platform integration

### Hosting the native context menu

**Windows: the classic menu is readable; the modern one is not.** The classic shell menu is
built from `IContextMenu` objects, so it can be both hosted and read:

1. Build it with `SHCreateDefaultContextMenu` and a `DEFCONTEXTMENU`: one folder's
   `IShellFolder` plus the selected child PIDLs. A picker selection comes from one
   directory, so the single-folder limit costs nothing here; a selection spanning folders
   is the weak spot the API does not serve.
2. Fill it with `QueryContextMenu(hmenu, 0, idCmdFirst, idCmdLast, CMF_NORMAL |
   CMF_ITEMMENU)`. Add `CMF_SYNCCASCADEMENU` when the items are about to be read, so lazy
   submenus populate during the call, and `CMF_EXTENDEDVERBS` for Shift+right-click.
3. Either show the native popup with `TrackPopupMenuEx(TPM_RETURNCMD)` and invoke the
   result, or walk the `HMENU` with `GetMenuItemInfo` (text, id, state, check, submenu,
   bitmap) and draw the rows in our menu, invoking the same `IContextMenu` through
   `CMINVOKECOMMANDINFOEX` with the id minus `idCmdFirst`.

The second path is a lossy re-host, not a faithful import:

| What the classic menu carries | In our menu |
| --- | --- |
| text rows, separators, submenus, checked and disabled state | readable and reproducible |
| owner-drawn rows (`MFT_OWNERDRAW`, `HBMMENU_CALLBACK`) | no readable text; the handler paints them through `WM_DRAWITEM` |
| icons | HBITMAPs at best, often absent; per-verb icons are not a stable contract |
| lazily populated submenus | empty unless `CMF_SYNCCASCADEMENU` or `WM_INITMENUPOPUP` forwarding fills them |
| the standard verbs (Cut/Copy/Rename/Delete, Properties) | duplicated by our own rows unless filtered by canonical verb (`GetCommandString(GCS_VERBW)`) |
| third-party extensions | run in our process, with the stability risk that carries |

The object and its `HMENU` must stay alive from enumeration to invocation, and a handler
that assumes Explorer's site or view can fail outside it. Hosting the native popup instead
requires forwarding `WM_INITMENUPOPUP`, `WM_DRAWITEM`, `WM_MEASUREITEM` and `WM_MENUCHAR`
to `IContextMenu3`, and accepts the system menu theme: dark classic menus have no
supported API. The Windows 11 compact menu has
no documented hosting API: apps add commands through `IExplorerCommand` and package
identity, and modern-only entries do not appear in a classic host. `IExplorerCommandProvider`
is not the missing enumerator: it is a Vista-era provider interface an extension implements
so the shell can obtain that extension's own commands (`IEnumExplorerCommand` is documented
as the commands "to be put into the command bar"), and its methods take a site or a command
GUID, never a selection. It cannot answer "the menu for this `IShellItemArray`" because it
is never given one. Discovering third-party modern commands means reading
`windows.fileExplorerContextMenus` from package manifests and instantiating each CLSID as
an `IExplorerCommand`; that is unsupported as a hosting model, excludes Explorer's own
rows, and is what menu-manager tools do to toggle entries, not to draw a menu.

**Wayland: no.** There is no context-menu protocol and no portal for one. FileTransfer
moves files between sandboxed apps, Trash is fd-based and for sandboxed apps, and OpenURI
opens. Nautilus, Dolphin and Thunar extensions are in-process plugins of those file
managers and cannot populate another application's menu. A file manager's own verbs
(Compress, Share, cloud placeholders) are unavailable.

**Verdict.** The picker draws its own menu on both platforms. On Windows, a `ShellMenu`
capability offers **host** mode: the native classic menu behind one "More options" row, which
is exact behaviour and loses only our theming. Merging the readable rows into our menu was the
second mode in the plan; it is not built, because the read is lossy and a row it cannot carry
would be an action the user cannot reach. Wayland has no equivalent.

### Reusing native file operations

On Windows, `IFileOperation` provides native progress and conflict UI, recycle-bin
deletion and Explorer's undo record in one call. It also needs an STA thread, blocks until
done, shows its own dialogs (ownable, not embeddable), reports through a sink, and puts
undo in Explorer rather than here. It is a good trash primitive and a reasonable large-job
fallback later. As the primary engine it would hand the dialog's feedback to foreign
windows and split the semantics across two implementations.

On Linux, GIO is linkable but drags in GLib; KIO is C++/Qt with no stable C ABI and cannot
be linked. The portable engine plus the Trash Specification is the only design that
behaves the same on both targets.

**Verdict.** One engine, owned here, with the platform operator supplying the three things
`std::fs` cannot: symlinks, trash and name validation.

## Implementation sites

| Crate | Addition |
| --- | --- |
| `ui-core` | effect sets and negotiation hooks; richer `DroppedOn`; `DropOutcome`; `ClipboardContent::Files`; paste content; menu rows with separators, submenus and checks; Menu key |
| `ui-platform` | `file_operation` (request, plan, worker, `FileOperator` with `same_volume`, portable `FsOperator`); `uri_list`; the deferred-drop answer; programmatic menus and the menu renderer |
| `ui-platform-win32` | recycle via `IFileOperation`, `CreateSymbolicLinkW`, name validation; right-drag in `IDropSource`/`IDropTarget`; `CF_HDROP` with `Preferred DropEffect`; the classic shell menu, hosted or read |
| `ui-platform-wayland` | freedesktop trash, `std::os::unix::fs::symlink`; action negotiation and `ask` with deferred `finish`; both cut markers |
| `ui-picker` | row and background menus, drag payloads, drop targets, right-drag menu, rename field, cut dimming, footer progress and conflict panel, Fluent catalogs |
| `ui-widgets` | a larger progress presentation when the footer is not enough |

The layering test stays satisfied. The portable engine uses `std::fs`, which the boundary
already permits; everything needing `std::os` sits in a platform crate behind
`FileOperator`.

### Feedback in the dialog

The footer carries the operation, and the band above it names the files:

- While a job runs: "Copying 3 of 12 — 45%", the current name and a cancel button, over a
  band that lists the batch one card per source path, sorted, the item being worked on
  lifted. The band shows a few cards and scrolls inside them, so a batch of hundreds reads
  the same as one of three. One job runs at a time; later requests queue in order.
- A conflict suspends the band and opens a modal over the dialog, in the same in-dialog
  form as the save's Replace question: Replace / Skip / Keep Both / Rename, with Apply to
  All. `Enter` answers Replace and `Escape` cancels; the list waits behind the scrim.
- A finish with failures: "1 item could not be moved" with the names and reasons, in a
  small non-modal window of its own beside the dialog. Silent partial success is not
  acceptable.
- The confirmations that name a set — permanent delete and undo-copy — list it on the same
  cards, capped and scrolling: the sentence says how many, the cards say which ones.
- Cut rows dim and a successful paste clears the dimming.
- Rename reuses the inline field, not a dialog.

All strings go through the Fluent catalogs (`picker-operations`, `picker-conflict-*`,
`picker-operation-failed`, `picker-rename-invalid`, …), with plural selectors as the
existing item counts use them.

## Cut line

| Phase | Scope | Visible result |
| --- | --- | --- |
| A | `uri_list`, `ClipboardContent::Files`, cut markers, `FileOperations` and `FsOperator`, platform trash and symlinks | none; unit tests on temporary directories |
| B | row payloads, the sweep/row split, modifier negotiation, drop targets, in-app and OS move/copy, right-drag menu, Wayland `ask` | files move by drag |
| C | programmatic menus, row and background menus, shortcut ids, inline rename, cut/copy/paste, trash and permanent delete, progress and conflicts | the verbs are reachable from the mouse and the keyboard |
| D | undo journal, spring-loading beyond the fixed delay, Move to / Copy to, Compress, Properties, the Windows shell menu, `IFileOperation` for large jobs, portal transfer | parity with the managers above |

Not taken: a cut that survives our process exiting (the window owns the clipboard), a
transactional batch, and hard-link parity. Mature managers do not offer hard links from
the UI; this design does, as its own extension, not as a compatibility claim.

## Verification

- One test per rule that can destroy data: a move into a child is refused; copying a
  symlink recreates the link; a cross-volume move never removes the source before the
  destination exists; a cancelled copy leaves no `.part` file; a cut is consumed only
  after success.
- One test per encoding boundary: `text/uri-list` round-trips a non-ASCII path, a space
  and a `%`; a malformed line is reported, not ignored.
- A name a job is asked to create keeps its plain form until something holds it
  (`a_free_name_is_kept_and_a_taken_one_steps_aside`, in `ui-platform`).
- Worker tests follow the `Listings` pattern: a finished transfer forces a frame, and a
  transfer is not abandoned when its window closes.
- What only a person can judge — the drag ghost, the right-drag menu's position,
  hover-to-open timing, the footer wording — is checked in `picker_demo`, not asserted.

## Status

Phase A is in place: `ui-platform::uri_list`, `ClipboardContent::Files` and the unified
paste payload, the cut markers on both desktops, and `ui-platform::file_operation` with the
portable engine, the worker and the platform operators (Windows recycle bin and links,
freedesktop trash and links).

Phase B is in place: `DragData` carries an effect set and a preference, the session
negotiates with the held modifiers, Win32 and Wayland report a `DropOutcome` with the side
that performed the action, the secondary button arms a drag on either backend, and the
picker's rows and cells carry a `text/uri-list` payload with directory rows and the panel
as drop targets. A right drag opens the in-app decision menu (Move Here, Copy Here, Create
Link Here) at the release point, and the action runs through `FileOperations`. The picker
refreshes the touched folders when an operation finishes.

One deviation from the plan: the engine has no escalation from an in-app drag to a native
one, so a row drag is native from the threshold and does not keep sweeping the selection.
The sweep gesture the model keeps is still used by the empty area when it grows one; the
drop target under the pointer is highlighted instead.

The same-volume rule runs at the drop, in the picker, because the engine never sees the
paths a foreign payload carries until the drop reads them. Two consequences are known and
accepted: the pointer badge of a foreign drag says Copy even where the drop will move, and
only the picker applies the rule (another drop target of this app would have to ask for the
same policy explicitly).

Phase C is in place. `ui-core::menu` carries separators, submenus, checked rows, disabled
rows with a reason and a default row; `AppLoop::open_menu_at` and `Cx::open_menu` open the
app's own rows, a pick reports `Change::MenuItem` and a dismissal `MenuClosed`, and a
secondary tap on a node that declared no menu reports `Change::MenuRequested` with the press
point so the app builds its rows. The Menu key and Shift+F10 open the focused node's declared
menu on both backends and, when no node declares one, fall through to the app's bindings.

The picker binds row and background menus to `MenuCommand`s (cut, copy, paste, rename, trash,
permanent delete, new folder, symbolic and hard link with their refusal reasons, copy path,
select all, invert, show hidden, refresh) and reaches the row menu from the keyboard through
the Menu key. Inline rename opens the row's field with the base name selected, asks the
platform's `validate_name` and a collision check before it commits, and commits on Enter or
on focus loss. Cut, copy and paste go through `ClipboardContent::Files` and `Cx::request_paste`;
the clipboard's answer arrives as `Change::Paste` on the target `run_builtin` remembers. A cut
dims its rows, and the paste consumes only the sources that succeeded.

Drops no longer start with `KeepBoth`: a job runs with `ConflictPolicy::Ask`, a collision
suspends the footer band and opens the conflict question over the dialog (Replace / Skip /
Keep Both / Rename, with Apply to All), and a finish with failures opens the failure window,
a modeless window of its own (a popup under the toolbar only where the platform has no
window surfaces). The footer carries the job's kind, item count, percent, current name and
a cancel button; the band above it lists the batch as cards, sorted by path, the current item
lifted and the rest scrolling, and the permanent-delete and undo questions list their own set
the same way.

Known deferrals: a menu's Paste row is enabled whenever the service exists, because the engine
has no clipboard-content listener to dim it or to say "Paste 3 items"; Open With, Sort By and a
"New" submenu are not built; the in-place field can only rename a row the listing already shows,
so a folder that was just created takes its field one refresh later; a create-folder failure
reuses the failure list, because folder creation is not a worker job. The Move to / Copy to
chooser is a navigator only: it has no New Folder row and no typed path, so a destination that
does not exist yet has to be made in the list behind it.

Phase D is in place for every item that does not need a missing platform dependency. `UndoJournal` records the inverse of every finished job — a move back
to its sources, a rename back to its old name, and a copy's created files, which the trash
removes after a confirmation — and `Ctrl+Z` or the background menu's Undo row runs it; a delete
records nothing, because its inverse is not knowable. Spring-loading is an app timer on the frame
clock: a drag at rest on a folder row, a breadcrumb segment or a sidebar row navigates into it
after `SPRING_DELAY` (one second), and Space answers immediately. Alt+Enter or the row menu's Properties row opens a small non-modal window beside the dialog with the type,
location, size and modification time, or the count and summed size of a selection, captured when
it opened. The row menu's Move to and Copy to rows open a folder chooser instead: it browses with
the dialog's own listing service — the rows are the subfolders the crumb menus read, and the
places and mounted volumes outside the current folder are jumps to another root — and its answer
lands the selection in the folder on show through the same request a drop builds, so the
conflict, failure, undo and refresh paths are the ones every job already has. On Windows the row
menu ends with "More Options…", which hosts the native classic shell menu: the command asks the
build for it, and the host runs `ShellMenuHost::show_native` once that frame has presented, at the
point the menu opened. When the host can read any of the menu, its rows also join the row menu
under a "Shell Menu" submenu: the right press asks the host to read them on its own thread, the
menu opens once the rows are in (or after 200 ms without an answer), and when the answer lands
later it replaces the open menu's rows where it stands — the first press on a file type no
handler has loaded yet must not be the one press without the merge. A merged row runs by the
command id the read reported. A row the shell draws itself is left out and
counted; "More Options…" stays, so the native popup still reaches it, and the submenu says the
rest is there.

Not built in Phase D, with the reason each waits: Compress needs an archive writer, and on
Windows the shell menu offers the system's own; `IFileOperation` as a large-job fallback and the
Wayland `FileTransfer` portal both wait for their platform dependency. The shell menu's rows merge
with one row of ours beside them: a row the shell draws itself is counted and left out rather
than refusing the merge, so a slow or partly unreadable menu still contributes what it can — see
the correction on that decision below.

## Corrections

**A drop target stands for a change, while entering it is navigation.** The first picker took
any drop a row or the background offered and let the planning rules answer: a Copy onto the
folder the sources already sat in duplicated them, and a drag onto its own folder showed a
highlight and then did nothing. The first fix refused such targets the drop altogether, which
also refused them the drag, so a drag that went up a breadcrumb could not come back into the
folder it had started in. Every folder target now takes the drag, and one rule, `moves_into`,
decides both the wash and the release: a source whose parent is the destination, or a
destination inside the source, lights nothing up and the release filters the decoded payload
through the same function — the only point a foreign drag's sources are known. The hover that
springs ignores the rule, because entering is not a drop. Breadcrumbs and the sidebar's places
and drives are targets on the same terms, and the hold was raised from half a second to one
second because the old delay opened folders under a pointer still on its way elsewhere. A right
press on the empty area reads the current folder into the shell menu and describes that folder
in Properties. Caught by `a_drop_needs_a_change_but_an_entry_is_navigation` for the rule, the
crumb destination and the entry; the sidebar targets and the one-second hold were checked by
hand in `picker_demo`, since no test drives a foreign payload.

**A right press belongs to the item, not to the pixel under it.** The gesture reported the
innermost node that consumed the pointer, and a picture is a consumer where a glyph's text is
not: a press on a grid cell's picture reached the dialog with the mark's id, matched no row,
and opened the background's menu, and a sidebar row matched nothing and had no subject of its
own. The mark is now declared pass-through, so the press lands on the row or the cell, and the
press's id resolves to one menu target — a list row, the folder behind the background, or a
sidebar row's folder — which decides the menu's rows, the shell read and the subject of
Properties. Found by the report that every grid item showed the background's menu; checked by
hand in `picker_demo` in both layouts, since no test walks the hit index. The background's
menu also never reached the shell at all: only the row branch merged the shell's rows and
offered "More Options…", so the folder's own menu had no shell rows and no native door. The
merge now runs for every subject, and a drive root — which has no parent path to prove a
shared folder — is read as the one folder it is.

**A wheel during a system drag waited for the drop.** A drag the dialog itself carries runs
`DoDragDrop` on its thread, so the window procedure's `Event::Scroll` sat in the window's
queue until the gesture ended and the next turn drained it; the wheel could not move the list
the drag was over. The drag's frame pump — already the only thing running inside that loop —
now takes the queued wheel events, dispatches them and asks for the frame that shows the new
offset. Found by the report that the wheel did nothing while dragging; the change is on the
pump path alone, and a hand check in `picker_demo` is what confirms that OLE delivers the
wheel to this window inside its loop, which no test can drive.

**A shell command that opens a window of its own waited for the next right press.** The
properties sheet is shown from a message the shell posts to the thread that invoked the
command. The worker waited on a condvar, so that message sat in its queue and the sheet never
appeared; the next right press ran `QueryContextMenu` on the same thread and pumped messages
as a side effect, which is why the dialog then showed and the command looked deferred. The
commands that looked immediate — "创建快捷方式" — open no window, which is what put the fault
in the thread rather than in the invoke. The worker now waits with `MsgWaitForMultipleObjectsEx`
on a thread message and dispatches what arrives through `window::pump_messages`, which is the
pump an STA owes regardless. Found by the reported pair of items and by a run where
`InvokeCommand` returned `S_OK` and the invocation counter moved while no dialog existed;
verified by hand in `picker_demo` (right press → 系统菜单 → 属性 shows the sheet without a
second press), since no test drives the real property sheet.

**A finished shell command showed nothing until the next right press.** The worker woke the
surface by posting `WM_NULL`. A message with no event only ends the loop's wait: the driver
finds no event, no animation and no silent change, so it sleeps again without building a
frame, and `poll_shell_invocations` — which runs in the build — never saw the counter move.
The command had run, but the folder was never re-read, so the click looked like it did
nothing; the next right press built a menu, polled the counter and showed the result, which
made the command look deferred to it. The worker now posts `WAKE_MESSAGE` (`WM_APP + 0x52`),
which the window procedure turns into `silent_change`; a late read answer had the same hole
and now arrives on the frame the message itself owes. Caught by the picker's report that a
merged row's effect appeared on the next right press, and by reading `Driver::decide`: with
no event and no `silent_change` it returns `Turn::Idle`, so the posted message never owed the
frame that polls the counter.

**Move to / Copy to was rejected as "a second chooser" and is not one.** The deferral argued
that the verb nests a picker inside the dialog, and that a caller can already open the picker
to pick a destination. The user inside the dialog cannot: other than a drag to another window,
there was no way to name a destination folder at all. The chooser that shipped is a question
panel with a scrolled list: it reads subfolders through the same `ListingSource::get` and
`ListingSource::invalidate` the crumbs use, draws the same jump rows, and starts the job
through `start_operation`, the drop's own entry into `FileOperations`. It adds no second
listing service, no second conflict panel and no second progress presentation.

**A row drag is native from the threshold, so it no longer sweeps.** The plan had the row
payload and the sweep share one gesture, assuming the engine could escalate an in-app drag
to a native one once the pointer left the window. No escalation exists, and the choice was
between a drag that cannot leave the dialog and a second gesture. The native drag won,
because taking files to another application is the reason the payload exists; the empty
area is where the sweep returns.

**The same-volume rule runs in the picker at the drop, not in the engine.** The engine's
negotiation has no volume information, and OLE reveals a foreign payload's paths only when
the drop reads them. The engine therefore normalizes a drop report to the source's
permitted set and marks the drag `foreign`, and the target that owns both paths and volumes
answers. The pointer badge still follows the engine, so a foreign same-volume drag previews
Copy while the drop moves.

**Wayland's `ask` is not offered; the decision menu is the app's own.** The plan had the
Wayland source hold `finish` while the user answered a question through the host. A
destination-side menu needs the release point and the payload, which both arrive before
`finish`, but the protocol has no right drag and the compositor has already committed the
action by then. The picker draws its menu where the release landed instead, and Wayland
inbound drags — always primary — perform the action the engine negotiated.

**A drop's collision asks instead of keeping both.** Phase B started every job with
`ConflictPolicy::KeepBoth` so that a drop would never stop on a question the dialog could
not draw; the result was a silent second copy under a new name. The worker now reports the
collision and holds the item until `answer` arrives, and the picker draws the panel the save
overwrite question already uses. The cut paste also learned to own its success: `consume_cut`
matches the report's id to the job the paste started, so an unrelated finish cannot clear
the dimming. The worker's half is `the_worker_delivers_a_conflict_and_accepts_the_answer`;
the panel itself is judged in `picker_demo`.

**The app hears a menu request on the tap's release, not on the press.** The first
programmatic request fired at the press, which is before the gesture can be told from a right
drag; a row drag with a payload would open a menu and then drop. `SecondaryPress` now carries
the node to ask, and the release makes the same decision for the declared and the app-owned
paths: a travelled drag reports its drop and opens nothing.
`a_secondary_press_without_a_declared_menu_asks_the_app_for_one` pins both halves.

**The Menu key falls through when no node declares a menu.** The engine consumed the key for
its own menu and stopped there, so an app that builds its rows, as the picker does, could
never hear it. The key now tries the declared menus up the focus chain and then continues to
the app's bindings, the same division of labour a secondary tap uses. Wayland gained the
other half of the key: Shift+F10 names `Key::Menu` as Win32 already did, and F1–F24 map to
`Key::Function`, without which the F2 rename chord never arrived on that backend.
`the_menu_key_and_shift_f10_open_the_focused_nodes_declared_menu` pins the declared path; the
picker's own path is checked in `picker_demo`.

**A paste answer is a change, not a return value.** The old built-in edited the focused
control and discarded everything else. The engine now remembers the target `run_builtin`
would have used (the focused node, else the root), reports the clipboard's content there as
`Change::Paste`, and keeps a text editor's own paste ahead of it. `Cx::request_paste` starts
the same read from a build, which is how the menu's Paste row works without a keyboard.
`a_paste_without_an_editor_reaches_the_app_and_a_text_paste_still_edits` and
`a_cx_paste_request_answers_on_the_focused_target` cover the two routes.

**Undo is a journal of inverses, applied only after a report succeeds.** The first sketch kept
the request and re-ran its opposite; a move's sources are gone by then, and a half-finished job
has no clean opposite. `UndoAction` is built from the finished report's `succeeded` outcomes —
`MoveBack` pairs each destination with its source, `RenameBack` keeps the old name, and
`RemoveCopy` lists what the copy created. The picker applies the last entry only when the undo
job's own report has no failures, and it confirms a copy's undo first, because removing what a
copy created has no inverse of its own. Deletes record nothing. The inverse construction is
`undo_of_a_move_puts_back_what_moved` and
`a_rename_and_a_copy_are_undoable_but_a_delete_is_not`.

**Spring-loading is an app timer, not an engine gesture state.** The hold must outlast the
pointer's silence, and the engine has no timer that runs while nothing moves; `cx.now()` is the
frame clock, and reading it records `Dep::Now`, which is also what keeps frames coming during
the hold. The picker stamps the entry time on the first frame after `DragEntered` and opens the
folder when the clock passes `SPRING_DELAY`; Space opens it at once. The clock semantics are
what `a_drag_resting_on_a_folder_opens_it_after_the_hold` drives, with an injected time.

**The shell menu is hosted, never merged.** Reading a classic menu's rows loses owner-drawn
rows without text, icons, and submenus that only fill on `WM_INITMENUPOPUP`, and each lost row
is an action the user cannot reach. The picker therefore offers one row, "More Options…", and
hands the selection to the native popup. `ShellMenuHost::show_native` takes the window's
logical client point — the app never holds a screen point — and the Win32 host converts it with
`ClientToScreen` at the window's own DPI, which is why it holds an owner `HWND` rather than
recomputing one. The token contract between `rows` and `invoke` says a menu whose command ids
shift between reads is out of contract; hosting keeps the `IContextMenu` alive for the whole
popup and invokes the choice with the same object it was read from.

**The shell's rows do merge, and a row that cannot be carried is left out, not fatal.** The
entry above superseded itself once files-community/Files was read: Files builds the same classic
menu (`IShellFolder.GetUIObjectOf(IID_IContextMenu)`, `QueryContextMenu`, recursive
`GetMenuItemInfo`, `hbmpItem` icons, `IContextMenu2::HandleMenuMsg(WM_INITMENUPOPUP)` for lazy
cascades) and merges its rows into its own flyout, filtering the verbs it implements natively.
The first merge here kept the old all-or-nothing read and answered `None` for the whole menu when
any row could not be carried. That was wrong for the same reason the refusal existed: the action
was never unreachable, because "More Options…" and its native popup stay in the menu, so refusing
the merge only threw away the rows that could be carried — which is most of them. The read now
returns `ShellMenuRead { rows, omitted }`: owner-drawn rows, callback bitmaps, empty cascades, an
id outside the range or an empty label are counted and skipped, and the rest merge. The rows join
under one "Shell Menu" submenu rather than being flattened, which is where Files' filter list
would otherwise be needed, and when `omitted` is not zero the submenu ends with a disabled note
saying those items are only in the system menu. Read and commands run on a dedicated STA worker
with a message pump (the `shell-menu` thread, which folds a still-queued read into the newest
one and waits with `MsgWaitForMultipleObjectsEx` so a posted message — the properties sheet is
one — is dispatched), because `QueryContextMenu` and `InvokeCommand` execute third-party
in-process code; the
worker wakes the surface with `WAKE_MESSAGE` (`WM_APP + 0x52`), which the window procedure
turns into the same "a frame is owed" demand a focus change makes; the frame polls `take_rows`
and `invocations`, and a right press waits at most `MENU_READ_WAIT` before the menu opens
without merged rows. The
clock read that keeps those frames coming is the same `Dep::Now` the spring-loading hold uses.
Files' current `main` goes further and moves the work into an out-of-process
`Files.App.Server` with a bounded STA worker pool, which is where this goes if a hanging
extension ever matters in practice. What catches it now:
`the_shells_rows_join_the_menu_and_run_by_token`, `an_unreadable_shell_menu_opens_with_the_native_row`,
`an_owner_drawn_row_is_omitted_and_the_rest_still_merges` and `an_empty_cascade_is_omitted`.

**The shell menu runs from the host's turn, not inside the build.** The first implementation called
`ShellMenuHost::show_native` straight from the menu row's command, which the engine runs in
`App::changed` inside `AppLoop::frame`. `TrackPopupMenuEx` keeps the thread in the shell's own modal
loop until the user dismisses the menu, so that frame never painted with the engine's menu closed,
the host never reconciled, and the app had no turn left to answer the next input. The fix is the
engine's existing post-frame pattern for `NativeDrag`: the picker's command records the request,
the build hands it over with `Cx::request_shell_menu`, and the Win32 presenter runs
`show_native` at the start of the next turn — the host reconciles the engine's panels at the end
of a step, so only then is the menu the user picked from off the screen — on the requesting
surface's own `HWND` (`Win32ShellMenu::for_window`, not the slot that follows the last opened
surface), between `modal::arm` and `modal::disarm`, so every other surface keeps drawing from the
timer. A shown menu requests one frame, which is also where the picker re-reads the folder. What
catches it now: `a_shell_menu_request_leaves_for_the_host_after_the_frame` and
`the_shell_menu_is_handed_to_the_host_and_the_folder_read_again`.

**The shell object outlives both the apartment and the id it was built from.** With the popup
moved out of the build, closing it still froze the process once `TrackPopupMenuEx` returned. The
Win32 builder entered a COM apartment with a guard scoped to `build`, then handed `IContextMenu`
and a child PIDL back out of it, so the popup, the invoke and the release all ran after
`CoUninitialize`. `SHBindToParent`'s child also points into the absolute PIDL it was given rather
than owning memory, yet the absolute was freed before the menu used the child and the child was
freed again afterwards. The menu painted from freed memory — which is why it looked fine — and the
second free corrupted the heap, so the release inside `BuiltMenu::drop`, after the menu closed,
blocked in the allocator. The fix is a lifetime, not a guard: `BuiltMenu` holds the apartment and
the absolute PIDLs and drops them after the context and the menu, and the child is never freed.
A scripted reproduction (`picker_demo`, request a shell menu from the build, close it) hangs in the
PIDL free before this change and completes the turn after it; the shell popup is not a unit test,
because it needs the real shell and a display.

**Properties snapshots its facts when the panel opens.** A panel that re-read the selection
would describe one item while the list behind it points at another; it captures the name,
type, folder location, summed size and modification time at the click, and Escape, Enter or
its Close button dismisses it. It reports and does not question, so it is a non-modal window
beside the dialog: the verbs keep working while it stands. What only a person can judge — the
two-column layout and plural counts — belongs to `picker_demo`, not a test.

**The conflict and failure reports were footer popups.** Both were
`Presentation::Platform` panels anchored above the accept button: the conflict question sat in
the bottom-right corner, out of the way of the list but where no other toolkit puts a
question, and its `Enter` resolved against the dialog's own binding instead of answering it.
The questions are in-dialog modals now — a scrim, a centred surface, `Enter` for the default
answer and `Escape` to cancel — and the failures window became a modeless system window of
its own, which is what Properties had already become. [File
Picker](./file-picker.md#questions-and-reading-windows) owns the design;
`enter_answers_a_question_and_other_keys_wait` pins the routing.

## See also

- [File Picker](./file-picker.md): the dialog these verbs act on.
- [Pointer Model](./pointer-model.md): drag sessions, the secondary button and the tap rule.
- [Keyboard Shortcuts](./shortcuts.md): bindings and the text editor's priority.
- [Popovers and Tooltips](./popovers.md): the overlay pattern the conflict panel follows.
- [Dialogs](./dialogs.md): the window a block of progress may need.
- [Platform Boundary](../platform-boundary.md): where the operator implementations live.
