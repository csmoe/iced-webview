use iced::widget::{
    button, center, center_x, column, container, horizontal_space, scrollable, text, text_input,
};
use iced::window;
use iced::{Center, Element, Fill, Function, Subscription, Task, Theme, Vector};
use iced_webview::{BrowserId, LifeSpanEvent, launch, pre_init};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

use std::collections::BTreeMap;

mod icy_cef;

fn main() -> iced::Result {
    pre_init().unwrap();

    iced::daemon(Example::new, Example::update, Example::view)
        .subscription(Example::subscription)
        .title(Example::title)
        .theme(Example::theme)
        .scale_factor(Example::scale_factor)
        .run()
}

struct Example {
    windows: BTreeMap<window::Id, Window>,
}

#[derive(Debug)]
struct Window {
    title: String,
    scale_input: String,
    current_scale: f64,
    theme: Theme,
}

#[derive(Debug, Clone)]
enum Message {
    OpenWindow,
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    ScaleInputChanged(window::Id, String),
    ScaleChanged(window::Id, String),
    TitleChanged(window::Id, String),
    TickCef,
    Created(Arc<Receiver<LifeSpanEvent>>),
    Done(BrowserId),
}

impl Example {
    fn new() -> (Self, Task<Message>) {
        iced_webview::init().unwrap();
        let (_id, open) = window::open(window::Settings::default());
        (
            Self {
                windows: BTreeMap::new(),
            },
            open.map(Message::WindowOpened),
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
            Message::TickCef => {
                cef::do_message_loop_work();
                Task::none()
            }
            Message::OpenWindow => {
                let Some(last_window) = self.windows.keys().last() else {
                    return Task::none();
                };
                Task::none()
            }
            Message::Created(rx) => {
                print!("created");
                Task::none()
                //Task::perform(async move { rx.recv().await }, |id| id).map(|event| match event {
                //    Some(LifeSpanEvent::Created { browser_id }) => Message::Done(browser_id),
                //    _ => Message::Done(0.into()),
                //})
            }
            Message::Done(id) => {
                println!("webview done");
                Task::none()
            }
            Message::WindowOpened(id) => window::get_position(id)
                .then(move |position| {
                    window::get_size(id).then(move |size| {
                        window::run_with_handle(id, move |handle| {
                            Arc::new(
                                launch(
                                    handle.as_raw(),
                                    (position.unwrap(), size),
                                    "https://www.testufo.com".parse().unwrap(),
                                )
                                .unwrap(),
                            )
                        })
                    })
                })
                .map(Message::Created),
            Message::WindowClosed(id) => {
                self.windows.remove(&id);

                if self.windows.is_empty() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Message::ScaleInputChanged(id, scale) => {
                if let Some(window) = self.windows.get_mut(&id) {
                    window.scale_input = scale;
                }

                Task::none()
            }
            Message::ScaleChanged(id, scale) => {
                if let Some(window) = self.windows.get_mut(&id) {
                    window.current_scale = scale
                        .parse::<f64>()
                        .unwrap_or(window.current_scale)
                        .clamp(0.5, 5.0);
                }

                Task::none()
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
            center(window.view(window_id)).into()
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

    fn scale_factor(&self, window: window::Id) -> f64 {
        self.windows
            .get(&window)
            .map(|window| window.current_scale)
            .unwrap_or(1.0)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            window::close_events().map(Message::WindowClosed),
            iced::time::every(std::time::Duration::from_millis(30)).map(|_| Message::TickCef),
        ])
    }
}

impl Window {
    fn new(count: usize) -> Self {
        Self {
            title: format!("Window_{}", count),
            scale_input: "1.0".to_string(),
            current_scale: 1.0,
            theme: Theme::ALL[count % Theme::ALL.len()].clone(),
        }
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        let scale_input = column![
            text("Window scale factor:"),
            text_input("Window Scale", &self.scale_input)
                .on_input(Message::ScaleInputChanged.with(id))
                .on_submit(Message::ScaleChanged(id, self.scale_input.to_string()))
        ];

        let title_input = column![
            text("Window title:"),
            text_input("Window Title", &self.title)
                .on_input(Message::TitleChanged.with(id))
                .id(format!("input-{id}"))
        ];

        let new_window_button = button(text("New Window")).on_press(Message::OpenWindow);

        let content = column![scale_input, title_input, new_window_button]
            .spacing(50)
            .width(Fill)
            .align_x(Center)
            .width(200);

        container(scrollable(center_x(content))).padding(10).into()
    }
}
