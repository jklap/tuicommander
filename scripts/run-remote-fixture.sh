#!/usr/bin/env bash
# Fixture only: binary MUST be built with --no-default-features --features tuic-core/test-support.
# This uses the existing test config fallback, so no production config directory is written.
# Share this launcher for remote MCP, detection and browser-grid verification.
set -euo pipefail
fixture_binary="${1:?usage: run-remote-fixture.sh <test-support binary> [port] [instance]}"
fixture_port="${2:-19877}"
fixture_instance="${3:-tuic-remote-fixture}"
if [[ ! "$fixture_instance" =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
  echo "Invalid fixture instance" >&2; exit 2
fi
# Scratch lives under the caller's temp dir (or TUIC_FIXTURE_ROOT), never $HOME.
fixture_root="${TUIC_FIXTURE_ROOT:-${TMPDIR:-/tmp}}"
fixture_root="${fixture_root%/}"
# A named instance binds <TMPDIR>/tuic-mcp-<16 hex>.sock, and a separately
# launched `tuic` client must export the same TMPDIR to find it. Honour
# TUIC_FIXTURE_TMPDIR; otherwise use a short private per-instance dir that
# keeps that socket within sun_path (TMPDIR at most 61 chars).
if [[ -n "${TUIC_FIXTURE_TMPDIR:-}" ]]; then
  fixture_tmpdir="${TUIC_FIXTURE_TMPDIR%/}"
else
  instance_hash="$(printf '%s' "$fixture_instance" | cksum | cut -d ' ' -f 1)"
  fixture_tmpdir="$fixture_root/tuic-rf-$instance_hash"
  if (( ${#fixture_tmpdir} > 61 )); then
    fixture_tmpdir="/tmp/tuic-rf-$instance_hash"
  fi
fi
if (( ${#fixture_tmpdir} > 61 )); then
  echo "Fixture TMPDIR $fixture_tmpdir is ${#fixture_tmpdir} chars; the named socket needs at most 61. Set TUIC_FIXTURE_TMPDIR to a shorter dir." >&2
  exit 2
fi
mkdir -p -m 0700 "$fixture_tmpdir"
if [[ -L "$fixture_tmpdir" || ! -O "$fixture_tmpdir" ]]; then
  echo "Fixture TMPDIR $fixture_tmpdir is not a directory owned by you" >&2; exit 2
fi
export TMPDIR="$fixture_tmpdir/"
export TUIC_TEST_TMP_ROOT="$fixture_root/tuic-remote-fixture/$fixture_instance/"
export PAGER=cat GIT_PAGER=cat
mkdir -p "$TUIC_TEST_TMP_ROOT"
echo "Fixture TMPDIR: $TMPDIR (export TMPDIR=$TMPDIR for a separately launched tuic client)"
export TUIC_PORT="$fixture_port"
export TUIC_PAIRING_TOKEN="${TUIC_FIXTURE_TOKEN:-tuic-1421-fixture}"
echo "Fixture URL: http://127.0.0.1:$fixture_port (instance $fixture_instance)"
exec "$fixture_binary" --instance "$fixture_instance" --bind 127.0.0.1 --no-agent-configs
