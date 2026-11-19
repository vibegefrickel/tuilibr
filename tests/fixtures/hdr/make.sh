#!/bin/sh
# Rebuilds the HDR fixtures of src/decode/hdr.rs and their references through the video cube.
#
# Needs avifenc, heif-enc (Homebrew libheif with x265), cjxl 0.12, ImageMagick 7 (magick), sips,
# python3, ffmpeg and just. Run it from any directory: ./tests/fixtures/hdr/make.sh
#
# The chart is 128x64 pixels: 4 rows of 8 patches, 16x16 pixels each. The values are BT.2100
# signal values from 0 to 1 with the BT.2020 primaries, rounded to 10 bit:
#   row 0: a gray ramp: 0, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 1
#   row 1: red, green, blue, cyan, magenta, yellow at 0.6, and two pale colors
#   row 2: 8 darker colors, like the ColorChecker patches 1 to 8
#   row 3: 8 highlight colors from 0.85 to 0.95, for the roll-off and the gamut map
# The same signal is PQ (pq.*) and HLG (hlg.*):
#   pq.avif, hlg.avif: 10 bit, 4:4:4, nclx 9/16/9 and 9/18/9, full range
#   pq.heic, hlg.heic: 10 bit, 4:4:4, nclx 9/16/9 and 9/18/9, full range
#   pq.jxl, hlg.jxl: lossless, the enum encodings Rec2100PQ and Rec2100HLG
#   pq-lossy.jxl, hlg-lossy.jxl: the same enum encodings, lossy (distance 1), so the samples
#     are XYB and jxl-oxide renders them into the signal
#   pq-icc.jxl: lossless, the BT.2100 PQ ICC profile of macOS (with a cicp tag), no enum
#   pq.png, hlg.png: 16 bit with a cICP chunk 9/16/0/1 and 9/18/0/1
#   pq-4000.avif, pq-4000.heic: as pq.avif and pq.heic, with a clli box of MaxCLL 4000 nits.
#     avifenc and heif-enc write no mdcv box, so the unit tests of heif_boxes.rs cover mdcv.
#   pq-4000-8bit.heic: pq-4000.heic in 8 bit, which takes the 8-bit route of the decoder
#   pq-4000-thumb.heic: pq-4000.heic with an embedded thumbnail of 32x16 pixels (heif-enc -t 32)
#   pq-icc-4000.png: the PQ ICC profile of pq-icc.jxl in an iCCP chunk, no cICP, with a cLLI chunk
#     of 4000 nits
#   pq-4000.png: pq.png with a cLLI chunk of MaxCLL 4000 nits (40000000 in units of 0.0001 cd/m2)
#   pq-4000.jxl: pq.jxl with intensity_target 4000 nits in the image header
#
# The references are `ref/<curve>-<target>.png`, for the targets p3 and srgb: ffmpeg maps the
# signal with the HDR cube of the terminal player (`tone_map::Cube`, written by the ignored test
# `write_the_player_hdr_cubes`), with the filters of the player: `format=rgb48le`,
# `lut3d=interp=tetrahedral`, then 8-bit RGB. `ref/pq4000-<target>.png` uses the PQ cube of the
# 4000 nit bucket.
set -eu

cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

awk 'BEGIN {
  rows[0] = "0,0,0 0.1,0.1,0.1 0.25,0.25,0.25 0.4,0.4,0.4 0.5,0.5,0.5 0.6,0.6,0.6 0.75,0.75,0.75 1,1,1"
  rows[1] = "0.6,0,0 0,0.6,0 0,0,0.6 0,0.6,0.6 0.6,0,0.6 0.6,0.6,0 0.55,0.45,0.4 0.35,0.45,0.55"
  rows[2] = "0.32,0.23,0.19 0.53,0.41,0.36 0.27,0.34,0.43 0.24,0.3,0.18 0.37,0.35,0.49 0.28,0.52,0.47 0.59,0.35,0.12 0.22,0.25,0.46"
  rows[3] = "0.9,0.6,0.3 0.3,0.9,0.6 0.6,0.3,0.9 0.95,0.95,0.6 0.85,0.2,0.2 0.2,0.85,0.2 0.2,0.2,0.85 0.9,0.9,0.9"
  print "P3"; print "128 64"; print "65535"
  for (y = 0; y < 64; y++) {
    split(rows[int(y / 16)], patches, " ")
    line = ""
    for (x = 0; x < 128; x++) {
      split(patches[int(x / 16) + 1], c, ",")
      for (i = 1; i <= 3; i++) {
        code = int(c[i] * 1023 + 0.5)
        line = line int(code * 65535 / 1023 + 0.5) " "
      }
    }
    print line
  }
}' > "$work/plain.ppm"
magick "$work/plain.ppm" -depth 16 -define png:exclude-chunk=all "$work/signal.png"
# cjxl reads only the binary PPM.
magick "$work/plain.ppm" -depth 16 "$work/signal.ppm"

for curve in pq hlg; do
  case $curve in
    pq) transfer=16 jxl=Rec2100PQ ;;
    hlg) transfer=18 jxl=Rec2100HLG ;;
  esac
  # avifenc warns that quality 100 with the matrix 9 is not lossless: the YCbCr round trip
  # moves a sample by at most 1 of 1023.
  avifenc -d 10 -y 444 -q 100 -r full --cicp "9/$transfer/9" "$work/signal.png" "$curve.avif" \
    > /dev/null 2>&1
  heif-enc -b 10 -p chroma=444 -q 100 --matrix_coefficients 9 --colour_primaries 9 \
    --transfer_characteristic "$transfer" --full_range_flag 1 "$work/signal.png" -o "$curve.heic" \
    > /dev/null
  cjxl "$work/signal.ppm" "$curve.jxl" -x "color_space=$jxl" -d 0 --quiet
  cjxl "$work/signal.ppm" "$curve-lossy.jxl" -x "color_space=$jxl" -d 1 --quiet
done
cjxl "$work/signal.ppm" pq-4000.jxl -x color_space=Rec2100PQ -d 0 --intensity_target=4000 --quiet
avifenc -d 10 -y 444 -q 100 -r full --cicp 9/16/9 --clli 4000,400 "$work/signal.png" \
  pq-4000.avif > /dev/null 2>&1
heif-enc -b 10 -p chroma=444 -q 100 --matrix_coefficients 9 --colour_primaries 9 \
  --transfer_characteristic 16 --full_range_flag 1 --clli 4000,400 "$work/signal.png" \
  -o pq-4000.heic > /dev/null
magick "$work/signal.png" -depth 8 "$work/signal8.png"
heif-enc -p chroma=444 -q 100 --matrix_coefficients 9 --colour_primaries 9 \
  --transfer_characteristic 16 --full_range_flag 1 --clli 4000,400 "$work/signal8.png" \
  -o pq-4000-8bit.heic > /dev/null
heif-enc -b 10 -p chroma=444 -q 100 --matrix_coefficients 9 --colour_primaries 9 \
  --transfer_characteristic 16 --full_range_flag 1 --clli 4000,400 -t 32 "$work/signal.png" \
  -o pq-4000-thumb.heic > /dev/null

# sips writes the BT.2100 PQ ICC profile of macOS into a PNG. A lossless JPEG XL file keeps the
# profile and gets no enum encoding.
sips -s format png pq.jxl --out "$work/pq-sips.png" > /dev/null
magick "$work/pq-sips.png" "$work/pq.icc"
magick "$work/signal.png" -set profile "$work/pq.icc" "$work/pq-icc.png"
cjxl "$work/pq-icc.png" pq-icc.jxl -d 0 --quiet

# ImageMagick cannot write a cICP chunk, so python3 inserts it after IHDR, with a cLLI chunk
# when a MaxCLL in nits is given. The transfer 0 writes no cICP chunk.
insert_cicp() {
  python3 -I - "$@" <<'EOF'
import struct, sys, zlib
source, target, transfer = sys.argv[1:4]
max_cll = int(sys.argv[4]) if len(sys.argv) > 4 else None
data = open(source, "rb").read()
end_of_ihdr = 8 + 8 + 13 + 4

def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

chunks = chunk(b"cICP", bytes([9, int(transfer), 0, 1])) if transfer != "0" else b""
if max_cll is not None:
    chunks += chunk(b"cLLI", struct.pack(">II", max_cll * 10000, 400 * 10000))
open(target, "wb").write(data[:end_of_ihdr] + chunks + data[end_of_ihdr:])
EOF
}
insert_cicp "$work/signal.png" pq.png 16
insert_cicp "$work/signal.png" hlg.png 18
insert_cicp "$work/signal.png" pq-4000.png 16 4000
insert_cicp "$work/pq-icc.png" pq-icc-4000.png 0 4000

# The cubes of the player, then the references through them.
PHEWR_HDR_CUBE_DIR=$work just test-real write_the_player_hdr_cubes > /dev/null 2>&1
rm -rf ref
mkdir ref
for curve in pq pq4000 hlg; do
  for target in p3 srgb; do
    ffmpeg -v error -i "$work/signal.png" \
      -vf "format=rgb48le,lut3d=file=$work/$curve-$target.cube:interp=tetrahedral" \
      -pix_fmt rgb24 -y "ref/$curve-$target.png"
  done
done
