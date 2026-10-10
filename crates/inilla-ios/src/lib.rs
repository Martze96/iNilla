//! **iNilla** — benilla on iOS. This crate is the app's entry point and nothing else.
//!
//! `ios/main.c` is the executable's `main`, and it calls [`inilla_main`]. From there it is the
//! same client the desktop runs: [`benilla_app::inilla_boot`] lays out the iOS container (the
//! state folder in Documents, the game data in Application Support, the session log, a Data folder
//! the player dropped in via Files imported in place), then [`benilla_app::run`] builds the Bevy
//! app, whose winit runner calls `UIApplicationMain` and never returns.

use benilla_app::BuildId;

/// The iOS entry point. Returns only if the app exits through Bevy (a quit from the game menu);
/// iOS itself ends the process from inside `UIApplicationMain`.
#[no_mangle]
pub extern "C" fn inilla_main() -> i32 {
    benilla_app::inilla_boot();
    let exit = benilla_app::run(BuildId {
        version: env!("CARGO_PKG_VERSION"),
        describe: env!("BENILLA_GIT_DESCRIBE"),
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
        // The phone carries no source folder: its state lives in the app container.
        project_dir: "",
        ..Default::default()
    });
    match exit {
        benilla_app::AppExit::Success => 0,
        benilla_app::AppExit::Error(code) => i32::from(code.get()),
    }
}
