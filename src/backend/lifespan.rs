use cef::ImplBrowser;
use cef::ImplLifeSpanHandler;
use cef::WrapLifeSpanHandler;
use cef::rc::*;
use cef::sys;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use super::BrowserId;

#[derive(Clone, Debug)]
pub enum LifeSpanEvent {
    Created { browser_id: BrowserId },
    Closed { browser_id: BrowserId },
}

impl IcyLifeSpanHandler {
    pub fn new() -> (Self, Receiver<LifeSpanEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        (
            Self {
                object: std::ptr::null_mut(),
                tx,
            },
            rx,
        )
    }
}

pub(crate) struct IcyLifeSpanHandler {
    object: *mut RcImpl<sys::cef_life_span_handler_t, Self>,
    tx: Sender<LifeSpanEvent>,
}

impl Rc for IcyLifeSpanHandler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapLifeSpanHandler for IcyLifeSpanHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for IcyLifeSpanHandler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            tx: self.tx.clone(),
        }
    }
}

impl ImplLifeSpanHandler for IcyLifeSpanHandler {
    fn get_raw(&self) -> *mut sys::_cef_life_span_handler_t {
        self.object.cast()
    }

    fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.tx.try_send(LifeSpanEvent::Created {
            browser_id: browser.identifier().into(),
        }) {
            tracing::warn!(?err, "cannot send life span event");
        }
    }

    fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
        let Some(browser) = browser else {
            return;
        };
        if let Err(err) = self.tx.try_send(LifeSpanEvent::Closed {
            browser_id: browser.identifier().into(),
        }) {
            tracing::warn!(?err, "cannot send life span event");
        }
    }
}
