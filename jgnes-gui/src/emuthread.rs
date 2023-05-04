use jgnes_native_driver::{JgnesDynamicConfig, JgnesNativeConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

#[must_use]
pub(crate) fn start(
    dynamic_config: JgnesDynamicConfig,
    is_running: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
) -> Sender<JgnesNativeConfig> {
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || loop {
        let config = receiver.recv().unwrap();

        is_running.store(true, Ordering::Relaxed);
        if let Err(err) = jgnes_native_driver::run(&config, dynamic_config.clone()) {
            *emulation_error.lock().unwrap() = Some(err);
        }

        is_running.store(false, Ordering::Relaxed);
        dynamic_config.quit_signal.store(false, Ordering::Relaxed);
    });

    sender
}
