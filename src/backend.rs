mod context_menu;
mod keyboard;
mod lifespan;
mod load;
mod render;
mod request_context;

use std::ptr::null_mut;

use cef::RenderHandler;
use context_menu::ContextMenuHandlerBuilder;
use context_menu::IcyContextMenuHandler;
use keyboard::IcyKeyboardHandler;
use keyboard::KeyboardHandlerBuilder;
use lifespan::IcyLifeSpanHandler;
pub use lifespan::LifeSpanEvent;
use lifespan::LifeSpanHandlerBuilder;
use load::IcyLoadHandler;
use load::LoadHandlerBuilder;
pub use render::*;
pub(crate) use request_context::IcyRequestContextHandler;
pub(crate) use request_context::RequestContextHandlerBuilder;

use cef::ContextMenuHandler;
use cef::ImplClient;
use cef::KeyboardHandler;
use cef::LifeSpanHandler;
use cef::LoadHandler;
use cef::WrapClient;
use cef::rc::*;
use cef::sys;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash, Ord)]
pub struct BrowserId(i32);

impl From<i32> for BrowserId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

impl BrowserId {
    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

pub struct ClientEventSubscriber {
    pub lifespan_rx: Receiver<LifeSpanEvent>,
    pub load_rx: UnboundedReceiver<load::LoadEvent>,
}

pub struct IcyClient {
    pub state: IcyClientState,
    pub events: ClientEventSubscriber,
}

pub enum CefIpcMessage {
    EditableNodeFocused {
        x: f32,
        y: f32,
        width: i32,
        height: i32,
    },
    CaretPositionChanged {
        offset: f32,
    },
}

#[derive(Clone)]
pub struct ClientHandlers {
    load_handler: IcyLoadHandler,
    lifespan_handler: IcyLifeSpanHandler,
    render_handler: IcyRenderHandler,
    context_menu_handler: IcyContextMenuHandler,
    keyboard_handler: IcyKeyboardHandler,
    process_message_tx: UnboundedSender<CefIpcMessage>,
}

impl IcyClient {
    pub fn new(device_scale_factor: f32, view_rect: cef::Rect) -> (Self, ClientHandlers) {
        let (load_handler, load_rx) = IcyLoadHandler::new();
        let (render_handler, render_state) = IcyRenderHandler::new(device_scale_factor, view_rect);
        let (lifespan_handler, lifespan_rx) = IcyLifeSpanHandler::new();
        let context_menu_handler = IcyContextMenuHandler::new();
        let keyboard_handler = IcyKeyboardHandler::new();
        let (process_message_tx, _) = unbounded_channel();
        let state = IcyClientState {
            render: render_state,
        };
        let events = ClientEventSubscriber {
            lifespan_rx,
            load_rx,
        };
        let handlers = ClientHandlers {
            load_handler,
            lifespan_handler,
            render_handler,
            context_menu_handler,
            keyboard_handler,
            process_message_tx,
        };
        (Self { state, events }, handlers)
    }
}

pub struct ClientBuilder {
    object: *mut cef::rc::RcImpl<cef::sys::cef_client_t, Self>,
    load_handler: LoadHandler,
    lifespan_handler: LifeSpanHandler,
    render_handler: RenderHandler,
    context_menu_handler: ContextMenuHandler,
    keyboard_handler: KeyboardHandler,
    process_message_tx: UnboundedSender<CefIpcMessage>,
}

impl ClientBuilder {
    pub fn build(client: ClientHandlers) -> cef::Client {
        let ClientHandlers {
            load_handler,
            lifespan_handler,
            render_handler,
            keyboard_handler,
            process_message_tx,
            context_menu_handler,
        } = client;
        let load_handler = LoadHandlerBuilder::build(load_handler);
        let lifespan_handler = LifeSpanHandlerBuilder::build(lifespan_handler);
        let render_handler = RenderHandlerBuilder::build(render_handler);
        let context_menu_handler = ContextMenuHandlerBuilder::build(context_menu_handler);
        let keyboard_handler = KeyboardHandlerBuilder::build(keyboard_handler);
        cef::Client::new(Self {
            object: null_mut(),
            lifespan_handler,
            render_handler,
            context_menu_handler,
            process_message_tx,
            keyboard_handler,
            load_handler,
        })
    }
}

#[derive(Clone)]
pub struct IcyClientState {
    render: IcyRenderState,
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
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_client_t, Self>) {
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
}
