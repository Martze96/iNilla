//! Where things live inside the iOS app container.
//!
//! - `Documents/` is what the Files app and Finder/iTunes file sharing show the player
//!   (`UIFileSharingEnabled`). benilla's state folder (`benilla-config/`: settings, key bindings,
//!   macros, saved variables, **AddOns**, screenshots, crash reports and session logs) goes here,
//!   so every one of those is reachable without a Mac — and it is also the drop box a player
//!   copies their `Data` folder into to import it.
//! - `Library/Application Support/iNilla/Data/` holds the game data once imported or downloaded:
//!   out of the player's way (a 5 GB folder in Files is one accidental delete from a re-download)
//!   and excluded from iCloud backup.
//!
//! Resolved from `$HOME`, which iOS points at the container root for the app (and LiveContainer
//! points at the guest app's container). Off iOS every function answers `None` unless
//! `$INILLA_CONTAINER` names a folder to play the container: the desktop build otherwise keeps
//! benilla's own resolution (`$BENILLA_HOME`, `$WOW_DATA`, beside the binary).

use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    if cfg!(target_os = "ios") {
        return std::env::var_os("HOME").map(PathBuf::from);
    }
    // Off iOS, `$INILLA_CONTAINER` stands in for the app container, so the desktop build can
    // rehearse the iOS layout (the Documents import, the data download) against a scratch folder.
    std::env::var_os("INILLA_CONTAINER")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether paths resolve into an (emulated or real) app container.
pub fn in_container() -> bool {
    home().is_some()
}

/// `Documents/` — visible in the Files app.
pub fn documents() -> Option<PathBuf> {
    home().map(|h| h.join("Documents"))
}

/// `Library/Application Support/iNilla/`.
pub fn app_support() -> Option<PathBuf> {
    home().map(|h| h.join("Library").join("Application Support").join("iNilla"))
}

/// `Library/Caches/iNilla/` — may be purged by the OS under storage pressure.
pub fn caches() -> Option<PathBuf> {
    home().map(|h| h.join("Library").join("Caches").join("iNilla"))
}

/// The game data folder (`WOW_DATA`).
pub fn data_dir() -> Option<PathBuf> {
    app_support().map(|d| d.join("Data"))
}

/// benilla's state folder (`BENILLA_HOME`).
pub fn config_dir() -> Option<PathBuf> {
    documents().map(|d| d.join("benilla-config"))
}

/// What runs iNilla: installed as its own app, or as a guest inside LiveContainer, which gives
/// each guest app a home folder inside its own container and its own `Info.plist`, entitlements
/// and Files-app entry in place of the guest's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    App,
    LiveContainer,
    Desktop,
}

pub fn host() -> Host {
    if !cfg!(target_os = "ios") {
        return Host::Desktop;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    classify_home(&home, std::env::var_os("LC_HOME_PATH").is_some())
}

/// LiveContainer points a guest's `$HOME` at `<its Documents>/Data/Application/<folder>` (or
/// the same under `LiveContainer/` in its app group) and keeps its own home in `$LC_HOME_PATH`;
/// an installed app's home is `…/Containers/Data/Application/<UUID>`, never under a Documents.
pub fn classify_home(home: &str, lc_env: bool) -> Host {
    if lc_env
        || home.contains("/Documents/Data/Application/")
        || home.contains("/LiveContainer/Data/Application/")
    {
        Host::LiveContainer
    } else {
        Host::App
    }
}

/// Where the Files app shows `Documents/`, as the taps to get there.
pub fn files_location() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    files_location_for(host(), &home)
}

pub fn files_location_for(host: Host, home: &str) -> String {
    match host {
        Host::LiveContainer if home.contains("/Documents/Data/Application/") => {
            let folder = home.trim_end_matches('/').rsplit('/').next().unwrap_or("");
            format!("On My iPhone → LiveContainer → Data → Application → {folder} → Documents")
        }
        // A shared (app group) guest's folder is not one the Files app lists.
        Host::LiveContainer => {
            "not shown in the Files app for a shared LiveContainer app — download the data \
             instead, or make iNilla a private app in LiveContainer"
                .to_string()
        }
        Host::App => "On My iPhone → iNilla".to_string(),
        Host::Desktop => match documents() {
            Some(d) => d.display().to_string(),
            None => "benilla-config beside the binary".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_app_is_not_a_guest() {
        let home =
            "/private/var/mobile/Containers/Data/Application/0B5D6C1E-7A0F-4B9E-9C84-1F2E3D4C5B6A";
        assert_eq!(classify_home(home, false), Host::App);
        assert_eq!(files_location_for(Host::App, home), "On My iPhone → iNilla");
    }

    #[test]
    fn a_livecontainer_guest_is_found_by_its_home() {
        let home = "/private/var/mobile/Containers/Data/Application/0B5D6C1E/Documents/Data/Application/iNilla-2F1A";
        assert_eq!(classify_home(home, false), Host::LiveContainer);
        assert_eq!(
            files_location_for(Host::LiveContainer, home),
            "On My iPhone → LiveContainer → Data → Application → iNilla-2F1A → Documents"
        );
        let shared = "/private/var/mobile/Containers/Shared/AppGroup/9C1D/LiveContainer/Data/Application/77AA";
        assert_eq!(classify_home(shared, false), Host::LiveContainer);
        assert!(files_location_for(Host::LiveContainer, shared).contains("download"));
        assert_eq!(classify_home("/anything", true), Host::LiveContainer);
    }
}
