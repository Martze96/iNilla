//! **First run: the game data.** Two ways in, both ending at the same folder:
//!
//! - **Copy it in** — the player's own 1.12.1 `Data` folder, copied into iNilla's Documents by
//!   Finder / the Apple Devices app (File Sharing), the Files app (iCloud Drive, a USB-C stick,
//!   a network share) or AirDrop. Checked and moved into place without a second copy.
//! - **Download it** — from the data host of an iNilla server, the player's own or their operator's:
//!   a manifest of the archives with their hashes, then each archive, resumable, verified.
//!
//! The app itself ships no Blizzard files; whichever way they arrive, they are the player's own.

use bevy_egui::egui;

use super::{hint, restart, row, Overlay, Panel};
use crate::inilla::data::DataState;
use crate::inilla::hud::GOLD;
use crate::inilla::Inilla;

pub(super) fn show(
    ui: &mut egui::Ui,
    overlay: &mut Overlay,
    inilla: &mut Inilla,
    data: &mut DataState,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let st = &data.status;
        if st.complete() {
            ui.label(
                egui::RichText::new(format!(
                    "✔ Game data ready — {} archives, {:.1} GB.",
                    st.present.len(),
                    st.total_bytes() as f64 / 1e9
                ))
                .color(egui::Color32::LIGHT_GREEN)
                .size(19.0),
            );
            if !st.missing_expected.is_empty() {
                hint(ui, &format!("Also usually present, but not required: {}.", st.missing_expected.join(", ")));
            }
            if data.needs_restart() {
                ui.label("iNilla opens the archives when it starts. Close it and open it again to play.");
                if ui.add(egui::Button::new(egui::RichText::new("Close iNilla now").color(GOLD)).min_size(egui::vec2(200.0, 44.0))).clicked() {
                    inilla.settings.setup_done = true;
                    restart(inilla);
                }
                hint(ui, super::relaunch_hint());
            } else if ui.button("Continue").clicked() {
                inilla.settings.setup_done = true;
                overlay.panel = Some(Panel::Launcher);
            }
            ui.separator();
        } else {
            ui.label(
                egui::RichText::new("iNilla needs the game data from your own World of Warcraft 1.12.1 (build 5875) client.")
                    .size(18.0),
            );
            hint(
                ui,
                "About 5 GB: the .MPQ archives in the client's Data folder. The app contains no \
                 Blizzard files — they are yours, read at runtime. English (enUS) is the \
                 tested language; German (deDE) data works the same way.",
            );
            if !st.present.is_empty() {
                ui.label(format!(
                    "Found {} of the archives so far; missing: {}.",
                    st.present.len(),
                    st.missing_required.join(", ")
                ));
            }
        }

        if let Some((label, p)) = data.busy() {
            ui.separator();
            let frac = if p.bytes_total > 0 {
                p.bytes_done as f32 / p.bytes_total as f32
            } else if p.file_count > 0 {
                p.file_index as f32 / p.file_count as f32
            } else {
                0.0
            };
            ui.label(format!("{label}: {} {}", p.state, p.file));
            ui.add(egui::ProgressBar::new(frac).show_percentage().desired_width(ui.available_width().min(560.0)));
            if p.bytes_total > 0 {
                ui.label(format!(
                    "{:.2} of {:.2} GB",
                    p.bytes_done as f64 / 1e9,
                    p.bytes_total as f64 / 1e9
                ));
            }
            if label == "download" && ui.button("Pause").clicked() {
                data.pause();
            }
            hint(ui, "Keep iNilla open and the screen on; a paused or interrupted download resumes where it stopped.");
            return;
        }

        if let Some((ok, msg)) = &data.message {
            ui.colored_label(if *ok { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED }, msg);
        }

        // ── 1 · copy it in ──
        ui.add_space(6.0);
        ui.label(egui::RichText::new("1 · Copy your Data folder into iNilla").strong().color(GOLD));
        ui.label(
            "• From a Mac or PC: connect the iPhone, open Finder (Windows: Apple Devices / iTunes) → \
             your iPhone → Files → iNilla (in LiveContainer: LiveContainer, then the folder \
             below), and drag the client's Data folder in.",
        );
        ui.label(format!(
            "• On the iPhone: Files app → {} — paste the Data folder from iCloud Drive, a USB-C \
             drive or a server.",
            inilla_platform::paths::files_location()
        ));
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("Check for copied data").min_size(egui::vec2(220.0, 44.0))).clicked() {
                data.scan_import();
            }
            if ui.button("Check again after restart").on_hover_text("A folder copied in before launch is imported automatically").clicked() {
                restart(inilla);
            }
        });

        // ── 2 · download it ──
        ui.add_space(10.0);
        ui.label(egui::RichText::new("2 · Download it from your server").strong().color(GOLD));
        if overlay.data_url.is_empty() {
            // The server's own data URL, or where an iNilla server puts it: port 8099 beside the
            // realm the player already added.
            if let Some(url) = inilla.launcher.current_server().map(|s| match &s.data {
                Some(d) if !d.url.is_empty() => d.url.clone(),
                _ => format!("http://{}:8099/", s.host.trim()),
            }) {
                overlay.data_url = url;
            }
        }
        row(ui, "Data URL", |ui| {
            ui.add(egui::TextEdit::singleline(&mut overlay.data_url).hint_text("http://192.168.1.10:8099/").desired_width(360.0));
        });
        row(ui, "Access token", |ui| {
            ui.add(egui::TextEdit::singleline(&mut overlay.data_token).password(true).hint_text("if the host needs one").desired_width(360.0));
        });
        hint(
            ui,
            "An iNilla server serves this on port 8099 (github.com/Martze96/iNilla-server). The \
             token is only needed if the server set one. Only serve the game to yourself and \
             people who own it.",
        );
        let url = overlay.data_url.trim().to_string();
        let token = Some(overlay.data_token.clone());
        ui.horizontal(|ui| {
            if ui.add_enabled(!url.is_empty(), egui::Button::new("Check server").min_size(egui::vec2(160.0, 44.0))).clicked() {
                data.fetch_manifest(url.clone(), token.clone(), data.locale.clone());
            }
            if let Some(m) = &data.manifest {
                let free = data.free_bytes();
                let need = m.total_bytes().saturating_sub(data.status.total_bytes());
                let enough = free.is_none_or(|f| f > need + 200_000_000);
                let label = if data.status.present.is_empty() { "Download" } else { "Download / resume" };
                if ui
                    .add_enabled(enough, egui::Button::new(egui::RichText::new(label).color(GOLD)).min_size(egui::vec2(180.0, 44.0)))
                    .clicked()
                {
                    data.download(token.clone());
                }
                if let Some(f) = free {
                    ui.label(format!("{:.1} GB free", f as f64 / 1e9));
                    if !enough {
                        ui.colored_label(egui::Color32::LIGHT_RED, "not enough space");
                    }
                }
            }
        });

        // ── the language, when the server has game data in more than one ──
        if let Some(locales) = data.locales.clone() {
            let current = data.locale.clone().unwrap_or_else(|| locales.default.clone());
            row(ui, "Language", |ui| {
                for l in &locales.locales {
                    let name = inilla_core::addons::locale_name(&l.locale);
                    if ui.selectable_label(l.locale == current, name).clicked() && l.locale != current {
                        data.fetch_manifest(url.clone(), token.clone(), Some(l.locale.clone()));
                    }
                }
            });
            hint(
                ui,
                "The game data is one language at a time: switching downloads that language's \
                 archives (about 5 GB) in place of the current ones. Quests, items and NPCs follow \
                 it too.",
            );
        }

        // ── addons ──
        if let Some(addons) = data.addons.clone().filter(|a| !a.addons.is_empty()) {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("3 · Addons from your server").strong().color(GOLD));
            let installed = crate::inilla::data::addons_dir()
                .map(|d| inilla_core::addons::InstalledAddons::load(&d))
                .unwrap_or_default();
            for a in &addons.addons {
                let version = a.version.clone().unwrap_or_default();
                let state = match installed.addons.get(&a.name) {
                    Some(v) if *v == version => "installed",
                    Some(_) => "update available",
                    None => "not installed",
                };
                ui.label(format!(
                    "• {} {} — {:.1} MB, {state}",
                    a.name,
                    version,
                    a.bytes() as f64 / 1e6
                ));
            }
            hint(
                ui,
                "Installed into benilla-config/AddOns and kept current from here. Addons you copy \
                 in yourself are never touched.",
            );
            if ui
                .add(egui::Button::new("Install / update addons").min_size(egui::vec2(220.0, 44.0)))
                .clicked()
            {
                data.sync_addons(token.clone());
            }
        }
    });
}
