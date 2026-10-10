//! **The touch HUD, input half**: Bevy's touches into [`inilla_core::touch::TouchEngine`], and
//! the engine's answer back out — bindings into [`Synth`], mouse operations into the
//! [`VirtualMouse`], HUD actions (views, the radial, iNilla's menu) here. Drawing is [`super::hud`].
//!
//! Who owns a finger is decided when it lands: iNilla's own screens (egui) first, then the
//! engine's order — a HUD element, a stock UI frame (asked of the FrameXML hit test), the stick
//! zone, the world. On the glue screens (login, character select/create) every finger is a
//! mouse: they are Bevy UI, not FrameXML, and hover-then-click is exactly what they need.

use std::collections::HashSet;

use bevy::ecs::system::{NonSendMarker, SystemParam};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use benilla_ui::script::UiScript;
use inilla_core::pad::{PadBinding, Special};
use inilla_core::touch::{
    HubItem, Phase, TouchEngine, TouchFrame, TouchInput as KTouch, TouchLayout,
};

use super::mouse::VirtualMouse;
use super::platform::SafeArea;
use super::synth::Synth;
use super::ui::Overlay;
use super::{Inilla, InputProducers};
use crate::char_select::ClientState;
use crate::creature_anim::Casting;
use crate::net::{ObjectStore, SelfPlayer};
use crate::spell::ActiveChannel;

#[derive(Resource, Default)]
pub(crate) struct TouchState {
    engine: TouchEngine,
    /// The last frame's answer, for the HUD to draw.
    pub(crate) frame: TouchFrame,
    /// Fingers iNilla's own screens own — never fed to the engine.
    egui_owned: HashSet<u64>,
    /// Seconds (real time) of the last touch, so the controller HUD can yield to touch.
    pub(crate) last_touch: f64,
    /// An empty layout for when the HUD is hidden (fingers are still a mouse).
    bare: TouchLayout,
    /// The engine has been told which commands are held (the registry's `Held` kinds).
    held_known: bool,
}

/// Actions a HUD element or controller asked for that are not game bindings.
#[derive(Resource, Default)]
pub(crate) struct HudActions {
    pub(crate) interact: bool,
    pub(crate) open_menu: bool,
    pub(crate) toggle_hud: bool,
    /// A radial entry picked (index into `RADIAL_ITEMS`).
    pub(crate) radial_pick: Option<usize>,
}

/// Is the touch HUD showing its elements this frame?
pub(crate) fn hud_visible(
    state: &Inilla,
    client: &ClientState,
    overlay: &Overlay,
    pad_active: bool,
) -> bool {
    super::touch_device()
        && state.layout.enabled
        && *client == ClientState::InWorld
        && !overlay.blocks_game()
        && !(pad_active && state.settings.hide_touch_with_controller)
}

pub(crate) struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchState>()
            .init_resource::<HudActions>()
            .add_systems(PreUpdate, process.in_set(InputProducers))
            .add_systems(OnExit(ClientState::InWorld), reset);
        if super::touch_device() {
            // Every touch reaches the game once — as iNilla's mouse. Bevy UI's focus system (the
            // glue screens' buttons) and bevy_picking would otherwise also act on the raw touch,
            // and a tap on the login button would press it twice.
            app.insert_resource(bevy::picking::input::PointerInputSettings {
                is_touch_enabled: false,
                is_mouse_enabled: true,
            })
            .add_systems(
                PreUpdate,
                forget_touches
                    .after(bevy::input::InputSystems)
                    .before(bevy::ui::UiSystems::Focus),
            );
        }
    }
}

fn forget_touches(mut touches: ResMut<bevy::input::touch::Touches>) {
    touches.reset_all();
}

/// Leaving the world drops every finger and releases whatever the virtual mouse held.
fn reset(mut state: ResMut<TouchState>, mut vm: ResMut<VirtualMouse>) {
    state.engine.reset();
    state.egui_owned.clear();
    vm.reset();
}

/// What the touch pass reads about the world around it.
#[derive(SystemParam)]
struct Env<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    script: Option<NonSend<'w, UiScript>>,
    safe: Res<'w, SafeArea>,
    time: Res<'w, Time<Real>>,
    client: Res<'w, State<ClientState>>,
    overlay: Res<'w, Overlay>,
    pad: Res<'w, super::pad::PadHud>,
}

/// Whether a jump now would cost the player something: seated (a jump stands them up, ending a
/// meal or a drink) or casting and channelling (a jump interrupts). Jump then waits for the
/// release, so holding it for the hub costs nothing.
#[derive(SystemParam)]
struct Posture<'w, 's> {
    me: Query<'w, 's, (&'static ObjectStore, Has<Casting>), With<SelfPlayer>>,
    channel: Option<Res<'w, ActiveChannel>>,
}

impl Posture<'_, '_> {
    fn jump_would_cost(&self) -> bool {
        let (seated, casting) = self.me.single().map_or((false, false), |(store, casting)| {
            (store.0.unit_stand_state() != 0, casting)
        });
        let channelling = self
            .channel
            .as_ref()
            .is_some_and(|c| c.current(std::time::Instant::now()).is_some());
        seated || casting || channelling
    }
}

#[allow(clippy::too_many_arguments)]
fn process(
    _main: NonSendMarker,
    mut touches: MessageReader<TouchInput>,
    mut state: ResMut<TouchState>,
    mut inilla: ResMut<Inilla>,
    mut synth: ResMut<Synth>,
    mut vm: ResMut<VirtualMouse>,
    mut actions: ResMut<HudActions>,
    env: Env,
    posture: Posture,
    mut contexts: EguiContexts,
) {
    let Env {
        windows,
        script,
        safe,
        time,
        client,
        overlay,
        pad,
    } = env;
    if !super::touch_device() {
        touches.clear();
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let screen = safe.screen(window.width(), window.height());
    let in_world = *client.get() == ClientState::InWorld;
    // The HUD yields to a controller in use, and comes back with the next touch.
    let pad_active = pad.active(now) && pad.last_active > state.last_touch;
    let show = hud_visible(&inilla, client.get(), &overlay, pad_active);
    let blocked = overlay.blocks_game();

    // "Is a mouse-enabled FrameXML frame under this point?" — the input pass's own conversion
    // (window points, y-down → the VM's y-up 768-virtual units ÷ the UI scale).
    let win_h = window.height();
    let seam = crate::ui_script::seam_scale(win_h);
    let ui_hit = |p: [f32; 2]| -> bool {
        if !in_world {
            // Glue screens are Bevy UI: every finger is a plain mouse there.
            return true;
        }
        let Some(s) = script.as_deref() else {
            return false;
        };
        let (x, y) = (p[0] / seam, (win_h - p[1]) / seam);
        s.hit_test(x, y).is_some_and(|id| !s.is_world_frame(id))
    };

    let ctx = contexts.ctx_mut().ok();
    let TouchState {
        engine,
        egui_owned,
        bare,
        last_touch,
        held_known,
        ..
    } = &mut *state;
    if !*held_known {
        *held_known = true;
        engine.set_held_commands(
            super::synth::COMMANDS
                .iter()
                .filter(|(name, _)| super::synth::is_movement_or_held(name))
                .map(|(name, _)| (*name).to_string()),
        );
    }
    engine.set_defer_jump(in_world && posture.jump_would_cost());
    let layout: &TouchLayout = if show { &inilla.layout } else { bare };
    for t in touches.read() {
        *last_touch = now;
        let pos = [t.position.x, t.position.y];
        let phase = match t.phase {
            TouchPhase::Started => Phase::Started,
            TouchPhase::Moved => Phase::Moved,
            TouchPhase::Ended => Phase::Ended,
            TouchPhase::Canceled => Phase::Canceled,
        };
        if phase == Phase::Started {
            let over_egui = ctx.as_ref().is_some_and(|c| {
                c.layer_id_at(egui::pos2(pos[0], pos[1]))
                    .is_some_and(|l| l.order != egui::Order::Background)
            });
            if blocked || over_egui {
                egui_owned.insert(t.id);
                continue;
            }
        }
        if egui_owned.contains(&t.id) {
            if matches!(phase, Phase::Ended | Phase::Canceled) {
                egui_owned.remove(&t.id);
            }
            continue;
        }
        engine.input(
            KTouch {
                id: t.id,
                phase,
                pos,
            },
            layout,
            &screen,
            now,
            &ui_hit,
        );
    }
    let frame = engine.frame(layout, now);

    synth.movement(&frame.movement);
    let mut special = |b: &PadBinding| -> bool {
        let PadBinding::Special(s) = b else {
            return false;
        };
        match s {
            Special::Interact => actions.interact = true,
            Special::InillaMenu => actions.open_menu = true,
            Special::ToggleTouchHud => actions.toggle_hud = true,
            _ => {}
        }
        true
    };
    for b in &frame.pressed {
        if !special(b) {
            synth.press(b.clone());
        }
    }
    for b in &frame.released {
        if !matches!(b, PadBinding::Special(_)) {
            synth.release(b.clone());
        }
    }
    for b in &frame.held {
        if !matches!(b, PadBinding::Special(_)) {
            synth.hold(b.clone());
        }
    }
    if frame.zoom > 0.0 {
        synth.fire("CAMERAZOOMIN", frame.zoom);
    } else if frame.zoom < 0.0 {
        synth.fire("CAMERAZOOMOUT", -frame.zoom);
    }
    vm.extend(frame.mouse.iter().copied());
    if frame.next_view {
        inilla.layout.next_view();
    }
    if let Some(i) = frame.radial_pick {
        actions.radial_pick = Some(i);
    }
    match &frame.hub_pick {
        Some(HubItem::View(v)) if inilla.layout.views.contains(v) => {
            inilla.layout.active_view = v.clone();
        }
        Some(HubItem::Window(i)) => actions.radial_pick = Some(*i),
        _ => {}
    }
    state.frame = frame;
}
