use jgnes_proc_macros::{EnumDisplay, EnumFromStr};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumDisplay, EnumFromStr,
)]
pub enum WgpuBackend {
    #[default]
    Auto,
    Vulkan,
    Direct3d12,
    Metal,
    BrowserAuto,
    WebGpu,
    OpenGl,
}

impl WgpuBackend {
    pub(crate) fn to_wgpu_backends(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::PRIMARY,
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Direct3d12 => wgpu::Backends::DX12,
            Self::Metal => wgpu::Backends::METAL,
            Self::BrowserAuto => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            Self::WebGpu => wgpu::Backends::BROWSER_WEBGPU,
            Self::OpenGl => wgpu::Backends::GL,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumDisplay, EnumFromStr,
)]
pub enum AspectRatio {
    #[default]
    Ntsc,
    Pal,
    SquarePixels,
    FourThree,
    Stretched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Overscan {
    pub top: u8,
    pub left: u8,
    pub right: u8,
    pub bottom: u8,
}

impl Display for Overscan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Overscan[Top={}, Left={}, Bottom={}, Right={}]",
            self.top, self.left, self.bottom, self.right
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumDisplay, EnumFromStr,
)]
pub enum VSyncMode {
    #[default]
    Enabled,
    Disabled,
    Fast,
}

impl VSyncMode {
    pub(crate) fn to_present_mode(self) -> wgpu::PresentMode {
        match self {
            Self::Enabled => wgpu::PresentMode::Fifo,
            Self::Disabled => wgpu::PresentMode::Immediate,
            Self::Fast => wgpu::PresentMode::Mailbox,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderScale(u32);

impl RenderScale {
    pub const ONE: Self = Self(1);
    pub const TWO: Self = Self(2);
    pub const THREE: Self = Self(3);

    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for RenderScale {
    fn default() -> Self {
        Self::THREE
    }
}

impl TryFrom<u32> for RenderScale {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1..=16 => Ok(Self(value)),
            _ => Err(format!("Invalid render scale value: {value}")),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumDisplay, EnumFromStr,
)]
pub enum GpuFilterMode {
    #[default]
    NearestNeighbor,
    LinearInterpolation,
}

impl GpuFilterMode {
    pub(crate) fn to_wgpu_filter_mode(self) -> wgpu::FilterMode {
        match self {
            Self::NearestNeighbor => wgpu::FilterMode::Nearest,
            Self::LinearInterpolation => wgpu::FilterMode::Linear,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrescalingMode {
    Gpu(RenderScale),
    Cpu(RenderScale),
}

impl PrescalingMode {
    #[must_use]
    pub fn gpu_render_scale(self) -> u32 {
        match self {
            Self::Gpu(render_scale) => render_scale.get(),
            Self::Cpu(_) => 1,
        }
    }

    #[must_use]
    pub fn cpu_render_scale(self) -> u32 {
        match self {
            Self::Gpu(_) => 1,
            Self::Cpu(render_scale) => render_scale.get(),
        }
    }
}

impl Default for PrescalingMode {
    fn default() -> Self {
        Self::Gpu(RenderScale::ONE)
    }
}

impl Display for PrescalingMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu(render_scale) => write!(f, "GPU {}x", render_scale.get()),
            Self::Cpu(render_scale) => write!(f, "CPU {}x", render_scale.get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSkip(pub u8);

impl FrameSkip {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub fn should_skip(self, frame_count: u64) -> bool {
        self.0 != 0 && frame_count % (u64::from(self.0) + 1) != 0
    }
}

#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub vsync_mode: VSyncMode,
    pub wgpu_backend: WgpuBackend,
    pub gpu_filter_mode: GpuFilterMode,
    pub prescaling_mode: PrescalingMode,
    pub aspect_ratio: AspectRatio,
    pub overscan: Overscan,
    pub frame_skip: FrameSkip,
    pub forced_integer_height_scaling: bool,
    pub use_webgl2_limits: bool,
}
