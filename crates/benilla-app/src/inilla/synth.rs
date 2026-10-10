//! **The command seam** — how the touch HUD, the controller and the remote console press game
//! commands. Commands are addressed **by their `Bindings.xml` name** and go through
//! [`BindingsState`] and [`UiScript::execute_binding`], so rebinding a key does not change what
//! a HUD button does.

use std::collections::HashSet;

use bevy::prelude::*;

use benilla_ui::script::plain::PlainValue;
use benilla_ui::script::UiScript;
use inilla_core::pad::PadBinding;

use crate::bindings::BindingsState;
use crate::ui_script::UiKeyboardCapture;

pub(crate) const COMMANDS: &[(&str, &str)] = &[
    ("MOVEANDSTEER", "BINDING_HEADER_MOVEMENT"),
    ("MOVEFORWARD", "BINDING_HEADER_MOVEMENT"),
    ("MOVEBACKWARD", "BINDING_HEADER_MOVEMENT"),
    ("TURNLEFT", "BINDING_HEADER_MOVEMENT"),
    ("TURNRIGHT", "BINDING_HEADER_MOVEMENT"),
    ("STRAFELEFT", "BINDING_HEADER_MOVEMENT"),
    ("STRAFERIGHT", "BINDING_HEADER_MOVEMENT"),
    ("JUMP", "BINDING_HEADER_MOVEMENT"),
    ("SITORSTAND", "BINDING_HEADER_MOVEMENT"),
    ("TOGGLESHEATH", "BINDING_HEADER_MOVEMENT"),
    ("TOGGLEAUTORUN", "BINDING_HEADER_MOVEMENT"),
    ("TOGGLERUN", "BINDING_HEADER_MOVEMENT"),
    ("FOLLOWTARGET", "BINDING_HEADER_MOVEMENT"),
    ("OPENCHAT", "BINDING_HEADER_CHAT"),
    ("OPENCHATSLASH", "BINDING_HEADER_CHAT"),
    ("CHATPAGEUP", "BINDING_HEADER_CHAT"),
    ("CHATPAGEDOWN", "BINDING_HEADER_CHAT"),
    ("CHATBOTTOM", "BINDING_HEADER_CHAT"),
    ("REPLY", "BINDING_HEADER_CHAT"),
    ("REPLY2", "BINDING_HEADER_CHAT"),
    ("COMBATLOGPAGEUP", "BINDING_HEADER_CHAT"),
    ("COMBATLOGPAGEDOWN", "BINDING_HEADER_CHAT"),
    ("COMBATLOGBOTTOM", "BINDING_HEADER_CHAT"),
    ("TOGGLECOMBATLOG", "BINDING_HEADER_CHAT"),
    ("ACTIONBUTTON1", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON2", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON3", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON4", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON5", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON6", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON7", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON8", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON9", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON10", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON11", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONBUTTON12", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON1", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON2", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON3", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON4", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON5", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON6", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON7", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON8", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON9", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON10", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON11", "BINDING_HEADER_ACTIONBAR"),
    ("SELFACTIONBUTTON12", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON1", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON2", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON3", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON4", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON5", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON6", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON7", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON8", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON9", "BINDING_HEADER_ACTIONBAR"),
    ("SHAPESHIFTBUTTON10", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON1", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON2", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON3", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON4", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON5", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON6", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON7", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON8", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON9", "BINDING_HEADER_ACTIONBAR"),
    ("BONUSACTIONBUTTON10", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE1", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE2", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE3", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE4", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE5", "BINDING_HEADER_ACTIONBAR"),
    ("ACTIONPAGE6", "BINDING_HEADER_ACTIONBAR"),
    ("PREVIOUSACTIONPAGE", "BINDING_HEADER_ACTIONBAR"),
    ("NEXTACTIONPAGE", "BINDING_HEADER_ACTIONBAR"),
    ("TOGGLEACTIONBARLOCK", "BINDING_HEADER_ACTIONBAR"),
    ("TOGGLEAUTOSELFCAST", "BINDING_HEADER_ACTIONBAR"),
    ("TARGETNEARESTENEMY", "BINDING_HEADER_TARGETING"),
    ("TARGETPREVIOUSENEMY", "BINDING_HEADER_TARGETING"),
    ("TARGETNEARESTFRIEND", "BINDING_HEADER_TARGETING"),
    ("TARGETPREVIOUSFRIEND", "BINDING_HEADER_TARGETING"),
    ("TARGETSELF", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYMEMBER1", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYMEMBER2", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYMEMBER3", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYMEMBER4", "BINDING_HEADER_TARGETING"),
    ("TARGETPET", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYPET1", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYPET2", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYPET3", "BINDING_HEADER_TARGETING"),
    ("TARGETPARTYPET4", "BINDING_HEADER_TARGETING"),
    ("TARGETLASTHOSTILE", "BINDING_HEADER_TARGETING"),
    ("ASSISTTARGET", "BINDING_HEADER_TARGETING"),
    ("NAMEPLATES", "BINDING_HEADER_TARGETING"),
    ("FRIENDNAMEPLATES", "BINDING_HEADER_TARGETING"),
    ("ALLNAMEPLATES", "BINDING_HEADER_TARGETING"),
    ("ATTACKTARGET", "BINDING_HEADER_TARGETING"),
    ("PETATTACK", "BINDING_HEADER_TARGETING"),
    ("TOGGLECHARACTER0", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBACKPACK", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBAG1", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBAG2", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBAG3", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBAG4", "BINDING_HEADER_INTERFACE"),
    ("OPENALLBAGS", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEKEYRING", "BINDING_HEADER_INTERFACE"),
    ("TOGGLESPELLBOOK", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEPETBOOK", "BINDING_HEADER_INTERFACE"),
    ("TOGGLETALENTS", "BINDING_HEADER_INTERFACE"),
    ("TOGGLECHARACTER4", "BINDING_HEADER_INTERFACE"),
    ("TOGGLECHARACTER3", "BINDING_HEADER_INTERFACE"),
    ("TOGGLECHARACTER2", "BINDING_HEADER_INTERFACE"),
    ("TOGGLECHARACTER1", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEQUESTLOG", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEGAMEMENU", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEMINIMAP", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEWORLDMAP", "BINDING_HEADER_INTERFACE"),
    ("TOGGLESOCIAL", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEFRIENDSTAB", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEWHOTAB", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEGUILDTAB", "BINDING_HEADER_INTERFACE"),
    ("TOGGLERAIDTAB", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEWORLDSTATESCORES", "BINDING_HEADER_INTERFACE"),
    ("TOGGLEBATTLEFIELDMINIMAP", "BINDING_HEADER_INTERFACE"),
    ("MINIMAPZOOMIN", "BINDING_HEADER_MISC"),
    ("MINIMAPZOOMOUT", "BINDING_HEADER_MISC"),
    ("TOGGLEMUSIC", "BINDING_HEADER_MISC"),
    ("TOGGLESOUND", "BINDING_HEADER_MISC"),
    ("MASTERVOLUMEUP", "BINDING_HEADER_MISC"),
    ("MASTERVOLUMEDOWN", "BINDING_HEADER_MISC"),
    ("TOGGLEUI", "BINDING_HEADER_MISC"),
    ("TOGGLEFPS", "BINDING_HEADER_MISC"),
    ("SCREENSHOT", "BINDING_HEADER_MISC"),
    ("NEXTVIEW", "BINDING_HEADER_CAMERA"),
    ("PREVVIEW", "BINDING_HEADER_CAMERA"),
    ("CAMERAZOOMIN", "BINDING_HEADER_CAMERA"),
    ("CAMERAZOOMOUT", "BINDING_HEADER_CAMERA"),
    ("SETVIEW1", "BINDING_HEADER_CAMERA"),
    ("SETVIEW2", "BINDING_HEADER_CAMERA"),
    ("SETVIEW3", "BINDING_HEADER_CAMERA"),
    ("SETVIEW4", "BINDING_HEADER_CAMERA"),
    ("SETVIEW5", "BINDING_HEADER_CAMERA"),
    ("SAVEVIEW2", "BINDING_HEADER_CAMERA"),
    ("SAVEVIEW3", "BINDING_HEADER_CAMERA"),
    ("SAVEVIEW4", "BINDING_HEADER_CAMERA"),
    ("SAVEVIEW5", "BINDING_HEADER_CAMERA"),
    ("RESETVIEW2", "BINDING_HEADER_CAMERA"),
    ("RESETVIEW3", "BINDING_HEADER_CAMERA"),
    ("RESETVIEW4", "BINDING_HEADER_CAMERA"),
    ("RESETVIEW5", "BINDING_HEADER_CAMERA"),
    ("FLIPCAMERAYAW", "BINDING_HEADER_CAMERA"),
    ("MULTIACTIONBAR1BUTTON1", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON2", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON3", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON4", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON5", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON6", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON7", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON8", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON9", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON10", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON11", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR1BUTTON12", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON1", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON2", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON3", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON4", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON5", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON6", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON7", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON8", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON9", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON10", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON11", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR2BUTTON12", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON1", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON2", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON3", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON4", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON5", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON6", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON7", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON8", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON9", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON10", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON11", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR3BUTTON12", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON1", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON2", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON3", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON4", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON5", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON6", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON7", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON8", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON9", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON10", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON11", "BINDING_HEADER_MULTIACTIONBAR"),
    ("MULTIACTIONBAR4BUTTON12", "BINDING_HEADER_MULTIACTIONBAR"),
    ("RAIDTARGET1", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET2", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET3", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET4", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET5", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET6", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET7", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGET8", "BINDING_HEADER_RAID_TARGET"),
    ("RAIDTARGETNONE", "BINDING_HEADER_RAID_TARGET"),
];

/// Every command name the registry knows, with its category — the settings' binding picker.
pub(crate) fn command_names() -> impl Iterator<Item = (&'static str, &'static str)> {
    COMMANDS.iter().copied()
}

pub(crate) fn is_movement_or_held(cmd: &str) -> bool {
    matches!(
        cmd,
        "MOVEFORWARD"
            | "MOVEBACKWARD"
            | "TURNLEFT"
            | "TURNRIGHT"
            | "STRAFELEFT"
            | "STRAFERIGHT"
            | "MOVEANDSTEER"
            | "PITCHUP"
            | "PITCHDOWN"
    )
}

/// A Lua evaluation somebody is waiting on (the remote console).
pub(crate) struct LuaJob {
    pub(crate) chunk: String,
    pub(crate) reply: Option<crossbeam_channel::Sender<Result<Vec<PlainValue>, String>>>,
}

/// This frame's synthetic input, from every producer.
#[derive(Resource, Default)]
pub(crate) struct Synth {
    /// Bindings held down right now, re-stated by their producers every frame.
    held: Vec<PadBinding>,
    pressed: Vec<PadBinding>,
    released: Vec<PadBinding>,
    /// Movement commands the sticks hold this frame (always `Held` kinds).
    movement: Vec<&'static str>,
    /// Host commands to fire with an amount (the pinch zoom).
    fires: Vec<(&'static str, f32)>,
    lua: Vec<LuaJob>,
    chat: Vec<String>,
    /// Holds latched last frame, to release what is no longer held.
    latched: HashSet<String>,
    /// Held-kind commands pressed-and-released within one frame: latched this frame, dropped next.
    taps: Vec<String>,
}

impl Synth {
    pub(crate) fn press(&mut self, b: PadBinding) {
        self.pressed.push(b);
    }

    pub(crate) fn release(&mut self, b: PadBinding) {
        self.released.push(b);
    }

    pub(crate) fn hold(&mut self, b: PadBinding) {
        self.held.push(b);
    }

    pub(crate) fn movement(&mut self, cmds: &[&'static str]) {
        self.movement.extend_from_slice(cmds);
    }

    pub(crate) fn fire(&mut self, cmd: &'static str, amount: f32) {
        self.fires.push((cmd, amount));
    }

    pub(crate) fn lua(&mut self, job: LuaJob) {
        self.lua.push(job);
    }

    pub(crate) fn chat(&mut self, line: String) {
        self.chat.push(line);
    }
}

/// Apply the frame's synthetic input. Runs in the binding slot: after the key dispatch cleared
/// this frame's edges and before the player controller reads them (`BindingSet` → `WorldStage::Input`).
pub(crate) fn apply(
    mut binds: ResMut<BindingsState>,
    mut synth: ResMut<Synth>,
    capture: Res<UiKeyboardCapture>,
    mut script: Option<NonSendMut<UiScript>>,
) {
    let synth = &mut *synth;
    let pressed = std::mem::take(&mut synth.pressed);
    let released = std::mem::take(&mut synth.released);
    let held = std::mem::take(&mut synth.held);
    let movement = std::mem::take(&mut synth.movement);
    let fires = std::mem::take(&mut synth.fires);

    // What should be latched this frame.
    let mut want: HashSet<String> = HashSet::new();
    if !capture.typing {
        for name in movement {
            want.insert(name.to_string());
        }
        for b in &held {
            if let PadBinding::Command(name) = b {
                if is_movement_or_held(name) {
                    want.insert(name.clone());
                }
            }
        }
        for c in synth.taps.drain(..) {
            want.insert(c);
        }
    } else {
        synth.taps.clear();
    }

    let run = |script: &mut Option<NonSendMut<UiScript>>, body: &str| {
        if let Some(s) = script.as_deref() {
            crate::ui_script::run_or_warn(s, body);
        }
    };

    for b in &pressed {
        match b {
            PadBinding::Command(name) => {
                if is_movement_or_held(name) {
                    if !capture.typing && !want.contains(name) {
                        want.insert(name.clone());
                        synth.taps.push(name.clone());
                    }
                } else if let Some(s) = script.as_mut() {
                    if let Err(e) = s.execute_binding(name, true) {
                        warn!("inilla: synth press({name}): {e}");
                    }
                }
            }
            PadBinding::Lua(chunk) => run(&mut script, chunk),
            // Serviced by `run_vm_ops` at the top of the next frame, through the chat box's
            // own submit path, so slash commands parse client-side exactly as typed.
            PadBinding::Chat(line) => synth.chat.push(line.clone()),
            PadBinding::None | PadBinding::Special(_) => {}
        }
    }
    for b in &released {
        if let PadBinding::Command(name) = b {
            if !is_movement_or_held(name) {
                if let Some(s) = script.as_mut() {
                    if let Err(e) = s.execute_binding(name, false) {
                        warn!("inilla: synth release({name}): {e}");
                    }
                }
            }
        }
    }
    for (name, amount) in fires {
        match name {
            "CAMERAZOOMIN" => binds.synth_fire(crate::bindings::Input::CameraZoomIn, amount),
            "CAMERAZOOMOUT" => binds.synth_fire(crate::bindings::Input::CameraZoomOut, amount),
            _ => {
                if let Some(s) = script.as_mut() {
                    let _ = s.execute_binding(name, true);
                }
            }
        }
    }

    for c in synth.latched.difference(&want) {
        binds.synth_release(&mut script, c);
    }
    for c in &want {
        binds.synth_hold(&mut script, c);
    }
    synth.latched = want;

    // Apply any binding calls made by Lua during execute_binding directly into BindingsState.
    if let Some(s) = script.as_mut() {
        let inputs = s.take_binding_input();
        binds.apply_script_input(inputs);
    }
}

/// The VM-side ops — chat lines and Lua evaluation — before the VM ticks, so a `/cast` typed by
/// a button is serviced this frame.
pub(crate) fn run_vm_ops(script: Option<NonSendMut<UiScript>>, mut synth: ResMut<Synth>) {
    let Some(mut script) = script else {
        for job in synth.lua.drain(..) {
            if let Some(tx) = job.reply {
                let _ = tx.send(Err("no interface is loaded in this state".into()));
            }
        }
        synth.chat.clear();
        return;
    };
    for line in synth.chat.drain(..) {
        script.push_chat_input(line);
    }
    for job in synth.lua.drain(..) {
        let result = script.eval_plain(&job.chunk);
        if let Some(tx) = job.reply {
            let _ = tx.send(result);
        }
    }
}

/// Drop every synthetic latch at the world's edges, so nothing held on the way out is still held
/// on the way back in.
pub(crate) fn release_all(
    mut binds: ResMut<BindingsState>,
    mut synth: ResMut<Synth>,
    mut script: Option<NonSendMut<UiScript>>,
) {
    binds.synth_release_all(&mut script);
    synth.latched.clear();
    synth.taps.clear();
    synth.held.clear();
    synth.pressed.clear();
    synth.released.clear();
    synth.movement.clear();
    synth.fires.clear();
}
