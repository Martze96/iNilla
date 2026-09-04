//! The **query** half of the overhead questgiver markers: when to ask the server what an NPC's
//! `!`/`?` should be — and, for a GameObject, when to ask a question whose answer is thrown away.
//! The rendering half is [`super`].
//!
//! **A GameObject is queried and never rendered** (decision 1872, wow-re `questgiver-marker.md`
//! §W14). The reference sends `CMSG_QUESTGIVER_STATUS_QUERY` for a quest-flagged GameObject from
//! both object sweeps, but its answer handler `0x5dc9f0` resolves the GUID with typemask **8**
//! (`0x468460` is a bitmask AND against `OBJECT_FIELD_TYPE`), so a GameObject's `0x21` returns NULL
//! and the packet dies at `0x5dca2f` — and even if it did not, a GameObject has no `+0xcb8` status
//! slot, no `+0xb2c` marker slot, and (11 models out of ~1600, none of them a poster) no
//! attachment 18 to hang a marker from. So: **send the query, drop the answer, render nothing.**
//!
//! The server only ever *answers* `CMSG_QUESTGIVER_STATUS_QUERY` (vmangos `QuestHandler.cpp`) — it
//! never pushes — so every refresh point is the client's own to trigger, and a status that is never
//! re-asked for is a marker frozen at whatever it was when we first saw the NPC. That made this a
//! surprisingly deep fidelity question; the answer is decisions 0650 (the reference's trigger set,
//! byte-enumerated) and 0654 (what benilla implements of it), and it lives on [`query_statuses`].

use std::collections::HashMap;

use bevy::prelude::*;

use benilla_protocol::EntityKind;

use crate::net::{ClientCommand, GuidIndex, NetCommands, NetEntity, ObjectStore, SelfPlayer};
use crate::target::cursor_mode::go_reaction;
use crate::target::ring::Factions;
use crate::ui_quest::QuestGiver;

/// `UNIT_NPC_FLAGS` questgiver service bit (vanilla; `update_object.rs`'s accessor doc).
const NPC_FLAG_QUESTGIVER: u32 = 0x2;
/// `GAMEOBJECT_FLAGS` bit **2** (mask `0x4`) — the whole of the sweep's GameObject gate, and the
/// one place a GameObject GUID can reach this opcode at all.
///
/// Byte-pinned (wow-re `questgiver-marker.md` §W14.6): `0x5eb0ef mov eax,[edx+0xc];
/// 0x5eb0f2 shr eax,0x2; 0x5eb0f5 test al,1` — shift **2**, so mask `0x4`, not the `0x2` the unit
/// leg tests 25 bytes later. `[<GO block>+0xc]` is absolute field index 9 = `GAMEOBJECT_FLAGS`,
/// named from the binary's own UpdateField name table (`0x83b8a8` → the string at `0x83be84`).
/// The *name* `GO_FLAG_INTERACT_COND` is vmangos's, not the image's — but the bit is settled from
/// shipped content as well as from the branch: across the genuine 2005-06 sniff corpus, all 122
/// GameObjects the real client sent a status query for carry it, and none without it does, against
/// a 79.2 % base rate.
const GO_FLAG_INTERACT_COND: u32 = 0x4;

/// Ask the dialog status of every questgiver-flagged creature, and re-ask when the answer could
/// have changed. The server only ever *answers* queries (vmangos `QuestHandler.cpp`), so every
/// refresh point is the client's own to trigger — a status never re-asked for is a marker frozen
/// at whatever it was when we first saw the NPC.
///
/// The reference's refresh law is a **descriptor field watch**, not a hand-picked event list: it
/// registers handlers on specific self-player fields through `0x468070`, and the handler sweeps
/// every visible object (`0x5eb070` → `0x468380`, one query per object passing the questgiver
/// gate). Byte-pinned in wow-re `questgiver-marker.md` §W1–W12; the benilla side, and the
/// still-open gaps, are decision 0650 (the byte-level trigger set, superseding 0647's wrong
/// "level is a deviation") and decision 0654 (what benilla implements of it).
///
/// **The sweep** — every visible questgiver is re-asked when any *self* input to the server's
/// answer moves. The reference gets this from six descriptor watches plus four packet handlers;
/// ours is [`self_generation`] (the descriptor half, recomputed only when our store actually
/// changes) XOR [`QuestGiver::reask_epoch`] (the packet half, bumped from `net/apply`). A changed
/// generation clears the whole asked set, which is the sweep.
///
/// **Per unit** — the reference queries one unit from its create/init path (`0x607380`) and when
/// its own fields move: the questgiver bit (`0x60b490`), the flightmaster bit (`0x60b4c5`), and
/// anything that could change its *reaction* to us (`0x606f0a`). Ours keys the asked set on
/// [`unit_ask_key`], so any of those re-asks that one NPC; and the entry is dropped when the guid
/// leaves [`GuidIndex`], which is the create-path query (the reference caches the answer *on* the
/// unit at `unit+0xcb8`, so its cache dies with the object — ours is a map and needs the prune).
///
/// **Per GameObject — on a sweep, and ONLY on a sweep.** A quest-giving GameObject (a wanted
/// poster, a suspicious barrel) is a questgiver on the wire exactly as a creature is: the same
/// sweep callback tests typemask bit 5 and sends the same opcode for it (`0x5eb0a0` @ `0x5eb159`,
/// and its sibling `0x5eb3f0` @ `0x5eb456`), gated on [`GO_FLAG_INTERACT_COND`] and on the
/// GameObject's own reaction toward us being **> 1** ([`go_reaction`], the reference's `0x5f7fd0`).
/// The wire corpus shows it happening in 20 of 62 real sessions.
///
/// But there is **no bring-up query for a GameObject** — the contrast with units is exact and it
/// is the point (wow-re §W14.8, a closed caller census: a GameObject GUID can reach this opcode
/// through exactly two instructions in the whole image and both are inside a sweep callback; the
/// `CGGameObject_C` ctor and its vtable slot 3 call no sender, where `CGUnit_C`'s slot 3 does). So
/// a GameObject is asked about only when a sweep happens to run while it is in view, and a player
/// who completes a quest elsewhere and then walks up to the turn-in object never asks about it at
/// all. That is a real, slightly surprising reference behaviour, and **a per-guid asked-set that
/// queries on first sight is a deviation, not a fix** — hence the `swept` gate below rather than
/// an [`unit_ask_key`] analogue. Nothing visible turns on any of this: the answer is dropped
/// ([`crate::net::apply`]'s own typemask gate), which is decision 1872's whole point.
///
/// **The teardown leg** — an NPC whose questgiver bit goes off loses its cached status, so the
/// marker goes with it (`0x5eb0a0`'s own branch), and is re-asked if the bit returns. There is no
/// GameObject counterpart: a GameObject has no status to invalidate (`+0xcb8` is `0x9a8` bytes past
/// the end of a `0x310`-byte `CGGameObject_C`), and all 8 of `0x6073f0`'s call sites are unit-side.
///
/// **Still not covered**, and why: the reference also sweeps when an *item* moves between
/// containers (an `ITEM` typeid watch, `0x5d9375` — quest availability can depend on carried
/// items). benilla does not stream item objects (`EntityKind` has no `Item`), so there is nothing
/// to watch; the equipment and equipped-bag guids in our own descriptor *are* folded, which covers
/// equipping, unequipping and swapping a bag but not moving a stack inside one. Per unit, the
/// reference's reaction refresh also keys on charm/persuade/duel-team/`PLAYER_BYTES_3`; we key on
/// the faction template, and catch a standing change through the reputation sweep instead.
#[allow(clippy::type_complexity)] // a Bevy system: each param is one resource, the app's convention
pub(super) fn query_statuses(
    self_q: Query<Ref<ObjectStore>, With<SelfPlayer>>,
    objects: Query<(&crate::net::Guid, &NetEntity, &ObjectStore), Without<SelfPlayer>>,
    index: Res<GuidIndex>,
    factions: Option<Res<Factions>>,
    mut quest: ResMut<QuestGiver>,
    commands: Res<NetCommands>,
    mut state: Local<QueryState>,
) {
    let Some(store) = self_q.iter().next() else {
        return;
    };
    // The descriptor half is only recomputed when our own store actually changed — it walks 20
    // quest-log slots, 128 skill slots and 23 inventory slots, which has no business running every
    // frame. The packet half (the epoch) is a plain counter, so it is always cheap to fold.
    if store.is_changed() {
        state.fields = self_generation(&store.0);
    }
    let generation = state.fields ^ (u64::from(quest.reask_epoch()) << 32);
    // **This frame IS the sweep.** Both of the reference's object sweeps (`0x5eb070` and the full
    // re-query `0x5eb3c0`) walk every object in the manager once, synchronously, when one of the
    // 13 local-player state changes fires them; a changed generation is that moment, here. The
    // GameObject leg below fires only on it, because that is the only way a GameObject GUID ever
    // reaches the wire (§W14.8).
    let swept = state.generation != generation;
    if swept {
        state.generation = generation;
        state.asked.clear();
    }
    // Object lifetime is the other half of the cache key: a guid that left the world drops both
    // its "already asked" mark and its cached status, so re-entering view re-asks from scratch.
    state.asked.retain(|guid, _| index.0.contains_key(guid));
    quest.retain_statuses(|npc| index.0.contains_key(&npc));
    for (guid, net, obj) in &objects {
        match net.kind {
            // `0x5eb0a0` @ `0x5eb0d3`: `cmp eax,9; je` — typemask EXACTLY `OBJECT|UNIT`, a plain
            // creature. A player is `0x19` and falls out of the sweep here, before any flag test.
            EntityKind::Unit => {
                if obj.0.unit_npc_flags() & NPC_FLAG_QUESTGIVER == 0 {
                    // No longer a questgiver: drop the stale answer (and the marker with it) —
                    // **unconditionally**, never "only if we had asked". The reference's sweep
                    // callback (`0x5eb0a0`) tears the marker down on the flag test alone, with no
                    // prior-asked precondition (decision 0647), and the gate we used to put here
                    // was disarmed by the very sweep that arrives with it: an escort's giver drops
                    // `UNIT_NPC_FLAGS` in the same server tick as the quest-log write (vmangos
                    // `FollowerAI::StartFollow`, `ScriptedEscortAI`'s "disable npcflags"), so the
                    // quest-log change bumps the generation, `asked` is cleared above, `remove`
                    // finds nothing, and the cached AVAILABLE status — with its `!` — was frozen
                    // over the NPC for the whole escort, with no way back: the flag stays off, so
                    // this arm never queries either (B257).
                    state.asked.remove(&guid.0);
                    quest.clear_status(guid.0);
                    continue;
                }
                // Re-ask when this unit's own key moves — its service bits or its faction template.
                // The reference does the same per-unit query off those field watches.
                let key = unit_ask_key(&obj.0);
                if state.asked.insert(guid.0, key) != Some(key) {
                    let _ = commands
                        .0
                        .send(ClientCommand::QuestgiverStatusQuery { npc: guid.0 });
                }
            }
            // `0x5eb0da shr ecx,5; test cl,1` — typemask bit 5, `TYPEMASK_GAMEOBJECT`. Sweep-only,
            // no asked-set, no teardown: see this function's doc and §W14.8.
            EntityKind::GameObject if swept => {
                if obj.0.gameobject_flags() & GO_FLAG_INTERACT_COND == 0 {
                    continue;
                }
                // `0x5eb101 cmp eax,1; jle` — the GameObject's reaction toward us must be at least
                // **2**. Not a friendliness test: Unfriendly(2) and Neutral(3) both pass, and a
                // faction-less GameObject resolves Neutral. `None` = unresolvable (no catalog yet)
                // and passes, the same "no opinion" rule the cursor's own gate uses.
                let reaction = go_reaction(
                    factions.as_deref(),
                    obj.0.gameobject_faction(),
                    Some(&store),
                );
                if !reaction.is_none_or(|r| r > 1) {
                    continue;
                }
                let _ = commands
                    .0
                    .send(ClientCommand::QuestgiverStatusQuery { npc: guid.0 });
            }
            _ => {}
        }
    }
}

/// The per-unit ask key: re-ask that NPC whenever this moves. `UNIT_NPC_FLAGS` carries both service
/// bits the reference watches — questgiver `0x2` (`0x60b490`) and flightmaster `0x8` (`0x60b4c5`) —
/// and `UNIT_FIELD_FACTIONTEMPLATE` is the reaction input we can see (`0x606f0a`); a charm or a
/// faction swap changes what the server will answer for a hostile giver.
fn unit_ask_key(fields: &benilla_protocol::ObjectFields) -> u64 {
    u64::from(fields.unit_npc_flags())
        | (u64::from(fields.unit_faction_template().unwrap_or(0)) << 32)
}

/// Fold every *self* descriptor field the reference watches into one value — a change to any of
/// them sweeps every visible questgiver, because each is an input the server's `GetDialogStatus`
/// reads. The reference registers one handler per field (`0x468070`); we hash the lot, which is
/// the same thing observed from the outside. Recomputed only when our own store changes.
///
/// Level (`SatisfyQuestLevel`, the grey `!`), the quest log, money, every skill
/// (`SatisfyQuestSkill`), `PLAYER_FLAGS`, health, and our equipment/bag guids — the last standing
/// in for the reference's `ITEM` watch, as far as our object model can see it.
fn self_generation(fields: &benilla_protocol::ObjectFields) -> u64 {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut h = 0xcbf2_9ce4_8422_2325u64; // FNV offset
    let mut fold = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(FNV_PRIME);
    };
    fold(u64::from(fields.unit_level().unwrap_or(0)));
    // Health enters as the ALIVE/DEAD bit, not the raw value — and that is the PINNED reading now,
    // not a conservative guess (wow-re `questgiver-marker.md` §W13). The `UNIT_FIELD_HEALTH` watch
    // gates on the zero CROSSING, in `0x6046f0`, one level above where the handler chain suggested:
    // `0x604774 jg` / `0x604778 jle` require `old > 0 && new <= 0` = **death** before the sweep is
    // reached, and `0x6047f0`/`0x6047f4` take `old <= 0 && new > 0` = **resurrect** down the sibling
    // branch. Ordinary damage, heal and regen ticks fall through both and reach nothing — which is
    // why the captures show no query bursts tracking combat (one carries 28 consecutive non-lethal
    // packets, 70 → 2 HP, with zero bursts; the single packet crossing to 0 has the only burst in
    // the stretch).
    //
    // Our bit flips on BOTH crossings, and that is now known to be exactly right — the note this
    // comment used to carry ("one named over-refresh... the reference sweeps GameObject questgivers
    // there") was wrong on both halves and is CORRECTED by wow-re §W14.5/§W14.11. `0x5eb3c0` is not
    // a GameObject sweep: `0x468380` applies no type filter at all, so it walks **every** object in
    // the manager — it is the FULL re-query sweep, and its creature arm is the heavier one
    // (`0x607380`: an unconditional marker teardown plus two queries, questgiver `0x182` and taxi
    // `0x1aa`), the GameObject arm the lighter. So the death edge sweeps (`0x5eb070`) and the
    // **revive** edge sweeps too (`0x5eb3c0`), and one sweep per crossing is the reference's own
    // behaviour rather than a deviation we were paying for. 0654, 1872.
    fold(u64::from(fields.unit_health().unwrap_or(1) == 0));
    fold(u64::from(fields.player_flags()));
    fold(u64::from(fields.player_money().unwrap_or(0)));
    for slot in 0..benilla_protocol::messages::PLAYER_QUEST_LOG_SLOTS {
        if let Some(s) = fields.player_quest_log(slot) {
            if s.quest_id != 0 {
                fold(u64::from(s.quest_id) ^ (u64::from(s.state) << 32));
            }
        }
    }
    // The reference watches the skill *value* half specifically (`edx=0x84c`), so the id + current
    // value are what matter — a rank-up is the event, not a temporary bonus ticking.
    for slot in 0..benilla_protocol::messages::PLAYER_SKILL_SLOTS {
        if let Some(s) = fields.player_skill(slot) {
            if s.skill_id != 0 {
                fold(u64::from(s.skill_id) ^ (u64::from(s.value) << 32));
            }
        }
    }
    for i in 0..23 {
        if let Some(guid) = fields.player_inv_slot(i) {
            fold(guid);
        }
    }
    h
}

/// [`query_statuses`]'s memory across frames.
#[derive(Default)]
pub(super) struct QueryState {
    /// The last folded [`self_generation`] — kept separately from `generation` so the expensive
    /// descriptor walk can be skipped on frames where our own store didn't change.
    fields: u64,
    /// `fields` combined with the packet epoch; a change here is the sweep.
    generation: u64,
    /// Which guids we've asked about, and the [`unit_ask_key`] we asked at.
    asked: HashMap<u64, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The re-ask law (decision 0654). The server only *answers* queries, so a status we never
    /// re-ask for is a marker frozen at whatever it was when the NPC came into view. Three
    /// invalidations, all exercised here and all faithful: our own level (the ding that turns a
    /// grey `!` gold — the reference's `UNIT_FIELD_LEVEL` field watch), the object leaving and
    /// re-entering the world (its per-object query at create), and the questgiver bit going off
    /// (its teardown branch).
    #[test]
    fn the_status_is_re_asked_on_a_ding_on_re_entry_and_dropped_with_the_flag() {
        use crate::net::{Guid, NetEntity};
        use benilla_protocol::{EntityKind, ObjectFields};

        const NPC: u64 = 0xdead_beef;
        const FIELD_LEVEL: u16 = 34; // UNIT_FIELD_LEVEL
        const FIELD_NPC_FLAGS: u16 = 147; // UNIT_NPC_FLAGS

        let net_entity = || NetEntity {
            kind: EntityKind::Unit,
            display_id: None,
            scale: 1.0,
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.insert_resource(NetCommands(tx))
            .init_resource::<GuidIndex>()
            .init_resource::<QuestGiver>()
            .add_systems(Update, query_statuses);

        let me = app
            .world_mut()
            .spawn((
                SelfPlayer,
                net_entity(),
                Guid(1),
                ObjectStore(ObjectFields::from_pairs(&[(FIELD_LEVEL, 5)])),
            ))
            .id();
        let spawn_npc = |app: &mut App| {
            let e = app
                .world_mut()
                .spawn((
                    net_entity(),
                    Guid(NPC),
                    ObjectStore(ObjectFields::from_pairs(&[(FIELD_NPC_FLAGS, 0x2)])),
                ))
                .id();
            app.world_mut().resource_mut::<GuidIndex>().0.insert(NPC, e);
            e
        };
        let asked = |app: &mut App| -> usize {
            app.update();
            rx.try_iter()
                .filter(
                    |c| matches!(c, ClientCommand::QuestgiverStatusQuery { npc } if *npc == NPC),
                )
                .count()
        };

        let npc = spawn_npc(&mut app);
        assert_eq!(asked(&mut app), 1, "asked once when it comes into view");
        assert_eq!(asked(&mut app), 0, "not again while nothing has changed");

        // The ding: `SatisfyQuestLevel` is what made the answer UNAVAILABLE, so the whole set is
        // re-asked — the marker can't learn it went gold any other way.
        *app.world_mut()
            .entity_mut(me)
            .get_mut::<ObjectStore>()
            .unwrap() = ObjectStore(ObjectFields::from_pairs(&[(FIELD_LEVEL, 6)]));
        assert_eq!(asked(&mut app), 1, "our level changed — re-ask everyone");
        assert_eq!(asked(&mut app), 0, "…once");

        // Out of view: the cached status dies with the object (the reference caches it *on* the
        // unit), so the marker can't flash stale on the way back in.
        app.world_mut()
            .resource_mut::<QuestGiver>()
            .set_status(NPC, 5);
        app.world_mut().resource_mut::<GuidIndex>().0.remove(&NPC);
        app.world_mut().entity_mut(npc).despawn();
        assert_eq!(asked(&mut app), 0, "gone: nothing to ask");
        assert!(
            app.world().resource::<QuestGiver>().status(NPC).is_none(),
            "the cached status goes with the object"
        );

        let npc = spawn_npc(&mut app);
        assert_eq!(asked(&mut app), 1, "back in view — asked afresh");

        // The flag goes off: drop the answer, which drops the marker with it.
        app.world_mut()
            .resource_mut::<QuestGiver>()
            .set_status(NPC, 5);
        *app.world_mut()
            .entity_mut(npc)
            .get_mut::<ObjectStore>()
            .unwrap() = ObjectStore(ObjectFields::from_pairs(&[(FIELD_NPC_FLAGS, 0x1)]));
        assert_eq!(
            asked(&mut app),
            0,
            "no longer a questgiver — nothing to ask"
        );
        assert!(
            app.world().resource::<QuestGiver>().status(NPC).is_none(),
            "and its stale answer is dropped"
        );
    }

    /// **B257 — an escort giver's `!` outlived the accept.** The teardown branch used to be gated on
    /// the guid still being in `asked` ("did we ever query it?"), which is a memo about *questions*
    /// standing in for a fact about *answers* — and the sweep clears that memo. On accepting an
    /// escort both halves land in the same update: vmangos writes the quest-log slot and, from the
    /// script hook in the same handler, zeroes `UNIT_NPC_FLAGS`
    /// (`FollowerAI::StartFollow` / `ScriptedEscortAI`'s "disable npcflags"). So the quest-log write
    /// bumps the generation, `asked.clear()` runs first, `remove` finds nothing, and the cached
    /// AVAILABLE status stays — forever, because the flag stays off and this arm never queries.
    /// The reference tears down on the flag test alone (`0x5eb0a0`, decision 0647).
    ///
    /// The control below is the half that must NOT change: an ordinary giver, whose flag stays on,
    /// is re-asked by the same sweep rather than torn down.
    #[test]
    fn an_escort_giver_loses_its_marker_when_the_flag_drops_with_the_quest_log_write() {
        use crate::net::{Guid, NetEntity};
        use benilla_protocol::{EntityKind, ObjectFields};

        const ESCORT: u64 = 0x5115; // Mist: gives the quest, then follows with npcflags off
        const PLAIN: u64 = 0x5116; // an ordinary giver standing next to her
        const FIELD_NPC_FLAGS: u16 = 147;
        const FIELD_QUEST_LOG_1_1: u16 = 198;

        let net_entity = || NetEntity {
            kind: EntityKind::Unit,
            display_id: None,
            scale: 1.0,
        };
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.insert_resource(NetCommands(tx))
            .init_resource::<GuidIndex>()
            .init_resource::<QuestGiver>()
            .add_systems(Update, query_statuses);

        let me = app
            .world_mut()
            .spawn((SelfPlayer, net_entity(), Guid(1), ObjectStore::default()))
            .id();
        let spawn = |app: &mut App, guid: u64| {
            let e = app
                .world_mut()
                .spawn((
                    net_entity(),
                    Guid(guid),
                    ObjectStore(ObjectFields::from_pairs(&[(FIELD_NPC_FLAGS, 0x2)])),
                ))
                .id();
            app.world_mut()
                .resource_mut::<GuidIndex>()
                .0
                .insert(guid, e);
            e
        };
        let escort = spawn(&mut app, ESCORT);
        spawn(&mut app, PLAIN);
        app.update(); // both asked, both marked in `asked`

        // The server's answer: a gold `!` over each.
        for npc in [ESCORT, PLAIN] {
            app.world_mut()
                .resource_mut::<QuestGiver>()
                .set_status(npc, 5);
        }

        // The accept, as one update: the quest-log slot appears in OUR store (the sweep) and the
        // escort's questgiver bit goes off (the teardown) — in the same drained batch.
        *app.world_mut()
            .entity_mut(me)
            .get_mut::<ObjectStore>()
            .unwrap() = ObjectStore(ObjectFields::from_pairs(&[(FIELD_QUEST_LOG_1_1, 938)]));
        *app.world_mut()
            .entity_mut(escort)
            .get_mut::<ObjectStore>()
            .unwrap() = ObjectStore(ObjectFields::from_pairs(&[(FIELD_NPC_FLAGS, 0x0)]));
        app.update();

        assert!(
            app.world()
                .resource::<QuestGiver>()
                .status(ESCORT)
                .is_none(),
            "the escortee stopped being a questgiver: its stale AVAILABLE status — and the `!` \
             the marker layer draws from it — must go, sweep or no sweep"
        );
        assert_eq!(
            app.world().resource::<QuestGiver>().status(PLAIN),
            Some(5),
            "the control: a giver whose flag is still on keeps its answer until the server \
             replies to the sweep's fresh query"
        );

        // And it self-heals: when the escort ends and the bit returns, the NPC is asked again.
        *app.world_mut()
            .entity_mut(escort)
            .get_mut::<ObjectStore>()
            .unwrap() = ObjectStore(ObjectFields::from_pairs(&[(FIELD_NPC_FLAGS, 0x2)]));
        app.update();
        assert!(
            app.world()
                .resource::<QuestGiver>()
                .status(ESCORT)
                .is_none(),
            "still nothing cached — the answer arrives from the server, not from us"
        );
    }

    /// **A GameObject is asked about on a SWEEP, and only on a sweep** (decision 1872, wow-re
    /// `questgiver-marker.md` §W14.6/§W14.8). Three things are being pinned here, and the third is
    /// the one a client "improves" without noticing:
    ///
    /// - the gate is `GAMEOBJECT_FLAGS` bit 2 (mask `0x4`) — `0x5eb0ef shr eax,0x2; test al,1` —
    ///   and a quest-less GameObject beside it is never asked about at all;
    /// - a sweep asks exactly once, and the frames between sweeps are silent;
    /// - **there is no bring-up query.** A GameObject entering view asks nothing; it waits for the
    ///   next sweep. The unit control in the same world does the opposite on the same frame, which
    ///   is what makes this a contrast and not an accident of ordering.
    #[test]
    fn a_gameobject_is_asked_on_a_sweep_and_never_at_first_sight() {
        use crate::net::{Guid, NetEntity};
        use benilla_protocol::ObjectFields;

        /// The Goldshire `Wanted Poster`'s real guid (`HIGHGUID_GAMEOBJECT`, template 68, spawn
        /// 26843) and its real `GAMEOBJECT_FLAGS` — INTERACT_COND | NODESPAWN, read off the live
        /// wire by `WOW_PROBE_GOQUEST`.
        const POSTER: u64 = 0xf110_0000_0044_68db;
        const POSTER_FLAGS: u32 = 0x24;
        /// A GameObject next to it with no quest condition: a plain door, `NODESPAWN` only.
        const DOOR: u64 = 0xf110_0000_0045_0001;
        const DOOR_FLAGS: u32 = 0x20;
        /// The control: an ordinary creature questgiver, which the same sweep treats differently.
        const NPC: u64 = 0x2222;
        const FIELD_LEVEL: u16 = 34;
        const FIELD_NPC_FLAGS: u16 = 147;
        const FIELD_GAMEOBJECT_FLAGS: u16 = 9;

        let (tx, rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.insert_resource(NetCommands(tx))
            .init_resource::<GuidIndex>()
            .init_resource::<QuestGiver>()
            .add_systems(Update, query_statuses);

        let spawn = |app: &mut App, guid: u64, kind: EntityKind, fields: &[(u16, u32)]| {
            let e = app
                .world_mut()
                .spawn((
                    NetEntity {
                        kind,
                        display_id: None,
                        scale: 1.0,
                    },
                    Guid(guid),
                    ObjectStore(ObjectFields::from_pairs(fields)),
                ))
                .id();
            app.world_mut()
                .resource_mut::<GuidIndex>()
                .0
                .insert(guid, e);
        };
        app.world_mut().spawn((
            SelfPlayer,
            Guid(1),
            ObjectStore(ObjectFields::from_pairs(&[(FIELD_LEVEL, 5)])),
        ));
        // Drain per update, then count per guid — several objects are in flight at once here.
        let asked = |app: &mut App| -> Vec<u64> {
            app.update();
            rx.try_iter()
                .filter_map(|c| match c {
                    ClientCommand::QuestgiverStatusQuery { npc } => Some(npc),
                    _ => None,
                })
                .collect()
        };

        // Settle the generation first: the very first frame in a fresh world IS a sweep (nothing
        // has been folded yet), so "first sight" can only be asked of a later frame.
        assert!(asked(&mut app).is_empty(), "nothing in view yet");

        spawn(
            &mut app,
            POSTER,
            EntityKind::GameObject,
            &[(FIELD_GAMEOBJECT_FLAGS, POSTER_FLAGS)],
        );
        spawn(
            &mut app,
            DOOR,
            EntityKind::GameObject,
            &[(FIELD_GAMEOBJECT_FLAGS, DOOR_FLAGS)],
        );
        spawn(
            &mut app,
            NPC,
            EntityKind::Unit,
            &[(FIELD_NPC_FLAGS, NPC_FLAG_QUESTGIVER)],
        );

        assert_eq!(
            asked(&mut app),
            vec![NPC],
            "first sight: the creature is asked from its own create path, the poster is NOT — \
             the GameObject class has no bring-up query (§W14.8)"
        );
        assert!(
            asked(&mut app).is_empty(),
            "and the frames after it stay silent"
        );

        // A sweep: any of the reference's 13 local-player triggers. The packet epoch stands in for
        // its four packet handlers.
        app.world_mut().resource_mut::<QuestGiver>().bump_reask();
        let swept = asked(&mut app);
        assert!(
            swept.contains(&POSTER),
            "the sweep asks about the poster — {swept:x?}"
        );
        assert!(
            swept.contains(&NPC),
            "and re-asks the creature, as it always did — {swept:x?}"
        );
        assert!(
            !swept.contains(&DOOR),
            "but never the GameObject without GAMEOBJECT_FLAGS bit 2 — {swept:x?}"
        );
        assert_eq!(
            swept.iter().filter(|g| **g == POSTER).count(),
            1,
            "exactly once per sweep"
        );

        assert!(
            asked(&mut app).is_empty(),
            "between sweeps, nothing — a GameObject has no per-object key to re-ask on"
        );

        // ...and it is not one-shot: the next sweep asks again, because the reference keeps no
        // per-GameObject memo of having asked (there is nowhere on a CGGameObject_C to keep one).
        app.world_mut().resource_mut::<QuestGiver>().bump_reask();
        assert!(
            asked(&mut app).contains(&POSTER),
            "and again on the next sweep"
        );
    }

    /// The rest of the reference's trigger set (0654): the remaining self-descriptor watches, the
    /// packet epoch that stands in for its four packet handlers, and the per-unit key that catches
    /// a unit's own service bits or faction moving. Each leg must re-ask exactly once.
    #[test]
    fn every_recorded_trigger_re_asks_exactly_once() {
        use crate::net::{Guid, NetEntity};
        use benilla_protocol::{EntityKind, ObjectFields};

        const NPC: u64 = 0x1234;
        const FIELD_HEALTH: u16 = 22; // UNIT_FIELD_HEALTH
        const FIELD_NPC_FLAGS: u16 = 147;
        const FIELD_FACTION: u16 = 35; // UNIT_FIELD_FACTIONTEMPLATE
        const FIELD_PLAYER_FLAGS: u16 = 190;
        const FIELD_COINAGE: u16 = 1176;
        const FIELD_SKILL_1_1: u16 = 718;
        const FIELD_INV_SLOT_0: u16 = 486; // PLAYER_FIELD_INV_SLOT_HEAD

        let net_entity = || NetEntity {
            kind: EntityKind::Unit,
            display_id: None,
            scale: 1.0,
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.insert_resource(NetCommands(tx))
            .init_resource::<GuidIndex>()
            .init_resource::<QuestGiver>()
            .add_systems(Update, query_statuses);

        // Our own store starts with every watched field present, so each leg below is a CHANGE to
        // one field rather than a field appearing for the first time.
        let base_self = vec![
            (34u16, 5u32),
            (FIELD_HEALTH, 100),
            (FIELD_PLAYER_FLAGS, 0),
            (FIELD_COINAGE, 500),
            (FIELD_SKILL_1_1, 186 | (1 << 16)), // skill id 186, step 1
            (FIELD_SKILL_1_1 + 1, 75 | (300 << 16)), // value 75 / max 300
            (FIELD_INV_SLOT_0, 0xaaaa),
            (FIELD_INV_SLOT_0 + 1, 0),
        ];
        let me = app
            .world_mut()
            .spawn((
                SelfPlayer,
                net_entity(),
                Guid(1),
                ObjectStore(ObjectFields::from_pairs(&base_self)),
            ))
            .id();
        let npc_e = app
            .world_mut()
            .spawn((
                net_entity(),
                Guid(NPC),
                ObjectStore(ObjectFields::from_pairs(&[
                    (FIELD_NPC_FLAGS, 0x2),
                    (FIELD_FACTION, 35),
                ])),
            ))
            .id();
        app.world_mut()
            .resource_mut::<GuidIndex>()
            .0
            .insert(NPC, npc_e);

        let asked = |app: &mut App| -> usize {
            app.update();
            rx.try_iter()
                .filter(
                    |c| matches!(c, ClientCommand::QuestgiverStatusQuery { npc } if *npc == NPC),
                )
                .count()
        };
        // Replace one field of our own descriptor, keeping the rest — the shape a values-update
        // delta actually has.
        let set_self = |app: &mut App, field: u16, value: u32| {
            let mut pairs = base_self.clone();
            match pairs.iter_mut().find(|(f, _)| *f == field) {
                Some(p) => p.1 = value,
                None => pairs.push((field, value)),
            }
            *app.world_mut()
                .entity_mut(me)
                .get_mut::<ObjectStore>()
                .unwrap() = ObjectStore(ObjectFields::from_pairs(&pairs));
        };

        assert_eq!(asked(&mut app), 1, "the first sight");
        assert_eq!(asked(&mut app), 0, "then quiet");

        for (label, field, value) in [
            ("death", FIELD_HEALTH, 0u32), // the alive/dead bit, not the raw value — see 0654
            ("PLAYER_FLAGS", FIELD_PLAYER_FLAGS, 0x10),
            ("money", FIELD_COINAGE, 900),
            ("a skill rank", FIELD_SKILL_1_1 + 1, 80 | (300 << 16)),
            ("an equipped item", FIELD_INV_SLOT_0, 0xbbbb),
        ] {
            set_self(&mut app, field, value);
            assert_eq!(asked(&mut app), 1, "{label} changed — re-ask");
            assert_eq!(asked(&mut app), 0, "{label}: exactly once");
        }

        // The packet half: reputation, the group roster and the quest packets all land here.
        app.world_mut().resource_mut::<QuestGiver>().bump_reask();
        assert_eq!(asked(&mut app), 1, "a swept packet — re-ask");
        assert_eq!(asked(&mut app), 0, "exactly once");

        // Per unit: its flightmaster bit, then its faction template.
        let set_npc = |app: &mut App, pairs: &[(u16, u32)]| {
            *app.world_mut()
                .entity_mut(npc_e)
                .get_mut::<ObjectStore>()
                .unwrap() = ObjectStore(ObjectFields::from_pairs(pairs));
        };
        set_npc(
            &mut app,
            &[(FIELD_NPC_FLAGS, 0x2 | 0x8), (FIELD_FACTION, 35)],
        );
        assert_eq!(asked(&mut app), 1, "its flightmaster bit — re-ask that one");
        assert_eq!(asked(&mut app), 0, "exactly once");

        set_npc(
            &mut app,
            &[(FIELD_NPC_FLAGS, 0x2 | 0x8), (FIELD_FACTION, 12)],
        );
        assert_eq!(asked(&mut app), 1, "its faction template — re-ask that one");
        assert_eq!(asked(&mut app), 0, "exactly once");
    }
}
