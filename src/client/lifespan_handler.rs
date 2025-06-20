use cef;
use cef::{ImplBrowser, ImplLifeSpanHandler, LifeSpanHandler, WrapLifeSpanHandler, rc::*, sys};
use std::ptr::null_mut;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    BrowserId,
    instance::{LaunchId, new_webview, remove_webview},
};

#[derive(Clone, Debug)]
pub enum LifeSpanEvent {
    Created { browser_id: BrowserId },
    Closed { browser_id: BrowserId },
}

#[derive(Clone)]
pub struct IcyLifeSpanHandler {
    tx: Sender<LifeSpanEvent>,
    launch_id: LaunchId,
}

impl IcyLifeSpanHandler {
    pub fn new(launch_id: LaunchId) -> (Self, Receiver<LifeSpanEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        (Self { tx, launch_id }, rx)
    }
}

impl LifeSpanHandlerBuilder {
    pub fn build(handler: IcyLifeSpanHandler) -> LifeSpanHandler {
        LifeSpanHandler::new(Self {
            object: null_mut(),
            handler,
        })
    }
}

pub(crate) struct LifeSpanHandlerBuilder {
    object: *mut RcImpl<sys::cef_life_span_handler_t, Self>,
    handler: IcyLifeSpanHandler,
}

impl Rc for LifeSpanHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapLifeSpanHandler for LifeSpanHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for LifeSpanHandlerBuilder {
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

impl ImplLifeSpanHandler for LifeSpanHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_life_span_handler_t {
        self.object.cast()
    }

    fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
        let Some(browser) = browser else {
            return;
        };
        if self
            .handler
            .tx
            .try_send(LifeSpanEvent::Created {
                browser_id: browser.identifier().into(),
            })
            .inspect_err(|err| {
                tracing::warn!(?err, "cannot send life span event");
            })
            .is_ok()
        {
            new_webview(self.handler.launch_id, browser.identifier().into());
        }
    }

    fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
        let Some(browser) = browser else {
            return;
        };

        remove_webview(browser.identifier().into());
        if let Err(err) = self.handler.tx.try_send(LifeSpanEvent::Closed {
            browser_id: browser.identifier().into(),
        }) {
            tracing::warn!(?err, "cannot send life span event");
        }
    }

    fn on_before_popup(
        &self,
        _browser: Option<&mut cef::Browser>,
        _frame: Option<&mut cef::Frame>,
        _popup_id: ::std::os::raw::c_int,
        _target_url: Option<&cef::CefString>,
        _target_frame_name: Option<&cef::CefString>,
        _target_disposition: cef::WindowOpenDisposition,
        _user_gesture: ::std::os::raw::c_int,
        _popup_features: Option<&cef::PopupFeatures>,
        _window_info: Option<&mut cef::WindowInfo>,
        _client: Option<&mut Option<impl cef::ImplClient>>,
        _settings: Option<&mut cef::BrowserSettings>,
        _extra_info: Option<&mut Option<cef::DictionaryValue>>,
        _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
    ) -> ::std::os::raw::c_int {
        return true as _;
    }
}
