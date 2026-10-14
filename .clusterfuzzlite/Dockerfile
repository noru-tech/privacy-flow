# ClusterFuzzLite build image. The base image's nightly toolchain (RUSTUP_TOOLCHAIN) takes
# precedence over rust-toolchain.toml, which cargo-fuzz needs for the sanitizer flags.
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:c62b4a41865adec0ffd93e9d21be7654a8359f75dee2e96a409325a494e93040
COPY . $SRC/privacy-flow
WORKDIR $SRC/privacy-flow
COPY .clusterfuzzlite/build.sh $SRC/
