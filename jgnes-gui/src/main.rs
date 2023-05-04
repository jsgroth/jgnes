use eframe::NativeOptions;
use env_logger::Env;
use jgnes_gui::App;
use std::path::PathBuf;
use std::str::FromStr;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // TODO configurability
    let config_path = PathBuf::from_str("jgnes-config.toml").unwrap();

    let options = NativeOptions::default();

    eframe::run_native(
        "jgnes",
        options,
        Box::new(|_cc| Box::new(App::new(config_path))),
    )
}
