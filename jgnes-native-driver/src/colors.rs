use crate::Overscan;
use jgnes_core::{ColorEmphasis, FrameBuffer};
use std::ops::Range;

// TODO do colors properly
const COLOR_MAPPING: &[u8; 8 * 64 * 3] = include_bytes!("nespalette.pal");

fn get_color_emphasis_offset(color_emphasis: ColorEmphasis) -> u16 {
    64 * u16::from(color_emphasis.red)
        + 128 * u16::from(color_emphasis.green)
        + 256 * u16::from(color_emphasis.blue)
}

fn clear_rows(rows: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for row in rows {
        for col in 0..jgnes_core::SCREEN_WIDTH as usize {
            let start = (row - 8) * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

fn clear_cols(cols: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for col in cols {
        for row in 0..jgnes_core::VISIBLE_SCREEN_HEIGHT as usize {
            let start = row * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

pub(crate) fn sdl_texture_updater(
    frame_buffer: &FrameBuffer,
    color_emphasis: ColorEmphasis,
    overscan: Overscan,
) -> impl FnOnce(&mut [u8], usize) + '_ {
    let top = 8 + overscan.top as usize;
    let bottom = jgnes_core::SCREEN_HEIGHT as usize - 8 - overscan.bottom as usize;
    let left = overscan.left as usize;
    let right = jgnes_core::SCREEN_WIDTH as usize - overscan.right as usize;

    let screen_height = jgnes_core::SCREEN_HEIGHT as usize;
    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
    move |pixels, pitch| {
        clear_rows(8..top, pixels, pitch);
        clear_rows(bottom..screen_height - 8, pixels, pitch);
        clear_cols(0..left, pixels, pitch);
        clear_cols(right..jgnes_core::SCREEN_WIDTH as usize, pixels, pitch);

        for (i, scanline) in frame_buffer
            .iter()
            .enumerate()
            .filter(|(i, _)| (top..bottom).contains(i))
        {
            for (j, nes_color) in scanline
                .iter()
                .copied()
                .enumerate()
                .filter(|(j, _)| (left..right).contains(j))
            {
                let color_map_index = color_emphasis_offset + (3 * nes_color) as usize;
                let start = (i - 8) * pitch + 3 * j;
                pixels[start..start + 3]
                    .copy_from_slice(&COLOR_MAPPING[color_map_index..color_map_index + 3]);
            }
        }
    }
}

pub(crate) fn to_rgba(frame_buffer: &FrameBuffer, color_emphasis: ColorEmphasis, out: &mut [u8]) {
    let screen_width = jgnes_core::SCREEN_WIDTH as usize;
    let screen_height = jgnes_core::SCREEN_HEIGHT as usize;
    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
    for (i, scanline) in frame_buffer[8..screen_height - 8].iter().enumerate() {
        for (j, nes_color) in scanline.iter().copied().enumerate() {
            let color_map_index = color_emphasis_offset + (3 * nes_color) as usize;
            let [r, g, b] = COLOR_MAPPING[color_map_index..color_map_index + 3]
            else {
                unreachable!("destructuring a slice of size 3 into [a, b, c]")
            };

            // TODO configurable
            let out_index = i * 4 * screen_width + j * 4;
            out[out_index..out_index + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
}
