use env_logger::Env;
use std::env;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut args = env::args();
    args.next();

    let path = args.next().expect("missing filename");

    jgnes_native_driver::run(&path)
}
