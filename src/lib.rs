use browser::{AppBuilder, IcyBrowserProcessHandler};
use cef::ImplCommandLine;

mod backend;
mod browser;
mod settings;
mod webview;

pub use backend::BrowserId;
pub use backend::LifeSpanEvent;
pub use webview::WebView;
pub use webview::launch;

pub fn pre_init() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = cef::library_loader::LibraryLoader::new(
            &std::env::current_exe().expect("cannot get current exe"),
        );
        loader.load()?;
        loader
    };
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    Ok(())
}

pub fn init() -> anyhow::Result<()> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = AppBuilder::build(IcyBrowserProcessHandler::new(tx));
    let args = cef::args::Args::new();
    let Some(cmd) = args.as_cmd_line() else {
        anyhow::bail!("cannot get cmd line");
    };
    let switch = cef::CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    let sandbox = cef::sandbox_info::SandboxInfo::new();
    let ret = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        sandbox.as_mut_ptr(),
    );
    if is_browser_process {
        if ret != -1 {
            anyhow::bail!("cannot execute browser process");
        }
    } else {
        if ret < 0 {
            anyhow::bail!("cannot execute child process");
        }
        // non-browser process does not initialize cef
        return Ok(());
    }

    let settings = settings::CefSettings::new();
    let ret = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings.into_cef_settings()),
        Some(&mut app),
        sandbox.as_mut_ptr(),
    );
    if ret != 1 {
        anyhow::bail!("cannot initialize");
    }
    Ok(())
}
