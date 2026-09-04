//! `thudcensus`: the whole-table view of the **death thud** — the body-fall sound a corpse makes
//! as it lands (`$DTH` → `0x6236e0`), the sibling of the same event's camera shake.
//!
//! Two halves, because the report it exists for ("a big corpse hits the ground silently") has two
//! possible causes and they look identical in game:
//!
//! 1. **The table** — the `DeathThudLookups.dbc` matrix in full, `SizeClass × TerrainTypeSoundID`
//!    → the land and water `SoundEntries` kits, named. An empty cell here is *authored* silence
//!    (Sand and Soggy have no water column at all), and telling that apart from a bug is the
//!    whole point of printing the grid rather than the rows.
//! 2. **The population** — which creature M2s actually key a `$DTH` at all, on which sequences,
//!    and what size class each resolves to. A model that keys none is silent in the reference
//!    too; a model that keys one and still sounds wrong is a runtime question, not a data one.
//!
//! The cross-check that validates the column map: every live `DeathThudLookups` sound id must
//! land on a real `SoundEntries` row, every terrain axis value on a real `TerrainTypeSounds` row,
//! and every model's `SizeClass` inside `0..=4` — a shifted field would scatter across the id
//! space and hit holes, exactly as `shakecensus` checks fields 11/12.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use benilla_formats::Chain;

/// A model path reduced to a comparison key: lowercase, back-slashed, extension dropped. The DBCs
/// name models `.mdx`; the archives hold them as `.m2`.
fn mdx_key(path: &str) -> String {
    let p = path.to_ascii_lowercase().replace('/', "\\");
    match p.rsplit_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => p,
    }
}

/// The `SizeClass` names the shipped kits give themselves (`DeathThudSmallDirt` …
/// `DeathThudColossalWood`).
const SIZE_NAMES: [&str; 5] = ["Small", "Medium", "Large", "Giant", "Colossal"];

fn size_name(class: i32) -> String {
    match usize::try_from(class).ok().and_then(|c| SIZE_NAMES.get(c)) {
        Some(n) => format!("{class} {n}"),
        None => format!("{class} ??"),
    }
}

pub fn thudcensus(chain: &mut Chain) -> Result<()> {
    let thuds = benilla_formats::load_death_thud_catalog(chain)?;
    let steps = benilla_formats::load_footstep_catalog(chain)?;
    let kits = benilla_formats::load_sound_kit_catalog(chain)?;
    let creatures = benilla_formats::load_creature_catalog(chain)?;

    // ── 1 · the table ────────────────────────────────────────────────────────────────────────
    // The terrain axis, with the TerrainType rows that reach each id — a terrain sound nothing
    // names is unreachable however well-populated its column is.
    let mut terrain_names: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for terrain in 0..64 {
        if let Some(sound) = steps.sound_class_of(terrain) {
            terrain_names.entry(sound).or_default().push(terrain);
        }
    }
    let axis: Vec<u32> = thuds.terrain_sounds().collect();
    let classes: BTreeSet<u32> = thuds.size_classes();

    println!(
        "DeathThudLookups.dbc — {} rows over {} size class(es) × {} TerrainTypeSounds id(s)\n",
        thuds.len(),
        classes.len(),
        axis.len()
    );
    let mut dangling: Vec<String> = Vec::new();
    let name_of = |id: u32, dangling: &mut Vec<String>, what: &str| -> String {
        if id == 0 {
            return "—".to_string();
        }
        match kits.get(id) {
            Some(k) => format!("{id} {}", k.name),
            None => {
                dangling.push(format!("{what}: SoundEntries {id} does not exist"));
                format!("{id} DANGLING")
            }
        }
    };
    for &sound in &axis {
        let reached = match terrain_names.get(&sound) {
            None => "  ** no TerrainType names it **".to_string(),
            Some(t) => format!(
                "  TerrainType {}",
                t.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
            ),
        };
        println!("terrain sound {sound}{reached}");
        for &class in &classes {
            let cell = match thuds.resolve(class, sound) {
                None => "—  NO ROW".to_string(),
                Some((land, water)) => format!(
                    "{:<28}  water {}",
                    name_of(land, &mut dangling, &format!("({class}, {sound}) land")),
                    name_of(water, &mut dangling, &format!("({class}, {sound}) water")),
                ),
            };
            println!("    {:<12}  {cell}", size_name(class as i32));
        }
    }

    // ── 2 · the population ───────────────────────────────────────────────────────────────────
    // Which creature models key a `$DTH`, and the size class(es) the displays that reach each one
    // resolve to. A model whose displays disagree is normal — 2 971 displays override their
    // model's column — and the census prints the whole set so a mis-sized thud is visible.
    let mut resolved: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (display, model) in creatures.display_models() {
        if let Some(class) = creatures.size_class(display) {
            resolved.entry(model).or_default().insert(class);
        }
    }
    let mut by_key: BTreeMap<String, Vec<(u32, i32)>> = BTreeMap::new();
    let mut off_axis = Vec::new();
    for (id, path, class) in creatures.sized_models() {
        if !(0..=4).contains(&class) {
            off_axis.push(format!("{path} ({id}): SizeClass = {class}"));
        }
        by_key.entry(mdx_key(path)).or_default().push((id, class));
    }

    println!("\n$DTH animation events — which creature models thud at all\n");
    let mut keyed = 0u32;
    let mut marks = 0u32;
    let mut silent: Vec<String> = Vec::new();
    let mut keyed_classes: BTreeMap<u32, u32> = BTreeMap::new();
    for name in crate::scan::m2_names(chain, None)? {
        let key = mdx_key(&name);
        let Some(models) = by_key.get(&key) else {
            continue; // not a creature model: `CGUnit_C::HandleAnimEvent` never sees its events
        };
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        let mut rows = Vec::new();
        for a in benilla_formats::parse_m2_animations(&bytes) {
            for e in a.events.iter().filter(|e| &e.ident == b"$DTH") {
                marks += 1;
                rows.push(format!(
                    "    seq {:>2} anim {:>3} {:>7.3}s",
                    a.seq_index, a.anim_id, e.time
                ));
            }
        }
        let sizes: BTreeSet<u32> = models
            .iter()
            .flat_map(|(id, _)| resolved.get(id).into_iter().flatten().copied())
            .collect();
        let label = format!(
            "{name}   [{}]",
            if sizes.is_empty() {
                "no display reaches it".to_string()
            } else {
                sizes
                    .iter()
                    .map(|c| size_name(*c as i32))
                    .collect::<Vec<_>>()
                    .join(" · ")
            }
        );
        if rows.is_empty() {
            silent.push(label);
            continue;
        }
        keyed += 1;
        for c in &sizes {
            *keyed_classes.entry(*c).or_default() += 1;
        }
        println!("  {label}");
        for r in rows {
            println!("{r}");
        }
    }

    println!(
        "\n{keyed} creature model(s) key a $DTH ({marks} marker(s)); {} key none",
        silent.len()
    );
    for c in classes {
        println!(
            "  {:<12}  {} keyed model(s)",
            size_name(c as i32),
            keyed_classes.get(&c).copied().unwrap_or(0)
        );
    }
    if !silent.is_empty() {
        println!("\nkeying no $DTH — silent in the reference too:");
        for s in &silent {
            println!("  {s}");
        }
    }
    for d in &dangling {
        println!("  DANGLING {d}");
    }
    for o in &off_axis {
        println!("  OFF-AXIS {o}  (the reference's unsigned `>= 5` gate silences this model)");
    }
    println!(
        "\n{} dangling sound id(s); {} model(s) off the 0..=4 size axis",
        dangling.len(),
        off_axis.len()
    );
    Ok(())
}
