#!/usr/bin/env bash

OFF='\033[0m'
RED='\033[0;31m'
BRIGHT_RED='\033[0;91m'
BRIGHT_YELLOW='\033[0;93m'
GREEN='\033[0;32m'
BLUE='\033[0;94m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD_RED='\033[1;31m'
BOLD_GREEN='\033[1;32m'
BOLD_BLUE='\033[1;34m'
BOLD_PURPLE='\033[1;35m'
BOLD_CYAN='\033[1;36m'
BOLD_YELLOW='\033[1;33m'
BOLD_UNDERLINED='\033[1;4m'
BOLD='\033[1m'

# NOTE: the below logic was stolen from the "nightly.yml" GHA

# Append nightly timestamp to current version (strip any existing -nightly suffix first)
RAW_VERSION=$(jq -r '.version' "src-tauri/tauri.conf.json")
VERSION="${RAW_VERSION%%-nightly*}"
NIGHTLY="${VERSION}-nightly.$(date -u +%Y%m%d).t$(date -u +%H%M)c"
echo -e "${GREEN}Nightly version: ${NIGHTLY} (from ${RAW_VERSION})${OFF}"

# Patch tauri.conf.json: version + add nightly updater endpoint
python3 -c "
import json
c = json.load(open('src-tauri/tauri.conf.json'))
c['version'] = '${NIGHTLY}'
nightly_ep = 'https://github.com/sstraus/tuicommander/releases/download/nightly/latest.json'
ep = c['plugins']['updater']['endpoints']
if nightly_ep not in ep:
    ep.insert(0, nightly_ep)
json.dump(c, open('src-tauri/tauri.conf.json','w'), indent=2)
print('Patched tauri.conf.json')
"

# Patch Cargo.toml version
sed "s/^version = \"${VERSION}\"/version = \"${NIGHTLY}\"/" "src-tauri/Cargo.toml" > "tmp.toml"
mv "tmp.toml" "src-tauri/Cargo.toml"

export RUSTC_WRAPPER="sccache"
export CMAKE_C_COMPILER_LAUNCHER="sccache"
export CMAKE_CXX_COMPILER_LAUNCHER="sccache"

# do a local signing of the app (and the sidecars)
export APPLE_SIGNING_IDENTITY="-"

make build
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}Build failed${OFF}"
    exit 1
fi

# enable debug level logging in the app's Info.plist
/usr/libexec/PlistBuddy \
    -c "Add :LSEnvironment dict" \
    -c "Add :LSEnvironment:RUST_LOG string 'info,tuicommander_lib::pty=debug,tuicommander_lib::state=debug'" \
    src-tauri/target/release/bundle/macos/TUICommander.app/Contents/Info.plist

echo -e "${GREEN}codesign: ${BOLD_GREEN}TUICommander.app${OFF}"
# sign post changes to Info.plist
codesign \
    --force \
    --sign - \
    src-tauri/target/release/bundle/macos/TUICommander.app
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}codesign failed${OFF}"
    exit 1
fi

echo -e "${GREEN}codesign verification: ${BOLD_GREEN}TUICommander.app${OFF}"
codesign \
    --verify \
    --verbose \
    src-tauri/target/release/bundle/macos/TUICommander.app 2>&1 \
    | grep --color=always -E 'valid on disk|satisfies its Designated Requirement|$'
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}codesign failed${OFF}"
    exit 1
fi

echo -e "${GREEN}codesign verification: ${BOLD_GREEN}TUICommander.app${OFF}"
codesign \
    --display \
    --check-notarization \
    --entitlements - \
    --xml \
    --requirements - \
    --verbose=4 \
    src-tauri/target/release/bundle/macos/TUICommander.app 2>&1 \
    | grep --color=always -E 'CodeDirectory.*|Signature.*|$'
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}codesign failed${OFF}"
    exit 1
fi

echo -e "${GREEN}notarization verification: ${BOLD_GREEN}TUICommander.app${OFF}"
codesign \
    --display \
    --check-notarization \
    -vvv \
    src-tauri/target/release/bundle/macos/TUICommander.app 2>&1 \
    | grep --color=always -E 'CodeDirectory.*|$'
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}codesign failed${OFF}"
    exit 1
fi

echo -e "${GREEN}Gatekeeper verification: ${BOLD_GREEN}TUICommander.app${OFF}"
spctl \
    --assess \
    --verbose \
    src-tauri/target/release/bundle/macos/TUICommander.app 2>&1 \
    | grep --color=always -E 'rejected|$'
if [ $? -ne 0 ] && [ "$APPLE_SIGNING_IDENTITY" != '-' ]; then
    # we only care about the Gatekeeper check if it's not a local dev signing
    echo -e "${BOLD_RED}Failed Gatekeeper check"
    exit 1
fi

# spot checks, all should be good, only report if not
{ codesign --verify src-tauri/target/release/bundle/macos/TUICommander.app/Contents/MacOS/tuic && \
    codesign --verify src-tauri/target/release/bundle/macos/TUICommander.app/Contents/MacOS/tuic-bridge && \
    codesign --verify src-tauri/target/release/bundle/macos/TUICommander.app/Contents/MacOS/tuic-hook && \
    codesign --verify src-tauri/target/release/bundle/macos/TUICommander.app/Contents/MacOS/tuic-remote ; \
} || { echo "Unable to verify status of all sidecar code-signing"; exit 1; }

open src-tauri/target/release/bundle/

echo -e "${GREEN}Build complete${OFF}"

echo -e "${GREEN}Restoring version files${OFF}"
diff_check() {
    local FILE="$1"
    git diff --quiet \
        -I '(^version =|"version": |download/nightly)' \
        --ignore-cr-at-eol \
        "$FILE"
    if [ $? -eq 0 ]; then
        echo "No substantial changes to ${FILE}"
        git restore "$FILE"
    fi
}

diff_check "src-tauri/Cargo.lock"
diff_check "src-tauri/Cargo.toml"
diff_check "src-tauri/tauri.conf.json"

echo -e "${GREEN}Updating plugins${OFF}"
rsync -aPvF \
    --delete \
    plugins/build-cleaner \
    plugins/md-kanban \
    plugins/mdkb-dashboard \
    plugins/plan \
    plugins/rtk-dashboard \
    plugins/stories-ticker \
    plugins/tuic-vscode-icons \
    plugins/wiz-kanban \
    plugins/csv-preview \
    plugins/docx-preview \
    plugins/xlsx-preview \
    examples/plugins/repo-dashboard \
    examples/plugins/claude-status \
    examples/plugins/report-watcher \
    --exclude "main.test.js" \
    ~/Library/Application\ Support/com.tuic.commander/plugins/

exit 0



esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:nothing




codesign --force --deep --sign "$$SIGN_ID" \
	--entitlements src-tauri/Entitlements.plist \
	--identifier "$(BUNDLE_ID)" \
	--options runtime \
	"$(APP_BUNDLE)";


# Import macOS signing certificate
DEVELOPER_ID_CERT_BASE64: ${{ secrets.DEVELOPER_ID_CERT_BASE64 }}
DEVELOPER_ID_CERT_PASSWORD: ${{ secrets.DEVELOPER_ID_CERT_PASSWORD }}

KEYCHAIN_PATH=$RUNNER_TEMP/app-signing.keychain-db
KEYCHAIN_PASSWORD=$(openssl rand -hex 16)

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"

CERT_PATH=$RUNNER_TEMP/cert.p12
echo "$DEVELOPER_ID_CERT_BASE64" | base64 --decode > "$CERT_PATH"
security import "$CERT_PATH" \
    -k "$KEYCHAIN_PATH" \
    -P "$DEVELOPER_ID_CERT_PASSWORD" \
    -T /usr/bin/codesign
rm -f "$CERT_PATH"

security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security list-keychains -d user -s "$KEYCHAIN_PATH" $(security list-keychains -d user | tr -d '"')

IDENTITY=$(security find-identity -v -p codesigning "$KEYCHAIN_PATH" | grep "Developer ID Application:" | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "APPLE_SIGNING_IDENTITY=$IDENTITY" >> "$GITHUB_ENV"
echo "KEYCHAIN_PATH=$KEYCHAIN_PATH" >> "$GITHUB_ENV"


# Write Apple API key file for notarization
NOTARIZE_KEY_BASE64: ${{ secrets.NOTARIZE_KEY_BASE64 }}
NOTARIZE_KEY_ID: ${{ secrets.NOTARIZE_KEY_ID }}

KEY_PATH="$RUNNER_TEMP/AuthKey_${NOTARIZE_KEY_ID}.p8"
echo "$NOTARIZE_KEY_BASE64" | base64 --decode > "$KEY_PATH"
echo "APPLE_API_KEY_PATH=$KEY_PATH" >> "$GITHUB_ENV"


APPLE_CERTIFICATE: ${{ secrets.DEVELOPER_ID_CERT_BASE64 }}
APPLE_CERTIFICATE_PASSWORD: ${{ secrets.DEVELOPER_ID_CERT_PASSWORD }}
APPLE_SIGNING_IDENTITY: ${{ env.APPLE_SIGNING_IDENTITY }}
APPLE_API_KEY_PATH: ${{ env.APPLE_API_KEY_PATH }}
APPLE_API_ISSUER: ${{ secrets.NOTARIZE_ISSUER_ID }}
APPLE_API_KEY: ${{ secrets.NOTARIZE_KEY_ID }}
TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

#
### build
#

# Cleanup macOS keychain
security delete-keychain "$KEYCHAIN_PATH" 2>/dev/null || true
rm -f "$APPLE_API_KEY_PATH" 2>/dev/null || true


# Typecheck
pnpm exec tsc --noEmit

# Tests + coverage
# Runs the same suite as `pnpm exec vitest run` plus coverage collection, enforcing
# the floor thresholds in vitest.config.ts (see that file's comment) — one pass
# instead of running the whole suite twice.
pnpm exec vitest run --coverage --reporter=verbose


# Plugin tests
# Self-test first: it always passes today (exercises its own fixtures),
# so it can't mask a self-test regression behind the real check's
# expected failure below (the plugins submodule currently has zero
# tests, so `test:plugins` failing here is the known, tracked state).
pnpm test:plugins:test && pnpm test:plugins

# Architecture cycles
pnpm architecture:cycles && pnpm architecture:cycles:test

# No literal NUL bytes in source
pnpm check:no-nul-bytes && pnpm check:no-nul-bytes:test

exit $?
# test-edit-probe
