use std::time::{Duration, Instant};

use crate::geometry::Rect;
use crate::render::OPAQUE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub rect: Option<Rect>,
    pub opacity: u8,
    pub leaving: bool,
}

impl Look {
    const HIDDEN: Self = Self {
        rect: None,
        opacity: 0,
        leaving: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Hidden,
    Shown { rect: Rect },
    Leaving { rect: Rect, since: Instant },
}

#[derive(Debug)]
pub struct Fade {
    duration: Duration,
    state: State,
}

impl Fade {
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            state: State::Hidden,
        }
    }

    pub fn track(&mut self, rect: Option<Rect>, now: Instant) {
        self.state = match (self.state, rect) {
            (_, Some(rect)) => State::Shown { rect },
            (State::Shown { rect }, None) => State::Leaving { rect, since: now },
            (state, None) => state,
        };
    }

    pub fn look(&self, now: Instant) -> Look {
        match self.state {
            State::Hidden => Look::HIDDEN,
            State::Shown { rect } => Look {
                rect: Some(rect),
                opacity: OPAQUE,
                leaving: false,
            },
            State::Leaving { rect, since } => {
                let gone = progress(since, now, self.duration);
                if gone >= 1.0 {
                    return Look::HIDDEN;
                }
                Look {
                    rect: Some(rect),
                    opacity: ((1.0 - gone) * OPAQUE as f64).round() as u8,
                    leaving: true,
                }
            }
        }
    }
}

fn progress(since: Instant, now: Instant, duration: Duration) -> f64 {
    if duration.is_zero() {
        return 1.0;
    }

    (now.saturating_duration_since(since).as_secs_f64() / duration.as_secs_f64()).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: Rect = Rect::new(10, 10, 20, 20);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_selection_in_progress_is_fully_opaque() {
        let mut fade = Fade::new(ms(100));
        let t0 = Instant::now();

        fade.track(Some(RECT), t0);
        assert_eq!(
            fade.look(t0 + ms(500)),
            Look {
                rect: Some(RECT),
                opacity: OPAQUE,
                leaving: false,
            }
        );
    }

    #[test]
    fn without_a_duration_the_selection_vanishes_at_once() {
        let mut fade = Fade::new(Duration::ZERO);
        let t0 = Instant::now();

        fade.track(Some(RECT), t0);
        fade.track(None, t0);
        assert_eq!(fade.look(t0), Look::HIDDEN);
    }

    #[test]
    fn a_fade_out_keeps_the_last_rect_and_thins_it_until_it_is_gone() {
        let mut fade = Fade::new(ms(100));
        let t0 = Instant::now();
        let last = Rect::new(10, 10, 40, 40);

        fade.track(Some(RECT), t0);
        fade.track(Some(last), t0);
        fade.track(None, t0);

        let start = fade.look(t0);
        assert_eq!(start.rect, Some(last));
        assert_eq!(start.opacity, OPAQUE);
        assert!(start.leaving);

        let midway = fade.look(t0 + ms(50));
        assert_eq!(midway.rect, Some(last));
        assert_eq!(midway.opacity, 128);

        assert_eq!(fade.look(t0 + ms(100)), Look::HIDDEN);
        assert_eq!(fade.look(t0 + ms(500)), Look::HIDDEN);
    }

    #[test]
    fn a_repeated_release_does_not_restart_the_fade() {
        let mut fade = Fade::new(ms(100));
        let t0 = Instant::now();

        fade.track(Some(RECT), t0);
        fade.track(None, t0);
        fade.track(None, t0 + ms(50));
        assert_eq!(fade.look(t0 + ms(100)), Look::HIDDEN);
    }

    #[test]
    fn a_new_drag_replaces_a_selection_that_is_fading_out() {
        let mut fade = Fade::new(ms(100));
        let t0 = Instant::now();
        let next = Rect::new(100, 100, 5, 5);

        fade.track(Some(RECT), t0);
        fade.track(None, t0);
        fade.track(Some(next), t0 + ms(20));

        let look = fade.look(t0 + ms(20));
        assert_eq!(look.rect, Some(next));
        assert_eq!(look.opacity, OPAQUE);
        assert!(!look.leaving);
    }

    #[test]
    fn nothing_to_track_stays_hidden() {
        let mut fade = Fade::new(ms(100));
        let t0 = Instant::now();

        fade.track(None, t0);
        assert_eq!(fade.look(t0), Look::HIDDEN);
    }
}
