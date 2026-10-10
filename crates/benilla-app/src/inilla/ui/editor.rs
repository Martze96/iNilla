//! **The HUD layout editor**: the touch elements drawn over the live game, draggable; tap one to
//! select it and the editor's bar edits it — size, opacity, which views it shows in, what a
//! button does, hidden or shown. Positions snap to a 4-point grid and are stored relative to the
//! element's anchor corner, so a layout moves with the safe area and between phone sizes.
//!
//! Everything is one egui area, and the bar is laid out *after* the elements in it: egui gives a
//! touch to the last widget laid out under it, so the bar's buttons win over the full-screen
//! canvas and over any element beneath the bar. (A panel of its own sat on egui's background
//! layer, under the canvas, and never saw a tap: Done and Reset did nothing.)

use bevy_egui::egui;

use inilla_core::pad::{PadBinding, Special};
use inilla_core::touch::{default_layout, ElementKind};

use super::Overlay;
use crate::inilla::hud::GOLD;
use crate::inilla::platform::SafeArea;
use crate::inilla::Inilla;

#[derive(Clone, Debug, Default)]
pub(crate) struct EditorState {
    selected: Option<usize>,
    /// The editor's own view filter (defaults to the layout's active view).
    view: Option<String>,
    /// The bar sits at the bottom of the screen instead of under the top row.
    bar_at_bottom: bool,
    /// "Reset all" was tapped once; the next tap resets.
    confirm_reset: bool,
}

/// A button big enough for a fingertip.
fn big(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(text).min_size(egui::vec2(44.0, 36.0))
}

pub(super) fn show(
    ctx: &egui::Context,
    overlay: &mut Overlay,
    inilla: &mut Inilla,
    safe: &SafeArea,
) {
    let screen_rect = ctx.content_rect();
    let screen = safe.screen(screen_rect.width(), screen_rect.height());
    let view = overlay
        .editor
        .view
        .clone()
        .unwrap_or_else(|| inilla.layout.active_view.clone());
    let scale = inilla.layout.hud_scale;

    // A full-screen area: every touch while editing is the editor's.
    egui::Area::new(egui::Id::new("inilla-editor"))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (canvas, _) = ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
            let painter = ui.painter_at(canvas);
            painter.rect_filled(canvas, 0.0, egui::Color32::from_black_alpha(90));
            // The safe area, so the player sees where the notch and home bar are.
            let s = screen.safe_rect();
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(s.min[0], s.min[1]),
                    egui::pos2(s.max[0], s.max[1]),
                ),
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(40)),
                egui::StrokeKind::Inside,
            );
            let n = inilla.layout.elements.len();
            for i in 0..n {
                let el = inilla.layout.elements[i].clone();
                if !el.in_view(&view) && view != "Minimal" {
                    continue;
                }
                let r = el.rect(&screen, scale);
                let rect = egui::Rect::from_min_max(
                    egui::pos2(r.min[0], r.min[1]),
                    egui::pos2(r.max[0], r.max[1]),
                );
                let resp = ui.interact(
                    rect,
                    egui::Id::new(("hud-el", i)),
                    egui::Sense::click_and_drag(),
                );
                if resp.dragged() {
                    let c = rect.center() + resp.drag_delta();
                    let snapped = [(c.x / 4.0).round() * 4.0, (c.y / 4.0).round() * 4.0];
                    inilla.layout.elements[i].set_center(snapped, &screen, scale);
                    overlay.editor.selected = Some(i);
                    overlay.editor.confirm_reset = false;
                }
                if resp.clicked() {
                    overlay.editor.selected = Some(i);
                    overlay.editor.confirm_reset = false;
                }
                let sel = overlay.editor.selected == Some(i);
                let fill = if el.visible {
                    egui::Color32::from_rgba_unmultiplied(12, 12, 16, 170)
                } else {
                    egui::Color32::from_rgba_unmultiplied(60, 20, 20, 120)
                };
                let stroke = egui::Stroke::new(
                    if sel { 3.0_f32 } else { 1.5_f32 },
                    if sel {
                        GOLD
                    } else {
                        egui::Color32::from_white_alpha(120)
                    },
                );
                if matches!(el.kind, ElementKind::Stick) {
                    painter.rect(
                        rect,
                        10.0,
                        egui::Color32::from_rgba_unmultiplied(212, 175, 55, 25),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "movement stick zone",
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                } else {
                    painter.rect(rect, 10.0, fill, stroke, egui::StrokeKind::Inside);
                    let label = el
                        .label
                        .clone()
                        .or_else(|| el.binding().map(|b| b.label()))
                        .unwrap_or_else(|| el.id.clone());
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(13.0),
                        egui::Color32::WHITE,
                    );
                }
            }

            // The bar, last, so its buttons are on top of everything above.
            let width = (screen.size[0] - safe.0[1] - safe.0[3] - 24.0).min(720.0);
            let x = (screen.size[0] - width) / 2.0;
            let max_height = screen.size[1] * 0.45;
            let y = if overlay.editor.bar_at_bottom {
                screen.size[1] - safe.0[2] - max_height - 8.0
            } else {
                // Below the top row of HUD buttons (the menu, view and iNilla buttons).
                safe.0[0] + 72.0
            };
            let area = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, max_height));
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(if overlay.editor.bar_at_bottom {
                        egui::Layout::bottom_up(egui::Align::Center)
                    } else {
                        egui::Layout::top_down(egui::Align::Center)
                    }),
                |ui| bar(ui, overlay, inilla, &view, width),
            );
        });
}

fn bar(ui: &mut egui::Ui, overlay: &mut Overlay, inilla: &mut Inilla, view: &str, width: f32) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 14, 20, 240))
        .stroke(egui::Stroke::new(1.5_f32, GOLD))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Edit HUD").strong().color(GOLD));
                egui::ComboBox::from_id_salt("editor-view")
                    .selected_text(format!("View: {view}"))
                    .show_ui(ui, |ui| {
                        for v in inilla.layout.views.clone() {
                            if ui.selectable_label(v == view, &v).clicked() {
                                overlay.editor.view = Some(v);
                            }
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Done").color(GOLD))
                                .min_size(egui::vec2(90.0, 40.0)),
                        )
                        .clicked()
                    {
                        overlay.panel = None;
                        overlay.editor = EditorState::default();
                        return;
                    }
                    let reset = if overlay.editor.confirm_reset {
                        egui::RichText::new("Tap again to reset").color(egui::Color32::LIGHT_RED)
                    } else {
                        egui::RichText::new("Reset all")
                    };
                    if ui.add(big(reset)).clicked() {
                        if overlay.editor.confirm_reset {
                            let enabled = inilla.layout.enabled;
                            inilla.layout = default_layout();
                            inilla.layout.enabled = enabled;
                            overlay.editor.selected = None;
                            overlay.editor.confirm_reset = false;
                        } else {
                            overlay.editor.confirm_reset = true;
                        }
                    }
                    let flip = if overlay.editor.bar_at_bottom {
                        "⬆ Bar up"
                    } else {
                        "⬇ Bar down"
                    };
                    if ui.add(big(flip)).clicked() {
                        overlay.editor.bar_at_bottom = !overlay.editor.bar_at_bottom;
                    }
                });
            });

            let Some(i) = overlay
                .editor
                .selected
                .filter(|i| *i < inilla.layout.elements.len())
            else {
                ui.label(
                    egui::RichText::new("Tap an element to change it, drag it to move it.")
                        .color(egui::Color32::from_white_alpha(180)),
                );
                return;
            };
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let el = &mut inilla.layout.elements[i];
                ui.label(egui::RichText::new(&el.id).monospace().color(GOLD));
                if !matches!(el.kind, ElementKind::Stick) {
                    if ui.add(big("−")).clicked() {
                        el.size = [(el.size[0] - 4.0).max(28.0), (el.size[1] - 4.0).max(28.0)];
                    }
                    if ui.add(big("+")).clicked() {
                        el.size = [(el.size[0] + 4.0).min(120.0), (el.size[1] + 4.0).min(120.0)];
                    }
                }
                ui.add(egui::Slider::new(&mut el.opacity, 0.2..=1.5).text("opacity"));
                ui.checkbox(&mut el.visible, "shown");
                let in_view = el.views.iter().any(|v| v == view);
                let mut all = el.views.is_empty();
                if ui.checkbox(&mut all, "all views").changed() {
                    el.views = if all {
                        Vec::new()
                    } else {
                        vec![view.to_string()]
                    };
                }
                if !all {
                    let mut here = in_view;
                    if ui.checkbox(&mut here, format!("in {view}")).changed() {
                        if here {
                            el.views.push(view.to_string());
                        } else {
                            el.views.retain(|v| v != view);
                        }
                    }
                }
                if let ElementKind::Button(b) = &mut el.kind {
                    let current = b.clone();
                    let mut pick = current.clone();
                    egui::ComboBox::from_id_salt("editor-binding")
                        .selected_text(format!("does: {}", current.label()))
                        .height(360.0)
                        .show_ui(ui, |ui| {
                            for c in [
                                PadBinding::Special(Special::Interact),
                                PadBinding::Special(Special::InillaMenu),
                            ]
                            .into_iter()
                            .chain(
                                crate::inilla::synth::command_names()
                                    .map(|(n, _)| PadBinding::cmd(n)),
                            ) {
                                let label = c.label();
                                ui.selectable_value(&mut pick, c, label);
                            }
                        });
                    if pick != current {
                        el.label = None;
                        *b = pick;
                    }
                }
                if let ElementKind::ActionSlot(slot) = &mut el.kind {
                    ui.add(egui::DragValue::new(slot).range(1..=120).prefix("slot "));
                }
                let id = el.id.clone();
                if ui.add(big("Reset this")).clicked() {
                    if let Some(d) = default_layout().elements.into_iter().find(|d| d.id == id) {
                        inilla.layout.elements[i] = d;
                    }
                }
            });
        });
}
