#!/usr/bin/env bash

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
cd ..

./ci/jobs/build-and-test.sh

case "$OSTYPE" in
  linux*)
    ./ci/jobs/fuzz.sh
  ;;
esac

./ci/jobs/clippy.sh
./ci/jobs/rustfmt.sh

echo "----------------------------------------"
echo "All tests finished!"
