use crate::{js, NesButton};
use jgnes_renderer::config::{AspectRatio, GpuFilterMode, Overscan, PrescalingMode, RenderScale};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableConfig {
    #[serde(default)]
    aspect_ratio: AspectRatio,
    #[serde(default)]
    gpu_filter_mode: GpuFilterMode,
    #[serde(default = "default_render_scale")]
    render_scale: RenderScale,
    #[serde(default)]
    overscan: Overscan,
    #[serde(default)]
    remove_sprite_limit: bool,
    #[serde(default = "true_fn")]
    audio_enabled: bool,
    #[serde(default = "true_fn")]
    audio_sync_enabled: bool,
    #[serde(default)]
    silence_ultrasonic_triangle_output: bool,
}

impl SerializableConfig {
    const LOCAL_STORAGE_KEY: &'static str = "__config";
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
    pub(crate) aspect_ratio: Rc<Cell<AspectRatio>>,
    pub(crate) gpu_filter_mode: Rc<Cell<GpuFilterMode>>,
    pub(crate) render_scale: Rc<Cell<RenderScale>>,
    pub(crate) overscan: Rc<Cell<Overscan>>,
    pub(crate) remove_sprite_limit: Rc<Cell<bool>>,
    pub(crate) audio_enabled: Rc<Cell<bool>>,
    pub(crate) audio_sync_enabled: Rc<Cell<bool>>,
    pub(crate) silence_ultrasonic_triangle_output: Rc<Cell<bool>>,
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

        js::loadFromLocalStorage(SerializableConfig::LOCAL_STORAGE_KEY)
            .and_then(|config_str| serde_json::from_str::<SerializableConfig>(&config_str).ok())
            .map_or_else(Self::default, |config| Self {
                inputs: Rc::new(RefCell::new(inputs)),
                aspect_ratio: Rc::new(Cell::new(config.aspect_ratio)),
                gpu_filter_mode: Rc::new(Cell::new(config.gpu_filter_mode)),
                render_scale: Rc::new(Cell::new(config.render_scale)),
                overscan: Rc::new(Cell::new(config.overscan)),
                remove_sprite_limit: Rc::new(Cell::new(config.remove_sprite_limit)),
                audio_enabled: Rc::new(Cell::new(config.audio_enabled)),
                audio_sync_enabled: Rc::new(Cell::new(config.audio_sync_enabled)),
                silence_ultrasonic_triangle_output: Rc::new(Cell::new(
                    config.silence_ultrasonic_triangle_output,
                )),
                ..Self::default()
            })
    }

    pub fn aspect_ratio(&self) -> String {
        let s = match self.aspect_ratio.get() {
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
        self.aspect_ratio.set(aspect_ratio);

        self.save_to_local_storage();
    }

    pub fn filter_mode(&self) -> String {
        let s = match self.gpu_filter_mode.get() {
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
        self.gpu_filter_mode.set(gpu_filter_mode);

        self.save_to_local_storage();
    }

    pub fn render_scale(&self) -> u32 {
        self.render_scale.get().get()
    }

    pub fn set_render_scale(&self, value: u32) {
        let Ok(render_scale) = RenderScale::try_from(value) else { return };
        self.render_scale.set(render_scale);
    }

    pub fn overscan_left(&self) -> bool {
        self.overscan.get().left != 0
    }

    pub fn set_overscan_left(&self, value: bool) {
        let overscan = Overscan {
            left: overscan_value(value),
            ..self.overscan.get()
        };
        self.overscan.set(overscan);
        self.save_to_local_storage();
    }

    pub fn overscan_right(&self) -> bool {
        self.overscan.get().right != 0
    }

    pub fn set_overscan_right(&self, value: bool) {
        let overscan = Overscan {
            right: overscan_value(value),
            ..self.overscan.get()
        };
        self.overscan.set(overscan);
        self.save_to_local_storage();
    }

    pub fn overscan_top(&self) -> bool {
        self.overscan.get().top != 0
    }

    pub fn set_overscan_top(&self, value: bool) {
        let overscan = Overscan {
            top: overscan_value(value),
            ..self.overscan.get()
        };
        self.overscan.set(overscan);
        self.save_to_local_storage();
    }

    pub fn overscan_bottom(&self) -> bool {
        self.overscan.get().bottom != 0
    }

    pub fn set_overscan_bottom(&self, value: bool) {
        let overscan = Overscan {
            bottom: overscan_value(value),
            ..self.overscan.get()
        };
        self.overscan.set(overscan);
        self.save_to_local_storage();
    }

    pub fn get_remove_sprite_limit(&self) -> bool {
        self.remove_sprite_limit.get()
    }

    pub fn set_remove_sprite_limit(&self, value: bool) {
        self.remove_sprite_limit.set(value);
    }

    pub fn audio_enabled(&self) -> bool {
        self.audio_enabled.get()
    }

    pub fn set_audio_enabled(&self, value: bool) {
        self.audio_enabled.set(value);
        self.save_to_local_storage();
    }

    pub fn audio_sync_enabled(&self) -> bool {
        self.audio_sync_enabled.get()
    }

    pub fn set_audio_sync_enabled(&self, value: bool) {
        self.audio_sync_enabled.set(value);
        self.save_to_local_storage();
    }

    pub fn silence_ultrasonic_triangle_output(&self) -> bool {
        self.silence_ultrasonic_triangle_output.get()
    }

    pub fn set_silence_ultrasonic_triangle_output(&self, value: bool) {
        self.silence_ultrasonic_triangle_output.set(value);
        self.save_to_local_storage();
    }

    pub fn inputs(&self) -> InputConfig {
        self.inputs.borrow().clone()
    }

    pub fn restore_defaults(&self) {
        self.restore_defaults_requested.set(true);

        let default = JgnesWebConfig::default();

        *self.inputs.borrow_mut() = default.inputs.borrow().clone();
        self.aspect_ratio.set(default.aspect_ratio.get());
        self.gpu_filter_mode.set(default.gpu_filter_mode.get());
        self.render_scale.set(default.render_scale.get());
        self.overscan.set(default.overscan.get());
        self.remove_sprite_limit
            .set(default.remove_sprite_limit.get());
        self.audio_enabled.set(default.audio_enabled.get());
        self.audio_sync_enabled
            .set(default.audio_sync_enabled.get());
        self.silence_ultrasonic_triangle_output
            .set(default.silence_ultrasonic_triangle_output.get());

        self.save_to_local_storage();

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
    #[cfg(feature = "webgl")]
    pub(crate) fn get_prescaling_mode(&self) -> PrescalingMode {
        PrescalingMode::Cpu(self.render_scale.get())
    }

    #[cfg(not(feature = "webgl"))]
    pub(crate) fn get_prescaling_mode(&self) -> PrescalingMode {
        PrescalingMode::Gpu(self.render_scale.get())
    }

    fn to_serializable_config(&self) -> SerializableConfig {
        SerializableConfig {
            aspect_ratio: self.aspect_ratio.get(),
            gpu_filter_mode: self.gpu_filter_mode.get(),
            render_scale: self.render_scale.get(),
            overscan: self.overscan.get(),
            remove_sprite_limit: self.remove_sprite_limit.get(),
            audio_enabled: self.audio_enabled.get(),
            audio_sync_enabled: self.audio_sync_enabled.get(),
            silence_ultrasonic_triangle_output: self.silence_ultrasonic_triangle_output.get(),
        }
    }

    fn save_to_local_storage(&self) {
        let config = self.to_serializable_config();
        save_to_local_storage(SerializableConfig::LOCAL_STORAGE_KEY, &config);

        save_to_local_storage(InputConfig::LOCAL_STORAGE_KEY, &*self.inputs.borrow());
    }
}

impl Default for JgnesWebConfig {
    fn default() -> Self {
        JgnesWebConfig {
            inputs: Rc::default(),
            aspect_ratio: Rc::new(Cell::new(AspectRatio::Ntsc)),
            gpu_filter_mode: Rc::new(Cell::new(GpuFilterMode::NearestNeighbor)),
            render_scale: Rc::new(Cell::new(default_render_scale())),
            overscan: Rc::default(),
            remove_sprite_limit: Rc::new(Cell::new(false)),
            audio_enabled: Rc::new(Cell::new(true)),
            audio_sync_enabled: Rc::new(Cell::new(true)),
            silence_ultrasonic_triangle_output: Rc::new(Cell::new(false)),
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
