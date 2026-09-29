# Pin the opt-in Ubuntu CI reproduction environment.
# Refresh these pins deliberately and review their effect on rendered media.
FROM rust:1.98.1-slim-bookworm@sha256:dacc9e51f252243eb59d2fb4cb4ad8b0d3f607b6a82c398cf8a321e59ff778a7 AS rust
FROM node:24.14.0-bookworm-slim@sha256:4bd6219054c8bebcd26a66bfd8ca0bd6e1024b4b97474c59bb7ee3bbcbef4fe8 AS node
FROM ubuntu:24.04@sha256:496754492fb28b4d3049432f2ca787449331e23fb14f0dd3fffea86bf5a93eb4

# Snapshot fixes Mesa, LLVM and their dependencies instead of following runner updates.
RUN apt-get update --snapshot 20260928T000000Z \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       --snapshot 20260928T000000Z \
       build-essential ca-certificates git pkg-config python3 \
       libwayland-dev libxkbcommon-dev libx11-dev libxrandr-dev libxi-dev \
       libxcursor-dev libvulkan1 mesa-vulkan-drivers vulkan-tools \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust /usr/local/rustup /opt/rustup
COPY --from=rust /usr/local/cargo/bin/ /usr/local/bin/
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx

ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/n3-cache/cargo \
    CARGO_TARGET_DIR=/n3-cache/target \
    CARGO_BUILD_JOBS=2 \
    N3_DOCS_RENDERER=lavapipe \
    VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
    GALLIUM_DRIVER=llvmpipe \
    GALLIUM_OVERRIDE_CPU_CAPS=sse2 \
    LP_NUM_THREADS=2 \
    XDG_RUNTIME_DIR=/tmp/runtime-n3

WORKDIR /workspace
COPY rust-toolchain.toml ./
RUN rustup component add clippy rustfmt \
    && chmod -R a+rX /opt/rustup \
    && (test -f "$VK_DRIVER_FILES" || test -f /usr/share/vulkan/icd.d/lvp_icd.json)
