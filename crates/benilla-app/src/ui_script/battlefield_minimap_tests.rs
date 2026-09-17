//! The **battle map** — `Blizzard_BattlefieldMinimap`, the reference's own LoadOnDemand addon,
//! driven through the road stock `UIParent.lua` drives it down: `ToggleBattlefieldMinimap()`
//! (SHIFT-M) → `BattlefieldMinimap_LoadUI()` → `UIParentLoadAddOn`.
//!
//! benilla has registered the addon as a chain row since 1957 and a live run confirmed it loads —
//! and that was the whole of the evidence. Nothing had ever checked that its twelve overlay
//! textures get cut from `GetMapOverlayInfo`, that its POI pool grows, that the battleground blips
//! and the flag carrier reach it, that its arrow is the *mini* singleton rather than the world
//! map's, or that its tab, dropdown and opacity slider work at all. This file is that check: the
//! whole addon, top to bottom, against the engine verbs it actually calls.
//!
//! **The harness is the whole manifest, not a cut dependency prefix** — the deliberate difference
//! from `tradeskill_frame.rs` / `talent_frame.rs` / `auction_frame.rs`, whose windows each sit on
//! a handful of FrameXML files. This one reads `MiniMapBattlefieldFrame.status` (Minimap.xml),
//! `GetNumWorldStateUI` and `SHOW_BATTLEFIELD_MINIMAP` (WorldStateFrame.lua), `UIOptionsFrame`
//! (UIOptionsFrame.xml), `OpacityFrameSlider` (ColorPickerFrame.xml), the `WorldMapUnitTemplate`
//! family (WorldMapFrameTemplates.xml) and `MAX_PARTY_MEMBERS`/`MAX_RAID_MEMBERS` — five corners
//! of the manifest, which is exactly the state a player is in when they press SHIFT-M. It also
//! lives *in* the crate rather than under `tests/`, because the LoadOnDemand road needs
//! [`super::test_ui::seat_chain_addon`] and an integration test cannot reach it (see
//! `tests/common/mod.rs`'s own note on the `#[cfg(test)]` boundary).

use benilla_ui::script::{
    BattlefieldFlagView, BattlefieldPositionView, QuadContent, UiScript, UnitState,
    WorldMapContinentView, WorldMapLandmarkView, WorldMapOverlayView, WorldMapZoneView,
    WorldStateUiView, ARROW_MODEL,
};

/// **The manifest's own warning floor** — one row, and it is a decided permanent gap rather than
/// anything this addon does: stock `OptionsFrame.lua:300` reads `GetCVar("gxRefresh")` and 2177
/// keeps that CVar unregistered on purpose, because nothing here can set a refresh rate.
/// `world_entry_tests`' own `KNOWN` list carries the same row for the same reason. Pinning the
/// *exact* set rather than allowing any warning is what makes [`quiet`] a real assertion: a
/// warning the addon causes shows up as a second row.
const MANIFEST_WARNINGS: [&str; 1] = ["unknown CVar 'gxRefresh' (not host-registered) — ignored"];

/// Zero Lua errors, and no host warning the manifest did not already carry.
fn quiet(s: &UiScript) {
    assert!(s.errors().is_empty(), "script errors: {:#?}", s.errors());
    assert_eq!(
        s.warnings(),
        MANIFEST_WARNINGS,
        "host warnings beyond the manifest's own"
    );
}

/// The full interface with the addon **registered but not loaded** — the state a character is in
/// the moment before SHIFT-M.
///
/// The player exists because the in-game UI materializes on world entry (1051) and several stock
/// OnLoads format `UnitName("player")` into their labels; a manifest load with no player is a
/// state the client never reaches (1848).
fn session() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.set_unit(
        "player",
        Some(UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    super::test_ui::seat_chain_addon(&mut s, "Blizzard_BattlefieldMinimap");
    s.resolve();
    quiet(&s);
    s
}

/// The arrow file's facts, as the host hands them over (2007/2015) — `MinimapArrow.m2`'s one
/// looping 3.333 s Stand and its header box, which the implicit rect and the re-centring read.
/// Seated *after* the manifest on purpose: the world map's own arrow is created during that load,
/// so this is the mini arrow's facts and only the mini arrow's.
fn arrow_facts(s: &mut UiScript) {
    use benilla_ui::widget::{ModelFileFacts, SequenceFacts};
    s.set_model_facts(
        ARROW_MODEL,
        ModelFileFacts {
            sequences: vec![SequenceFacts {
                anim_id: 0,
                duration_ms: 3333,
                looping: true,
            }],
            bbox: ([-0.0127, -0.0118, 0.0], [0.0135, 0.0145, 0.0]),
            cameras: 0,
        },
    );
}

/// One `WorldMapOverlay` row, revealed by a single explore bit.
fn overlay(texture: &str, w: u32, h: u32, ox: u32, oy: u32, bit: u32) -> WorldMapOverlayView {
    WorldMapOverlayView {
        texture: format!("Interface\\WorldMap\\WarsongGulch\\{texture}"),
        width: w,
        height: h,
        offset_x: ox,
        offset_y: oy,
        explore_bits: vec![bit],
        hit_rect: (0, 0, 0, 0),
        area_name: None,
    }
}

/// A one-zone catalog whose zone **has overlays that make the slicing loop do real work**.
///
/// The sizes are chosen so both arms of the reference's tile walk run. `SilverwingHold` is
/// 300×200: `ceil(300/256) = 2` wide, `ceil(200/256) = 1` tall, so its second column takes the
/// remainder arm (`mod(300,256) = 44` px in a 64-px power-of-two file) while the first takes the
/// full-tile arm, and its single row takes the remainder arm vertically (`mod(200,256) = 200` in a
/// 256-px file). `WarsongLumberMill` is exactly 256×256 — the one case where *both* `mod`s come
/// out zero and the reference's `if ( texturePixelWidth == 0 ) then texturePixelWidth = 256` guard
/// is what keeps the tile from collapsing. Between them they need three textures, which is also
/// what makes the second half of [`the_overlay_textures_are_cut_from_the_revealed_overlays`] — the
/// tail that parks — observable.
fn catalog() -> Vec<WorldMapContinentView> {
    vec![WorldMapContinentView {
        name: "Kalimdor".into(),
        map_file: "Kalimdor".into(),
        world_rect: (0.0, 0.0, 1.0, 1.0),
        loc_rect: (10000.0, -10000.0, 10000.0, -10000.0),
        zone_grid: Vec::new(),
        zones: vec![WorldMapZoneView {
            name: "Warsong Gulch".into(),
            area_id: 3277,
            map_file: "WarsongGulch".into(),
            loc_rect: (1000.0, 0.0, 1000.0, 0.0),
            overlays: vec![
                overlay("SilverwingHold", 300, 200, 100, 50, 1),
                overlay("WarsongLumberMill", 256, 256, 400, 300, 2),
            ],
        }],
    }]
}

/// Put the session in a battleground and press SHIFT-M.
///
/// The world-state push is the gate, not scenery: `BattlefieldMinimap_Toggle` shows nothing unless
/// `MiniMapBattlefieldFrame.status == "active"` **or** `GetNumWorldStateUI() > 0`, which is the
/// reference refusing to open a battle map outside a battle. The catalog + explored bitset put the
/// displayed map on a zone that has revealed overlays, and `player_zone` is what the window's own
/// `<OnShow>` `SetMapToCurrentZone()` lands on.
fn open(s: &mut UiScript) {
    s.set_world_map_catalog(catalog());
    s.set_world_map_explored(vec![0b110; 64]);
    s.set_world_map_feed(
        Some((1, 1)),
        Some((0.5, 0.5)),
        0.0,
        None,
        Vec::new(),
        Vec::new(),
    );
    s.set_world_state_ui(vec![WorldStateUiView {
        ui_state: 1,
        text: "Flags captured: 2".into(),
        ..Default::default()
    }]);
    s.tick(0.0);
    s.fire_event("UPDATE_WORLD_STATES", vec![]);
    s.run("ToggleBattlefieldMinimap()").expect("SHIFT-M");
    assert!(
        s.eval::<bool>("return BattlefieldMinimap:IsShown()")
            .unwrap(),
        "the battle map is up"
    );
}

/// The window's own per-frame update, driven the way its `<OnUpdate>` script runs it: `this` is
/// the frame and the elapsed time its one argument.
fn update(s: &mut UiScript, elapsed: f64) {
    s.run(&format!(
        "this = BattlefieldMinimap BattlefieldMinimap_OnUpdate({elapsed}) this = nil"
    ))
    .unwrap();
}

/// A frame's centre in the screen units the pointer speaks (its own layout centre × its effective
/// scale) — what a hit test and a `mouse_move` want.
fn screen_centre(s: &mut UiScript, frame: &str) -> (f32, f32) {
    s.resolve();
    let (x, y, eff) = s
        .eval::<(f64, f64, f64)>(&format!(
            "local a, b = {frame}:GetCenter() return a, b, {frame}:GetEffectiveScale()"
        ))
        .unwrap_or_else(|e| panic!("{frame}:GetCenter() — {e}"));
    ((x * eff) as f32, (y * eff) as f32)
}

/// The mini arrow's screen centre. It is anonymous and it is the window's **last** child (its
/// `Create…` runs in the `<OnLoad>`, after the XML's own forty-eight), so it is found by kind
/// rather than by name or position.
fn arrow_centre(s: &mut UiScript) -> (f32, f32) {
    s.resolve();
    let (x, y, eff) = s
        .eval::<(f64, f64, f64)>(
            r#"for _, c in ipairs({ BattlefieldMinimap:GetChildren() }) do
                   if c:GetObjectType() == "Model" then
                       local a, b = c:GetCenter()
                       return a, b, c:GetEffectiveScale()
                   end
               end"#,
        )
        .expect("the battle map has a Model child");
    ((x * eff) as f32, (y * eff) as f32)
}

fn shown(s: &UiScript, frame: &str) -> bool {
    s.eval::<bool>(&format!("return {frame}:IsShown()"))
        .unwrap_or_else(|e| panic!("{frame}:IsShown() — {e}"))
}

fn num(s: &UiScript, expr: &str) -> f64 {
    s.eval::<f64>(&format!("return {expr}"))
        .unwrap_or_else(|e| panic!("{expr} — {e}"))
}

/// **The addon loads on demand and every frame its XML declares materializes.**
///
/// Driven down the reference's own road — `ToggleBattlefieldMinimap()` is what SHIFT-M is bound
/// to, and its first line is `BattlefieldMinimap_LoadUI()` → `UIParentLoadAddOn(...)` → the
/// registry's `LoadAddOn`. Loading it any other way (running the `.xml` as a chain file, the way
/// the auction and tradeskill suites load theirs) would skip the registry, `ADDON_LOADED` and its
/// `arg1` filter — and the `ADDON_LOADED` arm is where the tab gets its seat, the dropdown gets
/// initialized and the opacity gets applied, so it would skip most of the window's initial state
/// too.
///
/// The gate is asserted in both directions first: the load happens either way, but a battle map
/// opens only inside a battle.
#[test]
fn the_addon_loads_on_demand_through_uiparents_own_road() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    assert_eq!(
        s.eval::<Option<i64>>("return IsAddOnLoaded(\"Blizzard_BattlefieldMinimap\")")
            .unwrap(),
        None,
        "nothing is loaded before the first toggle"
    );

    // Out of a battleground: the addon still loads — `BattlefieldMinimap_LoadUI()` runs
    // unconditionally — and then `BattlefieldMinimap_Toggle`'s own gate declines to show it.
    s.run("ToggleBattlefieldMinimap()").unwrap();
    assert_eq!(
        s.eval::<i64>("return IsAddOnLoaded(\"Blizzard_BattlefieldMinimap\")")
            .unwrap(),
        1,
        "the demand load ran"
    );
    assert!(
        !shown(&s, "BattlefieldMinimap"),
        "…and a battle map does not open outside a battle: no active queue, no world-state rows"
    );
    assert_eq!(
        s.eval::<String>("return SHOW_BATTLEFIELD_MINIMAP").unwrap(),
        "0",
        "the RegisterForSave global stays at WorldStateFrame.lua's own boot value"
    );

    // Every frame, region and font string the `.xml` declares, by name. The loop answers the FIRST
    // missing one rather than a bare false, so a regression names itself.
    let missing: Option<String> = s
        .eval(
            r#"
            local names = {
                "BattlefieldMinimap", "BattlefieldMinimapBackground", "BattlefieldMinimapCorner",
                "BattlefieldMinimapCloseButton", "BattlefieldMinimapCorpse",
                "BattlefieldMinimapTab", "BattlefieldMinimapTabLeft", "BattlefieldMinimapTabMiddle",
                "BattlefieldMinimapTabRight", "BattlefieldMinimapTabFlash",
                "BattlefieldMinimapTabText", "BattlefieldMinimapTabDropDown",
            }
            for i = 1, 12 do table.insert(names, "BattlefieldMinimap" .. i) end
            for i = 1, 4 do table.insert(names, "BattlefieldMinimapParty" .. i) end
            for i = 1, 40 do table.insert(names, "BattlefieldMinimapRaid" .. i) end
            for i = 1, 2 do table.insert(names, "BattlefieldMinimapFlag" .. i) end
            for _, n in ipairs(names) do
                if not getglobal(n) then return n end
            end
            return nil
            "#,
        )
        .unwrap();
    assert_eq!(missing, None, "a declared frame never materialized");

    // `BattlefieldMinimapRaidUnitTemplate`'s own OnLoad ran on each of the forty: the unit token
    // from the frame's `id=`, and the party blip art on its `$parentIcon`.
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapRaid17.unit")
            .unwrap(),
        "raid17"
    );
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapRaid17Icon:GetTexture()")
            .unwrap(),
        "Interface\\WorldMap\\WorldMapPartyIcon"
    );

    // The `ADDON_LOADED` arm: the saved-variable table came up on `BattlefieldMinimapDefaults`…
    assert_eq!(
        s.eval::<(f64, bool, bool)>(
            "return BattlefieldMinimapOptions.opacity, BattlefieldMinimapOptions.locked, \
             BattlefieldMinimapOptions.showPlayers"
        )
        .unwrap(),
        (0.7, true, true)
    );
    // …the tab took its default seat (no saved position → the `-225-CONTAINER_OFFSET_X` corner,
    // whose two constants are ContainerFrame.lua's and UIParent.lua's)…
    let (point, relative, rel_point, x, y) = s
        .eval::<(String, String, String, f64, f64)>(
            "local p, r, rp, ox, oy = BattlefieldMinimapTab:GetPoint(1) \
             return p, r:GetName(), rp, ox, oy",
        )
        .unwrap();
    assert_eq!(
        (point.as_str(), relative.as_str(), rel_point.as_str()),
        ("BOTTOMLEFT", "UIParent", "BOTTOMRIGHT")
    );
    assert_eq!(
        (x, y),
        (
            -225.0 - num(&s, "CONTAINER_OFFSET_X"),
            num(&s, "BATTLEFIELD_TAB_OFFSET_Y")
        )
    );
    // …and `BattlefieldMinimap_SetOpacity()` ran off the seeded slider value, so the window is
    // already wearing the default 0.7 opacity (its alpha is `1 - value`) before it is ever shown.
    assert!(
        (num(&s, "BattlefieldMinimapBackground:GetAlpha()") - 0.3).abs() < 1e-6,
        "the load-time opacity pass reached the border texture"
    );

    // Now inside a battleground, the same toggle opens it.
    open(&mut s);
    assert_eq!(
        s.eval::<String>("return SHOW_BATTLEFIELD_MINIMAP").unwrap(),
        "1"
    );
    assert!(
        shown(&s, "BattlefieldMinimapTab"),
        "the tab comes up with it"
    );
    quiet(&s);
}

/// **The twelve map tiles and the overlay textures are cut from the map the engine is showing.**
///
/// Two loops, both reading host verbs the addon shares with the world map. The tiles are
/// `GetMapInfo()`'s art folder repeated `NUM_WORLDMAP_DETAIL_TILES` times; the overlays are
/// `GetNumMapOverlays()` / `GetMapOverlayInfo(i)` sliced into 256-px tiles, each scaled by the
/// window's own `BattlefieldMinimap1:GetWidth()/256` — a **56/256 = 0.21875** shrink, which is the
/// whole reason this window's overlay geometry is not the world map's and has to be checked
/// separately.
///
/// Every number below is that arithmetic done by hand from [`catalog`]'s two overlays, so the test
/// fails on a wrong scale, a wrong remainder, a dropped power-of-two round-up or a sign flip in
/// the y offset — not merely on "something was drawn".
#[test]
fn the_overlay_textures_are_cut_from_the_revealed_overlays() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);

    // The window's `<OnShow>` already ran `SetMapToCurrentZone()` and `BattlefieldMinimap_Update()`,
    // so the displayed map is the player's zone and the tiles are laid.
    assert_eq!(
        s.eval::<String>("return GetMapInfo()").unwrap(),
        "WarsongGulch"
    );
    for i in [1, 7, 12] {
        assert_eq!(
            s.eval::<String>(&format!("return BattlefieldMinimap{i}:GetTexture()"))
                .unwrap(),
            format!("Interface\\WorldMap\\WarsongGulch\\WarsongGulch{i}"),
            "detail tile {i}"
        );
    }

    assert_eq!(s.eval::<i64>("return GetNumMapOverlays()").unwrap(), 2);
    assert_eq!(
        s.eval::<i64>("return NUM_BATTLEFIELDMAP_OVERLAYS").unwrap(),
        3,
        "2 + 1 tiles: the 300×200 overlay needs two, the 256×256 one needs one"
    );

    // The reference reads back `(ULx, ULy, LLx, LLy, URx, URy, LRx, LRy)` since 1840; the old
    // `(left, right, top, bottom)` rect is positions 1, 5, 2, 4.
    let tex_rect = |s: &UiScript, n: u32| -> (f64, f64, f64, f64) {
        let (l, t, _, b, r, ..): (f64, f64, f64, f64, f64, f64, f64, f64) = s
            .eval(&format!(
                "return BattlefieldMinimapOverlay{n}:GetTexCoord()"
            ))
            .unwrap();
        (l, r, t, b)
    };
    let seat = |s: &UiScript, n: u32| -> (String, String, f64, f64) {
        s.eval(&format!(
            "local p, r, rp, x, y = BattlefieldMinimapOverlay{n}:GetPoint(1) return p, rp, x, y"
        ))
        .unwrap()
    };
    const SCALE: f64 = 56.0 / 256.0;

    // Tile 1 of SilverwingHold: a full 256-px column, 200 px tall in a 256-px file, at the
    // overlay's own (100, 50) offset — x right, y DOWN, both through the shrink.
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapOverlay1:GetTexture()")
            .unwrap(),
        "Interface\\WorldMap\\WarsongGulch\\SilverwingHold1"
    );
    assert_eq!(
        (
            num(&s, "BattlefieldMinimapOverlay1:GetWidth()"),
            num(&s, "BattlefieldMinimapOverlay1:GetHeight()")
        ),
        (256.0 * SCALE, 200.0 * SCALE)
    );
    assert_eq!(tex_rect(&s, 1), (0.0, 1.0, 0.0, 200.0 / 256.0));
    assert_eq!(
        seat(&s, 1),
        (
            "TOPLEFT".into(),
            "TOPLEFT".into(),
            100.0 * SCALE,
            -(50.0 * SCALE)
        )
    );

    // Tile 2: the remainder column — 44 px of art in the 64-px power-of-two file the client rounds
    // up to, seated exactly one full tile (256 × scale) to the right of tile 1.
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapOverlay2:GetTexture()")
            .unwrap(),
        "Interface\\WorldMap\\WarsongGulch\\SilverwingHold2"
    );
    assert_eq!(
        num(&s, "BattlefieldMinimapOverlay2:GetWidth()"),
        44.0 * SCALE
    );
    assert_eq!(tex_rect(&s, 2), (0.0, 44.0 / 64.0, 0.0, 200.0 / 256.0));
    assert_eq!(
        seat(&s, 2),
        (
            "TOPLEFT".into(),
            "TOPLEFT".into(),
            (100.0 + 256.0) * SCALE,
            -(50.0 * SCALE)
        )
    );

    // Tile 3 is the second overlay's only tile: exactly 256×256, the case where both `mod`s are
    // zero and the reference's `== 0 then 256` guards are load-bearing — a missing guard would
    // give this a zero-width, zero-height, `0..0` cropped tile.
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapOverlay3:GetTexture()")
            .unwrap(),
        "Interface\\WorldMap\\WarsongGulch\\WarsongLumberMill1"
    );
    assert_eq!(
        (
            num(&s, "BattlefieldMinimapOverlay3:GetWidth()"),
            num(&s, "BattlefieldMinimapOverlay3:GetHeight()")
        ),
        (56.0, 56.0)
    );
    assert_eq!(tex_rect(&s, 3), (0.0, 1.0, 0.0, 1.0));
    for n in 1..=3 {
        assert!(
            shown(&s, &format!("BattlefieldMinimapOverlay{n}")),
            "overlay tile {n} is lit"
        );
    }

    // Un-explore the lumber mill: the host stops returning that overlay, the repaint lights two
    // tiles and PARKS the third — the pool never shrinks, exactly as the world map's does not.
    s.set_world_map_explored(vec![0b010; 64]);
    s.tick(0.0); // the push queues WORLD_MAP_UPDATE, the addon's own repaint event
    assert_eq!(s.eval::<i64>("return GetNumMapOverlays()").unwrap(), 1);
    assert!(shown(&s, "BattlefieldMinimapOverlay1") && shown(&s, "BattlefieldMinimapOverlay2"));
    assert!(
        !shown(&s, "BattlefieldMinimapOverlay3"),
        "the tail parks hidden rather than being destroyed"
    );
    assert_eq!(
        s.eval::<i64>("return NUM_BATTLEFIELDMAP_OVERLAYS").unwrap(),
        3,
        "…and the pool keeps its high-water mark"
    );
    quiet(&s);
}

/// **The POI layer** — `GetNumMapLandmarks` / `GetMapLandmarkInfo` → `BattlefieldMinimap_CreatePOI`
/// → `WorldMap_GetPOITextureCoords`, the same three verbs the world map's own pool walks, but
/// seated against the 225×150 battle map and sized by `GetBattlefieldMapIconScale()` rather than
/// left at the flat 12 px the world map uses.
///
/// That scale is the point of the second half: the reference multiplies `DEFAULT_POI_ICON_SIZE` by
/// it on every repaint, so a battleground whose `MinimapIconScale` is not 1 draws bigger icons on
/// the battle map and identical ones on the world map.
#[test]
fn the_poi_pool_grows_from_the_landmarks_and_parks_its_tail() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);
    assert_eq!(s.eval::<i64>("return NUM_BATTLEFIELDMAP_POIS").unwrap(), 0);

    let landmark = |name: &str, icon: u32, uv: (f32, f32)| WorldMapLandmarkView {
        name: name.into(),
        description: String::new(),
        texture_index: icon,
        uv,
    };
    // Icon 6 is `ICON_POI_REDFLAG`, icon 9 the second row's first cell — two different cells of
    // the 8×8 `POIIcons` atlas, so a hard-coded crop cannot pass both.
    s.set_world_map_landmarks(vec![
        landmark("Silverwing Flag", 6, (0.25, 0.5)),
        landmark("Warsong Flag", 9, (0.75, 0.25)),
    ]);
    s.tick(0.0);

    assert_eq!(s.eval::<i64>("return NUM_BATTLEFIELDMAP_POIS").unwrap(), 2);
    assert!(shown(&s, "BattlefieldMinimapPOI1") && shown(&s, "BattlefieldMinimapPOI2"));
    // Cell 6 = column 6, row 0; cell 9 = column 1, row 1. `coordIncrement` is 16/128 = 0.125.
    let cell = |s: &UiScript, n: u32| -> (f64, f64) {
        let (l, t, ..): (f64, f64, f64, f64, f64, f64, f64, f64) = s
            .eval(&format!(
                "return BattlefieldMinimapPOI{n}Texture:GetTexCoord()"
            ))
            .unwrap();
        (l, t)
    };
    assert_eq!(cell(&s, 1), (0.75, 0.0));
    assert_eq!(cell(&s, 2), (0.125, 0.125));
    // Seated CENTER against the window's TOPLEFT, x × its width and y × MINUS its height — v runs
    // down the sheet where frame y runs up, and the sign is what mirrors every icon if it is wrong.
    assert_eq!(
        s.eval::<(String, String, String, f64, f64)>(
            "local p, r, rp, x, y = BattlefieldMinimapPOI1:GetPoint(1) return p, r:GetName(), rp, x, y"
        )
        .unwrap(),
        (
            "CENTER".into(),
            "BattlefieldMinimap".into(),
            "TOPLEFT".into(),
            0.25 * 225.0,
            -0.5 * 150.0
        )
    );
    // The default scale leaves the icon at `DEFAULT_POI_ICON_SIZE`.
    assert_eq!(num(&s, "BattlefieldMinimapPOI1:GetWidth()"), 12.0);

    // A battleground whose map row carries a bigger `MinimapIconScale` grows every icon by it.
    s.set_battlefield_positions(Vec::new(), None, 1.5);
    s.run("BattlefieldMinimap_Update()").unwrap();
    assert_eq!(
        (
            num(&s, "BattlefieldMinimapPOI1:GetWidth()"),
            num(&s, "BattlefieldMinimapPOI1:GetHeight()")
        ),
        (18.0, 18.0),
        "DEFAULT_POI_ICON_SIZE × GetBattlefieldMapIconScale()"
    );

    // One landmark left: the pool keeps both frames, re-seats the first and parks the second.
    s.set_world_map_landmarks(vec![landmark("Warsong Flag", 9, (0.75, 0.25))]);
    s.tick(0.0);
    assert_eq!(
        s.eval::<i64>("return NUM_BATTLEFIELDMAP_POIS").unwrap(),
        2,
        "the pool never shrinks"
    );
    assert!(shown(&s, "BattlefieldMinimapPOI1"));
    assert_eq!(
        cell(&s, 1),
        (0.125, 0.125),
        "slot 1 was re-seated on the surviving landmark, not left on the stale one"
    );
    assert!(!shown(&s, "BattlefieldMinimapPOI2"), "slot 2 parked hidden");
    quiet(&s);
}

/// **The battleground blips** — the four position verbs the window polls every frame, plus the
/// party slots it shares with the world map, driven through the real `<OnUpdate>`.
///
/// The team-member loop is the subtle one: with no raid up, `playerCount` stays 0, so
/// `BattlefieldMinimapRaid<i>` takes `GetBattlefieldPosition(i)` — the raid frames double as the
/// battleground roster. The `(0, 0)` pair is the hide sentinel on every one of these verbs, and
/// the name each row carries is what the blip's tooltip shows for a teammate who is in neither
/// your party nor your raid.
#[test]
fn the_battleground_blips_and_the_flag_follow_the_position_family() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);

    s.set_battlefield_positions(
        vec![
            BattlefieldPositionView {
                uv: (0.25, 0.75),
                name: Some("Alliedguy".into()),
            },
            BattlefieldPositionView {
                uv: (0.0, 0.0),
                name: None,
            },
        ],
        Some(BattlefieldFlagView {
            uv: (0.4, 0.6),
            token: Some("HordeFlag".into()),
        }),
        1.0,
    );
    s.set_world_map_feed(
        Some((1, 1)),
        Some((0.5, 0.25)),
        0.0,
        None,
        vec![Some((0.1, 0.2)), None],
        Vec::new(),
    );
    update(&mut s, 0.1);

    assert!(
        shown(&s, "BattlefieldMinimapRaid1"),
        "the placed teammate shows"
    );
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapRaid1.name")
            .unwrap(),
        "Alliedguy",
        "…carrying the name the tooltip reads for a non-group teammate"
    );
    assert_eq!(
        s.eval::<(String, String, f64, f64)>(
            "local p, r, rp, x, y = BattlefieldMinimapRaid1:GetPoint(1) return p, rp, x, y"
        )
        .unwrap(),
        (
            "CENTER".into(),
            "TOPLEFT".into(),
            0.25 * 225.0,
            -0.75 * 150.0
        )
    );
    assert!(
        !shown(&s, "BattlefieldMinimapRaid2"),
        "the (0,0) teammate hides"
    );
    assert!(
        !shown(&s, "BattlefieldMinimapRaid40"),
        "…and so does every slot past the roster"
    );

    // The party arm reads `GetPlayerMapPosition("party"..i)`, the world map's own feed.
    assert!(shown(&s, "BattlefieldMinimapParty1"));
    assert_eq!(
        s.eval::<(f64, f64)>(
            "local _, _, _, x, y = BattlefieldMinimapParty1:GetPoint(1) return x, y"
        )
        .unwrap(),
        (0.1 * 225.0, -0.2 * 150.0)
    );
    assert!(
        !shown(&s, "BattlefieldMinimapParty2"),
        "a slot with no position hides"
    );

    // The carrier takes flag frame 1, wearing the token's own art; the second frame stays parked.
    assert!(shown(&s, "BattlefieldMinimapFlag1"));
    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapFlag1Texture:GetTexture()")
            .unwrap(),
        "Interface\\WorldStateFrame\\HordeFlag"
    );
    assert!(!shown(&s, "BattlefieldMinimapFlag2"));

    // Hovering a teammate's blip names them — `BattlefieldMinimapUnit_OnEnter` prefers the row's
    // own `name` over `UnitName(unit)`, which is the only way a non-group teammate gets a label.
    let (bx, by) = screen_centre(&mut s, "BattlefieldMinimapRaid1");
    assert_eq!(
        s.hit_test_name(bx, by).as_deref(),
        Some("BattlefieldMinimapRaid1"),
        "the blip is what the cursor is over"
    );
    s.mouse_move(bx, by);
    assert!(s.eval::<bool>("return GameTooltip:IsShown()").unwrap());
    assert_eq!(
        s.eval::<String>("return GameTooltipTextLeft1:GetText()")
            .unwrap(),
        "Alliedguy"
    );
    s.mouse_move(5.0, 5.0);

    // The dropdown's "Show Teammates" off: the party and raid blips go in one pass, before any of
    // the position verbs are consulted — and **the flag carrier stays**. That asymmetry is the
    // reference's own: `Blizzard_BattlefieldMinimap.lua:245-252`'s `not showPlayers` arm hides
    // `BattlefieldMinimapParty1..4` and `BattlefieldMinimapRaid1..40` and nothing else, so the two
    // flag frames simply keep whatever the last showPlayers-on pass left on them. Pinned in both
    // directions on purpose: "hide everything" is the obvious tidy-up and it is not what 1.12 does.
    s.run("BattlefieldMinimapOptions.showPlayers = false")
        .unwrap();
    update(&mut s, 0.1);
    for f in ["BattlefieldMinimapRaid1", "BattlefieldMinimapParty1"] {
        assert!(!shown(&s, f), "{f} hides while teammates are switched off");
    }
    assert!(
        shown(&s, "BattlefieldMinimapFlag1"),
        "…but the carrier's flag is not in that arm's two loops, so it stays up"
    );

    // Back on, and then leaving the battleground — the empty push — hides them for real.
    s.run("BattlefieldMinimapOptions.showPlayers = true")
        .unwrap();
    update(&mut s, 0.1);
    assert!(shown(&s, "BattlefieldMinimapRaid1"));
    s.set_battlefield_positions(Vec::new(), None, 1.0);
    update(&mut s, 0.1);
    assert!(
        !shown(&s, "BattlefieldMinimapRaid1") && !shown(&s, "BattlefieldMinimapFlag1"),
        "leaving the battleground takes the blips with it"
    );
    quiet(&s);
}

/// **The player arrow is the MINI singleton, not the world map's.**
///
/// The two are separate slots in the engine (`Arrow::World` / `Arrow::Mini`), each created once per
/// session by its own `Create…` verb and never freed — so the failure this guards against is the
/// addon's `CreateMiniWorldMapArrowFrame(BattlefieldMinimap)` being answered with the world map's
/// existing arrow, which would leave the battle map with no arrow at all and yank the world map's
/// across the screen on every `PositionMini…`.
///
/// Told apart by the one property that distinguishes them at the renderer: the mini's model scale
/// is `G48 · 10/9` where the world map's is `G48 · 5/3` (`G48 = 1/√(aspect²+1)`, so at 4:3 that is
/// 0.6667 against 1.0). The seat is cross-checked in Lua against the window's own TOPLEFT.
#[test]
fn the_player_arrow_is_the_minis_own_singleton_seated_by_the_update() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    arrow_facts(&mut s);
    open(&mut s);

    // `BattlefieldMinimap_OnLoad` made it: an anonymous `Model` child of the window.
    assert_eq!(
        s.eval::<i64>(
            r#"local n = 0
               for _, c in ipairs({ BattlefieldMinimap:GetChildren() }) do
                   if c:GetObjectType() == "Model" then n = n + 1 end
               end
               return n"#
        )
        .unwrap(),
        1,
        "exactly one arrow pane, parented to the battle map"
    );

    s.set_world_map_feed(
        Some((1, 1)),
        Some((0.25, 0.75)),
        1.25,
        None,
        Vec::new(),
        Vec::new(),
    );
    update(&mut s, 0.1);

    // The seat, read back in Lua — the arrow is anonymous, so it is found among the children by
    // kind. Same CENTER-on-TOPLEFT law as every blip.
    let seat = s
        .eval::<(bool, String, String, f64, f64)>(
            r#"for _, c in ipairs({ BattlefieldMinimap:GetChildren() }) do
                   if c:GetObjectType() == "Model" then
                       local p, r, rp, x, y = c:GetPoint(1)
                       return c:IsShown(), p, rp, x, y
                   end
               end"#,
        )
        .unwrap();
    assert_eq!(
        seat,
        (
            true,
            "CENTER".into(),
            "TOPLEFT".into(),
            0.25 * 225.0,
            -0.75 * 150.0
        )
    );

    // The renderer's side: the arrow model, turned to the player's facing by
    // `UpdateWorldMapArrowFrames`, at the MINI's own model scale, on a rect centred where Lua says.
    let (cx, cy) = arrow_centre(&mut s);
    let pane = s
        .extract()
        .into_iter()
        .find(|q| {
            matches!(&q.content, QuadContent::ModelPane { model: Some(m), .. } if m == ARROW_MODEL)
                && q.rect.is_some_and(|r| {
                    ((r.left + r.right) / 2.0 - cx).abs() < 0.5
                        && ((r.top + r.bottom) / 2.0 - cy).abs() < 0.5
                })
        })
        .expect("the battle map's arrow pane is in the render list");
    let QuadContent::ModelPane {
        facing,
        model_scale,
        ..
    } = pane.content
    else {
        unreachable!()
    };
    assert!(
        (facing - 1.25).abs() < 1e-6,
        "turned to the player's facing, got {facing}"
    );
    assert!(
        (model_scale - 10.0 / 9.0 * 0.6).abs() < 1e-4,
        "the MINI arrow's own `G48 · 10/9` scale — the world map's would be 1.0 here, got \
         {model_scale}"
    );

    // Off the displayed map: the `(0, 0)` sentinel routes through `ShowMiniWorldMapArrowFrame(nil)`
    // and the pane leaves the render list entirely.
    s.set_world_map_feed(Some((1, 1)), None, 0.0, None, Vec::new(), Vec::new());
    update(&mut s, 0.1);
    s.resolve();
    assert!(
        !s.extract().into_iter().any(|q| matches!(
            &q.content,
            QuadContent::ModelPane { model: Some(m), .. } if m == ARROW_MODEL
        ) && q.rect.is_some()),
        "off-map hides the arrow"
    );
    quiet(&s);
}

/// **The tab** — the strip of chat-frame art the window hangs from: it comes and goes with the
/// window, fades in under the cursor on the reference's own three-frame ramp, and refuses to be
/// dragged while the options say the map is locked.
///
/// The hover ramp is worth spelling out because it looks like a bug and is not. Frame 1 takes the
/// `else` arm and only *starts* hovering (it stores the cursor into the globals `CURSOR_OLD_X/Y`
/// and never into `BattlefieldMinimap.oldX/oldy` — the reference's own slip). Frame 2 therefore
/// compares a nil `oldX` against the cursor, misses, and resets `hoverTime` to 0 while finally
/// recording the position. Only frame 3 can accumulate, so the `BATTLEFIELD_TAB_SHOW_DELAY` clock
/// does not start until the third update after the cursor arrives. A "fix" that fades on frame 2
/// goes red here, which is the point.
#[test]
fn the_tab_follows_the_window_fades_in_on_hover_and_gates_its_drag_on_the_lock() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);

    assert_eq!(
        s.eval::<String>("return BattlefieldMinimapTabText:GetText()")
            .unwrap(),
        "Battle Map",
        "the tab wears BATTLEFIELD_MINIMAP, resized by PanelTemplates_TabResize in the OnShow"
    );
    assert_eq!(
        num(&s, "BattlefieldMinimapTab:GetAlpha()"),
        0.0,
        "…and starts invisible: the OnLoad's own SetAlpha(0)"
    );

    // The hover ramp.
    let (mx, my) = screen_centre(&mut s, "BattlefieldMinimap");
    s.mouse_move(mx, my);
    update(&mut s, 0.3);
    assert_eq!(
        s.eval::<(Option<i64>, f64, Option<f64>)>(
            "return BattlefieldMinimap.hover, BattlefieldMinimap.hoverTime, BattlefieldMinimap.oldX"
        )
        .unwrap(),
        (Some(1), 0.0, None),
        "frame 1 only starts hovering — and leaves oldX nil, the reference's own slip"
    );
    update(&mut s, 0.3);
    assert_eq!(
        num(&s, "BattlefieldMinimap.hoverTime"),
        0.0,
        "frame 2 misses the nil oldX and RESTARTS the clock"
    );
    update(&mut s, 0.3);
    assert_eq!(
        s.eval::<(f64, Option<i64>)>(
            "return BattlefieldMinimap.hoverTime, BattlefieldMinimap.hasBeenFaded"
        )
        .unwrap(),
        (0.3, Some(1)),
        "frame 3 finally crosses BATTLEFIELD_TAB_SHOW_DELAY and starts the fade"
    );
    // `UIFrameFadeIn` runs on the fade manager's own update, from 0 to DEFAULT_BATTLEFIELD_TAB_ALPHA
    // over BATTLEFIELD_TAB_FADE_TIME.
    s.tick(0.05);
    let part_way = num(&s, "BattlefieldMinimapTab:GetAlpha()");
    assert!(
        part_way > 0.0 && part_way < 0.75,
        "part way through the 0.15 s fade, got {part_way}"
    );
    s.tick(0.2);
    assert_eq!(num(&s, "BattlefieldMinimapTab:GetAlpha()"), 0.75);

    // The cursor leaves: the else arm fades it back to the alpha it was remembered at.
    s.mouse_move(5.0, 5.0);
    update(&mut s, 0.1);
    assert_eq!(
        s.eval::<Option<i64>>("return BattlefieldMinimap.hover")
            .unwrap(),
        None,
        "the hover state is cleared on the way out"
    );
    s.tick(0.2);
    assert_eq!(num(&s, "BattlefieldMinimapTab:GetAlpha()"), 0.0);

    // The lock. A left click on a locked tab returns before `StartMoving`, and `StartMoving` is
    // what stamps the userPlaced bit (2193), so the bit is the observable in both directions.
    assert!(!s
        .eval::<bool>("return BattlefieldMinimapTab:IsUserPlaced()")
        .unwrap());
    s.run("BattlefieldMinimapTab:Click(\"LeftButton\")")
        .unwrap();
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapTab:IsUserPlaced()")
            .unwrap(),
        "a locked tab does not start a drag"
    );
    s.run("BattlefieldMinimapOptions.locked = false BattlefieldMinimapTab:Click(\"LeftButton\")")
        .unwrap();
    assert!(
        s.eval::<bool>("return BattlefieldMinimapTab:IsUserPlaced()")
            .unwrap(),
        "unlocked, the same click starts one"
    );
    s.run("BattlefieldMinimapTab:StopMovingOrSizing()").unwrap();

    // `PLAYER_LOGOUT` writes a user-placed tab's centre into the saved table and hands the seat
    // back to the file's own anchor; a tab nobody moved clears the row instead.
    s.resolve();
    let centre = s
        .eval::<(f64, f64)>("return BattlefieldMinimapTab:GetCenter()")
        .unwrap();
    s.fire_event("PLAYER_LOGOUT", vec![]);
    assert_eq!(
        s.eval::<(f64, f64)>(
            "return BattlefieldMinimapOptions.position.x, BattlefieldMinimapOptions.position.y"
        )
        .unwrap(),
        centre
    );
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapTab:IsUserPlaced()")
            .unwrap(),
        "…and the bit is handed back, so the row is what restores the seat next login"
    );
    s.fire_event("PLAYER_LOGOUT", vec![]);
    assert_eq!(
        s.eval::<Option<i64>>("return BattlefieldMinimapOptions.position ~= nil and 1 or nil")
            .unwrap(),
        None,
        "a tab nobody placed clears the saved row"
    );

    // The second toggle closes it, and the tab goes with it through the window's own OnHide.
    s.run("ToggleBattlefieldMinimap()").unwrap();
    assert!(!shown(&s, "BattlefieldMinimap") && !shown(&s, "BattlefieldMinimapTab"));
    assert_eq!(
        s.eval::<String>("return SHOW_BATTLEFIELD_MINIMAP").unwrap(),
        "0"
    );

    // And leaving the battleground closes it on the client's own event, not on a toggle: the
    // `PLAYER_ENTERING_WORLD` arm hides a battle map with no battle behind it.
    s.run("ToggleBattlefieldMinimap()").unwrap();
    assert!(shown(&s, "BattlefieldMinimap"));
    s.set_world_state_ui(Vec::new());
    s.tick(0.0);
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    assert!(
        !shown(&s, "BattlefieldMinimap"),
        "no world-state rows and no active queue: the window hides itself"
    );
    quiet(&s);
}

/// **The tab's right-click menu and the opacity slider it opens** — the window's whole options
/// surface, and the only place `BattlefieldMinimapOptions` is written by a player.
///
/// Worth stating what this path does **not** touch, because it is easy to assume otherwise:
/// `BattlefieldMinimap_SetOpacity` reads no CVar. Its input is `OpacityFrameSlider:GetValue()` —
/// ColorPickerFrame.xml's shared vertical slider, which the addon borrows by pointing
/// `OpacityFrame.opacityFunc` at itself — and what persists is the addon's own
/// `SavedVariablesPerCharacter` table plus WorldStateFrame.lua's `RegisterForSave` global
/// `SHOW_BATTLEFIELD_MINIMAP`. There is no battle-map opacity CVar in 1.12 to register or to give
/// a reference default to, so 1804's rule does not reach this window.
///
/// The alpha formula has two tiers and both are pinned: the border and the twelve detail tiles get
/// `1 - value`, while the overlays, the close button and the corner get a further 0.15 off it
/// (and, below 0.15, nothing off at all) — so a regression that applies one tier everywhere shows
/// up as the map art and its explored overlays being the same brightness.
#[test]
fn the_dropdown_and_the_opacity_slider_drive_the_saved_options() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);

    // A real right-click on the tab — `RegisterForClicks` admits `RightButtonUp`, and the handler's
    // first arm opens the menu on the tab's own anchor.
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    assert!(shown(&s, "DropDownList1"), "the menu opened");
    let row = |s: &UiScript, n: u32| -> (String, bool) {
        (
            s.eval(&format!("return DropDownList1Button{n}:GetText()"))
                .unwrap(),
            s.eval(&format!("return DropDownList1Button{n}Check:IsShown()"))
                .unwrap(),
        )
    };
    // Both toggles come up checked off `BattlefieldMinimapDefaults`; the opacity row is an action,
    // so it carries no check.
    assert_eq!(row(&s, 1), ("Show Teammates".into(), true));
    assert_eq!(row(&s, 2), ("Lock Battle Map".into(), true));
    assert_eq!(row(&s, 3), ("Change Opacity".into(), false));

    s.run("DropDownList1Button1:Click()").unwrap();
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapOptions.showPlayers")
            .unwrap(),
        "row 1 flips Show Teammates"
    );
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    assert_eq!(
        row(&s, 1),
        ("Show Teammates".into(), false),
        "…and the menu re-initializes off the new value"
    );
    s.run("DropDownList1Button2:Click()").unwrap();
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapOptions.locked")
            .unwrap(),
        "row 2 flips the lock"
    );

    // Row 3 borrows the shared opacity frame: re-anchored onto the battle map's left edge, with
    // both of its hooks pointed at this window.
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    s.run("DropDownList1Button3:Click()").unwrap();
    assert!(shown(&s, "OpacityFrame"));
    assert_eq!(
        s.eval::<(String, String, String, f64, f64)>(
            "local p, r, rp, x, y = OpacityFrame:GetPoint(1) return p, r:GetName(), rp, x, y"
        )
        .unwrap(),
        (
            "TOPRIGHT".into(),
            "BattlefieldMinimap".into(),
            "TOPLEFT".into(),
            0.0,
            7.0
        )
    );

    // Dragging the slider repaints through `OpacityFrame.opacityFunc` — the slider's own
    // `<OnValueChanged>` is the only caller, so this is the live path, not a direct call.
    s.run("OpacityFrameSlider:SetValue(0.25)").unwrap();
    let alpha = |s: &UiScript, f: &str| num(s, &format!("{f}:GetAlpha()"));
    for f in [
        "BattlefieldMinimapBackground",
        "BattlefieldMinimap1",
        "BattlefieldMinimap12",
    ] {
        assert!(
            (alpha(&s, f) - 0.75).abs() < 1e-6,
            "{f} takes the plain 1 - value"
        );
    }
    for f in [
        "BattlefieldMinimapOverlay1",
        "BattlefieldMinimapCloseButton",
        "BattlefieldMinimapCorner",
    ] {
        assert!(
            (alpha(&s, f) - 0.6).abs() < 1e-6,
            "{f} takes a further 0.15 off, got {}",
            alpha(&s, f)
        );
    }
    // At the bottom of the second tier the subtraction stops rather than going negative.
    s.run("OpacityFrameSlider:SetValue(0.9)").unwrap();
    assert!((alpha(&s, "BattlefieldMinimapBackground") - 0.1).abs() < 1e-6);
    assert!(
        (alpha(&s, "BattlefieldMinimapOverlay1") - 0.1).abs() < 1e-6,
        "under 0.15 the overlays keep the plain alpha"
    );

    // Dismissing the frame runs `saveOpacityFunc`, which is what puts the value in the saved table.
    s.run("OpacityFrameCloseButton:Click()").unwrap();
    assert!(!shown(&s, "OpacityFrame"));
    assert!(
        (num(&s, "BattlefieldMinimapOptions.opacity") - 0.9).abs() < 1e-6,
        "the close saved the slider's value for next login"
    );
    quiet(&s);
}
