//! 设置窗口：配置文件的唯一编辑者。
//!
//! 只改本构建认识的键，文件里其它版本留下的段落原样保留（`Config::save_to`）。保存之后
//! 给 daemon 发 `reload`；没有 daemon 时保存本身仍然生效，下次启动读到的就是新值。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crabsnap_core::config::Config;
use crabsnap_core::hotkeys::Shortcut;
use crabsnap_core::protocol::{Request, Response};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind, Sense};
use ui_design::Appearance;
use ui_i18n::{Catalog, Language, LanguageChoice};
use ui_platform::element::Element;
use ui_platform::host::{Host, OpenOptions};
use ui_platform::hotkeys::parse_chords;
use ui_platform::view::App;
use ui_platform::window::SurfaceSpec;
use ui_style::{Env, StyleScope};
use ui_widgets::Cx;
use ui_widgets_md3::{
    ButtonParams, ButtonSize, ButtonStyle, Md3Kit, Md3Options, TextFieldParams, TextFieldVariant,
};

/// The capture backends this build offers: a Wayland rung is not a choice on Windows.
use crabsnap_platform::CAPTURE_BACKENDS as BACKENDS;
const FORMATS: [&str; 4] = ["auto", "png", "jpeg", "webp"];

const DIRECTORY_FIELD: &str = "settings/directory";
const PROGRAM_FIELD: &str = "settings/program";
const FIELD_WIDTH: f32 = 592.0;

/// The field editing one action's chord.
fn shortcut_id(name: &'static str) -> Id {
    Id::of(("settings/shortcut", name))
}

/// Open the window; `CRABSNAP_SECONDS` still bounds a scripted run.
///
/// The subcommand's flags are the scripted path: with `--save` they apply to the file, the
/// daemon is told to reload, and no window opens.
pub(crate) fn run(args: &crate::cli::SettingsArgs, language: &Arc<Language>) -> Result<(), String> {
    let mut config = Config::load(&crabsnap_platform::paths::config_path())?;
    if args.save {
        Script::of(args)?.apply(&mut config)?;
        store(&config)?;
        println!("{}", crabsnap_platform::paths::config_path().display());
        return Ok(());
    }
    let backend = ui_host::connect().map_err(|error| format!("the display: {error}"))?;
    let mut host = Host::new(Box::new(backend));
    let catalog = language.catalog();
    let title = catalog.text_by_id("settings-title").to_string();
    let deadline = std::env::var("CRABSNAP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map(|seconds| Instant::now() + Duration::from_secs_f32(seconds));
    let window = SettingsWindow {
        shortcuts: Shortcut::ALL
            .iter()
            .map(|action| {
                config
                    .shortcuts
                    .0
                    .get(action.name())
                    .cloned()
                    .unwrap_or_default()
            })
            .collect(),
        choice: language.choice(),
        language: Arc::clone(language),
        directory: config
            .shots
            .directory
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        program: config
            .codec
            .program
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        config,
        catalog: Arc::clone(&catalog),
        env: environment(language),
        kit: Md3Kit::new(),
        status: None,
        close: false,
    };
    host.open(
        SurfaceSpec::window("dev.crabsnap.settings", title, (640, 740)),
        window,
        OpenOptions {
            fonts: vec![crate::fonts::icon_face()],
            ..OpenOptions::default()
        },
    )?;
    host.run_until(deadline)?;
    Ok(())
}

/// The app's environment: the language for words, the MD3 protocol in its dark scheme.
fn environment(language: &Arc<Language>) -> Arc<Env> {
    let catalogued = Env::with(Some(Env::root()), Arc::clone(language));
    Env::with_scope(
        Some(catalogued),
        StyleScope::new(Appearance::Dark),
        Md3Options::default(),
    )
}

/// 命令行的脚本路径：不想开窗时用它验证「保存 + 通知 daemon」。
struct Script {
    backend: Option<Option<String>>,
    format: Option<Option<String>>,
    directory: Option<String>,
    program: Option<String>,
    lang: Option<LanguageChoice>,
    shortcuts: Vec<(String, String)>,
}

impl Script {
    /// The flags as clap parsed them; `auto` is the absence of an override, and a flag that
    /// is not given leaves the file's own value alone.
    fn of(args: &crate::cli::SettingsArgs) -> Result<Script, String> {
        Ok(Script {
            backend: choice(args.backend.as_deref(), BACKENDS, "--backend")?,
            format: choice(args.format.as_deref(), &FORMATS, "--format")?,
            directory: args.directory.clone(),
            program: args.ffmpeg.clone(),
            lang: match args.ui_language.as_deref() {
                None => None,
                Some(value) => Some(
                    LanguageChoice::parse(value).map_err(|error| format!("--ui-language: {error}"))?,
                ),
            },
            shortcuts: args
                .shortcut
                .iter()
                .map(|value| match value.split_once('=') {
                    Some((name, chord)) => {
                        Ok((name.trim().to_string(), chord.trim().to_string()))
                    }
                    None => Err(format!("--shortcut expects NAME=CHORD, got {value}")),
                })
                .collect::<Result<Vec<_>, String>>()?,
        })
    }

    fn apply(self, config: &mut Config) -> Result<(), String> {
        if let Some(backend) = self.backend {
            config.capture.backend = backend;
        }
        if let Some(format) = self.format {
            config.shots.format = format;
        }
        if let Some(directory) = self.directory {
            config.shots.directory = typed_path(&directory);
        }
        if let Some(program) = self.program {
            config.codec.program = typed_path(&program);
        }
        if let Some(choice) = self.lang {
            config.ui.language = Some(choice.to_string());
        }
        for (name, chord) in self.shortcuts {
            let action = Shortcut::from_name(&name)
                .ok_or_else(|| format!("--shortcut: {name} is not an action"))?;
            if chord.is_empty() {
                config.shortcuts.0.remove(action.name());
            } else {
                parse_chords(&chord).map_err(|error| format!("--shortcut {name}: {error}"))?;
                config.shortcuts.0.insert(action.name().to_string(), chord);
            }
        }
        Ok(())
    }
}

/// 输入框里的路径：全是空白等于没填。
fn typed_path(text: &str) -> Option<std::path::PathBuf> {
    typed(text).map(std::path::PathBuf::from)
}

/// 输入框里的自由值：全是空白等于没填。
fn typed(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// What the window says about a chord it cannot read.
fn chord_message(catalog: &Catalog, name: &str, text: &str) -> String {
    let mut args = ui_i18n::Args::new();
    args.set("name", name.to_string());
    args.set("chord", text.to_string());
    catalog.text_by_id_with("settings-bad-chord", &args).to_string()
}

/// One flag holding a choice: absent stays absent, `auto` is the absence of an override, and
/// anything else has to be one this build offers.
fn choice(
    value: Option<&str>,
    options: &[&'static str],
    flag: &str,
) -> Result<Option<Option<String>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !options.contains(&value) {
        return Err(format!("{flag} expects one of {}", options.join(", ")));
    }
    Ok(Some((value != "auto").then(|| value.to_string())))
}

struct SettingsWindow {
    /// The free-form values, as the fields hold them; saved into `config` on Save.
    /// One entry per `Shortcut::ALL`, in that order.
    shortcuts: Vec<String>,
    /// The language item's state; applying it rebuilds the window's own words at once.
    choice: LanguageChoice,
    language: Arc<Language>,
    directory: String,
    program: String,
    config: Config,
    catalog: Arc<Catalog>,
    env: Arc<Env>,
    kit: Md3Kit,
    /// What the last Save did, shown under the buttons.
    status: Option<String>,
    /// Set by Close, drained by the next build.
    close: bool,
}

impl SettingsWindow {
    /// One free-form value: the app owns the buffer, the engine edits it and reports the text.
    fn field(
        &self,
        id: Id,
        label: &str,
        hint: &str,
        text: &str,
        wcx: &mut Cx<'_, '_>,
    ) -> Box<dyn Element> {
        let params = TextFieldParams::new(id, TextFieldVariant::Outlined, FIELD_WIDTH)
            .label(label)
            .placeholder(hint)
            .text(text);
        self.control(params, wcx)
    }

    fn control(&self, params: TextFieldParams<'_>, wcx: &mut Cx<'_, '_>) -> Box<dyn Element> {
        let control = self.kit.text_field(params, wcx);
        wcx.element(control)
    }

    /// A field whose label and hint come from the catalog.
    fn catalogued(
        &self,
        id: &'static str,
        label_key: &str,
        hint_key: &str,
        text: &str,
        wcx: &mut Cx<'_, '_>,
    ) -> Box<dyn Element> {
        let label = self.catalog.text_by_id(label_key).to_string();
        let hint = self.catalog.text_by_id(hint_key).to_string();
        self.field(Id::str(id), &label, &hint, text, wcx)
    }

    /// A chord the parser refuses is reported before the file is written.
    fn chord_error(&self) -> Option<String> {
        for (index, action) in Shortcut::ALL.iter().enumerate() {
            let text = self.shortcuts[index].trim();
            if text.is_empty() {
                continue;
            }
            if parse_chords(text).is_err() {
                return Some(chord_message(&self.catalog, action.name(), text));
            }
        }
        None
    }

    /// Take the fields' text into the configuration; an empty field means no override, except
    /// where the file already cleared the action — a save must not quietly rebind that key.
    fn absorb(&mut self) {
        for (index, action) in Shortcut::ALL.iter().enumerate() {
            let text = self.shortcuts[index].trim();
            let cleared = self
                .config
                .shortcuts
                .0
                .get(action.name())
                .is_some_and(|value| value.trim().is_empty());
            if !text.is_empty() {
                self.config
                    .shortcuts
                    .0
                    .insert(action.name().to_string(), text.to_string());
            } else if !cleared {
                self.config.shortcuts.0.remove(action.name());
            }
        }
        self.config.ui.language = Some(self.choice.to_string());
        self.config.shots.directory = typed_path(&self.directory);
        self.config.codec.program = typed_path(&self.program);
    }

    /// Apply a language at once: the window is built from the same object it changes.
    fn select(&mut self, choice: LanguageChoice) {
        if let Err(error) = self.language.set_choice(choice.clone()) {
            self.status = Some(format!("language: {error:?}"));
            return;
        }
        self.choice = choice;
        self.catalog = self.language.catalog();
        self.config.ui.language = Some(self.choice.to_string());
    }

    /// One button of a choice group, filled when it is the current value.
    fn option(
        &self,
        id: Id,
        text: String,
        selected: bool,
        wcx: &mut Cx<'_, '_>,
    ) -> Box<dyn Element> {
        let params = ButtonParams::new(id, text)
            .style(if selected {
                ButtonStyle::Filled
            } else {
                ButtonStyle::Outlined
            })
            .size(ButtonSize::Small);
        let control = self.kit.button(params, wcx);
        wcx.element(control)
    }

    fn save(&mut self) {
        self.absorb();
        if let Some(error) = self.chord_error() {
            self.status = Some(error);
            return;
        }
        match store(&self.config) {
            Ok(note) => {
                self.status =
                    Some(note.unwrap_or_else(|| self.catalog.text_by_id("settings-saved").to_string()));
            }
            Err(error) => self.status = Some(error),
        }
    }
}

/// Write the file, then ask the daemon to re-read it; a daemon that is not running is not an
/// error, because the file is what it reads next time.
fn store(config: &Config) -> Result<Option<String>, String> {
    config.save_to(&crabsnap_platform::paths::config_path())?;
    match crate::run::request(Request::Reload)? {
        None | Some(Response::Ok) => Ok(None),
        Some(Response::Error(error)) => Ok(Some(error)),
        Some(other) => Ok(Some(format!("unexpected answer: {other:?}"))),
    }
}

impl App for SettingsWindow {
    fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        if let ui_core::change::Change::Text(next) = change {
            if id == Id::str(DIRECTORY_FIELD) {
                self.directory = next;
                return true;
            }
            if id == Id::str(PROGRAM_FIELD) {
                self.program = next;
                return true;
            }
            for (index, action) in Shortcut::ALL.iter().enumerate() {
                if id == shortcut_id(action.name()) {
                    self.shortcuts[index] = next;
                    return true;
                }
            }
            return false;
        }
        if !matches!(change, ui_core::change::Change::Activated) {
            return false;
        }
        for &name in BACKENDS {
            if id == option_id("backend", name) {
                // `auto` is the absence of an override, not a backend called auto.
                self.config.capture.backend = (name != "auto").then(|| name.to_string());
                return true;
            }
        }
        for name in FORMATS {
            if id == option_id("format", name) {
                self.config.shots.format = (name != "auto").then(|| name.to_string());
                return true;
            }
        }
        if id == option_id("lang", "auto") {
            self.select(LanguageChoice::Auto);
            return true;
        }
        for (tag, _) in ui_i18n::language_names() {
            if id != option_id("lang", tag) {
                continue;
            }
            match tag.parse() {
                Ok(locale) => self.select(LanguageChoice::Fixed(locale)),
                Err(error) => self.status = Some(format!("language: {error}")),
            }
            return true;
        }
        if id == action_id("save") {
            self.save();
            return true;
        }
        if id == action_id("close") {
            self.close = true;
            return true;
        }
        false
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        if self.close {
            cx.close_window();
        }
        let mut wcx = Cx::new(cx, Arc::clone(&self.env));
        // The window is a surface of the same scheme its controls resolve against.
        let pack = self.kit.pack_for(&wcx);
        let ink = pack.ink("onSurface").unwrap_or(Color::WHITE);
        let muted = pack.ink("onSurfaceVariant").unwrap_or(ink);
        let surface = pack.ink("surface").unwrap_or(Color::BLACK);
        // The fields outgrow a fixed window, so the whole form scrolls: the window is a
        // scroller with one natural-height column inside it.
        let mut root = ui_platform::Container::new(Id::str("settings"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(24.0))
            .gap(16.0);
        root = root.child_decl(label(
            "settings/title",
            self.catalog.text_by_id("settings-title"),
            20.0,
            ink,
        ));

        for (group, options, current) in [
            ("backend", BACKENDS, self.config.capture.backend.as_deref()),
            ("format", &FORMATS[..], self.config.shots.format.as_deref()),
        ] {
            let heading = self
                .catalog
                .text_by_id(&format!("settings-{group}-label"))
                .to_string();
            root = root.child_decl(label(
                match group {
                    "backend" => "settings/backend-label",
                    _ => "settings/format-label",
                },
                Arc::from(heading),
                13.0,
                muted,
            ));
            let mut row =
                ui_platform::Container::new(Id::of(("settings-row", group)), NodeKind::Row)
                    .gap(8.0);
            for name in options.iter().copied() {
                let selected = match (group, current) {
                    ("backend", current) => current == (name != "auto").then_some(name),
                    (_, current) => current == (name != "auto").then_some(name),
                };
                let text = self
                    .catalog
                    .text_by_id(&format!("settings-{group}-{name}"))
                    .to_string();
                let params = ButtonParams::new(option_id(group, name), text)
                    .style(if selected {
                        ButtonStyle::Filled
                    } else {
                        ButtonStyle::Outlined
                    })
                    .size(ButtonSize::Small);
                let control = self.kit.button(params, &mut wcx);
                row = row.child(wcx.element(control));
            }
            root = root.child(Box::new(row));
        }

        // The language item is a choice, not a tag to type: `Auto`, then one button per
        // shipped language, each written in its own script.
        let heading = self.catalog.text_by_id("settings-lang-label").to_string();
        root = root.child_decl(label(
            "settings/lang-label",
            Arc::from(heading),
            13.0,
            muted,
        ));
        let mut row =
            ui_platform::Container::new(Id::str("settings/lang-row"), NodeKind::Row).gap(8.0);
        let auto = self.catalog.text_by_id("settings-lang-auto").to_string();
        let selected = matches!(self.choice, LanguageChoice::Auto);
        row = row.child(self.option(option_id("lang", "auto"), auto, selected, &mut wcx));
        for (tag, name) in ui_i18n::language_names() {
            let selected = matches!(&self.choice, LanguageChoice::Fixed(locale) if *locale == tag);
            row = row.child(self.option(
                option_id("lang", tag),
                name.to_string(),
                selected,
                &mut wcx,
            ));
        }
        root = root.child(Box::new(row));
        root = root.child_decl(label(
            "settings/shortcuts-label",
            self.catalog.text_by_id("settings-shortcuts-label"),
            13.0,
            muted,
        ));
        for (index, action) in Shortcut::ALL.iter().enumerate() {
            // The action's own name is the schema's; the catalog gives it a word a reader
            // knows, and a key that is missing falls back to the schema rather than to blank.
            let label = self
                .catalog
                .text_by_id(&format!("settings-shortcut-{}", action.name()))
                .to_string();
            let mut hint_args = ui_i18n::Args::new();
            hint_args.set("chord", action.default_chords().join(", "));
            let hint = self
                .catalog
                .text_by_id_with("settings-shortcut-hint", &hint_args)
                .to_string();
            // The built-in chord is worth reading only while it is the one in effect: an
            // empty field shows it under itself, where it stays readable, and a filled one
            // shows its own value instead.
            let cleared = self
                .config
                .shortcuts
                .0
                .get(action.name())
                .is_some_and(|value| value.trim().is_empty());
            let cleared_hint = self.catalog.text_by_id("settings-shortcut-cleared");
            let mut params = TextFieldParams::new(
                shortcut_id(action.name()),
                TextFieldVariant::Outlined,
                FIELD_WIDTH,
            )
            .label(&label)
            .text(&self.shortcuts[index]);
            params = if !self.shortcuts[index].trim().is_empty() {
                params.placeholder(&hint)
            } else if cleared {
                params.supporting(&cleared_hint)
            } else {
                params.supporting(&hint)
            };
            root = root.child(self.control(params, &mut wcx));
        }
        root = root.child(self.catalogued(
            DIRECTORY_FIELD,
            "settings-directory-label",
            "settings-directory-hint",
            &self.directory,
            &mut wcx,
        ));
        root = root.child(self.catalogued(
            PROGRAM_FIELD,
            "settings-ffmpeg-label",
            "settings-ffmpeg-hint",
            &self.program,
            &mut wcx,
        ));

        let mut note = ui_i18n::Args::new();
        note.set("path", crabsnap_platform::paths::config_path().display().to_string());
        let note_text = self
            .catalog
            .text_by_id_with("settings-file-note", &note)
            .to_string();
        root = root.child_decl(label("settings/note", Arc::from(note_text), 12.0, muted));

        let mut actions =
            ui_platform::Container::new(Id::str("settings/actions"), NodeKind::Row).gap(8.0);
        let save = ButtonParams::new(
            action_id("save"),
            self.catalog.text_by_id("settings-save").to_string(),
        )
        .style(ButtonStyle::Filled)
        .size(ButtonSize::Small);
        let control = self.kit.button(save, &mut wcx);
        actions = actions.child(wcx.element(control));
        let close = ButtonParams::new(
            action_id("close"),
            self.catalog.text_by_id("settings-close").to_string(),
        )
        .style(ButtonStyle::Text)
        .size(ButtonSize::Small);
        let control = self.kit.button(close, &mut wcx);
        actions = actions.child(wcx.element(control));
        root = root.child(Box::new(actions));

        if let Some(status) = &self.status {
            root = root.child_decl(label(
                "settings/status",
                Arc::from(status.as_str()),
                12.0,
                ink,
            ));
        }
        let scroller = ui_platform::Container::new(Id::str("settings/scroll"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .sense(Sense::HOVER | Sense::SCROLL_OWNED)
            .scrollable()
            .child(Box::new(root));
        let window = ui_platform::Container::new(Id::str("settings/window"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .background(surface, Corners::all(0.0))
            .child(Box::new(scroller));
        Box::new(window)
    }
}

fn option_id(group: &'static str, name: &'static str) -> Id {
    Id::of(("settings-option", group, name))
}

fn action_id(name: &'static str) -> Id {
    Id::of(("settings-action", name))
}

fn label(id: &'static str, body: Arc<str>, size: f32, ink: Color) -> Decl {
    let mut decl = Decl::container(Id::str(id), NodeKind::Text).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(size, size * 1.4),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    decl.data = ui_core::NodeData::Text {
        block: ui_core::TextBlock::plain(body.to_string(), ui_core::TextStyle::new(size)).into(),
        ink,
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl
}
