//! A simple peek-out-and-hide motion used until the timeline player lands.
//! It is a pure function of time so it can be tested without a window.

/// Seconds to slide out (with a small overshoot).
pub const RISE: f32 = 0.55;
/// Seconds to stay out.
pub const HOLD: f32 = 2.5;
/// Seconds to slide back behind the edge.
pub const HIDE: f32 = 0.45;
pub const DURATION: f32 = RISE + HOLD + HIDE;

/// Ease-out with a slight overshoot ("back" easing).
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.2;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// How far the cube is out `t` seconds after the peek started, from 0.0
/// (hidden) to 1.0 (out). `None` once the peek is over.
pub fn reveal_at(t: f32) -> Option<f32> {
    if t < 0.0 {
        Some(0.0)
    } else if t < RISE {
        Some(ease_out_back(t / RISE))
    } else if t < RISE + HOLD {
        Some(1.0)
    } else if t < DURATION {
        Some(1.0 - ease_in_cubic((t - RISE - HOLD) / HIDE))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_hidden_and_ends_hidden() {
        assert_eq!(reveal_at(0.0), Some(0.0));
        assert!(reveal_at(DURATION - 0.001).unwrap() < 0.01);
        assert_eq!(reveal_at(DURATION), None);
    }

    #[test]
    fn fully_out_while_holding() {
        assert_eq!(reveal_at(RISE + HOLD / 2.0), Some(1.0));
    }

    #[test]
    fn rise_overshoots_a_little() {
        let peak = (0..100)
            .map(|i| reveal_at(RISE * i as f32 / 100.0).unwrap())
            .fold(0.0f32, f32::max);
        assert!(peak > 1.0 && peak < 1.15, "{peak}");
    }

    #[test]
    fn easing_hits_end_points() {
        assert!((ease_out_back(0.0)).abs() < 1e-6);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(ease_in_cubic(1.0), 1.0);
    }
}
