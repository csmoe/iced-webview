use crate::{
    BrowserId,
    {
        client::ClientEventSubscriber, webview::update_caret_offset,
        webview::update_focused_editable_node,
    },
};
use crate::{
    client::{ClientBuilder, IcyClient, IcyClientState, LifeSpanEvent, LoadEvent},
    request::{IcyRequestContextHandler, RequestContextHandlerBuilder},
};
use cef;
use cef::*;
use iced::{Element, Subscription, Task, widget::Image, window};
use std::{
    cell::RefCell, collections::BTreeMap, fmt::Debug, sync::atomic::AtomicUsize, time::Duration,
};
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

pub enum CefAction {
    Loaded(BrowserId),
    Run(Task<CefMessage>),
    Created(BrowserId),
    Closed(BrowserId),
    UpdateView(BrowserId),
    None,
}

#[derive(Clone)]
pub enum CefMessage {
    Loaded(BrowserId),
    Create(window::Id, url::Url, iced::Point, iced::Size, f32),
    Created(BrowserId),
    Closed(BrowserId),
    UpdateCaretOffset(BrowserId, f32),
    EditableNodeFocused(BrowserId, iced::Rectangle),
    PumpLoop(Duration),
    UpdateView(BrowserId),
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
            Self::PumpLoop(delay) => f.debug_tuple("PumpLoop").field(delay).finish(),
            Self::UpdateView(browser_id) => f.debug_tuple("UpdateView").field(browser_id).finish(),
            Self::UpdateCaretOffset(browser_id, offset) => f
                .debug_tuple("UpdateCaretOffset")
                .field(browser_id)
                .field(offset)
                .finish(),
            Self::EditableNodeFocused(browser_id, rect) => f
                .debug_tuple("EditableNodeFocused")
                .field(browser_id)
                .field(rect)
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
            CefAction::UpdateView(browser_id) => {
                f.debug_tuple("UpdateView").field(browser_id).finish()
            }
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

thread_local! {
    pub static LAUNCHED_BROWSERS: RefCell<BTreeMap<LaunchId, IcyClientState>> = RefCell::new(BTreeMap::new());
    pub static WEBVIEW_STATES: RefCell<BTreeMap<BrowserId, IcyClientState>> = RefCell::new(BTreeMap::new());
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

pub(crate) fn get_pixels<Message>(browser_id: BrowserId) -> Option<Image> {
    use iced::widget::image::Handle;
    WEBVIEW_STATES.with_borrow(|states| {
        states.get(&browser_id).map(|state| {
            let (width, height) = state.render.size();
            Image::new(Handle::from_rgba(
                width as _,
                height as _,
                state.render.pixels().clone(),
            ))
        })
    })
}

pub(crate) fn get_cursor_type(browser_id: BrowserId) -> Option<cef::CursorType> {
    WEBVIEW_STATES.with_borrow(|states| {
        states
            .get(&browser_id)
            .map(|state| state.display.cursor_type.borrow().clone())
    })
}

pub(crate) fn resize(browser_id: BrowserId, bound: cef::Rect) {
    WEBVIEW_STATES.with_borrow_mut(|states| {
        if let Some(state) = states.get(&browser_id) {
            state.render.set_view_rect(bound);
            if let Some(host) =
                browser_host_get_browser_by_identifier(browser_id.inner()).and_then(|b| b.host())
            {
                host.was_resized();
            }
        }
    })
}

#[derive(Debug)]
pub struct CefComponent {}

impl CefComponent {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_window_info(
        &self,
        id: window::Id,
    ) -> Task<(window::Id, iced::Point, iced::Size, f32)> {
        window::get_position(id)
            .and_then(move |position| {
                window::get_scale_factor(id).map(move |factor| (position, factor))
            })
            .then(move |(position, factor)| {
                window::get_size(id).map(move |size| (position, size, factor))
            })
            .map(move |(position, size, factor)| (id, position, size, factor))
    }
}

impl CefComponent {
    fn launch_webview(
        launch_id: LaunchId,
        url: url::Url,
        bound: cef::Rect,
        device_scale_factor: f32,
    ) -> Task<CefMessage> {
        let (client, handlers) = IcyClient::new(launch_id, device_scale_factor, bound);
        let IcyClient { state, subscribers } = client;

        let mut windowinfo = cef::WindowInfo {
            windowless_rendering_enabled: true as _,
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

        tracing::info!("trying to create browser");

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
            tracing::error!("cannot create browser");
            return Task::none();
        }

        new_browser(launch_id, state);

        let ClientEventSubscriber {
            lifespan_rx,
            load_rx,
            process_message_rx,
            render_rx,
        } = subscribers;
        Task::batch([
            //Task::stream(tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
            //  Duration::from_millis(60),
            //)))
            //.map(|delay| CefMessage::PumpLoop(delay.elapsed())),
            Task::stream(UnboundedReceiverStream::new(render_rx)).map(CefMessage::UpdateView),
            Task::stream(tokio_stream::wrappers::ReceiverStream::new(lifespan_rx)).map(
                move |event| match event {
                    LifeSpanEvent::Closed { browser_id } => CefMessage::Closed(browser_id.into()),
                    LifeSpanEvent::Created { browser_id } => CefMessage::Created(browser_id),
                },
            ),
            Task::stream(
                UnboundedReceiverStream::new(load_rx).map(move |msg| match msg {
                    LoadEvent::Start { .. } => CefMessage::PumpLoop(Duration::from_secs(1)),
                    LoadEvent::Changed { .. } => CefMessage::PumpLoop(Duration::from_secs(1)),
                    LoadEvent::End { .. } => CefMessage::PumpLoop(Duration::from_secs(1)),
                    LoadEvent::Error { .. } => CefMessage::PumpLoop(Duration::from_secs(1)),
                }),
            ),
            Task::stream(UnboundedReceiverStream::new(process_message_rx)).map(|msg| match msg {
                crate::client::CefIpcMessage::EditableNodeFocused {
                    browser_id,
                    x,
                    y,
                    width,
                    height,
                } => CefMessage::EditableNodeFocused(
                    browser_id.into(),
                    iced::Rectangle {
                        x: x as _,
                        y: y as _,
                        width: width as _,
                        height: height as _,
                    },
                ),
                crate::client::CefIpcMessage::CaretPositionChanged { browser_id, offset } => {
                    CefMessage::UpdateCaretOffset(browser_id.into(), offset)
                }
            }),
        ])
    }

    pub fn update(&mut self, action: CefMessage) -> CefAction {
        match action {
            CefMessage::PumpLoop(_delay) => {
                cef::do_message_loop_work();
                CefAction::None
            }
            CefMessage::UpdateView(browser_id) => CefAction::UpdateView(browser_id),
            CefMessage::Create(_window_id, url, position, size, device_scale_factor) => {
                let launch_id = LaunchId::unique();
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
                ))
            }
            CefMessage::Created(browser_id) => {
                tracing::info!(?browser_id, "created");

                CefAction::Created(browser_id)
            }
            CefMessage::UpdateCaretOffset(browser_id, offset) => {
                CefAction::Run(update_caret_offset::<Self>(browser_id, offset).discard())
            }
            CefMessage::EditableNodeFocused(browser_id, rect) => {
                CefAction::Run(update_focused_editable_node::<Self>(browser_id, rect).discard())
            }
            CefMessage::Closed(browser_id) => CefAction::Closed(browser_id),
            CefMessage::Loaded(browwser_id) => CefAction::Loaded(browwser_id),
        }
    }

    pub fn view(&self, browser_id: BrowserId) -> Element<'static, CefMessage> {
        iced::widget::responsive(move |size| crate::webview::webview(browser_id, size).into())
            .into()
    }

    pub fn subscription(&self) -> Subscription<CefMessage> {
        Subscription::none()
    }
}
