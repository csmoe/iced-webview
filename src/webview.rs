use crate::backend::BrowserId;
use crate::backend::ClientBuilder;
use crate::backend::IcyClient;
use crate::backend::LifeSpanEvent;
use cef::CefStringUtf8;
use cef::ImplBrowser;
use cef::ImplView;
use iced::wgpu::rwh::RawWindowHandle;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use url::Url;

pub fn launch(
    window: RawWindowHandle,
    bound: (iced::Point<i32>, iced::Size<i32>),
    url: Url,
) -> anyhow::Result<oneshot::Receiver<LifeSpanEvent>> {
    let (point, size) = bound;
    let parent = match window {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(handle) => handle.hwnd.get(),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(handle) => handle.ns_view.get(),
        _ => anyhow::bail!("unsupported window handle"),
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
        ..Default::default()
    };
    let (client, handlers) = IcyClient::new();
    let browser_settings = cef::BrowserSettings::default();
    let mut cef_client = ClientBuilder::build(handlers);
    let ret = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut cef_client),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        Option::<&mut cef::DictionaryValue>::None,
        Option::<&mut cef::RequestContext>::None,
    );
    if ret != 1 {
        anyhow::bail!("failed to create browser");
    }
    let IcyClient {
        load_rx,
        lifespan_rxs: (create_rx, close_rx),
    } = client;

    Ok(create_rx)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(i32);

#[derive(Clone)]
pub struct WebView {
    browser_id: BrowserId,
    browser: cef::Browser,
    window: iced::window::Id,
    tab: Option<TabId>,
    _marker: std::marker::PhantomData<*const ()>,
}

impl std::hash::Hash for WebView {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.browser_id.hash(state);
        self.window.hash(state);
        self.tab.hash(state);
    }
}

impl std::fmt::Debug for WebView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebView")
            .field("browser_id", &self.browser_id)
            .field("window", &self.window)
            .field("tab", &self.tab)
            .finish()
    }
}

impl PartialEq for WebView {
    fn eq(&self, other: &Self) -> bool {
        self.browser_id == other.browser_id && self.window == other.window && self.tab == other.tab
    }
}
impl Eq for WebView {}

impl WebView {
    pub fn new(browser_id: BrowserId, browser: cef::Browser, window: iced::window::Id) -> Self {
        Self {
            browser,
            browser_id,
            window,
            tab: None,
            _marker: std::marker::PhantomData,
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

    pub fn resize(&mut self, width: i32, height: i32) {
        if let Some(view) = self.view() {
            view.set_size(Some(&cef::Size {
                width: width as _,
                height: height as _,
            }));
        }
    }

    fn host(&self) -> Option<cef::BrowserHost> {
        self.browser.get_host()
    }

    fn view(&mut self) -> Option<cef::BrowserView> {
        cef::browser_view_get_for_browser(Some(&mut self.browser))
    }
}
