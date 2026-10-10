//! **Settings**: iNilla's own switches, grouped for a thumb. Game settings (graphics details,
//! sound volumes, key bindings, the interface options) stay in the stock options windows — the
//! Game menu is one tap away on the radial.

use bevy_egui::egui;

use benilla_world::build_id::BuildId;
use inilla_core::pad::{Layer, PadBinding, PadButton, PadProfile, Special};
use inilla_core::settings::Quality;
use inilla_core::touch::default_layout;

use super::{hint, row, Overlay, Panel, SettingsTab};
use crate::inilla::data::DataState;
use crate::inilla::hud::GOLD;
use crate::inilla::pad::PadHud;
use crate::inilla::Inilla;

pub(super) fn show(
    ui: &mut egui::Ui,
    overlay: &mut Overlay,
    inilla: &mut Inilla,
    data: &DataState,
    pad: &PadHud,
    build: &BuildId,
) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [
            (SettingsTab::General, "General"),
            (SettingsTab::Touch, "Touch"),
            (SettingsTab::Controller, "Controller"),
            (SettingsTab::Audio, "Audio"),
            (SettingsTab::Debug, "Debug"),
            (SettingsTab::About, "About"),
        ] {
            if ui
                .add(egui::Button::selectable(
                    overlay.tab == tab,
                    egui::RichText::new(label).size(18.0),
                ))
                .clicked()
            {
                overlay.tab = tab;
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| match overlay.tab {
        SettingsTab::General => general(ui, inilla),
        SettingsTab::Touch => touch(ui, overlay, inilla),
        SettingsTab::Controller => controller(ui, inilla, pad),
        SettingsTab::Audio => audio(ui, inilla),
        SettingsTab::Debug => debug(ui, overlay, inilla),
        SettingsTab::About => about(ui, inilla, data, build),
    });
}

fn general(ui: &mut egui::Ui, k: &mut Inilla) {
    let s = &mut k.settings;
    row(ui, "Graphics", |ui| {
        for q in Quality::ALL {
            ui.radio_value(&mut s.quality, q, q.label());
        }
    });
    hint(ui, "Battery saver caps at 30 fps with a short view; High runs at the display's full rate. Custom leaves the stock video options alone.");
    row(ui, "Render resolution", |ui| {
        ui.add(
            egui::Slider::new(&mut s.render_scale, 0.5..=1.0)
                .step_by(0.05)
                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
        );
    });
    hint(ui, "The 3D world renders at this fraction of the screen; the interface stays sharp. 80 % is a good phone default.");
    let mut auto = s.ui_scale == 0.0;
    row(ui, "Interface size", |ui| {
        if ui.checkbox(&mut auto, "Automatic").changed() {
            s.ui_scale = if auto { 0.0 } else { 1.25 };
        }
        if !auto {
            ui.add(egui::Slider::new(&mut s.ui_scale, 0.64..=1.5).step_by(0.01));
        }
    });
    hint(
        ui,
        "Automatic is 1.25 on a phone: action buttons near fingertip size, and quest and vendor windows still clear of the action bars.",
    );
    ui.checkbox(&mut s.keep_screen_on, "Keep the screen on while playing");
    ui.checkbox(
        &mut s.auto_keyboard,
        "Show the keyboard when a text box is focused",
    );
    ui.checkbox(&mut s.haptics, "Haptic feedback");
    ui.checkbox(
        &mut s.compact_stock_ui,
        "Compact stock interface (hide the action bar's gryphons)",
    );
    ui.checkbox(
        &mut s.island_margin,
        "Keep windows clear of the Dynamic Island",
    );
    hint(ui, "Game windows stop where the Dynamic Island (or the notch) begins, on whichever side it is; the world still fills the screen, and the menu buttons sit beside the island.");
}

fn touch(ui: &mut egui::Ui, overlay: &mut Overlay, k: &mut Inilla) {
    let l = &mut k.layout;
    ui.checkbox(&mut l.enabled, "Show the touch controls");
    ui.checkbox(
        &mut k.settings.hide_touch_with_controller,
        "Hide them while a controller is in use",
    );
    row(ui, "Size", |ui| {
        ui.add(egui::Slider::new(&mut l.hud_scale, 0.6..=1.6).step_by(0.05));
    });
    row(ui, "Opacity", |ui| {
        ui.add(egui::Slider::new(&mut l.opacity, 0.15..=1.0).step_by(0.05));
    });
    row(ui, "View", |ui| {
        let views = l.views.clone();
        for v in views {
            ui.radio_value(&mut l.active_view, v.clone(), v);
        }
    });
    hint(ui, "Views swap the buttons around the jump button: Combat shows action slots, Explore travel verbs, Social chat verbs, Minimal only the essentials. The view button (its icon shows the view) cycles them in play.");
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Edit layout…").color(GOLD))
                    .min_size(egui::vec2(160.0, 44.0)),
            )
            .clicked()
        {
            overlay.panel = Some(Panel::Editor);
        }
        if ui.button("Reset layout").clicked() {
            let enabled = l.enabled;
            *l = default_layout();
            l.enabled = enabled;
        }
    });
    ui.separator();
    ui.label(egui::RichText::new("Feel").strong());
    let f = &mut l.feel;
    row(ui, "Look sensitivity", |ui| {
        ui.add(egui::Slider::new(&mut f.look_sensitivity, 0.3..=4.0).step_by(0.1));
    });
    ui.checkbox(&mut f.invert_y, "Invert look up/down");
    ui.checkbox(
        &mut f.drag_turns_character,
        "Dragging turns the character with the camera",
    );
    row(ui, "Long press (right click)", |ui| {
        ui.add(egui::Slider::new(&mut f.long_press_ms, 250..=900).suffix(" ms"));
    });
    row(ui, "Double tap window", |ui| {
        ui.add(egui::Slider::new(&mut f.double_tap_ms, 150..=500).suffix(" ms"));
    });
    row(ui, "Stick size", |ui| {
        ui.add(egui::Slider::new(&mut f.stick_radius, 36.0..=90.0).suffix(" pt"));
    });
    row(ui, "Left stick sideways", |ui| {
        ui.radio_value(&mut f.stick_steers, false, "Strafe");
        ui.radio_value(&mut f.stick_steers, true, "Steer");
    });
    hint(ui, "Steer turns the character and the camera with it, like the A and D keys; strafe side-steps, like Q and E. Forward and back walk either way.");
    hint(ui, "Hold jump for the hub: slide to a view on the inner arc or a window on the outer one and let go. Hold any other button to see what it does; letting go then does nothing.");
    hint(ui, "On the world: tap = select, double tap or long press = interact/attack, drag = look, pinch = zoom. On a window: tap = click, long press = right click, drag = move items, two fingers = scroll.");
}

/// Every binding a controller button can take, for the picker.
fn choices() -> Vec<PadBinding> {
    let mut v = vec![
        PadBinding::None,
        PadBinding::Special(Special::Interact),
        PadBinding::Special(Special::RadialMenu),
        PadBinding::Special(Special::VirtualCursor),
        PadBinding::Special(Special::InillaMenu),
        PadBinding::Special(Special::ToggleTouchHud),
    ];
    v.extend(crate::inilla::synth::command_names().map(|(n, _)| PadBinding::cmd(n)));
    v
}

fn controller(ui: &mut egui::Ui, k: &mut Inilla, pad: &PadHud) {
    match &pad.name {
        Some(n) => ui.label(
            egui::RichText::new(format!("Connected: {n}")).color(egui::Color32::LIGHT_GREEN),
        ),
        None => ui.label(
            "No controller connected. Pair one in iOS Settings → Bluetooth, or attach a Backbone.",
        ),
    };
    let p = &mut k.pad;
    row(ui, "Stick deadzone", |ui| {
        ui.add(egui::Slider::new(&mut p.stick_deadzone, 0.05..=0.4).step_by(0.01));
    });
    row(ui, "Trigger threshold", |ui| {
        ui.add(egui::Slider::new(&mut p.trigger_threshold, 0.1..=0.9).step_by(0.05));
    });
    row(ui, "Turn speed", |ui| {
        ui.add(egui::Slider::new(&mut p.look_yaw_speed, 60.0..=480.0).suffix(" °/s"));
    });
    row(ui, "Look up/down speed", |ui| {
        ui.add(egui::Slider::new(&mut p.look_pitch_speed, 30.0..=300.0).suffix(" °/s"));
    });
    row(ui, "Response curve", |ui| {
        ui.add(egui::Slider::new(&mut p.look_curve, 1.0..=3.0).step_by(0.1));
    });
    ui.checkbox(&mut p.invert_y, "Invert look up/down");
    ui.checkbox(
        &mut k.settings.controller_hotkeys,
        "Show controller buttons on the action bars",
    );
    ui.checkbox(
        &mut k.settings.controller_bar,
        "Show the controller bar at the bottom of the screen",
    );
    hint(
        ui,
        "Each action button shows what presses it (X, LT A, RT ↑) while the controller is in use, \
         and holding a trigger lights up the buttons it switches to. LT uses the bottom-left bar, \
         RT the bottom-right, LT+RT the right bar.",
    );
    let p = &mut k.pad;
    row(ui, "Cursor speed", |ui| {
        ui.add(egui::Slider::new(&mut p.cursor_speed, 300.0..=2000.0).suffix(" pt/s"));
    });
    ui.checkbox(
        &mut p.swap_confirm,
        "Confirm with B / cancel with A (Nintendo layout)",
    );
    ui.checkbox(
        &mut p.auto_cursor,
        "Cursor mode opens with loot, vendor and quest windows",
    );
    ui.separator();
    ui.label(egui::RichText::new("Mapping").strong());
    hint(ui, "Hold LT, RT or both to switch the face buttons and d-pad to another layer — 32 bindings without leaving the sticks. By default LT is the bottom-left action bar, RT the bottom-right, LT+RT the right bar: drag spells onto those bars as on desktop.");
    let all = choices();
    let glyphs = pad.glyphs;
    egui::Grid::new("pad-map")
        .striped(true)
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label("");
            for l in Layer::ALL {
                ui.label(egui::RichText::new(l.label()).strong().color(GOLD));
            }
            ui.end_row();
            let buttons = [
                PadButton::A,
                PadButton::B,
                PadButton::X,
                PadButton::Y,
                PadButton::Up,
                PadButton::Right,
                PadButton::Down,
                PadButton::Left,
                PadButton::LB,
                PadButton::RB,
                PadButton::L3,
                PadButton::R3,
                PadButton::Menu,
                PadButton::Options,
            ];
            for b in buttons {
                ui.label(egui::RichText::new(b.glyph(glyphs)).size(18.0));
                for l in Layer::ALL {
                    let current = p.binding(l, b).clone();
                    let mut pick = current.clone();
                    egui::ComboBox::from_id_salt((
                        "padmap",
                        l.label(),
                        b.glyph(Default::default()),
                    ))
                    .selected_text(current.label())
                    .width(140.0)
                    .height(360.0)
                    .show_ui(ui, |ui| {
                        for c in &all {
                            ui.selectable_value(&mut pick, c.clone(), c.label());
                        }
                    });
                    if pick != current {
                        p.set_binding(l, b, pick);
                    }
                }
                ui.end_row();
            }
        });
    if ui.button("Reset controller settings").clicked() {
        *p = PadProfile::default();
    }
}

fn audio(ui: &mut egui::Ui, k: &mut Inilla) {
    let s = &mut k.settings;
    ui.checkbox(
        &mut s.mix_with_other_audio,
        "Let music and podcasts from other apps keep playing",
    );
    ui.checkbox(
        &mut s.ignore_silent_switch,
        "Play sound even when the silent switch is on",
    );
    hint(
        ui,
        "Volumes, music and ambience are in the stock Sound Options (Game menu → Sound & Voice).",
    );
}

fn debug(ui: &mut egui::Ui, overlay: &mut Overlay, k: &mut Inilla) {
    let d = &mut k.settings.debug;
    let was = d.remote_console;
    ui.checkbox(&mut d.remote_console, "Remote console on the local network");
    if d.remote_console && !was && d.console_token.is_empty() {
        d.console_token = crate::inilla::console::new_token();
    }
    if d.remote_console {
        let port = d.console_port;
        for ip in crate::inilla::console::local_addresses() {
            ui.label(
                egui::RichText::new(format!("http://{ip}:{port}/?token={}", d.console_token))
                    .monospace()
                    .color(GOLD),
            );
        }
        hint(ui, "Open that address in a browser on a computer on the same Wi-Fi: the live log, Lua, game commands, screenshots and the benilla-config files. Anyone with the address can drive the game — turn it off when you are done.");
        ui.horizontal(|ui| {
            if ui.button("Copy address").clicked() {
                if let Some(ip) = crate::inilla::console::local_addresses().first() {
                    inilla_platform::pasteboard::set(&format!(
                        "http://{ip}:{port}/?token={}",
                        d.console_token
                    ));
                    overlay.toast("Console address copied");
                }
            }
            if ui.button("New token").clicked() {
                d.console_token = crate::inilla::console::new_token();
            }
        });
    }
    ui.checkbox(
        &mut d.log_to_file,
        "Write a session log (Files → iNilla → benilla-config → Diagnostics)",
    );
    ui.checkbox(&mut d.perf_overlay, "Show frame rate, memory and heat");
    ui.checkbox(&mut d.show_touch_zones, "Show touch zones");
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new("View session log").min_size(egui::vec2(180.0, 44.0)))
            .clicked()
        {
            overlay.panel = Some(Panel::Logs);
        }
        if ui.button("Game data…").clicked() {
            overlay.panel = Some(Panel::Setup);
        }
    });
}

fn about(ui: &mut egui::Ui, k: &mut Inilla, data: &DataState, build: &BuildId) {
    ui.label(
        egui::RichText::new("iNilla — benilla for iPhone")
            .strong()
            .color(GOLD),
    );
    ui.label(format!(
        "Build {} ({}, {})",
        build.short, build.date, build.profile
    ));
    ui.label(inilla_platform::device::describe());
    ui.label(format!(
        "Game data: {} archives, {:.1} GB{}",
        data.status.present.len(),
        data.status.total_bytes() as f64 / 1e9,
        data.dir
            .as_ref()
            .map(|d| format!(" in {}", d.display()))
            .unwrap_or_default()
    ));
    hint(ui, "benilla is a from-scratch World of Warcraft 1.12.1 client in Rust (github.com/samwhosung/benilla); iNilla is its iOS port. Neither ships Blizzard content, nor is affiliated with Blizzard Entertainment.");
    ui.add_space(12.0);
    if ui
        .button(egui::RichText::new("Reset all iNilla settings").color(egui::Color32::LIGHT_RED))
        .clicked()
    {
        k.settings = Default::default();
        k.settings.setup_done = true;
        k.layout = default_layout();
        k.pad = PadProfile::default();
    }
}
