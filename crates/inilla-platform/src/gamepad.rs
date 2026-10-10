//! Game controllers through Apple's **GameController** framework — the one path iOS offers for
//! MFi, Xbox, DualSense/DualShock, Switch Pro and phone-clip controllers like the **Backbone One**
//! (all of them present `GCExtendedGamepad`). Polled once a frame on the main thread; a controller
//! paired in iOS Settings → Bluetooth (or plugged in over USB-C/Lightning) appears here on its own.
//!
//! Off iOS [`poll`] reports nothing: the desktop build reads pads through Bevy's gilrs input
//! instead (see the app's `inilla::pad`).

use inilla_core::pad::PadState;

/// One connected controller.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    /// Stable while the controller stays connected.
    pub id: u64,
    /// `vendorName`, e.g. "Backbone One" / "Xbox Wireless Controller".
    pub name: String,
    /// `productCategory`, e.g. "Xbox One", "DualSense", "MFi".
    pub category: String,
    pub state: PadState,
}

/// Every connected extended gamepad, this instant.
pub fn poll(out: &mut Vec<Pad>) {
    out.clear();
    #[cfg(target_os = "ios")]
    ios::poll(out);
}

/// Rumble `strength` (`0..1`) for `seconds` on controller `id`, where it has haptics.
pub fn rumble(id: u64, strength: f32, seconds: f32) {
    // CoreHaptics over `GCController.haptics` needs an engine per controller; left for a later
    // round — the call site is in place so the mapping's `rumble` setting has somewhere to go.
    let _ = (id, strength, seconds);
}

#[cfg(target_os = "ios")]
mod ios {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::{NSArray, NSString};

    use inilla_core::pad::{PadButton, PadState};

    use super::Pad;
    use crate::objc::class;

    // The framework has to be linked for its classes to exist at runtime.
    #[link(name = "GameController", kind = "framework")]
    extern "C" {}

    /// A `GCControllerButtonInput`'s pressed state (a nil button — an optional one the
    /// controller lacks — reads as up).
    fn pressed(button: Option<Retained<AnyObject>>) -> bool {
        button.is_some_and(|b| {
            let p: Bool = unsafe { msg_send![&*b, isPressed] };
            p.as_bool()
        })
    }

    fn value(button: Option<Retained<AnyObject>>) -> f32 {
        button.map_or(0.0, |b| unsafe { msg_send![&*b, value] })
    }

    /// A `GCControllerDirectionPad`'s axes, `+y` up.
    fn stick(pad: Option<Retained<AnyObject>>) -> [f32; 2] {
        let Some(pad) = pad else {
            return [0.0; 2];
        };
        let x: Option<Retained<AnyObject>> = unsafe { msg_send![&*pad, xAxis] };
        let y: Option<Retained<AnyObject>> = unsafe { msg_send![&*pad, yAxis] };
        [value(x), value(y)]
    }

    /// `[obj selector]` for an object-returning property. Every selector used here exists on
    /// `GCExtendedGamepad` from iOS 13 on (the deployment target is 17); the optional buttons
    /// answer nil on controllers that lack them.
    macro_rules! prop {
        ($o:expr, $sel:ident) => {{
            let v: Option<Retained<AnyObject>> = unsafe { msg_send![$o, $sel] };
            v
        }};
    }

    pub(super) fn poll(out: &mut Vec<Pad>) {
        let Some(cls) = class("GCController") else {
            return;
        };
        let list: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![cls, controllers] };
        let Some(list) = list else {
            return;
        };
        for ctrl in list.iter() {
            let Some(ext) = prop!(&*ctrl, extendedGamepad) else {
                continue;
            };
            let e = &*ext;
            let mut state = PadState {
                connected: true,
                left: stick(prop!(e, leftThumbstick)),
                right: stick(prop!(e, rightThumbstick)),
                lt: value(prop!(e, leftTrigger)),
                rt: value(prop!(e, rightTrigger)),
                buttons: 0,
            };
            state.set(PadButton::A, pressed(prop!(e, buttonA)));
            state.set(PadButton::B, pressed(prop!(e, buttonB)));
            state.set(PadButton::X, pressed(prop!(e, buttonX)));
            state.set(PadButton::Y, pressed(prop!(e, buttonY)));
            state.set(PadButton::LB, pressed(prop!(e, leftShoulder)));
            state.set(PadButton::RB, pressed(prop!(e, rightShoulder)));
            state.set(PadButton::L3, pressed(prop!(e, leftThumbstickButton)));
            state.set(PadButton::R3, pressed(prop!(e, rightThumbstickButton)));
            state.set(PadButton::Menu, pressed(prop!(e, buttonMenu)));
            state.set(PadButton::Options, pressed(prop!(e, buttonOptions)));
            state.set(PadButton::Home, pressed(prop!(e, buttonHome)));
            if let Some(d) = prop!(e, dpad) {
                state.set(PadButton::Up, pressed(prop!(&*d, up)));
                state.set(PadButton::Down, pressed(prop!(&*d, down)));
                state.set(PadButton::Left, pressed(prop!(&*d, left)));
                state.set(PadButton::Right, pressed(prop!(&*d, right)));
            }

            let name: Option<Retained<NSString>> = unsafe { msg_send![&*ctrl, vendorName] };
            let category: Option<Retained<NSString>> =
                unsafe { msg_send![&*ctrl, productCategory] };
            out.push(Pad {
                id: Retained::as_ptr(&ctrl) as u64,
                name: name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Controller".into()),
                category: category.map(|s| s.to_string()).unwrap_or_default(),
                state,
            });
        }
    }
}
