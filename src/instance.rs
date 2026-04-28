use crate::{
    BrowserId, Webview,
    client::{
        CefSubscriptionRecipe, CefView, CefViewWrapper, ClientEventSubscriber, RelayEvent,
        SubscriberDepositor,
    },
};
use crate::{
    client::{ClientBuilder, IcyClient, IcyClientState, LifeSpanEvent, LoadEvent},
    request::{IcyRequestContextHandler, RequestContextHandlerBuilder},
};
use arc_swap::ArcSwapOption;
use cef;
use cef::*;
use iced::{Element, Subscription, Task, mouse, window};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex, atomic::AtomicUsize},
};

pub enum CefAction {
    Loaded(BrowserId),
    Run(Task<CefMessage>),
    Created(BrowserId),
    Closed(BrowserId),
    None,
}

#[derive(Clone)]
pub enum CefMessage {
    Loaded(BrowserId),
    Create(window::Id, url::Url, iced::Point, iced::Size, f32),
    Created(BrowserId),
    Closed(BrowserId),
    BoundsChanged(BrowserId, iced::Rectangle),
    UpdateCaretOffset(BrowserId, f32),
    FocusedNodeChanged(BrowserId, iced::Rectangle),
    UpdateView(BrowserId),
    CursorChanged(BrowserId, mouse::Interaction),
}

impl std::fmt::Debug for CefMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create(id, url, point, size, device_scale_factor) => f
                .debug_tuple("Create")
                .field(id)
                .field(url)
                .field(point)
                .field(size)
                .field(device_scale_factor)
                .finish(),
            Self::Loaded(browser_id) => f.debug_tuple("Loaded").field(browser_id).finish(),
            Self::Created(browser_id) => f.debug_tuple("Created").field(browser_id).finish(),
            Self::Closed(browser_id) => f.debug_tuple("Closed").field(browser_id).finish(),
            Self::BoundsChanged(browser_id, bounds) => f
                .debug_tuple("BoundsChanged")
                .field(browser_id)
                .field(bounds)
                .finish(),
            Self::UpdateView(browser_id) => f.debug_tuple("UpdateView").field(browser_id).finish(),
            Self::UpdateCaretOffset(browser_id, offset) => f
                .debug_tuple("UpdateCaretOffset")
                .field(browser_id)
                .field(offset)
                .finish(),
            Self::FocusedNodeChanged(browser_id, rect) => f
                .debug_tuple("FocusedNodeChanged")
                .field(browser_id)
                .field(rect)
                .finish(),
            Self::CursorChanged(browser_id, cursor) => f
                .debug_tuple("CursorChanged")
                .field(browser_id)
                .field(cursor)
                .finish(),
        }
    }
}

impl std::fmt::Debug for CefAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CefAction::Run(_) => f.debug_tuple("Run").finish(),
            CefAction::Loaded(browser_id) => f.debug_tuple("Loaded").field(browser_id).finish(),
            CefAction::Created(browser_id) => f.debug_tuple("Created").field(browser_id).finish(),
            CefAction::Closed(browser_id) => f.debug_tuple("Closed").field(browser_id).finish(),
            CefAction::None => f.debug_tuple("None").finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LaunchId(usize);

impl LaunchId {
    pub fn unique() -> Self {
        static NEXT_UNIQUE_LAUNCH_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_UNIQUE_LAUNCH_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(id)
    }
}

#[derive(Clone)]
pub struct DragData {
    pub data: cef::DragData,
    pub operation: cef::DragOperationsMask,
    pub x: f32,
    pub y: f32,
}

thread_local! {
    pub static LAUNCHED_BROWSERS: RefCell<BTreeMap<LaunchId, IcyClientState>> = RefCell::new(BTreeMap::new());
    pub static WEBVIEW_STATES: RefCell<BTreeMap<BrowserId, IcyClientState>> = RefCell::new(BTreeMap::new());
}

pub(crate) fn webview_states()
-> &'static std::thread::LocalKey<RefCell<BTreeMap<BrowserId, IcyClientState>>> {
    &WEBVIEW_STATES
}

pub(crate) fn new_browser(launch_id: LaunchId, browser: IcyClientState) {
    LAUNCHED_BROWSERS.with_borrow_mut(|browsers| browsers.insert(launch_id, browser));
}

pub(crate) fn new_webview(launch_id: LaunchId, browser_id: BrowserId) {
    if let Some((_, state)) =
        LAUNCHED_BROWSERS.with_borrow_mut(|browsers| browsers.remove_entry(&launch_id))
    {
        WEBVIEW_STATES.with_borrow_mut(|states| states.insert(browser_id, state));
    }
}

pub(crate) fn remove_webview(browser_id: BrowserId) {
    WEBVIEW_STATES.with_borrow_mut(|states| {
        states.remove(&browser_id);
    });
}

pub(crate) fn resize(browser_id: BrowserId, bound: iced::Rectangle) {
    WEBVIEW_STATES.with_borrow_mut(|states| {
        if let Some(state) = states.get(&browser_id)
            && bound.width > 0.0
            && bound.height > 0.0
        {
            state.render.set_view_rect(cef::Rect {
                x: bound.x as i32,
                y: bound.y as i32,
                width: bound.width as i32,
                height: bound.height as i32,
            });
            state
                .render
                .set_view_origin(iced::Point::new(bound.x, bound.y));
            if let Some(host) =
                browser_host_get_browser_by_identifier(browser_id.inner()).and_then(|b| b.host())
            {
                host.was_resized();
            }
        }
    })
}

pub struct CefComponent {
    window_id: Option<window::Id>,
    browser_id: Option<BrowserId>,
    browser: Option<cef::Browser>,
    focused_node: Option<iced::Rectangle>,
    caret_offset: Option<f32>,
    composition_position: Option<iced::Point>,
    cursor: Option<mouse::Interaction>,
    bounds: Option<iced::Rectangle>,
    hw_view: Arc<ArcSwapOption<CefView>>,
    popup_view: Arc<ArcSwapOption<CefView>>,
    subscription_id: Option<LaunchId>,
    subscriber_depositor: SubscriberDepositor,
    relay_tx: Option<tokio::sync::broadcast::Sender<RelayEvent>>,
}

impl CefComponent {
    pub fn new() -> Self {
        Self {
            window_id: None,
            browser_id: None,
            browser: None,
            focused_node: None,
            caret_offset: None,
            composition_position: None,
            cursor: None,
            bounds: None,
            hw_view: Arc::new(ArcSwapOption::const_empty()),
            popup_view: Arc::new(ArcSwapOption::const_empty()),
            subscription_id: None,
            subscriber_depositor: Arc::new(Mutex::new(None)),
            relay_tx: None,
        }
    }

    pub fn get_window_info(
        &self,
        id: window::Id,
    ) -> Task<(window::Id, iced::Point, iced::Size, f32)> {
        window::position(id)
            .and_then(move |position| {
                window::scale_factor(id).map(move |factor| (position, factor))
            })
            .then(move |(position, factor)| {
                window::size(id).map(move |size| (position, size, factor))
            })
            .map(move |(position, size, factor)| (id, position, size, factor))
    }

    fn launch_webview(
        launch_id: LaunchId,
        url: url::Url,
        bound: cef::Rect,
        device_scale_factor: f32,
        hw_view: Arc<ArcSwapOption<CefView>>,
        popup_view: Arc<ArcSwapOption<CefView>>,
        subscriber_depositor: SubscriberDepositor,
    ) -> Task<CefMessage> {
        let (client, handlers) = IcyClient::new(
            launch_id,
            device_scale_factor,
            bound,
            hw_view,
            popup_view,
            None,
        );
        let IcyClient { state, subscribers } = client;

        let mut windowinfo = cef::WindowInfo {
            windowless_rendering_enabled: true as _,
            shared_texture_enabled: true as _,
            external_begin_frame_enabled: true as _,
            ..Default::default()
        };
        windowinfo.runtime_style = cef::sys::cef_runtime_style_t::CEF_RUNTIME_STYLE_ALLOY.into();

        let mut context = cef::request_context_create_context(
            Some(&RequestContextSettings::default()),
            Some(&mut RequestContextHandlerBuilder::build(
                IcyRequestContextHandler {},
            )),
        );
        let browser_settings = cef::BrowserSettings {
            windowless_frame_rate: 60,
            default_encoding: "utf-8".into(),
            javascript_access_clipboard: cef::sys::cef_state_t::STATE_ENABLED.into(),
            javascript: cef::sys::cef_state_t::STATE_ENABLED.into(),
            ..Default::default()
        };

        let ret = cef::browser_host_create_browser(
            Some(&windowinfo),
            Some(&mut ClientBuilder::build(handlers)),
            Some(
                &std::env::var("TEST_URL")
                    .unwrap_or(url.into())
                    .as_str()
                    .into(),
            ),
            Some(&browser_settings),
            None::<&mut cef::DictionaryValue>,
            context.as_mut(),
        );
        if ret != 1 {
            return Task::none();
        }

        new_browser(launch_id, state);
        if let Some(tx) = subscriber_depositor
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
        {
            let _ = tx.send(subscribers);
        }
        Task::none()
    }

    pub fn update(&mut self, action: CefMessage) -> CefAction {
        match action {
            CefMessage::UpdateView(browser_id) => {
                if browser_id.inner() >= 0 {
                    if self.browser_id == Some(browser_id)
                        && let Some(browser) =
                            cef::browser_host_get_browser_by_identifier(browser_id.inner())
                    {
                        self.browser = Some(browser);
                    }
                }
                CefAction::None
            }
            CefMessage::BoundsChanged(browser_id, bounds) => {
                self.bounds = Some(bounds);
                resize(browser_id, bounds);
                CefAction::None
            }
            CefMessage::Create(window_id, url, position, size, device_scale_factor) => {
                self.window_id = Some(window_id);
                let hw_view = self.hw_view.clone();
                let popup_view = self.popup_view.clone();
                let (sub_tx, sub_rx) = tokio::sync::oneshot::channel::<ClientEventSubscriber>();
                let subscriber_depositor: SubscriberDepositor = Arc::new(Mutex::new(Some(sub_tx)));
                let (relay_tx, _) = tokio::sync::broadcast::channel(512);
                let relay_tx_for_task = relay_tx.clone();
                let hw_view_for_task = hw_view.clone();
                let popup_view_for_task = popup_view.clone();
                tokio::spawn(async move {
                    let Ok(subscriber) = sub_rx.await else { return };
                    use tokio_stream::StreamExt as _;
                    let mut s = std::pin::pin!(subscriber.into_stream());
                    let mut lifecycle_buf: Vec<RelayEvent> = Vec::new();
                    while let Some(item) = s.next().await {
                        let has_receivers = relay_tx_for_task.receiver_count() > 0;
                        if !has_receivers {
                            hw_view_for_task.store(None);
                            popup_view_for_task.store(None);
                        }
                        if has_receivers && !lifecycle_buf.is_empty() {
                            let buffered = std::mem::take(&mut lifecycle_buf);
                            let mut drain = buffered.into_iter();
                            for evt in &mut drain {
                                if let Err(e) = relay_tx_for_task.send(evt) {
                                    lifecycle_buf.push(e.0);
                                    break;
                                }
                            }
                            lifecycle_buf.extend(drain);
                        }
                        if let Err(e) = relay_tx_for_task.send(item) {
                            if e.0.lifecycle {
                                lifecycle_buf.push(e.0);
                            }
                        }
                    }
                });
                let launch_id = LaunchId::unique();
                self.subscription_id = Some(launch_id);
                self.subscriber_depositor = subscriber_depositor.clone();
                self.relay_tx = Some(relay_tx.clone());
                CefAction::Run(Self::launch_webview(
                    launch_id,
                    url,
                    cef::Rect {
                        x: position.x as _,
                        y: position.y as _,
                        width: size.width as _,
                        height: size.height as _,
                    },
                    device_scale_factor,
                    hw_view,
                    popup_view,
                    subscriber_depositor,
                ))
            }
            CefMessage::Created(browser_id) => {
                self.browser_id = Some(browser_id);
                self.browser = cef::browser_host_get_browser_by_identifier(browser_id.inner());
                CefAction::Created(browser_id)
            }
            CefMessage::UpdateCaretOffset(_, offset) => {
                if offset < 0.0 {
                    self.caret_offset = None;
                } else {
                    self.caret_offset = Some(offset);
                }
                CefAction::None
            }
            CefMessage::FocusedNodeChanged(_, node) => {
                self.focused_node = Some(node);
                CefAction::None
            }
            CefMessage::CursorChanged(_, cursor) => {
                self.cursor = Some(cursor);
                CefAction::None
            }
            CefMessage::Closed(browser_id) => {
                if self.browser_id == Some(browser_id) {
                    self.browser_id = None;
                    self.browser = None;
                    self.hw_view.store(None);
                    self.popup_view.store(None);
                    self.subscription_id = None;
                    self.subscriber_depositor = Arc::new(Mutex::new(None));
                    self.relay_tx = None;
                }
                CefAction::Closed(browser_id)
            }
            CefMessage::Loaded(browser_id) => CefAction::Loaded(browser_id),
        }
    }

    pub fn view(&self) -> Element<'_, CefMessage> {
        let Some(browser_id) = self.browser_id else {
            return iced::widget::space().into();
        };

        let popup_view = self.popup_view.load_full();
        let popup_bounds = popup_view.as_ref().and_then(|view| view.bounds);
        let popup_offset = popup_view.as_ref().and_then(|view| view.popup_offset);

        let content: Element<'_, CefMessage> = if let Some(view) = self.hw_view.load_full() {
            if let Some(handle) = view.software_handle() {
                iced::widget::responsive(move |size| {
                    iced::widget::Image::new(handle.clone())
                        .width(size.width)
                        .height(size.height)
                        .into()
                })
                .into()
            } else {
                iced::widget::responsive(move |size| {
                    iced::widget::shader::<CefMessage, _>(CefViewWrapper(view.clone()))
                        .width(size.width)
                        .height(size.height)
                        .into()
                })
                .into()
            }
        } else {
            iced::widget::space().into()
        };

        Webview::new(browser_id, content)
            .popup_bounds(popup_bounds)
            .popup_offset(popup_offset)
            .focused_node(self.focused_node)
            .caret_offset(self.caret_offset)
            .composition_position(self.composition_position)
            .cursor_interaction(self.cursor)
            .on_bounds_changed(move |bounds| CefMessage::BoundsChanged(browser_id, bounds))
            .into()
    }

    pub fn subscription(&self) -> Subscription<CefMessage> {
        if let (Some(id), Some(relay_tx)) = (self.subscription_id, self.relay_tx.clone()) {
            iced::advanced::subscription::from_recipe(CefSubscriptionRecipe { id, relay_tx })
        } else {
            Subscription::none()
        }
    }
}
