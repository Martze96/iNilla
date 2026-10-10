//! **The launcher**: pick a server, pick an account, play. Servers carry the realmlist, the
//! optional realm-address override and an optional data source; accounts carry a username and a
//! choice of whether the password lives in the Keychain. Logging in goes through the stock login
//! screen's own submit (`session::LoginQueue`), so its dialogs, queue and errors are the real ones.
//!
//! There is no registration here on purpose: 1.12's protocol has no way to create an account, and
//! a server's own sign-up (its website, or `account create` on its console) is the place for it.

use bevy_egui::egui;

use inilla_core::profiles::{is_private_host, Account, Core, DataSource, ServerProfile};

use super::{hint, row, Overlay, Panel};
use crate::inilla::data::DataState;
use crate::inilla::hud::GOLD;
use crate::inilla::session::{self, LoginQueue};
use crate::inilla::Inilla;

/// The add/edit server form.
#[derive(Clone, Debug, Default)]
pub(crate) struct ServerForm {
    editing: Option<String>,
    name: String,
    host: String,
    port: String,
    realm_override: String,
    realm_name: String,
    core: Core,
    data_url: String,
    notes: String,
    errors: Vec<String>,
}

impl ServerForm {
    fn new() -> Self {
        Self {
            port: "3724".into(),
            ..Default::default()
        }
    }

    fn from(s: &ServerProfile) -> Self {
        Self {
            editing: Some(s.id.clone()),
            name: s.name.clone(),
            host: s.host.clone(),
            port: s.port.to_string(),
            realm_override: s.realm_override.clone().unwrap_or_default(),
            realm_name: s.realm_name.clone().unwrap_or_default(),
            core: s.core,
            data_url: s.data.as_ref().map(|d| d.url.clone()).unwrap_or_default(),
            notes: s.notes.clone(),
            errors: Vec::new(),
        }
    }

    fn build(&self) -> Result<ServerProfile, Vec<String>> {
        let mut p = ServerProfile::new(self.name.trim(), self.host.trim());
        if let Some(id) = &self.editing {
            p.id = id.clone();
        }
        p.port = match self.port.trim().parse::<u16>() {
            Ok(v) if v > 0 => v,
            _ => return Err(vec!["The port must be a number between 1 and 65535.".into()]),
        };
        let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        p.realm_override = opt(&self.realm_override);
        p.realm_name = opt(&self.realm_name);
        p.core = self.core;
        p.data = opt(&self.data_url).map(|url| DataSource { url, token: None });
        p.notes = self.notes.trim().to_string();
        let errs = p.validate();
        if errs.is_empty() {
            Ok(p)
        } else {
            Err(errs)
        }
    }
}

/// The add/edit account form.
#[derive(Clone, Debug, Default)]
pub(crate) struct AccountForm {
    editing: Option<String>,
    server: String,
    username: String,
    password: String,
    remember: bool,
    auto_login: bool,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    overlay: &mut Overlay,
    inilla: &mut Inilla,
    queue: &mut LoginQueue,
    data: &DataState,
) {
    if let Some(form) = overlay.server_form.as_mut() {
        if server_form(ui, form, inilla) {
            overlay.server_form = None;
        }
        return;
    }
    if let Some(form) = overlay.account_form.as_mut() {
        let mut refused = false;
        if account_form(ui, form, inilla, &mut refused) {
            overlay.account_form = None;
        }
        if refused {
            overlay.toast(NOT_SAVED);
        }
        return;
    }
    if !data.ready_at_boot {
        ui.colored_label(egui::Color32::YELLOW, "The game data is not installed yet.");
        if ui.button("Set up game data").clicked() {
            overlay.panel = Some(Panel::Setup);
        }
        ui.separator();
    }
    if inilla.launcher.servers.is_empty() {
        ui.label("Add the server you play on — the address you would put in realmlist.wtf.");
        hint(
            ui,
            "Any 1.12.1 core works (vmangos, cMaNGOS). The server must have Warden turned off.",
        );
        if ui.button("+ Add server").clicked() {
            overlay.server_form = Some(ServerForm::new());
        }
        return;
    }

    let selected = overlay
        .selected_server
        .clone()
        .filter(|id| inilla.launcher.server(id).is_some())
        .or_else(|| inilla.launcher.current_server().map(|s| s.id.clone()));
    overlay.selected_server = selected.clone();

    ui.columns(2, |cols| {
        // ── servers ──
        let ui = &mut cols[0];
        ui.label(egui::RichText::new("Servers").strong());
        egui::ScrollArea::vertical()
            .id_salt("servers")
            .max_height(ui.available_height() - 60.0)
            .show(ui, |ui| {
                for s in inilla.launcher.servers.clone() {
                    let is_sel = selected.as_deref() == Some(s.id.as_str());
                    let badge = if s.is_local() { "LAN" } else { "Internet" };
                    let text = format!("{}   {}  ·  {}", s.name, s.realmlist(), badge);
                    if ui
                        .add_sized(
                            [ui.available_width(), 40.0],
                            egui::Button::selectable(is_sel, text),
                        )
                        .clicked()
                    {
                        overlay.selected_server = Some(s.id.clone());
                        inilla.launcher.last_server = Some(s.id.clone());
                        inilla.launcher.last_account = None;
                    }
                }
            });
        ui.horizontal(|ui| {
            if ui.button("+ Add server").clicked() {
                overlay.server_form = Some(ServerForm::new());
            }
            if let Some(s) = selected
                .as_deref()
                .and_then(|id| inilla.launcher.server(id))
            {
                if ui.button("Edit").clicked() {
                    overlay.server_form = Some(ServerForm::from(s));
                }
            }
        });

        // ── accounts on the selected server ──
        let ui = &mut cols[1];
        let Some(server) = selected
            .as_deref()
            .and_then(|id| inilla.launcher.server(id))
            .cloned()
        else {
            return;
        };
        ui.label(egui::RichText::new(format!("Accounts on {}", server.name)).strong());
        if let Some(o) = server.realm_override.as_deref().filter(|o| !o.is_empty()) {
            hint(
                ui,
                &format!("World server reached at {o} (realm address override)."),
            );
        }
        let accounts: Vec<Account> = inilla.launcher.accounts_for(&server.id).cloned().collect();
        if accounts.is_empty() {
            hint(
                ui,
                "No accounts yet. Add the account name and password you use on this server.",
            );
        }
        for a in accounts {
            ui.horizontal(|ui| {
                // The Keychain is asked on the click, not every frame.
                let saved = a.remember_password;
                ui.add_sized(
                    [180.0, 40.0],
                    egui::Label::new(egui::RichText::new(&a.username).size(18.0)),
                );
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("▶ Log in").color(GOLD))
                            .min_size(egui::vec2(110.0, 40.0)),
                    )
                    .clicked()
                {
                    inilla.launcher.last_server = Some(server.id.clone());
                    inilla.launcher.last_account = Some(a.id.clone());
                    match session::saved_password(&a) {
                        Some(pass) => {
                            queue.request(&a.username, &pass);
                            overlay.panel = None;
                        }
                        None => overlay.password_prompt = Some((a.id.clone(), String::new())),
                    }
                }
                if ui.button("Edit").clicked() {
                    overlay.account_form = Some(AccountForm {
                        editing: Some(a.id.clone()),
                        server: server.id.clone(),
                        username: a.username.clone(),
                        password: String::new(),
                        remember: a.remember_password,
                        auto_login: a.auto_login,
                    });
                }
                if !saved {
                    ui.label(
                        egui::RichText::new("no saved password")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
            });
            // The password prompt for this account.
            if let Some((id, pass)) = overlay.password_prompt.as_mut() {
                if *id == a.id {
                    let mut go = false;
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(pass)
                                .password(true)
                                .hint_text("Password")
                                .desired_width(200.0),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            go = true;
                        }
                        if ui.button("Log in").clicked() {
                            go = true;
                        }
                    });
                    if go && !pass.is_empty() {
                        let saved = !a.remember_password || remember(&a.secret_key(), pass);
                        queue.request(&a.username, pass);
                        overlay.password_prompt = None;
                        overlay.panel = None;
                        if !saved {
                            overlay.toast(NOT_SAVED);
                        }
                    }
                }
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("+ Add account").clicked() {
                overlay.account_form = Some(AccountForm {
                    server: server.id.clone(),
                    remember: true,
                    ..Default::default()
                });
            }
            if ui
                .button("Type it in myself")
                .on_hover_text("Close this and use the login screen")
                .clicked()
            {
                overlay.panel = None;
            }
        });
        if !server.notes.is_empty() {
            ui.add_space(8.0);
            hint(ui, &server.notes);
        }
    });
}

/// Returns `true` when the form is done (saved or cancelled).
fn server_form(ui: &mut egui::Ui, f: &mut ServerForm, inilla: &mut Inilla) -> bool {
    let mut done = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(egui::RichText::new(if f.editing.is_some() { "Edit server" } else { "Add server" }).strong());
        row(ui, "Name", |ui| {
            ui.add(egui::TextEdit::singleline(&mut f.name).hint_text("My vmangos").desired_width(320.0));
        });
        row(ui, "Address (realmlist)", |ui| {
            ui.add(egui::TextEdit::singleline(&mut f.host).hint_text("192.168.1.10 or wow.example.com").desired_width(320.0));
        });
        let private = is_private_host(&f.host);
        if !f.host.trim().is_empty() {
            hint(
                ui,
                if private {
                    "A local-network address: reachable at home. Add a public address below to play from outside."
                } else {
                    "A public address: reachable from anywhere."
                },
            );
        }
        row(ui, "Auth port", |ui| {
            ui.add(egui::TextEdit::singleline(&mut f.port).desired_width(90.0));
        });
        row(ui, "Server software", |ui| {
            for c in [Core::Vmangos, Core::Cmangos, Core::Other] {
                ui.radio_value(&mut f.core, c, c.label());
            }
        });
        ui.collapsing("Advanced", |ui| {
            row(ui, "World server host", |ui| {
                ui.add(egui::TextEdit::singleline(&mut f.realm_override).hint_text("leave empty to use the realm list's").desired_width(320.0));
            });
            hint(
                ui,
                "Use when the realm list hands out an address this phone cannot reach (a LAN IP \
                 while you are outside, or a public IP inside a network without hairpin NAT). \
                 host or host:port.",
            );
            row(ui, "Realm", |ui| {
                ui.add(egui::TextEdit::singleline(&mut f.realm_name).hint_text("pick automatically after login").desired_width(320.0));
            });
            row(ui, "Game data URL", |ui| {
                ui.add(egui::TextEdit::singleline(&mut f.data_url).hint_text("https://…/data/ (inilla-datahost)").desired_width(320.0));
            });
            row(ui, "Notes", |ui| {
                ui.add(egui::TextEdit::multiline(&mut f.notes).desired_rows(2).desired_width(320.0));
            });
        });
        for e in &f.errors {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new("Save").color(GOLD)).min_size(egui::vec2(100.0, 40.0))).clicked() {
                match f.build() {
                    Ok(p) => {
                        if let Some(existing) = f.editing.as_deref().and_then(|id| inilla.launcher.server_mut(id)) {
                            *existing = p;
                        } else {
                            let id = inilla.launcher.add_server(p);
                            inilla.launcher.last_server = Some(id);
                        }
                        done = true;
                    }
                    Err(errs) => f.errors = errs,
                }
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
            if let Some(id) = f.editing.clone() {
                ui.add_space(24.0);
                if ui.button(egui::RichText::new("Delete server").color(egui::Color32::LIGHT_RED)).clicked() {
                    for account in inilla.launcher.remove_server(&id) {
                        inilla_platform::keychain::delete(&format!("account:{account}"));
                    }
                    done = true;
                }
            }
        });
    });
    done
}

fn account_form(
    ui: &mut egui::Ui,
    f: &mut AccountForm,
    inilla: &mut Inilla,
    refused: &mut bool,
) -> bool {
    let mut done = false;
    ui.label(
        egui::RichText::new(if f.editing.is_some() {
            "Edit account"
        } else {
            "Add account"
        })
        .strong(),
    );
    row(ui, "Account name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut f.username).desired_width(260.0));
    });
    row(ui, "Password", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.password)
                .password(true)
                .hint_text(if f.editing.is_some() { "unchanged" } else { "" })
                .desired_width(260.0),
        );
    });
    ui.checkbox(
        &mut f.remember,
        "Remember the password (kept in the iOS Keychain)",
    );
    ui.add_enabled(
        f.remember,
        egui::Checkbox::new(&mut f.auto_login, "Log in automatically when iNilla starts"),
    );
    hint(
        ui,
        "iNilla cannot create accounts: 1.12 has no in-game sign-up. Register on your server's \
         website or with `account create` on its console, then add the account here.",
    );
    ui.horizontal(|ui| {
        let can_save = !f.username.trim().is_empty();
        if ui
            .add_enabled(
                can_save,
                egui::Button::new(egui::RichText::new("Save").color(GOLD))
                    .min_size(egui::vec2(100.0, 40.0)),
            )
            .clicked()
        {
            let id = match f.editing.clone() {
                Some(id) => {
                    if let Some(a) = inilla.launcher.accounts.iter_mut().find(|a| a.id == id) {
                        a.username = f.username.trim().to_string();
                        a.remember_password = f.remember;
                        a.auto_login = f.remember && f.auto_login;
                    }
                    id
                }
                None => {
                    let mut a = Account::new(&f.server, &f.username);
                    a.remember_password = f.remember;
                    a.auto_login = f.remember && f.auto_login;
                    inilla.launcher.add_account(a)
                }
            };
            let key = format!("account:{id}");
            if !f.remember {
                inilla_platform::keychain::delete(&key);
            } else if !f.password.is_empty() {
                *refused = !remember(&key, &f.password);
            }
            if f.auto_login {
                // Only one account logs in by itself.
                for a in inilla.launcher.accounts.iter_mut().filter(|a| a.id != id) {
                    a.auto_login = false;
                }
            }
            inilla.launcher.last_account = Some(id);
            f.password.clear();
            done = true;
        }
        if ui.button("Cancel").clicked() {
            done = true;
        }
        if let Some(id) = f.editing.clone() {
            ui.add_space(24.0);
            if ui
                .button(egui::RichText::new("Delete account").color(egui::Color32::LIGHT_RED))
                .clicked()
            {
                inilla_platform::keychain::delete(&format!("account:{id}"));
                inilla.launcher.remove_account(&id);
                done = true;
            }
        }
    });
    done
}

const NOT_SAVED: &str = "Password not saved: the Keychain refused it";

/// Save a password in the Keychain. A LiveContainer set up without keychain access groups
/// refuses; the account then asks for the password at every login, and the caller says so.
fn remember(key: &str, password: &str) -> bool {
    match inilla_platform::keychain::set(key, password) {
        Ok(()) => true,
        Err(e) => {
            bevy::log::warn!("inilla: keychain: {e}");
            false
        }
    }
}
