# Native aarch64 build environment for the Pi Zero 2 W (Debian trixie, glibc 2.41).
# Built on Colima's arm64 VM, so this compiles natively (no QEMU, no cross-toolchain).
# Bookworm base (glibc 2.36) keeps the produced binary forward-compatible with the Pi.
FROM rust:1-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libasound2-dev \
        pkg-config \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
