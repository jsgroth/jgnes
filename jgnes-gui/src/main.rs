use eframe::NativeOptions;
use env_logger::Env;
use jgnes_gui::App;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let options = NativeOptions::default();

    eframe::run_native("jgnes", options, Box::new(|_cc| Box::new(App::new())))
}
