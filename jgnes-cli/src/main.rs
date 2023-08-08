use clap::Parser;
use env_logger::Env;
use jgnes_core::TimingMode;
use jgnes_native_driver::{
    InputConfig, JgnesDynamicConfig, JgnesNativeConfig, JgnesSharedConfig, NativeRenderer,
};
use jgnes_proc_macros::{EnumDisplay, EnumFromStr};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RenderScale, Scanlines, Shader, VSyncMode, WgpuBackend,
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
enum ShaderType {
    None,
    #[default]
    Prescale,
    GaussianBlur,
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

    /// Shader type (None / Prescale / GaussianBlur)
    #[arg(long, default_value_t)]
    shader_type: ShaderType,

    /// Gaussian blur stdev for Gaussian blur shader
    #[arg(long, default_value_t = 1.5)]
    blur_stdev: f64,

    /// Gaussian blur radius for Gaussian blur shader
    #[arg(long, default_value_t = 16)]
    blur_radius: u32,

    /// Scanlines setting (None / Black / Dim)
    #[arg(long, default_value_t)]
    scanlines: Scanlines,

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

    /// Disable audio refresh rate adjustment; by default, the audio signal is downsampled so that
    /// syncing to audio will run the emulator at exactly 60FPS (NTSC) / 50FPS (PAL) instead of the
    /// NES native refresh rate (~60.1FPS for NTSC, ~50.007FPS for PAL)
    #[arg(long = "no-audio-refresh-rate-adjustment", default_value_t = true, action = clap::ArgAction::SetFalse)]
    audio_refresh_rate_adjustment: bool,

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
    fn render_scale(&self) -> RenderScale {
        RenderScale::try_from(self.render_scale).expect("render_scale arg is invalid")
    }

    fn blur_stdev(&self) -> f64 {
        assert!(
            !self.blur_stdev.is_nan() && !self.blur_stdev.is_sign_negative(),
            "Gaussian blur stdev cannot be negative or NaN"
        );
        self.blur_stdev
    }

    fn shader(&self) -> Shader {
        match self.shader_type {
            ShaderType::None => Shader::None,
            ShaderType::Prescale => Shader::Prescale(self.render_scale()),
            ShaderType::GaussianBlur => Shader::GaussianBlur {
                prescale_factor: self.render_scale(),
                stdev: self.blur_stdev(),
                radius: self.blur_radius,
            },
        }
    }

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

    let (shared_config, _) = JgnesSharedConfig::new(JgnesDynamicConfig {
        gpu_filter_mode: args.gpu_filter_mode,
        shader: args.shader(),
        scanlines: args.scanlines,
        aspect_ratio: args.aspect_ratio,
        overscan: args.overscan(),
        forced_integer_height_scaling: args.forced_integer_height_scaling,
        vsync_mode: args.vsync_mode,
        remove_sprite_limit: args.remove_sprite_limit,
        pal_black_border: args.pal_black_border,
        sync_to_audio: args.sync_to_audio,
        audio_refresh_rate_adjustment: args.audio_refresh_rate_adjustment,
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
