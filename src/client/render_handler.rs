use crate::BrowserId;
use cef;
use cef::{rc::*, sys, *};
use std::{
    cell::{Ref, RefCell},
    fmt::Debug,
    ptr::null_mut,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone)]
pub struct IcyRenderHandler {
    state: IcyRenderState,
    tx: UnboundedSender<BrowserId>,
}

impl IcyRenderHandler {
    pub fn new(
        device_scale_factor: f32,
        view_rect: cef::Rect,
    ) -> (Self, IcyRenderState, UnboundedReceiver<BrowserId>) {
        let device_scale_factor = std::rc::Rc::new(RefCell::new(device_scale_factor));
        let view_rect = std::rc::Rc::new(RefCell::new(view_rect));
        let size = std::rc::Rc::new(RefCell::new((0, 0)));
        let (tx, rx) = unbounded_channel();
        let state = IcyRenderState {
            pixels: std::rc::Rc::new(RefCell::new(Vec::with_capacity(1024 * 1024))),
            device_scale_factor,
            view_rect,
            size,
        };
        (
            Self {
                state: state.clone(),
                tx,
            },
            state,
            rx,
        )
    }
}

pub struct RenderHandlerBuilder {
    object: *mut RcImpl<sys::cef_render_handler_t, Self>,
    handler: IcyRenderHandler,
}

#[derive(Clone)]
pub struct IcyRenderState {
    pub(crate) pixels: std::rc::Rc<RefCell<Vec<u8>>>,
    pub(crate) device_scale_factor: std::rc::Rc<RefCell<f32>>,
    pub(crate) view_rect: std::rc::Rc<RefCell<cef::Rect>>,
    pub(crate) size: std::rc::Rc<RefCell<(i32, i32)>>,
}

impl Debug for IcyRenderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderState")
            .field("pixels", &self.pixels.borrow().len())
            .field("scale_factor", &self.device_scale_factor())
            .finish()
    }
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

    pub fn pixels(&self) -> Ref<'_, Vec<u8>> {
        self.pixels.borrow()
    }
    pub fn size(&self) -> (i32, i32) {
        self.size.borrow().clone()
    }
}

impl RenderHandlerBuilder {
    pub fn build(handler: IcyRenderHandler) -> RenderHandler {
        RenderHandler::new(Self {
            object: null_mut(),
            handler,
        })
    }
}

impl Rc for RenderHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}
impl WrapRenderHandler for RenderHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_handler_t, Self>) {
        self.object = object;
    }
}
impl Clone for RenderHandlerBuilder {
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

impl ImplRenderHandler for RenderHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_render_handler_t {
        self.object.cast()
    }

    fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
        if let Some(rect) = rect {
            let view_rect = self.handler.state.view_rect();
            if view_rect.height > 0 && view_rect.width > 0 {
                *rect = view_rect;
            }
        }
    }

    fn screen_info(
        &self,
        _browser: Option<&mut Browser>,
        screen_info: Option<&mut ScreenInfo>,
    ) -> ::std::os::raw::c_int {
        if let Some(screen_info) = screen_info {
            screen_info.device_scale_factor = *self.handler.state.device_scale_factor.borrow();
            return true as _;
        }
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
        dirty_rects_count: usize,
        dirty_rects: Option<&Rect>,
        buffer: *const u8,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let browser_id: BrowserId = browser.identifier().into();
        if type_ != cef::sys::cef_paint_element_type_t::PET_VIEW.into() {
            return;
        }

        let mut size = self.handler.state.size.borrow_mut();
        let size_changed = (width as usize, height as usize) != (size.0 as _, size.1 as _);
        *size = (width as _, height as _);

        let source_buffer =
            unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) };

        let mut pixels = self.handler.state.pixels.borrow_mut();
        if pixels.is_empty() || size_changed {
            pixels.clear();
            pixels.extend_from_slice(source_buffer);

            for chunk in pixels.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }
        } else {
            if dirty_rects_count > 0 {
                if let Some(rects) = dirty_rects {
                    let dirty_rects = unsafe {
                        std::slice::from_raw_parts(std::ptr::addr_of!(rects), dirty_rects_count)
                    };

                    for rect in dirty_rects {
                        let x = rect.x.max(0).min(width - 1) as usize;
                        let y = rect.y.max(0).min(height - 1) as usize;
                        let rect_width = rect.width.min(width - rect.x) as usize;
                        let rect_height = rect.height.min(height - rect.y) as usize;

                        for row in 0..rect_height {
                            let y_offset = (y + row) * size.0 as usize;
                            for col in 0..rect_width {
                                let idx = (y_offset + x + col) * 4;
                                pixels[idx] = source_buffer[idx + 2];
                                pixels[idx + 1] = source_buffer[idx + 1];
                                pixels[idx + 2] = source_buffer[idx];
                                pixels[idx + 3] = source_buffer[idx + 3];
                            }
                        }
                    }
                }
            }
        }

        pixels.shrink_to_fit();

        _ = self.handler.tx.send(browser_id);
    }
}
