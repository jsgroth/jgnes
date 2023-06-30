// Miscellaneous UI functions called from Rust

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

export function afterInputReconfigure(buttonId, buttonText) {
    document.querySelectorAll("input.input-config").forEach((element) => {
        element.disabled = false;
    });

    document.querySelector("canvas").classList.remove("grayed-out");
    document.getElementById("jgnes-wasm").classList.remove("grayed-out");

    document.getElementById(buttonId).setAttribute("value", buttonText);
}

export function focusCanvas() {
    document.querySelector("canvas").focus();
}

export function setSaveButtonsEnabled(enabled) {
    document.querySelectorAll("input.save-button").forEach((element) => {
        element.disabled = !enabled;
    });
}