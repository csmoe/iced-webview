use crate::{
    BrowserId, Webview,
    client::{CefFrame, ClientEventSubscriber},
};
use crate::{
    client::{ClientBuilder, IcyClient, IcyClientState, LifeSpanEvent, LoadEvent},
    request::{IcyRequestContextHandler, RequestContextHandlerBuilder},
};
use cef;
use cef::*;
use iced::{
    Element, Subscription, Task,
    keyboard::{
        Key,
        key::{Code, Named, Physical},
    },
    window,
};
use iced_core::mouse::Click;
use std::{
    cell::RefCell, collections::BTreeMap, fmt::Debug, sync::atomic::AtomicUsize, time::Duration,
};
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

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
    UpdateCaretOffset(BrowserId, f32),
    FocusedNodeChanged(BrowserId, iced::Rectangle),
    KeyEvent(iced::keyboard::Event),
    MouseEvent(iced::Point, iced::mouse::Event),
    InputMethodEvent(iced_core::input_method::Event),
    UpdateView(CefFrame),
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
            Self::UpdateView(browser_id) => f.debug_tuple("UpdateView").field(browser_id).finish(),
            Self::UpdateCaretOffset(browser_id, offset) => f
                .debug_tuple("UpdateCaretOffset")
                .field(browser_id)
                .field(offset)
                .finish(),
            Self::FocusedNodeChanged(browser_id, rect) => f
                .debug_tuple("EditableNodeFocused")
                .field(browser_id)
                .field(rect)
                .finish(),
            Self::MouseEvent(point, event) => f
                .debug_tuple("MouseEvent")
                .field(point)
                .field(event)
                .finish(),
            Self::InputMethodEvent(event) => {
                f.debug_tuple("InputMethodEvent").field(event).finish()
            }
            Self::KeyEvent(event) => f.debug_tuple("KeyEvent").field(event).finish(),
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

pub(crate) fn get_cursor_type(browser_id: BrowserId) -> Option<cef::CursorType> {
    WEBVIEW_STATES.with_borrow(|states| {
        states
            .get(&browser_id)
            .map(|state| state.display.cursor_type.borrow().clone())
    })
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
            if let Some(host) =
                browser_host_get_browser_by_identifier(browser_id.inner()).and_then(|b| b.host())
            {
                host.was_resized();
            }
        }
    })
}

pub struct CefComponent {
    view: Option<CefFrame>,
    host: Option<BrowserHost>,
    focused_node: Option<iced::Rectangle>,
    caret_offset: Option<f32>,
    last_click: Option<Click>,
    last_button_modifiers: u32,
}

impl CefComponent {
    pub fn new() -> Self {
        Self {
            view: None,
            focused_node: None,
            caret_offset: None,
            last_click: None,
            last_button_modifiers: 0,
            host: None,
        }
    }

    fn send_ime_event(&mut self, event: iced_core::input_method::Event, caret_offset: Option<f32>) {
        use cef::ImplBrowserHost;
        use iced_core::input_method::Event;

        let Some(host) = &self.view.as_ref().and_then(|view| {
            cef::browser_host_get_browser_by_identifier(view.browser_id().inner())
                .and_then(|b| b.host())
        }) else {
            return;
        };
        let Some(_click) = self.last_click else {
            return;
        };
        let Some(caret_offset) = caret_offset else {
            return;
        };

        match event {
            Event::Opened => {}
            Event::Preedit(text, range) => {
                if range.is_some() {
                    let cef_range = cef::Range {
                        from: caret_offset as _,
                        to: text.len() as _,
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

    fn send_key_event(&mut self, event: iced::keyboard::Event) {
        use iced::keyboard;

        let Some(host) = &self.view.as_ref().and_then(|view| {
            cef::browser_host_get_browser_by_identifier(view.browser_id().inner())
                .and_then(|b| b.host())
        }) else {
            return;
        };
        let cef_event = match event {
            iced::keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            } => iced_key_to_cef_key(
                KeyPress::Press,
                Some(modified_key),
                Some(key),
                physical_key.into(),
                modifiers,
                text,
            ),
            iced::keyboard::Event::KeyReleased {
                key,
                modified_key,
                physical_key,
                modifiers,
                ..
            } => iced_key_to_cef_key(
                KeyPress::Unpress,
                Some(modified_key),
                Some(key),
                physical_key.into(),
                modifiers,
                None,
            ),
            iced::keyboard::Event::ModifiersChanged(_) => None,
        };
        host.send_key_event(cef_event.as_ref());

        #[derive(Debug, PartialEq, Eq)]
        enum KeyPress {
            Press,
            Unpress,
        }

        fn iced_key_to_cef_key(
            press: KeyPress,
            modified_key: Option<keyboard::Key>,
            key: Option<keyboard::Key>,
            physical_key: Option<keyboard::key::Physical>,
            modifiers: keyboard::Modifiers,
            _text: Option<iced_core::SmolStr>,
        ) -> Option<cef::KeyEvent> {
            let mut modifiers_: cef::EventFlags =
                cef::sys::cef_event_flags_t::EVENTFLAG_NONE.into();
            let mut ty = match press {
                KeyPress::Press => cef::sys::cef_key_event_type_t::KEYEVENT_KEYDOWN,
                KeyPress::Unpress => cef::sys::cef_key_event_type_t::KEYEVENT_KEYUP,
            };
            if matches!(modified_key, Some(keyboard::Key::Character(_)))
                && matches!(press, KeyPress::Press)
            {
                ty = cef::sys::cef_key_event_type_t::KEYEVENT_CHAR;
            };

            if modifiers.control() {
                *modifiers_.as_mut() |= cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN;
            } else if modifiers.alt() {
                *modifiers_.as_mut() |= cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN;
            } else if modifiers.shift() {
                *modifiers_.as_mut() |= cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN;
            } else if modifiers.logo() {
                *modifiers_.as_mut() |= cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN;
            }

            let mut cef_keyevent = cef::KeyEvent::default();

            cef_keyevent.type_ = ty.into();
            cef_keyevent.windows_key_code = match &modified_key {
                Some(keyboard::Key::Named(code)) => to_virtual_key(*code) as _,
                Some(keyboard::Key::Character(c)) => c.chars().next().unwrap_or_default() as _,
                _ => 0,
            };

            cef_keyevent.native_key_code = match &physical_key {
                Some(Physical::Code(code)) => to_native_key(*code) as _,
                _ => 0,
            };
            cef_keyevent.is_system_key = false as _;
            cef_keyevent.focus_on_editable_field = false as _;
            cef_keyevent.modifiers = modifiers_.as_ref().0;
            cef_keyevent.character = match &modified_key {
                Some(Key::Character(c)) => c.chars().next().unwrap_or_default() as _,
                Some(Key::Named(named)) => named_key_to_text(*named).unwrap_or_default() as _,
                _ => 0,
            };
            cef_keyevent.unmodified_character = match &key {
                Some(Key::Character(c)) => c.chars().next().unwrap_or_default() as _,
                Some(Key::Named(named)) => named_key_to_text(*named).unwrap_or_default() as _,
                _ => 0,
            };

            Some(cef_keyevent)
        }
    }

    fn send_mouse_event(&mut self, point: iced::Point, event: iced::mouse::Event) {
        let Some(host) = &self.view.as_ref().and_then(|view| {
            cef::browser_host_get_browser_by_identifier(view.browser_id().inner())
                .and_then(|b| b.host())
        }) else {
            return;
        };
        use iced::advanced::mouse::click::Kind;
        match event {
            iced::mouse::Event::ButtonPressed(button) => {
                let previous = self.last_click.take();
                self.last_click.replace(Click::new(point, button, previous));
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
                self.last_button_modifiers = modifier.0;

                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: modifier.0,
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
                self.last_button_modifiers = 0;
                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: modifier.0,
                };
                host.send_mouse_click_event(Some(&event), type_.into(), true as _, 1);
            }
            iced::mouse::Event::WheelScrolled { delta } => {
                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: cef::sys::cef_event_flags_t::EVENTFLAG_SCROLL_BY_PAGE.0,
                };
                match delta {
                    iced::mouse::ScrollDelta::Lines { x, y } => {
                        let y = if y < 0.0 {
                            -100.0
                        } else if y > 0.0 {
                            100.0
                        } else {
                            y
                        };
                        host.send_mouse_wheel_event(Some(&event), x as _, y as _);
                    }
                    iced::mouse::ScrollDelta::Pixels { x, y } => {
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
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: 0,
                };
                host.send_mouse_move_event(Some(&event), false as _);
            }
            iced::mouse::Event::CursorMoved { position } => {
                let event = cef::MouseEvent {
                    x: position.x as _,
                    y: position.y as _,
                    modifiers: self.last_button_modifiers,
                };
                host.send_mouse_move_event(Some(&event), false as _);
            }
            iced::mouse::Event::CursorLeft => {
                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: self.last_button_modifiers,
                };
                host.send_mouse_move_event(Some(&event), true as _);
            }
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
            Task::stream(UnboundedReceiverStream::new(render_rx)).map(CefMessage::UpdateView),
            Task::stream(tokio_stream::wrappers::ReceiverStream::new(lifespan_rx)).map(
                move |event| match event {
                    LifeSpanEvent::Closed { browser_id } => CefMessage::Closed(browser_id.into()),
                    LifeSpanEvent::Created { browser_id } => CefMessage::Created(browser_id),
                },
            ),
            Task::stream(UnboundedReceiverStream::new(process_message_rx)).map(|msg| match msg {
                crate::client::CefIpcMessage::FocusedNodeChanged {
                    browser_id,
                    x,
                    y,
                    width,
                    height,
                } => CefMessage::FocusedNodeChanged(
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
            CefMessage::InputMethodEvent(event) => {
                self.send_ime_event(event, self.caret_offset);
                CefAction::None
            }
            CefMessage::KeyEvent(event) => {
                self.send_key_event(event);
                CefAction::None
            }
            CefMessage::MouseEvent(point, event) => {
                self.send_mouse_event(point, event);
                CefAction::None
            }
            CefMessage::UpdateView(view) => {
                self.view.replace(view);
                CefAction::None
            }
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
                if let Some(host) = cef::browser_host_get_browser_by_identifier(browser_id.inner())
                    .and_then(|b| b.host())
                {
                    host.send_external_begin_frame();
                    self.host.replace(host);
                }

                CefAction::Created(browser_id)
            }
            CefMessage::UpdateCaretOffset(_, offset) => {
                self.caret_offset.replace(offset);
                CefAction::None
            }
            CefMessage::FocusedNodeChanged(_, node) => {
                self.focused_node.replace(node);
                CefAction::None
            }
            CefMessage::Closed(browser_id) => CefAction::Closed(browser_id),
            CefMessage::Loaded(browwser_id) => CefAction::Loaded(browwser_id),
        }
    }

    pub fn view(&self) -> Element<'_, CefMessage> {
        if let Some(view) = self.view.as_ref() {
            iced::widget::responsive(|size| {
                Webview::new(
                    view.browser_id(),
                    iced::widget::shader(view.clone())
                        .width(size.width)
                        .height(size.height),
                )
                .focused_node(self.focused_node)
                .caret_offset(self.caret_offset)
                .on_key_event(CefMessage::KeyEvent)
                .on_input_method_event(CefMessage::InputMethodEvent)
                .on_mouse_event(CefMessage::MouseEvent)
                .into()
            })
            .into()
        } else {
            if let Some(host) = self.host.as_ref() {
                host.send_external_begin_frame();
            }
            iced::widget::space().into()
        }
    }

    pub fn subscription(&self) -> Subscription<CefMessage> {
        Subscription::none()
    }
}

fn to_native_key(keycode: Code) -> u32 {
    match keycode {
        Code::KeyA => {
            if cfg!(target_os = "macos") {
                0x00
            } else {
                0x41
            }
        }
        Code::KeyB => {
            if cfg!(target_os = "macos") {
                0x0B
            } else {
                0x42
            }
        }
        Code::KeyC => {
            if cfg!(target_os = "macos") {
                0x08
            } else {
                0x43
            }
        }
        Code::KeyD => {
            if cfg!(target_os = "macos") {
                0x02
            } else {
                0x44
            }
        }
        Code::KeyE => {
            if cfg!(target_os = "macos") {
                0x0E
            } else {
                0x45
            }
        }
        Code::KeyF => {
            if cfg!(target_os = "macos") {
                0x03
            } else {
                0x46
            }
        }
        Code::KeyG => {
            if cfg!(target_os = "macos") {
                0x05
            } else {
                0x47
            }
        }
        Code::KeyH => {
            if cfg!(target_os = "macos") {
                0x04
            } else {
                0x48
            }
        }
        Code::KeyI => {
            if cfg!(target_os = "macos") {
                0x22
            } else {
                0x49
            }
        }
        Code::KeyJ => {
            if cfg!(target_os = "macos") {
                0x26
            } else {
                0x4A
            }
        }
        Code::KeyK => {
            if cfg!(target_os = "macos") {
                0x28
            } else {
                0x4B
            }
        }
        Code::KeyL => {
            if cfg!(target_os = "macos") {
                0x25
            } else {
                0x4C
            }
        }
        Code::KeyM => {
            if cfg!(target_os = "macos") {
                0x2E
            } else {
                0x4D
            }
        }
        Code::KeyN => {
            if cfg!(target_os = "macos") {
                0x2D
            } else {
                0x4E
            }
        }
        Code::KeyO => {
            if cfg!(target_os = "macos") {
                0x1F
            } else {
                0x4F
            }
        }
        Code::KeyP => {
            if cfg!(target_os = "macos") {
                0x23
            } else {
                0x50
            }
        }
        Code::KeyQ => {
            if cfg!(target_os = "macos") {
                0x0C
            } else {
                0x51
            }
        }
        Code::KeyR => {
            if cfg!(target_os = "macos") {
                0x0F
            } else {
                0x52
            }
        }
        Code::KeyS => {
            if cfg!(target_os = "macos") {
                0x01
            } else {
                0x53
            }
        }
        Code::KeyT => {
            if cfg!(target_os = "macos") {
                0x11
            } else {
                0x54
            }
        }
        Code::KeyU => {
            if cfg!(target_os = "macos") {
                0x20
            } else {
                0x55
            }
        }
        Code::KeyV => {
            if cfg!(target_os = "macos") {
                0x09
            } else {
                0x56
            }
        }
        Code::KeyW => {
            if cfg!(target_os = "macos") {
                0x0D
            } else {
                0x57
            }
        }
        Code::KeyX => {
            if cfg!(target_os = "macos") {
                0x07
            } else {
                0x58
            }
        }
        Code::KeyY => {
            if cfg!(target_os = "macos") {
                0x10
            } else {
                0x59
            }
        }
        Code::KeyZ => {
            if cfg!(target_os = "macos") {
                0x06
            } else {
                0x5A
            }
        }

        Code::Digit0 => {
            if cfg!(target_os = "macos") {
                0x1D
            } else {
                0x30
            }
        }
        Code::Digit1 => {
            if cfg!(target_os = "macos") {
                0x12
            } else {
                0x31
            }
        }
        Code::Digit2 => {
            if cfg!(target_os = "macos") {
                0x13
            } else {
                0x32
            }
        }
        Code::Digit3 => {
            if cfg!(target_os = "macos") {
                0x14
            } else {
                0x33
            }
        }
        Code::Digit4 => {
            if cfg!(target_os = "macos") {
                0x15
            } else {
                0x34
            }
        }
        Code::Digit5 => {
            if cfg!(target_os = "macos") {
                0x17
            } else {
                0x35
            }
        }
        Code::Digit6 => {
            if cfg!(target_os = "macos") {
                0x16
            } else {
                0x36
            }
        }
        Code::Digit7 => {
            if cfg!(target_os = "macos") {
                0x1A
            } else {
                0x37
            }
        }
        Code::Digit8 => {
            if cfg!(target_os = "macos") {
                0x1C
            } else {
                0x38
            }
        }
        Code::Digit9 => {
            if cfg!(target_os = "macos") {
                0x19
            } else {
                0x39
            }
        }

        // Function keys
        Code::F1 => {
            if cfg!(target_os = "macos") {
                0x7A
            } else {
                0x70
            }
        }
        Code::F2 => {
            if cfg!(target_os = "macos") {
                0x78
            } else {
                0x71
            }
        }
        Code::F3 => {
            if cfg!(target_os = "macos") {
                0x63
            } else {
                0x72
            }
        }
        Code::F4 => {
            if cfg!(target_os = "macos") {
                0x76
            } else {
                0x73
            }
        }
        Code::F5 => {
            if cfg!(target_os = "macos") {
                0x60
            } else {
                0x74
            }
        }
        Code::F6 => {
            if cfg!(target_os = "macos") {
                0x61
            } else {
                0x75
            }
        }
        Code::F7 => {
            if cfg!(target_os = "macos") {
                0x62
            } else {
                0x76
            }
        }
        Code::F8 => {
            if cfg!(target_os = "macos") {
                0x64
            } else {
                0x77
            }
        }
        Code::F9 => {
            if cfg!(target_os = "macos") {
                0x65
            } else {
                0x78
            }
        }
        Code::F10 => {
            if cfg!(target_os = "macos") {
                0x6D
            } else {
                0x79
            }
        }
        Code::F11 => {
            if cfg!(target_os = "macos") {
                0x67
            } else {
                0x7A
            }
        }
        Code::F12 => {
            if cfg!(target_os = "macos") {
                0x6F
            } else {
                0x7B
            }
        }

        Code::Enter => {
            if cfg!(target_os = "macos") {
                0x24
            } else {
                0x0D
            }
        }
        Code::Space => {
            if cfg!(target_os = "macos") {
                0x31
            } else {
                0x20
            }
        }
        Code::Backspace => {
            if cfg!(target_os = "macos") {
                0x33
            } else {
                0x08
            }
        }
        Code::Delete => {
            if cfg!(target_os = "macos") {
                0x75
            } else {
                0x2E
            }
        }
        Code::Tab => {
            if cfg!(target_os = "macos") {
                0x30
            } else {
                0x09
            }
        }
        Code::Escape => {
            if cfg!(target_os = "macos") {
                0x35
            } else {
                0x1B
            }
        }
        Code::Insert => {
            if cfg!(target_os = "macos") {
                0x72
            } else {
                0x2D
            }
        }
        Code::Home => {
            if cfg!(target_os = "macos") {
                0x73
            } else {
                0x24
            }
        }
        Code::End => {
            if cfg!(target_os = "macos") {
                0x77
            } else {
                0x23
            }
        }
        Code::PageUp => {
            if cfg!(target_os = "macos") {
                0x74
            } else {
                0x21
            }
        }
        Code::PageDown => {
            if cfg!(target_os = "macos") {
                0x79
            } else {
                0x22
            }
        }

        // Arrow keys
        Code::ArrowLeft => {
            if cfg!(target_os = "macos") {
                0x7B
            } else {
                0x25
            }
        }
        Code::ArrowUp => {
            if cfg!(target_os = "macos") {
                0x7E
            } else {
                0x26
            }
        }
        Code::ArrowRight => {
            if cfg!(target_os = "macos") {
                0x7C
            } else {
                0x27
            }
        }
        Code::ArrowDown => {
            if cfg!(target_os = "macos") {
                0x7D
            } else {
                0x28
            }
        }

        // Modifier keys
        Code::ShiftLeft => {
            if cfg!(target_os = "macos") {
                0x38
            } else {
                0xA0
            }
        }
        Code::ShiftRight => {
            if cfg!(target_os = "macos") {
                0x3C
            } else {
                0xA1
            }
        }
        Code::ControlLeft => {
            if cfg!(target_os = "macos") {
                0x3B
            } else {
                0xA2
            }
        }
        Code::ControlRight => {
            if cfg!(target_os = "macos") {
                0x3E
            } else {
                0xA3
            }
        }
        Code::AltLeft => {
            if cfg!(target_os = "macos") {
                0x3A
            } else {
                0xA4
            }
        }
        Code::AltRight => {
            if cfg!(target_os = "macos") {
                0x3D
            } else {
                0xA5
            }
        }
        Code::SuperLeft => {
            if cfg!(target_os = "macos") {
                0x37
            } else {
                0x5B
            }
        }
        Code::SuperRight => {
            if cfg!(target_os = "macos") {
                0x36
            } else {
                0x5C
            }
        }

        // Lock keys
        Code::CapsLock => {
            if cfg!(target_os = "macos") {
                0x39
            } else {
                0x14
            }
        }
        Code::NumLock => {
            if cfg!(target_os = "macos") {
                0x47
            } else {
                0x90
            }
        }
        Code::ScrollLock => 0x91,

        Code::Semicolon => {
            if cfg!(target_os = "macos") {
                0x29
            } else {
                0xBA
            }
        }
        Code::Equal => {
            if cfg!(target_os = "macos") {
                0x18
            } else {
                0xBB
            }
        }
        Code::Comma => {
            if cfg!(target_os = "macos") {
                0x2B
            } else {
                0xBC
            }
        }
        Code::Minus => {
            if cfg!(target_os = "macos") {
                0x1B
            } else {
                0xBD
            }
        }
        Code::Period => {
            if cfg!(target_os = "macos") {
                0x2F
            } else {
                0xBE
            }
        }
        Code::Slash => {
            if cfg!(target_os = "macos") {
                0x2C
            } else {
                0xBF
            }
        }
        Code::Backquote => {
            if cfg!(target_os = "macos") {
                0x32
            } else {
                0xC0
            }
        }
        Code::BracketLeft => {
            if cfg!(target_os = "macos") {
                0x21
            } else {
                0xDB
            }
        }
        Code::Backslash => {
            if cfg!(target_os = "macos") {
                0x2A
            } else {
                0xDC
            }
        }
        Code::BracketRight => {
            if cfg!(target_os = "macos") {
                0x1E
            } else {
                0xDD
            }
        }
        Code::Quote => {
            if cfg!(target_os = "macos") {
                0x27
            } else {
                0xDE
            }
        }

        Code::Numpad0 => {
            if cfg!(target_os = "macos") {
                0x52
            } else {
                0x60
            }
        }
        Code::Numpad1 => {
            if cfg!(target_os = "macos") {
                0x53
            } else {
                0x61
            }
        }
        Code::Numpad2 => {
            if cfg!(target_os = "macos") {
                0x54
            } else {
                0x62
            }
        }
        Code::Numpad3 => {
            if cfg!(target_os = "macos") {
                0x55
            } else {
                0x63
            }
        }
        Code::Numpad4 => {
            if cfg!(target_os = "macos") {
                0x56
            } else {
                0x64
            }
        }
        Code::Numpad5 => {
            if cfg!(target_os = "macos") {
                0x57
            } else {
                0x65
            }
        }
        Code::Numpad6 => {
            if cfg!(target_os = "macos") {
                0x58
            } else {
                0x66
            }
        }
        Code::Numpad7 => {
            if cfg!(target_os = "macos") {
                0x59
            } else {
                0x67
            }
        }
        Code::Numpad8 => {
            if cfg!(target_os = "macos") {
                0x5B
            } else {
                0x68
            }
        }
        Code::Numpad9 => {
            if cfg!(target_os = "macos") {
                0x5C
            } else {
                0x69
            }
        }
        Code::NumpadMultiply => {
            if cfg!(target_os = "macos") {
                0x43
            } else {
                0x6A
            }
        }
        Code::NumpadAdd => {
            if cfg!(target_os = "macos") {
                0x45
            } else {
                0x6B
            }
        }
        Code::NumpadSubtract => {
            if cfg!(target_os = "macos") {
                0x4E
            } else {
                0x6D
            }
        }
        Code::NumpadDecimal => {
            if cfg!(target_os = "macos") {
                0x41
            } else {
                0x6E
            }
        }
        Code::NumpadDivide => {
            if cfg!(target_os = "macos") {
                0x4B
            } else {
                0x6F
            }
        }

        _ => 0,
    }
}

fn to_virtual_key(keycode: Named) -> i32 {
    match keycode {
        Named::F1 => 0x70,
        Named::F2 => 0x71,
        Named::F3 => 0x72,
        Named::F4 => 0x73,
        Named::F5 => 0x74,
        Named::F6 => 0x75,
        Named::F7 => 0x76,
        Named::F8 => 0x77,
        Named::F9 => 0x78,
        Named::F10 => 0x79,
        Named::F11 => 0x7A,
        Named::F12 => 0x7B,

        Named::Enter => 0x0D,
        Named::Space => 0x20,
        Named::Backspace => 0x08,
        Named::Delete => 0x2E,
        Named::Tab => 0x09,
        Named::Escape => 0x1B,
        Named::Insert => 0x2D,
        Named::Home => 0x24,
        Named::End => 0x23,
        Named::PageUp => 0x21,
        Named::PageDown => 0x22,

        Named::ArrowLeft => 0x25,
        Named::ArrowUp => 0x26,
        Named::ArrowRight => 0x27,
        Named::ArrowDown => 0x28,

        Named::Shift => 0x10,
        Named::Control => 0x11,
        Named::Alt => 0x12,
        Named::Super => 0x5B,

        Named::CapsLock => 0x14,
        Named::NumLock => 0x90,
        Named::ScrollLock => 0x91,
        _ => 0,
    }
}

fn named_key_to_text(named_key: Named) -> Option<char> {
    match named_key {
        Named::Enter => Some('\r'),
        Named::Backspace => Some('\x08'),
        Named::Tab => Some('\t'),
        Named::Space => Some(' '),
        Named::Escape => Some('\x1b'),
        _ => None,
    }
}
