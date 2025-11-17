use super::instance::{get_cursor_type, resize};
use crate::BrowserId;
use cef::{ImplBrowser, ImplBrowserHost};
use iced::{self};
use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme,
    advanced::{
        Clipboard, Shell,
        layout::{Layout, Limits, Node},
        renderer::Style,
        widget::{Operation, Tree, Widget, tree},
    },
    keyboard::{self},
    mouse::{self, Cursor},
};
use iced_core::{self, input_method};

struct CefState {
    bounds: iced::Rectangle,
    browser_id: BrowserId,
    browser_host: Option<cef::BrowserHost>,
}

impl CefState {
    fn resize(&mut self, bound: iced::Rectangle) {
        resize(self.browser_id, bound);
    }

    fn input_method(
        &self,
        cursor: Option<iced::Point>,
        bound: iced::Rectangle,
        focused_node: Option<iced::Rectangle>,
        caret_offset: Option<f32>,
    ) -> iced_core::InputMethod {
        if let Some(p) = focused_node.map(|n| n.position()).or(cursor)
            && let Some(offset) = caret_offset
        {
            let x = p.x + bound.x;
            let y = p.y + bound.y + 8.0;
            return iced_core::input_method::InputMethod::Enabled {
                position: iced::Point::new(x + offset, y),
                purpose: iced_core::input_method::Purpose::Normal,
                preedit: None,
            };
        }
        iced_core::input_method::InputMethod::Disabled
    }
}

pub struct Webview<'a, Message> {
    browser_id: BrowserId,
    content: Element<'a, Message, Theme, Renderer>,
    focused_node: Option<iced::Rectangle>,
    caret_offset: Option<f32>,
    on_input_method_event: Option<Box<dyn Fn(input_method::Event) -> Message + 'a>>,
    on_key_event: Option<Box<dyn Fn(keyboard::Event) -> Message + 'a>>,
    on_mouse_event: Option<Box<dyn Fn(iced::Point, mouse::Event) -> Message + 'a>>,
}

impl<'a, Message> Webview<'a, Message> {
    pub fn new(
        browser_id: BrowserId,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Webview {
            browser_id,
            content: content.into(),
            focused_node: None,
            caret_offset: None,
            on_input_method_event: None,
            on_key_event: None,
            on_mouse_event: None,
        }
    }

    pub fn focused_node(mut self, focused_node: Option<iced::Rectangle>) -> Self {
        self.focused_node = focused_node;
        self
    }

    pub fn caret_offset(mut self, caret_offset: Option<f32>) -> Self {
        self.caret_offset = caret_offset;
        self
    }

    pub fn on_input_method_event(
        mut self,
        on_input_method_event: impl Fn(input_method::Event) -> Message + 'a,
    ) -> Self {
        self.on_input_method_event = Some(Box::new(on_input_method_event));
        self
    }

    pub fn on_key_event(mut self, on_key_event: impl Fn(keyboard::Event) -> Message + 'a) -> Self {
        self.on_key_event = Some(Box::new(on_key_event));
        self
    }

    pub fn on_mouse_event(
        mut self,
        on_mouse_event: impl Fn(iced::Point, mouse::Event) -> Message + 'a,
    ) -> Self {
        self.on_mouse_event = Some(Box::new(on_mouse_event));
        self
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Webview<'_, Message>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<CefState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(CefState {
            browser_id: self.browser_id,
            bounds: iced::Rectangle::with_size(iced::Size::ZERO),
            browser_host: cef::browser_host_get_browser_by_identifier(self.browser_id.inner())
                .and_then(|browser| browser.host()),
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let _state = tree.state.downcast_mut::<CefState>();

        // generate the child layout
        let child_layout =
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits);

        Node::with_children(child_layout.size(), vec![child_layout])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        #[allow(clippy::unwrap_used)]
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<CefState>();
        let bounds = layout.bounds();
        if state.bounds != bounds {
            state.bounds = bounds.into();
            state.resize(bounds);
        }

        match event {
            Event::Keyboard(event) => {
                if let Some(on_key_event) = &self.on_key_event {
                    shell.publish(on_key_event(event.clone()));
                    shell.capture_event();
                }
            }
            Event::Mouse(event) => {
                if let Some(point) = cursor.position_in(bounds)
                    && let Some(on_mouse_event) = &self.on_mouse_event
                {
                    shell.publish(on_mouse_event(point, event.clone()));
                    shell.capture_event();
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(_now)) => {
                if let Some(browser_host) = &state.browser_host {
                    browser_host.send_external_begin_frame();
                }
                shell.request_input_method::<String>(&state.input_method(
                    cursor.position_in(bounds),
                    bounds,
                    self.focused_node,
                    self.caret_offset,
                ));
            }
            Event::InputMethod(event) => {
                if let Some(on_input_method_event) = &self.on_input_method_event {
                    shell.publish(on_input_method_event(event.clone()));
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn operate(
        &mut self,
        state: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = state.state.downcast_mut::<CefState>();
        operation.custom(None, layout.bounds(), state);
    }

    fn mouse_interaction(
        &self,
        _state: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.position_in(layout.bounds()).is_some() {
            if let Some(cursor_type) = get_cursor_type(self.browser_id) {
                return map_cursor(cursor_type);
            }
        }
        return mouse::Interaction::None;

        #[inline]
        fn map_cursor(type_: cef::CursorType) -> mouse::Interaction {
            use cef::sys::cef_cursor_type_t;
            match type_.as_ref() {
                cef_cursor_type_t::CT_HAND => mouse::Interaction::Pointer,
                cef_cursor_type_t::CT_WAIT => mouse::Interaction::Working,
                cef_cursor_type_t::CT_HELP => mouse::Interaction::Help,
                cef_cursor_type_t::CT_EASTWESTRESIZE
                | cef_cursor_type_t::CT_WESTRESIZE
                | cef_cursor_type_t::CT_EASTRESIZE => mouse::Interaction::ResizingHorizontally,
                cef_cursor_type_t::CT_NORTHWESTRESIZE
                | cef_cursor_type_t::CT_SOUTHEASTRESIZE
                | cef_cursor_type_t::CT_NORTHWESTSOUTHEASTRESIZE => {
                    mouse::Interaction::ResizingDiagonallyDown
                }
                cef_cursor_type_t::CT_NORTHSOUTHRESIZE
                | cef_cursor_type_t::CT_SOUTHRESIZE
                | cef_cursor_type_t::CT_NORTHRESIZE => mouse::Interaction::ResizingVertically,
                cef_cursor_type_t::CT_NORTHEASTSOUTHWESTRESIZE
                | cef_cursor_type_t::CT_SOUTHWESTRESIZE
                | cef_cursor_type_t::CT_NORTHEASTRESIZE => mouse::Interaction::ResizingDiagonallyUp,
                cef_cursor_type_t::CT_CELL => mouse::Interaction::Cell,
                cef_cursor_type_t::CT_IBEAM => mouse::Interaction::Text,
                cef_cursor_type_t::CT_MOVE => mouse::Interaction::Move,
                cef_cursor_type_t::CT_COPY => mouse::Interaction::Copy,
                cef_cursor_type_t::CT_NOTALLOWED => mouse::Interaction::NotAllowed,
                cef_cursor_type_t::CT_ZOOMIN => mouse::Interaction::ZoomIn,
                cef_cursor_type_t::CT_ZOOMOUT => mouse::Interaction::ZoomOut,
                cef_cursor_type_t::CT_GRAB => mouse::Interaction::Grab,
                cef_cursor_type_t::CT_GRABBING => mouse::Interaction::Grabbing,
                cef_cursor_type_t::CT_CROSS => mouse::Interaction::Crosshair,
                cef_cursor_type_t::CT_MIDDLE_PANNING_VERTICAL
                | cef_cursor_type_t::CT_COLUMNRESIZE
                | cef_cursor_type_t::CT_ROWRESIZE
                | cef_cursor_type_t::CT_MIDDLEPANNING
                | cef_cursor_type_t::CT_EASTPANNING
                | cef_cursor_type_t::CT_NORTHPANNING
                | cef_cursor_type_t::CT_NORTHEASTPANNING
                | cef_cursor_type_t::CT_NORTHWESTPANNING
                | cef_cursor_type_t::CT_SOUTHPANNING
                | cef_cursor_type_t::CT_SOUTHEASTPANNING
                | cef_cursor_type_t::CT_SOUTHWESTPANNING
                | cef_cursor_type_t::CT_WESTPANNING
                | cef_cursor_type_t::CT_VERTICALTEXT
                | cef_cursor_type_t::CT_CONTEXTMENU
                | cef_cursor_type_t::CT_ALIAS
                | cef_cursor_type_t::CT_PROGRESS
                | cef_cursor_type_t::CT_NODROP
                | cef_cursor_type_t::CT_MIDDLE_PANNING_HORIZONTAL
                | cef_cursor_type_t::CT_CUSTOM
                | cef_cursor_type_t::CT_DND_NONE
                | cef_cursor_type_t::CT_DND_MOVE
                | cef_cursor_type_t::CT_DND_COPY
                | cef_cursor_type_t::CT_DND_LINK
                | cef_cursor_type_t::CT_NUM_VALUES
                | _ => mouse::Interaction::None,
            }
        }
    }
}

impl<'a, Message: 'a, Theme, Renderer> From<Webview<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
    Webview<'a, Message>: Widget<Message, Theme, Renderer>,
{
    fn from(wrapper: Webview<'a, Message>) -> Self {
        Self::new(wrapper)
    }
}
