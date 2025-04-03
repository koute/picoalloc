#!/usr/bin/env bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ../..

echo ">> cargo clippy"
RUSTFLAGS="-D warnings" cargo clippy --all

cd ../..
