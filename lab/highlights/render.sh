#!/usr/bin/env bash
# Draws the sketches in docs/plan-explainer.md: the highlights, and the central circle's construction. Needs ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release -q -p pencil-cli
out=$(mktemp -d)
pencil=target/release/pencil
lab=lab/highlights

for s in 1_part 2_ring 3_spot 4_arms 5_taper 6_circle; do
  "$pencil" render "$lab/spike.pen" --style corpus/theme.style --style "$lab/styles/base.style" \
    --style "$lab/styles/$s.style" --t 1 --size 480 --out "$out/$s.png" > /dev/null
done
# Flat, cusp and loop, as the near edge moves.
for t in 0.5 1.5 2.5; do
  "$pencil" render "$lab/near.pen" --style corpus/theme.style --style "$lab/styles/near.style" \
    --t "$t" --size 480 --out "$out/near_$t.png" > /dev/null
done

ffmpeg -y -loglevel error \
  -i "$out/1_part.png" -i "$out/2_ring.png" -i "$out/3_spot.png" \
  -i "$out/4_arms.png" -i "$out/5_taper.png" -i "$out/6_circle.png" \
  -i "$out/near_0.5.png" -i "$out/near_1.5.png" -i "$out/near_2.5.png" \
  -filter_complex "[0][1][2]hstack=3[a];[3][4][5]hstack=3[b];[6][7][8]hstack=3[c];[a][b][c]vstack=3" \
  docs/highlights.png
echo "wrote docs/highlights.png"

for t in 1.1 2.4; do
  "$pencil" render "$lab/central.pen" --style corpus/theme.style --style "$lab/styles/central.style" \
    --t "$t" --size 640 --out "$out/central_$t.png" > /dev/null
done
ffmpeg -y -loglevel error -i "$out/central_1.1.png" -i "$out/central_2.4.png" \
  -filter_complex hstack=2 docs/central.png
echo "wrote docs/central.png"
