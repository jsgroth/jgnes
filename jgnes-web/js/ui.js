// Miscellaneous UI functions called from Rust

export function initComplete() {
    document.getElementById("jgnes-config-and-info").hidden = false;
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