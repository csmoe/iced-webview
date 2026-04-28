use std::fmt::Debug;

use cef::{ImplBrowser, ImplBrowserHost, *};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    BrowserId,
    instance::{LaunchId, new_webview, remove_webview},
};

#[derive(Clone)]
pub enum LifeSpanEvent {
    Created {
        browser_id: BrowserId,
        is_popup: bool,
    },
    Closed {
        browser_id: BrowserId,
    },
    CreatePopup {
        browser_id: BrowserId,
        target_url: String,
    },
}

impl Debug for LifeSpanEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed { browser_id } => f
                .debug_struct("LifeSpanEvent::Closed")
                .field("browser_id", &browser_id)
                .finish(),
            Self::Created {
                browser_id,
                is_popup,
            } => f
                .debug_struct("LifeSpanEvent::Created")
                .field("browser_id", &browser_id)
                .field("is_popup", &is_popup)
                .finish(),
            Self::CreatePopup { browser_id, .. } => f
                .debug_struct("LifeSpanEvent::CreatePopup")
                .field("browser_id", &browser_id)
                .finish(),
        }
    }
}

#[derive(Clone)]
pub struct IcyLifeSpanHandler {
    tx: Sender<LifeSpanEvent>,
    launch_id: std::cell::RefCell<LaunchId>,
}

impl IcyLifeSpanHandler {
    pub fn new(launch_id: LaunchId) -> (Self, Receiver<LifeSpanEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        (
            Self {
                tx,
                launch_id: std::cell::RefCell::new(launch_id),
            },
            rx,
        )
    }
}

wrap_life_span_handler! {
    pub(crate) struct LifeSpanHandlerBuilder {
        handler: IcyLifeSpanHandler,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
            let Some(browser) = browser else {
                return;
            };
            if let Some(host) = browser.host() {
                host.set_focus(false as _);
            }

            let browser_id = browser.identifier().into();
            let is_popup = browser.is_popup() == 1;
            let event = LifeSpanEvent::Created {
                browser_id,
                is_popup,
            };
            tracing::info!(?event, "browser created in cef");
            if self
                .handler
                .tx
                .try_send(event)
                .inspect_err(|err| {
                    tracing::warn!(?browser_id, is_popup, %err, "cannot send life span event");
                })
                .is_ok()
            {
                new_webview(*self.handler.launch_id.borrow(), browser_id);
            }
        }

        fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
            let Some(browser) = browser else {
                return;
            };
            let browser_id = browser.identifier().into();

            remove_webview(browser_id);
            let is_popup = browser.is_popup() == 1;
            tracing::info!(?browser_id, "browser closed");
            if let Err(err) = self.handler.tx.try_send(LifeSpanEvent::Closed { browser_id }) {
                tracing::warn!(?browser_id, is_popup, %err, "cannot send life span event");
            }
        }

        fn on_before_popup(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut cef::Frame>,
            popup_id: ::std::os::raw::c_int,
            target_url: Option<&cef::CefString>,
            _target_frame_name: Option<&cef::CefString>,
            _target_disposition: cef::WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&cef::PopupFeatures>,
            _window_info: Option<&mut cef::WindowInfo>,
            _client: Option<&mut Option<cef::Client>>,
            _settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            tracing::info!(popup_id, user_gesture, "new popup browser");
            if let Some(browser) = browser
                && let Some(target_url) = target_url.map(|url| url.to_string())
            {
                if let Err(err) = self.handler.tx.try_send(LifeSpanEvent::CreatePopup {
                    browser_id: browser.identifier().into(),
                    target_url,
                }) {
                    tracing::warn!(%err, "cannot send life span event");
                }

                tracing::info!(popup_id, "popup to be created");

                return true as _;
            }
            true as _
        }

        fn on_before_popup_aborted(
            &self,
            browser: Option<&mut cef::Browser>,
            popup_id: ::std::os::raw::c_int,
        ) {
            let browser_id: Option<BrowserId> = browser.map(|b| b.identifier().into());
            tracing::warn!(?browser_id, popup_id, "popup aborted");
        }

        fn on_before_dev_tools_popup(
            &self,
            _browser: Option<&mut cef::Browser>,
            _window_info: Option<&mut cef::WindowInfo>,
            _client: Option<&mut Option<cef::Client>>,
            _settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            use_default_window: Option<&mut ::std::os::raw::c_int>,
        ) {
            tracing::info!("open devtools window");
            if let Some(default_window) = use_default_window {
                *default_window = true as _;
            }
        }
    }
}

impl LifeSpanHandlerBuilder {
    pub fn build(handler: IcyLifeSpanHandler) -> LifeSpanHandler {
        Self::new(handler)
    }
}
