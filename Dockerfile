# Step 1: Compute a recipe file
FROM rust:1.61.0-slim-buster as planner
WORKDIR /app
RUN cargo install cargo-chef
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Step 2: Cache project dependencies
FROM rust:1.61.0-slim-buster as cacher
WORKDIR /app
RUN rustup target add x86_64-unknown-linux-musl
RUN apt-get update && apt-get install -y \
  musl-tools \
  && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

# Step 3: Build the binary
FROM rust:1.61.0-slim-buster as builder
WORKDIR /app
RUN rustup target add x86_64-unknown-linux-musl
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=cacher /app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN cargo build --release --target x86_64-unknown-linux-musl
RUN ls -l /app/target/x86_64-unknown-linux-musl/release

# Step 4: Create the final image with binary and deps
FROM debian:buster-slim
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/tgreddit .
RUN apt-get update && apt-get install -y \
  curl python3 ffmpeg \
  && rm -rf /var/lib/apt/lists/*
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/download/2022.05.18/yt-dlp -o /usr/local/bin/yt-dlp
RUN chmod a+rx /usr/local/bin/yt-dlp
ENTRYPOINT ["./tgreddit"]
