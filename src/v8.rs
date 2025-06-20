use cef;
use cef::{
    ImplV8Handler, V8Handler, V8Value, WrapV8Handler,
    rc::{Rc, RcImpl},
};
use std::panic::AssertUnwindSafe;

#[derive(Clone)]
struct IcyV8Handler<
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value> + Clone,
> {
    inner: F,
}

impl<F> IcyV8Handler<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value> + Clone,
{
    fn new(function: F) -> Self {
        Self { inner: function }
    }

    fn execute(
        self,
        name: &str,
        this: &mut V8Value,
        arguments: &[Option<V8Value>],
    ) -> anyhow::Result<V8Value> {
        (self.inner)(name, this, arguments)
    }
}

pub struct IcyV8HandlerBuilder<F: Clone>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    object: *mut RcImpl<cef::sys::cef_v8_handler_t, Self>,
    handler: IcyV8Handler<F>,
}

impl<F: Clone> IcyV8HandlerBuilder<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    pub fn build(function: F) -> V8Handler {
        V8Handler::new(Self {
            object: std::ptr::null_mut(),
            handler: IcyV8Handler::new(function),
        })
    }
}

impl<F: Clone> Rc for IcyV8HandlerBuilder<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    fn as_base(&self) -> &cef::sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl<F: Clone> Clone for IcyV8HandlerBuilder<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            handler: self.handler.clone(),
        }
    }
}

impl<F: Clone> WrapV8Handler for IcyV8HandlerBuilder<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    fn wrap_rc(&mut self, object: *mut RcImpl<cef::sys::_cef_v8_handler_t, Self>) {
        self.object = object;
    }
}

impl<F: Clone> ImplV8Handler for IcyV8HandlerBuilder<F>
where
    F: FnOnce(&str, &mut V8Value, &[Option<V8Value>]) -> anyhow::Result<V8Value>,
{
    fn get_raw(&self) -> *mut cef::sys::_cef_v8_handler_t {
        self.object.cast()
    }

    fn execute(
        &self,
        name: Option<&cef::CefString>,
        object: Option<&mut V8Value>,
        arguments: Option<&[Option<V8Value>]>,
        retval: Option<&mut Option<V8Value>>,
        exception: Option<&mut cef::CefString>,
    ) -> ::std::os::raw::c_int {
        let Some(name) = name else {
            return false as _;
        };
        let Some(this) = object else {
            return false as _;
        };

        let args = arguments.unwrap_or(&[]);
        match std::panic::catch_unwind(AssertUnwindSafe(|| {
            self.handler.clone().execute(&name.to_string(), this, args)
        })) {
            Ok(Ok(r)) => {
                retval.map(|v| v.replace(r));
            }
            Ok(Err(err)) => {
                exception.map(|e| *e = format!("{err:?}").as_str().into());
            }
            Err(err) => {
                exception.map(|e| *e = format!("Rust panic: {err:?}").as_str().into());
            }
        }

        return true as _;
    }
}
