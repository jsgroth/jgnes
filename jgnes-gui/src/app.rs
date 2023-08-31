use crate::emuthread::EmuThreadTask;
use crate::romlist::RomMetadata;
use crate::{emuthread, romlist};
use eframe::Frame;
use egui::panel::TopBottomSide;
use egui::{
    menu, Align, Button, CentralPanel, Color32, Context, Grid, Key, KeyboardShortcut, Layout,
    Modifiers, TextEdit, TopBottomPanel, Ui, Vec2, Widget, Window,
};
use egui_extras::{Column, TableBuilder};
use jgnes_core::TimingMode;
use jgnes_native_driver::{
    HotkeyConfig, InputCollectResult, InputConfig, InputConfigBase, InputType, JgnesDynamicConfig,
    JgnesNativeConfig, JgnesSharedConfig, JoystickInput, KeyboardInput, NativeRenderer,
};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RenderScale, Scanlines, Shader, VSyncMode, WgpuBackend,
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

fn default_window_width() -> u32 {
    (f64::from(3 * 224) * 64.0 / 49.0).ceil() as u32
}

fn default_window_height() -> u32 {
    3 * 224
}

fn default_gpu_filter_mode() -> GpuFilterMode {
    GpuFilterMode::LinearInterpolation
}

fn default_blur_stdev() -> f64 {
    1.5
}

fn default_blur_radius() -> u32 {
    16
}

fn default_ff_multiplier() -> u8 {
    2
}

fn default_rewind_buffer_len_secs() -> u64 {
    10
}

fn true_fn() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum ShaderType {
    None,
    #[default]
    Prescale,
    GaussianBlur,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default = "default_window_width")]
    window_width: u32,
    #[serde(default = "default_window_height")]
    window_height: u32,
    #[serde(default)]
    renderer: NativeRenderer,
    #[serde(default)]
    wgpu_backend: WgpuBackend,
    #[serde(default = "default_gpu_filter_mode")]
    gpu_filter_mode: GpuFilterMode,
    #[serde(default)]
    shader_type: ShaderType,
    #[serde(default)]
    render_scale: RenderScale,
    #[serde(default = "default_blur_stdev")]
    blur_stdev: f64,
    #[serde(default = "default_blur_radius")]
    blur_radius: u32,
    #[serde(default)]
    scanlines: Scanlines,
    #[serde(default)]
    aspect_ratio: AspectRatio,
    #[serde(default)]
    overscan: Overscan,
    #[serde(default)]
    forced_integer_height_scaling: bool,
    #[serde(default)]
    forced_timing_mode: Option<TimingMode>,
    #[serde(default)]
    remove_sprite_limit: bool,
    #[serde(default)]
    pal_black_border: bool,
    #[serde(default = "true_fn")]
    sync_to_audio: bool,
    #[serde(default = "true_fn")]
    audio_refresh_rate_adjustment: bool,
    #[serde(default)]
    silence_ultrasonic_triangle_output: bool,
    #[serde(default)]
    launch_fullscreen: bool,
    #[serde(default)]
    vsync_mode: VSyncMode,
    #[serde(default = "default_ff_multiplier")]
    fast_forward_multiplier: u8,
    #[serde(default = "default_rewind_buffer_len_secs")]
    rewind_buffer_len_secs: u64,
    #[serde(default)]
    rom_search_dir: Option<String>,
    #[serde(default)]
    input: InputConfig,
}

impl AppConfig {
    fn to_jgnes_dynamic_config(&self) -> JgnesDynamicConfig {
        let shader = match self.shader_type {
            ShaderType::None => Shader::None,
            ShaderType::Prescale => Shader::Prescale(self.render_scale),
            ShaderType::GaussianBlur => Shader::GaussianBlur {
                prescale_factor: self.render_scale,
                stdev: self.blur_stdev,
                radius: self.blur_radius,
            },
        };

        JgnesDynamicConfig {
            gpu_filter_mode: self.gpu_filter_mode,
            shader,
            scanlines: self.scanlines,
            aspect_ratio: self.aspect_ratio,
            overscan: self.overscan,
            forced_integer_height_scaling: self.forced_integer_height_scaling,
            vsync_mode: self.vsync_mode,
            remove_sprite_limit: self.remove_sprite_limit,
            pal_black_border: self.pal_black_border,
            sync_to_audio: self.sync_to_audio,
            audio_refresh_rate_adjustment: self.audio_refresh_rate_adjustment,
            silence_ultrasonic_triangle_output: self.silence_ultrasonic_triangle_output,
            fast_forward_multiplier: self.fast_forward_multiplier,
            rewind_buffer_len: Duration::from_secs(self.rewind_buffer_len_secs),
            input_config: self.input.clone(),
        }
    }

    fn to_jgnes_native_config(
        &self,
        nes_file_path: String,
    ) -> (JgnesNativeConfig, Receiver<Option<InputCollectResult>>) {
        let (shared_config, input_reconfigure_receiver) =
            JgnesSharedConfig::new(self.to_jgnes_dynamic_config());

        let native_config = JgnesNativeConfig {
            nes_file_path,
            forced_timing_mode: self.forced_timing_mode,
            window_width: self.window_width,
            window_height: self.window_height,
            renderer: self.renderer,
            wgpu_backend: self.wgpu_backend,
            launch_fullscreen: self.launch_fullscreen,
            shared_config,
        };

        (native_config, input_reconfigure_receiver)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        toml::from_str("")
            .expect("AppConfig should always deserialize successfully from empty string")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindow {
    GeneralSettings,
    VideoSettings,
    AudioSettings,
    InputSettings,
    HotkeySettings,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Player {
    P1,
    P2,
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
    ff_multiplier_text: String,
    ff_multiplier_invalid: bool,
    rewind_buffer_len_text: String,
    rewind_buffer_len_invalid: bool,
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
            self.app_state.send_input_configure_request(self.input_type, self.axis_deadzone);
            self.app_state.waiting_for_input =
                Some(WaitingForInput::NesButton(self.player, self.nes_button));
        }
    }
}

struct HotkeyButton<'a> {
    button: Button,
    hotkey: Hotkey,
    axis_deadzone: u16,
    on_disabled_hover_text: Option<String>,
    app_state: &'a mut AppState,
}

impl<'a> HotkeyButton<'a> {
    fn new(hotkey: Hotkey, app: &'a mut App) -> Self {
        let current_value = match hotkey {
            Hotkey::Quit => app.config.input.hotkeys.quit.as_ref(),
            Hotkey::ToggleFullscreen => app.config.input.hotkeys.toggle_fullscreen.as_ref(),
            Hotkey::SaveState => app.config.input.hotkeys.save_state.as_ref(),
            Hotkey::LoadState => app.config.input.hotkeys.load_state.as_ref(),
            Hotkey::SoftReset => app.config.input.hotkeys.soft_reset.as_ref(),
            Hotkey::HardReset => app.config.input.hotkeys.hard_reset.as_ref(),
            Hotkey::FastForward => app.config.input.hotkeys.fast_forward.as_ref(),
            Hotkey::Rewind => app.config.input.hotkeys.rewind.as_ref(),
        };
        let button_text = current_value.map_or("<None>", String::as_str);

        Self {
            button: Button::new(button_text),
            hotkey,
            axis_deadzone: app.config.input.axis_deadzone,
            on_disabled_hover_text: None,
            app_state: &mut app.state,
        }
    }

    fn ui(self, ui: &mut Ui) {
        let mut response = self.button.ui(ui);
        if let Some(on_disabled_hover_text) = self.on_disabled_hover_text {
            response = response.on_disabled_hover_text(on_disabled_hover_text);
        }
        if response.clicked() {
            self.app_state.waiting_for_input = Some(WaitingForInput::Hotkey(self.hotkey));
            self.app_state.send_input_configure_request(InputType::Keyboard, self.axis_deadzone);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hotkey {
    Quit,
    ToggleFullscreen,
    SaveState,
    LoadState,
    SoftReset,
    HardReset,
    FastForward,
    Rewind,
}

impl Hotkey {
    const ALL: &'static [Self] = &[
        Self::Quit,
        Self::ToggleFullscreen,
        Self::SaveState,
        Self::LoadState,
        Self::SoftReset,
        Self::HardReset,
        Self::FastForward,
        Self::Rewind,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Quit => "Quit",
            Self::ToggleFullscreen => "Toggle Fullscreen",
            Self::SaveState => "Save State",
            Self::LoadState => "Load State",
            Self::SoftReset => "Soft Reset",
            Self::HardReset => "Hard Reset",
            Self::FastForward => "Fast Forward",
            Self::Rewind => "Rewind",
        }
    }
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

fn get_hotkey_field(hotkey_config: &mut HotkeyConfig, hotkey: Hotkey) -> &mut Option<String> {
    match hotkey {
        Hotkey::Quit => &mut hotkey_config.quit,
        Hotkey::ToggleFullscreen => &mut hotkey_config.toggle_fullscreen,
        Hotkey::SaveState => &mut hotkey_config.save_state,
        Hotkey::LoadState => &mut hotkey_config.load_state,
        Hotkey::SoftReset => &mut hotkey_config.soft_reset,
        Hotkey::HardReset => &mut hotkey_config.hard_reset,
        Hotkey::FastForward => &mut hotkey_config.fast_forward,
        Hotkey::Rewind => &mut hotkey_config.rewind,
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitingForInput {
    NesButton(Player, NesButton),
    Hotkey(Hotkey),
}

struct RunningEmulatorState {
    shared_config: JgnesSharedConfig,
    input_reconfigure_receiver: Receiver<Option<InputCollectResult>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputReceiveResult {
    Received(Option<InputCollectResult>),
    NotReceived,
}

struct ShaderState {
    render_scale_text: String,
    render_scale_invalid: bool,
    blur_stdev_text: String,
    blur_stdev_invalid: bool,
    blur_radius_text: String,
    blur_radius_invalid: bool,
}

struct AppState {
    window_width_text: String,
    window_width_invalid: bool,
    window_height_text: String,
    window_height_invalid: bool,
    shader: ShaderState,
    overscan: OverscanState,
    input: InputState,
    rom_list: Vec<RomMetadata>,
    open_window: Option<OpenWindow>,
    error_window_open: bool,
    open_input_window: Option<InputWindow>,
    waiting_for_input: Option<WaitingForInput>,
    emulator_is_running: Arc<AtomicBool>,
    running_emulator_state: Option<RunningEmulatorState>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
    thread_task_sender: Sender<EmuThreadTask>,
    thread_input_receiver: Receiver<Option<InputCollectResult>>,
}

impl AppState {
    fn new(config: &AppConfig) -> Self {
        let is_running = Arc::new(AtomicBool::new(false));
        let emulation_error = Arc::new(Mutex::new(None));
        let (thread_task_sender, thread_input_receiver) =
            emuthread::start(Arc::clone(&is_running), Arc::clone(&emulation_error));
        let shader_state = ShaderState {
            render_scale_text: config.render_scale.get().to_string(),
            render_scale_invalid: false,
            blur_stdev_text: config.blur_stdev.to_string(),
            blur_stdev_invalid: false,
            blur_radius_text: config.blur_radius.to_string(),
            blur_radius_invalid: false,
        };
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
            ff_multiplier_text: config.fast_forward_multiplier.to_string(),
            ff_multiplier_invalid: false,
            rewind_buffer_len_text: config.rewind_buffer_len_secs.to_string(),
            rewind_buffer_len_invalid: false,
        };
        Self {
            window_width_text: config.window_width.to_string(),
            window_width_invalid: false,
            window_height_text: config.window_height.to_string(),
            window_height_invalid: false,
            shader: shader_state,
            overscan: overscan_state,
            input: input_state,
            rom_list: Vec::new(),
            open_window: None,
            error_window_open: false,
            open_input_window: None,
            waiting_for_input: None,
            emulator_is_running: is_running,
            running_emulator_state: None,
            emulation_error,
            thread_task_sender,
            thread_input_receiver,
        }
    }

    fn stop_emulator_if_running(&self) {
        if self.emulator_is_running.load(Ordering::Relaxed) {
            log::info!("Setting quit signal to stop running emulator");
            if let Some(running_emulator_state) = &self.running_emulator_state {
                running_emulator_state.shared_config.request_quit();
            }
        }
    }

    fn is_any_window_open(&self) -> bool {
        self.open_window.is_some() || self.error_window_open || self.open_input_window.is_some()
    }

    fn send_input_configure_request(&self, input_type: InputType, axis_deadzone: u16) {
        match (self.emulator_is_running.load(Ordering::Relaxed), &self.running_emulator_state) {
            (true, Some(running_emulator_state)) => {
                running_emulator_state.shared_config.request_input_configure(input_type);
            }
            (true, None) => {
                // ???
                panic!("running emulator state should always be Some while emulator is running");
            }
            (false, _) => {
                self.thread_task_sender
                    .send(EmuThreadTask::CollectInput { input_type, axis_deadzone })
                    .expect("Sending collect input task should not fail");
            }
        }
    }

    fn recv_input_reconfigure_response(&self) -> InputReceiveResult {
        if let Some(running_emulator_state) = &self.running_emulator_state {
            if let Ok(input_collect_result) = running_emulator_state
                .input_reconfigure_receiver
                .recv_timeout(Duration::from_millis(1))
            {
                return InputReceiveResult::Received(input_collect_result);
            }
        }

        self.thread_input_receiver
            .recv_timeout(Duration::from_millis(1))
            .ok()
            .map_or(InputReceiveResult::NotReceived, InputReceiveResult::Received)
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
        Self { text, config_value, invalid, allowed_values, desired_width: None }
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

        let mut app = Self { config_path, config, state };
        app.refresh_rom_list();
        app
    }

    fn handle_open(&mut self) {
        let file = FileDialog::new().add_filter("nes", &["nes"]).pick_file();
        if let Some(file) = file {
            self.state.stop_emulator_if_running();

            self.launch_emulator(file);
        }
    }

    fn launch_emulator<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();

        let file_path_str = path.to_string_lossy().to_string();
        let (native_config, input_reconfigure_receiver) =
            self.config.to_jgnes_native_config(file_path_str);

        self.state
            .thread_task_sender
            .send(EmuThreadTask::RunEmulator(Box::new(native_config.clone())))
            .unwrap();

        self.state.running_emulator_state = Some(RunningEmulatorState {
            shared_config: native_config.shared_config,
            input_reconfigure_receiver,
        });
    }

    fn save_config(&mut self) {
        let config_str =
            toml::to_string(&self.config).expect("Config should always be serializable");
        fs::write(&self.config_path, config_str).expect("Unable to save config file");
    }

    fn update_running_emulator_config(&mut self) {
        let Some(running_emulator_state) = &self.state.running_emulator_state else {
            return;
        };

        let dynamic_config =
            &mut *running_emulator_state.shared_config.get_dynamic_config().lock().unwrap();

        *dynamic_config = self.config.to_jgnes_dynamic_config();

        running_emulator_state.shared_config.request_config_reload();
    }

    fn render_central_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            ui.set_enabled(!self.state.is_any_window_open());

            match &self.config.rom_search_dir {
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
                            for metadata in self.state.rom_list.clone() {
                                body.row(40.0, |mut row| {
                                    row.col(|ui| {
                                        let button = Button::new(&metadata.file_name_no_ext)
                                            .min_size(Vec2::new(300.0, 30.0))
                                            .wrap(true);
                                        if button.ui(ui).clicked() {
                                            self.state.stop_emulator_if_running();
                                            self.launch_emulator(&metadata.full_path);
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
            }
        });
    }

    fn render_general_settings_window(&mut self, ctx: &Context) {
        let mut general_settings_open = true;
        Window::new("General Settings").resizable(false).open(&mut general_settings_open).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    let button_text = self.config.rom_search_dir.as_deref().unwrap_or("<None>");
                    if ui.button(button_text).clicked() {
                        if let Some(dir) = FileDialog::new().pick_folder() {
                            self.config.rom_search_dir = dir.to_str().map(String::from);
                        }
                    }

                    ui.label("ROM search directory");

                    if ui.button("Clear").clicked() {
                        self.config.rom_search_dir = None;
                    }
                });

                ui.checkbox(
                    &mut self.config.remove_sprite_limit,
                    "Remove 8 sprite per scanline limit",
                )
                .on_hover_text("Eliminates sprite flickering but can cause bugs");

                ui.group(|ui| {
                    ui.set_enabled(!self.state.emulator_is_running.load(Ordering::Relaxed));

                    let disabled_hover_text =
                        "Cannot change forced timing mode while emulator is running";

                    ui.label("Forced timing mode")
                        .on_hover_text("If set, ignore timing mode in cartridge header")
                        .on_disabled_hover_text(disabled_hover_text);

                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.forced_timing_mode, None, "None")
                            .on_disabled_hover_text(disabled_hover_text);

                        ui.radio_value(
                            &mut self.config.forced_timing_mode,
                            Some(TimingMode::Ntsc),
                            "NTSC",
                        )
                        .on_disabled_hover_text(disabled_hover_text);

                        ui.radio_value(
                            &mut self.config.forced_timing_mode,
                            Some(TimingMode::Pal),
                            "PAL",
                        )
                        .on_disabled_hover_text(disabled_hover_text);
                    });
                });
            },
        );
        if !general_settings_open {
            self.state.open_window = None;
        }
    }

    fn render_video_settings_window(&mut self, ctx: &Context) {
        let mut video_settings_open = true;
        Window::new("Video Settings")
            .resizable(false)
            .open(&mut video_settings_open)
            .show(ctx, |ui| {
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
                    ui.set_enabled(!self.state.emulator_is_running.load(Ordering::Relaxed));

                    ui.label("Renderer");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.renderer, NativeRenderer::Wgpu, "wgpu")
                            .on_disabled_hover_text("Cannot change renderer while emulator is running");
                        ui.radio_value(&mut self.config.renderer, NativeRenderer::Sdl2, "SDL2")
                            .on_disabled_hover_text("Cannot change renderer while emulator is running");
                    });
                });

                ui.group(|ui| {
                    ui.set_enabled(!self.state.emulator_is_running.load(Ordering::Relaxed) && self.config.renderer == NativeRenderer::Wgpu);

                    let disabled_text = match self.config.renderer {
                        NativeRenderer::Sdl2 => "Not applicable to SDL2 renderer",
                        NativeRenderer::Wgpu => "Cannot change wgpu backend while emulator is running"
                    };

                    ui.label("wgpu backend");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Auto, "Auto")
                            .on_disabled_hover_text(disabled_text);
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Vulkan, "Vulkan")
                            .on_disabled_hover_text(disabled_text);
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Direct3d12, "Direct3D 12")
                            .on_disabled_hover_text(disabled_text);
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::Metal, "Metal")
                            .on_disabled_hover_text(disabled_text);
                        ui.radio_value(&mut self.config.wgpu_backend, WgpuBackend::OpenGl, "OpenGL")
                            .on_disabled_hover_text(disabled_text);
                    });
                });

                ui.group(|ui| {
                    ui.label("VSync mode");

                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu || !self.state.emulator_is_running.load(Ordering::Relaxed));

                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Enabled, "Enabled")
                            .on_disabled_hover_text("SDL2 renderer cannot change VSync mode while running");
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Disabled, "Disabled")
                            .on_disabled_hover_text("SDL2 renderer cannot change VSync mode while running");

                        ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);
                        ui.radio_value(&mut self.config.vsync_mode, VSyncMode::Fast, "Fast")
                            .on_disabled_hover_text("Fast VSync is only supported with the wgpu renderer");
                    });
                });

                ui.group(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);

                    let disabled_hover_text = "Only nearest neighbor sampling is supported with SDL2 renderer";
                    ui.label("Image filtering")
                        .on_disabled_hover_text(disabled_hover_text);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.gpu_filter_mode, GpuFilterMode::NearestNeighbor, "Nearest neighbor")
                            .on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.gpu_filter_mode, GpuFilterMode::LinearInterpolation, "Linear interpolation")
                            .on_disabled_hover_text(disabled_hover_text);
                    });
                });

                ui.group(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);

                    let disabled_hover_text = "Shaders are not supported with SDL2 renderer";
                    ui.label("Shader").on_disabled_hover_text(disabled_hover_text);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.shader_type, ShaderType::None, "None").on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.shader_type, ShaderType::Prescale, "Prescale").on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.shader_type, ShaderType::GaussianBlur, "Gaussian blur").on_disabled_hover_text(disabled_hover_text);
                    });
                });

                ui.horizontal(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu && [ShaderType::Prescale, ShaderType::GaussianBlur].contains(&self.config.shader_type));

                    if !TextEdit::singleline(&mut self.state.shader.render_scale_text).desired_width(30.0).ui(ui).has_focus() {
                        match RenderScale::try_from(self.state.shader.render_scale_text.parse::<u32>().unwrap_or(0)) {
                            Ok(render_scale) => {
                                self.state.shader.render_scale_invalid = false;
                                self.config.render_scale = render_scale;
                            }
                            Err(_) => {
                                self.state.shader.render_scale_invalid = true;
                            }
                        }
                    }
                    ui.label("Prescale factor")
                        .on_hover_text("The image will be integer upscaled by this factor before filtering");

                    ui.set_enabled(self.config.shader_type == ShaderType::GaussianBlur);

                    if !TextEdit::singleline(&mut self.state.shader.blur_stdev_text).desired_width(30.0).ui(ui).has_focus() {
                        match self.state.shader.blur_stdev_text.parse::<f64>() {
                            Ok(blur_stdev) if !blur_stdev.is_nan() && !blur_stdev.is_sign_negative() => {
                                self.state.shader.blur_stdev_invalid = false;
                                self.config.blur_stdev = blur_stdev;
                            }
                            _ => {
                                self.state.shader.blur_stdev_invalid = true;
                            }
                        }
                    }
                    ui.label("Blur stdev");

                    NumericTextInput::new(
                        &mut self.state.shader.blur_radius_text,
                        &mut self.config.blur_radius,
                        &mut self.state.shader.blur_radius_invalid,
                        1..=u32::MAX,
                    )
                        .desired_width(30.0)
                        .ui(ui);
                    ui.label("Blur radius");
                });

                if self.state.shader.render_scale_invalid {
                    ui.colored_label(Color32::RED, "Scaling factor must be an integer between 1 and 16");
                }
                if self.state.shader.blur_stdev_invalid {
                    ui.colored_label(Color32::RED, "Blur stdev must be a floating point number that is not negative or NaN");
                }
                if self.state.shader.blur_radius_invalid {
                    ui.colored_label(Color32::RED, "Blur radius must be a non-negative integer");
                }

                ui.group(|ui| {
                    ui.set_enabled(self.config.renderer == NativeRenderer::Wgpu);

                    let scanlines_hover_text = "Works best with integer height scaling";
                    let disabled_hover_text = "Scanlines are not supported with SDL2 renderer";

                    ui.label("Scanlines").on_disabled_hover_text(disabled_hover_text);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.scanlines, Scanlines::None, "None")
                            .on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.scanlines, Scanlines::Dim, "Dim")
                            .on_hover_text(scanlines_hover_text)
                            .on_disabled_hover_text(disabled_hover_text);
                        ui.radio_value(&mut self.config.scanlines, Scanlines::Black, "Black")
                            .on_hover_text(scanlines_hover_text)
                            .on_disabled_hover_text(disabled_hover_text);
                    });
                });

                ui.group(|ui| {
                    ui.label("Aspect ratio");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Ntsc, "NTSC")
                            .on_hover_text("8:7 pixel aspect ratio, 64:49 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Pal, "PAL")
                            .on_hover_text("11:8 pixel aspect ratio, 22:15 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::SquarePixels, "Square pixels")
                            .on_hover_text("1:1 pixel aspect ratio, 8:7 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::FourThree, "4:3")
                            .on_hover_text("7:6 pixel aspect ratio, 4:3 screen aspect ratio");
                        ui.radio_value(&mut self.config.aspect_ratio, AspectRatio::Stretched, "Stretched")
                            .on_hover_text("Image will be stretched to fill the entire display area");
                    });
                });

                ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                    ui.set_enabled(self.config.aspect_ratio != AspectRatio::Stretched);
                    ui.checkbox(&mut self.config.forced_integer_height_scaling, "Forced integer scaling for height")
                        .on_hover_text("Image height will always be the highest possible integer multiple of native (224px)")
                        .on_disabled_hover_text("This option is not available in stretched image mode");
                });

                ui.checkbox(&mut self.config.pal_black_border, "Emulate PAL black border")
                    .on_hover_text("Removes top scanline plus two columns of pixels in each row");

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

                ui.checkbox(
                    &mut self.config.audio_refresh_rate_adjustment,
                    "Apply 60Hz refresh rate adjustment",
                )
                    .on_hover_text("Adjust audio downsampling rate to time audio to 60FPS instead of ~60.1FPS (NTSC)");

                ui.checkbox(
                    &mut self.config.silence_ultrasonic_triangle_output,
                    "Silence triangle wave channel at ultrasonic frequencies",
                )
                .on_hover_text("This is less accurate but can reduce audio popping in some games");
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
        let Some(InputWindow(player, input_type)) = self.state.open_input_window else {
            return;
        };

        let window_title = format!("{player:?} {input_type:?} Configuration");

        let mut input_subwindow_open = true;
        Window::new(&window_title).resizable(false).open(&mut input_subwindow_open).show(
            ctx,
            |ui| {
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
            },
        );
        if !input_subwindow_open {
            self.state.open_input_window = None;
        }
    }

    fn render_hotkey_settings_window(&mut self, ctx: &Context) {
        let mut hotkey_settings_open = true;
        Window::new("Hotkey Settings").resizable(false).open(&mut hotkey_settings_open).show(
            ctx,
            |ui| {
                Grid::new("hotkey_settings_grid").show(ui, |ui| {
                    for &hotkey in Hotkey::ALL {
                        ui.label(format!("{}:", hotkey.label()));

                        HotkeyButton::new(hotkey, self).ui(ui);

                        if ui.button("Clear").clicked() {
                            *get_hotkey_field(&mut self.config.input.hotkeys, hotkey) = None;
                        }

                        ui.end_row();
                    }
                });

                ui.horizontal(|ui| {
                    NumericTextInput::new(
                        &mut self.state.input.ff_multiplier_text,
                        &mut self.config.fast_forward_multiplier,
                        &mut self.state.input.ff_multiplier_invalid,
                        2..=16,
                    )
                    .desired_width(40.0)
                    .ui(ui);
                    ui.label("Fast forward multiplier");
                });
                if self.state.input.ff_multiplier_invalid {
                    ui.colored_label(
                        Color32::RED,
                        "Fast forward multiplier must be an integer between 2 and 16",
                    );
                }

                ui.horizontal(|ui| {
                    NumericTextInput::new(
                        &mut self.state.input.rewind_buffer_len_text,
                        &mut self.config.rewind_buffer_len_secs,
                        &mut self.state.input.rewind_buffer_len_invalid,
                        0..=u64::MAX,
                    )
                    .desired_width(40.0)
                    .ui(ui);
                    ui.label("Rewind buffer length in seconds");
                });
                if self.state.input.rewind_buffer_len_invalid {
                    ui.colored_label(
                        Color32::RED,
                        "Rewind buffer length must be a non-negative integer",
                    );
                }
            },
        );
        if !hotkey_settings_open {
            self.state.open_window = None;
        }
    }

    fn render_about_window(&mut self, ctx: &Context) {
        let mut about_open = true;
        Window::new("About").resizable(false).open(&mut about_open).show(ctx, |ui| {
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
        let Some(waiting_for_input) = self.state.waiting_for_input else {
            return;
        };

        if let InputReceiveResult::Received(collect_result) =
            self.state.recv_input_reconfigure_response()
        {
            self.state.waiting_for_input = None;

            match waiting_for_input {
                WaitingForInput::NesButton(player, nes_button) => match collect_result {
                    Some(InputCollectResult::Keyboard(keycode)) => {
                        *get_keyboard_field(&mut self.config.input, player, nes_button) =
                            Some(KeyboardInput::from(keycode));
                    }
                    Some(InputCollectResult::Gamepad(joystick_input)) => {
                        *get_joystick_field(&mut self.config.input, player, nes_button) =
                            Some(joystick_input);
                    }
                    None => {}
                },
                WaitingForInput::Hotkey(hotkey) => match collect_result {
                    Some(InputCollectResult::Keyboard(keycode)) => {
                        *get_hotkey_field(&mut self.config.input.hotkeys, hotkey) =
                            Some(keycode.name());
                    }
                    Some(InputCollectResult::Gamepad(..)) => {
                        panic!("hotkey input results should always be keyboard")
                    }
                    None => {}
                },
            }
        }
    }

    fn refresh_rom_list(&mut self) {
        let Some(rom_search_dir) = &self.config.rom_search_dir else {
            return;
        };

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
            self.state.error_window_open = true;
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
            ui.set_enabled(!self.state.is_any_window_open());
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

                ui.menu_button("Settings", |ui| {
                    if ui.button("General").clicked() {
                        self.state.open_window = Some(OpenWindow::GeneralSettings);
                        ui.close_menu();
                    }

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

                    if ui.button("Hotkeys").clicked() {
                        self.state.open_window = Some(OpenWindow::HotkeySettings);
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
            Some(OpenWindow::GeneralSettings) => {
                self.render_general_settings_window(ctx);
            }
            Some(OpenWindow::VideoSettings) => {
                self.render_video_settings_window(ctx);
            }
            Some(OpenWindow::AudioSettings) => {
                self.render_audio_settings_window(ctx);
            }
            Some(OpenWindow::InputSettings) => {
                self.render_input_settings_window(ctx);
            }
            Some(OpenWindow::HotkeySettings) => {
                self.render_hotkey_settings_window(ctx);
            }
            Some(OpenWindow::About) => {
                self.render_about_window(ctx);
            }
            None => {}
        }

        if self.state.error_window_open {
            let mut error_open = true;
            Window::new("Error").resizable(false).open(&mut error_open).show(ctx, |ui| {
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
                self.state.error_window_open = false;
                *self.state.emulation_error.lock().unwrap() = None;
            }
        }

        if prev_config != self.config {
            self.save_config();
            self.refresh_rom_list();

            if self.state.emulator_is_running.load(Ordering::Relaxed) {
                self.update_running_emulator_config();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default_does_not_panic() {
        let _app_config = AppConfig::default();
    }
}
