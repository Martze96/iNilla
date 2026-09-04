//! The dialog engine's verbs, app half (decision 1963): the feeds behind the stock
//! `StaticPopup.lua` dialogs benilla never raised and the drains behind their buttons, each to
//! wow-re's `staticpopup-dialog-bindings.md` (VERIFIED at the bytes unless a line says INFERRED).
//!
//! * **Pet trainer** — `SMSG_PET_UNLEARN_CONFIRM {guid, cost}` latches both and owes
//!   `CONFIRM_PET_UNLEARN(cost)`; `ConfirmPetUnlearn()` answers with `CMSG_PET_UNLEARN {guid}` unless
//!   the cost outruns the purse (`ERR_NOT_ENOUGH_MONEY`, nothing sent). The talent-wipe twin
//!   (`crate::ui_talent_wipe`), latch for latch, leash for leash: the trainer walking out of
//!   `INTERACT_DISTANCE` closes the question, which is what `CheckPetUntrainerDist()` polls.
//! * **Instance boot** — every `SMSG_RAID_GROUP_ONLY {delayMs, reason}` fires an event: a positive
//!   delay arms the deadline and `INSTANCE_BOOT_START`, zero clears it and `INSTANCE_BOOT_STOP`,
//!   and only the zero leg names reason 1/2 on screen. `GetInstanceBootTimeRemaining()` reads
//!   whole seconds off the deadline; nothing clears it but a zero packet.
//! * **Area spirit healer** — `SMSG_AREA_SPIRIT_HEALER_TIME {guid, ms}` arms the wave clock and
//!   fires `AREA_SPIRIT_HEALER_IN_RANGE` when the guid is the cached healer's; Accept sends
//!   `0x2E3` with that guid, Cancel is the cancel-aura of spell 2584 plus `_OUT_OF_RANGE`. **The
//!   cache has no writer yet**: the reference's per-frame proximity scan (20 yd to acquire, 22 to
//!   retain, `0x4923b0`) is not carved for its unit filter, so no healer is ever cached here and
//!   Accept stays silent — named in the record, not guessed at.
//! * **Battlefield queue** — `SMSG_BATTLEFIELD_STATUS` fills one of three slots and fires
//!   `UPDATE_BATTLEFIELD_STATUS`; `AcceptBattlefieldPort(index, accept)` sends the slot's map id
//!   with the answer as one byte.
//! * **Meeting stone** — `SMSG 0x295 {areaId, status}` stores the queued area and fires
//!   `MEETINGSTONE_CHANGED`; `CancelMeetingStoneRequest()` sends `0x293` unless in a party led by
//!   someone else (`ERR_MEETING_STONE_NOT_LEADER`). The five status messages the reference prints
//!   are not built: the carve lists the five strings but not which status byte picks which.

use std::time::Instant;

use benilla_protocol::messages::BattlefieldStatus;
use benilla_ui::script::{ScriptValue, UiScript};
use bevy::prelude::*;

use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfGuid, SelfPlayer};
use crate::ui_party::GroupState;
use crate::ui_script::UiInput;
use crate::ui_session::{close_npc_session_out_of_range, NpcSession};

/// The pet trainer's pending question: the latch (`0xc4d7b0/b4`) and the cost (`0xc4d7b8`).
#[derive(Resource, Default)]
pub(crate) struct PetUnlearnState {
    npc: Option<u64>,
    cost: u32,
    ask: bool,
}

impl PetUnlearnState {
    /// The inbound `SMSG_PET_UNLEARN_CONFIRM`: latch and owe the dialog.
    pub(crate) fn ask(&mut self, npc: u64, cost: u32) {
        self.npc = Some(npc);
        self.cost = cost;
        self.ask = true;
    }

    fn pending(&self) -> Option<u64> {
        self.npc
    }
}

impl NpcSession for PetUnlearnState {
    fn npc(&self) -> Option<u64> {
        self.npc
    }
    fn close(&mut self) {
        self.npc = None;
        self.cost = 0;
        self.ask = false;
    }
}

/// The instance-boot clock (`[0xb4e34c]`), and what the last packet owes the UI.
#[derive(Resource, Default)]
pub(crate) struct InstanceBoot {
    deadline: Option<Instant>,
    /// Events owed, in arrival order (`INSTANCE_BOOT_START` / `_STOP`).
    events: Vec<&'static str>,
    /// Error lines owed (`ERR_RAID_GROUP_ONLY` / `_FULL`), the zero-delay leg's.
    errors: Vec<&'static str>,
}

impl InstanceBoot {
    /// `SMSG_RAID_GROUP_ONLY`: `delay > 0` arms, else clears — and the event fires either way.
    pub(crate) fn apply(&mut self, delay_ms: u32, reason: u32, now: Instant) {
        if delay_ms > 0 {
            self.deadline = Some(now + std::time::Duration::from_millis(u64::from(delay_ms)));
            self.events.push("INSTANCE_BOOT_START");
        } else {
            self.deadline = None;
            self.events.push("INSTANCE_BOOT_STOP");
            match reason {
                1 => self.errors.push("ERR_RAID_GROUP_ONLY"),
                2 => self.errors.push("ERR_RAID_GROUP_FULL"),
                _ => {}
            }
        }
    }

    /// Whole seconds left, 0 when idle or past — the reference's unsigned divide of a clamped
    /// millisecond remainder.
    pub(crate) fn secs(&self, now: Instant) -> u32 {
        self.deadline
            .map(|d| d.saturating_duration_since(now).as_secs())
            .map_or(0, |s| u32::try_from(s).unwrap_or(u32::MAX))
    }
}

/// The current-area spirit healer (`[0xb4e330/334]`) and its wave clock (`[0xb4e338]`).
#[derive(Resource, Default)]
pub(crate) struct AreaSpiritHealer {
    /// The cached healer. No writer yet — see the module doc.
    healer: Option<u64>,
    deadline: Option<Instant>,
    in_range: bool,
}

impl AreaSpiritHealer {
    /// `SMSG_AREA_SPIRIT_HEALER_TIME`: for the cached healer with a positive time, arm the clock
    /// (a zero-landing deadline reads as 1 ms in the reference) and owe `_IN_RANGE`.
    pub(crate) fn on_time(&mut self, healer: u64, ms: u32, now: Instant) {
        if self.healer == Some(healer) && ms > 0 {
            self.deadline = Some(now + std::time::Duration::from_millis(u64::from(ms)));
            self.in_range = true;
        }
    }

    fn secs(&self, now: Instant) -> u32 {
        self.deadline
            .map(|d| d.saturating_duration_since(now).as_secs())
            .map_or(0, |s| u32::try_from(s).unwrap_or(u32::MAX))
    }
}

/// The three battleground queue slots (`0xb6e9d0`, stride `0x20`).
#[derive(Resource, Default)]
pub(crate) struct BattlefieldQueue {
    slots: [Option<BattlefieldStatus>; 3],
    changed: bool,
}

impl BattlefieldQueue {
    /// `SMSG_BATTLEFIELD_STATUS`: an out-of-range slot aborts the handler; a zero map clears.
    pub(crate) fn apply(&mut self, status: BattlefieldStatus) {
        let Some(slot) = self.slots.get_mut(status.slot as usize) else {
            return;
        };
        *slot = (status.map_id != 0).then_some(status);
        self.changed = true;
    }

    /// The map id `AcceptBattlefieldPort` sends for a 1-based slot, if the slot holds a queue.
    fn map_id(&self, index: u8) -> Option<u32> {
        self.slots
            .get(usize::from(index).checked_sub(1)?)
            .and_then(|s| s.as_ref())
            .map(|s| s.map_id)
    }
}

/// The meeting-stone queue (`[0xb72038]`).
#[derive(Resource, Default)]
pub(crate) struct MeetingStone {
    pub(crate) area: u32,
    pub(crate) status: Option<u8>,
    changed: bool,
}

impl MeetingStone {
    /// `SMSG 0x295`: the area is stored unconditionally; the status picks the (unbuilt) message.
    pub(crate) fn apply(&mut self, area: u32, status: u8) {
        self.area = area;
        self.status = Some(status);
        self.changed = true;
    }
}

fn feed_dialog_verbs(
    script: Option<NonSendMut<UiScript>>,
    mut pet: ResMut<PetUnlearnState>,
    mut boot: ResMut<InstanceBoot>,
    mut spirit: ResMut<AreaSpiritHealer>,
    mut queue: ResMut<BattlefieldQueue>,
    mut stone: ResMut<MeetingStone>,
    mut sink: crate::ui_action::MessageSink,
) {
    let Some(mut script) = script else {
        return;
    };
    let now = Instant::now();

    script.set_pet_untrainer_pending(pet.pending().is_some());
    if pet.ask {
        pet.ask = false;
        script.fire_event(
            "CONFIRM_PET_UNLEARN",
            vec![ScriptValue::Int(i64::from(pet.cost))],
        );
    }

    script.set_instance_boot_secs(boot.secs(now));
    for event in std::mem::take(&mut boot.events) {
        script.fire_event(event, vec![]);
    }
    let lines: Vec<_> = std::mem::take(&mut boot.errors)
        .into_iter()
        .filter_map(|key| crate::ui_action::keyed_line(&script, key))
        .collect();
    if !lines.is_empty() {
        crate::ui_action::show_messages(&mut script, &mut sink, "ui_dialog_verbs", lines);
    }

    let secs = spirit.secs(now);
    script.set_area_spirit_healer(spirit.healer.is_some(), secs);
    if std::mem::take(&mut spirit.in_range) {
        script.fire_event("AREA_SPIRIT_HEALER_IN_RANGE", vec![]);
    }

    if std::mem::take(&mut queue.changed) {
        script.fire_event("UPDATE_BATTLEFIELD_STATUS", vec![]);
    }
    if std::mem::take(&mut stone.changed) {
        script.fire_event("MEETINGSTONE_CHANGED", vec![]);
    }
}

/// The pet trainer's confirm and the spirit healer's accept — the two drains over a latch.
fn drain_latch_verbs(
    script: Option<NonSendMut<UiScript>>,
    pet: Res<PetUnlearnState>,
    spirit: Res<AreaSpiritHealer>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    commands: Res<NetCommands>,
    mut sink: crate::ui_action::MessageSink,
) {
    let Some(mut script) = script else {
        return;
    };

    // ConfirmPetUnlearn: the latch, then the money gate — `cost > coinage` shows
    // ERR_NOT_ENOUGH_MONEY and sends nothing; otherwise `0x2F0` with the latched guid.
    let confirms = script.take_pet_unlearn_confirms();
    if confirms > 0 {
        if let Some(npc) = pet.pending() {
            let money = self_q
                .single()
                .ok()
                .and_then(|store| store.0.player_money())
                .unwrap_or(0);
            if pet.cost > money {
                if let Some(line) = crate::ui_action::keyed_line(&script, "ERR_NOT_ENOUGH_MONEY") {
                    crate::ui_action::show_messages(
                        &mut script,
                        &mut sink,
                        "ui_dialog_verbs",
                        [line],
                    );
                }
            } else {
                for _ in 0..confirms {
                    let _ = commands.0.send(ClientCommand::PetUnlearn { trainer: npc });
                }
            }
        }
    }

    // AcceptAreaSpiritHeal: the cached healer's guid (the binding was silent without one).
    let accepts = script.take_area_spirit_accepts();
    if let Some(healer) = spirit.healer {
        for _ in 0..accepts {
            let _ = commands
                .0
                .send(ClientCommand::AreaSpiritHealerQueue { healer });
        }
    }
}

/// The battleground port and the meeting-stone leave — the two drains over a queue.
fn drain_queue_verbs(
    script: Option<NonSendMut<UiScript>>,
    queue: Res<BattlefieldQueue>,
    group: Option<Res<GroupState>>,
    self_guid: Res<SelfGuid>,
    commands: Res<NetCommands>,
    mut sink: crate::ui_action::MessageSink,
) {
    let Some(mut script) = script else {
        return;
    };

    // AcceptBattlefieldPort: the slot's map id and the one-byte answer.
    for (index, accept) in script.take_battlefield_port_requests() {
        if let Some(map_id) = queue.map_id(index) {
            let _ = commands
                .0
                .send(ClientCommand::BattlefieldPort { map_id, accept });
        }
    }

    // CancelMeetingStoneRequest: in a party and not its leader → ERR_MEETING_STONE_NOT_LEADER;
    // otherwise `0x293`, whatever is or is not queued.
    let cancels = script.take_meeting_stone_cancels();
    if cancels > 0 {
        let not_leader = group
            .as_deref()
            .is_some_and(|g| g.in_group && Some(g.leader) != self_guid.0);
        if not_leader {
            if let Some(line) =
                crate::ui_action::keyed_line(&script, "ERR_MEETING_STONE_NOT_LEADER")
            {
                crate::ui_action::show_messages(&mut script, &mut sink, "ui_dialog_verbs", [line]);
            }
        } else {
            for _ in 0..cancels {
                let _ = commands.0.send(ClientCommand::MeetingStoneLeave);
            }
        }
    }
}

pub(crate) struct UiDialogVerbsPlugin;

impl Plugin for UiDialogVerbsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PetUnlearnState>()
            .init_resource::<InstanceBoot>()
            .init_resource::<AreaSpiritHealer>()
            .init_resource::<BattlefieldQueue>()
            .init_resource::<MeetingStone>()
            .add_systems(
                Update,
                (
                    close_npc_session_out_of_range::<PetUnlearnState>.before(feed_dialog_verbs),
                    feed_dialog_verbs.before(UiInput),
                    drain_latch_verbs.after(UiInput),
                    drain_queue_verbs.after(UiInput),
                ),
            );
    }
}

/// The area spirit healer's aura, `0xA18` — `CancelAreaSpiritHeal`'s one spell (the engine's
/// `dialog_verbs::AREA_SPIRIT_HEALER_SPELL`), read here only to check its flags against the data.
#[cfg(test)]
const AREA_SPIRIT_HEALER_SPELL: u32 = 2584;

/// The generic cancel-aura routine's refusal (`0x6e7040`, wow-re `staticpopup-dialog-bindings.md`
/// §6): it returns without sending when the spell's `AttributesEx` has bit 13 set and bit 2
/// clear **and** `0x5ee290(player)` holds. Whether the third leg ever matters for spell 2584 is
/// decided by the first two, read off the shipped Spell.dbc in [`tests::spell_2584_never_trips_the_cancel_gate`].
#[cfg(test)]
fn cancel_gate_could_apply(attributes_ex: u32) -> bool {
    attributes_ex & 0x2000 != 0 && attributes_ex & 0x4 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spell 2584's flags off the shipped Spell.dbc: the cancel-aura gate's first two legs.
    #[test]
    fn spell_2584_never_trips_the_cancel_gate() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let catalog = benilla_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
        let ex = catalog
            .get(AREA_SPIRIT_HEALER_SPELL)
            .map(|d| d.attributes_ex)
            .expect("spell 2584 in Spell.dbc");
        assert!(
            !cancel_gate_could_apply(ex),
            "spell 2584 AttributesEx = {ex:#x}: the gate's third leg would decide, and it is uncarved"
        );
    }

    #[test]
    fn the_boot_clock_arms_on_a_delay_and_clears_on_zero_with_the_reason() {
        let mut boot = InstanceBoot::default();
        let now = Instant::now();
        boot.apply(60_500, 0, now);
        assert_eq!(boot.events, vec!["INSTANCE_BOOT_START"]);
        assert_eq!(boot.secs(now), 60, "whole seconds, truncated");
        boot.apply(0, 1, now);
        assert_eq!(
            boot.events,
            vec!["INSTANCE_BOOT_START", "INSTANCE_BOOT_STOP"]
        );
        assert_eq!(boot.errors, vec!["ERR_RAID_GROUP_ONLY"]);
        assert_eq!(boot.secs(now), 0);
        boot.apply(0, 7, now);
        assert_eq!(boot.errors.len(), 1, "a reason outside 1/2 names nothing");
    }

    #[test]
    fn the_spirit_healer_clock_only_arms_for_the_cached_healer() {
        let mut spirit = AreaSpiritHealer::default();
        let now = Instant::now();
        spirit.on_time(0x77, 30_000, now);
        assert!(!spirit.in_range, "no healer cached: the packet is ignored");
        spirit.healer = Some(0x77);
        spirit.on_time(0x78, 30_000, now);
        assert!(!spirit.in_range, "another guid: ignored");
        spirit.on_time(0x77, 0, now);
        assert!(!spirit.in_range, "a zero time: ignored");
        spirit.on_time(0x77, 30_000, now);
        assert!(spirit.in_range);
        assert_eq!(spirit.secs(now), 30);
    }

    #[test]
    fn the_queue_keeps_three_slots_and_answers_a_port_by_map() {
        let mut q = BattlefieldQueue::default();
        let status = |slot, map_id| BattlefieldStatus {
            slot,
            map_id,
            bracket: 0,
            unknown: 0,
            status: 2,
            time_ms: Some(0),
            in_progress: None,
        };
        q.apply(status(1, 489));
        q.apply(status(5, 30));
        assert_eq!(
            q.map_id(2),
            Some(489),
            "1-based from Lua, 0-based on the wire"
        );
        assert_eq!(q.map_id(1), None);
        assert_eq!(q.map_id(4), None);
        q.apply(status(1, 0));
        assert_eq!(q.map_id(2), None, "a zero map clears the slot");
    }

    #[test]
    fn the_pet_question_latches_and_closes_like_the_talent_wipe() {
        let mut pet = PetUnlearnState::default();
        pet.ask(0x2b, 10_000);
        assert_eq!(pet.pending(), Some(0x2b));
        assert!(pet.ask);
        pet.close();
        assert_eq!(pet.pending(), None);
        assert_eq!(pet.cost, 0);
    }
}
