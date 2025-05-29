use cef::ImplApp;
use cef::ImplBrowserProcessHandler;
use cef::ImplCommandLine;
use cef::WrapApp;
use cef::rc::{Rc, RcImpl};
use cef::{WrapBrowserProcessHandler, sys};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::ptr::null_mut;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::unbounded_channel;

use tokio::sync::mpsc::UnboundedSender;

use crate::BrowserId;
use crate::IcyClientState;

#[derive(Clone)]
pub struct IcyCefApp {
    osr_browsers: std::rc::Rc<RefCell<BTreeMap<BrowserId, IcyClientState>>>,
    state_queue: std::rc::Rc<RefCell<BTreeMap<i32, IcyClientState>>>,
}

impl IcyCefApp {
    pub fn new() -> Self {
        Self {
            osr_browsers: std::rc::Rc::new(RefCell::new(BTreeMap::new())),
            state_queue: std::rc::Rc::new(RefCell::new(BTreeMap::new())),
        }
    }
    pub fn add_osr_browser(&mut self, browser_id: BrowserId, client_state: IcyClientState) {
        self.osr_browsers
            .borrow_mut()
            .insert(browser_id, client_state);
    }

    pub fn remove_osr_browser(&mut self, browser_id: BrowserId) {
        self.osr_browsers.borrow_mut().remove(&browser_id);
    }

}

pub(crate) struct AppBuilder {
    object: *mut RcImpl<cef::sys::_cef_app_t, Self>,
    app: IcyCefApp,
    browser_handler: cef::BrowserProcessHandler,
}

impl AppBuilder {
    pub(crate) fn build(app: IcyCefApp, browser_handler: IcyBrowserProcessHandler) -> cef::App {
        cef::App::new(Self {
            object: std::ptr::null_mut(),
            app,
            browser_handler: BrowserProcessHandlerBuilder::build(browser_handler),
        })
    }
}

impl WrapApp for AppBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef::sys::_cef_app_t, Self>) {
        self.object = object;
    }
}

impl Clone for AppBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc = &mut *self.object;
            rc.interface.add_ref();
            self.object
        };
        Self {
            object,
            app: self.app.clone(),
            browser_handler: self.browser_handler.clone(),
        }
    }
}

impl Rc for AppBuilder {
    fn as_base(&self) -> &cef::sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplApp for AppBuilder {
    fn get_raw(&self) -> *mut cef::sys::_cef_app_t {
        self.object as *mut cef::sys::_cef_app_t
    }

    fn on_before_command_line_processing(
        &self,
        process_type: Option<&cef::CefStringUtf16>,
        command_line: Option<&mut cef::CommandLine>,
    ) {
        if let Some(process_type) = process_type {
            tracing::info!(
                process_type = cef::CefStringUtf8::from(process_type).to_string(),
                "process launching",
            );
        }
        let Some(command_line) = command_line else {
            tracing::error!("no command line");
            return;
        };

        command_line.append_switch(Some(&"disable-javascript-open-windows".into()));
        command_line.append_switch(Some(&"disable-web-security".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-chrome-login-prompt".into()));
        command_line.append_switch(Some(&"allow-running-insecure-content".into()));
        command_line.append_switch(Some(&"no-startup-window".into()));
        command_line.append_switch(Some(&"disable-popup-blocking".into()));
        command_line.append_switch(Some(&"noerrdialogs".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"use-mock-keychain".into()));
        command_line.append_switch(Some(&"disable-spell-checking".into()));
        command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        command_line
            .append_switch_with_value(Some(&"remote-debugging-port".into()), Some(&"9229".into()));
        tracing::info!("pre-set command line done");
    }

    fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
        Some(self.browser_handler.clone())
    }
}

#[derive(Clone)]
pub struct IcyBrowserProcessHandler {
    tx: UnboundedSender<BrowserProcessMessage>,
}

impl IcyBrowserProcessHandler {
    pub fn new() -> (Self, UnboundedReceiver<BrowserProcessMessage>) {
        let (tx, rx) = unbounded_channel();
        (Self { tx }, rx)
    }
}

pub struct BrowserProcessHandlerBuilder {
    object: *mut RcImpl<sys::cef_browser_process_handler_t, Self>,
    handler: IcyBrowserProcessHandler,
}

pub enum BrowserProcessMessage {
    Ready,
    Tick(Duration),
}

impl BrowserProcessHandlerBuilder {
    pub fn build(handler: IcyBrowserProcessHandler) -> cef::BrowserProcessHandler {
        cef::BrowserProcessHandler::new(Self {
            object: null_mut(),
            handler,
        })
    }
}

impl Rc for BrowserProcessHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapBrowserProcessHandler for BrowserProcessHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_browser_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for BrowserProcessHandlerBuilder {
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

impl ImplBrowserProcessHandler for BrowserProcessHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_browser_process_handler_t {
        self.object.cast()
    }

    #[tracing::instrument(skip(self))]
    fn on_context_initialized(&self) {
        if let Err(err) = self.handler.tx.send(BrowserProcessMessage::Ready) {
            tracing::warn!(?err, "cannot send browser process message");
        } else {
            tracing::debug!("cef context intialized");
        }
    }

    fn on_before_child_process_launch(&self, command_line: Option<&mut cef::CommandLine>) {
        let Some(command_line) = command_line else {
            return;
        };

        command_line.append_switch(Some(&"disable-javascript-open-windows".into()));
        command_line.append_switch(Some(&"disable-web-security".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-chrome-login-prompt".into()));
        command_line.append_switch(Some(&"allow-running-insecure-content".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
    }

    fn on_schedule_message_pump_work(&self, delay_ms: i64) {
        if let Err(err) = self
            .handler
            .tx
            .send(BrowserProcessMessage::Tick(Duration::from_millis(
                delay_ms as _,
            )))
        {
            tracing::warn!(?err, "cannot pump cef loop");
        }
    }
}
