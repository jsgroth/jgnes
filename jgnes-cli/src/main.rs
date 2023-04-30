use env_logger::Env;
use jgnes_core::{
    AudioPlayer, ColorEmphasis, EmulationError, Emulator, FrameBuffer, InputPoller, JoypadState,
    Renderer,
};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::path::Path;
use std::rc::Rc;
use std::{env, fs};

// TODO do colors properly
const COLOR_MAPPING: &[u8; 8 * 64 * 3] = include_bytes!("../../nespalette.pal");

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
        let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
        self.texture
            .with_lock(None, |pixels, pitch| {
                for (i, scanline) in frame_buffer[8..232].iter().enumerate() {
                    for (j, nes_color) in scanline.iter().copied().enumerate() {
                        let color_map_index = color_emphasis_offset + (3 * nes_color) as usize;
                        let start = i * pitch + 3 * j;
                        pixels[start..start + 3]
                            .copy_from_slice(&COLOR_MAPPING[color_map_index..color_map_index + 3]);
                    }
                }
            })
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

fn get_color_emphasis_offset(color_emphasis: ColorEmphasis) -> u16 {
    64 * u16::from(color_emphasis.red)
        + 128 * u16::from(color_emphasis.green)
        + 256 * u16::from(color_emphasis.blue)
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut args = env::args();
    args.next();

    let path = args.next().expect("missing filename");
    let file_name = Path::new(&path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap();

    let file_bytes = fs::read(Path::new(&path))?;

    let sdl_ctx = sdl2::init().map_err(anyhow::Error::msg)?;
    let video_subsystem = sdl_ctx.video().map_err(anyhow::Error::msg)?;
    let audio_subsystem = sdl_ctx.audio().map_err(anyhow::Error::msg)?;

    let window = video_subsystem
        .window(&format!("jgnes - {file_name}"), 3 * 256, 3 * 224)
        .build()?;
    let mut canvas = window.into_canvas().present_vsync().build()?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let texture_creator = canvas.texture_creator();

    let renderer = SdlRenderer::new(canvas, &texture_creator)?;

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

    let mut event_pump = sdl_ctx.event_pump().map_err(anyhow::Error::msg)?;

    let mut emulator = Emulator::create(&file_bytes, renderer, audio_player, input_poller)?;

    let mut ticks = 0_u64;
    loop {
        if let Err(err) = emulator.tick() {
            return match err {
                EmulationError::Render(err) | EmulationError::Audio(err) => Err(err),
            };
        }

        ticks += 1;
        if ticks % 15000 == 0 {
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
