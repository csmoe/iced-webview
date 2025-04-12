use std::time::Duration;

use cef::App;
use cef::ImplApp;
use cef::WrapApp;
use cef::rc::{Rc, RcImpl};
use cef::{BrowserProcessHandler, ImplBrowserProcessHandler};
use cef::{WrapBrowserProcessHandler, sys};

use iced::wgpu::core::command;
use tokio::sync::mpsc::UnboundedSender;

pub struct IcyCefApp {}

impl IcyCefApp {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct AppBuilder {
    object: *mut RcImpl<sys::cef_app_t, Self>,
    browser_handler: IcyBrowserProcessHandler,
}

impl AppBuilder {
    pub fn build(handler: IcyBrowserProcessHandler) -> App {
        App::new(Self {
            object: std::ptr::null_mut(),
            browser_handler: handler,
        })
    }
}

impl Rc for AppBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for AppBuilder {
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

impl WrapApp for AppBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_app_t, Self>) {
        self.object = object;
    }
}

impl ImplApp for AppBuilder {
    fn get_raw(&self) -> *mut sys::_cef_app_t {
        self.object.cast()
    }
    fn on_before_command_line_processing(
        &self,
        process_type: Option<&cef::CefString>,
        command_line: Option<&mut impl cef::ImplCommandLine>,
    ) {
        if let Some(_process_type) = process_type {
            tracing::info!("on_before_command_line_processing");
        }
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
    }
}

#[derive(Clone)]
pub struct IcyBrowserProcessHandler {
    tx: UnboundedSender<BrowserProcessMessage>,
}

pub enum BrowserProcessMessage {
    Ready,
    Tick(Duration),
}

impl IcyBrowserProcessHandler {
    pub fn new(tx: UnboundedSender<BrowserProcessMessage>) -> Self {
        Self { tx }
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
        if let Err(err) = self.handler.tx.send(BrowserProcessMessage::Ready) {
            tracing::warn!(?err, "cannot send browser process message");
        } else {
            tracing::info!("cef context intialized");
        }
    }

    fn on_before_child_process_launch(&self, command_line: Option<&mut impl cef::ImplCommandLine>) {
        let Some(command_line) = command_line else {
            return;
        };

        command_line.append_switch(Some(&"disable-javascript-open-windows".into()));
        command_line.append_switch(Some(&"disable-web-security".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-chrome-login-prompt".into()));
        command_line.append_switch(Some(&"allow-running-insecure-content".into()));
        //command_line.append_switch(Some(&"no-startup-window".into()));
        //command_line.append_switch(Some(&"disable-popup-blocking".into()));
        //command_line.append_switch(Some(&"noerrdialogs".into()));
        command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
        command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        //command_line.append_switch(Some(&"disable-gpu".into()));
        //command_line.append_switch(Some(&"disable-gpu-compositing".into()));
        command_line.append_switch(Some(&"ignore-certificate-errors".into()));
        command_line.append_switch(Some(&"ignore-ssl-errors".into()));
        //command_line.append_switch(Some(&"--use-gl=angle".into()));
        //command_line.append_switch(Some(&"--use-angle=swiftshader".into()));
        //command_line.append_switch(Some(&"enable-unsafe-swiftshader".into()));
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
