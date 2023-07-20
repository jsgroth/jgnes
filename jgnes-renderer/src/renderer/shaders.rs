use crate::config::{PrescalingMode, RenderScale, Scanlines, Shader};
use crate::renderer::Vertex2d;
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

struct ComputePipeline {
    label: String,
    bind_groups: Vec<wgpu::BindGroup>,
    pipeline: wgpu::ComputePipeline,
    workgroups: (u32, u32, u32),
}

impl ComputePipeline {
    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&self.label),
        });

        compute_pass.set_pipeline(&self.pipeline);
        for (group_idx, bind_group) in self.bind_groups.iter().enumerate() {
            compute_pass.set_bind_group(group_idx as u32, bind_group, &[]);
        }

        compute_pass.dispatch_workgroups(self.workgroups.0, self.workgroups.1, self.workgroups.2);
    }
}

pub struct ComputePipelineState {
    render_texture: Option<(wgpu::Texture, wgpu::TextureView)>,
    compute_pipelines: Vec<ComputePipeline>,
}

impl ComputePipelineState {
    fn none() -> Self {
        Self {
            render_texture: None,
            compute_pipelines: vec![],
        }
    }

    pub fn create(
        shader: Shader,
        timing_mode: TimingMode,
        device: &wgpu::Device,
        input_texture_view: &wgpu::TextureView,
    ) -> Self {
        match shader {
            Shader::Prescale(PrescalingMode::Gpu, render_scale) if render_scale.get() > 1 => {
                let gpu_render_scale = render_scale.get();
                create_prescale_compute_pipeline(
                    gpu_render_scale,
                    timing_mode,
                    device,
                    input_texture_view,
                )
            }
            Shader::GaussianBlur {
                prescale_factor,
                stdev,
                radius,
            } => create_blur_compute_pipeline(
                device,
                timing_mode,
                prescale_factor,
                stdev,
                radius,
                input_texture_view,
            ),
            _ => Self::none(),
        }
    }

    pub fn get_render_texture<'a>(
        &'a self,
        input_texture_view: &'a wgpu::TextureView,
    ) -> &'a wgpu::TextureView {
        self.render_texture
            .as_ref()
            .map_or(input_texture_view, |(_, view)| view)
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        for compute_pipeline in &self.compute_pipelines {
            compute_pipeline.dispatch(encoder);
        }
    }
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

fn create_compute_bind_group_layout(
    device: &wgpu::Device,
    label: &str,
    output_texture_format: wgpu::TextureFormat,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
                    format: output_texture_format,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}

fn create_compute_bind_group(
    device: &wgpu::Device,
    label: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
    input_texture: &wgpu::TextureView,
    output_texture: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_texture),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(output_texture),
            },
        ],
    })
}

fn create_blur_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_blur_bind_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    weights_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: globals_buffer,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: weights_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    })
}

fn create_prescale_compute_pipeline(
    gpu_render_scale: u32,
    timing_mode: TimingMode,
    device: &wgpu::Device,
    input_texture_view: &wgpu::TextureView,
) -> ComputePipelineState {
    let scaled_texture = create_scaled_texture(device, gpu_render_scale, timing_mode);
    let scaled_texture_view = scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group_layout = create_compute_bind_group_layout(
        device,
        "prescale_bind_group_layout",
        scaled_texture.format(),
    );
    let bind_group = create_compute_bind_group(
        device,
        "prescale_bind_group",
        &bind_group_layout,
        input_texture_view,
        &scaled_texture_view,
    );

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("compute_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let shader_module = device.create_shader_module(wgpu::include_wgsl!("texture_scale.wgsl"));
    let entry_point = format!("texture_scale_{gpu_render_scale}x");
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("compute_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: &entry_point,
    });

    let workgroups = (
        jgnes_core::SCREEN_WIDTH.into(),
        timing_mode.visible_screen_height().into(),
        1,
    );

    ComputePipelineState {
        render_texture: Some((scaled_texture, scaled_texture_view)),
        compute_pipelines: vec![ComputePipeline {
            label: "scale_pipeline".into(),
            bind_groups: vec![bind_group],
            pipeline,
            workgroups,
        }],
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

fn create_blur_compute_pipeline(
    device: &wgpu::Device,
    timing_mode: TimingMode,
    prescale_factor: RenderScale,
    blur_stdev: f64,
    blur_radius: u32,
    input_texture_view: &wgpu::TextureView,
) -> ComputePipelineState {
    let prescale_pipeline_state = create_prescale_compute_pipeline(
        prescale_factor.get(),
        timing_mode,
        device,
        input_texture_view,
    );

    assert_eq!(prescale_pipeline_state.compute_pipelines.len(), 1);
    let prescale_pipeline = prescale_pipeline_state
        .compute_pipelines
        .into_iter()
        .next()
        .unwrap();
    let (scaled_texture, scaled_texture_view) = prescale_pipeline_state.render_texture.unwrap();

    let buffer_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("buffer_texture"),
        size: scaled_texture.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let buffer_texture_view = buffer_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let shader_module = device.create_shader_module(wgpu::include_wgsl!("blur.wgsl"));
    let workgroups = (
        scaled_texture.size().width / 16,
        scaled_texture.size().height / 16,
        1,
    );

    let blur_weights = compute_blur_weights(blur_stdev, blur_radius);
    let weights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("blur_weights_buffer"),
        contents: bytemuck::cast_slice(&blur_weights),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
    });

    let horizontal_globals = BlurGlobals::new(scaled_texture.size(), BlurDirection::Horizontal);
    let horizontal_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("horizontal_globals_buffer"),
        contents: &horizontal_globals.to_bytes(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
    });

    let globals_bind_group_layout =
        create_blur_bind_group_layout(device, "compute_globals_bind_group_layout");

    let horizontal_bind_group_layout = create_compute_bind_group_layout(
        device,
        "horizontal_blur_bind_group_layout",
        scaled_texture.format(),
    );
    let horizontal_bind_group = create_compute_bind_group(
        device,
        "horizontal_blur_bind_group",
        &horizontal_bind_group_layout,
        &scaled_texture_view,
        &buffer_texture_view,
    );

    let horizontal_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("horizontal_blur_pipeline_layout"),
            bind_group_layouts: &[&horizontal_bind_group_layout, &globals_bind_group_layout],
            push_constant_ranges: &[],
        });
    let horizontal_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("horizontal_blur_pipeline"),
        layout: Some(&horizontal_pipeline_layout),
        module: &shader_module,
        entry_point: "blur_fs",
    });

    let horizontal_globals_bind_group = create_blur_bind_group(
        device,
        "compute_globals_bind_groups",
        &globals_bind_group_layout,
        &horizontal_globals_buffer,
        &weights_buffer,
    );
    let horizontal_compute_pipeline = ComputePipeline {
        label: "horizontal_blur".into(),
        bind_groups: vec![horizontal_bind_group, horizontal_globals_bind_group],
        pipeline: horizontal_pipeline,
        workgroups,
    };

    let vertical_bind_group_layout = create_compute_bind_group_layout(
        device,
        "vertical_blur_bind_group_layout",
        scaled_texture.format(),
    );
    let vertical_bind_group = create_compute_bind_group(
        device,
        "vertical_blur_bind_group",
        &vertical_bind_group_layout,
        &buffer_texture_view,
        &scaled_texture_view,
    );

    let vertical_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vertical_blur_pipeline_layout"),
        bind_group_layouts: &[&vertical_bind_group_layout, &globals_bind_group_layout],
        push_constant_ranges: &[],
    });
    let vertical_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("vertical_blur_pipeline"),
        layout: Some(&vertical_pipeline_layout),
        module: &shader_module,
        entry_point: "blur_fs",
    });

    let vertical_globals = BlurGlobals::new(scaled_texture.size(), BlurDirection::Vertical);
    let vertical_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertical_globals_buffer"),
        contents: &vertical_globals.to_bytes(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
    });

    let vertical_globals_bind_group = create_blur_bind_group(
        device,
        "compute_globals_bind_groups",
        &globals_bind_group_layout,
        &vertical_globals_buffer,
        &weights_buffer,
    );
    let vertical_compute_pipeline = ComputePipeline {
        label: "vertical_blur".into(),
        bind_groups: vec![vertical_bind_group, vertical_globals_bind_group],
        pipeline: vertical_pipeline,
        workgroups,
    };

    let compute_pipelines = vec![
        prescale_pipeline,
        horizontal_compute_pipeline,
        vertical_compute_pipeline,
    ];

    ComputePipelineState {
        render_texture: Some((scaled_texture, scaled_texture_view)),
        compute_pipelines,
    }
}

pub struct RenderPipelineState {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    pipeline: wgpu::RenderPipeline,
}

impl RenderPipelineState {
    pub fn create(
        scanlines: Scanlines,
        device: &wgpu::Device,
        input_texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        fs_globals_buffer: &wgpu::Buffer,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let bind_group_layout = create_render_bind_group_layout(device);
        let bind_group = create_render_bind_group(
            device,
            &bind_group_layout,
            input_texture_view,
            sampler,
            fs_globals_buffer,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = create_render_pipeline(scanlines, device, &pipeline_layout, surface_format);

        Self {
            bind_group_layout,
            bind_group,
            pipeline_layout,
            pipeline,
        }
    }

    pub fn recreate_bind_group(
        &mut self,
        device: &wgpu::Device,
        input_texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        fs_globals_buffer: &wgpu::Buffer,
    ) {
        self.bind_group = create_render_bind_group(
            device,
            &self.bind_group_layout,
            input_texture_view,
            sampler,
            fs_globals_buffer,
        );
    }

    pub fn recreate_pipeline(
        &mut self,
        shader: Scanlines,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) {
        self.pipeline =
            create_render_pipeline(shader, device, &self.pipeline_layout, surface_format);
    }

    pub fn draw(
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

fn create_render_pipeline(
    scanlines: Scanlines,
    device: &wgpu::Device,
    render_pipeline_layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

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
    fn validate_compute_globals_size() {
        let blur_globals = BlurGlobals::new(wgpu::Extent3d::default(), BlurDirection::Horizontal);
        let _: [u8; BlurGlobals::SIZE] = blur_globals.to_bytes();
    }
}
