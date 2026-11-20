//! Pure stance and swing trajectory shared by grounded feet.
use super::ease;
use std::f32::consts::PI;

/// A foot stays in support for most of a step, then clears the ground on
/// a shorter return arc. Its backwards speed agrees on both sides of contact.
pub(super) fn beast_step(phase: f32, stance: f32, span: f32, clearance: f32) -> (f32, f32) {
    let phase = phase.rem_euclid(1.0);
    if phase < stance {
        (span * (0.5 - phase / stance), 0.0)
    } else {
        let t = (phase - stance) / (1.0 - stance);
        let carry = -span * (1.0 - stance) / stance * t * (1.0 - t) * (1.0 - 2.0 * t);
        (
            span * (ease(t) - 0.5) + carry,
            clearance * (PI * t).sin().powi(2),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stance_stays_grounded_and_moves_backward_linearly() {
        for phase in [0.0, 0.125, 0.25, 0.375] {
            let (along, lift) = beast_step(phase, 0.5, 2.0, 0.125);
            assert_eq!(along, 1.0 - 4.0 * phase);
            assert_eq!(lift, 0.0);
        }
    }

    #[test]
    fn swing_reaches_the_authored_peak_then_returns_to_contact() {
        assert_eq!(beast_step(0.5, 0.5, 2.0, 0.125), (-1.0, 0.0));
        assert_eq!(beast_step(0.75, 0.5, 2.0, 0.125), (0.0, 0.125));
        assert_eq!(beast_step(1.0, 0.5, 2.0, 0.125), (1.0, 0.0));
    }

    #[test]
    fn exact_binary_phases_wrap_in_both_directions() {
        for phase in [0.0, 0.125, 0.5, 0.75, 0.875] {
            let expected = beast_step(phase, 0.625, 0.75, 0.125);
            for periods in [-4.0, -1.0, 1.0, 4.0] {
                assert_eq!(beast_step(phase + periods, 0.625, 0.75, 0.125), expected);
            }
        }
    }

    #[test]
    fn authored_stance_boundaries_keep_contact_continuous() {
        for stance in [
            0.52f32, 0.53, 0.55, 0.57, 0.6, 0.62, 0.63, 0.64, 0.65, 0.67, 0.69, 0.73,
        ] {
            let span = 0.75;
            let at = beast_step(stance, stance, span, 0.1);
            assert_eq!(at, (-span * 0.5, 0.0));
            for phase in [
                f32::from_bits(stance.to_bits() - 1),
                f32::from_bits(stance.to_bits() + 1),
            ] {
                let (along, lift) = beast_step(phase, stance, span, 0.1);
                assert!((along - at.0).abs() < 1e-5);
                assert!(lift.abs() < 1e-5);
            }
            let before_wrap = beast_step(f32::from_bits(1.0f32.to_bits() - 1), stance, span, 0.1);
            assert!((before_wrap.0 - span * 0.5).abs() < 1e-5);
            assert!(before_wrap.1.abs() < 1e-5);
        }
    }

    #[test]
    fn swing_lift_is_nonnegative_and_bounded_by_authored_clearance() {
        for i in 0..=256 {
            let (_, lift) = beast_step(i as f32 / 256.0, 0.625, 0.75, 0.125);
            assert!((0.0..=0.125).contains(&lift));
        }
    }
}
