use crate::config::{Color, Config};
use crate::geometry::Rect;

pub const BYTES_PER_PIXEL: usize = 4;
pub const OPAQUE: u8 = u8::MAX;

pub fn draw(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    rect: Option<Rect>,
    opacity: u8,
    stale: Option<Rect>,
    config: &Config,
) {
    Canvas {
        pixels: canvas,
        width: canvas_width,
        height: canvas_height,
    }
    .draw(rect, opacity, stale, config);
}

const BLANK: Color = Color::rgba(0, 0, 0, 0);

struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
}

impl Canvas<'_> {
    fn draw(&mut self, rect: Option<Rect>, opacity: u8, stale: Option<Rect>, config: &Config) {
        if let Some(stale) = stale {
            self.fill_rect(stale, BLANK);
        }

        let Some(rect) = rect else { return };
        if !rect.overlaps_surface(self.width, self.height) {
            return;
        }

        let inset = config.border_width.min(rect.width / 2).min(rect.height / 2);
        let radius = config
            .corner_radius
            .min(rect.width / 2)
            .min(rect.height / 2);

        let fill = config.fill.faded(opacity);
        let border = config.border.faded(opacity);

        if inset > 0 && !border.is_transparent() {
            self.fill_rounded_rect(rect, radius, border);
        }

        let interior = Rect::new(
            rect.x + inset as i32,
            rect.y + inset as i32,
            rect.width - inset * 2,
            rect.height - inset * 2,
        );

        if !interior.is_empty() {
            self.fill_rounded_rect(interior, radius.saturating_sub(inset), fill);
        }
    }

    fn fill_rounded_rect(&mut self, rect: Rect, radius: u32, color: Color) {
        let radius = radius.min(rect.width / 2).min(rect.height / 2);
        if radius == 0 {
            self.fill_rect(rect, color);
            return;
        }

        let pixel = color.to_argb8888();
        let edge = radius as i32;
        let span_width = rect.width - radius * 2;

        self.fill_rect(
            Rect::new(rect.x, rect.y + edge, rect.width, rect.height - radius * 2),
            color,
        );

        let extent = radius as f64;
        for row in 0..radius {
            let top = rect.y + row as i32;
            let bottom = rect.bottom() - 1 - row as i32;

            for y in [top, bottom] {
                self.fill_rect(Rect::new(rect.x + edge, y, span_width, 1), color);
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
                    self.blend_pixel(x, y, pixel, coverage);
                }
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let rect = rect.clamp_to_surface(self.width, self.height);
        if rect.is_empty() {
            return;
        }

        let pixel = color.to_argb8888();
        let stride = self.width as usize * BYTES_PER_PIXEL;

        if rect.x == 0 && rect.width == self.width {
            let start = rect.y as usize * stride;
            let end = rect.bottom() as usize * stride;
            if let Some(block) = self.pixels.get_mut(start..end) {
                fill_pixels(block, pixel);
            }
            return;
        }

        let start_byte = rect.x as usize * BYTES_PER_PIXEL;
        let span_bytes = rect.width as usize * BYTES_PER_PIXEL;

        for row in rect.y as usize..rect.bottom() as usize {
            let offset = row * stride + start_byte;
            let Some(scanline) = self.pixels.get_mut(offset..offset + span_bytes) else {
                return;
            };
            fill_pixels(scanline, pixel);
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, pixel: [u8; 4], coverage: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }

        let offset = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
        let Some(target) = self.pixels.get_mut(offset..offset + BYTES_PER_PIXEL) else {
            return;
        };

        for (channel, source) in target.iter_mut().zip(pixel) {
            *channel = lerp(*channel, source, coverage);
        }
    }
}

fn fill_pixels(bytes: &mut [u8], pixel: [u8; 4]) {
    if let [b, g, r, a] = pixel
        && b == g
        && g == r
        && r == a
    {
        bytes.fill(b);
        return;
    }

    for chunk in bytes.chunks_exact_mut(BYTES_PER_PIXEL) {
        chunk.copy_from_slice(&pixel);
    }
}

fn coverage(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn lerp(from: u8, to: u8, t: u8) -> u8 {
    let t = t as u32;
    ((from as u32 * (255 - t) + to as u32 * t + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 8;
    const H: u32 = 8;

    fn canvas() -> Vec<u8> {
        vec![0xAB; (W * H) as usize * BYTES_PER_PIXEL]
    }

    fn all(w: u32, h: u32) -> Option<Rect> {
        Some(Rect::new(0, 0, w, h))
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
        draw(&mut c, W, H, None, OPAQUE, all(W, H), &config());
        assert!(c.iter().all(|&b| b == 0));
    }

    #[test]
    fn selection_paints_border_outside_and_fill_inside() {
        let mut c = canvas();
        let cfg = config();
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(2, 2, 4, 4)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

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
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(1, 1, 2, 2)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

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
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(2, 2, 4, 4)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

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
        draw(&mut c, w, h, selection, OPAQUE, all(w, h), &square);
        assert_eq!(pixel_at(&c, w, 0, 0), square.fill.to_argb8888());

        let mut c = sized_canvas(w, h);
        draw(&mut c, w, h, selection, OPAQUE, all(w, h), &rounded);
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
        draw(
            &mut c,
            w,
            h,
            Some(Rect::new(0, 0, w, h)),
            OPAQUE,
            all(w, h),
            &cfg,
        );

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
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(0, 0, W, H)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

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
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(6, 0, 8, 4)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

        assert_eq!(pixel(&c, 0, 1), [0; 4], "must not wrap onto the next row");
    }

    #[test]
    fn an_overhanging_selection_keeps_its_geometry() {
        let cfg = Config {
            border_width: 1,
            corner_radius: 0,
            ..config()
        };

        let mut c = canvas();
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(-4, 2, 8, 4)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

        assert_eq!(
            pixel(&c, 0, 3),
            cfg.fill.to_argb8888(),
            "interior, not border"
        );
        assert_eq!(
            pixel(&c, 3, 3),
            cfg.border.to_argb8888(),
            "the right border"
        );
        assert_eq!(pixel(&c, 4, 3), [0; 4], "past the selection");
    }

    #[test]
    fn an_overhanging_selection_is_clipped_not_wrapped() {
        let mut c = canvas();
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(6, 0, 8, 1)),
            OPAQUE,
            all(W, H),
            &cfg,
        );

        assert_eq!(pixel(&c, 7, 0), cfg.fill.to_argb8888());
        assert_eq!(pixel(&c, 0, 1), [0; 4], "must not wrap onto the next row");
    }

    #[test]
    fn erasing_the_stale_region_leaves_nothing_of_the_previous_frame() {
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        let mut c = canvas();

        let first = Rect::new(1, 1, 3, 3);
        draw(&mut c, W, H, Some(first), OPAQUE, all(W, H), &cfg);

        let second = Rect::new(4, 4, 3, 3);
        draw(&mut c, W, H, Some(second), OPAQUE, Some(first), &cfg);

        assert_eq!(pixel(&c, 2, 2), [0; 4], "the old selection is gone");
        assert_eq!(
            pixel(&c, 5, 5),
            cfg.fill.to_argb8888(),
            "the new one is drawn"
        );
    }

    #[test]
    fn an_overlapping_move_does_not_erase_the_new_selection() {
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        let mut c = canvas();

        let first = Rect::new(1, 1, 4, 4);
        draw(&mut c, W, H, Some(first), OPAQUE, all(W, H), &cfg);

        let second = Rect::new(2, 2, 4, 4);
        draw(&mut c, W, H, Some(second), OPAQUE, Some(first), &cfg);

        for (x, y) in [(2, 2), (3, 3), (5, 5)] {
            assert_eq!(pixel(&c, x, y), cfg.fill.to_argb8888(), "({x}, {y})");
        }
        assert_eq!(pixel(&c, 1, 1), [0; 4], "the vacated corner is cleared");
    }

    #[test]
    fn no_stale_region_leaves_the_canvas_untouched_outside_the_selection() {
        let cfg = Config {
            border_width: 0,
            ..config()
        };
        let mut c = canvas();
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(2, 2, 2, 2)),
            OPAQUE,
            None,
            &cfg,
        );

        assert_eq!(pixel(&c, 2, 2), cfg.fill.to_argb8888());
        assert_eq!(pixel(&c, 0, 0), [0xAB; 4], "nothing else was erased");
    }

    #[test]
    fn opacity_scales_the_alpha_of_both_colours() {
        let cfg = config();
        let mut c = canvas();
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(2, 2, 4, 4)),
            0x80,
            all(W, H),
            &cfg,
        );

        assert_eq!(pixel(&c, 2, 2), cfg.border.faded(0x80).to_argb8888());
        assert_eq!(pixel(&c, 3, 3), cfg.fill.faded(0x80).to_argb8888());
        assert_eq!(pixel(&c, 3, 3)[3], 0x80, "an opaque fill at half opacity");
    }

    #[test]
    fn zero_opacity_leaves_only_cleared_pixels() {
        let mut c = canvas();
        draw(
            &mut c,
            W,
            H,
            Some(Rect::new(2, 2, 4, 4)),
            0,
            all(W, H),
            &config(),
        );
        assert!(c.iter().all(|&b| b == 0));
    }
}
