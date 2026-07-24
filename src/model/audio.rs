use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

use super::{FrameCount, FrameRate};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
/// Canonical project audio properties used by compilation and rendering.
pub struct AudioSpec {
    /// Audio samples per second.
    pub sample_rate: u32,
    /// Number of interleaved output channels.
    pub channels: u8,
}

impl Default for AudioSpec {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
        }
    }
}

impl AudioSpec {
    pub(crate) fn samples_for_frames(
        self,
        frames: FrameCount,
        frame_rate: FrameRate,
        span: &SourceSpan,
    ) -> Result<u64> {
        let numerator = u128::from(frames.0)
            .checked_mul(u128::from(self.sample_rate))
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
        let denominator = u128::from(self.sample_rate)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
/// Exact duration and format of one standalone Audio value.
pub struct AudioDomain {
    /// Exact number of output samples per channel.
    pub samples: u64,
    /// Audio samples per second.
    pub sample_rate: u32,
    /// Number of interleaved channels.
    pub channels: u8,
}

fn arithmetic_error(span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_AUDIO_DURATION_OVERFLOW",
        "audio duration exceeds the supported range",
        span.clone(),
    )
}
