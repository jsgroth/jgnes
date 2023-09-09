// Miscellaneous UI functions called from Rust

/**
 * @param key {string}
 * @returns {string | null}
 */
export function loadFromLocalStorage(key) {
    return localStorage.getItem(key);
}

/**
 * @param key {string}
 * @param value {string}
 */
export function saveToLocalStorage(key, value) {
    localStorage.setItem(key, value);
}

export function initComplete() {
    document.getElementById("jgnes").classList.remove("hidden");
    document.getElementById("info-text").classList.remove("hidden");
    document.getElementById("loading-text").remove();
}

/**
 * @param buttonName {string}
 * @param buttonText {string}
 */
function setInputButtonText(buttonName, buttonText) {
    let buttonId = {
        "Up": "up-key",
        "Left": "left-key",
        "Right": "right-key",
        "Down": "down-key",
        "A": "a-key",
        "B": "b-key",
        "Start": "start-key",
        "Select": "select-key",
    }[buttonName];
    document.getElementById(buttonId).value = buttonText;
}

/**
 * @param jgnesWebConfig {JgnesWebConfig}
 */
export function setConfigDisplayValues(jgnesWebConfig) {
    document.querySelectorAll("input[name='aspect-ratio']").forEach((element) => {
        element.checked = element.value === jgnesWebConfig.aspect_ratio();
    });

    document.querySelectorAll("input[name='image-filter']").forEach((element) => {
        element.checked = element.value === jgnesWebConfig.filter_mode();
    });

    document.querySelectorAll("input[name='image-prescale']").forEach((element) => {
        element.checked = parseInt(element.value) === jgnesWebConfig.render_scale();
    });

    document.querySelectorAll("input[name='scanlines']").forEach((element) => {
        element.checked = element.value === jgnesWebConfig.scanlines();
    });

    document.getElementById("overscan-left").checked = jgnesWebConfig.overscan_left();
    document.getElementById("overscan-right").checked = jgnesWebConfig.overscan_right();
    document.getElementById("overscan-top").checked = jgnesWebConfig.overscan_top();
    document.getElementById("overscan-bottom").checked = jgnesWebConfig.overscan_bottom();

    document.getElementById("audio-enabled").checked = jgnesWebConfig.audio_enabled();
    document.getElementById("audio-sync-enabled").checked = jgnesWebConfig.audio_sync_enabled();
    document.getElementById("silence-triangle-ultrasonic").checked = jgnesWebConfig.silence_ultrasonic_triangle_output();

    document.getElementById("force-integer-scaling").checked = jgnesWebConfig.get_force_integer_scaling();
    document.getElementById("sprite-limit-disabled").checked = jgnesWebConfig.get_remove_sprite_limit();
    document.getElementById("frame-time-sync").checked = jgnesWebConfig.frame_time_sync();

    let inputConfig = jgnesWebConfig.inputs();
    setInputButtonText("Up", inputConfig.up());
    setInputButtonText("Left", inputConfig.left());
    setInputButtonText("Right", inputConfig.right());
    setInputButtonText("Down", inputConfig.down());
    setInputButtonText("A", inputConfig.a());
    setInputButtonText("B", inputConfig.b());
    setInputButtonText("Start", inputConfig.start());
    setInputButtonText("Select", inputConfig.select());
}

/**
 * @param buttonName {string}
 * @param buttonText {string}
 */
export function afterInputReconfigure(buttonName, buttonText) {
    document.querySelectorAll("input.input-config").forEach((element) => {
        element.disabled = false;
    });

    document.querySelector("canvas").classList.remove("grayed-out");
    document.getElementById("jgnes-wasm").classList.remove("grayed-out");

    setInputButtonText(buttonName, buttonText);
}

export function focusCanvas() {
    document.querySelector("canvas").focus();
}

/**
 * @param enabled {boolean}
 */
export function setSaveButtonsEnabled(enabled) {
    document.querySelectorAll("input.save-button").forEach((element) => {
        element.disabled = !enabled;
    });
}

/**
 * @param visible {boolean}
 */
export function setCursorVisible(visible) {
    let canvas = document.querySelector("canvas");
    if (visible) {
        canvas.classList.remove("cursor-hidden");
    } else {
        canvas.classList.add("cursor-hidden");
    }
}