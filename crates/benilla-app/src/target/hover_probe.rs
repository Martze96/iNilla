//! **The headless hover probe** (decision 2250) — aim the mouseover pick from a screen point when
//! the window has no OS cursor of its own, and say what the pick found.
//!
//! ## Why this exists
//!
//! An automated run **cannot hover anything**. The rig's probe window (640×360, corner-parked,
//! always-on-top) never receives the OS cursor: `Window::cursor_position()` stays `None` through a
//! `CGWarpMouseCursorPosition` *and* through HID-level `mouseMoved` events posted into its content
//! rect, so [`super::hover::update_hovered_object`] returns before the pick on every frame. The
//! consequence is not subtle — every report of the shape *"this object shows no tooltip"* has had to
//! be settled by the director hovering it and reading the card back to us, because the pick was
//! reachable only by a person with a mouse (2248 recorded that gap; this closes it).
//!
//! ## What it does
//!
//! `WOW_HOVER_PROBE` names where to aim, in the window's own cursor space (logical px, y-down):
//!
//! | value | aim |
//! |---|---|
//! | `centre` / `center` | the window's middle |
//! | `<x>,<y>` | that point |
//! | `sweep` | a 7×5 grid across the middle half of the window, one point per frame, cycling |
//!
//! `sweep` is the one that makes a rig run useful without a camera: standing a body in front of a
//! known object and asking *"what is anywhere near the middle of the screen"* answers the question
//! a fixed point can only answer if the aim was already right. It is a probe, not a camera search —
//! the grid is fixed, bounded and the same every run.
//!
//! **Armed only when the window reports no cursor**, so an attended run is never overridden: a
//! person's pointer always wins, and leaving the variable set costs a player nothing.

use bevy::prelude::*;
use bevy::window::Window;

/// The aim, parsed once per process.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Aim {
    Centre,
    At(i32, i32),
    Sweep,
}

fn aim() -> Option<Aim> {
    static AIM: std::sync::OnceLock<Option<Aim>> = std::sync::OnceLock::new();
    *AIM.get_or_init(|| {
        let raw = std::env::var("WOW_HOVER_PROBE").ok()?;
        let v = raw.trim();
        match v {
            "centre" | "center" => Some(Aim::Centre),
            "sweep" => Some(Aim::Sweep),
            _ => match v.split_once(',') {
                Some((x, y)) => match (x.trim().parse(), y.trim().parse()) {
                    (Ok(x), Ok(y)) => Some(Aim::At(x, y)),
                    _ => {
                        warn!("hover probe: WOW_HOVER_PROBE={v:?} is not `centre`, `sweep` or `x,y`");
                        None
                    }
                },
                None => {
                    warn!("hover probe: WOW_HOVER_PROBE={v:?} is not `centre`, `sweep` or `x,y`");
                    None
                }
            },
        }
    })
}

/// Is the probe armed at all? (Cheap enough to ask per frame — one `OnceLock` read.)
pub(super) fn armed() -> bool {
    aim().is_some()
}

/// The point to pick from, given the window and a frame counter — `None` when the probe is not
/// armed. The caller uses it **only** where the real cursor is absent.
pub(super) fn point(window: &Window, frame: u64) -> Option<Vec2> {
    let (w, h) = (window.width(), window.height());
    match aim()? {
        Aim::Centre => Some(Vec2::new(w / 2.0, h / 2.0)),
        Aim::At(x, y) => Some(Vec2::new(x as f32, y as f32)),
        // The grid: 7 columns × 5 rows over the middle half, so the edges of the frame (chrome,
        // sky, the player's own back) are never the answer. One cell per frame, cycling — at frame
        // rate the whole grid is covered ~2× a second.
        Aim::Sweep => {
            const COLS: u64 = 7;
            const ROWS: u64 = 5;
            let cell = frame % (COLS * ROWS);
            let (cx, cy) = (cell % COLS, cell / COLS);
            let fx = 0.25 + 0.5 * (cx as f32 / (COLS - 1) as f32);
            let fy = 0.25 + 0.5 * (cy as f32 / (ROWS - 1) as f32);
            Some(Vec2::new(w * fx, h * fy))
        }
    }
}

/// One line per *change*, so a stationary probe does not flood the log: what the pick found at the
/// aim point, and every term of the GameObject tooltip ladder the card shows a person (2248) —
/// eligibility, the published mouseover, and the ask-once template the plate needs.
#[derive(Default)]
pub(super) struct ProbeReport {
    last: Option<String>,
}

impl ProbeReport {
    pub(super) fn say(&mut self, line: String) {
        if self.last.as_deref() == Some(line.as_str()) {
            return;
        }
        info!("hover probe: {line}");
        self.last = Some(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid stays inside the middle half on both axes — the property that keeps a sweep off the
    /// frame's edges, where the answer is always sky or the player's own back.
    #[test]
    fn the_sweep_grid_stays_in_the_middle_half() {
        let w = 800.0_f32;
        let h = 600.0_f32;
        for cell in 0..35_u64 {
            let (cx, cy) = (cell % 7, cell / 7);
            let fx = 0.25 + 0.5 * (cx as f32 / 6.0);
            let fy = 0.25 + 0.5 * (cy as f32 / 4.0);
            let (x, y) = (w * fx, h * fy);
            assert!((w * 0.25..=w * 0.75).contains(&x), "col {cx} at {x}");
            assert!((h * 0.25..=h * 0.75).contains(&y), "row {cy} at {y}");
        }
    }

    /// A repeated line is said once — the property that makes the probe readable in a log rather
    /// than 60 identical lines a second.
    #[test]
    fn the_report_only_speaks_on_change() {
        let mut r = ProbeReport::default();
        r.say("a".into());
        assert_eq!(r.last.as_deref(), Some("a"));
        r.say("a".into());
        assert_eq!(r.last.as_deref(), Some("a"));
        r.say("b".into());
        assert_eq!(r.last.as_deref(), Some("b"));
    }
}
