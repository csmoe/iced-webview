use cef;
use cef::{
    CefStringUtf8, CefStringUtf16, ImplLoadHandler, LoadHandler, WrapLoadHandler,
    rc::{Rc, RcImpl},
    sys, *,
};
use std::ptr::null_mut;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

#[derive(Debug)]
#[allow(unused)]
pub enum LoadEvent {
    Changed {
        browser_id: Option<i32>,
        is_loading: bool,
        can_go_forward: bool,
        can_go_back: bool,
    },
    Start {
        browser_id: Option<i32>,
        frame_id: Option<String>,
        transition_type: u32,
    },
    End {
        browser_id: Option<i32>,
        frame_id: Option<String>,
        http_status_code: u16,
    },
    Error {
        browser_id: Option<i32>,
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
    pub fn new() -> (Self, UnboundedReceiver<LoadEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

pub(crate) struct LoadHandlerBuilder {
    object: *mut RcImpl<sys::_cef_load_handler_t, Self>,
    load_handler: IcyLoadHandler,
}

impl LoadHandlerBuilder {
    pub(crate) fn build(loader: IcyLoadHandler) -> LoadHandler {
        LoadHandler::new(Self {
            object: null_mut(),
            load_handler: loader,
        })
    }
}

impl WrapLoadHandler for LoadHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_load_handler_t, Self>) {
        self.object = object;
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

impl Clone for LoadHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            load_handler: self.load_handler.clone(),
        }
    }
}

impl ImplLoadHandler for LoadHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_load_handler_t {
        self.object.cast()
    }

    fn on_loading_state_change(
        &self,
        browser: Option<&mut Browser>,
        is_loading: ::std::os::raw::c_int,
        can_go_back: ::std::os::raw::c_int,
        can_go_forward: ::std::os::raw::c_int,
    ) {
        let event = LoadEvent::Changed {
            browser_id: browser.map(|b| b.identifier()),
            is_loading: is_loading == 1,
            can_go_forward: can_go_forward == 1,
            can_go_back: can_go_back == 1,
        };
        tracing::info!(?event);
        if let Err(err) = self.load_handler.tx.send(event) {
            tracing::error!(?err, "cannot send loading state changing event");
        }
    }

    fn on_load_start(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        transition_type: cef::TransitionType,
    ) {
        let event = LoadEvent::Start {
            browser_id: browser.map(|b| b.identifier()),

            frame_id: frame.and_then(|f| {
                (CefStringUtf8::from(&CefStringUtf16::from(&f.identifier())).as_str())
                    .map(str::to_string)
            }),
            transition_type: (*transition_type.as_ref()) as _,
        };
        tracing::info!(?event);
        if let Err(err) = self.load_handler.tx.send(event) {
            tracing::error!(?err, "cannot send loading start event");
        }
    }

    fn on_load_end(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        http_status_code: ::std::os::raw::c_int,
    ) {
        let event = LoadEvent::End {
            browser_id: browser.map(|b| b.identifier()),
            frame_id: frame.and_then(|f| {
                (CefStringUtf8::from(&CefStringUtf16::from(&f.identifier())).as_str())
                    .map(str::to_string)
            }),
            http_status_code: http_status_code as _,
        };
        tracing::info!(?event);
        if let Err(err) = self.load_handler.tx.send(event) {
            tracing::error!(?err, "cannot send loading end event");
        }
    }

    fn on_load_error(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        error_code: cef::Errorcode,
        error_text: Option<&cef::CefString>,
        failed_url: Option<&cef::CefString>,
    ) {
        let event = LoadEvent::Error {
            browser_id: browser.map(|b| b.identifier()),
            frame_id: frame.and_then(|f| {
                CefStringUtf8::from(&CefStringUtf16::from(&f.identifier()))
                    .as_str()
                    .map(|s| s.to_string())
            }),
            error_code: (*error_code.as_ref()) as _,
            error_text: error_text
                .map(CefStringUtf8::from)
                .and_then(|s| s.as_str().map(|s| s.to_string())),
            failed_url: failed_url
                .map(CefStringUtf8::from)
                .and_then(|s| s.as_str().map(|s| s.to_string())),
        };
        tracing::info!(?event);
        if let Err(err) = self.load_handler.tx.send(event) {
            tracing::error!(?err, "cannot send load start event");
        }
    }
}
