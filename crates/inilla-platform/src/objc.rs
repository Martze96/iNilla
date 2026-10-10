//! Small helpers over `objc2` shared by the iOS modules.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};

/// A class by name, or `None` when the framework that defines it is not loaded.
pub(crate) fn class(name: &str) -> Option<&'static AnyClass> {
    let c = std::ffi::CString::new(name).ok()?;
    AnyClass::get(&c)
}

/// `[UIApplication sharedApplication]`.
pub(crate) fn shared_application() -> Option<Retained<AnyObject>> {
    let cls = class("UIApplication")?;
    unsafe { msg_send![cls, sharedApplication] }
}
