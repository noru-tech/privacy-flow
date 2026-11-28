# ClusterFuzzLite build image. The base image's nightly toolchain (RUSTUP_TOOLCHAIN) takes
# precedence over rust-toolchain.toml, which cargo-fuzz needs for the sanitizer flags.
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:8d2315aeda612bc4176bf20057a72db2a7f61cdf4a8699829c147ab5af02d234
COPY . $SRC/privacy-flow
WORKDIR $SRC/privacy-flow
COPY .clusterfuzzlite/build.sh $SRC/
