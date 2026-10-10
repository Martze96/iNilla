# iNilla and upstream benilla

iNilla is a fork of [samwhosung/benilla](https://github.com/samwhosung/benilla) that **merges**
upstream as it publishes (the way [wenilla](https://github.com/Arnesen/wenilla) does). iNilla's own
code lives in its own crates and in `crates/benilla-app/src/inilla/`; changes inside upstream
files ("carries") are kept few and small so a merge conflicts on as little as possible.

## Syncing

```sh
git remote add upstream https://github.com/samwhosung/benilla   # once
git fetch upstream
git checkout -b sync-upstream-$(date +%Y%m%d)
git merge upstream/main          # a real merge commit — never a rebase or a squashed copy
# resolve the carries below if they conflict, then:
cargo test -p inilla-core && cargo check -p benilla-app
source ios/toolchain/linux-env.sh && cargo check --target aarch64-apple-ios -p inilla-ios
```

Open a PR from the sync branch and merge it **with a merge commit** (keeping upstream as the second
parent is what keeps the next sync small).

## The carries

Every upstream file iNilla changes, and why. Each change is marked `iNilla` in a comment.

| file | change |
|---|---|
| `Cargo.toml` | `[profile.ios]` (release, no LTO, symbols kept) |
| `crates/benilla-app/Cargo.toml` | depends on `inilla-core`, `inilla-platform`, `ureq`, `serde_json`; `bevy_egui` no longer optional; `arboard` not on iOS; the Wayland clipboard's `cfg` excludes iOS; `raw-window-handle` and `winit` on iOS |
| `crates/benilla-app/src/lib.rs` | `mod inilla`, `pub use inilla_boot`, `add_plugins(inilla::InillaPlugin)` |
| `crates/benilla-app/src/bindings.rs`, `bindings/script_input.rs` | the synthetic seam `synth_hold`/`synth_release`/`synth_fire` and `Held::Synth` |
| `crates/benilla-app/src/textinput/clipboard.rs` | the iOS pasteboard backend |
| `crates/benilla-app/src/login/mod.rs` | `Attempt`/`send` `pub(crate)`, `in_flight()` — the launcher submits through the login screen's path |
| `crates/benilla-app/src/net/io.rs` | the world address passes through `inilla::world_address` (the realm override) |
| `crates/benilla-app/src/player/camera.rs` | `LookConfig::yaw_rate`/`pitch_rate` `pub(crate)` (controller look in radians) |
| `crates/benilla-app/src/debug_panel/mod.rs` | the egui lane's demand gate also reads `inilla::EguiWanted` |
| `crates/benilla-ui/src/script/mod.rs`, `script/plain.rs` | `eval_plain` (wenilla's) — Lua results as plain values for the console and the HUD icons |
| `third_party/lua-src/lua-5.1.5/loslib.c`, `BENILLA.md` | `os.execute` answers "not supported" on iOS, where `system()` does not exist |
| `.github/workflows/close-prs.yml` | removed (upstream closes every PR; this repository takes them) |
| `crates/benilla-dbc/src/localize.rs` (new), `lib.rs` | `localize` / `data_locale`: a non-English client's DBC text moved into the English column every reader names |
| `crates/benilla-formats/src/chain.rs`, `lib.rs` | `Chain::open` notes the install's language off `ChrRaces.dbc`; `Chain::read` localizes `.dbc` files of a non-English install; `data_locale()` |
| `crates/benilla-protocol/src/auth.rs` | `set_client_locale`: the logon challenge reports the game data's language instead of a fixed `enUS` |
| `crates/benilla-ui/src/script/client.rs`, `mod.rs`, `stdlib.rs` | `set_locale`: `GetLocale()` answers the game data's language; the stdlib's fixed `enUS` only fills in when nothing registered one |
| `crates/benilla-app/src/addon_harness/render_tests.rs`, `use_probe_tests.rs` | the two tests whose fixtures are symlinks are `cfg(unix)`, so the app's tests build on Windows |
| `crates/benilla-ui/src/script/client.rs`, `script/model.rs`, `strings.rs`, `script/tooltip_item/render.rs` | `SetEuropeanNumbers`, which the German `Localization.lua` calls at `VARIABLES_LOADED`: `strings::decimal` / `fill_with` write the tooltips' reals (speed, DPS, ammo, cast time, cooldown) with a comma |

Upstream has no CI and never builds iOS, so an upstream change can break only the iOS build; the
`inilla-check` workflow's iOS type-check is what catches it.
