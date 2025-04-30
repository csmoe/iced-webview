#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[cfg(target_os = "macos")]
    #[error("cannot load chromium framework")]
    CannotLoadCefFrameWork,
    #[error("cannot load cef")]
    CannotLaunchProcess,
    #[error("cannot init cef")]
    CannotInitCef,
    #[error("cannot create browser")]
    CannotCreateBrowser,
    #[error("cef api error({0})")]
    Custom(String),
}

pub type Result<T> = core::result::Result<T, Error>;
