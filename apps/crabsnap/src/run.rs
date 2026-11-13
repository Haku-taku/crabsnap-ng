//! 命令分发：每条命令做什么。
//!
//! 抓取在本进程执行；贴图、退出与重载走本地通道交给 daemon。没有 daemon 时命令明确说出
//! 这一点，而不是悄悄换一种行为：第二个 daemon 会把贴图分裂到两边。

use std::sync::Arc;

use crabsnap_core::config::Config;
use crabsnap_core::media::Kind;
use crabsnap_core::protocol::{PinInfo, PinOrigin, PinSource, Request, Response};
use crabsnap_platform::{Client, ClientError};
use ui_i18n::{Catalog, Language};
use ui_platform::host::Backend as _;

use crate::cli::{
    CompletionsArgs, InfoArgs, InfoSub, Invocation, OutputArgs, PinSub, ShotArgs, ShotSub, Sub,
};

pub fn run(language: &Arc<Language>, invocation: Invocation) -> Result<(), String> {
    let no_daemon = invocation.no_daemon;
    match invocation.command {
        Sub::Shot(shot) => run_shot(language, &shot, no_daemon),
        Sub::Pin(pin) => run_pin(pin.command, no_daemon, language),
        Sub::Info(info) => run_info(info, no_daemon),
        Sub::Completions(args) => run_completions(&language.catalog(), args),
        Sub::Daemon(args) => crate::daemon::run(args.idle, Arc::clone(language)),
        Sub::Quit if no_daemon => Err("no daemon is running".into()),
        Sub::Quit => ask(Request::Quit, "asked the daemon to quit"),
        Sub::Reload if no_daemon => Err("no daemon is running".into()),
        Sub::Reload => ask(Request::Reload, "asked the daemon to reload"),
    }
}

/// 发一条没有返回值的请求。
fn ask(request: Request, done: &str) -> Result<(), String> {
    let mut client = Client::connect().map_err(|error| error.message())?;
    match client.request(&request).map_err(|error| error.message())? {
        Response::Ok => {
            println!("{done}");
            Ok(())
        }
        Response::Error(error) => Err(error),
        other => Err(format!("unexpected answer: {other:?}")),
    }
}

/// 连上 daemon 发一条请求；没有 daemon 时由调用者决定怎么办。
fn request(request: Request) -> Result<Option<Response>, String> {
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(ClientError::NoDaemon) => return Ok(None),
        Err(error) => return Err(error.message()),
    };
    client
        .request(&request)
        .map(Some)
        .map_err(|error| error.message())
}

fn run_shot(language: &Arc<Language>, shot: &ShotArgs, no_daemon: bool) -> Result<(), String> {
    match &shot.command {
        Some(ShotSub::Region(args)) => crate::shoot_region(&shot.common, args.demo, language),
        Some(ShotSub::Output(args)) => {
            if args.clipboard && no_daemon {
                return Err("no daemon is running: the clipboard must outlive this command".into());
            }
            crate::shoot_output(&shot.common, args, language)
        }
        // 省略抓取方式等于抓一个 output，和旧项目的默认一致。
        None => crate::shoot_output(&shot.common, &OutputArgs::default(), language),
    }
}

fn run_pin(command: PinSub, no_daemon: bool, language: &Arc<Language>) -> Result<(), String> {
    match command {
        PinSub::List => {
            if no_daemon {
                return Err("no daemon is running".into());
            }
            let Some(answer) = request(Request::PinList)? else {
                return Err("no daemon is running".into());
            };
            match answer {
                Response::Pins(pins) if pins.is_empty() => {
                    println!("no pins");
                    Ok(())
                }
                Response::Pins(pins) => {
                    for pin in pins {
                        print_pin(&pin);
                    }
                    Ok(())
                }
                Response::Error(error) => Err(error),
                other => Err(format!("unexpected answer: {other:?}")),
            }
        }
        PinSub::Add(args) => {
            let mut as_kind =
                match args.as_kind.as_deref() {
                    Some(token) => Some(Kind::from_name(token).ok_or_else(|| {
                        format!("unknown kind {token}: {}", Kind::NAMES.join(", "))
                    })?),
                    None => None,
                };
            // `--palette-from` is one file asked for as a palette: the factory then
            // quantizes what it decoded instead of reading the bytes as a GIMP file.
            let files: Vec<std::path::PathBuf> = match &args.palette_from {
                Some(path) => {
                    if as_kind.is_some() {
                        return Err("give either --palette-from or --as, not both".into());
                    }
                    as_kind = Some(Kind::Text(crabsnap_core::media::TextKind::Palette));
                    vec![path.clone()]
                }
                None => args.file.clone(),
            };
            let output = args.output.unwrap_or(0);
            let source = match (files.is_empty(), args.colour.as_deref(), args.clipboard) {
                (false, None, false) => PinSource::Paths(
                    files
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                ),
                (true, Some(code), false) => PinSource::Colour(code.to_string()),
                (true, None, true) => PinSource::Clipboard,
                (true, None, false) => {
                    return Err(
                        "pin add needs --file PATH..., --color CODE, --clipboard or --palette-from PATH"
                            .into(),
                    );
                }
                _ => {
                    return Err(
                        "give one of --file, --color, --clipboard or --palette-from, not two"
                            .into(),
                    );
                }
            };
            // The clipboard has no local form: the selection belongs to a process still
            // running, so only a daemon can hand it a pin.
            let local = match (args.colour.as_deref(), args.clipboard) {
                (None, true) => None,
                (Some(code), _) => Some(LocalPin::Colour(code)),
                (None, false) => Some(LocalPin::Files(&files)),
            };
            if no_daemon {
                return pin_where_possible(local, as_kind, output, language);
            }
            // 贴图必须归同一条连接：daemon 在跑时另开一条，会得到两套互不可见的贴图。
            let asked = Request::PinAdd {
                source,
                as_kind,
                output: args.output,
            };
            match request(asked)? {
                Some(Response::Ok) => Ok(()),
                Some(Response::Error(error)) => Err(error),
                Some(other) => Err(format!("unexpected answer: {other:?}")),
                None => pin_where_possible(local, as_kind, output, language),
            }
        }
        PinSub::Export(args) => pin_command(Request::PinExport {
            id: args.id,
            path: args.file.to_string_lossy().into_owned(),
            format: args.format,
        }),
        PinSub::Edit(args) => pin_command(Request::PinEdit { id: args.id }),
        PinSub::Close(args) => pin_command(Request::PinClose { id: args.id }),
        PinSub::Show(args) => pin_command(Request::PinVisible {
            id: args.id,
            shown: Some(true),
        }),
        PinSub::Hide(args) => pin_command(Request::PinVisible {
            id: args.id,
            shown: Some(false),
        }),
        PinSub::Toggle(args) => pin_command(Request::PinVisible {
            id: args.id,
            shown: None,
        }),
    }
}

/// Ask the daemon to hold a PNG on the clipboard; the selection must outlive this process.
pub(crate) fn copy_image(path: &str) -> Result<(), String> {
    match request(Request::CopyImage {
        path: path.to_string(),
    })? {
        Some(Response::Ok) => Ok(()),
        Some(Response::Error(error)) => Err(error),
        Some(other) => Err(format!("unexpected answer: {other:?}")),
        None => Err("no daemon is running: the clipboard must outlive this command".into()),
    }
}

/// 一条只对 daemon 有意义的贴图命令：像素与表面都在它那边。
fn pin_command(asked: Request) -> Result<(), String> {
    match request(asked)? {
        Some(Response::Ok) => Ok(()),
        Some(Response::Error(error)) => Err(error),
        Some(other) => Err(format!("unexpected answer: {other:?}")),
        None => Err("no daemon is running".into()),
    }
}

/// What a local `pin add` puts on the screen: files it must read and classify, or one
/// colour code the user stated outright.
enum LocalPin<'a> {
    Files(&'a [std::path::PathBuf]),
    Colour(&'a str),
}

/// A `pin add` the daemon could not take: a source with a local form is drawn here, and one
/// without says why it cannot be.
fn pin_where_possible(
    local: Option<LocalPin<'_>>,
    as_kind: Option<Kind>,
    output: usize,
    language: &Arc<Language>,
) -> Result<(), String> {
    match local {
        Some(local) => pin_locally(local, as_kind, output, language),
        None => Err("no daemon is running: the clipboard must be read where pins live".into()),
    }
}

/// 把本地的内容贴到屏上，直到用户关掉所有贴图；没有 daemon 时命令就是这样执行的。
fn pin_locally(
    local: LocalPin<'_>,
    as_kind: Option<Kind>,
    output: usize,
    language: &Arc<Language>,
) -> Result<(), String> {
    let mut backend = ui_host::connect().map_err(|error| format!("the display: {error}"))?;
    let outputs = backend.outputs();
    let Some(target) = outputs.get(output) else {
        return Err(format!(
            "this display reports {} output(s); there is no output {}",
            outputs.len(),
            output + 1
        ));
    };
    let screen = (target.width, target.height);
    let dots = target.scale;
    // The settings load before the files open: what reads a picture is the same authority
    // that writes one, and it comes from them.
    let settings = crate::Settings::load(Arc::clone(language))?;
    let (opened, errors) = match local {
        LocalPin::Files(files) => {
            crate::pin::open_all(files, dots, as_kind, settings.codecs.ffmpeg())
        }
        LocalPin::Colour(code) => match crate::pin::open_colour(code) {
            Ok(content) => (
                vec![crate::pin::Opened {
                    index: 0,
                    content,
                    origin: crabsnap_core::protocol::PinOrigin::Colour {
                        code: code.to_string(),
                    },
                }],
                Vec::new(),
            ),
            Err(error) => (Vec::new(), vec![error]),
        },
    };
    // 坏文件现在就报：贴图已经上屏，用户不该等关掉它们才知道少了哪一张。
    for error in &errors {
        eprintln!("{error}");
    }
    if opened.is_empty() {
        return Err(match errors.first() {
            Some(error) => error.clone(),
            None => "nothing could be pinned".into(),
        });
    }
    let staged = opened
        .into_iter()
        .map(|item| {
            let position = crate::pins::cascade_at(screen, item.content.size(), item.index);
            crate::Staged {
                content: item.content,
                origin: item.origin,
                position,
            }
        })
        .collect();
    let source = Some(target.name.clone());
    crate::show_pins(backend, staged, screen, dots, source, language, settings)?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} item(s) could not be pinned", errors.len()))
    }
}

/// `pin list` 的一行：来源，以及来源没写明的输出。
fn print_pin(pin: &PinInfo) {
    let on = match (&pin.origin, &pin.output) {
        (PinOrigin::Capture { .. }, _) | (_, None) => String::new(),
        (_, Some(output)) => format!(" on {output}"),
    };
    println!(
        "{} {} {} {}x{} at ({:.0}, {:.0}) {}{on}{}",
        pin.id,
        pin.kind.name(),
        pin.title.as_deref().unwrap_or("-"),
        pin.width,
        pin.height,
        pin.x,
        pin.y,
        pin.origin.describe(),
        if pin.shown { "" } else { " (hidden)" }
    );
}

fn run_info(info: InfoArgs, no_daemon: bool) -> Result<(), String> {
    match info.command {
        InfoSub::Config => {
            let path = crabsnap_platform::paths::config_path();
            let config = Config::load(&path)?;
            println!("{}", path.display());
            println!(
                "file: {}",
                if path.exists() {
                    "present"
                } else {
                    "not found; defaults"
                }
            );
            println!(
                "capture.backend: {}",
                config.capture.backend.as_deref().unwrap_or("auto")
            );
            let directory = crabsnap_platform::paths::shot_dir(config.shots.directory.as_deref());
            println!("shots.directory: {}", directory.display());
            println!(
                "shots.format: {}",
                config.shots.format.as_deref().unwrap_or("png")
            );
            println!(
                "codec.library_directory: {}",
                config
                    .codec
                    .library_directory
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "the dynamic loader's own search path".to_string())
            );
            println!(
                "codec.program: {}",
                config
                    .codec
                    .program
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "found on PATH, only when a library is missing".to_string())
            );
            for (key, value) in &config.codec.options {
                println!("codec.options.{key}: {value}");
            }
            let plan = crate::hotkeys::Plan::resolve(&config);
            for action in crabsnap_core::hotkeys::Shortcut::ALL {
                println!("shortcuts.{}: {}", action.name(), plan.describe(action));
            }
            for problem in &plan.problems {
                println!("shortcut problem: {problem}");
            }
            let (look, problems) = config.pin_look();
            println!("pin.radius: {}", look.radius);
            for (name, skin) in [("idle", look.idle), ("active", look.active)] {
                println!(
                    "pin.{name}: border {}px #{:02X}{:02X}{:02X}{:02X}, shadow blur {} spread {} alpha {:.2} #{:02X}{:02X}{:02X}",
                    skin.border_width,
                    skin.border[0],
                    skin.border[1],
                    skin.border[2],
                    skin.border[3],
                    skin.shadow_blur,
                    skin.shadow_spread,
                    skin.shadow_alpha,
                    skin.shadow[0],
                    skin.shadow[1],
                    skin.shadow[2]
                );
            }
            for problem in &problems {
                println!("pin problem: {problem}");
            }
            let choice = match config.ui.language.as_deref() {
                None => ui_i18n::LanguageChoice::Auto,
                Some(tag) => match ui_i18n::LanguageChoice::parse(tag) {
                    Ok(choice) => choice,
                    Err(error) => {
                        println!("ui.language: {tag} ({error}; following the system)");
                        ui_i18n::LanguageChoice::Auto
                    }
                },
            };
            println!("ui.language: {choice}");
            match crate::load_language(choice) {
                Ok(language) => println!("ui.language resolved: {}", language.resolved()),
                Err(error) => println!("ui.language: {error}"),
            }
            let system = ui_platform::posix_system_languages();
            if system.is_empty() {
                println!("system languages: none");
            } else {
                println!("system languages: {}", system.join(", "));
            }
            if !config.unknown.is_empty() {
                println!("ignored keys: {}", config.unknown.join(", "));
            }
            Ok(())
        }
        InfoSub::Formats(formats) => {
            let verbose = formats.verbose;
            let config = Config::load(&crabsnap_platform::paths::config_path())?;
            let codecs = crate::codec::Codecs::probe(
                config.codec.program.as_deref(),
                config.codec.library_directory.as_deref(),
                &config.codec.options,
            );
            if let Some(reason) = codecs.failure() {
                println!("codec: {reason}");
            }
            if let Some(reason) = codecs.ffmpeg().failure() {
                println!("ffmpeg: {reason}");
            }
            for encoding in codecs.available() {
                println!("{} ({})", encoding.name(), codecs.source(*encoding));
            }
            let ffmpeg = codecs.ffmpeg();
            for format in ffmpeg.formats() {
                if let Some(muxer) = &format.muxer {
                    println!("  {} muxer: {muxer}", format.extension);
                }
                let depth = match ffmpeg.deep_colour(format) {
                    Some(true) => "deep colour",
                    Some(false) => "8-bit",
                    None => "depth unknown",
                };
                let options = ffmpeg.options(format);
                println!(
                    "  {} encoder: {} ({}, {depth}, {} options)",
                    format.extension,
                    format.encoder,
                    format.description,
                    options.len()
                );
                if verbose {
                    for option in options {
                        println!(
                            "    -{:<24} default {:<10} {}",
                            option.name, option.default, option.help
                        );
                    }
                }
            }
            if ffmpeg.source().is_none() {
                let reason = ffmpeg.failure().unwrap_or_else(|| "no ffmpeg".into());
                println!("extended formats need ffmpeg: {reason}");
            }
            Ok(())
        }
        InfoSub::Status if no_daemon => crate::status(),
        InfoSub::Status => match request(Request::Status)? {
            Some(Response::Status(status)) => {
                println!("daemon: running");
                println!("capture: {}", status.capture);
                if status.outputs.is_empty() {
                    println!("outputs: none reported");
                }
                for output in status.outputs {
                    println!("output: {output}");
                }
                println!("pins: {}", status.pins);
                for line in &status.hotkeys {
                    println!("hotkey: {line}");
                }
                Ok(())
            }
            Some(Response::Error(error)) => Err(error),
            Some(other) => Err(format!("unexpected answer: {other:?}")),
            // 没有 daemon 时问本进程：能答的只有抓取后端。
            None => crate::status(),
        },
    }
}

fn run_completions(catalog: &Arc<Catalog>, args: CompletionsArgs) -> Result<(), String> {
    let mut command = crate::cli::command(catalog);
    let name = command.get_name().to_string();
    clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
    Ok(())
}
