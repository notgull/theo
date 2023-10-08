#!/bin/sh

set -eu

# Run CI-based tests for Theo

rx() {
  cmd="$1"
  shift

  (
    set -x
    "$cmd" "$@"
  )
}

theo_check_target() {
  target="$1"
  cmd="$2"

  echo ">> Check for $target"
  rustup add target "$target"
  rx cargo "$cmd" --target "$target" --no-default-features
  rx cargo "$cmd" --target "$target" --no-default-features \
      --features gl,wgl,egl
  cargo clean
}

theo_test_version() {
  version="$1"
  extended_tests="$2"

  rustup toolchain add "$version" --profile minimal
  rustup default "$version"

  echo ">> Testing various feature sets..."
  rx cargo test
  rx cargo build --all --all-features --all-targets
  rx cargo build --no-default-features --features x11,gl,egl,glx,wgl
  rx cargo build --no-default-features --features x11
  cargo clean

  if ! $extended_tests; then
    return
  fi
  
  theo_check_target wasm32-unknown-unknown build
  theo_check_target x86_64-pc-windows-gnu build
  theo_check_target x86_64-apple-darwin check
}

theo_tidy() {
  rustup toolchain add stable --profile minimal
  rustup default stable

  rx cargo fmt --all --check
  rx cargo clippy --all-features --all-targets
}

. "$HOME/.cargo/env"

theo_tidy
theo_test_version stable true
theo_test_version beta true
theo_test_version nightly true
theo_test_version 1.65.0 false

