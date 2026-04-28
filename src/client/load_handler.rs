use cef::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::BrowserId;

#[derive(Debug)]
#[allow(unused)]
pub enum LoadEvent {
    Changed {
        browser_id: BrowserId,
        is_loading: bool,
        can_go_forward: bool,
        can_go_back: bool,
    },
    Start {
        browser_id: BrowserId,
        frame_id: String,
        transition_type: u32,
    },
    End {
        browser_id: BrowserId,
        frame_id: String,
        http_status_code: u16,
    },
    Error {
        browser_id: BrowserId,
        frame_id: String,
        error_code: i32,
        error_text: Option<String>,
        failed_url: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct IcyLoadHandler {
    tx: UnboundedSender<LoadEvent>,
}

impl IcyLoadHandler {
    pub fn new() -> (Self, UnboundedReceiver<LoadEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

wrap_load_handler! {
    pub(crate) struct LoadHandlerBuilder {
        load_handler: IcyLoadHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let Some(browser) = browser else {
                return;
            };
            let event = LoadEvent::Changed {
                browser_id: browser.identifier().into(),
                is_loading: is_loading == 1,
                can_go_forward: can_go_forward == 1,
                can_go_back: can_go_back == 1,
            };
            tracing::info!(?event);
            if let Err(err) = self.load_handler.tx.send(event) {
                tracing::error!(%err, "cannot send loading state changing event");
            }
        }

        fn on_load_start(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            transition_type: cef::TransitionType,
        ) {
            let Some(frame) = frame else {
                return;
            };
            let Some(browser) = browser else {
                return;
            };
            let event = LoadEvent::Start {
                browser_id: browser.identifier().into(),
                frame_id: CefString::from(&frame.identifier()).to_string(),
                transition_type: (*transition_type.as_ref()) as _,
            };

            tracing::info!(?event);
            if let Err(err) = self.load_handler.tx.send(event) {
                tracing::error!(%err, "cannot send loading start event");
            }
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(browser) = browser else {
                return;
            };
            let Some(frame) = frame else {
                return;
            };
            let event = LoadEvent::End {
                browser_id: browser.identifier().into(),
                frame_id: CefString::from(&frame.identifier()).to_string(),
                http_status_code: http_status_code as _,
            };
            tracing::info!(?event);
            if let Err(err) = self.load_handler.tx.send(event) {
                tracing::error!(%err, "cannot send loading end event");
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
            let Some(frame) = frame else {
                return;
            };
            let Some(browser) = browser else {
                return;
            };
            let frame_id = CefString::from(&frame.identifier()).to_string();
            let event = LoadEvent::Error {
                browser_id: browser.identifier().into(),
                frame_id,
                error_code: (*error_code.as_ref()) as _,
                error_text: error_text
                    .map(cef::CefStringUtf8::from)
                    .and_then(|s| s.as_str().map(|s| s.to_string())),
                failed_url: failed_url
                    .map(cef::CefStringUtf8::from)
                    .and_then(|s| s.as_str().map(|s| s.to_string())),
            };
            tracing::info!(?event);
            if let Err(err) = self.load_handler.tx.send(event) {
                tracing::error!(%err, "cannot send load start event");
            }
        }
    }
}

impl LoadHandlerBuilder {
    pub(crate) fn build(loader: IcyLoadHandler) -> LoadHandler {
        Self::new(loader)
    }
}
