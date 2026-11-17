# File Icons

A picker draws a row for every item before any thumbnail exists: folders, drives,
symlinks, and every type the desktop's thumbnailers do not claim. The icon engine draws
one glyph from a font family, which covers the vocabulary but cannot show the desktop's
own type icon. This document fixes what a row shows, in which order, and where the
picture comes from on each platform.

## Problem

Three questions, in order:

1. What kind of item is this? A folder, a file, a symbolic link to either, a hard link,
   a mount point. The answer decides the glyph and whether a thumbnail may be asked for.
2. Which picture fills the slot? The platform's type icon, the framework's glyph, or a
   thumbnail that replaces either.
3. Who decides whether thumbnails are on at all? Never the module: it answers what it is
   asked, and the picker decides what to ask.

The icon engine has no raster channel today (`IconSource` is a `Glyph` or an
app-drawn `Custom` declaration), and reading a desktop icon theme is not cheap: the
themes on this machine ship their MIME icons almost entirely as SVG. See *Implementation
sites*.

## Comparison

| Framework | What a row starts with | When it becomes a picture | Where the icon comes from |
| --- | --- | --- | --- |
| Nautilus | the file's themed icon, always resolved first | `nautilus_file_should_show_thumbnail()` and the view's image widget swap in the thumbnail when eligible; the icon stays as the fallback | gio's themed `GIcon` for the content type, looked up in the GTK icon theme |
| Dolphin | the MIME icon from `KFileItem::iconName()` | `KFileItemModelRolesUpdater` puts a preview into `iconPixmap` instead, per view setting | KIconLoader over the icon theme |
| Windows shell | the item's own icon — embedded in an executable, named by a folder's `desktop.ini`, resolved through a shortcut — plus the overlay the shell badges it with | `IShellItemImageFactory::GetImage` without `SIIGBF_ICONONLY` | `SHGetFileInfoW` for the overlay slot, the shell image factory with `SIIGBF_ICONONLY` for the pixels |
| macOS | the type icon from Launch Services | Quick Look replaces it once a preview exists | `NSWorkspace.icon(for: UTType)` / `icon(forFile:)` |

> **Sources.** Nautilus: `nautilus_file_should_show_thumbnail()` in `src/nautilus-file.c`
> reads the `show-image-thumbnails` GSettings enum (`always`, `local-only`, `never`),
> checks the MIME type's size limit and the existing cache, and
> `NautilusNameCell.update_icon()` keeps `nautilus_file_get_icon_paintable()` as the
> fallback. Unresolved themed icons fall back to `application-x-generic` /
> `text-x-generic`, then to a bundled resource. Dolphin: `KFileItemModelRolesUpdater`
> fills the `iconPixmap` role with a preview or the MIME icon; `KIO::PreviewJob` reads
> the enabled plugin list from the `PreviewSettings` KConfig group, with
> `MaximumSize` / `MaximumRemoteSize` and an `isSlow()` choice between them. KIO's
> merge request !1651 (2024) routes MIME types no KIO plugin claims to the freedesktop
> `.thumbnailer` programs, exposed as individually toggleable entries that start off.

The shared shape: **the type icon is the base layer and a thumbnail is a conditional
replacement on top, never a retry after the icon failed.** An unavailable thumbnail is
cached (the freedesktop cache's `fail/` directory, Nautilus' `THUMBNAILING_FAILED`), so a
row does not ask
again every frame. Symbolic links keep the target's icon and add a badge; neither file
manager changes the type decision for them.

## Contract

`ui-platform/src/files.rs` carries the vocabulary:

| Type | Meaning |
| --- | --- |
| `FileKind` | `Folder`, `Drive`, `Document`, `Spreadsheet`, `Presentation`, `Pdf`, `Text`, `Code`, `Image`, `Video`, `Audio`, `Archive`, `Generic` |
| `FileKind::of_path` | the kind a name implies; a directory's name is never consulted, because only a caller that stat'ed it knows |
| `FileKind::of_mime` | the same fold from a content type |
| `mime_type` | extension to MIME type, one table shared with the Wayland thumbnailer lookup |

One table, not two: the Wayland provider needs the MIME string to match `.thumbnailer`
`MimeType` lines, and the label needs the category. A second table would drift, and the
two disagreeing is a class of bug no test would name.

`ui-platform/src/file_icons.rs` carries the platform half:

```rust
pub struct FileIconRequest {
    pub path: PathBuf,
    pub kind: FileKind,
    pub link: bool,
    pub size: u32,
}
pub enum BadgeCorner { TopLeft, TopRight, BottomLeft, BottomRight }
pub struct FileBadge { pub picture: ThumbnailPicture, pub corner: BadgeCorner }
pub struct FileIcon {
    pub identity: u64,
    pub base: ThumbnailPicture,
    pub badge: Option<FileBadge>,
}
pub trait FileIconProvider: Send + Sync {
    fn icon(&self, request: &FileIconRequest) -> Option<FileIcon>;
}
pub enum FileIconState {
    Loading,
    Ready(ImageEntry),
    Absent,
}
impl FileIcons {
    pub fn new(provider: Box<dyn FileIconProvider>, waker: Arc<dyn Waker>) -> Self;
    pub fn get(&self, request: &FileIconRequest) -> FileIconState;
    pub fn completed(&self) -> u64;
}
```

Invariants:

- **Per item, not per kind.** The request names the path — an executable's embedded icon, a
  folder's `desktop.ini`, a shortcut's target and a shell overlay are facts about the item —
  plus its kind as the fallback, whether the item is a link, and the pixel size. On Windows a
  shell query answers the item; on a freedesktop desktop the executable bit is a permission
  rather than a name, so the path is read there too. The kind is what every platform can
  answer when the item itself cannot be.
- **Off the frame thread, memoised.** The first version ran the lookup inside the build, on the
  theory that a theme read or a shell query about one item costs 1–10 ms. Measured on
  `System32`, a DLL's query costs 1.7–26 ms (most of them 8–18 ms), and a list scrolling fast
  reveals twenty to forty rows per frame: 200–600 ms of the frame thread spent asking the
  shell, which the user feels as the picker freezing while the scrollbar moves. The lookup now
  runs on the worker `ui-platform/src/pictures.rs` shares with thumbnails; `get` answers from
  memory or queues the request and answers `Loading`. The `Loading` frame draws the framework's
  glyph, which is the base of the chain rather than a flicker — the same frame a thumbnail wait
  draws the icon in. A finished request bumps `completed`, and `Host::turn` forces the frame
  that shows it, exactly as for a thumbnail.
- **`Absent` is cached** like a picture: an item without an icon is not re-asked per frame, and
  `clear` is what asks again.
- **Identity names the picture, not the item.** Two answers that share an identity and
  dimensions share one upload, so a folder of one type costs one icon. A provider derives it
  from what it knows names the pixels: a theme file's path, or the pixels' own bytes where the
  platform's bookkeeping can disagree with them.
- **One `Arc` for the life of an entry**, so the renderer uploads once. The id is derived
  from the picture's identity and size under the prefix `0x6963_6f6e_0000_0000`, which never
  meets the thumbnail or app counter ranges; both feed one texture table. A picture made
  again carries the id it had before, and the engine draws its new pixels through the same
  entry instead of appending a second.
- **Two tables, bounded differently.** Answers are keyed by request, cheap, and count-bounded
  (4096). Pictures are keyed by identity and size, own the pixels, and are byte-bounded
  (256 entries / 16 MiB); the eldest use of either is evicted. A request whose picture was
  evicted is asked again, so an answer never outlives its pixels.
- **A badge is the provider's, its corner is the platform's convention.** The cache draws it
  at the badge's own pixel size, flush with the named corner, cropping what overflows; scaling
  it would blur the only pixels the platform rendered.

`Backend::file_icons()` answers `None` by default, so a platform without icons loses
nothing; `Host::file_icons()` hands the app the same memoised handle, and a finished request
forces one frame on every surface.

### What a row draws

```
thumbnail Ready ─────────────────▶ the picture
thumbnail Loading / Unavailable ─▶ platform icon Ready ──────────▶ the picture
                                   platform icon Loading / Absent ▶ framework glyph
```

The framework glyph is the base of the chain, drawn while the platform's answer is still on
the worker. The platform icon is the base with the platform's badge already composed into it.
The base is never empty: every `FileKind` maps to a Material Symbols name present in the
generated table, which is what makes the chain total on every platform.

Eligibility is the picker's, not the module's: it checks the setting (`never`,
`local-only`, `always`, mirroring Nautilus), the file size against a limit (from
`FileVersion::len`), and whether the type has a thumbnailer. A row outside the policy
never sends a request, so disabled thumbnails cost nothing.

### Items

| Item | Kind | Picture | Notes |
| --- | --- | --- | --- |
| file | by name | thumbnail or icon | an executable's embedded icon on Windows |
| folder | `Folder` | icon, or a directory preview (later) | a folder's `desktop.ini` names its icon on Windows |
| mount point / drive | `Drive` | glyph | locality needs `statfs`/mountinfo, which is a platform crate's job |
| symbolic link | target's kind | target's picture plus a link badge | the shell's overlay slot on Windows, `emblem-symbolic-link` on a freedesktop desktop |
| hard link | file's kind | same as a file | nlink is not shown; two names, one inode |
| junction / reparse point (Win) | target's kind | target's picture plus the shell's badge | the shell answers both; `is_symlink` does not see a junction, so the badge comes from the shell, not from `link` |

## Implementation sites

| Where | What |
| --- | --- |
| `ui/crates/ui-platform/src/files.rs` | `FileKind`, the shared MIME table |
| `ui/crates/ui-platform/src/file_icons.rs` | provider trait, memoised handle, badge composition, bounds |
| `ui/crates/ui-platform/src/pictures.rs` | the worker, the newest-first queue, the answer/picture tables every picture service shares |
| `ui/crates/ui-platform/src/host.rs` | `Backend::file_icons`, `Host::file_icons`, the turn that asks for a frame |
| `ui/crates/ui-widgets/src/file_icon.rs` | `FileKind` to Material Symbols |
| `ui/crates/ui-platform-wayland/src/thumbnails.rs` | uses the shared MIME table for `.thumbnailer` matching |
| `ui/crates/ui-platform-win32/src/shell.rs` | the per-item shell icon, its overlay slot, the kind fallback, and the `HICON` reader the thumbnails share |
| `ui/crates/ui-icon-theme/src/icon_theme.rs` | the freedesktop reader: selection, `index.theme`, ranking, SVG and PNG |

### Linux and SVG

A freedesktop icon theme stores MIME icons as SVG on this machine: `breeze` has zero
PNGs (`mimetypes/32/text-x-generic.svg`), and `Adwaita` has a PNG only at 16×16 with
`scalable/` and `symbolic/` in SVG. A PNG-only reader therefore gets nothing from Breeze
and a 16px icon from Adwaita, so the reader renders SVG with resvg.

`ui-icon-theme` is its own crate, and `ui-platform-wayland` depends on it only behind the
`file-icons` feature, off by default (`ui-host` forwards it): an app with no picker never
links the renderer. The reader:

- picks the theme the desktop names — `kdeglobals` when `XDG_CURRENT_DESKTOP` says KDE,
  else `gtk-4.0`/`gtk-3.0` `settings.ini`'s `gtk-icon-theme-name`, else `kdeglobals`, else
  `hicolor`;
- reads `index.theme`: `Directories` and `ScaledDirectories` with each section's `Size`,
  and `Inherits` walked once with repeats dropped, ending at `hicolor`;
- asks for the standard name per kind (`application-pdf`, `x-office-document`, …) and
  ranks candidates any SVG first, then the smallest PNG at least as large as asked, then
  the largest below it;
- reads the executable bit before the kind, because the desktop has a more specific icon
  than any MIME family: `application-x-executable`, then the script icon, then `text-x-generic`;
- draws `emblem-symbolic-link` as a bottom-right badge on a link, which is the themed
  desktop's answer to the shell's overlay; the corner is the file manager's convention and is
  one constant to change (`BadgeCorner`);
- names the composed picture by the theme files that produced it, so every row of one type
  shares one upload;
- decodes PNG through `png` with `EXPAND`, so palette icons with `tRNS` work, and renders
  SVG at the requested size; a pixmap's premultiplied bytes become the straight alpha the
  picture type promises.

Limits: the chain is resolved once per process, so a theme switched mid-run is not noticed
until restart; the selection reads configuration files rather than GSettings or D-Bus;
`.xpm`, which the specification also allows, is not read.

### Windows and macOS

`ui-platform-win32` asks the shell about the item itself, in two parts.

The pixels come from `IShellItemImageFactory::GetImage` with
`SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK` at the requested size. The
factory is the shell's own item renderer, so it answers an executable's embedded icon, a
shortcut's resolved target and a folder's `desktop.ini` icon, and it renders at any size —
the grid asks 80 pixels. Verified here with a `desktop.ini` naming `notepad.exe,0`: the
folder's picture differs from a plain folder's at both 20 and 80.

The badge comes from `SHGetFileInfoW` with `SHGFI_SYSICONINDEX | SHGFI_SMALLICON |
SHGFI_ICON | SHGFI_OVERLAYINDEX`: the high byte of `iIcon` is the overlay slot, and
`IImageList::GetOverlayImage` turns that slot into an index in the same list, whose
`GetIcon` is decoded. The list is the one nearest the base size with a preference for one at
or below it, and its real size is read with `GetIconSize` — the system lists are DPI-scaled,
so the small list holds 20-pixel pictures at 125%, and a badge larger than the base would be
cropped by the composition rather than seen. A slot of 0 means no badge; the shell puts its
overlay in the bottom-left of its icon views, so that is the corner. The `HICON` this call
allocates is destroyed unread — only the slot is taken from it.

The kind fallback, for an item the shell refuses (a vanished file, a disconnected share),
is `SHGetFileInfoW` with `SHGFI_USEFILEATTRIBUTES` on a stand-in name (`file.docx`,
`file.xlsx`, `file.pdf`, …): a folder is answered by the directory attribute, and a drive —
which has no extension to stand for it — by the system volume's root path. That call asks
for `SHGFI_ICON | SHGFI_LARGEICON` and decodes the `HICON` directly.

The `HICON` becomes pixels through `GetIconInfo` and the same 32-bit `GetDIBits` conversion
the thumbnails use; when a legacy icon carries no alpha channel, its AND mask supplies the
transparency. The image factory is a COM object and refuses to be created on a thread with no
apartment (`0x800401F0`), so the provider opens one per thread on first use, as the thumbnail
worker does.

An item's identity is the hash of its pixels and overlay slot, not its image-list index. The
index and the pixels come from separate shell queries and can disagree: a folder whose
`desktop.ini` names an icon reports the plain folder index until the shell has read the file,
while the factory may already return the custom icon. Hashing what was actually handed over
keeps the share key equal to the picture it names.

Everything above runs on the icon worker, in the apartment that thread opens, and none of it
reads file contents or starts a process. macOS answers `NSWorkspace.icon(for: UTType)`
without touching the file, and `icon(forFile:)` per item; the `NSImage` becomes a
`CGImage` for pixels, and no provider exists yet. A provider with a thread preference of its
own has to marshal to it, because the worker is not the frame thread any more; `NSWorkspace`
is commonly used off the main thread, but the first macOS provider has to check that rather
than inherit a claim this page cannot make `[unverified]`.

### Directory previews

Dolphin's `directorythumbnail` paints a 2×2 set of child thumbnails over the folder
icon. The same shape fits a framework provider — it is a thumbnail, so it belongs behind
the thumbnail service, not here — but it is deferred until the picker needs it.

## Verification

Pure functions and the cache, so no test needs a theme, a display or the network:

| Test | What it would catch |
| --- | --- |
| `a_name_yields_its_kind_and_an_unknown_one_is_generic` | the table stops reaching the kind vocabulary, folds case, or guesses at an unknown name instead of answering `Generic` |
| `answers_with_one_identity_share_one_upload` | two items the provider identifies as one picture upload it twice, or the provider is asked once for two items |
| `one_item_is_asked_once_even_when_it_has_no_icon` | an item without an icon is re-queried per frame |
| `one_identity_at_two_sizes_keeps_two_pictures` | the table keys pictures by identity alone, so the grid's 80-pixel cell draws the row's 20-pixel picture |
| `a_remade_picture_keeps_its_id` | a picture made again after eviction mints a new id, so the engine's table grows by one picture per cache cycle |
| `an_icon_is_asked_for_off_the_frame_thread` | the provider goes back to running inside the build, which is the stall the worker exists to remove |
| `a_finished_icon_asks_every_surface_for_a_frame` | a completion stops forcing the frame that draws the icon, so it never appears until some other event arrives |
| `a_badge_covers_its_corner_and_blends_what_it_covers` | the badge lands in the wrong corner, scales, or replaces the pixels under its translucent edges instead of blending them |
| `the_overlay_slot_is_the_high_byte` | `iIcon` is split as if the overlay slot were in the low bits, so every item gets the icon of some index |
| `every_kind_maps_to_a_glyph_in_the_table` | a kind names a glyph the regenerated table no longer carries, which `IconSource::glyph` would otherwise panic on at draw time |
| `an_index_lists_its_directories_and_inherits` | `index.theme` stops yielding the directory sizes the ranking reads, or drops an inherited theme |
| `a_bigger_picture_wins_then_the_largest` | the size preference inverts: a large PNG beats a scalable one, or a too-small icon beats a sufficient one |
| `an_inherit_cycle_ends` | a theme whose `Inherits` points back at one already seen loops instead of closing |
| `a_premultiplied_pixel_becomes_straight` | a rendered SVG reaches the picture type with premultiplied channels, which darkens every translucent edge |

`cargo run --release -p ui-icon-theme --example icons_probe` reads the real theme chain.
Measured here (WhiteSur, release build): all thirteen kinds answer at 16, 32 and 128; the
first lookup of a process costs 0.2–8.7 ms (themes parsed, first SVG rendered), later ones
0.1–0.9 ms, and the handle's cache makes repeats free.

On Windows, `cargo run --release -p ui-platform-win32 --example files_probe -- [DIR]` takes
the first 64 entries of a directory and asks for each at 20 and 80 pixels, then asks the
thirteen kinds through the fallback. Measured here (Windows 11, 125% scaling, release build):
`System32` answered 24 items with 4 distinct pictures — folders, DLLs, PNGs each share one —
`powershell.exe` and `powershell_ise.exe` answered distinct pictures, a customized folder
answered a picture distinct from a plain folder's at both sizes, and a junction came back
with the shell's overlay. Per item the query cost 1.6–10 ms warm and 20–80 ms for the first
call of a process; `.lnk` files in the Start Menu cost 10–30 ms and 32.9 ms for one whose
target lives on another volume. Re-measured over `System32`'s first 64 entries at 20 pixels
(release build, 2026-11-16): 1.7–26 ms per item, with most DLLs at 8–18 ms — the number that
made the synchronous lookup untenable once a fast scroll revealed twenty rows in one frame.
The transparent pixels in each picture show the alpha path
working. The same `picker_demo` run that checked thumbnails drew Windows' folder, text and
generic-page icons on the rows of the repository root. The `HICON` reader's two layouts are
the `straighten` tests listed under [File Thumbnails](./thumbnails.md).

## Status

Partial. The per-item provider contract, the memoised handle with its two bounded tables on
the shared picture worker, the host wiring and the framework glyph mapping exist. Windows
answers the item's own icon and the
shell's overlay; Linux answers through `ui-icon-theme` (`file-icons` feature) with the theme's
MIME icon, the executable bit and a link emblem. macOS has none, and directory previews are
deferred. The picker that consumes the chain is built (`ui-picker`), so the settings that gate
thumbnails live in its `ThumbnailPolicy` rather than here.

The freedesktop badge corner and the executable naming were written against the specification
and the icon themes on hand, not against a live desktop; the first Linux visual test is what
confirms them.

## Corrections

**The icon was keyed by kind, so an item's own icon never appeared.** The first provider asked
`SHGetFileInfoW` about a made-up name (`file.zip`) with `SHGFI_USEFILEATTRIBUTES`, which the
shell answers from the extension alone. That is why an `.exe` drew the generic application
icon and a folder ignored its `desktop.ini`, and why no overlay could ever appear: an overlay
is a fact about one item. The request became the item's path, kind, link flag and size, and
the answer became a base picture plus an optional badge. `files_probe` and the picker's visual
test are what catch a return to kind-only lookup.

**An image-list index was read from one list and used in another.** The provider took
`iIcon` from `SHGetFileInfoW` asked for the small list and then read that index from the 32-,
48- or 256-pixel list, whichever was nearest the requested size. An index only names an entry
inside the list it came from: the probe answered every DLL and PNG in `System32` with one
picture — index 113 of the wrong list. The pixels now come from the shell's image factory at
the requested size, and the image list is read only for the overlay, where `GetOverlayImage`
returns an index in the very list it was asked of.

**Identity taken from the shell's index could name different pixels.** A folder whose
`desktop.ini` names an icon answers the plain folder's image index until the shell has read
the file, and the image factory resolves the icon independently. Two rows could then share an
upload that only one of them was. Windows now hashes the pixels it composed, which cannot
disagree with what it hands over. `answers_with_one_identity_share_one_upload` covers the
sharing branch; the desktop.ini comparison in `files_probe` covers the disagreement.

**The requested size was logical, so every picture was stretched.** The picker asked the
provider for 20 or 80 pixels and drew the result into a 20 or 80 logical box — 25 or 100
physical pixels at 125% — and the painter stretched the smaller texture to fill it. The
result was soft, with edges broken into steps at the fractional scale. Both the icon and the
thumbnail request now multiply the slot by the frame's device scale, so the texture is the
size of the box. The provider is measured at physical sizes in `files_probe`; the visual test
at a fractional scale is what catches a return.

**The lookup ran inside the frame, so fast scrolling stalled.** The contract reasoned that an
icon is a theme read or a shell query rather than a decode, priced at 1–10 ms, and let the
build ask the provider directly. A picker scrolling `C:\Windows\System32` froze instead: each
frame reveals twenty to forty rows, and each DLL's query costs 8–18 ms measured, so a frame
spent 200–600 ms in the shell while the scrollbar had already moved. The lookup now runs on
the picture worker the thumbnail service uses, `get` answers `Loading` until it lands, and
the row draws the framework glyph meanwhile — the base of the chain, not a flicker. The
`Loading` answer is what the first argument against a worker missed: a worker does not add a
frame that shows nothing, it adds the glyph the row would draw anyway. `Host::turn` forces the
frame that swaps it, as it does for a thumbnail.
`an_icon_is_asked_for_off_the_frame_thread` pins the thread, and
`a_finished_icon_asks_every_surface_for_a_frame` pins the wake.

**A picture minted a new id every time it was made, so scrolling leaked memory.** The service
allocated an id per production, and its picture table evicted entries once a directory named
more pictures than its bound — `System32` has 381 distinct icons at 20 pixels against a bound
of 256. Each time a row came back into view after its picture had been evicted, the picture
was made again under a new id, the row handed the engine that id, and the engine's texture
table — which keeps every registered id for the run — held the old pixels too. Scrolling
worked better and leaked faster: every traversal of the directory added roughly a picture per
distinct icon. Ids are now derived from what names the picture: the provider's identity and
the size here, the request for a thumbnail. A re-made picture replaces the engine's copy, and
the picker hands the picture over every frame instead of guarding the first. A scan of
`System32`'s 5,041 entries at 20 pixels found 381 distinct identities (release build; one
`SHGetFileInfoW` and one image per entry), so a traversal can now register at most that many
pictures however often the cache cycles.
`a_remade_picture_keeps_its_id` and `a_remade_thumbnail_keeps_its_id` pin it.

**The engine kept every build's picture for the run, so browsing grew the process.** The
engine's picture table was a registry: `Cx::register_image` drained into the same map
`AppLoop::register_image` wrote, and nothing ever removed an entry. Keeping a picture for the
run is right for the wallpaper an app registers once, and wrong for a build that hands over
what it shows. The picker registers a thumbnail per visible row, and Windows answers with
`SIIGBF_BIGGERSIZEOK`, so an 80-pixel cell can receive a 256×256 shell bitmap — 256 KiB. In
the picker demo memory rose from 39.9 MB to 200 MB as thumbnails loaded, and the table only
grew. A picture drained from a build now lives for the frame that painted it;
`AppLoop::register_image` is what keeps one for the run. The service cache stays bounded
(512 pictures / 64 MiB) and is dropped after 60 s with no request unless a consumer holds a
claim (`FileIcons::keep_alive`), so a closed picker does not keep it for the process while an
open one left untouched keeps its icons; a service whose last handle drops also stops its
worker, so the cache is not held past the last holder. `a_builds_picture_does_not_outlive_its_frame`
pins the lifetime, `an_idle_cache_is_dropped_and_the_next_look_asks_again` the drop, and
`a_claimed_cache_survives_the_idle_window` the exemption.

**A badge was chosen from a list larger than its base, so it cropped away.** The overlay was
read from the first image list at or above the requested size. At 125% that list holds
40-pixel pictures while the list row's icon is 25, and the composition crops a badge that
overflows — the shortcut arrow sat outside the crop and never appeared, while the slot was
still reported. The overlay now takes the list nearest the base, preferring one at or below
it, and reads the list's real size instead of assuming 16/32/48/256. The PNG dump of
`Everything.lnk@25` from `files_probe` is what shows the arrow in the corner.

## See also

- [File Thumbnails](./thumbnails.md) — the decode layer a thumbnail row replaces the icon with.
- [Icons](./icons.md) — what an icon slot is and how glyphs are resolved.
