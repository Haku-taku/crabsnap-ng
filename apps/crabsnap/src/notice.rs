//! The line a surface shows after an action that changed nothing on screen.
//!
//! The bar is the MD3 snackbar, and its life is one engine animation: a pulse lands it at
//! full, the departure holds and lets go, and the control's own fade does the rest. It
//! arrives on a spring, so it comes up from below the surface's edge and settles with one
//! bounce. No surface reads the clock or keeps a timer of its own.

use std::path::Path;
use std::sync::Arc;

use ui_core::anim::{AnimSpec, Easing};
use ui_core::Id;
use ui_i18n::Catalog;
use ui_platform::element::Element;
use ui_widgets::Cx;
use ui_widgets_md3::{Md3Kit, SnackbarParams};

/// How long the bar holds before it leaves. MD3 allows four seconds; an answer this small
/// has been read long before that, and the bar is in the way of the picture.
const HOLD: f32 = 2.0;
/// A landing: the bar is simply up, as the zoom chip is.
const LAND: AnimSpec = AnimSpec::new(0.0, Easing::Linear);
/// The departure is the clock: the value holds, then falls, and the bar fades with it.
const DEPART: AnimSpec = AnimSpec::new(0.0, Easing::Linear).delayed(HOLD);
/// The arrival spring: underdamped, so the bar overshoots its rest place once and settles.
const ARRIVAL_STIFFNESS: f32 = 420.0;
const ARRIVAL_DAMPING: f32 = 22.0;
/// The bar's ceiling, and its inset from the surface's edge.
const WIDTH: f32 = 344.0;
const MARGIN: f32 = 24.0;

/// Which action a failure names; the bar reuses the action's own label.
#[derive(Clone, Copy)]
pub(crate) enum Action {
    Copy,
    Save,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Copy => "action-copy",
            Action::Save => "action-save",
        }
    }
}

/// The bar's line after a copy went to the clipboard.
pub(crate) fn copied(catalog: &Catalog) -> Arc<str> {
    catalog.text_by_id("notice-copied")
}

/// The bar's line after a file was written; it names the file, not the whole path.
pub(crate) fn saved(catalog: &Catalog, path: &Path) -> Arc<str> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let mut args = ui_i18n::Args::new();
    args.set("file", name);
    catalog.text_by_id_with("notice-saved", &args)
}

/// The bar's line after an action failed; the reason stays on the terminal.
pub(crate) fn failed(catalog: &Catalog, action: Action) -> Arc<str> {
    let mut args = ui_i18n::Args::new();
    args.set("action", catalog.text_by_id(action.label()).to_string());
    catalog.text_by_id_with("notice-failed", &args)
}

/// The bar a surface shows after an action whose only evidence is the bar itself.
pub(crate) struct Notice {
    /// The line on the bar; the last one stays while the departure runs.
    line: Option<Arc<str>>,
    /// Whether this build starts a showing.
    pulse: bool,
}

impl Notice {
    pub(crate) fn new() -> Self {
        Self {
            line: None,
            pulse: false,
        }
    }

    /// Put `line` on the bar, replacing whatever it said.
    pub(crate) fn show(&mut self, line: Arc<str>) {
        self.line = Some(line);
        self.pulse = true;
    }

    /// Build the bar across `surface_width` points; it paints nothing while it is down.
    pub(crate) fn build(
        &mut self,
        cx: &mut Cx<'_, '_>,
        kit: &Md3Kit,
        surface_width: f32,
    ) -> Box<dyn Element> {
        let pulse = std::mem::take(&mut self.pulse);
        let up = self.up(cx, pulse);
        let reveal = self.reveal(cx, pulse, up);
        let bar = kit.snackbar(
            SnackbarParams {
                id: Id::str("notice/bar"),
                message: self.line.as_deref().unwrap_or_default(),
                action: None,
                two_lines: false,
                floating: true,
                width: (surface_width - MARGIN * 2.0).min(WIDTH),
                visible: up,
                overlay: Some(MARGIN),
                reveal,
            },
            cx,
        );
        cx.element(bar)
    }

    /// Whether the bar is up this build, driving its clock as a side effect.
    fn up(&mut self, cx: &mut Cx<'_, '_>, pulse: bool) -> bool {
        let bar = Id::str("notice/bar");
        let clock = bar.with("clock");
        let mut at = cx.at(bar);
        if pulse {
            at.animate(clock, 1.0, LAND);
        }
        at.animate(clock, 0.0, DEPART) > 0.5
    }

    /// How far in the bar has come, from the engine's spring; driving it is the side effect.
    fn reveal(&mut self, cx: &mut Cx<'_, '_>, pulse: bool, up: bool) -> f32 {
        let slide = Id::str("notice/bar").with("slide");
        let mut at = cx.at(Id::str("notice/bar"));
        if pulse {
            // A spring's first drive settles where it is sent, and this one has to start
            // below the edge: park it at the bottom, then let the next drive carry it in.
            at.forget_spring(slide);
            at.spring(slide, 0.0, ARRIVAL_STIFFNESS, ARRIVAL_DAMPING);
        }
        at.spring(
            slide,
            if up { 1.0 } else { 0.0 },
            ARRIVAL_STIFFNESS,
            ARRIVAL_DAMPING,
        )
        .x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::AnimStore;

    /// One build, as `view` runs it: `pulse` is the showing's own frame, `now` the clock's.
    fn frame(notice: &mut Notice, store: &mut AnimStore, now: f64, pulse: bool) -> (bool, f32) {
        let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        let mut cx = Cx::new(&mut core, ui_style::Env::root());
        let up = notice.up(&mut cx, pulse);
        (up, notice.reveal(&mut cx, pulse, up))
    }

    /// A showing is up at once, holds for the bar's whole life, and is down after it. The
    /// landing and the departure are two calls on one value, so a wrong order or a missing
    /// delay shows here rather than as a bar that never leaves or never appears.
    #[test]
    fn a_showing_holds_for_its_whole_life_and_then_leaves() {
        let mut store = AnimStore::new();
        let mut notice = Notice::new();
        notice.show("copied".into());
        assert!(
            frame(&mut notice, &mut store, 0.0, true).0,
            "a showing lands at full"
        );
        assert!(
            frame(&mut notice, &mut store, HOLD as f64 - 0.5, false).0,
            "the bar holds for its life"
        );
        assert!(
            !frame(&mut notice, &mut store, HOLD as f64 + 0.5, false).0,
            "the departure takes it away"
        );
    }

    /// The bar travels: it starts below the surface's edge and hangs past its rest place
    /// once before settling there. A tween in place would pass the hold and fail this.
    #[test]
    fn the_arrival_comes_up_from_below_and_bounces_once() {
        let mut store = AnimStore::new();
        let mut notice = Notice::new();
        notice.show("saved".into());
        let (_, start) = frame(&mut notice, &mut store, 0.0, true);
        assert!(start < 0.05, "the bar starts below the edge, got {start}");
        let mut peak = start;
        let mut now = 0.0;
        while now < HOLD as f64 - 0.5 {
            now += 1.0 / 60.0;
            peak = peak.max(frame(&mut notice, &mut store, now, false).1);
        }
        assert!(peak > 1.01, "the arrival overshoots its rest place, got {peak}");
        let (_, rest) = frame(&mut notice, &mut store, now, false);
        assert!((rest - 1.0).abs() < 0.001, "and comes to rest in it, got {rest}");
    }
}
