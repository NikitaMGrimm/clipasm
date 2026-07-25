use std::num::{NonZeroU8, NonZeroU32};

use serde::{Serialize, Serializer};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

use super::{FrameCount, FrameRate};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
/// Canonical project audio properties used by compilation and rendering.
///
/// Sample rate and channel count are always greater than zero.
pub struct AudioSpec {
    sample_rate: NonZeroU32,
    channels: NonZeroU8,
}

impl AudioSpec {
    /// Construct a valid project audio format.
    ///
    /// Returns `None` when the sample rate or channel count is zero.
    #[must_use]
    pub const fn new(sample_rate: u32, channels: u8) -> Option<Self> {
        let Some(sample_rate) = NonZeroU32::new(sample_rate) else {
            return None;
        };
        let Some(channels) = NonZeroU8::new(channels) else {
            return None;
        };
        Some(Self {
            sample_rate,
            channels,
        })
    }

    /// Return the number of audio samples per second.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.sample_rate.get()
    }

    /// Return the number of interleaved output channels.
    #[must_use]
    pub const fn channels(self) -> u8 {
        self.channels.get()
    }

    pub(crate) fn samples_for_frames(
        self,
        frames: FrameCount,
        frame_rate: FrameRate,
        span: &SourceSpan,
    ) -> Result<u64> {
        let numerator = u128::from(frames.0)
            .checked_mul(u128::from(self.sample_rate()))
            .and_then(|value| value.checked_mul(u128::from(frame_rate.denominator())))
            .ok_or_else(|| arithmetic_error(span))?;
        let denominator = u128::from(frame_rate.numerator());
        let samples = numerator
            .checked_add(denominator - 1)
            .ok_or_else(|| arithmetic_error(span))?
            / denominator;
        u64::try_from(samples).map_err(|_| arithmetic_error(span))
    }

    pub(crate) fn frames_for_samples(
        self,
        samples: u64,
        frame_rate: FrameRate,
        span: &SourceSpan,
    ) -> Result<FrameCount> {
        let numerator = u128::from(samples)
            .checked_mul(u128::from(frame_rate.numerator()))
            .ok_or_else(|| arithmetic_error(span))?;
        let denominator = u128::from(self.sample_rate())
            .checked_mul(u128::from(frame_rate.denominator()))
            .ok_or_else(|| arithmetic_error(span))?;
        let frames = numerator
            .checked_add(denominator - 1)
            .ok_or_else(|| arithmetic_error(span))?
            / denominator;
        Ok(FrameCount(
            u64::try_from(frames).map_err(|_| arithmetic_error(span))?,
        ))
    }
}

impl Default for AudioSpec {
    fn default() -> Self {
        Self::new(48_000, 2).expect("default audio format is positive")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Exact duration and format of one standalone Audio value.
pub struct AudioDomain {
    samples: u64,
    spec: AudioSpec,
}

impl AudioDomain {
    /// Construct an exact Audio domain in the supplied format.
    #[must_use]
    pub const fn new(samples: u64, spec: AudioSpec) -> Self {
        Self { samples, spec }
    }

    /// Return the exact number of samples per channel.
    #[must_use]
    pub const fn samples(self) -> u64 {
        self.samples
    }

    /// Return the complete audio format.
    #[must_use]
    pub const fn audio_spec(self) -> AudioSpec {
        self.spec
    }

    /// Return the number of samples per second.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.spec.sample_rate()
    }

    /// Return the number of interleaved channels.
    #[must_use]
    pub const fn channels(self) -> u8 {
        self.spec.channels()
    }
}

impl Serialize for AudioDomain {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Document {
            samples: u64,
            sample_rate: u32,
            channels: u8,
        }

        Document {
            samples: self.samples,
            sample_rate: self.sample_rate(),
            channels: self.channels(),
        }
        .serialize(serializer)
    }
}

fn arithmetic_error(span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_AUDIO_DURATION_OVERFLOW",
        "audio duration exceeds the supported range",
        span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_specs_and_domains_protect_format_without_changing_json() {
        assert!(AudioSpec::new(0, 2).is_none());
        assert!(AudioSpec::new(48_000, 0).is_none());

        let spec = AudioSpec::new(48_000, 2).expect("audio spec");
        assert_eq!(spec.sample_rate(), 48_000);
        assert_eq!(spec.channels(), 2);

        let domain = AudioDomain::new(96_000, spec);
        assert_eq!(domain.samples(), 96_000);
        assert_eq!(domain.audio_spec(), spec);
        assert_eq!(
            serde_json::to_value(domain).expect("domain JSON"),
            serde_json::json!({
                "samples": 96000,
                "sample_rate": 48000,
                "channels": 2,
            })
        );
    }
}
