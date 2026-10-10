//! **Action slot icons for the HUD**: what each slot the touch HUD or the controller bar shows
//! holds — its texture, cooldown, usability and count — asked of the game's own API
//! (`GetActionTexture`, `GetActionCooldown`, `IsUsableAction`, `GetActionCount`) five times a
//! second, and the texture loaded from the player's archives as an egui image.
//!
//! A slot is read through the stock button its binding presses, picked the way `ActionButtonUp`
//! picks it: the stance bar's `BonusActionButtonN` while `BonusActionBarFrame` is shown (a
//! warrior's stances, a druid's forms, a rogue's stealth), paged by `ActionButton_GetPagedID` —
//! so the HUD shows what the press casts, not what page 1 of the main bar holds.
//!
//! A held HUD button's tooltip comes from here too: the game's own for an action slot
//! (`GameTooltip:SetAction`, beside the button), and for any other button its key binding's name
//! as the game spells it in the player's language (`BINDING_NAME_…`), which the HUD draws.
//!
//! The Lua is 1.12's dialect: no `#`, so lists grow with `table.insert` and are walked with
//! `table.getn`.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};

use benilla_assets::{BlpLoaderSettings, BlpVariant};
use benilla_ui::script::plain::PlainValue;
use benilla_ui::script::UiScript;
use inilla_core::pad::{command_label, PadBinding, Special};
use inilla_core::touch::{slot_binding, Element, ElementKind, TouchLayout};

use super::platform::SafeArea;
use super::touch::TouchState;
use super::Inilla;
use crate::char_select::ClientState;

/// One action slot, as the HUD draws it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SlotInfo {
    pub(crate) texture: Option<String>,
    /// `(start, duration)` in game seconds (`GetTime`), `(0, 0)` when ready.
    pub(crate) cooldown: (f64, f64),
    pub(crate) usable: bool,
    pub(crate) count: u32,
    /// The stock button's hotkey text (`1`, `s-2`), which the HUD repeats on its own button so
    /// the two read as one; empty when the button shows none.
    pub(crate) hotkey: String,
    /// The action slot a press uses right now (the stance bar's, the page's).
    pub(crate) action: u32,
}

#[derive(Resource, Default)]
pub(crate) struct Icons {
    pub(crate) slots: HashMap<u32, SlotInfo>,
    /// `GetTime()` when the slots were read, and the real time it was read at — the HUD
    /// advances cooldowns between reads.
    pub(crate) game_time: f64,
    pub(crate) read_at: f64,
    textures: HashMap<String, (Handle<Image>, Option<egui::TextureId>)>,
}

impl Icons {
    /// Seconds of cooldown left on `slot` at real time `now`.
    pub(crate) fn remaining(&self, slot: u32, now: f64) -> f64 {
        let Some(s) = self.slots.get(&slot) else {
            return 0.0;
        };
        let (start, dur) = s.cooldown;
        if dur <= 1.5 {
            // The global cooldown is shown by the stock bar; a spinner on every button for it
            // is noise on a phone.
            return 0.0;
        }
        let game_now = self.game_time + (now - self.read_at);
        (start + dur - game_now).max(0.0)
    }

    /// The egui texture for `path` (a 1.12 texture path, `Interface\Icons\…`), loading it on first
    /// sight.
    pub(crate) fn texture(
        &mut self,
        path: &str,
        server: &AssetServer,
        contexts: &mut EguiContexts,
    ) -> Option<egui::TextureId> {
        let entry = self.textures.entry(path.to_string()).or_insert_with(|| {
            let url = format!(
                "mpq://{}.blp",
                path.replace('\\', "/")
                    .trim_end_matches(".blp")
                    .trim_end_matches(".BLP")
            );
            let h: Handle<Image> = server.load_with_settings(url, |s: &mut BlpLoaderSettings| {
                s.variant = BlpVariant::Sprite;
            });
            (h, None)
        });
        if entry.1.is_none() && server.is_loaded(&entry.0) {
            entry.1 = Some(contexts.add_image(EguiTextureHandle::Strong(entry.0.clone())));
        }
        entry.1
    }
}

pub(crate) struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Icons>()
            .init_resource::<HudTooltip>()
            .add_systems(
                Update,
                (read_slots, hud_tooltip).after(crate::ui_script::UiInput),
            );
    }
}

/// The slots anything on screen shows: the touch layout's action slots and the controller's
/// action-button bindings.
fn wanted_slots(k: &Inilla) -> Vec<u32> {
    let mut v: Vec<u32> = k
        .layout
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::ActionSlot(s) => Some(*s),
            ElementKind::Button(b) => b.action_slot(),
            _ => None,
        })
        .chain(k.pad.rows.iter().filter_map(|r| r.binding.action_slot()))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn read_slots(
    mut icons: ResMut<Icons>,
    inilla: Res<Inilla>,
    script: Option<NonSend<UiScript>>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next || *client.get() != ClientState::InWorld {
        return;
    }
    *next = now + 0.2;
    let Some(script) = script else {
        return;
    };
    let slots = wanted_slots(&inilla);
    if slots.is_empty() {
        return;
    }
    // `{slot, "ActionButton", 3}`: the slot the HUD names and the stock button that presses it
    // (`""` for a slot no button presses, which is read as it is).
    let list: Vec<String> = slots
        .iter()
        .map(|&slot| {
            let (prefix, n) = slot_binding(slot)
                .action_button()
                .map_or(("", 0), |(bar, n)| (bar.frame_prefix(), n));
            format!("{{{slot}, \"{prefix}\", {n}}}")
        })
        .collect();
    let chunk = format!(
        "local r = {{}} local s = {{{}}} \
         local bonus = BonusActionBarFrame and BonusActionBarFrame:IsShown() \
         for i = 1, table.getn(s) do local e = s[i] local n = e[1] local hk = \"\" \
           local name = e[2] \
           if name ~= \"\" and ActionButton_GetPagedID then \
             if name == \"ActionButton\" and bonus then name = \"BonusActionButton\" end \
             local b = getglobal(name .. e[3]) \
             if b then \
               n = ActionButton_GetPagedID(b) \
               local h = getglobal(name .. e[3] .. \"HotKey\") \
               hk = h and h:GetText() or \"\" \
             end \
           end \
           local t = GetActionTexture(n) local st, du = GetActionCooldown(n) \
           local u = IsUsableAction(n) local c = GetActionCount and GetActionCount(n) or 0 \
           table.insert(r, {{e[1], t or \"\", st or 0, du or 0, u and 1 or 0, c or 0, hk, n}}) \
         end return GetTime(), r",
        list.join(",")
    );
    let Ok(values) = script.eval_plain(&chunk) else {
        return;
    };
    let mut it = values.into_iter();
    let Some(PlainValue::Num(game_time)) = it.next() else {
        return;
    };
    let Some(PlainValue::List(rows)) = it.next() else {
        return;
    };
    icons.slots.clear();
    for row in rows {
        let PlainValue::List(f) = row else { continue };
        let num = |i: usize| match f.get(i) {
            Some(PlainValue::Num(n)) => *n,
            _ => 0.0,
        };
        let text = |i: usize| match f.get(i) {
            Some(PlainValue::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        icons.slots.insert(
            num(0) as u32,
            SlotInfo {
                texture: text(1),
                cooldown: (num(2), num(3)),
                usable: num(4) != 0.0,
                count: num(5) as u32,
                hotkey: text(6).unwrap_or_default(),
                action: num(7) as u32,
            },
        );
    }
    icons.game_time = game_time;
    icons.read_at = now;
}

/// The tooltip of the HUD button a finger holds.
#[derive(Resource, Default)]
pub(crate) struct HudTooltip {
    /// The element whose tooltip is up, and whether the game's own tooltip frame shows it.
    shown: Option<(usize, bool)>,
    /// What the HUD draws for an element the game's tooltip does not cover.
    pub(crate) text: Option<(usize, String)>,
}

fn hud_tooltip(
    script: Option<NonSend<UiScript>>,
    touch: Res<TouchState>,
    inilla: Res<Inilla>,
    icons: Res<Icons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    safe: Res<SafeArea>,
    mut tip: ResMut<HudTooltip>,
) {
    let want = touch.frame.tooltip;
    if tip.shown.map(|(i, _)| i) == want {
        return;
    }
    let Some(script) = script else {
        return;
    };
    if let Some((_, true)) = tip.shown {
        let _ = script.eval_plain("GameTooltip:Hide()");
    }
    tip.shown = None;
    tip.text = None;
    let Some(i) = want else {
        return;
    };
    let Some(el) = inilla.layout.elements.get(i) else {
        return;
    };
    let slot = match el.kind {
        ElementKind::ActionSlot(s) => Some(s),
        _ => el.binding().and_then(|b| b.action_slot()),
    };
    let action = slot
        .and_then(|s| icons.slots.get(&s))
        .map(|info| info.action)
        .filter(|a| *a > 0);
    if let (Some(action), Ok(window)) = (action, windows.single()) {
        let screen = safe.screen(window.width(), window.height());
        let r = el.rect(&screen, inilla.layout.hud_scale);
        let (w, h) = (screen.size[0].max(1.0), screen.size[1].max(1.0));
        // Beside the button, on the side toward the middle of the screen.
        let (point, x) = if r.center()[0] > w / 2.0 {
            ("BOTTOMRIGHT", r.min[0])
        } else {
            ("BOTTOMLEFT", r.max[0])
        };
        let chunk = format!(
            "if not HasAction({action}) then return 0 end \
             GameTooltip:SetOwner(UIParent, \"ANCHOR_NONE\") GameTooltip:ClearAllPoints() \
             GameTooltip:SetPoint(\"{point}\", UIParent, \"BOTTOMLEFT\", \
               UIParent:GetWidth() * {fx}, UIParent:GetHeight() * {fy}) \
             GameTooltip:SetAction({action}) GameTooltip:Show() return 1",
            fx = x / w,
            fy = (h - r.min[1]) / h,
        );
        let shown = matches!(
            script.eval_plain(&chunk).ok().and_then(|v| v.into_iter().next()),
            Some(PlainValue::Num(n)) if n == 1.0
        );
        if shown {
            tip.shown = Some((i, true));
            return;
        }
    }
    tip.shown = Some((i, false));
    tip.text = Some((i, describe(el, &inilla.layout, &script)));
}

/// What a HUD element does, in words: a key binding's name as the game spells it in the player's
/// language (`BINDING_NAME_TOGGLEWORLDMAP`), else iNilla's own.
fn describe(el: &Element, layout: &TouchLayout, script: &UiScript) -> String {
    if matches!(el.kind, ElementKind::NextView) {
        return format!(
            "{}: tap for the next view. Hold jump for every view and window.",
            layout.active_view
        );
    }
    match el.binding() {
        Some(PadBinding::Command(cmd)) => {
            let plain = cmd.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let named = plain
                .then(|| {
                    script
                        .eval_plain(&format!("return getglobal(\"BINDING_NAME_{cmd}\") or \"\""))
                        .ok()
                })
                .flatten()
                .and_then(|v| match v.into_iter().next() {
                    Some(PlainValue::Str(s)) if !s.is_empty() => Some(s),
                    _ => None,
                });
            named.unwrap_or_else(|| command_label(&cmd))
        }
        Some(PadBinding::Special(Special::Interact)) => {
            "Interact with your target: talk, loot, attack".into()
        }
        Some(PadBinding::Special(Special::InillaMenu)) => {
            "iNilla: settings, servers and the layout editor".into()
        }
        Some(b) => b.label(),
        None => el.label.clone().unwrap_or_default(),
    }
}
