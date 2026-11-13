//! A pin's zoom: a lattice of scales, so a picture can come back to where it started.
//!
//! The wheel adds whole steps to an integer and the scale is a function of that integer, so
//! native size and every power of two are states rather than accumulations: any path of
//! steps that ends on the same count draws at the same scale, to the bit.

/// Steps per octave: 2× is a multiple of this, so every power of two is a step on the
/// lattice. 32 keeps one trackpad step under 2.2 % of the picture.
pub(crate) const STEPS_PER_OCTAVE: i32 = 32;

/// Steps one wheel notch takes: a little over 9 %, the size the wheel has always moved in.
pub(crate) const STEPS_PER_NOTCH: i32 = 4;

/// What one wheel notch is worth in the engine's pixels.
const WHEEL_NOTCH: f32 = 40.0;

/// The drawn size, in logical points, a pin's long side stays between.
const SMALLEST_DRAWN: f32 = 32.0;
const LARGEST_DRAWN: f32 = 16384.0;

/// The envelope no content asks past, as multiples of its native size.
const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 32.0;

/// The scale at `steps`, as `2^(steps / STEPS_PER_OCTAVE)`.
///
/// The octaves are built from a float's exponent, so native size and every 2× are exact
/// however the count was reached; only the fraction inside an octave goes through `exp2`.
pub(crate) fn scale(steps: i32) -> f32 {
    let octave = steps.div_euclid(STEPS_PER_OCTAVE);
    let rest = steps.rem_euclid(STEPS_PER_OCTAVE);
    let power = f32::from_bits(((octave + 127) as u32) << 23);
    power * ((rest as f32 / STEPS_PER_OCTAVE as f32).exp2())
}

/// The factor between two lattice points: one exponential, not a quotient of two.
pub(crate) fn ratio(from: i32, to: i32) -> f32 {
    scale(to - from)
}

/// The steps a wheel delta of `value` engine pixels is worth, fractional: a trackpad's
/// deltas are, and the pin banks the remainder until it is worth a whole step.
pub(crate) fn travel(value: f64) -> f32 {
    value as f32 / WHEEL_NOTCH * STEPS_PER_NOTCH as f32
}

/// The whole notches `value` is worth, fractional. A caller with its own unit per notch
/// (the loupe counts zoom levels, not lattice steps) banks the remainder the same way.
pub(crate) fn notches(value: f64) -> f32 {
    value as f32 / WHEEL_NOTCH
}

/// The steps a content of `size` logical points may zoom between: its long side is drawn
/// between 32 and 16384 points, and inside the shared envelope. Both ends are lattice
/// points, so a pin pinned at a limit is pinned at a scale another path can return to.
pub(crate) fn range(size: (f32, f32)) -> (i32, i32) {
    let (floor, ceiling) = (steps_at_least(MIN_ZOOM), steps_at_most(MAX_ZOOM));
    let longest = size.0.max(size.1);
    if longest <= f32::EPSILON {
        return (floor, ceiling);
    }
    let lowest = steps_at_least(SMALLEST_DRAWN / longest).clamp(floor, ceiling);
    let highest = steps_at_most(LARGEST_DRAWN / longest).clamp(floor, ceiling);
    // A content too small or too large for the range to hold both ends keeps the one scale
    // the envelope leaves it, rather than an inverted pair no clamp can satisfy.
    (lowest.min(highest), highest)
}

/// The fewest steps that draw at least `scale`.
fn steps_at_least(scale: f32) -> i32 {
    (scale.log2() * STEPS_PER_OCTAVE as f32).ceil() as i32
}

/// The most steps that draw at most `scale`.
fn steps_at_most(scale: f32) -> i32 {
    (scale.log2() * STEPS_PER_OCTAVE as f32).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native size and every power of two are exact states of the count, and a count that
    /// comes back is the same picture however far the wheel wandered in between.
    #[test]
    fn a_count_draws_at_one_scale_however_it_was_reached() {
        assert_eq!(scale(0), 1.0, "native size is exact, not 0.99999994");
        for octave in -4..=5 {
            let steps = octave * STEPS_PER_OCTAVE;
            assert_eq!(
                scale(steps),
                2f32.powi(octave),
                "{steps} steps must be exactly 2^{octave}"
            );
        }
        // A wheel that racks up as many notches up as down is back at native, whatever the
        // order: nine up and nine down is the requirement, not 1.0000001.
        let mut steps = 0;
        for notch in [
            1, 1, 1, -1, 1, 1, -1, -1, 1, 1, 1, -1, -1, -1, 1, -1, -1, -1,
        ] {
            steps += notch * STEPS_PER_NOTCH;
        }
        assert_eq!(steps, 0);
        assert_eq!(scale(steps), 1.0);
    }

    /// A notch is four steps and a trackpad's pixels are fractional steps; both ends of a
    /// content's range are lattice points that draw inside the envelope.
    #[test]
    fn the_wheel_and_the_range_land_on_the_lattice() {
        assert_eq!(travel(40.0), 4.0, "one notch");
        assert_eq!(travel(-40.0), -4.0);
        assert_eq!(travel(10.0), 1.0, "a trackpad's quarter notch");

        let (lowest, highest) = range((3840.0, 2160.0));
        assert!(
            scale(highest) > 1.0,
            "a picture wider than the output must grow"
        );
        assert!(scale(lowest) <= 1.0);
        assert!(scale(highest) * 3840.0 <= LARGEST_DRAWN);

        let (lowest, highest) = range((2.0, 2.0));
        assert_eq!(
            scale(lowest),
            16.0,
            "a dot starts at sixteen times its size"
        );
        assert_eq!(scale(highest), MAX_ZOOM, "and the ceiling still caps it");

        // A content the envelope cannot hold: one scale, inside the envelope, on the grid.
        for size in [(0.0, 0.0), (0.1, 0.1), (1e6, 5.0)] {
            let (lowest, highest) = range(size);
            assert!(lowest <= highest, "{size:?} → ({lowest}, {highest})");
            let (lowest, highest) = (scale(lowest), scale(highest));
            assert!(
                lowest >= MIN_ZOOM && highest <= MAX_ZOOM,
                "{size:?} → ({lowest}, {highest})"
            );
        }
    }
}
