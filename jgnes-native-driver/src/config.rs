use crate::GpuFilterMode;
use sdl2::keyboard::Keycode;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NativeRenderer {
    Sdl2,
    #[default]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AspectRatio {
    #[default]
    Ntsc,
    SquarePixels,
    FourThree,
    Stretched,
}

impl Display for AspectRatio {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ntsc => write!(f, "Ntsc"),
            Self::SquarePixels => write!(f, "SquarePixels"),
            Self::FourThree => write!(f, "FourThree"),
            Self::Stretched => write!(f, "Stretched"),
        }
    }
}

impl FromStr for AspectRatio {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Ntsc" => Ok(Self::Ntsc),
            "SquarePixels" => Ok(Self::SquarePixels),
            "FourThree" => Ok(Self::FourThree),
            "Stretched" => Ok(Self::Stretched),
            _ => Err(format!("invalid aspect ratio string: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Overscan {
    pub top: u8,
    pub left: u8,
    pub right: u8,
    pub bottom: u8,
}

impl Overscan {
    pub(crate) fn validate(self) -> Result<Self, anyhow::Error> {
        if self.top > 112 || self.bottom > 112 {
            return Err(anyhow::Error::msg(format!(
                "Vertical overscan cannot be more than 112; top={}, bottom={}",
                self.top, self.bottom
            )));
        }

        if self.left > 128 || self.right > 128 {
            return Err(anyhow::Error::msg(format!(
                "Horizontal overscan cannot be more than 128; left={}, right={}",
                self.left, self.right
            )));
        }

        Ok(self)
    }
}

impl Display for Overscan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Overscan[Top={}, Left={}, Bottom={}, Right={}]",
            self.top, self.left, self.bottom, self.right
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VSyncMode {
    #[default]
    Enabled,
    Disabled,
    Fast,
    Adaptive,
}

impl Display for VSyncMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "Enabled"),
            Self::Disabled => write!(f, "Disabled"),
            Self::Fast => write!(f, "Fast"),
            Self::Adaptive => write!(f, "Adaptive"),
        }
    }
}

impl FromStr for VSyncMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Enabled" => Ok(Self::Enabled),
            "Disabled" => Ok(Self::Disabled),
            "Fast" => Ok(Self::Fast),
            "Adaptive" => Ok(Self::Adaptive),
            _ => Err(format!("invalid VSync mode string: {s}")),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisDirection {
    Positive,
    Negative,
}

impl AxisDirection {
    fn sign_str(self) -> &'static str {
        match self {
            Self::Positive => "+",
            Self::Negative => "-",
        }
    }
}

impl Display for AxisDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Positive => write!(f, "Positive"),
            Self::Negative => write!(f, "Negative"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum HatDirection {
    #[default]
    Up,
    Left,
    Right,
    Down,
}

impl HatDirection {
    pub(crate) const ALL: [Self; 4] = [Self::Up, Self::Left, Self::Right, Self::Down];
}

impl Display for HatDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up => write!(f, "Up"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::Down => write!(f, "Down"),
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
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            quit: Some(Keycode::Escape.name()),
            toggle_fullscreen: Some(Keycode::F9.name()),
            save_state: Some(Keycode::F5.name()),
            load_state: Some(Keycode::F6.name()),
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
        write!(
            f,
            "    Load State: {}",
            fmt_option(self.load_state.as_ref())
        )?;

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
    pub window_width: u32,
    pub window_height: u32,
    pub renderer: NativeRenderer,
    pub wgpu_backend: WgpuBackend,
    pub gpu_filter_mode: GpuFilterMode,
    pub aspect_ratio: AspectRatio,
    pub overscan: Overscan,
    pub forced_integer_height_scaling: bool,
    pub vsync_mode: VSyncMode,
    pub sync_to_audio: bool,
    pub launch_fullscreen: bool,
    pub input_config: InputConfig,
}

impl Display for JgnesNativeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "nes_file_path: {}", self.nes_file_path)?;
        writeln!(f, "window_width: {}", self.window_width)?;
        writeln!(f, "window_height: {}", self.window_height)?;
        writeln!(f, "renderer: {}", self.renderer)?;
        writeln!(f, "wgpu_backend: {}", self.wgpu_backend)?;
        writeln!(f, "gpu_filter_mode: {}", self.gpu_filter_mode)?;
        writeln!(f, "aspect_ratio: {}", self.aspect_ratio)?;
        writeln!(f, "overscan: {}", self.overscan)?;
        writeln!(
            f,
            "forced_integer_height_scaling: {}",
            self.forced_integer_height_scaling
        )?;
        writeln!(f, "vsync_mode: {}", self.vsync_mode)?;
        writeln!(f, "sync_to_audio: {}", self.sync_to_audio)?;
        writeln!(f, "launch_fullscreen: {}", self.launch_fullscreen)?;
        writeln!(f, "input_config: {}", self.input_config)?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JgnesDynamicConfig {
    pub quit_signal: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WgpuBackend {
    #[default]
    Auto,
    Vulkan,
    Direct3d12,
    Metal,
}

impl Display for WgpuBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Auto"),
            Self::Vulkan => write!(f, "Vulkan"),
            Self::Direct3d12 => write!(f, "Direct3d12"),
            Self::Metal => write!(f, "Metal"),
        }
    }
}

impl FromStr for WgpuBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Auto" => Ok(Self::Auto),
            "Vulkan" => Ok(Self::Vulkan),
            "Direct3d12" => Ok(Self::Direct3d12),
            "Metal" => Ok(Self::Metal),
            _ => Err(format!("invalid wgpu backend string: {s}")),
        }
    }
}

impl WgpuBackend {
    pub(crate) fn to_wgpu_backends(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::PRIMARY,
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Direct3d12 => wgpu::Backends::DX12,
            Self::Metal => wgpu::Backends::METAL,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RendererConfig {
    pub(crate) vsync_mode: VSyncMode,
    pub(crate) wgpu_backend: WgpuBackend,
    pub(crate) gpu_filter_mode: GpuFilterMode,
    pub(crate) aspect_ratio: AspectRatio,
    pub(crate) overscan: Overscan,
    pub(crate) forced_integer_height_scaling: bool,
}
