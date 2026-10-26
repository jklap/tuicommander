#!/usr/bin/env bats
# Unit + end-to-end tests for ./tuic (the tuic/tmux bundle-resolving shim).
#
# Hermetic: no network, no real TUICommander, no real `ps`. HOME points at a
# throwaway dir, `ps` is a fake first on PATH, and fake .app bundles are built
# with PlistBuddy. A bundle's Contents/MacOS/tuic is a symlink to /bin/sh, so
# `tuic -c 'echo "$0"'` reveals the argv0 the shim exec'd it with.
#
# Run: bats tuic.bats

SCRIPT="${BATS_TEST_DIRNAME}/tuic"

setup() {
  export HOME="${BATS_TEST_TMPDIR}/home"
  mkdir -p "${HOME}/Applications" "${BATS_TEST_TMPDIR}/bin"
  export PATH="${BATS_TEST_TMPDIR}/bin:${PATH}"
  unset TUIC_APP_VERSION FAKE_PS_MODE FAKE_PS_TREE FAKE_PS_PROCS

  # Fake ps. FAKE_PS_MODE: ok (default) | blocked (always fails, like the
  # sandbox) | midfail (the pid probe works, every other call fails).
  # FAKE_PS_TREE: "pid ppid comm" lines for `-o comm=/ppid= -p PID` lookups.
  # FAKE_PS_PROCS: "pid comm" lines returned by `ps -axo pid=,comm=`.
  cat > "${BATS_TEST_TMPDIR}/bin/ps" <<'EOF'
#!/usr/bin/env bash
mode="${FAKE_PS_MODE:-ok}"
[ "${mode}" = "blocked" ] && { echo "ps: Operation not permitted" >&2; exit 1; }
case "$*" in
  *"-o pid= -p"*) echo 1 ;;
  *"-axo"*)
    [ "${mode}" = "midfail" ] && exit 1
    printf '%s\n' "${FAKE_PS_PROCS:-}" ;;
  *"-o comm= -p"*)
    [ "${mode}" = "midfail" ] && exit 1
    pid="${*: -1}"
    printf '%s\n' "${FAKE_PS_TREE:-}" | awk -v p="${pid}" '$1==p{$1="";$2="";sub(/^ +/,"");print}' ;;
  *"-o ppid= -p"*)
    [ "${mode}" = "midfail" ] && exit 1
    pid="${*: -1}"
    printf '%s\n' "${FAKE_PS_TREE:-}" | awk -v p="${pid}" '$1==p{print $2}' ;;
esac
EOF
  chmod +x "${BATS_TEST_TMPDIR}/bin/ps"

  # Driver: source the script, optionally probe ps, run one function, then
  # dump the globals so tests can assert on state as well as the return code.
  cat > "${BATS_TEST_TMPDIR}/drive.sh" <<EOF
source "${SCRIPT}"
invoked_as="\${INVOKED_AS:-tuic}"
if [ "\${DRIVE_PROBE:-1}" = 1 ]; then probe_ps; fi
rc=0; "\$@" || rc=\$?
echo "rc=\${rc}"
echo "bundle=\${bundle}"
echo "ps_available=\${ps_available}"
echo "none_running=\${none_running}"
printf 'ambiguous=%s\n' "\${ambiguous//\$'\n'/|}"
i=0; while [ "\${i}" -lt "\${#notes[@]}" ]; do echo "note=\${notes[\$i]}"; i=\$((i+1)); done
EOF
}

# make_bundle <path> <bundle-id> <version>: fake .app whose tuic -> /bin/sh.
make_bundle() {
  local path="$1" id="$2" ver="$3"
  mkdir -p "${path}/Contents/MacOS"
  /usr/libexec/PlistBuddy \
    -c "Add :CFBundleIdentifier string ${id}" \
    -c "Add :CFBundleShortVersionString string ${ver}" \
    "${path}/Contents/Info.plist" >/dev/null
  ln -s /bin/sh "${path}/Contents/MacOS/tuic"
}

app() { make_bundle "${HOME}/Applications/$1.app" "${2:-com.tuic.commander}" "${3:-9.9.9}"; }
bin_of() { echo "${HOME}/Applications/$1.app/Contents/MacOS/tuicommander"; }
drive() { run bash "${BATS_TEST_TMPDIR}/drive.sh" "$@"; }

# ---------------------------------------------------------------- is_tuicommander_bundle

@test "is_tuicommander_bundle: true for the TUICommander bundle id" {
  app Good
  drive is_tuicommander_bundle "${HOME}/Applications/Good.app"
  [[ "${output}" == *"rc=0"* ]]
}

@test "is_tuicommander_bundle: false for another app's bundle id" {
  app Other com.example.other
  drive is_tuicommander_bundle "${HOME}/Applications/Other.app"
  [[ "${output}" == *"rc=1"* ]]
}

@test "is_tuicommander_bundle: false when Info.plist is missing" {
  mkdir -p "${HOME}/Applications/Empty.app/Contents"
  drive is_tuicommander_bundle "${HOME}/Applications/Empty.app"
  [[ "${output}" == *"rc=1"* ]]
}

@test "is_tuicommander_bundle: false when Info.plist is unreadable garbage" {
  mkdir -p "${HOME}/Applications/Bad.app/Contents"
  echo "not a plist" > "${HOME}/Applications/Bad.app/Contents/Info.plist"
  drive is_tuicommander_bundle "${HOME}/Applications/Bad.app"
  [[ "${output}" == *"rc=1"* ]]
}

@test "is_tuicommander_bundle: a renamed bundle still matches (identity is the plist, not the name)" {
  app TUICommander_renamed_copy
  drive is_tuicommander_bundle "${HOME}/Applications/TUICommander_renamed_copy.app"
  [[ "${output}" == *"rc=0"* ]]
}

# ---------------------------------------------------------------- probe_ps

@test "probe_ps: ps_available stays 1 when ps works" {
  DRIVE_PROBE=0 drive probe_ps
  [[ "${output}" == *"ps_available=1"* ]]
}

@test "probe_ps: ps_available becomes 0 when ps is blocked" {
  FAKE_PS_MODE=blocked DRIVE_PROBE=0 drive probe_ps
  [[ "${output}" == *"ps_available=0"* ]]
}

@test "probe_ps: ps's own error text does not leak to the terminal" {
  FAKE_PS_MODE=blocked DRIVE_PROBE=0 drive probe_ps
  [[ "${output}" != *"Operation not permitted"* ]]
}

# ---------------------------------------------------------------- bundle_from_ancestors

@test "ancestors: finds a TUICommander ancestor several levels up" {
  app Real
  export FAKE_PS_TREE="500 400 zsh
400 300 login
300 1 $(bin_of Real)"
  drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/Real.app"* ]]
}

@test "ancestors: skips a */Contents/MacOS/* process whose bundle id is not TUICommander" {
  app Imposter com.example.other
  export FAKE_PS_TREE="500 400 zsh
400 1 $(bin_of Imposter)"
  drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"note=this shell is not a descendant"* ]]
}

@test "ancestors: no TUICommander in the chain records a 'not a descendant' note" {
  export FAKE_PS_TREE="500 400 zsh
400 1 launchd"
  drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"not a descendant of any TUICommander.app"* ]]
  [[ "${output}" == *"none_running=0"* ]]
}

@test "ancestors: ps blocked is reported as inconclusive, not as 'not a descendant'" {
  FAKE_PS_MODE=blocked drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"cannot inspect the process tree"* ]]
  [[ "${output}" != *"not a descendant"* ]]
}

@test "ancestors: ps failing mid-walk is recorded with the pid that failed" {
  FAKE_PS_MODE=midfail drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"'ps' failed for pid 500"* ]]
}

@test "ancestors: stops at pid 1 and pid 0" {
  export FAKE_PS_TREE="500 1 zsh"
  drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=1"* ]]
  drive bundle_from_ancestors 1
  [[ "${output}" == *"rc=1"* ]]
  drive bundle_from_ancestors 0
  [[ "${output}" == *"rc=1"* ]]
}

@test "ancestors: an empty start pid does not loop forever" {
  drive bundle_from_ancestors ""
  [[ "${output}" == *"rc=1"* ]]
}

@test "ancestors: handles a bundle path containing spaces" {
  make_bundle "${HOME}/Applications/My Tuic.app" com.tuic.commander 9.9.9
  export FAKE_PS_TREE="500 1 ${HOME}/Applications/My Tuic.app/Contents/MacOS/tuicommander"
  drive bundle_from_ancestors 500
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/My Tuic.app"* ]]
}

# ---------------------------------------------------------------- bundle_from_env_version

@test "env_version: unset TUIC_APP_VERSION is a noted failure" {
  drive bundle_from_env_version
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"TUIC_APP_VERSION is not set"* ]]
}

@test "env_version: unique version match resolves the bundle" {
  app Only
  TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/Only.app"* ]]
}

@test "env_version: picks the matching version among several side-by-side builds" {
  app Old "" 1.0.0
  app New "" 2.0.0
  app Newer "" 3.0.0
  TUIC_APP_VERSION=2.0.0 drive bundle_from_env_version
  [[ "${output}" == *"bundle=${HOME}/Applications/New.app"* ]]
}

@test "env_version: no installed bundle matches (stale version) is a noted failure" {
  app Only
  TUIC_APP_VERSION=0.0.1 drive bundle_from_env_version
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"matches no installed bundle"* ]]
  [[ "${output}" == *"ambiguous="$'\n'* ]]
}

@test "env_version: ignores non-TUICommander apps that carry the same version" {
  app Imposter com.example.other 9.9.9
  TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"matches no installed bundle"* ]]
}

@test "env_version: ignores a stray .app directory with no Info.plist" {
  mkdir -p "${HOME}/Applications/NotABundle.app"
  TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=1"* ]]
}

@test "env_version: two same-version copies, one canonical TUICommander.app, picks canonical" {
  app One
  app TUICommander
  TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/TUICommander.app"* ]]
}

@test "env_version: two same-version copies with no canonical one is ambiguous" {
  app One
  app Two
  TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"bundle="$'\n'* ]]
  [[ "${output}" == *"One.app"* && "${output}" == *"Two.app"* ]]
  [[ "${output}" == *"note=several installed bundles share version 9.9.9"* ]]
}

@test "env_version: works with ps completely blocked (the sandbox case)" {
  app Only
  FAKE_PS_MODE=blocked TUIC_APP_VERSION=9.9.9 drive bundle_from_env_version
  [[ "${output}" == *"rc=0"* ]]
}

# ---------------------------------------------------------------- bundle_from_running_instances

@test "running: ps blocked is inconclusive and never sets none_running" {
  FAKE_PS_MODE=blocked drive bundle_from_running_instances
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"none_running=0"* ]]
  [[ "${output}" == *"cannot scan running processes"* ]]
}

@test "running: 'ps -ax' failing after a good probe is inconclusive too" {
  FAKE_PS_MODE=midfail drive bundle_from_running_instances
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"none_running=0"* ]]
  [[ "${output}" == *"'ps -ax' failed"* ]]
}

@test "running: a working scan that finds nothing sets none_running=1" {
  export FAKE_PS_PROCS="1 /sbin/launchd
200 /usr/bin/zsh"
  drive bundle_from_running_instances
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"none_running=1"* ]]
}

@test "running: exactly one running bundle resolves" {
  app Real
  export FAKE_PS_PROCS="1 /sbin/launchd
42 $(bin_of Real)"
  drive bundle_from_running_instances
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/Real.app"* ]]
  [[ "${output}" == *"none_running=0"* ]]
}

@test "running: several processes from the SAME bundle count as one" {
  app Real
  export FAKE_PS_PROCS="42 $(bin_of Real)
43 $(bin_of Real)"
  drive bundle_from_running_instances
  [[ "${output}" == *"rc=0"* ]]
}

@test "running: two distinct bundles is ambiguous and lists both" {
  app A
  app B
  export FAKE_PS_PROCS="42 $(bin_of A)
43 $(bin_of B)"
  drive bundle_from_running_instances
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"A.app"* && "${output}" == *"B.app"* ]]
  [[ "${output}" == *"several TUICommander.app instances are running"* ]]
  [[ "${output}" == *"none_running=0"* ]]
}

@test "running: a non-TUICommander .app process is ignored" {
  app Imposter com.example.other
  export FAKE_PS_PROCS="42 $(bin_of Imposter)"
  drive bundle_from_running_instances
  [[ "${output}" == *"none_running=1"* ]]
}

# ---------------------------------------------------------------- resolve_bundle (ordering)

@test "resolve: ancestor beats the env version (it identifies THIS shell's app)" {
  app Old "" 1.0.0
  app New "" 2.0.0
  export FAKE_PS_TREE="500 1 $(bin_of Old)"
  TUIC_APP_VERSION=2.0.0 drive resolve_bundle 500
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/Old.app"* ]]
}

@test "resolve: falls through to env version when ps is blocked" {
  app Only
  FAKE_PS_MODE=blocked TUIC_APP_VERSION=9.9.9 drive resolve_bundle 500
  [[ "${output}" == *"rc=0"* ]]
}

@test "resolve: falls through to the running scan when there is no ancestor and no env" {
  app Real
  export FAKE_PS_TREE="500 1 zsh"
  export FAKE_PS_PROCS="42 $(bin_of Real)"
  drive resolve_bundle 500
  [[ "${output}" == *"rc=0"* ]]
  [[ "${output}" == *"bundle=${HOME}/Applications/Real.app"* ]]
}

@test "resolve: everything blocked leaves none_running=0 and one note per stage" {
  FAKE_PS_MODE=blocked drive resolve_bundle 500
  [[ "${output}" == *"rc=1"* ]]
  [[ "${output}" == *"none_running=0"* ]]
  [[ "${output}" == *"cannot inspect the process tree"* ]]
  [[ "${output}" == *"TUIC_APP_VERSION is not set"* ]]
  [[ "${output}" == *"cannot scan running processes"* ]]
}

# ---------------------------------------------------------------- fail_lookup messages (end-to-end)

@test "fail: ps blocked + no env says 'could not determine', never 'no running'" {
  FAKE_PS_MODE=blocked run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"could not determine which TUICommander.app to use"* ]]
  [[ "${output}" == *"may still be running"* ]]
  [[ "${output}" != *"no running TUICommander.app"* ]]
}

@test "fail: ps blocked + stale env lists the stale version as a reason" {
  FAKE_PS_MODE=blocked TUIC_APP_VERSION=0.0.1 run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"TUIC_APP_VERSION=0.0.1 matches no installed bundle"* ]]
  [[ "${output}" != *"no running TUICommander.app"* ]]
}

@test "fail: a working scan that finds nothing is the ONLY path that says 'no running'" {
  FAKE_PS_PROCS="1 /sbin/launchd" run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"no running TUICommander.app found"* ]]
  [[ "${output}" != *"could not determine"* ]]
}

@test "fail: ps failing mid-way is 'could not determine', not 'no running'" {
  FAKE_PS_MODE=midfail run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"could not determine"* ]]
  [[ "${output}" != *"no running TUICommander.app"* ]]
}

@test "fail: ambiguous installed copies list the candidates and a hint" {
  app One
  app Two
  FAKE_PS_MODE=blocked TUIC_APP_VERSION=9.9.9 run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"ambiguous"* ]]
  [[ "${output}" == *"One.app"* && "${output}" == *"Two.app"* ]]
  [[ "${output}" == *"refusing to guess"* ]]
  [[ "${output}" == *"remove the extra copies"* ]]
  [[ "${output}" != *"no running TUICommander.app"* ]]
}

@test "fail: ambiguity wins even when a working scan finds nothing running" {
  app One
  app Two
  FAKE_PS_PROCS="1 /sbin/launchd" TUIC_APP_VERSION=9.9.9 run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"ambiguous"* ]]
  [[ "${output}" != *"no running TUICommander.app"* ]]
}

@test "fail: two running instances not tied to this shell is ambiguous" {
  app A
  app B
  FAKE_PS_PROCS="42 $(bin_of A)
43 $(bin_of B)" run "${SCRIPT}" --version
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"ambiguous"* ]]
  [[ "${output}" == *"running instances"* ]]
}

@test "fail: messages are prefixed with the invoked name (tmux)" {
  ln -s "${SCRIPT}" "${BATS_TEST_TMPDIR}/bin/tmux"
  FAKE_PS_PROCS="1 /sbin/launchd" run "${BATS_TEST_TMPDIR}/bin/tmux" -V
  [ "${status}" -eq 1 ]
  [[ "${output}" == "tmux: no running TUICommander.app found"* ]]
}

@test "fail_lookup: with zero notes and no flags it still prints the generic message and exits 1" {
  run bash -c "source '${SCRIPT}'; fail_lookup"
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"could not determine"* ]]
}

# ---------------------------------------------------------------- find_executable

@test "find_executable: defaults to Contents/MacOS/tuic" {
  app Real
  bundle="${HOME}/Applications/Real.app"
  run bash -c "source '${SCRIPT}'; bundle='${bundle}'; invoked_as=tuic; find_executable; echo \"\${real_tuic}\""
  [ "${status}" -eq 0 ]
  [ "${output}" = "${bundle}/Contents/MacOS/tuic" ]
}

@test "find_executable: prefers a same-named binary (tuic-bridge) when the bundle ships one" {
  app Real
  printf '#!/bin/sh\necho bridge\n' > "${HOME}/Applications/Real.app/Contents/MacOS/tuic-bridge"
  chmod +x "${HOME}/Applications/Real.app/Contents/MacOS/tuic-bridge"
  run bash -c "source '${SCRIPT}'; bundle='${HOME}/Applications/Real.app'; invoked_as=tuic-bridge; find_executable; echo \"\${real_tuic}\""
  [ "${output}" = "${HOME}/Applications/Real.app/Contents/MacOS/tuic-bridge" ]
}

@test "find_executable: falls back to tuic for tmux when the bundle has no tmux binary" {
  app Real
  run bash -c "source '${SCRIPT}'; bundle='${HOME}/Applications/Real.app'; invoked_as=tmux; find_executable; echo \"\${real_tuic}\""
  [ "${output}" = "${HOME}/Applications/Real.app/Contents/MacOS/tuic" ]
}

@test "find_executable: a non-executable binary exits 1 with a clear error" {
  mkdir -p "${HOME}/Applications/Broken.app/Contents/MacOS"
  : > "${HOME}/Applications/Broken.app/Contents/MacOS/tuic"
  run bash -c "source '${SCRIPT}'; bundle='${HOME}/Applications/Broken.app'; invoked_as=tuic; find_executable"
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"has no executable Contents/MacOS/tuic"* ]]
}

@test "find_executable: a missing binary exits 1 with a clear error" {
  mkdir -p "${HOME}/Applications/Hollow.app/Contents/MacOS"
  run bash -c "source '${SCRIPT}'; bundle='${HOME}/Applications/Hollow.app'; invoked_as=tuic; find_executable"
  [ "${status}" -eq 1 ]
  [[ "${output}" == *"has no executable"* ]]
}

# ---------------------------------------------------------------- end-to-end (main + exec)

@test "e2e: resolves via env version and passes args through" {
  app Only
  TUIC_APP_VERSION=9.9.9 run "${SCRIPT}" -c 'echo "args ok"'
  [ "${status}" -eq 0 ]
  [ "${output}" = "args ok" ]
}

@test "e2e: preserves argv0 as 'tuic'" {
  app Only
  TUIC_APP_VERSION=9.9.9 run "${SCRIPT}" -c 'echo "$0"'
  [ "${output}" = "tuic" ]
}

@test "e2e: invoked through a 'tmux' symlink, argv0 is 'tmux' (tmux-compat dispatch)" {
  app Only
  ln -s "${SCRIPT}" "${BATS_TEST_TMPDIR}/bin/tmux"
  TUIC_APP_VERSION=9.9.9 run "${BATS_TEST_TMPDIR}/bin/tmux" -c 'echo "$0"'
  [ "${status}" -eq 0 ]
  [ "${output}" = "tmux" ]
}

@test "e2e: a real ancestor in the process tree wins over a different env version" {
  app Old "" 1.0.0
  app New "" 2.0.0
  # Make each bundle's tuic report its own name so we can see which one ran.
  for n in Old New; do
    rm "${HOME}/Applications/${n}.app/Contents/MacOS/tuic"
    printf '#!/bin/sh\necho %s\n' "${n}" > "${HOME}/Applications/${n}.app/Contents/MacOS/tuic"
    chmod +x "${HOME}/Applications/${n}.app/Contents/MacOS/tuic"
  done
  # The script's parent is the bash below, so key the fake tree on its pid.
  run bash -c "export FAKE_PS_TREE=\"\$\$ 77 zsh
77 1 $(bin_of Old)\"; TUIC_APP_VERSION=2.0.0 '${SCRIPT}'; exit \$?"
  [ "${status}" -eq 0 ]
  [ "${output}" = "Old" ]
}

@test "e2e: exec failure (bad interpreter) reports and exits 126" {
  mkdir -p "${HOME}/Applications/Bad.app/Contents/MacOS"
  /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string com.tuic.commander" \
    -c "Add :CFBundleShortVersionString string 9.9.9" \
    "${HOME}/Applications/Bad.app/Contents/Info.plist" >/dev/null
  printf '#!/nonexistent/interpreter\n' > "${HOME}/Applications/Bad.app/Contents/MacOS/tuic"
  chmod +x "${HOME}/Applications/Bad.app/Contents/MacOS/tuic"
  TUIC_APP_VERSION=9.9.9 run "${SCRIPT}" --version
  [ "${status}" -eq 126 ]
  [[ "${output}" == *"failed to exec"* ]]
}

@test "e2e: sourcing the script does not run main or exec anything" {
  run bash -c "source '${SCRIPT}'; echo sourced-ok"
  [ "${status}" -eq 0 ]
  [ "${output}" = "sourced-ok" ]
}
