#!/usr/bin/env bash
# run-remote-fixture.sh must never write under $HOME, must keep the named
# socket's TMPDIR within the sun_path budget, must honour TUIC_FIXTURE_TMPDIR
# and must print the TMPDIR it chose. A fake binary stands in for tuic-remote.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-remote-fixture-test.XXXXXX")"
# Short enough to hold the named socket even where TMPDIR is long.
short="$(mktemp -d /tmp/tuic-rft.XXXXXX)"
instance="tuic-fixture-test-$$"
cleanup() {
  chmod -R u+rwX "$fixture"
  rm -rf "$fixture" "$short"
  for made in "/tmp/tuic-rf-$(printf '%s' "$instance" | cksum | cut -d ' ' -f 1)"; do
    [[ -d "$made" && -O "$made" ]] && rmdir "$made" 2>/dev/null || true
  done
}
trap cleanup EXIT

home="$fixture/home"
mkdir -p "$home"
chmod 0555 "$home"
fake="$fixture/fake-tuic-remote"
printf '#!/bin/sh\nprintf "TMPDIR=%%s\\nROOT=%%s\\n" "$TMPDIR" "$TUIC_TEST_TMP_ROOT"\n' > "$fake"
chmod +x "$fake"

launch() {
  env -u TUIC_FIXTURE_TMPDIR -u TUIC_FIXTURE_ROOT HOME="$home" "$@" \
    bash "$root/scripts/run-remote-fixture.sh" "$fake" 19999 "$instance"
}
field() { printf '%s\n' "$1" | sed -n "s/^$2=//p"; }

# Catches: a long caller TMPDIR producing a named socket path past sun_path.
long_tmp="$fixture/a-deliberately-long-caller-temp-dir-that-cannot-hold-the-socket"
mkdir -p "$long_tmp"
out="$(launch TMPDIR="$long_tmp/")"
chosen="$(field "$out" TMPDIR)"
printf '%s\n' "$out" | grep -Fq "Fixture TMPDIR: $chosen" \
  || { echo "launcher did not print its TMPDIR" >&2; printf '%s\n' "$out" >&2; exit 1; }
(( ${#chosen} - 1 <= 61 )) || { echo "fixture TMPDIR $chosen is over 61 chars" >&2; exit 1; }
case "$(field "$out" ROOT)" in
  "$long_tmp/tuic-remote-fixture/$instance/") ;;
  *) echo "config fallback root left the caller's temp dir: $out" >&2; exit 1 ;;
esac

# Catches: clobbering a TMPDIR the caller chose for a separately launched client.
mine="$short/sock"
out="$(launch TMPDIR="$long_tmp/" TUIC_FIXTURE_TMPDIR="$mine")"
test "$(field "$out" TMPDIR)" = "$mine/" || { echo "TUIC_FIXTURE_TMPDIR ignored: $out" >&2; exit 1; }
test "$(stat -f '%Lp' "$mine" 2>/dev/null || stat -c '%a' "$mine")" = 700 \
  || { echo "fixture TMPDIR is not private" >&2; exit 1; }

# Catches: an over-budget override failing later as a mystery bind error.
if launch TUIC_FIXTURE_TMPDIR="$long_tmp/and/even/longer/than/that" >/dev/null 2>&1; then
  echo "an over-budget TUIC_FIXTURE_TMPDIR was accepted" >&2; exit 1
fi

if [ -n "$(find "$home" -mindepth 1 -print -quit)" ]; then
  echo "the fixture launcher wrote below HOME" >&2; exit 1
fi
