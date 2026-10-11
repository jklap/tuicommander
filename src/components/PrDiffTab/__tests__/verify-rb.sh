#!/usr/bin/env bash
# Targeted verification inside an rb source replica, invoked through build-slot.
set -euo pipefail
pnpm install --offline --frozen-lockfile
pnpm exec tsc --noEmit
scripts/with-test-tmp.sh pnpm vitest run src/components/PrDiffTab src/__tests__/components/PrDiffTab.test.tsx src/__tests__/components/DiffFileList.test.tsx src/__tests__/components/DiffFileListMeasure.test.tsx src/__tests__/githubReviewTransport.test.ts
export PLAYWRIGHT_BROWSERS_PATH=/home/rb/Gits/.tmp/pw/browsers
/home/rb/Gits/.tmp/pw/node_modules/.bin/playwright-core install webkit
mkdir -p reports/visual
./node_modules/.bin/vite --host 127.0.0.1 --port 4180 --strictPort > reports/visual/pr-review-vite.log 2>&1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true' EXIT
for (( attempt = 0; attempt < 120; attempt++ )); do
	if curl --silent --fail http://127.0.0.1:4180/src/components/PrDiffTab/__tests__/preview.html > /dev/null; then break; fi
	if ! kill -0 "$vite_pid" 2>/dev/null; then cat reports/visual/pr-review-vite.log; exit 1; fi
	sleep 0.5
done
PW_ENGINES=chromium-desktop node src/components/PrDiffTab/__tests__/visual.mjs
