use crate::{
    BrowserId,
    instance::{DragData, resize, webview_states},
};
use arc_swap::ArcSwapOption;
use cef::*;
use iced;
use std::{
    cell::{OnceCell, Ref, RefCell},
    fmt::Debug,
    ops::Deref,
    sync::Arc,
};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender, unbounded_channel};
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ClientToScreen,
};

struct CachedGpuResources {
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
}

struct CachedPaintTexture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

#[derive(Clone)]
pub struct IcyRenderHandler {
    state: IcyRenderState,
    cef_frame_tx: UnboundedSender<BrowserId>,
    drag_tx: Sender<(BrowserId, DragData)>,
    ime_composition_tx: Sender<(BrowserId, f32, f32)>,
}

#[derive(Clone)]
enum CefViewContent {
    Hardware {
        bind_group: wgpu::BindGroup,
        dirty_rect: Option<iced::Rectangle>,
        capture_counter: u64,
        swap_rb: bool,
        is_popup: bool,
    },
    Software {
        handle: iced::widget::image::Handle,
        allocation: Option<anyhow::Result<iced::widget::image::Allocation, ()>>,
    },
}

#[derive(Clone, Debug)]
pub struct CefViewWrapper(pub Arc<CefView>);

impl Deref for CefViewWrapper {
    type Target = CefView;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct CefView {
    pub(crate) browser_id: BrowserId,
    pub(crate) bounds: Option<iced::Rectangle>,
    pub(crate) popup_offset: Option<iced::Vector>,
    pub(crate) timestamp: std::time::Instant,
    pub(crate) should_resize: bool,
    content: CefViewContent,
}

impl Debug for CefView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CefView")
            .field("browser_id", &self.browser_id)
            .field("mode", &self.mode())
            .field("capture_counter", &self.capture_counter())
            .finish()
    }
}

impl CefView {
    pub fn new_hardware(
        browser_id: BrowserId,
        bind_group: wgpu::BindGroup,
        dirty_rect: Option<iced::Rectangle>,
        bounds: Option<iced::Rectangle>,
        popup_offset: Option<iced::Vector>,
        should_resize: bool,
        capture_counter: u64,
        swap_rb: bool,
        is_popup: bool,
    ) -> Self {
        Self {
            browser_id,
            bounds,
            popup_offset,
            timestamp: std::time::Instant::now(),
            should_resize,
            content: CefViewContent::Hardware {
                bind_group,
                dirty_rect,
                capture_counter,
                swap_rb,
                is_popup,
            },
        }
    }

    pub fn new_software(browser_id: BrowserId, handle: iced::widget::image::Handle) -> Self {
        Self {
            browser_id,
            bounds: None,
            popup_offset: None,
            timestamp: std::time::Instant::now(),
            should_resize: false,
            content: CefViewContent::Software {
                handle,
                allocation: None,
            },
        }
    }

    pub fn software_handle(&self) -> Option<iced::widget::image::Handle> {
        match &self.content {
            CefViewContent::Software { handle, .. } => Some(handle.clone()),
            CefViewContent::Hardware { .. } => None,
        }
    }

    pub fn software_allocation(
        &self,
    ) -> Option<anyhow::Result<iced::widget::image::Allocation, ()>> {
        match &self.content {
            CefViewContent::Software { allocation, .. } => allocation.clone(),
            CefViewContent::Hardware { .. } => None,
        }
    }

    pub fn with_software_allocation(
        &self,
        allocation: anyhow::Result<iced::widget::image::Allocation, ()>,
    ) -> Option<Self> {
        match &self.content {
            CefViewContent::Software { handle, .. } => Some(Self {
                browser_id: self.browser_id,
                bounds: self.bounds,
                popup_offset: self.popup_offset,
                timestamp: self.timestamp,
                should_resize: self.should_resize,
                content: CefViewContent::Software {
                    handle: handle.clone(),
                    allocation: Some(allocation),
                },
            }),
            CefViewContent::Hardware { .. } => None,
        }
    }

    fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        match &self.content {
            CefViewContent::Hardware { bind_group, .. } => Some(bind_group),
            CefViewContent::Software { .. } => None,
        }
    }

    fn dirty_rect(&self) -> Option<iced::Rectangle> {
        match &self.content {
            CefViewContent::Hardware { dirty_rect, .. } => *dirty_rect,
            CefViewContent::Software { .. } => None,
        }
    }

    fn capture_counter(&self) -> Option<u64> {
        match &self.content {
            CefViewContent::Hardware {
                capture_counter, ..
            } => Some(*capture_counter),
            CefViewContent::Software { .. } => None,
        }
    }

    fn mode(&self) -> &'static str {
        match &self.content {
            CefViewContent::Hardware { .. } => "hardware",
            CefViewContent::Software { .. } => "software",
        }
    }

    fn swap_rb(&self) -> bool {
        match &self.content {
            CefViewContent::Hardware { swap_rb, .. } => *swap_rb,
            CefViewContent::Software { .. } => false,
        }
    }

    fn is_popup(&self) -> bool {
        match &self.content {
            CefViewContent::Hardware { is_popup, .. } => *is_popup,
            CefViewContent::Software { .. } => false,
        }
    }
}

fn convert_bgra_to_rgba(src: &[u8], dst: &mut Vec<u8>) {
    debug_assert_eq!(src.len() % 4, 0);
    dst.resize(src.len(), 0);
    let pixels = src.len() / 4;
    let src_ptr = src.as_ptr();
    let dst_ptr = dst.as_mut_ptr();
    for index in 0..pixels {
        unsafe {
            let bgra = u32::from_le(std::ptr::read_unaligned(
                src_ptr.add(index * 4).cast::<u32>(),
            ));
            let rgba =
                (bgra & 0xFF00_FF00) | ((bgra & 0x00FF_0000) >> 16) | ((bgra & 0x0000_00FF) << 16);
            std::ptr::write_unaligned(dst_ptr.add(index * 4).cast::<u32>(), rgba.to_le());
        }
    }
}

impl IcyRenderHandler {
    pub fn new(
        device_scale_factor: f32,
        view_rect: cef::Rect,
        hw_view: Arc<ArcSwapOption<CefView>>,
        popup_view: Arc<ArcSwapOption<CefView>>,
        native_window: Option<isize>,
    ) -> (
        Self,
        IcyRenderState,
        UnboundedReceiver<BrowserId>,
        Receiver<(BrowserId, DragData)>,
        Receiver<(BrowserId, f32, f32)>,
    ) {
        let (cef_frame_tx, cef_frame_rx) = unbounded_channel();
        let (drag_tx, drag_rx) = tokio::sync::mpsc::channel(4);
        let (ime_composition_tx, ime_composition_rx) = tokio::sync::mpsc::channel(10);
        let state = IcyRenderState {
            inner: std::rc::Rc::new(RefCell::new(RenderStateInner {
                pixels: Vec::new(),
                view_rect,

                view_origin: iced::Point::ORIGIN,
                native_window,
                composition_position: (0.0, 0.0),
                cached_paint_texture: None,
                popup_rect: None,
                popup_original_rect: None,
            })),
            hw_view,
            popup_view,
            cached_gpu: std::rc::Rc::new(OnceCell::new()),
        };
        (
            Self {
                state: state.clone(),
                cef_frame_tx,
                drag_tx,
                ime_composition_tx,
            },
            state,
            cef_frame_rx,
            drag_rx,
            ime_composition_rx,
        )
    }
}

struct RenderStateInner {
    pixels: Vec<u8>,
    view_rect: cef::Rect,
    view_origin: iced::Point,
    native_window: Option<isize>,
    composition_position: (f32, f32),
    cached_paint_texture: Option<CachedPaintTexture>,
    popup_rect: Option<iced::Rectangle>,
    popup_original_rect: Option<iced::Rectangle>,
}

#[derive(Clone)]
pub struct IcyRenderState {
    inner: std::rc::Rc<RefCell<RenderStateInner>>,
    pub(crate) hw_view: Arc<ArcSwapOption<CefView>>,
    pub(crate) popup_view: Arc<ArcSwapOption<CefView>>,
    cached_gpu: std::rc::Rc<OnceCell<CachedGpuResources>>,
}

impl Debug for IcyRenderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.borrow();
        f.debug_struct("RenderState")
            .field("pixels", &inner.pixels.len())
            .finish()
    }
}

impl IcyRenderState {
    pub fn view_rect(&self) -> cef::Rect {
        cef::Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        }
        //self.inner.borrow().view_rect
    }

    pub fn device_scale_factor(&self) -> f32 {
        /*
        self.inner
            .borrow()
            .screen
            .as_ref()
            .map(|screen| screen.device_scale_factor as f32)
            .unwrap_or(1.0)
            */
        1.5
    }

    pub fn view_origin(&self) -> iced::Point {
        self.inner.borrow().view_origin
    }

    pub fn native_window(&self) -> Option<isize> {
        self.inner.borrow().native_window
    }

    pub fn popup_rect(&self) -> Option<iced::Rectangle> {
        self.inner.borrow().popup_rect
    }

    pub fn popup_offset(&self) -> Option<iced::Vector> {
        let inner = self.inner.borrow();
        match (inner.popup_rect, inner.popup_original_rect) {
            (Some(rect), Some(original_rect)) => Some(iced::Vector::new(
                original_rect.x - rect.x,
                original_rect.y - rect.y,
            )),
            _ => None,
        }
    }

    pub fn set_device_scale_factor(&self, device_scale_factor: f32) {
        //if let Some(screen) = self.inner.borrow_mut().screen.as_mut() {
        //    screen.device_scale_factor = device_scale_factor as _;
        //}
    }

    pub fn set_view_rect(&self, view_rect: cef::Rect) {
        if view_rect.width > 0 && view_rect.height > 0 {
            self.inner.borrow_mut().view_rect = view_rect;
        }
    }

    pub fn set_view_origin(&self, view_origin: iced::Point) {
        self.inner.borrow_mut().view_origin = view_origin;
    }

    pub fn set_popup_rect(&self, popup_rect: Option<iced::Rectangle>) {
        let mut inner = self.inner.borrow_mut();
        inner.popup_original_rect = popup_rect;
        inner.popup_rect = popup_rect.map(|mut popup_rect| {
            let view_width = inner.view_rect.width.max(0) as f32;
            let view_height = inner.view_rect.height.max(0) as f32;
            if popup_rect.x < 0.0 {
                popup_rect.x = 0.0;
            }
            if popup_rect.y < 0.0 {
                popup_rect.y = 0.0;
            }
            if popup_rect.x + popup_rect.width > view_width {
                popup_rect.x = view_width - popup_rect.width;
            }
            if popup_rect.y + popup_rect.height > view_height {
                popup_rect.y = view_height - popup_rect.height;
            }
            if popup_rect.x < 0.0 {
                popup_rect.x = 0.0;
            }
            if popup_rect.y < 0.0 {
                popup_rect.y = 0.0;
            }
            popup_rect
        });
    }

    pub fn clear_popup_view(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.popup_rect = None;
        inner.popup_original_rect = None;
        self.popup_view.store(None);
    }

    fn get_or_init_gpu(&self, device: &wgpu::Device) -> &CachedGpuResources {
        self.cached_gpu.get_or_init(|| {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                    label: Some("cef_bind_group_layout"),
                });
            CachedGpuResources {
                sampler,
                bind_group_layout,
            }
        })
    }

    fn get_or_create_paint_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Ref<'_, CachedPaintTexture> {
        {
            let inner = self.inner.borrow();
            if let Some(ref texture) = inner.cached_paint_texture {
                if texture.width == width && texture.height == height && texture.format == format {
                    return Ref::map(inner, |inner| inner.cached_paint_texture.as_ref().unwrap());
                }
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            label: Some("cef_paint_texture"),
            view_formats: &[],
        });
        self.inner.borrow_mut().cached_paint_texture = Some(CachedPaintTexture {
            texture,
            width,
            height,
            format,
        });
        Ref::map(self.inner.borrow(), |inner| {
            inner.cached_paint_texture.as_ref().unwrap()
        })
    }
}

wrap_render_handler! {
    pub struct RenderHandlerBuilder {
        handler: IcyRenderHandler,
    }

    impl RenderHandler {
    fn root_screen_rect(
        &self,
        _browser: Option<&mut Browser>,
        _rect: Option<&mut Rect>,
    ) -> ::std::os::raw::c_int {
        false as _
    }

    fn view_rect(&self, browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
        let Some(rect) = rect else {
            return;
        };
        if let Some(browser_id) = browser.map(|b| b.identifier().into())
            && webview_states().with_borrow(|states| {
                if let Some(state) = states.get(&browser_id) {
                    let size = state.render.view_rect();
                    *rect = Rect {
                        width: if size.width > 0 { size.width } else { 1 },
                        height: if size.height > 0 { size.height } else { 1 },
                        x: size.x,
                        y: size.y,
                    };
                    return true;
                }
                false
            })
        {
            return;
        }
        *rect = Rect {
            width: 1,
            height: 1,
            ..Default::default()
        };
    }

    fn screen_point(
        &self,
        browser: Option<&mut Browser>,
        view_x: ::std::os::raw::c_int,
        view_y: ::std::os::raw::c_int,
        screen_x: Option<&mut ::std::os::raw::c_int>,
        screen_y: Option<&mut ::std::os::raw::c_int>,
    ) -> ::std::os::raw::c_int {
        let (Some(screen_x), Some(screen_y)) = (screen_x, screen_y) else {
            return false as _;
        };
        #[cfg(target_os = "windows")]
        {
            if let Some(browser_id) = browser.map(|b| b.identifier().into()) {
                return webview_states().with_borrow(|states| {
                    if let Some(state) = states.get(&browser_id)
                        && let Some(hwnd) = state.render.native_window()
                    {
                        let scale_factor = state.render.device_scale_factor().max(1.0);
                        let origin = state.render.view_origin();
                        let mut point = POINT {
                            x: ((origin.x + view_x as f32) * scale_factor).round() as i32,
                            y: ((origin.y + view_y as f32) * scale_factor).round() as i32,
                        };
                        unsafe {
                            _ = ClientToScreen(HWND(hwnd as *mut _), &mut point);
                        }
                        *screen_x = point.x;
                        *screen_y = point.y;
                        return true;
                    }
                    false
                }) as _;
            }
        }
        false as _
    }

    fn screen_info(
        &self,
        browser: Option<&mut Browser>,
        screen_info: Option<&mut cef::ScreenInfo>,
    ) -> ::std::os::raw::c_int {
        if let Some(screen_info) = screen_info
            && let Some(browser_id) = browser.map(|b| b.identifier().into())
        {
            return webview_states().with_borrow(|states| {
                if let Some(state) = states.get(&browser_id) {
                    let screen = todo!();
                    screen_info.device_scale_factor = screen.device_scale_factor as _;
                    screen_info.rect.x = 0;
                    screen_info.rect.y = 0;
                    screen_info.available_rect.x = 0;
                    screen_info.available_rect.y = 0;
                    return true;
                }
                false
            }) as _;
        }
        false as _
    }

    fn start_dragging(
        &self,
        browser: Option<&mut Browser>,
        drag_data: Option<&mut cef::DragData>,
        allowed_ops: DragOperationsMask,
        x: ::std::os::raw::c_int,
        y: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int {
        let Some(browser) = browser else {
            return false as _;
        };
        let Some(drag_data) = drag_data else {
            return false as _;
        };
        let Some(drag_data) = <_ as ImplDragData>::clone(drag_data) else {
            return false as _;
        };
        _ = self.handler.drag_tx.try_send((
            browser.identifier().into(),
            DragData {
                data: drag_data,
                operation: allowed_ops,
                x: x as _,
                y: y as _,
            },
        ));
        true as _
    }

    fn on_popup_show(&self, browser: Option<&mut Browser>, show: ::std::os::raw::c_int) {
        let Some(_browser) = browser else {
            return;
        };
        if show != 0 {
            return;
        }
        self.handler.state.clear_popup_view();
    }

    fn on_popup_size(&self, _browser: Option<&mut Browser>, rect: Option<&Rect>) {
        self.handler.state.set_popup_rect(rect.map(|rect| {
            iced::Rectangle::new(
                iced::Point::new(rect.x as f32, rect.y as f32),
                iced::Size::new(rect.width.max(0) as f32, rect.height.max(0) as f32),
            )
        }));
    }

    fn on_ime_composition_range_changed(
        &self,
        browser: Option<&mut Browser>,
        _selected_range: Option<&Range>,
        character_bounds: Option<&[Rect]>,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let Some(bounds) = character_bounds else {
            return;
        };
        let (prev_x, prev_y) = self.handler.state.inner.borrow().composition_position;
        if let Some(bounds) = bounds.last() {
            let x = bounds.x;
            let y = bounds.y;
            self.handler.state.inner.borrow_mut().composition_position = (x as f32, y as f32);
            if prev_y != y as f32 {
                _ = self.handler.ime_composition_tx.try_send((
                    browser.identifier().into(),
                    x as f32,
                    y as f32,
                ));
            }
        }
    }

    fn on_accelerated_paint(
        &self,
        browser: Option<&mut Browser>,
        type_: PaintElementType,
        dirty_rects: Option<&[Rect]>,
        info: Option<&AcceleratedPaintInfo>,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let Some(info) = info else {
            return;
        };
        let browser_id = browser.identifier().into();
        let dirty_rects = dirty_rects.and_then(|rects| {
            rects.first().map(|rect| {
                iced::Rectangle::new(
                    iced::Point::new(rect.x as f32, rect.y as f32),
                    iced::Size::new(rect.width as f32, rect.height as f32),
                )
            })
        });
        let is_view = type_ == cef::sys::cef_paint_element_type_t::PET_VIEW.into();
        let is_popup = type_ == cef::sys::cef_paint_element_type_t::PET_POPUP.into();
        if !is_view && !is_popup {
            return;
        }

        let device = todo!();
        let queue = todo!();

        let src_texture = {
            use cef::osr_texture_import::shared_texture_handle::SharedTextureHandle;
            let shared_handle = SharedTextureHandle::new(info);
            if let SharedTextureHandle::Unsupported = shared_handle {
                return;
            }
            match shared_handle.import_texture(device) {
                Ok(texture) => texture,
                Err(_) => return,
            }
        };

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Cef Texture"),
            size: wgpu::Extent3d {
                width: info.extra.coded_size.width as _,
                height: info.extra.coded_size.height as _,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let cached_dst = self.handler.state.get_or_create_paint_texture(
            device,
            texture_desc.size.width,
            texture_desc.size.height,
            texture_desc.format,
        );
        let dst_texture = &cached_dst.texture;
        let dst_view = dst_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("CEF Copy Texture View"),
            ..Default::default()
        });
        {
            let mut copy = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("CEF Copy Texture Command"),
            });
            wgpu::util::TextureBlitter::new(device, texture_desc.format).copy(
                device,
                &mut copy,
                &src_texture.create_view(&Default::default()),
                &dst_view,
            );
            queue.submit(Some(copy.finish()));
        }
        let cached = self.handler.state.get_or_init_gpu(device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cef Texture Bind Group"),
            layout: &cached.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&cached.sampler),
                },
            ],
        });

        let bounds = if is_popup {
            Some(self.handler.state.popup_rect().unwrap_or_else(|| {
                iced::Rectangle::new(
                    iced::Point::ORIGIN,
                    iced::Size::new(
                        info.extra.coded_size.width as f32,
                        info.extra.coded_size.height as f32,
                    ),
                )
            }))
        } else {
            None
        };
        let popup_offset = if is_popup {
            self.handler.state.popup_offset()
        } else {
            None
        };
        let view = Arc::new(CefView::new_hardware(
            browser_id,
            bind_group,
            dirty_rects,
            bounds,
            popup_offset,
            !is_popup,
            info.extra.capture_counter,
            false,
            is_popup,
        ));
        if is_popup {
            self.handler.state.popup_view.store(Some(view));
        } else {
            self.handler.state.hw_view.store(Some(view));
            if self.handler.state.popup_rect().is_some()
                && let Some(host) = browser.host()
            {
                host.invalidate(cef::PaintElementType::POPUP);
            }
        }
        _ = self.handler.cef_frame_tx.send(browser_id);
    }

    fn on_paint(
        &self,
        browser: Option<&mut Browser>,
        type_: PaintElementType,
        _dirty_rects: Option<&[Rect]>,
        buffer: *const u8,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let browser_id: BrowserId = browser.identifier().into();
        let is_view = type_ == cef::sys::cef_paint_element_type_t::PET_VIEW.into();
        let is_popup = type_ == cef::sys::cef_paint_element_type_t::PET_POPUP.into();
        if !is_view && !is_popup {
            return;
        }
        if buffer.is_null() || width <= 0 || height <= 0 {
            return;
        }
        let buffer_size = (width * height * 4) as usize;
        let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

        if let (Some(device), Some(queue)) = (todo!(), todo!()) {
            let cached = self.handler.state.get_or_init_gpu(device);
            let cached_tex = self.handler.state.get_or_create_paint_texture(
                device,
                width as u32,
                height as u32,
                wgpu::TextureFormat::Bgra8Unorm,
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &cached_tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                buffer_slice,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width as u32),
                    rows_per_image: Some(height as u32),
                },
                wgpu::Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
            );
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Cef Texture Bind Group"),
                layout: &cached.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &cached_tex
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor {
                                    label: Some("Cef Texture View"),
                                    ..Default::default()
                                }),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&cached.sampler),
                    },
                ],
            });
            drop(cached_tex);
            let bounds = if is_popup {
                Some(self.handler.state.popup_rect().unwrap_or_else(|| {
                    iced::Rectangle::new(
                        iced::Point::ORIGIN,
                        iced::Size::new(width as f32, height as f32),
                    )
                }))
            } else {
                None
            };
            let popup_offset = if is_popup {
                self.handler.state.popup_offset()
            } else {
                None
            };
            let view = Arc::new(CefView::new_hardware(
                browser_id,
                bind_group,
                None,
                bounds,
                popup_offset,
                !is_popup,
                0,
                false,
                is_popup,
            ));
            if is_popup {
                self.handler.state.popup_view.store(Some(view));
            } else {
                self.handler.state.hw_view.store(Some(view));
            }
        } else {
            let handle = {
                let mut inner = self.handler.state.inner.borrow_mut();
                convert_bgra_to_rgba(buffer_slice, &mut inner.pixels);
                iced::widget::image::Handle::from_rgba(
                    width as u32,
                    height as u32,
                    inner.pixels.clone(),
                )
            };
            self.handler
                .state
                .hw_view
                .store(Some(Arc::new(CefView::new_software(browser_id, handle))));
        }
        _ = self.handler.cef_frame_tx.send(browser_id);
    }
}
    }

impl RenderHandlerBuilder {
    pub fn build(handler: IcyRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

pub(crate) mod hw_renderer {
    use iced::{self};
    use iced_wgpu::wgpu;

    use crate::{BrowserId, client::render_handler::CefViewWrapper, instance::resize};

    pub struct CefPipeline {
        pipeline: wgpu::RenderPipeline,
        swizzle_pipeline: wgpu::RenderPipeline,
        quad: Geometry,
        bounds: iced::Rectangle<f32>,
    }

    impl CefPipeline {
        pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
            let texture_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Cef Texture Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Cef Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });
            let swizzle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Cef Swizzle Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader_rgba.wgsl").into()),
            });
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Cef Pipeline Layout"),
                    bind_group_layouts: &[&texture_bind_group_layout],
                    immediate_size: 0,
                });
            let create_pipeline = |label: &'static str, shader: &wgpu::ShaderModule, blend| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Vertex::desc()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview_mask: None,
                    cache: None,
                })
            };
            let alpha_blend = Some(wgpu::BlendState {
                color: wgpu::BlendComponent::OVER,
                alpha: wgpu::BlendComponent::OVER,
            });
            let pipeline = create_pipeline("Cef Alpha Render Pipeline", &shader, alpha_blend);
            let swizzle_pipeline = create_pipeline(
                "Cef Alpha Swizzle Render Pipeline",
                &swizzle_shader,
                alpha_blend,
            );
            Self {
                pipeline,
                swizzle_pipeline,
                quad: Geometry::new(device),
                bounds: iced::Rectangle::INFINITE,
            }
        }

        fn render(
            &self,
            bind_group: &wgpu::BindGroup,
            render_pass: &mut wgpu::RenderPass,
            _bounds: iced::Rectangle<f32>,
            _dirty_rect: Option<iced::Rectangle<f32>>,
            swap_rb: bool,
            _is_popup: bool,
        ) {
            render_pass.set_pipeline(if swap_rb {
                &self.swizzle_pipeline
            } else {
                &self.pipeline
            });
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.quad.vertex_buffer.slice(..));
            render_pass.draw(0..self.quad.vertex_count, 0..1);
        }
    }

    #[derive(Debug)]
    pub struct Primitive {
        bind_group: wgpu::BindGroup,
        browser_id: BrowserId,
        bounds: iced::Rectangle,
        dirty_rect: Option<iced::Rectangle>,
        should_resize: bool,
        swap_rb: bool,
        is_popup: bool,
    }

    impl Primitive {
        fn new(
            bind_group: wgpu::BindGroup,
            browser_id: BrowserId,
            bounds: iced::Rectangle,
            dirty_rect: Option<iced::Rectangle>,
            should_resize: bool,
            swap_rb: bool,
            is_popup: bool,
        ) -> Self {
            Self {
                bind_group,
                browser_id,
                bounds,
                dirty_rect,
                should_resize,
                swap_rb,
                is_popup,
            }
        }
    }

    impl<Message> iced::widget::shader::Program<Message> for CefViewWrapper {
        type State = Option<BrowserId>;
        type Primitive = Primitive;

        fn draw(
            &self,
            _state: &Self::State,
            _cursor: iced::mouse::Cursor,
            bounds: iced::Rectangle,
        ) -> Self::Primitive {
            Primitive::new(
                self.bind_group()
                    .cloned()
                    .expect("shader webview frame must come from accelerated paint"),
                self.browser_id,
                bounds,
                self.dirty_rect(),
                self.should_resize,
                self.swap_rb(),
                self.is_popup(),
            )
        }

        fn update(
            &self,
            state: &mut Self::State,
            _event: &iced::Event,
            _bounds: iced::Rectangle,
            _cursor: iced::advanced::mouse::Cursor,
        ) -> Option<iced::widget::Action<Message>> {
            if Some(self.0.browser_id) != *state {
                state.replace(self.0.browser_id);
            }
            None
        }
    }
    impl iced::widget::shader::Pipeline for CefPipeline {
        fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self
        where
            Self: Sized,
        {
            Self::new(device, format)
        }
    }

    impl iced::widget::shader::Primitive for Primitive {
        type Pipeline = CefPipeline;

        fn prepare(
            &self,
            pipeline: &mut Self::Pipeline,
            _device: &wgpu::Device,
            _queue: &wgpu::Queue,
            bounds: &iced::Rectangle,
            _viewport: &iced::widget::shader::Viewport,
        ) {
            if self.should_resize && pipeline.bounds != *bounds {
                pipeline.bounds = *bounds;
                resize(self.browser_id, *bounds);
            }
        }

        fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
            pipeline.render(
                &self.bind_group,
                render_pass,
                self.bounds,
                self.dirty_rect,
                self.swap_rb,
                self.is_popup,
            );
            true
        }
    }

    #[repr(C)]
    #[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        tex_coords: [f32; 2],
    }

    impl Vertex {
        const ATTRIBS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

        fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &Self::ATTRIBS,
            }
        }
    }

    struct Geometry {
        vertex_buffer: wgpu::Buffer,
        vertex_count: u32,
    }

    impl Geometry {
        fn new(device: &wgpu::Device) -> Self {
            use wgpu::util::DeviceExt as _;
            let x = -1.0;
            let y = 1.0;
            let width = 2.0;
            let height = 2.0;
            let z = 1.0;
            let vertices = [
                Vertex {
                    position: [x, y, z],
                    tex_coords: [0.0, 0.0],
                },
                Vertex {
                    position: [x + width, y, z],
                    tex_coords: [1.0, 0.0],
                },
                Vertex {
                    position: [x, y - height, z],
                    tex_coords: [0.0, 1.0],
                },
                Vertex {
                    position: [x + width, y - height, z],
                    tex_coords: [1.0, 1.0],
                },
            ];
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Quad Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            Self {
                vertex_buffer,
                vertex_count: vertices.len() as u32,
            }
        }
    }
}
