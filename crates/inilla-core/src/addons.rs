//! **What a data host offers besides the archives**: the languages it has game data for, and the
//! addons that go with each (an iNilla server bundles pfQuest). The app installs a host's addons
//! into benilla's `AddOns` folder and keeps them current, and never touches an addon the player
//! put there themselves.
//!
//! ```text
//! GET <host>/locales.json              {"default":"enUS","locales":[{"locale":"deDE","path":"deDE/","bytes":…}]}
//! GET <base>/addons.json               {"format":1,"locale":"deDE","addons":[{"name","version","files":[{"path","size","sha256"}]}]}
//! GET <base>/addons/<path>             one file, e.g. addons/pfQuest/pfQuest.toc
//! ```
//!
//! `<base>` is the host for its default language and `<host>/<path>` for another, so the archives
//! and the addons of one language always come from the same place.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data::{join_url, sha256_file, Fetch, Progress};

/// One language a host has game data for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostLocale {
    /// `enUS`, `deDE`, …
    pub locale: String,
    /// Where that language's manifest, files and addons live, relative to the host (`deDE/`).
    pub path: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub addons: usize,
}

/// `GET <host>/locales.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostLocales {
    pub default: String,
    pub locales: Vec<HostLocale>,
}

impl HostLocales {
    /// The base URL for `locale` on `host`: the host itself for its default language.
    pub fn base_for(&self, host: &str, locale: &str) -> String {
        match self.locales.iter().find(|l| l.locale == locale) {
            Some(l) if l.locale != self.default => join_url(host, &l.path),
            _ => host.to_string(),
        }
    }
}

/// A host's languages, or `None` for a host that predates them (it offers one language, at its
/// root).
pub fn fetch_locales(fetch: &dyn Fetch, host: &str) -> Option<HostLocales> {
    let text = fetch.text(&join_url(host, "locales.json")).ok()?;
    let l: HostLocales = serde_json::from_str(&text).ok()?;
    (!l.locales.is_empty()).then_some(l)
}

/// A language's name in that language, for the picker. Korean and Chinese are named in English:
/// iNilla's screens carry no Hangul or Han font, and a missing glyph draws as a box.
pub fn locale_name(locale: &str) -> &str {
    match locale {
        "enUS" => "English",
        "enGB" => "English (UK)",
        "deDE" => "Deutsch",
        "frFR" => "Français",
        "esES" => "Español",
        "esMX" => "Español (México)",
        "ruRU" => "Русский",
        "koKR" => "Korean",
        "zhCN" => "Chinese (Simplified)",
        "zhTW" => "Chinese (Traditional)",
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonFile {
    /// `pfQuest/db/deDE/quests.lua`: forward slashes, starting with the addon's own folder.
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Addon {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub files: Vec<AddonFile>,
}

impl Addon {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// `GET <base>/addons.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonManifest {
    pub format: u32,
    #[serde(default)]
    pub locale: String,
    pub addons: Vec<Addon>,
}

pub const ADDON_FORMAT: u32 = 1;

/// The addons `<base>` offers; an empty list from a host that has none (or predates them).
pub fn fetch_addons(fetch: &dyn Fetch, base: &str) -> AddonManifest {
    let parsed = fetch
        .text(&join_url(base, "addons.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<AddonManifest>(&t).ok())
        .filter(|m| m.format == ADDON_FORMAT);
    parsed.unwrap_or(AddonManifest {
        format: ADDON_FORMAT,
        locale: String::new(),
        addons: Vec::new(),
    })
}

/// Which addon folders a host installed, kept in `AddOns/.inilla-addons.json`: an update or a
/// removal only ever touches these, never an addon the player copied in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddons {
    /// Folder name → version.
    pub addons: BTreeMap<String, String>,
    /// The language they were installed for.
    #[serde(default)]
    pub locale: String,
}

pub const RECORD: &str = ".inilla-addons.json";

impl InstalledAddons {
    pub fn load(addons_dir: &Path) -> Self {
        fs::read_to_string(addons_dir.join(RECORD))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self, addons_dir: &Path) -> std::io::Result<()> {
        fs::write(
            addons_dir.join(RECORD),
            serde_json::to_string_pretty(self).unwrap_or_default(),
        )
    }
}

/// A file an addon manifest may name: inside the addon's own folder, no way out of it.
pub fn safe_addon_path(addon: &str, path: &str) -> bool {
    let mut parts = path.split('/');
    parts.next() == Some(addon)
        && !addon.is_empty()
        && !addon.starts_with('.')
        && path.split('/').skip(1).count() > 0
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(['\\', ':', '\0']))
}

/// `db/deDE/quests.lua` → itself; a space or anything unusual percent-encoded.
fn url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// What [`sync_addons`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Addons installed or updated: `(name, version)`.
    pub updated: Vec<(String, String)>,
    /// Addons already current.
    pub current: usize,
    /// Addons a host had installed and no longer offers, now removed.
    pub removed: Vec<String>,
}

/// Bring `addons_dir` in line with `manifest`: fetch every file that is missing or differs
/// (verified against its hash before it replaces anything), delete files a host-installed addon
/// no longer has, and remove host-installed addons the host no longer offers. An addon folder the
/// player made that the host now also offers is replaced by the host's copy.
pub fn sync_addons(
    fetch: &dyn Fetch,
    base: &str,
    manifest: &AddonManifest,
    addons_dir: &Path,
    mut report: impl FnMut(&Progress),
    keep_going: &dyn Fn() -> bool,
) -> Result<SyncReport, String> {
    fs::create_dir_all(addons_dir).map_err(|e| format!("{}: {e}", addons_dir.display()))?;
    let mut record = InstalledAddons::load(addons_dir);
    let mut out = SyncReport::default();
    for a in &manifest.addons {
        if let Some(bad) = a.files.iter().find(|f| !safe_addon_path(&a.name, &f.path)) {
            return Err(format!("refusing the addon file {:?}", bad.path));
        }
    }

    // What has to come down: every file whose copy here is missing or differs.
    let mut todo: Vec<(&Addon, &AddonFile)> = Vec::new();
    for a in &manifest.addons {
        for f in &a.files {
            let dest = addons_dir.join(&f.path);
            let same = fs::metadata(&dest).is_ok_and(|m| m.len() == f.size)
                && sha256_file(&dest, |_| {}).is_ok_and(|h| h == f.sha256);
            if !same {
                todo.push((a, f));
            }
        }
    }
    let mut p = Progress {
        file_count: todo.len(),
        bytes_total: todo.iter().map(|(_, f)| f.size).sum(),
        state: "addons".into(),
        ..Default::default()
    };
    report(&p);
    for (i, (a, f)) in todo.iter().enumerate() {
        if !keep_going() {
            return Err("paused".into());
        }
        p.file = f.path.clone();
        p.file_index = i;
        report(&p);
        let url = join_url(base, &format!("addons/{}", url_path(&f.path)));
        let (mut body, _) = fetch.body(&url, 0)?;
        let dest = addons_dir.join(&f.path);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let part = dest.with_extension("inilla-part");
        let mut bytes = Vec::with_capacity(f.size as usize);
        std::io::Read::read_to_end(&mut body, &mut bytes)
            .map_err(|e| format!("{}: {e}", f.path))?;
        let sha: String = {
            use sha2::{Digest, Sha256};
            Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        if bytes.len() as u64 != f.size || sha != f.sha256 {
            return Err(format!(
                "{} ({}) failed its checksum; try again",
                f.path, a.name
            ));
        }
        let mut out_file =
            fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        out_file
            .write_all(&bytes)
            .map_err(|e| format!("{}: {e}", part.display()))?;
        drop(out_file);
        fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        p.bytes_done += f.size;
        report(&p);
    }

    // Tidy: files an offered addon no longer has, and host-installed addons no longer offered.
    for a in &manifest.addons {
        let keep: std::collections::HashSet<&str> =
            a.files.iter().map(|f| f.path.as_str()).collect();
        prune(&addons_dir.join(&a.name), &a.name, &keep);
        let version = a.version.clone().unwrap_or_default();
        let changed = todo.iter().any(|(t, _)| t.name == a.name)
            || record.addons.get(&a.name) != Some(&version);
        if changed {
            out.updated.push((a.name.clone(), version.clone()));
        } else {
            out.current += 1;
        }
        record.addons.insert(a.name.clone(), version);
    }
    let offered: Vec<&str> = manifest.addons.iter().map(|a| a.name.as_str()).collect();
    let gone: Vec<String> = record
        .addons
        .keys()
        .filter(|n| !offered.contains(&n.as_str()))
        .cloned()
        .collect();
    for name in gone {
        if !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.') {
            let _ = fs::remove_dir_all(addons_dir.join(&name));
        }
        record.addons.remove(&name);
        out.removed.push(name);
    }
    record.locale = manifest.locale.clone();
    record
        .save(addons_dir)
        .map_err(|e| format!("{}: {e}", addons_dir.join(RECORD).display()))?;
    p.state = "done".into();
    report(&p);
    Ok(out)
}

/// Delete every file under `dir` that `keep` does not list (paths `prefix/…`), then the folders
/// that leaves empty.
fn prune(dir: &Path, prefix: &str, keep: &std::collections::HashSet<&str>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let rel = format!("{prefix}/{name}");
        let Ok(kind) = e.file_type() else {
            continue;
        };
        if kind.is_dir() {
            prune(&e.path(), &rel, keep);
            let _ = fs::remove_dir(e.path());
        } else if !keep.contains(rel.as_str()) {
            let _ = fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("inilla-addons-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn file(path: &str, body: &str) -> AddonFile {
        use sha2::{Digest, Sha256};
        AddonFile {
            path: path.into(),
            size: body.len() as u64,
            sha256: Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        }
    }

    /// A host that serves fixed bodies by URL and counts what it was asked for.
    struct Host {
        bodies: BTreeMap<String, String>,
        asked: Mutex<Vec<String>>,
    }

    impl Fetch for Host {
        fn text(&self, url: &str) -> Result<String, String> {
            self.bodies
                .get(url)
                .cloned()
                .ok_or_else(|| "not found (404)".into())
        }
        fn body(&self, url: &str, _offset: u64) -> Result<(Box<dyn Read + Send>, bool), String> {
            self.asked.lock().unwrap().push(url.to_string());
            let b = self.text(url)?;
            Ok((Box::new(std::io::Cursor::new(b.into_bytes())), false))
        }
    }

    fn host(files: &[(&str, &str)]) -> Host {
        Host {
            bodies: files
                .iter()
                .map(|(p, b)| (format!("http://h/deDE/addons/{p}"), b.to_string()))
                .collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn manifest(files: &[(&str, &str)], version: &str) -> AddonManifest {
        AddonManifest {
            format: 1,
            locale: "deDE".into(),
            addons: vec![Addon {
                name: "pfQuest".into(),
                version: Some(version.into()),
                files: files.iter().map(|(p, b)| file(p, b)).collect(),
            }],
        }
    }

    #[test]
    fn paths_stay_inside_their_addon() {
        assert!(safe_addon_path("pfQuest", "pfQuest/db/deDE/quests.lua"));
        assert!(!safe_addon_path("pfQuest", "pfQuest"));
        assert!(!safe_addon_path("pfQuest", "Other/x.lua"));
        assert!(!safe_addon_path("pfQuest", "pfQuest/../../x"));
        assert!(!safe_addon_path("pfQuest", "pfQuest//x"));
        assert!(!safe_addon_path("pfQuest", "pfQuest/a\\b"));
        assert!(!safe_addon_path("..", "../x"));
        assert_eq!(url_path("pfQuest/a b+c.lua"), "pfQuest/a%20b%2Bc.lua");
    }

    #[test]
    fn install_update_and_leave_the_players_own_alone() {
        let dir = tmp("sync");
        // The player's own addon, which the host knows nothing of.
        fs::create_dir_all(dir.join("Mine")).unwrap();
        fs::write(dir.join("Mine/Mine.toc"), "mine").unwrap();

        let v1 = [
            ("pfQuest/pfQuest.toc", "toc 1"),
            ("pfQuest/db/q.lua", "quests"),
        ];
        let h = host(&v1);
        let r = sync_addons(
            &h,
            "http://h/deDE/",
            &manifest(&v1, "1"),
            &dir,
            |_| {},
            &|| true,
        )
        .unwrap();
        assert_eq!(r.updated, vec![("pfQuest".into(), "1".into())]);
        assert_eq!(
            fs::read_to_string(dir.join("pfQuest/db/q.lua")).unwrap(),
            "quests"
        );
        assert_eq!(InstalledAddons::load(&dir).locale, "deDE");

        // Nothing changed: nothing is fetched.
        let h = host(&v1);
        let r = sync_addons(
            &h,
            "http://h/deDE/",
            &manifest(&v1, "1"),
            &dir,
            |_| {},
            &|| true,
        )
        .unwrap();
        assert_eq!((r.current, h.asked.lock().unwrap().len()), (1, 0));

        // A new version: one file changes, one goes away, one arrives.
        let v2 = [("pfQuest/pfQuest.toc", "toc 2"), ("pfQuest/new.lua", "new")];
        let h = host(&v2);
        sync_addons(
            &h,
            "http://h/deDE/",
            &manifest(&v2, "2"),
            &dir,
            |_| {},
            &|| true,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("pfQuest/pfQuest.toc")).unwrap(),
            "toc 2"
        );
        assert!(!dir.join("pfQuest/db/q.lua").exists());
        assert!(
            !dir.join("pfQuest/db").exists(),
            "an emptied folder goes too"
        );
        assert!(dir.join("pfQuest/new.lua").exists());

        // The host stops offering it: it goes; the player's own stays.
        let empty = AddonManifest {
            format: 1,
            locale: "deDE".into(),
            addons: vec![],
        };
        let r = sync_addons(&host(&[]), "http://h/deDE/", &empty, &dir, |_| {}, &|| true).unwrap();
        assert_eq!(r.removed, vec!["pfQuest".to_string()]);
        assert!(!dir.join("pfQuest").exists());
        assert!(dir.join("Mine/Mine.toc").exists());
    }

    #[test]
    fn a_corrupt_file_is_refused_and_nothing_is_replaced() {
        let dir = tmp("corrupt");
        let files = [("pfQuest/pfQuest.toc", "good")];
        let mut m = manifest(&files, "1");
        m.addons[0].files[0].sha256 = "00".repeat(32);
        let err =
            sync_addons(&host(&files), "http://h/deDE/", &m, &dir, |_| {}, &|| true).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!dir.join("pfQuest/pfQuest.toc").exists());
    }

    #[test]
    fn a_language_has_its_own_base() {
        let l = HostLocales {
            default: "enUS".into(),
            locales: vec![
                HostLocale {
                    locale: "enUS".into(),
                    path: "enUS/".into(),
                    bytes: 1,
                    addons: 1,
                },
                HostLocale {
                    locale: "deDE".into(),
                    path: "deDE/".into(),
                    bytes: 1,
                    addons: 1,
                },
            ],
        };
        assert_eq!(l.base_for("http://h:8099/", "enUS"), "http://h:8099/");
        assert_eq!(l.base_for("http://h:8099/", "deDE"), "http://h:8099/deDE/");
        assert_eq!(locale_name("deDE"), "Deutsch");
    }
}
