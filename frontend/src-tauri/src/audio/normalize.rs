//! Audio peak normalization for ASR preprocessing.
//!
//! Normalizes each speech segment to a consistent peak level before sending
//! it to the STT model. ASR models (Parakeet, Whisper) produce noticeably
//! more accurate transcripts when input amplitude is consistent across
//! segments — avoids per-segment volume drift from affecting confidence.

/// Target peak amplitude, equivalent to -3 dBFS. Leaves 3 dB of headroom
/// so that downstream clipping / limiting never triggers.
const TARGET_PEAK: f32 = 0.707;

/// Silence threshold below which we do NOT normalize (avoid amplifying noise).
/// -40 dBFS ≈ 0.01 — anything quieter is treated as silence and left alone.
const SILENCE_FLOOR: f32 = 0.01;

/// Peak-normalize a mono audio buffer in-place.
///
/// - Measures the absolute peak of the signal.
/// - If the peak is above `SILENCE_FLOOR`, scales all samples so the new peak
///   equals `TARGET_PEAK` (-3 dBFS).
/// - If the peak is at or below `SILENCE_FLOOR`, leaves the buffer unchanged
///   to avoid amplifying background noise in silent segments.
///
/// Returns the gain factor applied (1.0 if unchanged).
pub fn peak_normalize(samples: &mut [f32]) -> f32 {
    let peak = samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));

    if peak <= SILENCE_FLOOR {
        return 1.0;
    }

    let gain = TARGET_PEAK / peak;
    for s in samples.iter_mut() {
        *s *= gain;
    }
    gain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_quiet_signal_to_target_peak() {
        let mut samples = vec![0.1, -0.2, 0.15, -0.05];
        let gain = peak_normalize(&mut samples);
        assert!((gain - (TARGET_PEAK / 0.2)).abs() < 1e-5);
        let new_peak = samples.iter().fold(0.0f32, |a, &s| a.max(s.abs()));
        assert!((new_peak - TARGET_PEAK).abs() < 1e-5);
    }

    #[test]
    fn attenuates_loud_signal_to_target_peak() {
        let mut samples = vec![0.9, -0.95, 0.8];
        peak_normalize(&mut samples);
        let new_peak = samples.iter().fold(0.0f32, |a, &s| a.max(s.abs()));
        assert!((new_peak - TARGET_PEAK).abs() < 1e-5);
    }

    #[test]
    fn leaves_silence_unchanged() {
        let mut samples = vec![0.001, -0.002, 0.003, 0.0];
        let original = samples.clone();
        let gain = peak_normalize(&mut samples);
        assert_eq!(gain, 1.0);
        assert_eq!(samples, original);
    }

    #[test]
    fn handles_empty_buffer() {
        let mut samples: Vec<f32> = vec![];
        let gain = peak_normalize(&mut samples);
        assert_eq!(gain, 1.0);
    }
}
