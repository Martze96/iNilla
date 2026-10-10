//! Account passwords and access tokens. On iOS they live in the **Keychain** (a generic password
//! item per key, service `inilla`): encrypted at rest, kept across app updates and SideStore
//! re-signs, never in a file the Files app can see.
//!
//! Off iOS there is no keychain wired up, so secrets fall back to one TOML file in a directory
//! the caller names ([`set_fallback_dir`]) — the desktop build is a development tool, and the
//! file says so in its header. Nothing is stored anywhere until a fallback dir is set.

use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(target_os = "ios")]
const SERVICE: &str = "inilla";

static FALLBACK: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Where the desktop fallback file lives (`<dir>/secrets.toml`). Ignored on iOS.
pub fn set_fallback_dir(dir: PathBuf) {
    if let Ok(mut g) = FALLBACK.lock() {
        *g = Some(dir);
    }
}

pub fn get(key: &str) -> Option<String> {
    #[cfg(target_os = "ios")]
    {
        security_framework::passwords::get_generic_password(SERVICE, key)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    }
    #[cfg(not(target_os = "ios"))]
    {
        file::load().remove(key)
    }
}

pub fn set(key: &str, secret: &str) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        security_framework::passwords::set_generic_password(SERVICE, key, secret.as_bytes())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "ios"))]
    {
        let mut all = file::load();
        all.insert(key.to_string(), secret.to_string());
        file::save(&all)
    }
}

pub fn delete(key: &str) {
    #[cfg(target_os = "ios")]
    {
        let _ = security_framework::passwords::delete_generic_password(SERVICE, key);
    }
    #[cfg(not(target_os = "ios"))]
    {
        let mut all = file::load();
        if all.remove(key).is_some() {
            let _ = file::save(&all);
        }
    }
}

#[cfg(not(target_os = "ios"))]
mod file {
    use std::collections::BTreeMap;

    fn path() -> Option<std::path::PathBuf> {
        super::FALLBACK
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .map(|d| d.join("secrets.toml"))
    }

    pub(super) fn load() -> BTreeMap<String, String> {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub(super) fn save(all: &BTreeMap<String, String>) -> Result<(), String> {
        let Some(p) = path() else {
            return Err("no secrets folder set".into());
        };
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = toml::to_string(all).map_err(|e| e.to_string())?;
        let text = format!(
            "# iNilla desktop build: saved passwords, in PLAIN TEXT.\n\
             # The iOS app keeps these in the Keychain; this file exists only off iOS.\n{body}"
        );
        std::fs::write(&p, text).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_round_trips() {
        let dir = std::env::temp_dir().join(format!("inilla-keychain-{}", std::process::id()));
        set_fallback_dir(dir.clone());
        set("account:a", "hunter2").unwrap();
        assert_eq!(get("account:a").as_deref(), Some("hunter2"));
        delete("account:a");
        assert_eq!(get("account:a"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
