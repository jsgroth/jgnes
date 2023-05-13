#![cfg(target_arch = "wasm32")]

mod audio;

use crate::audio::{AudioQueue, EnqueueResult};
use jgnes_core::audio::LowPassFilter;
use jgnes_core::{AudioPlayer, Emulator, InputPoller, JoypadState, SaveWriter, TickEffect};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RenderScale, RendererConfig, VSyncMode, WgpuBackend,
};
use jgnes_renderer::WgpuRenderer;
use rfd::AsyncFileDialog;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextOptions};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::web::WindowExtWebSys;
use winit::window::{Window, WindowBuilder};

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen(module = "/js/save-writer.js")]
extern "C" {
    fn loadFromLocalStorage(key: &str) -> Option<String>;

    fn saveToLocalStorage(key: &str, value: &str);
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
    type Err = String;

    fn persist_sram(&mut self, sram: &[u8]) -> Result<(), Self::Err> {
        let sram_hex: String = sram
            .iter()
            .copied()
            .map(|byte| format!("{byte:02X}"))
            .collect();
        saveToLocalStorage(&self.file_name, &sram_hex);
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
        *self.p1_joypad_state.borrow()
    }

    fn poll_p2_input(&self) -> JoypadState {
        JoypadState::default()
    }
}

struct WebAudioPlayer {
    audio_queue: AudioQueue,
    low_pass_filter: LowPassFilter,
    sample_count: u64,
    audio_enabled: Rc<RefCell<bool>>,
}

impl WebAudioPlayer {
    fn new(audio_queue: AudioQueue, audio_enabled: Rc<RefCell<bool>>) -> Self {
        Self {
            audio_queue,
            low_pass_filter: LowPassFilter::new(),
            sample_count: 0,
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

        self.sample_count += 1;
        if jgnes_core::audio::should_output_sample(
            self.sample_count,
            AUDIO_OUTPUT_FREQUENCY,
            DISPLAY_RATE,
        ) {
            let output_sample = self.low_pass_filter.output_sample();
            if self.audio_queue.push_if_space(output_sample as f32)? == EnqueueResult::BufferFull {
                log::warn!("Audio queue is full, dropping sample");
            }
        }

        Ok(())
    }
}

struct State {
    emulator: Emulator<WgpuRenderer<Window>, WebAudioPlayer, WebInputPoller, WebSaveWriter>,
    input_handler: InputHandler,
    aspect_ratio: AspectRatio,
    filter_mode: GpuFilterMode,
    overscan: Overscan,
}

impl State {
    fn window(&self) -> &Window {
        self.emulator.get_renderer().window()
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
    reset_requested: Rc<RefCell<bool>>,
}

#[wasm_bindgen]
impl JgnesWebConfig {
    #[wasm_bindgen(constructor)]
    pub fn new() -> JgnesWebConfig {
        JgnesWebConfig {
            aspect_ratio: Rc::new(RefCell::new(AspectRatio::Ntsc)),
            gpu_filter_mode: Rc::new(RefCell::new(GpuFilterMode::NearestNeighbor)),
            overscan: Rc::default(),
            audio_enabled: Rc::new(RefCell::new(true)),
            reset_requested: Rc::new(RefCell::new(false)),
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

    pub fn reset_emulator(&self) {
        *self.reset_requested.borrow_mut() = true;
    }

    pub fn clone(&self) -> JgnesWebConfig {
        <JgnesWebConfig as Clone>::clone(self)
    }
}

fn set_overscan_field(value: bool, field: &mut u8) {
    *field = if value { 8 } else { 0 };
}

#[allow(clippy::missing_panics_doc)]
#[wasm_bindgen]
pub async fn run(config: JgnesWebConfig) {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .build(&event_loop)
        .expect("Unable to create window");

    window.set_inner_size(PhysicalSize::new(878, 672));

    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let dst = doc.get_element_by_id("jgnes-wasm")?;
            let canvas = web_sys::Element::from(window.canvas());
            dst.append_child(&canvas).ok()?;
            Some(())
        })
        .expect("Couldn't append canvas to document body");

    let renderer = WgpuRenderer::from_window(
        window,
        window_size,
        RendererConfig {
            vsync_mode: VSyncMode::Enabled,
            wgpu_backend: WgpuBackend::BrowserAuto,
            gpu_filter_mode: *config.gpu_filter_mode.borrow(),
            aspect_ratio: *config.aspect_ratio.borrow(),
            overscan: Overscan::default(),
            forced_integer_height_scaling: false,
            use_webgl2_limits: true,
        },
    )
    .await
    .map_err(|err| alert_and_panic(&err.to_string()))
    .unwrap();

    let input_handler = InputHandler {
        p1_joypad_state: Rc::default(),
    };
    let input_poller = WebInputPoller {
        p1_joypad_state: Rc::clone(&input_handler.p1_joypad_state),
    };

    let file = AsyncFileDialog::new()
        .pick_file()
        .await
        .unwrap_or_else(|| alert_and_panic("no file selected"));

    let sav_bytes = loadFromLocalStorage(&file.file_name()).map(|hex| {
        let mut sav_bytes = Vec::with_capacity(hex.len() / 2);
        for i in 0..hex.len() / 2 {
            let byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
                .expect("invalid hex char in save bytes");
            sav_bytes.push(byte);
        }
        sav_bytes
    });

    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let dst = doc.get_element_by_id("rom-file-name")?;
            dst.set_text_content(Some(&file.file_name()));
            Some(())
        })
        .expect("Unable to write file name into the DOM");

    let audio_ctx = AudioContext::new_with_context_options(
        AudioContextOptions::new().sample_rate(AUDIO_OUTPUT_FREQUENCY as f32),
    )
    .unwrap();
    let audio_queue = AudioQueue::new();
    let _audio_worklet = audio::initialize_audio_worklet(&audio_ctx, &audio_queue)
        .await
        .unwrap();

    let audio_player = WebAudioPlayer::new(audio_queue, Rc::clone(&config.audio_enabled));

    let emulator = Emulator::create(
        file.read().await,
        sav_bytes,
        renderer,
        audio_player,
        input_poller,
        WebSaveWriter {
            file_name: file.file_name(),
        },
    )
    .map_err(|err| alert_and_panic(&err.to_string()))
    .unwrap();

    let mut state = State {
        emulator,
        input_handler,
        aspect_ratio: *config.aspect_ratio.borrow(),
        filter_mode: *config.gpu_filter_mode.borrow(),
        overscan: *config.overscan.borrow(),
    };

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: win_event,
            window_id,
        } if window_id == state.window().id() => {
            state.input_handler.handle_window_event(&win_event);

            if let WindowEvent::CloseRequested = win_event {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::MainEventsCleared => {
            let config_aspect_ratio = *config.aspect_ratio.borrow();
            if config_aspect_ratio != state.aspect_ratio {
                state
                    .emulator
                    .get_renderer_mut()
                    .update_aspect_ratio(config_aspect_ratio);
                state.aspect_ratio = config_aspect_ratio;
            }

            let config_filter_mode = *config.gpu_filter_mode.borrow();
            if config_filter_mode != state.filter_mode {
                state
                    .emulator
                    .get_renderer_mut()
                    .update_filter_mode(config_filter_mode);
                state.filter_mode = config_filter_mode;
            }

            let config_overscan = *config.overscan.borrow();
            if config_overscan != state.overscan {
                state
                    .emulator
                    .get_renderer_mut()
                    .update_overscan(config_overscan);
                state.overscan = config_overscan;
            }

            if *config.reset_requested.borrow() {
                *config.reset_requested.borrow_mut() = false;
                state.emulator.soft_reset();
            }

            // Tick the emulator until it renders the next frame
            while state.emulator.tick().expect("emulation error") != TickEffect::FrameRendered {}
        }
        _ => {}
    });
}
