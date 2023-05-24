#![cfg(target_arch = "wasm32")]

mod audio;

use crate::audio::{AudioQueue, EnqueueResult};
use base64::engine::general_purpose;
use base64::Engine;
use jgnes_core::audio::{DownsampleAction, DownsampleCounter, LowPassFilter};
use jgnes_core::{
    AudioPlayer, Emulator, EmulatorConfig, InputPoller, JoypadState, SaveWriter, TickEffect,
};
use jgnes_renderer::config::{
    AspectRatio, FrameSkip, GpuFilterMode, Overscan, RenderScale, RendererConfig, VSyncMode,
    WgpuBackend,
};
use jgnes_renderer::WgpuRenderer;
use js_sys::{Promise, Uint8Array};
use rfd::AsyncFileDialog;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextOptions};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::platform::web::WindowExtWebSys;
use winit::window::{Window, WindowBuilder, WindowId};

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen(module = "/js/save-writer.js")]
extern "C" {
    fn loadFromLocalStorage(key: &str) -> Option<String>;

    fn saveToLocalStorage(key: &str, value: &str);
}

#[wasm_bindgen(module = "/js/ui.js")]
extern "C" {
    fn initComplete();

    fn afterInputReconfigure(buttonId: &str, buttonText: &str);

    fn focusCanvas();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[wasm_bindgen]
pub enum NesButton {
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
    fn element_id(self) -> &'static str {
        match self {
            Self::Up => "up-key",
            Self::Left => "left-key",
            Self::Right => "right-key",
            Self::Down => "down-key",
            Self::A => "a-key",
            Self::B => "b-key",
            Self::Start => "start-key",
            Self::Select => "select-key",
        }
    }
}

#[must_use]
#[wasm_bindgen]
pub fn b64_to_bytes(s: &str) -> Option<Uint8Array> {
    match general_purpose::STANDARD.decode(s) {
        Ok(bytes) => {
            let array = Uint8Array::new_with_length(bytes.len() as u32);
            for (i, byte) in bytes.iter().copied().enumerate() {
                array.set_index(i as u32, byte);
            }
            Some(array)
        }
        Err(_) => None,
    }
}

fn alert_and_panic(s: &str) -> ! {
    alert(s);
    panic!("{s}")
}

fn window_size(window: &Window) -> (u32, u32) {
    let PhysicalSize { width, height } = window.inner_size();
    (width, height)
}

struct WebSaveWriter {
    file_name: String,
}

impl SaveWriter for WebSaveWriter {
    type Err = ();

    fn persist_sram(&mut self, sram: &[u8]) -> Result<(), Self::Err> {
        let sram_b64 = general_purpose::STANDARD.encode(sram);
        saveToLocalStorage(&self.file_name, &sram_b64);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputHandlerState {
    RunningEmulator,
    WaitingForInput(NesButton),
}

struct InputHandler {
    button_mapping: HashMap<VirtualKeyCode, Vec<NesButton>>,
    p1_joypad_state: Rc<RefCell<JoypadState>>,
    handler_state: InputHandlerState,
}

impl InputHandler {
    fn new() -> Self {
        let mut default_mapping = HashMap::new();
        default_mapping.insert(VirtualKeyCode::Up, vec![NesButton::Up]);
        default_mapping.insert(VirtualKeyCode::Left, vec![NesButton::Left]);
        default_mapping.insert(VirtualKeyCode::Right, vec![NesButton::Right]);
        default_mapping.insert(VirtualKeyCode::Down, vec![NesButton::Down]);
        default_mapping.insert(VirtualKeyCode::Z, vec![NesButton::A]);
        default_mapping.insert(VirtualKeyCode::X, vec![NesButton::B]);
        default_mapping.insert(VirtualKeyCode::Return, vec![NesButton::Start]);
        default_mapping.insert(VirtualKeyCode::RShift, vec![NesButton::Select]);

        Self {
            button_mapping: default_mapping,
            p1_joypad_state: Rc::default(),
            handler_state: InputHandlerState::RunningEmulator,
        }
    }

    fn get_field_mut(joypad_state: &mut JoypadState, button: NesButton) -> &mut bool {
        match button {
            NesButton::Up => &mut joypad_state.up,
            NesButton::Left => &mut joypad_state.left,
            NesButton::Right => &mut joypad_state.right,
            NesButton::Down => &mut joypad_state.down,
            NesButton::A => &mut joypad_state.a,
            NesButton::B => &mut joypad_state.b,
            NesButton::Start => &mut joypad_state.start,
            NesButton::Select => &mut joypad_state.select,
        }
    }

    fn remove_mapping_for_button(&mut self, button: NesButton) {
        for buttons in self.button_mapping.values_mut() {
            buttons.retain(|other_button| *other_button != button);
        }
        self.button_mapping.retain(|_, buttons| !buttons.is_empty());
    }

    fn handle_window_event(&mut self, event: &WindowEvent<'_>) {
        if let WindowEvent::KeyboardInput {
            input:
                KeyboardInput {
                    virtual_keycode: Some(keycode),
                    state,
                    ..
                },
            ..
        } = event
        {
            match self.handler_state {
                InputHandlerState::RunningEmulator => {
                    for &button in self.button_mapping.get(keycode).unwrap_or(&vec![]) {
                        let mut joypad_state = self.p1_joypad_state.borrow_mut();
                        let field = Self::get_field_mut(&mut joypad_state, button);
                        *field = match state {
                            ElementState::Pressed => true,
                            ElementState::Released => false,
                        };
                    }
                }
                InputHandlerState::WaitingForInput(button) => {
                    if *state == ElementState::Pressed {
                        self.button_mapping
                            .entry(*keycode)
                            .or_default()
                            .push(button);
                        *self.p1_joypad_state.borrow_mut() = JoypadState::new();
                        self.handler_state = InputHandlerState::RunningEmulator;

                        afterInputReconfigure(button.element_id(), &format!("{keycode:?}"));
                    }
                }
            }
        }
    }
}

struct WebInputPoller {
    p1_joypad_state: Rc<RefCell<JoypadState>>,
}

impl InputPoller for WebInputPoller {
    fn poll_p1_input(&self) -> JoypadState {
        self.p1_joypad_state.borrow().sanitize_opposing_directions()
    }

    fn poll_p2_input(&self) -> JoypadState {
        JoypadState::default()
    }
}

struct WebAudioPlayer {
    audio_queue: AudioQueue,
    low_pass_filter: LowPassFilter,
    downsample_counter: DownsampleCounter,
    audio_enabled: Rc<RefCell<bool>>,
}

impl WebAudioPlayer {
    fn new(audio_queue: AudioQueue, audio_enabled: Rc<RefCell<bool>>) -> Self {
        Self {
            audio_queue,
            low_pass_filter: LowPassFilter::new(),
            downsample_counter: DownsampleCounter::new(AUDIO_OUTPUT_FREQUENCY, DISPLAY_RATE),
            audio_enabled,
        }
    }
}

const AUDIO_OUTPUT_FREQUENCY: f64 = 48000.0;
const DISPLAY_RATE: f64 = 60.0;

impl AudioPlayer for WebAudioPlayer {
    type Err = JsValue;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
        if !*self.audio_enabled.borrow() {
            return Ok(());
        }

        self.low_pass_filter.collect_sample(sample);

        if self.downsample_counter.increment() == DownsampleAction::OutputSample {
            let output_sample = self.low_pass_filter.output_sample();
            if self.audio_queue.push_if_space(output_sample as f32)? == EnqueueResult::BufferFull {
                log::warn!("Audio queue is full, dropping sample");
            }
        }

        Ok(())
    }
}

type WebRenderer = Rc<RefCell<WgpuRenderer<Window>>>;

type WebEmulator =
    Emulator<WebRenderer, Rc<RefCell<WebAudioPlayer>>, WebInputPoller, WebSaveWriter>;

struct State {
    emulator: Option<WebEmulator>,
    renderer: WebRenderer,
    audio_player: Rc<RefCell<WebAudioPlayer>>,
    audio_ctx: AudioContext,
    input_handler: InputHandler,
    aspect_ratio: AspectRatio,
    filter_mode: GpuFilterMode,
    overscan: Overscan,
    user_interacted: bool,
}

impl State {
    fn window_id(&self) -> WindowId {
        self.renderer.borrow().window().id()
    }
}

#[wasm_bindgen(start)]
pub fn init_logger() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Unable to initialize logger");
}

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct JgnesWebConfig {
    aspect_ratio: Rc<RefCell<AspectRatio>>,
    gpu_filter_mode: Rc<RefCell<GpuFilterMode>>,
    overscan: Rc<RefCell<Overscan>>,
    audio_enabled: Rc<RefCell<bool>>,
    audio_sync_enabled: Rc<RefCell<bool>>,
    silence_ultrasonic_triangle_output: Rc<RefCell<bool>>,
    reconfig_input_request: Rc<RefCell<Option<NesButton>>>,
    open_file_requested: Rc<RefCell<bool>>,
    reset_requested: Rc<RefCell<bool>>,
    current_filename: Rc<RefCell<String>>,
}

#[wasm_bindgen]
impl JgnesWebConfig {
    #[must_use]
    #[wasm_bindgen(constructor)]
    pub fn new() -> JgnesWebConfig {
        JgnesWebConfig {
            aspect_ratio: Rc::new(RefCell::new(AspectRatio::Ntsc)),
            gpu_filter_mode: Rc::new(RefCell::new(GpuFilterMode::NearestNeighbor)),
            overscan: Rc::default(),
            audio_enabled: Rc::new(RefCell::new(true)),
            audio_sync_enabled: Rc::new(RefCell::new(true)),
            silence_ultrasonic_triangle_output: Rc::new(RefCell::new(false)),
            reconfig_input_request: Rc::new(RefCell::new(None)),
            open_file_requested: Rc::new(RefCell::new(false)),
            reset_requested: Rc::new(RefCell::new(false)),
            current_filename: Rc::new(RefCell::new(String::new())),
        }
    }

    pub fn set_aspect_ratio(&self, aspect_ratio: &str) {
        let aspect_ratio = match aspect_ratio {
            "Ntsc" => AspectRatio::Ntsc,
            "SquarePixels" => AspectRatio::SquarePixels,
            _ => return,
        };
        *self.aspect_ratio.borrow_mut() = aspect_ratio;
    }

    pub fn set_filter_mode(&self, gpu_filter_mode: &str) {
        let gpu_filter_mode = match gpu_filter_mode {
            "NearestNeighbor" => GpuFilterMode::NearestNeighbor,
            "Linear" => GpuFilterMode::Linear(RenderScale::ONE),
            "Linear2x" => GpuFilterMode::LinearCpuScaled(RenderScale::TWO),
            "Linear3x" => GpuFilterMode::LinearCpuScaled(RenderScale::THREE),
            _ => return,
        };
        *self.gpu_filter_mode.borrow_mut() = gpu_filter_mode;
    }

    pub fn set_overscan_left(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().left);
    }

    pub fn set_overscan_right(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().right);
    }

    pub fn set_overscan_top(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().top);
    }

    pub fn set_overscan_bottom(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().bottom);
    }

    pub fn set_audio_enabled(&self, value: bool) {
        *self.audio_enabled.borrow_mut() = value;
    }

    pub fn set_audio_sync_enabled(&self, value: bool) {
        *self.audio_sync_enabled.borrow_mut() = value;
    }

    pub fn set_silence_ultrasonic_triangle_output(&self, value: bool) {
        *self.silence_ultrasonic_triangle_output.borrow_mut() = value;
    }

    pub fn reconfigure_input(&self, button: NesButton) {
        *self.reconfig_input_request.borrow_mut() = Some(button);
    }

    pub fn open_new_file(&self) {
        *self.open_file_requested.borrow_mut() = true;
    }

    pub fn reset_emulator(&self) {
        *self.reset_requested.borrow_mut() = true;
    }

    #[must_use]
    pub fn get_current_filename(&self) -> String {
        self.current_filename.borrow().clone()
    }

    // Duplicated definition so clone() can be called from JS
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn clone(&self) -> JgnesWebConfig {
        <JgnesWebConfig as Clone>::clone(self)
    }
}

impl Default for JgnesWebConfig {
    fn default() -> Self {
        Self::new()
    }
}

fn set_overscan_field(value: bool, field: &mut u8) {
    *field = if value { 8 } else { 0 };
}

fn load_sav_bytes(file_name: &str) -> Option<Vec<u8>> {
    loadFromLocalStorage(file_name)
        .and_then(|sav_b64| general_purpose::STANDARD.decode(sav_b64).ok())
}

fn set_rom_file_name_text(file_name: &str) {
    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let dst = doc.get_element_by_id("rom-file-name")?;
            dst.set_text_content(Some(file_name));
            Some(())
        })
        .expect("Unable to write file name into the DOM");
}

fn set_download_save_enabled(enabled: bool) {
    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let button = doc.get_element_by_id("jgnes-download-sav-button")?;
            if enabled {
                button.remove_attribute("disabled").ok()?;
            } else {
                button.set_attribute("disabled", "").ok()?;
            }
            Some(())
        })
        .expect("Unable to enable/disable download save button");
}

async fn open_file_in_event_loop(event_loop_proxy: EventLoopProxy<JgnesUserEvent>) {
    let Some(file) = AsyncFileDialog::new().add_filter("nes", &["nes"]).pick_file().await else { return };

    let file_bytes = file.read().await;
    let file_name = file.file_name();
    event_loop_proxy
        .send_event(JgnesUserEvent::RomFileLoaded {
            file_bytes,
            file_name,
        })
        .unwrap();
}

#[derive(Debug, Clone)]
enum JgnesUserEvent {
    RomFileLoaded {
        file_bytes: Vec<u8>,
        file_name: String,
    },
}

#[allow(clippy::missing_panics_doc)]
#[wasm_bindgen]
pub async fn run(config: JgnesWebConfig) {
    let event_loop = EventLoopBuilder::<JgnesUserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .build(&event_loop)
        .expect("Unable to create window");

    window.set_inner_size(LogicalSize::new(878, 672));

    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let dst = doc.get_element_by_id("jgnes-wasm")?;
            let canvas = web_sys::Element::from(window.canvas());
            dst.append_child(&canvas).ok()?;
            Some(())
        })
        .expect("Couldn't append canvas to document body");

    let gpu_filter_mode = *config.gpu_filter_mode.borrow();
    let aspect_ratio = *config.aspect_ratio.borrow();
    let renderer = WgpuRenderer::from_window(
        window,
        window_size,
        RendererConfig {
            vsync_mode: VSyncMode::Enabled,
            wgpu_backend: WgpuBackend::BrowserAuto,
            gpu_filter_mode,
            aspect_ratio,
            overscan: Overscan::default(),
            frame_skip: FrameSkip::ZERO,
            forced_integer_height_scaling: false,
            use_webgl2_limits: true,
        },
    )
    .await
    .map_err(|err| alert_and_panic(&err.to_string()))
    .unwrap();
    let renderer = Rc::new(RefCell::new(renderer));

    let audio_ctx = AudioContext::new_with_context_options(
        AudioContextOptions::new().sample_rate(AUDIO_OUTPUT_FREQUENCY as f32),
    )
    .unwrap();
    let audio_queue = AudioQueue::new();
    let _audio_worklet = audio::initialize_audio_worklet(&audio_ctx, &audio_queue)
        .await
        .unwrap();

    let audio_player = WebAudioPlayer::new(audio_queue, Rc::clone(&config.audio_enabled));
    let audio_player = Rc::new(RefCell::new(audio_player));

    let input_handler = InputHandler::new();

    let state = State {
        emulator: None,
        renderer,
        audio_player,
        audio_ctx,
        input_handler,
        aspect_ratio: *config.aspect_ratio.borrow(),
        filter_mode: *config.gpu_filter_mode.borrow(),
        overscan: *config.overscan.borrow(),
        user_interacted: false,
    };

    initComplete();

    run_event_loop(event_loop, config, state);
}

fn run_event_loop(
    event_loop: EventLoop<JgnesUserEvent>,
    config: JgnesWebConfig,
    mut state: State,
) -> ! {
    let event_loop_proxy = event_loop.create_proxy();

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::UserEvent(JgnesUserEvent::RomFileLoaded {
                file_bytes,
                file_name,
            }) => {
                let sav_bytes = load_sav_bytes(&file_name);

                let input_poller = WebInputPoller {
                    p1_joypad_state: Rc::clone(&state.input_handler.p1_joypad_state),
                };
                let save_writer = WebSaveWriter {
                    file_name: file_name.clone(),
                };

                match Emulator::create(
                    file_bytes,
                    sav_bytes,
                    Rc::clone(&state.renderer),
                    Rc::clone(&state.audio_player),
                    input_poller,
                    save_writer,
                ) {
                    Ok(emulator) => {
                        if !state.user_interacted {
                            state.user_interacted = true;

                            let _: Promise = state.audio_ctx.resume().unwrap();
                        }

                        set_rom_file_name_text(&file_name);
                        *config.current_filename.borrow_mut() = file_name;
                        set_download_save_enabled(emulator.has_persistent_ram());
                        focusCanvas();
                        state.emulator = Some(emulator);
                    }
                    Err(err) => {
                        alert(&format!("Error initializing emulator: {err}"));
                        log::error!("Error initializing emulator: {err}");
                    }
                }
            }
            Event::WindowEvent {
                event: win_event,
                window_id,
            } if window_id == state.window_id() => {
                state.input_handler.handle_window_event(&win_event);

                if let WindowEvent::CloseRequested = win_event {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared => {
                let config_aspect_ratio = *config.aspect_ratio.borrow();
                if config_aspect_ratio != state.aspect_ratio {
                    state
                        .renderer
                        .borrow_mut()
                        .update_aspect_ratio(config_aspect_ratio);
                    state.aspect_ratio = config_aspect_ratio;
                }

                let config_filter_mode = *config.gpu_filter_mode.borrow();
                if config_filter_mode != state.filter_mode {
                    state
                        .renderer
                        .borrow_mut()
                        .update_filter_mode(config_filter_mode);
                    state.filter_mode = config_filter_mode;
                }

                let config_overscan = *config.overscan.borrow();
                if config_overscan != state.overscan {
                    state.renderer.borrow_mut().update_overscan(config_overscan);
                    state.overscan = config_overscan;
                }

                if *config.open_file_requested.borrow() {
                    *config.open_file_requested.borrow_mut() = false;

                    wasm_bindgen_futures::spawn_local(open_file_in_event_loop(
                        event_loop_proxy.clone(),
                    ));
                }

                if *config.reset_requested.borrow() {
                    *config.reset_requested.borrow_mut() = false;
                    if let Some(emulator) = &mut state.emulator {
                        emulator.soft_reset();
                    }
                }

                if let Some(button) = config.reconfig_input_request.borrow_mut().take() {
                    state.input_handler.remove_mapping_for_button(button);
                    state.input_handler.handler_state = InputHandlerState::WaitingForInput(button);
                }

                // Don't tick the emulator while waiting for input configuration
                if !matches!(
                    state.input_handler.handler_state,
                    InputHandlerState::WaitingForInput(_)
                ) {
                    // If audio sync is enabled, only run the emulator if the audio queue isn't filling up
                    let should_wait_for_audio = *config.audio_sync_enabled.borrow()
                        && state.audio_player.borrow().audio_queue.len().unwrap() > 1024;
                    if !should_wait_for_audio {
                        if let Some(emulator) = &mut state.emulator {
                            let emulator_config = EmulatorConfig {
                                silence_ultrasonic_triangle_output: *config
                                    .silence_ultrasonic_triangle_output
                                    .borrow(),
                            };

                            // Tick the emulator until it renders the next frame
                            while emulator.tick(&emulator_config).expect("emulation error")
                                != TickEffect::FrameRendered
                            {}
                        }
                    }
                }
            }
            _ => {}
        }
    })
}
