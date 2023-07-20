use crate::{js, NesButton};
use jgnes_renderer::config::{AspectRatio, GpuFilterMode, Overscan, RenderScale, Scanlines};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use winit::event::VirtualKeyCode;

fn default_render_scale() -> RenderScale {
    RenderScale::ONE
}

fn true_fn() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigFields {
    #[serde(default)]
    pub(crate) aspect_ratio: AspectRatio,
    #[serde(default)]
    pub(crate) gpu_filter_mode: GpuFilterMode,
    #[serde(default = "default_render_scale")]
    pub(crate) render_scale: RenderScale,
    #[serde(default)]
    pub(crate) scanlines: Scanlines,
    #[serde(default)]
    pub(crate) overscan: Overscan,
    #[serde(default)]
    pub(crate) force_integer_scaling: bool,
    #[serde(default)]
    pub(crate) remove_sprite_limit: bool,
    #[serde(default = "true_fn")]
    pub(crate) audio_enabled: bool,
    #[serde(default = "true_fn")]
    pub(crate) audio_sync_enabled: bool,
    #[serde(default)]
    pub(crate) silence_ultrasonic_triangle_output: bool,
}

impl ConfigFields {
    const LOCAL_STORAGE_KEY: &'static str = "__config";

    fn save(&self) {
        save_to_local_storage(Self::LOCAL_STORAGE_KEY, self);
    }
}

impl Default for ConfigFields {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

fn save_to_local_storage<S: Serialize>(key: &str, value: &S) {
    let s = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(err) => {
            log::error!("error serializing config: {err}");
            return;
        }
    };

    js::saveToLocalStorage(key, &s);
}

// Allow unsafe_derive_deserialize because the only unsafe usage here is a call to the JS
// saveToLocalStorage function in `set_key(..)`
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct InputConfig {
    pub(crate) up: VirtualKeyCode,
    pub(crate) left: VirtualKeyCode,
    pub(crate) right: VirtualKeyCode,
    pub(crate) down: VirtualKeyCode,
    pub(crate) a: VirtualKeyCode,
    pub(crate) b: VirtualKeyCode,
    pub(crate) start: VirtualKeyCode,
    pub(crate) select: VirtualKeyCode,
}

impl InputConfig {
    const LOCAL_STORAGE_KEY: &'static str = "__inputs";

    pub fn set_key(&mut self, button: NesButton, keycode: VirtualKeyCode) {
        let field = match button {
            NesButton::Up => &mut self.up,
            NesButton::Left => &mut self.left,
            NesButton::Right => &mut self.right,
            NesButton::Down => &mut self.down,
            NesButton::A => &mut self.a,
            NesButton::B => &mut self.b,
            NesButton::Start => &mut self.start,
            NesButton::Select => &mut self.select,
        };
        *field = keycode;

        save_to_local_storage(Self::LOCAL_STORAGE_KEY, self);
    }
}

#[wasm_bindgen]
impl InputConfig {
    pub fn up(&self) -> String {
        format!("{:?}", self.up)
    }

    pub fn left(&self) -> String {
        format!("{:?}", self.left)
    }

    pub fn right(&self) -> String {
        format!("{:?}", self.right)
    }

    pub fn down(&self) -> String {
        format!("{:?}", self.down)
    }

    pub fn a(&self) -> String {
        format!("{:?}", self.a)
    }

    pub fn b(&self) -> String {
        format!("{:?}", self.b)
    }

    pub fn start(&self) -> String {
        format!("{:?}", self.start)
    }

    pub fn select(&self) -> String {
        format!("{:?}", self.select)
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            up: VirtualKeyCode::Up,
            left: VirtualKeyCode::Left,
            right: VirtualKeyCode::Right,
            down: VirtualKeyCode::Down,
            a: VirtualKeyCode::Z,
            b: VirtualKeyCode::X,
            start: VirtualKeyCode::Return,
            select: VirtualKeyCode::RShift,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct JgnesWebConfig {
    pub(crate) inputs: Rc<RefCell<InputConfig>>,
    pub(crate) fields: Rc<RefCell<ConfigFields>>,
    pub(crate) reconfig_input_request: Rc<Cell<Option<NesButton>>>,
    pub(crate) open_file_requested: Rc<Cell<bool>>,
    pub(crate) reset_requested: Rc<Cell<bool>>,
    pub(crate) upload_save_file_requested: Rc<Cell<bool>>,
    pub(crate) restore_defaults_requested: Rc<Cell<bool>>,
    pub(crate) current_filename: Rc<RefCell<String>>,
}

const NTSC: &str = "Ntsc";
const PAL: &str = "Pal";
const SQUARE_PIXELS: &str = "SquarePixels";

const NEAREST_NEIGHBOR: &str = "NearestNeighbor";
const LINEAR_INTERPOLATION: &str = "LinearInterpolation";

#[wasm_bindgen]
impl JgnesWebConfig {
    #[must_use]
    #[wasm_bindgen(constructor)]
    pub fn new() -> JgnesWebConfig {
        let inputs = js::loadFromLocalStorage(InputConfig::LOCAL_STORAGE_KEY)
            .and_then(|config_str| serde_json::from_str::<InputConfig>(&config_str).ok())
            .unwrap_or_default();

        let fields = js::loadFromLocalStorage(ConfigFields::LOCAL_STORAGE_KEY)
            .and_then(|config_str| serde_json::from_str::<ConfigFields>(&config_str).ok())
            .unwrap_or_default();

        Self {
            inputs: Rc::new(RefCell::new(inputs)),
            fields: Rc::new(RefCell::new(fields)),
            ..Self::default()
        }
    }

    pub fn aspect_ratio(&self) -> String {
        let s = match self.fields.borrow().aspect_ratio {
            AspectRatio::Ntsc => NTSC,
            AspectRatio::Pal => PAL,
            AspectRatio::SquarePixels => SQUARE_PIXELS,
            // Web frontend only supports NTSC / PAL / Square pixels
            _ => "",
        };
        s.into()
    }

    pub fn set_aspect_ratio(&self, aspect_ratio: &str) {
        let aspect_ratio = match aspect_ratio {
            NTSC => AspectRatio::Ntsc,
            PAL => AspectRatio::Pal,
            SQUARE_PIXELS => AspectRatio::SquarePixels,
            _ => return,
        };
        let mut fields = self.fields.borrow_mut();
        fields.aspect_ratio = aspect_ratio;
        fields.save();
    }

    pub fn filter_mode(&self) -> String {
        let s = match self.fields.borrow().gpu_filter_mode {
            GpuFilterMode::NearestNeighbor => NEAREST_NEIGHBOR,
            GpuFilterMode::LinearInterpolation => LINEAR_INTERPOLATION,
        };
        s.into()
    }

    pub fn set_filter_mode(&self, gpu_filter_mode: &str) {
        let gpu_filter_mode = match gpu_filter_mode {
            NEAREST_NEIGHBOR => GpuFilterMode::NearestNeighbor,
            LINEAR_INTERPOLATION => GpuFilterMode::LinearInterpolation,
            _ => return,
        };
        let mut fields = self.fields.borrow_mut();
        fields.gpu_filter_mode = gpu_filter_mode;
        fields.save();
    }

    pub fn render_scale(&self) -> u32 {
        self.fields.borrow().render_scale.get()
    }

    pub fn set_render_scale(&self, value: u32) {
        let Ok(render_scale) = RenderScale::try_from(value) else { return };
        let mut fields = self.fields.borrow_mut();
        fields.render_scale = render_scale;
        fields.save();
    }

    pub fn scanlines(&self) -> String {
        format!("{}", self.fields.borrow().scanlines)
    }

    pub fn set_scanlines(&self, scanlines: &str) {
        let Ok(scanlines) = scanlines.parse() else { return };
        let mut fields = self.fields.borrow_mut();
        fields.scanlines = scanlines;
        fields.save();
    }

    pub fn overscan_left(&self) -> bool {
        self.fields.borrow().overscan.left != 0
    }

    pub fn set_overscan_left(&self, value: bool) {
        let overscan = Overscan {
            left: overscan_value(value),
            ..self.fields.borrow().overscan
        };
        self.set_overscan(overscan);
    }

    pub fn overscan_right(&self) -> bool {
        self.fields.borrow().overscan.right != 0
    }

    pub fn set_overscan_right(&self, value: bool) {
        let overscan = Overscan {
            right: overscan_value(value),
            ..self.fields.borrow().overscan
        };
        self.set_overscan(overscan);
    }

    pub fn overscan_top(&self) -> bool {
        self.fields.borrow().overscan.top != 0
    }

    pub fn set_overscan_top(&self, value: bool) {
        let overscan = Overscan {
            top: overscan_value(value),
            ..self.fields.borrow().overscan
        };
        self.set_overscan(overscan);
    }

    pub fn overscan_bottom(&self) -> bool {
        self.fields.borrow().overscan.bottom != 0
    }

    pub fn set_overscan_bottom(&self, value: bool) {
        let overscan = Overscan {
            bottom: overscan_value(value),
            ..self.fields.borrow().overscan
        };
        self.set_overscan(overscan);
    }

    pub fn get_force_integer_scaling(&self) -> bool {
        self.fields.borrow().force_integer_scaling
    }

    pub fn set_force_integer_scaling(&self, value: bool) {
        let mut fields = self.fields.borrow_mut();
        fields.force_integer_scaling = value;
        fields.save();
    }

    pub fn get_remove_sprite_limit(&self) -> bool {
        self.fields.borrow().remove_sprite_limit
    }

    pub fn set_remove_sprite_limit(&self, value: bool) {
        let mut fields = self.fields.borrow_mut();
        fields.remove_sprite_limit = value;
        fields.save();
    }

    pub fn audio_enabled(&self) -> bool {
        self.fields.borrow().audio_enabled
    }

    pub fn set_audio_enabled(&self, value: bool) {
        let mut fields = self.fields.borrow_mut();
        fields.audio_enabled = value;
        fields.save();
    }

    pub fn audio_sync_enabled(&self) -> bool {
        self.fields.borrow().audio_sync_enabled
    }

    pub fn set_audio_sync_enabled(&self, value: bool) {
        let mut fields = self.fields.borrow_mut();
        fields.audio_sync_enabled = value;
        fields.save();
    }

    pub fn silence_ultrasonic_triangle_output(&self) -> bool {
        self.fields.borrow().silence_ultrasonic_triangle_output
    }

    pub fn set_silence_ultrasonic_triangle_output(&self, value: bool) {
        let mut fields = self.fields.borrow_mut();
        fields.silence_ultrasonic_triangle_output = value;
        fields.save();
    }

    pub fn inputs(&self) -> InputConfig {
        self.inputs.borrow().clone()
    }

    pub fn restore_defaults(&self) {
        self.restore_defaults_requested.set(true);

        *self.inputs.borrow_mut() = InputConfig::default();
        *self.fields.borrow_mut() = ConfigFields::default();

        save_to_local_storage(InputConfig::LOCAL_STORAGE_KEY, &InputConfig::default());
        save_to_local_storage(ConfigFields::LOCAL_STORAGE_KEY, &ConfigFields::default());

        js::setConfigDisplayValues(self.clone());
    }

    pub fn reconfigure_input(&self, button: NesButton) {
        self.reconfig_input_request.set(Some(button));
    }

    pub fn open_new_file(&self) {
        self.open_file_requested.set(true);
    }

    pub fn reset_emulator(&self) {
        self.reset_requested.set(true);
    }

    pub fn upload_save_file(&self) {
        self.upload_save_file_requested.set(true);
    }

    #[must_use]
    pub fn current_filename(&self) -> String {
        self.current_filename.borrow().clone()
    }

    // Duplicated definition so clone() can be called from JS
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn clone(&self) -> JgnesWebConfig {
        <JgnesWebConfig as Clone>::clone(self)
    }
}

impl JgnesWebConfig {
    fn set_overscan(&self, overscan: Overscan) {
        let mut fields = self.fields.borrow_mut();
        fields.overscan = overscan;
        fields.save();
    }
}

impl Default for JgnesWebConfig {
    fn default() -> Self {
        JgnesWebConfig {
            inputs: Rc::default(),
            fields: Rc::default(),
            reconfig_input_request: Rc::new(Cell::new(None)),
            open_file_requested: Rc::new(Cell::new(false)),
            reset_requested: Rc::new(Cell::new(false)),
            upload_save_file_requested: Rc::new(Cell::new(false)),
            restore_defaults_requested: Rc::new(Cell::new(false)),
            current_filename: Rc::new(RefCell::new(String::new())),
        }
    }
}

fn overscan_value(value: bool) -> u8 {
    if value {
        8
    } else {
        0
    }
}
