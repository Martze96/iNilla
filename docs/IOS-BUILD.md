# Building and installing iNilla.ipa

## The short version

1. **Actions → inilla-ipa → Run workflow.** About 35 minutes the first time (the Rust build,
   on Linux), about 13 once the cache is warm.
2. On the iPhone, open the run in the GitHub app or Safari, download the **iNilla-ipa** artifact
   (a zip containing `iNilla-<commit>.ipa`; the Files app unzips it with a tap).
3. **SideStore → My Apps → ＋** → pick the `.ipa`. SideStore signs it with your Apple ID.
4. First launch: iNilla asks for the game data — see [DATA.md](DATA.md).

**Releases** publish a GitHub pre-release with `iNilla-<version>.ipa` attached. The release text
comes first (its first line the title), then the build facts (commit, SDK, size, SHA-256), the
install steps and GitHub's generated changelog. Two ways to start one:

- **Actions → inilla-ipa → Run workflow** with `release` (e.g. `v0.2.0`) and `notes` (the release
  text) filled in: GitHub creates the tag on the commit that was built.
- From a checkout, an annotated tag, whose message is the release text:

  ```sh
  git tag -a v0.2.0 -m "Short title" -m "What changed, for players."
  git push origin v0.2.0
  ```

Release files do not count against the Actions storage quota; run artifacts do (the `.ipa` is kept
14 days, the intermediate static library 1 day).

## How the build is split

| step | where | why |
|---|---|---|
| the whole Rust build → `libinilla_ios.a` | Linux (`ios/build-staticlib.sh`) | A static library is an archive, not a link: no Apple linker or SDK is involved. The handful of C files in the graph (Lua, blake3, ring, tracing-oslog's wrapper) compile with the host clang against the open-source Darwin libc headers that Zig distributes (`ios/toolchain/linux-env.sh`, pinned by hash), plus a small `<os/log.h>` shim. |
| link + bundle + sign + zip → `iNilla.ipa` | macOS (`ios/package-ipa.sh`) | The link needs the iPhoneOS SDK's framework stubs. A couple of minutes. |

Why it matters: on a **private** repository GitHub counts macOS minutes **10×** against the plan's
included minutes (2,000 a month on Free, 3,000 on Pro); Linux counts 1×, and every job is rounded
up to a whole minute. A full build on macOS would spend ~300–400 of them. Measured with this split:
a warm `.ipa` build is **~23** (12 Linux + 1 budget + 1 macOS minute ×10), the first, cold one ~46;
a `inilla-check` on push is ~4 (~20 cold).

### The minutes watch

`inilla-ipa` starts with a budget report, and `inilla-budget` runs every Monday; the weekly one
**fails at 60 %** so GitHub emails you. To enable both, create a fine-grained personal access token
with the account permission **Plan → Read-only** and save it as the repository secret
**`BILLING_TOKEN`**. Optional repository variables: `INILLA_ACTIONS_QUOTA` (your plan's included
minutes, default 2000) and `INILLA_BUDGET_WARN` (default 60). At the threshold, making the
repository public (Settings → General → Change visibility) makes the standard runners free.

`inilla-check` (every push) is Linux-only: the unit tests, the desktop type-check and the iOS
type-check — a few minutes with a warm cache.

## Building locally

On any Linux or macOS machine with rustup:

```sh
ios/build-staticlib.sh                         # → target/inilla-ios-out/libinilla_ios.a
INILLA_FEATURES=dev ios/build-staticlib.sh    # with benilla's dev instruments
```

Beside the archive: `native-libs.txt` (the frameworks and system libraries rustc reports for the
link) and `lib/`, the native archives a build script links without bundling them into the
staticlib (`liblua5.1.a`, from `third_party/lua-src`).

and on a Mac with Xcode:

```sh
ios/package-ipa.sh                             # → target/inilla-ios-out/iNilla.ipa
```

The quick iOS compile check (seconds to minutes, no codegen):

```sh
source ios/toolchain/linux-env.sh
cargo check --target aarch64-apple-ios -p inilla-ios
```

## Things the packaging takes care of

- **The UIScene deadline.** Apple has said that from the release after iOS 26, an app linked
  against the newest SDK must adopt the UIScene life cycle or it will not launch. winit 0.30
  (Bevy 0.18) uses the classic app delegate, so `package-ipa.sh` records at most **SDK 26.0** in the
  binary's `LC_BUILD_VERSION` (with `vtool`) even when the runner's Xcode is newer — the same result
  as building with Xcode 26. Override with `INILLA_CLAIM_SDK`.
- **Deployment target** iOS 17.0; built and tested for iOS 26/27 on iPhone 15 Pro and newer.
- **Info.plist**: landscape only, full screen, status bar and home indicator hidden, ProMotion
  (`CADisableMinimumFrameDurationOnPhone`), Files-app sharing for Documents, local-network usage
  text, game-controller and Game Mode keys.
- **Entitlements**: `increased-memory-limit` (a free-account signer drops what the account cannot
  have; the app runs either way) and `get-task-allow`.
- **Signing**: ad-hoc. SideStore, AltStore and LiveContainer re-sign.

## Installing

**SideStore (free Apple ID).** Apps signed with a free account expire after **7 days**; refresh them
in SideStore (it can do it in the background) — the app's data, settings and game files stay.
A free account allows 3 sideloaded apps at once (SideStore counts as one). SideStore's own support
for iOS 27 is being worked on upstream; if SideStore itself will not install, see its GitHub for
the current route (the community `SideInstaller` project installs it on-device).

**LiveContainer** (inside SideStore): import the `.ipa` there to run iNilla without spending an app
slot and to swap in a new build in seconds, without re-signing — the fastest loop for trying
builds. iNilla is a single binary with no extensions, which is what LiveContainer needs. As a
guest it runs under LiveContainer's own `Info.plist` and entitlements, which changes a few things,
all handled:

- **Files**: keep iNilla a *private* app in LiveContainer. Its Documents then shows at Files →
  On My iPhone → LiveContainer → Data → Application → *its folder* → Documents; the setup screen
  names the folder. A *shared* app's folder is not visible in Files — download the data instead.
- **Landscape**: LiveContainer's plist allows portrait; iNilla locks its own view controller to
  landscape.
- **Passwords**: saved in LiveContainer's keychain. If LiveContainer's keychain access is not set
  up, the save fails; iNilla says so and asks for the password at each login.
- **Memory**: no `increased-memory-limit` (LiveContainer's entitlements apply). An 8 GB iPhone
  runs the Balanced preset; if iOS ends the game in crowded places, lower Quality or install
  iNilla as its own app.
- **Relaunch**: after the data import, open LiveContainer and run iNilla again (or add its
  home-screen shortcut in LiveContainer). The logs and About page say `in LiveContainer`.
- **Launching on iOS 27** depends on LiveContainer's own build (the UIScene deadline above applies
  to its binary, not iNilla's).

**Updating**: install the new `.ipa` over the old one (SideStore → My Apps → ＋ again, same bundle
id). Documents, the game data and the Keychain are kept.

## When something is off

- The app opens and closes at once → the session log from the attempt is in
  Files → On My iPhone → iNilla → benilla-config → Diagnostics (`session-*.log`, `crash-*.txt`).
- "Warden" in the login error → the server runs Warden; benilla cannot join it.
- A black screen after login → Settings → Debug → turn on the perf overlay (memory headroom) and
  the remote console, and read the log live ([DEBUGGING.md](DEBUGGING.md)).
