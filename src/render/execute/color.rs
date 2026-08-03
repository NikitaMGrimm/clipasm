use crate::model::{
    ColorPrimaries, ColorRange, ColorSpec, MatrixCoefficients, TransferCharacteristic,
};
use crate::preflight::{PreparedSourceColor, VideoEncoding};

const SDR_NOMINAL_PEAK_NITS: u16 = 100;
const LINEAR_PIXEL_FORMAT: &str = "gbrpf32le";

pub(super) fn source_to_linear_rgb(source: &PreparedSourceColor) -> String {
    to_linear_rgb(source.color(), source.chroma_location())
}

pub(super) fn to_linear_rgb(source: ColorSpec, chroma_location: Option<&str>) -> String {
    let chroma = chroma_location
        .map(|location| format!(":chromalin={location}"))
        .unwrap_or_default();
    format!(
        "zscale=matrixin={}:transferin={}:primariesin={}:rangein={}{}:matrix=gbr:transfer=linear:primaries=bt709:range=full:npl={SDR_NOMINAL_PEAK_NITS}:agamma=0,format={LINEAR_PIXEL_FORMAT}",
        matrix_name(source.matrix()),
        transfer_name(source.transfer()),
        primaries_name(source.primaries()),
        range_name(source.range()),
        chroma,
    )
}

pub(super) fn working_to_linear_rgb() -> String {
    to_linear_rgb(ColorSpec::SDR_BT709, None)
}

pub(super) fn linear_rgb_to_encoding(encoding: VideoEncoding) -> String {
    debug_assert_eq!(encoding.color(), ColorSpec::SDR_BT709);
    let chroma = encoding
        .chroma_location()
        .map(|location| format!(":chromal={location}"))
        .unwrap_or_default();
    format!(
        "zscale=matrixin=gbr:transferin=linear:primariesin=bt709:rangein=full:matrix=bt709:transfer=bt709:primaries=bt709:range=limited{chroma}:npl={SDR_NOMINAL_PEAK_NITS}:agamma=0:dither=error_diffusion,format={},{}",
        encoding.pixel_format(),
        tag_sdr_bt709(),
    )
}

pub(super) fn working_to_encoding(encoding: VideoEncoding) -> String {
    debug_assert_eq!(encoding.color(), ColorSpec::SDR_BT709);
    let chroma = encoding
        .chroma_location()
        .map(|location| format!(":chromal={location}"))
        .unwrap_or_default();
    format!(
        "zscale=matrixin=bt709:transferin=bt709:primariesin=bt709:rangein=limited:matrix=bt709:transfer=bt709:primaries=bt709:range=limited{chroma}:npl={SDR_NOMINAL_PEAK_NITS}:agamma=0:dither=error_diffusion,format={},{}",
        encoding.pixel_format(),
        tag_sdr_bt709(),
    )
}

pub(super) const fn tag_sdr_bt709() -> &'static str {
    "setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709"
}

fn primaries_name(primaries: ColorPrimaries) -> &'static str {
    match primaries {
        ColorPrimaries::Bt709 => "bt709",
    }
}

fn transfer_name(transfer: TransferCharacteristic) -> &'static str {
    match transfer {
        TransferCharacteristic::Bt709 => "bt709",
        TransferCharacteristic::Srgb => "iec61966-2-1",
        TransferCharacteristic::Linear => "linear",
    }
}

fn matrix_name(matrix: MatrixCoefficients) -> &'static str {
    match matrix {
        MatrixCoefficients::Bt709 => "bt709",
        MatrixCoefficients::Bt601 => "smpte170m",
        MatrixCoefficients::Rgb => "gbr",
    }
}

fn range_name(range: ColorRange) -> &'static str {
    match range {
        ColorRange::Limited => "limited",
        ColorRange::Full => "full",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_name_every_color_side_explicitly() {
        let ingress = to_linear_rgb(ColorSpec::SRGB_RGB, None);
        assert!(ingress.contains("matrixin=gbr"));
        assert!(ingress.contains("transferin=iec61966-2-1"));
        assert!(ingress.contains("rangein=full"));
        assert!(ingress.contains("transfer=linear"));
        assert!(ingress.contains("agamma=0"));
        assert!(ingress.contains("npl=100"));

        let working = linear_rgb_to_encoding(
            crate::preflight::RenderPolicy::CURRENT.working_video_encoding(),
        );
        assert!(working.contains("matrixin=gbr"));
        assert!(working.contains("format=yuv444p10le"));
        assert!(working.contains("setparams=range=limited"));
    }
}
