set positional-arguments
set shell := ["bash", "-euo", "pipefail", "-c"]

# List available commands
default:
    @just --list

# Run all checks: format → clippy-fix → clippy → test
check: format clippy-fix clippy test

# Format Rust files
format:
    cargo fmt --all

# Run clippy and fail on any warnings
clippy:
    cargo clippy -- -D clippy::all

# Auto-fix clippy warnings
clippy-fix:
    cargo clippy --fix --allow-dirty -- -W clippy::all

# Build the project
build:
    cargo build

# Build release binary
build-release:
    cargo build --release

# Run tests
test *FLAGS:
    cargo test {{FLAGS}}

# Watch and run tests
testw *FLAGS:
    fd .rs | entr -r cargo test {{FLAGS}}

# Watch and run the application
dev *FLAGS:
    fd .rs | entr -r cargo run {{FLAGS}}

# Install release binary globally
install:
    cargo install --path .

# Release a new version (patch, minor, or major)
release bump:
    @scripts/release {{bump}}
