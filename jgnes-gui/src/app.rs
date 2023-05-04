use crate::emuthread;
use eframe::Frame;
use egui::panel::TopBottomSide;
use egui::{
    menu, Button, Color32, Context, Key, KeyboardShortcut, Modifiers, TopBottomPanel, Widget,
    Window,
};
use jgnes_native_driver::{
    GpuFilterMode, JgnesDynamicConfig, JgnesNativeConfig, NativeRenderer, RenderScale,
};
use rfd::FileDialog;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

struct AppConfig;

impl AppConfig {
    fn new() -> Self {
        Self
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindow {
    EmulationError,
}

struct AppState {
    open_window: Option<OpenWindow>,
    emulator_is_running: Arc<AtomicBool>,
    emulator_quit_signal: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
    emu_thread_sender: Sender<JgnesNativeConfig>,
}

impl AppState {
    fn new() -> Self {
        let is_running = Arc::new(AtomicBool::new(false));
        let quit_signal = Arc::new(AtomicBool::new(false));
        let emulation_error = Arc::new(Mutex::new(None));
        let emu_thread_sender = emuthread::start(
            JgnesDynamicConfig {
                quit_signal: Arc::clone(&quit_signal),
            },
            Arc::clone(&is_running),
            Arc::clone(&emulation_error),
        );
        Self {
            open_window: None,
            emulator_is_running: is_running,
            emulator_quit_signal: quit_signal,
            emulation_error,
            emu_thread_sender,
        }
    }

    fn stop_emulator_if_running(&mut self) {
        if self.emulator_is_running.load(Ordering::Relaxed) {
            log::info!("Setting quit signal to stop running emulator");
            self.emulator_quit_signal.store(true, Ordering::Relaxed);
        }
    }
}

pub struct App {
    _config: AppConfig,
    state: AppState,
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self {
            _config: AppConfig::new(),
            state: AppState::new(),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    fn handle_open(&mut self) {
        let file = FileDialog::new().add_filter("nes", &["nes"]).pick_file();
        if let Some(file) = file {
            self.state.stop_emulator_if_running();

            launch_emulator(file, &self.state.emu_thread_sender);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        if self.state.emulation_error.lock().unwrap().is_some() {
            self.state.open_window = Some(OpenWindow::EmulationError);
        }

        let open_shortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::O);
        if ctx.input_mut(|input| input.consume_shortcut(&open_shortcut)) {
            self.handle_open();
        }

        let quit_shortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Q);
        if ctx.input_mut(|input| input.consume_shortcut(&quit_shortcut)) {
            frame.close();
        }

        TopBottomPanel::new(TopBottomSide::Top, "top_bottom_panel").show(ctx, |ui| {
            ui.set_enabled(self.state.open_window.is_none());
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        self.handle_open();
                        ui.close_menu();
                    }

                    let quit_button = Button::new("Quit")
                        .shortcut_text(ctx.format_shortcut(&quit_shortcut))
                        .ui(ui);
                    if quit_button.clicked() {
                        frame.close();
                    }
                });
            });
        });

        if self.state.open_window == Some(OpenWindow::EmulationError) {
            let mut error_open = true;
            Window::new("Error")
                .resizable(false)
                .open(&mut error_open)
                .show(ctx, |ui| {
                    ui.colored_label(
                        Color32::RED,
                        self.state
                            .emulation_error
                            .lock()
                            .unwrap()
                            .as_ref()
                            .map_or(String::new(), anyhow::Error::to_string),
                    );
                });
            if !error_open {
                self.state.open_window = None;
                *self.state.emulation_error.lock().unwrap() = None;
            }
        }
    }
}

fn launch_emulator<P: AsRef<Path>>(path: P, sender: &Sender<JgnesNativeConfig>) {
    let path = path.as_ref();

    let file_path_str = path.to_string_lossy().to_string();
    sender
        .send(JgnesNativeConfig {
            nes_file_path: file_path_str,
            window_width: 640,
            window_height: 480,
            renderer: NativeRenderer::Wgpu,
            gpu_filter_mode: GpuFilterMode::Linear(RenderScale::try_from(2).unwrap()),
        })
        .unwrap();
}
