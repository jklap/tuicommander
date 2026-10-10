#!/bin/sh
# Nextest setup script: give every test process the same scratch roots
# scripts/with-test-tmp.sh would. TUIC_TEST_TMP_ROOT when a wrapper already
# chose one, else this checkout's default under the host temp dir (order in
# scripts/test-tmp-lib.sh). Never derived from $HOME.
set -eu

# shellcheck source=../../scripts/test-tmp-lib.sh
. ../scripts/test-tmp-lib.sh
host=$(tuic_test_host_tmpdir)
root=$(tuic_test_tmp_root ..)
root=$(cd "$root" && pwd)
{
    printf 'TUIC_TEST_HOST_TMPDIR=%s\n' "$host"
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
} >> "$NEXTEST_ENV"
