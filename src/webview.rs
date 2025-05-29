use std::cell::RefCell;

use crate::backend::ClientHandlers;
use crate::BrowserId;
use crate::backend::ClientBuilder;
use crate::backend::RequestContextHandlerBuilder;
use cef::Browser;
use iced::Element;
use iced::Size;
use iced::advanced::Widget;
use iced::widget::horizontal_space;
use iced_core::layout;
use iced_core::mouse::Click;
use url::Url;

pub fn launch_browser(
    //window: RawWindowHandle,
    handlers: ClientHandlers,
    url: Url,
) -> crate::Result<()> {
    let window_info = cef::WindowInfo {
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let browser_settings = cef::BrowserSettings {
        default_encoding: cef::CefString::from("UTF-8"),
        ..Default::default()
    };
    let mut request_context = cef::request_context_create_context(
        Some(&cef::RequestContextSettings::default()),
        Some(&mut RequestContextHandlerBuilder::build(
            crate::backend::IcyRequestContextHandler::new(),
        )),
    );
    let ret = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut ClientBuilder::build(handlers)),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        Option::<&mut cef::DictionaryValue>::None,
        request_context.as_mut(),
    );
    if ret != 1 {
        return Err(crate::error::Error::CannotCreateBrowser);
    }

    Ok(())
}

pub struct Webview<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    browser_id: BrowserId,
    view: Box<dyn Fn(BrowserId) -> Element<'a, Message, Theme, Renderer> + 'a>,
    content: RefCell<Element<'a, Message, Theme, Renderer>>,
    focused_editable_dom_node: Option<iced::Rectangle>,
    _marker: std::marker::PhantomData<(&'a (), *const (), Message, Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> Webview<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    pub fn new(
        browser_id: BrowserId,
        view: impl Fn(BrowserId) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        Self {
            view: Box::new(view),
            content: RefCell::new(Element::new(horizontal_space().width(0))),
            browser_id,
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

struct State {
    last_click: Option<Click>,
    previous_size: Option<iced::Size>,
    browser: Option<Browser>,
}

#[allow(unused)]
impl<'a, Message, Theme, Renderer: iced::advanced::Renderer> Widget<Message, Theme, Renderer>
    for Webview<'a, Message, Theme, Renderer>
{
    fn state(&self) -> iced::advanced::widget::tree::State {
        let browser = cef::browser_host_get_browser_by_identifier(self.browser_id.as_i32());
        iced::advanced::widget::tree::State::new(State {
            last_click: None,
            previous_size: None,
            browser,
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
        layout::Node::new(limits.max())
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
        #[allow(clippy::unwrap_used)]
        self.content
            .borrow_mut()
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn update(
        &mut self,
        state: &mut iced_core::widget::Tree,
        event: &iced::Event,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) {
        self.content.borrow_mut().as_widget_mut().update(
            state, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
}
