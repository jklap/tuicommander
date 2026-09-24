#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fixture="$fixture_dir/test-failing-fixture.sh"
printf '#!/usr/bin/env bash\nexit 1\n' > "$fixture"

if output="$(SHELL_TEST_DIR="$fixture_dir" make -C "$project_root" test-shell 2>&1)"; then
  echo "expected the shell test runner to fail" >&2
  exit 1
fi

if ! printf '%s\n' "$output" | grep -Fq "$fixture"; then
  echo "shell test runner did not execute the failing fixture" >&2
  printf '%s\n' "$output" >&2
  exit 1
fi

printf '#!/usr/bin/env bash\nexit 0\n' > "$fixture"
SHELL_TEST_DIR="$fixture_dir" make -C "$project_root" test-shell
