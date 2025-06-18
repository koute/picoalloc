#!/bin/bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

cd fuzz

echo ">> cargo fuzz run (allocator_system)"
cargo fuzz run allocator_system -- -runs=20000

echo ">> cargo fuzz run (allocator_buffer)"
cargo fuzz run allocator_buffer -- -runs=20000
