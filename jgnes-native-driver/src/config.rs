use jgnes_core::{EmulatorConfig, TimingMode};
use jgnes_proc_macros::{EnumDisplay, EnumFromStr};
use jgnes_renderer::config::{
    AspectRatio, GpuFilterMode, Overscan, RendererConfig, Scanlines, Shader, VSyncMode, WgpuBackend,
};
use sdl2::joystick::HatState;
use sdl2::keyboard::Keycode;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumDisplay, EnumFromStr,
)]
pub enum NativeRenderer {
    Sdl2,
    #[default]
    Wgpu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfigBase<T> {
    pub up: Option<T>,
    pub left: Option<T>,
    pub right: Option<T>,
    pub down: Option<T>,
    pub a: Option<T>,
    pub b: Option<T>,
    pub start: Option<T>,
    pub select: Option<T>,
}

impl<T> Default for InputConfigBase<T> {
    fn default() -> Self {
        Self {
            up: None,
            left: None,
            right: None,
            down: None,
            a: None,
            b: None,
            start: None,
            select: None,
        }
    }
}

impl<T: Display> Display for InputConfigBase<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Up={}, Left={}, Right={}, Down={}, A={}, B={}, Start={}, Select={}",
            fmt_option(self.up.as_ref()),
            fmt_option(self.left.as_ref()),
            fmt_option(self.right.as_ref()),
            fmt_option(self.down.as_ref()),
            fmt_option(self.a.as_ref()),
            fmt_option(self.b.as_ref()),
            fmt_option(self.start.as_ref()),
            fmt_option(self.select.as_ref()),
        )
    }
}

fn fmt_option<T: Display>(option: Option<&T>) -> String {
    option.map_or("<None>".into(), ToString::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyboardInput(String);

impl KeyboardInput {
    fn to_keycode(&self) -> Keycode {
        Keycode::from_name(&self.0).expect("KeyboardInput should never contain an invalid keycode")
    }
}

impl From<Keycode> for KeyboardInput {
    fn from(value: Keycode) -> Self {
        Self(value.name())
    }
}

impl TryFrom<KeyboardInput> for Keycode {
    type Error = String;

    fn try_from(value: KeyboardInput) -> Result<Self, Self::Error> {
        Keycode::from_name(&value.0).ok_or_else(|| format!("invalid keycode name: {}", value.0))
    }
}

impl Display for KeyboardInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumDisplay)]
pub enum AxisDirection {
    Positive,
    Negative,
}

impl AxisDirection {
    #[must_use]
    pub fn from_value(value: i16) -> Self {
        if value >= 0 {
            // Arbitrarily assign 0 to positive direction; this function should generally not be
            // called with 0 due to deadzone
            Self::Positive
        } else {
            Self::Negative
        }
    }

    fn sign_str(self) -> &'static str {
        match self {
            Self::Positive => "+",
            Self::Negative => "-",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, EnumDisplay)]
pub enum HatDirection {
    #[default]
    Up,
    Left,
    Right,
    Down,
}

impl HatDirection {
    pub(crate) const ALL: [Self; 4] = [Self::Up, Self::Left, Self::Right, Self::Down];

    #[must_use]
    pub fn from_hat_state(state: HatState) -> Option<Self> {
        match state {
            HatState::Up => Some(HatDirection::Up),
            HatState::Left => Some(HatDirection::Left),
            HatState::Right => Some(HatDirection::Right),
            HatState::Down => Some(HatDirection::Down),
            // Ignore diagonals
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JoystickInput {
    Button {
        device_id: u32,
        button_idx: u8,
    },
    Axis {
        device_id: u32,
        axis_idx: u8,
        direction: AxisDirection,
    },
    Hat {
        device_id: u32,
        hat_idx: u8,
        direction: HatDirection,
    },
}

impl Display for JoystickInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Button {
                device_id,
                button_idx,
            } => write!(f, "Joy {device_id} Button {button_idx}"),
            Self::Axis {
                device_id,
                axis_idx,
                direction,
            } => write!(
                f,
                "Joy {device_id} Axis {axis_idx} {}",
                direction.sign_str()
            ),
            Self::Hat {
                device_id,
                hat_idx,
                direction,
            } => write!(f, "Joy {device_id} Hat {hat_idx} {direction}"),
        }
    }
}

pub type KeyboardInputConfig = InputConfigBase<KeyboardInput>;
pub type JoystickInputConfig = InputConfigBase<JoystickInput>;

impl InputConfigBase<KeyboardInput> {
    pub(crate) fn to_keycode_config(&self) -> InputConfigBase<Keycode> {
        InputConfigBase {
            up: self.up.as_ref().map(KeyboardInput::to_keycode),
            left: self.left.as_ref().map(KeyboardInput::to_keycode),
            right: self.right.as_ref().map(KeyboardInput::to_keycode),
            down: self.down.as_ref().map(KeyboardInput::to_keycode),
            a: self.a.as_ref().map(KeyboardInput::to_keycode),
            b: self.b.as_ref().map(KeyboardInput::to_keycode),
            start: self.start.as_ref().map(KeyboardInput::to_keycode),
            select: self.select.as_ref().map(KeyboardInput::to_keycode),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInputConfig {
    pub keyboard: KeyboardInputConfig,
    pub joystick: JoystickInputConfig,
}

impl Display for PlayerInputConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        writeln!(f, "    Keyboard: {}", self.keyboard)?;
        write!(f, "    Joystick: {}", self.joystick)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub quit: Option<String>,
    pub toggle_fullscreen: Option<String>,
    pub save_state: Option<String>,
    pub load_state: Option<String>,
    pub soft_reset: Option<String>,
    pub hard_reset: Option<String>,
    pub fast_forward: Option<String>,
    pub rewind: Option<String>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            quit: Some(Keycode::Escape.name()),
            toggle_fullscreen: Some(Keycode::F9.name()),
            save_state: Some(Keycode::F5.name()),
            load_state: Some(Keycode::F6.name()),
            soft_reset: Some(Keycode::F3.name()),
            hard_reset: Some(Keycode::F4.name()),
            fast_forward: Some(Keycode::Tab.name()),
            rewind: Some(Keycode::Backquote.name()),
        }
    }
}

impl Display for HotkeyConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        writeln!(f, "    Quit: {}", fmt_option(self.quit.as_ref()))?;
        writeln!(
            f,
            "    Toggle Fullscreen: {}",
            fmt_option(self.toggle_fullscreen.as_ref())
        )?;
        writeln!(
            f,
            "    Save State: {}",
            fmt_option(self.save_state.as_ref())
        )?;
        writeln!(
            f,
            "    Load State: {}",
            fmt_option(self.load_state.as_ref())
        )?;
        writeln!(
            f,
            "    Soft Reset: {}",
            fmt_option(self.soft_reset.as_ref())
        )?;
        writeln!(
            f,
            "    Hard Reset: {}",
            fmt_option(self.hard_reset.as_ref())
        )?;
        writeln!(
            f,
            "    Fast Forward: {}",
            fmt_option(self.fast_forward.as_ref())
        )?;
        write!(f, "    Rewind: {}", fmt_option(self.rewind.as_ref()))?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfig {
    pub p1: PlayerInputConfig,
    pub p2: PlayerInputConfig,
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    pub axis_deadzone: u16,
    pub allow_opposite_directions: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        let p1_keyboard = KeyboardInputConfig {
            up: Some(KeyboardInput(Keycode::Up.name())),
            left: Some(KeyboardInput(Keycode::Left.name())),
            right: Some(KeyboardInput(Keycode::Right.name())),
            down: Some(KeyboardInput(Keycode::Down.name())),
            a: Some(KeyboardInput(Keycode::Z.name())),
            b: Some(KeyboardInput(Keycode::X.name())),
            start: Some(KeyboardInput(Keycode::Return.name())),
            select: Some(KeyboardInput(Keycode::RShift.name())),
        };
        Self {
            p1: PlayerInputConfig {
                keyboard: p1_keyboard,
                joystick: JoystickInputConfig::default(),
            },
            p2: PlayerInputConfig {
                keyboard: KeyboardInputConfig::default(),
                joystick: JoystickInputConfig::default(),
            },
            hotkeys: HotkeyConfig::default(),
            axis_deadzone: 5000,
            allow_opposite_directions: false,
        }
    }
}

impl Display for InputConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        writeln!(f, "  Player 1: {}", self.p1)?;
        writeln!(f, "  Player 2: {}", self.p2)?;
        writeln!(f, "  Hotkeys: {}", self.hotkeys)?;
        writeln!(f, "  axis_deadzone: {}", self.axis_deadzone)?;
        writeln!(
            f,
            "  allow_opposite_directions: {}",
            self.allow_opposite_directions
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JgnesNativeConfig {
    pub nes_file_path: String,
    pub forced_timing_mode: Option<TimingMode>,
    pub window_width: u32,
    pub window_height: u32,
    pub renderer: NativeRenderer,
    pub wgpu_backend: WgpuBackend,
    pub launch_fullscreen: bool,
    pub shared_config: JgnesSharedConfig,
}

impl Display for JgnesNativeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "nes_file_path: {}", self.nes_file_path)?;
        writeln!(
            f,
            "forced_timing_mode: {}",
            fmt_option(self.forced_timing_mode.as_ref())
        )?;
        writeln!(f, "window_width: {}", self.window_width)?;
        writeln!(f, "window_height: {}", self.window_height)?;
        writeln!(f, "renderer: {}", self.renderer)?;
        writeln!(f, "wgpu_backend: {}", self.wgpu_backend)?;
        writeln!(f, "launch_fullscreen: {}", self.launch_fullscreen)?;

        Ok(())
    }
}

/// Configuration that can be modified while the emulator is running.
#[derive(Debug, Clone)]
pub struct JgnesDynamicConfig {
    pub gpu_filter_mode: GpuFilterMode,
    pub shader: Shader,
    pub scanlines: Scanlines,
    pub aspect_ratio: AspectRatio,
    pub overscan: Overscan,
    pub forced_integer_height_scaling: bool,
    pub vsync_mode: VSyncMode,
    pub remove_sprite_limit: bool,
    pub pal_black_border: bool,
    pub sync_to_audio: bool,
    pub silence_ultrasonic_triangle_output: bool,
    pub fast_forward_multiplier: u8,
    pub rewind_buffer_len: Duration,
    pub input_config: InputConfig,
}

impl JgnesDynamicConfig {
    pub(crate) fn to_renderer_config(&self, wgpu_backend: WgpuBackend) -> RendererConfig {
        RendererConfig {
            vsync_mode: self.vsync_mode,
            wgpu_backend,
            gpu_filter_mode: self.gpu_filter_mode,
            shader: self.shader,
            scanlines: self.scanlines,
            aspect_ratio: self.aspect_ratio,
            overscan: self.overscan,
            forced_integer_height_scaling: self.forced_integer_height_scaling,
            use_webgl2_limits: false,
        }
    }

    pub(crate) fn update_emulator_config(&self, emulator_config: &mut EmulatorConfig) {
        emulator_config.remove_sprite_limit = self.remove_sprite_limit;
        emulator_config.pal_black_border = self.pal_black_border;
        emulator_config.silence_ultrasonic_triangle_output =
            self.silence_ultrasonic_triangle_output;
    }
}

impl Display for JgnesDynamicConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "remove_sprite_limit: {}", self.remove_sprite_limit)?;
        writeln!(f, "gpu_filter_mode: {}", self.gpu_filter_mode)?;
        writeln!(f, "shader: {}", self.shader)?;
        writeln!(f, "scanlines: {}", self.scanlines)?;
        writeln!(f, "aspect_ratio: {}", self.aspect_ratio)?;
        writeln!(f, "overscan: {}", self.overscan)?;
        writeln!(
            f,
            "forced_integer_height_scaling: {}",
            self.forced_integer_height_scaling
        )?;
        writeln!(f, "vsync_mode: {}", self.vsync_mode)?;
        writeln!(f, "pal_black_border: {}", self.pal_black_border)?;
        writeln!(f, "sync_to_audio: {}", self.sync_to_audio)?;
        writeln!(
            f,
            "silence_ultrasonic_triangle_output: {}",
            self.silence_ultrasonic_triangle_output
        )?;
        writeln!(
            f,
            "fast_forward_multiplier: {}",
            self.fast_forward_multiplier
        )?;
        writeln!(
            f,
            "rewind_buffer_len_seconds: {}",
            self.rewind_buffer_len.as_secs()
        )?;
        writeln!(f, "input_config: {}", self.input_config)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Keyboard = 0,
    Gamepad = 1,
}

impl InputType {
    fn to_discriminant(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_discriminant(discriminant: u8) -> Option<Self> {
        match discriminant {
            0 => Some(Self::Keyboard),
            1 => Some(Self::Gamepad),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputCollectResult {
    Keyboard(Keycode),
    Gamepad(JoystickInput),
}

/// A wrapper around shared dynamic configuration state and signals that the emulator driver can
/// send to the emulator.
#[derive(Debug, Clone)]
pub struct JgnesSharedConfig {
    pub(crate) dynamic_config: Arc<Mutex<JgnesDynamicConfig>>,
    pub(crate) config_reload_signal: Arc<AtomicBool>,
    pub(crate) quit_signal: Arc<AtomicBool>,
    pub(crate) input_reconfigure_sender: Sender<Option<InputCollectResult>>,
    pub(crate) input_reconfigure_signal: Arc<AtomicU8>,
}

impl JgnesSharedConfig {
    pub(crate) const NO_INPUT_RECONFIGURE: u8 = u8::MAX;

    #[must_use]
    pub fn new(
        initial_dynamic_config: JgnesDynamicConfig,
    ) -> (Self, Receiver<Option<InputCollectResult>>) {
        let (input_reconfigure_sender, input_reconfigure_recv) = mpsc::channel();

        let config = Self {
            dynamic_config: Arc::new(Mutex::new(initial_dynamic_config)),
            config_reload_signal: Arc::new(AtomicBool::new(false)),
            quit_signal: Arc::new(AtomicBool::new(false)),
            input_reconfigure_sender,
            input_reconfigure_signal: Arc::new(AtomicU8::new(Self::NO_INPUT_RECONFIGURE)),
        };

        (config, input_reconfigure_recv)
    }

    #[must_use]
    pub fn get_dynamic_config(&self) -> &Arc<Mutex<JgnesDynamicConfig>> {
        &self.dynamic_config
    }

    pub fn request_config_reload(&self) {
        self.config_reload_signal.store(true, Ordering::Relaxed);
    }

    pub fn request_quit(&self) {
        self.quit_signal.store(true, Ordering::Relaxed);
    }

    pub fn request_input_configure(&self, input_type: InputType) {
        self.input_reconfigure_signal
            .store(input_type.to_discriminant(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_input_reconfigure_returns_none() {
        assert_eq!(
            None,
            InputType::from_discriminant(JgnesSharedConfig::NO_INPUT_RECONFIGURE)
        );
    }
}
