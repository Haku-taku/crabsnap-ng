//! Shortcuts: what can be bound, the chords the file chooses, and how they reach a surface.

use crabsnap_core::config::Config;
use crabsnap_core::hotkeys::{Place, Shortcut};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::Modifiers;
use ui_platform::hotkeys::parse_chords;

/// The id a press arrives under. One per action, whatever surface bound it: each surface
/// knows what cancelling means to it.
pub const REGION: ShortcutId = ShortcutId("crabsnap.region");
pub const OUTPUT: ShortcutId = ShortcutId("crabsnap.output");
pub const CANCEL: ShortcutId = ShortcutId("crabsnap.cancel");
pub const UNDO: ShortcutId = ShortcutId("crabsnap.undo");
pub const REDO: ShortcutId = ShortcutId("crabsnap.redo");
pub const SAVE: ShortcutId = ShortcutId("crabsnap.save");
pub const COPY: ShortcutId = ShortcutId("crabsnap.copy");
pub const PIN: ShortcutId = ShortcutId("crabsnap.pin");
pub const CURSOR: ShortcutId = ShortcutId("crabsnap.cursor");
pub const CLEAR: ShortcutId = ShortcutId("crabsnap.clear");
pub const CONFIRM: ShortcutId = ShortcutId("crabsnap.confirm");
pub const CURSOR_LEFT: ShortcutId = ShortcutId("crabsnap.cursor-left");
pub const CURSOR_RIGHT: ShortcutId = ShortcutId("crabsnap.cursor-right");
pub const CURSOR_UP: ShortcutId = ShortcutId("crabsnap.cursor-up");
pub const CURSOR_DOWN: ShortcutId = ShortcutId("crabsnap.cursor-down");

/// The id an action's presses arrive under.
pub fn id(action: Shortcut) -> ShortcutId {
    match action {
        Shortcut::Region => REGION,
        Shortcut::Output => OUTPUT,
        Shortcut::Cancel => CANCEL,
        Shortcut::Undo => UNDO,
        Shortcut::Redo => REDO,
        Shortcut::Save => SAVE,
        Shortcut::Copy => COPY,
        Shortcut::Pin => PIN,
        Shortcut::Cursor => CURSOR,
        Shortcut::Clear => CLEAR,
        Shortcut::Confirm => CONFIRM,
        Shortcut::CursorLeft => CURSOR_LEFT,
        Shortcut::CursorRight => CURSOR_RIGHT,
        Shortcut::CursorUp => CURSOR_UP,
        Shortcut::CursorDown => CURSOR_DOWN,
    }
}

/// `Shift` alone, the modifier a direction reads for its coarse step.
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    logo: false,
};

/// One action and the chords the file or the defaults settled on.
struct Resolved {
    action: Shortcut,
    chords: Vec<KeyChord>,
    /// The spellings they were parsed from, in the same order, for menus and reports.
    texts: Vec<String>,
    /// Whether the file chose them, rather than the built-in default.
    configured: bool,
}

/// What the configuration resolves to.
#[derive(Default)]
pub struct Plan {
    resolved: Vec<Resolved>,
    /// Lines the user should see: a chord that does not parse, an action that does not exist.
    pub problems: Vec<String>,
}

impl Plan {
    pub fn resolve(config: &Config) -> Self {
        let mut plan = Plan::default();
        for action in Shortcut::ALL {
            let (texts, configured): (Vec<String>, bool) =
                match config.shortcuts.0.get(action.name()) {
                // A missing key means the built-in default; an empty value means the user
                // cleared the action, which is a decision and not a mistake.
                None => (
                    action
                        .default_chords()
                        .iter()
                        .map(|chord| chord.to_string())
                        .collect(),
                    false,
                ),
                Some(text) if text.trim().is_empty() => continue,
                Some(text) => (
                    text.split(',')
                        .map(|chord| chord.trim().to_string())
                        .collect(),
                    true,
                ),
            };
            match parse_chords(&texts.join(", ")) {
                Ok(chords) => {
                    // A system-wide binding is registered as one literal trigger, so it
                    // cannot say "with or without": the chord is refused rather than bound
                    // to a spelling the file never named.
                    if action.places().contains(&Place::Global)
                        && chords.iter().any(|chord| !chord.any.is_empty())
                    {
                        plan.problems.push(format!(
                            "shortcuts.{}: an optional modifier cannot be a global hotkey",
                            action.name()
                        ));
                        continue;
                    }
                    plan.resolved.push(Resolved {
                        action,
                        chords,
                        texts,
                        configured,
                    });
                }
                Err(error) => plan
                    .problems
                    .push(format!("shortcuts.{}: {error}", action.name())),
            }
        }
        for name in config.shortcuts.0.keys() {
            if Shortcut::from_name(name).is_none() {
                plan.problems
                    .push(format!("shortcuts.{name}: no such action"));
            }
        }
        plan
    }

    fn get(&self, action: Shortcut) -> Option<&Resolved> {
        self.resolved.iter().find(|one| one.action == action)
    }

    /// Every chord a surface in `place` answers, ready for the surface's key table.
    ///
    /// An action bound in two places contributes to both: one spelling, one meaning.
    pub fn chords(&self, place: Place) -> Vec<(KeyChord, ShortcutId)> {
        let mut table = Vec::new();
        for one in self
            .resolved
            .iter()
            .filter(|one| one.action.places().contains(&place))
        {
            for &chord in &one.chords {
                // A direction is the same action whether or not `Shift` is held: that is
                // its coarse step, so the modifier is left free rather than bound a second
                // time, and the press says which one it got.
                let entry = if one.action.steps_cursor() && !chord.mods.shift {
                    chord.allowing(SHIFT)
                } else {
                    chord
                };
                table.push((entry, id(one.action)));
            }
        }
        table
    }

    /// The chords the OS registers, with the action each one runs, and the spelling to
    /// report: a bind failure has to name both the action and the chord the file asked for.
    pub fn global_entries(&self) -> Vec<(Shortcut, KeyChord, String)> {
        self.resolved
            .iter()
            .filter(|one| one.action.places().contains(&Place::Global))
            .flat_map(|one| {
                one.chords
                    .iter()
                    .zip(&one.texts)
                    .map(|(&chord, text)| (one.action, chord, text.clone()))
            })
            .collect()
    }

    /// The chord a tooltip or a menu names for `action`: the first one, spelled as the file
    /// spells it. `None` when the action is cleared.
    pub fn hint(&self, action: Shortcut) -> Option<String> {
        self.get(action)
            .and_then(|one| one.texts.first().cloned())
    }

    /// The chord `hint` names, for a row that must run the key it prints.
    pub fn chord(&self, action: Shortcut) -> Option<KeyChord> {
        self.get(action).and_then(|one| one.chords.first().copied())
    }

    /// The line `info config` prints for one action.
    pub fn describe(&self, action: Shortcut) -> String {
        match self.get(action) {
            Some(one) => {
                let text = one.texts.join(", ");
                if one.configured {
                    text
                } else {
                    format!("{text} (default)")
                }
            }
            None => "cleared".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(entries: &[(&str, &str)]) -> Config {
        let mut config = Config::default();
        for (name, text) in entries {
            config
                .shortcuts
                .0
                .insert((*name).to_string(), (*text).to_string());
        }
        config
    }

    /// A missing action takes its default, a written one wins, an empty one is cleared, and a
    /// name this build does not know is named rather than silently dropped.
    #[test]
    fn the_file_overrides_the_defaults_and_names_what_it_cannot_use() {
        let plain = Plan::resolve(&Config::default());
        for action in Shortcut::ALL
            .iter()
            .filter(|action| action.places().contains(&Place::Overlay))
        {
            for spelling in action.default_chords() {
                let chord = parse_chords(spelling).expect("one chord").remove(0);
                // Through the resolver, not equality: a direction's entry leaves `Shift`
                // free, so it is the same press without being the same value.
                assert_eq!(
                    ui_core::shortcut::best_match(
                        &plain.chords(Place::Overlay),
                        chord.key,
                        chord.mods
                    ),
                    Some(id(*action)),
                    "{}: {spelling} is bound on the surface that names it",
                    action.name()
                );
            }
        }
        assert_eq!(
            plain.describe(Shortcut::Region),
            "Ctrl+Shift+4 (default)",
            "a default is reported as such"
        );
        assert!(plain.problems.is_empty());

        let chosen = Plan::resolve(&config(&[("region", "Print")]));
        assert_eq!(chosen.hint(Shortcut::Region).as_deref(), Some("Print"));
        assert_eq!(chosen.describe(Shortcut::Region), "Print");

        let cleared = Plan::resolve(&config(&[("region", "")]));
        assert!(cleared.hint(Shortcut::Region).is_none());
        assert_eq!(cleared.describe(Shortcut::Region), "cleared");
        assert!(
            !cleared
                .chords(Place::Global)
                .contains(&(parse_chords("Print").expect("one chord").remove(0), REGION)),
            "a cleared action binds no chord"
        );

        let broken = Plan::resolve(&config(&[("region", "Hyper+X"), ("annotate", "F2")]));
        assert_eq!(
            broken.problems,
            [
                "shortcuts.region: unknown modifier hyper",
                "shortcuts.annotate: no such action"
            ]
        );
        assert!(
            broken.chords(Place::Global).iter().all(|(_, id)| *id != REGION),
            "a broken chord binds nothing"
        );
    }

    /// A direction is one entry whatever `Shift` does, and that entry — not a copy per
    /// surface — answers on every surface that names the action. The modifier left free
    /// widens the chord to that modifier's own spellings and no others; every other action
    /// stays exact. Left exact, every spelling had to be bound beside the chord, and the
    /// plan's `Shift` variant was a second entry that could fall out of step with the first.
    #[test]
    fn a_direction_answers_its_spellings_on_every_place_that_names_it() {
        let plan = Plan::resolve(&Config::default());
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        for spelling in ["A", "Left"] {
            let chord = parse_chords(spelling).expect("one chord").remove(0);
            // Spelled out rather than through the production helper: the expectation must
            // not move with the code it checks.
            let free = KeyChord { any: shift, ..chord };
            for place in [Place::Overlay, Place::Pin] {
                let table = plan.chords(place);
                assert!(
                    table.contains(&(free, CURSOR_LEFT)),
                    "{place:?} answers {spelling} with Shift left free"
                );
                let answered = |mods| ui_core::shortcut::best_match(&table, chord.key, mods);
                assert_eq!(answered(Modifiers::NONE), Some(CURSOR_LEFT));
                assert_eq!(answered(shift), Some(CURSOR_LEFT));
                assert_ne!(
                    answered(ctrl),
                    Some(CURSOR_LEFT),
                    "a free Shift frees nothing else"
                );
            }
        }
        // Only a direction reads a modifier for its meaning; every other action stays exact.
        let save = plan.chord(Shortcut::Save).expect("save has a default");
        assert!(
            !save.matches(save.key, shift.union(ctrl)),
            "save does not answer Ctrl+Shift"
        );
        let cancel: Vec<_> = [Place::Overlay, Place::Pin]
            .into_iter()
            .filter(|place| plan.chords(*place).iter().any(|(_, id)| *id == CANCEL))
            .collect();
        assert_eq!(cancel.len(), 2, "cancel is bound on both surfaces");
    }
}
