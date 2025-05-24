mod context_menu;
mod lifespan;
mod load;
mod render;
mod request_context;

use context_menu::IcyContextMenuHandler;
use lifespan::IcyLifeSpanHandler;
pub use lifespan::LifeSpanEvent;
use load::IcyLoadHandler;
pub use render::*;
pub(crate) use request_context::IcyRequestContextHandler;
pub(crate) use request_context::IcyRequestContextHandler;

use cef::Client;
use cef::ContextMenuHandler;
use cef::ImplClient;
use cef::LifeSpanHandler;
use cef::LoadHandler;
use cef::WrapClient;
use cef::rc::*;
use cef::sys;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::UnboundedReceiver;

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

impl IcyClient {
    pub fn new() -> (Self, IcyClientState) {
        let (load_tx, load_rx) = tokio::sync::mpsc::unbounded_channel();
        //let (create_tx, create_rx) = oneshot::channel();
        //let (close_tx, close_rx) = oneshot::channel();
        let (lifespan_tx, lifespan_rx) = tokio::sync::mpsc::channel(2);
        let client = IcyClient {
            load_rx,
            lifespan_rx,
        };

        let handlers = IcyClientHandlers {
            context_menu: context_menu::IcyContextMenuHandler::new(),
            load: load::IcyLoadHandler::new(load_tx),
            lifespan: lifespan::IcyLifeSpanHandler::new(lifespan_tx),
        };
        (client, handlers)
    }
}

pub struct IcyClientHandlers {
    pub(crate) context_menu: context_menu::IcyContextMenuHandler,
    pub(crate) lifespan: lifespan::IcyLifeSpanHandler,
    pub(crate) load: load::IcyLoadHandler,
    pub(crate) render: render::IcyRenderHandler,
}

pub struct IcyClient {
    object: *mut cef::rc::RcImpl<cef::sys::cef_client_t, Self>,
    load: IcyLoadHandler,
    lifespan: IcyLifeSpanHandler,
    context_menu: IcyContextMenuHandler,
    render: IcyRenderHandler,
    request_context: IcyRequestContextHandler,
}

impl IcyClient {
    pub fn new(device_scale_factor: f32, rect: iced::Rectangle) -> (Self, IcyClientState) {
        let (render, render_state) = render::IcyRenderHandler::new(device_scale_factor, rect);
        let client = IcyClient {
            object: std::ptr::null_mut(),
            render,
            lifespan,
            load,
        };
    }

    fn into_cef_client(self) -> Client {
        Client::new(self)
    }
}

pub struct IcyClientState {
    render: IcyRenderState,
}

impl Rc for IcyClient {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapClient for IcyClient {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_client_t, Self>) {
        self.object = object;
    }
}

impl Clone for IcyClient {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            load: self.load.clone(),
            lifespan: self.lifespan.clone(),
            context_menu: self.context_menu.clone(),
            render: self.render.clone(),
            request_context: self.request_context.clone(),
        }
    }
}

impl ImplClient for IcyClient {
    fn get_raw(&self) -> *mut sys::_cef_client_t {
        self.object.cast()
    }

    fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
        Some(ContextMenuHandler::new(self.context_menu.clone()))
    }

    fn life_span_handler(&self) -> Option<LifeSpanHandler> {
        Some(LifeSpanHandler::new(self.lifespan.clone()))
    }

    fn load_handler(&self) -> Option<LoadHandler> {
        Some(LoadHandler::new(self.load.clone()))
    }
}
