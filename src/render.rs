use crate::config::{Color, Config};
use crate::geometry::Rect;

pub const BYTES_PER_PIXEL: usize = 4;

pub fn draw(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    rect: Option<Rect>,
    config: &Config,
) {
    canvas.fill(0);

    let Some(rect) = rect else { return };
    let rect = rect.clamp_to_surface(canvas_width, canvas_height);
    if rect.is_empty() {
        return;
    }

    let inset = config.border_width.min(rect.width / 2).min(rect.height / 2);

    if inset > 0 && !config.border.is_transparent() {
        fill_rect(canvas, canvas_width, canvas_height, rect, config.border);
    }

    let interior = Rect::new(
        rect.x + inset as i32,
        rect.y + inset as i32,
        rect.width - inset * 2,
        rect.height - inset * 2,
    );

    if !interior.is_empty() {
        fill_rect(canvas, canvas_width, canvas_height, interior, config.fill);
    }
}

fn fill_rect(canvas: &mut [u8], canvas_width: u32, canvas_height: u32, rect: Rect, color: Color) {
    let rect = rect.clamp_to_surface(canvas_width, canvas_height);
    if rect.is_empty() {
        return;
    }

    let pixel = color.to_argb8888();
    let stride = canvas_width as usize * BYTES_PER_PIXEL;
    let start_byte = rect.x as usize * BYTES_PER_PIXEL;
    let span_bytes = rect.width as usize * BYTES_PER_PIXEL;

    for row in rect.y as usize..rect.bottom() as usize {
        let offset = row * stride + start_byte;
        let Some(scanline) = canvas.get_mut(offset..offset + span_bytes) else {
            return;
        };
        for chunk in scanline.chunks_exact_mut(BYTES_PER_PIXEL) {
            chunk.copy_from_slice(&pixel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 8;
    const H: u32 = 8;

    fn canvas() -> Vec<u8> {
        vec![0xAB; (W * H) as usize * BYTES_PER_PIXEL]
    }

    fn pixel(canvas: &[u8], x: u32, y: u32) -> [u8; 4] {
        let offset = (y * W + x) as usize * BYTES_PER_PIXEL;
        canvas[offset..offset + 4].try_into().unwrap()
    }

    fn config() -> Config {
        Config {
            fill: Color::rgba(0x00, 0x00, 0xff, 0xff),
            border: Color::rgba(0xff, 0x00, 0x00, 0xff),
            border_width: 1,
            ..Config::default()
        }
    }

    #[test]
    fn no_selection_clears_stale_pixels() {
        let mut c = canvas();
        draw(&mut c, W, H, None, &config());
        assert!(c.iter().all(|&b| b == 0));
    }

    #[test]
    fn selection_paints_border_outside_and_fill_inside() {
        let mut c = canvas();
        let cfg = config();
        draw(&mut c, W, H, Some(Rect::new(2, 2, 4, 4)), &cfg);

        assert_eq!(pixel(&c, 2, 2), cfg.border.to_argb8888(), "top-left border");
        assert_eq!(
            pixel(&c, 5, 5),
            cfg.border.to_argb8888(),
            "bottom-right border"
        );
        assert_eq!(pixel(&c, 3, 3), cfg.fill.to_argb8888(), "interior");
        assert_eq!(pixel(&c, 0, 0), [0; 4], "outside the selection");
        assert_eq!(pixel(&c, 6, 6), [0; 4], "just past the selection");
    }

    #[test]
    fn a_selection_thinner_than_its_border_is_all_border() {
        let mut c = canvas();
        let cfg = Config {
            border_width: 4,
            ..config()
        };
        draw(&mut c, W, H, Some(Rect::new(1, 1, 2, 2)), &cfg);

        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            assert_eq!(pixel(&c, x, y), cfg.border.to_argb8888());
        }
    }

    #[test]
    fn zero_border_width_fills_the_whole_selection() {
        let mut c = canvas();
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        draw(&mut c, W, H, Some(Rect::new(2, 2, 4, 4)), &cfg);

        assert_eq!(pixel(&c, 2, 2), cfg.fill.to_argb8888());
        assert_eq!(pixel(&c, 5, 5), cfg.fill.to_argb8888());
    }

    #[test]
    fn an_overhanging_selection_is_clipped_not_wrapped() {
        let mut c = canvas();
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        draw(&mut c, W, H, Some(Rect::new(6, 0, 8, 1)), &cfg);

        assert_eq!(pixel(&c, 7, 0), cfg.fill.to_argb8888());
        assert_eq!(pixel(&c, 0, 1), [0; 4], "must not wrap onto the next row");
    }
}
