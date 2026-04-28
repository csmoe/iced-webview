use super::instance::resize;
use crate::BrowserId;
use crate::convert::{KeyPress, iced_key_to_cef_key};
use cef::{BrowserHost, ImplBrowser, ImplBrowserHost};
use iced::{
    self, Element, Event, Length, Rectangle, Renderer, Size, Theme,
    advanced::{
        Shell,
        layout::{Layout, Limits, Node},
        mouse::Click,
        renderer::Style,
        widget::{Operation, Tree, Widget, tree},
    },
    keyboard,
    mouse::{self, Cursor},
};
use std::{collections::HashSet, time::Duration};

struct CefState {
    browser_id: BrowserId,
    browser_host: Option<BrowserHost>,
    cursor_in_bounds: Option<iced::Point>,
    last_cursor_position: Option<iced::Point>,
    last_button_modifiers: u32,
    last_click: Option<Click>,
    last_mouse_down_on_view: bool,
    active_touch_ids: HashSet<u64>,
    bounds: iced::Rectangle,
    webview_focused: bool,
    window_focused: bool,
    last_begin_frame_time: std::time::Instant,
    begin_frame_interval: Duration,
}

impl CefState {
    fn should_begin_frame(&self) -> bool {
        if self.webview_focused && self.window_focused {
            true
        } else {
            self.last_begin_frame_time.elapsed() >= self.begin_frame_interval
        }
    }

    fn send_begin_frame(&mut self) {
        if let Some(host) = &self.browser_host {
            host.send_external_begin_frame();
            self.last_begin_frame_time = std::time::Instant::now();
        }
    }

    fn try_send_begin_frame(&mut self) {
        if self.should_begin_frame() {
            self.send_begin_frame();
        }
    }

    fn update_browser(&mut self, browser_id: BrowserId) {
        let browser_switched = self.browser_id != browser_id;
        if browser_switched {
            if let Some(host) = &self.browser_host {
                host.send_capture_lost_event();
                host.was_hidden(true as _);
            }
            self.browser_id = browser_id;
            self.browser_host = cef::browser_host_get_browser_by_identifier(
                self.browser_id.inner(),
            )
            .and_then(|browser| (browser.is_valid() == 1).then(|| browser.host()).flatten());
            if let Some(host) = &self.browser_host {
                host.was_hidden(false as _);
                host.invalidate(Default::default());
                resize(self.browser_id, self.bounds);
            }
            self.last_click = None;
            self.last_button_modifiers = 0;
            self.last_cursor_position = None;
            self.last_mouse_down_on_view = false;
            self.active_touch_ids.clear();
            self.cursor_in_bounds = None;
        }
    }

    fn is_over_popup_widget(
        &self,
        point: iced::Point,
        popup_bounds: Option<iced::Rectangle>,
    ) -> bool {
        popup_bounds.is_some_and(|popup| {
            point.x >= popup.x
                && point.x < popup.x + popup.width
                && point.y >= popup.y
                && point.y < popup.y + popup.height
        })
    }

    fn update_focus(&mut self, in_bounds: bool) {
        if let Some(host) = self.browser_host.as_mut()
            && self.webview_focused != in_bounds
        {
            self.webview_focused = in_bounds;
            host.set_focus((self.webview_focused && self.window_focused) as _);
        }
    }

    fn input_method(
        &self,
        bound: iced::Rectangle,
        focused_node: Option<iced::Rectangle>,
        caret_offset: Option<f32>,
        composition_position: Option<iced::Point>,
    ) -> iced::advanced::InputMethod {
        if let Some(position) = composition_position {
            return iced::advanced::input_method::InputMethod::Enabled {
                cursor: iced::Rectangle::new(
                    iced::Point::new(position.x + bound.x, position.y + bound.y + 8.0),
                    iced::Size::UNIT,
                ),
                purpose: iced::advanced::input_method::Purpose::Normal,
                preedit: None,
            };
        }
        if let Some(p) = focused_node.map(|n| n.position()).or(self.cursor_in_bounds)
            && let Some(offset) = caret_offset
        {
            let x = p.x + bound.x;
            let y = p.y + bound.y + 8.0;
            return iced::advanced::input_method::InputMethod::Enabled {
                cursor: iced::Rectangle::new(iced::Point::new(x + offset, y), iced::Size::UNIT),
                purpose: iced::advanced::input_method::Purpose::Normal,
                preedit: None,
            };
        }
        iced::advanced::InputMethod::Disabled
    }

    fn send_ime_event(&mut self, event: &iced::advanced::input_method::Event) {
        use iced::advanced::input_method::Event;

        let Some(host) = &self.browser_host else {
            return;
        };
        match event {
            Event::Opened => {}
            Event::Preedit(text, range) => {
                let to = text.len();
                if range.is_some() {
                    let cef_range = cef::Range {
                        from: 0 as _,
                        to: to as _,
                    };
                    host.ime_set_composition(
                        Some(&text.as_str().into()),
                        None,
                        Some(&cef::Range {
                            from: u32::MAX,
                            to: u32::MAX,
                        }),
                        Some(&cef_range),
                    );
                }
            }
            Event::Commit(text) => {
                host.ime_commit_text(
                    Some(&text.as_str().into()),
                    Some(&cef::Range {
                        from: u32::MAX,
                        to: u32::MAX,
                    }),
                    0,
                );
            }
            Event::Closed => {
                host.ime_finish_composing_text(true as _);
            }
        }
    }

    fn send_key_event(&mut self, event: &keyboard::Event) {
        let Some(host) = &self.browser_host else {
            return;
        };
        let cef_event = match event {
            keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                location,
                ..
            } => iced_key_to_cef_key(
                KeyPress::Press,
                Some(modified_key),
                Some(key),
                Some(physical_key),
                modifiers,
                text,
                *location,
            ),
            keyboard::Event::KeyReleased {
                key,
                modified_key,
                physical_key,
                modifiers,
                location,
                ..
            } => iced_key_to_cef_key(
                KeyPress::UnPress,
                Some(modified_key),
                Some(key),
                Some(physical_key),
                modifiers,
                &None,
                *location,
            ),
            keyboard::Event::ModifiersChanged(_) => None,
        };
        if let Some((mut event, should_send_char)) = cef_event {
            host.send_key_event(Some(&event));
            if should_send_char {
                event.type_ = cef::sys::cef_key_event_type_t::KEYEVENT_CHAR.into();
                #[cfg(target_os = "windows")]
                {
                    event.windows_key_code = event.character as _;
                }
                host.send_key_event(Some(&event));
            }
        }
    }

    fn send_mouse_event(
        &mut self,
        cursor: Option<iced::Point>,
        relative_cursor: Option<iced::Point>,
        popup_bounds: Option<iced::Rectangle>,
        popup_offset: Option<iced::Vector>,
        event: iced::mouse::Event,
    ) {
        let Some(host) = &self.browser_host else {
            return;
        };
        let inside_bounds = relative_cursor.is_some();
        use iced::advanced::mouse::click::Kind;
        let Some(point) = relative_cursor.or(cursor) else {
            return;
        };
        let point_for_cef = if self.is_over_popup_widget(point, popup_bounds) {
            if let Some(offset) = popup_offset {
                iced::Point::new(point.x + offset.x, point.y + offset.y)
            } else {
                point
            }
        } else {
            point
        };
        self.last_cursor_position = cursor;
        match event {
            iced::mouse::Event::ButtonPressed(button) => {
                host.set_focus(inside_bounds as _);
                self.last_mouse_down_on_view = !self.is_over_popup_widget(point, popup_bounds);
                if inside_bounds {
                    let previous = self.last_click.take();
                    self.last_click.replace(Click::new(point, button, previous));
                }
                let (modifier, type_) = match button {
                    iced::mouse::Button::Left => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_LEFT,
                    ),
                    iced::mouse::Button::Right => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_RIGHT,
                    ),
                    iced::mouse::Button::Middle => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_MIDDLE,
                    ),
                    _ => return,
                };
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: modifier.0 as u32,
                };
                let count = self
                    .last_click
                    .map(|c| match c.kind() {
                        Kind::Single => 1,
                        Kind::Double => 2,
                        Kind::Triple => 3,
                    })
                    .unwrap_or(1);
                host.send_mouse_click_event(Some(&event), type_.into(), false as _, count);
                if inside_bounds {
                    self.last_button_modifiers = modifier.0 as u32;
                }
            }
            iced::mouse::Event::ButtonReleased(button) => {
                let (modifier, type_) = match button {
                    iced::mouse::Button::Left => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_LEFT,
                    ),
                    iced::mouse::Button::Right => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_RIGHT,
                    ),
                    iced::mouse::Button::Middle => (
                        cef::sys::cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON,
                        cef::sys::cef_mouse_button_type_t::MBT_MIDDLE,
                    ),
                    _ => return,
                };
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: modifier.0 as u32,
                };
                host.send_mouse_click_event(Some(&event), type_.into(), true as _, 1);
                self.last_button_modifiers = 0;
            }
            iced::mouse::Event::WheelScrolled { delta } => {
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: cef::sys::cef_event_flags_t::EVENTFLAG_SCROLL_BY_PAGE.0 as u32,
                };
                match delta {
                    iced::mouse::ScrollDelta::Lines { x, y }
                    | iced::mouse::ScrollDelta::Pixels { x, y } => {
                        let y = if y < 0.0 {
                            -100.0
                        } else if y > 0.0 {
                            100.0
                        } else {
                            y
                        };
                        host.send_mouse_wheel_event(Some(&event), x as _, y as _);
                    }
                }
            }
            iced::mouse::Event::CursorEntered => {
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: 0,
                };
                host.send_mouse_move_event(Some(&event), false as _);
            }
            iced::mouse::Event::CursorMoved { position } => {
                let point_for_cef = if self.is_over_popup_widget(position, popup_bounds) {
                    if let Some(offset) = popup_offset {
                        iced::Point::new(position.x + offset.x, position.y + offset.y)
                    } else {
                        position
                    }
                } else {
                    position
                };
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: self.last_button_modifiers,
                };
                host.send_mouse_move_event(Some(&event), false as _);
            }
            iced::mouse::Event::CursorLeft => {
                let event = cef::MouseEvent {
                    x: point_for_cef.x as _,
                    y: point_for_cef.y as _,
                    modifiers: self.last_button_modifiers,
                };
                host.send_mouse_move_event(Some(&event), true as _);
            }
        }
    }
}

pub struct Webview<'a, Message> {
    browser_id: BrowserId,
    content: Element<'a, Message, Theme, Renderer>,
    popup_bounds: Option<iced::Rectangle>,
    popup_offset: Option<iced::Vector>,
    focused_node: Option<iced::Rectangle>,
    caret_offset: Option<f32>,
    composition_position: Option<iced::Point>,
    cursor_interaction: Option<mouse::Interaction>,
    on_bounds_changed: Option<Box<dyn Fn(iced::Rectangle) -> Message + 'a>>,
}

impl<'a, Message> Webview<'a, Message> {
    pub fn new(
        browser_id: BrowserId,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            browser_id,
            content: content.into(),
            popup_bounds: None,
            popup_offset: None,
            focused_node: None,
            caret_offset: None,
            composition_position: None,
            cursor_interaction: None,
            on_bounds_changed: None,
        }
    }

    pub fn popup_bounds(mut self, popup_bounds: Option<iced::Rectangle>) -> Self {
        self.popup_bounds = popup_bounds;
        self
    }

    pub fn popup_offset(mut self, popup_offset: Option<iced::Vector>) -> Self {
        self.popup_offset = popup_offset;
        self
    }

    pub fn focused_node(mut self, focused_node: Option<iced::Rectangle>) -> Self {
        self.focused_node = focused_node;
        self
    }

    pub fn caret_offset(mut self, caret_offset: Option<f32>) -> Self {
        self.caret_offset = caret_offset;
        self
    }

    pub fn composition_position(mut self, composition_position: Option<iced::Point>) -> Self {
        self.composition_position = composition_position;
        self
    }

    pub fn cursor_interaction(mut self, cursor_interaction: Option<mouse::Interaction>) -> Self {
        self.cursor_interaction = cursor_interaction;
        self
    }

    pub fn on_bounds_changed(
        mut self,
        on_bounds_changed: impl Fn(iced::Rectangle) -> Message + 'a,
    ) -> Self {
        self.on_bounds_changed = Some(Box::new(on_bounds_changed));
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
            browser_host: cef::browser_host_get_browser_by_identifier(self.browser_id.inner())
                .and_then(|browser| browser.host()),
            cursor_in_bounds: None,
            last_cursor_position: None,
            last_button_modifiers: 0,
            last_click: None,
            last_mouse_down_on_view: false,
            active_touch_ids: HashSet::new(),
            bounds: iced::Rectangle::with_size(iced::Size::ZERO),
            webview_focused: false,
            window_focused: true,
            last_begin_frame_time: std::time::Instant::now(),
            begin_frame_interval: Duration::from_millis(16),
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<CefState>();
        state.update_browser(self.browser_id);
        let bounds = layout.bounds();
        if state.bounds != bounds {
            state.bounds = bounds.into();
            if let Some(on_bounds_changed) = &self.on_bounds_changed {
                shell.publish(on_bounds_changed(bounds));
            } else {
                resize(self.browser_id, bounds);
            }
        }

        match event {
            Event::Keyboard(event) => {
                if cursor.position_in(bounds).is_some() {
                    state.send_key_event(event);
                    shell.capture_event();
                }
            }
            Event::Mouse(event) => {
                state.cursor_in_bounds = cursor.position_in(bounds);
                if cursor.position().is_some() {
                    state.update_focus(cursor.position_in(bounds).is_some());
                    state.send_mouse_event(
                        cursor.position(),
                        cursor.position_in(bounds),
                        self.popup_bounds,
                        self.popup_offset,
                        *event,
                    );
                    if cursor.position_in(bounds).is_some() {
                        shell.capture_event();
                    }
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(_)) => {
                state.try_send_begin_frame();
                shell.request_input_method::<String>(&state.input_method(
                    bounds,
                    self.focused_node,
                    self.caret_offset,
                    self.composition_position,
                ));
            }
            Event::InputMethod(event) => {
                state.send_ime_event(event);
                shell.request_redraw();
                if cursor.position_in(bounds).is_some() {
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
            return self.cursor_interaction.unwrap_or(mouse::Interaction::None);
        }
        mouse::Interaction::None
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
