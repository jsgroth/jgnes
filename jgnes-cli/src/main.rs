use clap::Parser;
use env_logger::Env;
use jgnes_native_driver::JgnesNativeConfig;

#[derive(Parser)]
struct CliArgs {
    #[arg(short = 'f', long)]
    nes_file_path: String,

    #[arg(short = 'w', long, default_value_t = 3 * 256)]
    window_width: u32,

    #[arg(short = 'l', long, default_value_t = 3 * 224)]
    window_height: u32,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = CliArgs::parse();

    let config = JgnesNativeConfig {
        nes_file_path: args.nes_file_path,
        window_width: args.window_width,
        window_height: args.window_height,
    };

    jgnes_native_driver::run(&config)
}
