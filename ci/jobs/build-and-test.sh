#!/bin/bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

echo ">> cargo test (debug)"
cargo test --all

echo ">> cargo test (release)"
cargo test --all --release

echo ">> cargo test (paranoid)"
cargo test --features paranoid

echo ">> cargo test (paranoid, global allocator)"
cargo test --features paranoid,global_allocator_rust

echo ">> cargo build (native)"
cargo build -p picoalloc_native --release

echo ">> cargo check (PolkaVM)"
RUSTC_BOOTSTRAP=1 cargo check --target=ci/riscv64emac-unknown-none-polkavm.json -Z build-std=core

echo ">> cargo check (CoreVM)"
RUSTC_BOOTSTRAP=1 cargo check --target=ci/riscv64emac-unknown-none-polkavm.json -Z build-std=core --features corevm

echo ">> cargo check (riscv32i-unknown-none-elf)"
cargo check --target=riscv32i-unknown-none-elf
