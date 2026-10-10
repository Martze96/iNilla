# The game data

iNilla reads the game — art, models, maps, sounds, the stock interface — from **your own**
World of Warcraft **1.12.1 (build 5875)** client, as benilla does on desktop. The app contains no
Blizzard files. It needs the `.MPQ` archives at the root of the client's `Data` folder (about 5 GB):
`dbc`, `fonts`, `interface`, `misc`, `model`, `patch`, `patch-2`, `sound`, `speech`, `terrain`,
`texture`, `wmo`. English (enUS) clients are the tested ones; a German (deDE) client works the same
way (its text is found in its own language column), untested so far.

On the device they end up in the app's private Application Support folder, excluded from iCloud
backup, and survive restarts, app updates and SideStore refreshes. You do this once.

## Way 1 — copy the folder in

iNilla's **Documents** folder is visible to the Files app and to Finder:

- **From a Mac**: connect the iPhone, Finder → your iPhone → **Files** → iNilla → drag the
  client's `Data` folder onto it.
- **From Windows**: the **Apple Devices** app (or iTunes) → your iPhone → **Files** → iNilla →
  add the `Data` folder.
- **On the iPhone**: **Files** → On My iPhone → **iNilla** → paste the `Data` folder from iCloud
  Drive, a USB-C stick or drive, a network share (Files → ⋯ → Connect to Server), or AirDrop.
- **In LiveContainer**: the folder is On My iPhone → LiveContainer → Data → Application →
  *iNilla's folder* → Documents (the setup screen shows the exact name; the app must be
  *private* in LiveContainer for Files to list it).

Then open iNilla: a complete folder found in Documents (at most three levels deep, e.g.
`Documents/World of Warcraft/Data`) is **moved** into place at launch — a rename, instant, no second
copy — and the app goes straight to the login screen. If iNilla was already open, use **Check for
copied data** on the setup screen, then close and reopen it.

## Way 2 — download it from your server

The [iNilla server](https://github.com/Martze96/iNilla-server) runs a data host next to its
vmangos (`docker compose up -d`, or only the data host beside a server you already have). It
serves the archives of the client's `Data` folder on port 8099.

In iNilla's setup screen enter `http://<that machine>:8099/` (it is filled in from the server you
added; a token only if the host set one), **Check server**, then **Download**. When the server has
the game in more than one language, pick one first; its addons (pfQuest, …) install right after
the archives. It fetches a manifest (names, sizes, SHA-256), then each
archive; an interrupted download **resumes** where it stopped (HTTP `Range`), and every file is
checked against its hash before it is used. Keep the screen on and the app open while it runs.

- It serves only the mountable archives at the root of `Data/` — no other file on the disk.
- For the internet, put it behind HTTPS (Caddy, nginx) or reach it over a VPN (Tailscale,
  WireGuard). iNilla speaks both http and https (public CA certificates).
- A server profile can carry the data URL (Servers → Edit → Advanced), so setup pre-fills it.

The wire format (`manifest.json`, `files/<name>`, and `locales.json`, `addons.json`,
`addons/<path>`) is `inilla-core`'s `data` and `addons` modules; the host is the other half of it.

## A word on copyright

The archives are Blizzard's. Copying your own client onto your own phone, or serving it to your own
devices, is between you and your copy of the game. Serving it to other people is distributing
Blizzard's files, whoever runs the server — keep the token on and share only with people who own
the game.
