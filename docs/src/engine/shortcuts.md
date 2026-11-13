# Keyboard Shortcuts

A key chord is resolved through eight layers, first match wins. Bindings and
built-in text editing share one table so that an application can rebind either.

## Model

| Type | Role |
| --- | --- |
| `KeyChord { key, mods, any }` | One key plus the modifiers it needs: `mods` must be held, `any` may be held or not. Exact by default — one extra modifier is a different chord |
| `ShortcutId` | A stable static name; applications rebind behaviour by this name |
| `Change::Shortcut(id)` | A node reports that one of its bindings fired |
| `Change::KeyChord(chord)` | A capture node reports the chord the user pressed |
| `ContainerParams::shortcuts` | The node's binding table; `Decl::shortcut(chord, id)` appends |

`Key::Char` stores the unshifted character, and comparison ignores case, so a
platform's `Char('K')` with Shift already applied still matches
`Ctrl+Shift+K`.

## Resolution order

A `KeyPressed` is resolved in `AppLoop::dispatch`:

1. **An open menu's rows**: a row that carries a chord picks itself when that
   chord is pressed. A caret still keeps the keys it types with, so `Enter` in
   a text field commits the text and not the row. `Escape` closes the menu; any
   other key falls through to the rest of this list.
2. **Capture node** (`Sense::CAPTURE_KEYS`): the nearest one on the focus
   chain takes the key and receives `Change::KeyChord`. `Tab` is exempt, or
   the focus ring would trap the user.
3. **Focus chain**: nearest binding walking up from the focused node.
4. **Hover chain**: bindings may be claimed by hovering alone.
5. **The caret's own keys**: while a text control is being edited, a key that
   types into it or moves it answers through the built-in table before any
   application binding, unless `Ctrl`, `Alt`, or `Logo` is held. Without this
   gate an app binding for `A` would take the letter the field is waiting for.
6. **The caret's commands**: that caret keeps the built-ins a command modifier
   spells as well — copy, cut, paste, undo, select-all, delete-forward — so an
   app binding for `Ctrl+C` does not copy the picture behind the field.
7. **Application bindings** registered with `AppLoop::bind_shortcut`.
8. **Built-in table**: the engine's text editing actions.

If nobody claims the key, it falls through to ordinary key handling: arrow
navigation, `Enter`/`Space` activation.

The layers decide who answers; within a layer the most specific binding wins —
the one leaving the fewest modifiers free — and registration order breaks
ties. A chord with a free modifier therefore never shadows an exact chord
bound beside it, and `ui_core::shortcut::best_match` is the one place that
ordering lives.

## A menu row's chord

A `MenuItem` has two fields for one chord: the spelling shown at the row's
trailing edge (`with_shortcut`) and the `KeyChord` the menu answers
(`with_accelerator`). `with_chord` sets both, so the row cannot print a key it
does not run. A disabled row never answers its chord.

The engine's built-in text rows take their chord from the loop's bindings by
id, so an app that rebinds copy sees the new chord printed and answered. A pick,
by pointer or by chord, reports the row's id as `Change::MenuItem` and closes
the menu.

## Built-ins and rebinding

The built-in text actions (`text.copy`, `text.cut`, `text.paste`,
`text.select-all`, `text.home`, `text.end`, `text.document-home`,
wordwise left/right, and their shifted variants) live in a table, not in
`dispatch`. `rebind_shortcut(id, chord)` moves one to a different chord;
`unbind_shortcut(id)` makes it stop responding.

> **Wrong path:** hardcoding a key branch in `dispatch`. It will keep working
> after a rebind, so the built-in and the binding disagree. Navigation keys are
> the deliberate exception: arrows, `Tab`, `Enter`, and `Space` are not
> rebindable commands.

## One id, several chords

A binding table is a list of `(chord, id)` pairs, not a map from an id to one
chord, so binding the same id twice gives the action two keys: either fires,
and the node reports the same `Change::Shortcut(id)`. `bind_shortcut` appends;
calling it again adds a chord rather than replacing the first.

`rebind_shortcut` is the one call that replaces: it drops every chord the id
had and installs one. An app that binds a list and later rebinds an entry keeps
a single chord afterwards, which is why the screenshot app resolves its whole
table from the configuration instead of rebinding entry by entry.

`chord_of` answers the first chord of an id, so a menu row or a tooltip that
names "the" shortcut shows one of them. An app that wants to name all of them
carries the spellings it parsed, as the screenshot app's plan does for
`info config`.

## A lone character as a key

A plain letter never arrives as `KeyPressed`: with no command modifier held,
the compositor hands the surface text. A surface with nothing to type into
therefore reads a one-character `Text` event as the key it names — lowercased,
as every chord stores it — and resolves it through the same layers. That is
what lets the capture overlay bind `A` and `D` to its cursor steps.

The caret gate is the other half of that contract: the same letter reaches a
field being edited as text and never as a command, so `typed_key` and
`caret_owns` disagree about who owns a plain `A` exactly when a caret exists.
A capital matches a chord that names `Shift` or leaves it free, never a plain
`A` binding: that is a different chord.

## Modifiers as gesture input

`Cx::modifiers()` exposes held modifiers to controls and records a
dependency, so a cached declaration cannot keep a stale preview. This is the
entry point for "the modifier changes what the *pointer* gesture means"
(`Alt`+drag and similar), where no chord resolves at all.

A modifier that changes what a *keyboard* command means is declared free in the
chord instead: `KeyChord::allowing(Modifiers::SHIFT)` matches the press with and
without `Shift`, and nothing with another modifier held. `App::shortcut`
receives the modifiers the press resolved with, so the effect is decided by
that press rather than by whatever is held at the next build. A modifier left
free is that one modifier, not a blanket "any modifiers": the screenshot app's
plan frees `Shift` alone on each direction, so `Ctrl+Shift+A` is still a
different press.

## Capture fields

`Md3ShortcutField` subscribes to `CAPTURE_KEYS` and only to that: it must see
the chord it is about to bind, before any binding claims it. While only
modifiers are held it previews the chord. Labels go through Fluent
(`ui_widgets::shortcut_label`), never through a hardcoded key name.

## Global hotkeys

A surface can be born with bindings: `OpenOptions::shortcuts` binds chords to ids before the
surface's first frame, and the app answers them in `App::shortcut`. This is how the capture
overlay gets its `Escape` — the app's build has no door to the binding table, and a surface
that swallowed every press would otherwise take no way out. A pin canvas gets the same chord
for the pin under the pointer; both spell it through one helper, so the two cannot drift.

`ui-platform/src/hotkeys.rs` defines `GlobalHotkeys` (`register`/`unregister`/`poll`),
`NoGlobalHotkeys` for a platform with no system-wide hotkeys, and `parse_chord`
for the spelling a configuration file uses. The host polls the backend once per
turn and hands each `ShortcutId` to `Host::on_hotkey`, with the same `MessageCtx`
a channel request gets — a hotkey opens surfaces and reads the output list.

The platform is chosen when the backend is built; which transport a platform
has is chosen at runtime, once, by the backend. Wayland's portable transport is
the portal (`GlobalShortcuts`, behind the `global-hotkeys` feature, on by
default because this crate is Linux's platform layer); a no-portal desktop gets
`NoGlobalHotkeys`, so registration answers `Unsupported` instead of the app
believing a chord is bound.

- The engine's `Key` names `Print` and `Function(1..=24)`; the codec maps them to
  the keysyms `Print` and `F1`..`F24`, so a configuration can bind the screenshot key.
- The portal accepts a **string trigger** (`CTRL+SHIFT+k`), not a modifier
  bitmask. Keysym names come from `xkbcommon-keysyms.h`; unknown keys are
  refused rather than guessed.
- A session may bind only once, so each binding opens its own session.
- `Activated` arrives on the portal's own thread, not the compositor's
  connection. The reader bumps an `eventfd` after queueing the id; the
  backend adds that descriptor to its `wait` poll and drains it, which is the
  only thing that can wake an idle loop for a hotkey.
- `poll` never blocks: it drains a mailbox the reader fills.
- Closing the session is the only release the portal offers, so the backend sends `Close`
  when it is dropped; a process that exits without it leaves the chord grabbed until the
  portal itself restarts.
- A nested compositor has no portal of its own. Binding from inside a nested session binds
  on the host's portal and therefore on the host's desktop — exercises that bind for real
  belong on a disposable account or a throwaway portal, not on a developer's session.
- The portal has no unbind method. `unregister` drops our mapping; the
  compositor binding remains until the session closes.
- A trigger is one literal string, so a chord with a free modifier cannot be a
  global hotkey. The screenshot app refuses that spelling in the file instead
  of binding a modifier set the trigger cannot name.

## Corrections

**A command that read `Shift` had to be bound once per spelling.** Chords matched exactly, so
`Shift+Left` never reached a binding for `Left`; the screenshot app worked around it by deriving
a shifted chord for every direction and reading the modifier again at build time — one frame
after the press, where a `Shift` released in between already read as absent. The chord now
carries the modifiers it leaves free, the tables resolve the most specific binding, and
`App::shortcut` receives the modifiers the press resolved with.
`a_free_modifier_widens_the_chord_without_shadowing_an_exact_one` pins the predicate and the
ordering; `a_direction_answers_its_spellings_on_every_place_that_names_it` pins the app's table.

**A press was answered by starting a second process.** The overlay and the captures drove their
own loops, so the daemon could not open a surface on its connection without a second reader; the
press ran `crabsnap shot region` as a child. Two processes meant two connections, a second load
of the settings and the catalog, and a daemon that never learned the outcome — a second press
could not tell that an overlay was already on screen. The overlay now opens on the daemon's own
loop and the pin it produces is drained from `on_turn`; `MessageCtx::is_open` answers the
duplicate-press question from the loop's own bookkeeping. Nothing in the hotkey path starts a
process.

**An app binding could take the text's own commands.** The caret gate covered
only the keys without a command modifier, so a surface that bound `Ctrl+C` for
its own copy took the press from a focused text field, and `Ctrl+Z` undid the
picture instead of the typing. The screenshot app's new menu chords exposed it:
copying a pin while its text mark was being edited would have copied the
picture. The built-in editing actions now answer before application bindings
whenever a caret is open. Caught by
`a_caret_keeps_the_text_commands_from_an_app_binding`, which binds `Ctrl+A`
for the app and asserts the field's text is what gets selected.

**Every Ctrl chord resolved to no key.** Key names were taken from
`key_get_utf8`, which is a control byte whenever Ctrl is held, so the chord
never matched a binding. Naming uses `key_get_one_sym` with the keysym prefix
stripped; UTF-8 is reserved for committed text, the only path that composes
dead keys.

**Rebinding had no effect.** The built-in actions were hardcoded branches in
`dispatch`, so `rebind_shortcut` changed a table nothing consulted. They now
live in the table, and the old branches were deleted; leaving them would have
silently restored the default.

**The portal trigger was encoded as an X11 modifier bitmask.** The portal's
`preferred_trigger` is a string of uppercase modifier tokens joined with `+`
and a keysym name; a mask table exists only in X11/GDK. The XML shows no
`combined_key`/`modifiers` pair. The codec now encodes the string and has unit
tests for both directions.

## Traps

| Change | Symptom |
| --- | --- |
| Add a key branch in `dispatch` | Rebinding appears to work but the hardcoded branch still fires |
| Treat the chord as case-sensitive | `Ctrl+Shift+K` never matches |
| Capture `Tab` in a field | Focus cannot leave the field |
| Assume a modifier mask for global hotkeys | The portal refuses or binds the wrong key |
| Guess keysym names for punctuation | Wrong or missing binding; use the published table |
| Bind twice expecting a replacement | Both chords fire; `rebind_shortcut` is the replacing call |
| Name "the" chord of an id that has several | `chord_of` answers the first; the rest never show |
| Expect a plain letter while a field is focused | The caret gate keeps it; only a command modifier resolves |
| Read `Shift` and expect the plain chord to fire | The press matches nothing; leave the modifier free in the chord and read `held` in `Shortcut` |
| Read `Cx::modifiers()` for a keyboard command | It answers at the next build, not at the press; use the `held` argument |

## Status

Implemented: exact chords, chords that leave named modifiers free, several
chords per id, eight-layer resolution with most-specific-first inside a layer,
the press's modifiers handed to `App::shortcut`, rebinding, built-ins, capture
fields, plain letters as keys where no caret exists, modifier exposure, the
portal global-hotkey backend behind a feature.

Not implemented: a free modifier on a global hotkey (the trigger is one literal
string), shortcut scopes beyond "control" and "application". The portal path has
not been verified with a real press: binding, and the daemon's action path —
capture, overlay on the host loop, `Escape` cancelling it, the pin handoff —
were exercised by an in-process probe, not by pressing a chord on the desktop.

## See also

- [Pointer Model](./pointer-model.md): modifiers that change a gesture.
- [Text and Input](./text-and-input.md): the editing actions these bindings
  drive.
- [Platform Boundary](../platform-boundary.md): why the portal backend is
  feature-gated.
