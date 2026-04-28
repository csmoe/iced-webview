use cef::*;
use std::ffi::c_int;

#[derive(Clone)]
pub struct IcyKeyboardHandler {}

impl IcyKeyboardHandler {
    pub fn new() -> Self {
        Self {}
    }
}

wrap_keyboard_handler! {
    pub(crate) struct KeyboardHandlerBuilder {
        keyboard_handler: IcyKeyboardHandler,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&cef::KeyEvent>,
            /*
            #[cfg(target_os = "windows")] _os_event: Option<&mut sys::MSG>,
            #[cfg(target_os = "macos")] _os_event: *mut u8,
            #[cfg(target_os = "linux")] _os_event: Option<&mut sys::XEvent>,
            */
            _os_event: *mut u8,
            is_keyboard_shortcut: Option<&mut c_int>,
        ) -> c_int {
            let Some(browser) = browser else {
                return false as _;
            };
            let Some(frame) = browser.focused_frame() else {
                return false as _;
            };
            let Some(event) = event else {
                return false as _;
            };
            use cef::sys::cef_event_flags_t;
            let modifiers = cef_event_flags_t(event.modifiers as _);

            #[cfg(target_os = "macos")]
            let ctrl = modifiers
                & (cef_event_flags_t::EVENTFLAG_SHIFT_DOWN
                    | (cef_event_flags_t::EVENTFLAG_ALT_DOWN
                        | cef_event_flags_t::EVENTFLAG_COMMAND_DOWN))
                == cef_event_flags_t::EVENTFLAG_COMMAND_DOWN;

            #[cfg(target_os = "windows")]
            let ctrl = modifiers
                & (cef_event_flags_t::EVENTFLAG_CONTROL_DOWN
                    | cef_event_flags_t::EVENTFLAG_SHIFT_DOWN
                    | cef_event_flags_t::EVENTFLAG_ALT_DOWN)
                == cef_event_flags_t::EVENTFLAG_CONTROL_DOWN;

            #[cfg(target_os = "linux")]
            let ctrl = modifiers & cef_event_flags_t::EVENTFLAG_CONTROL_DOWN
                == cef_event_flags_t::EVENTFLAG_CONTROL_DOWN;

            if event.type_ == cef::sys::cef_key_event_type_t::KEYEVENT_RAWKEYDOWN.into() {
                let code = event.windows_key_code;
                if ctrl {
                    match code {
                        0x41 => frame.select_all(),
                        0x5a => frame.undo(),
                        0x58 => frame.cut(),
                        0x43 => frame.copy(),
                        0x56 => frame.paste(),
                        0x59 => frame.redo(),
                        _ => return false as _,
                    }
                    if let Some(is_shortcut) = is_keyboard_shortcut {
                        *is_shortcut = true as _;
                    }
                    return true as _;
                }
            }
            false as _
        }
    }
}

impl KeyboardHandlerBuilder {
    pub(crate) fn build(loader: IcyKeyboardHandler) -> KeyboardHandler {
        Self::new(loader)
    }
}
