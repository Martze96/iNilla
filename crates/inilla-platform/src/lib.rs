//! **inilla-platform** — everything iNilla asks of the operating system, behind plain Rust
//! calls that compile everywhere.
//!
//! On iOS each module talks to its framework (UIKit, GameController, Security, AVFAudio) through
//! `objc2`'s `msg_send!` on classes looked up by name — a handful of well-known selectors, no
//! generated framework crates. Everywhere else the same calls are no-ops or small fallbacks, so
//! the desktop build (the fast place to develop the touch HUD and the controller mapping) runs
//! the same code paths up to this line.
//!
//! Main-thread rule: the UIKit calls (`device`, `pasteboard`, `gamepad`) must be made from the
//! main thread — in Bevy terms, from a system that takes a `NonSend` parameter or runs in the
//! main schedule on iOS, where Bevy's main thread *is* UIKit's.

pub mod audio;
pub mod device;
pub mod gamepad;
pub mod keychain;
pub mod pasteboard;
pub mod paths;
pub mod stdio;

#[cfg(target_os = "ios")]
mod objc;
