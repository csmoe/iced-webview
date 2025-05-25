use cef::ContextMenuHandler;
use cef::ImplContextMenuHandler;
use cef::ImplMenuModel;
use cef::WrapContextMenuHandler;
use cef::rc::*;
use cef::sys;

impl IcyContextMenuHandler {
    pub fn new() -> Self {
        Self {
            object: std::ptr::null_mut(),
        }
    }
}

pub(crate) struct IcyContextMenuHandler {
    object: *mut RcImpl<sys::cef_context_menu_handler_t, Self>,
}

impl Rc for IcyContextMenuHandler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapContextMenuHandler for IcyContextMenuHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_context_menu_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for IcyContextMenuHandler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self { object }
    }
}

impl ImplContextMenuHandler for IcyContextMenuHandler {
    fn get_raw(&self) -> *mut sys::_cef_context_menu_handler_t {
        self.object.cast()
    }

    fn on_before_context_menu(
        &self,
        _browser: Option<&mut cef::Browser>,
        _frame: Option<&mut cef::Frame>,
        _params: Option<&mut cef::ContextMenuParams>,
        model: Option<&mut cef::MenuModel>,
    ) {
        if let Some(model) = model {
            model.clear();
        }
    }
}
