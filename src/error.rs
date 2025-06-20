use thiserror::Error;

pub type Result<T> = core::result::Result<T, CefError>;

#[derive(Error, Debug)]
pub enum CefError {
    #[error("cannot launch process")]
    ProcessLaunchFailed,
    #[error("cannot init cef code: {0}")]
    CannotInit(i32),
    #[error("null ptr")]
    NullPtr,
    #[error("non utf8 path")]
    NonUtf8Path,
    #[error("{0}")]
    IoError(std::io::Error),
    #[error("tokio mpsc send error")]
    MpscSendError,
    #[error("{0}")]
    RecvError(tokio::sync::oneshot::error::RecvError),
    #[error("cannot post task thread_id={0}")]
    PostTaskFailed(u32),
    #[error("Cef Task Failed: {0}")]
    TaskError(anyhow::Error),
    #[error("custom: {0}")]
    Custom(&'static str),
}
