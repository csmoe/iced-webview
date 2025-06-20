use cef::{
    self, BrowserProcessHandler, ImplBrowserProcessHandler, WrapBrowserProcessHandler,
    rc::{Rc, RcImpl},
    sys, *,
};
use std::{cell::RefCell, collections::BTreeMap, time::Duration};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{BrowserId, BrowserProcessMessage, IcyClientState, instance::LaunchId};

#[derive(Clone, Debug)]
pub struct IcyCefApp {
    osr_browsers: std::rc::Rc<RefCell<BTreeMap<BrowserId, LaunchId>>>,
    states: BTreeMap<LaunchId, IcyClientState>,
}

impl IcyCefApp {
    pub fn new() -> Self {
        Self {
            osr_browsers: std::rc::Rc::new(RefCell::new(BTreeMap::new())),
            states: BTreeMap::new(),
        }
    }
    pub fn add_osr_browser(&mut self, browser_id: BrowserId, id: LaunchId) {
        self.osr_browsers.borrow_mut().insert(browser_id, id);
    }

    pub fn remove_osr_browser(&mut self, browser_id: BrowserId) {
        self.osr_browsers.borrow_mut().remove(&browser_id);
    }

    pub fn add_state(&mut self, id: LaunchId, state: IcyClientState) {
        self.states.insert(id, state);
    }

    pub fn remove_state(&mut self, id: LaunchId) {
        self.states.remove(&id);
    }

    pub fn get(&self, browser_id: BrowserId) -> Option<&IcyClientState> {
        if let Some(launch_id) = self.osr_browsers.borrow().get(&browser_id) {
            return self.states.get(launch_id);
        }
        return None;
    }

    pub fn get_mut(&mut self, browser_id: BrowserId) -> Option<&mut IcyClientState> {
        if let Some(launch_id) = self.osr_browsers.borrow().get(&browser_id) {
            return self.states.get_mut(launch_id);
        }
        return None;
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

pub(crate) struct BrowserProcessHandlerBuilder {
    object: *mut RcImpl<sys::cef_browser_process_handler_t, Self>,
    handler: IcyBrowserProcessHandler,
}

impl BrowserProcessHandlerBuilder {
    pub(crate) fn build(handler: IcyBrowserProcessHandler) -> BrowserProcessHandler {
        BrowserProcessHandler::new(Self {
            object: std::ptr::null_mut(),
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
        tracing::info!("cef context intialized");
        _ = self.handler.tx.send(BrowserProcessMessage::Ready);
    }

    fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
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
        command_line.append_switch(Some(&"ignore-certificate-errors".into()));
        command_line.append_switch(Some(&"ignore-ssl-errors".into()));
    }

    fn on_schedule_message_pump_work(&self, delay_ms: i64) {
        if delay_ms <= 0 {
            return;
        }
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
