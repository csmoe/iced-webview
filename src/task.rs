use cef;
use cef::{
    ImplTask, Task, WrapTask, currently_on,
    rc::{Rc, RcImpl},
};
use std::{cell::RefCell, ptr::null_mut, task::Poll, time::Duration};
use tokio::sync::oneshot;

use crate::error::{CefError, Result};

pub struct PostTaskFuture<T = ()> {
    pub rx: oneshot::Receiver<anyhow::Result<T>>,
}

impl<T> Future for PostTaskFuture<T> {
    type Output = anyhow::Result<T>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.rx).poll(cx) {
            Poll::Ready(Ok(r)) => Poll::Ready(r),
            Poll::Ready(Err(_)) => {
                Poll::Ready(Err(anyhow::anyhow!("cannot repost task execution result")))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct IcyTask<F: FnOnce()> {
    object: *mut RcImpl<cef::sys::cef_task_t, Self>,
    func: std::rc::Rc<RefCell<Option<F>>>,
}

impl<F: FnOnce()> IcyTask<F> {
    pub(crate) fn build(task: F) -> Task {
        Task::new(Self {
            object: null_mut(),
            func: std::rc::Rc::new(RefCell::new(Some(task))),
        })
    }
}

impl<F: FnOnce()> Rc for IcyTask<F> {
    fn as_base(&self) -> &cef::sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl<F: FnOnce()> Clone for IcyTask<F> {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            func: self.func.clone(),
        }
    }
}
impl<F: FnOnce()> WrapTask for IcyTask<F> {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef::sys::_cef_task_t, Self>) {
        self.object = object;
    }
}

impl<F: FnOnce()> ImplTask for IcyTask<F> {
    fn get_raw(&self) -> *mut cef::sys::_cef_task_t {
        self.object.cast()
    }

    fn execute(&self) {
        let Some(func) = self.func.take() else {
            return;
        };
        (func)()
    }
}

#[allow(unused)]
pub fn cef_post_task<F: FnOnce()>(thread_id: cef::ThreadId, task: F) -> Result<()> {
    if currently_on(thread_id) == 1 {
        task();
        return Ok(());
    }
    let mut task = IcyTask::build(task);
    let ret = cef::post_task(thread_id, Some(&mut task));
    if ret > 0 {
        Ok(())
    } else {
        Err(CefError::PostTaskFailed((*thread_id.as_ref()) as _))
    }
}

#[allow(unused)]
pub fn cef_post_delayed_task<F: FnOnce()>(
    thread_id: cef::ThreadId,
    delayed_ms: Duration,
    task: F,
) -> Result<()> {
    if currently_on(thread_id) == 1 {
        task();
        return Ok(());
    }
    let mut task = IcyTask::build(task);
    let ret = cef::post_delayed_task(thread_id, Some(&mut task), delayed_ms.as_millis() as _);
    if ret > 0 {
        Ok(())
    } else {
        Err(CefError::PostTaskFailed((*thread_id.as_ref()) as _))
    }
}

#[macro_export]
macro_rules! post_task_async {
    ($id: expr, $delay: expr, $($body: tt)*) => {{
        use futures::future::FutureExt;
        use std::future::IntoFuture;
        let (tx, rx) = tokio::sync::oneshot::channel();
        match crate::webview::task::cef_post_delayed_task($id, move || {
            let ret = (|| -> anyhow::Result<()> {
                $($body)*
            })();
            let _ = tx.send(ret);
        }, $delay) {
            Ok(_) => $crate::task::PostTaskFuture { rx }.into_future().left_future(),
            Err(e) => std::future::ready(Err(anyhow::Error::new(e))).right_future()
        }
    }};

    ($id: expr, $($body: tt)*) => {{
        use futures::future::FutureExt;
        use std::future::IntoFuture;
        let (tx, rx) = tokio::sync::oneshot::channel();
        match crate::webview::task::cef_post_task($id, move || {
            let ret = (|| -> anyhow::Result<()> {
                $($body)*
            })();
            let _ = tx.send(ret);
        }) {
            Ok(_) => $crate::task::PostTaskFuture { rx }.into_future().left_future(),
            Err(e) => std::future::ready(Err(anyhow::Error::new(e))).right_future()
        }
    }};
}

#[macro_export]
macro_rules! post_task_sync {
    ($id: expr, $delay: expr, $($body:tt)*) => {
        $crate::task::cef_post_delayed_task($id, $delay, $($body)*)
    };
    ($id: expr, $($body:tt)*) => {
        $crate::task::cef_post_task($id, $($body)*)
    }
}

#[macro_export]
macro_rules! post_task_async_ui {
    ($delay: expr, $($body:tt)*) => {
        $crate::post_task_async!(cef::sys::cef_thread_id_t::TID_UI.into(), $delay, $($body)*)
    };

    ($($body:tt)*) => {
       $crate::post_task_async!(cef::sys::cef_thread_id_t::TID_UI.into(), $($body)*)
    };
}

#[macro_export]
macro_rules! post_task_async_io {
    ($delay: expr, $($body:tt)*) => {
        $crate::post_task_async!(cef::sys::cef_thread_id_t::TID_IO.into(), $delay, $($body)*)
    };

    ($($body:tt)*) => {
        $crate::post_task_async!(cef::sys::cef_thread_id_t::TID_IO.into(), $($body)*)
    };
}

#[macro_export]
macro_rules! post_task_sync_ui {
    ($delay: expr, $($body:tt)*) => {
        $crate::post_task_sync!(cef::sys::cef_thread_id_t::TID_UI.into(), $delay, $($body)*)
    };

    ($($body:tt)*) => {
       $crate::post_task_sync!(cef::sys::cef_thread_id_t::TID_UI.into(), $($body)*)
    };
}

#[macro_export]
macro_rules! post_task_sync_io {
    ($delay: expr, $($body:tt)*) => {
        $crate::post_task_sync!(cef::sys::cef_thread_id_t::TID_IO.into(), $delay, $($body)*)
    };

    ($($body:tt)*) => {
       $crate::post_task_sync!(cef::sys::cef_thread_id_t::TID_IO.into(), $($body)*)
    };
}
