//! **Hints on the stock interface**, for a player new to a controller or a phone keyboard:
//!
//! - While a controller is in use, each stock action button shows the controller button that
//!   presses it (`X`, `LT A`, `RT ↑`) where the keyboard key would be. The first time a controller
//!   is used, the bars its trigger layers press are switched on, so what LT, RT and LT+RT fire is
//!   on screen. 1.12 hides them by default.
//! - While a trigger is held, the stock buttons its layer presses light up (`LockHighlight`), and
//!   the first time a controller is used one line says what the rest of it does.
//! - While the chat box has focus, its line is read back for the HUD's chat bar: in landscape the
//!   iOS keyboard covers the stock edit box.
//! - On a touch device the chat windows are locked (the stock "Lock Window"), so a thumb that
//!   lands on the chat on its way to the stick can neither move nor resize it.
//!
//! Both go through the game's own Lua. The hotkey text wraps `ActionButton_UpdateHotkeys`, so a
//! binding change or a stance swap that redraws a button redraws it with the hint.

use bevy::prelude::*;

use benilla_ui::script::plain::PlainValue;
use benilla_ui::script::UiScript;
use inilla_core::pad::{bars_in_use, hotkey_hints, hotkey_layers, ActionBar, Layer, PadButton};

use super::pad::PadHud;
use super::touch::TouchState;
use super::ui::Overlay;
use super::Inilla;
use crate::char_select::ClientState;
use crate::ui_script::UiKeyboardCapture;

/// The chat line being typed, as the stock edit box holds it.
#[derive(Resource, Default)]
pub(crate) struct ChatLine {
    /// A text box has the keyboard focus (the chat box or any other).
    pub(crate) typing: bool,
    /// The chat box has the focus (not another text box, such as a mail subject).
    pub(crate) chat: bool,
    /// `Say:`, `Party:`, `Tell Thrall:`: the edit box's header.
    pub(crate) header: String,
    pub(crate) text: String,
}

pub(crate) struct HintsPlugin;

impl Plugin for HintsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLine>()
            .init_resource::<ChatLock>()
            .init_resource::<LayerHighlight>()
            .add_systems(
                Update,
                (
                    controller_hotkeys,
                    highlight_layer,
                    controller_tip,
                    read_chat_line,
                    lock_chat_windows,
                )
                    .after(crate::ui_script::UiInput),
            );
    }
}

#[derive(Default)]
struct HotkeyState {
    /// What was last applied: on or off, and the hint text it applied.
    applied: Option<(bool, String)>,
    generation: u32,
    next: f64,
}

/// Every stock action button and stance-bar button, by frame-name prefix.
const BUTTON_FRAMES: [&str; 6] = [
    "ActionButton",
    "BonusActionButton",
    "MultiBarBottomLeftButton",
    "MultiBarBottomRightButton",
    "MultiBarRightButton",
    "MultiBarLeftButton",
];

fn lua_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[allow(clippy::too_many_arguments)]
fn controller_hotkeys(
    script: Option<NonSend<UiScript>>,
    mut inilla: ResMut<Inilla>,
    pad: Res<PadHud>,
    touch: Res<TouchState>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut state: Local<HotkeyState>,
) {
    let now = time.elapsed_secs_f64();
    if *client.get() != ClientState::InWorld {
        state.applied = None;
        return;
    }
    if now < state.next {
        return;
    }
    state.next = now + 0.5;
    let Some(script) = script else {
        return;
    };
    let pad_active = pad.active(now) && pad.last_active > touch.last_touch;

    // The trigger layers' bars, once: after that they are the stock options' to show or hide.
    if pad_active && !inilla.settings.controller_bars_shown {
        let mut show = String::new();
        for bar in bars_in_use(&inilla.pad) {
            let var = match bar {
                ActionBar::BottomLeft => "SHOW_MULTI_ACTIONBAR_1",
                ActionBar::BottomRight => "SHOW_MULTI_ACTIONBAR_2",
                // Right bar 2 only shows beside right bar 1.
                ActionBar::Right => "SHOW_MULTI_ACTIONBAR_3",
                ActionBar::Right2 => "SHOW_MULTI_ACTIONBAR_3 = 1 SHOW_MULTI_ACTIONBAR_4",
                ActionBar::Main => continue,
            };
            show.push_str(&format!("{var} = 1 "));
        }
        let chunk = format!(
            "if MultiActionBar_Update then {show} MultiActionBar_Update() \
             if UIParent_ManageFramePositions then UIParent_ManageFramePositions() end \
             return 1 end return 0"
        );
        if let Ok(v) = script.eval_plain(&chunk) {
            if matches!(v.first(), Some(PlainValue::Num(n)) if *n == 1.0) {
                inilla.settings.controller_bars_shown = true;
                info!("inilla: showed the controller's action bars");
            }
        }
    }

    let on = pad_active && inilla.settings.controller_hotkeys;
    let texts = if on {
        hotkey_hints(&inilla.pad, pad.glyphs)
            .into_iter()
            .map(|(frame, text)| format!("[{}] = {}", lua_str(&frame), lua_str(&text)))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };
    // A reloaded interface starts without the wrapper; ask which generation it has.
    let live = match script.eval_plain("return INILLA_HK and INILLA_HK.gen or 0") {
        Ok(v) => match v.first() {
            Some(PlainValue::Num(n)) => *n as u32,
            _ => 0,
        },
        Err(_) => return,
    };
    let want = (on, texts);
    if state.applied.as_ref() == Some(&want) && live == state.generation && live != 0 {
        return;
    }
    // Off and never applied in this interface: nothing to undo.
    if !on && live == 0 {
        state.applied = Some(want);
        return;
    }
    state.generation = state.generation.wrapping_add(1).max(1);
    let frames = BUTTON_FRAMES
        .iter()
        .map(|f| lua_str(f))
        .collect::<Vec<_>>()
        .join(", ");
    let chunk = format!(
        "if not INILLA_HK then \
           INILLA_HK = {{}} INILLA_HK_TEXT = {{}} \
           local stock = ActionButton_UpdateHotkeys \
           if not stock then return 0 end \
           ActionButton_UpdateHotkeys = function(kind) \
             stock(kind) \
             if INILLA_HK.on and this and this.GetName then \
               local hk = getglobal(this:GetName() .. \"HotKey\") \
               if hk then hk:SetText(INILLA_HK_TEXT[this:GetName()] or \"\") end \
             end \
           end \
         end \
         INILLA_HK.on = {on} INILLA_HK.gen = {gen} INILLA_HK_TEXT = {{ {texts} }} \
         local saved = this local bars = {{ {frames} }} \
         for b = 1, table.getn(bars) do for i = 1, 12 do \
           local f = getglobal(bars[b] .. i) \
           if f and f.GetName then this = f ActionButton_UpdateHotkeys(f.buttonType) end \
         end end \
         this = saved return 1",
        on = if want.0 { "1" } else { "nil" },
        gen = state.generation,
        texts = want.1,
    );
    match script.eval_plain(&chunk) {
        Ok(_) => state.applied = Some(want),
        Err(e) => {
            warn!("inilla: controller hotkeys: {e}");
            // Try again later rather than every half second.
            state.next = now + 10.0;
        }
    }
}

fn read_chat_line(
    script: Option<NonSend<UiScript>>,
    capture: Res<UiKeyboardCapture>,
    client: Res<State<ClientState>>,
    mut line: ResMut<ChatLine>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    if !capture.typing || *client.get() != ClientState::InWorld || !super::touch_device() {
        if line.typing || line.chat || !line.text.is_empty() {
            *line = ChatLine::default();
        }
        return;
    }
    if !line.typing {
        line.typing = true;
    }
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 0.1;
    let Some(script) = script else {
        return;
    };
    let Ok(v) = script.eval_plain(
        "local e = ChatFrameEditBox \
         if e and e:IsVisible() then \
           local h = ChatFrameEditBoxHeader \
           return 1, (h and h:GetText()) or \"\", e:GetText() or \"\" \
         end return 0, \"\", \"\"",
    ) else {
        return;
    };
    let text = |i: usize| match v.get(i) {
        Some(PlainValue::Str(s)) => s.clone(),
        _ => String::new(),
    };
    line.chat = matches!(v.first(), Some(PlainValue::Num(n)) if *n == 1.0);
    line.header = text(1);
    line.text = text(2);
}

/// When to next try locking the chat windows (real seconds).
#[derive(Resource, Default)]
struct ChatLock {
    next: f64,
}

/// Lock every chat window once per interface: `FCF_SetLocked`, what the tab menu's "Lock Window"
/// runs. The marker lives in the VM, so a reloaded interface is locked again.
fn lock_chat_windows(
    script: Option<NonSend<UiScript>>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut lock: ResMut<ChatLock>,
) {
    let now = time.elapsed_secs_f64();
    if !super::touch_device() || *client.get() != ClientState::InWorld || now < lock.next {
        return;
    }
    lock.next = now + 2.0;
    let Some(script) = script else {
        return;
    };
    let _ = script.eval_plain(
        "if INILLA_CHAT_LOCKED or not FCF_SetLocked then return end \
         for i = 1, NUM_CHAT_WINDOWS or 7 do \
           local f = getglobal(\"ChatFrame\" .. i) \
           if f then FCF_SetLocked(f, 1) end \
         end \
         INILLA_CHAT_LOCKED = 1",
    );
}

/// The trigger layer whose buttons are lit.
#[derive(Resource, Default)]
struct LayerHighlight {
    lit: Option<Layer>,
}

/// While LT, RT or both are held, the stock buttons that layer presses show their highlight: what
/// the face buttons and the d-pad fire now is lit on the bars themselves.
#[allow(clippy::too_many_arguments)]
fn highlight_layer(
    script: Option<NonSend<UiScript>>,
    inilla: Res<Inilla>,
    pad: Res<PadHud>,
    touch: Res<TouchState>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut hl: ResMut<LayerHighlight>,
) {
    let now = time.elapsed_secs_f64();
    let pad_active = pad.active(now) && pad.last_active > touch.last_touch;
    let want =
        (pad_active && *client.get() == ClientState::InWorld && inilla.settings.controller_hotkeys)
            .then_some(pad.frame.layer)
            .filter(|l| *l != Layer::Base);
    if want == hl.lit {
        return;
    }
    let Some(script) = script else {
        return;
    };
    let layers = hotkey_layers(&inilla.pad);
    let names = |lit: bool| {
        layers
            .iter()
            .filter(|(_, l)| !lit || Some(*l) == want)
            .map(|(f, _)| lua_str(f))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let chunk = format!(
        "local function each(t, m) for i = 1, table.getn(t) do \
           local b = getglobal(t[i]) if b and b[m] then b[m](b) end \
         end end \
         each({{ {all} }}, \"UnlockHighlight\") each({{ {lit} }}, \"LockHighlight\")",
        all = names(false),
        lit = names(true),
    );
    match script.eval_plain(&chunk) {
        Ok(_) => hl.lit = want,
        Err(e) => warn!("inilla: layer highlight: {e}"),
    }
}

/// The first time a controller is used in a session: one line on what the rest of it does (the
/// controller bar's old guide line).
fn controller_tip(
    pad: Res<PadHud>,
    touch: Res<TouchState>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut overlay: ResMut<Overlay>,
    mut shown: Local<bool>,
) {
    let now = time.elapsed_secs_f64();
    if *shown || *client.get() != ClientState::InWorld {
        return;
    }
    if !(pad.active(now) && pad.last_active > touch.last_touch) {
        return;
    }
    *shown = true;
    let g = |b: PadButton| b.glyph(pad.glyphs);
    overlay.toast(format!(
        "{}/{} target · hold {} or {} for more bars · {} menu · {} cursor",
        g(PadButton::LB),
        g(PadButton::RB),
        g(PadButton::LT),
        g(PadButton::RT),
        g(PadButton::Menu),
        g(PadButton::R3)
    ));
}
