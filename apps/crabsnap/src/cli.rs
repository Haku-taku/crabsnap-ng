//! 命令面：参数结构由 clap 声明，面向用户的说明文字由 Fluent 按 id 注入。
//!
//! 为什么文案不写在 derive 的文档注释里：`///` 会被 clap 当成 about/help，而本项目的
//! 用户可见文案只能来自 `locales/*.ftl`。结构与文案分离，加语言不必碰这个文件。
//!
//! ```text
//! crabsnap shot [output|region] [-f PATH] [--pin] [--delay SECS]
//! crabsnap pin  [add|list|export|close|show|hide|toggle]
//! crabsnap info [config|formats|status]
//! crabsnap daemon [--idle SECS] | settings | autostart [on|off] | quit | reload
//! crabsnap completions --shell <SHELL>
//! ```

use std::path::PathBuf;

use clap::{Args, Command as ClapCommand, CommandFactory, FromArgMatches, Parser, Subcommand};
use ui_i18n::Catalog;

/// 顶层参数：全局语言覆盖、`--no-daemon`，以及可选的子命令。
#[derive(Parser, Debug)]
#[command(name = "crabsnap", version, disable_help_subcommand = true)]
pub struct Cli {
    #[arg(long, global = true, value_name = "LANG")]
    pub lang: Option<String>,

    // 完全不使用 daemon：不探测 socket，也不把自己转成 daemon。
    #[arg(long, short = 'D', global = true)]
    pub no_daemon: bool,

    #[command(subcommand)]
    pub command: Option<Sub>,
}

/// 一次解析的结果：全局选项与命令。
#[derive(Debug)]
pub struct Invocation {
    pub no_daemon: bool,
    pub command: Sub,
}

#[derive(Subcommand, Debug)]
pub enum Sub {
    Shot(ShotArgs),
    Pin(PinArgs),
    Info(InfoArgs),
    Daemon(DaemonArgs),
    Settings(SettingsArgs),
    Autostart(AutostartArgs),
    Quit,
    Reload,
    Completions(CompletionsArgs),
}

/// 登录时是否启动 daemon：不给状态就只报当前状态。
#[derive(Args, Debug)]
pub struct AutostartArgs {
    // on 或 off；省略时只读不改。
    #[arg(value_name = "STATE")]
    pub state: Option<String>,
}

/// 设置窗口，以及不开窗时的脚本路径。
#[derive(Args, Debug, Default)]
pub struct SettingsArgs {
    // 写进配置的抓取后端；auto 等于删掉这条覆盖，不给则保留文件里的值。
    #[arg(long, value_name = "RUNG")]
    pub backend: Option<String>,

    // 写进配置的保存格式；auto 同上。
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,

    // 写进配置的截图目录；空字符串等于清掉这条覆盖。
    #[arg(long, value_name = "PATH")]
    pub directory: Option<String>,

    // 写进配置的 ffmpeg 程序路径。
    #[arg(long, value_name = "PATH")]
    pub ffmpeg: Option<String>,

    // 写进配置的界面语言：auto 或语言标签。顶层 `--lang` 只影响本次运行。
    #[arg(long, value_name = "TAG")]
    pub ui_language: Option<String>,

    // 写进配置的快捷键：动作名=和弦，可重复；和弦为空等于清掉这个动作。
    #[arg(long, value_name = "NAME=CHORD")]
    pub shortcut: Vec<String>,

    // 不打开窗口：把上面的值写进文件、通知 daemon，然后退出。
    #[arg(long)]
    pub save: bool,
}

/// 截图：抓哪一块由子命令决定，送到哪由通用选项决定。省略子命令等于 `output`。
#[derive(Args, Debug, Default)]
pub struct ShotArgs {
    #[command(subcommand)]
    pub command: Option<ShotSub>,

    #[command(flatten)]
    pub common: ShotCommon,
}

/// 所有抓取方式共用的选项。
#[derive(Args, Debug, Default, Clone)]
pub struct ShotCommon {
    // 写进这个文件；不给时落到截图目录里的时间戳文件名。
    #[arg(long, short = 'f', global = true, value_name = "PATH")]
    pub file: Option<PathBuf>,

    // 把抓到的图贴到屏幕上，而不是只写文件。
    #[arg(long, global = true)]
    pub pin: bool,

    // 抓之前等待的秒数，给用户时间摆好要拍的界面。
    #[arg(long, global = true, value_name = "SECS")]
    pub delay: Option<u64>,

    // 写出的编码：png、jpeg 或 webp；省略时看文件名，再看配置。
    #[arg(long, global = true, value_name = "FORMAT")]
    pub format: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ShotSub {
    Output(OutputArgs),
    Region(RegionArgs),
}

/// 抓一个输出，不需要覆盖层。
#[derive(Args, Debug, Default)]
pub struct OutputArgs {
    // 第几个输出；省略时抓第一个。
    #[arg(long, short = 'o', value_name = "N")]
    pub output: Option<u32>,

    // 把截图放进剪贴板；由 daemon 持有，命令退出后仍可粘贴。
    #[arg(long)]
    pub clipboard: bool,

    // 固定抓取后端，供回退链的调试：ext、screencopy 或 portal。
    #[arg(long, value_name = "RUNG")]
    pub backend: Option<String>,
}

/// 冻结屏幕后交互式拖一个选区。
#[derive(Args, Debug, Default)]
pub struct RegionArgs {
    // 把一个固定选区画出来，不需要真实拖拽。
    #[arg(long, global = true)]
    pub demo: bool,
}

#[derive(Args, Debug)]
pub struct PinArgs {
    #[command(subcommand)]
    pub command: PinSub,
}

#[derive(Subcommand, Debug)]
pub enum PinSub {
    Add(PinAddArgs),
    List,
    Export(PinExportArgs),
    /// Open the annotation editor over one pin.
    Edit(PinEditArgs),
    Close(PinCloseArgs),
    Show(PinTargetArgs),
    Hide(PinTargetArgs),
    Toggle(PinTargetArgs),
}

/// 指定哪一张贴图。指针属于客户端，daemon 看不见它，所以省略 `--id` 只能报错。
#[derive(Args, Debug, Default)]
pub struct PinTargetArgs {
    #[arg(long, value_name = "N")]
    pub id: Option<u64>,
}

/// 把文件贴到屏幕上，每个文件一张贴图。
#[derive(Args, Debug)]
pub struct PinAddArgs {
    // 一次给多个：每个文件一张贴图，坏的那个只报它自己。
    #[arg(long, short = 'f', value_name = "PATH", num_args = 1..)]
    pub file: Vec<PathBuf>,

    // 一个 CSS 颜色代码；与 `--file`、`--clipboard` 三选一。
    #[arg(long = "color", value_name = "CODE")]
    pub colour: Option<String>,

    // 读剪贴板：文件列表每个文件一张贴图，一张图片贴一次。
    #[arg(long)]
    pub clipboard: bool,

    // 从一张图片的配色生成色卡；等于 `--file PATH --as palette`。
    #[arg(long = "palette-from", value_name = "PATH")]
    pub palette_from: Option<PathBuf>,

    // 覆盖按内容做出的判断，给扩展名说谎的文件用。
    #[arg(long = "as", id = "as", value_name = "KIND")]
    pub as_kind: Option<String>,

    // 贴到第几个输出；省略时贴到第一个。
    #[arg(long, short = 'o', value_name = "N")]
    pub output: Option<usize>,
}

/// 把屏上的贴图编码进文件；像素在 daemon 里，所以按 id 指定。
#[derive(Args, Debug)]
pub struct PinExportArgs {
    #[arg(long, value_name = "N")]
    pub id: u64,

    #[arg(long, short = 'f', value_name = "PATH")]
    pub file: PathBuf,

    // 写出的编码；省略时看文件名，再看配置。
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,
}

/// 打开哪一张贴图的标注编辑器：daemon 看不见指针，所以要点名。
#[derive(Args, Debug)]
pub struct PinEditArgs {
    #[arg(long, value_name = "N")]
    pub id: u64,
}

/// 关掉哪一张贴图。
#[derive(Args, Debug)]
pub struct PinCloseArgs {
    // 关掉哪一个；省略时关掉指针下的那个。
    #[arg(long, value_name = "N")]
    pub id: Option<u64>,
}

#[derive(Args, Debug)]
pub struct InfoArgs {
    #[command(subcommand)]
    pub command: InfoSub,
}

#[derive(Subcommand, Debug)]
pub enum InfoSub {
    Config,
    Formats(FormatsArgs),
    Status,
}

#[derive(Args, Debug, Default)]
pub struct FormatsArgs {
    // 连每种格式的编码器参数一起列出来。
    #[arg(long)]
    pub verbose: bool,
}

/// 常驻服务。
#[derive(Args, Debug, Default)]
pub struct DaemonArgs {
    // 没有请求多久后退出；省略时永不退出。
    #[arg(long, value_name = "SECS")]
    pub idle: Option<u64>,
}

#[derive(Args, Debug)]
pub struct CompletionsArgs {
    #[arg(long, value_name = "SHELL")]
    pub shell: clap_complete::Shell,
}

/// 解析命令行。`None` 表示 clap 已经打印了 help/version，没有别的要做。
///
/// 语言必须在解析前定好：clap 在解析阶段就会渲染 `--help`，所以先做一次极轻量的
/// `--lang` 预扫描，再用它选文案。
pub fn parse(args: &[String], catalog: &Catalog) -> Result<Option<Invocation>, String> {
    let argv = std::iter::once("crabsnap".to_string()).chain(args.iter().cloned());
    let matches = match command(catalog).try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            error.print().ok();
            return Ok(None);
        }
        Err(error) => {
            error.print().ok();
            return Err("invalid command line".into());
        }
    };
    let cli = Cli::from_arg_matches(&matches).map_err(|error| error.to_string())?;
    let Some(command) = cli.command else {
        command(catalog).print_help().map_err(|e| e.to_string())?;
        println!();
        return Ok(None);
    };
    Ok(Some(Invocation {
        no_daemon: cli.no_daemon,
        command,
    }))
}

/// `--lang` 的预扫描：在 clap 解析之前定下语言，help 才会用对的语言渲染。
pub fn prescan_lang(args: &[String]) -> Option<String> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--lang=") {
            return Some(value.to_string());
        }
        if arg == "--lang" {
            return args.next().cloned();
        }
    }
    None
}

/// 完整的 clap 命令树，说明文字已经按 `catalog` 注入。
pub fn command(catalog: &Catalog) -> ClapCommand {
    let mut cmd = localize(Cli::command(), catalog, "cli");
    cmd.build();
    cmd
}

/// 按 id 给一棵命令树填文案：`cli-shot-output-file` 这样的路径。
///
/// 缺的键保持原样（通常是空），不塞占位符：全局选项在顶层定义一次，子命令里不重复。
fn localize(cmd: ClapCommand, catalog: &Catalog, key: &str) -> ClapCommand {
    let mut cmd = cmd;
    if let Some(text) = text(catalog, key) {
        cmd = cmd.about(text);
    }
    let args: Vec<String> = cmd
        .get_arguments()
        .map(|arg| arg.get_id().to_string())
        .collect();
    for id in args {
        let own = format!("{key}-{id}");
        if let Some(text) = text(catalog, &own) {
            cmd = cmd.mut_arg(id.clone(), move |arg| arg.help(text.clone()));
        }
    }
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    for name in names {
        let child = format!("{key}-{name}");
        cmd = cmd.mut_subcommand(name, move |sub| localize(sub, catalog, &child));
    }
    cmd
}

fn text(catalog: &Catalog, id: &str) -> Option<String> {
    catalog
        .has_id(id)
        .then(|| catalog.text_by_id(id).to_string())
}
