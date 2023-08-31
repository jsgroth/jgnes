// The generated Copy impl for Vertex2d violates this rule for some reason
#![allow(clippy::let_underscore_untyped)]

mod shaders;

use crate::config::{FrameSkip, GpuFilterMode, RendererConfig, Scanlines, VSyncMode, WgpuBackend};
use crate::renderer::shaders::{FragmentGlobals, RenderPipelineState};
use crate::{colors, DisplayArea};
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
    Vertex2d { position: [-1.0, 1.0], texture_coords: [0.0, 0.0] },
    Vertex2d { position: [-1.0, -1.0], texture_coords: [0.0, 1.0] },
    Vertex2d { position: [1.0, -1.0], texture_coords: [1.0, 1.0] },
    Vertex2d { position: [1.0, -1.0], texture_coords: [1.0, 1.0] },
    Vertex2d { position: [1.0, 1.0], texture_coords: [1.0, 0.0] },
    Vertex2d { position: [-1.0, 1.0], texture_coords: [0.0, 0.0] },
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
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    surface_capabilities: wgpu::SurfaceCapabilities,
    surface_config: wgpu::SurfaceConfiguration,
    texture: wgpu::Texture,
    texture_format: wgpu::TextureFormat,
    render_pipeline_state: RenderPipelineState,
    vertices: Vec<Vertex2d>,
    vertex_buffer: wgpu::Buffer,
    fs_globals: FragmentGlobals,
    fs_globals_buffer: wgpu::Buffer,
    frame_skip: FrameSkip,
    total_frames: u64,
    // SAFETY: The window must be declared after the surface so that it is not dropped before the
    // surface is dropped
    window: W,
    window_size_fn: WindowSizeFn<W>,
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

        log::info!("Using GPU adapter with backend {:?}", adapter.get_info().backend);

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

        if !surface_capabilities.present_modes.contains(&desired_present_mode) {
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

        let texture = create_texture(&device, texture_format, timing_mode);
        let sampler = create_sampler(&device, render_config.gpu_filter_mode);

        let display_area = crate::determine_display_area(
            window_width,
            window_height,
            render_config.aspect_ratio,
            render_config.forced_integer_height_scaling,
            timing_mode,
        );

        let vertices = compute_vertices(window_width, window_height, display_area);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let fs_globals = FragmentGlobals::new(display_area, timing_mode);
        let fs_globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fs_globals_buffer"),
            size: FragmentGlobals::SIZE as u64,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let render_pipeline_state = RenderPipelineState::create(
            &device,
            &texture,
            &sampler,
            &fs_globals_buffer,
            surface_format,
            render_config.shader,
            render_config.scanlines,
        );

        Ok(Self {
            render_config,
            timing_mode,
            output_buffer,
            device,
            queue,
            surface,
            surface_capabilities,
            surface_config,
            texture,
            texture_format,
            render_pipeline_state,
            vertices,
            vertex_buffer,
            fs_globals,
            fs_globals_buffer,
            frame_skip: FrameSkip::ZERO,
            total_frames: 0,
            window,
            window_size_fn,
        })
    }

    pub fn window(&self) -> &W {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut W {
        &mut self.window
    }

    pub fn wgpu_backend(&self) -> WgpuBackend {
        self.render_config.wgpu_backend
    }

    pub fn reconfigure_surface(&mut self) {
        let (window_width, window_height) = (self.window_size_fn)(&self.window);
        self.surface_config.width = window_width;
        self.surface_config.height = window_height;

        self.surface.configure(&self.device, &self.surface_config);

        let display_area = crate::determine_display_area(
            window_width,
            window_height,
            self.render_config.aspect_ratio,
            self.render_config.forced_integer_height_scaling,
            self.timing_mode,
        );

        self.vertices = compute_vertices(window_width, window_height, display_area);
        self.fs_globals = FragmentGlobals::new(display_area, self.timing_mode);
    }

    /// Update the rendering config. The `wgpu_backend` and `use_webgl2_limits` fields in the input
    /// config will be ignored, but all other fields will be updated and immediately applied.
    ///
    /// # Errors
    ///
    /// This method will return an error if VSync mode is updated and the driver/configuration does
    /// not support the new VSync mode.
    pub fn update_render_config(
        &mut self,
        render_config: RendererConfig,
    ) -> Result<(), WgpuRendererError> {
        let new_config = RendererConfig {
            wgpu_backend: self.render_config.wgpu_backend,
            use_webgl2_limits: self.render_config.use_webgl2_limits,
            ..render_config
        };

        if new_config != self.render_config {
            self.update_vsync_mode(new_config.vsync_mode)?;
            self.update_scanlines(new_config.scanlines);

            self.render_config = new_config;

            self.reinit_textures();
            self.reconfigure_surface();
        }

        Ok(())
    }

    pub fn update_frame_skip(&mut self, frame_skip: FrameSkip) {
        self.frame_skip = frame_skip;
    }

    fn update_scanlines(&mut self, scanlines: Scanlines) {
        if scanlines != self.render_config.scanlines {
            self.render_config.scanlines = scanlines;

            self.render_pipeline_state.recreate_render_pipeline(
                &self.device,
                scanlines,
                self.surface_config.format,
            );
        }
    }

    fn reinit_textures(&mut self) {
        let sampler = create_sampler(&self.device, self.render_config.gpu_filter_mode);

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

        self.render_pipeline_state = RenderPipelineState::create(
            &self.device,
            &self.texture,
            &sampler,
            &self.fs_globals_buffer,
            self.surface_config.format,
            self.render_config.shader,
            self.render_config.scanlines,
        );
    }

    fn update_vsync_mode(&mut self, vsync_mode: VSyncMode) -> Result<(), WgpuRendererError> {
        if vsync_mode == self.render_config.vsync_mode {
            return Ok(());
        }

        let present_mode = vsync_mode.to_present_mode();
        if !self.surface_capabilities.present_modes.contains(&present_mode) {
            return Err(WgpuRendererError::msg(unsupported_vsync_mode_error(
                vsync_mode,
                &self.surface_capabilities.present_modes,
            )));
        }

        self.render_config.vsync_mode = vsync_mode;
        self.surface_config.present_mode = present_mode;

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

fn create_texture(
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
    timing_mode: TimingMode,
) -> wgpu::Texture {
    let texture_size = wgpu::Extent3d {
        width: jgnes_core::SCREEN_WIDTH.into(),
        height: timing_mode.visible_screen_height().into(),
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

fn create_sampler(device: &wgpu::Device, filter_mode: GpuFilterMode) -> wgpu::Sampler {
    let sampler_filter_mode = filter_mode.to_wgpu_filter_mode();
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

#[allow(clippy::float_cmp)]
fn compute_vertex_position(
    position: f32,
    window_length: u32,
    display_area_pos: u32,
    display_area_length: u32,
) -> f32 {
    assert!(position == 1.0_f32 || position == -1.0_f32);

    let new_position = if position.is_sign_positive() {
        f64::from(display_area_pos + display_area_length) / f64::from(window_length) * 2.0 - 1.0
    } else {
        f64::from(display_area_pos) / f64::from(window_length) * 2.0 - 1.0
    };
    new_position as f32
}

fn compute_vertices(
    window_width: u32,
    window_height: u32,
    display_area: DisplayArea,
) -> Vec<Vertex2d> {
    VERTICES
        .into_iter()
        .map(|vertex| Vertex2d {
            position: [
                compute_vertex_position(
                    vertex.position[0],
                    window_width,
                    display_area.x,
                    display_area.width,
                ),
                compute_vertex_position(
                    vertex.position[1],
                    window_height,
                    display_area.y,
                    display_area.height,
                ),
            ],
            texture_coords: vertex.texture_coords,
        })
        .collect()
}

impl<W: HasRawDisplayHandle + HasRawWindowHandle> Renderer for WgpuRenderer<W> {
    type Err = WgpuRendererError;

    fn render_frame(
        &mut self,
        frame_buffer: &FrameBuffer,
        color_emphasis: ColorEmphasis,
    ) -> Result<(), Self::Err> {
        self.total_frames += 1;

        if self.frame_skip.should_skip(self.total_frames) {
            return Ok(());
        }

        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));

        self.queue.write_buffer(&self.fs_globals_buffer, 0, &self.fs_globals.to_bytes());

        colors::to_rgba(
            frame_buffer,
            color_emphasis,
            self.render_config.overscan,
            self.timing_mode,
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
                bytes_per_row: Some(4 * u32::from(jgnes_core::SCREEN_WIDTH)),
                rows_per_image: Some(u32::from(self.timing_mode.visible_screen_height())),
            },
            self.texture.size(),
        );

        let output = self.surface.get_current_texture()?;
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("command_encoder"),
        });

        self.render_pipeline_state.draw(
            &mut encoder,
            &self.vertex_buffer,
            VERTICES.len() as u32,
            &surface_view,
        );

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
