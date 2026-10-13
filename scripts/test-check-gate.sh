#!/usr/bin/env bash
# check-gate.sh against a stub `make` (and stub cargo/rustup, so nothing is
# built): it must exit with make's own status, print every byte make wrote
# (including a burst written right before make exits), keep the log only on
# failure, and stop make when the gate itself is interrupted or terminated.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-check-gate-test.XXXXXX")"
cleanup() {
  # A stub make left running by a failed assertion must not outlive the test.
  if [[ -f "$fixture/make.pid" ]]; then
    kill -KILL "$(cat "$fixture/make.pid")" 2>/dev/null || true
  fi
  rm -rf "$fixture"
}
trap cleanup EXIT

stubs="$fixture/bin"
gate_tmp="$fixture/tmp"
mkdir -p "$stubs" "$gate_tmp"
printf '#!/bin/sh\nexit 0\n' >"$stubs/cargo"
printf '#!/bin/sh\nexit 0\n' >"$stubs/rustup"
# STUB_MAKE_MODE: pass | fail (2000-line burst, then exit 3) | hang (records
# its pid, then sleeps until killed).
cat >"$stubs/make" <<'STUB'
#!/bin/sh
case "$STUB_MAKE_MODE" in
  pass)
    echo "  tsc ✓"
    exit 0
    ;;
  fail)
    echo "first line"
    i=1
    while [ "$i" -le 2000 ]; do
      echo "burst $i"
      i=$((i + 1))
    done
    echo "LAST LINE before exit"
    exit 3
    ;;
  hang)
    echo $$ >"$STUB_MAKE_PIDFILE"
    echo "hanging"
    exec sleep 300
    ;;
esac
exit 99
STUB
chmod +x "$stubs/cargo" "$stubs/rustup" "$stubs/make"

# exec: in `run_gate ... &` the job's pid must be the gate itself, not a
# subshell around it, or a signal to it never reaches the gate.
run_gate() {
  exec env PATH="$stubs:$PATH" TMPDIR="$gate_tmp" "$root/scripts/check-gate.sh"
}

# Catches: the gate reporting anything but make's own status, or losing the
# output make wrote just before it exited (the old `sleep 1; kill tail` race).
status=0
output="$(STUB_MAKE_MODE=fail run_gate 2>&1)" || status=$?
test "$status" -eq 3 || { echo "gate exited $status, make exited 3" >&2; exit 1; }
for want in "first line" "burst 2000" "LAST LINE before exit" "make check exited 3"; do
  grep -qF "$want" <<<"$output" || { echo "gate output lacks '$want'" >&2; exit 1; }
done
bursts="$(grep -cE '^burst [0-9]+$' <<<"$output" || true)"
test "$bursts" -eq 2000 || { echo "gate printed $bursts of 2000 burst lines (lost or duplicated)" >&2; exit 1; }
kept="$(sed -n 's/^Full log kept at: //p' <<<"$output")"
case "$kept" in
  "$gate_tmp"/tuic-check-gate.*) ;;
  *) echo "failure log '$kept' is not under the gate's TMPDIR $gate_tmp" >&2; exit 1 ;;
esac
test -f "$kept" || { echo "failure log $kept was not kept" >&2; exit 1; }
rm -f "$kept"

# Catches: a passing run leaving its log behind or misreporting success.
status=0
output="$(STUB_MAKE_MODE=pass run_gate 2>&1)" || status=$?
test "$status" -eq 0 || { echo "gate exited $status for a passing make" >&2; exit 1; }
grep -qF "All checks passed." <<<"$output" || { echo "gate did not report success" >&2; exit 1; }
grep -qF "tsc ✓" <<<"$output" || { echo "gate lost make's output on success" >&2; exit 1; }
if [ -n "$(find "$gate_tmp" -mindepth 1 -print -quit)" ]; then
  echo "a passing gate left files in its TMPDIR:" >&2
  ls -la "$gate_tmp" >&2
  exit 1
fi

# Catches: an interrupted or terminated gate leaving make (and the whole
# cargo run under it) running in the background.
wait_for_file() {
  local tries=0
  until [[ -s "$1" ]]; do
    tries=$((tries + 1))
    if ((tries > 200)); then
      echo "timed out waiting for $1" >&2
      return 1
    fi
    sleep 0.05
  done
}
wait_gone() {
  local tries=0
  while kill -0 "$1" 2>/dev/null; do
    tries=$((tries + 1))
    if ((tries > 200)); then
      return 1
    fi
    sleep 0.05
  done
}
for signal in TERM INT; do
  rm -f "$fixture/make.pid"
  # Job control on: a background job of a non-interactive shell ignores SIGINT,
  # and an ignored-at-entry signal cannot be trapped by the gate.
  set -m
  STUB_MAKE_MODE=hang STUB_MAKE_PIDFILE="$fixture/make.pid" run_gate >"$fixture/gate.out" 2>&1 &
  gate_pid=$!
  set +m
  wait_for_file "$fixture/make.pid"
  make_pid="$(cat "$fixture/make.pid")"
  kill "-$signal" "$gate_pid"
  status=0
  wait "$gate_pid" || status=$?
  case "$signal" in
    TERM) want=143 ;;
    INT) want=130 ;;
  esac
  test "$status" -eq "$want" || { echo "SIG$signal: gate exited $status, expected $want" >&2; cat "$fixture/gate.out" >&2; exit 1; }
  wait_gone "$make_pid" || { echo "SIG$signal: make ($make_pid) still running after the gate exited" >&2; cat "$fixture/gate.out" >&2; exit 1; }
  grep -qF "log kept at" "$fixture/gate.out" || { echo "SIG$signal: gate did not say where the log is" >&2; exit 1; }
  grep -qF "hanging" "$fixture/gate.out" || { echo "SIG$signal: gate did not drain make's output" >&2; exit 1; }
done
rm -f "$fixture/make.pid"
echo "check-gate.sh stub-make tests passed"
