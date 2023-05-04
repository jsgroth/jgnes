use crate::emuthread;
use eframe::Frame;
use egui::panel::TopBottomSide;
use egui::{
    menu, Align, Button, Color32, Context, Key, KeyboardShortcut, Layout, Modifiers, TextEdit,
    TopBottomPanel, Ui, Widget, Window,
};
use jgnes_native_driver::{
    AspectRatio, GpuFilterMode, JgnesDynamicConfig, JgnesNativeConfig, NativeRenderer, Overscan,
    RenderScale,
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::str::FromStr;
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
    #[serde(default)]
    aspect_ratio: AspectRatio,
    #[serde(default)]
    overscan: Overscan,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: 960,
            window_height: 720,
            renderer: NativeRenderer::Wgpu,
            gpu_filter_type: GpuFilterType::Linear,
            gpu_render_scale: RenderScale::THREE,
            aspect_ratio: AspectRatio::default(),
            overscan: Overscan::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindow {
    VideoSettings,
    EmulationError,
}

struct OverscanState {
    top_text: String,
    top_invalid: bool,
    left_text: String,
    left_invalid: bool,
    right_text: String,
    right_invalid: bool,
    bottom_text: String,
    bottom_invalid: bool,
}

impl OverscanState {
    fn invalid(&self) -> bool {
        self.top_invalid || self.left_invalid || self.right_invalid || self.bottom_invalid
    }
}

struct AppState {
    render_scale_text: String,
    render_scale_invalid: bool,
    window_width_text: String,
    window_width_invalid: bool,
    window_height_text: String,
    window_height_invalid: bool,
    overscan: OverscanState,
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
        let overscan_state = OverscanState {
            top_text: config.overscan.top.to_string(),
            top_invalid: false,
            left_text: config.overscan.left.to_string(),
            left_invalid: false,
            right_text: config.overscan.right.to_string(),
            right_invalid: false,
            bottom_text: config.overscan.bottom.to_string(),
            bottom_invalid: false,
        };
        Self {
            render_scale_text: config.gpu_render_scale.get().to_string(),
            render_scale_invalid: false,
            window_width_text: config.window_width.to_string(),
            window_width_invalid: false,
            window_height_text: config.window_height.to_string(),
            window_height_invalid: false,
            overscan: overscan_state,
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

struct NumericTextInput<'a, T> {
    text: &'a mut String,
    config_value: &'a mut T,
    invalid: &'a mut bool,
    allowed_values: RangeInclusive<T>,
    desired_width: Option<f32>,
}

impl<'a, T: FromStr + PartialOrd> NumericTextInput<'a, T> {
    fn new(
        text: &'a mut String,
        config_value: &'a mut T,
        invalid: &'a mut bool,
        allowed_values: RangeInclusive<T>,
    ) -> Self {
        Self {
            text,
            config_value,
            invalid,
            allowed_values,
            desired_width: None,
        }
    }

    fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    fn ui(self, ui: &mut Ui) {
        let mut text_edit = TextEdit::singleline(self.text);
        if let Some(desired_width) = self.desired_width {
            text_edit = text_edit.desired_width(desired_width);
        }
        let response = text_edit.ui(ui);
        if !response.has_focus() {
            match self.text.parse::<T>() {
                Ok(value) if self.allowed_values.contains(&value) => {
                    *self.config_value = value;
                    *self.invalid = false;
                }
                _ => {
                    *self.invalid = true;
                }
            }
        }
    }
}

const OVERSCAN_VERTICAL_MAX: u8 = 112;
const OVERSCAN_HORIZONTAL_MAX: u8 = 128;

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

                    let disabled_hover_text = "Only nearest neighbor filtering is supported with SDL2 renderer";
                    ui.label("Image filtering")
                        .on_disabled_hover_text(disabled_hover_text);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.gpu_filter_type, GpuFilterType::NearestNeighbor, "Nearest neighbor")
                            .on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.gpu_filter_type, GpuFilterType::Linear, "Linear interpolation")
                            .on_disabled_hover_text(disabled_hover_text);
                    });
                });

                ui.horizontal(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu && self.config.gpu_filter_type == GpuFilterType::Linear);

                    if !TextEdit::singleline(&mut self.state.render_scale_text).desired_width(30.0).ui(ui).has_focus() {
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

                ui.group(|ui| {
                    ui.label("Aspect ratio");
                    ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Ntsc, "NTSC")
                        .on_hover_text("8:7 pixel aspect ratio, 64:49 screen aspect ratio");
                    ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::SquarePixels, "Square pixels")
                        .on_hover_text("1:1 pixel aspect ratio, 8:7 screen aspect ratio");
                    ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::FourThree, "4:3")
                        .on_hover_text("7:6 pixel aspect ratio, 4:3 screen aspect ratio");
                    ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Stretched, "Stretched")
                        .on_hover_text("Image will be stretched to fill the entire display area");
                });

                ui.horizontal(|ui| {
                    NumericTextInput::new(
                        &mut self.state.window_width_text,
                        &mut self.config.window_width,
                        &mut self.state.window_width_invalid,
                        1..=u32::MAX,
                    )
                        .desired_width(60.0)
                        .ui(ui);
                    ui.label("Window width in pixels");
                });
                if self.state.window_width_invalid {
                    ui.colored_label(Color32::RED, "Window width must be a non-negative integer");
                }

                ui.horizontal(|ui| {
                    NumericTextInput::new(
                        &mut self.state.window_height_text,
                        &mut self.config.window_height,
                        &mut self.state.window_height_invalid,
                        1..=u32::MAX
                    )
                        .desired_width(60.0)
                        .ui(ui);
                    ui.label("Window height in pixels");
                });
                if self.state.window_height_invalid {
                    ui.colored_label(Color32::RED, "Window height must be a non-negative integer");
                }

                ui.group(|ui| {
                    ui.label("Overscan in pixels");

                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.label("Top");
                        NumericTextInput::new(
                            &mut self.state.overscan.top_text,
                            &mut self.config.overscan.top,
                            &mut self.state.overscan.top_invalid,
                            0..=OVERSCAN_VERTICAL_MAX,
                        )
                            .desired_width(40.0)
                            .ui(ui);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Left");
                        NumericTextInput::new(
                            &mut self.state.overscan.left_text,
                            &mut self.config.overscan.left,
                            &mut self.state.overscan.left_invalid,
                            0..=OVERSCAN_HORIZONTAL_MAX,
                        )
                            .desired_width(40.0)
                            .ui(ui);

                        ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Right");
                                NumericTextInput::new(
                                    &mut self.state.overscan.right_text,
                                    &mut self.config.overscan.right,
                                    &mut self.state.overscan.right_invalid,
                                    0..=OVERSCAN_HORIZONTAL_MAX,
                                )
                                    .desired_width(40.0)
                                    .ui(ui);
                            });
                        });
                    });

                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        NumericTextInput::new(
                            &mut self.state.overscan.bottom_text,
                            &mut self.config.overscan.bottom,
                            &mut self.state.overscan.bottom_invalid,
                            0..=OVERSCAN_VERTICAL_MAX,
                        )
                            .desired_width(40.0)
                            .ui(ui);
                        ui.label("Bottom");
                    });

                    if self.state.overscan.invalid() {
                        ui.colored_label(Color32::RED, "Overscan settings invalid; vertical must be 0-112 and horizontal must be 0-128");
                    }
                });
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
                    let open_button = Button::new("Open")
                        .shortcut_text(ctx.format_shortcut(&open_shortcut))
                        .ui(ui);
                    if open_button.clicked() {
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

                ui.set_enabled(
                    self.state.open_window.is_none()
                        && !self.state.emulator_is_running.load(Ordering::Relaxed),
                );
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
            aspect_ratio: config.aspect_ratio,
            overscan: config.overscan,
        })
        .unwrap();
}
