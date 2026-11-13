# System Tray

One icon in the desktop's notification area: a picture, a tooltip, a menu the
desktop draws, and a report when the user clicks it or picks a row. The engine
owns the shape; the desktop owns the look.

## Problem

A screenshot tool that lives in the background has nowhere to be. Its commands
arrive over a local channel or a hotkey, but a user who has forgotten the
hotkey needs something to click, and a process with no window has nothing to
click. The tray is that place: the desktop keeps it, so it survives every
window closing.

The desktop's tray is also the one place an app cannot draw. Windows keeps the
icon in its taskbar, macOS in its menu bar, and a Linux panel draws it from a
D-Bus object. None of them accepts an engine-drawn menu there, and none of them
reports a pointer position the engine could use to place one.

## Comparison

| Framework | Creation | Picture | Tooltip | Menu | Activation |
| --- | --- | --- | --- | --- | --- |
| Qt `QSystemTrayIcon` | constructor takes a `QIcon` | `setIcon` | `setToolTip` | `setContextMenu(QMenu*)` | `activated(ActivationReason)`: `Trigger`, `DoubleClick`, `Context`, `MiddleClick` |
| Electron `Tray` | `new Tray(image)` | `setImage` | `setToolTip` | `setContextMenu` | `click`, `right-click`, `double-click`, `middle-click` |
| tray-icon | `TrayIconBuilder` | `Icon::from_rgba` | `with_tooltip` | `with_menu` | `TrayIconEvent::Click { button, .. }` |
| Windows shell | `Shell_NotifyIcon` | `HICON` | `szTip` | `TrackPopupMenu` from the icon's window | callback message: `NIN_SELECT`, `WM_CONTEXTMENU`, `WM_LBUTTONDBLCLK` |
| Linux SNI | `org.kde.StatusNotifierItem` | `IconPixmap` | `ToolTip` | `Menu` object, `com.canonical.dbusmenu` | `Activate`, `SecondaryActivate`, `ContextMenu` |

The four share exactly one shape: create with a picture, then set the picture,
the tooltip and the menu, and hear about clicks and chosen rows. Everything
else is one desktop's addition — Qt's `showMessage` balloon, Electron's
`setTitle` (macOS only) and its `mouse-move` reports, SNI's `AttentionIcon`,
`OverlayIcon` and `Scroll`.

All of them hand the menu to the desktop instead of drawing it. Qt's
`QSystemTrayIcon` takes a `QMenu*` and on Linux publishes it through
`com.canonical.dbusmenu`
(`/z/@/usr/include/qt6/QtGui/6.12.0/QtGui/private/qstatusnotifieritemadaptor_p.h`);
Electron's `setContextMenu` builds a native menu; tray-icon shares muda with
the window menus. The tray module follows them: the app describes rows, the
desktop draws them.

## Contract

| Type | Role |
| --- | --- |
| `tray::Icon` | Straight sRGB RGBA8, `from_rgba` checks the length |
| `tray::Menu`, `tray::MenuEntry` | The menu: `Item`, `Check`, `Submenu`, `Separator`; a leaf row is a `ui_core::menu::MenuItem` |
| `tray::TrayEvent` | `Click { button, at }`, `DoubleClick { at }`, `Menu { id }` |
| `tray::TrayId` | What every report about one icon carries; never reused |
| `tray::Tray` | The app's handle: `set_icon`, `set_tooltip`, `set_menu`, `set_visible`; drop removes the icon |
| `Host::tray(icon)` | Puts an icon in the tray and returns its `Tray` |
| `Host::on_tray(handler)` | `FnMut(TrayId, TrayEvent, &mut MessageCtx) -> Effect`, the same context a hotkey handler gets |
| `Backend::create_tray`, `Backend::tray_events` | The platform half; the defaults answer that the platform has no tray |

The app holds the `Tray`; dropping it takes the icon out of the tray and frees
the platform's object. Nothing else in the loop holds a tray icon, so an icon
that outlives its handle cannot happen.

A report reaches the app on the loop's turn, like a hotkey press: the backend
queues what the shell sent, `Host` drains it, and each report gets its own
`MessageCtx`. The loop drains the queue even with no handler set, so an app
that creates its icon before it decides what a click does does not leave a
growing queue behind.

A click is reported whether or not the icon has a menu. A right-click both
opens the menu and reports `Click { button: Secondary }`, so a report is not
news that the menu stayed shut; an app that sets no menu is what gets a bare
report. This is tray-icon's and Electron's behaviour, not Qt's, where a
context menu swallows the activation.

`MenuEntry::Check` carries the app's own idea of the state. The engine does not
toggle it: the app hears `TrayEvent::Menu { id }`, updates its state and calls
`set_menu` again. A desktop menu is rebuilt from the app's menu each time it
opens, so that call is what makes the tick move, and the app's state and the
tick can never disagree.

The module deliberately has no balloon notifications, no title beside the icon,
no attention or overlay picture, no hover reports, no scroll reports, no radio
rows and no icon geometry. Each is one desktop's capability or a separate
permission; an app that needs one asks the platform crate for it.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui/crates/ui-platform/src/tray.rs` | The model, the events, `Tray`, `TrayHandle` |
| `ui/crates/ui-platform/src/host.rs` | `Host::tray`, `Host::on_tray`, the turn that drains reports, the default `Backend` answers |
| `ui/crates/ui-platform-win32/src/tray.rs` | The hidden window, `Shell_NotifyIcon`, the icon, the native menu |
| `ui/crates/ui-platform-win32/src/backend.rs` | Creates icons on the loop's own thread and holds the report queue |
| `apps/crabsnap/src/daemon.rs` | The app's one icon: its menu, and what each report does |
| `apps/crabsnap/src/tray_icon.rs` | The picture, rasterized from the engine's own primitives |

On Windows each icon owns a hidden top-level window. The shell posts its
callback there, and the popup menu belongs to that window, because Win32 menus
belong to a window. The window joins the thread's existing message queue, so
`Backend::wait` already wakes for a click and `pump_messages` already
delivers it; no second thread and no extra handle in the wait set.

The window takes no foreground of its own, but showing a menu raises it with
`SetForegroundWindow` first and posts `WM_NULL` after: a Win32 popup menu is
dismissed by a click on another window only when its owner held the
foreground, which is also why every toolkit does this.

Explorer forgets every icon when it restarts. The shell broadcasts the
registered message `TaskbarCreated`, and the window puts its icon back with
the picture and tooltip it kept.

The icon protocol is `NOTIFYICON_VERSION_4`: the callback's `wParam` is the
anchor point and `LOWORD(lParam)` is the notification. Win32 names menu
commands with 16 bits, so the native menu numbers the app's rows from one and
maps the chosen command back to the `Id` the app knows.

## Corrections

**The callback layout.** The first version of the Windows backend read the
notification from all of `lParam` and reported the pointer from `GetCursorPos`.
Version 4 puts the notification in `LOWORD(lParam)`, the icon id in
`HIWORD(lParam)`, and the anchor in `wParam`
(<https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw>);
the anchor is what the click was on, while the cursor may already have moved.
`one_press_is_one_click_however_the_shell_spells_it` now reads the anchor from
`wParam`.

**One press, two notifications.** A single left press can arrive twice —
`NIN_SELECT` and `WM_LBUTTONUP` both — and the same holds for
`WM_CONTEXTMENU` and `WM_RBUTTONUP`, which is what
`microsoft/microsoft-ui-reactor#1180` reports for a forwarding tray
(<https://github.com/microsoft/microsoft-ui-reactor/issues/1180>). The backend
keeps the last primary and the last secondary notification and drops a second
spelling within 250 ms. `one_press_is_one_click_however_the_shell_spells_it`
posts both spellings and expects one event.

**Straight alpha.** The icon's colour bitmap was first written premultiplied,
on the theory that `DrawIconEx` blends like `AlphaBlend`. A half-alpha red
pixel drawn over blue then came out as red 64, not 128: GDI reads a 32-bit
icon's colour as straight and multiplies by the alpha itself. The colour is
now copied as given, and `a_semi_transparent_icon_blends_the_way_the_app_drew_it`
draws the icon and reads the pixel back.

## Verification

| Test | What it would catch |
| --- | --- |
| `a_semi_transparent_icon_blends_the_way_the_app_drew_it` | Premultiplying the picture, or losing the alpha channel in the DIB |
| `a_menu_reports_the_row_that_was_chosen` | A command id that maps to the wrong row once submenus and separators shift the numbering |
| `one_press_is_one_click_however_the_shell_spells_it` | Dropping the duplicate rule, or reading the notification from the wrong half of `lParam` |

Registration with the running shell is not a unit test: it needs the
notification area. It was checked on Windows 11 by starting the release binary
the way a desktop icon does — `ShellExecute`, so no console — and reading the
icon back from the shell with `Shell_NotifyIconGetRect`
(`1974,1380`–`2014,1440`, inside the notification area's overflow), and by a
second start exiting `0` with the first still answering the channel. The
menu's appearance and dismissal still need a real right-click on the icon.

## Status

Windows: complete — icon, tooltip, menu, visibility, clicks, Explorer
restarts. The screenshot app's daemon is the one icon in use; a desktop start
of `crabsnap` presents it (see
[Command Line, Daemon and Settings](../apps/cli.md#starting-it)).
Linux: returns the backend's default, so `Host::tray` fails with
"this platform has no system tray"; the implementation would be a
`StatusNotifierItem` object over D-Bus plus a `com.canonical.dbusmenu` menu.
macOS: no backend in this workspace.
