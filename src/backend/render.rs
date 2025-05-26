use std::cell::RefCell;

use cef::rc::*;
use cef::sys;
use cef::*;

pub enum RenderEvent {
    Paint {
        browser_id: BrowserId,
        pixels: Vec<u8>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash, Ord)]
pub struct BrowserId(i32);

impl From<i32> for BrowserId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

pub struct IcyRenderHandler {
    object: *mut RcImpl<sys::cef_render_handler_t, Self>,
    pixels: RefCell<Vec<u8>>,
    device_scale_factor: RefCell<f32>,
    view_rect: RefCell<cef::Rect>,
}

#[derive(Clone)]
pub(crate) struct IcyRenderState {
    pub(crate) pixels: RefCell<Vec<u8>>,
    pub(crate) device_scale_factor: RefCell<f32>,
    pub(crate) view_rect: RefCell<cef::Rect>,
}

impl IcyRenderState {
    pub fn device_scale_factor(&self) -> f32 {
        *self.device_scale_factor.borrow()
    }

    pub fn view_rect(&self) -> cef::Rect {
        self.view_rect.borrow().clone()
    }

    pub fn set_device_scale_factor(&self, device_scale_factor: f32) {
        *self.device_scale_factor.borrow_mut() = device_scale_factor;
    }

    pub fn set_view_rect(&self, view_rect: cef::Rect) {
        *self.view_rect.borrow_mut() = view_rect;
    }
}

impl IcyRenderHandler {
    pub fn new(device_scale_factor: f32, view_rect: cef::Rect) -> (Self, IcyRenderState) {
        let device_scale_factor = RefCell::new(device_scale_factor);
        let pixels = RefCell::new(Vec::with_capacity(1024 * 1024));
        let view_rect = RefCell::new(view_rect);
        let handler = IcyRenderHandler {
            object: std::ptr::null_mut(),
            pixels: pixels.clone(),
            device_scale_factor: device_scale_factor.clone(),
            view_rect: view_rect.clone(),
        };
        (
            handler,
            IcyRenderState {
                pixels,
                device_scale_factor,
                view_rect,
            },
        )
    }

    pub fn into_cef_handler(self) -> RenderHandler {
        RenderHandler::new(self)
    }
}

impl Rc for IcyRenderHandler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}
impl WrapRenderHandler for IcyRenderHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_handler_t, Self>) {
        self.object = object;
    }
}
impl Clone for IcyRenderHandler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            pixels: self.pixels.clone(),
            device_scale_factor: self.device_scale_factor.clone(),
            view_rect: self.view_rect.clone(),
        }
    }
}

impl ImplRenderHandler for IcyRenderHandler {
    fn get_raw(&self) -> *mut sys::_cef_render_handler_t {
        self.object.cast()
    }

    fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
        if let Some(_rect) = rect {}
    }

    fn screen_info(
        &self,
        _browser: Option<&mut Browser>,
        screen_info: Option<&mut ScreenInfo>,
    ) -> ::std::os::raw::c_int {
        if let Some(_screen_info) = screen_info {}
        return false as _;
    }

    fn screen_point(
        &self,
        _browser: Option<&mut Browser>,
        _view_x: ::std::os::raw::c_int,
        _view_y: ::std::os::raw::c_int,
        _screen_x: Option<&mut ::std::os::raw::c_int>,
        _screen_y: Option<&mut ::std::os::raw::c_int>,
    ) -> ::std::os::raw::c_int {
        return false as _;
    }

    fn on_paint(
        &self,
        browser: Option<&mut Browser>,
        type_: PaintElementType,
        _dirty_rects_count: usize,
        _dirty_rects: Option<&Rect>,
        buffer: *const u8,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let _browser_id: BrowserId = browser.identifier().into();
        if type_ != cef::sys::cef_paint_element_type_t::PET_VIEW.into() {
            return;
        }
        let pixels =
            unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) };
        self.pixels.borrow_mut().clear();
        self.pixels.borrow_mut().extend_from_slice(pixels);
    }
}
