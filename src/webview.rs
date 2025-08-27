use super::instance::{get_cursor_type, get_pixels, resize};
use crate::BrowserId;
use cef::{ImplBrowser, ImplBrowserHost};
use iced::advanced::widget;
use iced::keyboard::key::Code;
use iced::{self, keyboard::key::Physical};
use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme,
    advanced::{
        Clipboard, Shell,
        layout::{Layout, Limits, Node},
        mouse::Click,
        renderer::Style,
        widget::{Id, Operation, Tree, Widget, tree},
    },
    keyboard::{self, Key, key::Named},
    mouse::{self, Cursor},
    widget::horizontal_space,
};
use iced_core;

pub fn webview<'a, Message: 'a>(browser_id: BrowserId, size: iced::Size) -> Webview<'a, Message> {
    let content = get_pixels::<Message>(browser_id).map_or_else(
        || Element::from(horizontal_space().width(0)),
        |img| img.width(size.width).height(size.height).into(),
    );
    Webview::new(browser_id, content)
}

struct CefState {
    last_click: Option<Click>,
    last_button_modifiers: u32,
    focused_editable_node: Option<iced::Rectangle>,
    caret_offset: Option<f32>,
    bounds: iced::Rectangle,
    browser_id: BrowserId,
    browser_host: Option<cef::BrowserHost>,
    is_closed: bool,
}

impl CefState {
    fn resize(&mut self, bound: iced::Rectangle) {
        if self.is_closed {
            return;
        }

        let rect = cef::Rect {
            x: bound.x as _,
            y: bound.y as _,
            width: bound.width as _,
            height: bound.height as _,
        };
        resize(self.browser_id, rect);
    }

    fn close(&mut self) {
        let Some(host) = &self.browser_host else {
            return;
        };
        self.is_closed = host.try_close_browser() == 1;
    }

    fn set_caret_offset(&mut self, caret_offset: f32) {
        self.caret_offset.replace(caret_offset);
    }

    fn set_focused_editable_node(&mut self, node: iced::Rectangle) {
        self.focused_editable_node.replace(node);
    }

    fn input_method(&self, bound: iced::Rectangle) -> iced_core::InputMethod {
        if !self.is_closed {
            if let Some(node) = &self.focused_editable_node {
                if let Some(offset) = &self.caret_offset {
                    let x = node.position().x + bound.position().x;
                    let y = node.position().y + bound.position().y;
                    return iced_core::input_method::InputMethod::Enabled {
                        position: iced::Point::new(x + *offset, y),
                        purpose: iced_core::input_method::Purpose::Normal,
                        preedit: None,
                    };
                }
            }
        }
        iced_core::input_method::InputMethod::Disabled
    }

    fn send_ime_event(&mut self, event: iced_core::input_method::Event) {
        let Some(host) = &self.browser_host else {
            return;
        };
        use iced_core::input_method::Event;
        let Some(_click) = self.last_click else {
            return;
        };
        let Some(caret_offset) = self.caret_offset else {
            return;
        };

        let Some(_editable_node_focused) = self.focused_editable_node else {
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
                        0,
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
        if self.is_closed {
            return;
        }
        let Some(host) = &mut self.browser_host else {
            return;
        };
        let cef_event = match event {
            keyboard::Event::KeyPressed {
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
            keyboard::Event::KeyReleased {
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
            keyboard::Event::ModifiersChanged(_) => None,
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
            let mut modifiers_ = 0;
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
                modifiers_ |= cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN as u32;
            } else if modifiers.alt() {
                modifiers_ |= cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN as u32;
            } else if modifiers.shift() {
                modifiers_ |= cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN as u32;
            } else if modifiers.logo() {
                modifiers_ |= cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN as u32;
            }

            let mut cef_keyevent = cef::KeyEvent::default();

            cef_keyevent.type_ = ty.into();
            cef_keyevent.windows_key_code = match &modified_key {
                Some(keyboard::Key::Named(code)) => {
                    named_to_virtual_key(*code).unwrap_or_default() as _
                }
                Some(keyboard::Key::Character(c)) => c.chars().next().unwrap_or_default() as _,
                _ => 0,
            };

            cef_keyevent.native_key_code = match &physical_key {
                Some(Physical::Code(code)) => key_to_native_key(*code).unwrap_or_default(),
                _ => 0,
            };
            cef_keyevent.is_system_key = false as _;
            cef_keyevent.focus_on_editable_field = false as _;
            cef_keyevent.modifiers = modifiers_;
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

        fn key_to_native_key(key: Code) -> Option<i32> {
            match key {
                // Letters
                Code::KeyA => Some(30),
                Code::KeyB => Some(48),
                Code::KeyC => Some(46),
                Code::KeyD => Some(32),
                Code::KeyE => Some(18),
                Code::KeyF => Some(33),
                Code::KeyG => Some(34),
                Code::KeyH => Some(35),
                Code::KeyI => Some(23),
                Code::KeyJ => Some(36),
                Code::KeyK => Some(37),
                Code::KeyL => Some(38),
                Code::KeyM => Some(50),
                Code::KeyN => Some(49),
                Code::KeyO => Some(24),
                Code::KeyP => Some(25),
                Code::KeyQ => Some(16),
                Code::KeyR => Some(19),
                Code::KeyS => Some(31),
                Code::KeyT => Some(20),
                Code::KeyU => Some(22),
                Code::KeyV => Some(47),
                Code::KeyW => Some(17),
                Code::KeyX => Some(45),
                Code::KeyY => Some(21),
                Code::KeyZ => Some(44),

                // Numbers (top row)
                Code::Digit1 => Some(2),
                Code::Digit2 => Some(3),
                Code::Digit3 => Some(4),
                Code::Digit4 => Some(5),
                Code::Digit5 => Some(6),
                Code::Digit6 => Some(7),
                Code::Digit7 => Some(8),
                Code::Digit8 => Some(9),
                Code::Digit9 => Some(10),
                Code::Digit0 => Some(11),

                // Function keys
                Code::F1 => Some(59),
                Code::F2 => Some(60),
                Code::F3 => Some(61),
                Code::F4 => Some(62),
                Code::F5 => Some(63),
                Code::F6 => Some(64),
                Code::F7 => Some(65),
                Code::F8 => Some(66),
                Code::F9 => Some(67),
                Code::F10 => Some(68),
                Code::F11 => Some(87),
                Code::F12 => Some(88),

                // Numpad
                Code::Numpad0 => Some(82),
                Code::Numpad1 => Some(79),
                Code::Numpad2 => Some(80),
                Code::Numpad3 => Some(81),
                Code::Numpad4 => Some(75),
                Code::Numpad5 => Some(76),
                Code::Numpad6 => Some(77),
                Code::Numpad7 => Some(71),
                Code::Numpad8 => Some(72),
                Code::Numpad9 => Some(73),
                Code::NumpadAdd => Some(78),
                Code::NumpadSubtract => Some(74),
                Code::NumpadMultiply => Some(55),
                Code::NumpadDivide => Some(53),
                Code::NumpadEnter => Some(28),
                Code::NumpadDecimal => Some(83),

                // Special keys
                Code::Escape => Some(1),
                Code::Tab => Some(15),
                Code::CapsLock => Some(58),
                Code::ShiftLeft => Some(42),
                Code::ControlLeft => Some(29),
                Code::AltLeft => Some(56),
                Code::Space => Some(57),
                Code::AltRight => Some(56), // Same as AltLeft, but with extended flag
                Code::ControlRight => Some(29), // Same as ControlLeft, but with extended flag
                Code::ShiftRight => Some(54),
                Code::Enter => Some(28),
                Code::Backspace => Some(14),

                // Navigation
                Code::Insert => Some(82),     // Extended
                Code::Delete => Some(83),     // Extended
                Code::Home => Some(71),       // Extended
                Code::End => Some(79),        // Extended
                Code::PageUp => Some(73),     // Extended
                Code::PageDown => Some(81),   // Extended
                Code::ArrowUp => Some(72),    // Extended
                Code::ArrowLeft => Some(75),  // Extended
                Code::ArrowDown => Some(80),  // Extended
                Code::ArrowRight => Some(77), // Extended

                // Punctuation and special characters
                Code::Minus => Some(12),
                Code::Equal => Some(13),
                Code::BracketLeft => Some(26),
                Code::BracketRight => Some(27),
                Code::Backslash => Some(43),
                Code::Semicolon => Some(39),
                Code::Quote => Some(40),
                Code::Backquote => Some(41),
                Code::Comma => Some(51),
                Code::Period => Some(52),
                Code::Slash => Some(53),

                // Other keys
                Code::PrintScreen => Some(42), // Special handling might be needed
                Code::ScrollLock => Some(70),
                Code::Pause => Some(69),
                Code::ContextMenu => Some(93), // Extended

                // Anything else
                _ => None,
            }
        }

        /*
        fn key_to_native_key(key: Named) -> i32 {
            match key {
                // Control keys (macOS Virtual Key Codes)
                Named::Enter => 0x24,     // kVK_Return
                Named::Tab => 0x30,       // kVK_Tab
                Named::Space => 0x31,     // kVK_Space
                Named::Backspace => 0x33, // kVK_Delete
                Named::Escape => 0x35,    // kVK_Escape
                Named::Meta => 0x37,      // kVK_Command
                Named::Shift => 0x38,     // kVK_Shift
                Named::CapsLock => 0x39,  // kVK_CapsLock
                Named::Alt => 0x3A,       // kVK_Option
                Named::Control => 0x3B,   // kVK_Control
                Named::Fn => 0x3F,        // kVK_Function

                // Volume controls (macOS Media Keys)
                Named::AudioVolumeUp => 0x48,   // kVK_VolumeUp
                Named::AudioVolumeDown => 0x49, // kVK_VolumeDown
                Named::AudioVolumeMute => 0x4A, // kVK_Mute

                // Function keys (F1-F20)
                Named::F1 => 0x7A,
                Named::F2 => 0x78,
                Named::F3 => 0x63,
                Named::F4 => 0x76,
                Named::F5 => 0x60,
                Named::F6 => 0x61,
                Named::F7 => 0x62,
                Named::F8 => 0x64,
                Named::F9 => 0x65,
                Named::F10 => 0x6D,
                Named::F11 => 0x67,
                Named::F12 => 0x6F,
                Named::F13 => 0x69,
                Named::F14 => 0x6B,
                Named::F15 => 0x71,
                Named::F16 => 0x6A,
                Named::F17 => 0x40,
                Named::F18 => 0x4F,
                Named::F19 => 0x50,
                Named::F20 => 0x5A,

                // Navigation keys
                Named::Help => 0x72,        // kVK_Help
                Named::Home => 0x73,        // kVK_Home
                Named::PageUp => 0x74,      // kVK_PageUp
                Named::Delete => 0x75,      // kVK_ForwardDelete
                Named::End => 0x77,         // kVK_End
                Named::PageDown => 0x79,    // kVK_PageDown
                Named::ArrowLeft => 0x7B,   // kVK_LeftArrow
                Named::ArrowRight => 0x7C,  // kVK_RightArrow
                Named::ArrowDown => 0x7D,   // kVK_DownArrow
                Named::ArrowUp => 0x7E,     // kVK_UpArrow
                Named::ContextMenu => 0x6E, // kVK_ContextualMenu

                // Unmapped keys
                _ => -1,
            }
        }*/

        /// Convert an Iced Named key to a Windows Virtual Key Code
        pub fn named_to_virtual_key(named: Named) -> Option<u32> {
            use iced::keyboard::key;
            match named {
                key::Named::Alt => Some(0x12),         // VK_MENU
                key::Named::AltGraph => None,          // No direct equivalent
                key::Named::Backspace => Some(0x08),   // VK_BACK
                key::Named::CapsLock => Some(0x14),    // VK_CAPITAL
                key::Named::Control => Some(0x11),     // VK_CONTROL
                key::Named::Delete => Some(0x2E),      // VK_DELETE
                key::Named::ArrowDown => Some(0x28),   // VK_DOWN
                key::Named::End => Some(0x23),         // VK_END
                key::Named::Escape => Some(0x1B),      // VK_ESCAPE
                key::Named::F1 => Some(0x70),          // VK_F1
                key::Named::F2 => Some(0x71),          // VK_F2
                key::Named::F3 => Some(0x72),          // VK_F3
                key::Named::F4 => Some(0x73),          // VK_F4
                key::Named::F5 => Some(0x74),          // VK_F5
                key::Named::F6 => Some(0x75),          // VK_F6
                key::Named::F7 => Some(0x76),          // VK_F7
                key::Named::F8 => Some(0x77),          // VK_F8
                key::Named::F9 => Some(0x78),          // VK_F9
                key::Named::F10 => Some(0x79),         // VK_F10
                key::Named::F11 => Some(0x7A),         // VK_F11
                key::Named::F12 => Some(0x7B),         // VK_F12
                key::Named::F13 => Some(0x7C),         // VK_F13
                key::Named::F14 => Some(0x7D),         // VK_F14
                key::Named::F15 => Some(0x7E),         // VK_F15
                key::Named::F16 => Some(0x7F),         // VK_F16
                key::Named::F17 => Some(0x80),         // VK_F17
                key::Named::F18 => Some(0x81),         // VK_F18
                key::Named::F19 => Some(0x82),         // VK_F19
                key::Named::F20 => Some(0x83),         // VK_F20
                key::Named::Home => Some(0x24),        // VK_HOME
                key::Named::Insert => Some(0x2D),      // VK_INSERT
                key::Named::ArrowLeft => Some(0x25),   // VK_LEFT
                key::Named::Meta => Some(0x5B),        // VK_LWIN (Left Windows key)
                key::Named::NumLock => Some(0x90),     // VK_NUMLOCK
                key::Named::PageDown => Some(0x22),    // VK_NEXT
                key::Named::PageUp => Some(0x21),      // VK_PRIOR
                key::Named::Pause => Some(0x13),       // VK_PAUSE
                key::Named::PrintScreen => Some(0x2C), // VK_SNAPSHOT
                key::Named::ArrowRight => Some(0x27),  // VK_RIGHT
                key::Named::ScrollLock => Some(0x91),  // VK_SCROLL
                key::Named::Shift => Some(0x10),       // VK_SHIFT
                key::Named::Space => Some(0x20),       // VK_SPACE
                key::Named::Tab => Some(0x09),         // VK_TAB
                key::Named::ArrowUp => Some(0x26),     // VK_UP
                key::Named::Enter => Some(0x0D),       // VK_RETURN
                _ => None,
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
    }

    fn send_mouse_event(&mut self, point: iced::Point, event: iced::mouse::Event) {
        if self.is_closed {
            return;
        }
        let Some(host) = &self.browser_host else {
            return;
        };
        use iced::advanced::mouse::click::Kind;
        match event {
            iced::mouse::Event::ButtonPressed(button) => {
                let previous = self.last_click.take();
                self.last_click.replace(Click::new(point, button, previous));
                // if let Some(node) = self.editable_node_focused {
                //    if !node.contains(point) {
                //        self.editable_node_focused = None;
                //    }
                //}

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
                self.last_button_modifiers = modifier as u32;

                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: modifier as u32,
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
                    modifiers: modifier as u32,
                };
                host.send_mouse_click_event(Some(&event), type_.into(), true as _, 1);
            }
            iced::mouse::Event::WheelScrolled { delta } => {
                let event = cef::MouseEvent {
                    x: point.x as _,
                    y: point.y as _,
                    modifiers: cef::sys::cef_event_flags_t::EVENTFLAG_SCROLL_BY_PAGE as _,
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
}

pub struct Webview<'a, Message> {
    browser_id: BrowserId,
    content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message> Webview<'a, Message> {
    pub fn new(
        browser_id: BrowserId,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Webview {
            browser_id,
            content: content.into(),
        }
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
            last_click: None,
            last_button_modifiers: 0,
            caret_offset: None,
            is_closed: false,
            focused_editable_node: None,
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

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let _state = tree.state.downcast_mut::<CefState>();

        // generate the child layout
        let child_layout = self
            .content
            .as_widget()
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
        state: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = state.state.downcast_mut::<CefState>();
        let bounds = layout.bounds();
        if state.bounds != bounds {
            state.bounds = bounds.into();
            state.resize(bounds);
        }

        match event {
            Event::Keyboard(event) => {
                state.send_key_event(event.clone());
                shell.capture_event();
            }
            Event::Mouse(event) => {
                if let Some(point) = cursor.position_in(bounds) {
                    state.send_mouse_event(point, event.clone());
                    shell.capture_event();
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(_now)) => {
                shell.request_input_method::<String>(&state.input_method(bounds));
                shell.request_redraw();
            }
            Event::InputMethod(event) => {
                state.send_ime_event(event.clone());
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn operate(
        &self,
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

pub fn update_caret_offset<T: Send + 'static>(
    browser_id: BrowserId,
    caret_offset: f32,
) -> iced::Task<T> {
    struct UpdateCaretOffset {
        browser_id: BrowserId,
        caret_offset: f32,
    }

    impl<T> Operation<T> for UpdateCaretOffset {
        fn container(
            &mut self,
            _id: Option<&Id>,
            _bounds: Rectangle,
            operate_on_children: &mut dyn FnMut(&mut dyn Operation<T>),
        ) {
            operate_on_children(self);
        }

        fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn std::any::Any) {
            if let Some(state) = state.downcast_mut::<CefState>() {
                if self.browser_id == state.browser_id {
                    state.set_caret_offset(self.caret_offset);
                }
            }
        }
    }
    widget::operate(UpdateCaretOffset {
        browser_id,
        caret_offset,
    })
}

pub fn update_focused_editable_node<T: Send + 'static>(
    browser_id: BrowserId,
    node: iced::Rectangle,
) -> iced::Task<T> {
    struct UpdateFocusedEditableNode {
        browser_id: BrowserId,
        node: iced::Rectangle,
    }

    impl<T> Operation<T> for UpdateFocusedEditableNode {
        fn container(
            &mut self,
            _id: Option<&Id>,
            _bounds: Rectangle,
            operate_on_children: &mut dyn FnMut(&mut dyn Operation<T>),
        ) {
            operate_on_children(self);
        }
        fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn std::any::Any) {
            if let Some(state) = state.downcast_mut::<CefState>() {
                if self.browser_id == state.browser_id {
                    state.set_focused_editable_node(self.node);
                }
            }
        }
    }
    widget::operate(UpdateFocusedEditableNode { browser_id, node })
}

pub fn close_webview<T: Send + 'static>(browser_id: BrowserId) -> iced::Task<T> {
    struct Close {
        browser_id: BrowserId,
    }

    impl<T> Operation<T> for Close {
        fn container(
            &mut self,
            _id: Option<&Id>,
            _bounds: Rectangle,
            operate_on_children: &mut dyn FnMut(&mut dyn Operation<T>),
        ) {
            operate_on_children(self);
        }
        fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn std::any::Any) {
            if let Some(state) = state.downcast_mut::<CefState>() {
                if self.browser_id == state.browser_id {
                    state.close();
                }
            }
        }
    }
    widget::operate(Close { browser_id })
}
