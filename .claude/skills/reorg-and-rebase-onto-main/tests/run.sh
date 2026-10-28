#!/usr/bin/env bash
# Run the pytest and bats suites through the repo's test-tmp wrapper.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
"$root/scripts/with-test-tmp.sh" uv run --with pytest pytest -q -p no:cacheprovider "$here"
"$root/scripts/with-test-tmp.sh" bats "$here"
