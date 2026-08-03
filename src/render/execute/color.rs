use crate::model::ColorSpec;
use crate::preflight::{PreparedSourceColor, VideoEncoding};

use super::super::color::{matrix, metadata, primaries, transfer, zscale_range};

const SDR_NOMINAL_PEAK_NITS: u16 = 100;
const LINEAR_PIXEL_FORMAT: &str = "gbrpf32le";

pub(super) fn source_to_linear_rgb(source: &PreparedSourceColor) -> String {
    to_linear_rgb(source.color(), source.chroma_location())
}

pub(super) fn to_linear_rgb(
    source: ColorSpec,
    chroma_location: Option<crate::preflight::ChromaLocation>,
) -> String {
    let destination = ColorSpec::LINEAR_RGB;
    let chroma = chroma_location
        .map(|location| format!(":chromalin={}", location.ffmpeg_name()))
        .unwrap_or_default();
    format!(
        "zscale=matrixin={}:transferin={}:primariesin={}:rangein={}{}:matrix={}:transfer={}:primaries={}:range={}:npl={SDR_NOMINAL_PEAK_NITS}:agamma=0,format={LINEAR_PIXEL_FORMAT}",
        matrix(source.matrix()),
        transfer(source.transfer()),
        primaries(source.primaries()),
        zscale_range(source.range()),
        chroma,
        matrix(destination.matrix()),
        transfer(destination.transfer()),
        primaries(destination.primaries()),
        zscale_range(destination.range()),
    )
}

pub(super) fn working_to_linear_rgb() -> String {
    to_linear_rgb(ColorSpec::SDR_BT709, None)
}

pub(super) fn linear_rgb_to_encoding(encoding: VideoEncoding) -> String {
    convert_encoding(ColorSpec::LINEAR_RGB, encoding)
}

pub(super) fn working_to_encoding(encoding: VideoEncoding) -> String {
    convert_encoding(ColorSpec::SDR_BT709, encoding)
}

fn convert_encoding(source: ColorSpec, encoding: VideoEncoding) -> String {
    let destination = encoding.color();
    let chroma = encoding
        .chroma_location()
        .map(|location| format!(":chromal={}", location.ffmpeg_name()))
        .unwrap_or_default();
    format!(
        "zscale=matrixin={}:transferin={}:primariesin={}:rangein={}:matrix={}:transfer={}:primaries={}:range={}{chroma}:npl={SDR_NOMINAL_PEAK_NITS}:agamma=0:dither=error_diffusion,format={},{}",
        matrix(source.matrix()),
        transfer(source.transfer()),
        primaries(source.primaries()),
        zscale_range(source.range()),
        matrix(destination.matrix()),
        transfer(destination.transfer()),
        primaries(destination.primaries()),
        zscale_range(destination.range()),
        encoding.pixel_format(),
        tag_color(destination),
    )
}

fn tag_color(color: ColorSpec) -> String {
    let names = metadata(color);
    format!(
        "setparams=range={}:color_primaries={}:color_trc={}:colorspace={}",
        zscale_range(color.range()),
        names.primaries,
        names.transfer,
        names.matrix,
    )
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
