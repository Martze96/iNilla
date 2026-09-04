//! The **NPC-service ladder** live probe (`WOW_PROBE_SERVICE=1`) — decision 1861's end-to-end
//! instrument, and the answer to "does right-clicking a trainer or a quest giver really open their
//! window instead of a gossip menu?".
//!
//! 1861 replaced a cursor-kind dispatch with the reference's own first-match-wins walk over
//! `UNIT_NPC_FLAGS` ([`crate::target::click::service_arm`]; wow-re
//! `object-layer/scratch/interact-dead-fork-and-npc-service-ladder.md` §C). **Bit 0 (GOSSIP) is
//! tested first**, so the flag — not the profession — decides: a trainer or questgiver that also
//! carries GOSSIP still opens a gossip menu, and only a flagless one opens its own window. That
//! precedence is the whole question, and it is invisible to a unit test: the bits come off the
//! wire, and what appears on screen is the *server's* answer to the opcode the ladder picked.
//!
//! So the probe walks four real Northshire/Stormwind NPCs, one per interesting flag shape, and for
//! each one reports the wire's `UNIT_NPC_FLAGS`, the arm the shipped ladder takes, the opcode it
//! sends, and which window actually opened:
//!
//! | NPC | `npc_flags` | arm | window |
//! |---|---|---|---|
//! | Marshal McBride (197) | `0x03` GOSSIP\|QUESTGIVER | Gossip | gossip menu |
//! | Llane Beshere (911) | `0x13` GOSSIP\|QUESTGIVER\|TRAINER | Gossip | gossip menu |
//! | *any pure questgiver with an offer* | `0x02`, GOSSIP clear | Questgiver | quest frame |
//! | Alma Jainrose (812) | `0x10` TRAINER only | Trainer | trainer frame |
//!
//! The flag column is live-DB verified this session (`vmangos-deploy` → `mangos`,
//! `SELECT entry, name, npc_flags FROM creature_template`), and reported again from the wire on
//! every run — a template edit shows up as a note beside the reading rather than as a mystery FAIL.
//!
//! **The ladder is called, never re-implemented.** The probe reads the same two inputs the click
//! reads (`ObjectStore::unit_npc_flags` and [`QuestGiver::status`]) and calls the shipped
//! `service_arm`/`service_action`; only the mouse hit-test and the cursor's range gray sit outside
//! it. A private copy of the bit table is exactly how a probe goes quietly stale (the B249 icon
//! map), so there isn't one.
//!
//! **The pure-questgiver row is found by predicate, not by entry.** Whether Deputy Willem has
//! anything for *this* probe character depends on what it has already turned in, and the bit-1 arm
//! is gated on that status (`[unit+0xcb8] ∉ {0,1}`). So that leg hops into Northshire and takes the
//! first streamed unit that is QUESTGIVER, not GOSSIP, and actually has an offer — reporting which
//! one it picked. If the character has cleared the whole valley it SKIPs and says so.
//!
//! One `PROBE_SERVICE: <leg> PASS/FAIL/SKIP <detail>` line per leg, then a final
//! `PROBE_SERVICE: DONE pass=<n> fail=<m> skip=<k>`. A wrong arm or a wrong window is a FAIL; an
//! environmental problem (the `.go` refused, the NPC never streamed, no offer left in the valley)
//! is a SKIP with the reason.
//!
//! ## The run recipe
//!
//! ```text
//! WOW_DATA=WoW/Data WOW_USER=probe4 WOW_PASS=pprobe4 WOW_CHAR=Probefour \
//!     WOW_UNATTENDED=1 WOW_PROBE_SERVICE=1 cargo run -q -p benilla
//! ```
//! (the slot-keyed probe identity — method.md "The local vmangos server"). Non-combat, GM mode
//! left as found, nothing bought and nothing turned in: every leg opens a window and closes it
//! client-side, which is what the reference's own close does (`ui_gossip`: there is no
//! `CMSG_GOSSIP_CLOSE` in 1.12).

use bevy::prelude::*;

use benilla_protocol::EntityKind;

use super::probes::ProbeClock;
use crate::net::{ChatKind, ClientCommand, Guid, NetCommands, NetEntity, ObjectStore, SelfPlayer};
use crate::player::Player;
use crate::target::click::{service_action, service_arm, ServiceAction, ServiceArm};
use crate::target::cursor_mode::npc_flags as f;
use crate::ui_gossip::GossipState;
use crate::ui_quest::QuestGiver;
use crate::ui_trainer::TrainerOpen;

/// How wide to look around a `.go` landing for the leg's NPC — **and no wider than the reference's
/// own service reach**, because the click cannot act past it either: beyond
/// [`crate::target::SERVICE_RANGE_SQ`] (5.5556 yd, `0xc4c28c`/`0xb4b32c`) the cursor goes gray, the
/// click sends nothing, and vmangos's `GetNPCIfCanInteractWith` refuses the opcode silently.
///
/// This constant is the reason the questgiver leg first read as a client defect: its hop point was
/// an invented Northshire "hub" ~29 yd from Deputy Willem, so the probe called a ladder the click
/// would never have reached, the server dropped the packet on its own distance check, and the
/// timeout printed a FAIL about the client (method.md §6 — prove the run before reading the
/// result). Every hop point is now an NPC's own spawn, and the scan re-checks the reach.
const SCAN_RANGE_SQ: f32 = crate::target::SERVICE_RANGE_SQ;
/// Let the hop land and the tile stream before the first scan.
const SETTLE_SECS: f64 = 3.0;
/// How long a leg waits for its NPC to stream in before calling the hop environmental.
const SCAN_TIMEOUT_SECS: f64 = 20.0;
/// How long a leg waits for its window after the opcode goes out. Generous for the same reason the
/// binder probe's is: the observer is starved during a terrain load, never the wire.
const WINDOW_TIMEOUT_SECS: f64 = 20.0;

/// Which NPC a leg is looking for.
enum Ident {
    /// A specific `creature_template.entry` — the identity check that cannot be confused by a
    /// neighbour who happens to share a flag.
    Entry(u32),
    /// The bit-1 arm's own shape: QUESTGIVER set, GOSSIP clear, and an offer live for THIS
    /// character (`SMSG_QUESTGIVER_STATUS` ∉ {0,1}) — see the module doc.
    PureQuestgiverWithOffer,
}

/// Which window the leg's arm must put on screen.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Window {
    Gossip,
    Quest,
    Trainer,
}

/// One leg: hop here, find this NPC, run the ladder on it, expect this arm and this window.
struct Leg {
    /// What the log calls it.
    name: &'static str,
    ident: Ident,
    map: u32,
    /// The spawn points to try, in order — each an NPC's own `creature.position_*`, so the landing
    /// is inside the service reach. A leg with several is one whose subject depends on what this
    /// character has already turned in (the questgiver leg); it moves to the next candidate when
    /// the one it hopped to has nothing to offer.
    at: &'static [[f32; 3]],
    /// `creature_template.npc_flags` as the world DB held it when this probe was written — printed
    /// beside the wire's value so a template edit reads as a note rather than a mystery.
    db_flags: u32,
    want_arm: ServiceArm,
    want_window: Window,
}

/// The four flag shapes worth a live reading (module doc's table). Northshire first — three of the
/// four legs share one hop's worth of terrain — then the Stormwind first-aid trainer.
const LEGS: &[Leg] = &[
    Leg {
        name: "Marshal McBride — GOSSIP|QUESTGIVER",
        ident: Ident::Entry(197),
        map: 0,
        at: &[[-8902.59, -162.606, 82.0223]],
        db_flags: 0x3,
        want_arm: ServiceArm::Gossip,
        want_window: Window::Gossip,
    },
    Leg {
        name: "Llane Beshere — GOSSIP|QUESTGIVER|TRAINER",
        ident: Ident::Entry(911),
        map: 0,
        at: &[[-8918.36, -208.411, 82.3088]],
        db_flags: 0x13,
        want_arm: ServiceArm::Gossip,
        want_window: Window::Gossip,
    },
    Leg {
        name: "a pure questgiver with an offer — QUESTGIVER, no GOSSIP",
        ident: Ident::PureQuestgiverWithOffer,
        map: 0,
        // Deputy Willem (823), Eagan Peltskinner (196), Falkhaan Isenstrider (6774) — the three
        // pure questgivers of Northshire, each at its own spawn (live-DB verified this session).
        at: &[
            [-8933.54, -136.523, 83.4466],
            [-8869.22, -163.237, 80.9719],
            [-9044.56, -45.9817, 88.4193],
        ],
        db_flags: 0x2,
        want_arm: ServiceArm::Questgiver,
        want_window: Window::Quest,
    },
    Leg {
        name: "Alma Jainrose — TRAINER, no GOSSIP",
        ident: Ident::Entry(812),
        map: 0,
        at: &[[-9237.78, -2041.65, 78.1678]],
        db_flags: 0x10,
        want_arm: ServiceArm::Trainer,
        want_window: Window::Trainer,
    },
];

pub(crate) struct ProbeServicePlugin;

impl Plugin for ProbeServicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServiceProbe>()
            .add_systems(Update, service_probe);
    }
}

#[derive(Resource, Default)]
struct ServiceProbe {
    phase: Phase,
    passes: u32,
    fails: u32,
    skips: u32,
    /// Latched once [`Phase::Done`] has fired its exit (never re-fire on a later frame).
    exited: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Phase {
    /// Not in-world yet, or between legs: `.go` for leg `i` still to send.
    #[default]
    Start,
    Hop {
        i: usize,
        /// Which of the leg's spawn points we hopped to.
        hop: usize,
        sent_at: f64,
    },
    /// The arm's action has gone out; waiting for the window it must open.
    Await {
        i: usize,
        since: f64,
        guid: u64,
    },
    Done,
}

/// A flag word as the log wants it — the hex plus the bit names, so a reader never has to decode
/// `0x13` by hand.
fn flag_names(flags: u32) -> String {
    const NAMES: [(u32, &str); 14] = [
        (f::GOSSIP, "GOSSIP"),
        (f::QUESTGIVER, "QUESTGIVER"),
        (f::VENDOR, "VENDOR"),
        (f::FLIGHTMASTER, "FLIGHTMASTER"),
        (f::TRAINER, "TRAINER"),
        (f::SPIRITHEALER, "SPIRITHEALER"),
        (f::SPIRITGUIDE, "SPIRITGUIDE"),
        (f::INNKEEPER, "INNKEEPER"),
        (f::BANKER, "BANKER"),
        (f::PETITIONER, "PETITIONER"),
        (f::TABARDDESIGNER, "TABARDDESIGNER"),
        (f::BATTLEMASTER, "BATTLEMASTER"),
        (f::AUCTIONEER, "AUCTIONEER"),
        (f::STABLEMASTER, "STABLEMASTER"),
    ];
    let set: Vec<&str> = NAMES
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    match set.is_empty() {
        true => format!("{flags:#x} (none)"),
        false => format!("{flags:#x} {}", set.join("|")),
    }
}

/// Which window is open on `guid`, if any — the observation every leg is judged on.
fn open_window(
    guid: u64,
    gossip: &GossipState,
    giver: &QuestGiver,
    trainer: &TrainerOpen,
) -> Option<Window> {
    // The gossip frame does not open on the packet: it holds closed until the greeting's
    // `CMSG_NPC_TEXT_QUERY` answers (B292's hold, `ui_gossip`), so the greeting is part of "open".
    if gossip.npc == Some(guid) && gossip.greeting.is_some() {
        return Some(Window::Gossip);
    }
    if giver.npc == Some(guid) && giver.view.is_some() {
        return Some(Window::Quest);
    }
    if trainer.trainer == Some(guid) {
        return Some(Window::Trainer);
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn service_probe(
    time: ProbeClock,
    mut probe: ResMut<ServiceProbe>,
    mut gossip: ResMut<GossipState>,
    mut giver: ResMut<QuestGiver>,
    mut trainer: ResMut<TrainerOpen>,
    self_player: Query<(), With<SelfPlayer>>,
    player: Res<Player>,
    units: Query<(&Guid, &NetEntity, &ObjectStore, &Transform), Without<SelfPlayer>>,
    net: Res<NetCommands>,
    mut exit: MessageWriter<AppExit>,
) {
    if self_player.is_empty() {
        return; // not in-world yet
    }
    let now = time.elapsed_secs_f64();

    match probe.phase {
        Phase::Start => {
            hop(&mut probe, &net, 0, 0, now);
        }
        Phase::Hop { i, hop: h, sent_at } => {
            if now - sent_at < SETTLE_SECS {
                return;
            }
            let leg = &LEGS[i];
            let me = player.pos;
            let found = units.iter().find(|(guid, net_e, store, tf)| {
                // The click's own gate: `dist² > SERVICE_RANGE_SQ` is the cursor's `unable`, and
                // the arm never runs. Acting outside it would measure nothing.
                if net_e.kind != EntityKind::Unit
                    || tf.translation.distance_squared(me) > SCAN_RANGE_SQ
                {
                    return false;
                }
                let flags = store.0.unit_npc_flags();
                match leg.ident {
                    Ident::Entry(entry) => store.0.object_entry() == Some(entry),
                    Ident::PureQuestgiverWithOffer => {
                        flags & f::QUESTGIVER != 0
                            && flags & f::GOSSIP == 0
                            && crate::target::cursor_mode::questgiver_has_quest(
                                giver.status(guid.0),
                            )
                    }
                }
            });
            let Some((guid, _, store, _)) = found else {
                if now - sent_at > SCAN_TIMEOUT_SECS {
                    if h + 1 < leg.at.len() {
                        info!(
                            "PROBE_SERVICE: ({}) nothing eligible at spawn {}/{} — trying the next",
                            leg.name,
                            h + 1,
                            leg.at.len()
                        );
                        hop(&mut probe, &net, i, h + 1, now);
                        return;
                    }
                    warn!(
                        "PROBE_SERVICE: SKIP ({}) — nothing matching streamed inside the \
                         {:.4}yd service reach of any of this leg's {} spawn point(s) in \
                         {SCAN_TIMEOUT_SECS}s each (the `.go` may have been refused, the tile may \
                         not have streamed, or — for the questgiver leg — this character has \
                         nothing left to pick up in the valley). Environmental.",
                        leg.name,
                        SCAN_RANGE_SQ.sqrt(),
                        leg.at.len()
                    );
                    probe.skips += 1;
                    next(&mut probe, &net, i, now);
                }
                return;
            };
            let guid = guid.0;
            let flags = store.0.unit_npc_flags();
            let entry = store.0.object_entry().unwrap_or(0);
            let status = giver.status(guid);

            // The shipped ladder, on the shipped inputs.
            let arm = service_arm(flags, status);
            let note = match flags == leg.db_flags {
                true => String::new(),
                false => format!(
                    " [note: the world DB read {} when this leg was written]",
                    flag_names(leg.db_flags)
                ),
            };
            let Some(arm) = arm else {
                error!(
                    "PROBE_SERVICE: FAIL ({}) — entry {entry} guid {guid:#x} flags {} \
                     status={status:?}: the ladder matched NO bit, so the click sends nothing. \
                     Wanted {:?}.{note}",
                    leg.name,
                    flag_names(flags),
                    leg.want_arm
                );
                probe.fails += 1;
                next(&mut probe, &net, i, now);
                return;
            };
            if arm != leg.want_arm {
                error!(
                    "PROBE_SERVICE: FAIL ({}) — entry {entry} guid {guid:#x} flags {} \
                     status={status:?}: the ladder took {arm:?}, wanted {:?}. First-match-wins \
                     low→high over UNIT_NPC_FLAGS is the reference's own order \
                     (`0x5f0289`…`0x5f05bc`); a disagreement here is the dispatch, not the \
                     server.{note}",
                    leg.name,
                    flag_names(flags),
                    leg.want_arm
                );
                probe.fails += 1;
                next(&mut probe, &net, i, now);
                return;
            }

            // A stale window from the previous leg would answer this one's question. The
            // reference's own close sends no packet, so a local clear is the whole of it.
            gossip.clear();
            giver.clear();
            trainer.clear();

            let sent = match service_action(arm, guid, false) {
                ServiceAction::Send(cmd) => {
                    let named = format!("{cmd:?}");
                    let _ = net.0.send(cmd);
                    named
                }
                // No leg here takes a dialog arm; if one ever does, say so rather than hang.
                other => {
                    let why: &str = match other {
                        ServiceAction::AskBinder => "CONFIRM_BINDER, no packet",
                        ServiceAction::AskSpiritHealer => "CONFIRM_XP_LOSS, no packet",
                        ServiceAction::Silent(w) => w,
                        ServiceAction::Send(_) => unreachable!(),
                    };
                    warn!(
                        "PROBE_SERVICE: SKIP ({}) — arm {arm:?} opens no window from a packet \
                         ({why}); this probe's legs are the four that do.{note}",
                        leg.name
                    );
                    probe.skips += 1;
                    next(&mut probe, &net, i, now);
                    return;
                }
            };
            info!(
                "PROBE_SERVICE: ({}) entry {entry} guid {guid:#x} flags {} status={status:?} \
                 → arm {arm:?} → {sent}{note}",
                leg.name,
                flag_names(flags)
            );
            probe.phase = Phase::Await {
                i,
                since: now,
                guid,
            };
        }
        Phase::Await { i, since, guid } => {
            let leg = &LEGS[i];
            match open_window(guid, &gossip, &giver, &trainer) {
                Some(open) if open == leg.want_window => {
                    info!(
                        "PROBE_SERVICE: PASS ({}) — {:?} window open on {guid:#x}{}",
                        leg.name,
                        open,
                        match open {
                            Window::Gossip => format!(
                                " ({} option(s), {} quest row(s), greeting {:?})",
                                gossip.options.len(),
                                gossip.quests.len(),
                                gossip.greeting.as_deref().unwrap_or("")
                            ),
                            Window::Quest => format!(" ({:?})", giver.view.as_ref().map(discr)),
                            Window::Trainer => format!(
                                " ({} service(s), type {})",
                                trainer.services.len(),
                                trainer.trainer_type
                            ),
                        }
                    );
                    probe.passes += 1;
                    next(&mut probe, &net, i, now);
                }
                Some(open) => {
                    error!(
                        "PROBE_SERVICE: FAIL ({}) — {open:?} window opened on {guid:#x}, wanted \
                         {:?}",
                        leg.name, leg.want_window
                    );
                    probe.fails += 1;
                    next(&mut probe, &net, i, now);
                }
                None if now - since > WINDOW_TIMEOUT_SECS => {
                    // Distinguish the one near-miss that is a real defect from a dead wire: the
                    // gossip session latched but its greeting never resolved, so the frame stayed
                    // shut (`GossipState::resolve_greeting`'s "Missing gossip text!" path).
                    let held = gossip.npc == Some(guid) && gossip.greeting.is_none();
                    error!(
                        "PROBE_SERVICE: FAIL ({}) — no {:?} window on {guid:#x} within \
                         {WINDOW_TIMEOUT_SECS}s of the send{}",
                        leg.name,
                        leg.want_window,
                        match held {
                            true => format!(
                                " (the gossip session latched on text_id {} but the greeting never \
                                 resolved — the frame never opens on that path)",
                                gossip.text_id
                            ),
                            false => String::new(),
                        }
                    );
                    probe.fails += 1;
                    next(&mut probe, &net, i, now);
                }
                None => {}
            }
        }
        Phase::Done => {
            if probe.exited {
                return;
            }
            probe.exited = true;
            info!(
                "PROBE_SERVICE: DONE pass={} fail={} skip={}",
                probe.passes, probe.fails, probe.skips
            );
            // The probe self-exit pattern (`ProbeExitPlugin::fire_probe_exit`): a polite AppExit
            // plus a hard backstop thread, so a net/winit teardown hang can't leave a zombie
            // client holding the probe account.
            exit.write(AppExit::Success);
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(5));
                warn!("PROBE_SERVICE: still alive 5s after AppExit — hard exit");
                std::process::exit(0);
            });
        }
    }
}

/// The open quest panel's shape, for the PASS line — `Greeting` (the multi-quest list) vs
/// `Detail` (the single quest's own frame) is exactly the distinction a reader wants here.
fn discr(view: &crate::ui_quest::QuestView) -> &'static str {
    use crate::ui_quest::QuestView as V;
    match view {
        V::Greeting(_) => "Greeting",
        V::Detail(_) => "Detail",
        V::Progress(_) => "Progress",
        V::Reward(_) => "Reward",
    }
}

/// Send leg `i`'s `.go` for spawn point `h` and enter [`Phase::Hop`].
fn hop(probe: &mut ServiceProbe, net: &NetCommands, i: usize, h: usize, now: f64) {
    let leg = &LEGS[i];
    let [x, y, z] = leg.at[h];
    info!(
        "PROBE_SERVICE: hopping to {} at ({x}, {y}, {z}) map {} (spawn {}/{})",
        leg.name,
        leg.map,
        h + 1,
        leg.at.len()
    );
    let _ = net.0.send(ClientCommand::Chat {
        kind: ChatKind::Say,
        target: None,
        text: format!(".go xyz {x} {y} {z} {}", leg.map),
    });
    probe.phase = Phase::Hop {
        i,
        hop: h,
        sent_at: now,
    };
}

/// Advance past leg `i`, or finish.
fn next(probe: &mut ServiceProbe, net: &NetCommands, i: usize, now: f64) {
    match i + 1 < LEGS.len() {
        true => hop(probe, net, i + 1, 0, now),
        false => probe.phase = Phase::Done,
    }
}
