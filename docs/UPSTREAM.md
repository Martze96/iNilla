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
| `crates/benilla-app/src/bindings.rs` | the synthetic seam: `Held::Synth` and `BindingLatches::synth_hold`/`synth_release`/`synth_release_all` |
| `crates/benilla-app/src/textinput/clipboard.rs` | the iOS pasteboard backend |
| `crates/benilla-app/src/net/io.rs` | the world address passes through `inilla::world_address` (the realm override) |
| `crates/benilla-app/src/player/camera.rs` | `LookConfig::yaw_rate`/`pitch_rate` `pub(crate)` (controller look in radians) |
| `crates/benilla-app/src/debug_panel/mod.rs` | the egui lane's demand gate also reads `inilla::EguiWanted` |
| `crates/benilla-ui/src/script/mod.rs`, `script/plain.rs` | `eval_plain` (wenilla's) — Lua results as plain values for the console and the HUD icons |
| `crates/benilla-app/src/addon_harness/render_tests.rs`, `use_probe_tests.rs` | the two tests whose fixtures are symlinks are `cfg(unix)`, so the app's tests build on Windows |

What iNilla relies on upstream for, with no carry:

- **Script calls need the call host lent** (`crate::script_calls`): every iNilla system that runs
  Lua which may make a call (`inilla::synth`, `inilla::session`) takes `HostedVm` or `VmWith` and
  runs inside it, or the call is refused with "no call host is lent". `synth::apply` runs inside
  the binding pass (`BindingHead` → `BindingTail`), so its commands' edges are that frame's.
- **The login is the stock screen's**: the launcher calls `DefaultServerLogin` in the glue VM
  (`inilla::session`), so the glue manager runs the logon, the realm connect and every dialog.
- **The client's language** comes from `Wow.ini` (`benilla_formats::Locale`): the DBC columns,
  the logon's locale, `GetLocale()` and `SetEuropeanNumbers` are upstream's.

Upstream's CI (`ci.yml`) runs only for a fork's pull request and never builds iOS, so an upstream
change can break the iOS build unseen; the `inilla-check` workflow's iOS type-check is what
catches it.
