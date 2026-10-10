#!/bin/sh
# Nextest setup script: give every test process the same scratch roots
# scripts/with-test-tmp.sh would. TUIC_TEST_TMP_ROOT when a wrapper already
# chose one, else a per-checkout dir under
# ${TUIC_TEST_TMP_BASE:-<host temp>/tuic-tests}. Never derived from $HOME.
set -eu

host=${TUIC_TEST_HOST_TMPDIR:-${TMPDIR:-/tmp}}
host=${host%/}
host=${host:-/}
if [ -n "${TUIC_TEST_TMP_ROOT:-}" ]; then
    root=$TUIC_TEST_TMP_ROOT
else
    checkout_hash=$(cd .. && pwd | cksum | cut -d ' ' -f 1)
    root="${TUIC_TEST_TMP_BASE:-$host/tuic-tests}/tuic-nextest-$checkout_hash"
fi
mkdir -p "$root"
root=$(cd "$root" && pwd)
{
    printf 'TUIC_TEST_HOST_TMPDIR=%s\n' "$host"
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
} >> "$NEXTEST_ENV"
