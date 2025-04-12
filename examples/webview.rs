use iced::wgpu::rwh::{RawWindowHandle, Win32WindowHandle};
use iced::widget::{
    button, center, center_x, column, container, horizontal_space, scrollable, text, text_input,
};
use iced::window;
use iced::{Center, Element, Fill, Function, Subscription, Task, Theme, Vector};
use std::sync::Mutex;

use cef::ImplView;
use cef::{CefString, ImplCommandLine, args::Args, sandbox_info::SandboxInfo};
use cef::{ImplBrowser, ImplBrowserHost, sys};

use std::collections::BTreeMap;

mod icy_cef;

fn main() -> iced::Result {
    let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();

    let sandbox = SandboxInfo::new();

    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    let window = std::sync::Arc::new(Mutex::new(None));
    let mut app = icy_cef::DemoApp::new(window.clone());

    let ret = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        sandbox.as_mut_ptr(),
    );

    if is_browser_process {
        println!("launch browser process");
        assert!(ret == -1, "cannot execute browser process");
    } else {
        let process_type = CefString::from(&cmd.get_switch_value(Some(&switch)));
        println!("launch process {process_type}");
        assert!(ret >= 0, "cannot execute non-browser process");
        // non-browser process does not initialize cef
        return Ok(());
    }

    let settings = cef::Settings::default();
    assert_eq!(
        cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            sandbox.as_mut_ptr()
        ),
        1
    );

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
    Created(window::Id),
    Done,
}

impl Example {
    fn new() -> (Self, Task<Message>) {
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
            Message::Created(id) => {
                println!("webview created");
                window::run_with_handle(id, |handle| {
                    let hwnd = handle.as_raw();
                    let hwnd = match hwnd {
                        RawWindowHandle::Win32(Win32WindowHandle { hwnd, .. }) => hwnd,
                        _ => panic!("unsupported window handle"),
                    };
                    let mut client = icy_cef::DemoClient::new();
                    let url = CefString::from("https://www.bing.com");

                    let window_info = cef::WindowInfo {
                        parent_window: cef::sys::HWND(hwnd.get() as _),
                        bounds: cef::Rect {
                            x: 500,
                            y: 300,
                            width: 800,
                            height: 600,
                        },
                        ..Default::default()
                    };
                    let mut browser = cef::browser_host_create_browser_sync(
                        Some(&window_info),
                        Some(&mut client),
                        Some(&url),
                        Some(&Default::default()),
                        Option::<&mut cef::DictionaryValue>::None,
                        Option::<&mut cef::RequestContext>::None,
                    )
                    .unwrap();
                    dbg!(browser.get_host().unwrap().has_view());
                    let view = cef::browser_view_get_for_browser(Some(&mut browser)).unwrap();
                    dbg!(view.get_preferred_size().height);
                    view.set_visible(true as _);
                    Message::Done
                })
            }
            Message::Done => {
                println!("webview done");
                Task::none()
            }
            Message::WindowOpened(id) => window::run_with_handle(id, move |handle| {
                let hwnd = handle.as_raw();
                let hwnd = match hwnd {
                    RawWindowHandle::Win32(Win32WindowHandle { hwnd, .. }) => hwnd,
                    _ => panic!("unsupported window handle"),
                };
                let mut client = icy_cef::DemoClient::new();
                let url = CefString::from("https://www.testufo.com");

                let window_info = cef::WindowInfo {
                    parent_window: cef::sys::HWND(hwnd.get() as _),
                    bounds: cef::Rect {
                        x: 500,
                        y: 300,
                        width: 800,
                        height: 600,
                    },
                    ..Default::default()
                };
                let mut browser = cef::browser_host_create_browser_sync(
                    Some(&window_info),
                    Some(&mut client),
                    Some(&url),
                    Some(&Default::default()),
                    Option::<&mut cef::DictionaryValue>::None,
                    Option::<&mut cef::RequestContext>::None,
                )
                .unwrap();
                dbg!(browser.get_host().unwrap().has_view());
                let view = cef::browser_view_get_for_browser(Some(&mut browser)).unwrap();
                view.set_visible(true as _);
                Message::Created(id)
            }),
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
