use crate::geometry::{Point, Rect};

pub const BTN_LEFT: u32 = 0x110;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    Armed { origin: Point },
    Dragging { origin: Point, current: Point },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redraw {
    Needed,
    None,
}

impl Redraw {
    pub fn is_needed(self) -> bool {
        self == Redraw::Needed
    }

    pub fn or(self, other: Redraw) -> Redraw {
        if self.is_needed() || other.is_needed() {
            Redraw::Needed
        } else {
            Redraw::None
        }
    }
}

#[derive(Debug)]
pub struct Selection {
    state: State,
    threshold: f64,
}

impl Selection {
    pub fn new(threshold: f64) -> Self {
        Self {
            state: State::Idle,
            threshold,
        }
    }

    pub fn rect(&self) -> Option<Rect> {
        match self.state {
            State::Idle | State::Armed { .. } => None,
            State::Dragging { origin, current } => Some(Rect::from_corners(origin, current)),
        }
    }

    pub fn press(&mut self, at: Point) -> Redraw {
        let was_visible = self.rect().is_some();
        self.state = State::Armed { origin: at };
        if was_visible {
            Redraw::Needed
        } else {
            Redraw::None
        }
    }

    pub fn motion(&mut self, at: Point) -> Redraw {
        match self.state {
            State::Idle => Redraw::None,
            State::Armed { origin } => {
                if origin.distance(at) >= self.threshold {
                    self.state = State::Dragging {
                        origin,
                        current: at,
                    };
                    Redraw::Needed
                } else {
                    Redraw::None
                }
            }
            State::Dragging { origin, current } => {
                if current == at {
                    return Redraw::None;
                }
                self.state = State::Dragging {
                    origin,
                    current: at,
                };
                Redraw::Needed
            }
        }
    }

    pub fn release(&mut self) -> (Option<Rect>, Redraw) {
        let rect = self.rect();
        let redraw = if rect.is_some() {
            Redraw::Needed
        } else {
            Redraw::None
        };
        self.state = State::Idle;
        (rect, redraw)
    }

    pub fn cancel(&mut self) -> Redraw {
        let redraw = if self.rect().is_some() {
            Redraw::Needed
        } else {
            Redraw::None
        };
        self.state = State::Idle;
        redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> Selection {
        Selection::new(3.0)
    }

    #[test]
    fn a_press_alone_draws_nothing() {
        let mut s = selection();
        assert_eq!(s.press(Point::new(10.0, 10.0)), Redraw::None);
        assert_eq!(s.rect(), None);
    }

    #[test]
    fn motion_below_the_threshold_stays_invisible() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        assert_eq!(s.motion(Point::new(11.0, 11.0)), Redraw::None);
        assert_eq!(s.rect(), None);
    }

    #[test]
    fn crossing_the_threshold_starts_drawing_from_the_press_origin() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        assert_eq!(s.motion(Point::new(30.0, 40.0)), Redraw::Needed);
        assert_eq!(s.rect(), Some(Rect::new(10, 10, 20, 30)));
    }

    #[test]
    fn repeated_motion_at_the_same_point_does_not_redraw() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        s.motion(Point::new(30.0, 40.0));
        assert_eq!(s.motion(Point::new(30.0, 40.0)), Redraw::None);
    }

    #[test]
    fn release_yields_the_rect_and_returns_to_idle() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        s.motion(Point::new(30.0, 40.0));

        let (rect, redraw) = s.release();
        assert_eq!(rect, Some(Rect::new(10, 10, 20, 30)));
        assert_eq!(redraw, Redraw::Needed);
        assert_eq!(s.rect(), None);
        assert_eq!(
            s.motion(Point::new(99.0, 99.0)),
            Redraw::None,
            "motion after release must not resume the drag"
        );
    }

    #[test]
    fn a_click_without_a_drag_yields_no_selection() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        s.motion(Point::new(11.0, 10.0));

        let (rect, redraw) = s.release();
        assert_eq!(rect, None);
        assert_eq!(redraw, Redraw::None);
    }

    #[test]
    fn cancel_discards_an_in_progress_drag() {
        let mut s = selection();
        s.press(Point::new(10.0, 10.0));
        s.motion(Point::new(30.0, 40.0));

        assert_eq!(s.cancel(), Redraw::Needed);
        assert_eq!(s.rect(), None);
        assert_eq!(
            s.motion(Point::new(99.0, 99.0)),
            Redraw::None,
            "motion after cancel must not resume the drag"
        );
    }

    #[test]
    fn motion_while_idle_is_ignored() {
        let mut s = selection();
        assert_eq!(s.motion(Point::new(30.0, 40.0)), Redraw::None);
        assert_eq!(s.rect(), None);
    }

    #[test]
    fn redraw_or_needs_a_redraw_if_either_side_does() {
        assert_eq!(Redraw::None.or(Redraw::None), Redraw::None);
        assert_eq!(Redraw::None.or(Redraw::Needed), Redraw::Needed);
        assert_eq!(Redraw::Needed.or(Redraw::None), Redraw::Needed);
    }
}
