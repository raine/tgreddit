build:
  cargo build

build-release:
  cargo build --release

dev *FLAGS:
  fd .rs | entr -r cargo run {{FLAGS}}

test *FLAGS:
  cargo test {{FLAGS}}

testw *FLAGS:
  fd .rs | entr -r cargo test {{FLAGS}}

install:
  cargo install --path .
