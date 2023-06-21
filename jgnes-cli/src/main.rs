use clap::Parser;
use env_logger::Env;
use jgnes_native_driver::{
    InputConfig, JgnesDynamicConfig, JgnesNativeConfig, JgnesSharedConfig, NativeRenderer,
};
use jgnes_proc_macros::{EnumDisplay, EnumFromStr};
use jgnes_renderer::config::{AspectRatio, GpuFilterMode, Overscan, VSyncMode, WgpuBackend};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumDisplay, EnumFromStr)]
enum GpuFilterType {
    NearestNeighbor,
    Linear,
}

#[derive(Parser)]
struct CliArgs {
    /// Path to iNES / NES 2.0 ROM file
    #[arg(short = 'f', long)]
    nes_file_path: String,

    /// Window width in pixels
    #[arg(short = 'w', long, default_value_t = 878)]
    window_width: u32,

    /// Window height in pixels
    #[arg(short = 'l', long, default_value_t = 672)]
    window_height: u32,

    /// Renderer (Sdl2 / Wgpu)
    #[arg(long, default_value_t)]
    renderer: NativeRenderer,

    /// Wgpu backend (Auto / Vulkan / Direct3d12 / Metal)
    #[arg(long, default_value_t)]
    wgpu_backend: WgpuBackend,

    /// GPU filter type (NearestNeighbor / Linear)
    #[arg(long, default_value_t = GpuFilterType::Linear)]
    gpu_filter_type: GpuFilterType,

    /// Internal resolution scale (1 to 16, only applicable to Wgpu renderer w/ linear filter mode)
    #[arg(long, default_value_t = 3)]
    gpu_render_scale: u32,

    /// Aspect ratio (Ntsc / Pal / SquarePixels / FourThree / Stretched)
    #[arg(long, default_value_t)]
    aspect_ratio: AspectRatio,

    /// Enable forced integer scaling for height
    #[arg(long, default_value_t)]
    forced_integer_height_scaling: bool,

    /// Emulate PAL black border
    #[arg(long, default_value_t)]
    pal_black_border: bool,

    /// Disable audio sync
    #[arg(long = "no-audio-sync", default_value_t = true, action = clap::ArgAction::SetFalse)]
    sync_to_audio: bool,

    /// Silence the triangle wave channel when it is outputting waves at ultrasonic frequencies
    #[arg(long, default_value_t)]
    silence_ultrasonic_triangle_output: bool,

    /// Launch in fullscreen
    #[arg(long = "fullscreen", default_value_t)]
    launch_fullscreen: bool,

    /// VSync mode (Enabled / Disabled / Fast)
    #[arg(long, default_value_t)]
    vsync_mode: VSyncMode,

    /// Left overscan in pixels
    #[arg(long, default_value_t)]
    overscan_left: u8,

    /// Fast forward multiplier
    #[arg(long, default_value_t = 2)]
    fast_forward_multiplier: u8,

    /// Rewind buffer length in seconds
    #[arg(long, default_value_t = 10)]
    rewind_buffer_len_secs: u64,

    /// Right overscan in pixels
    #[arg(long, default_value_t)]
    overscan_right: u8,

    /// Top overscan in pixels
    #[arg(long, default_value_t)]
    overscan_top: u8,

    /// Bottom overscan in pixels
    #[arg(long, default_value_t)]
    overscan_bottom: u8,
}

impl CliArgs {
    fn overscan(&self) -> Overscan {
        Overscan {
            top: self.overscan_top,
            left: self.overscan_left,
            right: self.overscan_right,
            bottom: self.overscan_bottom,
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = CliArgs::parse();

    let gpu_filter_mode = match args.gpu_filter_type {
        GpuFilterType::NearestNeighbor => GpuFilterMode::NearestNeighbor,
        GpuFilterType::Linear => {
            let render_scale = args
                .gpu_render_scale
                .try_into()
                .expect("invalid GPU render scale");
            GpuFilterMode::Linear(render_scale)
        }
    };

    let overscan = args.overscan();
    let (shared_config, _) = JgnesSharedConfig::new(JgnesDynamicConfig {
        gpu_filter_mode,
        aspect_ratio: args.aspect_ratio,
        overscan,
        forced_integer_height_scaling: args.forced_integer_height_scaling,
        vsync_mode: args.vsync_mode,
        pal_black_border: args.pal_black_border,
        sync_to_audio: args.sync_to_audio,
        silence_ultrasonic_triangle_output: args.silence_ultrasonic_triangle_output,
        fast_forward_multiplier: args.fast_forward_multiplier,
        rewind_buffer_len: Duration::from_secs(args.rewind_buffer_len_secs),
        input_config: InputConfig::default(),
    });
    let config = JgnesNativeConfig {
        nes_file_path: args.nes_file_path,
        window_width: args.window_width,
        window_height: args.window_height,
        renderer: args.renderer,
        wgpu_backend: args.wgpu_backend,
        launch_fullscreen: args.launch_fullscreen,
        shared_config,
    };

    jgnes_native_driver::run(&config)
}
