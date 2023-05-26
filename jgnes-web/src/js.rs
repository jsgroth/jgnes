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
    pub fn loadFromLocalStorage(key: &str) -> Option<String>;

    pub fn saveToLocalStorage(key: &str, value: &str);

    pub fn initComplete();

    pub fn afterInputReconfigure(buttonId: &str, buttonText: &str);

    pub fn focusCanvas();

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
