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
        const DISABLE_CRED: &str = "credentials_enable_service";
        const THEME1: &str = "browser.theme.color_scheme";
        const THEME2: &str = "browser.theme.color_scheme2";
        const FOLLOW: &str = "browser.theme.follows_system_colors";
        const CLIPBOARD_PREF: &str = "profile.content_settings.exceptions.clipboard";
        let Some(ctxt) = request_context else {
            return;
        };
        ctxt.set_chrome_color_scheme(
            cef::sys::cef_color_variant_t::CEF_COLOR_VARIANT_LIGHT.into(),
            true as _,
        );

        if let Some(mut value) = cef::value_create() {
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            value.set_bool(false as _);
            if ctxt.set_preference(
                Some(&CefString::from(DISABLE_CRED)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = DISABLE_CRED, "cannot set preference");
            }
        }

        if let Some(mut value) = cef::value_create() {
            value.set_int(1);
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            if ctxt.set_preference(
                Some(&CefString::from(THEME1)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = THEME1, "cannot set preference");
            }
        }

        if let Some(mut value) = cef::value_create() {
            value.set_int(1);
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            if ctxt.set_preference(
                Some(&CefString::from(THEME2)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = THEME2, "cannot set preference");
            }
        }

        if let Some(mut value) = cef::value_create() {
            value.set_bool(false as _);
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            if ctxt.set_preference(
                Some(&CefString::from(FOLLOW)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = FOLLOW, "cannot set preference");
            }
        }

        if let Some(mut value) = cef::value_create() {
            if let Some(mut origin_dict) = cef::dictionary_value_create() {
                if let Some(mut setting_dict) = cef::dictionary_value_create() {
                    setting_dict.set_int(Some(&"setting".into()), 1);
                    origin_dict.set_dictionary(Some(&"*,*".into()), Some(&mut setting_dict));
                }
                value.set_dictionary(Some(&mut origin_dict));
            }
            let error = unsafe { cef::sys::cef_string_userfree_utf16_alloc() };
            let mut error = CefString::from(error);
            if ctxt.set_preference(
                Some(&CefString::from(CLIPBOARD_PREF)),
                Some(&mut value),
                Some(&mut error),
            ) != 1
            {
                tracing::error!(key = CLIPBOARD_PREF, "cannot set clipboard preference");
            }
        }
    }
}
