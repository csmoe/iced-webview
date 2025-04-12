mod context_menu;
mod lifespan;
mod load;

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

#[derive(Clone, Copy, Debug)]
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

pub struct IcyClient {
    pub(crate) load_rx: UnboundedReceiver<load::LoadEvent>,
    pub(crate) lifespan_rx: Receiver<lifespan::LifeSpanEvent>,
}

impl IcyClient {
    pub fn new() -> (Self, IcyClientHandlers) {
        let (load_tx, load_rx) = tokio::sync::mpsc::unbounded_channel();
        let (lifespan_tx, lifespan_rx) = tokio::sync::mpsc::channel(1);

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
}

pub struct ClientBuilder {
    object: *mut cef::rc::RcImpl<cef::sys::cef_client_t, Self>,
    load: LoadHandler,
    lifespan: LifeSpanHandler,
    context_menu: ContextMenuHandler,
}

impl ClientBuilder {
    pub fn build(handlers: IcyClientHandlers) -> Client {
        let IcyClientHandlers {
            context_menu,
            lifespan,
            load,
        } = handlers;
        Client::new(Self {
            object: std::ptr::null_mut(),
            load: load::LoadHandlerBuilder::build(load),
            lifespan: lifespan::LifeSpanHandlerBuilder::build(lifespan),
            context_menu: context_menu::ContextMenuHandlerBuilder::build(context_menu),
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
            load: self.load.clone(),
            lifespan: self.lifespan.clone(),
            context_menu: self.context_menu.clone(),
        }
    }
}

impl ImplClient for ClientBuilder {
    fn get_raw(&self) -> *mut sys::_cef_client_t {
        self.object.cast()
    }

    fn get_context_menu_handler(&self) -> Option<ContextMenuHandler> {
        Some(self.context_menu.clone())
    }

    fn get_life_span_handler(&self) -> Option<LifeSpanHandler> {
        Some(self.lifespan.clone())
    }

    fn get_load_handler(&self) -> Option<LoadHandler> {
        Some(self.load.clone())
    }
}
