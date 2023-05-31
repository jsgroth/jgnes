use crate::{js, NesButton};
use jgnes_renderer::config::{AspectRatio, GpuFilterMode, Overscan, RenderScale};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableConfig {
    aspect_ratio: AspectRatio,
    gpu_filter_mode: GpuFilterMode,
    overscan: Overscan,
    audio_enabled: bool,
    audio_sync_enabled: bool,
    silence_ultrasonic_triangle_output: bool,
}

impl SerializableConfig {
    const LOCAL_STORAGE_KEY: &'static str = "__config";
}

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct JgnesWebConfig {
    pub(crate) aspect_ratio: Rc<RefCell<AspectRatio>>,
    pub(crate) gpu_filter_mode: Rc<RefCell<GpuFilterMode>>,
    pub(crate) overscan: Rc<RefCell<Overscan>>,
    pub(crate) audio_enabled: Rc<RefCell<bool>>,
    pub(crate) audio_sync_enabled: Rc<RefCell<bool>>,
    pub(crate) silence_ultrasonic_triangle_output: Rc<RefCell<bool>>,
    pub(crate) reconfig_input_request: Rc<RefCell<Option<NesButton>>>,
    pub(crate) open_file_requested: Rc<RefCell<bool>>,
    pub(crate) reset_requested: Rc<RefCell<bool>>,
    pub(crate) upload_save_file_requested: Rc<RefCell<bool>>,
    pub(crate) current_filename: Rc<RefCell<String>>,
}

const NTSC: &str = "Ntsc";
const SQUARE_PIXELS: &str = "SquarePixels";

const NEAREST_NEIGHBOR: &str = "NearestNeighbor";
const LINEAR_1X: &str = "Linear";
const LINEAR_2X: &str = "Linear2x";
const LINEAR_3X: &str = "Linear3x";

#[wasm_bindgen]
impl JgnesWebConfig {
    #[must_use]
    #[wasm_bindgen(constructor)]
    pub fn new() -> JgnesWebConfig {
        js::loadFromLocalStorage(SerializableConfig::LOCAL_STORAGE_KEY)
            .and_then(|config_str| serde_json::from_str::<SerializableConfig>(&config_str).ok())
            .map_or_else(Self::default, |config| Self {
                aspect_ratio: Rc::new(RefCell::new(config.aspect_ratio)),
                gpu_filter_mode: Rc::new(RefCell::new(config.gpu_filter_mode)),
                overscan: Rc::new(RefCell::new(config.overscan)),
                audio_enabled: Rc::new(RefCell::new(config.audio_enabled)),
                audio_sync_enabled: Rc::new(RefCell::new(config.audio_sync_enabled)),
                silence_ultrasonic_triangle_output: Rc::new(RefCell::new(
                    config.silence_ultrasonic_triangle_output,
                )),
                ..Self::default()
            })
    }

    pub fn get_aspect_ratio(&self) -> String {
        let s = match *self.aspect_ratio.borrow() {
            AspectRatio::Ntsc => NTSC,
            AspectRatio::SquarePixels => SQUARE_PIXELS,
            // Web frontend only supports NTSC / Square pixels
            _ => "",
        };
        s.into()
    }

    pub fn set_aspect_ratio(&self, aspect_ratio: &str) {
        let aspect_ratio = match aspect_ratio {
            NTSC => AspectRatio::Ntsc,
            SQUARE_PIXELS => AspectRatio::SquarePixels,
            _ => return,
        };
        *self.aspect_ratio.borrow_mut() = aspect_ratio;

        self.save_to_local_storage();
    }

    pub fn get_filter_mode(&self) -> String {
        let s = match *self.gpu_filter_mode.borrow() {
            GpuFilterMode::NearestNeighbor => NEAREST_NEIGHBOR,
            GpuFilterMode::Linear(RenderScale::ONE) => LINEAR_1X,
            GpuFilterMode::Linear(RenderScale::TWO)
            | GpuFilterMode::LinearCpuScaled(RenderScale::TWO) => LINEAR_2X,
            GpuFilterMode::Linear(RenderScale::THREE)
            | GpuFilterMode::LinearCpuScaled(RenderScale::THREE) => LINEAR_3X,
            // Other filter modes not supported by the web frontend
            _ => "",
        };
        s.into()
    }

    #[cfg(feature = "webgl")]
    pub fn set_filter_mode(&self, gpu_filter_mode: &str) {
        let gpu_filter_mode = match gpu_filter_mode {
            NEAREST_NEIGHBOR => GpuFilterMode::NearestNeighbor,
            LINEAR_1X => GpuFilterMode::Linear(RenderScale::ONE),
            LINEAR_2X => GpuFilterMode::LinearCpuScaled(RenderScale::TWO),
            LINEAR_3X => GpuFilterMode::LinearCpuScaled(RenderScale::THREE),
            _ => return,
        };
        *self.gpu_filter_mode.borrow_mut() = gpu_filter_mode;

        self.save_to_local_storage();
    }

    #[cfg(not(feature = "webgl"))]
    pub fn set_filter_mode(&self, gpu_filter_mode: &str) {
        let gpu_filter_mode = match gpu_filter_mode {
            NEAREST_NEIGHBOR => GpuFilterMode::NearestNeighbor,
            LINEAR_1X => GpuFilterMode::Linear(RenderScale::ONE),
            LINEAR_2X => GpuFilterMode::Linear(RenderScale::TWO),
            LINEAR_3X => GpuFilterMode::Linear(RenderScale::THREE),
            _ => return,
        };
        *self.gpu_filter_mode.borrow_mut() = gpu_filter_mode;

        self.save_to_local_storage();
    }

    pub fn get_overscan_left(&self) -> bool {
        self.overscan.borrow().left != 0
    }

    pub fn set_overscan_left(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().left);
        self.save_to_local_storage();
    }

    pub fn get_overscan_right(&self) -> bool {
        self.overscan.borrow().right != 0
    }

    pub fn set_overscan_right(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().right);
        self.save_to_local_storage();
    }

    pub fn get_overscan_top(&self) -> bool {
        self.overscan.borrow().top != 0
    }

    pub fn set_overscan_top(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().top);
        self.save_to_local_storage();
    }

    pub fn get_overscan_bottom(&self) -> bool {
        self.overscan.borrow().bottom != 0
    }

    pub fn set_overscan_bottom(&self, value: bool) {
        set_overscan_field(value, &mut self.overscan.borrow_mut().bottom);
        self.save_to_local_storage();
    }

    pub fn get_audio_enabled(&self) -> bool {
        *self.audio_enabled.borrow()
    }

    pub fn set_audio_enabled(&self, value: bool) {
        *self.audio_enabled.borrow_mut() = value;
        self.save_to_local_storage();
    }

    pub fn get_audio_sync_enabled(&self) -> bool {
        *self.audio_sync_enabled.borrow()
    }

    pub fn set_audio_sync_enabled(&self, value: bool) {
        *self.audio_sync_enabled.borrow_mut() = value;
        self.save_to_local_storage();
    }

    pub fn get_silence_ultrasonic_triangle_output(&self) -> bool {
        *self.silence_ultrasonic_triangle_output.borrow()
    }

    pub fn set_silence_ultrasonic_triangle_output(&self, value: bool) {
        *self.silence_ultrasonic_triangle_output.borrow_mut() = value;
        self.save_to_local_storage();
    }

    pub fn reconfigure_input(&self, button: NesButton) {
        *self.reconfig_input_request.borrow_mut() = Some(button);
    }

    pub fn open_new_file(&self) {
        *self.open_file_requested.borrow_mut() = true;
    }

    pub fn reset_emulator(&self) {
        *self.reset_requested.borrow_mut() = true;
    }

    pub fn upload_save_file(&self) {
        *self.upload_save_file_requested.borrow_mut() = true;
    }

    #[must_use]
    pub fn get_current_filename(&self) -> String {
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
    fn to_serializable_config(&self) -> SerializableConfig {
        SerializableConfig {
            aspect_ratio: *self.aspect_ratio.borrow(),
            gpu_filter_mode: *self.gpu_filter_mode.borrow(),
            overscan: *self.overscan.borrow(),
            audio_enabled: *self.audio_enabled.borrow(),
            audio_sync_enabled: *self.audio_sync_enabled.borrow(),
            silence_ultrasonic_triangle_output: *self.silence_ultrasonic_triangle_output.borrow(),
        }
    }

    fn save_to_local_storage(&self) {
        let config = self.to_serializable_config();
        let config_str = match serde_json::to_string(&config) {
            Ok(config) => config,
            Err(err) => {
                log::error!("error serializing config: {err}");
                return;
            }
        };

        js::saveToLocalStorage(SerializableConfig::LOCAL_STORAGE_KEY, &config_str);
    }
}

impl Default for JgnesWebConfig {
    fn default() -> Self {
        JgnesWebConfig {
            aspect_ratio: Rc::new(RefCell::new(AspectRatio::Ntsc)),
            gpu_filter_mode: Rc::new(RefCell::new(GpuFilterMode::NearestNeighbor)),
            overscan: Rc::default(),
            audio_enabled: Rc::new(RefCell::new(true)),
            audio_sync_enabled: Rc::new(RefCell::new(true)),
            silence_ultrasonic_triangle_output: Rc::new(RefCell::new(false)),
            reconfig_input_request: Rc::new(RefCell::new(None)),
            open_file_requested: Rc::new(RefCell::new(false)),
            reset_requested: Rc::new(RefCell::new(false)),
            upload_save_file_requested: Rc::new(RefCell::new(false)),
            current_filename: Rc::new(RefCell::new(String::new())),
        }
    }
}

fn set_overscan_field(value: bool, field: &mut u8) {
    *field = if value { 8 } else { 0 };
}
