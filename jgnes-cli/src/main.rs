use clap::Parser;
use env_logger::Env;
use jgnes_native_driver::{
    AspectRatio, GpuFilterMode, JgnesDynamicConfig, JgnesNativeConfig, NativeRenderer, Overscan,
};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GpuFilterType {
    NearestNeighbor,
    Linear,
}

impl Display for GpuFilterType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NearestNeighbor => write!(f, "NearestNeighbor"),
            Self::Linear => write!(f, "Linear"),
        }
    }
}

impl FromStr for GpuFilterType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NearestNeighbor" => Ok(Self::NearestNeighbor),
            "Linear" => Ok(Self::Linear),
            _ => Err(anyhow::Error::msg(format!(
                "invalid GPU filter type string: {s}"
            ))),
        }
    }
}

#[derive(Parser)]
struct CliArgs {
    /// Path to iNES / NES 2.0 ROM file
    #[arg(short = 'f', long)]
    nes_file_path: String,

    /// Window width in pixels
    #[arg(short = 'w', long, default_value_t = 3 * 256)]
    window_width: u32,

    /// Window height in pixels
    #[arg(short = 'l', long, default_value_t = 3 * 224)]
    window_height: u32,

    /// Renderer (Sdl2 / Wgpu)
    #[arg(long, default_value_t = NativeRenderer::Sdl2)]
    renderer: NativeRenderer,

    /// GPU filter type (NearestNeighbor / Linear)
    #[arg(long, default_value_t = GpuFilterType::Linear)]
    gpu_filter_type: GpuFilterType,

    /// Internal resolution scale (1 to 16, only applicable to Wgpu renderer w/ linear filter mode)
    #[arg(long, default_value_t = 3)]
    gpu_render_scale: u32,

    /// Aspect ratio (Ntsc / SquarePixels / FourThree / Stretched)
    #[arg(long, default_value_t = AspectRatio::SquarePixels)]
    aspect_ratio: AspectRatio,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = CliArgs::parse();

    let gpu_filter_mode = match args.gpu_filter_type {
        GpuFilterType::NearestNeighbor => GpuFilterMode::NearestNeighbor,
        GpuFilterType::Linear => {
            let render_scale = args.gpu_render_scale.try_into()?;
            GpuFilterMode::Linear(render_scale)
        }
    };

    let config = JgnesNativeConfig {
        nes_file_path: args.nes_file_path,
        window_width: args.window_width,
        window_height: args.window_height,
        renderer: args.renderer,
        gpu_filter_mode,
        aspect_ratio: args.aspect_ratio,
        overscan: Overscan::default(),
    };

    let dynamic_config = JgnesDynamicConfig {
        quit_signal: Arc::new(AtomicBool::new(false)),
    };

    jgnes_native_driver::run(&config, dynamic_config)
}
