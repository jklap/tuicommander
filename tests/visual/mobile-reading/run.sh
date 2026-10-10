#!/usr/bin/env bash
set -uo pipefail
mkdir -p junit.xml reports/visual/mobile-reading
if [[ ${1:-} == --browser-only ]]; then
    cp tests/visual/mobile-reading/green-frames.json junit.xml/frames.json || exit 1
    rust_rc=0
else
    export TUIC_MOBILE_REPLAY_JSON="$TUIC_TEST_TMP_ROOT/mobile-frames.json"
    cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuic-terminal --test vt_log_grid_cap --test state_vt_log -E 'binary(vt_log_grid_cap) | test(line_oriented) | test(alternate) | test(resize) | test(capacity) | test(evicted) | test(suppresses_extraction)' --no-fail-fast >junit.xml/rust.log 2>&1
    rust_rc=$?
    cat junit.xml/rust.log
    if [[ ! -f $TUIC_MOBILE_REPLAY_JSON ]]; then exit "$rust_rc"; fi
    cp "$TUIC_MOBILE_REPLAY_JSON" junit.xml/frames.json
fi
lib_root=/home/rb/Gits/.tmp/pw/mobile-reading-libs/root/usr/lib/aarch64-linux-gnu
export LD_LIBRARY_PATH="/home/rb/Gits/.tmp/pw/mobile-reading-libs/root/lib/aarch64-linux-gnu:$lib_root${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# Playwright checks dlopen libraries through system ldconfig, which cannot see this local extraction.
# WebKit still loads its actual libraries from LD_LIBRARY_PATH and must render successfully.
export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1
export GST_PLUGIN_PATH="$lib_root/gstreamer-1.0${GST_PLUGIN_PATH:+:$GST_PLUGIN_PATH}"
export PLAYWRIGHT_BROWSERS_PATH
PLAYWRIGHT_BROWSERS_PATH=/home/rb/Gits/.tmp/pw/browsers
export TUIC_MOBILE_WEBKIT_LAUNCHER="$(pwd)/tests/visual/mobile-reading/webkit.sh"
if [[ ! -d $PLAYWRIGHT_BROWSERS_PATH/webkit-2370 ]]; then echo 'WebKit browser missing'; exit 69; fi
pnpm exec vite --config tests/visual/mobile-reading/vite.config.ts >junit.xml/vite.log 2>&1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true' EXIT
for _ in {1..50}; do
    curl -fsS http://127.0.0.1:14381/tests/visual/mobile-reading/index.html >/dev/null && break
    sleep 0.2
done
node tests/visual/mobile-reading/capture.mjs http://127.0.0.1:14381 junit.xml/frames.json reports/visual/mobile-reading/green /home/rb/Gits/.tmp/pw/node_modules >junit.xml/browser.log 2>&1
browser_rc=$?
cat junit.xml/browser.log
if [[ -f tests/visual/mobile-reading/red-frames.json ]]; then
    node tests/visual/mobile-reading/capture.mjs http://127.0.0.1:14381 tests/visual/mobile-reading/red-frames.json reports/visual/mobile-reading/red /home/rb/Gits/.tmp/pw/node_modules >junit.xml/red-browser.log 2>&1
    red_rc=$?
    cat junit.xml/red-browser.log
    if (( red_rc == 0 )); then echo "Pre-fix replay unexpectedly retained text"; exit 1; fi
    if ! node -e 'const r = require("./reports/visual/mobile-reading/red/report.json"); if (r.missing !== true || r.after <= r.before) process.exit(1)'; then
        echo "RED replay did not prove displayed text disappeared"; exit 1
    fi
fi
if (( rust_rc != 0 )); then exit "$rust_rc"; fi
exit "$browser_rc"
