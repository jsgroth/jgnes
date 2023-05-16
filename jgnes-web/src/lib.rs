#![cfg(target_arch = "wasm32")]

mod audio;

use crate::audio::{AudioQueue, EnqueueResult};
use base64::engine::general_purpose;
use base64::Engine;
use jgnes_core::audio::{DownsampleAction, DownsampleCounter, LowPassFilter};
use jgnes_core::{AudioPlayer, Emulator, InputPoller, JoypadState, SaveWriter, TickEffect};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RenderScale, RendererConfig, VSyncMode, WgpuBackend,
};
use jgnes_renderer::WgpuRenderer;
use js_sys::Uint8Array;
use rfd::AsyncFileDialog;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextOptions};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
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

struct InputHandler {
    p1_joypad_state: Rc<RefCell<JoypadState>>,
}

impl InputHandler {
    fn get_field_mut(joypad_state: &mut JoypadState, keycode: VirtualKeyCode) -> Option<&mut bool> {
        let field = match keycode {
            VirtualKeyCode::Up => &mut joypad_state.up,
            VirtualKeyCode::Left => &mut joypad_state.left,
            VirtualKeyCode::Right => &mut joypad_state.right,
            VirtualKeyCode::Down => &mut joypad_state.down,
            VirtualKeyCode::Z => &mut joypad_state.a,
            VirtualKeyCode::X => &mut joypad_state.b,
            VirtualKeyCode::Return => &mut joypad_state.start,
            VirtualKeyCode::RShift => &mut joypad_state.select,
            _ => return None,
        };
        Some(field)
    }

    fn handle_window_event(&self, event: &WindowEvent<'_>) {
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
            let mut joypad_state = self.p1_joypad_state.borrow_mut();
            if let Some(field) = Self::get_field_mut(&mut joypad_state, *keycode) {
                *field = match state {
                    ElementState::Pressed => true,
                    ElementState::Released => false,
                };
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
    input_handler: InputHandler,
    aspect_ratio: AspectRatio,
    filter_mode: GpuFilterMode,
    overscan: Overscan,
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

async fn open_file_in_event_loop(event_loop_proxy: EventLoopProxy<(Vec<u8>, String)>) {
    let Some(file) = AsyncFileDialog::new().add_filter("nes", &["nes"]).pick_file().await else { return };

    let file_bytes = file.read().await;
    let file_name = file.file_name();
    event_loop_proxy
        .send_event((file_bytes, file_name))
        .unwrap();
}

#[allow(clippy::missing_panics_doc)]
#[wasm_bindgen]
pub async fn run(config: JgnesWebConfig) {
    let event_loop = EventLoopBuilder::<(Vec<u8>, String)>::with_user_event().build();
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

    let input_handler = InputHandler {
        p1_joypad_state: Rc::default(),
    };
    let input_poller = WebInputPoller {
        p1_joypad_state: Rc::clone(&input_handler.p1_joypad_state),
    };

    let emulator = match AsyncFileDialog::new()
        .add_filter("nes", &["nes"])
        .pick_file()
        .await
    {
        Some(file) => {
            let sav_bytes = load_sav_bytes(&file.file_name());

            match Emulator::create(
                file.read().await,
                sav_bytes,
                Rc::clone(&renderer),
                Rc::clone(&audio_player),
                input_poller,
                WebSaveWriter {
                    file_name: file.file_name(),
                },
            ) {
                Ok(emulator) => {
                    set_rom_file_name_text(&file.file_name());
                    *config.current_filename.borrow_mut() = file.file_name();
                    set_download_save_enabled(emulator.has_persistent_ram());
                    Some(emulator)
                }
                Err(err) => {
                    alert(&format!("Error initializing emulator: {err}"));
                    log::error!("Error initializing emulator: {err}");
                    None
                }
            }
        }
        None => None,
    };

    let mut state = State {
        emulator,
        renderer,
        input_handler,
        aspect_ratio: *config.aspect_ratio.borrow(),
        filter_mode: *config.gpu_filter_mode.borrow(),
        overscan: *config.overscan.borrow(),
    };

    let event_loop_proxy = event_loop.create_proxy();

    event_loop.run(move |event, _, control_flow| match event {
        Event::UserEvent((file_bytes, file_name)) => {
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
                Rc::clone(&audio_player),
                input_poller,
                save_writer,
            ) {
                Ok(emulator) => {
                    set_rom_file_name_text(&file_name);
                    *config.current_filename.borrow_mut() = file_name;
                    set_download_save_enabled(emulator.has_persistent_ram());
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

            // If audio sync is enabled, only run the emulator if the audio queue isn't filling up
            if !*config.audio_sync_enabled.borrow()
                || audio_player.borrow().audio_queue.len().unwrap() <= 1024
            {
                // Tick the emulator until it renders the next frame
                if let Some(emulator) = &mut state.emulator {
                    while emulator.tick().expect("emulation error") != TickEffect::FrameRendered {}
                }
            }
        }
        _ => {}
    });
}
