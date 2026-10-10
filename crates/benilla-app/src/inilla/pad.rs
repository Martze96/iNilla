//! **Game controllers**, the Bevy half: poll the pad (Apple's GameController on iOS, gilrs on a
//! desktop run with `$INILLA_PAD=1`), run [`PadMapper`], and hand its answer on — bindings to
//! [`Synth`], look and the cursor to the [`VirtualMouse`], the radial and the HUD actions here.
//!
//! **Look is the mouse's right-button look.** The right stick holds the virtual right button at
//! the centre of the screen and moves it, so the camera turns and the character faces it exactly
//! as a mouse player's does — through benilla's own mouse-look, its `MOUSE_LOOK_SPEED` slider and
//! its pitch clamp. Radians become pixels through that same [`LookConfig`], so the controller's
//! turn rate means what it says whatever the slider is at.
//!
//! **Cursor mode** turns the right stick into a pointer and A/B into left/right clicks, for the
//! windows that want one. It switches itself on when such a window opens (loot, a vendor, a quest,
//! the bags, …) and off when the last one closes; on the glue screens it is always on.

use bevy::ecs::system::NonSendMarker;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use benilla_ui::script::plain::PlainValue;
use benilla_ui::script::UiScript;
use benilla_world::view::WorldCamera;
use inilla_core::pad::{
    GlyphStyle, PadBinding, PadButton, PadFrame, PadMapper, PadState, Special, RADIAL_ITEMS,
};
use inilla_core::touch::{MouseButton as KButton, MouseOp};

use super::mouse::VirtualMouse;
use super::synth::Synth;
use super::touch::HudActions;
use super::ui::Overlay;
use super::{Inilla, InputProducers};
use crate::char_select::ClientState;
use crate::player::camera::LookConfig;
use crate::target::Selection;

/// What the controller is doing, for the HUD and for the touch HUD's step-aside.
#[derive(Resource, Default)]
pub(crate) struct PadHud {
    pub(crate) frame: PadFrame,
    /// The connected controller's name, when one is.
    pub(crate) name: Option<String>,
    pub(crate) glyphs: GlyphStyle,
    /// Real-time seconds of the last input.
    pub(crate) last_active: f64,
    /// The virtual cursor, in cursor mode.
    pub(crate) cursor: Option<Vec2>,
}

impl PadHud {
    /// A controller is connected and was used in the last 20 seconds.
    pub(crate) fn active(&self, now: f64) -> bool {
        self.name.is_some() && now - self.last_active < 20.0
    }
}

#[derive(Default)]
struct Look {
    holding: bool,
    last_motion: f64,
    travel: f32,
}

#[derive(Resource, Default)]
struct PadCtl {
    mapper: PadMapper,
    pads: Vec<inilla_platform::gamepad::Pad>,
    look: Look,
    /// Cursor mode was switched on by a window opening (and so closes with it).
    auto_cursor: bool,
    was_cursor: bool,
}

pub(crate) struct PadPlugin;

impl Plugin for PadPlugin {
    fn build(&self, app: &mut App) {
        // A desktop rehearsal reads pads through gilrs, which benilla's boot leaves out.
        if !cfg!(target_os = "ios")
            && std::env::var("INILLA_PAD").as_deref() == Ok("1")
            && !app.is_plugin_added::<bevy::gilrs::GilrsPlugin>()
        {
            app.add_plugins(bevy::gilrs::GilrsPlugin);
        }
        app.init_resource::<PadHud>()
            .init_resource::<PadCtl>()
            .add_systems(PreUpdate, poll.in_set(InputProducers))
            .add_systems(
                Update,
                (auto_cursor.after(crate::ui_script::UiInput), run_actions),
            );
    }
}

/// A Bevy gamepad as a [`PadState`] (desktop).
fn from_bevy(g: &Gamepad) -> PadState {
    let mut s = PadState {
        connected: true,
        left: g.left_stick().to_array(),
        right: g.right_stick().to_array(),
        lt: g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0),
        rt: g.get(GamepadButton::RightTrigger2).unwrap_or(0.0),
        buttons: 0,
    };
    for (b, gb) in [
        (PadButton::A, GamepadButton::South),
        (PadButton::B, GamepadButton::East),
        (PadButton::X, GamepadButton::West),
        (PadButton::Y, GamepadButton::North),
        (PadButton::LB, GamepadButton::LeftTrigger),
        (PadButton::RB, GamepadButton::RightTrigger),
        (PadButton::L3, GamepadButton::LeftThumb),
        (PadButton::R3, GamepadButton::RightThumb),
        (PadButton::Up, GamepadButton::DPadUp),
        (PadButton::Down, GamepadButton::DPadDown),
        (PadButton::Left, GamepadButton::DPadLeft),
        (PadButton::Right, GamepadButton::DPadRight),
        (PadButton::Menu, GamepadButton::Start),
        (PadButton::Options, GamepadButton::Select),
        (PadButton::Home, GamepadButton::Mode),
    ] {
        s.set(b, g.pressed(gb));
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn poll(
    _main: NonSendMarker,
    mut ctl: ResMut<PadCtl>,
    mut hud: ResMut<PadHud>,
    inilla: Res<Inilla>,
    mut synth: ResMut<Synth>,
    mut vm: ResMut<VirtualMouse>,
    mut actions: ResMut<HudActions>,
    gamepads: Query<(&Gamepad, Option<&Name>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    look_cfg: Option<Res<LookConfig>>,
    time: Res<Time<Real>>,
    client: Res<State<ClientState>>,
    overlay: Res<Overlay>,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs().min(0.1);
    let ctl = &mut *ctl;

    // ── read the first connected controller ──
    let (state, name) = if cfg!(target_os = "ios") {
        inilla_platform::gamepad::poll(&mut ctl.pads);
        match ctl.pads.first() {
            Some(p) => (
                p.state,
                Some(format!("{} {}", p.name, p.category).trim().to_string()),
            ),
            None => (PadState::default(), None),
        }
    } else {
        match gamepads.iter().next() {
            Some((g, n)) => (
                from_bevy(g),
                Some(n.map_or_else(|| "Controller".to_string(), |n| n.to_string())),
            ),
            None => (PadState::default(), None),
        }
    };
    if name != hud.name {
        if let Some(n) = &name {
            info!("inilla: controller connected — {n}");
            hud.glyphs = GlyphStyle::from_product(n);
        } else if hud.name.is_some() {
            info!("inilla: controller disconnected");
        }
        hud.name = name;
    }

    let in_world = *client.get() == ClientState::InWorld;
    // A iNilla screen is up: the controller drives egui's own focus instead (not the game).
    let state = if overlay.blocks_game() {
        PadState::default()
    } else {
        state
    };
    if !in_world && state.connected {
        ctl.mapper.set_cursor_mode(true);
    }
    let frame = ctl.mapper.update(&state, &inilla.pad, dt);
    if frame.active {
        hud.last_active = now;
    }

    // ── bindings ──
    synth.movement(&frame.movement);
    for b in &frame.pressed {
        match b {
            PadBinding::Special(Special::Interact) => actions.interact = true,
            PadBinding::Special(Special::InillaMenu) => actions.open_menu = true,
            PadBinding::Special(Special::ToggleTouchHud) => actions.toggle_hud = true,
            PadBinding::Special(Special::LeftClick) => vm.push(MouseOp::Down(KButton::Left)),
            PadBinding::Special(Special::RightClick) => vm.push(MouseOp::Down(KButton::Right)),
            PadBinding::Special(_) => {}
            other => synth.press(other.clone()),
        }
    }
    for b in &frame.released {
        match b {
            PadBinding::Special(Special::LeftClick) => vm.push(MouseOp::Up(KButton::Left)),
            PadBinding::Special(Special::RightClick) => vm.push(MouseOp::Up(KButton::Right)),
            PadBinding::Special(_) => {}
            other => synth.release(other.clone()),
        }
    }
    for b in &frame.held {
        if !matches!(b, PadBinding::Special(_)) {
            synth.hold(b.clone());
        }
    }
    if let Some(i) = frame.radial_pick {
        actions.radial_pick = Some(i);
    }

    let Ok(window) = windows.single() else {
        hud.frame = frame;
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    let center = size / 2.0;

    // ── cursor mode ──
    if frame.cursor_mode {
        let c = (hud.cursor.unwrap_or(center) + Vec2::new(frame.cursor[0], frame.cursor[1]))
            .clamp(Vec2::ZERO, size - Vec2::ONE);
        if hud.cursor != Some(c) {
            vm.push(MouseOp::MoveTo([c.x, c.y]));
        }
        hud.cursor = Some(c);
    } else if ctl.was_cursor {
        hud.cursor = None;
        vm.push(MouseOp::Leave);
    }
    ctl.was_cursor = frame.cursor_mode;

    // ── look: right-button mouse-look at the screen centre ──
    let look = frame.look;
    if look != [0.0, 0.0] && in_world && !frame.cursor_mode {
        let (yaw_rate, pitch_rate) = look_cfg
            .map(|c| (c.yaw_rate().max(1e-5), c.pitch_rate().max(1e-5)))
            .unwrap_or((0.0055, 0.0055));
        // Positive yaw turns left; a mouse turns left moving left (negative x).
        let px = Vec2::new(-look[0] / yaw_rate, -look[1] / pitch_rate);
        if !ctl.look.holding {
            vm.push(MouseOp::MoveTo([center.x, center.y]));
            vm.push(MouseOp::Down(KButton::Right));
            ctl.look.holding = true;
            ctl.look.travel = 0.0;
        }
        vm.push(MouseOp::Motion([px.x, px.y]));
        ctl.look.travel += px.length();
        ctl.look.last_motion = now;
    } else if ctl.look.holding && now - ctl.look.last_motion > 0.2 {
        // A flick too small to count as a drag would release as a right CLICK at the centre of the
        // screen — an attack on whatever stands there. Spend the drag budget with a net-zero wiggle
        // first so the release is always a look's.
        if ctl.look.travel < 24.0 {
            vm.push(MouseOp::Motion([16.0, 0.0]));
            vm.push(MouseOp::Motion([-16.0, 0.0]));
        }
        vm.push(MouseOp::Up(KButton::Right));
        ctl.look.holding = false;
    }
    hud.frame = frame;
}

/// The windows that want a pointer. Checked four times a second while a controller is in use.
const POINTER_WINDOWS: &[&str] = &[
    "LootFrame",
    "MerchantFrame",
    "GossipFrame",
    "QuestFrame",
    "TaxiFrame",
    "ClassTrainerFrame",
    "BankFrame",
    "MailFrame",
    "TradeFrame",
    "AuctionFrame",
    "StaticPopup1",
    "GameMenuFrame",
    "CharacterFrame",
    "SpellBookFrame",
    "QuestLogFrame",
    "FriendsFrame",
    "WorldMapFrame",
    "TalentFrame",
    "MacroFrame",
    "KeyBindingFrame",
    "OptionsFrame",
    "UIOptionsFrame",
    "SoundOptionsFrame",
    "ContainerFrame1",
    "ItemTextFrame",
    "TabardFrame",
    "GuildRegistrarFrame",
    "PetStableFrame",
    "CraftFrame",
    "TradeSkillFrame",
];

fn pointer_window_chunk() -> String {
    let names: Vec<String> = POINTER_WINDOWS.iter().map(|n| format!("\"{n}\"")).collect();
    format!(
        "local n = {{{}}} for i = 1, table.getn(n) do local f = getglobal(n[i]) \
         if f and f:IsVisible() then return true end end return false",
        names.join(",")
    )
}

fn auto_cursor(
    mut ctl: ResMut<PadCtl>,
    hud: Res<PadHud>,
    inilla: Res<Inilla>,
    script: Option<NonSend<UiScript>>,
    client: Res<State<ClientState>>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
    mut chunk: Local<Option<String>>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next || !inilla.pad.auto_cursor || !hud.active(now) {
        return;
    }
    *next = now + 0.25;
    if *client.get() != ClientState::InWorld {
        return;
    }
    let Some(script) = script else {
        return;
    };
    let chunk = chunk.get_or_insert_with(pointer_window_chunk);
    let open = matches!(
        script
            .eval_plain(chunk)
            .ok()
            .and_then(|v| v.into_iter().next()),
        Some(PlainValue::Bool(true))
    );
    let on = ctl.mapper.cursor_mode();
    if open && !on {
        ctl.mapper.set_cursor_mode(true);
        ctl.auto_cursor = true;
    } else if !open && on && ctl.auto_cursor {
        ctl.mapper.set_cursor_mode(false);
        ctl.auto_cursor = false;
    } else if !on {
        ctl.auto_cursor = false;
    }
}

/// The HUD actions both the touch HUD and the controller raise.
#[allow(clippy::too_many_arguments)]
fn run_actions(
    mut actions: ResMut<HudActions>,
    mut synth: ResMut<Synth>,
    mut vm: ResMut<VirtualMouse>,
    mut overlay: ResMut<Overlay>,
    mut inilla: ResMut<Inilla>,
    selection: Option<Res<Selection>>,
    cams: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    transforms: Query<&GlobalTransform>,
) {
    if let Some(i) = actions.radial_pick.take() {
        if let Some((_, cmd)) = RADIAL_ITEMS.get(i) {
            synth.press(PadBinding::cmd(cmd));
            synth.release(PadBinding::cmd(cmd));
        }
        if inilla.settings.haptics {
            inilla_platform::device::haptic(inilla_platform::device::Haptic::Selection);
        }
    }
    if std::mem::take(&mut actions.open_menu) {
        overlay.toggle_menu();
    }
    if std::mem::take(&mut actions.toggle_hud) {
        inilla.layout.enabled = !inilla.layout.enabled;
    }
    if std::mem::take(&mut actions.interact) {
        // Right-click the target where it stands on screen: the one route 1.12 has for talking,
        // looting, attacking and using — `act_on_right_click` reads the unit under the cursor.
        let target = selection.and_then(|s| s.target);
        let point = target.and_then(|e| {
            let world = transforms.get(e).ok()?.translation() + Vec3::Y * 1.0;
            let (cam, cam_tf) = cams.single().ok()?;
            cam.world_to_viewport(cam_tf, world).ok()
        });
        match point {
            Some(p) => {
                vm.push(MouseOp::MoveTo([p.x, p.y]));
                vm.push(MouseOp::Down(KButton::Right));
                vm.push(MouseOp::Up(KButton::Right));
            }
            // Nothing targeted (or it is off screen): target the nearest friend, as the
            // controller's "use" does in WoW Forever when there is nothing to use.
            None => {
                synth.press(PadBinding::cmd("TARGETNEARESTFRIEND"));
            }
        }
    }
}
