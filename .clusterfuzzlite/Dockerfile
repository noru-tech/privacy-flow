# ClusterFuzzLite build image. The base image's nightly toolchain (RUSTUP_TOOLCHAIN) takes
# precedence over rust-toolchain.toml, which cargo-fuzz needs for the sanitizer flags.
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:9643b583ef5aa3d956b3c170e93ef8283878c70c6df8c99d8a70d73c5019677f
COPY . $SRC/privacy-flow
WORKDIR $SRC/privacy-flow
COPY .clusterfuzzlite/build.sh $SRC/
