# ClusterFuzzLite build image. The base image's nightly toolchain (RUSTUP_TOOLCHAIN) takes
# precedence over rust-toolchain.toml, which cargo-fuzz needs for the sanitizer flags.
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f
COPY . $SRC/privacy-flow
WORKDIR $SRC/privacy-flow
COPY .clusterfuzzlite/build.sh $SRC/
