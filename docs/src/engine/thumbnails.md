# File Thumbnails

One file path in, one picture out, asked for while the loop keeps running: what
the desktop's own thumbnail system can show for a file, or the answer that there
is none. This page owns the request, the worker, the cache and the three
platform sources. It deliberately leaves file-type icons, directory listing and
the file picker itself to other pages.

## Problem

A file picker has to show a picture beside every visible row before a user will
trust it, and the engine has no way to ask for one.

Three things stand between an app and that picture.

Producing it is slow, and how slow depends on the file. A 6000×4000 JPEG is 24
megapixels; a video needs a decoder and a seek to the first frame; an office
document needs a converter that knows the container. None of that fits in a
frame, and a picker asks for dozens of them at once.

The engine draws pixels an app already has. `ImageEntry` takes an `Arc<Vec<u8>>`
and `register_image` puts it in the surface's texture table
(`ui/crates/ui-core/src/cx.rs:1064`), but somebody has to produce those bytes;
`ImageSource` has no variant that names a file, and it should not, because
`ui-core` reaches no filesystem.

Every desktop already has the answer, usually better than a first
implementation would: it knows the camera's orientation tag, the colour profile,
and it keeps a cache that other applications also use, so the second app to open
a folder pays nothing. An app that decodes for itself pays again, disagrees with
the desktop, and warms the cache nobody reads.

`crabsnap` does exactly that today for its own pictures: it decodes through
ffmpeg and keeps an LRU keyed by canonical path, modification time and length
(`docs/src/apps/pins.md`). A picker in the framework would be the second copy of
it, in a crate that other apps also use.

## Comparison

| Framework | What it offers | Where the picture comes from |
| --- | --- | --- |
| GTK / GNOME | `GnomeDesktopThumbnailFactory` | reads and writes the freedesktop cache; runs a matching `.thumbnailer` program first, and gdk-pixbuf in process when none answers |
| KDE | `KIO::PreviewJob` | a `thumbnail` KIO worker that generates in process, file by file |
| Qt | `QFileIconProvider`, `QImageReader` | icons from the style; decoding, no system thumbnails |
| Electron | `app.getFileIcon` | OS icon for a path; no thumbnails |
| Windows shell | `IShellItemImageFactory::GetImage` | every registered shell extension; `SIIGBF_ICONONLY` when an icon is wanted instead |
| macOS | `QLThumbnailGenerator` | Quick Look generators; `NSWorkspace.icon(forFile:)` for icons |

> **Sources:** Nautilus draws its pictures through gnome-desktop's thumbnail
> factory (`libgnome-desktop/gnome-desktop-thumbnail.c`), which tries an external
> `.thumbnailer` before gdk-pixbuf. Dolphin's `KIO::PreviewJob` is served by
> kio-extras' `thumbnail/thumbnail.cpp` worker. Microsoft documents
> `IShellItemImageFactory::GetImage` and its `SIIGBF_*` flags on Learn, and Apple
> documents `QLThumbnailGenerator` in Quick Look Thumbnailing. The Qt, Electron
> and macOS rows come from those API references and were not exercised in this
> repository (`[unverified]`).

Two things separate the desktop APIs from the toolkits that wrap them. The first
is what they return: Qt's `QFileIconProvider` and Electron's `getFileIcon` answer
with an *icon*, a small pictogram chosen by type, while GNOME, KDE and Windows
answer with a *thumbnail*, a reduced picture of the content. A picker needs
both, and they are different capabilities: an icon answers from the desktop's
theme or shell bookkeeping about a name, a thumbnail decodes the file's content
and may take seconds or never arrive. This page implements the second one only.

The second is that on Linux there is no API at all, only a convention. The
freedesktop Thumbnail Managing Standard defines a cache directory and its
contents, and `.thumbnailer` desktop entries define the programs that fill it;
gnome-desktop, Nautilus, KIO's thumbnail worker and tumbler are four readers of
the same files, not four protocols. A reader that follows the standard sees what
all of them produced, and needs no library.

## Contract

| Type | Role |
| --- | --- |
| `thumbnails::ThumbnailRequest` | `path`, `size`, and the `version` the file had when the caller saw it |
| `thumbnails::FileVersion` | `len` and `modified`, hashed into the cache key; `None` means the caller did not look |
| `thumbnails::ThumbnailState` | `Loading`, `Ready(ImageEntry)`, `Unavailable` |
| `thumbnails::Thumbnails` | The app's handle: `get`, `forget`, `clear`; cheap to clone |
| `thumbnails::Thumbnailer` | The platform half: `thumbnail(&ThumbnailRequest) -> Option<ThumbnailPicture>` |
| `thumbnails::ThumbnailPicture` | `width`, `height`, straight-alpha RGBA8 |
| `Backend::thumbnails()` | Returns the platform's `Thumbnailer`, or `None` where the platform has no thumbnail system |
| `Host::thumbnails()` | The `Thumbnails` handle, built in `Host::new` from the backend and the loop's waker |

```rust
pub struct ThumbnailRequest {
    pub path: PathBuf,
    pub size: u32,
    pub version: Option<FileVersion>,
}

pub enum ThumbnailState {
    Loading,
    Ready(ImageEntry),
    Unavailable,
}

impl Thumbnails {
    pub fn get(&self, request: &ThumbnailRequest) -> ThumbnailState;
    pub fn forget(&self, path: &Path);
    pub fn clear(&self);
    pub fn completed(&self) -> u64;
}
```

`get` never blocks and never touches a filesystem. It answers from memory; on a
miss it queues the request and returns `Loading`. A picker calls it once per
visible cell per frame and draws a placeholder or an icon where `Loading` comes
back.

An app takes the handle once, before `run`, the way it takes `Jobs`
(`ui/crates/ui-platform/src/host.rs:849`), and keeps it in its own state; the
view asks it per cell. `Host::thumbnails()` is `None` only where the backend has
no thumbnail source at all, and the app answers that with icons.

The platform is chosen once, and not by this module. `ui-host`'s target tables
pick one backend at build time, `Backend::thumbnails()` is what that backend
answers, and nothing in this module or in an app names a platform: a `cfg` here
would be the second mechanism for a decision the facade already makes, and the
boundary check would reject it
(`ui/crates/ui-platform/tests/platform_boundary.rs`).

The cache key is `(path, size, version)`. A file rewritten on disk is a
different key, so its old picture is never shown again, and an entry is reused
exactly while all three agree. The caller that already listed a directory has
the length and modification time in hand and passes them; a caller that has not
passes `None`, and the picture then lives for the run or until `forget`.

`size` is the longer side in physical pixels. The picture that comes back may be
larger, never smaller when the platform had one at all: the renderer scales it
down to the cell, and a larger source survives a scale change without a new
request.

`Ready` carries an `ImageEntry` whose `ImageSource::Pixels` is the same `Arc`
for the life of the entry. That is the engine's rule that the `Arc` is the
picture's identity (`docs/src/engine/frame-pipeline.md`): the renderer uploads
once, and the same thumbnail drawn in two windows is one upload in each. The
entry is made once and handed out by clone, so its `id` is stable too — it is
derived from the request, so a picture made again after its entry was evicted
carries the id it had before and draws through the same engine entry. The engine
keeps a build's picture for the frame that painted it, so a row scrolled past
does not hold its pixels; the cache below is what a second look finds. Pixels are
straight-alpha RGBA8 with `ContentColour::SDR` and `Sampling::Smooth`; thumbnails
are neither HDR nor pixel art.

`Unavailable` is an answer, not an absence. It is cached for the run, so a
corrupt video or a folder is asked about once rather than on every frame, and
`forget` is what retries it.

Construction never fails. A handle whose worker did not start, or whose worker
panicked under a platform call, answers `Unavailable` for everything, and logs
why: a picker with no pictures is a worse picker, not a crashed app. This is the
shape `Jobs` already has (`ui/crates/ui-platform/src/jobs.rs:49`), including
counting a panicked task as finished so the frame that shows the failure is
still built.

One worker thread takes requests, newest first, and skips a key that is already
queued. The queue holds at most 256 keys; a new request past that displaces the
oldest queued one, because a picker scrolling fast wants the rows it is looking
at and not the ones it flew past. A request already running cannot be cancelled:
at most one runs, so the cost of a stale request is one picture. The queue, the
worker and the two bounded tables are `ui-platform/src/pictures.rs`, shared with
[File Icons](./file-icons.md): both services answer the same three states from
the same shape of request, and one implementation is what keeps them from
drifting apart.

The worker is its own and not `Jobs`. `Jobs` is one worker shared by the app's
own work and taken in order, so a picker's stream of small requests would stand
in front of a save, and one thumbnailer that reaches its deadline would hold
every app job behind it for ten seconds. Discarding work is also the point here
and never the point there. What the two share is the part worth sharing: the
`Waker`, a completion counter the loop compares, and the rule that a panicked
task still counts as finished.

Each finished request bumps `completed` and calls the loop's `Waker`. `Host`
compares that counter once per turn, exactly as it does for `Jobs`
(`ui/crates/ui-platform/src/host.rs:1079`), and asks every surface for one frame
when it moved. The app's next `view` sees `Ready`.

Answers are bounded at 4096 keys, and the pictures they name at 512 pictures and
64 MiB of pixels, whichever comes first; the oldest use of either is dropped when
its bound is crossed. A dropped picture costs one more request, never a wrong
picture: the request whose picture was evicted is asked for again. The whole cache
is dropped after `IDLE` (60 s) with no `get` at all, so a picker closed a while ago
does not keep 64 MiB the host would otherwise hold for the run; a window reopened
inside that window still hits. A claim (`KeepAlive`, from `Thumbnails::keep_alive`)
states that a live consumer is merely idle rather than gone, and the sweep waits
while one exists: `PickerState` holds a claim per service for the dialog's lifetime,
so an untouched open dialog no longer pays one screenful of new requests on its next
build. The size bounds still apply under a claim. The worker also exits when the
last handle drops, taking both tables with it, so a closed window that drops its
host does not leave megabytes behind a thread nobody can reach.

The module deliberately does not: produce file-type icons, list a directory,
watch a file for changes, generate for a folder, decode any file format itself,
return animated frames, read or write HDR or a colour profile, or write the
desktop's record of a failure. Each is a separate capability, a codec the
framework would have to ship to every app, or a promise about another
application's cache that this module cannot keep.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui/crates/ui-platform/src/pictures.rs` | The worker, the newest-first queue, the answer and picture tables, the completion counter |
| `ui/crates/ui-platform/src/thumbnails.rs` | The request, the states, `Thumbnailer`, the pixel validation, the bounds |
| `ui/crates/ui-platform/src/host.rs` | `Backend::thumbnails` defaulting to `None`; `Host::thumbnails`; the turn that asks for a frame |
| `ui/crates/ui-platform-wayland/src/thumbnails.rs` | The freedesktop cache, the `.thumbnailer` programs, the decode fallback |
| `ui/crates/ui-platform-win32/src/shell.rs` | `IShellItemImageFactory` on a COM-initialised worker thread; the DIB-to-RGBA conversion |
| `ui/crates/ui-host/src/lib.rs` | Re-export only; the target table already picks the backend |

### Windows

`SHCreateItemFromParsingName` turns the path into an `IShellItem`, and
`IShellItemImageFactory::GetImage` asks the shell for a bitmap at the requested
size. The flags are `SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT |
SIIGBF_BIGGERSIZEOK`: `THUMBNAILONLY` is what makes the answer a thumbnail
rather than a file icon, so a file with no thumbnail source reports
`Unavailable` here as it does on the other platforms, and `BIGGERSIZEOK` lets
the shell hand over the larger bitmap it already cached instead of resampling.

The call may load any registered shell extension, so it runs on the module's
worker thread with `CoInitializeEx(None, COINIT_APARTMENTTHREADED)`: Microsoft's
registering-shell-extensions page requires handlers to declare
`ThreadingModel=Apartment`, and Explorer calls them from one. The apartment is
the worker's, so no shell object outlives the call. The worker has no message
pump, which is the risk this design accepts rather than one the API documents;
the crate's window thread is already an STA for its own reasons
(`ui-platform-win32/src/tsf.rs`), and a pump belongs in this thread only if a
handler is found to need one.

The bitmap comes back as a DIB whose layout is not fixed: the shell's own header
comment names 24-bit RGB, 32-bit RGB and premultiplied ARGB32 as the answers a
handler may give (`ShObjIdl_core.idl`), so the provider reads `bmBitsPixel` and
the alpha channel instead of assuming the widest layout. The bits are asked for
once, as top-down 32-bit BGRX, and converted in place into straight-alpha RGBA8:
a 32-bit answer keeps its alpha channel, a shallower one is opaque, and a 32-bit
answer whose alpha is entirely zero is treated as opaque too. Windows hands the
colour over with straight alpha — GDI multiplies a 32-bit icon's colour by its
alpha itself — so the conversion swaps red and blue and zeroes a fully transparent
pixel, nothing else. The conversion is a pure function so its two layouts and the
icon mask below can be tested without a shell or a bitmap handle.

### Linux

There is no library to call. Two layers, in order:

1. **The cache.** `$XDG_CACHE_HOME/thumbnails/{normal,large,x-large,xx-large}`
   holds PNGs named by the MD5 of the file's `file:` URI, at 128, 256, 512 and
   1024 pixels on the longer side, with the `Thumb::URI`, `Thumb::MTime`,
   `Thumb::Size` and `Thumb::Mimetype` `tEXt` chunks. An entry is valid only
   while its `Thumb::MTime` equals the file's current mtime and, when
   `Thumb::Size` is present, its length; `Thumb::URI` is checked when present. A
   miss leads to the next layer; a hit is decoded with the `png` crate, which is
   also where the chunks are read.

   The standard calls `Thumb::URI` required, and the cache on the machine this
   page was written on says most writers disagree: of 17,843 entries under
   `~/.cache/thumbnails/normal/`, 5,036 carry `Thumb::URI` and 12,807 do not,
   while every one carries `Thumb::MTime` and `Thumb::Size`. The name is the
   URI's MD5, so the chunk adds nothing a reader can act on; rejecting entries
   that lack it would re-generate thousands of pictures that are already right.
   The pictures are the tier's size on the longer side, not padded to a square.

2. **The thumbnailer programs.** `/usr/share/thumbnailers/*.thumbnailer` are
   desktop entries with `TryExec`, an `Exec` line and a `MimeType` list, in
   `%i` (local path), `%u` (URI), `%o` (output) and `%s` (size). The provider
   picks the entry whose `MimeType` matches the file's type and whose `TryExec`
   resolves, substitutes the placeholders, and runs it with no shell, in a
   temporary directory, with a ten-second deadline. The program writes a PNG; the
   provider decodes it, adds the chunks above and moves it into the cache, so the
   next reader of that file — including the file manager — pays nothing.

   All eight entries installed on this machine substitute `%s`; three pass `%i`
   and five pass `%u`, so both placeholders are load-bearing. One of them
   declares no `TryExec`, so an absent `TryExec` means the program named in
   `Exec` is the only check, not that the entry is skipped.

   `Exec` is not a shell command line — no pipes, variables or globs — but it is
   not a whitespace split either: quotes and backslash escapes count, and the
   provider has its own splitter for them. The placeholders are then replaced
   inside the words that splitter produced, the other order from gnome-desktop's
   substitute-then-parse: a path with a space cannot become two arguments because
   it never reaches the splitter.

   The `MimeType` match needs a type, and this is the one place the Linux
   provider guesses: by extension, from a table of the types `.thumbnailer`
   entries declare. A file whose extension lies gets no program and answers
   `Unavailable`.

When neither layer answers, so does the module. It carries no decoder for any
file type, which is the difference between it and gnome-desktop: that library
falls back to gdk-pixbuf in process, and gdk-pixbuf is a codec for every picture
format a distribution chose to install. In a framework crate that fallback is
the tail that wags the dependency list — every app that opens a file picker then
links a decoder stack it may never use — so the platform's own thumbnailers are
the only producers. A machine with no thumbnailer packages answers `Unavailable`
for everything, and an app that wants more decodes the types it cares about
itself, through its own jobs, the way `crabsnap` already does.

`org.freedesktop.thumbnails.Thumbnailer1` is not asked. Its `Queue` returns a
handle and `Ready` replays the source URIs it finished rather than the cache
paths, so a client still reads the same PNGs from the same directory; and the
service is tumbler's — Thunar's — not part of the standard. Speaking it would add
a protocol for one desktop's daemon and buy no picture the cache does not already
hold.

The failure record is read and never written. `~/.cache/thumbnails/fail/` holds
the same MD5 names with a `Thumb::MTime`, and a record whose `Thumb::MTime`
equals the file's current mtime means some other application already decided
this file has no thumbnail; answering `Unavailable` there saves spawning a
program that will fail. Writing one is not this module's business: a timeout here
may be a slow disk rather than a file without a picture, and a wrong record stops
the desktop from ever trying again.

The cache is shared with every other application, so a write is atomic: the PNG
is encoded into a temporary file in the target directory and renamed over the
name, which is what the standard requires and what keeps a half-written thumbnail
from being read by the file manager. The metadata is written from what the
provider measured itself, never from the caller's `FileVersion`, so an entry can
only be accepted by a reader whose own check agrees.

### macOS

No backend crate exists. `QLThumbnailGenerator` has been available since 10.15
and produces a `CGImage` for a request at a size, calling back when it is done on
a queue Apple does not name — which is why nothing in the portable half depends on
which thread a provider finishes on. It documents no size limit, so `size` stays a
request rather than a promise, as it is for the other platforms.
`NSWorkspace.icon(forFile:)` is the icon path, which [File Icons](./file-icons.md) covers.

## Verification

`ui/crates/ui-platform/src/thumbnails.rs` pins the module with a fake
`Thumbnailer` that the test releases one call at a time, so none of these touch a
filesystem:

| Test | What it would catch |
| --- | --- |
| `a_queued_key_is_asked_once` | `get` on every frame of a wait queues the key once, and the size reaches the provider as asked |
| `a_newer_version_is_a_new_request` | a rewritten file is asked again and the old picture is never handed out for it |
| `an_unavailable_file_is_asked_once` | a failure is cached instead of retried per frame, and `forget` is what retries |
| `the_queue_keeps_the_newest_requests` | the oldest queued request is displaced, not the newest |
| `a_ready_entry_hands_over_the_same_pixels` | the `Arc` is stable across frames, which the texture cache depends on |
| `a_remade_thumbnail_keeps_its_id` | `forget` and a re-decode mint a new id, so the engine's table grows by one picture per re-decode |

`the_queue_keeps_the_newest_requests` waits until the worker has taken its blocking request
before it fills the queue. Displacement is by the queue's oldest entry, and a worker thread
that the scheduler has not started yet leaves that blocker at the front — so without the
wait the test fills behind a blocker that the first filler displaces, and checks the wrong
displacement. The wait makes the setup express the premise instead of racing it.

`ui/crates/ui-platform-wayland/src/thumbnails.rs` pins the parts of the standard
that are pure computation:

| Test | What it would catch |
| --- | --- |
| `a_cache_entry_counts_only_for_the_file_it_was_made_from` | a stale `Thumb::MTime`, a mismatched `Thumb::Size`, a foreign `Thumb::URI`, and the entry without one that must still be accepted |
| `a_substituted_path_stays_one_argument` | `%i` beside other text, a path with a space and an apostrophe, and `%%` |
| `an_entry_yields_its_program_and_types` | the section name, `TryExec` present or absent, and a `MimeType` list |
| `a_size_maps_to_the_first_tier_that_holds_it` | the four tiers, their boundaries, and a zero size |
| `a_path_becomes_an_escaped_uri_and_its_md5` | the URI escaping and the MD5 name another application computes |

`ui/crates/ui-platform/tests/host_loop.rs` pins the wiring:
`a_finished_thumbnail_asks_every_surface_for_a_frame` fails if `Host::turn` stops
asking for a frame when a request completes, which is the one way a thumbnail
never reaches a build.

The Windows provider's pixel conversion is a pure function in
`ui/crates/ui-platform-win32/src/shell.rs`, so the shell itself is not needed to
pin it:

| Test | What it would catch |
| --- | --- |
| `colour_is_swapped_and_a_transparent_pixel_is_zeroed` | a 24-bit answer left with its alpha byte, a straight-alpha colour divided by its coverage (which over-brightens every icon edge), or a transparent pixel kept with a colour that bilinear sampling pulls into its neighbours |
| `a_mask_supplies_the_alpha_a_legacy_icon_lacks` | a pre-Vista icon whose AND mask is ignored, which paints its transparent corners as a black frame |

Not covered by tests, and checked by hand with
`cargo run --release -p ui-platform-wayland --example thumbnails_probe -- PATH`:
a picture is generated by the matching thumbnailer and written into the cache
with its chunks, a second call reads it back without running anything, a text
file answers `Unavailable`, and a thumbnailer that hangs is killed at the
deadline.

On Windows, `cargo run -p ui-platform-win32 --example files_probe -- DIR` asks the
shell for the same answers: the wallpaper folder answered 256×160 pictures for
its two JPEGs in 30 ms (first, handler and decoder loaded) and 2.7 ms (second,
the shell's cache), while a folder of HTML reported `None` for every file, which
is `SIIGBF_THUMBNAILONLY` doing its job. The `picker_demo` rows drew those two
pictures in the MD3 and Fluent dialogs.

## Status

Partial. The portable module, `Backend::thumbnails`, `Host::thumbnails` and the
Wayland and Windows providers exist. macOS has no provider, so its backend
answers `None` and `Host::thumbnails()` is `None` there.

## Corrections

**The shell's colour was divided back as if it were premultiplied.** On the theory that a
32-bit DIB from the image factory arrives premultiplied, the conversion divided every channel
by its alpha. The data is straight: measured on a 25-pixel `powershell.exe` icon, an
anti-aliased edge pixel reads `[21, 189, 255, 191]` — blue above its own coverage — and
dividing it produced cyan-green speckles along every edge, with the whole icon looking soft.
GDI's own treatment of icon colour, recorded under [Tray](./tray.md)'s correction, is the
same answer. The conversion now only swaps red and blue and zeroes transparent pixels;
`colour_is_swapped_and_a_transparent_pixel_is_zeroed` pins it, and the PNG dumps from
`files_probe -- DIR OUT` are what exposed the fringes.

## See also

- [System Tray](./tray.md) — the other platform service with a portable contract
  and one implementation per desktop.
- [Canvases](./canvas.md) — the other picture source a build can hand the engine,
  and how `ImageEntry` identity works.
- [Pins: Content, Sources and Editing](../apps/pins.md) — the app's own decode
  path, cache keys and why it exists.
