#![allow(unused)]

use iced::window;
use iced::{Element, Subscription, Task};
use iced_webview::{
    BrowserId, CefAction, CefComponent, CefMessage, IcyCefApp, init_cef, pre_init_cef,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;

fn main() -> iced::Result {
    let _pre = pre_init_cef();
    let (icy_cef_app, _browser_process_rx) = match init_cef() {
        Ok(Some((app, browser_process_rx))) => {
            (RefCell::new(app), RefCell::new(Some(browser_process_rx)))
        }
        Ok(None) => return Ok(()),
        Err(err) => {
            eprintln!("cannot initailize cef: {err:?}");
            std::process::exit(1);
        }
    };
    iced::daemon(Example::new, Example::update, Example::view)
        .subscription(Example::subscription)
        .run()
}

struct Example {
    webviews: BTreeMap<window::Id, CefComponent>,
}

#[derive(Clone)]
enum Message {
    WindowOpened(window::Id),
    NewWindow,
    Cef(window::Id, CefMessage),
    CloseWindow(window::Id),
    PumpLoop(Duration),
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::WindowOpened(window_id) => {
                f.debug_tuple("WindowOpened").field(window_id).finish()
            }
            Message::Cef(window_id, msg) => {
                f.debug_tuple("Cef").field(window_id).field(msg).finish()
            }
            Message::CloseWindow(window_id) => {
                f.debug_tuple("CloseWindow").field(window_id).finish()
            }
            Message::PumpLoop(duration) => f.debug_tuple("PumpLoop").field(duration).finish(),
            _ => f.debug_struct("Extra").finish(),
        }
    }
}

impl Example {
    fn new() -> (Self, Task<Message>) {
        let (_, open) = window::open(window::Settings::default());
        (
            Self {
                webviews: Default::default(),
            },
            open.map(Message::WindowOpened),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PumpLoop(_) => {
                cef::do_message_loop_work();
                Task::none()
            }
            Message::NewWindow => {
                let (_, open) = window::open(window::Settings::default());
                open.map(Message::WindowOpened)
            }
            Message::WindowOpened(id) => {
                let cef = CefComponent::new();
                self.webviews.insert(id, cef);
                self.webviews
                    .get(&id)
                    .unwrap()
                    .get_window_info(id)
                    .map(move |(id, position, size, factor)| {
                        CefMessage::Create(
                            id,
                            "https://github.com".parse().unwrap(),
                            position,
                            size,
                            factor,
                        )
                    })
                    .map(move |msg| Message::Cef(id, msg))
            }
            Message::CloseWindow(_) => Task::none(),
            Message::Cef(id, cef_message) => {
                if let Some(webview) = self.webviews.get_mut(&id) {
                    return match webview.update(cef_message) {
                        CefAction::Created(browser_id) => Task::none(),
                        CefAction::Run(task) => task.map(move |msg| Message::Cef(id, msg)),
                        CefAction::Loaded(browser_id) => Task::none(),
                        CefAction::Closed(browser_id) => {
                            cef::shutdown();
                            iced::exit()
                        }
                        CefAction::None => Task::none(),
                    };
                }
                Task::none()
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if let Some(webview) = self.webviews.get(&id) {
            webview.view().map(move |msg| Message::Cef(id, msg)).into()
        } else {
            iced::widget::space().width(0).into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            window::close_events().map(Message::CloseWindow),
            // tick the cef message loop at 60fps
            iced::time::every(std::time::Duration::from_millis(1000 / 17))
                .map(|delay| Message::PumpLoop(delay.elapsed())),
            iced::event::listen_with(|event, _, _| {
                if matches!(
                    event,
                    iced::event::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
                        ..
                    })
                ) {
                    Some(Message::NewWindow)
                } else {
                    None
                }
            }),
        ])
    }
}
