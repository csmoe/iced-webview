use cef;
use cef::{
    CefString, ImplRequestContextHandler, ImplValue, RequestContextHandler,
    WrapRequestContextHandler,
    rc::{Rc, RcImpl},
    sys, *,
};
use std::ptr::null_mut;

#[derive(Clone)]
pub struct IcyRequestContextHandler {}

pub(crate) struct RequestContextHandlerBuilder {
    object: *mut RcImpl<sys::cef_request_context_handler_t, Self>,
    handler: IcyRequestContextHandler,
}

impl RequestContextHandlerBuilder {
    pub(crate) fn build(handler: IcyRequestContextHandler) -> RequestContextHandler {
        RequestContextHandler::new(Self {
            object: null_mut(),
            handler,
        })
    }
}

impl WrapRequestContextHandler for RequestContextHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_request_context_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for RequestContextHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for RequestContextHandlerBuilder {
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

impl ImplRequestContextHandler for RequestContextHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_request_context_handler_t {
        self.object.cast()
    }

    fn on_request_context_initialized(&self, request_context: Option<&mut cef::RequestContext>) {
        tracing::info!("request context initialized");

        const KEY1: &str = "credentials_enable_service";
        const KEY2: &str = "session.restore_on_startup";
        let Some(ctxt) = request_context else {
            return;
        };

        if let Some(mut value) = cef::value_create() {
            // FIXME: better string alloc/free
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            value.set_bool(false as _);
            if ctxt.set_preference(
                Some(&CefString::from(KEY1)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = KEY1, "cannot set preference");
            }
        }

        if let Some(mut value) = cef::value_create() {
            value.set_int(5);
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);

            if ctxt.set_preference(
                Some(&CefString::from(KEY2)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = KEY2, "cannot set preference");
            }
        }
    }
}
