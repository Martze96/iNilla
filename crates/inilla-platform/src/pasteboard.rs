//! The system pasteboard (`UIPasteboard.generalPasteboard`) — the iOS backend for benilla's
//! copy/paste chords. On other platforms benilla keeps its own backends (arboard, Wayland); these
//! answer `None` / do nothing.

/// The pasteboard's text, if it holds any.
pub fn get() -> Option<String> {
    #[cfg(target_os = "ios")]
    {
        ios::get()
    }
    #[cfg(not(target_os = "ios"))]
    {
        None
    }
}

/// Put `text` on the pasteboard. Returns whether it worked.
pub fn set(text: &str) -> bool {
    #[cfg(target_os = "ios")]
    {
        ios::set(text)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = text;
        false
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    fn general() -> Option<Retained<AnyObject>> {
        let cls = crate::objc::class("UIPasteboard")?;
        unsafe { msg_send![cls, generalPasteboard] }
    }

    pub(super) fn get() -> Option<String> {
        let pb = general()?;
        let s: Option<Retained<NSString>> = unsafe { msg_send![&*pb, string] };
        s.map(|s| s.to_string()).filter(|s| !s.is_empty())
    }

    pub(super) fn set(text: &str) -> bool {
        let Some(pb) = general() else {
            return false;
        };
        let s = NSString::from_str(text);
        let _: () = unsafe { msg_send![&*pb, setString: &*s] };
        true
    }
}
