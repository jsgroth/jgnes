use crate::config::{RenderScale, Scanlines, Shader};
use crate::renderer::Vertex2d;
use crate::DisplayArea;
use jgnes_core::TimingMode;
use wgpu::util::DeviceExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlurDirection {
    Horizontal = 0,
    Vertical = 1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurGlobals {
    texture_width: u32,
    texture_height: u32,
    blur_direction: u32,
}

impl BlurGlobals {
    const SIZE: usize = 12;

    fn new(texture_size: wgpu::Extent3d, blur_direction: BlurDirection) -> Self {
        Self {
            texture_width: texture_size.width,
            texture_height: texture_size.height,
            blur_direction: blur_direction as u32,
        }
    }

    fn to_bytes(self) -> [u8; Self::SIZE] {
        bytemuck::cast(self)
    }
}

const FS_GLOBALS_PADDING: usize = 12;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FragmentGlobals {
    viewport_x: u32,
    viewport_y: u32,
    viewport_width: u32,
    viewport_height: u32,
    nes_visible_height: u32,
    // WebGL requires types to be a multiple of 16 bytes
    padding: [u8; FS_GLOBALS_PADDING],
}

impl FragmentGlobals {
    pub const SIZE: usize = 32;

    pub fn new(display_area: DisplayArea, timing_mode: TimingMode) -> Self {
        Self {
            viewport_x: display_area.x,
            viewport_y: display_area.y,
            viewport_width: display_area.width,
            viewport_height: display_area.height,
            nes_visible_height: timing_mode.visible_screen_height().into(),
            padding: [0; FS_GLOBALS_PADDING],
        }
    }

    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        bytemuck::cast(self)
    }
}

fn compute_blur_weights(stdev: f64, radius: u32) -> Vec<f32> {
    let len = (2 * radius + 1) as i32;
    let center = len / 2;

    let mut weights: Vec<_> = (0..len)
        .map(|i| {
            // Gaussian blur formula
            let x = f64::from(i - center);
            1.0 / (2.0 * std::f64::consts::PI * stdev.powi(2)).sqrt()
                * (-1.0 * x.powi(2) / (2.0 * stdev.powi(2))).exp()
        })
        .collect();

    // Normalize weights so they sum to 1
    let weight_sum = weights.iter().copied().sum::<f64>();
    for value in &mut weights {
        *value /= weight_sum;
    }

    weights.into_iter().map(|weight| weight as f32).collect()
}

struct TextureScalePipeline {
    scaled_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl TextureScalePipeline {
    fn create(device: &wgpu::Device, render_scale: RenderScale, input: &wgpu::Texture) -> Self {
        let render_scale = render_scale.get();

        let scaled_texture_size = wgpu::Extent3d {
            width: render_scale * input.width(),
            height: render_scale * input.height(),
            depth_or_array_layers: 1,
        };
        let scaled_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scaled_texture"),
            size: scaled_texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: input.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_scale_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());

        let render_scale_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("render_scale_buffer"),
            // Must be padded to 16 bytes for WebGL
            contents: bytemuck::cast_slice(&[render_scale, 0, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_scale_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &render_scale_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("texture_scale_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader_module = device.create_shader_module(wgpu::include_wgsl!("prescale.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("texture_scale_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[],
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
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: scaled_texture.format(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            scaled_texture,
            bind_group,
            pipeline,
        }
    }

    fn draw(&self, encoder: &mut wgpu::CommandEncoder) {
        let scaled_texture_view = self
            .scaled_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("texture_scale_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &scaled_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_pipeline(&self.pipeline);

        render_pass.draw(0..6, 0..1);
    }
}

struct BlurPipeline {
    buffer_texture: wgpu::Texture,
    horizontal_bind_group: wgpu::BindGroup,
    vertical_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl BlurPipeline {
    fn create(
        device: &wgpu::Device,
        input: &wgpu::Texture,
        blur_stdev: f64,
        blur_radius: u32,
    ) -> Self {
        let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());

        let buffer_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("buffer_texture"),
            size: input.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: input.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let buffer_texture_view =
            buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let blur_weights = compute_blur_weights(blur_stdev, blur_radius);
        let weights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur_weights_buffer"),
            contents: bytemuck::cast_slice(&blur_weights),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let horizontal_globals = BlurGlobals::new(input.size(), BlurDirection::Horizontal);
        let horizontal_globals_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("horizontal_globals_buffer"),
                contents: &horizontal_globals.to_bytes(),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let vertical_globals = BlurGlobals::new(input.size(), BlurDirection::Vertical);
        let vertical_globals_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertical_globals_buffer"),
                contents: &vertical_globals.to_bytes(),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let horizontal_bind_group = create_blur_bind_group(
            device,
            "horizontal_blur_bind_group",
            &bind_group_layout,
            &input_view,
            &horizontal_globals_buffer,
            &weights_buffer,
        );
        let vertical_bind_group = create_blur_bind_group(
            device,
            "vertical_blur_bind_group",
            &bind_group_layout,
            &buffer_texture_view,
            &vertical_globals_buffer,
            &weights_buffer,
        );

        let shader_module = device.create_shader_module(wgpu::include_wgsl!("blur.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[],
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
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: input.format(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            buffer_texture,
            horizontal_bind_group,
            vertical_bind_group,
            pipeline,
        }
    }

    fn draw(&self, encoder: &mut wgpu::CommandEncoder, scaled_texture: &wgpu::Texture) {
        let scaled_texture_view =
            scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer_texture_view = self
            .buffer_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut horizontal_render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("horizontal_blur_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &buffer_texture_view,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: true,
                        },
                        resolve_target: None,
                    })],
                    depth_stencil_attachment: None,
                });

            horizontal_render_pass.set_bind_group(0, &self.horizontal_bind_group, &[]);
            horizontal_render_pass.set_pipeline(&self.pipeline);

            horizontal_render_pass.draw(0..6, 0..1);
        }

        {
            let mut vertical_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vertical_blur_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scaled_texture_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
            });

            vertical_render_pass.set_bind_group(0, &self.vertical_bind_group, &[]);
            vertical_render_pass.set_pipeline(&self.pipeline);

            vertical_render_pass.draw(0..6, 0..1);
        }
    }
}

fn create_blur_bind_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    input_view: &wgpu::TextureView,
    globals_buffer: &wgpu::Buffer,
    weights_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: globals_buffer,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: weights_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    })
}

struct RenderPipeline {
    bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    pipeline: wgpu::RenderPipeline,
}

impl RenderPipeline {
    fn create(
        device: &wgpu::Device,
        input: &wgpu::Texture,
        sampler: &wgpu::Sampler,
        fs_globals_buffer: &wgpu::Buffer,
        output_format: wgpu::TextureFormat,
        scanlines: Scanlines,
    ) -> Self {
        let bind_group_layout = create_render_bind_group_layout(device);

        let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = create_render_bind_group(
            device,
            &bind_group_layout,
            &input_view,
            sampler,
            fs_globals_buffer,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = create_render_pipeline(scanlines, device, &pipeline_layout, output_format);

        Self {
            bind_group,
            pipeline_layout,
            pipeline,
        }
    }

    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        vertex_buffer: &wgpu::Buffer,
        num_vertices: u32,
        output_view: &wgpu::TextureView,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));

        render_pass.draw(0..num_vertices, 0..1);
    }
}

enum ShaderPipeline {
    Prescale(TextureScalePipeline),
    Blur(TextureScalePipeline, BlurPipeline),
}

pub struct RenderPipelineState {
    shader_pipeline: Option<ShaderPipeline>,
    render: RenderPipeline,
}

impl RenderPipelineState {
    pub fn create(
        device: &wgpu::Device,
        input: &wgpu::Texture,
        sampler: &wgpu::Sampler,
        fs_globals_buffer: &wgpu::Buffer,
        output_format: wgpu::TextureFormat,
        shader: Shader,
        scanlines: Scanlines,
    ) -> Self {
        let shader_pipeline = match shader {
            Shader::Prescale(render_scale) if render_scale.get() > 1 => Some(
                ShaderPipeline::Prescale(TextureScalePipeline::create(device, render_scale, input)),
            ),
            Shader::GaussianBlur {
                prescale_factor,
                stdev,
                radius,
            } => {
                let texture_scale = TextureScalePipeline::create(device, prescale_factor, input);
                let blur =
                    BlurPipeline::create(device, &texture_scale.scaled_texture, stdev, radius);
                Some(ShaderPipeline::Blur(texture_scale, blur))
            }
            _ => None,
        };

        let render_input = match &shader_pipeline {
            Some(
                ShaderPipeline::Prescale(texture_scale) | ShaderPipeline::Blur(texture_scale, _),
            ) => &texture_scale.scaled_texture,
            None => input,
        };
        let render = RenderPipeline::create(
            device,
            render_input,
            sampler,
            fs_globals_buffer,
            output_format,
            scanlines,
        );

        Self {
            shader_pipeline,
            render,
        }
    }

    pub fn recreate_render_pipeline(
        &mut self,
        device: &wgpu::Device,
        scanlines: Scanlines,
        output_format: wgpu::TextureFormat,
    ) {
        self.render.pipeline = create_render_pipeline(
            scanlines,
            device,
            &self.render.pipeline_layout,
            output_format,
        );
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        vertex_buffer: &wgpu::Buffer,
        num_vertices: u32,
        output_view: &wgpu::TextureView,
    ) {
        match &self.shader_pipeline {
            Some(ShaderPipeline::Prescale(texture_scale)) => {
                texture_scale.draw(encoder);
            }
            Some(ShaderPipeline::Blur(texture_scale, blur)) => {
                texture_scale.draw(encoder);
                blur.draw(encoder, &texture_scale.scaled_texture);
            }
            None => {}
        }

        self.render
            .draw(encoder, vertex_buffer, num_vertices, output_view);
    }
}

fn create_render_pipeline(
    scanlines: Scanlines,
    device: &wgpu::Device,
    render_pipeline_layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader_module = device.create_shader_module(wgpu::include_wgsl!("render.wgsl"));

    let fs_main = match scanlines {
        Scanlines::None => "basic_fs",
        Scanlines::Black => "black_scanlines_fs",
        Scanlines::Dim => "dim_scanlines_fs",
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render_pipeline"),
        layout: Some(render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader_module,
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
            module: &shader_module,
            entry_point: fs_main,
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    })
}

fn create_render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_render_bind_group(
    device: &wgpu::Device,
    render_bind_group_layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    fs_globals_buffer: &wgpu::Buffer,
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
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: fs_globals_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validaate_blur_globals_size() {
        let blur_globals = BlurGlobals::new(wgpu::Extent3d::default(), BlurDirection::Horizontal);
        let _: [u8; BlurGlobals::SIZE] = blur_globals.to_bytes();
    }

    #[test]
    fn validate_fragment_globals_size() {
        let _: [u8; FragmentGlobals::SIZE] = FragmentGlobals::default().to_bytes();

        assert_eq!(FragmentGlobals::SIZE % 16, 0);
    }
}
