//! **The reverse wall of decision 1177, measured** — how many files in `benilla-app` outside the
//! VM's own module name `UiScript`.
//!
//! 1177 (2026-08-10) drew the line: the feed layer becomes one module, the only legitimate caller
//! of the Lua VM, and everything else produces model values. Its instrument was 1160's — a wall
//! test in `tests/` whose number is ratcheted down by the work — and it named the number: **141
//! files**. The instrument was never built, and with nothing failing the count went the other
//! way: 164 files by 2026-09-16 (decision 2265 §A3). This file is the instrument. It does not
//! decide whether the crate move happens — that stays the director's — it only makes the number
//! visible on every test run and refuses to let it grow unnoticed.
//!
//! "Outside the VM's module" means any `.rs` under `crates/benilla-app/src` whose path does not
//! start with `ui_script/`; 1177's "feed module" does not exist yet, and when it does the
//! exclusion moves there. A file counts if it contains the identifier `UiScript` at all — a type
//! in a parameter list, a `use`, a doc comment naming the seam: each is a place that knows the
//! FrameXML VM exists, which is the coupling 1177 measures.
//!
//! Same shape as `world_api_wall.rs`: `CEILING` is the standing count, `SLACK` keeps a single
//! closure from failing the gate while making it impossible to bank a whole stage of work without
//! writing the new number down. `WOW_UISCRIPT_DUMP=1` prints the files.

use std::path::{Path, PathBuf};

/// The standing count. **2026-09-16, 164** — measured the day the instrument was built, against
/// 1177's 141 five weeks earlier.
const CEILING: usize = 164;

/// How far under [`CEILING`] the count may sit before the test asks for the ceiling to follow it.
const SLACK: usize = 8;

#[test]
fn the_vm_is_named_by_no_more_files_than_the_ceiling_says() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<String> = rs_files(&src)
        .into_iter()
        .filter_map(|p| {
            let rel = p
                .strip_prefix(&src)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with("ui_script/") || rel == "ui_script.rs" {
                return None;
            }
            let text = std::fs::read_to_string(&p).ok()?;
            names_ident(&text, "UiScript").then_some(rel)
        })
        .collect();
    files.sort();
    let n = files.len();
    eprintln!("files outside ui_script/ naming UiScript: {n} (ceiling {CEILING}, slack {SLACK})");
    if std::env::var("WOW_UISCRIPT_DUMP").is_ok() {
        eprintln!("{}", files.join("\n"));
    }
    assert!(
        n <= CEILING,
        "{n} files outside ui_script/ name UiScript; the ceiling is {CEILING}.\n\
         A new file learned that the FrameXML VM exists. Decision 1177's line is that only the \
         feed layer may know; either feed a model value instead, or — if this file genuinely is \
         feed code — raise CEILING here with the reason, the way world_api_wall.rs requires.\n\
         WOW_UISCRIPT_DUMP=1 lists the files."
    );
    assert!(
        n + SLACK >= CEILING,
        "only {n} files name UiScript and the ceiling still says {CEILING}. Lower CEILING to {n} \
         in this file so the next coupling has to earn its place — the ratchet only holds if the \
         number follows the work down."
    );
}

/// Does `text` contain `ident` as a whole identifier (not as part of a longer one)?
fn names_ident(text: &str, ident: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(i) = text[from..].find(ident) {
        let at = from + i;
        let end = at + ident.len();
        let before_ok = at == 0 || !text[..at].chars().next_back().is_some_and(is_ident);
        let after_ok = !text[end..].chars().next().is_some_and(is_ident);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_identifier_match_is_whole_word() {
    assert!(names_ident("fn f(s: NonSendMut<UiScript>)", "UiScript"));
    assert!(names_ident("UiScript", "UiScript"));
    assert!(!names_ident("UiScriptPlugin only", "UiScript"));
    assert!(!names_ident("MyUiScript", "UiScript"));
    assert!(names_ident("MyUiScript and UiScript", "UiScript"));
}
