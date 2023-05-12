use js_sys::{Array, Atomics, SharedArrayBuffer, Uint32Array};
use std::cmp;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioWorkletNode, AudioWorkletNodeOptions, ChannelCountMode};

const HEADER_LEN: u32 = 2;
const HEADER_LEN_BYTES: u32 = HEADER_LEN * 4;
const BUFFER_LEN: u32 = 8192;
const BUFFER_LEN_BYTES: u32 = BUFFER_LEN * 4;
const BUFFER_INDEX_MASK: u32 = BUFFER_LEN - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    Successful,
    BufferFull,
}

// A very simple lock-free queue implemented using a circular buffer.
// The header contains two 32-bit integers containing the current start and exclusive end indices.
#[wasm_bindgen]
pub struct AudioQueue {
    header: SharedArrayBuffer,
    header_typed: Uint32Array,
    buffer: SharedArrayBuffer,
    buffer_typed: Uint32Array,
}

impl Default for AudioQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<JsValue> for AudioQueue {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        let array = value.dyn_into::<Array>()?;
        let header = array.get(0).dyn_into::<SharedArrayBuffer>()?;
        let buffer = array.get(1).dyn_into::<SharedArrayBuffer>()?;
        Ok(Self::from_buffers(header, buffer))
    }
}

impl AudioQueue {
    pub fn new() -> Self {
        let header = SharedArrayBuffer::new(HEADER_LEN_BYTES);
        let buffer = SharedArrayBuffer::new(BUFFER_LEN_BYTES);
        Self::from_buffers(header, buffer)
    }

    pub fn from_buffers(header: SharedArrayBuffer, buffer: SharedArrayBuffer) -> Self {
        let header_typed = Uint32Array::new(&header);
        let buffer_typed = Uint32Array::new(&buffer);
        Self {
            header,
            header_typed,
            buffer,
            buffer_typed,
        }
    }

    pub fn push_if_space(&self, sample: f32) -> Result<EnqueueResult, JsValue> {
        let end = Atomics::load(&self.header_typed, 1)? as u32;
        let start = Atomics::load(&self.header_typed, 0)? as u32;

        if end == start.wrapping_sub(1) & BUFFER_INDEX_MASK {
            return Ok(EnqueueResult::BufferFull);
        }

        Atomics::store(&self.buffer_typed, end, sample.to_bits() as i32)?;
        let new_end = (end + 1) & BUFFER_INDEX_MASK;
        Atomics::store(&self.header_typed, 1, new_end as i32)?;

        Ok(EnqueueResult::Successful)
    }

    pub fn pop(&self) -> Result<Option<f32>, JsValue> {
        let start = Atomics::load(&self.header_typed, 0)? as u32;
        let end = Atomics::load(&self.header_typed, 1)? as u32;

        if start == end {
            return Ok(None);
        }

        let value = Atomics::load(&self.buffer_typed, start)?;
        let new_start = (start + 1) & BUFFER_INDEX_MASK;
        Atomics::store(&self.header_typed, 0, new_start as i32)?;

        let sample = f32::from_bits(value as u32);
        Ok(Some(sample))
    }

    fn to_js_value(&self) -> JsValue {
        Array::of2(&self.header, &self.buffer).into()
    }
}

const OUTPUT_BUFFER_THRESHOLD: usize = 4096;

#[wasm_bindgen]
pub struct AudioProcessor {
    audio_queue: AudioQueue,
    output_buffer: VecDeque<f32>,
}

#[wasm_bindgen]
impl AudioProcessor {
    #[wasm_bindgen(constructor)]
    pub fn new(audio_queue: JsValue) -> AudioProcessor {
        let audio_queue = AudioQueue::try_from(audio_queue).unwrap();

        AudioProcessor {
            audio_queue,
            output_buffer: VecDeque::new(),
        }
    }

    pub fn process(&mut self, output: &mut [f32]) {
        while let Some(sample) = self.audio_queue.pop().unwrap() {
            self.output_buffer.push_back(sample);
        }

        let len = cmp::min(self.output_buffer.len(), output.len());
        for value in output.iter_mut().take(len) {
            *value = self.output_buffer.pop_front().unwrap();
        }

        while self.output_buffer.len() > OUTPUT_BUFFER_THRESHOLD {
            self.output_buffer.pop_front();
        }
    }
}

pub async fn initialize_audio_worklet(
    audio_ctx: &AudioContext,
    audio_queue: &AudioQueue,
) -> Result<AudioWorkletNode, JsValue> {
    JsFuture::from(
        audio_ctx
            .audio_worklet()?
            .add_module("./audio-processor.js")?,
    )
    .await?;

    let mut node_options = AudioWorkletNodeOptions::new();
    node_options
        .channel_count_mode(ChannelCountMode::Explicit)
        .channel_count(1)
        .processor_options(Some(&Array::of3(
            &wasm_bindgen::module(),
            &wasm_bindgen::memory(),
            &audio_queue.to_js_value(),
        )));

    let worklet_node =
        AudioWorkletNode::new_with_options(audio_ctx, "audio-processor", &node_options)?;
    worklet_node.connect_with_audio_node(&audio_ctx.destination())?;

    Ok(worklet_node)
}
