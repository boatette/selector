#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx.hypot(dy)
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
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
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

    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub const fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub const fn bottom(&self) -> i32 {
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

    pub const fn translate(&self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
            ..*self
        }
    }

    pub fn union(&self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return *self;
        }

        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());

        Self {
            x,
            y,
            width: right.saturating_sub(x) as u32,
            height: bottom.saturating_sub(y) as u32,
        }
    }

    pub const fn grow(&self, margin: u32) -> Self {
        Self {
            x: self.x.saturating_sub(margin as i32),
            y: self.y.saturating_sub(margin as i32),
            width: self.width.saturating_add(margin * 2),
            height: self.height.saturating_add(margin * 2),
        }
    }

    pub const fn inset(&self, margin: u32) -> Self {
        Self {
            x: self.x.saturating_add(margin as i32),
            y: self.y.saturating_add(margin as i32),
            width: self.width.saturating_sub(margin.saturating_mul(2)),
            height: self.height.saturating_sub(margin.saturating_mul(2)),
        }
    }

    pub fn intersect(&self, other: Self) -> Self {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left).max(0) as u32,
            height: bottom.saturating_sub(top).max(0) as u32,
        }
    }

    pub fn minus(&self, hole: Self) -> [Self; 4] {
        const NONE: Rect = Rect::new(0, 0, 0, 0);

        let hole = self.intersect(hole);
        if hole.is_empty() {
            return [*self, NONE, NONE, NONE];
        }

        let above = (hole.y - self.y) as u32;
        let below = (self.bottom() - hole.bottom()) as u32;
        let before = (hole.x - self.x) as u32;
        let after = (self.right() - hole.right()) as u32;

        [
            Self::new(self.x, self.y, self.width, above),
            Self::new(self.x, hole.bottom(), self.width, below),
            Self::new(self.x, hole.y, before, hole.height),
            Self::new(hole.right(), hole.y, after, hole.height),
        ]
    }

    pub fn overlaps_surface(&self, width: u32, height: u32) -> bool {
        !self.clamp_to_surface(width, height).is_empty()
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

fn rounded_row_inset(radius: u32, row: u32) -> u32 {
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
    fn translating_moves_the_rect_without_resizing_it() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!(r.translate(-1920, 5), Rect::new(-1910, 25, 30, 40));
        assert_eq!(r.translate(0, 0), r);
    }

    #[test]
    fn clamping_a_fully_offscreen_rect_yields_empty() {
        let r = Rect::new(100, 100, 10, 10).clamp_to_surface(20, 20);
        assert!(r.is_empty());
    }

    #[test]
    fn overlapping_covers_partial_and_offscreen_rects() {
        assert!(Rect::new(-10, -10, 40, 40).overlaps_surface(20, 20));
        assert!(Rect::new(19, 19, 1, 1).overlaps_surface(20, 20));
        assert!(!Rect::new(100, 100, 10, 10).overlaps_surface(20, 20));
        assert!(!Rect::new(20, 0, 10, 10).overlaps_surface(20, 20));
        assert!(!Rect::new(0, 0, 0, 0).overlaps_surface(20, 20));
    }

    #[test]
    fn a_union_covers_both_rects() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(20, 5, 10, 10);
        assert_eq!(a.union(b), Rect::new(0, 0, 30, 15));
        assert_eq!(a.union(b), b.union(a));
    }

    #[test]
    fn a_union_with_an_empty_rect_is_the_other_rect() {
        let a = Rect::new(3, 4, 10, 10);
        assert_eq!(a.union(Rect::new(0, 0, 0, 0)), a);
        assert_eq!(Rect::new(0, 0, 0, 0).union(a), a);
    }

    #[test]
    fn a_union_of_nested_rects_is_the_outer_one() {
        let outer = Rect::new(0, 0, 20, 20);
        assert_eq!(outer.union(Rect::new(5, 5, 4, 4)), outer);
    }

    #[test]
    fn growing_expands_on_every_side() {
        assert_eq!(Rect::new(5, 5, 10, 10).grow(2), Rect::new(3, 3, 14, 14));
        assert_eq!(Rect::new(5, 5, 10, 10).grow(0), Rect::new(5, 5, 10, 10));
    }

    #[test]
    fn insetting_shrinks_on_every_side_until_nothing_is_left() {
        assert_eq!(Rect::new(5, 5, 10, 10).inset(2), Rect::new(7, 7, 6, 6));
        assert!(Rect::new(5, 5, 10, 10).inset(5).is_empty());
        assert!(Rect::new(5, 5, 10, 10).inset(99).is_empty());
    }

    #[test]
    fn an_intersection_is_the_shared_area() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(Rect::new(5, -5, 10, 10)), Rect::new(5, 0, 5, 5));
        assert_eq!(a.intersect(Rect::new(2, 2, 3, 3)), Rect::new(2, 2, 3, 3));
        assert!(a.intersect(Rect::new(10, 0, 5, 5)).is_empty());
        assert!(a.intersect(Rect::new(-50, -50, 5, 5)).is_empty());
    }

    fn pixels(rects: &[Rect]) -> Vec<(i32, i32)> {
        rects
            .iter()
            .flat_map(|r| {
                (r.y..r.bottom()).flat_map(move |y| (r.x..r.right()).map(move |x| (x, y)))
            })
            .collect()
    }

    #[test]
    fn subtracting_leaves_exactly_the_uncovered_pixels_once() {
        let r = Rect::new(-3, 2, 12, 9);
        let holes = [
            Rect::new(0, 4, 4, 3),
            Rect::new(-10, -10, 12, 15),
            Rect::new(5, 5, 20, 20),
            Rect::new(-3, 2, 12, 9),
            Rect::new(50, 50, 5, 5),
            Rect::new(0, 0, 0, 0),
        ];

        for hole in holes {
            let strips = r.minus(hole);
            let covered = pixels(&strips);
            let unique: std::collections::HashSet<_> = covered.iter().copied().collect();
            assert_eq!(unique.len(), covered.len(), "strips overlap for {hole:?}");

            let expected: std::collections::HashSet<_> = pixels(&[r])
                .into_iter()
                .filter(|&(x, y)| {
                    !(x >= hole.x && x < hole.right() && y >= hole.y && y < hole.bottom())
                })
                .collect();
            assert_eq!(unique, expected, "wrong coverage for {hole:?}");
        }
    }
}
