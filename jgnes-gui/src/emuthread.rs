use jgnes_native_driver::{
    AxisDirection, HatDirection, InputCollectResult, InputType, JgnesNativeConfig, JoystickInput,
};
use sdl2::event::Event;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::WindowCanvas;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::{process, thread};

pub(crate) enum EmuThreadTask {
    RunEmulator(Box<JgnesNativeConfig>),
    CollectInput { input_type: InputType, axis_deadzone: u16 },
}

#[must_use]
pub(crate) fn start(
    is_running: Arc<AtomicBool>,
    emulation_error: Arc<Mutex<Option<anyhow::Error>>>,
) -> (Sender<EmuThreadTask>, Receiver<Option<InputCollectResult>>) {
    let (task_sender, task_receiver) = mpsc::channel();
    let (input_sender, input_receiver) = mpsc::channel();

    thread::spawn(move || {
        std::panic::set_hook(Box::new(|panic_info| {
            log::error!("Emulation thread panicked, killing process: {panic_info}");
            process::exit(1);
        }));

        loop {
            let task = match task_receiver.recv() {
                Ok(task) => task,
                Err(err) => {
                    log::info!(
                        "Emulation thread terminating due to recv error (most likely caused by closing main window): {err}"
                    );
                    return;
                }
            };

            match task {
                EmuThreadTask::RunEmulator(config) => {
                    run_emulator(config, &is_running, &emulation_error);
                }
                EmuThreadTask::CollectInput { input_type, axis_deadzone } => {
                    match collect_input(input_type, axis_deadzone) {
                        Ok(collect_result) => {
                            if let Err(err) = input_sender.send(collect_result) {
                                log::info!(
                                    "Emulation thread terminating due to send error (most likely caused by closing main window): {err}"
                                );
                                return;
                            }
                        }
                        Err(err) => {
                            log::error!("Error collecting controller input: {err}");
                            if let Err(err) = input_sender.send(None) {
                                log::info!(
                                    "Emulation thread terminating due to send error (most likely caused by closing main window): {err}"
                                );
                                return;
                            }
                        }
                    }
                }
            }
        }
    });

    (task_sender, input_receiver)
}

fn run_emulator(
    config: Box<JgnesNativeConfig>,
    is_running: &Arc<AtomicBool>,
    emulation_error: &Arc<Mutex<Option<anyhow::Error>>>,
) {
    is_running.store(true, Ordering::Relaxed);
    if let Err(err) = jgnes_native_driver::run(&config) {
        *emulation_error.lock().unwrap() = Some(err);
    }

    is_running.store(false, Ordering::Relaxed);
}

fn collect_input(
    input_type: InputType,
    axis_deadzone: u16,
) -> Result<Option<InputCollectResult>, anyhow::Error> {
    let sdl_ctx = sdl2::init().map_err(anyhow::Error::msg)?;
    let video_subsystem = sdl_ctx.video().map_err(anyhow::Error::msg)?;
    let joystick_subsystem = sdl_ctx.joystick().map_err(anyhow::Error::msg)?;

    let window_title = match input_type {
        InputType::Keyboard => "Press a key",
        InputType::Gamepad => "Press a button",
    };
    let window = video_subsystem.window(window_title, 600, 100).build()?;
    let mut canvas = window.into_canvas().present_vsync().build()?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let mut event_pump = sdl_ctx.event_pump().map_err(anyhow::Error::msg)?;

    let mut joysticks = HashMap::new();
    let mut instance_id_to_device_id: HashMap<u32, u32> = HashMap::new();

    loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    return Ok(None);
                }
                Event::KeyDown { keycode: Some(keycode), .. }
                    if input_type == InputType::Keyboard =>
                {
                    return Ok(Some(InputCollectResult::Keyboard(keycode)));
                }
                Event::JoyDeviceAdded { which: device_id, .. }
                    if input_type == InputType::Gamepad =>
                {
                    let joystick = joystick_subsystem.open(device_id)?;
                    instance_id_to_device_id.insert(joystick.instance_id(), device_id);
                    joysticks.insert(device_id, joystick);
                }
                Event::JoyDeviceRemoved { which: instance_id, .. }
                    if input_type == InputType::Gamepad =>
                {
                    if let Some(device_id) = instance_id_to_device_id.remove(&instance_id) {
                        joysticks.remove(&device_id);
                    }
                }
                Event::JoyButtonDown { which: instance_id, button_idx, .. }
                    if input_type == InputType::Gamepad =>
                {
                    if let Some(&device_id) = instance_id_to_device_id.get(&instance_id) {
                        return Ok(Some(InputCollectResult::Gamepad(JoystickInput::Button {
                            device_id,
                            button_idx,
                        })));
                    }
                }
                Event::JoyAxisMotion { which: instance_id, axis_idx, value, .. }
                    if input_type == InputType::Gamepad =>
                {
                    if let Some(&device_id) = instance_id_to_device_id.get(&instance_id) {
                        if value.saturating_abs() as u16 >= axis_deadzone {
                            let direction = AxisDirection::from_value(value);
                            return Ok(Some(InputCollectResult::Gamepad(JoystickInput::Axis {
                                device_id,
                                axis_idx,
                                direction,
                            })));
                        }
                    }
                }
                Event::JoyHatMotion { which: instance_id, hat_idx, state, .. }
                    if input_type == InputType::Gamepad =>
                {
                    if let Some(&device_id) = instance_id_to_device_id.get(&instance_id) {
                        if let Some(direction) = HatDirection::from_hat_state(state) {
                            return Ok(Some(InputCollectResult::Gamepad(JoystickInput::Hat {
                                device_id,
                                hat_idx,
                                direction,
                            })));
                        }
                    }
                }
                _ => {}
            }
        }

        fill_with_random_colors(&mut canvas)?;
    }
}

fn fill_with_random_colors(canvas: &mut WindowCanvas) -> Result<(), anyhow::Error> {
    let (width, height) = canvas.window().size();
    let texture_creator = canvas.texture_creator();
    let mut texture =
        texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, width, height)?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();

    texture
        .with_lock(None, |pixels, pitch| {
            for i in 0..height as usize {
                for j in 0..width as usize {
                    let start = i * pitch + 3 * j;
                    pixels[start..start + 3].copy_from_slice(&[
                        rand::random(),
                        rand::random(),
                        rand::random(),
                    ]);
                }
            }
        })
        .map_err(anyhow::Error::msg)?;

    canvas.copy(&texture, None, None).map_err(anyhow::Error::msg)?;
    canvas.present();

    Ok(())
}
