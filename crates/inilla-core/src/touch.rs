//! **Touch controls**: the layout the player edits ([`TouchLayout`]) and the gesture engine
//! ([`TouchEngine`]) that turns fingers into game input.
//!
//! A finger is claimed, at the moment it lands, by the first of four owners that wants it:
//!
//! 1. **A HUD element** — a button, an action slot, the radial button. Explicit targets the player
//!    placed, so they win over everything under them. A tap acts on the release, as a key's up
//!    does for an action button; a hold shows what the element does and letting go then does
//!    nothing (iOS's long press), and sliding off cancels. Jump is the exception: it goes up on the
//!    press, and holding it opens the **hub** — the views and the windows, fanned out beside it.
//! 2. **The stock UI** — any mouse-enabled FrameXML frame under the finger (the caller answers
//!    that question). The finger becomes the mouse: hover while it rests (tooltips), a tap is a
//!    left click, a long press is a right click, a drag is a left-button drag (items, spells,
//!    sliders), two fingers are the mouse wheel. Inside the stick's zone a drag moves instead
//!    (the chat sits there), and the long press waits for the finger to lift.
//! 3. **The movement stick's zone** — the stick appears where the thumb lands (a floating stick,
//!    so it costs no screen space when it is not in use).
//! 4. **The world** — a drag is mouse-look, a tap a left click (select), a double tap or a long
//!    press a right click (interact/attack), two fingers pinch-zoom the camera.
//!
//! Positions are logical points, origin top-left, `+y` down — Bevy's touch coordinates.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use serde::{Deserialize, Serialize};

use crate::pad::{
    movement_commands, radial_index, steering_commands, PadBinding, Special, RADIAL_ITEMS,
};

pub type Vec2 = [f32; 2];

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: Vec2) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// An axis-aligned rectangle in points.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_center(c: Vec2, size: Vec2) -> Self {
        Self {
            min: [c[0] - size[0] / 2.0, c[1] - size[1] / 2.0],
            max: [c[0] + size[0] / 2.0, c[1] + size[1] / 2.0],
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p[0] >= self.min[0] && p[0] <= self.max[0] && p[1] >= self.min[1] && p[1] <= self.max[1]
    }

    pub fn center(&self) -> Vec2 {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }

    pub fn size(&self) -> Vec2 {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }

    /// Grow by `pad` on every side (the touch slop around small buttons).
    pub fn inflate(&self, pad: f32) -> Rect {
        Rect {
            min: [self.min[0] - pad, self.min[1] - pad],
            max: [self.max[0] + pad, self.max[1] + pad],
        }
    }
}

/// The screen the layout is placed on: its size and the safe-area insets (notch, Dynamic Island,
/// home indicator), all in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub size: Vec2,
    /// `[top, left, bottom, right]`.
    pub safe: [f32; 4],
    /// The free strip beside the camera housing (the Dynamic Island or the notch) in landscape,
    /// where the system buttons sit; `None` on a screen without one.
    pub strip: Option<Rect>,
}

/// The narrowest camera-side inset that still holds a button.
const STRIP_MIN: f32 = 36.0;

impl Screen {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            size: [w, h],
            safe: [0.0; 4],
            strip: None,
        }
    }

    /// The camera housing is on the left (`true`) or the right: the inset on that side becomes
    /// the strip the system buttons use, when it is wide enough for one.
    pub fn with_camera_on_left(mut self, left: bool) -> Self {
        let [w, h] = self.size;
        let inset = if left { self.safe[1] } else { self.safe[3] };
        self.strip = (inset >= STRIP_MIN).then(|| {
            if left {
                Rect::new([0.0, 0.0], [inset, h])
            } else {
                Rect::new([w - inset, 0.0], [w, h])
            }
        });
        self
    }

    pub fn safe_rect(&self) -> Rect {
        Rect {
            min: [self.safe[1], self.safe[0]],
            max: [self.size[0] - self.safe[3], self.size[1] - self.safe[2]],
        }
    }
}

/// Which point of the safe area an element's offset is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// The anchor's fraction of the safe rect (`0..1` on each axis).
    fn frac(self) -> Vec2 {
        match self {
            Anchor::TopLeft => [0.0, 0.0],
            Anchor::Top => [0.5, 0.0],
            Anchor::TopRight => [1.0, 0.0],
            Anchor::Left => [0.0, 0.5],
            Anchor::Center => [0.5, 0.5],
            Anchor::Right => [1.0, 0.5],
            Anchor::BottomLeft => [0.0, 1.0],
            Anchor::Bottom => [0.5, 1.0],
            Anchor::BottomRight => [1.0, 1.0],
        }
    }
}

/// What a HUD element is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ElementKind {
    /// The movement stick's zone: a finger landing inside spawns the stick under it.
    Stick,
    /// A button bound like a controller button.
    Button(PadBinding),
    /// An action slot (1..=120): shows the slot's icon and cooldown, presses the slot.
    ActionSlot(u32),
    /// Opens the touch radial menu (every window, one gesture away).
    Radial,
    /// Cycles the HUD to the next view.
    NextView,
}

/// One HUD element as the player placed it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// Stable id, for the editor and for merging new defaults into an old layout.
    pub id: String,
    pub kind: ElementKind,
    pub anchor: Anchor,
    /// The element's centre, in points from the anchor point, `+y` down.
    pub offset: Vec2,
    /// `[width, height]` in points at HUD scale 1.
    pub size: Vec2,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    /// A label to draw when there is no icon.
    #[serde(default)]
    pub label: Option<String>,
    /// The views this element shows in; empty = every view.
    #[serde(default)]
    pub views: Vec<String>,
    /// What holding the element does.
    #[serde(default)]
    pub hold: Hold,
    /// Its place in the strip beside the camera housing, when the screen has one: slots 0 and 1
    /// above the housing (0 nearest), 2 and 3 below. Moving the element in the editor frees it.
    #[serde(default)]
    pub strip: Option<u8>,
}

/// The centre of `slot` in `strip`. The camera housing sits at the strip's middle (the Dynamic
/// Island is about 126 points long in landscape), so the slots stack away from it.
fn strip_slot(strip: Rect, slot: u8) -> Vec2 {
    let c = strip.center();
    let d = 88.0 + f32::from(slot % 2) * 52.0;
    [c[0], if slot < 2 { c[1] - d } else { c[1] + d }]
}

/// What holding an element does, beyond a tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Hold {
    /// Say what the element does (its tooltip); letting go then does nothing — iOS's long press.
    #[default]
    Tooltip,
    /// Open the hub: the views on an inner arc and the windows on an outer one, fanned out from
    /// the element toward the middle of the screen. Jump's hold, which 1.12 gives nothing to do.
    Hub,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

impl Element {
    /// Where the element sits on `screen` at `scale` (HUD scale).
    pub fn rect(&self, screen: &Screen, scale: f32) -> Rect {
        if let (Some(slot), Some(strip)) = (self.strip, screen.strip) {
            let side = (self.size[0] * scale).min(strip.size()[0] - 4.0);
            return Rect::from_center(strip_slot(strip, slot), [side, side]);
        }
        let safe = screen.safe_rect();
        let f = self.anchor.frac();
        let anchor = [
            safe.min[0] + f[0] * (safe.max[0] - safe.min[0]),
            safe.min[1] + f[1] * (safe.max[1] - safe.min[1]),
        ];
        let c = [
            anchor[0] + self.offset[0] * scale,
            anchor[1] + self.offset[1] * scale,
        ];
        Rect::from_center(c, [self.size[0] * scale, self.size[1] * scale])
    }

    /// Move the element so its centre lands at `center` (the editor's drag), keeping its anchor.
    pub fn set_center(&mut self, center: Vec2, screen: &Screen, scale: f32) {
        self.strip = None;
        let safe = screen.safe_rect();
        let f = self.anchor.frac();
        let anchor = [
            safe.min[0] + f[0] * (safe.max[0] - safe.min[0]),
            safe.min[1] + f[1] * (safe.max[1] - safe.min[1]),
        ];
        let s = scale.max(0.1);
        self.offset = [(center[0] - anchor[0]) / s, (center[1] - anchor[1]) / s];
    }

    pub fn in_view(&self, view: &str) -> bool {
        self.views.is_empty() || self.views.iter().any(|v| v == view)
    }

    /// The binding a press of this element runs, when it is a pressable element.
    pub fn binding(&self) -> Option<PadBinding> {
        match &self.kind {
            ElementKind::Button(b) => Some(b.clone()),
            ElementKind::ActionSlot(slot) => Some(slot_binding(*slot)),
            _ => None,
        }
    }
}

/// The binding that presses action slot `slot`: the stock button command where one exists (so
/// paging, self-cast and the button's own press/release law apply), else `UseAction`.
pub fn slot_binding(slot: u32) -> PadBinding {
    let cmd = match slot {
        1..=12 => format!("ACTIONBUTTON{slot}"),
        61..=72 => format!("MULTIACTIONBAR1BUTTON{}", slot - 60),
        49..=60 => format!("MULTIACTIONBAR2BUTTON{}", slot - 48),
        25..=36 => format!("MULTIACTIONBAR3BUTTON{}", slot - 24),
        37..=48 => format!("MULTIACTIONBAR4BUTTON{}", slot - 36),
        _ => return PadBinding::Lua(format!("UseAction({slot})")),
    };
    PadBinding::Command(cmd)
}

/// How touches on the world behave.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TouchFeel {
    /// Look sensitivity: mouse travel per point of finger travel.
    pub look_sensitivity: f32,
    pub invert_y: bool,
    /// A look-drag turns the character with the camera (right-button look) rather than orbiting
    /// the camera alone (left-button look).
    pub drag_turns_character: bool,
    /// Milliseconds a still finger waits before a long press (right click).
    pub long_press_ms: u32,
    /// A second tap within this many milliseconds is a double tap (right click on the world).
    pub double_tap_ms: u32,
    /// Finger travel, in points, before a touch counts as a drag.
    pub slop: f32,
    /// Stick radius in points: full deflection at this distance from where the thumb landed.
    pub stick_radius: f32,
    /// Pinch travel, in points of finger-distance change, per zoom notch.
    pub pinch_per_notch: f32,
    /// Two-finger scroll over the UI: points of travel per wheel notch.
    pub scroll_per_notch: f32,
    /// Milliseconds a finger holds a HUD button before its tooltip shows.
    pub tooltip_ms: u32,
    /// Milliseconds a finger holds jump before the hub opens.
    pub hub_ms: u32,
    /// The stick's sideways axis turns the character and the camera (the keyboard's A/D) instead
    /// of strafing (Q/E).
    pub stick_steers: bool,
}

impl Default for TouchFeel {
    fn default() -> Self {
        Self {
            look_sensitivity: 1.6,
            invert_y: false,
            drag_turns_character: true,
            long_press_ms: 450,
            double_tap_ms: 300,
            slop: 9.0,
            stick_radius: 56.0,
            pinch_per_notch: 28.0,
            scroll_per_notch: 24.0,
            tooltip_ms: 500,
            hub_ms: 300,
            stick_steers: false,
        }
    }
}

/// The player's whole touch HUD, persisted as TOML.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TouchLayout {
    /// Bumped when the shipped defaults change shape; an older file gets the new elements merged in.
    pub version: u32,
    pub enabled: bool,
    pub hud_scale: f32,
    pub opacity: f32,
    pub views: Vec<String>,
    pub active_view: String,
    pub feel: TouchFeel,
    pub elements: Vec<Element>,
}

pub const LAYOUT_VERSION: u32 = 2;

impl Default for TouchLayout {
    fn default() -> Self {
        default_layout()
    }
}

/// The system buttons' strip slots in the shipped layout.
const STRIP_SLOTS: [(&str, u8); 3] = [("radial", 0), ("view", 1), ("inilla", 2)];

/// The shipped phone layout. The thumbs rest in the lower corners and everything sits there: the
/// movement stick bottom-left, jump in the bottom-right corner with a ring of six buttons around
/// it — the action slots in the Combat view, travel and social verbs in the others — so the thumb
/// pivots instead of travelling. The three system buttons sit top-centre, clear of the stock
/// player frame (top-left) and minimap (top-right). The centre of the screen carries nothing.
pub fn default_layout() -> TouchLayout {
    use Anchor::*;
    let el = |id: &str,
              kind: ElementKind,
              anchor,
              offset: Vec2,
              size: f32,
              label: Option<&str>,
              views: &[&str]| Element {
        id: id.into(),
        kind,
        anchor,
        offset,
        size: [size, size],
        opacity: 1.0,
        visible: true,
        label: label.map(Into::into),
        views: views.iter().map(|v| v.to_string()).collect(),
        hold: Hold::Tooltip,
        strip: None,
    };
    let button =
        |id: &str, binding: PadBinding, offset: Vec2, size: f32, label: &str, views: &[&str]| {
            el(
                id,
                ElementKind::Button(binding),
                BottomRight,
                offset,
                size,
                Some(label),
                views,
            )
        };
    // The ring around jump (offsets from the bottom-right corner; jump's centre is [-52, -52]).
    const RING: [Vec2; 6] = [
        [-122.0, -122.0],
        [-190.0, -100.0],
        [-100.0, -190.0],
        [-236.0, -40.0],
        [-176.0, -176.0],
        [-40.0, -236.0],
    ];
    let mut elements = vec![
        Element {
            id: "stick".into(),
            kind: ElementKind::Stick,
            anchor: BottomLeft,
            offset: [150.0, -110.0],
            size: [290.0, 210.0],
            opacity: 1.0,
            visible: true,
            label: None,
            views: vec![],
            hold: Hold::Tooltip,
            strip: None,
        },
        button(
            "jump",
            PadBinding::cmd("JUMP"),
            [-52.0, -52.0],
            74.0,
            "Jump",
            &[],
        ),
        button(
            "interact",
            PadBinding::Special(Special::Interact),
            [-146.0, -40.0],
            52.0,
            "Use",
            &["Combat", "Explore", "Social"],
        ),
        button(
            "target",
            PadBinding::cmd("TARGETNEARESTENEMY"),
            [-40.0, -146.0],
            52.0,
            "Tab",
            &["Combat", "Explore", "Social"],
        ),
        el(
            "radial",
            ElementKind::Radial,
            Top,
            [-48.0, 24.0],
            40.0,
            Some("☰"),
            &[],
        ),
        el(
            "view",
            ElementKind::NextView,
            Top,
            [0.0, 24.0],
            40.0,
            Some("View"),
            &[],
        ),
        el(
            "inilla",
            ElementKind::Button(PadBinding::Special(Special::InillaMenu)),
            Top,
            [48.0, 24.0],
            40.0,
            Some("N"),
            &[],
        ),
    ];
    // Holding jump opens the hub; 1.12 gives a held jump nothing else to do.
    if let Some(jump) = elements.iter_mut().find(|e| e.id == "jump") {
        jump.hold = Hold::Hub;
    }
    // Beside the Dynamic Island, where a phone has one: the menu and the view above it, iNilla's
    // own menu below.
    for (id, slot) in STRIP_SLOTS {
        if let Some(e) = elements.iter_mut().find(|e| e.id == id) {
            e.strip = Some(slot);
        }
    }
    for (i, off) in RING.iter().enumerate() {
        let n = i as u32 + 1;
        elements.push(Element {
            id: format!("slot{n}"),
            kind: ElementKind::ActionSlot(n),
            anchor: BottomRight,
            offset: *off,
            size: [50.0, 50.0],
            opacity: 1.0,
            visible: true,
            label: None,
            views: vec!["Combat".into()],
            hold: Hold::Tooltip,
            strip: None,
        });
    }
    let explore = [
        ("autorun", PadBinding::cmd("TOGGLEAUTORUN"), "Run"),
        ("map", PadBinding::cmd("TOGGLEWORLDMAP"), "Map"),
        ("bags", PadBinding::cmd("OPENALLBAGS"), "Bags"),
        ("sit", PadBinding::cmd("SITORSTAND"), "Sit"),
        ("mount", PadBinding::cmd("ACTIONBUTTON12"), "Mount"),
        ("quests", PadBinding::cmd("TOGGLEQUESTLOG"), "Quests"),
    ];
    for (i, (id, binding, label)) in explore.into_iter().enumerate() {
        elements.push(button(id, binding, RING[i], 48.0, label, &["Explore"]));
    }
    let social = [
        ("chat", PadBinding::cmd("OPENCHAT"), "Chat"),
        ("reply", PadBinding::cmd("REPLY"), "Reply"),
        ("wave", PadBinding::Chat("/wave".into()), "Wave"),
        ("sit_social", PadBinding::cmd("SITORSTAND"), "Sit"),
        ("social", PadBinding::cmd("TOGGLESOCIAL"), "Friends"),
        ("follow", PadBinding::cmd("FOLLOWTARGET"), "Follow"),
    ];
    for (i, (id, binding, label)) in social.into_iter().enumerate() {
        elements.push(button(id, binding, RING[i], 48.0, label, &["Social"]));
    }
    TouchLayout {
        version: LAYOUT_VERSION,
        enabled: true,
        hud_scale: 1.0,
        opacity: 0.55,
        views: vec![
            "Combat".into(),
            "Explore".into(),
            "Social".into(),
            "Minimal".into(),
        ],
        active_view: "Combat".into(),
        feel: TouchFeel::default(),
        elements,
    }
}

impl TouchLayout {
    /// Bring a layout saved by an older build up to date: elements the defaults gained since are
    /// added (hidden where the player might not want them), nothing the player moved is touched.
    pub fn upgrade(&mut self) {
        if self.version >= LAYOUT_VERSION {
            return;
        }
        let defaults = default_layout();
        if self.version < 2 {
            // Version 2: holding jump opens the hub, and the system buttons the player left where
            // they were move beside the Dynamic Island.
            if let Some(jump) = self.elements.iter_mut().find(|e| e.id == "jump") {
                jump.hold = Hold::Hub;
            }
            for (id, slot) in STRIP_SLOTS {
                let shipped = defaults.elements.iter().find(|e| e.id == id);
                if let (Some(e), Some(d)) = (self.elements.iter_mut().find(|e| e.id == id), shipped)
                {
                    if e.anchor == d.anchor && e.offset == d.offset {
                        e.strip = Some(slot);
                    }
                }
            }
        }
        for el in defaults.elements {
            if !self.elements.iter().any(|e| e.id == el.id) {
                self.elements.push(el);
            }
        }
        for v in defaults.views {
            if !self.views.contains(&v) {
                self.views.push(v);
            }
        }
        self.version = LAYOUT_VERSION;
    }

    pub fn next_view(&mut self) {
        if self.views.is_empty() {
            return;
        }
        let i = self
            .views
            .iter()
            .position(|v| *v == self.active_view)
            .map_or(0, |i| (i + 1) % self.views.len());
        self.active_view = self.views[i].clone();
    }

    /// The elements showing right now, with their index into [`Self::elements`].
    pub fn shown(&self) -> impl Iterator<Item = (usize, &Element)> {
        self.elements.iter().enumerate().filter(move |(_, e)| {
            let in_view = if self.active_view == "Minimal" {
                is_minimal(e)
            } else {
                e.in_view(&self.active_view)
            };
            e.visible && in_view
        })
    }

    /// The element under `p`, top-most (last drawn) first. The stick zone is not a hit target
    /// here — it is the lowest-priority owner and is tested separately.
    pub fn hit(&self, p: Vec2, screen: &Screen) -> Option<usize> {
        let mut found = None;
        for (i, e) in self.shown() {
            if matches!(e.kind, ElementKind::Stick) {
                continue;
            }
            // A small slop so a thumb that lands on the edge still counts.
            if e.rect(screen, self.hud_scale).inflate(4.0).contains(p) {
                found = Some(i);
            }
        }
        found
    }

    pub fn stick_zone(&self, screen: &Screen) -> Option<Rect> {
        self.shown()
            .find(|(_, e)| matches!(e.kind, ElementKind::Stick))
            .map(|(_, e)| e.rect(screen, self.hud_scale))
    }
}

/// The "Minimal" view keeps only what moving around needs.
fn is_minimal(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Stick | ElementKind::Radial | ElementKind::NextView
    ) || e.id == "jump"
        || e.id == "inilla"
}

/// A mouse button the engine drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

/// One synthetic mouse operation, applied in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseOp {
    /// Put the cursor at this point (logical points).
    MoveTo(Vec2),
    Down(MouseButton),
    Up(MouseButton),
    /// Relative motion in points (mouse-look).
    Motion(Vec2),
    /// Wheel notches, `+` = up/away.
    Wheel(f32),
    /// Nothing is touching the screen any more: the cursor leaves.
    Leave,
}

/// A touch as the platform reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchInput {
    pub id: u64,
    pub phase: Phase,
    pub pos: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
    Canceled,
}

/// What the HUD should draw for the stick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickView {
    pub origin: Vec2,
    pub knob: Vec2,
}

/// The touch radial as the HUD should draw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchRadial {
    pub center: Vec2,
    pub radius: f32,
    pub selected: Option<usize>,
}

/// The windows the hub's outer arc offers, as [`RADIAL_ITEMS`] indices (bags, character,
/// spellbook, talents, quest log, map), before its last entry, "More": the whole radial.
pub const HUB_WINDOWS: [usize; 6] = [1, 0, 2, 3, 4, 5];

/// One entry of the hub.
#[derive(Clone, Debug, PartialEq)]
pub enum HubItem {
    /// Switch the HUD to this view.
    View(String),
    /// Open a window: an index into [`RADIAL_ITEMS`].
    Window(usize),
    /// Every window: the radial menu.
    More,
}

/// The hub while it is open: two arcs of entries fanned out from the element that opened it.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchHub {
    pub center: Vec2,
    /// The radius of the views' arc (`rings[0]`) and of the windows' arc (`rings[1]`).
    pub radii: [f32; 2],
    /// Both arcs run from `start` through `sweep` radians, in screen angles (`+y` down).
    pub start: f32,
    pub sweep: f32,
    pub rings: [Vec<HubItem>; 2],
    /// The entry under the finger, `(ring, index)`.
    pub selected: Option<(usize, usize)>,
}

impl TouchHub {
    /// The hub of an element centred at `center`: a quarter turn and a little more, from the
    /// horizontal toward the vertical that face the middle of the screen, so it opens away from
    /// the edges the element sits by.
    pub fn open(center: Vec2, screen: &Screen, views: &[String], scale: f32) -> Self {
        let mid = screen.safe_rect().center();
        let across = if center[0] > mid[0] { PI } else { 0.0 };
        let up_down = if center[1] > mid[1] {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
        let mut sweep = up_down - across;
        if sweep > PI {
            sweep -= TAU;
        } else if sweep < -PI {
            sweep += TAU;
        }
        let pad = 5f32.to_radians().copysign(sweep);
        let mut windows: Vec<HubItem> = HUB_WINDOWS.iter().map(|&i| HubItem::Window(i)).collect();
        windows.push(HubItem::More);
        Self {
            center,
            radii: [110.0 * scale, 185.0 * scale],
            start: across - pad,
            sweep: sweep + 2.0 * pad,
            rings: [views.iter().cloned().map(HubItem::View).collect(), windows],
            selected: None,
        }
    }

    /// Where entry `i` of `ring` sits.
    pub fn pos(&self, ring: usize, i: usize) -> Vec2 {
        let n = self.rings[ring].len().max(1) as f32;
        let a = self.start + self.sweep * (i as f32 + 0.5) / n;
        let r = self.radii[ring];
        [self.center[0] + a.cos() * r, self.center[1] + a.sin() * r]
    }

    /// The entry a finger at `p` points at: the ring by distance (the views inside the circle
    /// midway between the arcs, the windows outside it), the entry by angle. `None` close to the
    /// centre (letting go there cancels) or off the ends of the arc.
    pub fn pick(&self, p: Vec2) -> Option<(usize, usize)> {
        let d = sub(p, self.center);
        let m = len(d);
        if m < self.radii[0] * 0.5 {
            return None;
        }
        let ring = usize::from(m >= (self.radii[0] + self.radii[1]) / 2.0);
        let n = self.rings[ring].len();
        if n == 0 {
            return None;
        }
        let mut rel = d[1].atan2(d[0]) - (self.start + self.sweep / 2.0);
        while rel > PI {
            rel -= TAU;
        }
        while rel < -PI {
            rel += TAU;
        }
        let t = rel / self.sweep + 0.5;
        let slack = 0.5 / n as f32;
        if !(-slack..=1.0 + slack).contains(&t) {
            return None;
        }
        Some((ring, ((t * n as f32).max(0.0) as usize).min(n - 1)))
    }

    pub fn item(&self, (ring, i): (usize, usize)) -> Option<&HubItem> {
        self.rings.get(ring)?.get(i)
    }
}

/// What the touches ask of the game this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchFrame {
    pub mouse: Vec<MouseOp>,
    pub pressed: Vec<PadBinding>,
    pub released: Vec<PadBinding>,
    pub held: Vec<PadBinding>,
    pub movement: Vec<&'static str>,
    pub stick: Option<StickView>,
    /// Camera zoom notches (`+` = in).
    pub zoom: f32,
    /// Elements currently under a finger (for the pressed highlight).
    pub pressed_elements: Vec<usize>,
    pub radial: Option<TouchRadial>,
    pub radial_pick: Option<usize>,
    /// The player tapped the next-view element.
    pub next_view: bool,
    /// The element whose tooltip a held finger asks for.
    pub tooltip: Option<usize>,
    /// The hub, while it is open.
    pub hub: Option<TouchHub>,
    /// The hub entry picked this frame.
    pub hub_pick: Option<HubItem>,
    /// Any finger down.
    pub active: bool,
}

#[derive(Clone, Debug)]
enum Role {
    Element {
        index: usize,
        binding: Option<PadBinding>,
        /// The element's rect when the finger landed, and when it landed.
        rect: Rect,
        since: f64,
        /// The binding went down on the press: a held command, or jump.
        fired: bool,
        /// The finger slid off the element: letting go does nothing.
        slid_off: bool,
        /// The tooltip showed: letting go does nothing.
        explained: bool,
    },
    Stick {
        origin: Vec2,
        pos: Vec2,
    },
    Ui {
        start: Vec2,
        last: Vec2,
        since: f64,
        dragging: bool,
        long_fired: bool,
        /// Landed in the stick's zone: a drag becomes the stick, and the long press is judged at
        /// the lift, so a thumb that rests on the chat before it moves right-clicks nothing.
        stick: bool,
    },
    World {
        start: Vec2,
        last: Vec2,
        since: f64,
        looking: bool,
        long_fired: bool,
    },
    /// The second finger of a UI scroll or a world pinch; its partner drives the gesture.
    Partner,
    Radial,
    /// The finger that opened the hub, pointing into it.
    Hub,
}

#[derive(Clone, Debug)]
struct Finger {
    role: Role,
    pos: Vec2,
}

/// The gesture engine. Feed it every touch as it arrives, then call [`Self::frame`] once a frame.
#[derive(Clone, Debug, Default)]
pub struct TouchEngine {
    fingers: BTreeMap<u64, Finger>,
    out: TouchFrame,
    /// Last world tap: where and when, for the double tap.
    last_tap: Option<(Vec2, f64)>,
    /// A two-finger gesture in progress: the last distance (pinch) or midpoint y (scroll).
    pinch: Option<f32>,
    scroll: Option<f32>,
    scroll_carry: f32,
    zoom_carry: f32,
    radial: Option<TouchRadial>,
    hub: Option<TouchHub>,
    /// Which mouse button a world look is holding.
    look_button: Option<MouseButton>,
    /// The cursor is somewhere (a finger was down, or a hover rests).
    cursor_shown: bool,
    /// The screen of the last touch, for what opens between touches (the hub).
    screen: Option<Screen>,
    /// Commands that hold for as long as their key does (movement, turning): a button bound to
    /// one presses on the press rather than tapping on the release.
    held_commands: BTreeSet<String>,
    /// Jump waits for the release: the player sits or casts, and a jump would stand them up or
    /// break the cast.
    defer_jump: bool,
}

impl TouchEngine {
    pub fn fingers(&self) -> usize {
        self.fingers.len()
    }

    /// The commands that are held rather than tapped (1.12's movement and turn bindings).
    pub fn set_held_commands(&mut self, names: impl IntoIterator<Item = String>) {
        self.held_commands = names.into_iter().collect();
    }

    /// Make jump wait for the release (while the player sits or casts), or go up on the press.
    pub fn set_defer_jump(&mut self, defer: bool) {
        self.defer_jump = defer;
    }

    fn is_held(&self, b: &PadBinding) -> bool {
        matches!(b, PadBinding::Command(c) if self.held_commands.contains(c))
    }

    /// Take one touch. `ui_hit` answers "is a mouse-enabled UI frame under this point?"; `now` is
    /// seconds on any monotonic clock.
    pub fn input(
        &mut self,
        t: TouchInput,
        layout: &TouchLayout,
        screen: &Screen,
        now: f64,
        ui_hit: &dyn Fn(Vec2) -> bool,
    ) {
        self.screen = Some(*screen);
        match t.phase {
            Phase::Started => self.start(t.id, t.pos, layout, screen, now, ui_hit),
            Phase::Moved => self.moved(t.id, t.pos, layout),
            Phase::Ended => self.end(t.id, t.pos, layout, now, false),
            Phase::Canceled => self.end(t.id, t.pos, layout, now, true),
        }
    }

    fn start(
        &mut self,
        id: u64,
        pos: Vec2,
        layout: &TouchLayout,
        screen: &Screen,
        now: f64,
        ui_hit: &dyn Fn(Vec2) -> bool,
    ) {
        // The touch radial owns the whole screen while it is up.
        if let Some(r) = self.radial.as_mut() {
            r.selected = pick_wedge(r, pos);
            self.fingers.insert(
                id,
                Finger {
                    role: Role::Radial,
                    pos,
                },
            );
            return;
        }
        // A second finger joining a UI touch or a world touch turns it into a two-finger gesture.
        if let Some((&other, _)) = self
            .fingers
            .iter()
            .find(|(_, f)| matches!(f.role, Role::Ui { .. } | Role::World { .. }))
        {
            let other_pos = self.fingers[&other].pos;
            let other_role = self.fingers[&other].role.clone();
            match other_role {
                Role::Ui { dragging, .. } => {
                    if dragging {
                        self.out.mouse.push(MouseOp::Up(MouseButton::Left));
                    }
                    self.scroll = Some((pos[1] + other_pos[1]) / 2.0);
                    self.scroll_carry = 0.0;
                }
                Role::World { looking, .. } => {
                    if looking {
                        self.end_look();
                    }
                    self.pinch = Some(len(sub(pos, other_pos)));
                    self.zoom_carry = 0.0;
                }
                _ => {}
            }
            if let Some(f) = self.fingers.get_mut(&other) {
                // The partner keeps its role but can no longer click or long-press.
                match &mut f.role {
                    Role::Ui {
                        long_fired,
                        dragging,
                        ..
                    } => {
                        *long_fired = true;
                        *dragging = false;
                    }
                    Role::World { long_fired, .. } => *long_fired = true,
                    _ => {}
                }
            }
            self.fingers.insert(
                id,
                Finger {
                    role: Role::Partner,
                    pos,
                },
            );
            return;
        }

        if let Some(index) = layout.hit(pos, screen) {
            let el = &layout.elements[index];
            let role = match &el.kind {
                ElementKind::Radial => {
                    self.radial = Some(radial_for(screen));
                    Role::Radial
                }
                _ => {
                    let binding = el.binding();
                    let fired = match el.hold {
                        // Jump goes up on the press, as its key does — unless the player sits or
                        // casts: then it waits for the release, so holding it for the hub costs
                        // neither the seat nor the cast.
                        Hold::Hub => binding.is_some() && !self.defer_jump,
                        // A held command holds for as long as the finger does.
                        Hold::Tooltip => binding.as_ref().is_some_and(|b| self.is_held(b)),
                    };
                    if fired {
                        if let Some(b) = &binding {
                            self.out.pressed.push(b.clone());
                        }
                    }
                    Role::Element {
                        index,
                        binding,
                        rect: el.rect(screen, layout.hud_scale),
                        since: now,
                        fired,
                        slid_off: false,
                        explained: false,
                    }
                }
            };
            self.fingers.insert(id, Finger { role, pos });
            return;
        }

        let stick_busy = self
            .fingers
            .values()
            .any(|f| matches!(f.role, Role::Stick { .. }));
        let in_stick_zone =
            !stick_busy && layout.stick_zone(screen).is_some_and(|z| z.contains(pos));

        if ui_hit(pos) {
            // Hover first: the tooltip shows while the finger rests; the click comes on release.
            self.out.mouse.push(MouseOp::MoveTo(pos));
            self.cursor_shown = true;
            self.fingers.insert(
                id,
                Finger {
                    role: Role::Ui {
                        start: pos,
                        last: pos,
                        since: now,
                        dragging: false,
                        long_fired: false,
                        stick: in_stick_zone,
                    },
                    pos,
                },
            );
            return;
        }

        if in_stick_zone {
            self.fingers.insert(
                id,
                Finger {
                    role: Role::Stick { origin: pos, pos },
                    pos,
                },
            );
            return;
        }

        self.fingers.insert(
            id,
            Finger {
                role: Role::World {
                    start: pos,
                    last: pos,
                    since: now,
                    looking: false,
                    long_fired: false,
                },
                pos,
            },
        );
    }

    fn moved(&mut self, id: u64, pos: Vec2, layout: &TouchLayout) {
        let feel = &layout.feel;
        let Some(f) = self.fingers.get_mut(&id) else {
            return;
        };
        let prev = f.pos;
        f.pos = pos;
        // Decided inside the match, done after it: the finger changes role.
        let mut becomes_stick = None;
        let mut opens_hub = None;
        match &mut f.role {
            Role::Stick { pos: p, .. } => *p = pos,
            Role::Element {
                index,
                rect,
                fired,
                slid_off,
                ..
            } => {
                let hub = layout
                    .elements
                    .get(*index)
                    .is_some_and(|e| e.hold == Hold::Hub);
                let [w, h] = rect.size();
                if hub {
                    // A flick out of the button opens the hub without waiting.
                    if len(sub(pos, rect.center())) > w.max(h) / 2.0 + 24.0 {
                        opens_hub = Some(rect.center());
                    }
                } else if !*fired && !rect.inflate(16.0).contains(pos) {
                    *slid_off = true;
                }
            }
            Role::Ui {
                start,
                last,
                dragging,
                long_fired,
                stick,
                ..
            } => {
                if self.scroll.is_some() {
                    *last = pos;
                } else if *stick && !*dragging && len(sub(pos, *start)) > feel.slop {
                    // In the stick's zone a drag moves: the thumb that landed on the chat walks.
                    becomes_stick = Some(*start);
                } else if !*dragging && !*long_fired && len(sub(pos, *start)) > feel.slop {
                    // A drag: press where the finger landed, then follow it (item pickup, sliders).
                    *dragging = true;
                    self.out.mouse.push(MouseOp::MoveTo(*start));
                    self.out.mouse.push(MouseOp::Down(MouseButton::Left));
                    self.out.mouse.push(MouseOp::MoveTo(pos));
                    *last = pos;
                } else if *dragging || !*long_fired {
                    self.out.mouse.push(MouseOp::MoveTo(pos));
                    *last = pos;
                }
            }
            Role::World {
                start,
                last,
                looking,
                long_fired,
                ..
            } => {
                if self.pinch.is_some() {
                    *last = pos;
                } else if !*looking && !*long_fired && len(sub(pos, *start)) > feel.slop {
                    *looking = true;
                    let start = *start;
                    let button = if feel.drag_turns_character {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    };
                    *last = pos;
                    self.look_button = Some(button);
                    self.out.mouse.push(MouseOp::MoveTo(start));
                    self.out.mouse.push(MouseOp::Down(button));
                    self.cursor_shown = true;
                    let d = sub(pos, start);
                    self.out.mouse.push(MouseOp::Motion(look_delta(d, feel)));
                } else if *looking {
                    let d = sub(pos, prev);
                    *last = pos;
                    self.out.mouse.push(MouseOp::Motion(look_delta(d, feel)));
                }
            }
            Role::Radial => {
                if let Some(r) = self.radial.as_mut() {
                    r.selected = pick_wedge(r, pos);
                }
            }
            Role::Hub => {
                if let Some(h) = self.hub.as_mut() {
                    h.selected = h.pick(pos);
                }
            }
            Role::Partner => {}
        }
        if let Some(origin) = becomes_stick {
            if let Some(f) = self.fingers.get_mut(&id) {
                f.role = Role::Stick { origin, pos };
            }
            self.hide_cursor_if_idle();
        }
        if let Some(center) = opens_hub {
            self.open_hub(id, center, layout);
        }
        self.two_finger(feel);
    }

    /// Open the hub for the element finger `id`: the element's press (jump) ends here, and the
    /// finger points into the hub from now on.
    fn open_hub(&mut self, id: u64, center: Vec2, layout: &TouchLayout) {
        let Some(screen) = self.screen else {
            return;
        };
        let Some(f) = self.fingers.get_mut(&id) else {
            return;
        };
        if let Role::Element {
            binding: Some(b),
            fired: true,
            ..
        } = &f.role
        {
            self.out.released.push(b.clone());
        }
        let mut hub = TouchHub::open(center, &screen, &layout.views, layout.hud_scale);
        hub.selected = hub.pick(f.pos);
        f.role = Role::Hub;
        self.hub = Some(hub);
    }

    /// The pointer goes away once no finger is the mouse any more.
    fn hide_cursor_if_idle(&mut self) {
        let mouse = self
            .fingers
            .values()
            .any(|f| matches!(f.role, Role::Ui { .. } | Role::World { .. }));
        if self.cursor_shown && !mouse {
            self.cursor_shown = false;
            self.out.mouse.push(MouseOp::Leave);
        }
    }

    /// Pinch and scroll, from the two fingers' current positions.
    fn two_finger(&mut self, feel: &TouchFeel) {
        let pts: Vec<Vec2> = self
            .fingers
            .values()
            .filter(|f| matches!(f.role, Role::Ui { .. } | Role::World { .. } | Role::Partner))
            .map(|f| f.pos)
            .collect();
        if pts.len() < 2 {
            return;
        }
        if let Some(last) = self.pinch {
            let d = len(sub(pts[0], pts[1]));
            self.zoom_carry += (d - last) / feel.pinch_per_notch.max(1.0);
            self.pinch = Some(d);
            let whole = self.zoom_carry.trunc();
            if whole != 0.0 {
                self.out.zoom += whole;
                self.zoom_carry -= whole;
            }
        }
        if let Some(last) = self.scroll {
            let y = (pts[0][1] + pts[1][1]) / 2.0;
            // Fingers moving down drag the content down: that is scrolling up, wheel `+`.
            self.scroll_carry += (y - last) / feel.scroll_per_notch.max(1.0);
            self.scroll = Some(y);
            let whole = self.scroll_carry.trunc();
            if whole != 0.0 {
                self.out.mouse.push(MouseOp::Wheel(whole));
                self.scroll_carry -= whole;
            }
        }
    }

    fn end_look(&mut self) {
        if let Some(b) = self.look_button.take() {
            self.out.mouse.push(MouseOp::Up(b));
        }
    }

    fn end(&mut self, id: u64, pos: Vec2, layout: &TouchLayout, now: f64, canceled: bool) {
        let Some(f) = self.fingers.remove(&id) else {
            return;
        };
        match f.role {
            Role::Element {
                index,
                binding,
                fired,
                slid_off,
                explained,
                ..
            } => {
                if fired {
                    if let Some(b) = binding {
                        self.out.released.push(b);
                    }
                } else if !slid_off && !explained && !canceled {
                    match binding {
                        // A tap: the press and the release together, on the lift.
                        Some(b) => {
                            self.out.pressed.push(b.clone());
                            self.out.released.push(b);
                        }
                        None if layout
                            .elements
                            .get(index)
                            .is_some_and(|e| matches!(e.kind, ElementKind::NextView)) =>
                        {
                            self.out.next_view = true;
                        }
                        None => {}
                    }
                }
            }
            Role::Stick { .. } => {}
            Role::Ui {
                start,
                since,
                dragging,
                long_fired,
                stick,
                ..
            } => {
                if dragging {
                    self.out.mouse.push(MouseOp::MoveTo(pos));
                    self.out.mouse.push(MouseOp::Up(MouseButton::Left));
                } else if !long_fired && !canceled && self.scroll.is_none() {
                    // In the stick's zone the long press is judged at the lift.
                    let long =
                        stick && (now - since) * 1000.0 >= f64::from(layout.feel.long_press_ms);
                    let button = if long {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    };
                    click(&mut self.out.mouse, start, button);
                }
            }
            Role::World {
                start,
                looking,
                long_fired,
                ..
            } => {
                if looking {
                    self.end_look();
                } else if !long_fired && !canceled && self.pinch.is_none() {
                    let double = self.last_tap.is_some_and(|(p, t)| {
                        (now - t) * 1000.0 <= f64::from(layout.feel.double_tap_ms)
                            && len(sub(p, start)) < 40.0
                    });
                    if double {
                        click(&mut self.out.mouse, start, MouseButton::Right);
                        self.last_tap = None;
                    } else {
                        click(&mut self.out.mouse, start, MouseButton::Left);
                        self.last_tap = Some((start, now));
                    }
                }
            }
            Role::Partner => {}
            Role::Radial => {
                if let Some(r) = self.radial.take() {
                    let sel = pick_wedge(&r, pos);
                    if sel.is_some() {
                        self.out.radial_pick = sel;
                    } else if len(sub(pos, r.center)) < r.radius * 0.3 {
                        // Released on the hub (a tap on the radial button): keep it up for a tap.
                        self.radial = Some(r);
                    }
                }
            }
            Role::Hub => {
                if let Some(hub) = self.hub.take() {
                    match hub.pick(pos).and_then(|s| hub.item(s)).cloned() {
                        _ if canceled => {}
                        // "More": the whole radial, up until a tap picks from it.
                        Some(HubItem::More) => self.radial = self.screen.as_ref().map(radial_for),
                        Some(item) => self.out.hub_pick = Some(item),
                        None => {}
                    }
                }
            }
        }
        let partners = self
            .fingers
            .values()
            .filter(|f| matches!(f.role, Role::Ui { .. } | Role::World { .. } | Role::Partner))
            .count();
        if partners < 2 {
            self.pinch = None;
            self.scroll = None;
        }
        if self.fingers.is_empty() && self.cursor_shown {
            self.cursor_shown = false;
            self.out.mouse.push(MouseOp::Leave);
        }
    }

    /// Close the touch radial without a pick (another input took over).
    pub fn close_radial(&mut self) {
        self.radial = None;
    }

    /// Drop every finger (the app lost focus, the HUD was switched off).
    pub fn reset(&mut self) {
        let ids: Vec<u64> = self.fingers.keys().copied().collect();
        let layout = TouchLayout::default();
        for id in ids {
            let pos = self.fingers[&id].pos;
            self.end(id, pos, &layout, 0.0, true);
        }
        self.radial = None;
        self.hub = None;
    }

    /// Close the frame: timers (long press, tooltips, the hub), the stick, the held set. Call
    /// once per frame after every [`Self::input`].
    pub fn frame(&mut self, layout: &TouchLayout, now: f64) -> TouchFrame {
        let long = f64::from(layout.feel.long_press_ms) / 1000.0;
        let tip = f64::from(layout.feel.tooltip_ms) / 1000.0;
        let hub_after = f64::from(layout.feel.hub_ms) / 1000.0;
        // A held jump opens the hub first, so the release of its press is in this frame's answer.
        let hubs: Vec<(u64, Vec2)> = self
            .fingers
            .iter()
            .filter_map(|(&id, f)| match &f.role {
                Role::Element {
                    index, rect, since, ..
                } if now - since >= hub_after
                    && layout
                        .elements
                        .get(*index)
                        .is_some_and(|e| e.hold == Hold::Hub) =>
                {
                    Some((id, rect.center()))
                }
                _ => None,
            })
            .collect();
        for (id, center) in hubs {
            self.open_hub(id, center, layout);
        }
        let mut stick = None;
        let mut out = std::mem::take(&mut self.out);
        for f in self.fingers.values_mut() {
            match &mut f.role {
                Role::Stick { origin, pos } => {
                    let r = layout.feel.stick_radius * layout.hud_scale;
                    let d = sub(*pos, *origin);
                    let m = len(d);
                    let knob = if m > r {
                        [origin[0] + d[0] / m * r, origin[1] + d[1] / m * r]
                    } else {
                        *pos
                    };
                    // Screen `+y` is down; the stick's `+y` is forward.
                    let v = [(knob[0] - origin[0]) / r, -(knob[1] - origin[1]) / r];
                    let v = crate::pad::radial_deadzone(v, 0.15);
                    out.movement = if layout.feel.stick_steers {
                        steering_commands(v)
                    } else {
                        movement_commands(v)
                    };
                    stick = Some(StickView {
                        origin: *origin,
                        knob,
                    });
                }
                Role::Ui {
                    start,
                    since,
                    dragging,
                    long_fired,
                    stick: in_zone,
                    ..
                } => {
                    if !*in_zone
                        && !*dragging
                        && !*long_fired
                        && now - *since >= long
                        && self.scroll.is_none()
                    {
                        *long_fired = true;
                        click(&mut out.mouse, *start, MouseButton::Right);
                    }
                }
                Role::World {
                    start,
                    since,
                    looking,
                    long_fired,
                    ..
                } => {
                    if !*looking && !*long_fired && now - *since >= long && self.pinch.is_none() {
                        *long_fired = true;
                        click(&mut out.mouse, *start, MouseButton::Right);
                    }
                }
                Role::Element {
                    index,
                    binding,
                    since,
                    fired,
                    slid_off,
                    explained,
                    ..
                } => {
                    if !*slid_off {
                        out.pressed_elements.push(*index);
                    }
                    if *fired {
                        if let Some(b) = binding {
                            out.held.push(b.clone());
                        }
                    }
                    let tooltip = layout
                        .elements
                        .get(*index)
                        .is_some_and(|e| e.hold == Hold::Tooltip);
                    if tooltip && !*fired && !*slid_off && now - *since >= tip {
                        *explained = true;
                    }
                    if *explained && !*slid_off {
                        out.tooltip = Some(*index);
                    }
                }
                Role::Partner | Role::Radial | Role::Hub => {}
            }
        }
        out.stick = stick;
        out.radial = self.radial;
        out.hub = self.hub.clone();
        out.active = !self.fingers.is_empty();
        out
    }
}

/// The touch radial, centred on the safe area.
fn radial_for(screen: &Screen) -> TouchRadial {
    TouchRadial {
        center: screen.safe_rect().center(),
        radius: (screen.size[1] * 0.36).max(110.0),
        selected: None,
    }
}

fn click(ops: &mut Vec<MouseOp>, at: Vec2, button: MouseButton) {
    ops.push(MouseOp::MoveTo(at));
    ops.push(MouseOp::Down(button));
    ops.push(MouseOp::Up(button));
}

fn look_delta(d: Vec2, feel: &TouchFeel) -> Vec2 {
    let y = if feel.invert_y { -d[1] } else { d[1] };
    [d[0] * feel.look_sensitivity, y * feel.look_sensitivity]
}

/// The wedge of the touch radial under `p`, or `None` on the hub or outside the ring.
fn pick_wedge(r: &TouchRadial, p: Vec2) -> Option<usize> {
    let d = sub(p, r.center);
    let m = len(d);
    if m < r.radius * 0.3 || m > r.radius * 1.35 {
        return None;
    }
    // Screen `+y` is down; the wedge math wants `+y` up.
    Some(radial_index([d[0], -d[1]], RADIAL_ITEMS.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 932.0;
    const H: f32 = 430.0;

    fn screen() -> Screen {
        Screen::new(W, H)
    }

    fn t(id: u64, phase: Phase, x: f32, y: f32) -> TouchInput {
        TouchInput {
            id,
            phase,
            pos: [x, y],
        }
    }

    fn no_ui(_: Vec2) -> bool {
        false
    }

    #[test]
    fn a_thumb_in_the_stick_zone_moves_forward() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        e.input(
            t(1, Phase::Started, 150.0, 330.0),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        e.input(
            t(1, Phase::Moved, 150.0, 270.0),
            &layout,
            &screen(),
            0.1,
            &no_ui,
        );
        let f = e.frame(&layout, 0.1);
        assert_eq!(f.movement, vec!["MOVEFORWARD"]);
        let s = f.stick.unwrap();
        assert_eq!(s.origin, [150.0, 330.0]);
        e.input(
            t(1, Phase::Ended, 150.0, 270.0),
            &layout,
            &screen(),
            0.2,
            &no_ui,
        );
        assert!(e.frame(&layout, 0.2).movement.is_empty());
    }

    #[test]
    fn the_jump_button_presses_and_releases_its_binding() {
        let layout = default_layout();
        let jump = layout.elements.iter().find(|e| e.id == "jump").unwrap();
        let c = jump.rect(&screen(), 1.0).center();
        let mut e = TouchEngine::default();
        e.input(
            t(7, Phase::Started, c[0], c[1]),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        let f = e.frame(&layout, 0.0);
        assert_eq!(f.pressed, vec![PadBinding::cmd("JUMP")]);
        assert_eq!(f.held, vec![PadBinding::cmd("JUMP")]);
        e.input(
            t(7, Phase::Ended, c[0], c[1]),
            &layout,
            &screen(),
            0.1,
            &no_ui,
        );
        assert_eq!(
            e.frame(&layout, 0.1).released,
            vec![PadBinding::cmd("JUMP")]
        );
    }

    #[test]
    fn a_world_tap_clicks_and_a_double_tap_right_clicks() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        let p = [466.0, 200.0];
        e.input(
            t(1, Phase::Started, p[0], p[1]),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        e.input(
            t(1, Phase::Ended, p[0], p[1]),
            &layout,
            &screen(),
            0.05,
            &no_ui,
        );
        let f = e.frame(&layout, 0.05);
        assert!(f.mouse.contains(&MouseOp::Down(MouseButton::Left)));
        e.input(
            t(2, Phase::Started, p[0], p[1]),
            &layout,
            &screen(),
            0.2,
            &no_ui,
        );
        e.input(
            t(2, Phase::Ended, p[0], p[1]),
            &layout,
            &screen(),
            0.25,
            &no_ui,
        );
        let f = e.frame(&layout, 0.25);
        assert!(f.mouse.contains(&MouseOp::Down(MouseButton::Right)));
    }

    #[test]
    fn a_world_drag_is_right_button_look() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        e.input(
            t(1, Phase::Started, 500.0, 200.0),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        e.input(
            t(1, Phase::Moved, 540.0, 200.0),
            &layout,
            &screen(),
            0.05,
            &no_ui,
        );
        let f = e.frame(&layout, 0.05);
        assert_eq!(f.mouse[0], MouseOp::MoveTo([500.0, 200.0]));
        assert_eq!(f.mouse[1], MouseOp::Down(MouseButton::Right));
        assert!(matches!(f.mouse[2], MouseOp::Motion([x, _]) if x > 0.0));
        e.input(
            t(1, Phase::Ended, 540.0, 200.0),
            &layout,
            &screen(),
            0.1,
            &no_ui,
        );
        let f = e.frame(&layout, 0.1);
        assert!(f.mouse.contains(&MouseOp::Up(MouseButton::Right)));
        assert!(
            !f.mouse.contains(&MouseOp::Down(MouseButton::Left)),
            "a drag is not a click"
        );
    }

    #[test]
    fn a_long_press_on_the_ui_is_a_right_click_and_no_left_click_follows() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        let ui = |_: Vec2| true;
        e.input(
            t(1, Phase::Started, 400.0, 100.0),
            &layout,
            &screen(),
            0.0,
            &ui,
        );
        let f = e.frame(&layout, 0.0);
        assert_eq!(
            f.mouse,
            vec![MouseOp::MoveTo([400.0, 100.0])],
            "hover first"
        );
        let f = e.frame(&layout, 0.5);
        assert!(f.mouse.contains(&MouseOp::Down(MouseButton::Right)));
        e.input(
            t(1, Phase::Ended, 400.0, 100.0),
            &layout,
            &screen(),
            0.6,
            &ui,
        );
        let f = e.frame(&layout, 0.6);
        assert!(!f.mouse.contains(&MouseOp::Down(MouseButton::Left)));
        assert!(f.mouse.contains(&MouseOp::Leave));
    }

    #[test]
    fn a_ui_drag_is_a_left_button_drag_from_where_it_landed() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        let ui = |_: Vec2| true;
        e.input(
            t(1, Phase::Started, 400.0, 100.0),
            &layout,
            &screen(),
            0.0,
            &ui,
        );
        e.input(
            t(1, Phase::Moved, 430.0, 100.0),
            &layout,
            &screen(),
            0.05,
            &ui,
        );
        e.input(
            t(1, Phase::Ended, 460.0, 120.0),
            &layout,
            &screen(),
            0.1,
            &ui,
        );
        let f = e.frame(&layout, 0.1);
        let down = f
            .mouse
            .iter()
            .position(|m| *m == MouseOp::Down(MouseButton::Left))
            .unwrap();
        assert_eq!(f.mouse[down - 1], MouseOp::MoveTo([400.0, 100.0]));
        assert!(f.mouse.contains(&MouseOp::Up(MouseButton::Left)));
    }

    #[test]
    fn two_fingers_on_the_world_pinch_zoom() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        e.input(
            t(1, Phase::Started, 400.0, 200.0),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        e.input(
            t(2, Phase::Started, 500.0, 200.0),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        e.input(
            t(2, Phase::Moved, 600.0, 200.0),
            &layout,
            &screen(),
            0.1,
            &no_ui,
        );
        let f = e.frame(&layout, 0.1);
        assert!(f.zoom >= 3.0, "spreading the fingers zooms in: {}", f.zoom);
        e.input(
            t(1, Phase::Ended, 400.0, 200.0),
            &layout,
            &screen(),
            0.2,
            &no_ui,
        );
        e.input(
            t(2, Phase::Ended, 600.0, 200.0),
            &layout,
            &screen(),
            0.2,
            &no_ui,
        );
        let f = e.frame(&layout, 0.2);
        assert!(
            !f.mouse.iter().any(|m| matches!(m, MouseOp::Down(_))),
            "no click after a pinch"
        );
    }

    #[test]
    fn the_radial_picks_the_wedge_it_is_released_on() {
        let layout = default_layout();
        let radial = layout.elements.iter().find(|e| e.id == "radial").unwrap();
        let c = radial.rect(&screen(), 1.0).center();
        let mut e = TouchEngine::default();
        e.input(
            t(1, Phase::Started, c[0], c[1]),
            &layout,
            &screen(),
            0.0,
            &no_ui,
        );
        let f = e.frame(&layout, 0.0);
        let r = f.radial.unwrap();
        // Straight up from the centre is wedge 0.
        let up = [r.center[0], r.center[1] - r.radius];
        e.input(
            t(1, Phase::Moved, up[0], up[1]),
            &layout,
            &screen(),
            0.1,
            &no_ui,
        );
        e.input(
            t(1, Phase::Ended, up[0], up[1]),
            &layout,
            &screen(),
            0.2,
            &no_ui,
        );
        let f = e.frame(&layout, 0.2);
        assert_eq!(f.radial_pick, Some(0));
        assert!(f.radial.is_none());
    }

    #[test]
    fn elements_move_by_their_centre_and_keep_their_anchor() {
        let s = screen();
        let mut el = default_layout().elements[1].clone();
        el.set_center([700.0, 300.0], &s, 1.0);
        let c = el.rect(&s, 1.0).center();
        assert!((c[0] - 700.0).abs() < 1e-3 && (c[1] - 300.0).abs() < 1e-3);
    }

    #[test]
    fn views_filter_elements() {
        let mut layout = default_layout();
        layout.active_view = "Minimal".into();
        let ids: Vec<&str> = layout.shown().map(|(_, e)| e.id.as_str()).collect();
        assert!(ids.contains(&"jump") && ids.contains(&"stick"));
        assert!(!ids.contains(&"slot1") && !ids.contains(&"bags"));
        layout.active_view = "Combat".into();
        let ids: Vec<&str> = layout.shown().map(|(_, e)| e.id.as_str()).collect();
        assert!(ids.contains(&"slot1") && !ids.contains(&"reply"));
    }

    #[test]
    fn action_slots_use_the_stock_button_commands() {
        assert_eq!(slot_binding(3), PadBinding::cmd("ACTIONBUTTON3"));
        assert_eq!(slot_binding(61), PadBinding::cmd("MULTIACTIONBAR1BUTTON1"));
        assert_eq!(slot_binding(100), PadBinding::Lua("UseAction(100)".into()));
    }

    #[test]
    fn the_layout_round_trips_through_toml() {
        let l = default_layout();
        let back: TouchLayout = toml::from_str(&toml::to_string(&l).unwrap()).unwrap();
        assert_eq!(l, back);
    }

    #[test]
    fn default_elements_fit_a_small_phone() {
        // iPhone SE-class landscape (667×375) — nothing may fall off the screen.
        let s = Screen::new(667.0, 375.0);
        let layout = default_layout();
        for e in &layout.elements {
            let r = e.rect(&s, layout.hud_scale);
            assert!(
                r.min[0] >= -1.0 && r.min[1] >= -1.0 && r.max[0] <= 668.0 && r.max[1] <= 376.0,
                "{} is off screen: {r:?}",
                e.id
            );
        }
    }

    fn center_of(layout: &TouchLayout, id: &str) -> (usize, Vec2) {
        let i = layout.elements.iter().position(|e| e.id == id).unwrap();
        (
            i,
            layout.elements[i]
                .rect(&screen(), layout.hud_scale)
                .center(),
        )
    }

    fn feed(e: &mut TouchEngine, layout: &TouchLayout, touch: TouchInput, now: f64) {
        e.input(touch, layout, &screen(), now, &no_ui);
    }

    #[test]
    fn a_thumb_that_lands_on_the_chat_in_the_stick_zone_walks_and_clicks_nothing() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        let chat = |_: Vec2| true;
        e.input(
            t(1, Phase::Started, 150.0, 330.0),
            &layout,
            &screen(),
            0.0,
            &chat,
        );
        e.input(
            t(1, Phase::Moved, 150.0, 270.0),
            &layout,
            &screen(),
            0.1,
            &chat,
        );
        let f = e.frame(&layout, 0.1);
        assert_eq!(f.movement, vec!["MOVEFORWARD"]);
        assert!(!f.mouse.iter().any(|m| matches!(m, MouseOp::Down(_))));
        assert!(
            f.mouse.contains(&MouseOp::Leave),
            "the hover over the chat ends"
        );
    }

    #[test]
    fn a_tap_on_the_chat_in_the_stick_zone_still_clicks() {
        let layout = default_layout();
        let mut e = TouchEngine::default();
        let chat = |_: Vec2| true;
        e.input(
            t(1, Phase::Started, 150.0, 330.0),
            &layout,
            &screen(),
            0.0,
            &chat,
        );
        e.input(
            t(1, Phase::Ended, 150.0, 330.0),
            &layout,
            &screen(),
            0.08,
            &chat,
        );
        let f = e.frame(&layout, 0.08);
        assert!(f.mouse.contains(&MouseOp::Down(MouseButton::Left)));
        assert!(f.movement.is_empty());
    }

    #[test]
    fn in_the_stick_zone_a_rest_then_a_lift_right_clicks_and_a_rest_then_a_drag_walks() {
        let layout = default_layout();
        let chat = |_: Vec2| true;
        let mut e = TouchEngine::default();
        e.input(
            t(1, Phase::Started, 150.0, 330.0),
            &layout,
            &screen(),
            0.0,
            &chat,
        );
        let f = e.frame(&layout, 0.6);
        assert!(
            !f.mouse.iter().any(|m| matches!(m, MouseOp::Down(_))),
            "no timed right click"
        );
        e.input(
            t(1, Phase::Ended, 150.0, 330.0),
            &layout,
            &screen(),
            0.6,
            &chat,
        );
        assert!(e
            .frame(&layout, 0.6)
            .mouse
            .contains(&MouseOp::Down(MouseButton::Right)));

        let mut e = TouchEngine::default();
        e.input(
            t(2, Phase::Started, 150.0, 330.0),
            &layout,
            &screen(),
            0.0,
            &chat,
        );
        e.frame(&layout, 0.6);
        e.input(
            t(2, Phase::Moved, 90.0, 330.0),
            &layout,
            &screen(),
            0.7,
            &chat,
        );
        let f = e.frame(&layout, 0.7);
        assert_eq!(f.movement, vec!["STRAFELEFT"]);
        assert!(!f.mouse.iter().any(|m| matches!(m, MouseOp::Down(_))));
    }

    #[test]
    fn a_hud_button_taps_on_the_release() {
        let layout = default_layout();
        let (_, c) = center_of(&layout, "slot1");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        assert!(e.frame(&layout, 0.0).pressed.is_empty());
        feed(&mut e, &layout, t(1, Phase::Ended, c[0], c[1]), 0.1);
        let f = e.frame(&layout, 0.1);
        assert_eq!(f.pressed, vec![PadBinding::cmd("ACTIONBUTTON1")]);
        assert_eq!(f.released, vec![PadBinding::cmd("ACTIONBUTTON1")]);
    }

    #[test]
    fn holding_a_hud_button_explains_it_and_letting_go_does_nothing() {
        let layout = default_layout();
        let (i, c) = center_of(&layout, "slot2");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        assert_eq!(e.frame(&layout, 0.2).tooltip, None);
        assert_eq!(e.frame(&layout, 0.6).tooltip, Some(i));
        feed(&mut e, &layout, t(1, Phase::Ended, c[0], c[1]), 0.7);
        let f = e.frame(&layout, 0.7);
        assert!(f.pressed.is_empty() && f.released.is_empty() && f.tooltip.is_none());
    }

    #[test]
    fn sliding_off_a_hud_button_cancels_it() {
        let layout = default_layout();
        let (_, c) = center_of(&layout, "slot3");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        feed(&mut e, &layout, t(1, Phase::Moved, c[0] - 90.0, c[1]), 0.1);
        feed(&mut e, &layout, t(1, Phase::Ended, c[0] - 90.0, c[1]), 0.2);
        assert!(e.frame(&layout, 0.2).pressed.is_empty());
    }

    #[test]
    fn a_button_bound_to_a_held_command_holds_from_the_press() {
        let mut layout = default_layout();
        let (i, c) = center_of(&layout, "interact");
        layout.elements[i].kind = ElementKind::Button(PadBinding::cmd("TURNLEFT"));
        let mut e = TouchEngine::default();
        e.set_held_commands(["TURNLEFT".to_string()]);
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        let f = e.frame(&layout, 0.0);
        assert_eq!(f.pressed, vec![PadBinding::cmd("TURNLEFT")]);
        assert_eq!(f.held, vec![PadBinding::cmd("TURNLEFT")]);
        assert_eq!(
            e.frame(&layout, 1.0).tooltip,
            None,
            "a held command is not explained"
        );
    }

    #[test]
    fn holding_jump_opens_the_hub_and_a_slide_picks_a_view() {
        let layout = default_layout();
        let (_, c) = center_of(&layout, "jump");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        let f = e.frame(&layout, 0.0);
        assert_eq!(
            f.pressed,
            vec![PadBinding::cmd("JUMP")],
            "jump goes up on the press"
        );
        assert!(f.hub.is_none());
        let f = e.frame(&layout, 0.35);
        let hub = f.hub.expect("the hub is open");
        assert_eq!(
            f.released,
            vec![PadBinding::cmd("JUMP")],
            "its press ends there"
        );
        let explore = hub.pos(0, 1);
        feed(
            &mut e,
            &layout,
            t(1, Phase::Moved, explore[0], explore[1]),
            0.4,
        );
        assert_eq!(e.frame(&layout, 0.4).hub.unwrap().selected, Some((0, 1)));
        feed(
            &mut e,
            &layout,
            t(1, Phase::Ended, explore[0], explore[1]),
            0.5,
        );
        let f = e.frame(&layout, 0.5);
        assert_eq!(f.hub_pick, Some(HubItem::View("Explore".into())));
        assert!(f.hub.is_none());
    }

    #[test]
    fn a_seated_player_jumps_on_the_release_and_the_hub_costs_no_jump() {
        let layout = default_layout();
        let (_, c) = center_of(&layout, "jump");
        let mut e = TouchEngine::default();
        e.set_defer_jump(true);
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        assert!(e.frame(&layout, 0.05).pressed.is_empty());
        feed(&mut e, &layout, t(1, Phase::Ended, c[0], c[1]), 0.1);
        assert_eq!(
            e.frame(&layout, 0.1).pressed,
            vec![PadBinding::cmd("JUMP")],
            "a tap still jumps"
        );

        feed(&mut e, &layout, t(2, Phase::Started, c[0], c[1]), 1.0);
        let f = e.frame(&layout, 1.4);
        assert!(f.hub.is_some());
        assert!(f.pressed.is_empty() && f.released.is_empty());
        feed(&mut e, &layout, t(2, Phase::Ended, c[0], c[1]), 1.5);
        let f = e.frame(&layout, 1.5);
        assert!(
            f.pressed.is_empty() && f.hub_pick.is_none(),
            "let go at the centre: nothing"
        );
    }

    #[test]
    fn a_flick_out_of_jump_opens_the_hub_at_once() {
        let layout = default_layout();
        let (_, c) = center_of(&layout, "jump");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        feed(
            &mut e,
            &layout,
            t(1, Phase::Moved, c[0] - 80.0, c[1] - 30.0),
            0.05,
        );
        assert!(e.frame(&layout, 0.05).hub.is_some());
    }

    #[test]
    fn the_hub_fits_the_screen_and_more_opens_the_radial() {
        for s in [screen(), Screen::new(667.0, 375.0)] {
            let layout = default_layout();
            let jump = layout.elements.iter().find(|e| e.id == "jump").unwrap();
            let c = jump.rect(&s, 1.0).center();
            let hub = TouchHub::open(c, &s, &layout.views, 1.0);
            for ring in 0..2 {
                for i in 0..hub.rings[ring].len() {
                    let p = hub.pos(ring, i);
                    assert!(
                        p[0] > 20.0
                            && p[1] > 20.0
                            && p[0] < s.size[0] - 20.0
                            && p[1] < s.size[1] - 20.0,
                        "entry {ring}/{i} at {p:?} is off a {:?} screen",
                        s.size
                    );
                    assert_eq!(
                        hub.pick(p),
                        Some((ring, i)),
                        "entry {ring}/{i} picks itself"
                    );
                }
            }
        }
        let layout = default_layout();
        let (_, c) = center_of(&layout, "jump");
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, c[0], c[1]), 0.0);
        let hub = e.frame(&layout, 0.35).hub.unwrap();
        let more = hub.pos(1, hub.rings[1].len() - 1);
        assert_eq!(hub.item(hub.pick(more).unwrap()), Some(&HubItem::More));
        feed(&mut e, &layout, t(1, Phase::Ended, more[0], more[1]), 0.5);
        let f = e.frame(&layout, 0.5);
        assert!(f.radial.is_some() && f.hub_pick.is_none());
    }

    #[test]
    fn a_steering_stick_turns_where_it_would_strafe() {
        let mut layout = default_layout();
        layout.feel.stick_steers = true;
        let mut e = TouchEngine::default();
        feed(&mut e, &layout, t(1, Phase::Started, 150.0, 330.0), 0.0);
        feed(&mut e, &layout, t(1, Phase::Moved, 90.0, 330.0), 0.1);
        assert_eq!(e.frame(&layout, 0.1).movement, vec!["TURNLEFT"]);
    }

    #[test]
    fn an_old_layout_learns_that_holding_jump_opens_the_hub() {
        let mut old = default_layout();
        old.version = 1;
        for e in &mut old.elements {
            e.hold = Hold::Tooltip;
        }
        old.upgrade();
        let jump = old.elements.iter().find(|e| e.id == "jump").unwrap();
        assert_eq!(jump.hold, Hold::Hub);
        assert_eq!(old.version, LAYOUT_VERSION);
    }

    /// An iPhone 15 in landscape: 852×393 points, 59 on each side, 21 for the home indicator.
    fn island_phone(left: bool) -> Screen {
        Screen {
            safe: [0.0, 59.0, 21.0, 59.0],
            ..Screen::new(852.0, 393.0)
        }
        .with_camera_on_left(left)
    }

    #[test]
    fn the_system_buttons_sit_beside_the_island_and_clear_of_it() {
        let layout = default_layout();
        for left in [true, false] {
            let s = island_phone(left);
            let strip = s.strip.expect("a 59-point inset holds a button");
            let rects: Vec<Rect> = ["radial", "view", "inilla"]
                .iter()
                .map(|id| {
                    let e = layout.elements.iter().find(|e| e.id == *id).unwrap();
                    e.rect(&s, layout.hud_scale)
                })
                .collect();
            let island_top = s.size[1] / 2.0 - 63.0;
            let island_bottom = s.size[1] / 2.0 + 63.0;
            for r in &rects {
                assert!(
                    r.min[0] >= strip.min[0] && r.max[0] <= strip.max[0],
                    "{r:?}"
                );
                assert!(r.min[1] >= 0.0 && r.max[1] <= s.size[1], "{r:?}");
                assert!(
                    r.max[1] <= island_top || r.min[1] >= island_bottom,
                    "{r:?} overlaps the island"
                );
            }
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let apart = a.max[1] <= b.min[1] || b.max[1] <= a.min[1];
                    assert!(apart, "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn without_a_strip_the_system_buttons_stay_top_centre_and_a_move_frees_one() {
        let mut layout = default_layout();
        let plain = Screen::new(852.0, 393.0);
        let i = layout.elements.iter().position(|e| e.id == "view").unwrap();
        let r = layout.elements[i].rect(&plain, 1.0);
        assert!((r.center()[0] - 426.0).abs() < 1.0 && r.center()[1] < 60.0);

        let s = island_phone(true);
        layout.elements[i].set_center([300.0, 200.0], &s, 1.0);
        assert_eq!(layout.elements[i].strip, None);
        let moved = layout.elements[i].rect(&s, 1.0).center();
        assert!((moved[0] - 300.0).abs() < 0.5 && (moved[1] - 200.0).abs() < 0.5);
    }

    #[test]
    fn a_narrow_inset_makes_no_strip() {
        let s = Screen {
            safe: [0.0, 20.0, 0.0, 20.0],
            ..Screen::new(667.0, 375.0)
        }
        .with_camera_on_left(true);
        assert_eq!(s.strip, None);
    }
}
