# Language and Locale

A catalogue turns a message key into text in one language. This page covers what
comes before it: where the system's language preference is read, how a language
choice is negotiated against the languages an application ships, and how a
running window changes language.

## Problem

Each binary used to resolve the language by itself, and the rules disagreed:

- `apps/crabsnap` read `--lang`, then `CRABSNAP_LANG`, then `[ui] lang`, then
  `LANG`, mapping `LANG` to `zh-CN` or `en` by a prefix test;
- the settings window — its own binary then, `crabsnap settings` now — read
  `CRABSNAP_LANG`, then `[ui] lang`, then `LANG` with its own parsing;
- the gallery read `GALLERY_LOCALE`.

Nothing represented "the language this application is showing". A catalogue was
built once at start-up and handed around as `Arc<Catalog>`, so a settings window
could change the file but not a window that was already open. The language was
also a free-text field in the settings window, with an error for a tag the
catalogue could not load: it offered no way to say "follow the system".

## Comparison

| Project | Model | What it establishes |
| --- | --- | --- |
| Android 13 app languages | `LocaleManager.setApplicationLocales`; an empty list follows the system; `localeConfig` declares the shipped set | A per-application choice over the system's, with the shipped set declared by the application |
| Apple platforms | `AppleLanguages` is an ordered array; per-app choice in Settings | The preference is a list, not one tag |
| Flutter | `PlatformDispatcher.locales` (ordered) and `supportedLocales`; `localeListResolutionCallback` resolves | The platform reports preferences; the framework negotiates |
| Qt | `QLocale::setDefault`, runtime `LanguageChange` | One current language for the process, and a notification when it changes |
| GTK / GLib | `g_get_language_names` builds an ordered list from `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`; `C`/`POSIX` disables translation | The Unix environment convention in full |
| Windows | `GetUserPreferredUILanguages`; `ApplicationLanguages.PrimaryLanguageOverride` | The system answer comes from a platform API, not an environment variable |
| `fluent-langneg` 0.14 | `negotiate_languages(requested, available, default, strategy)` with six matching levels | Negotiation is a solved problem with a reference implementation |

The shared shape: the platform reports an ordered list, the framework holds one
choice and negotiates it against the shipped languages, and the application
stores only the choice.

## Contract

### The system's languages

The application asks its own platform crate, `crabsnap_platform::system_languages()`, because
the command line builds its help text before any backend exists. Windows answers from
`GetUserPreferredUILanguages` under `MUI_LANGUAGE_NAME` — the list a person sets in Settings,
where no locale variable is involved — and a POSIX session answers from its environment, most
preferred first:

- the message category is `LC_ALL`, else `LC_MESSAGES`, else `LANG`; when it is
  `C` or `POSIX`, the answer is empty, because the user asked for no
  translation. GNU gettext ignores `LANGUAGE` in that case, and so does this;
- otherwise `LANGUAGE`'s colon-separated list comes first, then `LC_ALL`,
  `LC_MESSAGES` and `LANG`;
- each value loses its `.codeset` and `@modifier` suffixes, `_` becomes `-`, and
  repeats are dropped.

`ui_platform::posix_system_languages()` is that environment convention, and
`Backend::system_languages()` answers the same list to a running loop: its default calls the
function, and a backend whose operating system reports the preference through an API overrides
it. The Win32 backend reads `ui_platform_win32::system_languages`, which is what the app's
platform crate calls too, so a window, the daemon and the help text agree.

### The choice

```rust
pub enum LanguageChoice {
    Auto,
    Fixed(LanguageIdentifier),
}
```

`LanguageChoice::parse` reads `auto` or a BCP-47 tag, which is how the config
file and `--lang` spell it. `Display` writes the same two forms.

### The service

`ui_i18n::Language` owns the choice, the system list and the current catalogue:

```rust
impl Language {
    pub fn builtin(choice: LanguageChoice, system: &[String]) -> Result<Arc<Language>, LoadError>;
    pub fn from_sources(
        choice: LanguageChoice,
        system: &[String],
        sources: &[(&str, &str)],
    ) -> Result<Arc<Language>, LoadError>;

    pub fn choice(&self) -> LanguageChoice;
    pub fn set_choice(&self, choice: LanguageChoice) -> Result<(), LoadError>;
    pub fn catalog(&self) -> Arc<Catalog>;
    pub fn resolved(&self) -> LanguageIdentifier;
    pub fn revision(&self) -> u64;
}
```

`Auto` negotiates the system list; `Fixed` negotiates its one tag. Both append
`en`, so a language the application does not ship still renders English. The
languages an application ships are the sources it passes: a tag with no source
is never selected.

`set_choice` rebuilds the catalogue from the same sources and bumps the
revision. On failure the old catalogue stays and the error is returned. A switch
re-parses the Fluent sources; the work is one-shot, on a user action, and is not
cached.

### Negotiation

`Catalog::builtin` and `Catalog::from_sources` take the preference list and
build the bundle chain through `fluent_langneg` with the `Matching` strategy:
exact match, then the available locale as a range, then likely-subtag
maximisation, then variants, region-less maximisation, and finally a region
range. For each requested language the first level that matches wins, so the
result is ordered by the request. The chain always ends with the fallback
language when it is not already present.

`Catalog::locale()` reports the first language of the chain: the one that
supplied the text.

### Injection and switching

`Env` carries `Arc<Language>`, not `Arc<Catalog>`. `Cx` resolves the catalogue
from it once per element build, so every `Cx::t` call reads the language in
force for that frame. A window that keeps its `Env` therefore follows a switch
without rebuilding it; it only has to ask for a frame.

### The application setting

The config file spells the choice:

```toml
[ui]
language = "auto"
```

`auto` follows the system; any other value is a BCP-47 tag. The old `lang` key
is read as an alias and is dropped when the file is next written. The
precedence is `--lang` (one run), then `[ui] language`, then the system list,
then `en`. An unreadable value in the file does not stop the process: it is
reported and treated as `auto`, because the daemon must start even when the
setting is wrong.

The settings window is the only writer. Its language item is an option group:
`Auto` and one button per shipped language, labelled with that language's own
name. The names come from each catalogue's `language-name` message, generated
into `ui_i18n::LANGUAGE_NAMES`; a locale without one fails the build.

## Implementation sites

| File | Responsibility |
| --- | --- |
| `ui/crates/ui-platform/src/locale.rs` | The POSIX list and its precedence |
| `ui/crates/ui-platform-win32/src/locale.rs` | Windows' list, from `GetUserPreferredUILanguages` |
| `ui/crates/ui-platform/src/host.rs` | `Backend::system_languages` |
| `apps/crabsnap-platform/win32/src/platform.rs` | `system_languages` for the command line |
| `apps/crabsnap-platform/unix/src/platform.rs` | The same, from the environment |
| `ui/crates/ui-i18n/src/language.rs` | `LanguageChoice`, `Language`, negotiation |
| `ui/crates/ui-i18n/src/catalog.rs` | Preference lists, `Matching`, `Catalog::locale` |
| `ui/crates/ui-i18n/build.rs` | `LANGUAGE_NAMES` from each locale's `language-name` |
| `ui/crates/ui-widgets/src/cx.rs` | Resolving the catalogue from the `Env` |
| `apps/crabsnap-core/src/config.rs` | `[ui] language`, the `lang` alias |
| `apps/crabsnap/src/main.rs` | The choice, the catalogue, `Settings::language` |
| `apps/crabsnap/src/settings_window.rs` | The option group and `set_choice` |
| `ui/crates/ui-gallery/examples/gallery/shell.rs` | `--locale` for screenshots |

## Verification

| Test | What it would catch |
| --- | --- |
| `posix_languages_follow_gettext_precedence` | `LANGUAGE` losing to `LC_ALL`, or a `C` category still translating |
| `negotiation_picks_the_best_shipped_language` | A chain that skips the likely-subtag level, or falls back early |
| `a_switch_changes_the_text_and_bumps_the_revision` | A choice that does not rebuild, or a revision that does not move |
| `the_language_setting_reads_auto_a_tag_and_the_old_key` | A config value reaching the wrong branch |

## Corrections

**`auto` resolved to English on Windows.** The app read the list with
`ui_platform::posix_system_languages()`, the environment convention, directly. A Windows session
states no `LANG`, `LC_*` or `LANGUAGE`, so the list was empty and the command line's help, the
daemon, the settings window and the tray all came up English on a Chinese system — while the
backend's `system_languages` had asked Windows all along. `crabsnap info config` showed the
disagreement: `system languages: none` beside `ui.language resolved: en`, where
`GetUserPreferredUILanguages` reports `zh-CN, en-US`. The app now asks
`crabsnap_platform::system_languages()`, which is that same Win32 call and the environment on a
POSIX session, and the backend reads the one Win32 function; `info config` reads `system
languages: zh-CN, en-US` and `resolved: zh-CN`, and the settings window and tray menu render
Chinese. No test pins it, because the answer is a machine setting: the check that catches a
regression is `info config` on a system whose language is not English.

The first version of the settings window carried the language as free text and
validated it on Save; it could not express "follow the system", and
`CRABSNAP_LANG` existed only for scripts and the gallery. Both are gone: the
choice is one value with two states and the catalogue is resolved from it, so no
binary reads a private environment variable for language.

## Status

Implemented on 2026-11-11. `ui-platform` reads the POSIX list; `ui-i18n`
negotiates with `fluent-langneg` 0.14.2 and exposes `Language`; `Env` carries the
service; the config key, `--lang auto`, the settings option group, and the
daemon's `reload` are wired. The gallery takes `--locale`.

Two limits are deliberate. The settings window offers the languages `ui-i18n`
embeds, so a language added only to an application's own FTL is not offered.
And a window whose own FTL lacks the chosen language falls back to English for
its own messages, which is the normal Fluent chain rather than a separate rule.
A running process reads the system list once; a change to the system locale
applies to the next launch.
