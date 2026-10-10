//! **iNilla** — benilla on the iPhone. Everything the desktop client never needed and a phone
//! cannot do without, as one plugin over the unchanged game:
//!
//! - [`touch`] — the touch HUD: a floating movement stick, jump and a ring of action slots, views
//!   to switch between, mouse emulation over the stock interface, drag-to-look, pinch-zoom.
//! - [`pad`] — game controllers (the Backbone One, Xbox, DualSense…) in WoW Forever's layout:
//!   trigger layers for 32 bindings, a radial menu, a virtual cursor for windows.
//! - [`ui`] — iNilla's own screens: first-run setup (game data), servers and accounts, settings,
//!   the HUD layout editor, the log viewer.
//! - [`data`] — getting the game data onto the device: import from Documents, or a resumable,
//!   verified download from a server the player runs.
//! - [`session`] — the launcher's account and server reaching benilla's login, the realm address
//!   override, the automatic realm and character pick.
//! - [`platform`] — the iOS host: window options, screen-awake, the audio session, the on-screen
//!   keyboard, the safe area, the frame cap.
//! - [`console`] — the remote console: the session log, Lua and commands from a browser on the
//!   same Wi-Fi, for fast debugging without a Mac.
//!
//! Input from the HUD, the controller and the console reaches the game through two seams only:
//! [`synth`] (binding commands by name, into `BindingsState`) and [`mouse`] (an ordinary mouse,
//! as Bevy input messages). Neither can do anything a keyboard and mouse could not.

pub(crate) mod boot;
mod console;
mod data;
mod hints;
mod hud;
mod icons;
mod mouse;
mod pad;
mod platform;
mod session;
mod store;
mod synth;
mod touch;
mod ui;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::CameraOutputMode;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiContextSettings, EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};

use inilla_core::pad::PadProfile;
use inilla_core::profiles::Launcher;
use inilla_core::settings::InillaSettings;
use inilla_core::touch::TouchLayout;

use crate::char_select::ClientState;

pub(crate) use session::world_address;

/// Everything iNilla persists, loaded at build and saved (debounced) when it changes.
#[derive(Resource, Clone)]
pub(crate) struct Inilla {
    pub(crate) settings: InillaSettings,
    pub(crate) launcher: Launcher,
    pub(crate) layout: TouchLayout,
    pub(crate) pad: PadProfile,
}

/// Whether the egui lane (iNilla's HUD and screens share it with benilla's dev panel) must run
/// this frame. The dev panel's demand gate reads it too, so a dev build does not switch the HUD
/// off with its panel.
#[derive(Resource, Default)]
pub(crate) struct EguiWanted(pub(crate) bool);

/// Is this a touch device — iOS, or a desktop run with `$INILLA_TOUCH=1` rehearsing one.
pub(crate) fn touch_device() -> bool {
    cfg!(target_os = "ios") || std::env::var("INILLA_TOUCH").as_deref() == Ok("1")
}

pub(crate) struct InillaPlugin;

impl Plugin for InillaPlugin {
    fn build(&self, app: &mut App) {
        let state = Inilla {
            settings: store::load_settings(),
            launcher: store::load_launcher(),
            layout: store::load_layout(),
            pad: store::load_pad(),
        };
        if let Some(dir) = store::dir() {
            inilla_platform::keychain::set_fallback_dir(dir);
        }
        app.insert_resource(state)
            .init_resource::<EguiWanted>()
            .init_resource::<synth::Synth>()
            .init_resource::<mouse::VirtualMouse>();

        setup_egui(app);

        app.add_plugins((
            platform::PlatformPlugin,
            session::SessionPlugin,
            data::DataPlugin,
            icons::IconPlugin,
            touch::TouchPlugin,
            pad::PadPlugin,
            hud::HudPlugin,
            hints::HintsPlugin,
            ui::UiPlugin,
            console::ConsolePlugin,
        ));

        // The seams. The mouse lands before Bevy's input systems so every reader sees it as a
        // mouse; the commands land in the binding slot, as wenilla's bridge does.
        app.add_systems(
            PreUpdate,
            mouse::apply
                .after(InputProducers)
                .before(bevy::input::InputSystems),
        )
        .configure_sets(PreUpdate, InputProducers.before(bevy::input::InputSystems))
        .add_systems(
            Update,
            synth::run_vm_ops
                .after(benilla_world::schedule::WorldStage::Net)
                .before(crate::ui_script::UiInput),
        )
        .add_systems(
            Update,
            synth::apply
                .in_set(crate::ui_script::UiInput)
                .after(crate::bindings::BindingSet)
                .before(benilla_world::schedule::WorldStage::Input),
        )
        .add_systems(OnEnter(ClientState::InWorld), synth::release_all)
        .add_systems(OnExit(ClientState::InWorld), synth::release_all)
        .add_systems(Last, save_when_changed)
        .add_systems(Update, follow_data_language);
    }
}

/// The game data's language, handed to the two places that report it: the logon challenge (so
/// the server sends its quests, items and NPC names in it) and `GetLocale()` (so addons such as
/// pfQuest pick their own translation). Once, as soon as the archives are open.
fn follow_data_language(mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(locale) = benilla_formats::data_locale() else {
        return;
    };
    *done = true;
    benilla_protocol::auth::set_client_locale(locale);
    benilla_ui::script::set_locale(locale);
    info!("inilla: the game data is {locale}");
}

/// The `PreUpdate` systems that produce synthetic input (touch, controller) — ahead of
/// [`mouse::apply`], which carries it into Bevy's input.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InputProducers;

/// The egui lane. A dev build already has one (the debug panel's overlay camera, gated on demand);
/// a player build gets the same camera from here, and the same gate driven by [`EguiWanted`].
fn setup_egui(app: &mut App) {
    if app.is_plugin_added::<EguiPlugin>() {
        return;
    }
    app.add_plugins(EguiPlugin::default());
    {
        let mut egui = app.world_mut().resource_mut::<EguiGlobalSettings>();
        egui.auto_create_primary_context = false;
        egui.enable_cursor_icon_updates = false;
    }
    // After `hud::want_egui` (which runs before `UiInput`), so the gate follows this frame's
    // demand: gated first, the lane slept through the first frame.
    app.add_systems(Startup, spawn_overlay_camera)
        .add_systems(Update, gate_lane.after(crate::ui_script::UiInput));
}

/// Is the egui lane up, the primary context running a real pass this frame? Asleep
/// (`run_manually`), bevy_egui still runs `EguiPrimaryContextPass` once outside any pass — its
/// single-pass fallback — where a context that has never had a pass has no fonts and no layout,
/// so the first text drawn there panics. iNilla's egui systems run only while this holds.
pub(crate) fn lane_up(contexts: Query<&EguiContextSettings, With<PrimaryEguiContext>>) -> bool {
    contexts.iter().any(|s| !s.run_manually)
}

/// benilla's egui overlay camera, as the debug panel spawns it (`debug_panel::spawn_egui_camera`
/// carries the reasons for every field: no MSAA so the two 2D cameras agree on a depth texture,
/// a premultiplied write over the finished frame, a clear to transparent).
fn spawn_overlay_camera(mut commands: Commands) {
    commands.spawn((
        PrimaryEguiContext,
        Name::new("inilla egui overlay camera"),
        Camera2d,
        bevy::render::view::Msaa::Off,
        RenderLayers::none(),
        Camera {
            order: 2,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            msaa_writeback: bevy::camera::MsaaWriteback::Off,
            ..default()
        },
    ));
}

/// The player build's demand gate: the lane sleeps when iNilla has nothing to draw.
fn gate_lane(
    wanted: Res<EguiWanted>,
    mut cams: Query<(&mut Camera, &mut EguiContextSettings), With<PrimaryEguiContext>>,
    mut over: ResMut<crate::ui_script::EguiPointerOver>,
) {
    for (mut cam, mut settings) in &mut cams {
        if cam.is_active != wanted.0 {
            cam.is_active = wanted.0;
            if !wanted.0 {
                over.0 = false;
            }
        }
        if settings.run_manually == wanted.0 {
            settings.run_manually = !wanted.0;
        }
    }
}

/// Save what moved, at most once a second.
fn save_when_changed(
    state: Res<Inilla>,
    time: Res<Time<Real>>,
    mut saved: Local<Option<Inilla>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let Some(prev) = saved.as_ref() else {
        *saved = Some(state.clone());
        return;
    };
    if prev.settings != state.settings {
        store::save_settings(&state.settings);
    }
    if prev.launcher != state.launcher {
        store::save_launcher(&state.launcher);
    }
    if prev.layout != state.layout {
        store::save_layout(&state.layout);
    }
    if prev.pad != state.pad {
        store::save_pad(&state.pad);
    }
    *saved = Some(state.clone());
}
