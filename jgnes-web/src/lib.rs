#![cfg(target_arch = "wasm32")]

mod audio;

use crate::audio::{AudioQueue, EnqueueResult};
use jgnes_core::audio::LowPassFilter;
use jgnes_core::{AudioPlayer, Emulator, InputPoller, JoypadState, SaveWriter, TickEffect};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RendererConfig, VSyncMode, WgpuBackend,
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

fn alert_and_panic(s: &str) -> ! {
    alert(s);
    panic!("{s}")
}

fn window_size(window: &Window) -> (u32, u32) {
    let PhysicalSize { width, height } = window.inner_size();
    (width, height)
}

struct Null;

impl SaveWriter for Null {
    type Err = ();

    fn persist_sram(&mut self, _sram: &[u8]) -> Result<(), Self::Err> {
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
}

impl WebAudioPlayer {
    fn new(audio_queue: AudioQueue) -> Self {
        Self {
            audio_queue,
            low_pass_filter: LowPassFilter::new(),
            sample_count: 0,
        }
    }
}

const AUDIO_OUTPUT_FREQUENCY: f64 = 48000.0;
const DISPLAY_RATE: f64 = 60.0;

impl AudioPlayer for WebAudioPlayer {
    type Err = JsValue;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
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
    emulator: Emulator<WgpuRenderer<Window>, WebAudioPlayer, WebInputPoller, Null>,
    input_handler: InputHandler,
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

#[allow(clippy::missing_panics_doc)]
#[wasm_bindgen]
pub async fn run() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .build(&event_loop)
        .expect("Unable to create window");

    window.set_inner_size(PhysicalSize::new(768, 672));

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
            gpu_filter_mode: GpuFilterMode::NearestNeighbor,
            aspect_ratio: AspectRatio::SquarePixels,
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

    let audio_ctx = AudioContext::new_with_context_options(
        AudioContextOptions::new().sample_rate(AUDIO_OUTPUT_FREQUENCY as f32),
    )
    .unwrap();
    let audio_queue = AudioQueue::new();
    let _audio_worklet = audio::initialize_audio_worklet(&audio_ctx, &audio_queue)
        .await
        .unwrap();

    let audio_player = WebAudioPlayer::new(audio_queue);

    let emulator = Emulator::create(
        file.read().await,
        None,
        renderer,
        audio_player,
        input_poller,
        Null,
    )
    .map_err(|err| alert_and_panic(&err.to_string()))
    .unwrap();

    let mut state = State {
        emulator,
        input_handler,
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
            // Tick the emulator until it renders the next frame
            while state.emulator.tick().expect("emulation error") != TickEffect::FrameRendered {}
        }
        _ => {}
    });
}
