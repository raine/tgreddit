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
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# # Step 3: Build the binary
FROM rust:1.61.0-slim-buster as builder
WORKDIR /app
RUN apt-get update && apt-get install -y \
  musl-tools \
  && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=cacher /app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN rustup target add x86_64-unknown-linux-musl
# RUN cargo install --target x86_64-unknown-linux-musl --path .
RUN cargo build --release --target x86_64-unknown-linux-musl
RUN ls -l /app/target/x86_64-unknown-linux-musl/release

FROM scratch
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/tgreddit .
ENTRYPOINT ["./tgreddit"]
