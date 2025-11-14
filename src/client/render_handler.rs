use crate::BrowserId;
use crate::instance::resize;
use cef;
use cef::{rc::*, sys, *};
use iced_wgpu::window::compositor::hack_wgpu::{get_wgpu_device, get_wgpu_queue};
use std::{
    cell::{Ref, RefCell},
    fmt::Debug,
    ptr::null_mut,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone, Debug)]
pub struct CefFrame {
    browser_id: BrowserId,
    frame: wgpu::BindGroup,
}

impl CefFrame {
    pub fn view(&self) -> wgpu::BindGroup {
        self.frame.clone()
    }
}

#[derive(Clone)]
pub struct IcyRenderHandler {
    state: IcyRenderState,
    tx: UnboundedSender<CefFrame>,
}

impl IcyRenderHandler {
    pub fn new(
        device_scale_factor: f32,
        view_rect: cef::Rect,
    ) -> (Self, IcyRenderState, UnboundedReceiver<CefFrame>) {
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

    fn on_accelerated_paint(
        &self,
        browser: Option<&mut Browser>,
        type_: PaintElementType,
        dirty_rects_count: usize,
        dirty_rects: Option<&Rect>,
        info: Option<&AcceleratedPaintInfo>,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let Some(host) = browser.host() else {
            return;
        };

        let Some(info) = info else {
            return;
        };

        let browser_id = browser.identifier().into();
        assert!(info.extra.has_capture_update_rect == 1);

        let Some(device) = get_wgpu_device() else {
            return;
        };

        let src_texture = {
            use cef::osr_texture_import::shared_texture_handle::SharedTextureHandle;

            if type_ != PaintElementType::default() {
                return;
            }

            let shared_handle = SharedTextureHandle::new(info);
            if let SharedTextureHandle::Unsupported = shared_handle {
                return;
            }

            match shared_handle.import_texture(device) {
                Ok(texture) => texture,
                Err(err) => {
                    return;
                }
            }
        };

        let dst_texture = src_texture;
        /*
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
        let dst_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("CEF Copy Texture"),
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            ..texture_desc
        });
        let texture_view = dst_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("CEF Copy Texutre View"),
            ..Default::default()
        });
        let mut copy = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("CEF Copy Texture Command"),
        });
        wgpu::util::TextureBlitter::new(device, texture_desc.format).copy(
            device,
            &mut copy,
            &src_texture.create_view(&Default::default()),
            &texture_view,
        );
        queue.submit(Some(copy.finish()));
        device.poll(wgpu::PollType::Wait).unwrap();
        */
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cef Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dst_texture.create_view(
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

        _ = self.handler.tx.send(CefFrame {
            browser_id,
            frame: bind_group,
        });
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
        let Some(device) = get_wgpu_device() else {
            return;
        };
        let Some(queue) = get_wgpu_queue() else {
            return;
        };
        let browser_id: BrowserId = browser.identifier().into();
        if type_ != cef::sys::cef_paint_element_type_t::PET_VIEW.into() {
            return;
        }
        use wgpu::{Extent3d, TextureDimension, TextureUsages};

        if buffer.is_null() || width <= 0 || height <= 0 {
            return;
        }

        let buffer_size = (width * height * 4) as usize; // BGRA format
        let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("CEF Paint Texture"),
            size: Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let src_texture = device.create_texture(&texture_desc);
        let dst_texture = src_texture;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dst_texture,
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
            texture_desc.size,
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cef Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dst_texture.create_view(
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

        _ = self.handler.tx.send(CefFrame {
            browser_id,
            frame: bind_group,
        });
    }
}

pub struct CefPipeline {
    pipeline: wgpu::RenderPipeline,
    quad: Geometry,
    bounds: iced::Rectangle<f32>,
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
            quad: Geometry::new(device),
            bounds: iced::Rectangle::INFINITE,
        }
    }

    fn render(
        &self,
        bind_group: &wgpu::BindGroup,
        render_pass: &mut wgpu::RenderPass,
        _bounds: iced::Rectangle<f32>,
    ) {
        render_pass.set_pipeline(&self.pipeline);
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
}

impl Primitive {
    fn new(bind_group: wgpu::BindGroup, browser_id: BrowserId, bounds: iced::Rectangle) -> Self {
        Self {
            bind_group,
            browser_id,
            bounds,
        }
    }
}

impl<Message> iced::widget::shader::Program<Message> for CefFrame {
    type State = Option<BrowserId>;
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        bounds: iced::Rectangle,
    ) -> Self::Primitive {
        Primitive::new(self.frame.clone(), self.browser_id, bounds)
    }

    fn update(
        &self,
        state: &mut Self::State,
        _event: &iced::Event,
        _bounds: iced::Rectangle,
        _cursor: iced::advanced::mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        if Some(self.browser_id) != *state {
            state.replace(self.browser_id);
        }
        None
    }
}

impl iced::widget::shader::Primitive for Primitive {
    type Renderer = CefPipeline;

    fn initialize(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _format: wgpu::TextureFormat,
    ) -> Self::Renderer {
        CefPipeline::new(device)
    }
    fn prepare(
        &self,
        pipeline: &mut Self::Renderer,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        bounds: &iced::Rectangle,
        _viewport: &iced::widget::shader::Viewport,
    ) {
        if pipeline.bounds != *bounds {
            pipeline.bounds = *bounds;
            resize(self.browser_id, *bounds);
        }
    }

    fn draw(&self, renderer: &Self::Renderer, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        renderer.render(&self.bind_group, render_pass, self.bounds);
        true
    }

    fn render(
        &self,
        _renderer: &Self::Renderer,
        _encoder: &mut wgpu::CommandEncoder,
        _target: &wgpu::TextureView,
        _clip_bounds: &iced::Rectangle<u32>,
    ) {
        return;

        /*
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cef Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });
        render_pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        renderer.render(
            &self.bind_group,
            &mut render_pass,
            self.bounds,
            self.dirty_rect,
        );*/
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
