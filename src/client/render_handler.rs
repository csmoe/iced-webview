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
    device: wgpu::Device,
    cef_bind_group: std::rc::Rc<RefCell<Option<wgpu::BindGroup>>>,
}

impl IcyRenderHandler {
    pub fn new(
        device_scale_factor: f32,
        view_rect: cef::Rect,
    ) -> (Self, IcyRenderState, UnboundedReceiver<BrowserId>) {
        let device_scale_factor = std::rc::Rc::new(RefCell::new(device_scale_factor));
        let view_rect = std::rc::Rc::new(RefCell::new(view_rect));
        let size = std::rc::Rc::new(RefCell::new((0, 0)));
        let cef_bind_group = std::rc::Rc::new(RefCell::new(None));
        let (tx, rx) = unbounded_channel();
        let state = IcyRenderState {
            pixels: std::rc::Rc::new(RefCell::new(Vec::with_capacity(1024 * 1024))),
            device_scale_factor,
            view_rect,
            size,
            cef_bind_group,
        };
        (
            Self {
                state: state.clone(),
                tx,
                device: todo!(),
                cef_bind_group: cef_bind_group.clone(),
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
    pub(crate) cef_bind_group: std::rc::Rc<RefCell<Option<wgpu::BindGroup>>>,
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

    fn on_accelerated_paint(
        &self,
        _browser: Option<&mut Browser>,
        _type_: PaintElementType,
        _dirty_rects_count: usize,
        _dirty_rects: Option<&Rect>,
        info: Option<&AcceleratedPaintInfo>,
    ) {
        let Some(info) = info else {
            return;
        };
        let format = match info.format.as_ref() {
            cef::sys::cef_color_type_t::CEF_COLOR_TYPE_BGRA_8888 => wgpu::TextureFormat::Bgra8Unorm,
            cef::sys::cef_color_type_t::CEF_COLOR_TYPE_RGBA_8888 => wgpu::TextureFormat::Rgba8Unorm,
            _ => unreachable!("unsupported color type"),
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
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let handle = windows::Win32::Foundation::HANDLE(info.shared_texture_handle.cast());
        let resource = unsafe {
            self.handler
                .device
                .as_hal::<wgpu::wgc::api::Dx12>()
                .map(|hdevice| {
                    let raw_device = hdevice.raw_device();

                    let mut resource = None::<windows::Win32::Graphics::Direct3D12::ID3D12Resource>;
                    match raw_device.OpenSharedHandle(handle, &mut resource) {
                        Ok(_) => Ok(resource.unwrap()),
                        Err(e) => Err(e),
                    }
                })
        };
        let resource = resource.unwrap().unwrap();

        let src_texture = unsafe {
            let texture = <wgpu::wgc::api::Dx12 as wgpu::hal::Api>::Device::texture_from_raw(
                resource,
                texture_desc.format,
                texture_desc.dimension,
                texture_desc.size,
                1,
                1,
            );

            self.handler
                .device
                .create_texture_from_hal::<wgpu::wgc::api::Dx12>(texture, &texture_desc)
        };

        let sampler = self
            .handler
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
        let texture_bind_group_layout =
            self.handler
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let bind_group = self
            .handler
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Cef Texture Bind Group"),
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_texture.create_view(
                            &wgpu::TextureViewDescriptor {
                                label: Some("Cef Texture View"),
                                ..Default::default()
                            },
                        )),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
        self.handler.cef_bind_group.borrow_mut().replace(bind_group);
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

pub struct CefPipeline {
    pipeline: wgpu::RenderPipeline,
    quad: Geometry,
    //cef_bind_group: std::rc::Rc<RefCell<Option<wgpu::BindGroup>>>,
}

impl CefPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
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

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Cef Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                push_constant_ranges: &[],
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cef Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
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
            multiview: None,
            cache: None,
        });
        Self {
            pipeline,
            quad: Geometry::new(&device),
        }
    }

    fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        viewport: iced::Rectangle<u32>,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cef Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.quad.vertex_buffer.slice(..));
        render_pass.draw(0..self.quad.vertex_count, 0..1);
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

pub struct CefWebview {
    bind_group: wgpu::BindGroup,
}

impl CefWebview {
    pub fn new(bind_group: wgpu::BindGroup) -> Self {
        Self { bind_group }
    }
}

#[derive(Debug)]
pub struct Primitive {
    bind_group: wgpu::BindGroup,
}

impl Primitive {
    fn new(bind_group: wgpu::BindGroup) -> Self {
        Self { bind_group }
    }
}

impl<Message> iced::widget::shader::Program<Message> for CefWebview {
    type State = ();
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        bounds: iced::Rectangle,
    ) -> Self::Primitive {
        Primitive::new(self.bind_group.clone())
    }
}

impl iced::widget::shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut iced::widget::shader::Storage,
        _bounds: &iced::Rectangle,
        viewport: &iced::widget::shader::Viewport,
    ) {
        if !storage.has::<CefPipeline>() {
            storage.store(CefPipeline::new(
                device,
                //queue,
                //format,
                //viewport.physical_size(),
            ));
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced::widget::shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        // At this point our pipeline should always be initialized
        let pipeline = storage.get::<CefPipeline>().unwrap();

        // Render primitive
        pipeline.render(target, encoder, &self.bind_group, *clip_bounds);
    }
}
