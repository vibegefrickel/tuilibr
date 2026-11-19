#!/bin/sh
# Rebuilds the JPEG XL fixtures of src/decode/jxl.rs from one synthetic color chart.
#
# Needs cjxl and djxl 0.12 (Homebrew jpeg-xl), ImageMagick 7 (magick), exiftool and the
# ColorSync profiles of macOS. Run it from any directory: ./tests/fixtures/jxl/make.sh
#
# The chart is 64x32 pixels: 4 rows of 8 patches, 8x8 pixels each.
#   row 0: red, green, blue, cyan, magenta, yellow, white, black
#   row 1: a gray ramp from 0 to 255 in 8 steps
#   row 2: ColorChecker patches 1 to 8 (dark skin, light skin, blue sky, foliage, ...)
#   row 3: ColorChecker patches 9 to 16 (moderate red, purple, yellow green, ...)
# Most fixtures store the same code values in another color space. pq and hlg store the
# sRGB chart, converted by libjxl into a PQ or HLG signal.
#
# The reference of each fixture is `ref/<name>.png`: djxl renders it into 8-bit sRGB with the
# relative intent. pq, hlg, pq-icc, hlg-icc and gray-pq have no reference: phewr tone-maps them
# like the HDR cube of its terminal player (src/decode/hdr/), and tests/fixtures/hdr/ holds the
# references of that map.
set -eu

cd "$(dirname "$0")"
profiles=/System/Library/ColorSync/Profiles
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

awk 'BEGIN {
  rows[0] = "255,0,0 0,255,0 0,0,255 0,255,255 255,0,255 255,255,0 255,255,255 0,0,0"
  rows[1] = "0,0,0 36,36,36 73,73,73 109,109,109 146,146,146 182,182,182 219,219,219 255,255,255"
  rows[2] = "115,82,68 194,150,130 98,122,157 87,108,67 133,128,177 103,189,170 214,126,44 80,91,166"
  rows[3] = "193,90,99 94,60,108 157,188,64 224,163,46 56,61,150 70,148,73 175,54,60 231,199,31"
  print "P3"; print "64 32"; print "255"
  for (y = 0; y < 32; y++) {
    split(rows[int(y / 8)], patches, " ")
    line = ""
    for (x = 0; x < 64; x++) {
      split(patches[int(x / 8) + 1], c, ",")
      line = line c[1] " " c[2] " " c[3] " "
    }
    print line
  }
}' > "$work/plain.ppm"
# cjxl reads only the binary PPM.
magick "$work/plain.ppm" -depth 8 "$work/chart.ppm"
magick "$work/chart.ppm" -depth 16 "$work/chart16.ppm"
magick "$work/chart.ppm" -colorspace Gray -depth 8 "$work/gray.pgm"
# Rows 0 and 1 are opaque, rows 2 and 3 have an alpha of 128.
magick "$work/chart.ppm" -alpha set -channel A -fx 'j < 16 ? 1 : 128 / 255' +channel \
  "$work/alpha.png"
magick "$work/chart.ppm" -set profile "$profiles/Display P3.icc" "$work/p3.png"
magick "$work/chart.ppm" -set profile "$profiles/AdobeRGB1998.icc" "$work/adobe.png"
magick "$work/alpha.png" -set profile "$profiles/AdobeRGB1998.icc" "$work/adobe-alpha.png"
magick "$work/gray.pgm" -set profile "$profiles/Generic Gray Gamma 2.2 Profile.icc" \
  "$work/gray-icc.png"
magick "$work/chart.ppm" "$work/rotated.png"
magick "$work/chart.ppm" -quality 95 "$work/rotated.jpg"
exiftool -q -overwrite_original -Orientation#=6 "$work/rotated.png" "$work/rotated.jpg"

encode() {
  name=$1
  shift
  cjxl "$@" "$name.jxl" --quiet
}

encode srgb-lossless "$work/chart.ppm" -d 0
encode srgb-lossy "$work/chart.ppm" -d 1
encode srgb-16bit "$work/chart16.ppm" -d 0
encode srgb-alpha "$work/alpha.png" -d 0
encode gray "$work/gray.pgm" -d 0
encode p3-enum "$work/chart.ppm" -x color_space=DisplayP3 -d 0
encode p3-enum-lossy "$work/chart.ppm" -x color_space=DisplayP3 -d 1
# A lossless file keeps the ICC profile. A lossy file stores an enum encoding if one fits.
encode p3-icc "$work/p3.png" -d 0
encode adobe-rgb-icc "$work/adobe.png" -d 0
encode adobe-rgb-icc-alpha "$work/adobe-alpha.png" -d 0
# 16 levels over 64 pixels: most 16-bit samples lie between two 8-bit levels.
magick -size 32x64 gradient:'rgb(100,120,140)-rgb(116,136,156)' -rotate -90 -depth 16 \
  -set profile "$profiles/AdobeRGB1998.icc" "$work/adobe-16bit.png"
encode adobe-rgb-icc-16bit "$work/adobe-16bit.png" -d 0
encode gray-icc "$work/gray-icc.png" -d 0
# cjxl moves the EXIF orientation into the codestream header.
encode orientation-6 "$work/rotated.png" -d 0
encode jpeg-orientation-6 "$work/rotated.jpg" --lossless_jpeg=1

# libjxl converts the sRGB chart into the HDR signal: sRGB white becomes about 250 nits.
djxl srgb-lossless.jxl "$work/pq.png" --color_space=RGB_D65_202_Rel_PeQ --bits_per_sample=16 \
  --quiet
djxl srgb-lossless.jxl "$work/hlg.png" --color_space=RGB_D65_202_Rel_HLG --bits_per_sample=16 \
  --quiet
# The PPM drops the color chunks of the PNG, and -x gives cjxl the encoding.
magick "$work/pq.png" -depth 16 "$work/pq.ppm"
magick "$work/hlg.png" -depth 16 "$work/hlg.ppm"
encode pq "$work/pq.ppm" -x color_space=Rec2100PQ --intensity_target=1000 -d 1
encode hlg "$work/hlg.ppm" -x color_space=Rec2100HLG --intensity_target=1000 -d 1
# The gray ramp of the HDR chart (row 0 of ../hdr/pq.png) as a PQ signal in a Grayscale color
# space. The red channel holds the code values. Lossless, so the file takes the enum path without
# XYB.
magick ../hdr/pq.png -crop 128x16+0+0 +repage -channel R -separate -depth 16 \
  -define png:exclude-chunk=all "$work/gray-pq.pgm"
encode gray-pq "$work/gray-pq.pgm" -x color_space=Gra_D65_Rel_PeQ --intensity_target=1000 -d 0
# The same signals with the BT.2100 PQ and HLG ICC profiles of macOS (with a cicp tag), which
# sips writes into a PNG. A lossless file keeps the profile.
for tf in pq hlg; do
  sips -s format png "$tf.jxl" --out "$work/$tf-sips.png" > /dev/null
  magick "$work/$tf-sips.png" "$work/$tf.icc"
  magick "$work/$tf.ppm" -set profile "$work/$tf.icc" "$work/$tf-icc.png"
  encode "$tf-icc" "$work/$tf-icc.png" -d 0
done

rm -rf ref
mkdir ref
for file in *.jxl; do
  name=${file%.jxl}
  case $name in
    pq | hlg | pq-icc | hlg-icc | gray-pq) continue ;;
  esac
  djxl "$file" "ref/$name.png" --color_space=RGB_D65_SRG_Rel_SRG --bits_per_sample=8 \
    --display_nits=0 --quiet
done
