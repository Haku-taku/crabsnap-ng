//! CSS colour codes: what a palette pin reads when it is one colour, and the spellings
//! every palette pin shows for the colour it has selected.
//!
//! Only the forms a person types by hand are here — hex, `rgb()`, `hsl()`, with the
//! space-and-slash syntax CSS Color 4 also allows. `color()`, `lab()` and the rest name a
//! colour space this code has no conversion for, so they are refused by name rather than
//! guessed at.

/// One colour, 8 bits a channel: what the clipboard and the card spell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rgba {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
}

impl Rgba {
    pub(crate) const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// `#RRGGBB`, or `#RRGGBBAA` when the colour is not opaque.
    pub(crate) fn hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// `rgb(r, g, b)`, or `rgba(...)` with the alpha as a fraction.
    pub(crate) fn rgb(&self) -> String {
        if self.a == 255 {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({}, {}, {}, {})",
                self.r,
                self.g,
                self.b,
                fraction(self.a)
            )
        }
    }

    /// `hsl(h, s%, l%)`, or `hsla(...)` with the alpha as a fraction.
    pub(crate) fn hsl(&self) -> String {
        let (hue, saturation, lightness) = to_hsl(*self);
        if self.a == 255 {
            format!("hsl({hue}, {saturation}%, {lightness}%)")
        } else {
            format!(
                "hsla({hue}, {saturation}%, {lightness}%, {})",
                fraction(self.a)
            )
        }
    }
}

/// An alpha byte as CSS writes it: `0.5`, not `0.50` and not `128`.
fn fraction(alpha: u8) -> String {
    let value = f32::from(alpha) / 255.0;
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `hue` in degrees, `saturation` and `lightness` in whole percent.
fn to_hsl(colour: Rgba) -> (u32, u32, u32) {
    let r = f32::from(colour.r) / 255.0;
    let g = f32::from(colour.g) / 255.0;
    let b = f32::from(colour.b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) * 0.5;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0, 0, (lightness * 100.0).round() as u32);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (
        hue.round().rem_euclid(360.0) as u32,
        (saturation * 100.0).round().clamp(0.0, 100.0) as u32,
        (lightness * 100.0).round() as u32,
    )
}

/// Read one CSS colour code. The error names the forms this does accept.
pub(crate) fn parse(text: &str) -> Result<Rgba, String> {
    let text = text.trim();
    let refuse =
        || format!("{text:?} is not a CSS colour: use #RRGGBB, rgb(r, g, b) or hsl(h, s%, l%)");
    if let Some(hex) = text.strip_prefix('#') {
        return hex_value(hex).ok_or_else(refuse);
    }
    let lowered = text.to_ascii_lowercase();
    let body = |name: &str| {
        lowered
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('('))
            .and_then(|rest| rest.strip_suffix(')'))
            .map(|inner| inner.replace([',', '/'], " "))
    };
    if let Some(inner) = body("rgb").or_else(|| body("rgba")) {
        let parts: Vec<&str> = inner.split_whitespace().collect();
        let [red, green, blue, alpha @ ..] = parts.as_slice() else {
            return Err(refuse());
        };
        let channel = |part: &str| channel_part(part).ok_or_else(refuse);
        let alpha = match alpha {
            [] => Some(255),
            [only] => alpha_part(only),
            _ => None,
        }
        .ok_or_else(refuse)?;
        return Ok(Rgba {
            r: channel(red)?,
            g: channel(green)?,
            b: channel(blue)?,
            a: alpha,
        });
    }
    if let Some(inner) = body("hsl").or_else(|| body("hsla")) {
        let parts: Vec<&str> = inner.split_whitespace().collect();
        let [hue, saturation, lightness, alpha @ ..] = parts.as_slice() else {
            return Err(refuse());
        };
        let alpha = match alpha {
            [] => Some(255),
            [only] => alpha_part(only),
            _ => None,
        }
        .ok_or_else(refuse)?;
        let hue = hue
            .trim_end_matches("deg")
            .parse::<f32>()
            .map(|hue| hue.rem_euclid(360.0))
            .map_err(|_| refuse())?;
        let saturation = percent_part(saturation).ok_or_else(refuse)?;
        let lightness = percent_part(lightness).ok_or_else(refuse)?;
        return Ok(hsl_to_rgba(hue, saturation, lightness, alpha));
    }
    Err(refuse())
}

fn hex_value(text: &str) -> Option<Rgba> {
    if !text.is_ascii() {
        return None;
    }
    let wide = |at: usize| {
        let nibble = (text.as_bytes()[at] as char).to_digit(16)?;
        Some((nibble * 17) as u8)
    };
    let byte = |at: usize| u8::from_str_radix(text.get(at..at + 2)?, 16).ok();
    match text.len() {
        3 => Some(Rgba {
            r: wide(0)?,
            g: wide(1)?,
            b: wide(2)?,
            a: 255,
        }),
        4 => Some(Rgba {
            r: wide(0)?,
            g: wide(1)?,
            b: wide(2)?,
            a: wide(3)?,
        }),
        6 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: 255,
        }),
        8 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        }),
        _ => None,
    }
}

/// `255`, `100%` or `50%` as one channel of `rgb()`.
fn channel_part(text: &str) -> Option<u8> {
    match text.strip_suffix('%') {
        Some(percent) => Some((percent.parse::<f32>().ok()? / 100.0 * 255.0).round() as u8),
        None => text.parse::<u8>().ok(),
    }
}

/// `0.5` or `50%` as an alpha byte.
fn alpha_part(text: &str) -> Option<u8> {
    match text.strip_suffix('%') {
        Some(percent) => Some((percent.parse::<f32>().ok()? / 100.0 * 255.0).round() as u8),
        None => {
            let value = text.parse::<f32>().ok()?;
            Some((value * 255.0).round().clamp(0.0, 255.0) as u8)
        }
    }
}

/// `50%`, or a bare `50` read as a percentage: what a person writes by hand.
fn percent_part(text: &str) -> Option<f32> {
    let number = text.trim_end_matches('%').parse::<f32>().ok()?;
    Some((number / 100.0).clamp(0.0, 1.0))
}

fn hsl_to_rgba(hue: f32, saturation: f32, lightness: f32, alpha: u8) -> Rgba {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let x = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma * 0.5;
    let channel = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba {
        r: channel(r),
        g: channel(g),
        b: channel(b),
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hex and `rgb()` are exact; `hsl()` states whole degrees and percent, so it comes
    /// back within two steps per channel — the most that rounding can cost. A larger error
    /// would show a colour the code does not name, which is what these spells exist to avoid.
    #[test]
    fn a_colour_round_trips_through_every_spelling() {
        for code in ["#6750A4", "#FF0000", "#00FF00", "#0000FF", "#123456"] {
            let colour = parse(code).expect("the hex parses");
            assert_eq!(colour.hex(), code);
            assert_eq!(parse(&colour.rgb()).expect("its rgb parses"), colour);
            let back = parse(&colour.hsl()).expect("its hsl parses");
            let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
            assert!(
                near(back.r, colour.r) && near(back.g, colour.g) && near(back.b, colour.b),
                "{code} came back as {back:?} from {}",
                colour.hsl()
            );
        }
        assert_eq!(
            parse("#6750A4").expect("hex"),
            Rgba {
                r: 103,
                g: 80,
                b: 164,
                a: 255
            }
        );
    }

    #[test]
    fn the_css_spellings_people_write_all_parse() {
        let seed = Rgba {
            r: 103,
            g: 80,
            b: 164,
            a: 255,
        };
        for code in [
            "rgb(103, 80, 164)",
            "rgb(103 80 164)",
            "RGB(103,80,164)",
            " #6750a4 ",
            "rgb(40.4% 31.4% 64.3%)",
        ] {
            let parsed = parse(code).expect(code);
            let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
            assert!(
                near(parsed.r, seed.r) && near(parsed.g, seed.g) && near(parsed.b, seed.b),
                "{code} gave {parsed:?}"
            );
        }
        let red = Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        for code in ["hsl(0, 100%, 50%)", "hsl(0deg 100% 50%)", "hsl(360 100 50)"] {
            assert_eq!(parse(code).ok(), Some(red), "{code}");
        }
        assert_eq!(parse("#6750A480").expect("alpha hex").a, 0x80);
        assert_eq!(parse("rgba(103, 80, 164, 0.5)").expect("alpha").a, 128);
        assert!(
            parse("lab(50 20 30)").is_err(),
            "an unhandled colour space is refused"
        );
    }
}
