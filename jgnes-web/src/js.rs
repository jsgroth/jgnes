use base64::engine::general_purpose;
use base64::Engine;
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

    /// Un-gray the emulator display elements, and change the given button's text to the newly
    /// configured key.
    pub fn afterInputReconfigure(buttonId: &str, buttonText: &str);

    /// Focus the canvas element. Useful because the emulator can only receive inputs while the
    /// canvas has focus, and clicking on any UI element will take focus away.
    pub fn focusCanvas();

    /// Enable or disable the Download Save / Upload Save buttons.
    pub fn setSaveButtonsEnabled(enabled: bool);
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
