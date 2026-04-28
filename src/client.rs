use arc_swap::ArcSwapOption;
use cef::{self, DisplayHandler, ImplBrowser};
use cef::{
    Client, ContextMenuHandler, ImplClient, ImplProcessMessage, KeyboardHandler, LifeSpanHandler,
    LoadHandler, RenderHandler, WrapClient,
    rc::{Rc, RcImpl},
    sys,
};
use context_menu_handler::{ContextMenuHandlerBuilder, IcyContextMenuHandler};
pub use display_handler::DisplayEvent;
use display_handler::{DisplayHandlerBuilder, IcyDisplayHandler};
use keyboard_handler::{IcyKeyboardHandler, KeyboardHandlerBuilder};
pub use lifespan_handler::LifeSpanEvent;
use lifespan_handler::{IcyLifeSpanHandler, LifeSpanHandlerBuilder};
pub use load_handler::LoadEvent;
use load_handler::{IcyLoadHandler, LoadHandlerBuilder};
pub(crate) use render_handler::{CefView, CefViewWrapper};
use render_handler::{IcyRenderHandler, IcyRenderState, RenderHandlerBuilder};
use std::{
    ptr::null_mut,
    sync::{Arc, Mutex},
};
use tokio::sync::{
    mpsc::{Receiver, UnboundedReceiver, UnboundedSender, unbounded_channel},
    oneshot,
};

use crate::{BrowserId, instance::LaunchId};

mod context_menu_handler;
mod display_handler;
mod keyboard_handler;
mod lifespan_handler;
mod load_handler;
mod render_handler;

pub struct ClientEventSubscriber {
    pub lifespan_rx: Receiver<LifeSpanEvent>,
    pub load_rx: UnboundedReceiver<LoadEvent>,
    pub process_message_rx: UnboundedReceiver<CefIpcMessage>,
    pub render_rx: UnboundedReceiver<BrowserId>,
    pub display_rx: UnboundedReceiver<DisplayEvent>,
}

impl std::fmt::Debug for ClientEventSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientEventSubscriber")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct RelayEvent {
    pub msg: crate::CefMessage,
    pub lifecycle: bool,
}

pub type SubscriberDepositor = Arc<Mutex<Option<oneshot::Sender<ClientEventSubscriber>>>>;

pub struct CefSubscriptionRecipe {
    pub id: crate::instance::LaunchId,
    pub relay_tx: tokio::sync::broadcast::Sender<RelayEvent>,
}

impl iced::advanced::subscription::Recipe for CefSubscriptionRecipe {
    type Output = crate::CefMessage;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        use std::hash::Hash;
        std::any::TypeId::of::<Self>().hash(state);
        self.id.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: std::pin::Pin<
            Box<dyn tokio_stream::Stream<Item = iced::advanced::subscription::Event> + Send>,
        >,
    ) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = crate::CefMessage> + Send>> {
        let mut rx = self.relay_tx.subscribe();
        let (tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(relay_event) => {
                        if tx.send(relay_event.msg).is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(
            output_rx,
        ))
    }
}

impl ClientEventSubscriber {
    pub(crate) fn into_stream(
        self,
    ) -> impl tokio_stream::Stream<Item = RelayEvent> + Send + 'static {
        use tokio_stream::StreamExt;

        let ClientEventSubscriber {
            lifespan_rx,
            load_rx,
            mut process_message_rx,
            render_rx,
            display_rx,
        } = self;

        let (inner_tx, inner_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(msg) = process_message_rx.recv().await {
                let msg = match msg {
                    CefIpcMessage::FocusedNodeChanged {
                        browser_id,
                        x,
                        y,
                        width,
                        height,
                    } => crate::CefMessage::FocusedNodeChanged(
                        browser_id.into(),
                        iced::Rectangle {
                            x: x as _,
                            y: y as _,
                            width: width as _,
                            height: height as _,
                        },
                    ),
                    CefIpcMessage::CaretPositionChanged { browser_id, offset } => {
                        crate::CefMessage::UpdateCaretOffset(browser_id.into(), offset)
                    }
                };
                let _ = inner_tx.send(msg);
            }
        });

        let streams: Vec<std::pin::Pin<Box<dyn tokio_stream::Stream<Item = RelayEvent> + Send>>> = vec![
            Box::pin(
                tokio_stream::wrappers::ReceiverStream::new(lifespan_rx).map(|event| RelayEvent {
                    msg: match event {
                        LifeSpanEvent::Closed { browser_id } => {
                            crate::CefMessage::Closed(browser_id)
                        }
                        LifeSpanEvent::Created { browser_id, .. } => {
                            crate::CefMessage::Created(browser_id)
                        }
                        LifeSpanEvent::CreatePopup { browser_id, .. } => {
                            crate::CefMessage::UpdateView(browser_id)
                        }
                    },
                    lifecycle: true,
                }),
            ),
            Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(load_rx).map(|event| {
                    RelayEvent {
                        msg: match event {
                            LoadEvent::End { browser_id, .. } => {
                                crate::CefMessage::Loaded(browser_id)
                            }
                            LoadEvent::Changed { browser_id, .. }
                            | LoadEvent::Start { browser_id, .. }
                            | LoadEvent::Error { browser_id, .. } => {
                                crate::CefMessage::UpdateView(browser_id)
                            }
                        },
                        lifecycle: true,
                    }
                }),
            ),
            Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(render_rx).map(|browser_id| {
                    RelayEvent {
                        msg: crate::CefMessage::UpdateView(browser_id),
                        lifecycle: false,
                    }
                }),
            ),
            Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(display_rx).map(|event| {
                    match event {
                        DisplayEvent::CursorChanged {
                            browser_id,
                            cursor_type,
                        } => RelayEvent {
                            msg: crate::CefMessage::CursorChanged(browser_id, cursor_type),
                            lifecycle: false,
                        },
                        DisplayEvent::TitleChanged { browser_id, .. }
                        | DisplayEvent::UrlChanged { browser_id, .. } => RelayEvent {
                            msg: crate::CefMessage::UpdateView(browser_id),
                            lifecycle: true,
                        },
                    }
                }),
            ),
            Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(inner_rx).map(|msg| {
                    RelayEvent {
                        msg,
                        lifecycle: false,
                    }
                }),
            ),
        ];

        futures::stream::select_all(streams)
    }
}

pub struct IcyClient {
    pub state: IcyClientState,
    pub subscribers: ClientEventSubscriber,
}

#[derive(Clone, Debug)]
pub struct IcyClientState {
    pub render: IcyRenderState,
}

impl IcyClient {
    pub fn new(
        launch_id: LaunchId,
        device_scale_factor: f32,
        view_rect: cef::Rect,
        hw_view: Arc<ArcSwapOption<CefView>>,
        popup_view: Arc<ArcSwapOption<CefView>>,
        native_window: Option<isize>,
    ) -> (Self, IcyClientHandlers) {
        let (load_handler, load_rx) = IcyLoadHandler::new();
        let (display_handler, display_rx) = IcyDisplayHandler::new();
        let (render_handler, render_state, render_rx, _drag_rx, _ime_composition_rx) =
            IcyRenderHandler::new(
                device_scale_factor,
                view_rect,
                hw_view,
                popup_view,
                native_window,
            );
        let (lifespan_handler, lifespan_rx) = IcyLifeSpanHandler::new(launch_id);
        let context_menu_handler = IcyContextMenuHandler::new();
        let keyboard_handler = IcyKeyboardHandler::new();
        let (process_message_tx, process_message_rx) = unbounded_channel();

        let state = IcyClientState {
            render: render_state,
        };
        let subscribers = ClientEventSubscriber {
            lifespan_rx,
            load_rx,
            process_message_rx,
            render_rx,
            display_rx,
        };
        let handlers = IcyClientHandlers {
            load_handler,
            lifespan_handler,
            render_handler,
            context_menu_handler,
            keyboard_handler,
            display_handler,
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
    keyboard_handler: KeyboardHandler,
    display_handler: DisplayHandler,
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
        false as _
    }
}
