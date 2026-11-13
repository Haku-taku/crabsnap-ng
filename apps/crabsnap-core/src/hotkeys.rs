//! The actions a chord can run, and the names `[shortcuts]` uses for them.
//!
//! The list lives beside the schema because three binaries read it: the daemon binds the
//! OS-wide ones, each surface binds its own key table, and the settings window edits all of
//! them. What an action *runs* belongs to the app.

/// Where an action's chords are bound.
///
/// One action may be bound in more than one place — cancelling means the same thing on the
/// overlay and on a pin — and a chord written once applies to every place it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// The compositor, through the desktop's global-hotkey service.
    Global,
    /// The region overlay's own key table.
    Overlay,
    /// A pin canvas's key table.
    Pin,
}

/// One bindable action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shortcut {
    /// Select an area and keep it.
    Region,
    /// Capture one output with no selection.
    Output,
    /// Take nothing: close the overlay, or the pin the pointer rests on.
    Cancel,
    /// Undo the last mark in the open annotation session.
    Undo,
    /// Redo the mark the session undid last.
    Redo,
    /// Write the selection to a file.
    Save,
    /// Put the selection on the clipboard.
    Copy,
    /// Pin the selection on the screen.
    Pin,
    /// Put the cursor tool back: nothing is armed, so a press selects instead of drawing.
    Cursor,
    /// Drop the mark selection, and the selection itself when no mark is selected.
    Clear,
    /// Keep the marks an annotation session drew.
    Confirm,
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
}

impl Shortcut {
    pub const ALL: [Self; 15] = [
        Self::Region,
        Self::Output,
        Self::Cancel,
        Self::Undo,
        Self::Redo,
        Self::Save,
        Self::Copy,
        Self::Pin,
        Self::Cursor,
        Self::Clear,
        Self::Confirm,
        Self::CursorLeft,
        Self::CursorRight,
        Self::CursorUp,
        Self::CursorDown,
    ];

    /// The key `[shortcuts]` uses.
    pub fn name(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Output => "output",
            Self::Cancel => "cancel",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Save => "save",
            Self::Copy => "copy",
            Self::Pin => "pin",
            Self::Cursor => "cursor",
            Self::Clear => "clear",
            Self::Confirm => "confirm",
            Self::CursorLeft => "cursor-left",
            Self::CursorRight => "cursor-right",
            Self::CursorUp => "cursor-up",
            Self::CursorDown => "cursor-down",
        }
    }

    /// The chords when the file says nothing about this action.
    ///
    /// A direction has two, so the hand can stay on the letters or on the arrows.
    pub fn default_chords(self) -> &'static [&'static str] {
        match self {
            Self::Region => &["Ctrl+Shift+4"],
            Self::Output => &["Ctrl+Shift+3"],
            Self::Cancel => &["Escape"],
            Self::Undo => &["Ctrl+Z"],
            Self::Redo => &["Ctrl+Shift+Z"],
            Self::Save => &["Ctrl+S"],
            Self::Copy => &["Ctrl+C"],
            Self::Pin => &["Ctrl+P"],
            // The letter the pointer tools use elsewhere; the arrows are cursor steps, so no
            // direction may take it.
            Self::Cursor => &["V"],
            Self::Clear => &["Ctrl+D"],
            Self::Confirm => &["Enter"],
            Self::CursorLeft => &["A", "Left"],
            Self::CursorRight => &["D", "Right"],
            Self::CursorUp => &["W", "Up"],
            Self::CursorDown => &["S", "Down"],
        }
    }

    /// Where the action's chords are bound.
    pub fn places(self) -> &'static [Place] {
        match self {
            Self::Region | Self::Output => &[Place::Global],
            Self::Cancel | Self::Undo | Self::Redo => &[Place::Overlay, Place::Pin],
            // Only a pin mounts an annotation bar with a confirm mark; the overlay's session
            // ends by pinning or copying, so Enter would have nothing to press there.
            Self::Confirm => &[Place::Pin],
            // A pin's menu names these chords on its own rows, so they must run with the menu
            // shut as well, on the pin the pointer rests on.
            Self::Save | Self::Copy => &[Place::Overlay, Place::Pin],
            Self::Pin => &[Place::Overlay],
            // Both surfaces own an annotation session, so both can put a mark or the
            // selection down and both can put the cursor tool back.
            Self::Clear | Self::Cursor => &[Place::Overlay, Place::Pin],
            // A pin canvas draws on its own picture grid, so the same keys place the pointer
            // there too, one of that picture's pixels a press.
            Self::CursorLeft | Self::CursorRight | Self::CursorUp | Self::CursorDown => {
                &[Place::Overlay, Place::Pin]
            }
        }
    }

    /// Whether the action steps the pointer, so the same chord with `Shift` asks for the
    /// coarse step and is bound beside it.
    pub fn steps_cursor(self) -> bool {
        matches!(
            self,
            Self::CursorLeft | Self::CursorRight | Self::CursorUp | Self::CursorDown
        )
    }

    /// The action a `[shortcuts]` key names, if this build has it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action is reachable by its own name and no two share one: the schema's keys are
    /// a namespace, and a collision would silently move an action's chord to another one.
    #[test]
    fn the_names_are_unique_and_round_trip() {
        let mut names: Vec<&str> = Shortcut::ALL.iter().map(|action| action.name()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "two actions share a name");
        for action in Shortcut::ALL {
            assert_eq!(Shortcut::from_name(action.name()), Some(action));
            assert!(
                !action.places().is_empty() && !action.default_chords().is_empty(),
                "{} names no place or no default",
                action.name()
            );
        }
    }
}
