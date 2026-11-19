#!/usr/bin/env bash
# Regenerate the Kotlin bindings for the Rust core (run after changing core-ffi/src/lib.rs).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p touchtwin-ffi --lib
cargo run -q -p touchtwin-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/debug/libtouchtwin_ffi.so --language kotlin \
  --out-dir android/app/src/main/java --no-format
echo "Bindings written to android/app/src/main/java/dev/touchtwin/core/"
