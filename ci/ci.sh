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

  if ! $extended_tests; then
    return
  fi
  
  echo ">> Build for wasm32-unknown-unknown..."
  rustup target add wasm32-unknown-unknown
  rx cargo build --target wasm32-unknown-unknown --no-default-features
  rx cargo build --target wasm32-unknown-unknown --no-default-features --features gl

  echo ">> Build for x86_64-pc-windows-gnu"
  rustup target add x86_64-pc-windows-gnu
  rx cargo build --target x86_64-pc-windows-gnu
  rx cargo build --target x86_64-pc-windows-gnu --no-default-features --features gl,wgl,egl
}

theo_tidy() {
  rustup toolchain add stable --profile minimal
  rustup default stable

  rx cargo fmt --all --check
  rx cargo clippy --all-features --all-targets
}

if ! command -v rustup; then
  rustup-init -y || true
fi

theo_tidy
theo_test_version stable true
theo_test_version beta true
theo_test_version nightly true
theo_test_version 1.65.0 false

