#!/bin/bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

cargo check --features paranoid,global_allocator_rust --target=wasm32-wasip2

echo ">> cargo test (debug, WASM)"
wasmtime $(cargo test --features paranoid --target=wasm32-wasip2 --no-run --message-format=json | grep -oE '"executable":"[^"]+"' | cut -d ":" -f 2 | grep -oE '[^"]+')

echo ">> cargo test (release, WASM)"
wasmtime $(cargo test --features paranoid --release --target=wasm32-wasip2 --no-run --message-format=json | grep -oE '"executable":"[^"]+"' | cut -d ":" -f 2 | grep -oE '[^"]+')

echo ">> cargo test (debug, global allocator, WASM)"
wasmtime $(cargo test --features paranoid,global_allocator_rust --target=wasm32-wasip2 --no-run --message-format=json | grep -oE '"executable":"[^"]+"' | cut -d ":" -f 2 | grep -oE '[^"]+')
