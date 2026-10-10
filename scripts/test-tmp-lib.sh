# shellcheck shell=sh
# Test scratch roots for every shell entry point of the test harness
# (scripts/with-test-tmp.sh, src-tauri/scripts/nextest-test-tmp.sh,
# scripts/mutants.sh, the shell tests). POSIX sh, sourced, defines functions
# only. Same order as tuic-test-support's host_temp_dir()/test_base()/
# test_temp_root(); never reads $HOME and never defaults into a checkout.
#
#   host temp  TUIC_TEST_HOST_TMPDIR (exported once, before anything points
#              TMPDIR at a test root, so a default can never nest), else
#              TMPDIR, else the OS default temp dir
#   base       TUIC_TEST_TMP_BASE (explicit opt-in), else <host temp>/tuic-tests
#   root       TUIC_TEST_TMP_ROOT (inherited: the caller owns it), else
#              <base>/tuic-co-<checkout hash>

tuic_test_host_tmpdir() {
  _tuic_host=${TUIC_TEST_HOST_TMPDIR:-${TMPDIR:-}}
  if [ -z "$_tuic_host" ]; then
    # What Rust's std::env::temp_dir() answers on macOS without TMPDIR.
    _tuic_host=$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || true)
  fi
  if [ -z "$_tuic_host" ]; then
    _tuic_probe=$(mktemp -d)
    _tuic_host=$(dirname "$_tuic_probe")
    rmdir "$_tuic_probe"
  fi
  while [ "${_tuic_host%/}" != "$_tuic_host" ]; do _tuic_host=${_tuic_host%/}; done
  printf '%s\n' "${_tuic_host:-/}"
}

tuic_test_tmp_base() {
  _tuic_base=${TUIC_TEST_TMP_BASE:-$(tuic_test_host_tmpdir)/tuic-tests}
  printf '%s\n' "${_tuic_base%/}"
}

# FNV-1a 64 of a checkout's physical path: the same 16 hex digits as
# tuic-test-support's checkout_hash(), so every entry point picks one dir.
tuic_checkout_hash() {
  _tuic_hash=-3750763034362895579 # 0xcbf29ce484222325 as a signed 64-bit value
  for _tuic_byte in $(printf '%s' "$1" | od -An -v -tu1); do
    _tuic_hash=$(((_tuic_hash ^ _tuic_byte) * 1099511628211))
  done
  printf '%016x\n' "$_tuic_hash"
}

# Usage: tuic_test_tmp_root <dir inside the checkout>. Creates and prints it.
tuic_test_tmp_root() {
  if [ -n "${TUIC_TEST_TMP_ROOT:-}" ]; then
    _tuic_root=${TUIC_TEST_TMP_ROOT%/}
  else
    _tuic_checkout=$(cd "$(git -C "$1" rev-parse --show-toplevel)" && pwd -P)
    _tuic_root="$(tuic_test_tmp_base)/tuic-co-$(tuic_checkout_hash "$_tuic_checkout")"
  fi
  mkdir -p "$_tuic_root"
  printf '%s\n' "$_tuic_root"
}
