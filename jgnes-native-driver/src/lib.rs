mod config;
mod input;

use jgnes_core::audio::{DownsampleAction, DownsampleCounter, LowPassFilter};
use jgnes_core::{
    AudioPlayer, ColorEmphasis, EmulationError, EmulationState, Emulator, EmulatorConfig,
    EmulatorCreateArgs, FrameBuffer, InputPoller, JoypadState, Renderer, SaveWriter, TickEffect,
    TimingMode,
};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, EventType, WindowEvent};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, TextureValueError, WindowCanvas};
use sdl2::video::{FullscreenType, Window};
use sdl2::EventPump;
use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};
use std::{fs, thread};
use thiserror::Error;

pub use crate::config::{
    AxisDirection, HatDirection, HotkeyConfig, InputCollectResult, InputConfig, InputConfigBase,
    InputType, JgnesDynamicConfig, JgnesNativeConfig, JgnesSharedConfig, JoystickInput,
    JoystickInputConfig, KeyboardInput, KeyboardInputConfig, NativeRenderer, PlayerInputConfig,
};
use crate::input::{Hotkey, SdlInputHandler};
use jgnes_renderer::config::{FrameSkip, RendererConfig, VSyncMode};
use jgnes_renderer::{colors, WgpuRenderer};

const SDL_PIXEL_FORMAT: PixelFormatEnum = PixelFormatEnum::RGB24;

#[derive(Debug, Error)]
enum SdlRendererError {
    #[error("Error creating SDL2 texture: {source}")]
    CreateTexture {
        #[from]
        source: TextureValueError,
    },
    #[error("Error in SDL2 renderer: {msg}")]
    Other { msg: String },
}

impl SdlRendererError {
    fn msg(s: impl Into<String>) -> Self {
        Self::Other { msg: s.into() }
    }
}

struct SdlRenderer<'a, T> {
    canvas: WindowCanvas,
    texture_creator: &'a TextureCreator<T>,
    texture: Texture<'a>,
    config: RendererConfig,
    total_frames: u64,
    timing_mode: TimingMode,
}

impl<'a, T> SdlRenderer<'a, T> {
    fn new(
        canvas: WindowCanvas,
        texture_creator: &'a TextureCreator<T>,
        config: RendererConfig,
    ) -> anyhow::Result<Self> {
        let texture = texture_creator.create_texture_streaming(
            SDL_PIXEL_FORMAT,
            jgnes_core::SCREEN_WIDTH.into(),
            TimingMode::Ntsc.visible_screen_height().into(),
        )?;
        Ok(Self {
            canvas,
            texture_creator,
            texture,
            config,
            total_frames: 0,
            timing_mode: TimingMode::Ntsc,
        })
    }
}

impl<'a, T> Renderer for SdlRenderer<'a, T> {
    type Err = SdlRendererError;

    fn render_frame(
        &mut self,
        frame_buffer: &FrameBuffer,
        color_emphasis: ColorEmphasis,
    ) -> Result<(), Self::Err> {
        self.total_frames += 1;

        if self.config.frame_skip.should_skip(self.total_frames) {
            return Ok(());
        }

        self.texture
            .with_lock(
                None,
                colors::sdl_texture_updater(
                    frame_buffer,
                    color_emphasis,
                    self.config.overscan,
                    self.timing_mode,
                ),
            )
            .map_err(SdlRendererError::msg)?;

        let (window_width, window_height) = self.canvas.window().size();
        let display_area = jgnes_renderer::determine_display_area(
            window_width,
            window_height,
            self.config.aspect_ratio,
            self.config.forced_integer_height_scaling,
            self.timing_mode,
        );

        self.canvas.clear();
        let dst = Rect::new(
            display_area.x as i32,
            display_area.y as i32,
            display_area.width,
            display_area.height,
        );
        self.canvas
            .copy(&self.texture, None, dst)
            .map_err(SdlRendererError::msg)?;
        self.canvas.present();

        Ok(())
    }

    fn set_timing_mode(&mut self, timing_mode: TimingMode) -> Result<(), Self::Err> {
        self.timing_mode = timing_mode;

        self.texture = self.texture_creator.create_texture_streaming(
            SDL_PIXEL_FORMAT,
            jgnes_core::SCREEN_WIDTH.into(),
            timing_mode.visible_screen_height().into(),
        )?;

        Ok(())
    }
}

struct SdlAudioPlayer {
    audio_queue: AudioQueue<f32>,
    sync_to_audio: bool,
    sample_queue: Vec<f32>,
    low_pass_filter: LowPassFilter,
    downsample_counter: DownsampleCounter,
    frame_skip: FrameSkip,
    total_output_samples: u64,
}

impl SdlAudioPlayer {
    fn new(audio_queue: AudioQueue<f32>, sync_to_audio: bool) -> Self {
        Self {
            audio_queue,
            sync_to_audio,
            sample_queue: Vec::new(),
            low_pass_filter: LowPassFilter::new(),
            downsample_counter: DownsampleCounter::new(AUDIO_OUTPUT_FREQUENCY, DISPLAY_RATE),
            frame_skip: FrameSkip::ZERO,
            total_output_samples: 0,
        }
    }
}

const AUDIO_OUTPUT_FREQUENCY: f64 = 48000.0;
const DEVICE_BUFFER_SIZE: u16 = 64;
const DISPLAY_RATE: f64 = 60.0;
const SAMPLES_PER_FRAME: usize = 800;

impl AudioPlayer for SdlAudioPlayer {
    type Err = anyhow::Error;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
        self.low_pass_filter.collect_sample(sample);

        if self.downsample_counter.increment() == DownsampleAction::OutputSample {
            self.total_output_samples += 1;

            if !self.frame_skip.should_skip(self.total_output_samples) {
                self.sample_queue
                    .push(self.low_pass_filter.output_sample() as f32);
            }
        }

        if self.sample_queue.len() >= SAMPLES_PER_FRAME {
            // 1024 samples * 4 bytes per sample
            while self.sync_to_audio && self.audio_queue.size() >= 4096 {
                sleep(Duration::from_micros(250));
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

    fn set_timing_mode(&mut self, timing_mode: TimingMode) {
        self.downsample_counter.set_timing_mode(timing_mode);
    }
}

struct SdlInputPoller {
    p1_joypad_state: Rc<Cell<JoypadState>>,
    p2_joypad_state: Rc<Cell<JoypadState>>,
}

impl InputPoller for SdlInputPoller {
    #[inline]
    fn poll_p1_input(&self) -> JoypadState {
        self.p1_joypad_state.get()
    }

    #[inline]
    fn poll_p2_input(&self) -> JoypadState {
        self.p2_joypad_state.get()
    }
}

struct FsSaveWriter {
    path: PathBuf,
}

impl SaveWriter for FsSaveWriter {
    type Err = anyhow::Error;

    #[inline]
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

    fn set_frame_skip(&mut self, frame_skip: FrameSkip);

    fn handle_resize(&mut self);

    fn reload_config(&mut self, config: &JgnesDynamicConfig) -> Result<(), anyhow::Error>;
}

impl<'a, T> SdlWindowRenderer for SdlRenderer<'a, T> {
    fn window_mut(&mut self) -> &mut Window {
        self.canvas.window_mut()
    }

    fn set_frame_skip(&mut self, frame_skip: FrameSkip) {
        self.config.frame_skip = frame_skip;
    }

    fn handle_resize(&mut self) {
        // nothing to do
    }

    fn reload_config(&mut self, config: &JgnesDynamicConfig) -> Result<(), anyhow::Error> {
        self.config.aspect_ratio = config.aspect_ratio;
        self.config.overscan = config.overscan;
        self.config.forced_integer_height_scaling = config.forced_integer_height_scaling;
        // VSync mode is not configurable for the SDL2 renderer and filter mode is not applicable

        Ok(())
    }
}

impl SdlWindowRenderer for WgpuRenderer<Window> {
    fn window_mut(&mut self) -> &mut Window {
        self.window_mut()
    }

    fn set_frame_skip(&mut self, frame_skip: FrameSkip) {
        self.update_frame_skip(frame_skip);
    }

    fn handle_resize(&mut self) {
        self.reconfigure_surface();
    }

    fn reload_config(&mut self, config: &JgnesDynamicConfig) -> Result<(), anyhow::Error> {
        self.update_filter_mode(config.gpu_filter_mode);
        self.update_prescaling_mode(config.prescaling_mode);
        self.update_aspect_ratio(config.aspect_ratio);
        self.update_overscan(config.overscan);
        self.update_forced_integer_height_scaling(config.forced_integer_height_scaling);
        self.update_vsync_mode(config.vsync_mode)?;

        Ok(())
    }
}

/// Run the emulator in a loop until it terminates.
///
/// # Errors
///
/// This function will return an error if any issues are encountered rendering graphics, playing
/// audio, or writing a save file.
///
/// # Panics
///
/// This function will panic if it is unable to claim the dynamic config lock while attempting to
/// reload the dynamic config, which should only happen if another thread panics while holding that
/// lock.
pub fn run(config: &JgnesNativeConfig) -> anyhow::Result<()> {
    let dynamic_config = &config.shared_config.dynamic_config;

    log::info!("Running with config:\n{config}");
    {
        let dynamic_config = &*dynamic_config.lock().unwrap();
        log::info!("Initial dynamic config:\n{dynamic_config}");
    }

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
    window_builder.resizable();

    if config.launch_fullscreen {
        window_builder.fullscreen_desktop();
    }

    let window = init_window(window_builder.build()?)?;

    let renderer_config = {
        let dynamic_config = dynamic_config.lock().unwrap();

        RendererConfig {
            vsync_mode: dynamic_config.vsync_mode,
            wgpu_backend: config.wgpu_backend,
            gpu_filter_mode: dynamic_config.gpu_filter_mode,
            prescaling_mode: dynamic_config.prescaling_mode,
            aspect_ratio: dynamic_config.aspect_ratio,
            overscan: dynamic_config.overscan,
            frame_skip: FrameSkip::ZERO,
            forced_integer_height_scaling: dynamic_config.forced_integer_height_scaling,
            use_webgl2_limits: false,
        }
    };

    let audio_queue = audio_subsystem
        .open_queue(
            None,
            &AudioSpecDesired {
                freq: Some(AUDIO_OUTPUT_FREQUENCY as i32),
                channels: Some(1),
                samples: Some(DEVICE_BUFFER_SIZE),
            },
        )
        .map_err(anyhow::Error::msg)?;
    audio_queue.resume();
    let audio_player =
        SdlAudioPlayer::new(audio_queue, dynamic_config.lock().unwrap().sync_to_audio);

    let input_poller = SdlInputPoller {
        p1_joypad_state: Rc::default(),
        p2_joypad_state: Rc::default(),
    };
    let input_handler = SdlInputHandler::new(
        &joystick_subsystem,
        &dynamic_config.lock().unwrap().input_config,
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

    let save_state_path = Path::new(&config.nes_file_path).with_extension("ss0");

    match config.renderer {
        NativeRenderer::Sdl2 => {
            let mut canvas_builder = window.into_canvas();
            if renderer_config.vsync_mode == VSyncMode::Enabled {
                canvas_builder = canvas_builder.present_vsync();
            }
            let canvas = canvas_builder.build()?;
            let texture_creator = canvas.texture_creator();
            let renderer = SdlRenderer::new(canvas, &texture_creator, renderer_config)?;

            let emulator = Emulator::create(EmulatorCreateArgs {
                rom_bytes,
                sav_bytes,
                forced_timing_mode: config.forced_timing_mode,
                renderer,
                audio_player,
                input_poller,
                save_writer,
            })?;
            run_emulator(
                emulator,
                config,
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
            let emulator = Emulator::create(EmulatorCreateArgs {
                rom_bytes,
                sav_bytes,
                forced_timing_mode: config.forced_timing_mode,
                renderer,
                audio_player,
                input_poller,
                save_writer,
            })?;
            run_emulator(
                emulator,
                config,
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

struct RewindState {
    previous_states: VecDeque<EmulationState>,
    frame_count: u64,
    rewind_buffer_len: usize,
    rewinding: bool,
}

const REWIND_RECORD_INTERVAL: u64 = 3;
// 3 * 16.6~ ms
const THREE_FRAME_TIMES_NANOS: u64 = 50_000_000;

impl RewindState {
    fn compute_rewind_buffer_len(rewind_buffer_len: Duration) -> usize {
        (rewind_buffer_len.as_nanos() / u128::from(THREE_FRAME_TIMES_NANOS)) as usize
    }

    fn new(rewind_buffer_len: Duration) -> Self {
        let rewind_buffer_len = Self::compute_rewind_buffer_len(rewind_buffer_len);
        Self {
            previous_states: VecDeque::new(),
            frame_count: 0,
            rewind_buffer_len,
            rewinding: false,
        }
    }

    fn reload_buffer_len(&mut self, rewind_buffer_len: Duration) {
        self.rewind_buffer_len = Self::compute_rewind_buffer_len(rewind_buffer_len);
    }

    // Should be called once per frame; will internally store state every 3rd frame
    fn record<R, A, I, S>(&mut self, emulator: &Emulator<R, A, I, S>) {
        self.frame_count += 1;
        if self.frame_count % REWIND_RECORD_INTERVAL == 0 {
            self.previous_states.push_back(emulator.snapshot_state());

            while self.previous_states.len() > self.rewind_buffer_len {
                self.previous_states.pop_front();
            }
        }
    }

    // Rewind to the most recent previous state, and then sleep for the appropriate amount of time.
    // If the rewind buffer is empty then this method will do nothing and immediately return.
    fn rewind_once<R: Renderer, A, I, S>(
        &mut self,
        emulator: &mut Emulator<R, A, I, S>,
    ) -> Result<(), R::Err> {
        if let Some(state) = self.previous_states.pop_back() {
            emulator.load_state_snapshot(state);

            let start_time = SystemTime::now();
            emulator.force_render()?;

            while SystemTime::now().duration_since(start_time).unwrap()
                < Duration::from_nanos(THREE_FRAME_TIMES_NANOS)
            {
                sleep(Duration::from_micros(250));
            }
        }

        Ok(())
    }
}

// Windows needs a special implementation of sleep because by default, std::thread::sleep will always
// sleep for a minimum of about 15ms on Windows. The timeBeginPeriod syscall makes it possible to
// reduce the timer period to about 1ms.
#[cfg(target_os = "windows")]
fn sleep(duration: Duration) {
    // SAFETY: FFI calls to Windows syscalls. `thread::sleep` cannot panic, so each `timeBeginPeriod`
    // call will have a corresponding `timeEndPeriod` call with the same period value.
    unsafe {
        windows::Win32::Media::timeBeginPeriod(1);
        thread::sleep(duration);
        windows::Win32::Media::timeEndPeriod(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn sleep(duration: Duration) {
    thread::sleep(duration);
}

fn run_emulator<R, I, S, P>(
    mut emulator: Emulator<R, SdlAudioPlayer, I, S>,
    native_config: &JgnesNativeConfig,
    mut event_pump: EventPump,
    mut input_handler: SdlInputHandler<'_>,
    save_state_path: P,
) -> anyhow::Result<()>
where
    R: Renderer + SdlWindowRenderer,
    R::Err: std::error::Error + Send + Sync + 'static,
    I: InputPoller,
    S: SaveWriter<Err = anyhow::Error>,
    P: AsRef<Path>,
{
    let JgnesSharedConfig {
        dynamic_config,
        config_reload_signal,
        quit_signal,
        input_reconfigure_sender,
        input_reconfigure_signal,
    } = &native_config.shared_config;

    let save_state_path = save_state_path.as_ref();

    let mut emulator_config = EmulatorConfig::default();
    let mut fast_forward_multiplier;
    let mut rewind_state;

    {
        let dynamic_config = dynamic_config.lock().unwrap();

        dynamic_config.update_emulator_config(&mut emulator_config);
        fast_forward_multiplier = dynamic_config.fast_forward_multiplier;
        rewind_state = RewindState::new(dynamic_config.rewind_buffer_len);
    };

    let mut ticks = 0_u64;
    loop {
        if !rewind_state.rewinding {
            match emulator.tick(&emulator_config) {
                Ok(TickEffect::None) => {}
                Ok(TickEffect::FrameRendered) => {
                    rewind_state.record(&emulator);
                }
                Err(err) => {
                    return match err {
                        EmulationError::Render(err) => Err(err.into()),
                        EmulationError::Audio(err) | EmulationError::Save(err) => Err(err),
                    };
                }
            }

            ticks += 1;
        }

        if rewind_state.rewinding {
            rewind_state.rewind_once(&mut emulator)?;
        }

        if ticks % 15000 == 0 || rewind_state.rewinding {
            if quit_signal.load(Ordering::Relaxed) {
                return Ok(());
            }

            if config_reload_signal.load(Ordering::Relaxed) {
                config_reload_signal.store(false, Ordering::Relaxed);

                let dynamic_config = &*dynamic_config.lock().unwrap();

                log::info!("Reloading dynamic config: {dynamic_config}");

                dynamic_config.update_emulator_config(&mut emulator_config);

                let renderer = emulator.get_renderer_mut();
                renderer.reload_config(dynamic_config)?;

                emulator.get_audio_player_mut().sync_to_audio = dynamic_config.sync_to_audio;

                input_handler.reload_input_config(&dynamic_config.input_config);

                fast_forward_multiplier = dynamic_config.fast_forward_multiplier;
                rewind_state.reload_buffer_len(dynamic_config.rewind_buffer_len);
            }

            if let Some(input_type) =
                InputType::from_discriminant(input_reconfigure_signal.load(Ordering::Relaxed))
            {
                input_reconfigure_signal
                    .store(JgnesSharedConfig::NO_INPUT_RECONFIGURE, Ordering::Relaxed);

                // Attempt to ensure that pressed inputs will go to the SDL2 window; does not appear
                // to work on all platforms / window managers
                emulator.get_renderer_mut().window_mut().raise();

                match handle_input_reconfigure(input_type, &mut event_pump, &mut input_handler)? {
                    InputReconfigureResult::Input(input_collect_result) => {
                        log::info!("Sending input collect result {input_collect_result:?}");
                        input_reconfigure_sender
                            .send(Some(input_collect_result))
                            .unwrap();
                    }
                    InputReconfigureResult::Quit => {
                        input_reconfigure_sender.send(None).unwrap();
                        return Ok(());
                    }
                }
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
                            emulator.get_renderer_mut().handle_resize();
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
                                    emulator = emulator.hard_reset(None);
                                }
                                Hotkey::FastForward => {
                                    let frame_skip =
                                        FrameSkip(fast_forward_multiplier.saturating_sub(1));

                                    emulator.get_renderer_mut().set_frame_skip(frame_skip);
                                    emulator.get_audio_player_mut().frame_skip = frame_skip;
                                }
                                Hotkey::Rewind => {
                                    rewind_state.rewinding = true;
                                }
                            }
                        }
                    }
                    Event::KeyUp {
                        keycode: Some(keycode),
                        ..
                    } => {
                        for hotkey in input_handler.check_for_hotkeys(keycode) {
                            match hotkey {
                                Hotkey::FastForward => {
                                    emulator.get_renderer_mut().set_frame_skip(FrameSkip::ZERO);
                                    emulator.get_audio_player_mut().frame_skip = FrameSkip::ZERO;
                                }
                                Hotkey::Rewind => {
                                    rewind_state.rewinding = false;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputReconfigureResult {
    Input(InputCollectResult),
    Quit,
}

fn handle_input_reconfigure(
    input_type: InputType,
    event_pump: &mut EventPump,
    input_handler: &mut SdlInputHandler<'_>,
) -> Result<InputReconfigureResult, anyhow::Error> {
    log::info!("Input reconfigure requested for input type {input_type:?}");

    let axis_deadzone = input_handler.axis_deadzone();

    loop {
        for event in event_pump.poll_iter() {
            input_handler.handle_event(&event)?;

            match event {
                Event::Quit { .. } => {
                    return Ok(InputReconfigureResult::Quit);
                }
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } if input_type == InputType::Keyboard => {
                    return Ok(InputReconfigureResult::Input(InputCollectResult::Keyboard(
                        keycode,
                    )));
                }
                Event::JoyButtonDown {
                    which: instance_id,
                    button_idx,
                    ..
                } if input_type == InputType::Gamepad => {
                    if let Some(device_id) = input_handler.device_id_for(instance_id) {
                        return Ok(InputReconfigureResult::Input(InputCollectResult::Gamepad(
                            JoystickInput::Button {
                                device_id,
                                button_idx,
                            },
                        )));
                    }
                }
                Event::JoyAxisMotion {
                    which: instance_id,
                    axis_idx,
                    value,
                    ..
                } if input_type == InputType::Gamepad => {
                    if value.saturating_abs() as u16 >= axis_deadzone {
                        if let Some(device_id) = input_handler.device_id_for(instance_id) {
                            let direction = AxisDirection::from_value(value);
                            return Ok(InputReconfigureResult::Input(InputCollectResult::Gamepad(
                                JoystickInput::Axis {
                                    device_id,
                                    axis_idx,
                                    direction,
                                },
                            )));
                        }
                    }
                }
                Event::JoyHatMotion {
                    which: instance_id,
                    hat_idx,
                    state,
                    ..
                } if input_type == InputType::Gamepad => {
                    if let (Some(device_id), Some(direction)) = (
                        input_handler.device_id_for(instance_id),
                        HatDirection::from_hat_state(state),
                    ) {
                        return Ok(InputReconfigureResult::Input(InputCollectResult::Gamepad(
                            JoystickInput::Hat {
                                device_id,
                                hat_idx,
                                direction,
                            },
                        )));
                    }
                }
                _ => {}
            }
        }

        sleep(Duration::from_millis(1));
    }
}
