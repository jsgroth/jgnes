use crate::config::{PrescalingMode, Shader};
use crate::renderer::Vertex2d;
use jgnes_core::TimingMode;

struct ComputePipeline {
    label: String,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    workgroups: (u32, u32, u32),
}

impl ComputePipeline {
    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&self.label),
        });

        compute_pass.set_pipeline(&self.pipeline);
        compute_pass.set_bind_group(0, &self.bind_group, &[]);

        compute_pass.dispatch_workgroups(self.workgroups.0, self.workgroups.1, self.workgroups.2);
    }
}

pub struct ComputePipelineState {
    render_texture_view: Option<wgpu::TextureView>,
    compute_pipelines: Vec<ComputePipeline>,
}

impl ComputePipelineState {
    fn none() -> Self {
        Self {
            render_texture_view: None,
            compute_pipelines: vec![],
        }
    }

    pub fn create(
        prescaling_mode: PrescalingMode,
        timing_mode: TimingMode,
        device: &wgpu::Device,
        input_texture_view: &wgpu::TextureView,
    ) -> Self {
        match prescaling_mode {
            PrescalingMode::Gpu(render_scale) if render_scale.get() > 1 => {
                let gpu_render_scale = render_scale.get();
                create_prescale_compute_pipeline(
                    gpu_render_scale,
                    timing_mode,
                    device,
                    input_texture_view,
                )
            }
            _ => Self::none(),
        }
    }

    pub fn get_render_texture<'a>(
        &'a self,
        input_texture_view: &'a wgpu::TextureView,
    ) -> &'a wgpu::TextureView {
        self.render_texture_view
            .as_ref()
            .unwrap_or(input_texture_view)
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        for compute_pipeline in &self.compute_pipelines {
            compute_pipeline.dispatch(encoder);
        }
    }
}

fn create_prescale_compute_pipeline(
    gpu_render_scale: u32,
    timing_mode: TimingMode,
    device: &wgpu::Device,
    input_texture_view: &wgpu::TextureView,
) -> ComputePipelineState {
    let scaled_texture = create_scaled_texture(device, gpu_render_scale, timing_mode);
    let scaled_texture_view = scaled_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("compute_bind_group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&scaled_texture_view),
            },
        ],
    });

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
        render_texture_view: Some(scaled_texture_view),
        compute_pipelines: vec![ComputePipeline {
            label: "scale_pipeline".into(),
            bind_group,
            pipeline,
            workgroups,
        }],
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

pub struct RenderPipelineState {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline_layout: wgpu::PipelineLayout,
    pub pipeline: wgpu::RenderPipeline,
}

impl RenderPipelineState {
    pub fn create(
        shader: Shader,
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
        let pipeline = create_render_pipeline(shader, device, &pipeline_layout, surface_format);

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
        shader: Shader,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) {
        self.pipeline =
            create_render_pipeline(shader, device, &self.pipeline_layout, surface_format);
    }
}

fn create_render_pipeline(
    shader: Shader,
    device: &wgpu::Device,
    render_pipeline_layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    let fs_main = match shader {
        Shader::None => "basic_fs",
        Shader::BlackScanlines => "black_scanlines_fs",
        Shader::DimScanlines => "dim_scanlines_fs",
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
