// Polyfill necessary because the generated wasm-bindgen JS requires TextEncoder and TextDecoder,
// but they are not defined in the worklet thread
import "./TextEncoder.js";

import { initSync, AudioProcessor } from "./pkg/jgnes_web.js";

class JgnesAudioProcessor extends AudioWorkletProcessor {
    constructor(options) {
        super();

        let [module, memory, audioQueue] = options.processorOptions;
        initSync(module, memory);
        this.processor = new AudioProcessor(audioQueue);
    }
    process(inputs, outputs) {
        this.processor.process(outputs[0][0]);
        return true;
    }
}

registerProcessor("audio-processor", JgnesAudioProcessor);
