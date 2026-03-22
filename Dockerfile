# Build stages pinned to amd64 to avoid slow QEMU emulation for Rust compilation.
# Cross-compilation is used for arm64 targets instead.

FROM --platform=linux/amd64 rust:1.90.0-slim-bookworm as chef
RUN --mount=type=cache,id=cargo-registry-chef,target=/usr/local/cargo/registry \
  cargo install cargo-chef --locked

FROM --platform=linux/amd64 chef as planner
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM --platform=linux/amd64 chef as cacher
ARG TARGETARCH
WORKDIR /app
RUN rustup target add aarch64-unknown-linux-gnu
RUN apt-get update && apt-get install -y \
  gcc gcc-aarch64-linux-gnu musl-tools libssl-dev perl cmake make \
  && rm -rf /var/lib/apt/lists/*
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/usr/local/cargo/registry \
  if [ "$TARGETARCH" = "arm64" ]; then \
    cargo chef cook --release --target aarch64-unknown-linux-gnu --recipe-path recipe.json --features vendored-openssl; \
  else \
    cargo chef cook --release --target x86_64-unknown-linux-gnu --recipe-path recipe.json --features vendored-openssl; \
  fi

FROM --platform=linux/amd64 rust:1.90.0-slim-bookworm as builder
ARG TARGETARCH
WORKDIR /app
RUN rustup target add aarch64-unknown-linux-gnu
RUN apt-get update && apt-get install -y \
  gcc gcc-aarch64-linux-gnu libssl-dev perl cmake make \
  && rm -rf /var/lib/apt/lists/*
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=cacher /app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/usr/local/cargo/registry \
  if [ "$TARGETARCH" = "arm64" ]; then \
    cargo build --release --target aarch64-unknown-linux-gnu --features vendored-openssl && \
    cp target/aarch64-unknown-linux-gnu/release/tgreddit /app/tgreddit-bin; \
  else \
    cargo build --release --target x86_64-unknown-linux-gnu --features vendored-openssl && \
    cp target/x86_64-unknown-linux-gnu/release/tgreddit /app/tgreddit-bin; \
  fi

# Final stage uses the target platform natively
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/tgreddit-bin ./tgreddit
RUN apt-get update && apt-get install -y \
  curl python3 ffmpeg \
  && rm -rf /var/lib/apt/lists/*
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/download/2026.03.17/yt-dlp -o /usr/local/bin/yt-dlp
RUN chmod a+rx /usr/local/bin/yt-dlp
ENTRYPOINT ["./tgreddit"]
