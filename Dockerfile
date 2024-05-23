# Step 1: Compute a recipe file
FROM rust:1.70.0-slim-bookworm as chef
RUN --mount=type=cache,target=/usr/local/cargo/registry \
  cargo install cargo-chef

# Step 2: Compute a recipe file
FROM chef as planner
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Step 3: Cache project dependencies
FROM chef as cacher
WORKDIR /app
RUN rustup target add aarch64-unknown-linux-gnu
RUN apt-get update && apt-get install -y \
  gcc-aarch64-linux-gnu musl-tools libssl-dev perl cmake make \
  && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
  cargo chef cook --release --target aarch64-unknown-linux-gnu --recipe-path recipe.json --features vendored-openssl

# Step 4: Build the binary
FROM rust:1.70.0-slim-bookworm as builder
WORKDIR /app
RUN rustup target add aarch64-unknown-linux-gnu
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=cacher /app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build --release --target aarch64-unknown-linux-gnu --features vendored-openssl

# Step 5: Create the final image with binary and deps
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/aarch64-unknown-linux-gnu/release/tgreddit .
RUN apt-get update && apt-get install -y \
  curl python3 ffmpeg \
  && rm -rf /var/lib/apt/lists/*
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/download/2024.04.09/yt-dlp -o /usr/local/bin/yt-dlp
RUN chmod a+rx /usr/local/bin/yt-dlp
ENTRYPOINT ["./tgreddit"]
