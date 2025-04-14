use cef::CefString;
use cef::ImplRequestContext;
use cef::ImplRequestContextHandler;
use cef::ImplValue;
use cef::RequestContextHandler;
use cef::WrapRequestContextHandler;
use cef::rc::*;
use cef::sys;

#[derive(Clone)]
pub struct IcyRequestContextHandler {}

pub(crate) struct RequestContextHandlerBuilder {
    object: *mut RcImpl<sys::cef_request_context_handler_t, Self>,
    handler: IcyRequestContextHandler,
}

impl RequestContextHandlerBuilder {
    pub(crate) fn build(handler: IcyRequestContextHandler) -> RequestContextHandler {
        RequestContextHandler::new(Self {
            object: std::ptr::null_mut(),
            handler,
        })
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
impl WrapRequestContextHandler for RequestContextHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_request_context_handler_t, Self>) {
        self.object = object;
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

    fn on_request_context_initialized(
        &self,
        request_context: Option<&mut impl ImplRequestContext>,
    ) {
        let Some(request_context) = request_context else {
            return;
        };
        const KEY1: &str = "credentials_enable_service";
        const KEY2: &str = "session.restore_on_startup";
        if let Some(mut value) = cef::value_create() {
            let err = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(err);
            value.set_bool(false as _);
            if request_context.set_preference(
                Some(&CefString::from(KEY1)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!("Failed to set preference: {}", error.to_string());
            }
        }
        if let Some(mut value) = cef::value_create() {
            let err = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(err);
            value.set_int(5);
            if request_context.set_preference(
                Some(&CefString::from(KEY2)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!("Failed to set preference: {}", error.to_string());
            }
        }
    }
}
