mod audio;
mod colors;
mod config;
mod input;
mod render;

use crate::render::WgpuRenderer;
use jgnes_core::{
    AudioPlayer, ColorEmphasis, EmulationError, Emulator, FrameBuffer, InputPoller, JoypadState,
    Renderer, SaveWriter,
};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, EventType, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::video::{FullscreenType, Window};
use sdl2::EventPump;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{cmp, fs, thread};

use crate::audio::LowPassFilter;
use crate::config::RendererConfig;
use crate::input::SdlInputHandler;
pub use config::{
    AspectRatio, AxisDirection, HatDirection, InputConfig, InputConfigBase, JgnesDynamicConfig,
    JgnesNativeConfig, JoystickInput, JoystickInputConfig, KeyboardInput, KeyboardInputConfig,
    NativeRenderer, Overscan, PlayerInputConfig, VSyncMode,
};
pub use render::{GpuFilterMode, RenderScale};

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
        let display_area = determine_display_area(
            window_width,
            window_height,
            self.config.aspect_ratio,
            self.config.forced_integer_height_scaling,
        );

        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, display_area.to_sdl_rect())
            .map_err(anyhow::Error::msg)?;
        self.canvas.present();

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayArea {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl DisplayArea {
    fn to_sdl_rect(self) -> Rect {
        Rect::new(self.x as i32, self.y as i32, self.width, self.height)
    }
}

fn determine_display_area(
    window_width: u32,
    window_height: u32,
    aspect_ratio: AspectRatio,
    forced_integer_height_scaling: bool,
) -> DisplayArea {
    match aspect_ratio {
        AspectRatio::Stretched => DisplayArea {
            x: 0,
            y: 0,
            width: window_width,
            height: window_height,
        },
        AspectRatio::Ntsc | AspectRatio::SquarePixels | AspectRatio::FourThree => {
            let width_to_height_ratio = match aspect_ratio {
                AspectRatio::Ntsc => 64.0 / 49.0,
                AspectRatio::SquarePixels => 8.0 / 7.0,
                AspectRatio::FourThree => 4.0 / 3.0,
                AspectRatio::Stretched => unreachable!("nested match expressions"),
            };

            let visible_screen_height = u32::from(jgnes_core::VISIBLE_SCREEN_HEIGHT);

            let width = cmp::min(
                window_width,
                (f64::from(window_height) * width_to_height_ratio).round() as u32,
            );
            let height = cmp::min(
                window_height,
                (f64::from(width) / width_to_height_ratio).round() as u32,
            );
            let (width, height) =
                if forced_integer_height_scaling && height >= visible_screen_height {
                    let height = visible_screen_height * (height / visible_screen_height);
                    let width = cmp::min(
                        window_width,
                        (f64::from(height) * width_to_height_ratio).round() as u32,
                    );
                    (width, height)
                } else {
                    (width, height)
                };

            assert!(width <= window_width);
            assert!(height <= window_height);

            DisplayArea {
                x: (window_width - width) / 2,
                y: (window_height - height) / 2,
                width,
                height,
            }
        }
    }
}

struct SdlAudioPlayer {
    audio_queue: AudioQueue<f32>,
    sync_to_audio: bool,
    sample_queue: Vec<f32>,
    low_pass_filter: LowPassFilter,
    sample_count: u64,
}

impl SdlAudioPlayer {
    fn new(audio_queue: AudioQueue<f32>, sync_to_audio: bool) -> Self {
        Self {
            audio_queue,
            sync_to_audio,
            sample_queue: Vec::new(),
            low_pass_filter: LowPassFilter::new(),
            sample_count: 0,
        }
    }
}

impl AudioPlayer for SdlAudioPlayer {
    type Err = anyhow::Error;

    fn push_sample(&mut self, sample: f64) -> Result<(), Self::Err> {
        let prev_count = self.sample_count;
        self.sample_count += 1;

        self.low_pass_filter.collect_sample(sample);

        // TODO don't hardcode frequencies
        if (prev_count as f64 * 48000.0 / 1789772.72727273 * 60.0988 / 60.0).round() as u64
            != (self.sample_count as f64 * 48000.0 / 1789772.72727273 * 60.0988 / 60.0).round()
                as u64
        {
            self.sample_queue
                .push(self.low_pass_filter.output_sample() as f32);
        }

        // Arbitrary threshold
        if self.sample_queue.len() >= 16 {
            // 2048 samples * 4 bytes per sample
            while self.sync_to_audio && self.audio_queue.size() >= 8192 {
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

impl SdlWindowRenderer for WgpuRenderer {
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
        gpu_filter_mode: config.gpu_filter_mode,
        aspect_ratio: config.aspect_ratio,
        overscan: config.overscan.validate()?,
        forced_integer_height_scaling: config.forced_integer_height_scaling,
    };

    let audio_queue = audio_subsystem
        .open_queue(
            None,
            &AudioSpecDesired {
                freq: Some(48000),
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
    let display_area = determine_display_area(
        window_width,
        window_height,
        config.aspect_ratio,
        config.forced_integer_height_scaling,
    );
    log::info!(
        "Setting display area to {}x{} pixels with window size of {window_width}x{window_height} and aspect ratio {}",
        display_area.width, display_area.height, config.aspect_ratio
    );

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
            run_emulator(emulator, dynamic_config, event_pump, input_handler)
        }
        NativeRenderer::Wgpu => {
            let renderer = WgpuRenderer::from_window(window, renderer_config)?;
            let emulator = Emulator::create(
                rom_bytes,
                sav_bytes,
                renderer,
                audio_player,
                input_poller,
                save_writer,
            )?;
            run_emulator(emulator, dynamic_config, event_pump, input_handler)
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

fn run_emulator<
    R: Renderer<Err = anyhow::Error> + SdlWindowRenderer,
    A: AudioPlayer<Err = anyhow::Error>,
    I: InputPoller,
    S: SaveWriter<Err = anyhow::Error>,
>(
    mut emulator: Emulator<R, A, I, S>,
    dynamic_config: JgnesDynamicConfig,
    mut event_pump: EventPump,
    mut input_handler: SdlInputHandler<'_>,
) -> anyhow::Result<()> {
    let mut ticks = 0_u64;
    loop {
        if let Err(err) = emulator.tick() {
            return match err {
                EmulationError::Render(err)
                | EmulationError::Audio(err)
                | EmulationError::Save(err) => Err(err),
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
                    Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => {
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
                        keycode: Some(Keycode::F9),
                        ..
                    } => {
                        // TODO hotkey configuration
                        let window = emulator.get_renderer_mut().window_mut();
                        let new_fullscreen = match window.fullscreen_state() {
                            FullscreenType::Off => FullscreenType::Desktop,
                            _ => FullscreenType::Off,
                        };
                        window
                            .set_fullscreen(new_fullscreen)
                            .map_err(anyhow::Error::msg)?;
                    }
                    _ => {}
                }
            }
        }
    }
}
