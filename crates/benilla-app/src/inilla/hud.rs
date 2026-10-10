//! **Drawing the HUD** (egui painter, on the overlay lane): the touch elements, the floating
//! stick, the radial menu, the controller's layer bar and virtual cursor, the glue-screen
//! buttons, the perf overlay and toasts.
//!
//! Everything the HUD paints for show goes on egui's *background* order, which egui does not
//! treat as an interactive area — so a finger on a HUD button is the touch engine's, not egui's
//! (`touch::process` asks `layer_id_at` and skips only non-background layers). The few real egui
//! widgets here (the glue screens' keyboard and menu buttons) are ordinary foreground areas.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use std::collections::HashMap;

use inilla_core::pad::{Layer, PadBinding, PadButton, Special, RADIAL_ITEMS};
use inilla_core::touch::{ElementKind, HubItem, Screen, TouchHub};

use super::hints::ChatLine;
use super::icons::{HudTooltip, Icons};
use super::pad::PadHud;
use super::platform::{KeyboardRequest, SafeArea, SyntheticKeys};
use super::touch::{hud_visible, TouchState};
use super::ui::Overlay;
use super::{EguiWanted, Inilla};
use crate::char_select::ClientState;

pub(crate) const GOLD: egui::Color32 = egui::Color32::from_rgb(212, 175, 55);
const INK: egui::Color32 = egui::Color32::from_rgb(12, 12, 16);

fn alpha(c: egui::Color32, a: f32) -> egui::Color32 {
    let [r, g, b, _] = c.to_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub(crate) struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, want_egui.before(crate::ui_script::UiInput))
            .add_systems(EguiPrimaryContextPass, draw.run_if(super::lane_up));
    }
}

/// Keep the egui lane running whenever iNilla has something to show.
fn want_egui(
    mut wanted: ResMut<EguiWanted>,
    inilla: Res<Inilla>,
    overlay: Res<Overlay>,
    pad: Res<PadHud>,
) {
    let want = super::touch_device()
        || overlay.panel.is_some()
        || overlay.toast.is_some()
        || pad.frame.cursor_mode
        || pad.frame.radial.is_some()
        || inilla.settings.debug.perf_overlay;
    if wanted.0 != want {
        wanted.0 = want;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut contexts: EguiContexts,
    inilla: Res<Inilla>,
    touch: Res<TouchState>,
    pad: Res<PadHud>,
    mut icons: ResMut<Icons>,
    server: Res<AssetServer>,
    mut overlay: ResMut<Overlay>,
    mut keyboard: ResMut<KeyboardRequest>,
    safe: Res<SafeArea>,
    client: Res<State<ClientState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time<Real>>,
    chat: Res<ChatLine>,
    mut keys: ResMut<SyntheticKeys>,
    tooltip: Res<HudTooltip>,
    mut fps: Local<f32>,
) -> Result {
    let Ok(window) = windows.single() else {
        return Ok(());
    };
    let now = time.elapsed_secs_f64();
    let screen = safe.screen(window.width(), window.height());
    // Textures first: `icons.texture` needs the contexts mutably, the painter borrows one.
    let mut slot_textures: Vec<(u32, egui::TextureId)> = Vec::new();
    for (slot, info) in icons.slots.clone() {
        if let Some(path) = &info.texture {
            if let Some(id) = icons.texture(path, &server, &mut contexts) {
                slot_textures.push((slot, id));
            }
        }
    }
    // The game's own icons for the HUD's buttons and the radial menu.
    let mut icon_textures: HashMap<&'static str, egui::TextureId> = HashMap::new();
    for path in ICON_PATHS {
        if let Some(id) = icons.texture(path, &server, &mut contexts) {
            icon_textures.insert(path, id);
        }
    }
    let icon_for = |b: &PadBinding| binding_icon(b).and_then(|p| icon_textures.get(p).copied());
    let ctx = contexts.ctx_mut()?.clone();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("inilla-hud"),
    ));
    let pad_active = pad.active(now) && pad.last_active > touch.last_touch;
    let in_world = *client.get() == ClientState::InWorld;

    // ── the touch HUD ──
    if hud_visible(&inilla, client.get(), &overlay, pad_active) {
        let layout = &inilla.layout;
        let base = layout.opacity;
        for (i, el) in layout.shown() {
            let r = el.rect(&screen, layout.hud_scale);
            let rect = egui::Rect::from_min_max(
                egui::pos2(r.min[0], r.min[1]),
                egui::pos2(r.max[0], r.max[1]),
            );
            let pressed = touch.frame.pressed_elements.contains(&i);
            let a = base * el.opacity * if pressed { 1.6 } else { 1.0 };
            match &el.kind {
                ElementKind::Stick => {
                    if inilla.settings.debug.show_touch_zones {
                        painter.rect_stroke(
                            rect,
                            8.0,
                            egui::Stroke::new(1.0_f32, alpha(GOLD, 0.35)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if touch.frame.stick.is_none() {
                        // A faint hint where the thumb usually lands.
                        let c = rect.center();
                        painter.circle_stroke(
                            c,
                            layout.feel.stick_radius * layout.hud_scale,
                            egui::Stroke::new(2.0_f32, alpha(egui::Color32::WHITE, a * 0.35)),
                        );
                    }
                }
                ElementKind::ActionSlot(slot) => {
                    let tex = slot_textures
                        .iter()
                        .find(|(s, _)| s == slot)
                        .map(|(_, t)| *t);
                    let badge = slot_badge(&icons, *slot);
                    draw_slot(
                        &painter,
                        rect,
                        a,
                        tex,
                        &icons,
                        *slot,
                        now,
                        pressed,
                        Some(&badge),
                    );
                }
                ElementKind::NextView => {
                    draw_view_button(&painter, rect, a, &layout.active_view, pressed);
                }
                _ => {
                    let label = el
                        .label
                        .clone()
                        .or_else(|| el.binding().map(|b| b.label()))
                        .unwrap_or_default();
                    let tex = el
                        .binding()
                        .and_then(|b| b.action_slot())
                        .and_then(|s| slot_textures.iter().find(|(x, _)| *x == s).map(|(_, t)| *t));
                    if let Some(tex) = tex {
                        let slot = el.binding().and_then(|b| b.action_slot()).unwrap_or(0);
                        let badge = slot_badge(&icons, slot);
                        draw_slot(
                            &painter,
                            rect,
                            a,
                            Some(tex),
                            &icons,
                            slot,
                            now,
                            pressed,
                            Some(&badge),
                        );
                    } else if let Some(tex) = el.binding().and_then(|b| icon_for(&b)) {
                        draw_icon_button(&painter, rect, a, tex, pressed);
                    } else if matches!(el.binding(), Some(PadBinding::Command(c)) if c == "JUMP") {
                        draw_jump(&painter, rect, a, pressed);
                    } else {
                        draw_button(&painter, rect, a, &label, pressed);
                    }
                }
            }
        }
        if let Some(s) = touch.frame.stick {
            let o = egui::pos2(s.origin[0], s.origin[1]);
            let k = egui::pos2(s.knob[0], s.knob[1]);
            let rad = layout.feel.stick_radius * layout.hud_scale;
            painter.circle(
                o,
                rad,
                alpha(INK, base * 0.6),
                egui::Stroke::new(2.0_f32, alpha(GOLD, base)),
            );
            painter.circle_filled(
                k,
                rad * 0.42,
                alpha(egui::Color32::WHITE, (base * 1.4).min(0.9)),
            );
        }
        // A held button's words, when the game's own tooltip does not cover it.
        if let Some((i, text)) = &tooltip.text {
            if let Some(el) = layout.elements.get(*i) {
                let r = el.rect(&screen, layout.hud_scale);
                let anchor = egui::Rect::from_min_max(
                    egui::pos2(r.min[0], r.min[1]),
                    egui::pos2(r.max[0], r.max[1]),
                );
                draw_tip(&painter, anchor, text, &screen);
            }
        }
        if let Some(hub) = &touch.frame.hub {
            draw_hub(&painter, hub, &layout.active_view, &icon_textures);
        }
    }

    // ── the radial (touch or controller) ──
    if let Some(r) = touch.frame.radial {
        draw_radial(
            &painter,
            egui::pos2(r.center[0], r.center[1]),
            r.radius,
            r.selected,
            &icon_textures,
        );
    } else if let Some(r) = pad.frame.radial {
        let c = egui::pos2(screen.size[0] / 2.0, screen.size[1] / 2.0);
        draw_radial(
            &painter,
            c,
            (screen.size[1] * 0.36).max(110.0),
            r.selected,
            &icon_textures,
        );
    }

    // ── the controller bar ──
    if pad_active && in_world && !overlay.blocks_game() && inilla.settings.controller_bar {
        draw_pad_bar(
            &painter,
            &inilla,
            &pad,
            &screen,
            &slot_textures,
            &icons,
            &icon_for,
            now,
        );
    }
    if let Some(c) = pad.cursor {
        draw_cursor(&painter, egui::pos2(c.x, c.y));
    }

    // ── glue screens: the keyboard and iNilla's menu ──
    if super::touch_device() && !in_world && overlay.panel.is_none() {
        egui::Area::new(egui::Id::new("inilla-glue-buttons"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                egui::vec2(-12.0 - safe.0[3], 12.0 + safe.0[0]),
            )
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    let kb = if keyboard.0 {
                        "⌨ Hide keyboard"
                    } else {
                        "⌨ Keyboard"
                    };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(kb).size(16.0))
                                .min_size(egui::vec2(44.0, 40.0)),
                        )
                        .clicked()
                    {
                        keyboard.0 = !keyboard.0;
                    }
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("iNilla").size(16.0).color(GOLD))
                                .min_size(egui::vec2(44.0, 40.0)),
                        )
                        .clicked()
                    {
                        overlay.open_launcher();
                    }
                });
            });
    } else if in_world && keyboard.0 {
        keyboard.0 = false;
    }

    // ── the chat bar: the line being typed, above the iOS keyboard that covers the edit box ──
    if in_world && chat.typing {
        chat_bar(&ctx, &chat, &mut keys, &screen, &safe);
    }

    // ── perf overlay ──
    if inilla.settings.debug.perf_overlay {
        let dt = time.delta_secs().max(1e-4);
        *fps = if *fps == 0.0 {
            1.0 / dt
        } else {
            *fps * 0.95 + (1.0 / dt) * 0.05
        };
        let fps = *fps;
        let mem = inilla_platform::device::available_memory()
            .map(|b| format!(" · {:.0} MB free", b as f64 / 1_048_576.0))
            .unwrap_or_default();
        let thermal = match inilla_platform::device::thermal_state() {
            inilla_platform::device::Thermal::Nominal => "",
            inilla_platform::device::Thermal::Fair => " · warm",
            inilla_platform::device::Thermal::Serious => " · HOT",
            inilla_platform::device::Thermal::Critical => " · THROTTLING",
        };
        painter.text(
            egui::pos2(screen.size[0] / 2.0, 4.0 + safe.0[0]),
            egui::Align2::CENTER_TOP,
            format!("{fps:.0} fps{mem}{thermal}"),
            egui::FontId::monospace(12.0),
            egui::Color32::from_rgb(120, 255, 120),
        );
    }

    // ── toast ──
    if let Some((text, until)) = overlay.toast.clone() {
        if now > until {
            overlay.toast = None;
        } else {
            let pos = egui::pos2(screen.size[0] / 2.0, screen.size[1] * 0.18);
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(16.0),
                egui::Color32::WHITE,
            );
            let rect = egui::Rect::from_center_size(pos, galley.size() + egui::vec2(24.0, 14.0));
            painter.rect_filled(rect, 8.0, alpha(INK, 0.85));
            painter.galley(
                rect.center() - galley.size() / 2.0,
                galley,
                egui::Color32::WHITE,
            );
        }
    }
    Ok(())
}

fn draw_button(p: &egui::Painter, rect: egui::Rect, a: f32, label: &str, pressed: bool) {
    let r = rect.width().min(rect.height()) / 2.0;
    p.circle(
        rect.center(),
        r,
        alpha(INK, (a * 1.1).min(0.85)),
        egui::Stroke::new(
            if pressed { 3.0_f32 } else { 1.5_f32 },
            alpha(GOLD, (a * 1.5).min(1.0)),
        ),
    );
    let size = (r * 0.55).clamp(10.0, 18.0);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(size),
        alpha(egui::Color32::WHITE, (a * 1.8).min(1.0)),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_slot(
    p: &egui::Painter,
    rect: egui::Rect,
    a: f32,
    tex: Option<egui::TextureId>,
    icons: &Icons,
    slot: u32,
    now: f64,
    pressed: bool,
    // What the button is labelled with (see [`slot_badge`]): a corner chip over an icon, the
    // whole face of an empty slot.
    badge: Option<&str>,
) {
    let info = icons.slots.get(&slot);
    let rounding = 8.0;
    p.rect_filled(rect, rounding, alpha(INK, (a * 1.1).min(0.85)));
    if let Some(tex) = tex {
        let usable = info.is_none_or(|i| i.usable);
        let tint = if usable {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(110, 110, 160)
        };
        // Cropped like the stock action button's own tex coords, past the icon's baked border.
        let uv = egui::Rect::from_min_max(egui::pos2(0.07, 0.07), egui::pos2(0.93, 0.93));
        p.image(tex, rect.shrink(2.0), uv, alpha(tint, (a * 2.2).min(1.0)));
    }
    match (badge.filter(|b| !b.is_empty()), tex) {
        (Some(badge), Some(_)) => {
            let galley =
                p.layout_no_wrap(badge.to_string(), egui::FontId::proportional(11.0), GOLD);
            let chip = egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(3.0, 3.0),
                egui::vec2(galley.size().x.max(9.0) + 6.0, galley.size().y + 1.0),
            );
            p.rect_filled(chip, 4.0, alpha(INK, (a * 1.5).min(0.85)));
            p.galley(chip.center() - galley.size() / 2.0, galley, GOLD);
        }
        (Some(badge), None) => {
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                badge,
                egui::FontId::proportional((rect.height() * 0.36).clamp(12.0, 20.0)),
                alpha(egui::Color32::WHITE, (a * 1.2).min(0.6)),
            );
        }
        _ => {}
    }
    let left = icons.remaining(slot, now);
    if left > 0.0 {
        p.rect_filled(rect, rounding, egui::Color32::from_black_alpha(140));
        let text = if left >= 60.0 {
            format!("{}m", (left / 60.0).ceil())
        } else {
            format!("{}", left.ceil())
        };
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(255, 230, 120),
        );
    }
    if let Some(i) = info.filter(|i| i.count > 1) {
        p.text(
            rect.right_bottom() - egui::vec2(4.0, 2.0),
            egui::Align2::RIGHT_BOTTOM,
            i.count.to_string(),
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
    p.rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(
            if pressed { 3.0_f32 } else { 1.5_f32 },
            alpha(GOLD, (a * 1.6).min(1.0)),
        ),
        egui::StrokeKind::Inside,
    );
}

/// What a HUD action button is labelled with: the hotkey text the stock button it presses shows
/// (`1`), so the two read as one — else that button's number on its bar (`3`, `BL3`).
fn slot_badge(icons: &Icons, slot: u32) -> String {
    match icons.slots.get(&slot).map(|i| i.hotkey.trim()) {
        Some(hotkey) if !hotkey.is_empty() => hotkey.to_string(),
        _ => inilla_core::touch::slot_binding(slot)
            .action_button()
            .map_or_else(
                || slot.to_string(),
                |(bar, n)| format!("{}{n}", bar.short()),
            ),
    }
}

/// The view button: a picture of the view it is in — crossed swords, a compass, a speech bubble,
/// a closed eye — drawn, so it reads in any font. A view of the player's own naming shows its
/// initial.
fn draw_view_button(p: &egui::Painter, rect: egui::Rect, a: f32, view: &str, pressed: bool) {
    draw_button(p, rect, a, "", pressed);
    let c = rect.center();
    let w = rect.width().min(rect.height()) * 0.24;
    let ink = alpha(egui::Color32::WHITE, (a * 1.8).min(1.0));
    let stroke = egui::Stroke::new((w * 0.16).max(1.8), ink);
    let at = |x: f32, y: f32| c + egui::vec2(x * w, y * w);
    match view {
        "Combat" => {
            // Two blades crossing, hilts at the bottom.
            for s in [-1.0_f32, 1.0] {
                p.line_segment([at(-0.9 * s, 0.9), at(0.85 * s, -0.85)], stroke);
                p.line_segment([at(-0.9 * s, 0.35), at(-0.35 * s, 0.9)], stroke);
            }
        }
        "Explore" => {
            p.circle_stroke(c, w, stroke);
            let (n, e, s, west) = (at(0.0, -0.8), at(0.3, 0.0), at(0.0, 0.8), at(-0.3, 0.0));
            p.add(egui::Shape::convex_polygon(
                vec![n, e, west],
                alpha(GOLD, (a * 2.0).min(1.0)),
                egui::Stroke::NONE,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![s, west, e],
                ink,
                egui::Stroke::NONE,
            ));
        }
        "Social" => {
            let bubble = egui::Rect::from_min_max(at(-1.0, -0.75), at(1.0, 0.45));
            p.rect_stroke(bubble, w * 0.35, stroke, egui::StrokeKind::Middle);
            p.add(egui::Shape::convex_polygon(
                vec![at(-0.55, 0.45), at(-0.1, 0.45), at(-0.65, 0.95)],
                ink,
                egui::Stroke::NONE,
            ));
            for x in [-0.45, 0.0, 0.45] {
                p.circle_filled(at(x, -0.15), w * 0.11, ink);
            }
        }
        "Minimal" => {
            // An eye, struck through: the HUD out of sight.
            let lens: Vec<egui::Pos2> = (0..=24)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 24.0;
                    at(t.cos(), 0.55 * t.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(lens, stroke));
            p.circle_filled(c, w * 0.25, ink);
            p.line_segment([at(-0.95, 0.8), at(0.95, -0.8)], stroke);
        }
        other => {
            let initial: String = other.chars().take(1).collect();
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                initial,
                egui::FontId::proportional((w * 1.5).clamp(12.0, 20.0)),
                ink,
            );
        }
    }
}

fn draw_radial(
    p: &egui::Painter,
    c: egui::Pos2,
    radius: f32,
    selected: Option<usize>,
    icons: &HashMap<&'static str, egui::TextureId>,
) {
    let n = RADIAL_ITEMS.len();
    p.circle_filled(c, radius * 1.3, egui::Color32::from_black_alpha(150));
    p.circle_stroke(
        c,
        radius * 0.3,
        egui::Stroke::new(1.5_f32, alpha(GOLD, 0.6)),
    );
    for (i, (label, cmd)) in RADIAL_ITEMS.iter().enumerate() {
        // Wedge 0 is straight up, clockwise.
        let ang = (i as f32 / n as f32) * std::f32::consts::TAU;
        let dir = egui::vec2(ang.sin(), -ang.cos());
        let at = c + dir * radius;
        let sel = selected == Some(i);
        let r = if sel { 34.0 } else { 28.0 };
        p.circle(
            at,
            r,
            if sel {
                alpha(GOLD, 0.9)
            } else {
                alpha(INK, 0.85)
            },
            egui::Stroke::new(1.5_f32, alpha(GOLD, 0.9)),
        );
        match command_icon(cmd).and_then(|path| icons.get(path).copied()) {
            Some(tex) => {
                let side = r * 1.1;
                let rect = egui::Rect::from_center_size(at, egui::vec2(side, side));
                p.image(tex, rect, icon_uv(), egui::Color32::WHITE);
                // The name under the icon, so a new player learns what the picture means.
                let galley = p.layout_no_wrap(
                    (*label).to_string(),
                    egui::FontId::proportional(if sel { 13.0 } else { 11.0 }),
                    if sel { GOLD } else { egui::Color32::WHITE },
                );
                let top = at + egui::vec2(0.0, r + 3.0);
                let bg = egui::Rect::from_min_size(
                    egui::pos2(top.x - galley.size().x / 2.0 - 4.0, top.y - 1.0),
                    galley.size() + egui::vec2(8.0, 2.0),
                );
                p.rect_filled(bg, 4.0, alpha(INK, 0.8));
                p.galley(
                    egui::pos2(top.x - galley.size().x / 2.0, top.y),
                    galley,
                    egui::Color32::WHITE,
                );
            }
            None => {
                p.text(
                    at,
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(if sel { 13.0 } else { 11.0 }),
                    if sel { INK } else { egui::Color32::WHITE },
                );
            }
        }
    }
}

/// The hub: the views on the inner arc (their pictures), the windows on the outer one (the game's
/// icons) and "more" at its end, the entry under the finger enlarged and named beyond the arcs.
fn draw_hub(
    p: &egui::Painter,
    hub: &TouchHub,
    active: &str,
    icons: &HashMap<&'static str, egui::TextureId>,
) {
    let c = egui::pos2(hub.center[0], hub.center[1]);
    p.circle_filled(c, hub.radii[1] + 32.0, egui::Color32::from_black_alpha(120));
    for (ring, items) in hub.rings.iter().enumerate() {
        for (i, item) in items.iter().enumerate() {
            let [x, y] = hub.pos(ring, i);
            let at = egui::pos2(x, y);
            let sel = hub.selected == Some((ring, i));
            let r = if sel { 27.0 } else { 21.0 };
            let rect = egui::Rect::from_center_size(at, egui::vec2(r * 2.0, r * 2.0));
            let a = if sel { 1.0 } else { 0.7 };
            match item {
                HubItem::View(v) => {
                    draw_view_button(p, rect, a, v, sel);
                    if v == active {
                        p.circle_stroke(at, r + 4.0, egui::Stroke::new(2.0_f32, GOLD));
                    }
                }
                HubItem::Window(w) => {
                    let (label, cmd) = RADIAL_ITEMS[*w];
                    match command_icon(cmd).and_then(|path| icons.get(path).copied()) {
                        Some(tex) => draw_icon_button(p, rect, a, tex, sel),
                        None => draw_button(p, rect, a, label, sel),
                    }
                }
                HubItem::More => {
                    draw_button(p, rect, a, "", sel);
                    for dx in [-1.0_f32, 0.0, 1.0] {
                        p.circle_filled(
                            at + egui::vec2(dx * r * 0.36, 0.0),
                            r * 0.1,
                            egui::Color32::WHITE,
                        );
                    }
                }
            }
        }
    }
    if let Some(item) = hub.selected.and_then(|s| hub.item(s)) {
        let name = match item {
            HubItem::View(v) => v.clone(),
            HubItem::Window(w) => RADIAL_ITEMS[*w].0.to_string(),
            HubItem::More => "All windows".to_string(),
        };
        let mid = hub.start + hub.sweep / 2.0;
        let at = c + egui::vec2(mid.cos(), mid.sin()) * (hub.radii[1] + 58.0);
        let galley = p.layout_no_wrap(name, egui::FontId::proportional(16.0), GOLD);
        let bg = egui::Rect::from_center_size(at, galley.size() + egui::vec2(20.0, 10.0));
        p.rect_filled(bg, 8.0, alpha(INK, 0.9));
        p.galley(bg.center() - galley.size() / 2.0, galley, GOLD);
    }
}

/// A held HUD button's words, in a bubble above it (below it when there is no room above).
fn draw_tip(p: &egui::Painter, anchor: egui::Rect, text: &str, screen: &Screen) {
    let galley = p.layout(
        text.to_string(),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
        260.0,
    );
    let size = galley.size() + egui::vec2(20.0, 12.0);
    let mut min = egui::pos2(
        anchor.center().x - size.x / 2.0,
        anchor.min.y - 10.0 - size.y,
    );
    if min.y < 4.0 {
        min.y = anchor.max.y + 10.0;
    }
    min.x = min.x.clamp(4.0, (screen.size[0] - size.x - 4.0).max(4.0));
    let rect = egui::Rect::from_min_size(min, size);
    p.rect_filled(rect, 8.0, alpha(INK, 0.92));
    p.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0_f32, alpha(GOLD, 0.8)),
        egui::StrokeKind::Inside,
    );
    p.galley(
        rect.min + egui::vec2(10.0, 6.0),
        galley,
        egui::Color32::WHITE,
    );
}

/// An icon's texture coordinates, cropped like the stock action button's, past the baked border.
fn icon_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.07, 0.07), egui::pos2(0.93, 0.93))
}

/// Every icon the HUD may draw, loaded up front from the player's archives.
const ICON_PATHS: [&str; 20] = [
    r"Interface\Icons\Ability_Rogue_Sprint",
    r"Interface\Icons\INV_Misc_Map_01",
    r"Interface\Icons\INV_Misc_Bag_08",
    r"Interface\Icons\Spell_Nature_Sleep",
    r"Interface\Icons\INV_Misc_Book_08",
    r"Interface\Icons\INV_Misc_Book_09",
    r"Interface\Icons\INV_Misc_Note_01",
    r"Interface\Icons\INV_Letter_03",
    r"Interface\Icons\Spell_Holy_PrayerOfFortitude",
    r"Interface\Icons\Ability_Tracking",
    r"Interface\Icons\Ability_Hunter_SniperShot",
    r"Interface\Icons\INV_Gauntlets_04",
    r"Interface\Icons\INV_Chest_Cloth_17",
    r"Interface\Icons\Ability_Marksmanship",
    r"Interface\Icons\INV_Banner_02",
    r"Interface\Icons\Trade_Engineering",
    r"Interface\Icons\Ability_Hunter_BeastCall",
    r"Interface\Icons\INV_Misc_Gear_01",
    r"Interface\Icons\Ability_Hunter_Pathfinding",
    r"Interface\Icons\Spell_Holy_MagicalSentry",
];

/// The game icon for a binding command, where the stock interface has an obvious picture for it.
fn command_icon(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "TOGGLEAUTORUN" => ICON_PATHS[0],
        "TOGGLEWORLDMAP" => ICON_PATHS[1],
        "OPENALLBAGS" | "TOGGLEBACKPACK" => ICON_PATHS[2],
        "SITORSTAND" => ICON_PATHS[3],
        "TOGGLEQUESTLOG" => ICON_PATHS[4],
        "TOGGLESPELLBOOK" => ICON_PATHS[5],
        "OPENCHAT" => ICON_PATHS[6],
        "REPLY" => ICON_PATHS[7],
        "TOGGLESOCIAL" => ICON_PATHS[8],
        "FOLLOWTARGET" => ICON_PATHS[9],
        "TARGETNEARESTENEMY" | "TARGETPREVIOUSENEMY" => ICON_PATHS[10],
        "TOGGLECHARACTER0" => ICON_PATHS[12],
        "TOGGLETALENTS" => ICON_PATHS[13],
        "TOGGLECHARACTER2" => ICON_PATHS[14],
        "TOGGLECHARACTER1" => ICON_PATHS[15],
        "TOGGLECHARACTER3" => ICON_PATHS[16],
        "TOGGLEGAMEMENU" => ICON_PATHS[17],
        "TARGETNEARESTFRIEND" | "TARGETPREVIOUSFRIEND" => ICON_PATHS[18],
        "ASSISTTARGET" => ICON_PATHS[19],
        _ => return None,
    })
}

/// The game icon for a HUD button, when what it does has one.
fn binding_icon(b: &PadBinding) -> Option<&'static str> {
    match b {
        PadBinding::Command(c) => command_icon(c),
        PadBinding::Special(Special::Interact) => Some(ICON_PATHS[11]),
        _ => None,
    }
}

/// A button whose job is a picture: the icon in a round frame, like the action slots.
fn draw_icon_button(
    p: &egui::Painter,
    rect: egui::Rect,
    a: f32,
    tex: egui::TextureId,
    pressed: bool,
) {
    let r = rect.width().min(rect.height()) / 2.0;
    p.circle_filled(rect.center(), r, alpha(INK, (a * 1.1).min(0.85)));
    let side = r * 1.3;
    let img = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
    p.image(
        tex,
        img,
        icon_uv(),
        alpha(egui::Color32::WHITE, (a * 2.2).min(1.0)),
    );
    p.circle_stroke(
        rect.center(),
        r,
        egui::Stroke::new(
            if pressed { 3.0_f32 } else { 1.5_f32 },
            alpha(GOLD, (a * 1.5).min(1.0)),
        ),
    );
}

/// Jump: an upward double chevron, drawn, so it reads at any size and in any font.
fn draw_jump(p: &egui::Painter, rect: egui::Rect, a: f32, pressed: bool) {
    draw_button(p, rect, a, "", pressed);
    let c = rect.center();
    let w = rect.width().min(rect.height()) * 0.2;
    let stroke = egui::Stroke::new(
        (w * 0.3).max(2.5),
        alpha(egui::Color32::WHITE, (a * 1.8).min(1.0)),
    );
    for dy in [-w * 0.45, w * 0.45] {
        let tip = c + egui::vec2(0.0, dy - w * 0.45);
        p.line_segment([c + egui::vec2(-w, dy + w * 0.45), tip], stroke);
        p.line_segment([tip, c + egui::vec2(w, dy + w * 0.45)], stroke);
    }
}

/// `|cffffd000[Linen Cloth]|r` → `[Linen Cloth]`: the stock escape codes, which egui would print.
fn strip_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '|' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('c') => {
                for _ in 0..8 {
                    chars.next();
                }
            }
            Some('H') => {
                // A hyperlink's payload runs to the next `|h`.
                while let Some(d) = chars.next() {
                    if d == '|' && chars.peek() == Some(&'h') {
                        chars.next();
                        break;
                    }
                }
            }
            Some('r') | Some('h') => {}
            Some('|') => out.push('|'),
            Some(other) => {
                out.push('|');
                out.push(other);
            }
            None => out.push('|'),
        }
    }
    out
}

/// The line being typed, with Send and Close. In landscape the iOS keyboard covers the stock edit
/// box and has no Escape key, so without this a player can neither see what they type nor leave
/// the chat box without sending.
fn chat_bar(
    ctx: &egui::Context,
    chat: &ChatLine,
    keys: &mut SyntheticKeys,
    screen: &Screen,
    safe: &SafeArea,
) {
    let width = (screen.size[0] - safe.0[1] - safe.0[3] - 24.0).clamp(240.0, 760.0);
    egui::Area::new(egui::Id::new("inilla-chat-bar"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 6.0 + safe.0[0]))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(alpha(INK, 0.92))
                .stroke(egui::Stroke::new(1.5_f32, alpha(GOLD, 0.9)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close = ui.add(
                            egui::Button::new(egui::RichText::new("× Close").size(16.0))
                                .min_size(egui::vec2(96.0, 40.0)),
                        );
                        if close.clicked() {
                            keys.tap(KeyCode::Escape);
                        }
                        let send = ui.add(
                            egui::Button::new(egui::RichText::new("Send").size(16.0).color(GOLD))
                                .min_size(egui::vec2(96.0, 40.0)),
                        );
                        if send.clicked() {
                            keys.tap(KeyCode::Enter);
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if !chat.chat {
                                ui.label(
                                    egui::RichText::new("Typing…")
                                        .size(16.0)
                                        .color(egui::Color32::WHITE),
                                );
                                return;
                            }
                            let header = strip_codes(&chat.header);
                            if !header.is_empty() {
                                ui.label(egui::RichText::new(header).size(16.0).color(GOLD));
                            }
                            let text = strip_codes(&chat.text);
                            let shown = if text.is_empty() {
                                "…".to_string()
                            } else {
                                text
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(shown)
                                        .size(16.0)
                                        .color(egui::Color32::WHITE),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
        });
}

fn draw_cursor(p: &egui::Painter, at: egui::Pos2) {
    let pts = vec![
        at,
        at + egui::vec2(0.0, 20.0),
        at + egui::vec2(5.5, 15.0),
        at + egui::vec2(10.0, 23.0),
        at + egui::vec2(13.0, 21.5),
        at + egui::vec2(9.0, 14.0),
        at + egui::vec2(15.0, 14.0),
    ];
    p.add(egui::Shape::convex_polygon(
        pts,
        GOLD,
        egui::Stroke::new(1.5_f32, INK),
    ));
}

/// The controller's bar: the active layer's eight buttons, with the icon (or name) each one runs,
/// what to hold for the layer and which stock bar it presses, and a one-line guide to the rest.
#[allow(clippy::too_many_arguments)]
fn draw_pad_bar(
    p: &egui::Painter,
    inilla: &Inilla,
    pad: &PadHud,
    screen: &Screen,
    textures: &[(u32, egui::TextureId)],
    icons: &Icons,
    icon_for: &dyn Fn(&PadBinding) -> Option<egui::TextureId>,
    now: f64,
) {
    let layer = pad.frame.layer;
    let style = pad.glyphs;
    let size = 46.0;
    let gap = 6.0;
    let buttons = PadButton::LAYERED;
    let total = buttons.len() as f32 * (size + gap) - gap;
    let y = screen.size[1] - screen.safe[2] - size - 10.0;
    let mut x = (screen.size[0] - total) / 2.0;
    // What is held, and the stock bar this layer's buttons press (`LT · bottom-left bar`).
    let bar = buttons.iter().find_map(|b| {
        inilla
            .pad
            .binding(layer, *b)
            .action_button()
            .map(|(bar, _)| bar)
    });
    let (caption, caption_size, caption_color) = match (layer, bar) {
        (Layer::Base, _) if pad.frame.cursor_mode => (
            format!(
                "{} click · {} right-click · {} leave cursor",
                PadButton::A.glyph(style),
                PadButton::B.glyph(style),
                PadButton::R3.glyph(style)
            ),
            12.0,
            alpha(egui::Color32::WHITE, 0.85),
        ),
        // The base layer: what the rest of the controller does, for a player still learning it.
        (Layer::Base, _) => (
            format!(
                "{}/{} target · hold {} or {} for more bars · {} menu · {} cursor",
                PadButton::LB.glyph(style),
                PadButton::RB.glyph(style),
                PadButton::LT.glyph(style),
                PadButton::RT.glyph(style),
                PadButton::Menu.glyph(style),
                PadButton::R3.glyph(style)
            ),
            12.0,
            alpha(egui::Color32::WHITE, 0.85),
        ),
        (l, Some(bar)) => (
            format!("{} · {}", l.hold_label(style), bar.name()),
            14.0,
            GOLD,
        ),
        (l, None) => (l.hold_label(style), 14.0, GOLD),
    };
    let galley = p.layout_no_wrap(
        caption,
        egui::FontId::proportional(caption_size),
        caption_color,
    );
    let at = egui::pos2(
        screen.size[0] / 2.0 - galley.size().x / 2.0,
        y - 8.0 - galley.size().y,
    );
    p.rect_filled(
        egui::Rect::from_min_size(
            at - egui::vec2(8.0, 2.0),
            galley.size() + egui::vec2(16.0, 4.0),
        ),
        6.0,
        alpha(INK, 0.7),
    );
    p.galley(at, galley, caption_color);
    for b in buttons {
        let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(size, size));
        let binding = inilla.pad.binding(layer, b);
        let slot = binding.action_slot();
        let tex = slot.and_then(|s| textures.iter().find(|(x, _)| *x == s).map(|(_, t)| *t));
        let held = pad.frame.held.contains(binding);
        if tex.is_some() {
            draw_slot(p, rect, 0.7, tex, icons, slot.unwrap_or(0), now, held, None);
        } else if let Some(icon) = icon_for(binding) {
            draw_slot(p, rect, 0.7, Some(icon), icons, 0, now, held, None);
        } else {
            p.rect_filled(rect, 8.0, alpha(INK, 0.7));
            p.rect_stroke(
                rect,
                8.0,
                egui::Stroke::new(1.5_f32, alpha(GOLD, 0.8)),
                egui::StrokeKind::Inside,
            );
            // An empty action slot is just its number on the bar named above; anything else
            // says what it does.
            let (label, dim) = match binding.action_button() {
                Some((_, n)) => (n.to_string(), true),
                None if matches!(binding, PadBinding::None) => (String::new(), true),
                None => (binding.label(), false),
            };
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(if dim { 13.0 } else { 10.0 }),
                alpha(egui::Color32::WHITE, if dim { 0.45 } else { 1.0 }),
            );
        }
        // The button's glyph in the corner, on a chip so it reads over any icon.
        let chip = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(2.0, 2.0),
            egui::vec2(16.0, 15.0),
        );
        p.rect_filled(chip, 4.0, alpha(INK, 0.85));
        p.text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            b.glyph(style),
            egui::FontId::proportional(11.0),
            GOLD,
        );
        x += size + gap;
    }
}
