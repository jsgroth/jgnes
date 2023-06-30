// Miscellaneous UI functions called from Rust

import { button_name, NesButton } from "/pkg/jgnes_web.js";

export function loadFromLocalStorage(key) {
    return localStorage.getItem(key);
}

export function saveToLocalStorage(key, value) {
    localStorage.setItem(key, value);
}

export function initComplete() {
    document.getElementById("jgnes").classList.remove("hidden");
    document.getElementById("info-text").classList.remove("hidden");
    document.getElementById("loading-text").remove();
}

function setInputButtonText(nesButton, buttonText) {
    let buttonId = {
        "Up": "up-key",
        "Left": "left-key",
        "Right": "right-key",
        "Down": "down-key",
        "A": "a-key",
        "B": "b-key",
        "Start": "start-key",
        "Select": "select-key",
    }[button_name(nesButton)];
    document.getElementById(buttonId).value = buttonText;
}

export function setConfigDisplayValues(jgnesWebConfig) {
    document.querySelectorAll("input[name='aspect-ratio']").forEach((element) => {
        element.checked = element.value === jgnesWebConfig.aspect_ratio();
    });

    document.querySelectorAll("input[name='image-filter']").forEach((element) => {
        element.checked = element.value === jgnesWebConfig.filter_mode();
    });

    document.getElementById("overscan-left").checked = jgnesWebConfig.overscan_left();
    document.getElementById("overscan-right").checked = jgnesWebConfig.overscan_right();
    document.getElementById("overscan-top").checked = jgnesWebConfig.overscan_top();
    document.getElementById("overscan-bottom").checked = jgnesWebConfig.overscan_bottom();

    document.getElementById("audio-enabled").checked = jgnesWebConfig.audio_enabled();
    document.getElementById("audio-sync-enabled").checked = jgnesWebConfig.audio_sync_enabled();
    document.getElementById("silence-triangle-ultrasonic").checked = jgnesWebConfig.silence_ultrasonic_triangle_output();

    let inputConfig = jgnesWebConfig.inputs();
    setInputButtonText(NesButton.Up, inputConfig.up());
    setInputButtonText(NesButton.Left, inputConfig.left());
    setInputButtonText(NesButton.Right, inputConfig.right());
    setInputButtonText(NesButton.Down, inputConfig.down());
    setInputButtonText(NesButton.A, inputConfig.a());
    setInputButtonText(NesButton.B, inputConfig.b());
    setInputButtonText(NesButton.Start, inputConfig.start());
    setInputButtonText(NesButton.Select, inputConfig.select());
}

export function afterInputReconfigure(nesButton, buttonText) {
    document.querySelectorAll("input.input-config").forEach((element) => {
        element.disabled = false;
    });

    document.querySelector("canvas").classList.remove("grayed-out");
    document.getElementById("jgnes-wasm").classList.remove("grayed-out");

    setInputButtonText(nesButton, buttonText);
}

export function focusCanvas() {
    document.querySelector("canvas").focus();
}

export function setSaveButtonsEnabled(enabled) {
    document.querySelectorAll("input.save-button").forEach((element) => {
        element.disabled = !enabled;
    });
}