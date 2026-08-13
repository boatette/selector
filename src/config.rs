use smithay_client_toolkit::shell::wlr_layer::Layer;

// straight (non-premultiplied) 8-bit rgba
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }

    // wl_shm Argb8888 is little-endian and premultiplied, so scale the channels by alpha once here rather than at every pixel write
    pub fn to_argb8888(self) -> [u8; 4] {
        let premultiply = |c: u8| ((c as u32 * self.a as u32 + 127) / 255) as u8;
        [
            premultiply(self.b),
            premultiply(self.g),
            premultiply(self.r),
            self.a,
        ]
    }
}

// compiled-in defaults for now, this is the seam where a config file or cli flags land
#[derive(Debug, Clone)]
pub struct Config {
    pub fill: Color,
    pub border: Color,
    // zero disables the outline
    pub border_width: u32,
    // Bottom puts us above the wallpaper (Background) but below every ordinary window,
    // so drags land here only when the desktop underneath is empty
    pub layer: Layer,
    // how far the pointer must travel before a press counts as a drag, without it a plain click flashes a one-pixel rectangle
    pub drag_threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fill: Color::rgba(0x4c, 0x9e, 0xd9, 0x40),
            border: Color::rgba(0x4c, 0x9e, 0xd9, 0xcc),
            border_width: 1,
            layer: Layer::Bottom,
            drag_threshold: 3.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_colors_survive_premultiplication() {
        let argb = Color::rgba(0x12, 0x34, 0x56, 0xff).to_argb8888();
        assert_eq!(argb, [0x56, 0x34, 0x12, 0xff]);
    }

    #[test]
    fn half_alpha_scales_the_color_channels() {
        let argb = Color::rgba(0xff, 0xff, 0xff, 0x80).to_argb8888();
        assert_eq!(argb, [0x80, 0x80, 0x80, 0x80]);
    }

    #[test]
    fn zero_alpha_clears_every_channel() {
        assert_eq!(Color::rgba(0xff, 0xff, 0xff, 0).to_argb8888(), [0, 0, 0, 0]);
    }
}
