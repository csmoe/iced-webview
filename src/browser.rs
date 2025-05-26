use cef::App;
use cef::ImplApp;
use cef::ImplCommandLine;
use cef::WrapApp;
use cef::rc::{Rc, RcImpl};
use cef::{BrowserProcessHandler, ImplBrowserProcessHandler};
use cef::{WrapBrowserProcessHandler, sys};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::unbounded_channel;

use tokio::sync::mpsc::UnboundedSender;

pub struct IcyCefApp {
    object: *mut RcImpl<sys::cef_app_t, Self>,
    browser_handler: IcyBrowserProcessHandler,
}

impl IcyCefApp {
    pub fn new(browser_handler: IcyBrowserProcessHandler) -> Self {
        Self {
            object: std::ptr::null_mut(),
            browser_handler,
        }
    }
}

impl Rc for IcyCefApp {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for IcyCefApp {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            browser_handler: self.browser_handler.clone(),
        }
    }
}

impl WrapApp for IcyCefApp {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_app_t, Self>) {
        self.object = object;
    }
}

impl ImplApp for IcyCefApp {
    fn get_raw(&self) -> *mut sys::_cef_app_t {
        self.object.cast()
    }
    fn on_before_command_line_processing(
        &self,
        _process_type: Option<&cef::CefString>,
        command_line: Option<&mut cef::CommandLine>,
    ) {
        let Some(command_line) = command_line else {
            return;
        };
        command_line.append_switch(Some(&"disable-javascript-open-windows".into()));
        command_line.append_switch(Some(&"disable-web-security".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-chrome-login-prompt".into()));
        command_line.append_switch(Some(&"allow-running-insecure-content".into()));
        command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        command_line.append_switch(Some(&"disable-popup-blocking".into()));
        command_line.append_switch(Some(&"noerrdialogs".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"use-mock-keychain".into()));
        command_line.append_switch(Some(&"ignore-certificate-errors".into()));
        command_line.append_switch(Some(&"disable-spell-checking".into()));
        tracing::debug!("preset command line args done");
    }
}

pub struct IcyBrowserProcessHandler {
    object: *mut RcImpl<sys::cef_browser_process_handler_t, Self>,
    tx: UnboundedSender<BrowserProcessMessage>,
}

pub enum BrowserProcessMessage {
    Ready,
    Tick(Duration),
}

impl IcyBrowserProcessHandler {
    pub fn new() -> (Self, UnboundedReceiver<BrowserProcessMessage>) {
        let (tx, rx) = unbounded_channel();
        (
            Self {
                object: std::ptr::null_mut(),
                tx,
            },
            rx,
        )
    }
}

impl Rc for IcyBrowserProcessHandler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapBrowserProcessHandler for IcyBrowserProcessHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_browser_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for IcyBrowserProcessHandler {
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

impl ImplBrowserProcessHandler for IcyBrowserProcessHandler {
    fn get_raw(&self) -> *mut sys::_cef_browser_process_handler_t {
        self.object.cast()
    }

    #[tracing::instrument(skip(self))]
    fn on_context_initialized(&self) {
        if let Err(err) = self.tx.send(BrowserProcessMessage::Ready) {
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
            .tx
            .send(BrowserProcessMessage::Tick(Duration::from_millis(
                delay_ms as _,
            )))
        {
            tracing::warn!(?err, "cannot pump cef loop");
        }
    }
}
