//! Slider positions and the scalar Windows stores (decision 0002).
//!
//! A session's volume is a linear amplitude scalar. Loudness follows roughly the 0.6 power of
//! amplitude (Stevens' law), so a slider that moved the scalar linearly would do almost nothing
//! over its top half. Positions map through that power instead: at 50 % the scalar is about
//! 0.315 (-10 dB), which sounds half as loud. Both ends are exact, so 0 is silence and 100 %
//! is the app's full volume.
//!
//! Device volumes need none of this: Windows already tapers the endpoint scalar.

const EXPONENT: f32 = 1.0 / 0.6;

/// The session scalar for a slider position, both from 0.0 to 1.0.
pub fn scalar_from_position(position: f32) -> f32 {
    let position = clamp(position);
    if position >= 1.0 {
        return 1.0;
    }
    position.powf(EXPONENT)
}

/// The slider position showing a session scalar, both from 0.0 to 1.0.
pub fn position_from_scalar(scalar: f32) -> f32 {
    let scalar = clamp(scalar);
    if scalar >= 1.0 {
        return 1.0;
    }
    scalar.powf(1.0 / EXPONENT)
}

/// Into 0.0..=1.0; NaN is silence.
fn clamp(value: f32) -> f32 {
    if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_at_both_ends() {
        assert_eq!(scalar_from_position(0.0), 0.0);
        assert_eq!(scalar_from_position(1.0), 1.0);
        assert_eq!(position_from_scalar(0.0), 0.0);
        assert_eq!(position_from_scalar(1.0), 1.0);
    }

    #[test]
    fn out_of_range_is_clamped() {
        assert_eq!(scalar_from_position(-0.5), 0.0);
        assert_eq!(scalar_from_position(1.5), 1.0);
        assert_eq!(scalar_from_position(f32::NAN), 0.0);
        assert_eq!(position_from_scalar(2.0), 1.0);
    }

    #[test]
    fn monotonic() {
        let mut last = -1.0;
        for i in 0..=1000 {
            let scalar = scalar_from_position(i as f32 / 1000.0);
            assert!(scalar > last, "not increasing at {i}");
            last = scalar;
        }
    }

    #[test]
    fn half_way_sounds_half_as_loud() {
        let db = 20.0 * scalar_from_position(0.5).log10();
        assert!((db + 10.0).abs() < 0.1, "{db} dB");
    }

    #[test]
    fn round_trips() {
        for i in 0..=1000 {
            let position = i as f32 / 1000.0;
            let back = position_from_scalar(scalar_from_position(position));
            assert!((back - position).abs() < 1e-5, "{position} came back as {back}");
            let scalar = position;
            let back = scalar_from_position(position_from_scalar(scalar));
            assert!((back - scalar).abs() < 1e-5, "{scalar} came back as {back}");
        }
    }
}
