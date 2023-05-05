use jgnes_native_driver::{JgnesDynamicConfig, JgnesNativeConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex};
use std::{process, thread};

#[must_use]
pub(crate) fn start(
    dynamic_config: JgnesDynamicConfig,
    is_running: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
) -> Sender<JgnesNativeConfig> {
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        // TODO maybe better way of reporting unsupported VSync mode errors than killing the process
        std::panic::set_hook(Box::new(|panic_info| {
            log::error!("Emulation thread panicked, killing process: {panic_info}");
            process::exit(1);
        }));

        loop {
            let config = match receiver.recv() {
                Ok(config) => config,
                Err(err) => {
                    log::info!("Emulation thread terminating due to recv error (most likely caused by closing main window): {err}");
                    return;
                }
            };

            is_running.store(true, Ordering::Relaxed);
            if let Err(err) = jgnes_native_driver::run(&config, dynamic_config.clone()) {
                *emulation_error.lock().unwrap() = Some(err);
            }

            is_running.store(false, Ordering::Relaxed);
            dynamic_config.quit_signal.store(false, Ordering::Relaxed);
        }
    });

    sender
}
