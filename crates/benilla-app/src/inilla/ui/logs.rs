//! **The session log on the device**: the last lines of everything the client printed (the log
//! ring the remote console also reads), filterable, copyable — for the moment something breaks
//! and there is no computer at hand.

use bevy_egui::egui;

use super::Overlay;

const SHOWN: usize = 1500;

pub(super) fn show(ui: &mut egui::Ui, overlay: &mut Overlay) {
    let lines = inilla_core::logring::global().tail(SHOWN * 4);
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.add(
            egui::TextEdit::singleline(&mut overlay.log_filter)
                .hint_text("warn, error, net:, inilla:")
                .desired_width(260.0),
        );
        ui.checkbox(&mut overlay.log_follow, "follow");
        if ui.button("Copy").clicked() {
            let text: String = filtered(&lines, &overlay.log_filter)
                .map(|(_, l)| format!("{l}\n"))
                .collect();
            inilla_platform::pasteboard::set(&text);
            overlay.toast("Log copied");
        }
    });
    let shown: Vec<&(u64, String)> = filtered(&lines, &overlay.log_filter).collect();
    let skip = shown.len().saturating_sub(SHOWN);
    egui::ScrollArea::vertical()
        .stick_to_bottom(overlay.log_follow)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (_, l) in shown.into_iter().skip(skip) {
                let color = if l.contains("ERROR") || l.contains("panicked") {
                    egui::Color32::LIGHT_RED
                } else if l.contains("WARN") {
                    egui::Color32::from_rgb(240, 200, 90)
                } else {
                    egui::Color32::from_gray(210)
                };
                ui.label(egui::RichText::new(l).monospace().size(12.0).color(color));
            }
        });
}

fn filtered<'a>(
    lines: &'a [(u64, String)],
    filter: &'a str,
) -> impl Iterator<Item = &'a (u64, String)> {
    let f = filter.trim().to_ascii_lowercase();
    lines
        .iter()
        .filter(move |(_, l)| f.is_empty() || l.to_ascii_lowercase().contains(&f))
}
