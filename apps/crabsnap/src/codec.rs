//! 文件编码：PNG 随程序自带，其余格式一律交给 ffmpeg（见 `ffmpeg`）。
//!
//! 这张表是本机能力的唯一出口：`Ffmpeg` 说自己能写什么，这里把自带的 PNG 加进去。
//! 用户看到的「能写哪些格式」和真正编码时选的路，都从这一个地方来。

use std::collections::BTreeMap;
use std::path::Path;

use crate::capture::{Cropped, density, pixels_per_metre};
use crate::ffmpeg::{Ffmpeg, Format};

/// 一个能写出的文件格式：就是它的后缀。
///
/// 后缀是唯一身份——它同时决定文件名、muxer 的猜测和 `--format` 的写法。别名的写法
/// （`jpeg`、`tif`）在这里折成规范写法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding {
    extension: &'static str,
}

impl Encoding {
    pub const PNG: Self = Self { extension: "png" };

    pub fn extension(self) -> &'static str {
        self.extension
    }

    /// 名字就是后缀：`jpg` 而不是 `jpeg`，与写出的文件一致。
    pub fn name(self) -> &'static str {
        self.extension
    }

    pub fn is_png(self) -> bool {
        self.extension == Self::PNG.extension
    }

    pub fn parse(text: &str) -> Result<Encoding, String> {
        let text = text.to_ascii_lowercase();
        if text == Self::PNG.extension {
            return Ok(Self::PNG);
        }
        crate::ffmpeg::canonical(&text)
            .map(|extension| Self { extension })
            .ok_or_else(|| {
                let offered = std::iter::once(Self::PNG.extension)
                    .chain(crate::ffmpeg::offered().iter().copied())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("unknown format {text:?}; expected one of {offered}")
            })
    }

    /// 参数优先，其次文件名，再次配置，最后 PNG。
    pub fn resolve(
        flag: Option<&str>,
        path: Option<&Path>,
        configured: Option<&str>,
    ) -> Result<Encoding, String> {
        if let Some(name) = flag {
            return Self::parse(name);
        }
        if let Some(found) = path.and_then(Self::of_path) {
            return Ok(found);
        }
        match configured {
            Some(name) => Self::parse(name),
            None => Ok(Self::PNG),
        }
    }

    /// 文件名说了什么，且是本机能写的格式；说不出来就是 `None`。
    pub fn of_path(path: &Path) -> Option<Encoding> {
        let extension = path.extension()?.to_str()?;
        Self::parse(extension).ok()
    }
}

/// 逐格式参数，按后缀归类。
type ConfiguredOptions = Vec<(&'static str, Vec<(String, String)>)>;

/// 本机实际能写的格式。
pub struct Codecs {
    ffmpeg: Ffmpeg,
    available: Vec<Encoding>,
    /// 配置里给出的逐格式参数，按后缀归类；探测时已经核对过名字与取值。
    options: ConfiguredOptions,
    /// 用户配置的路径或参数不可用时的原因；`info formats` 会说出来。
    failure: Option<String>,
}

impl Codecs {
    /// 探测本机：PNG 始终有，其余看 ffmpeg 的库或可执行文件。
    ///
    /// `program` 与 `directory` 是用户配置的路径；写错时记下原因，而不是悄悄换一个地方找。
    pub fn probe(
        program: Option<&Path>,
        directory: Option<&Path>,
        options: &BTreeMap<String, String>,
    ) -> Self {
        let mut failure = None;
        let directory = match directory {
            Some(path) if path.is_dir() => Some(path),
            Some(path) => {
                failure = Some(format!(
                    "configured codec library directory {} is not a directory",
                    path.display()
                ));
                None
            }
            None => None,
        };
        let program = match program {
            Some(path) if path.is_file() => Some(path),
            Some(path) => {
                failure.get_or_insert_with(|| {
                    format!("configured ffmpeg {} is not a file", path.display())
                });
                None
            }
            None => None,
        };
        let ffmpeg = Ffmpeg::probe(program, directory);
        let mut available = vec![Encoding::PNG];
        available.extend(ffmpeg.formats().iter().map(|format| Encoding {
            extension: format.extension,
        }));
        let (options, problems) = collect_options(&ffmpeg, options);
        if !problems.is_empty() {
            let reason = problems.join("; ");
            match &mut failure {
                Some(existing) => {
                    existing.push_str("; ");
                    existing.push_str(&reason);
                }
                None => failure = Some(reason),
            }
        }
        Self {
            ffmpeg,
            available,
            options,
            failure,
        }
    }

    /// Why a configured path could not be used, when there is more to say than "nothing found".
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// The formats this machine can write, PNG first.
    pub fn available(&self) -> &[Encoding] {
        &self.available
    }

    /// Where a format comes from, for `info formats`.
    pub fn source(&self, encoding: Encoding) -> String {
        if encoding.is_png() {
            return "built in".into();
        }
        let encoder = self
            .ffmpeg
            .format(encoding.extension())
            .map(|format| format.encoder.as_str())
            .unwrap_or("?");
        match self.ffmpeg.source() {
            Some(source) => format!("{encoder} via {source}"),
            None => "unavailable".into(),
        }
    }

    /// 本机写扩展格式的 ffmpeg，供 `info formats` 报出参数表。
    pub fn ffmpeg(&self) -> &Ffmpeg {
        &self.ffmpeg
    }

    /// Refuse a format this machine cannot write, naming what is missing.
    pub fn ensure(&self, encoding: Encoding) -> Result<(), String> {
        if self.available.contains(&encoding) {
            return Ok(());
        }
        if let Some(reason) = &self.failure {
            return Err(format!("{reason} (PNG is built in)"));
        }
        let missing = if self.ffmpeg.source().is_some() {
            "the ffmpeg here has no encoder for it"
        } else {
            "there is no ffmpeg library or program here"
        };
        Err(format!(
            "this machine cannot write {}: {missing} (PNG is built in)",
            encoding.name()
        ))
    }

    /// The image in `encoding`: PNG here, everything else through ffmpeg.
    pub fn encode(&self, image: &Cropped, encoding: Encoding) -> Result<Vec<u8>, String> {
        self.ensure(encoding)?;
        if encoding.is_png() {
            return image.png();
        }
        let format = self.format(encoding)?;
        let empty = Vec::new();
        let configured = self
            .options
            .iter()
            .find(|(extension, _)| *extension == encoding.extension())
            .map(|(_, list)| list)
            .unwrap_or(&empty);
        let bytes = self.ffmpeg.encode(&format, image, configured)?;
        Ok(with_density(bytes, encoding, image.scale))
    }

    /// Write the image where the path says, in `encoding`.
    pub fn write(&self, image: &Cropped, path: &Path, encoding: Encoding) -> Result<(), String> {
        std::fs::write(path, self.encode(image, encoding)?)
            .map_err(|error| format!("write {}: {error}", path.display()))
    }

    fn format(&self, encoding: Encoding) -> Result<Format, String> {
        self.ffmpeg
            .format(encoding.extension())
            .cloned()
            .ok_or_else(|| format!("no encoder for .{}", encoding.extension()))
    }
}

/// 配置里的逐格式参数：键是 `<后缀>.<参数名>`，核对不过的键在这里被点名。
fn collect_options(
    ffmpeg: &Ffmpeg,
    options: &BTreeMap<String, String>,
) -> (ConfiguredOptions, Vec<String>) {
    let mut collected: ConfiguredOptions = Vec::new();
    let mut problems = Vec::new();
    for (key, value) in options {
        let Some((format, name)) = key.split_once('.') else {
            problems.push(format!("codec.options: {key:?} is not <format>.<option>"));
            continue;
        };
        let Ok(encoding) = Encoding::parse(format) else {
            problems.push(format!(
                "codec.options: {key:?} names no format this build offers"
            ));
            continue;
        };
        if encoding.is_png() {
            problems.push(format!(
                "codec.options: {key:?}: PNG is written by this program and takes no encoder options"
            ));
            continue;
        }
        let Some(known) = ffmpeg.format(encoding.extension()) else {
            problems.push(format!(
                "codec.options: {key:?}: this machine cannot write {}",
                encoding.extension()
            ));
            continue;
        };
        let mut pair = vec![(name.to_string(), value.clone())];
        if let Err(reason) = ffmpeg.check_options(known, &pair) {
            problems.push(format!("codec.options: {key:?}: {reason}"));
            continue;
        }
        match collected
            .iter_mut()
            .find(|(extension, _)| *extension == encoding.extension())
        {
            Some((_, list)) => list.append(&mut pair),
            None => collected.push((encoding.extension(), pair)),
        }
    }
    (collected, problems)
}

/// 把「这张图原本多大」写进容器自己的元数据字段。
///
/// PNG 的 `pHYs` 由这里的编码器写；JPEG 的密度在 JFIF 的 APP0 里，BMP 在信息头里，TIFF 在
/// 282/283 号标签里——ffmpeg 都不写我们要的值，所以在编码之后补上。WebP、AVIF、JXL 的密度
/// 只能放进 EXIF 之类的元数据项，GIF 和 QOI 根本没有这个字段；这几种不写，文档里写明了。
fn with_density(bytes: Vec<u8>, encoding: Encoding, scale: f32) -> Vec<u8> {
    let dpi = density(scale);
    match encoding.extension() {
        "jpg" => with_jfif_density(bytes, dpi),
        "bmp" => with_bmp_density(bytes, dpi),
        "tiff" => with_tiff_density(bytes, dpi),
        _ => bytes,
    }
}

fn read_u16(bytes: &[u8], at: usize, little: bool) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    })
}

fn read_u32(bytes: &[u8], at: usize, little: bool) -> Option<u32> {
    let quad = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(if little {
        u32::from_le_bytes(quad)
    } else {
        u32::from_be_bytes(quad)
    })
}

/// BMP 的每米像素在 `BITMAPINFOHEADER` 里，紧跟着图像尺寸之后。
fn with_bmp_density(mut bmp: Vec<u8>, dpi: u32) -> Vec<u8> {
    const FILE_HEADER: usize = 14;
    // 文件头之后是 DIB 头；12 字节的旧核心头没有这两个字段。
    let Some(size) = read_u32(&bmp, FILE_HEADER, true) else {
        return bmp;
    };
    if size < 40 {
        return bmp;
    }
    let per_metre = pixels_per_metre(dpi);
    let (x, y) = (FILE_HEADER + 24, FILE_HEADER + 28);
    if let Some(field) = bmp.get_mut(x..x + 4) {
        field.copy_from_slice(&per_metre.to_le_bytes());
    }
    if let Some(field) = bmp.get_mut(y..y + 4) {
        field.copy_from_slice(&per_metre.to_le_bytes());
    }
    bmp
}

/// TIFF 的分辨率是 282/283 号标签指向的一对 32 位有理数，单位由 296 号标签说是英寸。
fn with_tiff_density(mut tiff: Vec<u8>, dpi: u32) -> Vec<u8> {
    let little = match tiff.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return tiff,
    };
    let Some(ifd) = read_u32(&tiff, 4, little) else {
        return tiff;
    };
    let Some(count) = read_u16(&tiff, ifd as usize, little) else {
        return tiff;
    };
    for entry in 0..count as usize {
        let at = ifd as usize + 2 + entry * 12;
        let Some(tag) = read_u16(&tiff, at, little) else {
            break;
        };
        if tag != 282 && tag != 283 {
            continue;
        }
        // A RATIONAL with a count of one stores where its value is, not the value.
        let Some(value) = read_u32(&tiff, at + 8, little) else {
            break;
        };
        let value = value as usize;
        let encode = |number: u32| {
            if little {
                number.to_le_bytes()
            } else {
                number.to_be_bytes()
            }
        };
        for (offset, bytes) in [(0, encode(dpi)), (4, encode(1))] {
            if let Some(field) = tiff.get_mut(value + offset..value + offset + 4) {
                field.copy_from_slice(&bytes);
            }
        }
    }
    tiff
}

/// 给 JPEG 补一段 JFIF APP0，带上密度。
///
/// 格式规定 APP0 紧跟 SOI，所以插在第二个字节之后；已经有 APP0 的文件保留它自己的说法。
fn with_jfif_density(mut jpeg: Vec<u8>, dpi: u32) -> Vec<u8> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) || jpeg.get(2..4) == Some(&[0xFF, 0xE0][..]) {
        return jpeg;
    }
    let density = dpi.clamp(1, 65535) as u16;
    let mut header = vec![0xFF, 0xE0, 0x00, 0x10];
    header.extend_from_slice(b"JFIF\0");
    // 版本 1.1，密度单位是每英寸点数，宽高各一个，没有缩略图。
    header.extend_from_slice(&[0x01, 0x01, 0x01]);
    header.extend_from_slice(&density.to_be_bytes());
    header.extend_from_slice(&density.to_be_bytes());
    header.extend_from_slice(&[0x00, 0x00]);
    jpeg.splice(2..2, header);
    jpeg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 参数优先，其次文件名，再次配置，最后 PNG；认识不了的后缀不抢决定权。
    #[test]
    fn an_encoding_follows_flag_then_name_then_config() {
        let jpg = Path::new("/tmp/a.jpg");
        let unknown = Path::new("/tmp/a.zzz");
        let resolve = |flag: Option<&str>, path: Option<&Path>, config: Option<&str>| {
            Encoding::resolve(flag, path, config).expect("a valid encoding")
        };
        assert_eq!(resolve(None, None, None), Encoding::PNG, "the fallback");
        assert_eq!(
            resolve(None, None, Some("webp")).extension(),
            "webp",
            "config"
        );
        assert_eq!(
            resolve(None, Some(jpg), Some("webp")).extension(),
            "jpg",
            "the file name beats the config"
        );
        assert_eq!(
            resolve(Some("webp"), Some(jpg), Some("jpeg")).extension(),
            "webp",
            "the flag beats both"
        );
        assert_eq!(
            resolve(None, Some(unknown), Some("jpeg")).extension(),
            "jpg",
            "an extension nobody knows leaves the config in charge"
        );
        assert_eq!(
            resolve(Some("JPEG"), None, None).extension(),
            "jpg",
            "a name is not case sensitive and the alias folds to one spelling"
        );
        assert!(Encoding::resolve(Some("gif"), None, None).is_ok());
        assert!(Encoding::resolve(Some("mkv"), None, None).is_err());
    }

    /// 配置写错的路径要被说出来，而不是悄悄换一条路；PNG 不依赖任何一条。
    #[test]
    fn a_configured_path_that_is_not_there_is_named() {
        let codecs = Codecs::probe(
            Some(Path::new("/nonexistent/ffmpeg")),
            None,
            &BTreeMap::new(),
        );
        assert!(
            codecs
                .failure()
                .is_some_and(|reason| reason.contains("/nonexistent/ffmpeg")),
            "the configured path is the reason: {:?}",
            codecs.failure()
        );
        assert!(codecs.available().contains(&Encoding::PNG));
        let codecs = Codecs::probe(None, Some(Path::new("/nonexistent/libs")), &BTreeMap::new());
        assert!(
            codecs
                .failure()
                .is_some_and(|reason| reason.contains("/nonexistent/libs")),
            "the configured directory is the reason: {:?}",
            codecs.failure()
        );
        assert!(codecs.available().contains(&Encoding::PNG));
    }

    /// 逐格式参数的键形状：没有点、格式不认识、给 PNG 加参数，各有各的说法。
    #[test]
    fn malformed_per_format_keys_are_named() {
        let options = BTreeMap::from([
            ("png.dpi".to_string(), "192".to_string()),
            ("mkv.whatever".to_string(), "1".to_string()),
            ("webp".to_string(), "1".to_string()),
        ]);
        let codecs = Codecs::probe(None, None, &options);
        let reason = codecs.failure().expect("every bad key is named");
        for expected in [
            "\"png.dpi\": PNG is written by this program",
            "\"mkv.whatever\" names no format this build offers",
            "\"webp\" is not <format>.<option>",
        ] {
            assert!(
                reason.contains(expected),
                "{expected} is missing from {reason}"
            );
        }
        assert!(codecs.available().contains(&Encoding::PNG));
    }

    /// BMP 和 TIFF 的密度字段各自在哪儿，写在测试里以防改错偏移。
    #[test]
    fn density_lands_in_the_fields_bmp_and_tiff_define() {
        let mut bmp = vec![0u8; 54];
        bmp[0..2].copy_from_slice(b"BM");
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        let bmp = with_bmp_density(bmp, 192);
        assert_eq!(read_u32(&bmp, 38, true), Some(pixels_per_metre(192)));
        assert_eq!(read_u32(&bmp, 42, true), Some(pixels_per_metre(192)));

        let mut tiff = vec![0u8; 54];
        tiff[0..4].copy_from_slice(b"II*\0");
        tiff[4..8].copy_from_slice(&8u32.to_le_bytes());
        tiff[8..10].copy_from_slice(&2u16.to_le_bytes());
        for (entry, (tag, value)) in [(282u16, 38u32), (283, 46)].into_iter().enumerate() {
            let at = 10 + entry * 12;
            tiff[at..at + 2].copy_from_slice(&tag.to_le_bytes());
            tiff[at + 2..at + 4].copy_from_slice(&5u16.to_le_bytes());
            tiff[at + 4..at + 8].copy_from_slice(&1u32.to_le_bytes());
            tiff[at + 8..at + 12].copy_from_slice(&value.to_le_bytes());
        }
        let tiff = with_tiff_density(tiff, 192);
        for at in [38, 46] {
            assert_eq!(
                read_u32(&tiff, at, true),
                Some(192),
                "the numerator at {at}"
            );
            assert_eq!(
                read_u32(&tiff, at + 4, true),
                Some(1),
                "the denominator at {at}"
            );
        }

        assert_eq!(
            with_density(vec![0u8; 4], Encoding::parse("qoi").unwrap(), 2.0),
            vec![0u8; 4],
            "a format with no density field is left alone"
        );
    }

    /// APP0 紧跟 SOI，长度含自身，密度是每英寸点数；已有 APP0 的文件不动。
    #[test]
    fn a_jpeg_gains_its_density_in_a_jfif_header() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xFE, 0x00, 0x02];
        let marked = with_jfif_density(jpeg.clone(), 192);
        assert_eq!(
            &marked[..],
            &[
                0xFF, 0xD8, // SOI
                0xFF, 0xE0, 0x00, 0x10, // APP0, 16 bytes of segment
                b'J', b'F', b'I', b'F', 0x00, // identifier
                0x01, 0x01, // version 1.1
                0x01, // dots per inch
                0x00, 0xC0, 0x00, 0xC0, // 192 x 192
                0x00, 0x00, // no thumbnail
                0xFF, 0xFE, 0x00, 0x02, // the comment that followed SOI
            ]
        );
        let already = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02];
        assert_eq!(
            with_jfif_density(already.clone(), 96),
            already,
            "a JFIF header of its own is the file's answer"
        );
        assert_eq!(
            with_jfif_density(vec![0x89, b'P'], 96),
            vec![0x89, b'P'],
            "anything that is not a JPEG is left alone"
        );
    }
}
