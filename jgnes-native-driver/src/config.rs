use crate::GpuFilterMode;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeRenderer {
    Sdl2,
    Wgpu,
}

impl Display for NativeRenderer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sdl2 => write!(f, "Sdl2"),
            Self::Wgpu => write!(f, "Wgpu"),
        }
    }
}

impl FromStr for NativeRenderer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Sdl2" => Ok(Self::Sdl2),
            "Wgpu" => Ok(Self::Wgpu),
            _ => Err(format!("invalid renderer string: {s}")),
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

impl Overscan {
    pub(crate) fn validate(self) -> Result<Self, anyhow::Error> {
        if self.top > 112 || self.bottom > 112 {
            return Err(anyhow::Error::msg(format!(
                "Vertical overscan cannot be more than 112; top={}, bottom={}",
                self.top, self.bottom
            )));
        }

        if self.left > 128 || self.right > 128 {
            return Err(anyhow::Error::msg(format!(
                "Horizontal overscan cannot be more than 128; left={}, right={}",
                self.left, self.right
            )));
        }

        Ok(self)
    }
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

#[derive(Debug, Clone)]
pub struct JgnesNativeConfig {
    pub nes_file_path: String,
    pub window_width: u32,
    pub window_height: u32,
    pub renderer: NativeRenderer,
    pub gpu_filter_mode: GpuFilterMode,
    pub aspect_ratio: AspectRatio,
    pub overscan: Overscan,
    pub forced_integer_height_scaling: bool,
    pub vsync_mode: VSyncMode,
    pub sync_to_audio: bool,
    pub launch_fullscreen: bool,
}

impl Display for JgnesNativeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "nes_file_path: {}", self.nes_file_path)?;
        writeln!(f, "window_width: {}", self.window_width)?;
        writeln!(f, "window_height: {}", self.window_height)?;
        writeln!(f, "renderer: {}", self.renderer)?;
        writeln!(f, "gpu_filter_mode: {}", self.gpu_filter_mode)?;
        writeln!(f, "aspect_ratio: {}", self.aspect_ratio)?;
        writeln!(f, "overscan: {}", self.overscan)?;
        writeln!(
            f,
            "forced_integer_height_scaling: {}",
            self.forced_integer_height_scaling
        )?;
        writeln!(f, "vsync_mode: {}", self.vsync_mode)?;
        writeln!(f, "sync_to_audio: {}", self.sync_to_audio)?;
        writeln!(f, "launch_fullscreen: {}", self.launch_fullscreen)?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JgnesDynamicConfig {
    pub quit_signal: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub(crate) struct RendererConfig {
    pub(crate) vsync_mode: VSyncMode,
    pub(crate) gpu_filter_mode: GpuFilterMode,
    pub(crate) aspect_ratio: AspectRatio,
    pub(crate) overscan: Overscan,
    pub(crate) forced_integer_height_scaling: bool,
}
