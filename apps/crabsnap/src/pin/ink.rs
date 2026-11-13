//! The ink a text or palette pin paints with.
//!
//! A pin is a card on someone's desktop, not part of the app's chrome: it keeps one light
//! palette whatever the session's theme, so two pins of one file look the same everywhere.

use ui_core::geom::Color;

/// A card's colours.
pub(crate) struct Ink {
    pub(crate) surface: Color,
    pub(crate) outline: Color,
    pub(crate) body: Color,
    pub(crate) muted: Color,
    pub(crate) link: Color,
    pub(crate) code_fill: Color,
    pub(crate) code_ink: Color,
    pub(crate) selection: Color,
}

impl Ink {
    pub(crate) fn card() -> Self {
        Self {
            surface: Color::from_rgba8(252, 252, 253, 255),
            outline: Color::rgba(0.0, 0.0, 0.0, 0.12),
            body: Color::from_rgba8(32, 35, 42, 255),
            muted: Color::from_rgba8(110, 116, 130, 255),
            link: Color::from_rgba8(20, 90, 200, 255),
            code_fill: Color::rgba(0.0, 0.0, 0.0, 0.06),
            code_ink: Color::from_rgba8(60, 66, 78, 255),
            selection: Color {
                a: 0.3,
                ..Color::from_rgba8(20, 90, 200, 255)
            },
        }
    }

    /// One syntax colour: enough classes to tell a keyword from a string.
    pub(crate) fn token(&self, class: ui_highlight::TokenClass) -> Color {
        use ui_highlight::TokenClass::*;
        match class {
            Keyword => Color::from_rgba8(166, 38, 164, 255),
            Function => Color::from_rgba8(64, 120, 242, 255),
            Type | Attribute | Label => Color::from_rgba8(193, 132, 1, 255),
            String => Color::from_rgba8(80, 161, 79, 255),
            Number | Constant => Color::from_rgba8(152, 104, 1, 255),
            Property => Color::from_rgba8(228, 86, 73, 255),
            Comment => Color::from_rgba8(160, 161, 167, 255),
            Operator | Punctuation | Variable => self.code_ink,
        }
    }
}
