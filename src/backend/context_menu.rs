use cef::CefString;
use cef::ContextMenuHandler;
use cef::ImplContextMenuHandler;
use cef::WrapContextMenuHandler;
use cef::rc::*;
use cef::sys;

#[derive(Clone)]
pub struct IcyContextMenuHandler {}

impl IcyContextMenuHandler {
    pub fn new() -> Self {
        Self {}
    }
}

pub(crate) struct ContextMenuHandlerBuilder {
    object: *mut RcImpl<sys::cef_context_menu_handler_t, Self>,
    handler: IcyContextMenuHandler,
}

impl ContextMenuHandlerBuilder {
    pub(crate) fn build(handler: IcyContextMenuHandler) -> ContextMenuHandler {
        ContextMenuHandler::new(Self {
            object: std::ptr::null_mut(),
            handler,
        })
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

impl WrapContextMenuHandler for ContextMenuHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_context_menu_handler_t, Self>) {
        self.object = object;
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
            handler: self.handler.clone(),
        }
    }
}

impl ImplContextMenuHandler for ContextMenuHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_context_menu_handler_t {
        self.object.cast()
    }

    fn on_before_context_menu(
        &self,
        _browser: Option<&mut impl cef::ImplBrowser>,
        _frame: Option<&mut impl cef::ImplFrame>,
        _params: Option<&mut impl cef::ImplContextMenuParams>,
        model: Option<&mut impl cef::ImplMenuModel>,
    ) {
        if let Some(model) = model {
            model.clear();
        }
    }
}
