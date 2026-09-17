//! The **inside-a-battleground** live probe (`WOW_PROBE_BG=wsg|ab|av`) — the instrument that turns
//! "does a battleground work?" from a question nobody can answer into a census anyone can read.
//! Inert without the env.
//!
//! ## Why this exists
//!
//! Decisions 1963 / 1972 / 1974 / 1980 built the whole battleground **interface** — the queue, the
//! list window, the scoreboard, the always-up readout, the map blips, forty-odd Lua verbs — and
//! every one of them off the reference's own FrameXML. What none of them could build is the thing
//! on the far side of the port button: **nothing in this tree has ever entered map 489, 529 or
//! 30.** `ProbeBgQueuePlugin` stops at the queue by design (it is 2232's login-burst fixture);
//! the terrain, the world states, the objects, the spirit guides and the match's own end have
//! never been observed at all. So the gaps are not *known* to be gaps — they are unmeasured, and
//! an unmeasured subsystem is where a "should work by construction" quietly isn't.
//!
//! This probe is the measurement. It walks the real player road — level, greet a battlemaster on
//! the wire, queue, take the port through the **stock Lua verb** the reference's own dialog calls
//! — and then, from inside, prints a structured census every [`CENSUS_EVERY`] until
//! [`census_samples`] are in. Every later battleground round is judged against these lines.
//!
//! ## The one server-side lever, and why it is needed — and why it is turned back off
//!
//! A battleground does not start until `min_players_per_team` bodies are queued on each side —
//! four for Warsong Gulch on this server's patch, twenty for Alterac Valley
//! (`mangos.battleground_template`). One probe can never reach that. vmangos has the lever built
//! in: **`.debug bg`** toggles `BattleGroundMgr::m_testing`, and `BattleGroundQueue::CheckNormalMatch`
//! then starts a battleground as soon as *either* side has one player
//! (`BattleGroundMgr.cpp:557/623/756`). It is `SEC_ADMINISTRATOR` (6), exactly what a probe account
//! holds, and it is **in-memory only** — `m_testing` is initialised `false` in the manager's
//! constructor, so a server restart clears it whatever happens here. The probe turns it on at
//! Setup and off again at Report; a crashed run leaves it on until the next restart, which is
//! surprising but harmless (a solo queue would pop).
//!
//! Note that `.debug bg` is a **toggle, not a set**, and it announces itself to the whole realm
//! (`LANG_DEBUG_BG_ON`/`_OFF`). Both facts are reported on the probe's own lines so a run that
//! started from the wrong state reads as such rather than as a broken queue.
//!
//! **It is turned back off once the match is running** (see `DOORS_SAMPLE`), because leaving it on
//! makes a solo battleground unendable: vmangos decrements its premature-finish countdown inside an
//! `else if (!sBattleGroundMgr.isTesting())` branch (`BattleGround.cpp:337`), so while testing is on
//! the countdown is armed and then **frozen**. Decision 2290 said the opposite, from reading
//! `GetPrematureFinishTime()` without the branch that consumes it; 2296 corrects it, having watched
//! a run sit in Warsong Gulch for 492 s with `GetBattlefieldWinner()` nil the whole way.
//!
//! ## What the census reports, and why each column is there
//!
//! | column | the question it answers |
//! |---|---|
//! | `map` / `area` | did the worldport land, and does the terrain know where we are? |
//! | `pos` | is the body at the battleground's own start location, or at the origin? |
//! | `terrain` | [`WorldLoadProgress`] — did map 489's ADTs stream, or is the ground missing? |
//! | `ents` | units / players / gameobjects / dynamic / corpses actually mirrored into the ECS |
//! | `gos` | the gameobject **entries** in range — the flags, the doors, the banners |
//! | `states` | the raw world-state table and its scope: the score, the flag carriers, the timer |
//! | `alwaysup` | `GetNumWorldStateUI()` — what the reference's readout would actually draw |
//! | `status` | `GetBattlefieldStatus(1..3)`, straight out of the VM |
//! | `score` | `GetNumBattlefieldScores()` / `GetBattlefieldWinner()` |
//! | `guide` | the nearest `SPIRITGUIDE`-flagged unit — the graveyard's resurrect wave |
//! | `lua` | script errors collected since the last sample; a non-zero count is the finding |
//!
//! ## The run recipe
//!
//! ```sh
//! cd <slot> && WOW_USER=probe7 WOW_PASS=pprobe7 WOW_CHAR=Probeseven \
//!   WOW_UNATTENDED=1 WOW_NOSOUND=1 WOW_GM=off WOW_PROBE_BG=wsg \
//!   timeout 420 cargo run -p benilla 2>&1 | grep -E 'PROBE bg:'
//! ```
//!
//! `WOW_GM=off` is not optional here: GM mode re-templates the body's faction to 35, and a
//! battleground is the one place where every reaction, every objective and the server's own team
//! assignment reads off it (0649, 0679).
//!
//! Non-combat — the probe never attacks and never stands anywhere contested; with one player in
//! the instance there is nobody to fight. Pair with the SLOT-KEYED probe identity
//! (`method.md`, "The local vmangos server").

use bevy::ecs::system::NonSendMut;
use bevy::prelude::*;

use benilla_ui::script::UiScript;
use benilla_world::terrain_stream::{CurrentArea, WorldLoadProgress};
use benilla_world::world_map::CurrentMap;

use super::probes::ProbeClock;
use crate::net::{
    ChatKind, ClientCommand, DroppedOpcodes, Guid, NetCommands, NetEntity, ObjectStore, SelfPlayer,
};
use crate::player::Player;
use crate::target::cursor_mode::npc_flags;
use crate::ui_battlefield::Battlefield;
use crate::ui_dialog_verbs::BattlefieldQueue;
use crate::world_state::WorldStates;

/// One battleground's fixtures: what to queue for, and which battlemaster to greet.
///
/// All three battlemasters stand within ~35 yd of each other in Stormwind's PvP alcove, so one
/// `.go` reaches any of them — which is why the probe finds its NPC by **creature entry** off the
/// guid rather than by "the nearest battlemaster", a predicate that would pick whichever of the
/// three streamed in first.
struct Arena {
    /// `WOW_PROBE_BG`'s value.
    key: &'static str,
    /// The Map.dbc row — what `CMSG_BATTLEMASTER_JOIN` carries and what the worldport must land on.
    map: u32,
    /// The battlemaster's `creature_template.entry` (vmangos `battlemaster_entry`).
    npc_entry: u32,
    /// That NPC's display name, for the probe's own lines.
    npc_name: &'static str,
    /// Its spawn — the `.go xyz` target (vmangos `creature`, map 0, Stormwind).
    at: [f32; 3],
    /// The objective this battleground's **flag leg** goes for: the gameobject entry to click and
    /// where it stands on the battleground map (vmangos `gameobject`). `None` for a battleground
    /// whose objective is not a single clickable flag.
    flag: Option<(u32, [f32; 3])>,
}

/// The three 1.12 battlegrounds. Levels are not per-arena: [`QUEUE_LEVEL`] clears every bracket
/// floor on every content patch, so the probe never has to know which patch the server runs.
const ARENAS: [Arena; 3] = [
    Arena {
        key: "wsg",
        map: 489,
        npc_entry: 14981,
        npc_name: "Elfarran",
        at: [-8454.62, 318.85, 120.97],
        // The **Warsong Flag** at the Horde base — the one an Alliance body carries. Our own
        // Silverwing Flag (179830) is the capture point, not the pickup.
        flag: Some((179831, [916.02, 1434.40, 345.41])),
    },
    Arena {
        key: "ab",
        map: 529,
        npc_entry: 15008,
        npc_name: "Lady Hoteshem",
        at: [-8420.48, 328.71, 120.89],
        // Arathi Basin's objective is five capturable banners, not a carried flag.
        flag: None,
    },
    Arena {
        key: "av",
        map: 30,
        npc_entry: 7410,
        npc_name: "Thelman Slatefist",
        at: [-8424.55, 342.81, 120.89],
        // Alterac Valley's are towers, graveyards and captains.
        flag: None,
    },
];

/// The level the probe body is raised to.
///
/// 60 rather than the 25 `probe_bg_queue` uses, because Alterac Valley's floor is **51** on every
/// patch that ships it and the bracket is checked at the HELLO — a body under it is refused there,
/// silently, so the list never arrives and the failure reads like a broken client.
const QUEUE_LEVEL: u32 = 60;

/// How many times to drop and re-take the queue before giving up (see the `Joined` arm).
const MAX_REJOINS: u32 = 3;

/// The `.go`'s settle radius before the battlemaster scan. Not a gate — the server applies its own
/// interaction check; this only keeps the probe from greeting a battlemaster it has not reached.
const NEAR_YD: f32 = 20.0;

/// How often a census line is printed once inside.
const CENSUS_EVERY: f64 = 12.0;

/// How many census samples to take before reporting and leaving.
///
/// **Sized by the preparation phase, not by taste.** A battleground does not begin when you land
/// in it: vmangos holds every arrival behind closed doors for `BG_START_DELAY_2M` = **120 s**
/// (`BattleGround.cpp:248`, the four `BG_STARTING_EVENT_*` steps at 2 min / 1 min / 30 s / go),
/// and `.debug bg` does not shorten it. A window that ended before then would census nothing but
/// the pen and report it as the whole battleground. Sixteen × 12 s = **192 s** covers the full
/// prep, the doors dropping, and ~70 s of live match — while staying inside vmangos's
/// `BattleGround.PrematureFinishTimer` (5 min), the clock that ends an under-populated one.
/// Twelve × 12 s = **144 s**: the full prep, the doors dropping at ~116 s, and one sample past it,
/// which leaves room for the graveyard leg inside the same run.
const CENSUS_SAMPLES_DEFAULT: u32 = 12;

/// The census sample count, overridable with `WOW_PROBE_BG_SAMPLES`.
///
/// The default covers the preparation phase and the doors. **The reason it is a dial** is the end
/// of a match: with one body inside, vmangos starts its `BattleGround.PrematureFinishTimer` (5 min,
/// because `CreateNewBattleGround` reads `min_players_per_team` from the template and not from the
/// testing override, so a solo match is permanently under-populated) and then ends the battleground
/// with no winner. `WOW_PROBE_BG_SAMPLES=30` is a ~6-minute window, which reaches it — the
/// `MSG_PVP_LOG_DATA` "ended" byte, `GetBattlefieldWinner()` and the automatic scoreboard, all of
/// which are built and none of which anything here has watched.
fn census_samples() -> u32 {
    std::env::var("WOW_PROBE_BG_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CENSUS_SAMPLES_DEFAULT)
}

/// How often the ghost leg samples, and for how long. The battleground resurrect wave is a 30 s
/// server cycle (`BattleGround::Update`'s `m_lastResurrectTime`), so eleven 4 s samples cover a
/// full wave and a little of the next — enough to see the clock arm, tick down, and fire.
const GHOST_SAMPLE_EVERY: f64 = 4.0;
/// See [`GHOST_SAMPLE_EVERY`].
const GHOST_SAMPLES: u32 = 11;

/// How many 3 s samples the flag leg takes after the use.
const FLAG_SAMPLES: u32 = 5;

/// The census sample by which the doors have opened (they drop ~116 s after entry, and the census
/// ticks every 12 s). The testing flag is turned back off here — see the `Inside` arm.
const DOORS_SAMPLE: u32 = 11;

/// `SPIRITGUIDE` — `UNIT_NPC_FLAGS` bit 6, the flag the reference's area-spirit-healer acquire
/// scan keys on (wow-re `interact-dead-fork-and-npc-service-ladder.md` §C row 6).
const NPC_FLAG_SPIRITGUIDE: u32 = 1 << 6;

/// The **event tap** — a Lua frame the probe installs on entry that records every battleground
/// event the reference's own interface listens for, with its `arg1`.
///
/// This exists because the first run's finding could not be read off the log at all. vmangos sends
/// three `CHAT_MSG_BG_SYSTEM_NEUTRAL` lines during the countdown (`BattleGround.cpp:383/392/400`,
/// the one-minute / half-minute / has-begun `m_startMessageIds`) and a `SMSG_PLAY_SOUND`
/// (`PlaySoundToAll(SOUND_BG_START)`), and **nothing appeared** — but an absent log line is not an
/// absent event, because a chat kind that reaches the VM logs nothing on the way. Silence in a log
/// is not evidence; a tap on the event itself is.
///
/// It is written in the reference's own Lua dialect (`this`/`event`/`arg1` globals in `OnEvent`,
/// no `ipairs` over a literal) so it exercises the same road a 1.12 addon would.
const EVENT_TAP: &str = r#"
BenillaBgLog = {};
BenillaBgTap = CreateFrame("Frame");
BenillaBgTap:SetScript("OnEvent", function()
    table.insert(BenillaBgLog, event .. "(" .. tostring(arg1) .. ")");
end);
BenillaBgTap:RegisterEvent("CHAT_MSG_BG_SYSTEM_NEUTRAL");
BenillaBgTap:RegisterEvent("CHAT_MSG_BG_SYSTEM_ALLIANCE");
BenillaBgTap:RegisterEvent("CHAT_MSG_BG_SYSTEM_HORDE");
BenillaBgTap:RegisterEvent("CHAT_MSG_SYSTEM");
BenillaBgTap:RegisterEvent("CHAT_MSG_MONSTER_YELL");
BenillaBgTap:RegisterEvent("UPDATE_WORLD_STATES");
BenillaBgTap:RegisterEvent("UPDATE_BATTLEFIELD_STATUS");
BenillaBgTap:RegisterEvent("UPDATE_BATTLEFIELD_SCORE");
BenillaBgTap:RegisterEvent("AREA_SPIRIT_HEALER_IN_RANGE");
BenillaBgTap:RegisterEvent("AREA_SPIRIT_HEALER_OUT_OF_RANGE");
BenillaBgTap:RegisterEvent("PLAYER_DEAD");
BenillaBgTap:RegisterEvent("PLAYER_UNGHOST");
BenillaBgTap:RegisterEvent("PLAYER_ALIVE");
BenillaBgTap:RegisterEvent("ZONE_CHANGED_NEW_AREA");
"#;

/// Drain the tap: everything it recorded since the last census, then empty it.
const EVENT_DRAIN: &str = r#"
local out = "";
if BenillaBgLog then
    for i = 1, table.getn(BenillaBgLog) do out = out .. "  " .. BenillaBgLog[i]; end
    BenillaBgLog = {};
end
return out;
"#;

pub(crate) struct ProbeBgPlugin;

impl Plugin for ProbeBgPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BgProbe>().add_systems(Update, bg_probe);
    }
}

#[derive(Resource, Default)]
struct BgProbe {
    phase: Phase,
    /// The Lua event tap is in (or failed once and said so).
    tap_installed: bool,
    /// How many times the queue has been dropped and re-taken (see the `Joined` arm).
    rejoins: u32,
}

/// `Wait` → (levelled, hop sent) `Hopped` → (`.debug bg` sent) `Toggled` → (reply read, testing
/// confirmed on, battlemaster greeted) `Greeted` →
/// (join sent) `Joined` → (the port taken) `Ported` → (map 489 reached) `Inside` →
/// (`.die` sent) `Dying` → (released, ghost at the graveyard) `Ghost` → `Done`.
#[derive(Default, PartialEq)]
enum Phase {
    #[default]
    Wait,
    Hopped {
        sent_at: f64,
    },
    Toggled {
        sent_at: f64,
        corrected: bool,
    },
    Greeted {
        master: u64,
        sent_at: f64,
    },
    Joined {
        sent_at: f64,
    },
    Rejoining {
        at: f64,
    },
    Ported {
        sent_at: f64,
    },
    Inside {
        entered_at: f64,
        next_census: f64,
        samples: u32,
    },
    FlagHop {
        at: f64,
    },
    FlagUsed {
        at: f64,
        samples: u32,
    },
    Dying {
        sent_at: f64,
    },
    Ghost {
        released_at: f64,
        next_sample: f64,
        samples: u32,
    },
    Done,
}

/// Which battleground this run is for — `WOW_PROBE_BG`'s value, defaulting to Warsong Gulch (the
/// smallest, fastest and the only one whose objectives fit in one census window).
fn arena() -> &'static Arena {
    let want = std::env::var("WOW_PROBE_BG").unwrap_or_default();
    ARENAS
        .iter()
        .find(|a| a.key.eq_ignore_ascii_case(&want))
        .unwrap_or(&ARENAS[0])
}

/// A GM dot-command on the real chat wire; every reply is echoed by `net` as
/// `server says — …`, so a command that did nothing is visible as such.
fn gm(net: &NetCommands, text: impl Into<String>) {
    let _ = net.0.send(ClientCommand::Chat {
        kind: ChatKind::Say,
        target: None,
        text: text.into(),
    });
}

// One Bevy system's full input set — the guard-poi probe's shape, plus the census reads.
#[allow(clippy::too_many_arguments)]
fn bg_probe(
    time: ProbeClock,
    mut probe: ResMut<BgProbe>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
    player: Res<Player>,
    battlefield: Res<Battlefield>,
    queue: Res<BattlefieldQueue>,
    units: Query<(&Guid, &NetEntity, &ObjectStore, &Transform), Without<SelfPlayer>>,
    net: Res<NetCommands>,
    map: Option<Res<CurrentMap>>,
    area: Option<Res<CurrentArea>>,
    load: Option<Res<WorldLoadProgress>>,
    states: Res<WorldStates>,
    dropped: Res<DroppedOpcodes>,
    go_templates: Res<crate::go_templates::GameObjectTemplates>,
    mut idle: ResMut<crate::ui_chat::idle::LastInput>,
    mut script: Option<NonSendMut<UiScript>>,
) {
    let Ok(store) = me.single() else {
        return;
    };
    let arena = arena();
    let now = time.elapsed_secs_f64();

    // **This probe stands in for a player who is AT the keyboard.** benilla implements the
    // reference's idle handler faithfully (`ui_chat::idle`: auto-sit then auto-AFK at 300 000 ms
    // of no input), and vmangos removes an AFK player from a battleground outright
    // (`Player::ToggleAFK` → `LeaveBattleground`). Two correct behaviours, one on each side, and
    // together they eject an unattended probe five minutes in — which is exactly what the first
    // long run hit: at t=300 s the census read Stormwind, not Warsong Gulch, and the chat said
    // "You are now AFK". A battleground match cannot be watched to its end without this.
    idle.stamp_present(std::time::Duration::from_secs_f64(now));

    // **The tap goes in as soon as there is a VM to put it in, and not before.** The probe's own
    // first frame is a world-entry frame: the in-game interface has not materialized yet, so an
    // install at Setup gets `no VM` and every later `EVENTS` line reads `(none)` — which looks
    // exactly like "the battleground fired no events" and is the most misleading answer this
    // instrument could give. Retried every frame until it lands; one bool test once it has.
    if !probe.tap_installed {
        if let Some(script) = script.as_deref_mut() {
            match script.eval::<()>(EVENT_TAP) {
                Ok(()) => {
                    probe.tap_installed = true;
                    info!("PROBE bg: TAP installed");
                }
                Err(e) => {
                    probe.tap_installed = true; // a raise will not fix itself; say so once
                    error!("PROBE bg: TAP FAILURE — {e}");
                }
            }
        }
    }

    match probe.phase {
        Phase::Wait => {
            // Revive first, unconditionally: `CanInteractWithNPC` refuses a dead player outright,
            // and the refusal surfaces as an anticheat line about an "invalid creature" — which
            // reads like a wrong guid, not like a corpse (`probe_bg_queue`'s note).
            gm(&net, ".revive");
            let level = store.0.unit_level().unwrap_or(0);
            if level < QUEUE_LEVEL {
                gm(&net, format!(".levelup {}", QUEUE_LEVEL - level));
            }
            // **Deserter first.** Leaving a battleground any way but "the match ended" earns
            // spell **26013** (vmangos `Player.cpp:2178/18816`, `Battleground.CastDeserter`
            // defaults true), and `HandleBattlemasterJoinOpcode` then refuses the join outright
            // (`BG_GROUPJOIN_DESERTERS`) — so the *second* run of this probe never queues at all,
            // and reads as "the queue is broken" rather than "the last run left a debuff". Found
            // the hard way; cleared unconditionally, because a body without it does not care.
            gm(&net, ".unaura 26013");
            let [x, y, z] = arena.at;
            info!(
                "PROBE bg: SETUP arena={} map={} level>={QUEUE_LEVEL} master={}({})",
                arena.key, arena.map, arena.npc_name, arena.npc_entry,
            );
            gm(&net, format!(".go xyz {x} {y} {z} 0"));
            probe.phase = Phase::Hopped { sent_at: now };
        }
        Phase::Hopped { sent_at } => {
            if now - sent_at < 3.0 {
                return; // post-teleport settle: let the battlemasters stream in
            }
            // **The lever goes here, not in Setup, and its reply is read before anything queues.**
            // `.debug bg` is a TOGGLE (`BattleGroundMgr::ToggleTesting`), so a blind send is not
            // idempotent and a run that died before its own undo flips the next one the wrong way.
            // Correcting it *after* joining does not work either, and that is the part worth
            // writing down: vmangos evaluates a queue only when something SCHEDULES an update
            // (`BattleGroundMgr::Update` drains `m_queueUpdateScheduler`, which a join or a leave
            // fills — `BattleGroundMgr.cpp:1005-1028`). Flipping the flag under a queue entry that
            // is already sitting there schedules nothing, so the entry waits forever. Hence: get
            // the flag right first, then join once.
            gm(&net, ".debug bg");
            probe.phase = Phase::Toggled {
                sent_at: now,
                corrected: false,
            };
        }
        Phase::Toggled { sent_at, corrected } => {
            if now - sent_at < 1.5 {
                return; // let the world text come back and reach the tap
            }
            let seen = script
                .as_deref_mut()
                .and_then(|s| s.eval::<String>(EVENT_DRAIN).ok())
                .unwrap_or_default();
            if seen.contains("normal playercount") {
                if corrected {
                    error!(
                        "PROBE bg: FAILURE — `.debug bg` reads OFF after two sends; the account \
                         may be below SEC_ADMINISTRATOR (6) for it"
                    );
                    probe.phase = Phase::Done;
                    return;
                }
                info!("PROBE bg: TESTING was on; that send turned it OFF — sending once more");
                gm(&net, ".debug bg");
                probe.phase = Phase::Toggled {
                    sent_at: now,
                    corrected: true,
                };
                return;
            }
            if seen.contains("debugging") {
                info!("PROBE bg: TESTING on (1v0)");
            } else {
                warn!(
                    "PROBE bg: TESTING unread — no `.debug bg` reply in the tap ({seen:?}); \
                     continuing, but a queue that never pops is why"
                );
            }
            let here = player.pos;
            // By ENTRY, not by "the nearest battlemaster": all three stand in one alcove.
            let master = units.iter().find(|(guid, kind, store, tf)| {
                kind.kind == benilla_protocol::EntityKind::Unit
                    && store.0.unit_npc_flags() & npc_flags::BATTLEMASTER != 0
                    && benilla_protocol::guid::entry(guid.0) == Some(arena.npc_entry)
                    && tf.translation.distance(here) < NEAR_YD
            });
            if let Some((guid, ..)) = master {
                info!(
                    "PROBE bg: GREETING {} ({:#x}) on the wire",
                    arena.npc_name, guid.0
                );
                let _ = net.0.send(ClientCommand::BattlemasterHello { npc: guid.0 });
                probe.phase = Phase::Greeted {
                    master: guid.0,
                    sent_at: now,
                };
            } else if now - sent_at > 15.0 {
                error!(
                    "PROBE bg: FAILURE — {} (entry {}) never streamed within 15 yd in 15 s",
                    arena.npc_name, arena.npc_entry
                );
                probe.phase = Phase::Done;
            }
        }
        Phase::Greeted { master, sent_at } => {
            // Waiting for the LIST, not for a clock: its guid is the one the join must quote, and
            // its arrival is the proof the level gate was cleared.
            if battlefield.battlemaster() == Some(master) {
                info!("PROBE bg: LISTED — queueing for map {}", arena.map);
                let _ = net.0.send(ClientCommand::BattlemasterJoin {
                    battlemaster: master,
                    map_id: arena.map,
                    instance_id: 0,
                    as_group: false,
                });
                probe.phase = Phase::Joined { sent_at: now };
            } else if now - sent_at > 8.0 {
                error!(
                    "PROBE bg: FAILURE — no SMSG_BATTLEFIELD_LIST 8 s after the hello \
                     (a level under the bracket floor is refused here, silently)"
                );
                probe.phase = Phase::Done;
            }
        }
        Phase::Joined { sent_at } => {
            // Status 2 is "your battleground is ready, confirm within the deadline". The probe
            // takes it through the STOCK VERB, not through a hand-built packet: the whole point
            // is that the reference's own dialog road works.
            let ready = queue
                .slots()
                .iter()
                .position(|s| s.as_ref().is_some_and(|(st, _)| st.status == 2));
            if let Some(index) = ready {
                let slot = index + 1; // the verbs are 1-based
                info!("PROBE bg: CONFIRM — slot {slot} is ready; AcceptBattlefieldPort({slot}, 1)");
                if let Some(script) = script {
                    if let Err(e) = script.eval::<()>(&format!("AcceptBattlefieldPort({slot}, 1)"))
                    {
                        error!("PROBE bg: FAILURE — AcceptBattlefieldPort raised: {e}");
                        probe.phase = Phase::Done;
                        return;
                    }
                } else {
                    error!("PROBE bg: FAILURE — no VM to take the port through");
                    probe.phase = Phase::Done;
                    return;
                }
                probe.phase = Phase::Ported { sent_at: now };
            } else if now - sent_at > 12.0 && probe.rejoins < MAX_REJOINS {
                // **Leave the queue and join again.** vmangos evaluates a queue only when
                // something SCHEDULES an update, and a queue entry that was scheduled once and
                // did not match just sits there — no timer re-examines it
                // (`BattleGroundMgr::Update` drains `m_queueUpdateScheduler` and nothing refills
                // it on its own, `BattleGroundMgr.cpp:1005`). Whatever left the server's
                // bookkeeping unable to match this entry, a fresh one gets a fresh evaluation.
                // Both halves are real client verbs: `AcceptBattlefieldPort(slot, 0)` is the
                // queue's own decline, which no other probe or test here has ever walked.
                probe.rejoins += 1;
                let slot = queue
                    .slots()
                    .iter()
                    .position(|s| s.is_some())
                    .map_or(1, |i| i + 1);
                info!(
                    "PROBE bg: REJOIN {} — nothing popped in 12 s; \
                     AcceptBattlefieldPort({slot}, 0) then queueing again",
                    probe.rejoins
                );
                if let Some(script) = script.as_deref_mut() {
                    let _ = script.eval::<()>(&format!("AcceptBattlefieldPort({slot}, 0)"));
                }
                probe.phase = Phase::Rejoining { at: now };
            } else if now - sent_at > 30.0 {
                let slots: Vec<String> = queue
                    .slots()
                    .iter()
                    .map(|s| {
                        s.as_ref().map_or_else(
                            || "-".into(),
                            |(st, _)| format!("{}:{}", st.map_id, st.status),
                        )
                    })
                    .collect();
                error!(
                    "PROBE bg: FAILURE — no slot reached status 2 in 30 s (slots: {}). \
                     The testing flag was confirmed ON before the join, so the cause is not that: \
                     look for Deserter (26013) on the body, a bracket the server refused, or a \
                     battleground this body is still registered in from a killed run.",
                    slots.join(" ")
                );
                probe.phase = Phase::Done;
            }
        }
        Phase::Ported { sent_at } => {
            let here = map.as_ref().map_or(0, |m| m.0);
            if here == arena.map {
                info!(
                    "PROBE bg: ENTERED map {} after {:.1}s — census every {CENSUS_EVERY:.0}s × {}",
                    arena.map,
                    now - sent_at,
                    census_samples(),
                );
                probe.phase = Phase::Inside {
                    entered_at: now,
                    next_census: now + CENSUS_EVERY,
                    samples: 0,
                };
            } else if now - sent_at > 30.0 {
                error!(
                    "PROBE bg: FAILURE — 30 s after the port the map is still {here}, not {}",
                    arena.map
                );
                probe.phase = Phase::Done;
            }
        }
        Phase::Inside {
            entered_at,
            next_census,
            samples,
        } => {
            if now < next_census {
                return;
            }
            let Some(mut script) = script else {
                error!("PROBE bg: FAILURE — the VM went away inside the battleground");
                probe.phase = Phase::Done;
                return;
            };
            census(
                now - entered_at,
                arena,
                &player,
                &units,
                map.as_deref(),
                area.as_deref(),
                load.as_deref(),
                &states,
                &dropped,
                &go_templates,
                &queue,
                &mut script,
            );
            let samples = samples + 1;

            // **Flip the testing flag back OFF once the match is running**, which is the only way
            // a solo battleground can ever end. vmangos decrements `m_prematureCountDownTimer`
            // inside an `else if (!sBattleGroundMgr.isTesting())` branch (`BattleGround.cpp:337`),
            // so while testing is on the countdown is armed and **frozen** — the arm above it runs
            // once, the fire below it never reaches its condition, and a one-player Warsong Gulch
            // runs forever (vanilla WSG has no time limit; three captures is its only other end).
            // A run that measured this stayed in for 492 s with `winner` nil the whole way.
            //
            // Turned off at the first sample past the doors, so the flag is only on for the queue
            // that needed it. The countdown then runs its full `BattleGround.PrematureFinishTimer`
            // (5 min) from that moment, which is what [`census_samples`] has to cover.
            if samples == DOORS_SAMPLE {
                info!(
                    "PROBE bg: TESTING off — the premature-finish countdown is frozen while it is \
                     on, so a solo match could never end"
                );
                gm(&net, ".debug bg");
            }

            if samples >= census_samples() {
                // **The graveyard leg.** `.die` is the one thing that clears the probe's god
                // shield by design (0677), so a probe CAN die on purpose; a battleground death is
                // also the only place the area-spirit-healer arc (2291) is reachable at all, and
                // a mechanism that has never been watched fire is not a mechanism anyone should
                // claim. Non-combat: nothing kills us, we ask to be dead.
                match arena.flag {
                    Some((entry, [x, y, z])) => {
                        info!("PROBE bg: FLAG — hopping to {entry} at ({x}, {y}, {z}) to take it");
                        gm(&net, format!(".go xyz {x} {y} {z} {}", arena.map));
                        probe.phase = Phase::FlagHop { at: now };
                    }
                    None => {
                        info!("PROBE bg: FLAG — none for this battleground; on to the graveyard");
                        gm(&net, ".die");
                        probe.phase = Phase::Dying { sent_at: now };
                    }
                }
            } else {
                probe.phase = Phase::Inside {
                    entered_at,
                    next_census: now + CENSUS_EVERY,
                    samples,
                };
            }
        }
        Phase::Rejoining { at } => {
            if now - at < 2.0 {
                return; // let the decline land and the slot clear
            }
            let Some(master) = battlefield.battlemaster() else {
                error!("PROBE bg: FAILURE — the battlemaster list went away before the rejoin");
                probe.phase = Phase::Done;
                return;
            };
            let _ = net.0.send(ClientCommand::BattlemasterJoin {
                battlemaster: master,
                map_id: arena.map,
                instance_id: 0,
                as_group: false,
            });
            probe.phase = Phase::Joined { sent_at: now };
        }
        Phase::FlagHop { at } => {
            if now - at < 4.0 {
                return; // the hop crosses the map; let the far base stream in
            }
            let Some((entry, _)) = arena.flag else {
                probe.phase = Phase::Done;
                return;
            };
            let found = units.iter().find(|(guid, kind, _, tf)| {
                kind.kind == benilla_protocol::EntityKind::GameObject
                    && benilla_protocol::guid::entry(guid.0) == Some(entry)
                    && tf.translation.distance(player.pos) < 15.0
            });
            match found {
                Some((guid, _, _, tf)) => {
                    // What the click's own GameObject ladder would decide, reported rather than
                    // assumed: a FLAGSTAND carries no lock, so `resolve_go_action` takes its
                    // `Use` arm and the packet is `CMSG_GAMEOBJ_USE` — the one sent here.
                    let lock = go_templates.get(guid.0).map_or(u32::MAX, |t| t.lock_id);
                    info!(
                        "PROBE bg: FLAG {entry} streamed at {:.1} yd, lock_id={lock} — CMSG_GAMEOBJ_USE",
                        tf.translation.distance(player.pos)
                    );
                    let _ = net.0.send(ClientCommand::GameObjUse { guid: guid.0 });
                    probe.phase = Phase::FlagUsed {
                        at: now,
                        samples: 0,
                    };
                }
                None if now - at > 20.0 => {
                    error!("PROBE bg: FLAG FAILURE — {entry} never streamed within 15 yd in 20 s");
                    gm(&net, ".die");
                    probe.phase = Phase::Dying { sent_at: now };
                }
                None => {}
            }
        }
        Phase::FlagUsed { at, samples } => {
            if now - at < f64::from(samples + 1) * 3.0 {
                return;
            }
            let Some(script) = script else {
                probe.phase = Phase::Done;
                return;
            };
            // **The two readings that say the flag is on the body.** World state 2339 is the
            // ALLIANCE readout row's icon selector, and it is only ever 1 or 2 — 2 meaning the
            // flag that row is about is being carried (vmangos `BattleGroundWS.cpp:512` and
            // `FillInitialWorldStates` 713-722). It is **not** the four-value `m_flagState` enum,
            // which never reaches the client at all; that distinction is why a census reading
            // `2338=1 2339=1` for a whole match is correct rather than a stuck value. The aura is
            // the other half: what the reference actually draws on the body.
            let auras = script
                .eval::<String>(
                    r#"
                    local out = "";
                    for i = 1, 16 do
                        local t = UnitBuff("player", i);
                        if t then out = out .. " " .. t; end
                    end
                    return out;
                    "#,
                )
                .unwrap_or_else(|e| format!("<raised: {e}>"));
            let events = script.eval::<String>(EVENT_DRAIN).unwrap_or_default();
            info!(
                "PROBE bg: FLAG t={:.0}s ws2338={} ws2339={} captures={}/{} buffs=[{}]{}",
                now - at,
                states.get(2338),
                states.get(2339),
                states.get(1581),
                states.get(1582),
                auras.trim(),
                if events.trim().is_empty() {
                    String::new()
                } else {
                    format!(" events={events}")
                },
            );
            let samples = samples + 1;
            if samples >= FLAG_SAMPLES {
                info!("PROBE bg: FLAG done — on to the graveyard");
                gm(&net, ".die");
                probe.phase = Phase::Dying { sent_at: now };
            } else {
                probe.phase = Phase::FlagUsed { at, samples };
            }
        }
        Phase::Dying { sent_at } => {
            if store.0.unit_is_dead() {
                info!("PROBE bg: DEAD — releasing (CMSG_REPOP_REQUEST)");
                let _ = net.0.send(ClientCommand::RepopRequest);
                probe.phase = Phase::Ghost {
                    released_at: now,
                    next_sample: now + GHOST_SAMPLE_EVERY,
                    samples: 0,
                };
            } else if now - sent_at > 15.0 {
                error!(
                    "PROBE bg: FAILURE — still alive 15 s after `.die` (is the shield re-armed?)"
                );
                probe.phase = Phase::Done;
            }
        }
        Phase::Ghost {
            released_at,
            next_sample,
            samples,
        } => {
            if now < next_sample {
                return;
            }
            let Some(mut script) = script else {
                probe.phase = Phase::Done;
                return;
            };
            // The whole point of this leg, in four readings: are we a ghost, is a spirit guide in
            // range, did the engine adopt it (`GetAreaSpiritHealerTime` is non-zero only once
            // `SMSG_AREA_SPIRIT_HEALER_TIME` answered the query the adopt sent), and did the
            // stock dialog's event fire.
            let ghost = store.0.player_is_ghost();
            let nearest = units
                .iter()
                .filter(|(_, k, st, _)| {
                    k.kind == benilla_protocol::EntityKind::Unit
                        && st.0.unit_npc_flags() & NPC_FLAG_SPIRITGUIDE != 0
                })
                .map(|(g, _, _, tf)| (g.0, tf.translation.distance(player.pos)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let wave = script
                .eval::<String>(
                    "return tostring(GetAreaSpiritHealerTime()) .. \" popup=\" .. \
                     tostring(StaticPopup_Visible and StaticPopup_Visible(\"AREA_SPIRIT_HEAL\"))",
                )
                .unwrap_or_else(|e| format!("<raised: {e}>"));
            let events = script.eval::<String>(EVENT_DRAIN).unwrap_or_default();
            info!(
                "PROBE bg: GHOST t={:.0}s ghost={ghost} pos=({:.0},{:.0},{:.0}) area={:?} guide={} wave={wave}{}",
                now - released_at,
                player.pos.x,
                player.pos.y,
                player.pos.z,
                area.as_deref().and_then(|a| a.0),
                nearest.map_or_else(
                    || "none".to_string(),
                    |(g, d)| format!("{g:#x}@{d:.1}yd")
                ),
                if events.trim().is_empty() {
                    String::new()
                } else {
                    format!(" events={events}")
                },
            );
            let samples = samples + 1;
            if samples >= GHOST_SAMPLES {
                // Take the wave if one is offered — the verb that was permanently silent before
                // 2291, and the only way back to the world from a battleground graveyard.
                match script.eval::<()>("AcceptAreaSpiritHeal()") {
                    Ok(()) => info!("PROBE bg: GHOST AcceptAreaSpiritHeal() sent"),
                    Err(e) => error!("PROBE bg: GHOST AcceptAreaSpiritHeal() raised: {e}"),
                }
                report(arena, &net, &queue, &mut script);
                probe.phase = Phase::Done;
            } else {
                probe.phase = Phase::Ghost {
                    released_at,
                    next_sample: now + GHOST_SAMPLE_EVERY,
                    samples,
                };
            }
        }
        Phase::Done => {}
    }
}

/// One census sample — eleven greppable `PROBE bg:` lines describing everything the client can see
/// from inside the battleground. Deliberately verbose: this is the first look anyone has had, and
/// a column nobody needed is cheaper than a round trip for one that was left out.
#[allow(clippy::too_many_arguments)]
fn census(
    t: f64,
    arena: &Arena,
    player: &Player,
    units: &Query<(&Guid, &NetEntity, &ObjectStore, &Transform), Without<SelfPlayer>>,
    map: Option<&CurrentMap>,
    area: Option<&CurrentArea>,
    load: Option<&WorldLoadProgress>,
    states: &WorldStates,
    dropped: &DroppedOpcodes,
    go_templates: &crate::go_templates::GameObjectTemplates,
    queue: &BattlefieldQueue,
    script: &mut UiScript,
) {
    use benilla_protocol::EntityKind as K;

    let p = player.pos;
    let terrain = load.map_or_else(
        || "no-progress".to_string(),
        |l| {
            format!(
                "{}/{} focus={} scene={} colliders={}",
                l.ready, l.total, l.focus_resident, l.scene_ready, l.colliders_pending
            )
        },
    );
    info!(
        "PROBE bg: CENSUS t={t:.0}s map={} area={:?} pos=({:.0},{:.0},{:.0}) terrain={terrain}",
        map.map_or(0, |m| m.0),
        area.and_then(|a| a.0),
        p.x,
        p.y,
        p.z,
    );

    // Entities, by kind — "is there a world here at all?"
    let (mut u, mut pl, mut go, mut dy, mut co) = (0, 0, 0, 0, 0);
    let mut go_entries: Vec<(u32, Option<String>)> = Vec::new();
    let mut nearest_guide: Option<(u64, f32)> = None;
    for (guid, kind, store, tf) in units.iter() {
        match kind.kind {
            K::Unit => {
                u += 1;
                if store.0.unit_npc_flags() & NPC_FLAG_SPIRITGUIDE != 0 {
                    let d = tf.translation.distance(p);
                    if nearest_guide.is_none_or(|(_, best)| d < best) {
                        nearest_guide = Some((guid.0, d));
                    }
                }
            }
            K::Player => pl += 1,
            K::GameObject => {
                go += 1;
                if let Some(e) = benilla_protocol::guid::entry(guid.0) {
                    // The NAME, not just the entry: "179918" is a number to look up in a database
                    // and `Doodad_PortcullisActive01` is the battleground's gate. The cache is the
                    // ask-once `GAMEOBJECT_QUERY` store (0239) the client already fills for every
                    // GO that streams in, so this costs nothing and answers for free.
                    go_entries.push((e, go_templates.get(guid.0).map(|t| t.name.clone())));
                }
            }
            K::DynamicObject => dy += 1,
            K::Corpse => co += 1,
            K::Other => {}
        }
    }
    go_entries.sort_unstable();
    let mut counted: Vec<String> = Vec::new();
    let mut i = 0;
    while i < go_entries.len() {
        let (e, ref name) = go_entries[i];
        let n = go_entries[i..].iter().take_while(|(x, _)| *x == e).count();
        let label = name
            .as_deref()
            .map_or_else(|| e.to_string(), |n| format!("{e}:{n}"));
        counted.push(if n > 1 {
            format!("{label}×{n}")
        } else {
            label
        });
        i += n;
    }
    info!("PROBE bg: ENTS units={u} players={pl} gos={go} dyn={dy} corpses={co}");
    info!(
        "PROBE bg: GOS [{}]",
        if counted.is_empty() {
            "none".to_string()
        } else {
            counted.join(" ")
        }
    );
    info!(
        "PROBE bg: GUIDE {}",
        nearest_guide.map_or_else(
            || "none in range".to_string(),
            |(g, d)| format!("{g:#x} at {d:.1} yd")
        )
    );

    // The world-state table — the battleground's whole score, as raw dwords.
    let mut pairs: Vec<(u32, i32)> = states.pairs().collect();
    pairs.sort_unstable_by_key(|(k, _)| *k);
    info!(
        "PROBE bg: STATES scope={:?} n={} [{}]",
        states.scope(),
        pairs.len(),
        pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // What the reference's own readout would draw, straight out of the VM.
    let alwaysup = script
        .eval::<String>(
            r#"
            local n = GetNumWorldStateUI()
            local out = tostring(n)
            for i = 1, n do
                local ui, state, hidden, text, icon = GetWorldStateUIInfo(i)
                out = out .. " | " .. tostring(state) .. " '" .. tostring(text) .. "' icon=" .. tostring(icon)
            end
            return out
            "#,
        )
        .unwrap_or_else(|e| format!("<raised: {e}>"));
    info!("PROBE bg: ALWAYSUP {alwaysup}");

    let status = script
        .eval::<String>(
            r#"
            local out = ""
            for i = 1, 3 do
                local s, name, id, lo, hi = GetBattlefieldStatus(i)
                out = out .. i .. "=" .. tostring(s) .. "/" .. tostring(name) .. "/" .. tostring(id) .. " "
            end
            return out .. "runtime=" .. tostring(GetBattlefieldInstanceRunTime())
            "#,
        )
        .unwrap_or_else(|e| format!("<raised: {e}>"));
    info!(
        "PROBE bg: STATUS {status} active_map={:?}",
        queue.active_map()
    );

    let score = script
        .eval::<String>(
            r#"
            RequestBattlefieldScoreData()
            return tostring(GetNumBattlefieldScores()) .. " winner=" .. tostring(GetBattlefieldWinner())
            "#,
        )
        .unwrap_or_else(|e| format!("<raised: {e}>"));
    info!("PROBE bg: SCORE n={score}");

    // **The wire-coverage tally** — every opcode the codec threw on the floor, by name. This is
    // the column the first run did not have and most needed: a battleground that sends something
    // benilla has no arm for is invisible in every other reading here, and reads as "nothing
    // happened" rather than as a gap. `DroppedOpcodes` is never cleared, so the numbers are
    // cumulative over the whole session — a row that grows between samples is the finding.
    let mut drops: Vec<(u16, u64, u64)> = dropped
        .0
        .iter()
        .map(|(&op, t)| (op, t.unknown, t.unparseable))
        .collect();
    drops.sort_unstable_by_key(|(op, _, _)| *op);
    info!(
        "PROBE bg: DROPPED [{}]",
        if drops.is_empty() {
            "none".to_string()
        } else {
            drops
                .iter()
                .map(|(op, unk, bad)| {
                    let name = benilla_protocol::messages::opcode_name(*op)
                        .map_or_else(|| format!("{op:#06x}"), str::to_string);
                    format!("{name}:unknown={unk},unparseable={bad}")
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    );

    // The event tap's window (see `EVENT_TAP`): what the VM was actually told since the last
    // sample. An empty window while the server is running the start countdown is a finding.
    let events = script
        .eval::<String>(EVENT_DRAIN)
        .unwrap_or_else(|e| format!("<raised: {e}>"));
    info!(
        "PROBE bg: EVENTS{}",
        if events.trim().is_empty() {
            "  (none)".to_string()
        } else {
            events
        }
    );

    let errors = script.take_errors();
    if errors.is_empty() {
        info!("PROBE bg: LUA clean");
    } else {
        for e in &errors {
            error!("PROBE bg: LUA ERROR {e}");
        }
    }
    let _ = arena;
}

/// The run's last act: drive the two battleground windows the census cannot reach by watching
/// (they are opened by a keybind, never by an event), then put the server's testing toggle back
/// and leave the battleground the way a player does.
fn report(arena: &Arena, net: &NetCommands, queue: &BattlefieldQueue, script: &mut UiScript) {
    // The scoreboard and the battlefield minimap: both are LoadOnDemand-shaped roads nothing in
    // this tree has ever walked. A raise here is the finding.
    // **Read both windows BEFORE toggling them.** The scoreboard shows itself when a match ends —
    // `WorldStateScoreFrame_Update` does `if (GetBattlefieldWinner()) then ShowUIPanel(...)`,
    // driven by `UPDATE_BATTLEFIELD_SCORE` — so after a real ending the toggle below *hides* it,
    // and reading only the post-toggle state reports `IsShown=nil` for the run where the feature
    // worked. That is the wrong way round, and it cost a round of inference to notice.
    //
    // Through `getglobal`, because `BattlefieldMinimap` is LoadOnDemand: indexing it before the
    // toggle that loads it is a nil-index raise, and "not loaded yet" is a reading, not an error.
    for name in ["WorldStateScoreFrame", "BattlefieldMinimap"] {
        let shown = script
            .eval::<String>(&format!(
                "local f = getglobal(\"{name}\")                  if not f then return \"not-loaded\" end return tostring(f:IsShown())"
            ))
            .unwrap_or_else(|e| format!("<raised: {e}>"));
        info!("PROBE bg: BEFORE-TOGGLE {name}:IsShown={shown}");
    }
    for (what, chunk) in [
        ("SCOREFRAME", "ToggleWorldStateScoreFrame()"),
        ("BFMINIMAP", "ToggleBattlefieldMinimap()"),
    ] {
        match script.eval::<()>(chunk) {
            Ok(()) => {
                let shown = script
                    .eval::<String>(&format!(
                        "return tostring({}:IsShown())",
                        if what == "SCOREFRAME" {
                            "WorldStateScoreFrame"
                        } else {
                            "BattlefieldMinimap"
                        }
                    ))
                    .unwrap_or_else(|e| format!("<raised: {e}>"));
                info!("PROBE bg: {what} {chunk} ok, IsShown={shown}");
            }
            Err(e) => error!("PROBE bg: {what} FAILURE — {chunk} raised: {e}"),
        }
    }
    for e in script.take_errors() {
        error!("PROBE bg: LUA ERROR {e}");
    }

    // `LeaveBattlefield` is gated client-side on the scoreboard's "ended" byte (1972), so while a
    // match is live the verb is correctly silent — which means the honest way out is the same one
    // a player uses when they give up on a battleground: the GM teleport home. The queue slot the
    // server still holds is cleared by the leave it sends on the map change.
    info!(
        "PROBE bg: LEAVING map {} (active_map={:?})",
        arena.map,
        queue.active_map()
    );
    let _ = script.eval::<()>("LeaveBattlefield()");
    gm(net, ".recall");
    // The lever was already put back at `DOORS_SAMPLE` (see the `Inside` arm) — a second toggle
    // here would turn it back ON and leave it that way for the next run.
    info!("PROBE bg: DONE");
}
