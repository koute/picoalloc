#!/bin/bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

cd fuzz

echo ">> cargo fuzz run"
cargo fuzz run allocator_system -- -runs=20000
