use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

impl Display for WgpuBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Auto"),
            Self::Vulkan => write!(f, "Vulkan"),
            Self::Direct3d12 => write!(f, "Direct3d12"),
            Self::Metal => write!(f, "Metal"),
            Self::BrowserAuto => write!(f, "BrowserAuto"),
            Self::WebGpu => write!(f, "WebGpu"),
            Self::OpenGl => write!(f, "OpenGl"),
        }
    }
}

impl FromStr for WgpuBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Auto" => Ok(Self::Auto),
            "Vulkan" => Ok(Self::Vulkan),
            "Direct3d12" => Ok(Self::Direct3d12),
            "Metal" => Ok(Self::Metal),
            "BrowserAuto" => Ok(Self::BrowserAuto),
            "WebGpu" => Ok(Self::WebGpu),
            "OpenGl" => Ok(Self::OpenGl),
            _ => Err(format!("invalid wgpu backend string: {s}")),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AspectRatio {
    #[default]
    Ntsc,
    SquarePixels,
    FourThree,
    Stretched,
}

impl Display for AspectRatio {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ntsc => write!(f, "Ntsc"),
            Self::SquarePixels => write!(f, "SquarePixels"),
            Self::FourThree => write!(f, "FourThree"),
            Self::Stretched => write!(f, "Stretched"),
        }
    }
}

impl FromStr for AspectRatio {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Ntsc" => Ok(Self::Ntsc),
            "SquarePixels" => Ok(Self::SquarePixels),
            "FourThree" => Ok(Self::FourThree),
            "Stretched" => Ok(Self::Stretched),
            _ => Err(format!("invalid aspect ratio string: {s}")),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VSyncMode {
    #[default]
    Enabled,
    Disabled,
    Fast,
    Adaptive,
}

impl VSyncMode {
    pub(crate) fn to_present_mode(self) -> wgpu::PresentMode {
        match self {
            Self::Enabled => wgpu::PresentMode::Fifo,
            Self::Disabled => wgpu::PresentMode::Immediate,
            Self::Fast => wgpu::PresentMode::Mailbox,
            Self::Adaptive => wgpu::PresentMode::FifoRelaxed,
        }
    }
}

impl Display for VSyncMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "Enabled"),
            Self::Disabled => write!(f, "Disabled"),
            Self::Fast => write!(f, "Fast"),
            Self::Adaptive => write!(f, "Adaptive"),
        }
    }
}

impl FromStr for VSyncMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Enabled" => Ok(Self::Enabled),
            "Disabled" => Ok(Self::Disabled),
            "Fast" => Ok(Self::Fast),
            "Adaptive" => Ok(Self::Adaptive),
            _ => Err(format!("invalid VSync mode string: {s}")),
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
    LinearCpuScaled(RenderScale),
}

impl GpuFilterMode {
    #[must_use]
    pub fn cpu_render_scale(self) -> u32 {
        match self {
            Self::NearestNeighbor | Self::Linear(_) => 1,
            Self::LinearCpuScaled(render_scale) => render_scale.get(),
        }
    }

    #[must_use]
    pub fn gpu_render_scale(self) -> u32 {
        match self {
            Self::NearestNeighbor | Self::LinearCpuScaled(_) => 1,
            Self::Linear(render_scale) => render_scale.get(),
        }
    }
}

impl Display for GpuFilterMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NearestNeighbor => write!(f, "NearestNeighbor"),
            Self::Linear(render_scale) => write!(f, "Linear {}x", render_scale.0),
            Self::LinearCpuScaled(render_scale) => {
                write!(f, "Linear {}x (CPU scaled)", render_scale.0)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub vsync_mode: VSyncMode,
    pub wgpu_backend: WgpuBackend,
    pub gpu_filter_mode: GpuFilterMode,
    pub aspect_ratio: AspectRatio,
    pub overscan: Overscan,
    pub forced_integer_height_scaling: bool,
    pub use_webgl2_limits: bool,
}
