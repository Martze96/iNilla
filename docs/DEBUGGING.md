# Debugging iNilla — the fast loop without a Mac

Four tools, from "no computer at all" to "change code and see it in minutes".

## 1. On the phone: the session log

Every launch writes `Documents/benilla-config/Diagnostics/session-<UTC time>.log` — everything
the client prints (the build banner, the preflight, the network, every warning), flushed line by
line, so a crash leaves everything up to its last words. The last 10 sessions are kept. A panic
also leaves benilla's `crash-<unix>.txt` beside them, with the payload, a backtrace and the last
few hundred log lines.

- **Read them on the phone**: Settings → Debug → *View session log* (filter, copy), or the Files app
  → On My iPhone → iNilla → benilla-config → Diagnostics.
- **Get them to a computer**: Files → share, AirDrop; or Finder / Apple Devices file sharing.

The perf overlay (Settings → Debug) shows frame rate, **memory headroom** (what iOS will allow
before it kills the app) and the thermal state — the three things that differ most from desktop.

## 2. On the same Wi-Fi: the remote console

Settings → Debug → **Remote console on the local network**. The screen shows an address like
`http://192.168.1.23:7878/?token=…`; open it in any browser on the same network and you get:

- the **live log** (long-poll, filterable, savable),
- a **Lua prompt** into the running interface — `return UnitName('target'), UnitHealth('target')`,
  `ReloadUI()`, anything FrameXML can do — with the values rendered back,
- **game commands** by binding name (`JUMP`, `TARGETNEARESTENEMY`, `TOGGLEGAMEMENU`) and
  **chat lines / slash commands** (`/who`, `/console fpsJournal 1`, `.gm on`),
- **screenshots** (they land in `benilla-config/Screenshots/` and show up in the file browser),
- the **`benilla-config` files**: download crash reports and logs, **upload** an edited
  `config.toml`, a touch layout (`inilla/touch.toml`), or an addon's files into `AddOns/`.

Every request needs the token; *New token* revokes the old one. Turn the console off when you are
done — anyone with the address can drive the game.

The API, for scripts: `GET /api/log?since=N&wait=1`, `POST /api/lua` (body = chunk),
`POST /api/cmd` (`{"op":"press","cmd":"JUMP"}`, `{"op":"chat","text":"/sit"}`,
`{"op":"screenshot"}`), `GET /api/state`, `GET|PUT|DELETE /api/file?path=…`,
`GET /api/files?path=…`, all with `Authorization: Bearer <token>`.

## 3. On the desktop: rehearse the phone

Most of iNilla is platform-independent, and the desktop build runs the same code:

```sh
# the touch HUD with the mouse as a finger, the phone's automatic UI scale, the frame cap:
INILLA_TOUCH=1 WOW_DATA=/path/to/WoW/Data cargo play
# a gamepad through gilrs, in the WoW-Forever mapping:
INILLA_PAD=1 WOW_DATA=/path/to/WoW/Data cargo play
# the iOS container layout (Documents import, the data download, the session log) in a folder:
INILLA_CONTAINER=/tmp/fakephone cargo play
```

(`cargo play` is benilla's release-with-incremental alias. `WOW_USER`/`WOW_PASS` still log in
without typing.) The engine-free logic has its own tests, which run in a second:

```sh
cargo test -p inilla-core -p inilla-platform
```

— a bug in "what should this finger or button do" is reproduced there as a test first.

## 4. The build loop

1. Change code; `cargo test -p inilla-core` and `cargo check -p benilla-app` locally.
2. `source ios/toolchain/linux-env.sh && cargo check --target aarch64-apple-ios -p inilla-ios`
   catches iOS-only breakage (a `cfg(target_os = "ios")` path) in a minute, on Linux.
3. Push: `inilla-check` repeats both. Run `inilla-ipa` when you want a build on the phone.
4. Install into **LiveContainer** for the quickest swap (no re-sign), or over the old app in
   SideStore. Your settings and data stay.
5. Reproduce with the remote console open; attach the session log and crash report to the issue.
