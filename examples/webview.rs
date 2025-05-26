use iced::widget::{container, horizontal_space};
use iced::window;
use iced::{Element, Subscription, Task, Theme};
use iced_webview::{
    BrowserId, BrowserProcessMessage, ClientEventSubscriber, IcyCefApp, IcyClientState,
    LifeSpanEvent, init_cef, launch_browser, pre_init_cef,
};
use std::cell::RefCell;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

use std::collections::BTreeMap;

fn main() -> iced::Result {
    let _pre = pre_init_cef();
    let (icy_cef_app, browser_process_rx) = match init_cef() {
        Ok(Some((app, browser_process_rx))) => (
            RefCell::new(Some(app)),
            RefCell::new(Some(browser_process_rx)),
        ),
        Ok(None) => return Ok(()),
        Err(err) => {
            eprintln!("cannot initailize cef");
            std::process::exit(1);
        }
    };

    iced::daemon(
        move || {
            Example::new(
                icy_cef_app.borrow_mut().take().unwrap(),
                browser_process_rx.borrow_mut().take().unwrap(),
            )
        },
        Example::update,
        Example::view,
    )
    .subscription(Example::subscription)
    .title(Example::title)
    .theme(Example::theme)
    .run()
}

struct Example {
    icy_cef_app: IcyCefApp,
    windows: BTreeMap<window::Id, Window>,
}

#[derive(Debug)]
struct Window {
    title: String,
    scale_input: String,
    theme: Theme,
}

enum Message {
    OpenWindow,
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    TitleChanged(window::Id, String),
    TickCef(Duration),
    Created(IcyClientState, std::sync::Arc<ClientEventSubscriber>),
    Done(BrowserId),
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::OpenWindow => write!(f, "Message::OpenWindow"),
            Message::WindowOpened(id) => f.debug_tuple("Message::WindowOpened").finish(),
            Message::WindowClosed(id) => f.debug_tuple("Message::WindowClosed").finish(),
            Message::TitleChanged(id, title) => {
                f.debug_tuple("Message::TitleChanged").field(title).finish()
            }
            Message::TickCef(duration) => {
                f.debug_tuple("Message::TickCef").field(duration).finish()
            }
            Message::Created(state, subscriber) => write!(f, "Message::Created"),
            Message::Done(browser_id) => f.debug_tuple("Message::Done").field(browser_id).finish(),
        }
    }
}

impl Example {
    fn new(
        icy_cef_app: IcyCefApp,
        browser_process_rx: UnboundedReceiver<BrowserProcessMessage>,
    ) -> (Self, Task<Message>) {
        let (id, open) = window::open(window::Settings::default());
        let mut browser_process_rx = Some(browser_process_rx);
        (
            Self {
                icy_cef_app,
                windows: BTreeMap::new(),
            },
            open.then(move |id| {
                Task::stream(UnboundedReceiverStream::new(
                    browser_process_rx.take().unwrap(),
                ))
                .map(move |msg| match msg {
                    BrowserProcessMessage::Ready => Message::WindowOpened(id),
                    BrowserProcessMessage::Tick(delay) => Message::TickCef(delay),
                })
            }),
        )
    }

    fn title(&self, window: window::Id) -> String {
        self.windows
            .get(&window)
            .map(|window| window.title.clone())
            .unwrap_or_default()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TickCef(_) => {
                cef::do_message_loop_work();
                Task::none()
            }
            Message::OpenWindow => {
                let Some(last_window) = self.windows.keys().last() else {
                    return Task::none();
                };
                Task::none()
            }
            Message::Created(state, subscribers) => Task::stream(
                tokio_stream::wrappers::ReceiverStream::new(
                    Arc::into_inner(subscribers).unwrap().lifespan_rx,
                )
                .map(|event| match event {
                    LifeSpanEvent::Created { browser_id } => Message::Done(browser_id),
                    _ => Message::Done(0.into()),
                }),
            ),

            Message::Done(id) => {
                println!("webview done {id:?}");
                Task::none()
            }
            Message::WindowOpened(id) => window::get_position(id)
                .and_then(move |position| {
                    window::get_scale_factor(id).map(move |factor| (position, factor))
                })
                .then(move |(position, factor)| {
                    window::get_size(id).map(move |size| (position, factor, size))
                })
                .map(move |(position, factor, size)| {
                    let rect = cef::Rect {
                        x: position.x as _,
                        y: position.y as _,
                        width: size.width.floor() as _,
                        height: size.height.floor() as _,
                    };
                    launch_browser(factor, rect, "https://www.testufo.com".parse().unwrap())
                        .unwrap()
                })
                .map(|(state, subscribers)| Message::Created(state, Arc::new(subscribers))),
            Message::WindowClosed(id) => {
                self.windows.remove(&id);

                if self.windows.is_empty() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }

            Message::TitleChanged(id, title) => {
                if let Some(window) = self.windows.get_mut(&id) {
                    window.title = title;
                }

                Task::none()
            }
        }
    }

    fn view(&self, window_id: window::Id) -> Element<Message> {
        if let Some(window) = self.windows.get(&window_id) {
            iced::widget::center(window.view(window_id)).into()
        } else {
            horizontal_space().into()
        }
    }

    fn theme(&self, window: window::Id) -> Theme {
        if let Some(window) = self.windows.get(&window) {
            window.theme.clone()
        } else {
            Theme::default()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            window::close_events().map(Message::WindowClosed),
            iced::time::every(std::time::Duration::from_millis(1000 / 17))
                .map(|delay| Message::TickCef(delay.elapsed())),
        ])
    }
}

impl Window {
    fn new(count: usize) -> Self {
        Self {
            title: format!("Window_{}", count),
            scale_input: "1.0".to_string(),
            theme: Theme::ALL[count % Theme::ALL.len()].clone(),
        }
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        container(horizontal_space()).into()
    }
}
