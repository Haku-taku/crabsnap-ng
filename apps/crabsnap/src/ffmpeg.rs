//! 扩展格式的编码：先用本进程里的 ffmpeg 库，库不在才用 ffmpeg 可执行文件。
//!
//! 写 PNG 不在这里——它随程序自带（见 `codec`）。读图与写其余格式一律交给 ffmpeg：容器、
//! 编码器、像素格式和参数表都从 libavformat/libavcodec 自己问出来，而不是在这里抄一份
//! ffmpeg 的知识。唯一静态的是「我们愿意提供哪些后缀」：这是产品选择（只出静图），不是能力。
//!
//! 两条路的分工：库这条路知道编码器的 id、像素格式和 AVOptions；可执行文件只知道它自己的
//! 命令行，所以用它自己的列表命令回答（`-encoders`、`-h encoder=NAME`）。
//!
//! 为什么经 C 而不是 Rust 绑定：见 `crabsnap-platform/ffmpeg/ffi/ffmpeg_shim.h`。

mod decode;
mod executable;

use std::path::Path;

use crabsnap_platform::shim;

use crate::capture::Cropped;

/// 我们提供的后缀，不含 png（它自带）。顺序就是 `info formats` 里的顺序。
const OFFERED: &[&str] = &["jpg", "webp", "avif", "jxl", "tiff", "bmp", "gif", "qoi"];

/// 提供的后缀，规范化以后；`png` 不在其中。
pub fn offered() -> &'static [&'static str] {
    OFFERED
}

/// 把一个后缀（含别名）折成我们提供的写法；不认识就是 `None`。
pub fn canonical(extension: &str) -> Option<&'static str> {
    let extension = match extension {
        "jpeg" => "jpg",
        "tif" => "tiff",
        other => other,
    };
    OFFERED
        .iter()
        .copied()
        .find(|offered| *offered == extension)
}

/// 由 ffmpeg 给出的一个容器及其编码器。
#[derive(Clone, Debug)]
pub struct Format {
    /// 文件名后缀，也是 `av_guess_format` 认的那个词。
    pub extension: &'static str,
    pub encoder: String,
    /// 怎么封装：muxer 的名字、MIME 类型和它默认的编码；可执行文件那条路不知道这些，
    /// 它按文件名自己推断。
    pub muxer: Option<String>,
    /// 编码器的长名字，给人看。
    pub description: String,
}

/// 一个编码器参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatOption {
    pub name: String,
    pub help: String,
    pub default: String,
}

/// 本机写扩展格式的能力。
pub struct Ffmpeg {
    source: Source,
    formats: Vec<Format>,
    /// 库这条路不通时，可执行文件那条路拒绝的原因。
    program_failure: Option<String>,
}

enum Source {
    /// 本进程里 dlopen 的 libavformat/libavcodec/libavutil，带着它们的版本号。
    Libraries(String),
    /// `ffmpeg` 可执行文件，库缺失或与头文件版本不符时的退路。
    Program(executable::Program),
    None,
}

impl Ffmpeg {
    pub fn probe(program: Option<&Path>, library_directory: Option<&Path>) -> Self {
        // Probing is what `reload` re-runs, so it starts from nothing loaded: a directory
        // fixed, or a library installed, since the last probe is visible to this one.
        shim::reopen(library_directory);
        if let Ok(versions) = shim::open() {
            let muxers = shim::muxers();
            let encoders = shim::encoders();
            let formats = offered_in_libraries(&muxers, &encoders);
            return Self {
                source: Source::Libraries(versions),
                formats,
                program_failure: None,
            };
        }
        let Some(path) = program.map(Path::to_path_buf).or_else(executable::find) else {
            return Self {
                source: Source::None,
                formats: Vec::new(),
                program_failure: None,
            };
        };
        match executable::Program::open(path) {
            Ok(program) => {
                let formats = program.formats();
                Self {
                    source: Source::Program(program),
                    formats,
                    program_failure: None,
                }
            }
            Err(reason) => Self {
                source: Source::None,
                formats: Vec::new(),
                program_failure: Some(reason),
            },
        }
    }

    /// Where the formats come from, for `info formats`; `None` when nothing does.
    pub fn source(&self) -> Option<String> {
        match &self.source {
            Source::Libraries(versions) => Some(format!("ffmpeg libraries {versions}")),
            Source::Program(program) => {
                Some(format!("ffmpeg program {}", program.path().display()))
            }
            Source::None => None,
        }
    }

    /// Why the libraries were not used, and why the program was not either, when neither was.
    pub fn failure(&self) -> Option<String> {
        if matches!(self.source, Source::Libraries(_)) {
            return None;
        }
        let mut reasons = Vec::new();
        if let Err(reason) = shim::open() {
            reasons.push(format!("libraries: {reason}"));
        }
        if let Some(reason) = &self.program_failure {
            reasons.push(format!("program: {reason}"));
        }
        if reasons.is_empty() {
            reasons.push("no ffmpeg libraries and no ffmpeg program on PATH".into());
        }
        Some(reasons.join("; "))
    }

    pub fn formats(&self) -> &[Format] {
        &self.formats
    }

    pub fn format(&self, extension: &str) -> Option<&Format> {
        self.formats
            .iter()
            .find(|format| format.extension == extension)
    }

    /// The encoder's own parameters; empty when this source cannot list them.
    pub fn options(&self, format: &Format) -> Vec<FormatOption> {
        match &self.source {
            Source::Libraries(_) => shim::options(&format.encoder)
                .unwrap_or_default()
                .iter()
                .map(|option| FormatOption {
                    name: option.name().to_string(),
                    help: option.help().to_string(),
                    default: option.default_text().to_string(),
                })
                .collect(),
            Source::Program(program) => program.options(&format.encoder),
            Source::None => Vec::new(),
        }
    }

    /// Is this encoder able to carry more than 8 bits per channel?
    pub fn deep_colour(&self, format: &Format) -> Option<bool> {
        match &self.source {
            Source::Libraries(_) => shim::pix_fmts(&format.encoder)
                .map(|formats| formats.iter().any(|format| deep_pixel_format(*format))),
            Source::Program(program) => program.deep_colour(&format.encoder),
            Source::None => None,
        }
    }

    /// Whether one configured parameter can be honoured here.
    ///
    /// `None` means this source cannot answer: the program prints no table for the context
    /// options, so a name it does not know is caught when ffmpeg refuses it, not here.
    pub fn accepts_option(&self, format: &Format, name: &str) -> Option<bool> {
        match &self.source {
            Source::Libraries(_) => shim::has_option(&format.encoder, name),
            Source::Program(_) | Source::None => None,
        }
    }

    /// Check one format's configured parameters before anything is encoded.
    pub fn check_options(
        &self,
        format: &Format,
        configured: &[(String, String)],
    ) -> Result<(), String> {
        settle(format.extension, configured)?;
        for (name, _) in configured {
            if name == "global_quality" {
                continue;
            }
            if self.accepts_option(format, name) == Some(false) {
                return Err(format!(
                    "the {} encoder has no option {name:?}",
                    format.encoder
                ));
            }
        }
        Ok(())
    }

    pub fn encode(
        &self,
        format: &Format,
        image: &Cropped,
        configured: &[(String, String)],
    ) -> Result<Vec<u8>, String> {
        let settings = settle(format.extension, configured)?;
        match &self.source {
            Source::Libraries(_) => encode_in_process(format, image, &settings),
            Source::Program(program) => program.encode(format, image, &settings),
            Source::None => Err("this machine has no ffmpeg to write extended formats".into()),
        }
    }
}

/// 每通道多于 8 位（或浮点）的像素格式，按 ffmpeg 命名规则认。
fn deep_pixel_format(format: i32) -> bool {
    shim::pix_fmt_name(format).is_some_and(|name| deep_name(&name))
}

fn deep_name(name: &str) -> bool {
    let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| name.ends_with(suffix));
    if ends(&[
        "10le", "10be", "12le", "12be", "16le", "16be", "48le", "48be", "64le", "64be", "f32le",
        "f32be", "f64le", "f64be", "f16le", "f16be",
    ]) {
        return true;
    }
    // p010le, p210le, p410le: the number is the bits, not the chroma ratio.
    let bytes = name.as_bytes();
    bytes.len() > 2 && bytes[0] == b'p' && bytes[1].is_ascii_digit() && name.ends_with("le")
}

/// What each format asks of its encoder.
///
/// The same recipe feeds both sources: the libraries set these as AVOptions, the program as
/// command-line flags, so a file comes out the same either way.
#[derive(Clone, Copy, Default)]
struct Recipe {
    /// The pixel layout to hand the encoder; `None` lets it pick its own.
    pixel_format: Option<&'static str>,
    options: &'static [(&'static str, &'static str)],
    /// Encoders that only read the old qscale want this instead of a quality option.
    global_quality: Option<i32>,
}

/// `FF_QP2LAMBDA`: the unit `global_quality` counts in.
const QSCALE_UNIT: i32 = 118;

fn recipe(extension: &str) -> Recipe {
    match extension {
        // 2 is the qscale the libjpeg path used to ask for: text stays readable.
        "jpg" => Recipe {
            pixel_format: Some("yuvj420p"),
            options: &[],
            global_quality: Some(2 * QSCALE_UNIT),
        },
        // Screenshots stay sharp: lossless, and BGRA so libwebp keeps exact colours instead
        // of subsampling chroma.
        "webp" => Recipe {
            pixel_format: Some("bgra"),
            options: &[("lossless", "1")],
            global_quality: None,
        },
        _ => Recipe {
            pixel_format: None,
            options: &[],
            global_quality: None,
        },
    }
}

/// The parameters one file is written with: the format's recipe, with the configured ones
/// layered on top by name.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub pixel_format: Option<&'static str>,
    pub options: Vec<(String, String)>,
    /// The old qscale, for encoders that read it instead of a quality option.
    pub global_quality: Option<i32>,
}

fn settle(extension: &str, configured: &[(String, String)]) -> Result<Settings, String> {
    let recipe = recipe(extension);
    let mut settings = Settings {
        pixel_format: recipe.pixel_format,
        options: recipe
            .options
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        global_quality: recipe.global_quality,
    };
    for (name, value) in configured {
        if name == "global_quality" {
            settings.global_quality = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| format!("global_quality takes a whole number, not {value:?}"))?,
            );
            continue;
        }
        settings.options.retain(|(existing, _)| existing != name);
        settings.options.push((name.clone(), value.clone()));
    }
    Ok(settings)
}

fn encode_in_process(
    format: &Format,
    image: &Cropped,
    settings: &Settings,
) -> Result<Vec<u8>, String> {
    let rgba = image.rendition();
    let options: Vec<(&str, &str)> = settings
        .options
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    shim::encode(
        &format.encoder,
        format.extension,
        &shim::Picture {
            width: image.width as i32,
            height: image.height as i32,
            rgba: &rgba,
            pixel_format: settings.pixel_format,
            colour: shim::srgb(),
            global_quality: settings.global_quality,
            options: &options,
        },
    )
}

/// The offered extensions, each with the encoder libavcodec would use for it.
fn offered_in_libraries(muxers: &[shim::Muxer], encoders: &[shim::Encoder]) -> Vec<Format> {
    let mut formats = Vec::new();
    for extension in OFFERED {
        let codec = shim::guess_codec(extension);
        if codec == 0 {
            continue;
        }
        let Some(encoder) = choose_encoder(codec, encoders) else {
            continue;
        };
        formats.push(Format {
            extension,
            encoder: encoder.name().to_string(),
            muxer: muxer_for(muxers, extension),
            description: encoder.long_name().to_string(),
        });
    }
    formats
}

/// The first muxer whose extension list names this extension, described for `info formats`.
fn muxer_for(muxers: &[shim::Muxer], extension: &str) -> Option<String> {
    // av_guess_format matches the muxer's name as well as its extensions, and some muxers
    // (qoi) list none.
    let muxer = muxers.iter().find(|muxer| {
        muxer.name().eq_ignore_ascii_case(extension)
            || muxer
                .extensions()
                .split(',')
                .any(|candidate| candidate.trim().eq_ignore_ascii_case(extension))
    })?;
    let mut detail = format!("{} ({})", muxer.name(), muxer.long_name());
    if !muxer.mime().is_empty() {
        detail.push_str(&format!(", {}", muxer.mime()));
    }
    if let Some(codec) = shim::codec_name(shim::guess_codec(extension)) {
        detail.push_str(&format!(", codec {codec}"));
    }
    Some(detail)
}

/// The encoder to ask for: software ones, the still variant before the animation one, and
/// the `lib*` (external library) builds before ffmpeg's own.
fn choose_encoder(codec: i32, encoders: &[shim::Encoder]) -> Option<shim::Encoder> {
    let mut candidates: Vec<&shim::Encoder> = encoders
        .iter()
        .filter(|encoder| encoder.id() == codec && is_software(encoder.name()))
        .collect();
    candidates.sort_by(|left, right| {
        let left = left.name();
        let right = right.name();
        (!left.starts_with("lib"), left).cmp(&(!right.starts_with("lib"), right))
    });
    candidates.into_iter().next().cloned()
}

/// Encoders that need a GPU or a device context, and the animation-only variants.
fn is_software(name: &str) -> bool {
    const ACCELERATED: &[&str] = &[
        "vaapi",
        "nvenc",
        "qsv",
        "vulkan",
        "amf",
        "videotoolbox",
        "mediacodec",
        "cuvid",
        "opencl",
        "d3d12",
        "rkmpp",
        "v4l2m2m",
        "mf_",
    ];
    const NOT_A_STILL: &[&str] = &["_anim"];
    let lower = name.to_ascii_lowercase();
    !ACCELERATED.iter().any(|mark| lower.contains(mark))
        && !NOT_A_STILL.iter().any(|mark| lower.ends_with(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animation_variant_and_hardware_encoders_are_passed_over() {
        let encoder =
            |name: &str, codec: i32| shim::Encoder::new(name.into(), String::new(), codec);
        let choose = |names: &[&str], codec: i32| {
            let encoders: Vec<shim::Encoder> = names
                .iter()
                .map(|name| encoder(name, codec))
                .collect();
            choose_encoder(codec, &encoders).map(|encoder| encoder.name().to_string())
        };
        assert_eq!(
            choose(&["libwebp_anim", "webp"], 61),
            Some("webp".into()),
            "the animation encoder is for animations"
        );
        assert_eq!(
            choose(&["webp_vaapi", "webp"], 61),
            Some("webp".into()),
            "a GPU encoder is not opened for a still"
        );
        assert_eq!(
            choose(&["webp", "libwebp"], 61),
            Some("libwebp".into()),
            "the library build wins over ffmpeg's own"
        );
        let mixed = [encoder("libwebp", 61), encoder("mjpeg", 7)];
        assert_eq!(
            choose_encoder(7, &mixed).map(|encoder| encoder.name().to_string()),
            Some("mjpeg".into()),
            "an encoder of another codec is not a candidate"
        );
    }

    /// 配置里的同名参数顶掉配方默认值，其余配方项保留；`global_quality` 走自己的字段。
    #[test]
    fn configured_parameters_win_over_the_recipe_by_name() {
        let configured = [
            ("lossless".to_string(), "0".to_string()),
            ("quality".to_string(), "90".to_string()),
        ];
        let settings = settle("webp", &configured).expect("both are plain parameters");
        assert_eq!(
            settings.pixel_format,
            Some("bgra"),
            "the layout is the recipe's"
        );
        assert!(
            settings
                .options
                .contains(&("lossless".to_string(), "0".to_string())),
            "the configured value replaces the recipe's"
        );
        assert!(
            !settings
                .options
                .contains(&("lossless".to_string(), "1".to_string())),
            "the recipe's lossless is gone, not appended"
        );
        assert!(
            settings
                .options
                .contains(&("quality".to_string(), "90".to_string()))
        );

        let settings =
            settle("jpg", &[("global_quality".to_string(), "354".to_string())]).expect("a number");
        assert_eq!(settings.global_quality, Some(354), "the configured qscale");
        assert!(
            settle("jpg", &[("global_quality".to_string(), "high".to_string())]).is_err(),
            "a qscale that is not a number is refused"
        );
    }

    #[test]
    fn pixel_format_names_tell_8_bit_from_deeper() {
        for name in [
            "yuv420p10le",
            "gbrpf32le",
            "gray16be",
            "rgb48be",
            "p010le",
            "p210le",
        ] {
            assert!(deep_name(name), "{name} is deeper than 8 bits");
        }
        for name in ["yuv420p", "yuv410p", "bgra", "rgb24", "pal8", "gray"] {
            assert!(!deep_name(name), "{name} is 8 bits");
        }
    }
}
