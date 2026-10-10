//! **Servers and accounts**: what the launcher remembers.
//!
//! A [`ServerProfile`] is everything needed to reach one realm: the auth server (realmd,
//! `host:3724`), an optional **realm address override** and an optional data source. The
//! override exists because a realmd hands out the world server's address from its `realmlist`
//! table, and that address is only right from one side of a NAT: a server whose realmlist says
//! `192.168.1.10` is unreachable from the internet, one that says a public IP or hostname is often
//! unreachable from inside the LAN (no hairpin NAT). With the override the client keeps the
//! realm's port and dials the host the player named, from wherever they are.
//!
//! An [`Account`] is a username on a profile. Passwords never live here: they go to the platform
//! keychain under the account's id (`inilla-platform::keychain`).

use serde::{Deserialize, Serialize};

pub const DEFAULT_AUTH_PORT: u16 = 3724;
pub const DEFAULT_WORLD_PORT: u16 = 8085;

/// Which emulator the server runs. Both speak the same 1.12.1 protocol; the choice only picks the
/// defaults that differ (and is shown to the player).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Core {
    #[default]
    Vmangos,
    Cmangos,
    Other,
}

impl Core {
    pub fn label(self) -> &'static str {
        match self {
            Core::Vmangos => "vmangos",
            Core::Cmangos => "cMaNGOS",
            Core::Other => "other 1.12.1",
        }
    }
}

/// Where the game data comes from for this server, when it offers it.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DataSource {
    /// Base URL of a `inilla-datahost` (or any server that serves its manifest), e.g.
    /// `https://wow.example.com/data/`.
    pub url: String,
    /// Access token, sent as `Authorization: Bearer …`. Kept in the keychain when saved from the
    /// app; a value here is honoured for hand-written files.
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    /// The auth server's host or IP (the `set realmlist` value).
    pub host: String,
    #[serde(default = "default_auth_port")]
    pub port: u16,
    /// Dial this host for the world server instead of the address the realm list names (the
    /// realm's port is kept unless this carries its own `:port`).
    #[serde(default)]
    pub realm_override: Option<String>,
    /// Pick this realm automatically after login, when the list offers it.
    #[serde(default)]
    pub realm_name: Option<String>,
    #[serde(default)]
    pub core: Core,
    #[serde(default)]
    pub data: Option<DataSource>,
    /// Free text for the player (who runs it, the website, …).
    #[serde(default)]
    pub notes: String,
}

fn default_auth_port() -> u16 {
    DEFAULT_AUTH_PORT
}

impl ServerProfile {
    pub fn new(name: &str, host: &str) -> Self {
        Self {
            id: new_id(name),
            name: name.to_string(),
            host: host.trim().to_string(),
            port: DEFAULT_AUTH_PORT,
            realm_override: None,
            realm_name: None,
            core: Core::default(),
            data: None,
            notes: String::new(),
        }
    }

    /// The realmlist string benilla dials: `host` or `host:port` when the port is not the stock one.
    pub fn realmlist(&self) -> String {
        if self.port == DEFAULT_AUTH_PORT {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Whether the auth host is on a private network — the launcher says "local" for these.
    pub fn is_local(&self) -> bool {
        is_private_host(&self.host)
    }

    /// Apply the realm override to the address a realm list handed out (`host:port`).
    pub fn world_address(&self, listed: &str) -> String {
        let Some(over) = self
            .realm_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return listed.to_string();
        };
        if split_host_port(over).1.is_some() {
            return over.to_string();
        }
        let port = split_host_port(listed).1.unwrap_or(DEFAULT_WORLD_PORT);
        format!("{over}:{port}")
    }

    /// Problems worth showing before the player presses Connect.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.name.trim().is_empty() {
            out.push("The server needs a name.".into());
        }
        let host = self.host.trim();
        if host.is_empty() {
            out.push("Enter the server's address (the realmlist).".into());
        } else if host.contains("://") || host.contains('/') {
            out.push("The address is a host name or IP, without http:// or a path.".into());
        } else if host.contains(' ') {
            out.push("The address cannot contain spaces.".into());
        }
        if self.port == 0 {
            out.push("The port must be between 1 and 65535.".into());
        }
        if let Some(d) = &self.data {
            let u = d.url.trim();
            if !(u.is_empty() || u.starts_with("http://") || u.starts_with("https://")) {
                out.push("The data URL must start with http:// or https://.".into());
            }
        }
        out
    }
}

/// `host[:port]` → `(host, port)`. IPv6 literals are `[addr]:port`.
pub fn split_host_port(s: &str) -> (&str, Option<u16>) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('[') {
        if let Some((addr, tail)) = rest.split_once(']') {
            return (addr, tail.strip_prefix(':').and_then(|p| p.parse().ok()));
        }
    }
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => match p.parse() {
            Ok(port) => (h, Some(port)),
            Err(_) => (s, None),
        },
        _ => (s, None),
    }
}

/// RFC 1918 / loopback / link-local / `.local` names.
pub fn is_private_host(host: &str) -> bool {
    let (h, _) = split_host_port(host);
    let h = h.to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".local") || h.ends_with(".lan") || h.ends_with(".home") {
        return true;
    }
    let parts: Vec<u8> = h.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 4 || h.split('.').count() != 4 {
        return h == "::1" || h.starts_with("fe80:") || h.starts_with("fd");
    }
    matches!(
        (parts[0], parts[1]),
        (10, _) | (127, _) | (192, 168) | (169, 254) | (100, 64..=127)
    ) || (parts[0] == 172 && (16..=31).contains(&parts[1]))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub profile: String,
    pub username: String,
    /// The password is in the keychain under [`Account::id`].
    #[serde(default)]
    pub remember_password: bool,
    /// Log straight in when this account is picked (needs a remembered password).
    #[serde(default)]
    pub auto_login: bool,
    /// The character to enter the world with, when the roster has it.
    #[serde(default)]
    pub character: Option<String>,
}

impl Account {
    pub fn new(profile: &str, username: &str) -> Self {
        Self {
            id: new_id(&format!("{profile}-{username}")),
            profile: profile.to_string(),
            // 1.12 account names are case-insensitive and upper-cased on the wire.
            username: username.trim().to_string(),
            remember_password: true,
            auto_login: false,
            character: None,
        }
    }

    /// The keychain key for this account's password.
    pub fn secret_key(&self) -> String {
        format!("account:{}", self.id)
    }
}

/// Everything the launcher persists (`inilla/servers.toml`).
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Launcher {
    pub servers: Vec<ServerProfile>,
    pub accounts: Vec<Account>,
    pub last_server: Option<String>,
    pub last_account: Option<String>,
}

impl Launcher {
    pub fn server(&self, id: &str) -> Option<&ServerProfile> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn server_mut(&mut self, id: &str) -> Option<&mut ServerProfile> {
        self.servers.iter_mut().find(|s| s.id == id)
    }

    pub fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn accounts_for<'a>(&'a self, server: &'a str) -> impl Iterator<Item = &'a Account> {
        self.accounts.iter().filter(move |a| a.profile == server)
    }

    pub fn current_server(&self) -> Option<&ServerProfile> {
        self.last_server
            .as_deref()
            .and_then(|id| self.server(id))
            .or_else(|| self.servers.first())
    }

    pub fn current_account(&self) -> Option<&Account> {
        let server = self.current_server()?;
        self.last_account
            .as_deref()
            .and_then(|id| self.account(id))
            .filter(|a| a.profile == server.id)
            .or_else(|| self.accounts_for(&server.id).next())
    }

    /// Add a server, keeping ids unique.
    pub fn add_server(&mut self, mut s: ServerProfile) -> String {
        while self.server(&s.id).is_some() {
            s.id = format!("{}-{}", s.id, self.servers.len() + 1);
        }
        let id = s.id.clone();
        self.servers.push(s);
        id
    }

    /// Add an account, or return the existing one with the same name on the same server.
    pub fn add_account(&mut self, a: Account) -> String {
        if let Some(existing) = self
            .accounts
            .iter()
            .find(|x| x.profile == a.profile && x.username.eq_ignore_ascii_case(&a.username))
        {
            return existing.id.clone();
        }
        let mut a = a;
        while self.account(&a.id).is_some() {
            a.id = format!("{}-{}", a.id, self.accounts.len() + 1);
        }
        let id = a.id.clone();
        self.accounts.push(a);
        id
    }

    /// Remove a server and its accounts; returns the removed account ids (their keychain entries
    /// are the caller's to delete).
    pub fn remove_server(&mut self, id: &str) -> Vec<String> {
        self.servers.retain(|s| s.id != id);
        let gone: Vec<String> = self
            .accounts
            .iter()
            .filter(|a| a.profile == id)
            .map(|a| a.id.clone())
            .collect();
        self.accounts.retain(|a| a.profile != id);
        if self.last_server.as_deref() == Some(id) {
            self.last_server = None;
            self.last_account = None;
        }
        gone
    }

    pub fn remove_account(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.last_account.as_deref() == Some(id) {
            self.last_account = None;
        }
    }
}

/// A readable, stable-enough id: the slug of `seed` plus a clock-derived suffix.
pub fn new_id(seed: &str) -> String {
    let slug: String = seed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or(0);
    let slug = if slug.is_empty() {
        "x".to_string()
    } else {
        slug
    };
    format!("{}-{:06x}", &slug[..slug.len().min(24)], nanos & 0xff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_override_keeps_the_realm_port_unless_it_names_one() {
        let mut p = ServerProfile::new("home", "192.168.1.10");
        assert_eq!(p.world_address("10.0.0.5:8085"), "10.0.0.5:8085");
        p.realm_override = Some("play.example.com".into());
        assert_eq!(p.world_address("10.0.0.5:8086"), "play.example.com:8086");
        p.realm_override = Some("play.example.com:9000".into());
        assert_eq!(p.world_address("10.0.0.5:8086"), "play.example.com:9000");
        p.realm_override = Some("  ".into());
        assert_eq!(p.world_address("10.0.0.5:8086"), "10.0.0.5:8086");
    }

    #[test]
    fn realmlist_carries_the_port_only_when_it_is_not_stock() {
        let mut p = ServerProfile::new("x", "wow.example.com");
        assert_eq!(p.realmlist(), "wow.example.com");
        p.port = 5000;
        assert_eq!(p.realmlist(), "wow.example.com:5000");
    }

    #[test]
    fn private_hosts_are_recognised() {
        for h in [
            "192.168.0.2",
            "10.1.2.3",
            "172.20.0.1",
            "127.0.0.1",
            "nas.local",
            "localhost",
        ] {
            assert!(is_private_host(h), "{h}");
        }
        for h in ["8.8.8.8", "172.32.0.1", "wow.example.com", "172.15.0.1"] {
            assert!(!is_private_host(h), "{h}");
        }
    }

    #[test]
    fn host_port_split_handles_ipv6() {
        assert_eq!(split_host_port("a.b:12"), ("a.b", Some(12)));
        assert_eq!(split_host_port("a.b"), ("a.b", None));
        assert_eq!(split_host_port("[::1]:3724"), ("::1", Some(3724)));
        assert_eq!(split_host_port("::1"), ("::1", None));
    }

    #[test]
    fn validation_catches_urls_in_the_host_field() {
        let p = ServerProfile::new("x", "http://wow.example.com/");
        assert!(!p.validate().is_empty());
        assert!(ServerProfile::new("x", "wow.example.com")
            .validate()
            .is_empty());
    }

    #[test]
    fn accounts_are_unique_per_server_and_name() {
        let mut l = Launcher::default();
        let s = l.add_server(ServerProfile::new("home", "192.168.1.2"));
        let a = l.add_account(Account::new(&s, "Alice"));
        let b = l.add_account(Account::new(&s, "alice"));
        assert_eq!(a, b);
        assert_eq!(l.accounts.len(), 1);
        let gone = l.remove_server(&s);
        assert_eq!(gone, vec![a]);
        assert!(l.accounts.is_empty());
    }

    #[test]
    fn the_launcher_round_trips_through_toml() {
        let mut l = Launcher::default();
        let s = l.add_server(ServerProfile::new("home", "192.168.1.2"));
        l.add_account(Account::new(&s, "alice"));
        l.last_server = Some(s);
        let back: Launcher = toml::from_str(&toml::to_string(&l).unwrap()).unwrap();
        assert_eq!(l, back);
    }
}
