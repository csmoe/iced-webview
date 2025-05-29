use browser::AppBuilder;
use browser::IcyBrowserProcessHandler;
use cef::ImplCommandLine;
use error::Error;
use tokio::sync::mpsc::UnboundedReceiver;

mod backend;
mod browser;
mod error;
mod settings;
mod webview;

use crate::error::Result;

pub use backend::ClientEventSubscriber;
pub use backend::IcyClientState;
pub use backend::LifeSpanEvent;
pub use browser::BrowserProcessMessage;
pub use browser::IcyCefApp;
pub use webview::Webview;
pub use webview::launch_browser;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash, Ord)]
pub struct BrowserId(i32);

impl BrowserId {
    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

impl From<i32> for BrowserId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

#[cfg(target_os = "macos")]
pub fn pre_init_cef() -> Result<cef::library_loader::LibraryLoader> {
    let loader = cef::library_loader::LibraryLoader::new(
        &std::env::current_exe().expect("cannot get current exe"),
        std::env::args().any(|f| f.starts_with("--type=")),
    );
    if !loader.load() {
        std::panic!("cannot load cef library");
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    Ok(loader)
}

#[cfg(target_os = "windows")]
pub fn pre_init_cef() -> Result<()> {
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    Ok(())
}

pub fn init_cef() -> Result<Option<(IcyCefApp, UnboundedReceiver<BrowserProcessMessage>)>> {
    let args = cef::args::Args::new();
    let Some(cmd) = args.as_cmd_line() else {
        return Err(Error::Custom("cannot get cmd line".into()));
    };
    let (browser_handler, rx) = IcyBrowserProcessHandler::new();
    let app = IcyCefApp::new();
    let mut cef_app = AppBuilder::build(app.clone(), browser_handler);
    let switch = cef::CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    let sandbox = cef::sandbox_info::SandboxInfo::new();
    let ret = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut cef_app),
        sandbox.as_mut_ptr(),
    );
    if is_browser_process {
        if ret != -1 {
            return Err(Error::CannotLaunchProcess);
        }
    } else {
        if ret < 0 {
            return Err(Error::CannotLaunchProcess);
        }
        // non-browser process does not initialize cef
        return Ok(None);
    }

    let settings = settings::CefSettings::new();
    let ret = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings.into_cef_settings()),
        Some(&mut cef_app),
        sandbox.as_mut_ptr(),
    );
    if ret != 1 {
        return Err(Error::CannotInitCef);
    }
    Ok(Some((app, rx)))
}
