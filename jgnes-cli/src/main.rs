use clap::Parser;
use env_logger::Env;
use jgnes_core::TimingMode;
use jgnes_native_driver::{
    InputConfig, JgnesDynamicConfig, JgnesNativeConfig, JgnesSharedConfig, NativeRenderer,
};
use jgnes_proc_macros::{EnumDisplay, EnumFromStr};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, PrescalingMode, RenderScale, VSyncMode, WgpuBackend,
};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumDisplay, EnumFromStr)]
enum OptionalTimingMode {
    Ntsc,
    Pal,
    #[default]
    None,
}

impl OptionalTimingMode {
    fn to_timing_mode(self) -> Option<TimingMode> {
        match self {
            Self::Ntsc => Some(TimingMode::Ntsc),
            Self::Pal => Some(TimingMode::Pal),
            Self::None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumDisplay, EnumFromStr)]
enum PrescalingType {
    #[default]
    Gpu,
    Cpu,
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

    /// Wgpu backend (Auto / Vulkan / Direct3d12 / Metal / OpenGl)
    #[arg(long, default_value_t)]
    wgpu_backend: WgpuBackend,

    /// GPU filter type (NearestNeighbor / LinearInterpolation)
    #[arg(long, default_value_t = GpuFilterMode::LinearInterpolation)]
    gpu_filter_mode: GpuFilterMode,

    /// Prescaling type (Gpu / Cpu)
    #[arg(long, default_value_t)]
    prescaling_type: PrescalingType,

    /// Internal resolution prescale factor (1 to 16, only applicable to Wgpu renderer)
    #[arg(long, default_value_t = 3)]
    render_scale: u32,

    /// Aspect ratio (Ntsc / Pal / SquarePixels / FourThree / Stretched)
    #[arg(long, default_value_t)]
    aspect_ratio: AspectRatio,

    /// Force a timing mode instead of relying on cartridge header (Ntsc / Pal)
    #[arg(long, default_value_t)]
    forced_timing_mode: OptionalTimingMode,

    /// Enable forced integer scaling for height
    #[arg(long, default_value_t)]
    forced_integer_height_scaling: bool,

    /// Remove the 8 sprite per scanline limit, which eliminates sprite flickering but can cause
    /// bugs
    #[arg(long, default_value_t)]
    remove_sprite_limit: bool,

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
    env_logger::Builder::from_env(Env::default().default_filter_or("info,wgpu_core::device=warn"))
        .init();

    let args = CliArgs::parse();

    let render_scale =
        RenderScale::try_from(args.render_scale).expect("render_scale arg is invalid");
    let prescaling_mode = match args.prescaling_type {
        PrescalingType::Gpu => PrescalingMode::Gpu(render_scale),
        PrescalingType::Cpu => PrescalingMode::Cpu(render_scale),
    };

    let overscan = args.overscan();
    let (shared_config, _) = JgnesSharedConfig::new(JgnesDynamicConfig {
        gpu_filter_mode: args.gpu_filter_mode,
        prescaling_mode,
        aspect_ratio: args.aspect_ratio,
        overscan,
        forced_integer_height_scaling: args.forced_integer_height_scaling,
        vsync_mode: args.vsync_mode,
        remove_sprite_limit: args.remove_sprite_limit,
        pal_black_border: args.pal_black_border,
        sync_to_audio: args.sync_to_audio,
        silence_ultrasonic_triangle_output: args.silence_ultrasonic_triangle_output,
        fast_forward_multiplier: args.fast_forward_multiplier,
        rewind_buffer_len: Duration::from_secs(args.rewind_buffer_len_secs),
        input_config: InputConfig::default(),
    });
    let config = JgnesNativeConfig {
        nes_file_path: args.nes_file_path,
        forced_timing_mode: args.forced_timing_mode.to_timing_mode(),
        window_width: args.window_width,
        window_height: args.window_height,
        renderer: args.renderer,
        wgpu_backend: args.wgpu_backend,
        launch_fullscreen: args.launch_fullscreen,
        shared_config,
    };

    jgnes_native_driver::run(&config)
}
