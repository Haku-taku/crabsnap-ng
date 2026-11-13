//! The faces this app ships.
//!
//! The engine names an icon family but carries no font, and Material Symbols is a system
//! font on a Linux desktop and nothing at all on Windows, so the bytes travel with the app.

use std::sync::{Arc, OnceLock};

use ui_platform::host::FontFace;

/// The Material Symbols variable font, Apache 2.0, from `google/material-design-icons`; see
/// `ui/crates/assets/fonts/README.md`.
static ICON_FONT: &[u8] = include_bytes!("../../../ui/crates/assets/fonts/MaterialSymbolsOutlined.ttf");

/// The icon face every surface draws its icons with: `IconSet::Outlined`'s own family name,
/// so a control that names an icon by name resolves here and not to the system's fallback.
pub(crate) fn icon_face() -> FontFace {
    // Parsed once per process: the bytes are ten megabytes and every surface wants the same.
    static DATA: OnceLock<Arc<[u8]>> = OnceLock::new();
    let data = Arc::clone(DATA.get_or_init(|| Arc::from(ICON_FONT)));
    FontFace {
        name: ui_widgets::IconSet::Outlined.family().to_string(),
        data,
    }
}
