use cef;
use cef::{
    ContextMenuHandler, ImplContextMenuHandler, WrapContextMenuHandler,
    rc::{Rc, RcImpl},
    sys, *,
};
use std::ptr::null_mut;

#[derive(Clone)]
pub struct IcyContextMenuHandler {}

impl IcyContextMenuHandler {
    pub fn new() -> Self {
        Self {}
    }
}

pub(crate) struct ContextMenuHandlerBuilder {
    object: *mut RcImpl<sys::_cef_context_menu_handler_t, Self>,
    context_menu_handler: IcyContextMenuHandler,
}

impl ContextMenuHandlerBuilder {
    pub(crate) fn build(context_menu_handler: IcyContextMenuHandler) -> ContextMenuHandler {
        ContextMenuHandler::new(Self {
            object: null_mut(),
            context_menu_handler,
        })
    }
}

impl WrapContextMenuHandler for ContextMenuHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_context_menu_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for ContextMenuHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for ContextMenuHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            context_menu_handler: self.context_menu_handler.clone(),
        }
    }
}

impl ImplContextMenuHandler for ContextMenuHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_context_menu_handler_t {
        self.object.cast()
    }
    fn on_before_context_menu(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        _params: Option<&mut ContextMenuParams>,
        model: Option<&mut MenuModel>,
    ) {
        if let Some(model) = model {
            model.clear();
        }
    }
}
