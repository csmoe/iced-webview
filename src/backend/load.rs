use super::BrowserId;
use cef::CefString;
use cef::CefStringUtf8;
use cef::ImplLoadHandler;
use cef::LoadHandler;
use cef::WrapLoadHandler;
use cef::rc::*;
use cef::sys;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Debug)]
#[allow(unused)]
pub enum LoadEvent {
    Changed {
        browser_id: Option<BrowserId>,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    Start {
        browser_id: Option<BrowserId>,
        frame_id: Option<String>,
        transition_type: u32,
    },
    End {
        browser_id: Option<BrowserId>,
        frame_id: Option<String>,
        http_status_code: u16,
    },
    Error {
        browser_id: Option<BrowserId>,
        frame_id: Option<String>,
        error_code: i32,
        error_text: Option<String>,
        failed_url: Option<String>,
    },
}

#[derive(Clone)]
pub struct IcyLoadHandler {
    tx: UnboundedSender<LoadEvent>,
}

impl IcyLoadHandler {
    pub fn new(tx: UnboundedSender<LoadEvent>) -> Self {
        Self { tx }
    }
}

pub(crate) struct LoadHandlerBuilder {
    object: *mut RcImpl<sys::cef_load_handler_t, Self>,
    handler: IcyLoadHandler,
}

impl LoadHandlerBuilder {
    pub(crate) fn build(handler: IcyLoadHandler) -> LoadHandler {
        LoadHandler::new(Self {
            object: std::ptr::null_mut(),
            handler,
        })
    }
}

impl Rc for LoadHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapLoadHandler for LoadHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_load_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for LoadHandlerBuilder {
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

impl ImplLoadHandler for LoadHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_load_handler_t {
        self.object.cast()
    }

    fn on_loading_state_change(
        &self,
        browser: Option<&mut impl cef::ImplBrowser>,
        is_loading: ::std::os::raw::c_int,
        can_go_back: ::std::os::raw::c_int,
        can_go_forward: ::std::os::raw::c_int,
    ) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.send(LoadEvent::Changed {
            browser_id: Some(browser.get_identifier().into()),
            is_loading: is_loading != 0,
            can_go_back: can_go_back != 0,
            can_go_forward: can_go_forward != 0,
        }) {
            tracing::warn!(?err, "cannotsend load event");
        }
    }

    fn on_load_start(
        &self,
        browser: Option<&mut impl cef::ImplBrowser>,
        frame: Option<&mut impl cef::ImplFrame>,
        transition_type: cef::TransitionType,
    ) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.send(LoadEvent::Start {
            browser_id: Some(browser.get_identifier().into()),
            frame_id: frame.and_then(|f| {
                CefStringUtf8::from(&CefString::from(&f.get_identifier()))
                    .as_str()
                    .map(str::to_string)
            }),
            transition_type: (*transition_type.as_ref()) as _,
        }) {
            tracing::warn!(?err, "cannotsend load event");
        }
    }

    fn on_load_end(
        &self,
        browser: Option<&mut impl cef::ImplBrowser>,
        frame: Option<&mut impl cef::ImplFrame>,
        http_status_code: ::std::os::raw::c_int,
    ) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.send(LoadEvent::End {
            browser_id: Some(browser.get_identifier().into()),
            frame_id: frame.and_then(|f| {
                CefStringUtf8::from(&CefString::from(&f.get_identifier()))
                    .as_str()
                    .map(str::to_string)
            }),
            http_status_code: http_status_code as _,
        }) {
            tracing::warn!(?err, "cannotsend load event");
        }
    }

    fn on_load_error(
        &self,
        browser: Option<&mut impl cef::ImplBrowser>,
        frame: Option<&mut impl cef::ImplFrame>,
        error_code: cef::Errorcode,
        error_text: Option<&CefString>,
        failed_url: Option<&CefString>,
    ) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.send(LoadEvent::Error {
            browser_id: Some(browser.get_identifier().into()),
            frame_id: frame.and_then(|f| {
                CefStringUtf8::from(&CefString::from(&f.get_identifier()))
                    .as_str()
                    .map(str::to_string)
            }),
            error_code: (*error_code.as_ref()) as _,
            error_text: error_text
                .map(CefStringUtf8::from)
                .and_then(|s| s.as_str().map(str::to_string)),
            failed_url: failed_url
                .map(CefStringUtf8::from)
                .and_then(|s| s.as_str().map(str::to_string)),
        }) {
            tracing::warn!(?err, "cannotsend load event");
        }
    }
}
