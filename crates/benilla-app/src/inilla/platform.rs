//! **The iOS host**, as Bevy systems: the window's iOS options, the screen kept awake, the audio
//! session, the safe area and the Dynamic Island, the on-screen keyboard, the frame cap, and the
//! iNilla settings that land on benilla CVars (render scale, UI scale, the quality preset).

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, ScreenEdge};

use inilla_core::touch::Screen;
use inilla_platform::device::Side;
use inilla_platform::{audio, device};

use super::Inilla;
use crate::char_select::ClientState;
use crate::cvars::Cvars;
use crate::ui_script::UiKeyboardCapture;

/// The safe-area insets in points, `[top, left, bottom, right]` — the notch / Dynamic Island in
/// landscape, the home indicator — and which side the camera housing is on.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub(crate) struct SafeArea(pub(crate) [f32; 4], pub(crate) Option<Side>);

impl SafeArea {
    /// The screen the HUD lays out on: the window, its safe area, and the strip beside the camera
    /// housing where the system buttons go.
    pub(crate) fn screen(&self, width: f32, height: f32) -> Screen {
        let s = Screen {
            safe: self.0,
            ..Screen::new(width, height)
        };
        match self.1 {
            Some(side) => s.with_camera_on_left(side == Side::Left),
            None => s,
        }
    }
}

/// The on-screen keyboard was asked for by a HUD button (glue screens, which have no focus event).
#[derive(Resource, Default)]
pub(crate) struct KeyboardRequest(pub(crate) bool);

/// Keys a HUD button presses for the player: the chat bar's Send (`Enter`) and Close (`Escape`),
/// which the iOS keyboard has no key for. Each goes down in one frame and up in the next, ahead of
/// Bevy's input systems, so the text law sees an ordinary key.
#[derive(Resource, Default)]
pub(crate) struct SyntheticKeys {
    queued: Vec<KeyCode>,
    down: Vec<KeyCode>,
}

impl SyntheticKeys {
    pub(crate) fn tap(&mut self, key: KeyCode) {
        self.queued.push(key);
    }
}

pub(crate) struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SafeArea>()
            .init_resource::<IslandInset>()
            .init_resource::<KeyboardRequest>()
            .init_resource::<SyntheticKeys>()
            .add_systems(Startup, window_options)
            .add_systems(
                PreUpdate,
                (return_key_is_enter, synthetic_keys)
                    .chain()
                    .before(bevy::input::InputSystems),
            )
            .add_systems(
                Update,
                (
                    track_safe_area,
                    lock_landscape,
                    keep_screen_awake,
                    audio_session,
                    keyboard_policy.after(crate::ui_script::UiInput),
                    apply_cvar_settings,
                    compact_stock_ui.after(crate::ui_script::UiInput),
                    clear_the_island.after(crate::ui_script::UiInput),
                ),
            )
            .add_systems(Last, frame_cap);
    }
}

/// Full screen as a game wants it: no home indicator, no status bar, and the screen edges defer
/// the system gestures (a swipe from an edge shows the grabber first instead of leaving the game).
fn window_options(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !cfg!(target_os = "ios") {
        return;
    }
    for mut w in &mut windows {
        w.prefers_home_indicator_hidden = true;
        w.prefers_status_bar_hidden = true;
        w.preferred_screen_edges_deferring_system_gestures = ScreenEdge::All;
        // iNilla reads raw touches; UIKit's recognisers would only steal them.
        w.recognize_pinch_gesture = false;
        w.recognize_rotation_gesture = false;
        w.recognize_doubletap_gesture = false;
        w.recognize_pan_gesture = None;
    }
}

/// Landscape only, held by the view controller itself: the installed app's `Info.plist` already
/// says so, but a host that runs iNilla inside itself (LiveContainer) applies its own plist,
/// which rotates to portrait.
#[cfg(target_os = "ios")]
fn lock_landscape(
    windows: Query<(Entity, &bevy::window::RawHandleWrapper), With<PrimaryWindow>>,
    _main: bevy::ecs::system::NonSendMarker,
    mut done: Local<bool>,
) {
    use winit::platform::ios::{ValidOrientations, WindowExtIOS};
    if *done {
        return;
    }
    let Ok((entity, handle)) = windows.single() else {
        return;
    };
    *done = bevy::winit::WINIT_WINDOWS.with_borrow(|winit_windows| {
        winit_windows
            .get_window(entity)
            .map(|window| window.set_valid_orientations(ValidOrientations::Landscape))
            .is_some()
    });
    // winit applies the mask with `attemptRotationToDeviceOrientation`, a no-op since iOS 16.
    if !*done {
        return;
    }
    if let raw_window_handle::RawWindowHandle::UiKit(h) = handle.get_window_handle() {
        device::refresh_orientations(h.ui_view.as_ptr());
    }
}

#[cfg(not(target_os = "ios"))]
fn lock_landscape() {}

/// The iOS keyboard's Return arrives as the text `"\n"` with no key code; benilla's text law keys
/// submit on `Enter`. Rewrite it before Bevy's keyboard system sees it.
fn return_key_is_enter(mut keys: ResMut<Messages<KeyboardInput>>) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let needs = keys.iter_current_update_messages().any(|k| {
        matches!(k.text.as_deref(), Some("\n") | Some("\r")) || is_newline_key(&k.logical_key)
    });
    if !needs {
        return;
    }
    // Messages cannot be edited in place: take this update's, drop the buffer (last frame's half
    // has been read by every reader already) and write this update's back, rewritten.
    let current: Vec<KeyboardInput> = keys.iter_current_update_messages().cloned().collect();
    keys.drain().for_each(drop);
    for mut k in current {
        if matches!(k.text.as_deref(), Some("\n") | Some("\r")) || is_newline_key(&k.logical_key) {
            k.key_code = KeyCode::Enter;
            k.logical_key = Key::Enter;
            k.text = None;
        }
        keys.write(k);
    }
}

fn synthetic_keys(
    mut keys: ResMut<SyntheticKeys>,
    mut out: ResMut<Messages<KeyboardInput>>,
    windows: Query<Entity, With<PrimaryWindow>>,
) {
    if keys.queued.is_empty() && keys.down.is_empty() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let logical = |k: KeyCode| match k {
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        _ => Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
    };
    for key in std::mem::take(&mut keys.down) {
        out.write(KeyboardInput {
            key_code: key,
            logical_key: logical(key),
            state: bevy::input::ButtonState::Released,
            text: None,
            repeat: false,
            window,
        });
    }
    for key in std::mem::take(&mut keys.queued) {
        out.write(KeyboardInput {
            key_code: key,
            logical_key: logical(key),
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        keys.down.push(key);
    }
}

fn is_newline_key(k: &Key) -> bool {
    matches!(k, Key::Character(c) if c.as_str() == "\n" || c.as_str() == "\r")
}

fn track_safe_area(
    windows: Query<&bevy::window::RawHandleWrapper, With<PrimaryWindow>>,
    mut area: ResMut<SafeArea>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    #[cfg(target_os = "ios")]
    {
        let Ok(handle) = windows.single() else {
            return;
        };
        if let raw_window_handle::RawWindowHandle::UiKit(h) = handle.get_window_handle() {
            let insets = device::safe_area_insets(h.ui_view.as_ptr());
            if area.0 != insets {
                area.0 = insets;
            }
            let side = device::camera_side(h.ui_view.as_ptr());
            if area.1 != side {
                area.1 = side;
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (&windows, &mut area);
    }
}

fn keep_screen_awake(
    state: Res<Inilla>,
    client: Res<State<ClientState>>,
    mut last: Local<Option<bool>>,
) {
    let want = state.settings.keep_screen_on && *client.get() == ClientState::InWorld;
    if *last != Some(want) {
        device::set_idle_timer_disabled(want);
        *last = Some(want);
    }
}

fn audio_session(state: Res<Inilla>, mut last: Local<Option<(bool, bool)>>) {
    let want = (
        state.settings.mix_with_other_audio,
        state.settings.ignore_silent_switch,
    );
    if *last == Some(want) {
        return;
    }
    *last = Some(want);
    if let Err(e) = audio::configure_session(want.0, want.1) {
        warn!("inilla: audio session: {e}");
    }
}

/// Show the keyboard while a text box has focus (chat, the stock popups) or a HUD button asked.
fn keyboard_policy(
    state: Res<Inilla>,
    capture: Res<UiKeyboardCapture>,
    request: Res<KeyboardRequest>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut last: Local<bool>,
) {
    if !super::touch_device() {
        return;
    }
    let want = request.0 || (state.settings.auto_keyboard && capture.typing);
    if want == *last {
        return;
    }
    *last = want;
    for mut w in &mut windows {
        w.ime_enabled = want;
    }
}

/// iNilla settings → benilla CVars, when they move.
fn apply_cvar_settings(
    state: Res<Inilla>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cvars: ResMut<Cvars>,
    mut last: Local<Option<(u32, u32, inilla_core::settings::Quality)>>,
) {
    if !super::touch_device() {
        return;
    }
    let height = windows.single().map_or(0.0, |w| w.height());
    let ui = state.settings.effective_ui_scale(height);
    let key = (
        (state.settings.render_scale * 100.0).round() as u32,
        (ui * 100.0).round() as u32,
        state.settings.quality,
    );
    if *last == Some(key) {
        return;
    }
    let first = last.is_none();
    *last = Some(key);
    cvars.set(
        "renderScale",
        &format!("{:.2}", state.settings.render_scale.clamp(0.5, 1.0)),
    );
    cvars.set("useUiScale", "1");
    cvars.set("uiScale", &format!("{ui:.2}"));
    // The preset writes its CVars when the player picks it, not on every launch — a detail the
    // player then changed in the stock options must survive a restart.
    if !first {
        for (name, value) in state.settings.quality.cvars() {
            cvars.set(name, value);
        }
    }
}

/// The preset's frame cap (`0` = the display's rate). Sleeping at the end of the frame is crude,
/// but it is the one limiter that works the same on every platform, and at 30 fps it halves the
/// work the phone does — which is the whole point of the battery preset.
fn frame_cap(state: Res<Inilla>, mut last: Local<Option<Instant>>) {
    if !super::touch_device() {
        return;
    }
    let cap = state.settings.quality.fps_cap();
    let now = Instant::now();
    if cap > 0 {
        let budget = Duration::from_secs_f64(1.0 / f64::from(cap));
        if let Some(prev) = *last {
            let spent = now.duration_since(prev);
            if spent < budget {
                std::thread::sleep(budget - spent);
            }
        }
    }
    *last = Some(Instant::now());
}

/// The main action bar's gryphons (`MainMenuBarLeftEndCap`/`RightEndCap`) are 128 units of
/// decoration at each end — on a phone, exactly the corners the thumbs and the HUD use. Hidden or
/// shown every couple of seconds so a `/reload` (a fresh interface) gets the setting too.
fn compact_stock_ui(
    state: Res<Inilla>,
    script: Option<NonSend<benilla_ui::script::UiScript>>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next || *client.get() != ClientState::InWorld {
        return;
    }
    *next = now + 2.0;
    let Some(script) = script else {
        return;
    };
    let verb = if state.settings.compact_stock_ui && super::touch_device() {
        "Hide"
    } else {
        "Show"
    };
    crate::ui_script::run_or_warn(
        &script,
        &format!(
            "if MainMenuBarLeftEndCap then MainMenuBarLeftEndCap:{verb}() end \
             if MainMenuBarRightEndCap then MainMenuBarRightEndCap:{verb}() end"
        ),
    );
}

/// When to next check the stock interface's inset (real seconds).
#[derive(Resource, Default)]
struct IslandInset {
    next: f64,
}

/// Keep the stock windows clear of the Dynamic Island: `UIParent` is anchored to the screen with
/// the safe-area inset on the camera's side only, so the bags, a vendor and the quest log stop
/// short of it while the 3D world, which is not `UIParent`'s, still runs edge to edge. The applied
/// inset is kept in the VM, so a reloaded interface gets it again; an interface never inset is
/// left as the stock anchors it.
#[allow(clippy::too_many_arguments)]
fn clear_the_island(
    state: Res<Inilla>,
    script: Option<NonSend<benilla_ui::script::UiScript>>,
    safe: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut inset: ResMut<IslandInset>,
) {
    let now = time.elapsed_secs_f64();
    if now < inset.next || *client.get() != ClientState::InWorld {
        return;
    }
    inset.next = now + 0.5;
    let (Some(script), Ok(window)) = (script, windows.single()) else {
        return;
    };
    let seam = crate::ui_script::seam_scale(window.height());
    let points = |side: Side, inset: f32| {
        let on = state.settings.island_margin && super::touch_device() && safe.1 == Some(side);
        if on {
            inset
        } else {
            0.0
        }
    };
    let left = points(Side::Left, safe.0[1]) / seam;
    let right = points(Side::Right, safe.0[3]) / seam;
    let key = format!("{left:.1}:{right:.1}");
    crate::ui_script::run_or_warn(
        &script,
        &format!(
            "if (INILLA_INSET or \"0.0:0.0\") ~= \"{key}\" and UIParent then \
               UIParent:ClearAllPoints() \
               UIParent:SetPoint(\"TOPLEFT\", nil, \"TOPLEFT\", {left:.1}, 0) \
               UIParent:SetPoint(\"BOTTOMRIGHT\", nil, \"BOTTOMRIGHT\", {r:.1}, 0) \
               INILLA_INSET = \"{key}\" \
             end",
            r = -right,
        ),
    );
}
