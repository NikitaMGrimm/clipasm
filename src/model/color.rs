use serde::Serialize;

/// Primaries that define the RGB chromaticities of a video signal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPrimaries {
    /// ITU-R BT.709 / sRGB primaries.
    Bt709,
}

/// Transfer characteristic that relates encoded samples to light.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferCharacteristic {
    /// ITU-R BT.709 transfer characteristic.
    Bt709,
    /// IEC 61966-2-1 sRGB transfer characteristic.
    Srgb,
    /// Linear-light samples.
    Linear,
}

/// Matrix coefficients, or RGB identity, used by a video signal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixCoefficients {
    /// ITU-R BT.709 non-constant-luminance Y'`CbCr` coefficients.
    Bt709,
    /// BT.601-family non-constant-luminance Y'`CbCr` coefficients.
    Bt601,
    /// RGB samples without a Y'`CbCr` matrix.
    Rgb,
}

/// Numeric range occupied by encoded component samples.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorRange {
    /// Studio/legal range.
    Limited,
    /// Full component range.
    Full,
}

/// Complete semantic interpretation of video component values.
///
/// Construction is intentionally closed: authored projects select a coherent
/// profile instead of assembling independent fields that may contradict one
/// another.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct ColorSpec {
    primaries: ColorPrimaries,
    transfer: TransferCharacteristic,
    matrix: MatrixCoefficients,
    range: ColorRange,
}

impl ColorSpec {
    /// The foundation SDR project profile.
    pub const SDR_BT709: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferCharacteristic::Bt709,
        matrix: MatrixCoefficients::Bt709,
        range: ColorRange::Limited,
    };

    pub(crate) const SRGB_RGB: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferCharacteristic::Srgb,
        matrix: MatrixCoefficients::Rgb,
        range: ColorRange::Full,
    };

    pub(crate) const SRGB_BT601_FULL: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferCharacteristic::Srgb,
        matrix: MatrixCoefficients::Bt601,
        range: ColorRange::Full,
    };

    pub(crate) const BT709_FULL: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferCharacteristic::Bt709,
        matrix: MatrixCoefficients::Bt709,
        range: ColorRange::Full,
    };

    /// Resolve an authored, coherent color-profile name.
    #[must_use]
    pub fn from_profile_name(name: &str) -> Option<Self> {
        match name {
            "sdr_bt709" => Some(Self::SDR_BT709),
            _ => None,
        }
    }

    /// Return the authored profile name for this semantic color contract.
    #[must_use]
    pub fn profile_name(self) -> &'static str {
        if self == Self::SDR_BT709 {
            "sdr_bt709"
        } else {
            "internal"
        }
    }

    /// Return the RGB primaries.
    #[must_use]
    pub const fn primaries(self) -> ColorPrimaries {
        self.primaries
    }

    /// Return the transfer characteristic.
    #[must_use]
    pub const fn transfer(self) -> TransferCharacteristic {
        self.transfer
    }

    /// Return the matrix coefficients or RGB identity.
    #[must_use]
    pub const fn matrix(self) -> MatrixCoefficients {
        self.matrix
    }

    /// Return the component range.
    #[must_use]
    pub const fn range(self) -> ColorRange {
        self.range
    }
}
