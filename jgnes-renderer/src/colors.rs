use crate::config::Overscan;
use jgnes_core::{ColorEmphasis, FrameBuffer, TimingMode};
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
            let start = row * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

fn clear_rows_rgba(rows: Range<usize>, pixels: &mut [u8], pitch: usize) {
    for row in rows {
        for col in 0..jgnes_core::SCREEN_WIDTH as usize {
            let start = row * pitch + 4 * col;
            pixels[start..start + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

fn clear_cols_rgb(cols: Range<usize>, pixels: &mut [u8], pitch: usize, visible_screen_height: u16) {
    for col in cols {
        for row in 0..visible_screen_height as usize {
            let start = row * pitch + 3 * col;
            pixels[start..start + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
}

fn clear_cols_rgba(
    cols: Range<usize>,
    pixels: &mut [u8],
    pitch: usize,
    visible_screen_height: u16,
) {
    for col in cols {
        for row in 0..visible_screen_height as usize {
            let start = row * pitch + 4 * col;
            pixels[start..start + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

fn row_offset_for(timing_mode: TimingMode) -> usize {
    match timing_mode {
        TimingMode::Ntsc => 8,
        TimingMode::Pal => 0,
    }
}

pub fn sdl_texture_updater(
    frame_buffer: &FrameBuffer,
    color_emphasis: ColorEmphasis,
    overscan: Overscan,
    timing_mode: TimingMode,
) -> impl FnOnce(&mut [u8], usize) + '_ {
    let screen_height = jgnes_core::SCREEN_HEIGHT as usize;
    let visible_screen_height = timing_mode.visible_screen_height();

    let row_offset = row_offset_for(timing_mode);

    let top = row_offset + overscan.top as usize;
    let bottom = screen_height - row_offset - overscan.bottom as usize;
    let left = overscan.left as usize;
    let right = jgnes_core::SCREEN_WIDTH as usize - overscan.right as usize;

    let top_clear_range = 0..overscan.top as usize;
    let bottom_clear_range = (visible_screen_height as usize - overscan.bottom as usize)
        ..(visible_screen_height as usize);

    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
    move |pixels, pitch| {
        clear_rows_rgb(top_clear_range, pixels, pitch);
        clear_rows_rgb(bottom_clear_range, pixels, pitch);
        clear_cols_rgb(0..left, pixels, pitch, visible_screen_height);
        clear_cols_rgb(
            right..jgnes_core::SCREEN_WIDTH as usize,
            pixels,
            pitch,
            visible_screen_height,
        );

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
                let start = (i - row_offset) * pitch + 3 * j;
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
    timing_mode: TimingMode,
    out: &mut [u8],
) {
    let row_offset = row_offset_for(timing_mode);

    let screen_width = jgnes_core::SCREEN_WIDTH as usize;
    let screen_height = jgnes_core::SCREEN_HEIGHT as usize;
    let visible_screen_height = timing_mode.visible_screen_height();

    let top = row_offset + overscan.top as usize;
    let bottom = screen_height - row_offset - overscan.bottom as usize;
    let left = overscan.left as usize;
    let right = screen_width - overscan.right as usize;

    let top_clear_range = 0..overscan.top as usize;
    let bottom_clear_range = (visible_screen_height as usize - overscan.bottom as usize)
        ..(visible_screen_height as usize);

    let pitch = 4 * screen_width;
    clear_rows_rgba(top_clear_range, out, pitch);
    clear_rows_rgba(bottom_clear_range, out, pitch);
    clear_cols_rgba(0..left, out, pitch, visible_screen_height);
    clear_cols_rgba(right..screen_width, out, pitch, visible_screen_height);

    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
    for (scanline_idx, scanline) in frame_buffer
        .iter()
        .enumerate()
        .filter(|(i, _)| (top..bottom).contains(i))
    {
        for (color_idx, nes_color) in scanline
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

            let out_index = (scanline_idx - row_offset) * 4 * screen_width + color_idx * 4;
            out[out_index..out_index + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_rgba_does_not_panic() {
        let frame_buffer =
            [[0; jgnes_core::SCREEN_WIDTH as usize]; jgnes_core::SCREEN_HEIGHT as usize];

        for &timing_mode in TimingMode::all() {
            let mut output_buffer = vec![
                0;
                4 * jgnes_core::SCREEN_WIDTH as usize
                    * timing_mode.visible_screen_height() as usize
            ];

            to_rgba(
                &frame_buffer,
                ColorEmphasis::default(),
                Overscan::default(),
                timing_mode,
                &mut output_buffer,
            );
            to_rgba(
                &frame_buffer,
                ColorEmphasis::default(),
                Overscan {
                    top: 8,
                    bottom: 8,
                    left: 8,
                    right: 8,
                },
                timing_mode,
                &mut output_buffer,
            );
        }
    }

    #[test]
    fn sdl_texture_updater_does_not_panic() {
        let frame_buffer =
            [[0; jgnes_core::SCREEN_WIDTH as usize]; jgnes_core::SCREEN_HEIGHT as usize];

        for &timing_mode in TimingMode::all() {
            let mut pixels = vec![
                0;
                3 * jgnes_core::SCREEN_WIDTH as usize
                    * timing_mode.visible_screen_height() as usize
            ];
            let pitch = 3 * jgnes_core::SCREEN_WIDTH as usize;

            let updater = sdl_texture_updater(
                &frame_buffer,
                ColorEmphasis::default(),
                Overscan::default(),
                timing_mode,
            );
            updater(&mut pixels, pitch);

            let updater = sdl_texture_updater(
                &frame_buffer,
                ColorEmphasis::default(),
                Overscan {
                    top: 8,
                    bottom: 8,
                    left: 8,
                    right: 8,
                },
                timing_mode,
            );
            updater(&mut pixels, pitch);
        }
    }
}
