#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx.hypot(dy)
    }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self {
        Self::new(x, y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_corners(a: Point, b: Point) -> Self {
        let left = a.x.min(b.x).floor();
        let top = a.y.min(b.y).floor();
        let right = a.x.max(b.x).ceil();
        let bottom = a.y.max(b.y).ceil();

        Self {
            x: left as i32,
            y: top as i32,
            width: (right - left).max(0.0) as u32,
            height: (bottom - top).max(0.0) as u32,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    pub fn rounded_spans(&self, radius: u32) -> Vec<Self> {
        let radius = radius.min(self.width / 2).min(self.height / 2);
        if radius == 0 || self.is_empty() {
            return vec![*self];
        }

        let mut spans = Vec::new();

        if self.height > radius * 2 {
            spans.push(Self::new(
                self.x,
                self.y + radius as i32,
                self.width,
                self.height - radius * 2,
            ));
        }

        let mut row = 0;
        while row < radius {
            let inset = rounded_row_inset(radius, row);
            let mut end = row + 1;
            while end < radius && rounded_row_inset(radius, end) == inset {
                end += 1;
            }

            let width = self.width - inset * 2;
            if width > 0 {
                let height = end - row;
                let left = self.x + inset as i32;
                spans.push(Self::new(left, self.y + row as i32, width, height));
                spans.push(Self::new(left, self.bottom() - end as i32, width, height));
            }

            row = end;
        }

        spans
    }

    pub fn clamp_to_surface(&self, width: u32, height: u32) -> Self {
        let left = self.x.clamp(0, width as i32);
        let top = self.y.clamp(0, height as i32);
        let right = self.right().clamp(0, width as i32);
        let bottom = self.bottom().clamp(0, height as i32);

        Self {
            x: left,
            y: top,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        }
    }
}

pub fn rounded_row_inset(radius: u32, row: u32) -> u32 {
    if radius == 0 || row >= radius {
        return 0;
    }

    let radius = radius as f64;
    let dy = radius - (row as f64 + 0.5);
    let inset = radius - (radius * radius - dy * dy).sqrt() - 0.5;

    inset.ceil().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_normalize_in_any_direction() {
        let expected = Rect::new(10, 20, 30, 40);
        let a = Point::new(10.0, 20.0);
        let b = Point::new(40.0, 60.0);
        assert_eq!(Rect::from_corners(a, b), expected);
        assert_eq!(Rect::from_corners(b, a), expected);
        assert_eq!(
            Rect::from_corners(Point::new(40.0, 20.0), Point::new(10.0, 60.0)),
            expected
        );
    }

    #[test]
    fn corners_round_outward_so_the_band_covers_the_pointer() {
        let r = Rect::from_corners(Point::new(10.7, 20.2), Point::new(40.1, 60.9));
        assert_eq!(r, Rect::new(10, 20, 31, 41));
    }

    #[test]
    fn degenerate_drag_is_empty() {
        let p = Point::new(5.0, 5.0);
        assert!(Rect::from_corners(p, p).is_empty());
    }

    #[test]
    fn clamping_trims_overhang_on_both_axes() {
        let r = Rect::new(-10, -10, 40, 40).clamp_to_surface(20, 20);
        assert_eq!(r, Rect::new(0, 0, 20, 20));
    }

    #[test]
    fn a_zero_radius_is_one_span() {
        let r = Rect::new(3, 4, 10, 8);
        assert_eq!(r.rounded_spans(0), vec![r]);
    }

    #[test]
    fn spans_stay_inside_the_rect_and_never_overlap() {
        let r = Rect::new(2, 3, 20, 14);
        let spans = r.rounded_spans(5);

        let mut seen = std::collections::HashSet::new();
        for span in &spans {
            assert!(span.x >= r.x && span.right() <= r.right(), "{span:?}");
            assert!(span.y >= r.y && span.bottom() <= r.bottom(), "{span:?}");
            for y in span.y..span.bottom() {
                for x in span.x..span.right() {
                    assert!(seen.insert((x, y)), "({x}, {y}) covered twice");
                }
            }
        }

        assert!(
            seen.contains(&(r.x, r.y + 7)),
            "the middle band reaches the edge"
        );
        assert!(!seen.contains(&(r.x, r.y)), "the corner is cut away");
    }

    #[test]
    fn spans_are_symmetric_on_both_axes() {
        let r = Rect::new(0, 0, 16, 16);
        let covered: std::collections::HashSet<_> = r
            .rounded_spans(6)
            .iter()
            .flat_map(|s| {
                (s.y..s.bottom()).flat_map(move |y| (s.x..s.right()).map(move |x| (x, y)))
            })
            .collect();

        for &(x, y) in &covered {
            assert!(covered.contains(&(15 - x, y)), "mirror of ({x}, {y})");
            assert!(covered.contains(&(x, 15 - y)), "mirror of ({x}, {y})");
        }
    }

    #[test]
    fn a_radius_larger_than_the_rect_is_clamped() {
        let r = Rect::new(0, 0, 8, 6);
        let spans = r.rounded_spans(99);
        assert!(!spans.is_empty());
        for span in spans {
            assert!(span.right() <= r.right() && span.bottom() <= r.bottom());
        }
    }

    #[test]
    fn clamping_a_fully_offscreen_rect_yields_empty() {
        let r = Rect::new(100, 100, 10, 10).clamp_to_surface(20, 20);
        assert!(r.is_empty());
    }
}
