#!/bin/bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

cd fuzz

echo ">> cargo fuzz run (allocator_system)"
cargo fuzz run allocator_system -- -runs=20000

echo ">> cargo fuzz run (allocator_buffer)"
cargo fuzz run allocator_buffer -- -runs=20000

echo ">> cargo fuzz run (allocator_buffer, no realloc_inplace)"
cargo fuzz run --no-default-features --features paranoid allocator_buffer -- -runs=20000

echo ">> cargo fuzz run (allocator_buffer, no paranoid)"
cargo fuzz run --no-default-features --features realloc_inplace allocator_buffer -- -runs=20000

echo ">> cargo fuzz run (allocator_buffer, no realloc_inplace, no paranoid)"
cargo fuzz run --no-default-features allocator_buffer -- -runs=20000
