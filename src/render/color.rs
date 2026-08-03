use serde::Serialize;

use crate::model::{
    ColorPrimaries, ColorRange, ColorSpec, MatrixCoefficients, TransferCharacteristic,
};

/// `FFmpeg` metadata spellings that must survive encoding and `FFprobe` inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ColorMetadata {
    pub(super) primaries: &'static str,
    pub(super) transfer: &'static str,
    pub(super) matrix: &'static str,
    pub(super) range: &'static str,
}

pub(super) const fn metadata(color: ColorSpec) -> ColorMetadata {
    ColorMetadata {
        primaries: primaries(color.primaries()),
        transfer: transfer(color.transfer()),
        matrix: matrix(color.matrix()),
        range: ffmpeg_range(color.range()),
    }
}

pub(super) const fn primaries(value: ColorPrimaries) -> &'static str {
    match value {
        ColorPrimaries::Bt709 => "bt709",
    }
}

pub(super) const fn transfer(value: TransferCharacteristic) -> &'static str {
    match value {
        TransferCharacteristic::Bt709 => "bt709",
        TransferCharacteristic::Srgb => "iec61966-2-1",
        TransferCharacteristic::Linear => "linear",
    }
}

pub(super) const fn matrix(value: MatrixCoefficients) -> &'static str {
    match value {
        MatrixCoefficients::Bt709 => "bt709",
        MatrixCoefficients::Bt601 => "smpte170m",
        MatrixCoefficients::Rgb => "gbr",
    }
}

pub(super) const fn zscale_range(value: ColorRange) -> &'static str {
    match value {
        ColorRange::Limited => "limited",
        ColorRange::Full => "full",
    }
}

const fn ffmpeg_range(value: ColorRange) -> &'static str {
    match value {
        ColorRange::Limited => "tv",
        ColorRange::Full => "pc",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_uses_ffprobe_spellings() {
        assert_eq!(
            metadata(ColorSpec::SDR_BT709),
            ColorMetadata {
                primaries: "bt709",
                transfer: "bt709",
                matrix: "bt709",
                range: "tv",
            }
        );
        assert_eq!(metadata(ColorSpec::SRGB_RGB).transfer, "iec61966-2-1");
        assert_eq!(metadata(ColorSpec::SRGB_BT601_FULL).matrix, "smpte170m");
        assert_eq!(metadata(ColorSpec::LINEAR_RGB).matrix, "gbr");
        assert_eq!(metadata(ColorSpec::LINEAR_RGB).range, "pc");
    }
}
