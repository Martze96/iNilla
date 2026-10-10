//! iNilla's files: four TOML documents in `benilla-config/inilla/`, beside benilla's own state
//! (so on iOS they sit in Documents, visible and editable in the Files app, and survive updates
//! and SideStore re-signs with everything else).
//!
//! | file | holds |
//! |---|---|
//! | `settings.toml` | [`InillaSettings`] — quality, UI scale, audio session, debug switches |
//! | `servers.toml` | [`Launcher`] — server profiles and accounts (never passwords) |
//! | `touch.toml` | [`TouchLayout`] — the HUD as the player arranged it |
//! | `controller.toml` | [`PadProfile`] — the controller mapping |
//!
//! A file that does not parse is kept aside as `<name>.broken` and the defaults are used, so a
//! hand edit gone wrong costs the edit, not the app.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

use inilla_core::pad::PadProfile;
use inilla_core::profiles::Launcher;
use inilla_core::settings::InillaSettings;
use inilla_core::touch::TouchLayout;

pub(crate) fn dir() -> Option<PathBuf> {
    crate::local_state::home().map(|h| h.join("inilla"))
}

fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    let Some(path) = dir().map(|d| d.join(name)) else {
        return T::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            bevy::log::warn!(
                "inilla: {} does not parse ({e}); using defaults",
                path.display()
            );
            let _ = std::fs::rename(&path, path.with_extension("toml.broken"));
            T::default()
        }
    }
}

fn save<T: Serialize>(name: &str, value: &T) {
    let Some(dir) = dir() else {
        return;
    };
    let text = match toml::to_string_pretty(value) {
        Ok(t) => t,
        Err(e) => {
            bevy::log::warn!("inilla: could not serialize {name}: {e}");
            return;
        }
    };
    if let Err(e) = crate::local_state::write_atomic(&dir.join(name), &text) {
        bevy::log::warn!("inilla: could not save {name}: {e}");
    }
}

pub(crate) fn load_settings() -> InillaSettings {
    load("settings.toml")
}

pub(crate) fn save_settings(v: &InillaSettings) {
    save("settings.toml", v);
}

pub(crate) fn load_launcher() -> Launcher {
    load("servers.toml")
}

pub(crate) fn save_launcher(v: &Launcher) {
    save("servers.toml", v);
}

pub(crate) fn load_layout() -> TouchLayout {
    let mut l: TouchLayout = load("touch.toml");
    l.upgrade();
    l
}

pub(crate) fn save_layout(v: &TouchLayout) {
    save("touch.toml", v);
}

pub(crate) fn load_pad() -> PadProfile {
    load("controller.toml")
}

pub(crate) fn save_pad(v: &PadProfile) {
    save("controller.toml", v);
}
