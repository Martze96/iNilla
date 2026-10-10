//! **inilla-core** — the engine-free half of iNilla, the iOS build of benilla.
//!
//! Everything here is plain data and plain logic: no Bevy, no UIKit, no network. That is what
//! makes it testable on any machine in a second (`cargo test -p inilla-core`), and it is where a
//! bug in "what should this finger/button do" is fixed without a device.
//!
//! - [`pad`] — controller state, the WoW-Forever-style mapping, the per-frame mapper.
//! - [`touch`] — the touch HUD layout and the gesture engine.
//! - [`profiles`] — servers (realmlist, realm override, data source) and accounts.
//! - [`data`] — the 1.12.1 `Data` folder: status, import, the manifest, resumable download.
//! - [`settings`] — iNilla's own settings (quality preset, UI scale, debug switches).
//! - [`http`] — the tiny HTTP/1.1 server under the data host and the remote console.
//! - [`logring`] — the process-wide log ring the console and the log viewer read.

pub mod addons;
pub mod data;
pub mod http;
pub mod logring;
pub mod pad;
pub mod profiles;
pub mod settings;
pub mod touch;
