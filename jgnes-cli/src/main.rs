use clap::Parser;
use env_logger::Env;
use jgnes_native_driver::{JgnesNativeConfig, NativeRenderer};

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

    /// Renderer (Sdl2 / Vulkan)
    #[arg(long, default_value_t = NativeRenderer::Sdl2)]
    renderer: NativeRenderer,

    /// Internal resolution scale (only applicable to Vulkan renderer)
    #[arg(long, default_value_t = 3)]
    render_scale: u32,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = CliArgs::parse();

    let config = JgnesNativeConfig {
        nes_file_path: args.nes_file_path,
        window_width: args.window_width,
        window_height: args.window_height,
        renderer: args.renderer,
        render_scale: args.render_scale,
    };

    jgnes_native_driver::run(&config)
}
