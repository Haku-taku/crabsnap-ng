//! 用 ffmpeg 可执行文件写扩展格式：库缺失或与头文件版本不符时的退路。
//!
//! 这是本项目唯一允许的子进程，而且只在扩展格式上出现；能用库的时候绝不起进程。它自己的
//! 列表命令（`-encoders`、`-h encoder=NAME`）就是它那一边的能力表——库那边问的是
//! libavcodec，这里问的是这个可执行文件自己。
//!
//! 输出先落到临时文件再读回：像 avif 这样需要回填头部的 muxer 不接受不可寻址的管道，
//! 而临时文件名带着后缀，正好让 ffmpeg 自己按后缀选 muxer。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Format, FormatOption, OFFERED, QSCALE_UNIT, Settings, deep_name, is_software};
use crate::capture::Cropped;

/// 哪些容器由哪个编码格式写出。库那边由 `av_guess_codec` 回答，可执行文件没有这个问法。
const CODECS: &[(&str, &str)] = &[
    ("jpg", "mjpeg"),
    ("webp", "webp"),
    ("avif", "av1"),
    ("jxl", "jpegxl"),
    ("tiff", "tiff"),
    ("bmp", "bmp"),
    ("gif", "gif"),
    ("qoi", "qoi"),
];

pub struct Program {
    path: PathBuf,
    formats: Vec<Format>,
}

struct ListedEncoder {
    name: String,
    long_name: String,
    codec: String,
}

impl Program {
    /// Ask the program what it can encode; an error when it will not even list its encoders.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let listing = run(&path, &["-hide_banner", "-encoders"])?;
        let formats = offered_in_program(&parse_encoders(&listing));
        Ok(Self { path, formats })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn formats(&self) -> Vec<Format> {
        self.formats.clone()
    }

    pub fn options(&self, encoder: &str) -> Vec<FormatOption> {
        match self.help(encoder) {
            Ok(help) => parse_options(&help),
            // `info formats` says the same thing for a missing list: nothing known.
            Err(_) => Vec::new(),
        }
    }

    pub fn deep_colour(&self, encoder: &str) -> Option<bool> {
        let help = self.help(encoder).ok()?;
        let formats = supported_pixel_formats(&help)?;
        Some(formats.iter().any(|name| deep_name(name)))
    }

    pub fn encode(
        &self,
        format: &Format,
        image: &Cropped,
        settings: &Settings,
    ) -> Result<Vec<u8>, String> {
        let target = Temporary::reserve(format.extension)?;
        let rgba = image.rendition();
        let size = format!("{}x{}", image.width, image.height);
        let mut command = Command::new(&self.path);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size,
            "-i",
            "-",
            "-frames:v",
            "1",
            "-c:v",
            &format.encoder,
        ]);
        if let Some(pixel_format) = settings.pixel_format {
            command.args(["-pix_fmt", pixel_format]);
        }
        if let Some(quality) = settings.global_quality {
            // `-q:v` is `global_quality` counted in FF_QP2LAMBDA, and it also sets the
            // QSCALE flag that tells the encoder to read it.
            let qscale = format!("{:.3}", f64::from(quality) / f64::from(QSCALE_UNIT));
            command.args(["-q:v", &qscale]);
        }
        for (name, value) in &settings.options {
            command.arg(format!("-{name}")).arg(value);
        }
        command.args([
            "-color_primaries",
            "bt709",
            "-color_trc",
            "iec61966_2_1",
            "-colorspace",
            "bt709",
            "-color_range",
            "pc",
        ]);
        command
            .arg(target.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("run {}: {error}", self.path.display()))?;
        {
            let mut stdin = child.stdin.take().ok_or("ffmpeg took no stdin")?;
            stdin
                .write_all(&rgba)
                .map_err(|error| format!("write to ffmpeg: {error}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("wait for ffmpeg: {error}"))?;
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr);
            let reason = reason.trim();
            return Err(if reason.is_empty() {
                format!("ffmpeg exited with {}", output.status)
            } else {
                format!("ffmpeg: {reason}")
            });
        }
        let bytes = std::fs::read(target.path())
            .map_err(|error| format!("read {}: {error}", target.path().display()))?;
        if bytes.is_empty() {
            return Err("ffmpeg produced no image".into());
        }
        Ok(bytes)
    }

    /// The first picture of `bytes`, written back out as RGBA8 PNG.
    ///
    /// PNG is the one thing both sides already agree on: the program can only describe a
    /// picture through a file, and this program reads and writes that format itself.
    pub fn decode(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        // A pipe cannot be sought, and an ISO-BMFF picture (AVIF, HEIF) keeps its index
        // after the picture: from stdin the program calls the file partial and gives no
        // frame. The source is a file for that reason, not to please the muxer.
        let source = Temporary::reserve("image")?;
        std::fs::write(source.path(), bytes)
            .map_err(|error| format!("write {}: {error}", source.path().display()))?;
        let output = Command::new(&self.path)
            .args(["-hide_banner", "-loglevel", "error"])
            .arg("-i")
            .arg(source.path())
            .args([
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "png",
                "-pix_fmt",
                "rgba",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("run {}: {error}", self.path.display()))?;
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr);
            let reason = reason.trim();
            return Err(if reason.is_empty() {
                format!("ffmpeg exited with {}", output.status)
            } else {
                format!("ffmpeg: {reason}")
            });
        }
        if output.stdout.is_empty() {
            return Err("ffmpeg produced no image".into());
        }
        Ok(output.stdout)
    }

    fn help(&self, encoder: &str) -> Result<String, String> {
        let encoder = format!("encoder={encoder}");
        run(&self.path, &["-hide_banner", "-h", &encoder])
    }
}

/// The first `ffmpeg` on `PATH`, the way a shell would find it.
pub fn find() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|directory| directory.join("ffmpeg"))
        .find(|candidate| candidate.is_file())
}

fn offered_in_program(encoders: &[ListedEncoder]) -> Vec<Format> {
    let mut formats = Vec::new();
    for extension in OFFERED {
        let codec = CODECS
            .iter()
            .find(|(offered, _)| offered == extension)
            .map(|(_, codec)| *codec);
        let Some(codec) = codec else { continue };
        let mut candidates: Vec<&ListedEncoder> = encoders
            .iter()
            .filter(|encoder| encoder.codec == codec && is_software(&encoder.name))
            .collect();
        candidates.sort_by(|left, right| {
            (!left.name.starts_with("lib"), &left.name)
                .cmp(&(!right.name.starts_with("lib"), &right.name))
        });
        let Some(encoder) = candidates.first() else {
            continue;
        };
        formats.push(Format {
            extension,
            encoder: encoder.name.clone(),
            muxer: None,
            description: encoder.long_name.clone(),
        });
    }
    formats
}

/// `ffmpeg -encoders`: flags, name, description ending in "(codec <id>)".
fn parse_encoders(listing: &str) -> Vec<ListedEncoder> {
    let mut encoders = Vec::new();
    // The legend above the dashed line explains the flag column; it is not a table row.
    let mut in_table = false;
    for line in listing.lines() {
        if line.trim_start().starts_with("---") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(flags) = fields.first() else {
            continue;
        };
        if !is_flag_column(flags) || fields.len() < 3 {
            continue;
        }
        let name = fields[1];
        encoders.push(ListedEncoder {
            name: name.to_string(),
            long_name: fields[2..].join(" "),
            // ffmpeg prints the codec only when it differs from the name.
            codec: codec_of(&fields).unwrap_or_else(|| name.to_string()),
        });
    }
    encoders
}

/// The flag column: media type letters and dots, never the header's `V..... = Video`.
fn is_flag_column(column: &str) -> bool {
    column.len() >= 6
        && column.chars().all(|c| c == '.' || c.is_ascii_uppercase())
        && !column.chars().all(|c| c == '.')
}

/// The `av1` in `... (codec av1)`, when the line has one.
fn codec_of(fields: &[&str]) -> Option<String> {
    let at = fields.iter().position(|field| *field == "(codec")?;
    fields
        .get(at + 1)
        .map(|codec| codec.trim_end_matches(')').to_string())
}

/// `ffmpeg -h encoder=NAME`: the AVOptions, one per line, consts indented without a dash.
fn parse_options(help: &str) -> Vec<FormatOption> {
    let mut options = Vec::new();
    for line in help.lines() {
        let Some(rest) = line.trim_start().strip_prefix('-') else {
            continue;
        };
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        let Some(kind_at) = rest.find('<') else {
            continue;
        };
        let after_kind = match rest[kind_at..].find('>') {
            Some(end) => &rest[kind_at + end + 1..],
            None => continue,
        };
        let mut fields = after_kind.split_whitespace();
        // The flag column such as "E..V......." comes before the description.
        let mut description = after_kind.trim_start();
        if let Some(flags) = fields.next().filter(|flags| is_flag_column(flags)) {
            description = after_kind.trim_start()[flags.len()..].trim_start();
        }
        let (help, default) = split_default(description);
        options.push(FormatOption {
            name: name.to_string(),
            help,
            default,
        });
    }
    options
}

/// The text before "(default X)", and X itself when it is there.
fn split_default(text: &str) -> (String, String) {
    let Some(at) = text.rfind("(default ") else {
        return (text.trim().to_string(), String::new());
    };
    let default = text[at + "(default ".len()..].trim_end_matches(')');
    let help = text[..at].trim().to_string();
    (help, default.trim().to_string())
}

/// The names after "Supported pixel formats:"; `None` when the help has no such line.
fn supported_pixel_formats(help: &str) -> Option<Vec<String>> {
    for line in help.lines() {
        let Some(rest) = line.trim_start().strip_prefix("Supported pixel formats:") else {
            continue;
        };
        let names: Vec<String> = rest
            .split_whitespace()
            .filter(|name| *name != "(none)")
            .map(str::to_string)
            .collect();
        return Some(names);
    }
    None
}

fn run(program: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("run {}: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} {} exited with {}",
            program.display(),
            arguments.join(" "),
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A file name nobody else has, removed when it goes out of scope.
struct Temporary(PathBuf);

impl Temporary {
    fn reserve(extension: &str) -> Result<Self, String> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        for _ in 0..8 {
            let name = format!(
                "crabsnap-{}-{}.{extension}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(name);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("create {}: {error}", path.display()));
                }
            }
        }
        Err("could not find a free temporary file name".into())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        // The file is ours and in the temporary directory; a failure here has no reader.
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("crabsnap: could not remove {}: {error}", self.0.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real listing shape: a flag column, the name, then a description ending in the
    /// codec. The legend lines above the table are not encoders.
    #[test]
    fn the_encoder_listing_yields_names_and_codecs() {
        let listing = "\
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libaom-av1           libaom AV1 (codec av1)
 VFS..D mjpeg                MJPEG (Motion JPEG) (codec mjpeg)
 V....D libwebp_anim         libwebp WebP image (codec webp)
 V....D libwebp              libwebp WebP image (codec webp)
";
        let encoders = parse_encoders(listing);
        let names: Vec<&str> = encoders.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["libaom-av1", "mjpeg", "libwebp_anim", "libwebp"]);
        assert_eq!(encoders[2].codec, "webp", "the suffix names the codec");
        assert_eq!(
            encoders[1].codec, "mjpeg",
            "without a suffix the encoder's name is the codec"
        );
        assert_eq!(
            parse_encoders(
                "Encoders:\n ------\n V....D png   PNG (Portable Network Graphics) image"
            )
            .first()
            .map(|encoder| encoder.codec.as_str()),
            Some("png"),
            "a row the legend does not explain is still a row"
        );
    }

    /// `-h encoder=` prints one option per dashed line; the const rows under an option are
    /// indented without a dash, and the default sits at the end.
    #[test]
    fn the_option_list_keeps_names_help_and_defaults() {
        let help = "\
Encoder libwebp [libwebp WebP image]:
    General capabilities: dr1
    Supported pixel formats: yuva420p bgra
libwebp AVOptions:
  -lossless           <boolean>    E..V....... Use lossless mode (default false)
  -quality            <int>        E..V....... Quality (from 0 to 100) (default 75)
  -preset             <int>        E..V....... Configuration preset (from -1 to 6) (default -1)
     none             -1            E..V.......
";
        let options = parse_options(help);
        assert_eq!(options.len(), 3, "const rows are not options");
        assert_eq!(options[0].name, "lossless");
        assert_eq!(options[0].default, "false");
        assert_eq!(options[1].help, "Quality (from 0 to 100)");
        assert_eq!(options[1].default, "75");
        assert_eq!(
            supported_pixel_formats(help),
            Some(vec!["yuva420p".to_string(), "bgra".to_string()])
        );
    }
}
