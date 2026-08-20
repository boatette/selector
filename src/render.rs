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
    let radius = config
        .corner_radius
        .min(rect.width / 2)
        .min(rect.height / 2);

    if inset > 0 && !config.border.is_transparent() {
        fill_rounded_rect(
            canvas,
            canvas_width,
            canvas_height,
            rect,
            radius,
            config.border,
        );
    }

    let interior = Rect::new(
        rect.x + inset as i32,
        rect.y + inset as i32,
        rect.width - inset * 2,
        rect.height - inset * 2,
    );

    if !interior.is_empty() {
        fill_rounded_rect(
            canvas,
            canvas_width,
            canvas_height,
            interior,
            radius.saturating_sub(inset),
            config.fill,
        );
    }
}

fn fill_rounded_rect(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    rect: Rect,
    radius: u32,
    color: Color,
) {
    let radius = radius.min(rect.width / 2).min(rect.height / 2);
    if radius == 0 {
        fill_rect(canvas, canvas_width, canvas_height, rect, color);
        return;
    }

    let pixel = color.to_argb8888();
    let edge = radius as i32;
    let span_width = rect.width - radius * 2;

    fill_rect(
        canvas,
        canvas_width,
        canvas_height,
        Rect::new(rect.x, rect.y + edge, rect.width, rect.height - radius * 2),
        color,
    );

    let extent = radius as f64;
    for row in 0..radius {
        let top = rect.y + row as i32;
        let bottom = rect.bottom() - 1 - row as i32;

        for y in [top, bottom] {
            let span = Rect::new(rect.x + edge, y, span_width, 1);
            fill_rect(canvas, canvas_width, canvas_height, span, color);
        }

        let dy = extent - (row as f64 + 0.5);
        for col in 0..radius {
            let dx = extent - (col as f64 + 0.5);
            let coverage = coverage(extent - dx.hypot(dy) + 0.5);
            if coverage == 0 {
                continue;
            }

            let left = rect.x + col as i32;
            let right = rect.right() - 1 - col as i32;
            for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
                blend_pixel(canvas, canvas_width, canvas_height, x, y, pixel, coverage);
            }
        }
    }
}

fn coverage(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn blend_pixel(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    x: i32,
    y: i32,
    pixel: [u8; 4],
    coverage: u8,
) {
    if x < 0 || y < 0 || x >= canvas_width as i32 || y >= canvas_height as i32 {
        return;
    }

    let offset = (y as usize * canvas_width as usize + x as usize) * BYTES_PER_PIXEL;
    let Some(target) = canvas.get_mut(offset..offset + BYTES_PER_PIXEL) else {
        return;
    };

    for (channel, source) in target.iter_mut().zip(pixel) {
        *channel = lerp(*channel, source, coverage);
    }
}

fn lerp(from: u8, to: u8, t: u8) -> u8 {
    let t = t as u32;
    ((from as u32 * (255 - t) + to as u32 * t + 127) / 255) as u8
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

    fn sized_canvas(w: u32, h: u32) -> Vec<u8> {
        vec![0xAB; (w * h) as usize * BYTES_PER_PIXEL]
    }

    fn pixel_at(canvas: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = (y * w + x) as usize * BYTES_PER_PIXEL;
        canvas[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn a_radius_rounds_away_the_corner_a_square_box_keeps() {
        let square = Config {
            border_width: 0,
            corner_radius: 0,
            ..config()
        };
        let rounded = Config {
            corner_radius: 5,
            ..square.clone()
        };

        let (w, h) = (16, 16);
        let selection = Some(Rect::new(0, 0, w, h));

        let mut c = sized_canvas(w, h);
        draw(&mut c, w, h, selection, &square);
        assert_eq!(pixel_at(&c, w, 0, 0), square.fill.to_argb8888());

        let mut c = sized_canvas(w, h);
        draw(&mut c, w, h, selection, &rounded);
        assert_eq!(pixel_at(&c, w, 0, 0), [0; 4], "the corner is cut away");
        assert_eq!(
            pixel_at(&c, w, 8, 0),
            rounded.fill.to_argb8888(),
            "the straight edge is untouched"
        );
        assert_eq!(
            pixel_at(&c, w, 8, 8),
            rounded.fill.to_argb8888(),
            "interior"
        );
    }

    #[test]
    fn the_border_ring_stays_concentric_on_the_curve() {
        let cfg = Config {
            border_width: 2,
            corner_radius: 6,
            ..config()
        };

        let (w, h) = (24, 24);
        let mut c = sized_canvas(w, h);
        draw(&mut c, w, h, Some(Rect::new(0, 0, w, h)), &cfg);

        let border = cfg.border.to_argb8888();
        let fill = cfg.fill.to_argb8888();

        assert_eq!(pixel_at(&c, w, 0, 0), [0; 4], "outside the outer curve");
        assert_eq!(pixel_at(&c, w, 2, 2), border, "between the two curves");
        assert_eq!(pixel_at(&c, w, 4, 4), fill, "inside the inner curve");
        assert_eq!(pixel_at(&c, w, 12, 0), border, "the straight edge");
        assert_eq!(pixel_at(&c, w, 12, 2), fill, "inside the straight edge");
    }

    #[test]
    fn a_radius_larger_than_the_selection_is_clamped() {
        let cfg = Config {
            border_width: 0,
            corner_radius: 100,
            ..config()
        };

        let mut c = canvas();
        draw(&mut c, W, H, Some(Rect::new(0, 0, W, H)), &cfg);

        assert_eq!(pixel(&c, 0, 0), [0; 4], "a circle has no corner");
        assert_eq!(pixel(&c, 4, 4), cfg.fill.to_argb8888(), "centre is filled");
    }

    #[test]
    fn an_overhanging_rounded_selection_does_not_wrap() {
        let cfg = Config {
            border_width: 0,
            corner_radius: 2,
            ..config()
        };

        let mut c = canvas();
        draw(&mut c, W, H, Some(Rect::new(6, 0, 8, 4)), &cfg);

        assert_eq!(pixel(&c, 0, 1), [0; 4], "must not wrap onto the next row");
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
