use crate::BrowserId;
use crate::backend::ClientEventSubscriber;
use crate::backend::IcyClient;
use crate::backend::IcyClientState;
use crate::backend::IcyRequestContextHandler;
use iced::Element;
use iced::Size;
use iced::advanced::Widget;
use url::Url;

pub fn launch_browser(
    //window: RawWindowHandle,
    device_scale_factor: f32,
    rect: cef::Rect,
    url: Url,
) -> crate::Result<(IcyClientState, ClientEventSubscriber)> {
    /*
        let parent = match window {
            #[cfg(target_os = "windows")]
            RawWindowHandle::Win32(handle) => handle.hwnd.get(),
            #[cfg(target_os = "macos")]
            RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
            _ => return Err(crate::Error::Custom("unsupported window handle".into())),
        };
    */
    let window_info = cef::WindowInfo {
        //#[cfg(target_os = "windows")]
        //parent_window: cef::sys::HWND(parent as _),
        //#[cfg(target_os = "macos")]
        //parent_view: parent as _,
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let (client, state, subscribers) = IcyClient::new(device_scale_factor, rect);
    let browser_settings = cef::BrowserSettings {
        default_encoding: cef::CefString::from("UTF-8"),
        ..Default::default()
    };
    let mut request_context = cef::request_context_create_context(
        Some(&cef::RequestContextSettings::default()),
        Some(&mut IcyRequestContextHandler::new()),
    );
    let ret = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut cef::Client::new(client)),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        Option::<&mut cef::DictionaryValue>::None,
        request_context.as_mut(),
    );
    if ret != 1 {
        return Err(crate::error::Error::CannotCreateBrowser);
    }

    Ok((state, subscribers))
}

pub struct Webview<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    view: Box<dyn Fn(BrowserId) -> Element<'a, Message, Theme, Renderer> + 'a>,
    focused_editable_dom_node: Option<iced::Rectangle>,
    _marker: std::marker::PhantomData<(&'a (), *const (), Message, Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> Webview<'a, Message, Theme, Renderer> {
    pub fn new(view: impl Fn(BrowserId) -> Element<'a, Message, Theme, Renderer> + 'a) -> Self {
        Self {
            view: Box::new(view),
            focused_editable_dom_node: None,
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
}

#[allow(unused)]
impl<'a, Message, Theme, Renderer: iced::advanced::Renderer> Widget<Message, Theme, Renderer>
    for Webview<'a, Message, Theme, Renderer>
{
    fn state(&self) -> iced::advanced::widget::tree::State {
        todo!()
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
