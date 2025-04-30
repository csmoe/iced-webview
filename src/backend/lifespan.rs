use cef::ImplLifeSpanHandler;
use cef::LifeSpanHandler;
use cef::WrapLifeSpanHandler;
use cef::rc::*;
use cef::sys;
use tokio::sync::mpsc::Sender;

use super::BrowserId;

#[derive(Clone, Debug)]
pub enum LifeSpanEvent {
    Created { browser_id: BrowserId },
    Closed { browser_id: BrowserId },
}

#[derive(Clone)]
pub struct IcyLifeSpanHandler {
    tx: Sender<LifeSpanEvent>,
}

impl IcyLifeSpanHandler {
    pub fn new(tx: Sender<LifeSpanEvent>) -> Self {
        Self { tx }
    }
}

pub(crate) struct LifeSpanHandlerBuilder {
    object: *mut RcImpl<sys::cef_life_span_handler_t, Self>,
    handler: IcyLifeSpanHandler,
}

impl LifeSpanHandlerBuilder {
    pub(crate) fn build(handler: IcyLifeSpanHandler) -> LifeSpanHandler {
        LifeSpanHandler::new(Self {
            object: std::ptr::null_mut(),
            handler,
        })
    }
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

    fn on_after_created(&self, browser: Option<&mut impl cef::ImplBrowser>) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.try_send(LifeSpanEvent::Created {
            browser_id: browser.identifier().into(),
        }) {
            tracing::warn!(?err, "cannot send life span event");
        }
    }

    fn on_before_close(&self, browser: Option<&mut impl cef::ImplBrowser>) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.handler.tx.try_send(LifeSpanEvent::Closed {
            browser_id: browser.identifier().into(),
        }) {
            tracing::warn!(?err, "cannot send life span event");
        }
    }
}
