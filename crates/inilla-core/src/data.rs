//! **The game data**: what a 1.12.1 `Data` folder must hold, how a server describes the copy it
//! offers ([`Manifest`]), and how a device gets from "nothing" to "complete" — by import (the
//! player copied their own folder into the app's Documents) or by download, resumable and
//! verified.
//!
//! Only the MPQs at the root of `Data/` matter: that is all benilla mounts (`benilla-formats`'
//! chain: the ten base archives, `patch.MPQ`, `patch-?.MPQ`, `speech2.MPQ`). Nothing here reads
//! inside an archive.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The archives a 1.12.1 client cannot start without.
pub const REQUIRED: &[&str] = &[
    "dbc.MPQ",
    "fonts.MPQ",
    "interface.MPQ",
    "misc.MPQ",
    "model.MPQ",
    "patch.MPQ",
    "sound.MPQ",
    "speech.MPQ",
    "terrain.MPQ",
    "texture.MPQ",
    "wmo.MPQ",
];

/// Archives a stock 1.12.1 install also has; missing them is worth a warning, not a refusal.
pub const EXPECTED: &[&str] = &["patch-2.MPQ"];

/// Whether `name` is an archive the chain would mount (case-insensitive, like the reference).
pub fn is_mounted_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if REQUIRED.iter().any(|r| r.eq_ignore_ascii_case(&lower)) || lower == "speech2.mpq" {
        return true;
    }
    lower
        .strip_prefix("patch-")
        .and_then(|r| r.strip_suffix(".mpq"))
        .is_some_and(|mid| mid.chars().count() == 1)
}

/// What a local `Data` folder holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataStatus {
    pub dir: PathBuf,
    /// `(name as found, size)` for every mountable archive present.
    pub present: Vec<(String, u64)>,
    pub missing_required: Vec<&'static str>,
    pub missing_expected: Vec<&'static str>,
}

impl DataStatus {
    pub fn complete(&self) -> bool {
        !self.present.is_empty() && self.missing_required.is_empty()
    }

    pub fn total_bytes(&self) -> u64 {
        self.present.iter().map(|(_, s)| s).sum()
    }
}

/// Look at `dir` (it need not exist).
pub fn status(dir: &Path) -> DataStatus {
    let mut present = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if !is_mounted_name(&name) {
                continue;
            }
            if let Ok(meta) = e.metadata() {
                if meta.is_file() && meta.len() > 0 {
                    present.push((name, meta.len()));
                }
            }
        }
    }
    present.sort();
    let has = |want: &str| present.iter().any(|(n, _)| n.eq_ignore_ascii_case(want));
    DataStatus {
        dir: dir.to_path_buf(),
        missing_required: REQUIRED.iter().copied().filter(|r| !has(r)).collect(),
        missing_expected: EXPECTED.iter().copied().filter(|r| !has(r)).collect(),
        present,
    }
}

/// One file a server offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    pub name: String,
    pub size: u64,
    /// Lower-case hex SHA-256 of the whole file.
    pub sha256: String,
}

/// What a data host offers: `GET <base>/manifest.json`, files at `GET <base>/files/<name>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// The client build the files belong to (5875 = 1.12.1).
    #[serde(default = "default_build")]
    pub build: u32,
    /// `enUS`, `deDE`, … as the operator declared it.
    #[serde(default)]
    pub locale: Option<String>,
    pub files: Vec<ManifestFile>,
}

fn default_build() -> u32 {
    5875
}

pub const MANIFEST_FORMAT: u32 = 1;

impl Manifest {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Problems that make this manifest unusable for a 1.12.1 client.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.format != MANIFEST_FORMAT {
            out.push(format!(
                "manifest format {} (this build reads {MANIFEST_FORMAT})",
                self.format
            ));
        }
        if self.build != 5875 {
            out.push(format!(
                "the server offers client build {} — iNilla needs 1.12.1 (5875)",
                self.build
            ));
        }
        for r in REQUIRED {
            if !self.files.iter().any(|f| f.name.eq_ignore_ascii_case(r)) {
                out.push(format!("the server does not offer {r}"));
            }
        }
        for f in &self.files {
            if f.name.contains('/') || f.name.contains('\\') || f.name.starts_with('.') {
                out.push(format!("refusing unsafe file name {:?}", f.name));
            }
        }
        out
    }

    /// Build a manifest by hashing every mountable archive in `dir` (the data host's job).
    pub fn build_from(
        dir: &Path,
        locale: Option<String>,
        mut progress: impl FnMut(&str, u64, u64),
    ) -> io::Result<Self> {
        let st = status(dir);
        let total = st.total_bytes();
        let mut done = 0;
        let mut files = Vec::new();
        for (name, size) in &st.present {
            let sha = sha256_file(&dir.join(name), |n| progress(name, done + n, total))?;
            done += size;
            files.push(ManifestFile {
                name: name.clone(),
                size: *size,
                sha256: sha,
            });
        }
        Ok(Manifest {
            format: MANIFEST_FORMAT,
            build: 5875,
            locale,
            files,
        })
    }
}

/// SHA-256 of a file, reporting bytes hashed so far.
pub fn sha256_file(path: &Path, mut progress: impl FnMut(u64)) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n_total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        n_total += n as u64;
        progress(n_total);
    }
    Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What to do with one manifest file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Already here at the right size.
    Keep,
    /// Fetch from byte `offset` (a `.part` from an earlier attempt is resumed).
    Fetch { offset: u64 },
}

/// The download plan: one step per manifest file, by size only (hashes are checked as each
/// download completes, and on demand by [`verify`]).
pub fn plan(manifest: &Manifest, dir: &Path) -> Vec<(ManifestFile, Step)> {
    manifest
        .files
        .iter()
        .map(|f| {
            let done = dir.join(&f.name);
            let part = part_path(dir, &f.name);
            let step = match fs::metadata(&done) {
                Ok(m) if m.len() == f.size => Step::Keep,
                _ => match fs::metadata(&part) {
                    Ok(m) if m.len() < f.size => Step::Fetch { offset: m.len() },
                    _ => Step::Fetch { offset: 0 },
                },
            };
            (f.clone(), step)
        })
        .collect()
}

pub fn part_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!(".{name}.part"))
}

/// Check every file in `dir` against the manifest's hashes; returns the names that differ.
pub fn verify(
    manifest: &Manifest,
    dir: &Path,
    mut progress: impl FnMut(&str, u64, u64),
) -> io::Result<Vec<String>> {
    let total = manifest.total_bytes();
    let mut done = 0;
    let mut bad = Vec::new();
    for f in &manifest.files {
        let path = dir.join(&f.name);
        let ok = match fs::metadata(&path) {
            Ok(m) if m.len() == f.size => {
                sha256_file(&path, |n| progress(&f.name, done + n, total))? == f.sha256
            }
            _ => false,
        };
        done += f.size;
        if !ok {
            bad.push(f.name.clone());
        }
    }
    Ok(bad)
}

/// The network half of a download, so the engine can be tested without one.
pub trait Fetch {
    /// `GET url` as text.
    fn text(&self, url: &str) -> Result<String, String>;
    /// `GET url` from byte `offset` (a `Range` request when `offset > 0`). Returns the body and
    /// whether the server honoured the range (a `200` to a range request restarts the file).
    fn body(&self, url: &str, offset: u64) -> Result<(Box<dyn Read + Send>, bool), String>;
}

/// Progress of a running download, polled by the UI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub file: String,
    pub file_index: usize,
    pub file_count: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub state: String,
}

/// `base` + `path`, with exactly one slash between them.
pub fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Fetch and parse `<base>/manifest.json`.
pub fn fetch_manifest(fetch: &dyn Fetch, base: &str) -> Result<Manifest, String> {
    let text = fetch.text(&join_url(base, "manifest.json"))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("manifest: {e}"))?;
    let problems = m.problems();
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    Ok(m)
}

/// Download everything [`plan`] says is missing into `dir`, resuming partial files, verifying each
/// file's hash as it completes. `report` is called often; returning `false` from `keep_going`
/// stops cleanly (the `.part` stays for the next attempt).
pub fn download(
    fetch: &dyn Fetch,
    base: &str,
    manifest: &Manifest,
    dir: &Path,
    mut report: impl FnMut(&Progress),
    keep_going: &dyn Fn() -> bool,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let steps = plan(manifest, dir);
    let total = manifest.total_bytes();
    let mut p = Progress {
        file_count: steps.len(),
        bytes_total: total,
        ..Default::default()
    };
    p.bytes_done = steps
        .iter()
        .map(|(f, s)| match s {
            Step::Keep => f.size,
            Step::Fetch { offset } => *offset,
        })
        .sum();
    for (i, (file, step)) in steps.iter().enumerate() {
        let Step::Fetch { offset } = *step else {
            continue;
        };
        p.file = file.name.clone();
        p.file_index = i;
        p.state = if offset > 0 {
            "resuming".into()
        } else {
            "downloading".into()
        };
        report(&p);
        let url = join_url(base, &format!("files/{}", file.name));
        let part = part_path(dir, &file.name);
        let (mut body, ranged) = fetch.body(&url, offset)?;
        let mut out = if offset > 0 && ranged {
            fs::OpenOptions::new()
                .append(true)
                .open(&part)
                .map_err(|e| format!("{}: {e}", part.display()))?
        } else {
            if offset > 0 {
                p.bytes_done -= offset;
            }
            fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?
        };
        let mut written = if ranged { offset } else { 0 };
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if !keep_going() {
                p.state = "paused".into();
                report(&p);
                return Err("paused".into());
            }
            let n = body
                .read(&mut buf)
                .map_err(|e| format!("{}: {e}", file.name))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("writing {}: {e}", part.display()))?;
            written += n as u64;
            p.bytes_done += n as u64;
            report(&p);
            if written > file.size {
                break;
            }
        }
        out.sync_all().ok();
        drop(out);
        if written != file.size {
            return Err(format!(
                "{}: got {written} of {} bytes — the connection dropped; try again to resume",
                file.name, file.size
            ));
        }
        p.state = "verifying".into();
        report(&p);
        let sha = sha256_file(&part, |_| {}).map_err(|e| format!("{}: {e}", file.name))?;
        if sha != file.sha256 {
            let _ = fs::remove_file(&part);
            return Err(format!(
                "{} failed its checksum and was discarded; try again",
                file.name
            ));
        }
        fs::rename(&part, dir.join(&file.name)).map_err(|e| format!("{}: {e}", file.name))?;
    }
    p.state = "done".into();
    p.file.clear();
    report(&p);
    Ok(())
}

/// A folder of archives found in the app's Documents, ready to import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportCandidate {
    pub dir: PathBuf,
    pub status: DataStatus,
}

/// Look for a `Data` folder (or loose MPQs) the player copied into `root` — through Finder, the
/// Files app or iTunes file sharing. Searches three levels deep, so `Documents/Data`,
/// `Documents/World of Warcraft/Data` and `Documents/WoW 1.12.1/World of Warcraft/Data` are found.
pub fn find_imports(root: &Path) -> Vec<ImportCandidate> {
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out.sort_by_key(|a| std::cmp::Reverse(a.status.present.len()));
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<ImportCandidate>) {
    let st = status(dir);
    if !st.present.is_empty() {
        out.push(ImportCandidate {
            dir: dir.to_path_buf(),
            status: st,
        });
    }
    if depth >= 3 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        // Our own folders and hidden ones are never import sources.
        if name.starts_with('.') || name == "benilla-config" || name == "Inbox" {
            continue;
        }
        if p.is_dir() {
            walk(&p, depth + 1, out);
        }
    }
}

/// Move the archives of `from` into `to` (a rename on the same volume, which Documents and
/// Application Support are: instant, no second 5 GB copy), falling back to copy-and-delete.
/// Returns how many files moved.
pub fn import(
    from: &Path,
    to: &Path,
    mut progress: impl FnMut(&str, usize, usize),
) -> io::Result<usize> {
    fs::create_dir_all(to)?;
    let st = status(from);
    let n = st.present.len();
    for (i, (name, _)) in st.present.iter().enumerate() {
        progress(name, i, n);
        let src = from.join(name);
        let dst = to.join(name);
        if fs::rename(&src, &dst).is_err() {
            fs::copy(&src, &dst)?;
            fs::remove_file(&src)?;
        }
    }
    progress("", n, n);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("inilla-data-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fill(dir: &Path, names: &[&str]) {
        for n in names {
            fs::write(dir.join(n), format!("mpq:{n}")).unwrap();
        }
    }

    #[test]
    fn mounted_names_follow_the_chain_law() {
        assert!(is_mounted_name("DBC.mpq"));
        assert!(is_mounted_name("patch-3.MPQ"));
        assert!(is_mounted_name("speech2.MPQ"));
        assert!(!is_mounted_name("patch-10.MPQ"));
        assert!(!is_mounted_name("realmlist.wtf"));
    }

    #[test]
    fn status_reports_what_is_missing() {
        let d = tmp("status");
        fill(&d, &REQUIRED[..5]);
        let st = status(&d);
        assert!(!st.complete());
        assert_eq!(st.missing_required.len(), REQUIRED.len() - 5);
        fill(&d, REQUIRED);
        let st = status(&d);
        assert!(st.complete());
        assert_eq!(st.missing_expected, vec!["patch-2.MPQ"]);
    }

    #[test]
    fn imports_are_found_nested_and_moved() {
        let root = tmp("import");
        let data = root.join("World of Warcraft").join("Data");
        fs::create_dir_all(&data).unwrap();
        fill(&data, REQUIRED);
        let found = find_imports(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dir, data);
        let to = root.join("target");
        let n = import(&data, &to, |_, _, _| {}).unwrap();
        assert_eq!(n, REQUIRED.len());
        assert!(status(&to).complete());
        assert!(status(&data).present.is_empty());
    }

    /// A fake server over a directory, which can cut the connection after `cut` bytes once.
    struct Fake {
        dir: PathBuf,
        manifest: String,
        cut: Mutex<Option<usize>>,
    }

    impl Fetch for Fake {
        fn text(&self, _url: &str) -> Result<String, String> {
            Ok(self.manifest.clone())
        }
        fn body(&self, url: &str, offset: u64) -> Result<(Box<dyn Read + Send>, bool), String> {
            let name = url.rsplit('/').next().unwrap();
            let bytes = fs::read(self.dir.join(name)).map_err(|e| e.to_string())?;
            let mut rest = bytes[offset as usize..].to_vec();
            if let Some(n) = self.cut.lock().unwrap().take() {
                rest.truncate(n);
            }
            Ok((Box::new(io::Cursor::new(rest)), true))
        }
    }

    #[test]
    fn a_download_resumes_after_a_drop_and_verifies() {
        let src = tmp("dl-src");
        fill(&src, REQUIRED);
        let m = Manifest::build_from(&src, Some("enUS".into()), |_, _, _| {}).unwrap();
        assert!(m.problems().is_empty(), "{:?}", m.problems());
        let fake = Fake {
            dir: src.clone(),
            manifest: serde_json::to_string(&m).unwrap(),
            cut: Mutex::new(Some(3)),
        };
        let dst = tmp("dl-dst");
        let fetched = fetch_manifest(&fake, "http://x/").unwrap();
        let first = download(&fake, "http://x", &fetched, &dst, |_| {}, &|| true);
        assert!(first.is_err(), "the cut connection is reported");
        // The partial file is resumed, not restarted.
        let steps = plan(&fetched, &dst);
        assert!(steps.iter().any(|(_, s)| *s == Step::Fetch { offset: 3 }));
        download(&fake, "http://x", &fetched, &dst, |_| {}, &|| true).unwrap();
        assert!(status(&dst).complete());
        assert!(verify(&fetched, &dst, |_, _, _| {}).unwrap().is_empty());
        assert!(plan(&fetched, &dst).iter().all(|(_, s)| *s == Step::Keep));
    }

    #[test]
    fn a_corrupt_download_is_discarded() {
        let src = tmp("bad-src");
        fill(&src, REQUIRED);
        let mut m = Manifest::build_from(&src, None, |_, _, _| {}).unwrap();
        m.files[0].sha256 = "00".repeat(32);
        let fake = Fake {
            dir: src,
            manifest: String::new(),
            cut: Mutex::new(None),
        };
        let dst = tmp("bad-dst");
        let err = download(&fake, "http://x", &m, &dst, |_| {}, &|| true).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!part_path(&dst, &m.files[0].name).exists());
    }

    #[test]
    fn unsafe_names_are_refused() {
        let m = Manifest {
            format: 1,
            build: 5875,
            locale: None,
            files: vec![ManifestFile {
                name: "../evil".into(),
                size: 1,
                sha256: String::new(),
            }],
        };
        assert!(m.problems().iter().any(|p| p.contains("unsafe")));
    }
}
