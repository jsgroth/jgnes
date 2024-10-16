use crate::config::JgnesWebConfig;
use base64::Engine;
use base64::engine::general_purpose;
use jgnes_proc_macros::build_time_pretty_str;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);
}

#[wasm_bindgen(module = "/js/ui.js")]
extern "C" {
    /// Retrieve an item from local storage. Returns None if no item exists for this key.
    pub fn loadFromLocalStorage(key: &str) -> Option<String>;

    /// Set an item in local storage.
    pub fn saveToLocalStorage(key: &str, value: &str);

    /// Remove the 'Loading...' text and unhide the UI.
    pub fn initComplete();

    /// Set all config displays to the values in the given config.
    pub fn setConfigDisplayValues(jgnesWebConfig: JgnesWebConfig);

    /// Un-gray the emulator display elements, and change the given button's text to the newly
    /// configured key.
    pub fn afterInputReconfigure(buttonName: &str, buttonText: &str);

    /// Focus the canvas element. Useful because the emulator can only receive inputs while the
    /// canvas has focus, and clicking on any UI element will take focus away.
    pub fn focusCanvas();

    /// Enable or disable the Download Save / Upload Save buttons.
    pub fn setSaveButtonsEnabled(enabled: bool);

    /// Set whether the cursor is visible when over the canvas element.
    pub fn setCursorVisible(visible: bool);
}

#[must_use]
#[wasm_bindgen]
pub fn b64_to_bytes(s: &str) -> Option<Uint8Array> {
    match general_purpose::STANDARD.decode(s) {
        Ok(bytes) => {
            let array = Uint8Array::new_with_length(bytes.len() as u32);
            for (i, byte) in bytes.iter().copied().enumerate() {
                array.set_index(i as u32, byte);
            }
            Some(array)
        }
        Err(_) => None,
    }
}

#[must_use]
#[wasm_bindgen]
pub fn get_build_timestamp() -> String {
    build_time_pretty_str!().into()
}
