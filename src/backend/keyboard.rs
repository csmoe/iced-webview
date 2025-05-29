use cef::rc::*;
use cef::*;
use std::ffi::*;
use std::ptr::null_mut;

#[derive(Clone)]
pub struct IcyKeyboardHandler {}

impl IcyKeyboardHandler {
    pub fn new() -> Self {
        Self {}
    }
}

pub(crate) struct KeyboardHandlerBuilder {
    object: *mut RcImpl<sys::_cef_keyboard_handler_t, Self>,
    keyboard_handler: IcyKeyboardHandler,
}

impl KeyboardHandlerBuilder {
    pub(crate) fn build(loader: IcyKeyboardHandler) -> KeyboardHandler {
        KeyboardHandler::new(Self {
            object: null_mut(),
            keyboard_handler: loader,
        })
    }
}

impl WrapKeyboardHandler for KeyboardHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_keyboard_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for KeyboardHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for KeyboardHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            keyboard_handler: self.keyboard_handler.clone(),
        }
    }
}

impl ImplKeyboardHandler for KeyboardHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_keyboard_handler_t {
        self.object.cast()
    }

    fn on_pre_key_event(
        &self,
        browser: Option<&mut Browser>,
        event: Option<&cef::KeyEvent>,
        #[cfg(target_os = "windows")] _os_event: Option<&mut sys::MSG>,
        #[cfg(target_os = "macos")] _os_event: *mut u8,
        #[cfg(target_os = "linux")] _os_event: Option<&mut sys::XEvent>,
        is_keyboard_shortcut: Option<&mut c_int>,
    ) -> c_int {
        let Some(browser) = browser else {
            return false as _;
        };
        let Some(_frame) = browser.focused_frame() else {
            return false as _;
        };
        let Some(event) = event else {
            return false as _;
        };
        #[cfg(target_os = "macos")]
        let ctrl =
            event.modifiers & (cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN as u32) != 0;

        #[cfg(target_os = "windows")]
        let ctrl =
            event.modifiers & (cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN as u32) != 0;
        if event.type_ == cef::sys::cef_key_event_type_t::KEYEVENT_RAWKEYDOWN.into() {
            match event.windows_key_code {
                97 | 122 | 120 | 99 | 118 | 121 if ctrl => {
                    is_keyboard_shortcut.map(|v| *v = true as _);
                    return false as _;
                }
                123 => {
                    is_keyboard_shortcut.map(|v| *v = true as _);
                    return false as _;
                }
                _ => return false as _,
            }
        }
        false as _
    }
    fn on_key_event(
        &self,
        browser: Option<&mut Browser>,
        event: Option<&cef::KeyEvent>,
        #[cfg(target_os = "windows")] _os_event: Option<&mut sys::MSG>,
        #[cfg(target_os = "macos")] _os_event: *mut u8,
        #[cfg(target_os = "linux")] _os_event: Option<&mut sys::XEvent>,
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
        #[cfg(target_os = "macos")]
        let ctrl =
            event.modifiers & (cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN as u32) != 0;

        #[cfg(target_os = "windows")]
        let ctrl =
            event.modifiers & (cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN as u32) != 0;
        let keydown = event.type_ == cef::sys::cef_key_event_type_t::KEYEVENT_RAWKEYDOWN.into();
        if keydown {
            if ctrl {
                match event.windows_key_code {
                    97 => frame.select_all(), /* A */
                    122 => frame.undo(),      /* Z */
                    120 => frame.cut(),       /* X */
                    99 => frame.copy(),       /* C */
                    118 => frame.paste(),     /* V */
                    121 => frame.redo(),      /* Y */

                    _ => return false as _,
                }
            } else {
                match event.windows_key_code {
                    123 => {
                        /* F12 */
                        if let Some(host) = browser.host() {
                            if host.has_dev_tools() == 1 {
                                host.close_dev_tools();
                            } else {
                                host.show_dev_tools(
                                    None,
                                    None::<&mut cef::Client>,
                                    Some(&cef::BrowserSettings::default()),
                                    None,
                                );
                            }
                        }
                    }
                    _ => return false as _,
                }
            }
            return true as _;
        }
        false as _
    }
}
