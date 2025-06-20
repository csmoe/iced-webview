use cef;
use cef::{rc::*, *};
use std::{cell::RefCell, ptr::null_mut};

#[derive(Clone)]
pub struct IcyDisplayHandler {
    state: IcyDisplayState,
}

#[derive(Clone, Debug)]
pub struct IcyDisplayState {
    pub cursor_type: std::rc::Rc<RefCell<CursorType>>,
}

impl IcyDisplayHandler {
    pub fn new() -> (Self, IcyDisplayState) {
        let state = IcyDisplayState {
            cursor_type: std::rc::Rc::new(RefCell::new(CursorType::default())),
        };
        (
            Self {
                state: state.clone(),
            },
            state,
        )
    }
}

pub(crate) struct DisplayHandlerBuilder {
    object: *mut RcImpl<sys::_cef_display_handler_t, Self>,
    display_handler: IcyDisplayHandler,
}

impl DisplayHandlerBuilder {
    pub(crate) fn build(display: IcyDisplayHandler) -> DisplayHandler {
        DisplayHandler::new(Self {
            object: null_mut(),
            display_handler: display,
        })
    }
}

impl WrapDisplayHandler for DisplayHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_display_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for DisplayHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for DisplayHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            display_handler: self.display_handler.clone(),
        }
    }
}

impl ImplDisplayHandler for DisplayHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_display_handler_t {
        self.object.cast()
    }

    fn on_cursor_change(
        &self,
        _browser: Option<&mut Browser>,
        #[cfg(target_os = "windows")] _cursor: sys::HCURSOR,
        #[cfg(target_os = "macos")] _cursor: *mut u8,
        #[cfg(target_os = "linux")] _cursor: c_ulong,
        type_: CursorType,
        _custom_cursor_info: Option<&CursorInfo>,
    ) -> ::std::os::raw::c_int {
        *self.display_handler.state.cursor_type.borrow_mut() = type_;
        return true as _;
    }
}
