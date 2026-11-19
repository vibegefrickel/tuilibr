#!/bin/sh
# Rebuilds the PNG fixtures of src/decode/raster.rs: one small picture in each PNG layout.
#
# Needs ImageMagick 7 (magick) and exiftool. Run it from any directory:
# ./tests/fixtures/png/make.sh
#
# Each picture is 32x16 pixels. The color files hold red (255,0,0) in the left half and blue
# (0,0,255) in the right half. The gray files hold black in the left half and gray 200 in the
# right half. In the files with alpha, the right half is fully transparent (alpha 0).
#   rgb16.png        16-bit RGB, as the HDR screenshots of an iPhone
#   rgba16.png       16-bit RGBA
#   rgba8.png        8-bit RGBA
#   palette.png      indexed colors (color type 3)
#   palette-trns.png indexed colors with a tRNS chunk for the transparent half
#   gray8.png        8-bit grayscale
#   gray16.png       16-bit grayscale
#   gray-alpha.png   8-bit grayscale with alpha
#   interlaced.png   8-bit RGB, Adam7 interlaced
#   orientation-6.png 8-bit RGB with an eXIf chunk, Orientation 6 (turn 90 degrees clockwise)
set -eu

cd "$(dirname "$0")"

color='-size 16x16 xc:#ff0000 -size 16x16 xc:#0000ff +append'
gray='-size 16x16 xc:#000000 -size 16x16 xc:#c8c8c8 +append'
color_clear='-size 16x16 xc:#ff0000ff -size 16x16 xc:#0000ff00 +append'
gray_clear='-size 16x16 xc:#000000ff -size 16x16 xc:#c8c8c800 +append'

# shellcheck disable=SC2086
{
  magick $color -depth 16 -strip PNG48:rgb16.png
  magick $color_clear -depth 16 -strip PNG64:rgba16.png
  magick $color_clear -depth 8 -strip PNG32:rgba8.png
  magick $color -strip PNG8:palette.png
  magick $color_clear -strip PNG8:palette-trns.png
  magick $gray -colorspace Gray -depth 8 -strip -define png:color-type=0 PNG:gray8.png
  magick $gray -colorspace Gray -depth 16 -strip -define png:color-type=0 -define png:bit-depth=16 PNG:gray16.png
  magick $gray_clear -colorspace Gray -depth 8 -strip -define png:color-type=4 PNG:gray-alpha.png
  magick $color -depth 8 -interlace PNG -strip PNG24:interlaced.png
  magick $color -depth 8 -strip PNG24:orientation-6.png
}
exiftool -q -overwrite_original -Orientation#=6 orientation-6.png
