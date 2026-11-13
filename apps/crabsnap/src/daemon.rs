//! 常驻服务：一条连接、一条本地通道。
//!
//! 能力在启动时解析一次（抓取后端已经探测好了），之后每条命令只走通道问答——这正是
//! daemon 存在的理由：不按命令重复探测，也不让第二个进程持有另一条连接。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crabsnap_core::hotkeys::Shortcut;
use crabsnap_core::media::Kind;
use crabsnap_core::protocol::{self, Op, PinSource, Request, Response, Status};
use ui_core::draw::ImageEntry;
use ui_i18n::Language;
use ui_platform::SurfaceId;
use ui_platform::host::{Effect, Host, MessageCtx, OpenOptions};
use ui_platform::hotkeys::GlobalHotkeyError;
use ui_platform::ipc::Reply;

use crate::Settings;
use crate::capture;
use crate::hotkeys::{self, Plan};
use crate::pins::{self, Pins};
use crate::{FROZEN_TEXTURE, Overlay, Pin, overlay_surface};

/// 服务的状态：命令能问到的都在这儿。
struct Service {
    /// 启动时解析出的抓取后端名。
    capture: String,
    /// 用户配置与这台机器的编码能力；`reload` 会重新读、重新探测。
    settings: Arc<Settings>,
    /// 屏上的贴图；daemon 自己不持有 surface，只持有它们背后的列表。
    pins: Pins,
    /// 当前绑定的热键，`reload` 先按它释放再重新解析。
    hotkeys: Plan,
    /// 绑定的结果，`info status` 原样报告。
    hotkey_lines: Vec<String>,
    /// 抓取要借的那条连接；reload 也靠它重新探测。
    grab: capture::Session,
    /// 屏幕上那块 overlay 交回的贴图，每回合收一次。
    pin_outbox: Rc<RefCell<Vec<Pin>>>,
    /// 当前那块 overlay；`None` 或已经关掉时，下一次按键才允许再开。
    overlay: Option<SurfaceId>,
    /// 已经请了、还没到的一帧：循环每回合推它一次，而不是在回合里等。
    pending: Option<(Box<dyn capture::Pending>, CaptureUse)>,
    /// 后台工作：复制大图时的解码与编码不该卡住这一轮。
    jobs: ui_platform::jobs::Jobs,
    /// 还在编码的复制，以及它们欠对端的那条应答。
    copy: Vec<PendingCopy>,
    /// 一个已请但还没到的剪贴板材料：命令已经答过了，这回合读到再贴。
    selection: Option<PendingSelection>,
}

/// A selection being read off the loop; the command was answered when it was asked for.
struct PendingSelection {
    as_kind: Option<Kind>,
    output: Option<usize>,
}

/// A PNG being encoded off the loop; the reply waits with it.
struct PendingCopy {
    job: ui_platform::jobs::Job<Result<Vec<u8>, String>>,
    reply: Box<dyn Reply + Send>,
}

/// What a capture is for, so the turn that finishes it knows what to do with the frame.
#[derive(Clone)]
enum CaptureUse {
    Region,
    Output,
    /// A palette waits for the pixel under a press: the frame answers it.
    Pick(crate::pin::PickRequest),
}

/// 运行到 `quit` 请求，或空闲超时。
pub fn run(idle: Option<u64>, language: Arc<Language>) -> Result<(), String> {
    // 已有 daemon 时不再起一个：第二个进程永远接收不到请求，贴图会被分裂到两边。
    if crabsnap_platform::Client::connect().is_ok() {
        return Err("a daemon is already running".into());
    }
    let backend = ui_host::connect().map_err(|error| format!("the display: {error}"))?;
    // 后端随后被 host 独占，抓取要借的那份连接在它还在手上时取下。
    let grab = capture::Session::of(&backend)?;
    let settings = Settings::load(language)?;
    let capture = probe(&grab, &settings)?;
    let mut host = Host::new(Box::new(backend));
    let service = Rc::new(RefCell::new(Service {
        capture,
        pins: Pins::new(
            settings.look,
            settings
                .shortcuts
                .chords(crabsnap_core::hotkeys::Place::Pin),
        ),
        settings,
        hotkeys: Plan::default(),
        hotkey_lines: Vec::new(),
        grab,
        pin_outbox: Rc::new(RefCell::new(Vec::new())),
        overlay: None,
        pending: None,
        jobs: host.jobs(),
        copy: Vec::new(),
        selection: None,
    }));
    {
        let mut borrowed = service.borrow_mut();
        bind_hotkeys(&mut borrowed, &mut host.context());
    }
    // A press acts on this process's own connection: the capture happens here, and the
    // overlay is a surface of this loop, so nothing is handed to another process.
    let pressed = Rc::clone(&service);
    host.on_hotkey(move |id, cx| {
        let Some(action) = Shortcut::ALL
            .into_iter()
            .find(|action| hotkeys::id(*action) == id)
        else {
            return Effect::None;
        };
        let mut service = pressed.borrow_mut();
        let result = match action {
            Shortcut::Region => start_capture(&mut service, CaptureUse::Region, cx),
            Shortcut::Output => start_capture(&mut service, CaptureUse::Output, cx),
            // Only the two above are bound with the desktop, so nothing else can arrive.
            other => Err(format!("{} is not a global action", other.name())),
        };
        if let Err(error) = result {
            eprintln!("hotkey {}: {error}", action.name());
        }
        Effect::None
    });
    let served = Rc::clone(&service);
    let last = Rc::new(Cell::new(Instant::now()));
    let activity = Rc::clone(&last);
    let address = crabsnap_platform::address();
    host.listen(&address, move |reply, bytes, cx| {
        activity.set(Instant::now());
        answer(&mut served.borrow_mut(), reply, bytes, cx)
    })
    .map_err(|error| format!("channel {address}: {error}"))?;
    let window = idle.filter(|seconds| *seconds > 0).map(Duration::from_secs);
    let served = Rc::clone(&service);
    host.on_turn(move |cx, turn| {
        let mut service = served.borrow_mut();
        // The idle deadline ends the daemon; a capture's own only bounds the wait, because
        // the loop must come back to read the frame the compositor sends.
        let mut wake = None;
        if let Some(window) = window {
            let idle = last.get() + window;
            if Instant::now() >= idle {
                turn.quit();
            }
            wake = Some(idle);
        }
        if let Some((mut pending, what)) = service.pending.take() {
            match pending.poll() {
                // Not yet: keep it and wake the loop when it would time out.
                None => {
                    let at = pending.deadline();
                    wake = Some(wake.map_or(at, |wake| wake.min(at)));
                    service.pending = Some((pending, what));
                }
                Some(Ok(frame)) => {
                    if let Err(error) = finish_capture(&mut service, what, frame, cx) {
                        eprintln!("capture: {error}");
                    }
                }
                Some(Err(error)) => eprintln!("capture: {error}"),
            }
        }
        // A palette asked for the colour under a press that found no pin: only this loop
        // can capture the screen, so the wait is answered here.
        if service.pending.is_none()
            && let Some(request) = service.pins.take_pick()
            && let Err(error) = start_capture(&mut service, CaptureUse::Pick(request.clone()), cx)
        {
            eprintln!("pick: {error}");
            if let Some(surface) = service.pins.picked(&request, None) {
                let _ = cx.request_frame(surface);
            }
        }
        // A copy finished encoding: publish it, then answer the peer that asked.
        let mut index = 0;
        while index < service.copy.len() {
            match service.copy[index].job.take() {
                None => index += 1,
                Some(Ok(png)) => {
                    let mut pending = service.copy.remove(index);
                    let held = cx.copy(
                        ui_platform::clipboard::ClipboardContent::Bytes {
                            mime: "image/png".into(),
                            data: std::sync::Arc::new(png),
                        },
                        ui_platform::clipboard::ClipboardKind::Selection,
                    );
                    let answer = if held {
                        Response::Ok
                    } else {
                        Response::Error("this compositor offers no clipboard".into())
                    };
                    // The effect is only whether to quit, which a copy never asks.
                    let _ = send(&mut *pending.reply, Op::CopyImage, answer);
                }
                Some(Err(error)) => {
                    let mut pending = service.copy.remove(index);
                    let _ = send(&mut *pending.reply, Op::CopyImage, Response::Error(error));
                }
            }
        }
        if let Some(deadline) = wake {
            turn.set_deadline(Some(deadline));
        }
        // A pin the overlay asked for becomes a canvas here, between two turns: the app may
        // ask, but only the loop owns the connection a new surface needs.
        let outbox = Rc::clone(&service.pin_outbox);
        pins::place_pending(&mut service.pins, &outbox, cx);
        // A clipboard read the command asked for: the bytes landed, so the pins appear.
        if service.selection.is_some()
            && let Some(selection) = cx.take_selection()
        {
            finish_selection(&mut service, selection, cx);
        }
    });
    println!("daemon: listening on {address}");
    host.run()?;
    Ok(())
}

/// 一条请求的答案，以及它是否要结束守护进程。
fn answer(
    service: &mut Service,
    reply: &mut dyn Reply,
    bytes: &[u8],
    cx: &mut MessageCtx<'_>,
) -> Effect {
    let request = match protocol::decode_request(bytes) {
        Ok(request) => request,
        Err(error) => {
            // 请求不可解析时不知道它想做哪件事；错误应答对任何操作都合法。
            send(reply, Op::Ping, Response::Error(error));
            return Effect::None;
        }
    };
    match request {
        Request::Ping => send(reply, Op::Ping, Response::Pong),
        Request::Quit => {
            send(reply, Op::Quit, Response::Ok);
            Effect::Quit
        }
        Request::Reload => match reload(service) {
            Ok(()) => {
                bind_hotkeys(service, cx);
                // The words changed with the language: repaint what is already on screen.
                let open = service.pins.surfaces().chain(service.overlay);
                for surface in open {
                    if let Err(error) = cx.request_frame(surface) {
                        eprintln!("reload: {error}");
                    }
                }
                send(reply, Op::Reload, Response::Ok)
            }
            Err(error) => send(reply, Op::Reload, Response::Error(error)),
        },
        Request::Status => send(
            reply,
            Op::Status,
            Response::Status(Status {
                capture: service.capture.clone(),
                outputs: cx
                    .outputs()
                    .iter()
                    .map(|output| {
                        format!(
                            "{} {}x{} at ({}, {})",
                            output.label(),
                            output.width,
                            output.height,
                            output.x,
                            output.y
                        )
                    })
                    .collect(),
                pins: service.pins.count(),
                hotkeys: service.hotkey_lines.clone(),
            }),
        ),
        Request::CopyImage { path } => {
            if let Err(error) = copy_image(&path, service, reply) {
                send(reply, Op::CopyImage, Response::Error(error));
            }
            Effect::None
        }
        Request::PinList => send(reply, Op::PinList, Response::Pins(service.pins.list())),
        Request::PinAdd {
            source,
            as_kind,
            output,
        } => match add_pin(service, source, as_kind, output, cx) {
            Ok(()) => send(reply, Op::PinAdd, Response::Ok),
            Err(error) => send(reply, Op::PinAdd, Response::Error(error)),
        },
        Request::PinEdit { id } => answer_shown(reply, Op::PinEdit, service.pins.edit(id), cx),
        Request::PinClose { id } => answer_shown(reply, Op::PinClose, service.pins.close(id), cx),
        Request::PinVisible { id, shown } => {
            answer_shown(reply, Op::PinVisible, service.pins.set_shown(id, shown), cx)
        }
        Request::PinExport { id, path, format } => {
            let path = std::path::Path::new(&path);
            let codecs = &service.settings.codecs;
            let configured = service.settings.config.shots.format.as_deref();
            let result = service
                .pins
                .export(Some(id), path, codecs, configured, format.as_deref());
            answer_result(reply, Op::PinExport, result)
        }
    }
}

/// Hold a PNG on the clipboard: the selection outlives the command that asked for it.
///
/// The answer is owed once the copy is actually on the clipboard, so the reply is
/// deferred and sent from the turn that publishes it.
fn copy_image(path: &str, service: &mut Service, reply: &mut dyn Reply) -> Result<(), String> {
    let Some(deferred) = reply.deferred() else {
        return Err("this channel cannot defer the copy's answer".into());
    };
    let path = path.to_string();
    // The worker reads the file, so it carries the settings that say what reads pictures.
    let settings = Arc::clone(&service.settings);
    let job = service.jobs.submit(move || read_png(&path, &settings))?;
    service.copy.push(PendingCopy {
        job,
        reply: deferred,
    });
    Ok(())
}

/// Read `path` as PNG, converting any other format on the way.
fn read_png(path: &str, settings: &crate::Settings) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
    // The clipboard holds PNG and nothing else, whatever the file was written as.
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(bytes);
    }
    let decoded = settings
        .codecs
        .ffmpeg()
        .decode(&bytes)
        .map_err(|error| format!("decode {path}: {error}"))?;
    let full = ui_core::geom::Rect::new(0.0, 0.0, decoded.width as f32, decoded.height as f32);
    crate::capture::Image {
        width: decoded.width,
        height: decoded.height,
        data: decoded.data,
        colour: decoded.colour,
        scale: crate::capture::png_scale(&bytes),
        // A file does not say which display will show it, so the model's own white decides
        // whether its light counts as more than SDR.
        light: crate::capture::Light::of(ui_core::colour::SDR_WHITE_NITS),
    }
    .crop(full)
    .png()
    .map_err(|error| format!("encode {path}: {error}"))
}

/// Bind what the file asks for, releasing the previous set first.
///
/// The desktop reports each outcome once, at startup and on reload: a chord another
/// application owns, or a desktop with no global hotkeys at all, must not look like silence.
fn bind_hotkeys(service: &mut Service, cx: &mut MessageCtx<'_>) {
    for (action, _, _) in service.hotkeys.global_entries() {
        cx.unbind_hotkey(hotkeys::id(action));
    }
    let plan = Plan::resolve(&service.settings.config);
    service.hotkey_lines.clear();
    for problem in &plan.problems {
        eprintln!("shortcuts: {problem}");
    }
    let mut unsupported = false;
    for (action, chord, text) in plan.global_entries() {
        let mut line = format!("{} = {text}", action.name());
        match cx.bind_hotkey(chord, hotkeys::id(action)) {
            Ok(()) => {}
            Err(GlobalHotkeyError::Unsupported) if !unsupported => {
                unsupported = true;
                line.push_str(" (this desktop has no global hotkeys)");
            }
            Err(GlobalHotkeyError::Unsupported) => line.push_str(" (unavailable)"),
            Err(error) => line.push_str(&format!(" (failed: {error:?})")),
        }
        println!("hotkey {line}");
        service.hotkey_lines.push(line);
    }
    service.hotkeys = plan;
}

/// 重新读配置并重新解析抓取后端：配置或输出变化后，能力可能已经不是启动时那一套。
fn reload(service: &mut Service) -> Result<(), String> {
    // The language object is shared with every open canvas; the new settings hold the same
    // one, so the choice is changed in place instead of replacing the object under them.
    let settings = Settings::load(Arc::clone(&service.settings.language))?;
    let capture = probe(&service.grab, &settings)?;
    let choice = crate::language_choice(None, Some(&settings.config))?;
    service
        .settings
        .language
        .set_choice(choice)
        .map_err(|error| format!("language: {error:?}"))?;
    service.capture = capture;
    service.settings = settings;
    Ok(())
}

/// Ask for one frame of the first output; a rung that can be polled parks it for `on_turn`.
///
/// The grab used to run here, which held the loop for as long as the compositor took. The
/// requests are the same; only the waiting moved out of the turn.
fn start_capture(
    service: &mut Service,
    what: CaptureUse,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    if matches!(what, CaptureUse::Region) && service.overlay.is_some_and(|id| cx.is_open(id)) {
        return Err("a selection is already on screen".into());
    }
    if service.pending.is_some() {
        return Err("a capture is already running".into());
    }
    // A pick names the output it landed on; the others take the first one, as they always
    // have.
    let output = match &what {
        CaptureUse::Pick(request) => request
            .output
            .as_deref()
            .and_then(|name| cx.outputs().iter().position(|output| output.name == name))
            .unwrap_or(0),
        _ => 0,
    };
    let Some(to_grab) = capture::open(
        &service.grab,
        service.settings.config.capture.backend.as_deref(),
        output,
    )?
    else {
        return Err("no screen capture backend is available".into());
    };
    match to_grab.begin(Duration::from_secs(2))? {
        capture::Started::Pending(pending) => {
            service.pending = Some((pending, what));
            Ok(())
        }
        // The portal answers over D-Bus and has no way to be polled; it waits as it always did.
        capture::Started::Blocking(mut blocking) => {
            let frame = blocking.grab(Duration::from_secs(2))?;
            finish_capture(service, what, frame, cx)
        }
    }
}

/// The frame arrived: do what the capture was for.
fn finish_capture(
    service: &mut Service,
    what: CaptureUse,
    frame: capture::Frame,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    match what {
        CaptureUse::Region => open_overlay(service, frame, cx),
        CaptureUse::Output => save_frame(service, frame),
        CaptureUse::Pick(request) => pick_frame(service, &request, frame, cx),
    }
}

/// Read the frame's pixel under the press and hand it to the palette that waits.
fn pick_frame(
    service: &mut Service,
    request: &crate::pin::PickRequest,
    frame: capture::Frame,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    let image = frame.into_image();
    let full = ui_core::geom::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
    let picture = image.crop(full);
    let colour = pixel_under(&picture, request.at, request.screen);
    if let Some(surface) = service.pins.picked(request, colour) {
        cx.request_frame(surface)?;
    }
    Ok(())
}

/// The picture's pixel at the same share of both axes as the press is of the canvas.
fn pixel_under(
    picture: &capture::Cropped,
    at: ui_core::geom::Point,
    screen: (u32, u32),
) -> Option<[u8; 4]> {
    if screen.0 == 0 || screen.1 == 0 || picture.width == 0 || picture.height == 0 {
        return None;
    }
    let x = at.x / screen.0 as f32 * picture.width as f32;
    let y = at.y / screen.1 as f32 * picture.height as f32;
    if !(0.0..picture.width as f32).contains(&x) || !(0.0..picture.height as f32).contains(&y) {
        return None;
    }
    picture.texel(x as usize, y as usize)
}

/// `region`: freeze the screen and let the user select, on this loop's own surface.
fn open_overlay(
    service: &mut Service,
    frame: capture::Frame,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    let source = frame.output.clone();
    let image = frame.into_image();
    let (width, height) = (image.width, image.height);
    let pixels = Arc::clone(&image.data);
    let colour = image.colour;
    let overlay = Overlay::new(
        image,
        source,
        Arc::clone(&service.settings),
        Arc::clone(&service.settings.language),
        Rc::clone(&service.pin_outbox),
        service.jobs.clone(),
    );
    let id = cx.open(
        overlay_surface(),
        Box::new(overlay),
        OpenOptions {
            uncapped: false,
            fonts: vec![crate::fonts::icon_face()],
            images: vec![ImageEntry {
                id: FROZEN_TEXTURE,
                width,
                height,
                source: ui_core::draw::ImageSource::Pixels(pixels),
                colour,
                sampling: ui_core::draw::Sampling::Smooth,
            }],
            // Bound here rather than at the surface's first build: a key pressed before the
            // first frame must already mean something.
            shortcuts: service
                .settings
                .shortcuts
                .chords(crabsnap_core::hotkeys::Place::Overlay),
        },
    )?;
    service.overlay = Some(id);
    println!("hotkey region: overlay {width}x{height}");
    Ok(())
}

/// `output`: save the whole output where screenshots go.
///
/// Encoding a full output runs on the worker: the hotkey also answers for pins and the
/// overlay, and none of them should wait for a PNG.
fn save_frame(service: &Service, frame: capture::Frame) -> Result<(), String> {
    let source = frame.output.clone();
    let image = frame.into_image();
    let settings = Arc::clone(&service.settings);
    service.jobs.submit(move || {
        let full = ui_core::geom::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
        let cropped = image.crop(full);
        match crate::save_shot(&cropped, source.as_deref(), &settings) {
            Ok(written) => println!("hotkey output: {written}"),
            Err(error) => eprintln!("hotkey output: {error}"),
        }
    })?;
    Ok(())
}

fn probe(grab: &capture::Session, settings: &Settings) -> Result<String, String> {
    Ok(
        match capture::open(grab, settings.config.capture.backend.as_deref(), 0)? {
            Some(capture) => capture.name().to_string(),
            None => "none".to_string(),
        },
    )
}

/// `Ok` or the reason it failed, both answering the operation that asked.
fn answer_result(reply: &mut dyn Reply, op: Op, result: Result<(), String>) -> Effect {
    match result {
        Ok(()) => send(reply, op, Response::Ok),
        Err(error) => send(reply, op, Response::Error(error)),
    }
}

/// Answer a change that alters what a canvas draws, waking that canvas first.
fn answer_shown(
    reply: &mut dyn Reply,
    op: Op,
    result: Result<Option<SurfaceId>, String>,
    cx: &mut MessageCtx<'_>,
) -> Effect {
    match result {
        Ok(surface) => {
            if let Some(surface) = surface
                && let Err(error) = cx.request_frame(surface)
            {
                eprintln!("pin: cannot redraw its canvas: {error}");
            }
            send(reply, op, Response::Ok)
        }
        Err(error) => send(reply, op, Response::Error(error)),
    }
}

/// 把文件贴到 daemon 自己的连接上：像素留在磁盘，不过通道。
///
/// 一个文件失败只报它自己，其余照常上屏；返回的错误因此可能是多行。
fn add_pin(
    service: &mut Service,
    source: PinSource,
    as_kind: Option<Kind>,
    output: Option<usize>,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    let index = output.unwrap_or(0);
    let (screen, dots, name) = {
        let outputs = cx.outputs();
        let target = outputs.get(index).ok_or_else(|| {
            format!(
                "this display reports {} output(s); there is no output {}",
                outputs.len(),
                index + 1
            )
        })?;
        (
            (target.width, target.height),
            target.scale,
            target.name.clone(),
        )
    };
    let (opened, mut errors) = match source {
        PinSource::Paths(paths) => {
            let files: Vec<std::path::PathBuf> =
                paths.iter().map(std::path::PathBuf::from).collect();
            crate::pin::open_all(&files, dots, as_kind, service.settings.codecs.ffmpeg())
        }
        PinSource::Colour(code) => match crate::pin::open_colour(&code) {
            Ok(content) => (
                vec![crate::pin::Opened {
                    index: 0,
                    content,
                    origin: crabsnap_core::protocol::PinOrigin::Colour { code },
                }],
                Vec::new(),
            ),
            Err(error) => (Vec::new(), vec![error]),
        },
        PinSource::Clipboard => {
            // The selection is read off the loop, so the command is answered when the offer
            // is known to carry something usable, and the pins appear when the bytes land.
            return start_selection(service, as_kind, output, cx);
        }
    };
    let language = Arc::clone(&service.settings.language);
    for item in opened {
        let placement = crate::Placement {
            screen,
            dots,
            position: pins::cascade_at(screen, item.content.size(), item.index),
            output: Some(name.clone()),
        };
        let pin = Pin::new(
            item.content,
            placement,
            item.origin,
            Arc::clone(&language),
            Arc::clone(&service.settings),
        );
        if let Err(error) = pins::place(&mut service.pins, pin, cx) {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// `pin add --clipboard`: read the selection on the loop, and answer as soon as the offer
/// says something can come of it.
///
/// The MIME list is known synchronously, so a clipboard with nothing usable fails at once;
/// the bytes themselves arrive on a later turn and the pins appear in the same session.
fn start_selection(
    service: &mut Service,
    as_kind: Option<Kind>,
    output: Option<usize>,
    cx: &mut MessageCtx<'_>,
) -> Result<(), String> {
    let offers = cx.selection_offers();
    let Some(mime) = pick_mime(&offers) else {
        let shown: Vec<&str> = offers.iter().map(String::as_str).collect();
        return Err(format!(
            "the clipboard offers nothing a pin can show ({})",
            shown.join(", ")
        ));
    };
    if !cx.read_selection(mime) {
        return Err("this compositor offers no clipboard to read".into());
    }
    service.selection = Some(PendingSelection { as_kind, output });
    Ok(())
}

/// The MIME to read, most usable first: a file list pins per file, an image pins once, and
/// text is the last thing a pin shows.
fn pick_mime(offers: &[String]) -> Option<&str> {
    const WANTED: [&str; 4] = [
        "text/uri-list",
        "x-special/gnome-copied-files",
        "image/png",
        "image/jpeg",
    ];
    for wanted in WANTED {
        if offers.iter().any(|mime| mime == wanted) {
            return Some(wanted);
        }
    }
    // Any image the classifier can name is a pin; a text selection is the fallback.
    offers
        .iter()
        .find(|mime| {
            mime.starts_with("image/")
                || mime.starts_with("text/plain")
                || mime.as_str() == "UTF8_STRING"
                || mime.as_str() == "STRING"
                || mime.as_str() == "TEXT"
        })
        .map(String::as_str)
}

/// The selection's bytes landed: turn them into pins.
///
/// The command was answered when the read started; this is the loop's own work, and a
/// failure here is a message to the terminal, not an answer to a peer that has gone.
fn finish_selection(
    service: &mut Service,
    selection: ui_platform::transfer::Selection,
    cx: &mut MessageCtx<'_>,
) {
    let Some(pending) = service.selection.take() else {
        return;
    };
    // An owner that closed its pipe produced no selection at all: an empty pin would claim
    // the clipboard held nothing, when the read is what failed.
    if selection.data.is_empty() {
        eprintln!(
            "pin: the clipboard's {} selection was empty",
            selection.mime
        );
        return;
    }
    let (screen, dots, name) = {
        let outputs = cx.outputs();
        let index = pending.output.unwrap_or(0);
        let target = match outputs.get(index) {
            Some(target) => target,
            None => {
                eprintln!(
                    "pin: this display reports {} output(s); there is no output {}",
                    outputs.len(),
                    index + 1
                );
                return;
            }
        };
        (
            (target.width, target.height),
            target.scale,
            target.name.clone(),
        )
    };
    let transfer =
        crabsnap_core::transfer::Transfer::from_selection(&selection.mime, selection.data);
    let (opened, errors) = match transfer {
        crabsnap_core::transfer::Transfer::Files(paths) => crate::pin::open_all(
            &paths,
            dots,
            pending.as_kind,
            service.settings.codecs.ffmpeg(),
        ),
        crabsnap_core::transfer::Transfer::Bytes { mime, data } => {
            match crate::pin::open_blob(
                &data,
                &mime,
                dots,
                pending.as_kind,
                service.settings.codecs.ffmpeg(),
            ) {
                Ok(content) => (
                    vec![crate::pin::Opened {
                        index: 0,
                        content,
                        origin: crabsnap_core::protocol::PinOrigin::Selection { mime },
                    }],
                    Vec::new(),
                ),
                Err(error) => (Vec::new(), vec![error]),
            }
        }
    };
    let language = Arc::clone(&service.settings.language);
    for item in opened {
        let placement = crate::Placement {
            screen,
            dots,
            position: pins::cascade_at(screen, item.content.size(), item.index),
            output: Some(name.clone()),
        };
        let pin = Pin::new(
            item.content,
            placement,
            item.origin,
            Arc::clone(&language),
            Arc::clone(&service.settings),
        );
        if let Err(error) = pins::place(&mut service.pins, pin, cx) {
            eprintln!("pin: {error}");
        }
    }
    for error in errors {
        eprintln!("pin: {error}");
    }
}

fn send(reply: &mut dyn Reply, op: Op, response: Response) -> Effect {
    match protocol::encode_response(op, &response) {
        Ok(bytes) => {
            // 对端已经断开就不必再答；这不是守护进程的错误。
            let _ = reply.send(&bytes);
        }
        Err(error) => eprintln!("daemon: cannot encode a response: {error}"),
    }
    Effect::None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The press and the frame share both axes: the same share of each lands on the same
    /// pixel, a press outside the canvas finds none, and a canvas with no size asks nothing.
    #[test]
    fn a_press_maps_to_the_captured_pixel() {
        let mut data = Vec::with_capacity(4 * 4 * 4);
        for y in 0..4u8 {
            for x in 0..4u8 {
                data.extend_from_slice(&[x * 60, y * 60, 0, 255]);
            }
        }
        let picture = capture::Cropped {
            origin: (0, 0),
            width: 4,
            height: 4,
            data: std::sync::Arc::new(data),
            colour: ui_core::colour::ContentColour::SDR,
            scale: 1.0,
            light: crate::capture::Light::sdr(),
        };
        let screen = (8, 8);
        assert_eq!(
            pixel_under(&picture, ui_core::geom::Point::new(1.0, 1.0), screen),
            Some([0, 0, 0, 255])
        );
        assert_eq!(
            pixel_under(&picture, ui_core::geom::Point::new(5.0, 7.0), screen),
            Some([120, 180, 0, 255]),
            "half across and three quarters down is the pixel there"
        );
        assert_eq!(
            pixel_under(&picture, ui_core::geom::Point::new(8.0, 1.0), screen),
            None,
            "the right edge is past the last pixel"
        );
        assert_eq!(
            pixel_under(&picture, ui_core::geom::Point::new(-1.0, 1.0), screen),
            None
        );
        assert_eq!(
            pixel_under(&picture, ui_core::geom::Point::new(1.0, 1.0), (0, 8)),
            None,
            "a canvas with no width has no share to take"
        );
    }
}
