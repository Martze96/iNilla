//! **Getting the game data onto the device**, the app half: the status the setup screen shows,
//! and the background jobs it starts — scan-and-import from Documents, fetch a server's manifest,
//! download (resumable, verified per file). The logic is `inilla_core::data`; this is threads,
//! progress and the HTTP client.
//!
//! benilla opens the archives when the app is built, so data that arrives while it runs needs one
//! relaunch; the setup screen says so and offers it. (A Documents import found at launch needs
//! none — `boot::prepare` moves it into place before the app exists.)

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::prelude::*;

use inilla_core::addons::{self as ka, AddonManifest, HostLocales, SyncReport};
use inilla_core::data::{self as kd, DataStatus, Fetch, Manifest, Progress};

/// What a finished job produced.
enum Outcome {
    Manifest(Box<Offer>),
    Downloaded(Option<SyncReport>),
    Addons(SyncReport),
    Imported(usize),
    NothingToImport,
}

/// What a host offers for one language: the archives, the addons, and where they come from.
pub(crate) struct Offer {
    pub(crate) manifest: Manifest,
    pub(crate) addons: AddonManifest,
    /// The host's languages, when it has more than one.
    pub(crate) locales: Option<HostLocales>,
    /// The language this offer is for (`enUS` when the host does not say).
    pub(crate) locale: String,
    /// The base URL the archives and addons of that language come from.
    pub(crate) base: String,
}

/// benilla's addon folder, `<benilla-config>/AddOns`.
pub(crate) fn addons_dir() -> Option<PathBuf> {
    crate::local_state::home().map(|h| h.join("AddOns"))
}

struct Job {
    label: &'static str,
    progress: Arc<Mutex<Progress>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<Outcome, String>>>,
}

#[derive(Resource)]
pub(crate) struct DataState {
    /// Where the data lives (the iOS container's, or `$WOW_DATA` on desktop).
    pub(crate) dir: Option<PathBuf>,
    pub(crate) status: DataStatus,
    /// The archives were complete when the app was built (so benilla has them open).
    pub(crate) ready_at_boot: bool,
    pub(crate) manifest: Option<Manifest>,
    /// The checked host's addons and languages, and the base URL the manifest came from.
    pub(crate) addons: Option<AddonManifest>,
    pub(crate) locales: Option<HostLocales>,
    pub(crate) locale: Option<String>,
    pub(crate) base: Option<String>,
    /// The last job's result, for the setup screen.
    pub(crate) message: Option<(bool, String)>,
    job: Option<Job>,
}

impl DataState {
    pub(crate) fn busy(&self) -> Option<(&'static str, Progress)> {
        self.job.as_ref().map(|j| {
            (
                j.label,
                j.progress.lock().map(|p| p.clone()).unwrap_or_default(),
            )
        })
    }

    /// Data is complete now but benilla did not have it at boot: a relaunch is needed.
    pub(crate) fn needs_restart(&self) -> bool {
        self.status.complete() && !self.ready_at_boot
    }

    pub(crate) fn refresh(&mut self) {
        if let Some(d) = &self.dir {
            self.status = kd::status(d);
        }
    }

    fn start(
        &mut self,
        label: &'static str,
        f: impl FnOnce(Arc<Mutex<Progress>>, Arc<AtomicBool>) -> Result<Outcome, String>
            + Send
            + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let progress = Arc::new(Mutex::new(Progress::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (progress.clone(), stop.clone());
        let handle = std::thread::Builder::new()
            .name(format!("inilla-{label}"))
            .spawn(move || f(p, s))
            .ok();
        self.message = None;
        self.job = Some(Job {
            label,
            progress,
            stop,
            handle,
        });
    }

    /// Look through Documents for a `Data` folder and move it into place.
    pub(crate) fn scan_import(&mut self) {
        let (Some(docs), Some(dir)) = (inilla_platform::paths::documents(), self.dir.clone())
        else {
            self.message = Some((
                false,
                "There is no Documents folder to import from on this platform.".into(),
            ));
            return;
        };
        self.start("import", move |progress, _| {
            let found = kd::find_imports(&docs);
            let Some(c) = found.into_iter().max_by_key(|c| c.status.present.len()) else {
                return Ok(Outcome::NothingToImport);
            };
            let n = kd::import(&c.dir, &dir, |name, i, total| {
                if let Ok(mut p) = progress.lock() {
                    p.file = name.to_string();
                    p.file_index = i;
                    p.file_count = total;
                    p.state = "moving".into();
                }
            })
            .map_err(|e| format!("import failed: {e}"))?;
            let _ = std::fs::remove_dir(&c.dir);
            Ok(Outcome::Imported(n))
        });
    }

    /// Ask the host at `url` what it offers: its languages, and for `locale` (or its default)
    /// the manifest and the addons.
    pub(crate) fn fetch_manifest(
        &mut self,
        url: String,
        token: Option<String>,
        locale: Option<String>,
    ) {
        self.start("manifest", move |_, _| {
            let http = Http::new(token);
            let locales = ka::fetch_locales(&http, &url);
            let base = match (&locales, &locale) {
                (Some(l), Some(want)) => l.base_for(&url, want),
                _ => url.clone(),
            };
            let manifest = kd::fetch_manifest(&http, &base)?;
            let addons = ka::fetch_addons(&http, &base);
            let locale = manifest
                .locale
                .clone()
                .or(locale)
                .unwrap_or_else(|| "enUS".into());
            Ok(Outcome::Manifest(Box::new(Offer {
                manifest,
                addons,
                locales: locales.filter(|l| l.locales.len() > 1),
                locale,
                base,
            })))
        });
    }

    /// Download what the manifest lists and the device lacks, then the host's addons.
    pub(crate) fn download(&mut self, token: Option<String>) {
        let (Some(manifest), Some(dir), Some(base)) =
            (self.manifest.clone(), self.dir.clone(), self.base.clone())
        else {
            return;
        };
        let addons = self.addons.clone().filter(|a| !a.addons.is_empty());
        self.start("download", move |progress, stop| {
            let http = Http::new(token);
            let keep = move || !stop.load(Ordering::Relaxed);
            let report = |p: &Progress| {
                if let Ok(mut g) = progress.lock() {
                    *g = p.clone();
                }
            };
            kd::download(&http, &base, &manifest, &dir, report, &keep)?;
            let synced = match (addons, addons_dir()) {
                (Some(a), Some(to)) => Some(ka::sync_addons(&http, &base, &a, &to, report, &keep)?),
                _ => None,
            };
            Ok(Outcome::Downloaded(synced))
        });
    }

    /// Install or update the checked host's addons.
    pub(crate) fn sync_addons(&mut self, token: Option<String>) {
        let (Some(addons), Some(base), Some(to)) =
            (self.addons.clone(), self.base.clone(), addons_dir())
        else {
            return;
        };
        self.start("addons", move |progress, stop| {
            let http = Http::new(token);
            let keep = move || !stop.load(Ordering::Relaxed);
            ka::sync_addons(
                &http,
                &base,
                &addons,
                &to,
                |p| {
                    if let Ok(mut g) = progress.lock() {
                        *g = p.clone();
                    }
                },
                &keep,
            )
            .map(Outcome::Addons)
        });
    }

    /// Stop the running download at the next buffer (its `.part` stays for a resume).
    pub(crate) fn pause(&mut self) {
        if let Some(j) = &self.job {
            j.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Bytes free on the data volume, when the platform says.
    pub(crate) fn free_bytes(&self) -> Option<u64> {
        let dir = self.dir.as_ref()?;
        free_space(dir)
    }
}

#[cfg(unix)]
fn free_space(dir: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a valid NUL-terminated path and an out-struct of the right type.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    // `f_bavail` is 32-bit on Darwin and 64-bit on Linux: the casts are only identity on one.
    #[allow(clippy::unnecessary_cast)]
    Some(s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_space(_dir: &std::path::Path) -> Option<u64> {
    None
}

/// The HTTP half of the download, over `ureq` (rustls).
struct Http {
    agent: ureq::Agent,
    token: Option<String>,
}

impl Http {
    fn new(token: Option<String>) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .timeout_read(Duration::from_secs(60))
                .build(),
            token: token.filter(|t| !t.trim().is_empty()),
        }
    }

    fn get(&self, url: &str) -> ureq::Request {
        let r = self.agent.get(url).set("User-Agent", "iNilla");
        match &self.token {
            Some(t) => r.set("Authorization", &format!("Bearer {}", t.trim())),
            None => r,
        }
    }
}

fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401 | 403, _) => "the server refused the token (401/403)".into(),
        ureq::Error::Status(404, _) => "not found (404) — check the URL".into(),
        ureq::Error::Status(code, _) => format!("the server answered {code}"),
        ureq::Error::Transport(t) => format!("could not reach the server: {t}"),
    }
}

impl Fetch for Http {
    fn text(&self, url: &str) -> Result<String, String> {
        self.get(url)
            .call()
            .map_err(describe)?
            .into_string()
            .map_err(|e| e.to_string())
    }

    fn body(&self, url: &str, offset: u64) -> Result<(Box<dyn Read + Send>, bool), String> {
        let mut req = self.get(url);
        if offset > 0 {
            req = req.set("Range", &format!("bytes={offset}-"));
        }
        let resp = req.call().map_err(describe)?;
        let ranged = resp.status() == 206;
        Ok((Box::new(resp.into_reader()), ranged))
    }
}

pub(crate) struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        let dir = inilla_platform::paths::data_dir()
            .or_else(|| {
                std::env::var_os("WOW_DATA")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(benilla_formats::wow_data);
        let status = dir.as_deref().map(kd::status).unwrap_or_default();
        // Complete at boot iff benilla was handed it: `boot::prepare` sets `WOW_DATA` only then.
        let ready = status.complete() && std::env::var_os("WOW_DATA").is_none_or(|v| !v.is_empty());
        app.insert_resource(DataState {
            dir,
            status,
            ready_at_boot: ready,
            manifest: None,
            addons: None,
            locales: None,
            locale: None,
            base: None,
            message: None,
            job: None,
        })
        .add_systems(Update, poll_job);
    }
}

fn poll_job(mut state: ResMut<DataState>) {
    let done = state
        .job
        .as_ref()
        .is_some_and(|j| j.handle.as_ref().is_none_or(|h| h.is_finished()));
    if !done {
        return;
    }
    let Some(mut job) = state.job.take() else {
        return;
    };
    let result = job
        .handle
        .take()
        .map(|h| h.join().unwrap_or_else(|_| Err("the job crashed".into())))
        .unwrap_or_else(|| Err("the job could not start".into()));
    state.refresh();
    state.message = Some(match result {
        Ok(Outcome::Manifest(offer)) => {
            let offer = *offer;
            let m = &offer.manifest;
            let gb = m.total_bytes() as f64 / 1e9;
            let addons = if offer.addons.addons.is_empty() {
                String::new()
            } else {
                format!(" and {}", addon_names(&offer.addons))
            };
            let text = format!(
                "The server offers {} archives ({gb:.1} GB, {}){addons}.",
                m.files.len(),
                ka::locale_name(&offer.locale),
            );
            state.manifest = Some(offer.manifest);
            state.addons = Some(offer.addons);
            state.locales = offer.locales;
            state.locale = Some(offer.locale);
            state.base = Some(offer.base);
            (true, text)
        }
        Ok(Outcome::Downloaded(synced)) => (
            true,
            match synced {
                Some(r) => format!("Download complete and verified. {}", sync_text(&r)),
                None => "Download complete and verified.".into(),
            },
        ),
        Ok(Outcome::Addons(r)) => (true, sync_text(&r)),
        Ok(Outcome::Imported(n)) => (true, format!("Imported {n} archives.")),
        Ok(Outcome::NothingToImport) => (
            false,
            "No Data folder found in iNilla's Documents yet. Copy it in and try again.".into(),
        ),
        Err(e) if e == "paused" => (true, "Paused — Resume continues where it stopped.".into()),
        Err(e) => (false, e),
    });
    info!(
        "inilla: data job {} finished — {}",
        job.label,
        state.message.as_ref().map_or("", |m| m.1.as_str())
    );
}

/// `pfQuest 7.0.1, Bagnon`
pub(crate) fn addon_names(m: &AddonManifest) -> String {
    m.addons
        .iter()
        .map(|a| match &a.version {
            Some(v) if !v.is_empty() => format!("{} {v}", a.name),
            _ => a.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn sync_text(r: &SyncReport) -> String {
    let mut parts = Vec::new();
    if !r.updated.is_empty() {
        let names: Vec<String> = r
            .updated
            .iter()
            .map(|(n, v)| {
                if v.is_empty() {
                    n.clone()
                } else {
                    format!("{n} {v}")
                }
            })
            .collect();
        parts.push(format!("Installed {}.", names.join(", ")));
    } else if r.current > 0 {
        parts.push("Addons are up to date.".to_string());
    }
    if !r.removed.is_empty() {
        parts.push(format!("Removed {}.", r.removed.join(", ")));
    }
    if parts.is_empty() {
        "This server offers no addons.".to_string()
    } else {
        parts.join(" ")
    }
}
