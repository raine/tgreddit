build:
  cargo build

build-release:
  cargo build --release

run-w *FLAGS:
  fd .rs | entr -r cargo run {{FLAGS}}

test-w *FLAGS:
  fd .rs | entr -r cargo test {{FLAGS}}

install:
  cargo install --path .
