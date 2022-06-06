build:
  cargo build

build-release:
  cargo build --release

run-w *FLAGS:
  fd .rs | entr -r cargo run {{FLAGS}}

install:
  cargo install --path .
