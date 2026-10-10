//! **The controller**: the raw state a platform backend reports ([`PadState`]), the player's
//! mapping ([`PadProfile`]) and the per-frame mapper ([`PadMapper`]) that turns one into the
//! other.
//!
//! The scheme follows WoW Forever's gamepad layout, fitted to 1.12's command set:
//!
//! - **Left stick** moves (`MOVEFORWARD`/`MOVEBACKWARD`/`STRAFELEFT`/`STRAFERIGHT`, eight-way),
//!   **right stick** looks (yaw turns the character, pitch tilts the camera), **L3** autoruns.
//! - **LB / RB** cycle friendly / enemy targets.
//! - **Face buttons and the d-pad** carry the actions. The triggers are *modifiers*, not
//!   actions: holding LT, RT or both switches the eight face/d-pad buttons to another layer, so
//!   32 bindings are in reach without letting go of the sticks.
//! - **Menu** opens the radial menu (every window the desktop client reaches by key), **Options**
//!   the game menu, and a dedicated binding toggles the **virtual cursor** for windows that want
//!   a pointer (vendors, loot, quest dialogs).
//!
//! The mapper knows nothing about how a command runs. It reports which bindings went down, which
//! came up and which are held, and the integration applies each by its dispatch kind (a held
//! movement latch, a Lua body, a host edge). A binding is resolved once, **at press time**: letting
//! go of LT before X still releases the LT-layer binding X pressed, never the base one.

use serde::{Deserialize, Serialize};

/// Every button a gamepad backend reports. Names are positional (Xbox letters): `A` is the south
/// face button on every controller, whatever it is printed as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    L3,
    R3,
    Up,
    Down,
    Left,
    Right,
    Menu,
    Options,
    Home,
}

impl PadButton {
    pub const ALL: [PadButton; 17] = [
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::LB,
        PadButton::RB,
        PadButton::LT,
        PadButton::RT,
        PadButton::L3,
        PadButton::R3,
        PadButton::Up,
        PadButton::Down,
        PadButton::Left,
        PadButton::Right,
        PadButton::Menu,
        PadButton::Options,
        PadButton::Home,
    ];

    /// The eight buttons the trigger layers remap.
    pub const LAYERED: [PadButton; 8] = [
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::Up,
        PadButton::Right,
        PadButton::Down,
        PadButton::Left,
    ];

    pub const fn bit(self) -> u32 {
        1 << (self as u32)
    }

    fn index(self) -> usize {
        self as usize
    }

    /// The label a HUD prints for this button in the given glyph style.
    pub fn glyph(self, style: GlyphStyle) -> &'static str {
        use PadButton::*;
        match (style, self) {
            (GlyphStyle::PlayStation, A) => "×",
            (GlyphStyle::PlayStation, B) => "○",
            (GlyphStyle::PlayStation, X) => "□",
            (GlyphStyle::PlayStation, Y) => "△",
            (GlyphStyle::PlayStation, LB) => "L1",
            (GlyphStyle::PlayStation, RB) => "R1",
            (GlyphStyle::PlayStation, LT) => "L2",
            (GlyphStyle::PlayStation, RT) => "R2",
            (GlyphStyle::Nintendo, A) => "B",
            (GlyphStyle::Nintendo, B) => "A",
            (GlyphStyle::Nintendo, X) => "Y",
            (GlyphStyle::Nintendo, Y) => "X",
            (GlyphStyle::Nintendo, LB) => "L",
            (GlyphStyle::Nintendo, RB) => "R",
            (GlyphStyle::Nintendo, LT) => "ZL",
            (GlyphStyle::Nintendo, RT) => "ZR",
            (_, A) => "A",
            (_, B) => "B",
            (_, X) => "X",
            (_, Y) => "Y",
            (_, LB) => "LB",
            (_, RB) => "RB",
            (_, LT) => "LT",
            (_, RT) => "RT",
            (_, L3) => "L3",
            (_, R3) => "R3",
            (_, Up) => "↑",
            (_, Down) => "↓",
            (_, Left) => "←",
            (_, Right) => "→",
            (_, Menu) => "☰",
            (_, Options) => "View",
            (_, Home) => "Home",
        }
    }
}

impl PadButton {
    /// [`PadButton::glyph`] in characters the game's own fonts carry (Arial Narrow's WGL4 set), for
    /// text the stock interface draws, such as an action button's hotkey.
    pub fn game_glyph(self, style: GlyphStyle) -> &'static str {
        use PadButton::*;
        match (style, self) {
            (GlyphStyle::PlayStation, A) => "\u{d7}",
            (GlyphStyle::PlayStation, Y) => "\u{25b2}",
            (_, Menu) => "Menu",
            (_, Options) => "View",
            (_, Home) => "Home",
            _ => self.glyph(style),
        }
    }
}

impl Layer {
    /// What the player holds for this layer, in the controller's own names (`LT`, `L2`, `ZL`).
    pub fn hold_label(self, style: GlyphStyle) -> String {
        match self {
            Layer::Base => String::new(),
            Layer::Lt => PadButton::LT.glyph(style).to_string(),
            Layer::Rt => PadButton::RT.glyph(style).to_string(),
            Layer::Both => format!(
                "{}+{}",
                PadButton::LT.glyph(style),
                PadButton::RT.glyph(style)
            ),
        }
    }
}

/// How a HUD should label the buttons of the connected controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GlyphStyle {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

impl GlyphStyle {
    /// A best guess from the controller's reported product name. The Backbone One reports itself
    /// with Xbox letters, and so does nearly everything else that is not a DualShock/DualSense
    /// or a Switch pad.
    pub fn from_product(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        if n.contains("dualsense") || n.contains("dualshock") || n.contains("playstation") {
            GlyphStyle::PlayStation
        } else if n.contains("switch") || n.contains("joy-con") || n.contains("nintendo") {
            GlyphStyle::Nintendo
        } else {
            GlyphStyle::Xbox
        }
    }
}

/// One controller's state this frame, as a backend reports it. Stick axes are `-1..=1` with `+y`
/// **up** (GameController's convention); triggers are `0..=1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadState {
    pub connected: bool,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub lt: f32,
    pub rt: f32,
    /// A [`PadButton::bit`] set. Backends may leave `LT`/`RT` unset; the mapper derives them from
    /// the analog values against the profile's threshold.
    pub buttons: u32,
}

impl PadState {
    pub fn pressed(&self, b: PadButton) -> bool {
        self.buttons & b.bit() != 0
    }

    pub fn set(&mut self, b: PadButton, down: bool) {
        if down {
            self.buttons |= b.bit();
        } else {
            self.buttons &= !b.bit();
        }
    }
}

/// Which trigger layer is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Layer {
    #[default]
    Base,
    Lt,
    Rt,
    Both,
}

impl Layer {
    pub const ALL: [Layer; 4] = [Layer::Base, Layer::Lt, Layer::Rt, Layer::Both];

    pub fn label(self) -> &'static str {
        match self {
            Layer::Base => "Base",
            Layer::Lt => "LT",
            Layer::Rt => "RT",
            Layer::Both => "LT+RT",
        }
    }
}

/// Things a button can do that are not a 1.12 binding command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Special {
    /// Hold to open the radial menu, pick with a stick, release to open the window.
    RadialMenu,
    /// Toggle the virtual cursor.
    VirtualCursor,
    /// Interact with the current target (the right-click a mouse player would give it).
    Interact,
    /// A left click at the virtual cursor (cursor mode's A).
    LeftClick,
    /// A right click at the virtual cursor (cursor mode's B).
    RightClick,
    /// Show or hide the touch controls.
    ToggleTouchHud,
    /// Open the iNilla settings.
    InillaMenu,
}

/// What one button is bound to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum PadBinding {
    None,
    /// A 1.12 binding command by its `Bindings.xml` name (`JUMP`, `ACTIONBUTTON1`).
    Command(String),
    /// A Lua chunk, run on press.
    Lua(String),
    /// A chat line as if typed (`/sit`, `/follow`).
    Chat(String),
    Special(Special),
}

impl PadBinding {
    pub fn cmd(name: &str) -> Self {
        PadBinding::Command(name.to_string())
    }

    pub fn is_none(&self) -> bool {
        matches!(self, PadBinding::None)
    }

    /// A short label for the HUD and the settings list.
    pub fn label(&self) -> String {
        match self {
            PadBinding::None => "—".into(),
            PadBinding::Command(c) => command_label(c),
            PadBinding::Lua(_) => "Lua".into(),
            PadBinding::Chat(c) => c.clone(),
            PadBinding::Special(s) => match s {
                Special::RadialMenu => "Menu".into(),
                Special::VirtualCursor => "Cursor".into(),
                Special::Interact => "Interact".into(),
                Special::LeftClick => "Click".into(),
                Special::RightClick => "R-Click".into(),
                Special::ToggleTouchHud => "Touch HUD".into(),
                Special::InillaMenu => "iNilla".into(),
            },
        }
    }

    /// The action slot (1..=120) this binding presses, when it is an action-button command. The
    /// HUD shows that slot's icon; 1.12's bars map to slots as `ActionButton_GetPagedID` does for
    /// page 1 and the four multibars.
    pub fn action_slot(&self) -> Option<u32> {
        let PadBinding::Command(c) = self else {
            return None;
        };
        let (bar, n) = action_command(c)?;
        // MultiBarBottomLeft = 61.., BottomRight = 49.., Right = 25.., Left = 37..
        let base = match bar {
            ActionBar::Main => 0,
            ActionBar::BottomLeft => 60,
            ActionBar::BottomRight => 48,
            ActionBar::Right => 24,
            ActionBar::Right2 => 36,
        };
        Some(base + n)
    }

    /// The action bar and button this binding presses, when it is an action-button command.
    pub fn action_button(&self) -> Option<(ActionBar, u32)> {
        match self {
            PadBinding::Command(c) => action_command(c),
            _ => None,
        }
    }
}

/// A readable name for a binding command, for HUD labels where no icon exists.
pub fn command_label(cmd: &str) -> String {
    let known = match cmd {
        "JUMP" => "Jump",
        "TARGETNEARESTENEMY" => "Target enemy",
        "TARGETPREVIOUSENEMY" => "Prev. enemy",
        "TARGETNEARESTFRIEND" => "Target friend",
        "TARGETPREVIOUSFRIEND" => "Prev. friend",
        "TARGETSELF" => "Target self",
        "TOGGLEAUTORUN" => "Autorun",
        "SITORSTAND" => "Sit",
        "TOGGLEGAMEMENU" => "Game menu",
        "ATTACKTARGET" => "Attack",
        "TOGGLECHARACTER0" => "Character",
        "TOGGLESPELLBOOK" => "Spellbook",
        "TOGGLETALENTS" => "Talents",
        "TOGGLEQUESTLOG" => "Quest log",
        "TOGGLEWORLDMAP" => "Map",
        "TOGGLESOCIAL" => "Social",
        "OPENALLBAGS" => "Bags",
        "TOGGLEBACKPACK" => "Backpack",
        "OPENCHAT" => "Chat",
        "REPLY" => "Reply",
        "CAMERAZOOMIN" => "Zoom in",
        "CAMERAZOOMOUT" => "Zoom out",
        "NEXTVIEW" => "Next view",
        "ASSISTTARGET" => "Assist",
        "FOLLOWTARGET" => "Follow",
        "TOGGLESHEATH" => "Sheath",
        "PETATTACK" => "Pet attack",
        "SCREENSHOT" => "Screenshot",
        "TOGGLEMINIMAP" => "Minimap",
        "NAMEPLATES" => "Nameplates",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    if let Some((bar, n)) = action_command(cmd) {
        return match bar {
            ActionBar::Main => format!("Action {n}"),
            // `R2·1`, not `R21`, where the bar's own name ends in a digit.
            b if b.short().ends_with(|c: char| c.is_ascii_digit()) => format!("{}·{n}", b.short()),
            b => format!("{}{n}", b.short()),
        };
    }
    cmd.to_string()
}

/// The five 1.12 action bars a binding command can press a button of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionBar {
    /// `ACTIONBUTTON1..12`, the main bar (its page follows the stance and the bar arrows).
    Main,
    /// `MULTIACTIONBAR1`: MultiBarBottomLeft.
    BottomLeft,
    /// `MULTIACTIONBAR2`: MultiBarBottomRight.
    BottomRight,
    /// `MULTIACTIONBAR3`: MultiBarRight.
    Right,
    /// `MULTIACTIONBAR4`: MultiBarLeft, which 1.12's options call "Right Bar 2".
    Right2,
}

impl ActionBar {
    /// The bar as the options window names it.
    pub fn name(self) -> &'static str {
        match self {
            ActionBar::Main => "main bar",
            ActionBar::BottomLeft => "bottom-left bar",
            ActionBar::BottomRight => "bottom-right bar",
            ActionBar::Right => "right bar",
            ActionBar::Right2 => "right bar 2",
        }
    }

    /// Two letters for a label that has no room (`BL3`).
    pub fn short(self) -> &'static str {
        match self {
            ActionBar::Main => "",
            ActionBar::BottomLeft => "BL",
            ActionBar::BottomRight => "BR",
            ActionBar::Right => "R",
            ActionBar::Right2 => "R2",
        }
    }

    /// The stock buttons' frame name before the button number (`MultiBarBottomLeftButton3`).
    pub fn frame_prefix(self) -> &'static str {
        match self {
            ActionBar::Main => "ActionButton",
            ActionBar::BottomLeft => "MultiBarBottomLeftButton",
            ActionBar::BottomRight => "MultiBarBottomRightButton",
            ActionBar::Right => "MultiBarRightButton",
            ActionBar::Right2 => "MultiBarLeftButton",
        }
    }
}

/// `ACTIONBUTTON3` → `(Main, 3)`, `MULTIACTIONBAR1BUTTON5` → `(BottomLeft, 5)`.
pub fn action_command(cmd: &str) -> Option<(ActionBar, u32)> {
    let (bar, n) = if let Some(n) = cmd.strip_prefix("ACTIONBUTTON") {
        (ActionBar::Main, n)
    } else {
        let (bar, n) = cmd.strip_prefix("MULTIACTIONBAR")?.split_once("BUTTON")?;
        let bar = match bar {
            "1" => ActionBar::BottomLeft,
            "2" => ActionBar::BottomRight,
            "3" => ActionBar::Right,
            "4" => ActionBar::Right2,
            _ => return None,
        };
        (bar, n)
    };
    let n: u32 = n.parse().ok()?;
    (1..=12).contains(&n).then_some((bar, n))
}

/// What each stock action button's hotkey text should say while a controller is in use:
/// `(frame name, text)` for every button a row of `profile` presses, such as
/// `("MultiBarBottomLeftButton1", "LT A")`. The main bar's buttons are listed twice, as
/// `ActionButtonN` and as the stance bar's `BonusActionButtonN` that stands in for them. The first
/// row to press a button wins, base layer first.
pub fn hotkey_hints(profile: &PadProfile, style: GlyphStyle) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for layer in Layer::ALL {
        for row in profile.rows.iter().filter(|r| r.layer == layer) {
            let Some((bar, n)) = row.binding.action_button() else {
                continue;
            };
            let hold = layer.hold_label(style);
            let glyph = row.button.game_glyph(style);
            let text = if hold.is_empty() {
                glyph.to_string()
            } else {
                format!("{hold} {glyph}")
            };
            for frame in pressed_frames(bar, n) {
                if !out.iter().any(|(f, _)| *f == frame) {
                    out.push((frame, text.clone()));
                }
            }
        }
    }
    out
}

/// The stock buttons a press of bar button `n` goes to: the bar's own, and for the main bar the
/// stance bar's button that stands in for it.
fn pressed_frames(bar: ActionBar, n: u32) -> Vec<String> {
    let mut frames = vec![format!("{}{n}", bar.frame_prefix())];
    if bar == ActionBar::Main {
        frames.push(format!("BonusActionButton{n}"));
    }
    frames
}

/// Which layer presses each stock action button, `(frame name, layer)` — the same buttons, and the
/// same first-row-wins order, as [`hotkey_hints`] labels.
pub fn hotkey_layers(profile: &PadProfile) -> Vec<(String, Layer)> {
    let mut out: Vec<(String, Layer)> = Vec::new();
    for layer in Layer::ALL {
        for row in profile.rows.iter().filter(|r| r.layer == layer) {
            let Some((bar, n)) = row.binding.action_button() else {
                continue;
            };
            for frame in pressed_frames(bar, n) {
                if !out.iter().any(|(f, _)| *f == frame) {
                    out.push((frame, layer));
                }
            }
        }
    }
    out
}

/// The multi-bars `profile` presses buttons on, which a player needs shown to see what they hold.
pub fn bars_in_use(profile: &PadProfile) -> Vec<ActionBar> {
    let mut bars: Vec<ActionBar> = profile
        .rows
        .iter()
        .filter_map(|r| r.binding.action_button().map(|(b, _)| b))
        .filter(|b| *b != ActionBar::Main)
        .collect();
    bars.sort_by_key(|b| *b as u8);
    bars.dedup();
    bars
}

/// One row of the mapping table: in `layer`, `button` does `binding`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadRow {
    pub layer: Layer,
    pub button: PadButton,
    pub binding: PadBinding,
}

/// The player's controller settings and mapping, persisted as TOML.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadProfile {
    /// Radial stick deadzone, `0..1`.
    pub stick_deadzone: f32,
    /// Trigger travel that counts as "held" for the layer switch.
    pub trigger_threshold: f32,
    /// Full-deflection turn rate, degrees per second.
    pub look_yaw_speed: f32,
    /// Full-deflection camera pitch rate, degrees per second.
    pub look_pitch_speed: f32,
    /// Exponent on the look response past the deadzone (1 = linear, higher = finer near centre).
    pub look_curve: f32,
    pub invert_y: bool,
    /// Full-deflection virtual cursor speed in logical points per second.
    pub cursor_speed: f32,
    /// Swap the A/B roles (Nintendo-style confirm on the east button).
    pub swap_confirm: bool,
    /// Rumble on hits and procs, where the controller can.
    pub rumble: bool,
    /// Enter cursor mode on its own when a window that needs a pointer opens (vendor, loot, …).
    pub auto_cursor: bool,
    pub rows: Vec<PadRow>,
}

impl Default for PadProfile {
    fn default() -> Self {
        Self {
            stick_deadzone: 0.18,
            trigger_threshold: 0.35,
            look_yaw_speed: 200.0,
            look_pitch_speed: 110.0,
            look_curve: 1.6,
            invert_y: false,
            cursor_speed: 900.0,
            swap_confirm: false,
            rumble: true,
            auto_cursor: true,
            rows: default_rows(),
        }
    }
}

/// The shipped mapping: jump and interact on the south/east buttons, the first six action slots
/// on the rest of the base layer, and one multibar per trigger layer — **LT = bottom-left bar,
/// RT = bottom-right bar, LT+RT = right bar** — so "what does LT+X do" is answered by looking at
/// the bar, and the player assigns spells by dragging them onto it as on desktop.
pub fn default_rows() -> Vec<PadRow> {
    use PadButton::*;
    let mut rows = Vec::new();
    let mut put = |layer, button, binding| {
        rows.push(PadRow {
            layer,
            button,
            binding,
        })
    };
    put(Layer::Base, A, PadBinding::cmd("JUMP"));
    put(Layer::Base, B, PadBinding::Special(Special::Interact));
    put(Layer::Base, X, PadBinding::cmd("ACTIONBUTTON1"));
    put(Layer::Base, Y, PadBinding::cmd("ACTIONBUTTON2"));
    put(Layer::Base, Up, PadBinding::cmd("ACTIONBUTTON3"));
    put(Layer::Base, Right, PadBinding::cmd("ACTIONBUTTON4"));
    put(Layer::Base, Down, PadBinding::cmd("ACTIONBUTTON5"));
    put(Layer::Base, Left, PadBinding::cmd("ACTIONBUTTON6"));
    put(Layer::Base, LB, PadBinding::cmd("TARGETNEARESTFRIEND"));
    put(Layer::Base, RB, PadBinding::cmd("TARGETNEARESTENEMY"));
    put(Layer::Base, L3, PadBinding::cmd("TOGGLEAUTORUN"));
    put(Layer::Base, R3, PadBinding::Special(Special::VirtualCursor));
    put(Layer::Base, Menu, PadBinding::Special(Special::RadialMenu));
    put(Layer::Base, Options, PadBinding::cmd("TOGGLEGAMEMENU"));
    for (layer, bar) in [(Layer::Lt, 1), (Layer::Rt, 2), (Layer::Both, 3)] {
        for (i, b) in PadButton::LAYERED.iter().enumerate() {
            put(
                layer,
                *b,
                PadBinding::Command(format!("MULTIACTIONBAR{bar}BUTTON{}", i + 1)),
            );
        }
    }
    // Held with a trigger, the shoulders step backwards through the same target lists.
    put(Layer::Lt, LB, PadBinding::cmd("TARGETPREVIOUSFRIEND"));
    put(Layer::Lt, RB, PadBinding::cmd("TARGETPREVIOUSENEMY"));
    put(Layer::Rt, LB, PadBinding::cmd("TARGETSELF"));
    put(Layer::Rt, RB, PadBinding::cmd("ASSISTTARGET"));
    rows
}

impl PadProfile {
    /// The binding for `button` in `layer`. A button the layer does not remap keeps its base
    /// binding, so LB still targets while LT is held unless the player bound it otherwise.
    pub fn binding(&self, layer: Layer, button: PadButton) -> &PadBinding {
        static NONE: PadBinding = PadBinding::None;
        let find = |l: Layer| {
            self.rows
                .iter()
                .find(|r| r.layer == l && r.button == button)
                .map(|r| &r.binding)
        };
        find(layer)
            .or_else(|| (layer != Layer::Base).then(|| find(Layer::Base)).flatten())
            .unwrap_or(&NONE)
    }

    pub fn set_binding(&mut self, layer: Layer, button: PadButton, binding: PadBinding) {
        if let Some(r) = self
            .rows
            .iter_mut()
            .find(|r| r.layer == layer && r.button == button)
        {
            r.binding = binding;
        } else {
            self.rows.push(PadRow {
                layer,
                button,
                binding,
            });
        }
    }
}

/// The radial menu's entries: every window a desktop player reaches by key, in the order they sit
/// around the ring (clockwise from the top).
pub const RADIAL_ITEMS: &[(&str, &str)] = &[
    ("Character", "TOGGLECHARACTER0"),
    ("Bags", "OPENALLBAGS"),
    ("Spellbook", "TOGGLESPELLBOOK"),
    ("Talents", "TOGGLETALENTS"),
    ("Quest log", "TOGGLEQUESTLOG"),
    ("Map", "TOGGLEWORLDMAP"),
    ("Social", "TOGGLESOCIAL"),
    ("Chat", "OPENCHAT"),
    ("Reputation", "TOGGLECHARACTER2"),
    ("Skills", "TOGGLECHARACTER1"),
    ("Pet", "TOGGLECHARACTER3"),
    ("Game menu", "TOGGLEGAMEMENU"),
];

/// The radial menu as the HUD should draw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialView {
    /// The highlighted entry, if the stick is pointing at one.
    pub selected: Option<usize>,
    /// `true` when it was opened by a tap and stays up until A (pick) or B (cancel).
    pub sticky: bool,
}

/// What the controller asks of the game this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadFrame {
    /// Bindings that went down this frame, resolved in the layer active at the press.
    pub pressed: Vec<PadBinding>,
    /// Bindings that came up this frame (the same values their presses resolved to).
    pub released: Vec<PadBinding>,
    /// Every binding currently held down, for held-kind commands that latch while held.
    pub held: Vec<PadBinding>,
    /// Movement commands the left stick holds this frame (`MOVEFORWARD`, …).
    pub movement: Vec<&'static str>,
    /// The left stick, deadzone-applied, for a HUD that shows it.
    pub move_vector: [f32; 2],
    /// This frame's look, in radians: `[yaw (+ = turn left), pitch (+ = look up)]`.
    pub look: [f32; 2],
    /// This frame's virtual-cursor travel in logical points, `+y` down (screen space).
    pub cursor: [f32; 2],
    pub layer: Layer,
    pub cursor_mode: bool,
    pub radial: Option<RadialView>,
    /// The radial entry picked this frame (index into [`RADIAL_ITEMS`]).
    pub radial_pick: Option<usize>,
    /// Any input at all this frame — the HUD uses it to know a controller is in use.
    pub active: bool,
}

/// Hold time under which a radial button press counts as a tap (sticky menu).
const RADIAL_TAP_SECS: f32 = 0.25;

/// The per-frame mapper. One per controller; feed it every frame, connected or not.
#[derive(Clone, Debug, Default)]
pub struct PadMapper {
    prev: u32,
    /// The binding each button resolved to when it went down.
    bound: [Option<PadBinding>; 17],
    cursor_mode: bool,
    radial: Option<RadialState>,
}

#[derive(Clone, Debug)]
struct RadialState {
    /// The button holding it open (released = pick), or `None` once it is sticky.
    button: Option<PadButton>,
    held_for: f32,
    selected: Option<usize>,
    moved: bool,
}

impl PadMapper {
    pub fn cursor_mode(&self) -> bool {
        self.cursor_mode
    }

    pub fn set_cursor_mode(&mut self, on: bool) {
        self.cursor_mode = on;
    }

    pub fn radial_open(&self) -> bool {
        self.radial.is_some()
    }

    /// Advance one frame. `dt` is the frame time in seconds.
    pub fn update(&mut self, raw: &PadState, profile: &PadProfile, dt: f32) -> PadFrame {
        let mut out = PadFrame::default();
        if !raw.connected {
            // A disconnect releases everything it was holding.
            for slot in self.bound.iter_mut() {
                if let Some(b) = slot.take() {
                    out.released.push(b);
                }
            }
            self.prev = 0;
            self.radial = None;
            return out;
        }

        let mut state = *raw;
        state.set(
            PadButton::LT,
            raw.lt >= profile.trigger_threshold || raw.pressed(PadButton::LT),
        );
        state.set(
            PadButton::RT,
            raw.rt >= profile.trigger_threshold || raw.pressed(PadButton::RT),
        );
        let now = state.buttons;
        let down = now & !self.prev;
        let up = self.prev & !now;
        self.prev = now;

        let layer = match (state.pressed(PadButton::LT), state.pressed(PadButton::RT)) {
            (true, true) => Layer::Both,
            (true, false) => Layer::Lt,
            (false, true) => Layer::Rt,
            (false, false) => Layer::Base,
        };
        out.layer = layer;

        let left = radial_deadzone(state.left, profile.stick_deadzone);
        let right = radial_deadzone(state.right, profile.stick_deadzone);
        out.move_vector = left;
        out.active = now != 0 || left != [0.0; 2] || right != [0.0; 2];

        // ── the radial menu, which owns the right stick and the face buttons while open ──
        if let Some(radial) = self.radial.as_mut() {
            radial.held_for += dt;
            let pointer = if right != [0.0; 2] { right } else { left };
            if magnitude(pointer) > 0.5 {
                radial.selected = Some(radial_index(pointer, RADIAL_ITEMS.len()));
                radial.moved = true;
            }
            let mut close = false;
            match radial.button {
                Some(b) if up & b.bit() != 0 => {
                    if radial.moved || radial.held_for >= RADIAL_TAP_SECS {
                        out.radial_pick = radial.selected;
                        close = true;
                    } else {
                        // A quick tap: the menu stays up until A or B.
                        radial.button = None;
                    }
                }
                None => {
                    let (confirm, cancel) = confirm_cancel(profile);
                    if down & confirm.bit() != 0 {
                        out.radial_pick = radial.selected;
                        close = true;
                    } else if down & (cancel.bit() | PadButton::Menu.bit()) != 0 {
                        close = true;
                    }
                }
                _ => {}
            }
            // D-pad steps the highlight, for players who would rather not aim a stick.
            let n = RADIAL_ITEMS.len();
            if down & (PadButton::Right.bit() | PadButton::Down.bit()) != 0 {
                radial.selected = Some(radial.selected.map_or(0, |s| (s + 1) % n));
            }
            if down & (PadButton::Left.bit() | PadButton::Up.bit()) != 0 {
                radial.selected = Some(radial.selected.map_or(n - 1, |s| (s + n - 1) % n));
            }
            out.radial = Some(RadialView {
                selected: radial.selected,
                sticky: radial.button.is_none(),
            });
            if close {
                self.radial = None;
                out.radial = None;
            }
            // Movement keeps working under the menu; nothing else reaches the game.
            out.movement = movement_commands(left);
            out.cursor_mode = self.cursor_mode;
            return out;
        }

        // ── buttons: resolve at press, release what the press resolved to ──
        for b in PadButton::ALL {
            if matches!(b, PadButton::LT | PadButton::RT | PadButton::Home) {
                continue;
            }
            let i = b.index();
            if down & b.bit() != 0 {
                let binding = self.resolve(b, layer, profile);
                match &binding {
                    PadBinding::Special(Special::RadialMenu) => {
                        self.radial = Some(RadialState {
                            button: Some(b),
                            held_for: 0.0,
                            selected: None,
                            moved: false,
                        });
                        out.radial = Some(RadialView {
                            selected: None,
                            sticky: false,
                        });
                        continue;
                    }
                    PadBinding::Special(Special::VirtualCursor) => {
                        self.cursor_mode = !self.cursor_mode;
                    }
                    _ => {}
                }
                if !binding.is_none() {
                    out.pressed.push(binding.clone());
                    self.bound[i] = Some(binding);
                }
            }
            if up & b.bit() != 0 {
                if let Some(binding) = self.bound[i].take() {
                    out.released.push(binding);
                }
            }
        }
        out.held = self.bound.iter().flatten().cloned().collect();

        // ── sticks ──
        out.movement = movement_commands(left);
        let shaped = |v: f32| v.signum() * v.abs().powf(profile.look_curve.max(0.1));
        if self.cursor_mode {
            out.cursor = [
                shaped(right[0]) * profile.cursor_speed * dt,
                -shaped(right[1]) * profile.cursor_speed * dt,
            ];
        } else {
            let pitch_sign = if profile.invert_y { -1.0 } else { 1.0 };
            out.look = [
                -shaped(right[0]) * profile.look_yaw_speed.to_radians() * dt,
                pitch_sign * shaped(right[1]) * profile.look_pitch_speed.to_radians() * dt,
            ];
        }
        out.cursor_mode = self.cursor_mode;
        out
    }

    fn resolve(&self, b: PadButton, layer: Layer, profile: &PadProfile) -> PadBinding {
        if self.cursor_mode {
            let (confirm, cancel) = confirm_cancel(profile);
            if b == confirm {
                return PadBinding::Special(Special::LeftClick);
            }
            if b == cancel {
                return PadBinding::Special(Special::RightClick);
            }
        }
        profile.binding(layer, b).clone()
    }
}

fn confirm_cancel(profile: &PadProfile) -> (PadButton, PadButton) {
    if profile.swap_confirm {
        (PadButton::B, PadButton::A)
    } else {
        (PadButton::A, PadButton::B)
    }
}

fn magnitude(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// A radial deadzone, rescaled so the output starts at 0 just past the edge and reaches 1 at full
/// deflection (no dead band, no jump).
pub fn radial_deadzone(v: [f32; 2], dz: f32) -> [f32; 2] {
    let m = magnitude(v);
    if m <= dz || m == 0.0 {
        return [0.0, 0.0];
    }
    let scaled = ((m - dz) / (1.0 - dz).max(1e-3)).min(1.0);
    [v[0] / m * scaled, v[1] / m * scaled]
}

/// Eight-way movement from a (deadzone-applied) stick: a direction holds when its component is
/// past `sin 22.5°` of the stick's heading, so each of the eight sectors is 45° wide.
pub fn movement_commands(v: [f32; 2]) -> Vec<&'static str> {
    let m = magnitude(v);
    if m == 0.0 {
        return Vec::new();
    }
    let (x, y) = (v[0] / m, v[1] / m);
    const S: f32 = 0.382_683_43; // sin(22.5°)
    let mut out = Vec::new();
    if y > S {
        out.push("MOVEFORWARD");
    } else if y < -S {
        out.push("MOVEBACKWARD");
    }
    if x > S {
        out.push("STRAFERIGHT");
    } else if x < -S {
        out.push("STRAFELEFT");
    }
    out
}

/// [`movement_commands`] with the sideways axis turning instead of strafing: the keyboard's A/D,
/// which turn the character and the camera with it (1.12's own turn/strafe model).
pub fn steering_commands(v: [f32; 2]) -> Vec<&'static str> {
    movement_commands(v)
        .into_iter()
        .map(|c| match c {
            "STRAFELEFT" => "TURNLEFT",
            "STRAFERIGHT" => "TURNRIGHT",
            other => other,
        })
        .collect()
}

/// Which of `n` equal wedges a direction points into, wedge 0 centred on straight up, clockwise.
pub fn radial_index(v: [f32; 2], n: usize) -> usize {
    let n = n.max(1);
    // atan2 of (x, y) is the clockwise angle from +y.
    let mut a = v[0].atan2(v[1]);
    if a < 0.0 {
        a += std::f32::consts::TAU;
    }
    let wedge = std::f32::consts::TAU / n as f32;
    (((a + wedge / 2.0) / wedge) as usize) % n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(buttons: &[PadButton]) -> PadState {
        let mut s = PadState {
            connected: true,
            ..Default::default()
        };
        for b in buttons {
            s.set(*b, true);
        }
        s
    }

    #[test]
    fn a_press_resolves_in_its_layer_and_releases_the_same_binding() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        let mut s = pad(&[]);
        s.lt = 1.0;
        m.update(&s, &profile, 0.016);
        s.set(PadButton::X, true);
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.layer, Layer::Lt);
        assert_eq!(f.pressed, vec![PadBinding::cmd("MULTIACTIONBAR1BUTTON3")]);
        // Let go of the trigger first: X still belongs to the LT layer until it comes up.
        s.lt = 0.0;
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.layer, Layer::Base);
        assert!(f.pressed.is_empty());
        assert_eq!(f.held, vec![PadBinding::cmd("MULTIACTIONBAR1BUTTON3")]);
        s.set(PadButton::X, false);
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.released, vec![PadBinding::cmd("MULTIACTIONBAR1BUTTON3")]);
    }

    #[test]
    fn both_triggers_select_the_fourth_layer() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        let mut s = pad(&[PadButton::A]);
        s.lt = 0.9;
        s.rt = 0.9;
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.layer, Layer::Both);
        assert_eq!(f.pressed, vec![PadBinding::cmd("MULTIACTIONBAR3BUTTON1")]);
    }

    #[test]
    fn unmapped_layer_buttons_fall_back_to_base() {
        let profile = PadProfile::default();
        assert_eq!(
            profile.binding(Layer::Both, PadButton::LB),
            &PadBinding::cmd("TARGETNEARESTFRIEND")
        );
        assert_eq!(
            profile.binding(Layer::Lt, PadButton::LB),
            &PadBinding::cmd("TARGETPREVIOUSFRIEND")
        );
    }

    #[test]
    fn the_stick_moves_eight_ways() {
        assert_eq!(movement_commands([0.0, 1.0]), vec!["MOVEFORWARD"]);
        assert_eq!(
            movement_commands([0.7, 0.7]),
            vec!["MOVEFORWARD", "STRAFERIGHT"]
        );
        assert_eq!(movement_commands([-1.0, 0.1]), vec!["STRAFELEFT"]);
        assert_eq!(movement_commands([0.0, -0.5]), vec!["MOVEBACKWARD"]);
        assert!(movement_commands([0.0, 0.0]).is_empty());
    }

    #[test]
    fn each_hinted_button_knows_the_layer_that_presses_it() {
        let profile = PadProfile::default();
        let layers = hotkey_layers(&profile);
        let hints = hotkey_hints(&profile, GlyphStyle::Xbox);
        assert_eq!(layers.len(), hints.len());
        let layer_of = |f: &str| layers.iter().find(|(n, _)| n == f).map(|(_, l)| *l);
        assert_eq!(layer_of("ActionButton1"), Some(Layer::Base));
        assert_eq!(layer_of("BonusActionButton1"), Some(Layer::Base));
        assert_eq!(layer_of("MultiBarBottomLeftButton1"), Some(Layer::Lt));
        assert_eq!(layer_of("MultiBarBottomRightButton1"), Some(Layer::Rt));
        assert_eq!(layer_of("MultiBarRightButton1"), Some(Layer::Both));
    }

    #[test]
    fn steering_turns_where_the_stick_would_strafe() {
        assert_eq!(steering_commands([0.0, 1.0]), vec!["MOVEFORWARD"]);
        assert_eq!(
            steering_commands([-0.7, 0.7]),
            vec!["MOVEFORWARD", "TURNLEFT"]
        );
        assert_eq!(steering_commands([1.0, 0.0]), vec!["TURNRIGHT"]);
    }

    #[test]
    fn the_deadzone_rescales_without_a_jump() {
        assert_eq!(radial_deadzone([0.1, 0.0], 0.2), [0.0, 0.0]);
        let v = radial_deadzone([0.21, 0.0], 0.2);
        assert!(v[0] > 0.0 && v[0] < 0.05);
        assert!((radial_deadzone([1.0, 0.0], 0.2)[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn radial_wedges_run_clockwise_from_the_top() {
        assert_eq!(radial_index([0.0, 1.0], 4), 0);
        assert_eq!(radial_index([1.0, 0.0], 4), 1);
        assert_eq!(radial_index([0.0, -1.0], 4), 2);
        assert_eq!(radial_index([-1.0, 0.0], 4), 3);
        assert_eq!(radial_index([-0.1, 1.0], 12), 0);
    }

    #[test]
    fn holding_menu_and_pointing_picks_on_release() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        let mut s = pad(&[PadButton::Menu]);
        let f = m.update(&s, &profile, 0.016);
        assert!(f.radial.is_some());
        assert!(
            f.pressed.is_empty(),
            "the radial button is not a game binding"
        );
        s.right = [1.0, 0.0];
        let f = m.update(&s, &profile, 0.016);
        let n = RADIAL_ITEMS.len();
        assert_eq!(f.radial.unwrap().selected, Some(n / 4));
        s.set(PadButton::Menu, false);
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.radial_pick, Some(n / 4));
        assert!(f.radial.is_none());
    }

    #[test]
    fn tapping_menu_leaves_a_sticky_radial_that_a_confirms() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        let mut s = pad(&[PadButton::Menu]);
        m.update(&s, &profile, 0.016);
        s.set(PadButton::Menu, false);
        let f = m.update(&s, &profile, 0.016);
        assert!(f.radial.unwrap().sticky);
        s.set(PadButton::Right, true);
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.radial.unwrap().selected, Some(0));
        s.set(PadButton::Right, false);
        m.update(&s, &profile, 0.016);
        s.set(PadButton::A, true);
        let f = m.update(&s, &profile, 0.016);
        assert_eq!(f.radial_pick, Some(0));
        assert!(!m.radial_open());
    }

    #[test]
    fn cursor_mode_turns_the_right_stick_into_a_pointer_and_a_b_into_clicks() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        let mut s = pad(&[PadButton::R3]);
        let f = m.update(&s, &profile, 0.016);
        assert!(f.cursor_mode);
        s.set(PadButton::R3, false);
        s.right = [1.0, 0.0];
        s.set(PadButton::A, true);
        let f = m.update(&s, &profile, 0.1);
        assert_eq!(f.pressed, vec![PadBinding::Special(Special::LeftClick)]);
        assert!(f.cursor[0] > 50.0);
        assert_eq!(f.look, [0.0, 0.0]);
    }

    #[test]
    fn a_disconnect_releases_every_hold() {
        let profile = PadProfile::default();
        let mut m = PadMapper::default();
        m.update(&pad(&[PadButton::A]), &profile, 0.016);
        let f = m.update(&PadState::default(), &profile, 0.016);
        assert_eq!(f.released, vec![PadBinding::cmd("JUMP")]);
    }

    #[test]
    fn action_slots_follow_the_bar_layout() {
        assert_eq!(PadBinding::cmd("ACTIONBUTTON3").action_slot(), Some(3));
        assert_eq!(
            PadBinding::cmd("MULTIACTIONBAR1BUTTON1").action_slot(),
            Some(61)
        );
        assert_eq!(
            PadBinding::cmd("MULTIACTIONBAR2BUTTON12").action_slot(),
            Some(60)
        );
        assert_eq!(
            PadBinding::cmd("MULTIACTIONBAR3BUTTON1").action_slot(),
            Some(25)
        );
        assert_eq!(PadBinding::cmd("JUMP").action_slot(), None);
    }

    #[test]
    fn multibar_labels_are_short_enough_for_a_button() {
        assert_eq!(PadBinding::cmd("MULTIACTIONBAR1BUTTON3").label(), "BL3");
        assert_eq!(PadBinding::cmd("MULTIACTIONBAR2BUTTON12").label(), "BR12");
        assert_eq!(PadBinding::cmd("MULTIACTIONBAR4BUTTON1").label(), "R2·1");
        assert_eq!(PadBinding::cmd("ACTIONBUTTON7").label(), "Action 7");
        assert_eq!(
            PadBinding::cmd("MULTIACTIONBAR3BUTTON2").action_button(),
            Some((ActionBar::Right, 2))
        );
        assert_eq!(action_command("MULTIACTIONBAR5BUTTON1"), None);
        assert_eq!(action_command("ACTIONBUTTON13"), None);
        for bar in [1, 2, 3, 4] {
            for n in 1..=12 {
                let label = PadBinding::cmd(&format!("MULTIACTIONBAR{bar}BUTTON{n}")).label();
                assert!(label.chars().count() <= 5, "{label}");
            }
        }
    }

    #[test]
    fn the_default_mapping_labels_its_action_buttons() {
        let hints = hotkey_hints(&PadProfile::default(), GlyphStyle::Xbox);
        let text = |f: &str| {
            hints
                .iter()
                .find(|(frame, _)| frame == f)
                .map(|(_, t)| t.as_str())
        };
        assert_eq!(text("ActionButton1"), Some("X"));
        assert_eq!(text("BonusActionButton1"), Some("X"));
        assert_eq!(text("ActionButton3"), Some("\u{2191}"));
        assert_eq!(text("MultiBarBottomLeftButton1"), Some("LT A"));
        assert_eq!(text("MultiBarBottomRightButton2"), Some("RT B"));
        assert_eq!(text("MultiBarRightButton1"), Some("LT+RT A"));
        assert_eq!(
            bars_in_use(&PadProfile::default()),
            vec![
                ActionBar::BottomLeft,
                ActionBar::BottomRight,
                ActionBar::Right
            ]
        );
    }

    #[test]
    fn layers_say_what_to_hold() {
        assert_eq!(Layer::Base.hold_label(GlyphStyle::Xbox), "");
        assert_eq!(Layer::Lt.hold_label(GlyphStyle::Xbox), "LT");
        assert_eq!(Layer::Both.hold_label(GlyphStyle::PlayStation), "L2+R2");
        assert_eq!(PadButton::A.game_glyph(GlyphStyle::PlayStation), "\u{d7}");
        assert_eq!(PadButton::Up.game_glyph(GlyphStyle::Xbox), "\u{2191}");
    }

    #[test]
    fn the_profile_round_trips_through_toml() {
        let p = PadProfile::default();
        let text = toml::to_string(&p).unwrap();
        let back: PadProfile = toml::from_str(&text).unwrap();
        assert_eq!(p, back);
    }
}
