#!/usr/bin/env bash
# scripts/test-tmp-lib.sh helpers that scripts/with-test-tmp.sh and the fixture
# runners rely on: tuic_unix_socket_path_max fails loudly (never a default)
# when it cannot read the one budget definition, and
# tuic_pin_compilers_for_sccache names CC/CXX on macOS with sccache only.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-lib-fns.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
lib="$root/scripts/test-tmp-lib.sh"

# --- tuic_unix_socket_path_max -------------------------------------------
# The real tree: the budget the rest of the harness pins.
got="$(. "$lib" && tuic_unix_socket_path_max "$root")"
test "$got" = 103 || { echo "UNIX_SOCKET_PATH_MAX read as '$got' from the real tree" >&2; exit 1; }

# A fake tree with a given tuic-ipc lib.rs body (or none at all).
fake_tree() {
  local tree="$fixture/$1"
  mkdir -p "$tree/src-tauri/crates/tuic-ipc/src"
  if [[ $# -gt 1 ]]; then
    printf '%s\n' "$2" >"$tree/src-tauri/crates/tuic-ipc/src/lib.rs"
  fi
  printf '%s\n' "$tree"
}
expect_parse_failure() {
  local what=$1 tree=$2 out status=0
  out="$(. "$lib" && tuic_unix_socket_path_max "$tree" 2>"$fixture/err")" || status=$?
  test "$status" -ne 0 || { echo "$what: parser succeeded with '$out' instead of failing" >&2; exit 1; }
  test -z "$out" || { echo "$what: parser printed '$out' on failure" >&2; exit 1; }
  grep -qF "cannot read UNIX_SOCKET_PATH_MAX" "$fixture/err" \
    || { echo "$what: no diagnostic on stderr:" >&2; cat "$fixture/err" >&2; exit 1; }
}
# Catches: a default value standing in for an unreadable definition.
expect_parse_failure "missing lib.rs" "$(fake_tree missing)"
expect_parse_failure "no constant" "$(fake_tree absent 'pub const OTHER: usize = 5;')"
expect_parse_failure "renamed type" "$(fake_tree retyped 'pub const UNIX_SOCKET_PATH_MAX: u32 = 103;')"
expect_parse_failure "non-numeric value" "$(fake_tree symbolic 'pub const UNIX_SOCKET_PATH_MAX: usize = SUN_LEN - 1;')"
# A well-formed line is read exactly, so the pin above is not a coincidence.
got="$(. "$lib" && tuic_unix_socket_path_max "$(fake_tree ok 'pub const UNIX_SOCKET_PATH_MAX: usize = 91;')")"
test "$got" = 91 || { echo "well-formed fake definition read as '$got', expected 91" >&2; exit 1; }

# --- tuic_pin_compilers_for_sccache ---------------------------------------
# PATH holds only stubs, so `uname` and `sccache` are exactly what each case
# says (the real PATH of a developer's Mac has sccache).
stub_dir() {
  local dir="$fixture/path-$1-$2"
  mkdir -p "$dir"
  printf '#!/bin/sh\necho %s\n' "$1" >"$dir/uname"
  chmod +x "$dir/uname"
  if [[ "$2" == sccache ]]; then
    printf '#!/bin/sh\nexit 0\n' >"$dir/sccache"
    chmod +x "$dir/sccache"
  fi
  printf '%s\n' "$dir"
}
pinned() { # <uname> <sccache|none> [CC value or empty]
  local dir
  dir="$(stub_dir "$1" "$2")"
  env -u CC -u CXX ${3:+CC="$3"} PATH="$dir" /bin/sh -c \
    '. "$1" && tuic_pin_compilers_for_sccache && printf "%s|%s" "${CC-<unset>}" "${CXX-<unset>}"' \
    sh "$lib"
}
# Catches: CC/CXX forced on Linux or without sccache (a rebuild of every
# native dep against an unwrapped run, and CI's compiler choice overridden).
test "$(pinned Darwin sccache)" = "cc|c++" || { echo "macOS+sccache: $(pinned Darwin sccache), expected cc|c++" >&2; exit 1; }
test "$(pinned Darwin none)" = "<unset>|<unset>" || { echo "macOS without sccache pinned: $(pinned Darwin none)" >&2; exit 1; }
test "$(pinned Linux sccache)" = "<unset>|<unset>" || { echo "Linux+sccache pinned: $(pinned Linux sccache)" >&2; exit 1; }
# Catches: a caller's own compiler being replaced.
test "$(pinned Darwin sccache clang-17)" = "clang-17|c++" \
  || { echo "explicit CC replaced: $(pinned Darwin sccache clang-17)" >&2; exit 1; }

# End to end: the wrapper agrees with the helper on this machine.
expected="$(env -u CC -u CXX /bin/sh -c '. "$1" && tuic_pin_compilers_for_sccache && printf "%s|%s" "${CC-<unset>}" "${CXX-<unset>}"' sh "$lib")"
seen="$(env -u CC -u CXX "$root/scripts/with-test-tmp.sh" sh -c 'printf "%s|%s" "${CC-<unset>}" "${CXX-<unset>}"')"
test "$seen" = "$expected" || { echo "with-test-tmp.sh CC/CXX '$seen' != helper's '$expected'" >&2; exit 1; }
