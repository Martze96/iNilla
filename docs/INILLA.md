# iNilla

iNilla is benilla on iOS: the same client, the same stock 1.12.1 interface off the player's own
archives, with the pieces a phone needs added around it and nothing inside the game rewritten.

## Where it lives

| path | what |
|---|---|
| `crates/inilla-core` | the engine-free logic: controller mapping and layers, the touch gesture engine and layout model, server profiles and accounts, the data manifest and resumable download, iNilla's settings, a small HTTP server, the log ring. Unit-tested, no Bevy (`cargo test -p inilla-core`). |
| `crates/inilla-platform` | the OS glue behind plain Rust calls: UIKit (pasteboard, idle timer, safe area, haptics), GameController, the Keychain, AVAudioSession, stdout/stderr capture, the container paths. `objc2` message sends on iOS, no-ops or fallbacks elsewhere. |
| `crates/benilla-app/src/inilla/` | the Bevy side: the plugin, the input seams, the touch HUD, the controller, the launcher and settings screens (egui), the data jobs, the session hooks, the remote console. |
| `crates/inilla-ios` | the iOS entry point: a static library exporting `inilla_main`. |
| `ios/` | packaging: `main.c`, `Info.plist.in`, entitlements, icons, the Linux cross toolchain, the build scripts. |
| `.github/workflows/inilla-*.yml` | the gate, the `.ipa` pipeline, the minutes watch. |

## How input reaches the game

Two seams, and nothing else:

- **Commands by name** (`inilla/synth.rs`): a HUD button, a controller button or the console
  presses `JUMP`, `ACTIONBUTTON3`, `TARGETNEARESTENEMY` — the 1.12 `Bindings.xml` names — through
  benilla's `BindingsState`, exactly as a key would (the synthetic seam is wenilla's web-bridge
  design). Held commands latch while held, host commands fire on the press, Lua-bodied commands
  run their body, action buttons run their down body on press and their up body on release.
- **An ordinary mouse** (`inilla/mouse.rs`): taps, long presses, drags, the controller's cursor
  and its right-stick look become `CursorMoved` / `MouseButtonInput` / `MouseMotion` /
  `MouseWheel` before Bevy's input systems run. The stock interface, world click-targeting and
  mouse-look all work unchanged — including the `MOUSE_LOOK_SPEED` slider.

So nothing iNilla does is something a keyboard and mouse player could not do.

## Touch controls

A finger is owned by the first of these that wants it, decided when it lands:

1. **iNilla's own screens** (the launcher, settings, the editor).
2. **A HUD element** — jump, the action ring, the view/radial/menu buttons. A tap acts on the
   release (an action button casts on the key's up in 1.12 too); holding a button shows what it
   does — the game's own tooltip for a spell, the key binding's name in the game's language for
   anything else — and letting go then does nothing; sliding off cancels. Holding **jump** opens
   the hub (below). A button bound to a held command (turn, strafe) holds for as long as the finger.
3. **The stock interface** — any mouse-enabled FrameXML frame under the finger (asked of benilla's
   own hit test). Rest = hover (tooltips), tap = left click, long press = right click, drag = a
   left-button drag (items, spells, sliders), two fingers = the mouse wheel. **Inside the stick
   zone a drag moves** instead — the chat sits there, and a thumb that lands on it on its way to
   the stick walks — while a tap still clicks and a rest-then-lift right-clicks. The chat windows
   are locked (the stock "Lock Window"), so nothing there moves or resizes by accident.
4. **The movement stick zone** (lower left) — the stick appears under the thumb. Sideways it
   strafes (Q/E), or with **Settings → Touch → Left stick sideways: Steer** it turns the character
   and the camera with it (A/D).
5. **The world** — tap = select, double tap or long press = right click (talk, loot, attack),
   drag = look (right-button look: the character turns with the camera; switchable), pinch = zoom.

On the login and character screens every finger is a plain mouse; a keyboard button raises the
iOS keyboard for the login fields (chat raises it by itself when the box takes focus).

**Chat.** While a text box has the focus, a bar at the top of the screen shows the line being
typed (in landscape the iOS keyboard covers the stock edit box) with **Send** and **× Close**:
Close is the Escape key the iOS keyboard does not have. It leaves the box without sending and
takes the keyboard down.

**Buttons are pictures** where the game has an obvious one: the Explore and Social views use the
game's own icons (bags, map, quest log, autorun, sit, chat, friends, follow, …), an action slot or
a button bound to one shows that slot's spell with its cooldown, and jump is an arrow. An action
button carries the hotkey text of the stock button it presses (`1`…`6`), so the HUD's button and the
bar's read as one; it shows what the press casts, stance bar included (a warrior's stances, a
druid's forms). The radial menu shows an icon and its name for each window.

**Views** swap the six buttons around jump: **Combat** (action slots 1–6 with icons and cooldowns),
**Explore** (autorun, map, bags, sit, mount = action 12, quest log), **Social** (chat, reply, wave,
sit, friends, follow), **Minimal** (stick and jump). The view button cycles them, and its picture
says which view is up (crossed swords, a compass, a speech bubble, a closed eye); the ☰ button opens
the **radial menu** — character, bags, spellbook, talents, quest log, map, social, chat,
reputation, skills, pet, game menu — every desktop window one gesture away.

**The hub.** Hold jump (or flick out of it): the views fan out on an inner arc and the windows —
bags, character, spellbook, talents, quest log, map, and "more" for the whole radial — on an outer
one, toward the middle of the screen. Slide onto one and let go; let go back on jump for nothing.
Jump itself goes up the moment it is pressed, as its key does, except while the character sits or
casts: then it waits for the release (a tap still jumps), so opening the hub costs neither a drink
nor a cast.

**The Dynamic Island.** The stock windows stop where the island (or the notch) begins, on
whichever side it is — `UIParent` is anchored to the screen with the safe area on the camera's side
— while the 3D world still fills the screen. The menu, view and iNilla buttons sit in the strip
beside the island (on a phone without one, at the top centre). Settings → General turns the
margin off.

**Settings → Touch → Edit layout** puts every element under your finger: drag to move (it snaps to
a 4-point grid and keeps its corner anchor, so it survives other screen sizes), `−`/`+` to resize,
opacity, shown/hidden, which views it appears in, and what a button does (any binding command).
The editor's bar sits under the top row of buttons (**⬇ Bar down** moves it to the bottom), with
**Reset this** for the selected element, **Reset all** (tap twice) and **Done**.

## Controllers

Anything iOS sees as an extended gamepad: the **Backbone One**, Xbox, DualSense/DualShock, Switch
Pro, MFi. The default mapping follows WoW Forever:

| | Base | LT held | RT held | LT + RT |
|---|---|---|---|---|
| A | Jump | bottom-left bar 1 | bottom-right bar 1 | right bar 1 |
| B | Interact (right-click the target) | … 2 | … 2 | … 2 |
| X / Y | action 1 / 2 | … 3 / 4 | … 3 / 4 | … 3 / 4 |
| d-pad ↑ → ↓ ← | actions 3–6 | … 5–8 | … 5–8 | … 5–8 |
| LB / RB | target friend / enemy | previous friend / enemy | target self / assist | (base) |
| left stick | move (8-way) · L3 autorun | | | |
| right stick | look · R3 virtual cursor | | | |
| ☰ Menu | radial menu (hold + point + release, or tap + d-pad + A) | | | |
| View / Options | game menu | | | |

"Bottom-left bar" is `MULTIACTIONBAR1` (action slots 61–72), "bottom-right" `MULTIACTIONBAR2`
(49–60), "right bar" `MULTIACTIONBAR3` (25–36): drag spells onto those bars as on desktop; holding
a trigger lights up the ones it switches the face buttons and the d-pad to.

**What presses what is on screen.** While the controller is in use, every stock action button
shows the controller button that presses it where the keyboard key would be (`X`, `LT A`,
`RT ↑`, in the controller's own letters — PlayStation and Nintendo pads get theirs). The first time
a controller is used, iNilla switches on the bottom-left, bottom-right and right bars the trigger
layers press (1.12 hides them by default) and one line says what the shoulders, triggers, ☰ and R3
do. **Holding a trigger lights up the buttons its layer presses** on those bars (their own
highlight). The controller bar — the held layer's eight buttons along the bottom of the screen —
is off by default; Settings → Controller turns it on, and turns the hotkey hints off.

**Cursor mode** (R3, and automatically while loot, vendor, quest, bag, spellbook, map, … windows
are open): the right stick moves a pointer, A clicks, B right-clicks, the left stick still walks.
On the glue screens it is always on. Everything above is remappable per layer in
**Settings → Controller**, along with deadzone, trigger threshold, turn and pitch speed, curve,
inversion, cursor speed and a Nintendo-style confirm swap. The touch HUD steps aside while a
controller is in use and comes back with the next touch.

## Languages and addons

The game speaks the language of its data. iNilla reads it off the install (`ChrRaces.dbc`'s race
names, as vmangos does) and, for anything but English, moves each DBC's text into the column the
readers use (`benilla-dbc`'s `localize`). The logon then reports that language, so vmangos sends
its quests, items and NPC names in it too, and `GetLocale()` answers it, so addons pick their own
translation. An iNilla server can host several languages; the Game data screen then offers a
choice (one language on the phone at a time: switching downloads that language's archives).

German is wired end to end and has been played on an iPhone: it needs a German 1.12.1 client's
`Data` on the server (see the iNilla server's docs). The German interface calls
`SetEuropeanNumbers`, so the tooltips write their decimals with a comma ("Tempo 2,60").

**Addons** come from the server too: its data host lists them per language (an iNilla server
ships pfQuest in English and German) and **Install / update addons** on the Game data screen, or a
download of the game data, installs them into `benilla-config/AddOns`, hash-checked, and keeps
them current. Addons the player copies in are never touched (`AddOns/.inilla-addons.json` records
which folders the server installed).

## The launcher

Server profiles hold the realmlist (host, port), the server software (vmangos / cMaNGOS / other —
the protocol is the same), an optional **world server host** (the realm-address override: the
realm list's address is often only reachable from one side of a NAT — a LAN IP from outside, a
public IP from inside a network without hairpin NAT), an optional realm to pick automatically,
and an optional game-data URL. Accounts hold a username; the password goes to the **iOS Keychain**
if you let it. Log in with one tap, switch accounts from the login screen, or let one account log
in by itself at launch. Account **registration** is not in the app: 1.12's protocol has no sign-up,
so it happens on the server's website or console (`account create`).

## Settings

- **General** — graphics preset (Battery saver 30 fps · Balanced 60 fps · High · Custom), render
  resolution (the 3D world at 50–100 % of the screen; the interface stays sharp), interface size
  (automatic is 1.25 on a phone: a stock action button near fingertip size, and quest and vendor
  windows still in reach), screen awake, keyboard, haptics, compact
  stock UI (hides the action bar's gryphons), windows clear of the Dynamic Island.
- **Touch**, **Controller** — above.
- **Audio** — keep other apps' music playing; ignore the silent switch.
- **Debug** — the remote console, the session log, the perf overlay (fps, memory headroom,
  thermal state), touch zones. See [DEBUGGING.md](DEBUGGING.md).

Game settings — video details, sound volumes, key bindings, interface options — stay where they are
in 1.12: the stock options windows (radial → Game menu).

Everything persists in `benilla-config/inilla/*.toml` in the app's Documents (visible in the Files
app), next to benilla's own `config.toml`, bindings, macros and saved variables.
