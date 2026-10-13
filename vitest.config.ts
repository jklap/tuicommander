import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import path from "node:path";
import { mkdirSync } from "node:fs";
import { testTmpRoot } from "./scripts/test-tmp-root.mjs";

// Vitest workers and their dependencies inherit this path. Vite's dev/build
// configuration is separate, so normal app processes retain the OS temp dir.
// Under scripts/with-test-tmp.sh this is the wrapper's per-run dir; a bare
// `vitest` run uses <TMPDIR>/tuic-tests (see scripts/test-tmp-root.mjs).
const testTmp = testTmpRoot();
mkdirSync(testTmp, { recursive: true });
process.env.TMPDIR = testTmp;
process.env.TMP = testTmp;
process.env.TEMP = testTmp;

export default defineConfig({
  plugins: [solid()],
  define: {
    __APP_VERSION__: JSON.stringify("0.3.0"),
  },
  resolve: {
    conditions: ["development", "browser"],
    alias: {
      // Mock SVG imports in tests
      "^.+\\.svg$": path.resolve(import.meta.dirname, "src/__tests__/mocks/svg.ts"),
    },
  },
  test: {
    // App tests only. Plugins ship their own `node:test` suites, which vitest
    // collects by default and then reports as "no test suite found" — run them
    // with `pnpm test:plugins` instead.
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    css: {
      modules: {
        classNameStrategy: "non-scoped",
      },
    },
    server: {
      deps: {
        inline: ["@git-diff-view/solid", "solid-codemirror"],
      },
    },
    environment: "happy-dom",
    globals: true,
    detectAsyncLeaks: true,
    // Vitest 4 can otherwise saturate the host while initializing this large suite,
    // causing tests or even new worker processes to time out under scheduler pressure.
    maxWorkers: 4,
    setupFiles: ["src/__tests__/setup.ts", "src/__tests__/mocks/tauri.ts"],
    alias: {
      "\\.svg$": path.resolve(import.meta.dirname, "src/__tests__/mocks/svg.ts"),
    },
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/__tests__/**",
        "src/index.tsx",
        "src/types/**",
        "src/**/index.ts",
        // Untestable without runtime: Tauri APIs, xterm.js, complex Tauri IPC
        "src/App.tsx",
        "src/components/Terminal/Terminal.tsx",
        "src/components/IdeLauncher/IdeLauncher.tsx",
      ],
      // These thresholds were declared at 80% but never actually enforced in CI (`pnpm
      // test:coverage` wasn't wired into any workflow), so real coverage drifted far below
      // that target unnoticed. f6b66396 set them to the measured floor as of 2026-08-22
      // (lines 49.7%, statements 46.79%, functions 45.23%, branches 42.95%). Ratcheted up
      // again as of 2026-08-24 (Phase 0 of the Smart Selection plan — a CanvasTerminal mount
      // harness plus targeted backfills brought lines to 52.84%, statements 49.56%, functions
      // 47.12%, branches 45.88%). Ratcheted up again as of 2026-08-25 (Smart Selection rule
      // editor cleanup — SelectionTab/ContextMenu/SettingFields backfills brought lines to
      // 53.77%, statements 50.57%, functions 48.24%, branches 46.7%). Ratcheted up again as of
      // 2026-08-26 (Activity Dashboard idle-ordering + keyboard-navigation plan — closing the
      // pre-existing test-coverage gaps first, per that plan's Phase 0, brought lines to
      // 58.49%, statements 55.37%, functions 53.27%, branches 51.18%). Ratcheted up again the
      // same day (double-click coordinate-desync fix in canvasTerminalSelection.ts/
      // smartSelection.ts plus the SelectionTab rule-row expand/collapse rework — regression
      // and gap-closing backfills on top of the above brought the combined floor to lines
      // 58.62%, statements 55.51%, functions 53.45%, branches 51.36% — the same whole-number
      // floor as just above, since this session's own contribution didn't cross another
      // integer point. Ratcheted up again as of 2026-09-14 (Session Diff Review — the DiffTab/
      // BranchDiffScrollView coverage backfill that preceded the feature, plus the new
      // SessionDiffTab tree's own tests, measured lines 65.97%, statements 63.08%, functions
      // 61.32%, branches 57.76%). Set just under that new floor so CI can keep enforcing "don't
      // regress" — ratchet up incrementally as coverage genuinely improves, rather than lowering
      // them again if a change makes CI red. Re-measured 2026-10-08 on the tree that replays
      // wip onto main (main had kept the never-enforced 80s; wip's 65/61/57/63 were measured
      // on wip alone): a clean-env `vitest run --coverage` over 766 files / 11,376 tests
      // measured lines 79.99%, statements 77.57%, functions 76.95%, branches 71.21%. Floors
      // below are each value minus one point, rounded down.
      thresholds: {
        lines: 78,
        functions: 75,
        branches: 70,
        statements: 76,
      },
    },
  },
});
