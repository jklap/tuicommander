# TUICommander Makefile
# Builds, signs, and packages the Tauri app for macOS distribution

APP_NAME=TUICommander
BINARY_NAME=tuicommander
BUNDLE_ID=com.tuic.commander
VERSION=$(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")

# Optional runtime override used by isolated verification sessions. An empty
# value preserves the production default.
TUIC_PORT?=

# Per-checkout config instance (#763-d219). `make dev` and `make test` both run
# on their OWN `instances/<id>/` config directory (repositories.json,
# agents.json, disabled_mcp_agents, ...), never the shared default one that
# Boss's real, separately-launched TUICommander.app uses — that release build is
# never started through this Makefile (see "Test instance vs orchestrator
# instance" in AGENTS.md).
#
# The id is derived from the checkout's own directory name rather than a shared
# literal like "tuic-test", so two worktrees running `make dev`/`make test` at
# the same time — the normal multi-agent shape in this repo — get separate
# instances instead of colliding on one repositories.json. It is a valid
# instance id by construction: lowercase, non-alphanumerics folded to single
# hyphens, no leading/trailing hyphen, `tuic-` + at most 58 characters = at
# most 63 (docs/backend/config.md). A directory name with no usable character
# falls back to `tuic-checkout`.
#
# Deliberately ONE global default for both targets: they are the same kind of
# launch now, so there is no target that must stay on the shared directory.
# Overrides: `make dev TUIC_APP_INSTANCE=<id>` / `TUIC_APP_INSTANCE=<id> make
# dev` pick another instance; an EMPTY value (`make dev TUIC_APP_INSTANCE=` or
# `TUIC_APP_INSTANCE= make dev`) opts back into the shared default config for
# one run and prints a loud warning. `scripts/check-make-instance-scope.sh`
# asks `make -n` what each target really expands (pre-commit + `make check`).
TUIC_CHECKOUT_INSTANCE:=$(shell basename "$(CURDIR)" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9' '-' | sed -E 's/-+/-/g; s/^-//' | cut -c1-58 | sed -E 's/-$$//')
TUIC_APP_INSTANCE?=tuic-$(or $(TUIC_CHECKOUT_INSTANCE),checkout)

# Expands to a recipe line ONLY when this run is on the shared default config,
# so `make -n` shows the warning exactly when it would fire.
WARN_SHARED_INSTANCE=$(if $(TUIC_APP_INSTANCE),,@echo "WARNING: TUIC_APP_INSTANCE is empty for this run - using the SHARED default config directory, the same one Boss's real TUICommander.app uses (repositories.json, agents.json, ...). Only do this on purpose." >&2)

# rtk (Rust Token Killer) is an optional output-compacting proxy: `rtk <cmd>`
# runs <cmd> and trims its output. It is a personal tool, not a project
# dependency, so every use below degrades to the raw command when it is absent.
# Force the raw commands with `make RTK= check`.
RTK?=$(shell command -v rtk)
# `rtk err <cmd>` keeps only <cmd>'s errors; with no rtk, run <cmd> unfiltered.
RTK_ERR=$(if $(RTK),$(RTK) err,)

# Code signing identity: override with SIGN_IDENTITY env var.
# Auto-detection order: Developer ID > ad-hoc.
SIGN_IDENTITY?=-

# Tauri build output
TAURI_TARGET=src-tauri/target/release/bundle/macos
APP_BUNDLE=$(TAURI_TARGET)/$(APP_NAME).app

# whisper-rs-sys bundles whisper.cpp which uses std::filesystem::path (macOS 10.15+).
# Must be exported so CMake subprocesses inherit it (.cargo/config.toml [env] alone is insufficient).
export MACOSX_DEPLOYMENT_TARGET ?= 10.15

# Distribution output
DIST_DIR=dist-release

# Updater artifacts (.sig/.tar.gz for auto-update) require TAURI_SIGNING_PRIVATE_KEY.
# CI/release builds set it; local/dev builds don't, so skip updater artifact
# generation in that case rather than failing the whole build.
TAURI_BUILD_FLAGS=$(if $(TAURI_SIGNING_PRIVATE_KEY),,--config '{"bundle":{"createUpdaterArtifacts":false}}')

.PHONY: all clean dev test test-shell build build-dmg check check-gate cov crap fmt sign verify-sign notarize release dist \
       nightly github-release preview bump release-notes hooks docs docs-serve \
       gh-debug-on gh-debug-off gh-debug-status gh-debug-logs gh-rate logs

all: build sign

# Directory searched by test-shell. Override for the runner's isolated fixture.
SHELL_TEST_DIR ?= scripts

# Run every repository shell test. New scripts named test-*.sh below scripts/
# join this target automatically; local scripts git ignores do not.
test-shell:
	@SHELL_TEST_DIR="$(SHELL_TEST_DIR)" scripts/with-test-tmp.sh bash -c 'set -euo pipefail; found=0; while IFS= read -r script; do git check-ignore -q "$$script" 2>/dev/null && continue; found=1; echo "shell test: $$script"; bash "$$script"; done < <(find "$$SHELL_TEST_DIR" -type f -name "test-*.sh" -print | LC_ALL=C sort); [ "$$found" -eq 1 ] || { echo "no shell tests found under $$SHELL_TEST_DIR" >&2; exit 1; }'

# Install tracked git hooks. Idempotent.
#   pre-commit — Makefile TUIC_APP_INSTANCE scope (bypass: --no-verify) +
#                agent-state fixture gate (bypass: TUIC_SKIP_FIXTURE_GATE=1)
#   pre-push   — GitHub-issue closing-keyword guard (bypass: TUIC_SKIP_ISSUE_CHECK=1)
hooks:
	@bash scripts/hooks/install-hooks.sh

# Run in development mode with frontend-only hot reload (debug tracing for our code only).
# Pre-builds sidecars required by Tauri and the frontend served from dist/.
# `--no-watch` disables the Tauri CLI's Rust file watcher: editing `src-tauri/**`
# (or its `.rs.tmp.*` scratch files) will NOT rebuild/restart the Rust backend.
# Vite HMR still reloads the UI (it runs as a separate `beforeDevCommand` process).
# Rust changes require a manual `make dev` restart — see src-tauri/AGENTS.md "Dev Hot Reload".
# Isolated per-checkout config instance by default — see the
# `TUIC_APP_INSTANCE` comment at the top. An inherited `TUIC_APP_INSTANCE` (a
# dotfile, direnv, or an exported `TUIC_APP_INSTANCE=` still in the shell)
# beats the Makefile default, and the guard in
# `scripts/check-make-instance-scope.sh` deliberately cannot see it. Say which
# config directory this is starting on, so the wrong one is the first line of
# output instead of something inferred from an empty repository list.
dev: hooks
	$(WARN_SHARED_INSTANCE)
	@pnpm build:sidecar
	@pnpm exec vite build
	@cd src-tauri && cargo build --bin tuic-remote --no-default-features
	@cd src-tauri && cargo build -p tuic-bridge -p tuic-cli
	@echo "Starting Tauri dev on $(if $(TUIC_APP_INSTANCE),the ISOLATED config instance '$(TUIC_APP_INSTANCE)' (instances/$(TUIC_APP_INSTANCE)) — not the shared one,the shared default config directory)"
	TUIC_APP_INSTANCE=$(TUIC_APP_INSTANCE) TUIC_PORT=$(TUIC_PORT) RUST_LOG=tuicommander_lib=debug,info pnpm tauri dev --no-watch -- --config 'target."cfg(target_os = \"macos\")".runner = ["python3", "$(CURDIR)/scripts/dev-exe-copy.py"]'

# Build frontend + launch Tauri dev (for quick manual, throwaway verification).
# Same per-checkout config instance as `make dev` — see the `TUIC_APP_INSTANCE`
# comment at the top.
test:
	$(WARN_SHARED_INSTANCE)
	@echo "Building Vite frontend..."
	@pnpm exec vite build
	@cd src-tauri && cargo build -p tuic-bridge -p tuic-cli
	@echo "Starting Tauri dev on $(if $(TUIC_APP_INSTANCE),the ISOLATED config instance '$(TUIC_APP_INSTANCE)' (instances/$(TUIC_APP_INSTANCE)),the SHARED default config directory)..."
	TAURI_CLI_WATCHER_IGNORE_FILENAME=.taurignore TUIC_APP_INSTANCE=$(TUIC_APP_INSTANCE) pnpm tauri dev

# Build .app only (default, fast — skips DMG)
build:
	@echo "Building TUICommander $(VERSION)..."
	pnpm tauri build --bundles app $(TAURI_BUILD_FLAGS)

# Build .app + DMG (for distribution)
build-dmg:
	@echo "Building TUICommander $(VERSION) with DMG..."
	pnpm tauri build --bundles app,dmg $(TAURI_BUILD_FLAGS)

# Auto-format frontend + Rust
fmt:
	@pnpm exec biome check --fix src/
	@cd src-tauri && cargo fmt

# Type-check, lint, format, and test (no Tauri build)
check: test-shell
	@echo "Running checks..."
	@scripts/with-test-tmp.sh $(RTK) pnpm exec tsc --noEmit && echo "  tsc ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm exec biome check --max-diagnostics=100 src/ && echo "  biome ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm architecture:cycles && scripts/with-test-tmp.sh $(RTK) pnpm architecture:cycles:test && scripts/with-test-tmp.sh $(RTK) pnpm test-tmp-root:test && echo "  architecture cycles ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm check:no-nul-bytes && scripts/with-test-tmp.sh $(RTK) pnpm check:no-nul-bytes:test && echo "  no NUL bytes ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm check:no-home-gits && scripts/with-test-tmp.sh $(RTK) pnpm check:no-home-gits:test && echo "  no HOME-relative Gits paths ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm check:no-hardcoded-tmp && scripts/with-test-tmp.sh $(RTK) pnpm check:no-hardcoded-tmp:test && echo "  no hard-coded temp-dir writes ✓"
	@scripts/with-test-tmp.sh bash -c 'caps=$$(sed -n "/const KNOWN_CAPABILITIES/,/];/p" src-tauri/src/plugins.rs | grep -oE "\"[a-z][a-z:_-]+\"" | tr -d "\""); miss=0; for c in $$caps; do for d in src-tauri/src/mcp_http/plugin_docs.rs docs/plugins.md; do grep -qF "$$c" "$$d" || { echo "  ✗ capability $$c missing from $$d"; miss=1; }; done; done; [ $$miss -eq 0 ]' && echo "  plugin-docs-sync ✓"
	@scripts/with-test-tmp.sh bash scripts/check-make-instance-scope.sh && echo "  make-instance-scope ✓"
	@scripts/with-test-tmp.sh bash scripts/check-make-dev-builds-sibling.sh && echo "  make-dev-builds-sibling ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm exec vite build && scripts/with-test-tmp.sh $(RTK) node scripts/report-frontend-bundles.mjs --check && echo "  frontend bundle budget ✓"
	@cd src-tauri && ../scripts/with-test-tmp.sh $(RTK) cargo fmt --check && echo "  rustfmt ✓"
# bm25 is a vendored third-party patch (patches/bm25): not ours to lint.
	@cd src-tauri && ../scripts/with-test-tmp.sh $(RTK) cargo clippy --workspace --exclude bm25 --release -- -D warnings && echo "  clippy ✓"
	@cd src-tauri && ulimit -n 10240 && ../scripts/with-test-tmp.sh $(RTK) cargo nextest run --workspace && ../scripts/with-test-tmp.sh $(RTK) cargo test --doc --workspace -q && echo "  rust tests ✓"
	@bash -o pipefail -c 'scripts/with-test-tmp.sh $(RTK) pnpm exec vitest run --reporter=dot 2>&1 | tail -3' && echo "  vitest ✓"
	@bash -o pipefail -c 'scripts/with-test-tmp.sh $(RTK) pnpm test:plugins 2>&1 | tail -3' && echo "  plugin tests ✓"
	@scripts/with-test-tmp.sh $(RTK) pnpm audit --audit-level=high && echo "  pnpm audit ✓"
	@cd src-tauri && ../scripts/with-test-tmp.sh $(RTK_ERR) cargo audit -q && echo "  cargo audit ✓"

# Runs `check` the way it needs to be run to actually trust the result:
# defensively rebuilds tuic-hook first (it can transiently read as 0 bytes
# even after a prior successful build — see src-tauri/AGENTS.md > Fresh
# Worktree Setup), warns instead of silently misreporting when the plugins/ submodule
# is uninitialized, and captures make's real exit code instead of a `tee`
# pipeline's (almost always 0, masking a real failure). Prefer this over
# `make check 2>&1 | tee log` directly.
check-gate:
	@./scripts/check-gate.sh

# Rust coverage: cargo-llvm-cov + nextest. Terminal summary + HTML report.
# Instrumented artifacts live in target/llvm-cov-target — the normal build
# cache is untouched, but the first run is a full cold rebuild (whisper.cpp
# included), so expect several minutes. Doctests are not measured
# (doctest coverage requires nightly).
cov:
	@cd src-tauri && ulimit -n 10240 && ../scripts/with-test-tmp.sh $(RTK) cargo llvm-cov nextest --workspace
	@cd src-tauri && ../scripts/with-test-tmp.sh $(RTK) cargo llvm-cov report --html
	@cd src-tauri && ../scripts/with-test-tmp.sh $(RTK) cargo llvm-cov report --lcov --output-path lcov.info
	@echo "HTML report: src-tauri/target/llvm-cov/html/index.html"

# Mutation testing over the Rust changes of a git range (default: last
# commit). --in-diff only: one incremental build + one test run per mutant, so
# a full-tree run is not offered. Runs --in-place in a disposable worktree
# under .tmp/ — see scripts/mutants.sh for why not the default tree copy.
RANGE?=HEAD~1
MUTANTS_ARGS?=
ifneq ($(filter mutants,$(MAKECMDGOALS)),)
MUTANTS_TRAILING_ARGS := $(filter-out mutants,$(MAKECMDGOALS))
.PHONY: $(MUTANTS_TRAILING_ARGS)
$(MUTANTS_TRAILING_ARGS):
endif
mutants:
	@scripts/mutants.sh $(RANGE) $(MUTANTS_ARGS) $(MUTANTS_TRAILING_ARGS)

# CRAP metric (complexity² × uncovered³ + complexity) over the coverage data
# from `make cov`. Thresholds and exclusions live in src-tauri/.cargo-crap.toml.
#
# Deliberately NOT `--workspace`: workspace mode walks every member root, and
# src-tauri/ is itself a member whose tree *contains* crates/ and patches/ — so
# each of those files is analyzed twice (417 reported "crappy" vs 312 real). One
# root from src-tauri/ sees the same code once and lets the `patches/**` exclude
# actually match.
crap:
	@cd src-tauri && test -f lcov.info || { echo "src-tauri/lcov.info missing — run 'make cov' first"; exit 1; }
	@cd src-tauri && $(RTK) cargo crap --lcov lcov.info --top 30

# GitHub API debug logging — toggle at runtime, view logs
gh-debug-on:
	@curl -s -X POST localhost:9876/repo/github-poller/api-debug -H 'Content-Type: application/json' -d '{"enabled":true}' && echo ""

gh-debug-off:
	@curl -s -X POST localhost:9876/repo/github-poller/api-debug -H 'Content-Type: application/json' -d '{"enabled":false}' && echo ""

gh-debug-status:
	@curl -s localhost:9876/repo/github-poller/api-debug && echo ""

gh-debug-logs:
	@curl -s 'localhost:9876/logs?source=github_api&limit=30'

# Tail recent terminal logs (needs Remote Access enabled in Settings).
# Override source/limit: make logs SRC=mcp N=100
logs:
	@curl -s "localhost:9876/logs?source=$(or $(SRC),terminal)&limit=$(or $(N),50)"

gh-rate:
	@curl -sH "Authorization: Bearer $$(gh auth token)" https://api.github.com/rate_limit | jq '.resources | {graphql, core}'

# Sign the built .app bundle
sign:
	@# Auto-detect best available signing certificate
	@if [ "$(SIGN_IDENTITY)" != "-" ]; then \
		SIGN_ID="$(SIGN_IDENTITY)"; \
	else \
		SIGN_ID=$$(security find-identity -v -p codesigning 2>/dev/null | grep "Developer ID Application:" | head -1 | sed 's/.*"\(.*\)".*/\1/'); \
		if [ -z "$$SIGN_ID" ]; then \
			echo "WARNING: No Developer ID found — using ad-hoc signing. Recipients will need to right-click > Open."; \
			SIGN_ID="-"; \
		fi; \
	fi; \
	echo "Signing with: $$SIGN_ID"; \
	codesign --force --deep --sign "$$SIGN_ID" \
		--entitlements src-tauri/Entitlements.plist \
		--identifier "$(BUNDLE_ID)" \
		--options runtime \
		"$(APP_BUNDLE)"; \
	echo "Signed: $(APP_BUNDLE)"

# Verify code signature
verify-sign:
	codesign -dvvv "$(APP_BUNDLE)"

# Notarize with Apple (requires stored credentials).
# First run: xcrun notarytool store-credentials "TUICommander" --apple-id YOUR_ID --team-id YOUR_TEAM
notarize: sign
	@echo "Creating zip for notarization..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip"
	@echo "Submitting to Apple notary service..."
	xcrun notarytool submit "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip" --keychain-profile "TUICommander" --wait
	@echo "Stapling notarization ticket..."
	xcrun stapler staple "$(APP_BUNDLE)"
	@rm -f "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip"
	@echo "Notarization complete."

# Build, sign, notarize, and create distributable zip
release: build-dmg sign notarize
	@echo "Creating distributable zip..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "Release artifact: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@ls -lh "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"

# Quick distribution without notarization (friends can right-click > Open)
dist: build sign
	@echo "Creating distributable zip (not notarized)..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "Distributable: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "NOTE: Recipients must right-click > Open on first launch (not notarized)."
	@ls -lh "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"

# --- GitHub CI workflows ---

# Push main to origin, triggering the Nightly workflow (builds tip release).
# Also force-moves the tip git tag to HEAD so the release points to the latest commit.
# Usage: make nightly
nightly:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	if [ "$$BRANCH" != "main" ]; then echo "ERROR: must be on main (currently on $$BRANCH)" && exit 1; fi; \
	if [ -n "$$(git status --porcelain)" ]; then echo "ERROR: working tree is dirty — commit or stash first" && exit 1; fi; \
	echo "==> Pushing main and updating tip tag..."; \
	git tag -f -m "Nightly tip for $$(git rev-parse --short HEAD)" tip; \
	git push origin main; \
	git push origin tip --force; \
	echo "==> Nightly triggered. Monitor: gh run list -w Nightly --limit 1"

# Bump version across all manifests (no commit, no tag).
# Usage: make bump V=0.6.2
# Do not report a complete bump if release-note generation fails after version edits.
bump:
	@if [ -z "$(V)" ]; then echo "ERROR: specify version with V=x.y.z" && exit 1; fi; \
	CUR=$$(grep '^version' src-tauri/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'); \
	echo "==> Bumping $$CUR → $(V)"; \
	sed -i '' '/^\[workspace.package\]/,/^\[/ s/^version = ".*"/version = "$(V)"/' src-tauri/Cargo.toml; \
	sed -i '' 's/"version": "[^"]*"/"version": "$(V)"/' src-tauri/tauri.conf.json; \
	sed -i '' 's/^  "version": "[^"]*"/  "version": "$(V)"/' package.json; \
	echo "  src-tauri/Cargo.toml [workspace.package] → $(V) (tuicommander, tuic-bridge, tuic-cli inherit)"; \
	echo "  src-tauri/tauri.conf.json → $(V)"; \
	echo "  package.json          → $(V)"; \
	(cd src-tauri && (cargo metadata --offline --format-version 1 >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null)); \
	echo "  src-tauri/Cargo.lock  → $(V) (workspace members re-pinned)"; \
	sed -i '' 's/^\*\*Version:\*\* .*/**Version:** $(V)/' SPEC.md; \
	echo "  SPEC.md               → $(V)"; \
	TODAY=$$(date +%Y-%m-%d); \
	sed -i '' 's/^## \[Unreleased\]/## [Unreleased]\n\n## [$(V)] - '"$$TODAY"'/' CHANGELOG.md; \
	echo "  CHANGELOG.md          → $(V) ($$TODAY)"; \
	echo "  release-notes.json   → generating..."; \
	./scripts/generate-release-notes.sh $(V) || { echo "ERROR: Release-note generation failed; version files are already updated. Release preparation is incomplete." >&2; exit 1; }; \
	echo "==> Done. Run 'cargo check' or 'make github-release' to continue."

# Generate AI-written release notes for a specific version.
# Usage: make release-notes V=1.3.0
release-notes:
	@if [ -z "$(V)" ]; then echo "ERROR: specify version with V=x.y.z" && exit 1; fi; \
	./scripts/generate-release-notes.sh $(V)

# Full versioned release: tag current version, push, wait for CI, publish.
# To bump first: make bump BUMP=patch (or minor|major), then make github-release.
# NOTE: sed -i '' is macOS syntax — run this from macOS only.
github-release:
	@set -e; \
	BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	if [ "$$BRANCH" != "main" ]; then echo "ERROR: must be on main (currently on $$BRANCH)" && exit 1; fi; \
	if [ -n "$$(git status --porcelain)" ]; then echo "ERROR: working tree is dirty — commit or stash first" && exit 1; fi; \
	CUR=$$(grep '^version' src-tauri/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'); \
	TAG="v$$CUR"; \
	if git rev-parse "$$TAG" >/dev/null 2>&1; then echo "ERROR: tag $$TAG already exists" && exit 1; fi; \
	echo "==> Releasing $$TAG"; \
	git tag -a -m "$$TAG" "$$TAG"; \
	COMMIT=$$(git rev-parse HEAD); \
	echo "--- Pushing..."; \
	git push origin main; \
	git push origin "$$TAG" || { git tag -d "$$TAG"; echo "ERROR: tag push failed — local tag removed, fix and re-run"; exit 1; }; \
	echo "--- Waiting for Release workflow on $$COMMIT..."; \
	sleep 10; \
	RUN_ID=""; \
	for i in 1 2 3 4 5; do \
		RUN_ID=$$(gh run list -w Release --limit 5 --json databaseId,headSha --jq ".[] | select(.headSha == \"$$COMMIT\") | .databaseId" 2>/dev/null | head -1) || true; \
		if [ -n "$$RUN_ID" ]; then break; fi; \
		echo "  run not found yet, retrying ($$i/5)..."; \
		sleep 5; \
	done; \
	if [ -z "$$RUN_ID" ]; then echo "ERROR: no Release workflow run found for $$COMMIT" && exit 1; fi; \
	echo "--- Watching run $$RUN_ID (Ctrl+C to detach)..."; \
	gh run watch "$$RUN_ID" --exit-status; \
	echo "--- Publishing draft release..."; \
	gh release edit "$$TAG" --draft=false; \
	echo "==> Released: $$(gh release view $$TAG --json url --jq .url)"

# Build a debug preview with a different app name to avoid conflicts with the real app.
# The resulting .app is named "TUIC-preview" with a separate bundle ID, so macOS and
# tests won't confuse it with the production TUICommander.
# Uses --debug for fast iteration; full release build only on github-release.
preview:
	@echo "Building TUIC-preview $(VERSION) (debug mode)..."
	pnpm tauri build --debug --bundles app --config '{"productName":"TUIC-preview","identifier":"com.tuic.preview","bundle":{"createUpdaterArtifacts":false},"app":{"windows":[{"title":"TUIC-preview","width":1200,"height":800,"minWidth":800,"minHeight":600,"decorations":true,"transparent":false,"resizable":true,"fullscreen":false,"hiddenTitle":true,"titleBarStyle":"Overlay","trafficLightPosition":{"x":13,"y":20},"backgroundColor":"#000000","dragDropEnabled":true}]}}'
	@echo "Launching TUIC-preview..."
	open "src-tauri/target/debug/bundle/macos/TUIC-preview.app"

# Build the documentation book + Pagefind search index into docs/book.
# Same script CI runs, so a local preview matches the deployed site exactly.
docs:
	@./scripts/build-docs.sh

# Preview the docs locally — the search needs to be served over HTTP, file:// won't do.
docs-serve: docs
	@echo "Docs at http://127.0.0.1:8123 (ctrl+c to stop)"
	@cd docs/book && python3 -m http.server 8123

# Clean build artifacts
clean:
	rm -rf $(DIST_DIR) docs/book
	cd src-tauri && cargo clean
