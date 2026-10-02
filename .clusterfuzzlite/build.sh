#!/bin/bash -eu
# Build the cargo-fuzz targets in fuzz/ and their seed corpora for ClusterFuzzLite.
cd "$SRC/privacy-flow/fuzz"
cargo fuzz build -O --debug-assertions
for target in lower scan config datamap; do
  cp "target/x86_64-unknown-linux-gnu/release/$target" "$OUT/"
done
python3 seeds.py --zip "$OUT"
# A scan runs the whole pipeline twice per input; give it room beyond the 25 s default.
printf '[libfuzzer]\nmax_len = 65536\ntimeout = 60\n' > "$OUT/scan.options"
