#![allow(unused)]

use iced::widget::horizontal_space;
use iced::window;
use iced::{Element, Subscription, Task};
use iced_webview::{
    BrowserId, CefAction, CefComponent, CefMessage, IcyCefApp, close_webview, init_cef,
    pre_init_cef,
};
use std::cell::RefCell;
use std::time::Duration;

fn main() -> iced::Result {
    let _pre = pre_init_cef();
    let (icy_cef_app, _browser_process_rx) = match init_cef() {
        Ok(Some((app, browser_process_rx))) => (
            RefCell::new(Some(app)),
            RefCell::new(Some(browser_process_rx)),
        ),
        Ok(None) => return Ok(()),
        Err(err) => {
            eprintln!("cannot initailize cef: {err:?}");
            std::process::exit(1);
        }
    };

    iced::daemon(
        move || Example::new(icy_cef_app.borrow_mut().take().unwrap()),
        Example::update,
        Example::view,
    )
    .subscription(Example::subscription)
    .run()
}

struct Example {
    icy_cef_app: IcyCefApp,
    cef: CefComponent,
    current_browser_id: Option<BrowserId>,
}

enum Message {
    WindowOpened(window::Id),
    Cef(CefMessage),
    CloseWindow(window::Id),
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::WindowOpened(window_id) => {
                f.debug_tuple("WindowOpened").field(window_id).finish()
            }
            Message::Cef(msg) => f.debug_tuple("Cef").field(msg).finish(),
            Message::CloseWindow(window_id) => {
                f.debug_tuple("CloseWindow").field(window_id).finish()
            }
        }
    }
}

impl Example {
    fn new(icy_cef_app: IcyCefApp) -> (Self, Task<Message>) {
        let (_, open) = window::open(window::Settings::default());
        (
            Self {
                icy_cef_app,
                current_browser_id: None,
                cef: CefComponent {},
            },
            open.map(Message::WindowOpened),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => window::get_position(id)
                .and_then(move |position| {
                    window::get_scale_factor(id).map(move |factor| (position, factor))
                })
                .then(move |(position, factor)| {
                    window::get_size(id).map(move |size| (position, factor, size))
                })
                .map(move |(position, factor, size)| {
                    CefMessage::Create(
                        id,
                        "https://testufo.com".parse().unwrap(),
                        position,
                        size,
                        factor,
                    )
                })
                .map(Message::Cef),
            Message::CloseWindow(_) => {
                if let Some(browser_id) = self.current_browser_id.take() {
                    close_webview(browser_id).map(Message::Cef)
                } else {
                    Task::none()
                }
            }
            Message::Cef(cef_message) => match self.cef.update(cef_message) {
                CefAction::Created(browser_id) => {
                    self.current_browser_id = Some(browser_id);
                    Task::none()
                }
                CefAction::Run(task) => task.map(Message::Cef),
                CefAction::Loaded(browser_id) => Task::none(),
                CefAction::UpdateView(browser_id) => Task::none(),
                CefAction::Closed(browser_id) => {
                    cef::shutdown();
                    iced::exit()
                }
                CefAction::None => Task::none(),
            },
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if let Some(browser_id) = self.current_browser_id {
            self.cef.view(browser_id).map(Message::Cef).into()
        } else {
            horizontal_space().width(0).into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            window::close_events().map(Message::CloseWindow),
            // tick the cef message loop at 60fps
            iced::time::every(std::time::Duration::from_millis(1000 / 17))
                .map(|delay| Message::Cef(CefMessage::PumpLoop(delay.elapsed()))),
        ])
    }
}
