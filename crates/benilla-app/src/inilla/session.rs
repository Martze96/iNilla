//! **The launcher meets the login.** A server profile becomes benilla's realmlist (and realm name,
//! and the world-address override); a saved account becomes a login attempt through the login
//! screen's own submit path, so the connecting dialog, the queue and every refusal read exactly as
//! for a typed login.
//!
//! The **realm address override** is read by the network thread when it dials the world server
//! (`net::io`, one call to [`world_address`]): the realm list's address is kept unless the active
//! profile names a host to use instead — the LAN/public NAT fix a phone that moves between Wi-Fi
//! and cellular needs.

use std::sync::RwLock;

use bevy::prelude::*;

use inilla_core::profiles::{Account, ServerProfile};

use super::Inilla;
use crate::char_select::ClientState;
use crate::cvars::Cvars;
use crate::glue::dialog::GlueDialog;
use crate::glue_strings::GlueStrings;
use crate::login::Attempt;

/// The active profile's override, for the network thread.
static WORLD_OVERRIDE: RwLock<Option<ServerProfile>> = RwLock::new(None);

/// The world address to dial for a realm the list advertised at `listed`.
pub(crate) fn world_address(listed: String) -> String {
    let guard = WORLD_OVERRIDE.read().ok();
    match guard.as_ref().and_then(|g| g.as_ref()) {
        Some(p) => {
            let addr = p.world_address(&listed);
            if addr != listed {
                bevy::log::info!("inilla: realm address {listed} → {addr} (profile override)");
            }
            addr
        }
        None => listed,
    }
}

/// A login the launcher asked for, waiting for the login screen.
#[derive(Resource, Default)]
pub(crate) struct LoginQueue {
    pending: Option<(String, String)>,
    /// The profile whose realmlist was last applied.
    applied: Option<String>,
}

impl LoginQueue {
    pub(crate) fn request(&mut self, user: &str, pass: &str) {
        self.pending = Some((user.to_string(), pass.to_string()));
    }
}

/// Make `server` the active one: realmlist, realm name, override.
pub(crate) fn activate(server: &ServerProfile, cvars: &mut Cvars) {
    cvars.set(crate::realmlist::CVAR_REALMLIST, &server.realmlist());
    if let Some(realm) = server
        .realm_name
        .as_deref()
        .filter(|r| !r.trim().is_empty())
    {
        cvars.set("realmName", realm.trim());
    }
    if let Ok(mut g) = WORLD_OVERRIDE.write() {
        *g = Some(server.clone());
    }
    info!(
        "inilla: server {:?} — realmlist {}{}",
        server.name,
        server.realmlist(),
        server
            .realm_override
            .as_deref()
            .filter(|o| !o.trim().is_empty())
            .map(|o| format!(", world host {o}"))
            .unwrap_or_default()
    );
}

/// The password saved for `account`, if the player chose to remember it.
pub(crate) fn saved_password(account: &Account) -> Option<String> {
    inilla_platform::keychain::get(&account.secret_key())
}

pub(crate) struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoginQueue>()
            .add_systems(Update, (apply_active_server, submit_queued_login).chain());
    }
}

/// Keep benilla's realmlist on the launcher's current server.
fn apply_active_server(
    state: Res<Inilla>,
    mut queue: ResMut<LoginQueue>,
    mut cvars: ResMut<Cvars>,
) {
    let Some(server) = state.launcher.current_server() else {
        return;
    };
    let fingerprint = format!(
        "{}|{}|{:?}|{:?}",
        server.id,
        server.realmlist(),
        server.realm_override,
        server.realm_name
    );
    if queue.applied.as_deref() == Some(fingerprint.as_str()) {
        return;
    }
    activate(server, &mut cvars);
    queue.applied = Some(fingerprint);
}

/// Hand a queued login to the login screen's submit path once the screen is up and idle.
fn submit_queued_login(
    state: Res<State<ClientState>>,
    mut queue: ResMut<LoginQueue>,
    mut attempt: Attempt,
    mut dialog: ResMut<GlueDialog>,
    strings: Option<Res<GlueStrings>>,
) {
    if *state.get() != ClientState::Login || attempt.in_flight() {
        return;
    }
    let Some((user, pass)) = queue.pending.take() else {
        return;
    };
    let text = strings
        .as_deref()
        .map(|s| s.text("LOGIN_STATE_CONNECTING", "Connecting").to_string())
        .unwrap_or_else(|| "Connecting".into());
    dialog.open_status(&text);
    info!("inilla: logging in as {user}");
    attempt.send(&user, &pass, true);
}
