// The generated Copy impl for Vertex2d violates this rule for some reason
#![allow(clippy::let_underscore_untyped)]

use crate::colors;
use crate::config::{AspectRatio, FrameSkip, GpuFilterMode, Overscan, RendererConfig, VSyncMode};
use jgnes_core::{ColorEmphasis, FrameBuffer, Renderer, TimingMode};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use std::{iter, mem};
use thiserror::Error;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex2d {
    position: [f32; 2],
    texture_coords: [f32; 2],
}

const VERTICES: [Vertex2d; 6] = [
    Vertex2d {
        position: [-1.0, 1.0],
        texture_coords: [0.0, 0.0],
    },
    Vertex2d {
        position: [-1.0, -1.0],
        texture_coords: [0.0, 1.0],
    },
    Vertex2d {
        position: [1.0, -1.0],
        texture_coords: [1.0, 1.0],
    },
    Vertex2d {
        position: [1.0, -1.0],
        texture_coords: [1.0, 1.0],
    },
    Vertex2d {
        position: [1.0, 1.0],
        texture_coords: [1.0, 0.0],
    },
    Vertex2d {
        position: [-1.0, 1.0],
        texture_coords: [0.0, 0.0],
    },
];

impl Vertex2d {
    const LAYOUT_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex2d>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::LAYOUT_ATTRIBUTES,
        }
    }
}

#[derive(Debug, Error)]
pub enum WgpuRendererError {
    #[error("Error creating wgpu surface: {source}")]
    CreateSurface {
        #[from]
        source: wgpu::CreateSurfaceError,
    },
    #[error("Error requesting wgpu device: {source}")]
    RequestDevice {
        #[from]
        source: wgpu::RequestDeviceError,
    },
    #[error("Error retrieving wgpu output surface: {source}")]
    OutputSurface {
        #[from]
        source: wgpu::SurfaceError,
    },
    #[error("Error in wgpu renderer: {msg}")]
    Other { msg: String },
}

impl WgpuRendererError {
    fn msg(s: impl Into<String>) -> Self {
        Self::Other { msg: s.into() }
    }
}

pub type WindowSizeFn<W> = fn(&W) -> (u32, u32);

pub struct WgpuRenderer<W> {
    render_config: RendererConfig,
    timing_mode: TimingMode,
    output_buffer: Vec<u8>,
    cpu_scale_output_buffer: Vec<u8>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    surface_capabilities: wgpu::SurfaceCapabilities,
    surface_config: wgpu::SurfaceConfiguration,
    texture_format: wgpu::TextureFormat,
    texture: wgpu::Texture,
    compute_resources: Option<(wgpu::BindGroup, wgpu::ComputePipeline)>,
    render_bind_group: wgpu::BindGroup,
    render_bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertices: Vec<Vertex2d>,
    vertex_buffer: wgpu::Buffer,
    // SAFETY: The window must be declared after the surface so that it is not dropped before the
    // surface is dropped
    window: W,
    window_size_fn: WindowSizeFn<W>,
    total_frames: u64,
}

impl<W> WgpuRenderer<W>
where
    W: HasRawWindowHandle + HasRawDisplayHandle,
{
    /// Create a new wgpu renderer which will output to the given window.
    ///
    /// # Errors
    ///
    /// This function will return an error if there are any problems initializing wgpu or the
    /// rendering pipeline.
    pub async fn from_window(
        window: W,
        window_size_fn: WindowSizeFn<W>,
        render_config: RendererConfig,
    ) -> Result<Self, WgpuRendererError> {
        let timing_mode = TimingMode::Ntsc;

        let output_buffer = vec![0; output_buffer_len(timing_mode)];

        let cpu_render_scale = render_config.gpu_filter_mode.cpu_render_scale() as usize;
        let cpu_scale_output_buffer =
            vec![0; output_buffer.len() * cpu_render_scale * cpu_render_scale];

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: render_config.wgpu_backend.to_wgpu_backends(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        });

        // SAFETY: The surface must not outlive the window it was created from.
        // The surface and window are both owned by WgpuRenderer, and the window field is declared
        // after the surface field, so the surface will always be dropped before the window is
        // dropped.
        let surface = unsafe { instance.create_surface(&window) }?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| WgpuRendererError::msg("Unable to obtain wgpu adapter"))?;

        log::info!(
            "Using GPU adapter with backend {:?}",
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("device"),
                    features: wgpu::Features::empty(),
                    limits: if render_config.use_webgl2_limits {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    },
                },
                None,
            )
            .await?;

        let (window_width, window_height) = window_size_fn(&window);

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or_else(|| {
                log::warn!("wgpu adapter does not support any sRGB texture formats; defaulting to first format in this list: {:?}", surface_capabilities.formats);
                surface_capabilities.formats[0]
            });

        let desired_present_mode = render_config.vsync_mode.to_present_mode();

        if !surface_capabilities
            .present_modes
            .contains(&desired_present_mode)
        {
            return Err(WgpuRendererError::msg(unsupported_vsync_mode_error(
                render_config.vsync_mode,
                &surface_capabilities.present_modes,
            )));
        }

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: window_width,
            height: window_height,
            present_mode: desired_present_mode,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let texture_format = if surface_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let cpu_render_scale = render_config.gpu_filter_mode.cpu_render_scale();
        let texture = create_texture(&device, texture_format, cpu_render_scale, timing_mode);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let gpu_render_scale = render_config.gpu_filter_mode.gpu_render_scale();
        let scaled_texture = create_scaled_texture(&device, gpu_render_scale, timing_mode);
        let scaled_texture_view =
            scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = create_sampler(&device, render_config.gpu_filter_mode);

        let vertices = compute_vertices(window_width, window_height, &render_config, timing_mode);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        // Compute pipeline is for texture scaling and is only needed if render scale is higher than 1
        let compute_resources = create_compute_resources(
            &device,
            &scaled_texture,
            &texture_view,
            &scaled_texture_view,
            gpu_render_scale,
        );

        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("render_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
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
        // Ignore the scaled texture if render scale is 1
        let render_bind_texture = if gpu_render_scale > 1 {
            &scaled_texture_view
        } else {
            &texture_view
        };
        let render_bind_group = create_render_bind_group(
            &device,
            &render_bind_group_layout,
            render_bind_texture,
            &sampler,
        );

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex2d::descriptor()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Ok(Self {
            render_config,
            timing_mode,
            output_buffer,
            cpu_scale_output_buffer,
            device,
            queue,
            surface,
            surface_capabilities,
            surface_config,
            texture_format,
            texture,
            compute_resources,
            render_bind_group,
            render_bind_group_layout,
            render_pipeline,
            vertices,
            vertex_buffer,
            window,
            window_size_fn,
            total_frames: 0,
        })
    }

    pub fn window(&self) -> &W {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut W {
        &mut self.window
    }

    pub fn reconfigure_surface(&mut self) {
        let (window_width, window_height) = (self.window_size_fn)(&self.window);
        self.surface_config.width = window_width;
        self.surface_config.height = window_height;

        self.surface.configure(&self.device, &self.surface_config);

        self.vertices = compute_vertices(
            window_width,
            window_height,
            &self.render_config,
            self.timing_mode,
        );
    }

    pub fn update_aspect_ratio(&mut self, aspect_ratio: AspectRatio) {
        self.render_config.aspect_ratio = aspect_ratio;
        self.reconfigure_surface();
    }

    pub fn update_filter_mode(&mut self, filter_mode: GpuFilterMode) {
        self.render_config.gpu_filter_mode = filter_mode;
        self.reinit_textures();
    }

    fn reinit_textures(&mut self) {
        let filter_mode = self.render_config.gpu_filter_mode;

        let sampler = create_sampler(&self.device, filter_mode);

        match filter_mode {
            GpuFilterMode::LinearCpuScaled(render_scale) => {
                let render_scale = render_scale.get() as usize;
                self.cpu_scale_output_buffer =
                    vec![0; self.output_buffer.len() * render_scale * render_scale];

                self.texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("texture"),
                    size: wgpu::Extent3d {
                        width: render_scale as u32 * u32::from(jgnes_core::SCREEN_WIDTH),
                        height: render_scale as u32
                            * u32::from(self.timing_mode.visible_screen_height()),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.texture_format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                let texture_view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                self.render_bind_group = create_render_bind_group(
                    &self.device,
                    &self.render_bind_group_layout,
                    &texture_view,
                    &sampler,
                );
            }
            GpuFilterMode::NearestNeighbor | GpuFilterMode::Linear(_) => {
                self.texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("texture"),
                    size: wgpu::Extent3d {
                        width: u32::from(jgnes_core::SCREEN_WIDTH),
                        height: u32::from(self.timing_mode.visible_screen_height()),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.texture_format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                let texture_view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                match filter_mode {
                    GpuFilterMode::NearestNeighbor => {
                        self.render_bind_group = create_render_bind_group(
                            &self.device,
                            &self.render_bind_group_layout,
                            &texture_view,
                            &sampler,
                        );
                    }
                    GpuFilterMode::Linear(render_scale) => {
                        let render_scale = render_scale.get();

                        let scaled_texture =
                            create_scaled_texture(&self.device, render_scale, self.timing_mode);
                        let scaled_texture_view =
                            scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());

                        self.compute_resources = create_compute_resources(
                            &self.device,
                            &scaled_texture,
                            &texture_view,
                            &scaled_texture_view,
                            render_scale,
                        );

                        let render_texture_view = if render_scale > 1 {
                            &scaled_texture_view
                        } else {
                            &texture_view
                        };
                        self.render_bind_group = create_render_bind_group(
                            &self.device,
                            &self.render_bind_group_layout,
                            render_texture_view,
                            &sampler,
                        );
                    }
                    GpuFilterMode::LinearCpuScaled(_) => unreachable!("nested match expressions"),
                }
            }
        }
    }

    pub fn update_overscan(&mut self, overscan: Overscan) {
        self.render_config.overscan = overscan;
        // No need to reconfigure surface, overscan is read on every frame rendered as it is only
        // used to determine which pixels to copy from the NES PPU frame buffer
    }

    pub fn update_frame_skip(&mut self, frame_skip: FrameSkip) {
        self.render_config.frame_skip = frame_skip;
    }

    pub fn update_forced_integer_height_scaling(&mut self, forced_integer_height_scaling: bool) {
        self.render_config.forced_integer_height_scaling = forced_integer_height_scaling;
        self.reconfigure_surface();
    }

    /// # Errors
    ///
    /// This method will return an error if the GPU driver does not support the specified vsync mode.
    pub fn update_vsync_mode(&mut self, vsync_mode: VSyncMode) -> Result<(), WgpuRendererError> {
        let present_mode = vsync_mode.to_present_mode();
        if !self
            .surface_capabilities
            .present_modes
            .contains(&present_mode)
        {
            return Err(WgpuRendererError::msg(unsupported_vsync_mode_error(
                vsync_mode,
                &self.surface_capabilities.present_modes,
            )));
        }

        self.render_config.vsync_mode = vsync_mode;
        self.surface_config.present_mode = present_mode;
        self.reconfigure_surface();

        Ok(())
    }
}

const fn output_buffer_len(timing_mode: TimingMode) -> usize {
    4 * jgnes_core::SCREEN_WIDTH as usize * timing_mode.visible_screen_height() as usize
}

fn unsupported_vsync_mode_error(
    vsync_mode: VSyncMode,
    supported_modes: &[wgpu::PresentMode],
) -> String {
    let desired_present_mode = vsync_mode.to_present_mode();
    format!(
        "GPU hardware/driver does not support VSync mode {vsync_mode} (wgpu present mode {desired_present_mode:?}); supported present modes are {supported_modes:?}",
    )
}

fn create_compute_resources(
    device: &wgpu::Device,
    scaled_texture: &wgpu::Texture,
    texture_view: &wgpu::TextureView,
    scaled_texture_view: &wgpu::TextureView,
    gpu_render_scale: u32,
) -> Option<(wgpu::BindGroup, wgpu::ComputePipeline)> {
    (gpu_render_scale > 1).then(|| {
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compute_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: scaled_texture.format(),
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_bind_group"),
            layout: &compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(scaled_texture_view),
                },
            ],
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("compute_pipeline_layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let texture_scale_shader =
            device.create_shader_module(wgpu::include_wgsl!("texture_scale.wgsl"));

        let compute_entry_point = format!("texture_scale_{gpu_render_scale}x");
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute_pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &texture_scale_shader,
            entry_point: &compute_entry_point,
        });

        (compute_bind_group, compute_pipeline)
    })
}

fn create_texture(
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
    cpu_render_scale: u32,
    timing_mode: TimingMode,
) -> wgpu::Texture {
    let texture_size = wgpu::Extent3d {
        width: cpu_render_scale * u32::from(jgnes_core::SCREEN_WIDTH),
        height: cpu_render_scale * u32::from(timing_mode.visible_screen_height()),
        depth_or_array_layers: 1,
    };
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_scaled_texture(
    device: &wgpu::Device,
    gpu_render_scale: u32,
    timing_mode: TimingMode,
) -> wgpu::Texture {
    let scaled_texture_size = wgpu::Extent3d {
        width: gpu_render_scale * u32::from(jgnes_core::SCREEN_WIDTH),
        height: gpu_render_scale * u32::from(timing_mode.visible_screen_height()),
        depth_or_array_layers: 1,
    };
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scaled_texture"),
        size: scaled_texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    })
}

fn create_render_bind_group(
    device: &wgpu::Device,
    render_bind_group_layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render_bind_group"),
        layout: render_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_sampler(device: &wgpu::Device, filter_mode: GpuFilterMode) -> wgpu::Sampler {
    let sampler_filter_mode = match filter_mode {
        GpuFilterMode::NearestNeighbor => wgpu::FilterMode::Nearest,
        GpuFilterMode::Linear(_) | GpuFilterMode::LinearCpuScaled(_) => wgpu::FilterMode::Linear,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: sampler_filter_mode,
        min_filter: sampler_filter_mode,
        mipmap_filter: sampler_filter_mode,
        ..wgpu::SamplerDescriptor::default()
    })
}

fn compute_vertices(
    window_width: u32,
    window_height: u32,
    render_config: &RendererConfig,
    timing_mode: TimingMode,
) -> Vec<Vertex2d> {
    let display_area = crate::determine_display_area(
        window_width,
        window_height,
        render_config.aspect_ratio,
        render_config.forced_integer_height_scaling,
        timing_mode,
    );
    VERTICES
        .into_iter()
        .map(|vertex| Vertex2d {
            position: [
                (f64::from(vertex.position[0]) * f64::from(display_area.width)
                    / f64::from(window_width)) as f32,
                (f64::from(vertex.position[1]) * f64::from(display_area.height)
                    / f64::from(window_height)) as f32,
            ],
            texture_coords: vertex.texture_coords,
        })
        .collect()
}

fn cpu_scale_texture(
    output_buffer: &[u8],
    scaled_buffer: &mut [u8],
    cpu_render_scale: u32,
    timing_mode: TimingMode,
) {
    for i in 0..u32::from(timing_mode.visible_screen_height()) {
        for j in 0..u32::from(jgnes_core::SCREEN_WIDTH) {
            let from_start = (i * 4 * u32::from(jgnes_core::SCREEN_WIDTH) + j * 4) as usize;

            for ii in 0..cpu_render_scale {
                for jj in 0..cpu_render_scale {
                    let to_start = ((cpu_render_scale * i + ii)
                        * 4
                        * cpu_render_scale
                        * u32::from(jgnes_core::SCREEN_WIDTH)
                        + (cpu_render_scale * j + jj) * 4)
                        as usize;
                    scaled_buffer[to_start..to_start + 4]
                        .copy_from_slice(&output_buffer[from_start..from_start + 4]);
                }
            }
        }
    }
}

impl<W: HasRawDisplayHandle + HasRawWindowHandle> Renderer for WgpuRenderer<W> {
    type Err = WgpuRendererError;

    fn render_frame(
        &mut self,
        frame_buffer: &FrameBuffer,
        color_emphasis: ColorEmphasis,
    ) -> Result<(), Self::Err> {
        self.total_frames += 1;

        if self.render_config.frame_skip.should_skip(self.total_frames) {
            return Ok(());
        }

        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));

        colors::to_rgba(
            frame_buffer,
            color_emphasis,
            self.render_config.overscan,
            self.timing_mode,
            &mut self.output_buffer,
        );

        let cpu_render_scale = self.render_config.gpu_filter_mode.cpu_render_scale();
        let render_buffer = if cpu_render_scale > 1 {
            cpu_scale_texture(
                &self.output_buffer,
                &mut self.cpu_scale_output_buffer,
                cpu_render_scale,
                self.timing_mode,
            );
            &self.cpu_scale_output_buffer
        } else {
            &self.output_buffer
        };

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            render_buffer,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * cpu_render_scale * u32::from(jgnes_core::SCREEN_WIDTH)),
                rows_per_image: Some(
                    cpu_render_scale * u32::from(self.timing_mode.visible_screen_height()),
                ),
            },
            self.texture.size(),
        );

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("command_encoder"),
            });

        if let Some((compute_bind_group, compute_pipeline)) = &self.compute_resources {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_pass"),
            });

            compute_pass.set_pipeline(compute_pipeline);
            compute_pass.set_bind_group(0, compute_bind_group, &[]);

            compute_pass.dispatch_workgroups(
                jgnes_core::SCREEN_WIDTH.into(),
                self.timing_mode.visible_screen_height().into(),
                1,
            );
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.render_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            render_pass.draw(0..VERTICES.len() as u32, 0..1);
        }

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn set_timing_mode(&mut self, timing_mode: TimingMode) -> Result<(), Self::Err> {
        self.timing_mode = timing_mode;

        self.output_buffer = vec![0; output_buffer_len(timing_mode)];

        self.reinit_textures();
        self.reconfigure_surface();

        Ok(())
    }
}
