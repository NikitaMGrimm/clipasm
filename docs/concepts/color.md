# Color and linear-light processing

ClipAsm currently has one project color profile:

```clipasm
config {
    video {
        color = sdr_bt709
    }
}
```

The setting defaults to `sdr_bt709`, but writing it makes the project intent
visible. It is one profile instead of four independent switches. Primaries,
transfer, matrix, and range describe different parts of a signal, but arbitrary
combinations are not necessarily meaningful or supported.

## The SDR BT.709 contract

Project Video uses BT.709 primaries, BT.709 transfer, BT.709 non-constant-
luminance Y'CbCr coefficients, and limited range. ClipAsm stores working Video
as 10-bit 4:4:4 FFV1 with signed-16-bit FLAC Audio. It publishes 8-bit 4:2:0 H.264 with left-positioned
chroma. Both forms carry explicit color metadata, and ClipAsm verifies it after
every render step.

Pixel format and color meaning are separate. `yuv420p` describes component
layout and depth. It does not by itself say whether samples are BT.709,
BT.2020, full range, limited range, SDR, or HDR.

## Source rules

Still images have an authored convention. Opaque untagged RGB images are sRGB.
JPEG Y'CbCr is interpreted as full-range BT.601 with centered chroma before it
is converted. ClipAsm currently rejects alpha and embedded ICC profiles because
correct support needs explicit compositing and ICC conversion policies.

Video files do not have a safe equivalent default. A `video(...)` source must
state BT.709 primaries, transfer, matrix, range, and chroma location when its
pixel format is subsampled. Missing metadata is rejected. ClipAsm does not
guess from frame size, codec, or file extension.

PQ (`smpte2084`), HLG (`arib-std-b67`), and HDR mastering metadata are rejected
under the SDR profile. Converting BT.2020 coordinates into BT.709 coordinates is
not tone mapping. A future HDR-to-SDR policy must define reference white,
nominal or mastering peak, target display, tone-mapping operator, and metadata
handling before it can produce predictable results.

## Display-linear pixel math

Encoded BT.709 samples are not proportional to displayed light. Averaging two
encoded code values therefore does not produce an optical midpoint. ClipAsm
converts picture data to full-range floating-point linear BT.709 RGB before:

- source fitting and resize interpolation;
- `zoom_in` perspective interpolation;
- the white fade in `flash_cut`;
- Video `crossfade` blending.

It then converts the result back to the canonical 10-bit working signal. Trim,
repeat, concat, and other routing-only operations preserve the canonical samples
without a conversion round trip.

“Linear” here is display-linear. For mastered BT.709 Video, zimg applies its
BT.1886-style display EOTF. This is not the inverse BT.709 camera OETF and is not
scene-linear radiance. ClipAsm fixes nominal peak luminance at 100 cd/m² and
disables zimg's approximate-gamma option so this behavior stays stable and
identity-bearing.

## Standards basis

- ITU-R BT.709 defines the HDTV primaries, signal transfer, and Y'CbCr
  coefficients.
- ITU-R BT.1886 defines the reference SDR display EOTF.
- ITU-R BT.2100 defines PQ and HLG HDR television systems.
- ITU-T H.273 defines independent code points for primaries, transfer,
  matrix coefficients, and range-related video signal metadata.

See [From source to published video](pipeline.md) for the surrounding pipeline
and [Files and configuration](../reference/language/files-and-configuration.md)
for authored settings.
