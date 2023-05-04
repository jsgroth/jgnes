mod colors;
mod render;

use crate::render::WgpuRenderer;
use jgnes_core::{
    AudioPlayer, ColorEmphasis, EmulationError, Emulator, FrameBuffer, InputPoller, JoypadState,
    Renderer, SaveWriter,
};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use render::{GpuFilterMode, RenderScale};

struct SdlRenderer<'a> {
    canvas: WindowCanvas,
    texture: Texture<'a>,
}

impl<'a> SdlRenderer<'a> {
    fn new<T>(
        canvas: WindowCanvas,
        texture_creator: &'a TextureCreator<T>,
    ) -> anyhow::Result<Self> {
        let texture = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, 256, 224)?;
        Ok(Self { canvas, texture })
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
                colors::sdl_texture_updater(frame_buffer, color_emphasis),
            )
            .map_err(anyhow::Error::msg)?;

        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .map_err(anyhow::Error::msg)?;
        self.canvas.present();

        Ok(())
    }
}

struct SdlAudioPlayer {
    audio_queue: AudioQueue<f32>,
}

impl AudioPlayer for SdlAudioPlayer {
    type Err = anyhow::Error;

    fn push_samples(&mut self, samples: &[f32]) -> Result<(), Self::Err> {
        self.audio_queue
            .queue_audio(samples)
            .map_err(anyhow::Error::msg)?;

        Ok(())
    }
}

struct SdlInputPoller {
    joypad_state: Rc<RefCell<JoypadState>>,
}

impl InputPoller for SdlInputPoller {
    fn poll_p1_input(&self) -> JoypadState {
        *self.joypad_state.borrow()
    }

    fn poll_p2_input(&self) -> JoypadState {
        JoypadState::default()
    }
}

struct SdlInputHandler {
    joypad_state: Rc<RefCell<JoypadState>>,
}

impl SdlInputHandler {
    fn set_field(&mut self, keycode: Keycode, value: bool) {
        match keycode {
            Keycode::Up => {
                self.joypad_state.borrow_mut().up = value;
            }
            Keycode::Down => {
                self.joypad_state.borrow_mut().down = value;
            }
            Keycode::Left => {
                self.joypad_state.borrow_mut().left = value;
            }
            Keycode::Right => {
                self.joypad_state.borrow_mut().right = value;
            }
            Keycode::Z => {
                self.joypad_state.borrow_mut().a = value;
            }
            Keycode::X => {
                self.joypad_state.borrow_mut().b = value;
            }
            Keycode::Return => {
                self.joypad_state.borrow_mut().start = value;
            }
            Keycode::RShift => {
                self.joypad_state.borrow_mut().select = value;
            }
            _ => {}
        };
    }

    fn key_down(&mut self, keycode: Keycode) {
        self.set_field(keycode, true);

        // Don't allow inputs of opposite directions
        match keycode {
            Keycode::Up => {
                self.joypad_state.borrow_mut().down = false;
            }
            Keycode::Down => {
                self.joypad_state.borrow_mut().up = false;
            }
            Keycode::Left => {
                self.joypad_state.borrow_mut().right = false;
            }
            Keycode::Right => {
                self.joypad_state.borrow_mut().left = false;
            }
            _ => {}
        }
    }

    fn key_up(&mut self, keycode: Keycode) {
        self.set_field(keycode, false);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRenderer {
    Sdl2,
    Wgpu,
}

impl Display for NativeRenderer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sdl2 => write!(f, "Sdl2"),
            Self::Wgpu => write!(f, "Wgpu"),
        }
    }
}

impl FromStr for NativeRenderer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Sdl2" => Ok(Self::Sdl2),
            "Wgpu" => Ok(Self::Wgpu),
            _ => Err(format!("invalid renderer string: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JgnesNativeConfig {
    pub nes_file_path: String,
    pub window_width: u32,
    pub window_height: u32,
    pub renderer: NativeRenderer,
    pub gpu_filter_mode: GpuFilterMode,
}

impl Display for JgnesNativeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "nes_file_path: {}", self.nes_file_path)?;
        writeln!(f, "window_width: {}", self.window_width)?;
        writeln!(f, "window_height: {}", self.window_height)?;
        writeln!(f, "renderer: {}", self.renderer)?;
        writeln!(f, "gpu_filter_mode: {}", self.gpu_filter_mode)?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JgnesDynamicConfig {
    pub quit_signal: Arc<AtomicBool>,
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

    sdl_ctx.mouse().show_cursor(false);

    let window = video_subsystem
        .window(
            &format!("jgnes - {file_name}"),
            config.window_width,
            config.window_height,
        )
        .build()?;
    let mut canvas = window.into_canvas().present_vsync().build()?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let texture_creator = canvas.texture_creator();

    let renderer: Box<dyn Renderer<Err = anyhow::Error>> = match config.renderer {
        NativeRenderer::Sdl2 => Box::new(SdlRenderer::new(canvas, &texture_creator)?),
        NativeRenderer::Wgpu => Box::new(WgpuRenderer::from_window(
            canvas.into_window(),
            config.gpu_filter_mode,
        )?),
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
    let audio_player = SdlAudioPlayer { audio_queue };

    let input_poller = SdlInputPoller {
        joypad_state: Rc::default(),
    };
    let mut input_handler = SdlInputHandler {
        joypad_state: Rc::clone(&input_poller.joypad_state),
    };

    let sav_path = Path::new(&config.nes_file_path).with_extension("sav");
    let sav_bytes = load_sav_file(&sav_path);
    let save_writer = FsSaveWriter {
        path: sav_path.clone(),
    };

    if sav_bytes.is_some() {
        log::info!("Loaded SRAM from {}", sav_path.display());
    }

    let mut event_pump = sdl_ctx.event_pump().map_err(anyhow::Error::msg)?;

    let mut emulator = Emulator::create(
        rom_bytes,
        sav_bytes,
        renderer,
        audio_player,
        input_poller,
        save_writer,
    )?;

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
                match event {
                    Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => {
                        return Ok(());
                    }
                    Event::KeyDown {
                        keycode: Some(keycode),
                        ..
                    } => {
                        input_handler.key_down(keycode);
                    }
                    Event::KeyUp {
                        keycode: Some(keycode),
                        ..
                    } => {
                        input_handler.key_up(keycode);
                    }
                    _ => {}
                }
            }
        }
    }
}
