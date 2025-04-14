use camino::Utf8PathBuf;
use iced::window::settings;

#[derive(Debug, Default)]
pub struct CefSettings {
    cache_path: Utf8PathBuf,
    user_agent: String,
    locale: Option<String>,
    log_severity: Option<String>,
    log_file_path: Utf8PathBuf,
}

impl CefSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cache_path(mut self, path: Utf8PathBuf) -> Self {
        self.cache_path = path;
        self
    }

    pub fn user_agent(mut self, agent: String) -> Self {
        self.user_agent = agent;
        self
    }

    pub(crate) fn into_cef_settings(self) -> cef::Settings {
        let Self {
            cache_path,
            user_agent,
            locale,
            log_file_path,
            ..
        } = self;
        let settings = cef::Settings {
            persist_session_cookies: true as _,
            cache_path: cache_path.as_str().into(),
            user_agent: user_agent.as_str().into(),
            accept_language_list: locale.unwrap_or("zh-CN".into()).as_str().into(),
            log_file: log_file_path.as_str().into(),
            ..Default::default()
        };
        settings
    }
}
