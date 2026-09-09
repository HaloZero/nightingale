# syntax=docker/dockerfile:1

# Multi-stage build for the Nightingale self-hosted web server.
#
# The final runtime base is parametrised so the same Dockerfile produces both
# the CPU image and the CUDA/GPU image:
#
#   CPU (default):
#     docker build -f docker/Dockerfile -t nightingale .
#
#   CUDA/GPU (needs the NVIDIA Container Toolkit at run time):
#     docker build -f docker/Dockerfile \
#       --build-arg RUNTIME_BASE=nvidia/cuda:12.6.2-runtime-ubuntu24.04 \
#       -t nightingale:cuda .
#
# The heavy ML stack (ffmpeg, uv, Python 3.10, PyTorch, WhisperX, Demucs, ...)
# is NOT baked in: it is downloaded on first launch into the mounted /data
# volume, exactly like the bare-metal install. Keep that volume around to avoid
# re-downloading several GB on every container recreate.

# Declared globally so it is usable in the runtime stage's FROM instruction.
ARG RUNTIME_BASE=debian:bookworm-slim

# ── Stage 1: frontend bundle ──────────────────────────────────────────────
# rust-embed slurps client/dist/ at compile time, so the React bundle must be
# built before the server binary.
FROM node:20-bookworm-slim AS frontend

RUN npm install -g pnpm@9

WORKDIR /build/client

# Install dependencies first so the layer is cached across source-only changes.
COPY client/package.json client/pnpm-lock.yaml client/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

COPY client/ ./
RUN pnpm build

# ── Stage 2: server binary ────────────────────────────────────────────────
# Pinned to bookworm so the produced glibc-linked binary runs on both the
# debian:bookworm-slim CPU base and the newer Ubuntu CUDA base.
FROM rust:1-bookworm AS builder

WORKDIR /build

# Bring in the whole workspace so `cargo build -p server` can resolve every
# member manifest (app-core, client/src-server, client/src-tauri, xtask).
COPY . .

# Drop in the pre-built frontend bundle for rust-embed.
COPY --from=frontend /build/client/dist ./client/dist

# Baked into the binary via app-core/build.rs so video backgrounds work.
# Optional: builds fine without it, video fetch just returns an error. Also
# what release builds do — the key ships inside the public binaries. The ARG is
# visible to the RUN below as an env var, which is what build.rs reads.
ARG PIXABAY_API_KEY=""

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked -p server \
    && cp target/release/server /usr/local/bin/nightingale

# ── Stage 3: runtime ──────────────────────────────────────────────────────
FROM ${RUNTIME_BASE} AS runtime

# ca-certificates: TLS for the bootstrap downloads (ureq).
# tar + xz-utils: extract the ffmpeg .tar.xz and uv .tar.gz archives.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tar xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Non-root service user. /data is both the app data dir and $HOME so that
# uv/pip/hf caches land on the persisted volume too.
RUN useradd --system --create-home --home-dir /data --shell /usr/sbin/nologin nightingale

COPY --from=builder /usr/local/bin/nightingale /usr/local/bin/nightingale

# NIGHTINGALE_LIBRARY_PATH defaults to /songs so the library is configured by
# convention: mount your music at /songs and it just works. Override it to a
# different path, or set it empty to use in-app / remote (Jellyfin, Navidrome)
# sources instead.
ENV NIGHTINGALE_DATA_PATH=/data \
    NIGHTINGALE_BIND=0.0.0.0:8080 \
    NIGHTINGALE_LIBRARY_PATH=/songs \
    HOME=/data

USER nightingale
WORKDIR /data
VOLUME ["/data"]
EXPOSE 8080

ENTRYPOINT ["nightingale"]
