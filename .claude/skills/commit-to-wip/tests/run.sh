#!/usr/bin/env bash
# Run the commit-to-wip tests with scratch under the repo's per-run test temp root.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
exec "$root/scripts/with-test-tmp.sh" "${BATS:-bats}" "$@" "$here"
