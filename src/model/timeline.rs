use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

use super::FrameCount;
use super::{AudioSpec, VideoSpec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrameSampleStep {
    whole: u64,
    remainder: u32,
    denominator: u32,
}

impl FrameSampleStep {
    #[must_use]
    pub(crate) const fn whole(self) -> u64 {
        self.whole
    }

    #[must_use]
    pub(crate) const fn remainder(self) -> u32 {
        self.remainder
    }

    #[must_use]
    pub(crate) const fn denominator(self) -> u32 {
        self.denominator
    }

    #[must_use]
    pub(crate) const fn covering_samples(self) -> Option<u64> {
        if self.remainder == 0 {
            Some(self.whole)
        } else {
            self.whole.checked_add(1)
        }
    }

    #[must_use]
    pub(crate) const fn is_integral(self) -> bool {
        self.remainder == 0
    }
}

/// Exact mapping between the project frame grid and audio sample grid.
///
/// Video and Audio retain their native integer units. This mapper owns the
/// rational boundary policy used whenever an operation crosses between them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TimelineRate {
    video: VideoSpec,
    audio: AudioSpec,
}

impl TimelineRate {
    #[must_use]
    pub(crate) const fn new(video: VideoSpec, audio: AudioSpec) -> Self {
        Self { video, audio }
    }

    pub(crate) fn frame_sample_step(
        self,
        frames: FrameCount,
        span: &SourceSpan,
    ) -> Result<FrameSampleStep> {
        let numerator = u128::from(frames.0)
            .checked_mul(u128::from(self.audio.sample_rate()))
            .and_then(|value| value.checked_mul(u128::from(self.video.fps().denominator())))
            .ok_or_else(|| audio_overflow(span))?;
        let denominator = u128::from(self.video.fps().numerator());
        let whole = numerator / denominator;
        let remainder = numerator % denominator;
        let divisor = gcd(remainder, denominator);
        Ok(FrameSampleStep {
            whole: u64::try_from(whole).map_err(|_| audio_overflow(span))?,
            remainder: u32::try_from(remainder / divisor).map_err(|_| audio_overflow(span))?,
            denominator: u32::try_from(denominator / divisor).map_err(|_| audio_overflow(span))?,
        })
    }

    /// Return the first audio sample boundary that covers the supplied frame
    /// boundary.
    pub(crate) fn sample_boundary(self, frame: u64, span: &SourceSpan) -> Result<u64> {
        let numerator = u128::from(frame)
            .checked_mul(u128::from(self.audio.sample_rate()))
            .and_then(|value| value.checked_mul(u128::from(self.video.fps().denominator())))
            .ok_or_else(|| audio_overflow(span))?;
        let denominator = u128::from(self.video.fps().numerator());
        checked_ceil_div(numerator, denominator, span)
    }

    /// Return the exact number of samples assigned between two cumulative
    /// frame boundaries.
    ///
    /// Mapping both absolute boundaries before subtracting makes adjacent
    /// segments telescope to the sample count of their combined frame range.
    pub(crate) fn samples_between_frames(
        self,
        start: u64,
        end: u64,
        span: &SourceSpan,
    ) -> Result<u64> {
        if start > end {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidTimeRange,
                "frame boundary start must not follow its end",
                span.clone(),
            ));
        }
        let start = self.sample_boundary(start, span)?;
        let end = self.sample_boundary(end, span)?;
        Ok(end - start)
    }

    /// Return the sample count covering a duration that starts at frame zero.
    pub(crate) fn samples_for_frames(self, frames: FrameCount, span: &SourceSpan) -> Result<u64> {
        self.sample_boundary(frames.0, span)
    }

    /// Map a signed project-frame displacement onto the sample grid.
    ///
    /// The magnitude uses the same covering boundary policy as positive frame
    /// coordinates. Applying the sign afterwards keeps opposite frame offsets
    /// symmetric and lets normalized expressions cancel before conversion.
    pub(crate) fn signed_sample_displacement(self, frames: i64, span: &SourceSpan) -> Result<i128> {
        let magnitude = self.sample_boundary(frames.unsigned_abs(), span)?;
        let magnitude = i128::from(magnitude);
        Ok(if frames < 0 { -magnitude } else { magnitude })
    }

    /// Return the smallest frame count whose boundary covers every supplied
    /// sample.
    pub(crate) fn frames_for_samples(self, samples: u64, span: &SourceSpan) -> Result<FrameCount> {
        let numerator = u128::from(samples)
            .checked_mul(u128::from(self.video.fps().numerator()))
            .ok_or_else(|| frame_overflow(span))?;
        let denominator = u128::from(self.audio.sample_rate())
            .checked_mul(u128::from(self.video.fps().denominator()))
            .ok_or_else(|| frame_overflow(span))?;
        let frames = ceil_div(numerator, denominator).ok_or_else(|| frame_overflow(span))?;
        Ok(FrameCount(
            u64::try_from(frames).map_err(|_| frame_overflow(span))?,
        ))
    }
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn checked_ceil_div(numerator: u128, denominator: u128, span: &SourceSpan) -> Result<u64> {
    let value = ceil_div(numerator, denominator).ok_or_else(|| audio_overflow(span))?;
    u64::try_from(value).map_err(|_| audio_overflow(span))
}

fn ceil_div(numerator: u128, denominator: u128) -> Option<u128> {
    let quotient = numerator / denominator;
    if numerator.is_multiple_of(denominator) {
        Some(quotient)
    } else {
        quotient.checked_add(1)
    }
}

fn audio_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::AudioDurationOverflow,
        "audio duration exceeds the supported range",
        span.clone(),
    )
}

fn frame_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::FrameOverflow,
        "duration exceeds the supported frame count",
        span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FrameRate;

    fn span() -> SourceSpan {
        SourceSpan::file_start("timeline.clipasm")
    }

    fn ntsc_timeline() -> TimelineRate {
        TimelineRate::new(
            VideoSpec::new(
                1920,
                1080,
                FrameRate::new(30_000, 1_001).expect("frame rate"),
            )
            .expect("video spec"),
            AudioSpec::new(48_000, 2).expect("audio spec"),
        )
    }

    #[test]
    fn cumulative_boundaries_distribute_fractional_samples_without_drift() {
        let timeline = ntsc_timeline();
        let boundaries = (0..=5)
            .map(|frame| timeline.sample_boundary(frame, &span()).expect("boundary"))
            .collect::<Vec<_>>();
        assert_eq!(boundaries, [0, 1602, 3204, 4805, 6407, 8008]);

        let segments = (0..5)
            .map(|frame| {
                timeline
                    .samples_between_frames(frame, frame + 1, &span())
                    .expect("segment")
            })
            .collect::<Vec<_>>();
        assert_eq!(segments, [1602, 1602, 1601, 1602, 1601]);
        assert_eq!(segments.iter().sum::<u64>(), boundaries[5]);

        let step = timeline
            .frame_sample_step(FrameCount(1), &span())
            .expect("sample step");
        assert_eq!(step.whole(), 1601);
        assert_eq!(step.remainder(), 3);
        assert_eq!(step.denominator(), 5);
        assert_eq!(step.covering_samples(), Some(1602));
    }

    #[test]
    fn frame_and_sample_covering_conversions_share_one_policy() {
        let timeline = ntsc_timeline();
        assert_eq!(
            timeline
                .samples_for_frames(FrameCount(5), &span())
                .expect("samples"),
            8008
        );
        assert_eq!(
            timeline.frames_for_samples(8008, &span()).expect("frames"),
            FrameCount(5)
        );
        assert_eq!(
            timeline
                .frames_for_samples(8009, &span())
                .expect("covering frames"),
            FrameCount(6)
        );
    }
}
