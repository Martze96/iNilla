//! The shopping-compare + chat-link tooltips over the REAL shipped XMLs (decision 0274 P4,
//! re-based by 2202): shift over an item hover seats `ShoppingTooltip1/2` BESIDE the tooltip at
//! `MerchantFrame.xml:63-80`'s own geometry, their armed `SetInventoryItem` renders the byte law's
//! compare shape over the template's own adopted small-font ladder, and `SetItemRef` fills the
//! parked `ItemRefTooltip`.
//!
//! **Why not the paper-doll listener these tests used to drive.** `SHOW_COMPARE_TOOLTIP` (event
//! 377) has zero fire sites in 5875 — `PaperDollFrame.lua:621-640` is dead code there, and the
//! vendor row is the only live consumer of the shopping plates (wow-re
//! `merchant-compare-item-law.md` §8). benilla fired it anyway until 2202, which is why a compare
//! needed the character window open, landed across the screen from the tooltip it compared to, and
//! could be left stale by a hover whose slot listener never answered.
//!
//! **The bag end of that flow is the REFERENCE's own code since 1751.** The hover source used to be
//! benilla's `BenillaBagSlot_OnEnter` on a `BenillaBagFrame`; it is the reference's
//! `ContainerFrameItemButton_OnEnter` on one of its recycled `ContainerFrame1..12` now. Same two
//! calls arm the compare (`GameTooltip:SetOwner` + `SetBagItem`), so what these tests pin is
//! unchanged — but the handler reads `this`, so they drive the mouse
//! ([`super::test_ui::hover`]) instead of calling it.
//!
//! **The doll end went the same way.** `CharacterFrame.xml` and `PaperDollFrame.xml` are the
//! reference's own too now, so `BenillaPaperDollSlot_OnEnter` is gone and the hover is stock
//! `PaperDollItemSlotButton_OnEnter` (`PaperDollFrame.lua:739-764`), which reads `this` as well.
//! The doll-slot names (`CharacterHeadSlot` and kin) are unchanged; the mouse is what reaches them.

use benilla_ui::script::{
    ContainerSlot, ContainerState, InvSlotView, InventorySlots, ItemTemplateView, UiScript,
    UnitState,
};

use super::test_ui::{bag_slot_button, hover, BAG_UI, CHARACTER_UI};

/// The two shared lists overlap heavily — `GlobalStrings`, the fonts, `UIParent`, the tooltip,
/// `Cooldown`, the action bar and `Interface\FrameXML\PaperDollFrame.xml` are in both — and
/// loading one file twice redeclares its frames. So a list is walked once and anything already
/// loaded is skipped, which also keeps the ORDER the first list asked for.
fn load_once(s: &UiScript, seen: &mut Vec<&'static str>, files: &[&'static str]) {
    for f in files {
        if seen.contains(f) {
            continue;
        }
        seen.push(f);
        super::test_ui::load_ui_strict(s, f);
    }
}

/// The window set the compare flow crosses **without a bag window**: the whole character window
/// (the listener's doll slots), plus [`ROUTER_UI`].
///
/// **Needs client data.** It always did — `Interface\FrameXML\ItemRef.xml` has been a chain entry
/// since it was pointed there — but the comment here claimed "deliberately install-free" until
/// 1751 made the claim impossible to miss. Callers open with `wow_data_or_skip!`.
fn harness() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    let mut seen = Vec::new();
    load_once(&s, &mut seen, CHARACTER_UI);
    load_once(&s, &mut seen, &ROUTER_UI);
    s.set_money(0);
    s.set_unit("player", Some(player()));
    s
}

/// The two reference files this harness carries beyond the character window: `MerchantFrame.xml`,
/// the other window the compare flow crosses, and `ItemRef.xml`, which declares the chat-link
/// router's own `ItemRefTooltip`. Both were in this harness's hand-copied list before 1751; the
/// manifest's order is the one kept (`FrameXML.toc` 63 → 77).
const ROUTER_UI: [&str; 4] = [
    "ScrollTemplates.xml", // our scroll kit + the placeholder icon
    "Interface\\FrameXML\\CharacterFrameTemplates.xml",
    "Interface\\FrameXML\\MerchantFrame.xml",
    "Interface\\FrameXML\\ItemRef.xml",
];

/// A level-60 player carrying **both** halves of the race and class pairs: `UnitRace`/`UnitClass`
/// answer `(localized, file)` or `nil, nil` — the binding `zip`s them — and stock
/// `PaperDollFrame_SetLevel` formats all three into `CharacterLevelText` unguarded
/// (`PaperDollFrame.lua:100-104`) on every show of the character window.
fn player() -> UnitState {
    UnitState {
        exists: true,
        level: 60,
        race: Some("Human".into()),
        race_file: Some("Human".into()),
        class: Some("Warrior".into()),
        class_file: Some("WARRIOR".into()),
        ..UnitState::default()
    }
}

/// [`harness`] with the REFERENCE's bag windows (1751) — the hover source. `BAG_UI` is the ordered
/// set a test needs before it can open one; `CHARACTER_UI` leads because the manifest seats the
/// character block above the containers (`FrameXML.toc` 53-58 → 65) and because it carries
/// `PaperDollFrame.xml`, which `MainMenuBarBagButtons` inherits its `BagSlotButtonTemplate` body
/// from.
///
/// **Needs client data**: both lists name chain entries, so callers open with `wow_data_or_skip!`.
fn harness_with_bags() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    let mut seen = Vec::new();
    load_once(&s, &mut seen, CHARACTER_UI);
    load_once(&s, &mut seen, BAG_UI);
    load_once(&s, &mut seen, &ROUTER_UI);
    s.set_money(0);
    s
}

/// An equipped helm in the head slot + a better helm in the backpack, both with templates —
/// the compare pair.
fn seed_items(s: &mut UiScript) {
    let mut inv: InventorySlots = Default::default();
    inv[1] = Some(InvSlotView {
        duration_ms: None,
        already_bound: false,
        bar_placeable: true,
        durability: None,
        flags: 0,
        item_id: 1234,
        icon: Some("Interface\\Icons\\INV_Helmet_01".into()),
        count: 1,
        contents_count: None,
        quality: 2,
        name: Some("Test Helm".into()),
        link: Some("|cff1eff00|Hitem:1234:0:0:0|h[Test Helm]|h|r".into()),
        locked: false,
        equip_slots: vec![1],
        creator: None,
        enchants: Vec::new(),
    });
    s.set_inventory_slots(inv);
    s.set_item_template(
        1234,
        ItemTemplateView {
            name: "Test Helm".into(),
            quality: 2,
            class: 4,
            subclass: 1,
            inventory_type: 1,
            armor: 40,
            description: "Snug.".into(),
            ..Default::default()
        },
    );
    s.set_item_template(
        2000,
        ItemTemplateView {
            name: "Another Helm".into(),
            quality: 3,
            class: 4,
            subclass: 1,
            inventory_type: 1,
            armor: 55,
            ..Default::default()
        },
    );
    let mut slots = std::collections::HashMap::new();
    slots.insert(
        1,
        ContainerSlot {
            duration_ms: None,
            petition: None,
            already_bound: false,
            bar_placeable: true,
            durability: None,
            texture: Some("Interface\\Icons\\INV_Helmet_02".into()),
            count: 1,
            quality: Some(3),
            item_id: 2000,
            link: Some("|cff0070dd|Hitem:2000:0:0:0|h[Another Helm]|h|r".into()),
            locked: false,
            equip_slots: vec![1],
            cooldown: None,
            readable: false,
            creator: None,
            flags: 0,
            enchants: Vec::new(),
        },
    );
    s.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots,
        }),
    );
}

/// Shift over a bag helm: `ShoppingTooltip1` seats at the tooltip's own TOPRIGHT (0, −10),
/// renders gray "Currently Equipped" + the equipped helm through the template's ADOPTED small-font
/// ladder (line 1 = GameFontNormalSmall's 10px face — the engine-created lines of the MAIN tooltip
/// stay on its own faces), the compact cut drops the description, and releasing shift hides the
/// pair. **The character window is irrelevant to all of it** — it is checked both closed and open,
/// because the listener that used to make it matter is dead code in 5875 (2202).
#[test]
fn shift_compare_over_a_bag_item_seats_beside_the_tooltip() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = harness_with_bags();
    s.set_unit("player", Some(player()));
    seed_items(&mut s);

    // Open the bag and hover the helm slot. The button is ASKED for by its own `GetID()` — the
    // reference numbers a window's buttons backwards and recycles the windows (1751), so neither
    // the frame name nor the button index is a property to assume.
    s.run("MainMenuBarBackpackButton:Click()").unwrap();
    s.take_sounds();
    let btn = bag_slot_button(&s, 0, 1);
    hover(&mut s, &btn);
    assert!(s.errors().is_empty(), "hover errors: {:?}", s.errors());

    // Character window CLOSED — and the compare shows anyway, beside the tooltip.
    s.set_modifiers(true, false, false);
    assert!(s.errors().is_empty(), "compare errors: {:?}", s.errors());
    let ok: bool = s
        .eval(
            "local p, rel, rp, x, y = ShoppingTooltip1:GetPoint() \
             return ShoppingTooltip1:IsShown() \
               and ShoppingTooltip1TextLeft1:GetText() == \"Currently Equipped\" \
               and ShoppingTooltip1TextLeft2:GetText() == \"Test Helm\" \
               and rel:GetName() == \"GameTooltip\" \
               and p == \"TOPLEFT\" and rp == \"TOPRIGHT\" and x == 0 and y == -10",
        )
        .unwrap();
    assert!(
        ok,
        "the compare plate seats beside the tooltip with the character window closed"
    );
    s.set_modifiers(false, false, false);

    // Opening the character window changes nothing about where it lands.
    s.run(r#"ToggleCharacter("PaperDollFrame")"#).unwrap();
    s.take_sounds();
    hover(&mut s, &btn);
    s.set_modifiers(true, false, false);
    assert!(s.errors().is_empty(), "compare errors: {:?}", s.errors());
    let ok: bool = s
        .eval(
            "local p, rel = ShoppingTooltip1:GetPoint() \
             return ShoppingTooltip1:IsShown() \
               and ShoppingTooltip1TextLeft2:GetText() == \"Test Helm\" \
               and rel:GetName() == \"GameTooltip\" and p == \"TOPLEFT\"",
        )
        .unwrap();
    assert!(ok, "the open character window does not move the plate");
    // The adopted ladder: the shopping plate's line 1 wears GameFontNormalSmall (10px); the
    // MAIN tooltip's engine-created line 1 keeps the header face — different sizes.
    let ok: bool = s
        .eval(
            "local _, sh = ShoppingTooltip1TextLeft1:GetFont() \
             local _, mh = GameTooltipTextLeft1:GetFont() \
             return sh == 10 and mh > sh",
        )
        .unwrap();
    assert!(
        ok,
        "the template's small-font ladder rides the compare plate"
    );
    // The compact cut: the equipped helm's description never prints on the compare plate.
    let ok: bool = s
        .eval(
            "for i = 1, ShoppingTooltip1:NumLines() do \
               if getglobal(\"ShoppingTooltip1TextLeft\" .. i):GetText() == \"\\\"Snug.\\\"\" then \
                 return false \
               end \
             end \
             return true",
        )
        .unwrap();
    assert!(ok, "compact cut drops the description");
    // One helm slot → exactly one shopping tooltip.
    assert!(
        !s.eval::<bool>("return ShoppingTooltip2:IsShown()").unwrap(),
        "a single-slot item fires one compare"
    );
    // Release hides the pair; the bag hover itself stays.
    s.set_modifiers(false, false, false);
    let ok: bool = s
        .eval("return not ShoppingTooltip1:IsShown() and GameTooltip:IsShown()")
        .unwrap();
    assert!(ok, "release hides the compare, keeps the hover");
    assert!(s.errors().is_empty(), "errors: {:?}", s.errors());
}

/// A chat item link through the ref router: `SetItemRef` shows the parked `ItemRefTooltip`
/// (BOTTOM +80, ANCHOR_PRESERVE keeps the XML seat) with the linked item's law; the corner
/// close button hides it.
#[test]
fn item_ref_tooltip_renders_a_chat_link() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = harness();
    seed_items(&mut s);
    s.run(
        r#"SetItemRef("item:2000", "|cff0070dd|Hitem:2000:0:0:0|h[Another Helm]|h|r", "LeftButton")"#,
    )
    .unwrap();
    assert!(s.errors().is_empty(), "link errors: {:?}", s.errors());
    let ok: bool = s
        .eval(
            "local p, rel, rp, x, y = ItemRefTooltip:GetPoint() \
             return ItemRefTooltip:IsShown() \
               and ItemRefTooltipTextLeft1:GetText() == \"Another Helm\" \
               and p == \"BOTTOM\" and y == 80",
        )
        .unwrap();
    assert!(ok, "the link tooltip shows at its parked seat");
    // The close button (ref ItemRefCloseButton): HideUIPanel drops it.
    s.run("HideUIPanel(ItemRefTooltip)").unwrap();
    assert!(
        !s.eval::<bool>("return ItemRefTooltip:IsShown()").unwrap(),
        "the close path hides the link tooltip"
    );
    assert!(s.errors().is_empty(), "errors: {:?}", s.errors());
}

/// Hovering the EQUIPPED item itself (a paper-doll slot) renders the INSTANCE — the live
/// durability pair off the slot view, never the template's authored max/max (ref
/// PaperDollItemSlotButton_OnEnter l.741: `SetInventoryItem`, not an id/template render;
/// director-caught: broken gear read 100% in the char window while the bag read it right).
/// And shift over the doll slot compares NOTHING — a worn item compared with itself is no
/// comparison: `SetInventoryItem` never arms, and its content clear drops any stale arm left
/// by an earlier bag hover.
#[test]
fn doll_hover_renders_the_live_instance_and_never_self_compares() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = harness_with_bags();
    s.set_unit("player", Some(player()));
    seed_items(&mut s);
    // Break the equipped helm: instance pair (0, 40); the template stays authored-full.
    let mut inv: InventorySlots = Default::default();
    inv[1] = Some(InvSlotView {
        duration_ms: None,
        already_bound: false,
        bar_placeable: true,
        durability: Some((0, 40)),
        flags: 0,
        item_id: 1234,
        icon: Some("Interface\\Icons\\INV_Helmet_01".into()),
        count: 1,
        contents_count: None,
        quality: 2,
        name: Some("Test Helm".into()),
        link: Some("|cff1eff00|Hitem:1234:0:0:0|h[Test Helm]|h|r".into()),
        locked: false,
        equip_slots: vec![1],
        creator: None,
        enchants: Vec::new(),
    });
    s.set_inventory_slots(inv);
    s.set_item_template(
        1234,
        ItemTemplateView {
            name: "Test Helm".into(),
            quality: 2,
            class: 4,
            subclass: 1,
            inventory_type: 1,
            armor: 40,
            max_durability: 40,
            ..Default::default()
        },
    );

    // Arm a compare first through a BAG hover (the stale-arm hazard the doll hover must clear) —
    // the reference's own `ContainerFrameItemButton_OnEnter` since 1751, reached by moving the
    // mouse onto the button rather than by calling the handler.
    s.run("MainMenuBarBackpackButton:Click()").unwrap();
    s.take_sounds();
    let btn = bag_slot_button(&s, 0, 1);
    hover(&mut s, &btn);
    s.run(r#"ToggleCharacter("PaperDollFrame")"#).unwrap();
    s.take_sounds();
    // …and the arm really IS live at this point — asserted, not assumed, because the whole test
    // below is a NEGATIVE and would pass just as well if the bag hover had quietly armed nothing.
    s.set_modifiers(true, false, false);
    assert!(
        s.eval::<bool>("return ShoppingTooltip1:IsShown()").unwrap(),
        "fixture: the bag hover armed a compare, so the doll hover has something to clear"
    );
    s.set_modifiers(false, false, false);

    // The doll hover: the live pair, not the template's 40/40. Stock
    // `PaperDollItemSlotButton_OnEnter` reads `this`, so the mouse is what reaches it — and moving
    // OFF the bag button is part of what this test needs anyway.
    hover(&mut s, "CharacterHeadSlot");
    assert!(s.errors().is_empty(), "hover errors: {:?}", s.errors());
    let found: String = s
        .eval(
            "for i = 1, GameTooltip:NumLines() do \
               local t = getglobal(\"GameTooltipTextLeft\" .. i):GetText() \
               if t and string.find(t, \"Durability\") then return t end \
             end \
             return \"<none>\"",
        )
        .unwrap();
    assert_eq!(
        found, "Durability 0 / 40",
        "the doll hover carries the instance's live pair"
    );

    // Shift over the worn item: no compare — not even off the bag hover's stale arm.
    s.set_modifiers(true, false, false);
    assert!(
        !s.eval::<bool>("return ShoppingTooltip1:IsShown()").unwrap(),
        "a worn item never compares with itself"
    );
    s.set_modifiers(false, false, false);
    assert!(s.errors().is_empty(), "errors: {:?}", s.errors());
}

/// A **quest reward** row, which is where the director found this: shift over an equippable
/// choice shows what is worn in that slot, beside the tooltip, with no character window open —
/// and a reward whose slot is EMPTY shows nothing at all rather than the plate an earlier hover
/// left behind.
///
/// That last clause is the reported symptom, reproduced before it was fixed: a Back-slot cape
/// hover painted `Currently Equipped / Rustmetal Bracers` — a *wrist* plate from an earlier
/// compare — sitting over the tooltip. It survived because the old drive fired
/// `SHOW_COMPARE_TOOLTIP` and left the plate's whole lifecycle to a paper-doll listener that
/// answers only while the character window is visible: with the window closed, nothing hid it and
/// nothing refilled it. The engine owns both plates on every drive now, so the miss path hides.
#[test]
fn shift_compare_over_a_quest_reward_never_shows_a_stale_plate() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = quest_harness();

    // Worn: bracers on the WRIST slot. The BACK slot is empty.
    let mut inv: InventorySlots = Default::default();
    inv[9] = Some(worn_item(7000, "Rustmetal Bracers"));
    s.set_inventory_slots(inv);
    s.set_item_template(7000, armor_template("Rustmetal Bracers", 9));
    s.set_item_template(8000, armor_template("Short Duskbat Cape", 16));
    s.set_item_template(8001, armor_template("Other Bracers", 9));

    // A wrist compare first — the plate this hover must not inherit. Rendered straight onto the
    // main tooltip, because what made the stale plate stick was the tooltip never HIDING between
    // the two renders (a mouse move between buttons hides it and would mask the bug).
    s.run(r#"GameTooltip:SetOwner(UIParent, "ANCHOR_NONE") GameTooltip:BenillaSetItemById(8001)"#)
        .unwrap();
    s.set_modifiers(true, false, false);
    let ok: bool = s
        .eval(
            "return ShoppingTooltip1:IsShown() \
               and ShoppingTooltip1TextLeft2:GetText() == \"Rustmetal Bracers\"",
        )
        .unwrap();
    assert!(ok, "fixture: the wrist compare really is on screen");

    // Now the cape, still holding shift, without the tooltip hiding in between: the back slot is
    // empty, so the plate goes away instead of riding along.
    s.run(r#"GameTooltip:SetOwner(UIParent, "ANCHOR_NONE") GameTooltip:BenillaSetItemById(8000)"#)
        .unwrap();
    assert!(
        !s.eval::<bool>("return ShoppingTooltip1:IsShown()").unwrap(),
        "an empty slot shows no compare, and never the previous hover's plate"
    );

    // And with something worn on the back, the real quest row compares against it — the mouse on
    // the stock reward button, no character window open.
    s.set_modifiers(false, false, false);
    let mut inv: InventorySlots = Default::default();
    inv[9] = Some(worn_item(7000, "Rustmetal Bracers"));
    inv[15] = Some(worn_item(7001, "Old Cloak"));
    s.set_inventory_slots(inv);
    s.set_item_template(7001, armor_template("Old Cloak", 16));
    s.fire_event("QUEST_COMPLETE", vec![]);
    hover(&mut s, "QuestRewardItem1");
    s.set_modifiers(true, false, false);
    assert!(s.errors().is_empty(), "compare errors: {:?}", s.errors());
    let ok: bool = s
        .eval(
            "local p, rel, rp, x, y = ShoppingTooltip1:GetPoint() \
             return not CharacterFrame:IsVisible() \
               and GameTooltipTextLeft1:GetText() == \"Short Duskbat Cape\" \
               and ShoppingTooltip1:IsShown() \
               and ShoppingTooltip1TextLeft1:GetText() == \"Currently Equipped\" \
               and ShoppingTooltip1TextLeft2:GetText() == \"Old Cloak\" \
               and rel:GetName() == \"GameTooltip\" \
               and p == \"TOPLEFT\" and rp == \"TOPRIGHT\" and x == 0 and y == -10",
        )
        .unwrap();
    assert!(
        ok,
        "the quest reward compares against the worn cloak, beside its own tooltip"
    );
    assert!(
        !s.eval::<bool>("return ShoppingTooltip2:IsShown()").unwrap(),
        "one candidate slot fills one plate"
    );

    // Release hides it; the reward hover itself stays.
    s.set_modifiers(false, false, false);
    let ok: bool = s
        .eval("return not ShoppingTooltip1:IsShown() and GameTooltip:IsShown()")
        .unwrap();
    assert!(ok, "release hides the compare, keeps the hover");
    assert!(s.errors().is_empty(), "errors: {:?}", s.errors());
}

/// [`harness`] plus the questgiver window, showing a one-choice reward panel.
fn quest_harness() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    let mut seen = Vec::new();
    load_once(&s, &mut seen, CHARACTER_UI);
    load_once(&s, &mut seen, &ROUTER_UI);
    load_once(
        &s,
        &mut seen,
        &[
            "Interface\\FrameXML\\QuestFrame.xml",
            "Interface\\FrameXML\\QuestLogFrame.xml",
        ],
    );
    s.set_money(0);
    s.set_unit("player", Some(player()));
    s.set_quest(Some(benilla_ui::script::QuestState {
        panel: benilla_ui::script::QuestPanel::Reward,
        title: "A Threat Within".into(),
        body: "Here — have one of the better items I've found.".into(),
        choices: vec![benilla_ui::script::QuestItemView {
            name: Some("Short Duskbat Cape".into()),
            texture: Some("Interface\\Icons\\INV_Misc_Cape_01".into()),
            count: 1,
            quality: 1,
            item_id: 8000,
            usable: true,
            link: Some("|cffffffff|Hitem:8000:0:0:0|h[Short Duskbat Cape]|h|r".into()),
        }],
        ..Default::default()
    }));
    s.fire_event("QUEST_COMPLETE", vec![]);
    s
}

/// One worn armour piece, template-backed by [`armor_template`].
fn worn_item(item_id: u32, name: &str) -> InvSlotView {
    InvSlotView {
        duration_ms: None,
        already_bound: false,
        bar_placeable: true,
        durability: None,
        flags: 0,
        item_id,
        icon: None,
        count: 1,
        contents_count: None,
        quality: 1,
        name: Some(name.into()),
        link: Some(format!("|cffffffff|Hitem:{item_id}:0:0:0|h[{name}]|h|r")),
        locked: false,
        equip_slots: Vec::new(),
        creator: None,
        enchants: Vec::new(),
    }
}

/// An armour template in one `InventoryType`. The CLASS matters: the selection law only compares
/// a worn item of the same item class (wow-re `merchant-compare-item-law.md` §3).
fn armor_template(name: &str, inventory_type: u32) -> ItemTemplateView {
    ItemTemplateView {
        name: name.into(),
        quality: 1,
        class: 4,
        subclass: 1,
        inventory_type,
        armor: 5,
        ..Default::default()
    }
}
