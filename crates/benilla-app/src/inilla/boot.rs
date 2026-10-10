//! **Before the app exists**: lay the iOS container out so benilla's own resolution finds it.
//!
//! benilla resolves its two folders through environment overrides (`$BENILLA_HOME` for its state,
//! `$WOW_DATA` for the game data), so the whole iOS layout is a matter of setting those before
//! [`crate::run`] builds the app — no change to either resolver. Also here, because it has to
//! happen before the first line is printed or the first archive opened:
//!
//! - the **session log** — stdout/stderr captured into `Diagnostics/session-<time>.log` and the
//!   log ring (the last 10 sessions are kept);
//! - the **Documents import** — a `Data` folder the player copied into iNilla's Documents (Finder,
//!   the Files app, a USB-C stick) is moved into place now, which is a rename on the same volume
//!   and instant, so a first launch after copying goes straight to the login screen;
//! - the log filter and colour defaults a phone wants.
//!
//! Off iOS this does nothing unless `$INILLA_CONTAINER` asks the desktop build to play the
//! container (`inilla_platform::paths`).

use std::path::Path;

use inilla_core::data;
use inilla_platform::{device, paths, stdio};

/// How many session logs to keep in `Diagnostics/`.
const KEEP_LOGS: usize = 10;

/// Prepare the process. Call once, first thing, before [`crate::run`].
pub fn prepare() {
    if !paths::in_container() {
        return;
    }
    let (Some(docs), Some(config), Some(support), Some(data_dir)) = (
        paths::documents(),
        paths::config_dir(),
        paths::app_support(),
        paths::data_dir(),
    ) else {
        return;
    };
    for d in [&docs, &config, &support, &data_dir] {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = device::exclude_from_backup(&support);

    // The state folder first: the settings (and with them the log switch) live in it.
    std::env::set_var("BENILLA_HOME", &config);
    let settings = super::store::load_settings();

    let diagnostics = config.join("Diagnostics");
    let log_path = diagnostics.join(format!("session-{}.log", timestamp()));
    let log_file = settings.debug.log_to_file.then_some(log_path.as_path());
    if let Err(e) = stdio::capture(log_file) {
        eprintln!("inilla: log capture unavailable: {e}");
    }
    prune_logs(&diagnostics);

    // Colours are for terminals; the log is read in the Files app and the remote console.
    std::env::set_var("NO_COLOR", "1");
    if std::env::var_os("RUST_LOG").is_none() {
        // benilla's own default (`wgpu=error,naga=warn`), plus silence for the one error iOS
        // produces by design: the virtual mouse moves the cursor every drag frame, and winit
        // answers every `set_cursor_position` on iOS with NotSupported.
        std::env::set_var(
            "RUST_LOG",
            "info,wgpu=error,naga=warn,bevy_winit::system=off",
        );
    }

    println!("inilla: {}", device::describe());
    println!("inilla: state {}", config.display());

    import_from_documents(&docs, &data_dir);

    let st = data::status(&data_dir);
    if st.complete() {
        println!(
            "inilla: game data ready — {} archives, {:.1} GB in {}",
            st.present.len(),
            st.total_bytes() as f64 / 1e9,
            data_dir.display()
        );
        std::env::set_var("WOW_DATA", &data_dir);
    } else {
        // `WOW_DATA=` (set, empty) is benilla's explicit "there is no install": the client
        // boots to its no-data path and iNilla's setup screen takes over.
        println!(
            "inilla: no complete game data yet (missing: {}) — setup will ask for it",
            st.missing_required.join(", ")
        );
        std::env::set_var("WOW_DATA", "");
    }
}

/// Move a complete `Data` folder found in Documents into the data directory.
fn import_from_documents(docs: &Path, data_dir: &Path) {
    let current = data::status(data_dir);
    if current.complete() {
        return;
    }
    let Some(found) = data::find_imports(docs)
        .into_iter()
        .find(|c| c.status.complete())
    else {
        return;
    };
    println!(
        "inilla: importing {} archives from {}",
        found.status.present.len(),
        found.dir.display()
    );
    match data::import(&found.dir, data_dir, |_, _, _| {}) {
        Ok(n) => {
            println!("inilla: imported {n} archives");
            // Leave an empty folder behind rather than a confusing half-copy; remove it if empty.
            let _ = std::fs::remove_dir(&found.dir);
        }
        Err(e) => eprintln!("inilla: import failed: {e}"),
    }
}

fn prune_logs(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("session-") && n.ends_with(".log"))
        })
        .collect();
    logs.sort();
    let excess = logs.len().saturating_sub(KEEP_LOGS);
    for p in logs.into_iter().take(excess) {
        let _ = std::fs::remove_file(p);
    }
}

/// `YYYYMMDD-HHMMSS` in UTC, from the system clock (no time-zone database on hand, and a log
/// name only has to sort).
pub(crate) fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}{mo:02}{d:02}-{h:02}{m:02}{s:02}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn timestamps_sort_and_have_the_right_shape() {
        let t = super::timestamp();
        assert_eq!(t.len(), 15);
        assert_eq!(&t[8..9], "-");
        assert!(t.starts_with("20"));
    }
}
