use crate::backend::BrowserId;
use crate::backend::IcyClient;
use crate::backend::IcyRenderState;
use crate::backend::IcyRequestContextHandler;
use crate::backend::IcyRequestContextHandler;
use crate::backend::LifeSpanEvent;
use cef::ImplBrowser;
use cef::ImplView;
use iced::Size;
use iced::advanced::Widget;
use iced::wgpu::rwh::RawWindowHandle;
use tokio::sync::mpsc::Receiver;
use url::Url;

pub fn launch(
    window: RawWindowHandle,
    bound: (iced::Point, iced::Size),
    url: Url,
) -> crate::Result<Receiver<LifeSpanEvent>> {
    let (point, size) = bound;
    let parent = match window {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(handle) => handle.hwnd.get(),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
        _ => return Err(crate::Error::Custom("unsupported window handle".into())),
    };
    let window_info = cef::WindowInfo {
        bounds: cef::Rect {
            x: point.x as _,
            y: point.y as _,
            width: size.width as _,
            height: size.height as _,
        },
        #[cfg(target_os = "windows")]
        parent_window: cef::sys::HWND(parent as _),
        #[cfg(target_os = "macos")]
        parent_view: parent as _,
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let (client, handlers) = IcyClient::new();
    let browser_settings = cef::BrowserSettings {
        default_encoding: cef::CefString::from("UTF-8"),
        ..Default::default()
    };
    let mut cef_client = ClientBuilder::build(handlers);
    let mut request_context = cef::request_context_create_context(
        Some(&cef::RequestContextSettings::default()),
        Some(&mut IcyRequestContextHandler::build(
            IcyRequestContextHandler {},
        )),
    );
    let ret = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut cef_client),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        Option::<&mut cef::DictionaryValue>::None,
        request_context.as_mut(),
    );
    if ret != 1 {
        return Err(crate::error::Error::CannotCreateBrowser);
    }
    let IcyClient {
        load_rx,
        lifespan_rx,
    } = client;

    Ok(lifespan_rx)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(i32);

#[derive(Clone)]
pub struct WebView<'a, Message = (), Theme = iced::Theme, Renderer = iced::Renderer> {
    browser_id: BrowserId,
    browser: cef::Browser,
    window: iced::window::Id,
    tab: Option<TabId>,
    focused_editable_dom_node: Option<iced::Rectangle>,
    _marker: std::marker::PhantomData<(&'a (), *const (), Message, Theme, Renderer)>,
}

impl std::hash::Hash for WebView<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.browser_id.hash(state);
        self.window.hash(state);
        self.tab.hash(state);
    }
}

impl std::fmt::Debug for WebView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebView")
            .field("browser_id", &self.browser_id)
            .field("window", &self.window)
            .field("tab", &self.tab)
            .finish()
    }
}

impl PartialEq for WebView<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.browser_id == other.browser_id && self.window == other.window && self.tab == other.tab
    }
}
impl Eq for WebView<'_> {}

impl WebView<'_> {
    pub fn new(browser_id: BrowserId, browser: cef::Browser, window: iced::window::Id) -> Self {
        Self {
            browser,
            browser_id,
            window,
            focused_editable_dom_node: None,
            tab: None,
            _marker: std::marker::PhantomData,
        }
    }

    fn focused_editable_dom_node(mut self, focused_editable_dom_node: iced::Rectangle) -> Self {
        self.focused_editable_dom_node = Some(focused_editable_dom_node);
        self
    }

    fn input_method(&self) -> iced_core::InputMethod {
        match self.focused_editable_dom_node {
            Some(node) => iced_core::InputMethod::Enabled {
                position: node.position(),
                purpose: iced_core::input_method::Purpose::Normal,
                preedit: None,
            },
            None => iced_core::InputMethod::Disabled,
        }
    }

    pub fn window(&self) -> iced::window::Id {
        self.window
    }

    pub fn tab(&self) -> Option<TabId> {
        self.tab
    }

    pub fn bind_tab(&mut self, tab: TabId) {
        self.tab = Some(tab);
    }

    pub fn hidden(&mut self) {
        if let Some(view) = self.view() {
            view.set_visible(false as _);
        }
    }

    pub fn show(&mut self) {
        if let Some(view) = self.view() {
            view.set_visible(true as _);
        }
    }

    pub fn resize(&mut self, size: Size) {
        if let Some(view) = self.view() {
            view.set_size(Some(&cef::Size {
                width: size.width as _,
                height: size.height as _,
            }));
        }
    }

    fn host(&self) -> Option<cef::BrowserHost> {
        self.browser.host()
    }

    fn view(&mut self) -> Option<cef::BrowserView> {
        cef::browser_view_get_for_browser(Some(&mut self.browser))
    }
}

struct WebviewState {
    render: IcyRenderState,
}

impl<'a, Message, Theme, Renderer> Widget<'a, Message, Theme, Renderer> for WebView<'a> {
    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(WebviewState {
            render: IcyRenderState::new(),
        })
    }

    fn size(&self) -> Size<iced::Length> {
        Size::new(iced::Length::Fill, iced::Length::Fill)
    }

    fn layout(
        &self,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        todo!()
    }

    fn draw(
        &self,
        tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
    }

    fn update(
        &mut self,
        _state: &mut iced_core::widget::Tree,
        event: &iced::Event,
        _layout: iced_core::Layout<'_>,
        _cursor: iced_core::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) {
        match event {
            iced::Event::Window(iced::window::Event::RedrawRequested(_)) => {
                shell.request_input_method(&self.input_method());
                shell.request_redraw();
            }
            _ => {}
        }
    }
}
