use cef::{self, DisplayHandler, ImplBrowser};
use cef::{
    Client, ContextMenuHandler, ImplClient, ImplProcessMessage, KeyboardHandler, LifeSpanHandler,
    LoadHandler, RenderHandler, WrapClient,
    rc::{Rc, RcImpl},
    sys,
};
use context_menu_handler::{ContextMenuHandlerBuilder, IcyContextMenuHandler};
use keyboard_handler::{IcyKeyboardHandler, IcyKeyboardState, KeyboardHandlerBuilder};
use lifespan_handler::{IcyLifeSpanHandler, LifeSpanHandlerBuilder};

use load_handler::{IcyLoadHandler, LoadHandlerBuilder};
use render_handler::{IcyRenderHandler, IcyRenderState, RenderHandlerBuilder};
use std::ptr::null_mut;
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    client::display_handler::{DisplayHandlerBuilder, IcyDisplayHandler, IcyDisplayState},
    instance::LaunchId,
};
mod context_menu_handler;
mod display_handler;
mod keyboard_handler;
mod lifespan_handler;
mod load_handler;
mod render_handler;

pub use lifespan_handler::LifeSpanEvent;
pub use load_handler::LoadEvent;

pub use render_handler::CefFrame;

pub struct ClientEventSubscriber {
    pub lifespan_rx: Receiver<LifeSpanEvent>,
    pub load_rx: UnboundedReceiver<LoadEvent>,
    pub process_message_rx: UnboundedReceiver<CefIpcMessage>,
    pub render_rx: UnboundedReceiver<CefFrame>,
}

pub struct IcyClient {
    pub state: IcyClientState,
    pub subscribers: ClientEventSubscriber,
}

#[derive(Clone, Debug)]
pub struct IcyClientState {
    pub render: IcyRenderState,
    pub keyboard: IcyKeyboardState,
    pub display: IcyDisplayState,
}

impl IcyClient {
    pub fn new(
        launch_id: LaunchId,
        device_scale_factor: f32,
        view_rect: cef::Rect,
    ) -> (Self, IcyClientHandlers) {
        let (load_handler, load_rx) = IcyLoadHandler::new();
        let (display_handler, display_state) = IcyDisplayHandler::new();
        let (render_handler, render_state, render_rx) =
            IcyRenderHandler::new(device_scale_factor, view_rect);
        let (lifespan_handler, lifespan_rx) = IcyLifeSpanHandler::new(launch_id);
        let context_menu_handler = IcyContextMenuHandler::new();
        let (keyboard_handler, keyboard_state) = IcyKeyboardHandler::new();
        let (process_message_tx, process_message_rx) = unbounded_channel();
        let state = IcyClientState {
            render: render_state,
            keyboard: keyboard_state,
            display: display_state,
        };
        let subscribers = ClientEventSubscriber {
            lifespan_rx,
            load_rx,
            render_rx,
            process_message_rx,
        };
        let handlers = IcyClientHandlers {
            load_handler,
            lifespan_handler,
            render_handler,
            context_menu_handler,
            display_handler,
            keyboard_handler,
            process_message_tx,
        };
        (Self { state, subscribers }, handlers)
    }
}

#[derive(Clone)]
pub struct IcyClientHandlers {
    load_handler: IcyLoadHandler,
    lifespan_handler: IcyLifeSpanHandler,
    render_handler: IcyRenderHandler,
    context_menu_handler: IcyContextMenuHandler,
    keyboard_handler: IcyKeyboardHandler,
    display_handler: IcyDisplayHandler,
    process_message_tx: UnboundedSender<CefIpcMessage>,
}

pub enum CefIpcMessage {
    FocusedNodeChanged {
        browser_id: i32,
        x: f32,
        y: f32,
        width: i32,
        height: i32,
    },
    CaretPositionChanged {
        browser_id: i32,
        offset: f32,
    },
}

pub(crate) struct ClientBuilder {
    object: *mut RcImpl<sys::cef_client_t, Self>,
    load_handler: LoadHandler,
    lifespan_handler: LifeSpanHandler,
    render_handler: RenderHandler,
    context_menu_handler: ContextMenuHandler,
    display_handler: DisplayHandler,
    keyboard_handler: KeyboardHandler,
    process_message_tx: UnboundedSender<CefIpcMessage>,
}

impl ClientBuilder {
    pub(crate) fn build(client_handlers: IcyClientHandlers) -> Client {
        let IcyClientHandlers {
            load_handler,
            lifespan_handler,
            render_handler,
            context_menu_handler,
            keyboard_handler,
            display_handler,
            process_message_tx,
        } = client_handlers;
        let load_handler = LoadHandlerBuilder::build(load_handler);
        let lifespan_handler = LifeSpanHandlerBuilder::build(lifespan_handler);
        let render_handler = RenderHandlerBuilder::build(render_handler);
        let context_menu_handler = ContextMenuHandlerBuilder::build(context_menu_handler);
        let keyboard_handler = KeyboardHandlerBuilder::build(keyboard_handler);
        let display_handler = DisplayHandlerBuilder::build(display_handler);
        Client::new(Self {
            object: null_mut(),
            load_handler,
            display_handler,
            lifespan_handler,
            render_handler,
            context_menu_handler,
            keyboard_handler,
            process_message_tx,
        })
    }
}

impl Rc for ClientBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapClient for ClientBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::cef_client_t, Self>) {
        self.object = object;
    }
}

impl Clone for ClientBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            load_handler: self.load_handler.clone(),
            render_handler: self.render_handler.clone(),
            lifespan_handler: self.lifespan_handler.clone(),
            context_menu_handler: self.context_menu_handler.clone(),
            keyboard_handler: self.keyboard_handler.clone(),
            display_handler: self.display_handler.clone(),
            process_message_tx: self.process_message_tx.clone(),
        }
    }
}

impl ImplClient for ClientBuilder {
    fn get_raw(&self) -> *mut sys::_cef_client_t {
        self.object.cast()
    }

    fn load_handler(&self) -> Option<cef::LoadHandler> {
        Some(self.load_handler.clone())
    }

    fn life_span_handler(&self) -> Option<cef::LifeSpanHandler> {
        Some(self.lifespan_handler.clone())
    }

    fn render_handler(&self) -> Option<cef::RenderHandler> {
        Some(self.render_handler.clone())
    }

    fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
        Some(self.context_menu_handler.clone())
    }

    fn keyboard_handler(&self) -> Option<KeyboardHandler> {
        Some(self.keyboard_handler.clone())
    }

    fn display_handler(&self) -> Option<DisplayHandler> {
        Some(self.display_handler.clone())
    }

    fn on_process_message_received(
        &self,
        browser: Option<&mut cef::Browser>,
        _frame: Option<&mut cef::Frame>,
        _source_process: cef::ProcessId,
        message: Option<&mut cef::ProcessMessage>,
    ) -> std::ffi::c_int {
        use cef::ImplListValue;
        let Some(browser) = browser else {
            return false as _;
        };
        let Some(message) = message else {
            return false as _;
        };
        let event_name =
            cef::CefStringUtf8::from(&cef::CefString::from(&message.name())).to_string();
        if "renderer.caret_offset_changed" == &event_name {
            if let Some(bound) = message.argument_list().map(|args| args.string(0)) {
                #[derive(serde::Deserialize)]
                struct Offset {
                    offset: f32,
                }
                let Ok(Offset { offset }) = serde_json::from_str(
                    &cef::CefStringUtf8::from(&cef::CefString::from(&bound)).to_string(),
                ) else {
                    return false as _;
                };
                if let Err(err) =
                    self.process_message_tx
                        .send(CefIpcMessage::CaretPositionChanged {
                            browser_id: browser.identifier(),
                            offset,
                        })
                {
                    tracing::error!(?err, "cannot send ipc message event");
                }
                return true as _;
            }
        }
        if "renderer.focused_node" == &event_name {
            if let Some(bound) = message.argument_list().map(|args| args.string(0)) {
                #[derive(serde::Deserialize)]
                struct Node {
                    x: f32,
                    y: f32,
                    width: f32,
                    height: f32,
                }
                let Ok(Node {
                    x,
                    y,
                    width,
                    height,
                }) = serde_json::from_str(
                    &cef::CefStringUtf8::from(&cef::CefString::from(&bound)).to_string(),
                )
                else {
                    return false as _;
                };
                if let Err(err) = self
                    .process_message_tx
                    .send(CefIpcMessage::FocusedNodeChanged {
                        browser_id: browser.identifier(),
                        x: x / 2.0,
                        y: y / 2.0,
                        width: (height / 2.0).floor() as _,
                        height: (width / 2.0).floor() as _,
                    })
                {
                    tracing::error!(?err, "cannot send ipc message event");
                }
                return true as _;
            }
        }
        return false as _;
    }
}
