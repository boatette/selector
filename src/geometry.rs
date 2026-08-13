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
    fn clamping_a_fully_offscreen_rect_yields_empty() {
        let r = Rect::new(100, 100, 10, 10).clamp_to_surface(20, 20);
        assert!(r.is_empty());
    }
}
