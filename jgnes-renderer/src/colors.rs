use crate::config::Overscan;
use jgnes_core::{ColorEmphasis, FrameBuffer};
use std::ops::Range;

// TODO support color customization
const COLOR_MAPPING: &[u8; 8 * 64 * 3] = include_bytes!("nespalette.pal");

fn get_color_emphasis_offset(color_emphasis: ColorEmphasis) -> u16 {
    3 * 64 * u16::from(color_emphasis.red)
        + 3 * 128 * u16::from(color_emphasis.green)
        + 3 * 256 * u16::from(color_emphasis.blue)
}

fn clear_rows_rgb(rows: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for row in rows {
        for col in 0..jgnes_core::SCREEN_WIDTH as usize {
            let start = (row - 8) * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

fn clear_rows_rgba(rows: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for row in rows {
        for col in 0..jgnes_core::SCREEN_WIDTH as usize {
            let start = (row - 8) * pitch + 4 * col;
            pixels[start..start + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

fn clear_cols_rgb(cols: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for col in cols {
        for row in 0..jgnes_core::VISIBLE_SCREEN_HEIGHT as usize {
            let start = row * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

fn clear_cols_rgba(cols: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for col in cols {
        for row in 0..jgnes_core::VISIBLE_SCREEN_HEIGHT as usize {
            let start = row * pitch + 4 * col;
            pixels[start..start + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

pub fn sdl_texture_updater(
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
        clear_rows_rgb(8..top, pixels, pitch);
        clear_rows_rgb(bottom..screen_height - 8, pixels, pitch);
        clear_cols_rgb(0..left, pixels, pitch);
        clear_cols_rgb(right..jgnes_core::SCREEN_WIDTH as usize, pixels, pitch);

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

pub fn to_rgba(
    frame_buffer: &FrameBuffer,
    color_emphasis: ColorEmphasis,
    overscan: Overscan,
    out: &mut [u8],
) {
    let top = 8 + overscan.top as usize;
    let bottom = jgnes_core::SCREEN_HEIGHT as usize - 8 - overscan.bottom as usize;
    let left = overscan.left as usize;
    let right = jgnes_core::SCREEN_WIDTH as usize - overscan.right as usize;

    let screen_width = jgnes_core::SCREEN_WIDTH as usize;
    let screen_height = jgnes_core::SCREEN_HEIGHT as usize;

    let pitch = 4 * screen_width;
    clear_rows_rgba(8..top, out, pitch);
    clear_rows_rgba(bottom..screen_height - 8, out, pitch);
    clear_cols_rgba(0..left, out, pitch);
    clear_cols_rgba(right..screen_width, out, pitch);

    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
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
            let [r, g, b] = COLOR_MAPPING[color_map_index..color_map_index + 3]
            else {
                unreachable!("destructuring a slice of size 3 into [a, b, c]")
            };

            let out_index = (i - 8) * 4 * screen_width + j * 4;
            out[out_index..out_index + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
}
