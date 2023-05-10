pub mod colors;
pub mod config;
mod renderer;

use crate::config::AspectRatio;
pub use renderer::WgpuRenderer;
use std::cmp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayArea {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Determine the display area given the specified window size, aspect ratio, and forced scaling.
#[must_use]
pub fn determine_display_area(
    window_width: u32,
    window_height: u32,
    aspect_ratio: AspectRatio,
    forced_integer_height_scaling: bool,
) -> DisplayArea {
    match aspect_ratio {
        AspectRatio::Stretched => DisplayArea {
            x: 0,
            y: 0,
            width: window_width,
            height: window_height,
        },
        AspectRatio::Ntsc | AspectRatio::SquarePixels | AspectRatio::FourThree => {
            let width_to_height_ratio = match aspect_ratio {
                AspectRatio::Ntsc => 64.0 / 49.0,
                AspectRatio::SquarePixels => 8.0 / 7.0,
                AspectRatio::FourThree => 4.0 / 3.0,
                AspectRatio::Stretched => unreachable!("nested match expressions"),
            };

            let visible_screen_height = u32::from(jgnes_core::VISIBLE_SCREEN_HEIGHT);

            let width = cmp::min(
                window_width,
                (f64::from(window_height) * width_to_height_ratio).round() as u32,
            );
            let height = cmp::min(
                window_height,
                (f64::from(width) / width_to_height_ratio).round() as u32,
            );
            let (width, height) =
                if forced_integer_height_scaling && height >= visible_screen_height {
                    let height = visible_screen_height * (height / visible_screen_height);
                    let width = cmp::min(
                        window_width,
                        (f64::from(height) * width_to_height_ratio).round() as u32,
                    );
                    (width, height)
                } else {
                    (width, height)
                };

            // The computed width and height should never be higher than the window width/height,
            // but this is nicer API-wise than using asserts.
            let width = cmp::min(width, window_width);
            let height = cmp::min(height, window_height);

            DisplayArea {
                x: (window_width - width) / 2,
                y: (window_height - height) / 2,
                width,
                height,
            }
        }
    }
}
