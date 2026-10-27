#!/usr/bin/env bash
#
# setup-worktree.sh — prepare a fresh checkout or linked worktree so the full test
# suite and scripts/check-gate.sh can run. Idempotent: safe to run again.
#
# Why each step exists is documented in src-tauri/AGENTS.md ("Fresh Worktree
# Setup") and docs/guides/development-setup.md ("Setting up a fresh worktree").
#
# Usage: scripts/setup-worktree.sh [options]
#   --skip-install      do not run pnpm install
#   --skip-submodule    do not initialise the plugins submodule
#   --skip-precompile   do not pre-compile the Rust test binaries (the slow step)
#   -h, --help          show this help
#
# Run it from a TUICommander terminal or any other: the cargo steps go through
# scripts/with-clean-tuic-env.sh so ambient TUIC_* variables never reach them.
#
# This script deliberately does NOT run `make dev` or `pnpm build:sidecar`: the
# empty sidecar placeholders below would then be rejected as stubs.
set -euo pipefail

skip_install=0
skip_submodule=0
skip_precompile=0
for arg in "$@"; do
	case "${arg}" in
	--skip-install) skip_install=1 ;;
	--skip-submodule) skip_submodule=1 ;;
	--skip-precompile) skip_precompile=1 ;;
	-h | --help)
		sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "setup-worktree: unknown option: ${arg}" >&2
		exit 2
		;;
	esac
done

step() { printf '==> %s\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
die() {
	printf 'ERROR: %s\n' "$*" >&2
	exit 1
}

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"
clean_env="${repo_root}/scripts/with-clean-tuic-env.sh"

# ---------------------------------------------------------------------------
# 1. Tools. Fail now, not minutes into a build.
# ---------------------------------------------------------------------------
step "Checking required tools"
missing=()
for tool in git node pnpm cargo rustc; do
	command -v "${tool}" >/dev/null 2>&1 || missing+=("${tool}")
done
if command -v cargo >/dev/null 2>&1; then
	cargo nextest --version >/dev/null 2>&1 || missing+=("cargo-nextest (cargo install cargo-nextest)")
	cargo audit --version >/dev/null 2>&1 || missing+=("cargo-audit (cargo install cargo-audit)")
	cargo fmt --version >/dev/null 2>&1 || missing+=("rustfmt (rustup component add rustfmt)")
	cargo clippy --version >/dev/null 2>&1 || missing+=("clippy (rustup component add clippy)")
fi
if [ "${#missing[@]}" -gt 0 ]; then
	printf 'ERROR: missing required tools:\n' >&2
	printf '  - %s\n' "${missing[@]}" >&2
	exit 1
fi

wanted_pnpm="$(node -p "require('./package.json').packageManager || ''" 2>/dev/null || true)"
if [ -n "${wanted_pnpm}" ] && [ "${wanted_pnpm}" != "pnpm@$(pnpm --version)" ]; then
	warn "package.json wants ${wanted_pnpm}, found pnpm@$(pnpm --version)"
fi

# ---------------------------------------------------------------------------
# 2. Frontend dependencies.
#    --frozen-lockfile: never rewrite pnpm-lock.yaml in a fresh branch, and fail
#    loudly if it disagrees with package.json / pnpm-workspace.yaml (as CI does).
#    --offline first: link from the shared store, no network; fall back online
#    when a package the branch added is not in the store yet.
#    Layout pinned to the local .pnpm one the main checkout uses: a session that
#    exports pnpm_config_enable_global_virtual_store=true otherwise converts
#    node_modules, and pnpm then wants a TTY to purge it.
# ---------------------------------------------------------------------------
if [ "${skip_install}" -eq 0 ]; then
	step "Installing frontend dependencies (pnpm, frozen lockfile)"
	# One attempt. A layout mismatch is not a network problem, so it ends the
	# script at once instead of being retried online.
	pnpm_install() {
		local out rc
		set +e
		out="$(env pnpm_config_enable_global_virtual_store=false pnpm install "$@" 2>&1)"
		rc=$?
		set -e
		printf '%s\n' "${out}"
		if [ "${rc}" -ne 0 ] && grep -q 'ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY' <<<"${out}"; then
			die "node_modules was built with a different pnpm layout and pnpm needs a TTY to purge it.
       Run this once in a real terminal and answer the purge prompt:
         pnpm_config_enable_global_virtual_store=false pnpm install --frozen-lockfile"
		fi
		return "${rc}"
	}
	if ! pnpm_install --offline --frozen-lockfile; then
		warn "offline install failed (a package missing from the store?); retrying online"
		pnpm_install --frozen-lockfile || die "pnpm install failed (see the output above)"
	fi
else
	step "Skipping pnpm install (--skip-install)"
fi

# ---------------------------------------------------------------------------
# 3. Stubs the Rust build needs. Only create what is absent: never replace a
#    real frontend build or a real sidecar with a stub.
# ---------------------------------------------------------------------------
step "Creating dist/ stubs (include_dir! embeds dist/ at compile time)"
mkdir -p dist
for page in index.html mobile.html; do
	# mobile.html too: the SPA fallback maps /mobile/* to it specifically.
	[ -e "dist/${page}" ] || printf '<html></html>\n' >"dist/${page}"
done

host_tuple="$(rustc --print host-tuple 2>/dev/null || rustc -vV | sed -n 's/^host: //p')"
[ -n "${host_tuple}" ] || die "could not determine the host target triple from rustc"
step "Creating sidecar placeholders for ${host_tuple} (build script only checks they exist)"
mkdir -p src-tauri/binaries
for sidecar in tuic-bridge tuic tuic-hook; do
	placeholder="src-tauri/binaries/${sidecar}-${host_tuple}"
	[ -e "${placeholder}" ] || : >"${placeholder}"
	chmod +x "${placeholder}"
done

# ---------------------------------------------------------------------------
# 4. A real tuic-hook. The golden_wire_output tests execute it, so a placeholder
#    fails them with `!text.is_empty()`. The binary can read as 0 bytes right
#    after a successful build (and a toolchain bump invalidates it): rebuild once.
# ---------------------------------------------------------------------------
hook_bin="${CARGO_TARGET_DIR:-src-tauri/target}/debug/tuic-hook"
build_hook() { (cd src-tauri && "${clean_env}" cargo build --package tuic-hook); }
step "Building tuic-hook"
build_hook
if [ ! -s "${hook_bin}" ]; then
	warn "${hook_bin} is 0 bytes right after a successful build; rebuilding once"
	build_hook
fi
[ -s "${hook_bin}" ] || die "${hook_bin} is still empty after a rebuild"

# ---------------------------------------------------------------------------
# 5. Plugins submodule. Uninitialised, `pnpm test:plugins` collects 0 tests and
#    exits 1. Init can fail where SSH is blocked (the agent sandbox): warn and
#    carry on rather than hanging on a prompt or retrying.
# ---------------------------------------------------------------------------
if [ "${skip_submodule}" -eq 0 ]; then
	step "Checking the plugins submodule"
	if git submodule status plugins >/dev/null 2>&1; then
		case "$(git submodule status plugins)" in
		-*)
			if ! GIT_TERMINAL_PROMPT=0 GIT_SSH_COMMAND="ssh -o BatchMode=yes -o ConnectTimeout=15" \
				git submodule update --init plugins; then
				warn "could not initialise plugins (SSH blocked?); not retrying"
			fi
			;;
		esac
		recorded="$(git ls-tree HEAD plugins | awk '{print $3}')"
		actual="$(git -C plugins rev-parse HEAD 2>/dev/null || true)"
		if [ -z "${actual}" ]; then
			warn "plugins is not initialised: pnpm test:plugins will collect 0 tests and exit 1"
		elif [ "${actual}" != "${recorded}" ]; then
			warn "plugins is at ${actual:0:9} but this branch records ${recorded:0:9}: drift, not a regression"
		fi
	else
		warn "no plugins submodule in this checkout; skipping"
	fi
else
	step "Skipping the plugins submodule (--skip-submodule)"
fi

# ---------------------------------------------------------------------------
# 6. Pre-compile the Rust test binaries (optional, slow). Without warming there
#    is no cloned target/, so the first nextest run is otherwise a from-scratch
#    build. It MUST be built the way `make check` builds it: through
#    with-test-tmp.sh with TUIC_TEST_FAKE_HOME=1. A plain `cargo nextest run
#    --no-run` compiles the same crates with a different environment, and cargo
#    keeps one fingerprint per crate: measured here, 79 crates (the C-dependency
#    build scripts) rebuilt in 56s when the two invocations alternated, and 0 when
#    the same one repeated.
# ---------------------------------------------------------------------------
if [ "${skip_precompile}" -eq 0 ]; then
	step "Pre-compiling Rust test binaries (cargo nextest run --workspace --no-run)"
	(
		cd src-tauri
		ulimit -n 10240 2>/dev/null || true
		TUIC_TEST_FAKE_HOME=1 "${clean_env}" ../scripts/with-test-tmp.sh \
			cargo nextest run --workspace --no-run
	)
else
	step "Skipping the Rust pre-compile (--skip-precompile)"
fi

step "Done. Run the full gate with:"
cat <<EOF
      scripts/with-clean-tuic-env.sh ./scripts/check-gate.sh > "\${TMPDIR:-.}/gate.log" 2>&1; echo "EXIT_CODE=\$?" >> "\${TMPDIR:-.}/gate.log"
EOF
