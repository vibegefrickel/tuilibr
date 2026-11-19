#!/bin/sh
# Rebuilds the color fixtures of src/decode/color.rs and their ColorSync references.
#
# Needs ImageMagick 7 (magick), heif-enc (Homebrew libheif), avifenc, python3, sips and the
# ColorSync profiles of macOS. Run it from any directory: ./tests/fixtures/color/make.sh
#
# The chart is the chart of tests/fixtures/jxl/make.sh: 64x32 pixels, 4 rows of 8 patches,
# 8x8 pixels each.
#   row 0: red, green, blue, cyan, magenta, yellow, white, black
#   row 1: a gray ramp from 0 to 255 in 8 steps
#   row 2: ColorChecker patches 1 to 8
#   row 3: ColorChecker patches 9 to 16
# The p3-* fixtures store these code values as Display P3, the srgb-* fixtures as sRGB, and
# adobe-rgb-icc.jpg as Adobe RGB 1998. Each fixture says its color space in another way:
#   p3-icc.jpg, .png, .webp, .tiff, .heic: the Display P3 ICC profile (JPEG APP2, iCCP, ICCP,
#     tag 34675, HEIF prof box)
#   p3-nclx.heic, p3-nclx.avif: an nclx box with primaries 12 and transfer 13, no ICC profile
#   p3-cicp.png: a cICP chunk 12/13/0/1, no ICC profile
#   p3-chrm.png: the cHRM chunk of Display P3 and gAMA 45455, no ICC profile
#   srgb.jpg, srgb.png: nothing, so phewr reads them as sRGB
#
# The references are `ref/<name>-p3.png` and `ref/<name>-srgb.png`: sips converts each fixture
# with `--matchTo` into the Display P3 or the sRGB profile of ColorSync. An untagged fixture
# gets the sRGB profile first. ColorSync reads cICP, and gAMA with cHRM, as well.
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
magick "$work/plain.ppm" -depth 8 "$work/plain.png"

# The JPEG fixtures keep full chroma (4:4:4) and quality 100, so the patches stay flat.
jpeg="-sampling-factor 1x1 -quality 100"
magick "$work/plain.png" -set profile "$profiles/Display P3.icc" "$work/p3.png"
magick "$work/p3.png" $jpeg p3-icc.jpg
magick "$work/p3.png" p3-icc.png
magick "$work/p3.png" -define webp:lossless=true p3-icc.webp
magick "$work/p3.png" -type TrueColor -compress none p3-icc.tiff
magick "$work/plain.png" -set profile "$profiles/AdobeRGB1998.icc" $jpeg adobe-rgb-icc.jpg
magick "$work/plain.png" $jpeg srgb.jpg
magick "$work/plain.png" -define png:exclude-chunk=all srgb.png

# heif-enc copies the ICC profile of the PNG into a prof box, as an iPhone writes it.
heif-enc -L "$work/p3.png" -o p3-icc.heic > /dev/null
heif-enc -L --matrix_coefficients 0 --colour_primaries 12 --transfer_characteristic 13 \
  --full_range_flag 1 "$work/plain.png" -o p3-nclx.heic > /dev/null
avifenc -l --cicp 12/13/0 "$work/plain.png" p3-nclx.avif > /dev/null

# Insert color chunks after IHDR. ImageMagick cannot write a cICP chunk, so python3 writes
# the color chunks.
insert_chunks() {
  python3 -I - "$@" <<'EOF'
import struct, sys, zlib
source, target, *chunks = sys.argv[1:]
data = open(source, "rb").read()
end_of_ihdr = 8 + 8 + 13 + 4
extra = b""
for chunk in chunks:
    kind, values = chunk.split(":")
    if kind == "cICP":
        body = bytes(int(v) for v in values.split(","))
    else:
        body = b"".join(struct.pack(">I", int(v)) for v in values.split(","))
    tag = kind.encode()
    extra += struct.pack(">I", len(body)) + tag + body
    extra += struct.pack(">I", zlib.crc32(tag + body))
open(target, "wb").write(data[:end_of_ihdr] + extra + data[end_of_ihdr:])
EOF
}
magick "$work/plain.png" -define png:exclude-chunk=all "$work/stripped.png"
insert_chunks "$work/stripped.png" p3-cicp.png "cICP:12,13,0,1"
insert_chunks "$work/stripped.png" p3-chrm.png "gAMA:45455" \
  "cHRM:31270,32900,68000,32000,26500,69000,15000,6000"

mkdir -p ref
reference() {
  name=$1
  source=$2
  sips -s format png --matchTo "$profiles/Display P3.icc" "$source" --out "ref/$name-p3.png" \
    > /dev/null
  sips -s format png --matchTo "$profiles/sRGB Profile.icc" "$source" --out "ref/$name-srgb.png" \
    > /dev/null
}
for name in p3-icc.jpg p3-icc.png p3-icc.webp p3-icc.tiff p3-icc.heic p3-nclx.heic p3-nclx.avif \
  p3-cicp.png p3-chrm.png adobe-rgb-icc.jpg; do
  reference "${name%.*}-${name##*.}" "$name"
done
for name in srgb.jpg srgb.png; do
  magick "$name" -set profile "$profiles/sRGB Profile.icc" "$work/tagged-$name.png"
  reference "${name%.*}-${name##*.}" "$work/tagged-$name.png"
done
