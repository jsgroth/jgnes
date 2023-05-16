mod config;
mod input;

use jgnes_core::audio::{DownsampleAction, DownsampleCounter, LowPassFilter};
use jgnes_core::{
    AudioPlayer, ColorEmphasis, EmulationError, Emulator, EmulatorConfig, FrameBuffer, InputPoller,
    JoypadState, Renderer, SaveWriter,
};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, EventType, WindowEvent};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::video::{FullscreenType, Window};
use sdl2::EventPump;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{fs, thread};

pub use crate::config::{
    AxisDirection, HatDirection, HotkeyConfig, InputConfig, InputConfigBase, JgnesDynamicConfig,
    JgnesNativeConfig, JoystickInput, JoystickInputConfig, KeyboardInput, KeyboardInputConfig,
    NativeRenderer, PlayerInputConfig,
};
use crate::input::{Hotkey, SdlInputHandler};
use jgnes_renderer::config::{RendererConfig, VSyncMode};
use jgnes_renderer::{colors, WgpuRenderer};

struct SdlRenderer<'a> {
    canvas: WindowCanvas,
    texture: Texture<'a>,
    config: RendererConfig,
}

impl<'a> SdlRenderer<'a> {
    fn new<T>(
        canvas: WindowCanvas,
        texture_creator: &'a TextureCreator<T>,
        config: RendererConfig,
    ) -> anyhow::Result<Self> {
        let texture = texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24,
            jgnes_core::SCREEN_WIDTH.into(),
            jgnes_core::VISIBLE_SCREEN_HEIGHT.into(),
        )?;
        Ok(Self {
            canvas,
            texture,
            config,
        })
    }
}

impl<'a> Renderer for SdlRenderer<'a> {
    type Err = anyhow::Error;

    fn render_frame(
        &mut self,
        frame_buffer: &FrameBuffer,
        color_emphasis: ColorEmphasis,
    ) -> Result<(), Self::Err> {
        self.texture
            .with_lock(
                None,
                colors::sdl_texture_updater(frame_buffer, color_emphasis, self.config.overscan),
            )
            .map_err(anyhow::Error::msg)?;

        let (window_width, window_height) = self.canvas.window().size();
        let display_area = jgnes_renderer::determine_display_area(
            window_width,
            window_height,
            self.config.aspect_ratio,
            self.config.forced_integer_height_scaling,
        );

        self.canvas.clear();
        let rect = Rect::new(
            display_area.x as i32,
            display_area.y as i32,
            display_area.width,
            display_area.height,
        );
        self.canvas
            .copy(&self.texture, None, rect)
            .map_err(anyhow::Error::msg)?;
        self.canvas.present();

        Ok(())
    }
}

struct SdlAudioPlayer {
    audio_queue: AudioQueue<f32>,
    sync_to_audio: bool,
    sample_queue: Vec<f32>,
    low_pass_filter: LowPassFilter,
    downsample_counter: DownsampleCounter,
}

impl SdlAudioPlayer {
    fn new(audio_queue: AudioQueue<f32>, sync_to_audio: bool) -> Self {
        Self {
            audio_queue,
            sync_to_audio,
            sample_queue: Vec::new(),
            low_pass_filter: LowPassFilter::new(),
            downsample_counter: DownsampleCounter::new(AUDIO_OUTPUT_FREQUENCY, DISPLAY_RATE),
        }
    }
}

const AUDIO_OUTPUT_FREQUENCY: f64 = 48000.0;
const DISPLAY_RATE: f64 = 60.0;

impl AudioPlayer for SdlAudioPlayer {
    type Err = anyhow::Error;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
        self.low_pass_filter.collect_sample(sample);

        if self.downsample_counter.increment() == DownsampleAction::OutputSample {
            self.sample_queue
                .push(self.low_pass_filter.output_sample() as f32);
        }

        // Arbitrary threshold
        if self.sample_queue.len() >= 16 {
            // 1024 samples * 4 bytes per sample
            while self.sync_to_audio && self.audio_queue.size() >= 4096 {
                thread::sleep(Duration::from_micros(250));
            }

            if self.audio_queue.size() < 8192 {
                self.audio_queue
                    .queue_audio(&self.sample_queue)
                    .map_err(anyhow::Error::msg)?;
            }
            // If audio sync is disabled, intentionally drop samples while the audio queue is full
            self.sample_queue.clear();
        }

        Ok(())
    }
}

struct SdlInputPoller {
    p1_joypad_state: Rc<RefCell<JoypadState>>,
    p2_joypad_state: Rc<RefCell<JoypadState>>,
}

impl InputPoller for SdlInputPoller {
    fn poll_p1_input(&self) -> JoypadState {
        *self.p1_joypad_state.borrow()
    }

    fn poll_p2_input(&self) -> JoypadState {
        *self.p2_joypad_state.borrow()
    }
}

struct FsSaveWriter {
    path: PathBuf,
}

impl SaveWriter for FsSaveWriter {
    type Err = anyhow::Error;

    fn persist_sram(&mut self, sram: &[u8]) -> Result<(), Self::Err> {
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, sram)?;
        fs::rename(tmp_path, &self.path)?;

        Ok(())
    }
}

fn load_sav_file<P: AsRef<Path>>(path: P) -> Option<Vec<u8>> {
    fs::read(path.as_ref()).ok()
}

trait SdlWindowRenderer {
    fn window_mut(&mut self) -> &mut Window;

    fn reconfigure(&mut self);
}

impl<'a> SdlWindowRenderer for SdlRenderer<'a> {
    fn window_mut(&mut self) -> &mut Window {
        self.canvas.window_mut()
    }

    fn reconfigure(&mut self) {
        // nothing to do
    }
}

impl SdlWindowRenderer for WgpuRenderer<Window> {
    fn window_mut(&mut self) -> &mut Window {
        self.window_mut()
    }

    fn reconfigure(&mut self) {
        self.reconfigure_surface();
    }
}

/// Run the emulator in a loop until it terminates.
///
/// # Errors
///
/// This function will return an error if any issues are encountered rendering graphics, playing
/// audio, or writing a save file.
pub fn run(config: &JgnesNativeConfig, dynamic_config: JgnesDynamicConfig) -> anyhow::Result<()> {
    log::info!("Running with config:\n{config}");

    let Some(file_name) = Path::new(&config.nes_file_path)
        .file_name()
        .and_then(OsStr::to_str)
    else {
        return Err(anyhow::Error::msg(format!("cannot determine file name of {}", config.nes_file_path)));
    };

    let rom_bytes = fs::read(Path::new(&config.nes_file_path))?;

    let sdl_ctx = sdl2::init().map_err(anyhow::Error::msg)?;
    let video_subsystem = sdl_ctx.video().map_err(anyhow::Error::msg)?;
    let audio_subsystem = sdl_ctx.audio().map_err(anyhow::Error::msg)?;
    let joystick_subsystem = sdl_ctx.joystick().map_err(anyhow::Error::msg)?;

    sdl_ctx.mouse().show_cursor(false);

    let mut window_builder = video_subsystem.window(
        &format!("jgnes - {file_name}"),
        config.window_width,
        config.window_height,
    );
    if config.launch_fullscreen {
        window_builder.fullscreen_desktop();
    }
    let window = init_window(window_builder.build()?)?;

    let renderer_config = RendererConfig {
        vsync_mode: config.vsync_mode,
        wgpu_backend: config.wgpu_backend,
        gpu_filter_mode: config.gpu_filter_mode,
        aspect_ratio: config.aspect_ratio,
        overscan: config.overscan,
        forced_integer_height_scaling: config.forced_integer_height_scaling,
        use_webgl2_limits: false,
    };

    let audio_queue = audio_subsystem
        .open_queue(
            None,
            &AudioSpecDesired {
                freq: Some(AUDIO_OUTPUT_FREQUENCY as i32),
                channels: Some(1),
                samples: Some(1024),
            },
        )
        .map_err(anyhow::Error::msg)?;
    audio_queue.resume();
    let audio_player = SdlAudioPlayer::new(audio_queue, config.sync_to_audio);

    let input_poller = SdlInputPoller {
        p1_joypad_state: Rc::default(),
        p2_joypad_state: Rc::default(),
    };
    let input_handler = SdlInputHandler::new(
        &joystick_subsystem,
        &config.input_config,
        Rc::clone(&input_poller.p1_joypad_state),
        Rc::clone(&input_poller.p2_joypad_state),
    );

    let sav_path = Path::new(&config.nes_file_path).with_extension("sav");
    let sav_bytes = load_sav_file(&sav_path);
    let save_writer = FsSaveWriter {
        path: sav_path.clone(),
    };

    if sav_bytes.is_some() {
        log::info!("Loaded SRAM from {}", sav_path.display());
    }

    let mut event_pump = sdl_ctx.event_pump().map_err(anyhow::Error::msg)?;
    event_pump.disable_event(EventType::MouseMotion);

    let (window_width, window_height) = window.size();
    let display_area = jgnes_renderer::determine_display_area(
        window_width,
        window_height,
        config.aspect_ratio,
        config.forced_integer_height_scaling,
    );
    log::info!(
        "Setting display area to {}x{} pixels with window size of {window_width}x{window_height} and aspect ratio {}",
        display_area.width, display_area.height, config.aspect_ratio
    );

    let save_state_path = Path::new(&config.nes_file_path).with_extension("ss0");

    let emulator_config = EmulatorConfig {
        silence_ultrasonic_triangle_output: config.silence_ultrasonic_triangle_output,
    };

    match config.renderer {
        NativeRenderer::Sdl2 => {
            let mut canvas_builder = window.into_canvas();
            if config.vsync_mode == VSyncMode::Enabled {
                canvas_builder = canvas_builder.present_vsync();
            }
            let canvas = canvas_builder.build()?;
            let texture_creator = canvas.texture_creator();
            let renderer = SdlRenderer::new(canvas, &texture_creator, renderer_config)?;

            let emulator = Emulator::create(
                rom_bytes,
                sav_bytes,
                renderer,
                audio_player,
                input_poller,
                save_writer,
            )?;
            run_emulator(
                emulator,
                emulator_config,
                dynamic_config,
                event_pump,
                input_handler,
                &save_state_path,
            )
        }
        NativeRenderer::Wgpu => {
            let renderer = pollster::block_on(WgpuRenderer::from_window(
                window,
                Window::size,
                renderer_config,
            ))?;
            let emulator = Emulator::create(
                rom_bytes,
                sav_bytes,
                renderer,
                audio_player,
                input_poller,
                save_writer,
            )?;
            run_emulator(
                emulator,
                emulator_config,
                dynamic_config,
                event_pump,
                input_handler,
                &save_state_path,
            )
        }
    }
}

fn init_window(window: Window) -> Result<Window, anyhow::Error> {
    let mut canvas = window.into_canvas().present_vsync().build()?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    Ok(canvas.into_window())
}

fn run_emulator<R, A, I, S, P>(
    mut emulator: Emulator<R, A, I, S>,
    emulator_config: EmulatorConfig,
    dynamic_config: JgnesDynamicConfig,
    mut event_pump: EventPump,
    mut input_handler: SdlInputHandler<'_>,
    save_state_path: P,
) -> anyhow::Result<()>
where
    R: Renderer<Err = anyhow::Error> + SdlWindowRenderer,
    A: AudioPlayer<Err = anyhow::Error>,
    I: InputPoller,
    S: SaveWriter<Err = anyhow::Error>,
    P: AsRef<Path>,
{
    let save_state_path = save_state_path.as_ref();

    let mut ticks = 0_u64;
    loop {
        if let Err(err) = emulator.tick(&emulator_config) {
            return match err {
                EmulationError::Render(err)
                | EmulationError::Audio(err)
                | EmulationError::Save(err) => Err(err),
                EmulationError::CpuInvalidOpcode(..) => Err(anyhow::Error::msg(err.to_string())),
            };
        }

        ticks += 1;
        if ticks % 15000 == 0 {
            if dynamic_config.quit_signal.load(Ordering::Relaxed) {
                return Ok(());
            }

            for event in event_pump.poll_iter() {
                input_handler.handle_event(&event)?;

                match event {
                    Event::Quit { .. } => {
                        return Ok(());
                    }
                    Event::Window { win_event, .. } => match win_event {
                        WindowEvent::FocusGained
                        | WindowEvent::FocusLost
                        | WindowEvent::TakeFocus
                        | WindowEvent::SizeChanged(..)
                        | WindowEvent::Resized(..)
                        | WindowEvent::Moved(..)
                        | WindowEvent::DisplayChanged(..)
                        | WindowEvent::Minimized
                        | WindowEvent::Maximized
                        | WindowEvent::Restored
                        | WindowEvent::Shown => {
                            emulator.get_renderer_mut().reconfigure();
                        }
                        _ => {}
                    },
                    Event::KeyDown {
                        keycode: Some(keycode),
                        ..
                    } => {
                        for hotkey in input_handler.check_for_hotkeys(keycode) {
                            match hotkey {
                                Hotkey::Quit => {
                                    return Ok(());
                                }
                                Hotkey::ToggleFullscreen => {
                                    let window = emulator.get_renderer_mut().window_mut();
                                    let new_fullscreen = match window.fullscreen_state() {
                                        FullscreenType::Off => FullscreenType::Desktop,
                                        _ => FullscreenType::Off,
                                    };
                                    window
                                        .set_fullscreen(new_fullscreen)
                                        .map_err(anyhow::Error::msg)?;
                                }
                                Hotkey::SaveState => {
                                    emulator.save_state(File::create(save_state_path)?)?;
                                    log::info!("Saved state to '{}'", save_state_path.display());
                                }
                                Hotkey::LoadState => match File::open(save_state_path) {
                                    Ok(file) => match emulator.load_state(file) {
                                        Ok(..) => {
                                            log::info!(
                                                "Successfully loaded save state from '{}'",
                                                save_state_path.display()
                                            );
                                        }
                                        Err(err) => {
                                            log::error!(
                                                "Error loading state from '{}': {err}",
                                                save_state_path.display()
                                            );
                                        }
                                    },
                                    Err(err) => {
                                        log::error!(
                                            "Cannot open file at '{}': {err}",
                                            save_state_path.display()
                                        );
                                    }
                                },
                                Hotkey::SoftReset => {
                                    log::info!("Performing soft reset");
                                    emulator.soft_reset();
                                }
                                Hotkey::HardReset => {
                                    log::info!("Performing hard reset");
                                    emulator = emulator.hard_reset();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
