//! The device: screen-awake, safe area, haptics, thermal state, memory headroom, backup
//! exclusion, identity. Every call is a cheap no-op off iOS.

use std::ffi::c_void;
use std::path::Path;

/// Keep the screen from dimming and locking (`UIApplication.idleTimerDisabled`). A controller
/// player never touches the screen, so without this the phone locks mid-fight.
pub fn set_idle_timer_disabled(disabled: bool) {
    #[cfg(target_os = "ios")]
    ios::set_idle_timer_disabled(disabled);
    #[cfg(not(target_os = "ios"))]
    let _ = disabled;
}

/// The safe-area insets of `ui_view` (the game's `UIView`, from Bevy's raw window handle) in
/// points: `[top, left, bottom, right]`. Zero off iOS or for a null view.
pub fn safe_area_insets(ui_view: *mut c_void) -> [f32; 4] {
    #[cfg(target_os = "ios")]
    {
        ios::safe_area_insets(ui_view)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = ui_view;
        [0.0; 4]
    }
}

/// A side of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Which side of the screen the camera housing (the notch or the Dynamic Island) is on, from the
/// interface orientation of `ui_view`'s window scene: landscape-right puts the top of the phone on
/// the left, landscape-left on the right. `None` in portrait, off iOS, or before a scene exists.
/// (The safe-area insets cannot say: in landscape iOS reports the same inset on both sides.)
pub fn camera_side(ui_view: *mut c_void) -> Option<Side> {
    #[cfg(target_os = "ios")]
    {
        match ios::interface_orientation(ui_view) {
            // UIInterfaceOrientationLandscapeRight: the home side is on the right.
            3 => Some(Side::Left),
            // UIInterfaceOrientationLandscapeLeft.
            4 => Some(Side::Right),
            _ => None,
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = ui_view;
        None
    }
}

/// Have UIKit re-read the supported orientations of the view controller behind `ui_view` (iOS
/// 16+), rotating away from one it no longer allows.
pub fn refresh_orientations(ui_view: *mut c_void) {
    #[cfg(target_os = "ios")]
    ios::refresh_orientations(ui_view);
    #[cfg(not(target_os = "ios"))]
    let _ = ui_view;
}

/// Taptic feedback styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Haptic {
    Light,
    Medium,
    Heavy,
    /// A selection tick (a radial wedge, a slider step).
    Selection,
}

pub fn haptic(kind: Haptic) {
    #[cfg(target_os = "ios")]
    ios::haptic(kind);
    #[cfg(not(target_os = "ios"))]
    let _ = kind;
}

/// `NSProcessInfo.thermalState`: how hard the phone is throttling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Thermal {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

pub fn thermal_state() -> Thermal {
    #[cfg(target_os = "ios")]
    {
        ios::thermal_state()
    }
    #[cfg(not(target_os = "ios"))]
    {
        Thermal::Nominal
    }
}

/// Bytes the process can still allocate before iOS kills it (`os_proc_available_memory`).
pub fn available_memory() -> Option<u64> {
    #[cfg(target_os = "ios")]
    {
        extern "C" {
            fn os_proc_available_memory() -> usize;
        }
        // SAFETY: a libSystem query with no arguments, available since iOS 13.
        Some(unsafe { os_proc_available_memory() } as u64)
    }
    #[cfg(not(target_os = "ios"))]
    {
        None
    }
}

/// Keep `path` (and, for a folder, everything in it) out of iCloud/iTunes backups.
pub fn exclude_from_backup(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        ios::exclude_from_backup(path)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = path;
        Ok(())
    }
}

/// `"iPhone16,2 · iOS 27.0"` (and `" · in LiveContainer"` as a guest), for logs and bug reports.
pub fn describe() -> String {
    #[cfg(target_os = "ios")]
    {
        match crate::paths::host() {
            crate::paths::Host::LiveContainer => format!("{} · in LiveContainer", ios::describe()),
            _ => ios::describe(),
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Open a URL in the system (a web page, the Settings app with `app-settings:`).
pub fn open_url(url: &str) {
    #[cfg(target_os = "ios")]
    ios::open_url(url);
    #[cfg(not(target_os = "ios"))]
    let _ = url;
}

#[cfg(target_os = "ios")]
mod ios {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::path::Path;

    use objc2::encode::{Encode, Encoding};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSNumber, NSString, NSURL};

    use super::{Haptic, Thermal};
    use crate::objc::{class, shared_application};

    pub(super) fn set_idle_timer_disabled(disabled: bool) {
        if let Some(app) = shared_application() {
            let _: () = unsafe { msg_send![&*app, setIdleTimerDisabled: Bool::new(disabled)] };
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct UIEdgeInsets {
        top: f64,
        left: f64,
        bottom: f64,
        right: f64,
    }

    // SAFETY: matches UIKit's `UIEdgeInsets` (four CGFloats, which are f64 on arm64).
    unsafe impl Encode for UIEdgeInsets {
        const ENCODING: Encoding = Encoding::Struct(
            "UIEdgeInsets",
            &[f64::ENCODING, f64::ENCODING, f64::ENCODING, f64::ENCODING],
        );
    }

    pub(super) fn safe_area_insets(ui_view: *mut c_void) -> [f32; 4] {
        if ui_view.is_null() {
            return [0.0; 4];
        }
        // SAFETY: the pointer is the live UIView from winit's raw window handle.
        let view = unsafe { &*(ui_view as *const AnyObject) };
        let i: UIEdgeInsets = unsafe { msg_send![view, safeAreaInsets] };
        [i.top as f32, i.left as f32, i.bottom as f32, i.right as f32]
    }

    /// `ui_view.window.windowScene.interfaceOrientation` (iOS 13+), `0` (unknown) when any link
    /// of that chain is missing.
    pub(super) fn interface_orientation(ui_view: *mut c_void) -> isize {
        if ui_view.is_null() {
            return 0;
        }
        // SAFETY: the pointer is the live UIView from winit's raw window handle; `window`,
        // `windowScene` and `interfaceOrientation` are UIKit properties on iOS 13 and later.
        let view = unsafe { &*(ui_view as *const AnyObject) };
        let window: Option<Retained<AnyObject>> = unsafe { msg_send![view, window] };
        let Some(window) = window else {
            return 0;
        };
        let scene: Option<Retained<AnyObject>> = unsafe { msg_send![&*window, windowScene] };
        let Some(scene) = scene else {
            return 0;
        };
        unsafe { msg_send![&*scene, interfaceOrientation] }
    }

    pub(super) fn refresh_orientations(ui_view: *mut c_void) {
        if ui_view.is_null() {
            return;
        }
        // SAFETY: the pointer is the live UIView from winit's raw window handle; every message
        // below is a UIKit property or method that exists on iOS 16 and later.
        let view = unsafe { &*(ui_view as *const AnyObject) };
        let window: Option<Retained<AnyObject>> = unsafe { msg_send![view, window] };
        let Some(window) = window else {
            return;
        };
        let root: Option<Retained<AnyObject>> = unsafe { msg_send![&*window, rootViewController] };
        if let Some(root) = root {
            let _: () =
                unsafe { msg_send![&*root, setNeedsUpdateOfSupportedInterfaceOrientations] };
        }
    }

    thread_local! {
        static IMPACT: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
        static SELECTION: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    /// One generator per kind, created on first use and kept: creating one per tap adds latency.
    fn generator(
        slot: &'static std::thread::LocalKey<RefCell<Option<Retained<AnyObject>>>>,
        class_name: &str,
        f: impl FnOnce(&AnyObject),
    ) {
        slot.with(|g| {
            let mut g = g.borrow_mut();
            if g.is_none() {
                if let Some(cls) = class(class_name) {
                    let obj: Option<Retained<AnyObject>> = unsafe { msg_send![cls, new] };
                    *g = obj;
                }
            }
            if let Some(obj) = g.as_deref() {
                f(obj);
            }
        });
    }

    pub(super) fn haptic(kind: Haptic) {
        let intensity: f64 = match kind {
            Haptic::Light => 0.4,
            Haptic::Medium => 0.7,
            Haptic::Heavy => 1.0,
            Haptic::Selection => {
                generator(&SELECTION, "UISelectionFeedbackGenerator", |g| {
                    let _: () = unsafe { msg_send![g, selectionChanged] };
                });
                return;
            }
        };
        generator(&IMPACT, "UIImpactFeedbackGenerator", |g| {
            let _: () = unsafe { msg_send![g, impactOccurredWithIntensity: intensity] };
        });
    }

    pub(super) fn thermal_state() -> Thermal {
        let Some(cls) = class("NSProcessInfo") else {
            return Thermal::Nominal;
        };
        let info: Option<Retained<AnyObject>> = unsafe { msg_send![cls, processInfo] };
        let Some(info) = info else {
            return Thermal::Nominal;
        };
        let state: isize = unsafe { msg_send![&*info, thermalState] };
        match state {
            1 => Thermal::Fair,
            2 => Thermal::Serious,
            3 => Thermal::Critical,
            _ => Thermal::Nominal,
        }
    }

    extern "C" {
        static NSURLIsExcludedFromBackupKey: &'static NSString;
    }

    pub(super) fn exclude_from_backup(path: &Path) -> Result<(), String> {
        let p = NSString::from_str(&path.to_string_lossy());
        let url: Retained<NSURL> = unsafe { msg_send![NSURL::class(), fileURLWithPath: &*p] };
        let yes = NSNumber::new_bool(true);
        let mut err: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: a Foundation constant, linked from Foundation.
        let key = unsafe { NSURLIsExcludedFromBackupKey };
        let ok: Bool =
            unsafe { msg_send![&*url, setResourceValue: &*yes, forKey: key, error: &mut err] };
        if ok.as_bool() {
            Ok(())
        } else {
            Err(format!("could not exclude {} from backup", path.display()))
        }
    }

    fn uname_machine() -> String {
        let mut u: libc::utsname = unsafe { std::mem::zeroed() };
        if unsafe { libc::uname(&mut u) } != 0 {
            return "unknown".into();
        }
        let bytes: Vec<u8> = u
            .machine
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub(super) fn describe() -> String {
        let version = class("UIDevice")
            .and_then(|cls| {
                let dev: Option<Retained<AnyObject>> = unsafe { msg_send![cls, currentDevice] };
                dev
            })
            .and_then(|dev| {
                let v: Option<Retained<NSString>> = unsafe { msg_send![&*dev, systemVersion] };
                v
            })
            .map(|v| v.to_string())
            .unwrap_or_default();
        format!("{} · iOS {version}", uname_machine())
    }

    pub(super) fn open_url(url: &str) {
        let Some(app) = shared_application() else {
            return;
        };
        let s = NSString::from_str(url);
        let u: Option<Retained<NSURL>> = unsafe { msg_send![NSURL::class(), URLWithString: &*s] };
        let Some(u) = u else {
            return;
        };
        let Some(options): Option<Retained<AnyObject>> =
            class("NSDictionary").and_then(|cls| unsafe { msg_send![cls, dictionary] })
        else {
            return;
        };
        let done: Option<&block2::Block<dyn Fn(Bool)>> = None;
        let _: () =
            unsafe { msg_send![&*app, openURL: &*u, options: &*options, completionHandler: done] };
    }
}
