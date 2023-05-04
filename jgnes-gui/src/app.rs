use crate::emuthread;
use eframe::Frame;
use egui::panel::TopBottomSide;
use egui::{
    menu, Button, Color32, Context, Key, KeyboardShortcut, Modifiers, TextEdit, TopBottomPanel,
    Widget, Window,
};
use jgnes_native_driver::{
    GpuFilterMode, JgnesDynamicConfig, JgnesNativeConfig, NativeRenderer, RenderScale,
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum GpuFilterType {
    NearestNeighbor,
    Linear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AppConfig {
    window_width: u32,
    window_height: u32,
    renderer: NativeRenderer,
    gpu_filter_type: GpuFilterType,
    gpu_render_scale: RenderScale,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 960,
            window_height: 720,
            renderer: NativeRenderer::Wgpu,
            gpu_filter_type: GpuFilterType::Linear,
            gpu_render_scale: RenderScale::THREE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindow {
    VideoSettings,
    EmulationError,
}

struct AppState {
    render_scale_text: String,
    render_scale_invalid: bool,
    window_width_text: String,
    window_width_invalid: bool,
    window_height_text: String,
    window_height_invalid: bool,
    open_window: Option<OpenWindow>,
    emulator_is_running: Arc<AtomicBool>,
    emulator_quit_signal: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
    emu_thread_sender: Sender<JgnesNativeConfig>,
}

impl AppState {
    fn new(config: &AppConfig) -> Self {
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
            render_scale_text: config.gpu_render_scale.get().to_string(),
            render_scale_invalid: false,
            window_width_text: config.window_width.to_string(),
            window_width_invalid: false,
            window_height_text: config.window_height.to_string(),
            window_height_invalid: false,
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
    config_path: PathBuf,
    config: AppConfig,
    state: AppState,
}

impl App {
    #[must_use]
    pub fn new(config_path: PathBuf) -> Self {
        let config = match load_config(&config_path) {
            Ok(config) => config,
            Err(err) => {
                log::warn!(
                    "Error attempting to load config from '{}', using default: {}",
                    config_path.display(),
                    err
                );
                AppConfig::default()
            }
        };

        let state = AppState::new(&config);

        Self {
            config_path,
            config,
            state,
        }
    }

    fn handle_open(&mut self) {
        let file = FileDialog::new().add_filter("nes", &["nes"]).pick_file();
        if let Some(file) = file {
            self.state.stop_emulator_if_running();

            launch_emulator(file, &self.state.emu_thread_sender, &self.config);
        }
    }

    fn save_config(&mut self) {
        let config_str =
            toml::to_string(&self.config).expect("Config should always be serializable");
        fs::write(&self.config_path, config_str).expect("Unable to save config file");
    }

    fn render_video_settings_window(&mut self, ctx: &Context) {
        let mut video_settings_open = true;
        Window::new("Video Settings")
            .resizable(false)
            .open(&mut video_settings_open)
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.label("Renderer");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.renderer, NativeRenderer::Wgpu, "wgpu");
                        ui.radio_value(&mut self.config.renderer, NativeRenderer::Sdl2, "SDL2");
                    });
                });

                ui.group(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);

                    ui.label("Image filtering");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.gpu_filter_type, GpuFilterType::Linear, "Linear interpolation");
                        ui.radio_value(&mut self.config.gpu_filter_type, GpuFilterType::NearestNeighbor, "Nearest neighbor");
                    });
                });

                ui.horizontal(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu && self.config.gpu_filter_type == GpuFilterType::Linear);

                    if !TextEdit::singleline(&mut self.state.render_scale_text).desired_width(60.0).ui(ui).has_focus() {
                        match RenderScale::try_from(self.state.render_scale_text.parse::<u32>().unwrap_or(0)) {
                            Ok(render_scale) => {
                                self.state.render_scale_invalid = false;
                                self.config.gpu_render_scale = render_scale;
                            }
                            Err(_) => {
                                self.state.render_scale_invalid = true;
                            }
                        }
                    }
                    ui.label("Linear interpolation scaling factor")
                        .on_hover_text("The image will be integer upscaled from 256x224 by this factor before linear interpolation");
                });
                if self.state.render_scale_invalid {
                    ui.colored_label(Color32::RED, "Scaling factor must be an integer between 1 and 16");
                }

                ui.horizontal(|ui| {
                    if !TextEdit::singleline(&mut self.state.window_width_text).desired_width(60.0).ui(ui).has_focus() {
                        match self.state.window_width_text.parse::<u32>() {
                            Ok(window_width) => {
                                self.state.window_width_invalid = false;
                                self.config.window_width = window_width;
                            }
                            Err(_) => {
                                self.state.window_width_invalid = true;
                            }
                        }
                    }
                    ui.label("Window width in pixels");
                });
                if self.state.window_width_invalid {
                    ui.colored_label(Color32::RED, "Window width must be an unsigned integer");
                }

                ui.horizontal(|ui| {
                    if !TextEdit::singleline(&mut self.state.window_height_text).desired_width(60.0).ui(ui).has_focus() {
                        match self.state.window_height_text.parse::<u32>() {
                            Ok(window_height) => {
                                self.state.window_height_invalid = false;
                                self.config.window_height = window_height;
                            }
                            Err(_) => {
                                self.state.window_height_invalid = true;
                            }
                        }
                    }
                    ui.label("Window height in pixels");
                });
                if self.state.window_height_invalid {
                    ui.colored_label(Color32::RED, "Window height must be an unsigned integer");
                }
            });
        if !video_settings_open {
            self.state.open_window = None;
        }
    }
}

fn load_config(path: &PathBuf) -> Result<AppConfig, anyhow::Error> {
    let config_str = fs::read_to_string(path)?;
    Ok(toml::from_str(&config_str)?)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        let prev_config = self.config.clone();

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

                ui.menu_button("Settings", |ui| {
                    if ui.button("Video").clicked() {
                        self.state.open_window = Some(OpenWindow::VideoSettings);
                        ui.close_menu();
                    }
                });
            });
        });

        match self.state.open_window {
            Some(OpenWindow::VideoSettings) => {
                self.render_video_settings_window(ctx);
            }
            Some(OpenWindow::EmulationError) => {
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
            _ => {}
        }

        if prev_config != self.config {
            self.save_config();
        }
    }
}

fn launch_emulator<P: AsRef<Path>>(
    path: P,
    sender: &Sender<JgnesNativeConfig>,
    config: &AppConfig,
) {
    let path = path.as_ref();

    let file_path_str = path.to_string_lossy().to_string();
    sender
        .send(JgnesNativeConfig {
            nes_file_path: file_path_str,
            window_width: config.window_width,
            window_height: config.window_height,
            renderer: config.renderer,
            gpu_filter_mode: match config.gpu_filter_type {
                GpuFilterType::NearestNeighbor => GpuFilterMode::NearestNeighbor,
                GpuFilterType::Linear => GpuFilterMode::Linear(config.gpu_render_scale),
            },
        })
        .unwrap();
}
