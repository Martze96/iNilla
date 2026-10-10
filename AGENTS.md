# iNilla (this fork)

This repository is **iNilla**, the iOS build of benilla: `crates/inilla-*`, `ios/`,
`crates/benilla-app/src/inilla/` and the `inilla-*` workflows are ours; everything else is
upstream benilla, merged in (see `docs/UPSTREAM.md` for the carries and how to sync). Read
`docs/INILLA.md` first, then `docs/IOS-BUILD.md` and `docs/DEBUGGING.md`.

- Engine-free logic goes in `inilla-core` with a unit test (`cargo test -p inilla-core`).
- OS calls go in `inilla-platform`, behind a function that compiles on every target.
- A change inside an upstream file is a carry: keep it minimal, mark it `iNilla`, list it in
  `docs/UPSTREAM.md`.
- Before pushing: `cargo check -p benilla-app` and
  `source ios/toolchain/linux-env.sh && cargo check --target aarch64-apple-ios -p inilla-ios`.
- Never commit anything from a WoW install (the `.MPQ` archives, extracted assets).

---

# benilla

A from-scratch World of Warcraft 1.12.1 client in Rust and Bevy. The reference client is the
spec: benilla is a faithful, modern implementation of it, not a place to get creative.

Read, in this order:

1. `docs/METHOD.md` — the rules and how we work. Binding for every session.
2. `docs/MAP.md` — what is built right now, generated from the code. Orient here before
   touching anything.
3. `docs/CONTRIBUTING.md` — what gets in and how a change is judged.

The rest of the root is code and data: `crates/` the workspace, `reference/` the 1.12 name
catalogues the tests check against, `scripts/` the gates and instruments, `third_party/`
vendored code.

@docs/METHOD.md
