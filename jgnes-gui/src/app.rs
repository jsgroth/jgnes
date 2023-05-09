use crate::emuthread::{EmuThreadTask, InputCollectResult};
use crate::romlist::RomMetadata;
use crate::{emuthread, romlist};
use eframe::Frame;
use egui::panel::TopBottomSide;
use egui::{
    menu, Align, Button, CentralPanel, Color32, Context, Grid, Key, KeyboardShortcut, Layout,
    Modifiers, TextEdit, TopBottomPanel, Ui, Vec2, Widget, Window,
};
use egui_extras::{Column, TableBuilder};
use jgnes_native_driver::{
    AspectRatio, GpuFilterMode, InputConfig, InputConfigBase, JgnesDynamicConfig,
    JgnesNativeConfig, JoystickInput, KeyboardInput, NativeRenderer, Overscan, RenderScale,
    VSyncMode, WgpuBackend,
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum GpuFilterType {
    NearestNeighbor,
    #[default]
    Linear,
}

fn default_window_width() -> u32 {
    (f64::from(3 * 224) * 64.0 / 49.0).ceil() as u32
}

fn default_window_height() -> u32 {
    3 * 224
}

fn true_fn() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default = "default_window_width")]
    window_width: u32,
    #[serde(default = "default_window_height")]
    window_height: u32,
    #[serde(default)]
    renderer: NativeRenderer,
    #[serde(default)]
    wgpu_backend: WgpuBackend,
    #[serde(default)]
    gpu_filter_type: GpuFilterType,
    #[serde(default)]
    gpu_render_scale: RenderScale,
    #[serde(default)]
    aspect_ratio: AspectRatio,
    #[serde(default)]
    overscan: Overscan,
    #[serde(default)]
    forced_integer_height_scaling: bool,
    #[serde(default = "true_fn")]
    sync_to_audio: bool,
    #[serde(default)]
    launch_fullscreen: bool,
    #[serde(default)]
    vsync_mode: VSyncMode,
    rom_search_dir: Option<String>,
    #[serde(default)]
    input: InputConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        toml::from_str("")
            .expect("AppConfig should always deserialize successfully from empty string")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindow {
    VideoSettings,
    AudioSettings,
    InputSettings,
    UiSettings,
    About,
    EmulationError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Player {
    P1,
    P2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputType {
    Keyboard,
    Gamepad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputWindow(Player, InputType);

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

struct InputState {
    axis_deadzone_text: String,
    axis_deadzone_invalid: bool,
}

struct InputButton<'a> {
    button: Button,
    player: Player,
    input_type: InputType,
    nes_button: NesButton,
    axis_deadzone: u16,
    app_state: &'a mut AppState,
}

impl<'a> InputButton<'a> {
    fn new(player: Player, input_type: InputType, nes_button: NesButton, app: &'a mut App) -> Self {
        let current_input_str = match input_type {
            InputType::Keyboard => get_keyboard_field(&mut app.config.input, player, nes_button)
                .as_ref()
                .map_or("<None>".into(), ToString::to_string),
            InputType::Gamepad => get_joystick_field(&mut app.config.input, player, nes_button)
                .as_ref()
                .map_or("<None>".into(), ToString::to_string),
        };
        let button = Button::new(current_input_str);
        Self {
            button,
            player,
            input_type,
            nes_button,
            axis_deadzone: app.config.input.axis_deadzone,
            app_state: &mut app.state,
        }
    }

    fn ui(self, ui: &mut Ui) {
        if self.button.ui(ui).clicked() {
            self.app_state.waiting_for_input = Some((self.player, self.nes_button));
            self.app_state
                .thread_task_sender
                .send(EmuThreadTask::CollectInput {
                    input_type: self.input_type,
                    axis_deadzone: self.axis_deadzone,
                })
                .expect("Sending collect input task should not fail");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NesButton {
    Up,
    Left,
    Right,
    Down,
    A,
    B,
    Start,
    Select,
}

impl NesButton {
    const ALL: [Self; 8] = [
        Self::Up,
        Self::Left,
        Self::Right,
        Self::Down,
        Self::A,
        Self::B,
        Self::Start,
        Self::Select,
    ];
}

fn get_keyboard_field(
    input_config: &mut InputConfig,
    player: Player,
    button: NesButton,
) -> &mut Option<KeyboardInput> {
    let player_config = match player {
        Player::P1 => &mut input_config.p1.keyboard,
        Player::P2 => &mut input_config.p2.keyboard,
    };

    get_input_field(player_config, button)
}

fn get_joystick_field(
    input_config: &mut InputConfig,
    player: Player,
    button: NesButton,
) -> &mut Option<JoystickInput> {
    let player_config = match player {
        Player::P1 => &mut input_config.p1.joystick,
        Player::P2 => &mut input_config.p2.joystick,
    };

    get_input_field(player_config, button)
}

fn get_input_field<T>(player_config: &mut InputConfigBase<T>, button: NesButton) -> &mut Option<T> {
    match button {
        NesButton::Up => &mut player_config.up,
        NesButton::Left => &mut player_config.left,
        NesButton::Right => &mut player_config.right,
        NesButton::Down => &mut player_config.down,
        NesButton::A => &mut player_config.a,
        NesButton::B => &mut player_config.b,
        NesButton::Start => &mut player_config.start,
        NesButton::Select => &mut player_config.select,
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
    input: InputState,
    rom_list: Vec<RomMetadata>,
    open_window: Option<OpenWindow>,
    open_input_window: Option<InputWindow>,
    waiting_for_input: Option<(Player, NesButton)>,
    emulator_is_running: Arc<AtomicBool>,
    emulator_quit_signal: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
    thread_task_sender: Sender<EmuThreadTask>,
    thread_input_receiver: Receiver<Option<InputCollectResult>>,
}

impl AppState {
    fn new(config: &AppConfig) -> Self {
        let is_running = Arc::new(AtomicBool::new(false));
        let quit_signal = Arc::new(AtomicBool::new(false));
        let emulation_error = Arc::new(Mutex::new(None));
        let (thread_task_sender, thread_input_receiver) = emuthread::start(
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
        let input_state = InputState {
            axis_deadzone_text: config.input.axis_deadzone.to_string(),
            axis_deadzone_invalid: false,
        };
        Self {
            render_scale_text: config.gpu_render_scale.get().to_string(),
            render_scale_invalid: false,
            window_width_text: config.window_width.to_string(),
            window_width_invalid: false,
            window_height_text: config.window_height.to_string(),
            window_height_invalid: false,
            overscan: overscan_state,
            input: input_state,
            rom_list: Vec::new(),
            open_window: None,
            open_input_window: None,
            waiting_for_input: None,
            emulator_is_running: is_running,
            emulator_quit_signal: quit_signal,
            emulation_error,
            thread_task_sender,
            thread_input_receiver,
        }
    }

    fn stop_emulator_if_running(&self) {
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

        let mut app = Self {
            config_path,
            config,
            state,
        };
        app.refresh_rom_list();
        app
    }

    fn handle_open(&mut self) {
        let file = FileDialog::new().add_filter("nes", &["nes"]).pick_file();
        if let Some(file) = file {
            self.state.stop_emulator_if_running();

            launch_emulator(file, &self.state.thread_task_sender, &self.config);
        }
    }

    fn save_config(&mut self) {
        let config_str =
            toml::to_string(&self.config).expect("Config should always be serializable");
        fs::write(&self.config_path, config_str).expect("Unable to save config file");
    }

    fn render_central_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| match &self.config.rom_search_dir {
            Some(_) => {
                TableBuilder::new(ui)
                    .auto_shrink([false; 2])
                    .striped(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::auto().at_most(300.0))
                    .columns(Column::auto(), 3)
                    .column(Column::remainder())
                    .header(30.0, |mut row| {
                        row.col(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("Name");
                            });
                        });
                        row.col(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("Board");
                            });
                        });
                        row.col(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("PRG ROM");
                            });
                        });
                        row.col(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("CHR ROM");
                            });
                        });

                        // Blank column to make the stripes extend to the right
                        row.col(|_ui| {});
                    })
                    .body(|mut body| {
                        for metadata in &self.state.rom_list {
                            body.row(40.0, |mut row| {
                                row.col(|ui| {
                                    let button = Button::new(&metadata.file_name_no_ext)
                                        .min_size(Vec2::new(300.0, 30.0))
                                        .wrap(true);
                                    if button.ui(ui).clicked() {
                                        self.state.stop_emulator_if_running();
                                        launch_emulator(
                                            &metadata.full_path,
                                            &self.state.thread_task_sender,
                                            &self.config,
                                        );
                                    }
                                });

                                row.col(|ui| {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(&metadata.mapper_name);
                                    });
                                });

                                row.col(|ui| {
                                    ui.centered_and_justified(|ui| {
                                        let size_kb = metadata.prg_rom_len / 1024;
                                        ui.label(format!("{size_kb}KB"));
                                    });
                                });

                                row.col(|ui| {
                                    ui.centered_and_justified(|ui| {
                                        let size_kb = metadata.chr_rom_len / 1024;
                                        if size_kb > 0 {
                                            ui.label(format!("{size_kb}KB"));
                                        } else {
                                            ui.label("None (RAM)");
                                        }
                                    });
                                });

                                // Blank column to make the stripes extend to the right
                                row.col(|_ui| {});
                            });
                        }
                    });
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("Configure a ROM search directory to see ROM list here");
                });
            }
        });
    }

    fn render_ui_settings_window(&mut self, ctx: &Context) {
        let mut ui_settings_open = true;
        Window::new("UI Settings")
            .resizable(false)
            .open(&mut ui_settings_open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("ROM search directory:");

                    let button_text = self.config.rom_search_dir.as_deref().unwrap_or("<None>");
                    if ui.button(button_text).clicked() {
                        if let Some(dir) = FileDialog::new().pick_folder() {
                            self.config.rom_search_dir = dir.to_str().map(String::from);
                        }
                    }

                    if ui.button("Clear").clicked() {
                        self.config.rom_search_dir = None;
                    }
                });
            });
        if !ui_settings_open {
            self.state.open_window = None;
        }
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

                    ui.label("wgpu backend");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Auto, "Auto");
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Vulkan, "Vulkan");
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Direct3d12, "Direct3D 12");
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Metal, "Metal");
                    });
                });

                ui.checkbox(&mut self.config.launch_fullscreen, "Launch in fullscreen");

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
                    ui.label("VSync mode");

                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Enabled, "Enabled");
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Disabled, "Disabled");

                        ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Fast, "Fast")
                            .on_disabled_hover_text("Fast VSync is only supported with the wgpu renderer");
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Adaptive, "Adaptive")
                            .on_disabled_hover_text("Adaptive VSync is only supported with the wgpu renderer");
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

                ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                    ui.set_enabled(self.config.aspect_ratio != AspectRatio::Stretched);
                    ui.checkbox(&mut self.config.forced_integer_height_scaling, "Forced integer scaling for height")
                        .on_hover_text("Image height will always be the highest possible integer multiple of native (224px)")
                        .on_disabled_hover_text("This option is not available in stretched image mode");
                });

                ui.group(|ui| {
                    ui.label("Aspect ratio");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Ntsc, "NTSC")
                            .on_hover_text("8:7 pixel aspect ratio, 64:49 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::SquarePixels, "Square pixels")
                            .on_hover_text("1:1 pixel aspect ratio, 8:7 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::FourThree, "4:3")
                            .on_hover_text("7:6 pixel aspect ratio, 4:3 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Stretched, "Stretched")
                            .on_hover_text("Image will be stretched to fill the entire display area");
                    });
                });

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

    fn render_audio_settings_window(&mut self, ctx: &Context) {
        let mut audio_settings_open = true;
        Window::new("Audio Settings")
            .resizable(false)
            .open(&mut audio_settings_open)
            .show(ctx, |ui| {
                ui.checkbox(
                    &mut self.config.sync_to_audio,
                    "Sync emulation speed to audio",
                );
            });
        if !audio_settings_open {
            self.state.open_window = None;
        }
    }

    fn render_input_settings_window(&mut self, ctx: &Context) {
        let mut input_settings_open = true;
        Window::new("Input Settings")
            .resizable(false)
            .open(&mut input_settings_open)
            .show(ctx, |ui| {
                ui.set_enabled(self.state.open_input_window.is_none() && self.state.waiting_for_input.is_none());

                ui.horizontal(|ui| {
                    if ui.button("P1 Keyboard Input").clicked() {
                        self.state.open_input_window = Some(InputWindow(Player::P1, InputType::Keyboard));
                    }

                    ui.add_space(20.0);

                    if ui.button("P1 Gamepad Input").clicked() {
                        self.state.open_input_window = Some(InputWindow(Player::P1, InputType::Gamepad));
                    }
                });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("P2 Keyboard Input").clicked() {
                        self.state.open_input_window = Some(InputWindow(Player::P2, InputType::Keyboard));
                    }

                    ui.add_space(20.0);

                    if ui.button("P2 Gamepad Input").clicked() {
                        self.state.open_input_window = Some(InputWindow(Player::P2, InputType::Gamepad));
                    }
                });

                ui.add_space(20.0);

                ui.checkbox(
                    &mut self.config.input.allow_opposite_directions,
                    "Allow simultaneous opposing directional inputs (left+right / up+down)",
                )
                    .on_hover_text("Some games exhibit severe glitches when opposing directions are pressed simultaneously");

                ui.horizontal(|ui| {
                    NumericTextInput::new(
                        &mut self.state.input.axis_deadzone_text,
                        &mut self.config.input.axis_deadzone,
                        &mut self.state.input.axis_deadzone_invalid,
                        0..=i16::MAX as u16,
                    )
                    .desired_width(55.0)
                    .ui(ui);
                    ui.label("Joystick axis deadzone (0-32767)");
                });
                if self.state.input.axis_deadzone_invalid {
                    ui.colored_label(
                        Color32::RED,
                        "Axis deadzone must be an integer between 0 and 32767",
                    );
                }
            });
        if !input_settings_open {
            self.state.open_window = None;
            self.state.open_input_window = None;
        }

        self.render_input_subwindow(ctx);
    }

    fn render_input_subwindow(&mut self, ctx: &Context) {
        let Some(InputWindow(player, input_type)) = self.state.open_input_window else { return };

        let window_title = format!("{player:?} {input_type:?} Configuration");

        let mut input_subwindow_open = true;
        Window::new(&window_title)
            .resizable(false)
            .open(&mut input_subwindow_open)
            .show(ctx, |ui| {
                ui.set_enabled(self.state.waiting_for_input.is_none());

                Grid::new(format!("{player:?}_{input_type:?}")).show(ui, |ui| {
                    for nes_button in NesButton::ALL {
                        ui.label(format!("{nes_button:?}:"));
                        InputButton::new(player, input_type, nes_button, self).ui(ui);

                        if ui.button("Clear").clicked() {
                            match input_type {
                                InputType::Keyboard => {
                                    *get_keyboard_field(
                                        &mut self.config.input,
                                        player,
                                        nes_button,
                                    ) = None;
                                }
                                InputType::Gamepad => {
                                    *get_joystick_field(
                                        &mut self.config.input,
                                        player,
                                        nes_button,
                                    ) = None;
                                }
                            }
                        }

                        ui.end_row();
                    }
                });
            });
        if !input_subwindow_open {
            self.state.open_input_window = None;
        }
    }

    fn render_about_window(&mut self, ctx: &Context) {
        let mut about_open = true;
        Window::new("About")
            .resizable(false)
            .open(&mut about_open)
            .show(ctx, |ui| {
                ui.heading("jgnes");

                ui.add_space(10.0);

                ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));

                ui.add_space(15.0);

                ui.label("Copyright © 2023 James Groth");

                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    ui.label("Source code:");
                    ui.hyperlink("https://github.com/jsgroth/jgnes");
                });
            });
        if !about_open {
            self.state.open_window = None;
        }
    }

    fn poll_for_input_thread_result(&mut self) {
        let Some((player, nes_button)) = self.state.waiting_for_input else { return };

        if let Ok(collect_result) = self
            .state
            .thread_input_receiver
            .recv_timeout(Duration::from_millis(1))
        {
            self.state.waiting_for_input = None;

            match collect_result {
                Some(InputCollectResult::Keyboard(keycode)) => {
                    *get_keyboard_field(&mut self.config.input, player, nes_button) =
                        Some(KeyboardInput::from(keycode));
                }
                Some(InputCollectResult::Gamepad(joystick_input)) => {
                    *get_joystick_field(&mut self.config.input, player, nes_button) =
                        Some(joystick_input);
                }
                None => {}
            }
        }
    }

    fn refresh_rom_list(&mut self) {
        let Some(rom_search_dir) = &self.config.rom_search_dir else { return };

        match romlist::get_rom_list(rom_search_dir) {
            Ok(mut rom_list) => {
                rom_list.sort_by(|a, b| a.file_name_no_ext.cmp(&b.file_name_no_ext));
                self.state.rom_list = rom_list;
            }
            Err(err) => {
                log::error!("Error retriving ROM list from {rom_search_dir}: {err}");
            }
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

        self.poll_for_input_thread_result();

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
            ui.set_enabled(
                self.state.open_window.is_none() && self.state.waiting_for_input.is_none(),
            );
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

                ui.set_enabled(!self.state.emulator_is_running.load(Ordering::Relaxed));
                ui.menu_button("Settings", |ui| {
                    if ui.button("Video").clicked() {
                        self.state.open_window = Some(OpenWindow::VideoSettings);
                        ui.close_menu();
                    }

                    if ui.button("Audio").clicked() {
                        self.state.open_window = Some(OpenWindow::AudioSettings);
                        ui.close_menu();
                    }

                    if ui.button("Input").clicked() {
                        self.state.open_window = Some(OpenWindow::InputSettings);
                        ui.close_menu();
                    }

                    if ui.button("Interface").clicked() {
                        self.state.open_window = Some(OpenWindow::UiSettings);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.state.open_window = Some(OpenWindow::About);
                        ui.close_menu();
                    }
                });
            });
        });

        self.render_central_panel(ctx);

        match self.state.open_window {
            Some(OpenWindow::VideoSettings) => {
                self.render_video_settings_window(ctx);
            }
            Some(OpenWindow::AudioSettings) => {
                self.render_audio_settings_window(ctx);
            }
            Some(OpenWindow::InputSettings) => {
                self.render_input_settings_window(ctx);
            }
            Some(OpenWindow::UiSettings) => {
                self.render_ui_settings_window(ctx);
            }
            Some(OpenWindow::About) => {
                self.render_about_window(ctx);
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
            None => {}
        }

        if prev_config != self.config {
            self.save_config();
            self.refresh_rom_list();
        }
    }
}

fn launch_emulator<P: AsRef<Path>>(path: P, sender: &Sender<EmuThreadTask>, config: &AppConfig) {
    let path = path.as_ref();

    let file_path_str = path.to_string_lossy().to_string();
    sender
        .send(EmuThreadTask::RunEmulator(Box::new(JgnesNativeConfig {
            nes_file_path: file_path_str,
            window_width: config.window_width,
            window_height: config.window_height,
            renderer: config.renderer,
            wgpu_backend: config.wgpu_backend,
            gpu_filter_mode: match config.gpu_filter_type {
                GpuFilterType::NearestNeighbor => GpuFilterMode::NearestNeighbor,
                GpuFilterType::Linear => GpuFilterMode::Linear(config.gpu_render_scale),
            },
            aspect_ratio: config.aspect_ratio,
            overscan: config.overscan,
            forced_integer_height_scaling: config.forced_integer_height_scaling,
            vsync_mode: config.vsync_mode,
            sync_to_audio: config.sync_to_audio,
            launch_fullscreen: config.launch_fullscreen,
            input_config: config.input.clone(),
        })))
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default_does_not_panic() {
        let _app_config = AppConfig::default();
    }
}
