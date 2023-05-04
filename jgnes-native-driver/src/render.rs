// The generated Copy impl for Vertex2d violates this rule for some reason
#![allow(clippy::let_underscore_untyped)]

use crate::{colors, RendererConfig};
use jgnes_core::{ColorEmphasis, FrameBuffer, Renderer};
use sdl2::video::Window;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::{iter, mem};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderScale(u32);

impl RenderScale {
    pub const TWO: Self = Self(2);
    pub const THREE: Self = Self(3);

    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for RenderScale {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1..=16 => Ok(Self(value)),
            _ => Err(anyhow::Error::msg(format!(
                "Invalid render scale value: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuFilterMode {
    NearestNeighbor,
    Linear(RenderScale),
}

impl Display for GpuFilterMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NearestNeighbor => write!(f, "NearestNeighbor"),
            Self::Linear(render_scale) => write!(f, "Linear {}x", render_scale.0),
        }
    }
}

pub(crate) struct WgpuRenderer {
    _window: Window,
    render_config: RendererConfig,
    output_buffer: Vec<u8>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    texture: wgpu::Texture,
    compute_bind_group: wgpu::BindGroup,
    compute_pipeline: Option<wgpu::ComputePipeline>,
    render_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
}

impl WgpuRenderer {
    pub(crate) fn from_window(
        window: Window,
        render_config: RendererConfig,
    ) -> anyhow::Result<Self> {
        // TODO configurable
        let output_buffer = vec![
            0;
            4 * jgnes_core::SCREEN_WIDTH as usize
                * jgnes_core::VISIBLE_SCREEN_HEIGHT as usize
        ];

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        });

        // SAFETY: The surface must not outlive the window it was created from.
        // The surface and window are both owned by WgpuRenderer so they will be dropped at the
        // same time.
        let surface = unsafe { instance.create_surface(&window) }?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow::Error::msg("Unable to obtain wgpu adapter"))?;

        log::info!(
            "Using GPU adapter with backend {:?}",
            adapter.get_info().backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("device"),
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
            },
            None,
        ))?;

        let (window_width, window_height) = window.size();

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .ok_or_else(|| anyhow::Error::msg("Unable to find an sRGB wgpu surface format"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: window_width,
            height: window_height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // TODO configurable dimensions
        let texture_size = wgpu::Extent3d {
            width: jgnes_core::SCREEN_WIDTH.into(),
            height: jgnes_core::VISIBLE_SCREEN_HEIGHT.into(),
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let render_scale = match render_config.gpu_filter_mode {
            GpuFilterMode::NearestNeighbor => 1,
            GpuFilterMode::Linear(render_scale) => render_scale.0,
        };
        let scaled_texture_size = wgpu::Extent3d {
            // TODO configurable
            width: render_scale * u32::from(jgnes_core::SCREEN_WIDTH),
            height: render_scale * u32::from(jgnes_core::VISIBLE_SCREEN_HEIGHT),
            depth_or_array_layers: 1,
        };
        let scaled_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scaled_texture"),
            size: scaled_texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let scaled_texture_view =
            scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler_filter_mode = match render_config.gpu_filter_mode {
            GpuFilterMode::NearestNeighbor => wgpu::FilterMode::Nearest,
            GpuFilterMode::Linear(_) => wgpu::FilterMode::Linear,
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: sampler_filter_mode,
            min_filter: sampler_filter_mode,
            mipmap_filter: sampler_filter_mode,
            ..wgpu::SamplerDescriptor::default()
        });

        let display_area =
            super::determine_display_area(window_width, window_height, render_config.aspect_ratio);
        let vertices: Vec<_> = VERTICES
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
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

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
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&scaled_texture_view),
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

        // Compute pipeline is for texture scaling and is only needed if render scale is higher than 1
        let compute_pipeline = (render_scale > 1).then(|| {
            let compute_entry_point = format!("texture_scale_{render_scale}x");
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("compute_pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &texture_scale_shader,
                entry_point: &compute_entry_point,
            })
        });

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
        let render_bind_texture = if render_scale > 1 {
            &scaled_texture_view
        } else {
            &texture_view
        };
        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_bind_group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(render_bind_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

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
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Ok(Self {
            _window: window,
            render_config,
            output_buffer,
            device,
            queue,
            surface,
            texture,
            compute_bind_group,
            compute_pipeline,
            render_bind_group,
            render_pipeline,
            vertex_buffer,
        })
    }
}

impl Renderer for WgpuRenderer {
    type Err = anyhow::Error;

    fn render_frame(
        &mut self,
        frame_buffer: &FrameBuffer,
        color_emphasis: ColorEmphasis,
    ) -> Result<(), Self::Err> {
        colors::to_rgba(
            frame_buffer,
            color_emphasis,
            self.render_config.overscan,
            &mut self.output_buffer,
        );

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.output_buffer,
            wgpu::ImageDataLayout {
                offset: 0,
                // TODO configurable
                bytes_per_row: Some(4 * u32::from(jgnes_core::SCREEN_WIDTH)),
                rows_per_image: Some(jgnes_core::VISIBLE_SCREEN_HEIGHT.into()),
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

        if let Some(compute_pipeline) = &self.compute_pipeline {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute_pass"),
            });

            compute_pass.set_pipeline(compute_pipeline);
            compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);

            // TODO configurable
            compute_pass.dispatch_workgroups(
                jgnes_core::SCREEN_WIDTH.into(),
                jgnes_core::VISIBLE_SCREEN_HEIGHT.into(),
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
}
