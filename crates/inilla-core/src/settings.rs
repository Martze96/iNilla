//! **iNilla's own settings** — the ones a phone needs and the 1.12 options window has no row for.
//! Game settings (graphics details, sound volumes, key bindings) stay benilla's CVars, edited in
//! the stock options windows; this file only holds what sits around them.

use serde::{Deserialize, Serialize};

/// A graphics preset; `Custom` leaves the CVars as the player set them in the stock options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Quality {
    /// Battery first: shorter view distance, 30 fps.
    Saver,
    /// The phone default: moderate view distance, 60 fps.
    #[default]
    Balanced,
    /// Pro-class phones on a charger: far view, the display's full refresh rate.
    High,
    Custom,
}

impl Quality {
    pub const ALL: [Quality; 4] = [
        Quality::Saver,
        Quality::Balanced,
        Quality::High,
        Quality::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Saver => "Battery saver",
            Quality::Balanced => "Balanced",
            Quality::High => "High",
            Quality::Custom => "Custom",
        }
    }

    /// Frame cap for the preset (`0` = the display's refresh rate).
    pub fn fps_cap(self) -> u32 {
        match self {
            Quality::Saver => 30,
            Quality::Balanced => 60,
            Quality::High | Quality::Custom => 0,
        }
    }

    /// The CVars the preset writes: `(name, value)` in 1.12 spelling.
    pub fn cvars(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Quality::Saver => &[("farclip", "177"), ("groundEffectDensity", "16")],
            Quality::Balanced => &[("farclip", "330"), ("groundEffectDensity", "32")],
            Quality::High => &[("farclip", "777"), ("groundEffectDensity", "64")],
            Quality::Custom => &[],
        }
    }
}

/// Developer switches — off in a player's hands, one tap away when something breaks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugSettings {
    /// Serve the remote console on the local network.
    pub remote_console: bool,
    pub console_port: u16,
    /// Required on every console request; generated on first enable.
    pub console_token: String,
    /// Keep a log file per session in `benilla-config/Diagnostics/`.
    pub log_to_file: bool,
    /// Show the frame-time / memory overlay.
    pub perf_overlay: bool,
    /// Draw the touch zones (stick zone, hit rects) over the HUD.
    pub show_touch_zones: bool,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            remote_console: false,
            console_port: 7878,
            console_token: String::new(),
            log_to_file: true,
            perf_overlay: false,
            show_touch_zones: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InillaSettings {
    pub quality: Quality,
    /// Render resolution as a fraction of the display (`0.5..=1.0`).
    pub render_scale: f32,
    /// The WoW UI scale. The stock slider stops at 1.0, which on a phone makes a 36-unit action
    /// button ~20 points tall — below a fingertip — so iNilla goes up to 1.5 (benilla's `uiScale`
    /// dial clamps there). `0` = automatic (see [`InillaSettings::effective_ui_scale`]).
    pub ui_scale: f32,
    /// Hide the decorative gryphons at the ends of the main action bar (frees the corners the
    /// touch HUD uses).
    pub compact_stock_ui: bool,
    /// Keep the stock windows clear of the Dynamic Island (or the notch): the interface stops at
    /// the safe area on the camera's side, and the 3D world still runs edge to edge.
    pub island_margin: bool,
    /// Keep the screen awake while in the world.
    pub keep_screen_on: bool,
    /// Let other apps' audio (music, a call) keep playing under the game.
    pub mix_with_other_audio: bool,
    /// Play sound even with the silent switch on.
    pub ignore_silent_switch: bool,
    pub haptics: bool,
    /// Show the on-screen keyboard as soon as a chat or login box takes focus.
    pub auto_keyboard: bool,
    /// Hide the touch HUD while a controller is being used; the next touch brings it back.
    pub hide_touch_with_controller: bool,
    /// While a controller is in use, the stock action buttons show the controller button that
    /// presses them (`X`, `LT A`) where the keyboard key would be.
    pub controller_hotkeys: bool,
    /// The controller bar: the held layer's eight buttons along the bottom of the screen. Off by
    /// default — the hotkey hints already say what presses what, and holding a trigger lights up
    /// the buttons its layer presses.
    pub controller_bar: bool,
    /// The bars the trigger layers press were switched on once, the first time a controller was
    /// used. Not a switch: the stock options own the bars after that.
    pub controller_bars_shown: bool,
    pub debug: DebugSettings,
    /// The first-run setup (data, server) has been completed once.
    pub setup_done: bool,
}

impl Default for InillaSettings {
    fn default() -> Self {
        Self {
            quality: Quality::default(),
            render_scale: 0.8,
            ui_scale: 0.0,
            compact_stock_ui: true,
            island_margin: true,
            keep_screen_on: true,
            mix_with_other_audio: false,
            ignore_silent_switch: false,
            haptics: true,
            auto_keyboard: true,
            hide_touch_with_controller: true,
            controller_hotkeys: true,
            controller_bar: false,
            controller_bars_shown: false,
            debug: DebugSettings::default(),
            setup_done: false,
        }
    }
}

impl InillaSettings {
    /// The UI scale to apply for a screen `height` points tall: the player's value, or the
    /// automatic one — a 36-unit stock action button at ~30 points, capped at 1.25. The stock
    /// panels hang 104 units under the top of the screen and are 512 tall; the old cap, 1.45,
    /// leaves 530 units of height, and a quest giver's or a vendor's buttons were out of reach on
    /// an iPhone. At 1.25 (614 units) they are reachable (device test).
    pub fn effective_ui_scale(&self, height: f32) -> f32 {
        if self.ui_scale > 0.0 {
            return self.ui_scale.clamp(0.5, 1.5);
        }
        if height <= 0.0 {
            return 1.0;
        }
        // One UI unit is `height / (768 / s)` points; solve 36 units = 30 points for s.
        let want = 30.0 / 36.0 * 768.0 / height;
        want.clamp(0.64, 1.25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_ui_scale_grows_on_a_phone_and_stays_stock_on_a_monitor() {
        let s = InillaSettings::default();
        // Every iPhone in landscape (375–440 points tall) gets the cap.
        assert_eq!(s.effective_ui_scale(430.0), 1.25);
        assert_eq!(s.effective_ui_scale(375.0), 1.25);
        let monitor = s.effective_ui_scale(1080.0);
        assert!(monitor < 0.7, "{monitor}");
        let pinned = InillaSettings {
            ui_scale: 1.2,
            ..Default::default()
        };
        assert_eq!(pinned.effective_ui_scale(430.0), 1.2);
    }

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let s = InillaSettings::default();
        let back: InillaSettings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
        let partial: InillaSettings = toml::from_str("render_scale = 0.5").unwrap();
        assert_eq!(partial.render_scale, 0.5);
        assert!(partial.keep_screen_on);
    }
}
