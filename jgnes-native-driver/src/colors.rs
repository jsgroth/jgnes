use jgnes_core::{ColorEmphasis, FrameBuffer};

// TODO do colors properly
const COLOR_MAPPING: &[u8; 8 * 64 * 3] = include_bytes!("nespalette.pal");

fn get_color_emphasis_offset(color_emphasis: ColorEmphasis) -> u16 {
    64 * u16::from(color_emphasis.red)
        + 128 * u16::from(color_emphasis.green)
        + 256 * u16::from(color_emphasis.blue)
}

pub(crate) fn sdl_texture_updater(
    frame_buffer: &FrameBuffer,
    color_emphasis: ColorEmphasis,
) -> impl FnOnce(&mut [u8], usize) + '_ {
    let color_emphasis_offset = get_color_emphasis_offset(color_emphasis) as usize;
    move |pixels, pitch| {
        for (i, scanline) in frame_buffer[8..232].iter().enumerate() {
            for (j, nes_color) in scanline.iter().copied().enumerate() {
                let color_map_index = color_emphasis_offset + (3 * nes_color) as usize;
                let start = i * pitch + 3 * j;
                pixels[start..start + 3]
                    .copy_from_slice(&COLOR_MAPPING[color_map_index..color_map_index + 3]);
            }
        }
    }
}
