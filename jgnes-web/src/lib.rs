#![cfg(target_arch = "wasm32")]

mod audio;
mod config;
mod js;

use crate::audio::{AudioQueue, EnqueueResult};
use crate::config::InputConfig;
use base64::engine::GeneralPurpose;
use base64::Engine;
use config::JgnesWebConfig;
use jgnes_core::audio::{DownsampleAction, DownsampleCounter, LowPassFilter};
use jgnes_core::{
    AudioPlayer, ColorEmphasis, Emulator, EmulatorConfig, EmulatorCreateArgs, InputPoller,
    JoypadState, Renderer, SaveWriter, TickEffect, TimingMode,
};
use jgnes_proc_macros::EnumDisplay;
use jgnes_renderer::config::{
    AspectRatio, FrameSkip, GpuFilterMode, Overscan, RendererConfig, VSyncMode, WgpuBackend,
};
use jgnes_renderer::WgpuRenderer;
use js_sys::Promise;
use rfd::AsyncFileDialog;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextOptions};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::platform::web::WindowExtWebSys;
use winit::window::{Fullscreen, Window, WindowBuilder, WindowId};

const BASE64_ENGINE: GeneralPurpose = base64::engine::general_purpose::STANDARD;

const FULLSCREEN_KEY: VirtualKeyCode = VirtualKeyCode::F8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumDisplay)]
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

fn alert_and_panic(s: &str) -> ! {
    js::alert(s);
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
        let sram_b64 = BASE64_ENGINE.encode(sram);
        js::saveToLocalStorage(&self.file_name, &sram_b64);
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
    p1_joypad_state: Rc<Cell<JoypadState>>,
    handler_state: InputHandlerState,
}

impl InputHandler {
    fn new(config: &InputConfig) -> Self {
        let mut initial_mapping: HashMap<VirtualKeyCode, Vec<NesButton>> = HashMap::new();
        for (button, keycode) in [
            (NesButton::Up, config.up),
            (NesButton::Left, config.left),
            (NesButton::Right, config.right),
            (NesButton::Down, config.down),
            (NesButton::A, config.a),
            (NesButton::B, config.b),
            (NesButton::Start, config.start),
            (NesButton::Select, config.select),
        ] {
            initial_mapping.entry(keycode).or_default().push(button);
        }

        Self {
            button_mapping: initial_mapping,
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

    fn handle_window_event(&mut self, event: &WindowEvent<'_>, config: &JgnesWebConfig) {
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
                        let mut joypad_state = self.p1_joypad_state.get();
                        let field = Self::get_field_mut(&mut joypad_state, button);
                        *field = match state {
                            ElementState::Pressed => true,
                            ElementState::Released => false,
                        };
                        self.p1_joypad_state.set(joypad_state);
                    }
                }
                InputHandlerState::WaitingForInput(button) => {
                    if *state == ElementState::Pressed {
                        self.button_mapping
                            .entry(*keycode)
                            .or_default()
                            .push(button);
                        self.p1_joypad_state.set(JoypadState::new());
                        self.handler_state = InputHandlerState::RunningEmulator;

                        config.inputs.borrow_mut().set_key(button, *keycode);

                        js::afterInputReconfigure(&format!("{button}"), &format!("{keycode:?}"));
                    }
                }
            }
        }
    }
}

struct WebInputPoller {
    p1_joypad_state: Rc<Cell<JoypadState>>,
}

impl InputPoller for WebInputPoller {
    fn poll_p1_input(&self) -> JoypadState {
        self.p1_joypad_state.get().sanitize_opposing_directions()
    }

    fn poll_p2_input(&self) -> JoypadState {
        JoypadState::default()
    }
}

struct WebAudioPlayer {
    audio_queue: AudioQueue,
    low_pass_filter: LowPassFilter,
    downsample_counter: DownsampleCounter,
    audio_enabled: Rc<Cell<bool>>,
}

impl WebAudioPlayer {
    fn new(audio_queue: AudioQueue, audio_enabled: Rc<Cell<bool>>) -> Self {
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

// This can cause audio lag in some configurations, but setting the threshold much lower than this
// is likely to cause audio skips
const AUDIO_QUEUE_THRESHOLD: u32 = 1200;

impl AudioPlayer for WebAudioPlayer {
    type Err = JsValue;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
        if !self.audio_enabled.get() {
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

    fn set_timing_mode(&mut self, timing_mode: TimingMode) {
        self.downsample_counter.set_timing_mode(timing_mode);
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

fn load_sav_bytes(file_name: &str) -> Option<Vec<u8>> {
    js::loadFromLocalStorage(file_name).and_then(|sav_b64| BASE64_ENGINE.decode(sav_b64).ok())
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

async fn upload_save_file(event_loop_proxy: EventLoopProxy<JgnesUserEvent>, file_name: String) {
    let Some(save_file) = AsyncFileDialog::new().add_filter("sav", &["sav"]).pick_file().await else { return };

    let save_bytes = save_file.read().await;

    event_loop_proxy
        .send_event(JgnesUserEvent::SaveFileLoaded {
            save_bytes,
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
    SaveFileLoaded {
        save_bytes: Vec<u8>,
        file_name: String,
    },
}

#[cfg(feature = "webgl")]
fn get_wgpu_backend() -> WgpuBackend {
    WgpuBackend::OpenGl
}

#[cfg(not(feature = "webgl"))]
fn get_wgpu_backend() -> WgpuBackend {
    WgpuBackend::WebGpu
}

#[allow(clippy::missing_panics_doc)]
#[wasm_bindgen]
pub async fn run_emulator(config: JgnesWebConfig) {
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

    let wgpu_backend = get_wgpu_backend();

    let gpu_filter_mode = config.gpu_filter_mode.get();
    let aspect_ratio = config.aspect_ratio.get();
    let overscan = config.overscan.get();
    let renderer = WgpuRenderer::from_window(
        window,
        window_size,
        RendererConfig {
            vsync_mode: VSyncMode::Enabled,
            wgpu_backend,
            gpu_filter_mode,
            aspect_ratio,
            overscan,
            frame_skip: FrameSkip::ZERO,
            forced_integer_height_scaling: false,
            use_webgl2_limits: wgpu_backend == WgpuBackend::OpenGl,
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

    let input_handler = InputHandler::new(&config.inputs.borrow());

    let state = State {
        emulator: None,
        renderer,
        audio_player,
        audio_ctx,
        input_handler,
        aspect_ratio: config.aspect_ratio.get(),
        filter_mode: config.gpu_filter_mode.get(),
        overscan: config.overscan.get(),
        user_interacted: false,
    };

    js::initComplete();

    run_event_loop(event_loop, config, state);
}

fn run_event_loop(
    event_loop: EventLoop<JgnesUserEvent>,
    config: JgnesWebConfig,
    mut state: State,
) -> ! {
    let event_loop_proxy = event_loop.create_proxy();

    // Used in white noise generator because rendering 60FPS is a bit much visually
    let mut odd_frame = false;

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

                match Emulator::create(EmulatorCreateArgs {
                    rom_bytes: file_bytes,
                    sav_bytes,
                    forced_timing_mode: None,
                    renderer: Rc::clone(&state.renderer),
                    audio_player: Rc::clone(&state.audio_player),
                    input_poller,
                    save_writer,
                }) {
                    Ok(emulator) => {
                        if !state.user_interacted {
                            state.user_interacted = true;

                            let _: Promise = state.audio_ctx.resume().unwrap();
                        }

                        set_rom_file_name_text(&file_name);
                        *config.current_filename.borrow_mut() = file_name;
                        js::setSaveButtonsEnabled(emulator.has_persistent_ram());
                        js::focusCanvas();
                        state.emulator = Some(emulator);
                    }
                    Err(err) => {
                        js::alert(&format!("Error initializing emulator: {err}"));
                        log::error!("Error initializing emulator: {err}");
                    }
                }
            }
            Event::UserEvent(JgnesUserEvent::SaveFileLoaded {
                save_bytes,
                file_name,
            }) => {
                let save_bytes_b64 = BASE64_ENGINE.encode(&save_bytes);
                js::saveToLocalStorage(&file_name, &save_bytes_b64);

                // Hard reset after uploading a save file
                state.emulator = state
                    .emulator
                    .take()
                    .map(|emulator| emulator.hard_reset(Some(save_bytes)));

                js::focusCanvas();
            }
            Event::WindowEvent {
                event: win_event,
                window_id,
            } if window_id == state.window_id() => {
                state.input_handler.handle_window_event(&win_event, &config);

                match win_event {
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(FULLSCREEN_KEY),
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    } => {
                        let mut renderer = state.renderer.borrow_mut();
                        let window = renderer.window_mut();

                        let new_fullscreen = match window.fullscreen() {
                            None => Some(Fullscreen::Borderless(None)),
                            Some(_) => None,
                        };
                        window.set_fullscreen(new_fullscreen);
                    }
                    WindowEvent::Resized(_) => {
                        let mut renderer = state.renderer.borrow_mut();
                        renderer.reconfigure_surface();

                        // Show cursor over canvas only when not in fullscreen mode
                        js::setCursorVisible(renderer.window().fullscreen().is_none());
                    }
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                }
            }
            Event::MainEventsCleared => {
                let config_aspect_ratio = config.aspect_ratio.get();
                if config_aspect_ratio != state.aspect_ratio {
                    state
                        .renderer
                        .borrow_mut()
                        .update_aspect_ratio(config_aspect_ratio);
                    state.aspect_ratio = config_aspect_ratio;
                }

                let config_filter_mode = config.gpu_filter_mode.get();
                if config_filter_mode != state.filter_mode {
                    state
                        .renderer
                        .borrow_mut()
                        .update_filter_mode(config_filter_mode);
                    state.filter_mode = config_filter_mode;
                }

                let config_overscan = config.overscan.get();
                if config_overscan != state.overscan {
                    state.renderer.borrow_mut().update_overscan(config_overscan);
                    state.overscan = config_overscan;
                }

                if config.open_file_requested.replace(false) {
                    wasm_bindgen_futures::spawn_local(open_file_in_event_loop(
                        event_loop_proxy.clone(),
                    ));
                }

                if config.reset_requested.replace(false) {
                    if let Some(emulator) = &mut state.emulator {
                        emulator.soft_reset();
                    }
                }

                if config.upload_save_file_requested.replace(false) {
                    wasm_bindgen_futures::spawn_local(upload_save_file(
                        event_loop_proxy.clone(),
                        config.current_filename(),
                    ));
                }

                if let Some(button) = config.reconfig_input_request.replace(None) {
                    state.input_handler.remove_mapping_for_button(button);
                    state.input_handler.handler_state = InputHandlerState::WaitingForInput(button);
                }

                // Don't tick the emulator while waiting for input configuration
                if !matches!(
                    state.input_handler.handler_state,
                    InputHandlerState::WaitingForInput(_)
                ) {
                    // If audio sync is enabled, only run the emulator if the audio queue isn't filling up
                    let audio_queue_len = state.audio_player.borrow().audio_queue.len().unwrap();
                    let should_wait_for_audio =
                        config.audio_sync_enabled.get() && audio_queue_len > AUDIO_QUEUE_THRESHOLD;
                    if !should_wait_for_audio {
                        match &mut state.emulator {
                            Some(emulator) => {
                                let emulator_config = EmulatorConfig {
                                    pal_black_border: false,
                                    silence_ultrasonic_triangle_output: config
                                        .silence_ultrasonic_triangle_output
                                        .get(),
                                };

                                // Tick the emulator until it renders the next frame
                                loop {
                                    match emulator.tick(&emulator_config) {
                                        Ok(TickEffect::None) => {}
                                        Ok(TickEffect::FrameRendered) => {
                                            break;
                                        }
                                        Err(err) => {
                                            // Assume emulator is now invalid
                                            state.emulator = None;
                                            js::alert(&format!(
                                                "Emulator terminated with error: {err:?}"
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                            None => {
                                odd_frame = !odd_frame;
                                if odd_frame {
                                    render_white_noise(&mut *state.renderer.borrow_mut()).unwrap();
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    })
}

fn render_white_noise<R: Renderer>(renderer: &mut R) -> Result<(), R::Err> {
    let frame_buffer = [[(); jgnes_core::SCREEN_WIDTH as usize];
        jgnes_core::SCREEN_HEIGHT as usize]
        .map(|arr| arr.map(|_| rand::random::<u8>() % 64));
    renderer.render_frame(&frame_buffer, ColorEmphasis::default())
}
