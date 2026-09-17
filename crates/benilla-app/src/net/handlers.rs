//! The inbound **handler table** (decision 2305) — the reference's own shape for the wire's
//! arrival side, and the cut 2265 §A1 asked for between the net bridge and the game windows.
//!
//! The real client keeps an opcode → handler table inside `NetClient` (`+0x74`, 828 slots; 387
//! registrations by 37 subsystem clusters — wow-re `net.md`) and its dispatcher `0x537aa0` knows
//! none of them: it looks the opcode up and calls what it finds, **in packet order**, discarding an
//! unregistered opcode in silence. Here the table is [`NetHandlers`]: a [`SessionEventKind`] →
//! handlers map that every subsystem fills for itself through [`NetHandlerApp::net_handler`], and
//! the drain ([`super::apply_net_updates`]) is exclusive over the world so the handlers — ordinary
//! systems taking `In<SessionEvent>`, registered as one-shots — run one after another in the
//! order the packets arrived, each seeing what the one before it did.
//!
//! **Why not one typed `Message<T>` per family with a reader system each** (2265's sketch): a
//! reader per family runs *after* the whole drain, so two families' packets interleaved in one
//! frame are handled family by family, not in packet order — a chat line and a combat-log line
//! that arrived in one drain would swap. The table keeps the one property 2265 said must survive
//! any split: one frame, packet order, before anything else runs.
//!
//! **The migration.** The dispatch `match` in `apply.rs` still owns every kind no subsystem has
//! claimed. One drain runs in two halves: the unclaimed events through the match first, in packet
//! order, then the claimed ones through the table, in packet order — so a window handler always
//! sees this frame's object updates, whatever the interleaving. A kind is owned by exactly one of
//! the two, checked on the built app by `every_session_event_kind_has_one_owner`; the one
//! exception is [`BROADCAST`], a session-end the match still handles and peeled windows also listen
//! to, which reaches both. When the last family leaves the match, the match, the halves and the
//! broadcast list go with it.

use std::collections::HashMap;

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::ecs::system::SystemId;
use bevy::prelude::*;

/// One registered handler: the one-shot system and the name it registers under (for the census).
type Handler = (SystemId<In<SessionEvent>>, &'static str);

/// The table. Filled at plugin build by every subsystem that answers a packet; read by the drain.
#[derive(Resource, Default)]
pub(crate) struct NetHandlers {
    by_kind: HashMap<SessionEventKind, Vec<Handler>>,
}

/// The kinds the dispatch match still owns **and** peeled subsystems listen to — a session end,
/// which every window that dies with the socket answers for itself. Cloned to the table after the
/// match has run. Empty once the session family itself is peeled.
pub(crate) const BROADCAST: &[SessionEventKind] = &[SessionEventKind::Disconnected];

impl NetHandlers {
    /// Is there at least one handler for this kind?
    pub(crate) fn handles(&self, kind: SessionEventKind) -> bool {
        self.by_kind.contains_key(&kind)
    }

    /// Every kind with a handler, with the handlers' names in registration order — the owner
    /// test's view of the table.
    #[cfg(test)]
    pub(crate) fn census(&self) -> std::collections::BTreeMap<SessionEventKind, Vec<&'static str>> {
        self.by_kind
            .iter()
            .map(|(k, v)| (*k, v.iter().map(|(_, n)| *n).collect()))
            .collect()
    }

    fn push(&mut self, kind: SessionEventKind, handler: Handler) {
        self.by_kind.entry(kind).or_default().push(handler);
    }

    /// Run every handler registered for the event's kind, in registration order, each with the
    /// event (cloned for all but the last).
    fn run(&self, world: &mut World, ev: SessionEvent) {
        let Some(list) = self.by_kind.get(&SessionEventKind::from(&ev)) else {
            return;
        };
        let Some(((last_id, last_name), rest)) = list.split_last() else {
            return;
        };
        for (id, name) in rest {
            call(world, *id, name, ev.clone());
        }
        call(world, *last_id, last_name, ev);
    }
}

fn call(world: &mut World, id: SystemId<In<SessionEvent>>, name: &str, ev: SessionEvent) {
    if let Err(e) = world.run_system_with(id, ev) {
        // A registered handler that cannot run is a bug in the registering plugin (a missing
        // resource, most likely), never the wire's fault — loud, so the smoke gate sees it.
        error!("net: handler `{name}` did not run: {e}");
    }
}

/// Registering a packet handler from a plugin: `app.net_handler(SessionEventKind::X, on_x)`,
/// where `on_x` is an ordinary system taking `In<SessionEvent>` and whatever it needs. Several
/// handlers may answer one kind; they run in registration order.
pub(crate) trait NetHandlerApp {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self;
}

impl NetHandlerApp for App {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self {
        let name = std::any::type_name_of_val(&handler);
        let id = self.world_mut().register_system(handler);
        self.world_mut()
            .get_resource_or_init::<NetHandlers>()
            .push(kind, (id, name));
        self
    }
}

/// One drain's dispatch: the events the table does not own go through `unclaimed` first — the
/// dispatch match, in packet order — then the owned ones run through the table, in packet order.
/// A [`BROADCAST`] kind reaches both.
pub(crate) fn dispatch(
    world: &mut World,
    events: Vec<SessionEvent>,
    unclaimed: impl FnOnce(&mut World, Vec<SessionEvent>),
) {
    let handlers = world.remove_resource::<NetHandlers>().unwrap_or_default();
    let mut to_match = Vec::with_capacity(events.len());
    let mut to_table = Vec::new();
    for ev in events {
        let kind = SessionEventKind::from(&ev);
        if !handlers.handles(kind) {
            to_match.push(ev);
        } else if BROADCAST.contains(&kind) {
            to_table.push(ev.clone());
            to_match.push(ev);
        } else {
            to_table.push(ev);
        }
    }
    unclaimed(world, to_match);
    for ev in to_table {
        handlers.run(world, ev);
    }
    world.insert_resource(handlers);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What ran, in order — a handler's name and the event it saw.
    #[derive(Resource, Default)]
    struct Log(Vec<String>);

    fn on_queued(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        let SessionEvent::LoginQueued { position, .. } = ev else {
            panic!("the table routes by kind");
        };
        log.0.push(format!("queued:{}", position.unwrap_or(0)));
    }

    fn on_logged_out(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        assert!(matches!(ev, SessionEvent::LoggedOut));
        log.0.push("logged_out".into());
    }

    fn on_logged_out_too(In(_): In<SessionEvent>, mut log: ResMut<Log>) {
        log.0.push("logged_out_too".into());
    }

    fn on_disconnected(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        let SessionEvent::Disconnected { reason, .. } = ev else {
            panic!("the table routes by kind");
        };
        log.0.push(format!("disconnected:{reason}"));
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Log>();
        app
    }

    fn queued(position: u32) -> SessionEvent {
        SessionEvent::LoginQueued {
            position: Some(position),
            realm: None,
        }
    }

    fn run(app: &mut App, events: Vec<SessionEvent>) -> Vec<String> {
        dispatch(app.world_mut(), events, |world, unclaimed| {
            let mut log = world.resource_mut::<Log>();
            for ev in unclaimed {
                log.0
                    .push(format!("match:{:?}", SessionEventKind::from(&ev)));
            }
        });
        std::mem::take(&mut app.world_mut().resource_mut::<Log>().0)
    }

    #[test]
    fn owned_kinds_run_in_packet_order_after_the_match_has_run_the_rest() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoginQueued, on_queued)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out);
        let log = run(
            &mut app,
            vec![
                queued(1),
                SessionEvent::LoginStage {
                    stage: benilla_protocol::LoginStage::Connecting,
                },
                SessionEvent::LoggedOut,
                queued(2),
            ],
        );
        assert_eq!(
            log,
            vec!["match:LoginStage", "queued:1", "logged_out", "queued:2"]
        );
    }

    #[test]
    fn several_handlers_on_one_kind_run_in_registration_order_each_with_the_event() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out_too);
        let log = run(&mut app, vec![SessionEvent::LoggedOut]);
        assert_eq!(log, vec!["logged_out", "logged_out_too"]);
        assert_eq!(
            app.world().resource::<NetHandlers>().census()[&SessionEventKind::LoggedOut].len(),
            2
        );
    }

    #[test]
    fn a_broadcast_kind_reaches_the_match_and_the_table() {
        let mut app = app();
        app.net_handler(SessionEventKind::Disconnected, on_disconnected);
        let log = run(
            &mut app,
            vec![SessionEvent::Disconnected {
                reason: "socket".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert_eq!(log, vec!["match:Disconnected", "disconnected:socket"]);
    }

    #[test]
    fn with_no_table_at_all_everything_goes_to_the_match() {
        let mut app = app();
        let log = run(&mut app, vec![SessionEvent::LoggedOut, queued(0)]);
        assert_eq!(log, vec!["match:LoggedOut", "match:LoginQueued"]);
        assert!(app.world().resource::<NetHandlers>().census().is_empty());
    }

    /// **Every kind has exactly one owner** — the dispatch match in `net/apply.rs` or the table,
    /// read off the built client — and the [`BROADCAST`] rows are the only kinds in both. A kind
    /// neither owns is a packet the client decodes and then drops on the floor.
    #[test]
    fn every_session_event_kind_has_one_owner() {
        use std::collections::BTreeSet;
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/apply.rs"),
        )
        .expect("the drain's source");
        let stripped = regex_lite_strip_comments(&source);
        let by_name: HashMap<String, SessionEventKind> = SessionEventKind::all()
            .map(|k| (format!("{k:?}"), k))
            .collect();
        let mut in_match: BTreeSet<SessionEventKind> = BTreeSet::new();
        for token in stripped.split("SessionEvent::").skip(1) {
            let name: String = token
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if let Some(k) = by_name.get(&name) {
                in_match.insert(*k);
            }
        }
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let table = app.world_mut().resource::<NetHandlers>().census();
        let in_table: BTreeSet<SessionEventKind> = table.keys().copied().collect();
        let mut problems = Vec::new();
        for kind in SessionEventKind::all() {
            let m = in_match.contains(&kind);
            let t = in_table.contains(&kind);
            let b = BROADCAST.contains(&kind);
            match (m, t, b) {
                (true, true, true) | (true, false, false) | (false, true, false) => {}
                (true, true, false) => problems.push(format!(
                    "{kind:?}: owned by the match AND handled by {:?} — a peeled kind leaves the match (or is a BROADCAST row)",
                    table[&kind]
                )),
                (true, false, true) => problems.push(format!(
                    "{kind:?}: a BROADCAST row nobody listens to — drop the row"
                )),
                (false, true, true) => problems.push(format!(
                    "{kind:?}: a BROADCAST row the match no longer owns — drop the row"
                )),
                (false, false, _) => problems.push(format!(
                    "{kind:?}: decoded and dropped — no arm in the match, no handler in the table"
                )),
            }
        }
        eprintln!(
            "session event kinds: {} — {} in the dispatch match, {} in the handler table ({} handlers), {} broadcast",
            SessionEventKind::all().count(),
            in_match.len(),
            in_table.len(),
            table.values().map(Vec::len).sum::<usize>(),
            BROADCAST.len()
        );
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// `//` comments out, so a variant named in prose does not count as an arm.
    fn regex_lite_strip_comments(s: &str) -> String {
        s.lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
