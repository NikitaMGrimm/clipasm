#!/bin/sh
set -eu

mkdir -p "$(dirname "$0")/generated"

ffmpeg -v error -y \
  -f lavfi -i "testsrc2=size=320x180:rate=30:duration=2" \
  -an -c:v libx264 -pix_fmt yuv420p \
  "$(dirname "$0")/generated/sample.mp4"

printf '%s\n' "generated examples/generated/sample.mp4"
