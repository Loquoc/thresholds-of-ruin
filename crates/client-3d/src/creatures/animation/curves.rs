//! Shared scalar transition curves for creature poses, effects and geometry.
//! Keep evaluation order stable: this is the existing presentation timing policy,
//! not a new interpolation or validation layer.

pub(in super::super) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rise from 0 to 1 over [a, b], hold, then fall back over [c, d].
pub(super) fn envelope(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if t < a || t > d {
        0.0
    } else if t < b {
        ease((t - a) / (b - a))
    } else if t < c {
        1.0
    } else {
        1.0 - ease((t - c) / (d - c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_clamps_and_retains_exact_quarter_stations() {
        for (input, expected) in [
            (f32::NEG_INFINITY, 0.0),
            (-1.0, 0.0),
            (-0.0, 0.0),
            (0.0, 0.0),
            (0.25, 0.15625),
            (0.5, 0.5),
            (0.75, 0.84375),
            (1.0, 1.0),
            (2.0, 1.0),
            (f32::INFINITY, 1.0),
        ] {
            assert_eq!(ease(input).to_bits(), f32::to_bits(expected));
        }
    }

    #[test]
    fn ease_is_bounded_and_monotone_over_the_unit_interval() {
        let mut previous = 0.0;
        for i in 0..=4096 {
            let value = ease(i as f32 / 4096.0);
            assert!((previous..=1.0).contains(&value));
            previous = value;
        }
    }

    #[test]
    fn envelope_has_exact_endpoints_plateau_and_mid_ramps() {
        for (time, expected) in [
            (-1.0, 0.0),
            (0.0, 0.0),
            (0.125, 0.5),
            (0.25, 1.0),
            (0.5, 1.0),
            (0.75, 1.0),
            (0.875, 0.5),
            (1.0, 0.0),
            (2.0, 0.0),
        ] {
            assert_eq!(
                envelope(time, 0.0, 0.25, 0.75, 1.0).to_bits(),
                f32::to_bits(expected)
            );
        }
    }

    #[test]
    fn touching_ramps_keep_the_peak_without_a_hold() {
        for (time, expected) in [(0.0, 0.0), (0.25, 0.5), (0.5, 1.0), (0.75, 0.5), (1.0, 0.0)] {
            assert_eq!(envelope(time, 0.0, 0.5, 0.5, 1.0), expected);
        }
    }

    #[test]
    fn shifted_envelope_keeps_authored_time_coordinates() {
        assert_eq!(envelope(2.0, 2.0, 3.0, 4.0, 6.0), 0.0);
        assert_eq!(envelope(2.5, 2.0, 3.0, 4.0, 6.0), 0.5);
        assert_eq!(envelope(3.5, 2.0, 3.0, 4.0, 6.0), 1.0);
        assert_eq!(envelope(5.0, 2.0, 3.0, 4.0, 6.0), 0.5);
        assert_eq!(envelope(6.0, 2.0, 3.0, 4.0, 6.0), 0.0);
    }

    #[test]
    fn nonfinite_time_retains_existing_propagation() {
        assert!(ease(f32::NAN).is_nan());
        assert!(envelope(f32::NAN, 0.0, 0.25, 0.75, 1.0).is_nan());
        for t in [f32::NEG_INFINITY, f32::INFINITY] {
            assert_eq!(envelope(t, 0.0, 0.25, 0.75, 1.0), 0.0);
        }
    }
}
