//! 配置文件：用户能改的东西只在这一个地方声明，默认值也是。
//!
//! 读不懂的配置是错误，不是静默回退：用户改了键名却没有任何效果，只会让人以为程序坏了。
//! 文件不存在是正常情况，等于全部默认值。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 用户配置。
///
/// 本构建不认识的键（旧版本的 `[annotate.*]`、`[ui]` 里没有消费方的项）被
/// 记下来而不是拒绝：用户机器上可能已经有一份别的版本的配置，读不懂它不该让截图工具整个
/// 罢工。反过来，本构建写进 schema 的键都必须有消费方——配置文件里的一个死键，就是一个
/// 「改了没效果」的陷阱。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub capture: Capture,
    pub shots: Shots,
    pub codec: Codec,
    pub ui: Ui,
    pub pin: Pin,
    pub shortcuts: Shortcuts,
    /// 载入时忽略掉的键，`info config` 会说出来。
    #[serde(skip)]
    pub unknown: Vec<String>,
}

/// 抓取。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capture {
    /// 固定抓取后端（`ext`、`screencopy`、`portal`）；省略时按回退链自动选。
    pub backend: Option<String>,
}

/// 编码：内置 PNG，扩展格式交给 ffmpeg。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Codec {
    /// 额外搜索共享库的目录；省略时只问动态链接器自己的搜索路径。
    pub library_directory: Option<PathBuf>,
    /// ffmpeg 可执行文件的完整路径；省略时在 `PATH` 里找。只有共享库缺失时才会用到它。
    pub program: Option<PathBuf>,
    /// 逐格式的编码器参数，键写作 `<后缀>.<参数名>`（`webp.lossless`、`avif.crf`），值是
    /// ffmpeg 自己认的文本。参数名与默认值的权威是编码器：`info formats --verbose` 列出
    /// 来的就是这里能写的键；写错的键会被点名，而不是悄悄不生效。
    pub options: BTreeMap<String, String>,
}

/// `[shortcuts]`：动作名 → 和弦拼写（`Ctrl+Shift+4`、`Print`），一个动作可写多个，用逗号分隔
/// （`A, Left`）。
///
/// 缺键表示用内置默认；键在而值为空，表示用户清除了这个动作。动作名取自
/// `crabsnap_core::hotkeys::Shortcut::name`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Shortcuts(pub BTreeMap<String, String>);

/// 界面偏好：只收本构建真的会读的键，没有消费方的旧键留给 `unknown` 报告。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// 界面语言：`auto` 跟随系统，或 BCP-47 标签（如 `zh-CN`）；省略即 `auto`。
    /// 旧键 `lang` 读作它的别名，下次保存时改写成这个键。
    #[serde(alias = "lang")]
    pub language: Option<String>,
}

/// `[pin]`：屏上贴图的外观。键都可省略，省略即内置外观。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pin {
    /// 圆角半径，逻辑像素；省略为 0，即直角。
    pub radius: Option<f32>,
    /// 静止时卡片的边框与投影；省略即内置的静止外观。
    pub idle: Option<Skin>,
    /// 指针停在卡片上时的边框与投影；省略即内置的激活外观。
    pub active: Option<Skin>,
    /// 旧键：`idle.shadow_blur` 的别名，`idle.shadow_blur` 写了就以它为准。
    pub shadow_blur: Option<f32>,
    /// 旧键：`idle.shadow_alpha` 的别名，`idle.shadow_alpha` 写了就以它为准。
    pub shadow_alpha: Option<f32>,
}

/// 一个状态下卡片的边框与投影；省略的键取该状态的内置值。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Skin {
    /// 边框粗细，逻辑像素；0 不画边框。
    pub border_width: Option<f32>,
    /// `#RRGGBB` 或 `#RRGGBBAA`（也收不带 `#` 的写法）。
    pub border_colour: Option<String>,
    /// 投影的模糊半径，逻辑像素；0 关掉投影。
    pub shadow_blur: Option<f32>,
    /// 投影的扩散半径，逻辑像素：负值收小，正值铺开。
    pub shadow_spread: Option<f32>,
    /// 投影的不透明度，0..=1。
    pub shadow_alpha: Option<f32>,
    /// 投影的颜色，`#RRGGBB`；浓度由 `shadow_alpha` 单独给出。
    pub shadow_colour: Option<String>,
}

/// 解析后的贴图外观：文件里没写的键取这里声明的默认值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinLook {
    pub radius: f32,
    pub idle: PinSkin,
    pub active: PinSkin,
}

impl PinLook {
    /// 指针停在贴图上时用哪一套边框与投影。
    pub fn skin(&self, active: bool) -> PinSkin {
        if active { self.active } else { self.idle }
    }
}

/// 一个状态下卡片的边框与投影，已经是绘制用的数值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinSkin {
    pub border_width: f32,
    /// 边框颜色，`[r, g, b, a]`。
    pub border: [u8; 4],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_alpha: f32,
    /// 投影颜色，`[r, g, b]`；浓度是 `shadow_alpha`。
    pub shadow: [u8; 3],
}

impl Default for PinLook {
    fn default() -> Self {
        Self {
            // A card is a card: its corners are rounded unless the file says otherwise.
            radius: 12.0,
            idle: PinSkin {
                border_width: 0.0,
                border: [0, 0, 0, 0],
                shadow_blur: 16.0,
                shadow_spread: 0.0,
                shadow_alpha: 120.0 / 255.0,
                shadow: [0, 0, 0],
            },
            // A hovered card lifts: a hairline ring and a slightly further, denser shadow.
            // The ring is drawn over the card's own edge, so it is a dark line: white would
            // vanish into the surface it outlines.
            active: PinSkin {
                border_width: 1.0,
                border: [0, 0, 0, 64],
                shadow_blur: 24.0,
                shadow_spread: 2.0,
                shadow_alpha: 0.55,
                shadow: [0, 0, 0],
            },
        }
    }
}

/// 截图文件落盘的位置。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shots {
    /// 截图目录；省略时用图片目录下的 `Screenshots`。
    pub directory: Option<PathBuf>,

    /// 默认编码（`png`、`jpeg`、`webp`）；省略时是 PNG。命令行与文件扩展名都优先于它。
    pub format: Option<String>,
}

impl Config {
    /// 读配置；没有文件就是默认值，读不懂则带上路径与原因。
    pub fn load(path: &Path) -> Result<Config, String> {
        Self::load_from(path)
    }

    /// 写回配置：只动本构建认识的键，其它段落原样保留。
    ///
    /// 用户机器上的文件可能还带着别的版本的 `[shortcuts]`、`[annotate.*]` 或 `[ui]` 的
    /// 其余键；整体重写会把它们全部丢掉，那不是「保存设置」，那是数据丢失。
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
        };
        let mut table: toml::Table = if text.trim().is_empty() {
            toml::Table::new()
        } else {
            toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?
        };
        write_section(
            &mut table,
            "capture",
            "backend",
            self.capture.backend.as_deref(),
        )?;
        write_section(
            &mut table,
            "shots",
            "directory",
            self.shots
                .directory
                .as_deref()
                .map(|directory| directory.to_string_lossy().into_owned())
                .as_deref(),
        )?;
        write_section(&mut table, "shots", "format", self.shots.format.as_deref())?;
        write_section(
            &mut table,
            "codec",
            "library_directory",
            self.codec
                .library_directory
                .as_deref()
                .map(|directory| directory.to_string_lossy().into_owned())
                .as_deref(),
        )?;
        write_section(
            &mut table,
            "codec",
            "program",
            self.codec
                .program
                .as_deref()
                .map(|program| program.to_string_lossy().into_owned())
                .as_deref(),
        )?;
        write_section(&mut table, "ui", "language", self.ui.language.as_deref())?;
        // The alias is read, never written: a file that had `lang` leaves with `language`.
        remove_key(&mut table, "ui", "lang");
        write_number(&mut table, "pin", "radius", self.pin.radius)?;
        write_number(&mut table, "pin", "shadow_blur", self.pin.shadow_blur)?;
        write_number(&mut table, "pin", "shadow_alpha", self.pin.shadow_alpha)?;
        // The shortcut table is written from the map: it came from this file, so a key the
        // map no longer has is a removal the window made, not a foreign entry to keep.
        for (name, value) in &self.shortcuts.0 {
            write_section(&mut table, "shortcuts", name, Some(value))?;
        }
        if self.shortcuts.0.is_empty() {
            table.remove("shortcuts");
        } else if let Some(section) = table
            .get_mut("shortcuts")
            .and_then(|entry| entry.as_table_mut())
        {
            section.retain(|name, _| self.shortcuts.0.contains_key(name));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        let text =
            toml::to_string_pretty(&table).map_err(|error| format!("encode config: {error}"))?;
        std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
    }

    /// 贴图外观，以及文件里那些被忽略的值；越界的数不能静默生效。
    pub fn pin_look(&self) -> (PinLook, Vec<String>) {
        let defaults = PinLook::default();
        let mut problems = Vec::new();
        let look = PinLook {
            radius: look_number(self.pin.radius, "radius", (0.0, 64.0), defaults.radius, &mut problems),
            idle: resolve_skin(
                self.pin.idle.as_ref(),
                "idle",
                defaults.idle,
                self.pin.shadow_blur,
                self.pin.shadow_alpha,
                &mut problems,
            ),
            active: resolve_skin(
                self.pin.active.as_ref(),
                "active",
                defaults.active,
                None,
                None,
                &mut problems,
            ),
        };
        (look, problems)
    }

    pub fn load_from(path: &Path) -> Result<Config, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(error) => return Err(format!("read {}: {error}", path.display())),
        };
        let table: toml::Table =
            toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;
        let unknown = unknown_keys(&table);
        let mut config: Config = table
            .try_into()
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        config.unknown = unknown;
        Ok(config)
    }
}

/// A length or alpha as a person would write it: no float noise in a message.
fn plain(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

/// One finite number inside `range`, or `fallback` with the problem named.
fn look_number(
    value: Option<f32>,
    name: &str,
    range: (f32, f32),
    fallback: f32,
    problems: &mut Vec<String>,
) -> f32 {
    match value {
        None => fallback,
        Some(value) if value.is_finite() && (range.0..=range.1).contains(&value) => value,
        Some(value) => {
            problems.push(format!(
                "pin.{name} = {} is outside {}..={}; using {}",
                plain(value),
                plain(range.0),
                plain(range.1),
                plain(fallback)
            ));
            fallback
        }
    }
}

/// `#RGB`, `#RRGGBB` or `#RRGGBBAA`, with or without the hash.
fn look_colour(text: &str) -> Option<[u8; 4]> {
    let text = text.trim().trim_start_matches('#');
    if !text.is_ascii() {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).ok();
    let wide = |at: usize| {
        let nibble = (text.as_bytes()[at] as char).to_digit(16)?;
        Some((nibble * 17) as u8)
    };
    match text.len() {
        3 => Some([wide(0)?, wide(1)?, wide(2)?, 255]),
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

fn look_rgba(
    text: Option<&str>,
    name: &str,
    fallback: [u8; 4],
    problems: &mut Vec<String>,
) -> [u8; 4] {
    let Some(text) = text else {
        return fallback;
    };
    match look_colour(text) {
        Some(colour) => colour,
        None => {
            problems.push(format!(
                "pin.{name} = {text:?} is not #RGB, #RRGGBB or #RRGGBBAA; ignored"
            ));
            fallback
        }
    }
}

/// One state's border and shadow: its table when it has one, the built-in look otherwise.
fn resolve_skin(
    state: Option<&Skin>,
    name: &str,
    fallback: PinSkin,
    legacy_blur: Option<f32>,
    legacy_alpha: Option<f32>,
    problems: &mut Vec<String>,
) -> PinSkin {
    let skin = state.cloned().unwrap_or_default();
    let shadow = match skin.shadow_colour.as_deref() {
        Some(text) => {
            let rgba = look_rgba(Some(text), &format!("{name}.shadow_colour"), [0, 0, 0, 255], problems);
            [rgba[0], rgba[1], rgba[2]]
        }
        None => fallback.shadow,
    };
    PinSkin {
        border_width: look_number(
            skin.border_width,
            &format!("{name}.border_width"),
            (0.0, 16.0),
            fallback.border_width,
            problems,
        ),
        border: look_rgba(
            skin.border_colour.as_deref(),
            &format!("{name}.border_colour"),
            fallback.border,
            problems,
        ),
        shadow_blur: look_number(
            skin.shadow_blur.or(legacy_blur),
            &format!("{name}.shadow_blur"),
            (0.0, 128.0),
            fallback.shadow_blur,
            problems,
        ),
        shadow_spread: look_number(
            skin.shadow_spread,
            &format!("{name}.shadow_spread"),
            (-64.0, 64.0),
            fallback.shadow_spread,
            problems,
        ),
        shadow_alpha: look_number(
            skin.shadow_alpha.or(legacy_alpha),
            &format!("{name}.shadow_alpha"),
            (0.0, 1.0),
            fallback.shadow_alpha,
            problems,
        ),
        shadow,
    }
}

/// 设置一个键；`None` 删掉它，让默认值重新生效。
fn write_section(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let Some(value) = value else {
        remove_key(table, section, key);
        return Ok(());
    };
    set_key(table, section, key, toml::Value::String(value.to_string()))
}

/// 设置一个数值键；`None` 删掉它，让默认值重新生效。
fn write_number(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    value: Option<f32>,
) -> Result<(), String> {
    let Some(value) = value else {
        remove_key(table, section, key);
        return Ok(());
    };
    set_key(table, section, key, toml::Value::Float(f64::from(value)))
}

fn remove_key(table: &mut toml::Table, section: &str, key: &str) {
    // Removing a key must not leave an empty table behind; a section nobody sets is noise.
    if let Some(existing) = table
        .get_mut(section)
        .and_then(|entry| entry.as_table_mut())
    {
        existing.remove(key);
        if existing.is_empty() {
            table.remove(section);
        }
    }
}

fn set_key(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    value: toml::Value,
) -> Result<(), String> {
    let entry = table
        .entry(section)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    entry
        .as_table_mut()
        .ok_or_else(|| format!("[{section}] in the config file is not a table"))?
        .insert(key.to_string(), value);
    Ok(())
}

/// 本构建不认识的键，一级一级写清楚。
/// The keys one `[pin.idle]` or `[pin.active]` table may carry.
const SKIN_KEYS: [&str; 6] = [
    "border_width",
    "border_colour",
    "shadow_blur",
    "shadow_spread",
    "shadow_alpha",
    "shadow_colour",
];

fn unknown_keys(table: &toml::Table) -> Vec<String> {
    let mut unknown = Vec::new();
    for (key, value) in table {
        // Every action name in `[shortcuts]` is the user's; the app decides which ones it
        // knows, so the whole table is known to the schema. `[codec.options]` is the same:
        // its keys are encoder parameter names, which only the encoder can judge.
        if key == "shortcuts" {
            continue;
        }
        let known: &[&str] = match key.as_str() {
            "capture" => &["backend"],
            "codec" => &["library_directory", "program", "options"],
            "shots" => &["directory", "format"],
            // `lang` is the old spelling, read as `language` and rewritten on save.
            "ui" => &["language", "lang"],
            "pin" => &["radius", "idle", "active", "shadow_blur", "shadow_alpha"],
            _ => &[],
        };
        match value.as_table() {
            Some(section) if !known.is_empty() => {
                for name in section.keys() {
                    if !known.contains(&name.as_str()) {
                        unknown.push(format!("{key}.{name}"));
                    }
                }
                if key == "pin" {
                    // The two states hold the same six keys; a typo there is just as silent.
                    for state in ["idle", "active"] {
                        let Some(skin) = section.get(state).and_then(|value| value.as_table())
                        else {
                            continue;
                        };
                        for name in skin.keys() {
                            if !SKIN_KEYS.contains(&name.as_str()) {
                                unknown.push(format!("{key}.{state}.{name}"));
                            }
                        }
                    }
                }
            }
            _ if !known.contains(&key.as_str()) => unknown.push(key.clone()),
            _ => {}
        }
    }
    unknown.sort();
    unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_defaults_and_a_foreign_key_is_reported_not_refused() {
        let dir = std::env::temp_dir().join(format!("crabsnap-config-{}", std::process::id()));
        let path = dir.join("config.toml");
        assert_eq!(
            Config::load_from(&path).expect("no file, no error"),
            Config::default()
        );

        std::fs::create_dir_all(&dir).expect("a directory for the test");
        std::fs::write(
            &path,
            "[capture]\nbackends = \"ext\"\n\n[annotate.arrow]\nwidth = 4\n",
        )
        .expect("write another version's file");
        let config = Config::load_from(&path).expect("a foreign file still loads");
        assert_eq!(config.capture.backend, None, "the typo names no field");
        assert_eq!(
            config.unknown,
            ["annotate", "capture.backends"],
            "what was ignored is named, once per key"
        );

        std::fs::write(
            &path,
            "[capture]\nbackend = \"ext\"\n\n[shortcuts]\nregion = \"Print\"\n",
        )
        .expect("write the key");
        let config = Config::load_from(&path).expect("the real key parses");
        assert_eq!(config.capture.backend.as_deref(), Some("ext"));
        assert_eq!(
            config.shortcuts.0.get("region").map(String::as_str),
            Some("Print"),
            "an action name is the user's, not the schema's"
        );
        assert!(
            config.unknown.is_empty(),
            "no shortcut name is reported as an unknown key"
        );

        std::fs::write(&path, "[capture\nbackend = 1\n").expect("write broken TOML");
        assert!(
            Config::load_from(&path).is_err(),
            "a file that is not TOML is still an error"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pin_value_outside_its_range_is_reported_and_the_default_applies() {
        let dir = std::env::temp_dir().join(format!("crabsnap-config-pin-{}", std::process::id()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("a directory for the test");
        std::fs::write(&path, "[pin]\nradius = -4.0\nshadow_alpha = 2.0\n")
            .expect("write the out-of-range values");
        let config = Config::load_from(&path).expect("the file still loads");
        let (look, problems) = config.pin_look();
        assert_eq!(
            look,
            PinLook::default(),
            "an impossible look is not applied"
        );
        assert_eq!(
            problems.len(),
            2,
            "each ignored value is named: {problems:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_old_lang_key_reads_as_language_and_is_rewritten() {
        let dir = std::env::temp_dir().join(format!("crabsnap-config-lang-{}", std::process::id()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("a directory for the test");
        std::fs::write(&path, "[ui]\nlang = \"zh-CN\"\n").expect("an old file");

        let config = Config::load_from(&path).expect("the alias loads");
        assert_eq!(config.ui.language.as_deref(), Some("zh-CN"));
        config.save_to(&path).expect("rewrite");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            text.contains("language = \"zh-CN\"") && !text.contains("lang ="),
            "the old key is rewritten, not kept: {text}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_keeps_the_sections_this_build_does_not_know() {
        let dir = std::env::temp_dir().join(format!("crabsnap-config-save-{}", std::process::id()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("a directory for the test");
        std::fs::write(&path, "[ui]\nfps = 120\n\n[annotate.arrow]\nwidth = 4\n")
            .expect("another version's file");

        let config = Config {
            capture: Capture {
                backend: Some("ext".into()),
            },
            shots: Shots {
                directory: Some(PathBuf::from("/tmp/shots")),
                format: Some("webp".into()),
            },
            codec: Codec::default(),
            ui: Ui {
                language: Some("zh-CN".into()),
            },
            pin: Pin {
                radius: Some(12.0),
                idle: None,
                active: None,
                shadow_blur: None,
                shadow_alpha: Some(0.25),
            },
            shortcuts: Shortcuts::default(),
            unknown: Vec::new(),
        };
        config.save_to(&path).expect("the file is written");

        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(text.contains("fps = 120"), "foreign values survive: {text}");
        assert!(
            text.contains("language = \"zh-CN\""),
            "the language is written: {text}"
        );
        assert!(text.contains("width = 4"), "foreign tables survive: {text}");
        assert!(
            text.contains("radius = 12.0") && text.contains("shadow_alpha = 0.25"),
            "pin numbers are written as numbers: {text}"
        );
        let loaded = Config::load_from(&path).expect("reload");
        assert_eq!(loaded.capture.backend.as_deref(), Some("ext"));
        assert_eq!(loaded.shots.format.as_deref(), Some("webp"));
        assert_eq!(
            loaded.shots.directory.as_deref(),
            Some(Path::new("/tmp/shots"))
        );
        assert_eq!(loaded.ui.language.as_deref(), Some("zh-CN"));
        assert_eq!(
            loaded.unknown,
            ["annotate", "ui.fps"],
            "a ui key without a consumer is named, not silently kept"
        );

        let mut with_shortcuts = loaded.clone();
        with_shortcuts
            .shortcuts
            .0
            .insert("region".into(), "Print".into());
        with_shortcuts.save_to(&path).expect("write a shortcut");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            text.contains("region = \"Print\""),
            "a chosen chord is written: {text}"
        );
        assert!(
            Config::load_from(&path)
                .expect("reload")
                .shortcuts
                .0
                .contains_key("region"),
            "and loads back"
        );

        let cleared = Config::default();
        cleared.save_to(&path).expect("rewrite with defaults");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !text.contains("backend"),
            "a cleared key is removed: {text}"
        );
        assert!(
            !text.contains("language") && !text.contains("lang"),
            "a cleared language is removed: {text}"
        );
        assert!(
            !text.contains("shortcuts"),
            "a cleared shortcut table is removed: {text}"
        );
        assert!(text.contains("fps = 120"), "foreign values still survive");

        std::fs::remove_dir_all(&dir).ok();
    }
}
