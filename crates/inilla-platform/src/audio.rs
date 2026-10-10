//! The iOS audio session. benilla's mixer opens its output through cpal (CoreAudio's RemoteIO on
//! iOS); what the *session* is — whether the game silences the player's music, whether the silent
//! switch mutes it — is the app's call, made here once at boot and again when the setting moves.
//!
//! | mix with others | ignore silent switch | category |
//! |---|---|---|
//! | no  | no  | `SoloAmbient` (the iOS default: other audio stops, silent switch mutes) |
//! | yes | no  | `Ambient` |
//! | no  | yes | `Playback` |
//! | yes | yes | `Playback` + `MixWithOthers` |

pub fn configure_session(mix_with_others: bool, ignore_silent_switch: bool) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        ios::configure(mix_with_others, ignore_silent_switch)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (mix_with_others, ignore_silent_switch);
        Ok(())
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::NSString;

    use crate::objc::class;

    #[link(name = "AVFAudio", kind = "framework")]
    extern "C" {
        static AVAudioSessionCategoryAmbient: &'static NSString;
        static AVAudioSessionCategorySoloAmbient: &'static NSString;
        static AVAudioSessionCategoryPlayback: &'static NSString;
    }

    /// `AVAudioSessionCategoryOptionMixWithOthers`.
    const MIX_WITH_OTHERS: usize = 0x1;

    pub(super) fn configure(mix: bool, ignore_silent: bool) -> Result<(), String> {
        let cls = class("AVAudioSession").ok_or("AVFAudio is not loaded")?;
        let session: Option<Retained<AnyObject>> = unsafe { msg_send![cls, sharedInstance] };
        let session = session.ok_or("no audio session")?;
        // SAFETY: framework constants, linked above.
        let (category, options) = unsafe {
            match (mix, ignore_silent) {
                (false, false) => (AVAudioSessionCategorySoloAmbient, 0),
                (true, false) => (AVAudioSessionCategoryAmbient, 0),
                (false, true) => (AVAudioSessionCategoryPlayback, 0),
                (true, true) => (AVAudioSessionCategoryPlayback, MIX_WITH_OTHERS),
            }
        };
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = unsafe {
            msg_send![&*session, setCategory: category, withOptions: options, error: &mut err]
        };
        if !ok.as_bool() {
            return Err("setCategory failed".into());
        }
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let _: Bool = unsafe { msg_send![&*session, setActive: Bool::YES, error: &mut err] };
        Ok(())
    }
}
