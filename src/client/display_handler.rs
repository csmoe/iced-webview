use cef::*;
use iced::mouse;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::BrowserId;

#[derive(Clone)]
pub struct IcyDisplayHandler {
    tx: UnboundedSender<DisplayEvent>,
}

#[derive(Debug)]
pub enum DisplayEvent {
    TitleChanged {
        browser_id: BrowserId,
        title: String,
    },
    UrlChanged {
        browser_id: BrowserId,
        url: String,
    },
    CursorChanged {
        browser_id: BrowserId,
        cursor_type: mouse::Interaction,
    },
}

impl IcyDisplayHandler {
    pub fn new() -> (Self, UnboundedReceiver<DisplayEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        (Self { tx }, rx)
    }
}

wrap_display_handler! {
    pub(crate) struct DisplayHandlerBuilder {
        display_handler: IcyDisplayHandler,
    }

    impl DisplayHandler {
        fn on_cursor_change(
            &self,
            browser: Option<&mut Browser>,
            /*
            #[cfg(target_os = "windows")] _cursor: sys::HCURSOR,
            #[cfg(target_os = "macos")] _cursor: *mut u8,
            #[cfg(target_os = "linux")] _cursor: ::std::os::raw::c_ulong,
            */
            _cursor: *mut u8,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            let Some(browser) = browser else {
                return false as _;
            };
            _ = self
                .display_handler
                .tx
                .send(DisplayEvent::CursorChanged {
                    browser_id: browser.identifier().into(),
                    cursor_type: map_cursor(type_),
                })
                .inspect_err(|err| {
                    tracing::error!(?err, "cannot send cursor changed event");
                });
            true as _
        }

        fn on_address_change(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, url: Option<&CefString>) {
            let Some(browser) = browser else {
                return;
            };

            let Some(url) = url else {
                return;
            };
            let event = DisplayEvent::UrlChanged {
                browser_id: browser.identifier().into(),
                url: url.to_string(),
            };
            if let Err(err) = self.display_handler.tx.send(event) {
                tracing::error!(%err, "cannot send loading end event");
            }
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let Some(browser) = browser else {
                return;
            };
            let Some(title) = title else {
                return;
            };
            let event = DisplayEvent::TitleChanged {
                browser_id: browser.identifier().into(),
                title: title.to_string(),
            };
            if let Err(err) = self.display_handler.tx.send(event) {
                tracing::error!(%err, "cannot send loading end event");
            }
        }
    }
}

impl DisplayHandlerBuilder {
    pub(crate) fn build(display: IcyDisplayHandler) -> DisplayHandler {
        Self::new(display)
    }
}

fn map_cursor(type_: cef::CursorType) -> mouse::Interaction {
    use cef::sys::cef_cursor_type_t;
    match type_.as_ref() {
        cef_cursor_type_t::CT_HAND => mouse::Interaction::Pointer,
        cef_cursor_type_t::CT_WAIT => mouse::Interaction::Wait,
        cef_cursor_type_t::CT_HELP => mouse::Interaction::Help,
        cef_cursor_type_t::CT_EASTWESTRESIZE
        | cef_cursor_type_t::CT_WESTRESIZE
        | cef_cursor_type_t::CT_EASTRESIZE => mouse::Interaction::ResizingHorizontally,
        cef_cursor_type_t::CT_NORTHWESTRESIZE
        | cef_cursor_type_t::CT_SOUTHEASTRESIZE
        | cef_cursor_type_t::CT_NORTHWESTSOUTHEASTRESIZE => {
            mouse::Interaction::ResizingDiagonallyDown
        }
        cef_cursor_type_t::CT_NORTHSOUTHRESIZE
        | cef_cursor_type_t::CT_SOUTHRESIZE
        | cef_cursor_type_t::CT_NORTHRESIZE => mouse::Interaction::ResizingVertically,
        cef_cursor_type_t::CT_NORTHEASTSOUTHWESTRESIZE
        | cef_cursor_type_t::CT_SOUTHWESTRESIZE
        | cef_cursor_type_t::CT_NORTHEASTRESIZE => mouse::Interaction::ResizingDiagonallyUp,
        cef_cursor_type_t::CT_CELL => mouse::Interaction::Cell,
        cef_cursor_type_t::CT_IBEAM => mouse::Interaction::Text,
        cef_cursor_type_t::CT_MOVE => mouse::Interaction::Move,
        cef_cursor_type_t::CT_COPY => mouse::Interaction::Copy,
        cef_cursor_type_t::CT_NOTALLOWED => mouse::Interaction::NotAllowed,
        cef_cursor_type_t::CT_ZOOMIN => mouse::Interaction::ZoomIn,
        cef_cursor_type_t::CT_ZOOMOUT => mouse::Interaction::ZoomOut,
        cef_cursor_type_t::CT_GRAB => mouse::Interaction::Grab,
        cef_cursor_type_t::CT_GRABBING => mouse::Interaction::Grabbing,
        cef_cursor_type_t::CT_CROSS => mouse::Interaction::Crosshair,
        cef_cursor_type_t::CT_ALIAS => mouse::Interaction::Alias,
        cef_cursor_type_t::CT_NODROP => mouse::Interaction::NoDrop,
        cef_cursor_type_t::CT_PROGRESS => mouse::Interaction::Progress,
        cef_cursor_type_t::CT_DND_NONE
        | cef_cursor_type_t::CT_COLUMNRESIZE
        | cef_cursor_type_t::CT_ROWRESIZE
        | cef_cursor_type_t::CT_VERTICALTEXT
        | cef_cursor_type_t::CT_DND_MOVE
        | cef_cursor_type_t::CT_DND_COPY
        | cef_cursor_type_t::CT_DND_LINK
        | cef_cursor_type_t::CT_MIDDLE_PANNING_VERTICAL
        | cef_cursor_type_t::CT_MIDDLEPANNING
        | cef_cursor_type_t::CT_EASTPANNING
        | cef_cursor_type_t::CT_NORTHPANNING
        | cef_cursor_type_t::CT_NORTHEASTPANNING
        | cef_cursor_type_t::CT_NORTHWESTPANNING
        | cef_cursor_type_t::CT_SOUTHPANNING
        | cef_cursor_type_t::CT_SOUTHEASTPANNING
        | cef_cursor_type_t::CT_SOUTHWESTPANNING
        | cef_cursor_type_t::CT_WESTPANNING
        | cef_cursor_type_t::CT_CONTEXTMENU
        | cef_cursor_type_t::CT_MIDDLE_PANNING_HORIZONTAL
        | cef_cursor_type_t::CT_CUSTOM
        | cef_cursor_type_t::CT_NUM_VALUES
        | _ => mouse::Interaction::None,
    }
}
