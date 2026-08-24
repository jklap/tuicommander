import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import path from "node:path";

export default defineConfig({
  plugins: [solid()],
  define: {
    __APP_VERSION__: JSON.stringify("0.3.0"),
  },
  resolve: {
    conditions: ["development", "browser"],
    alias: {
      // Mock SVG imports in tests
      "^.+\\.svg$": path.resolve(__dirname, "src/__tests__/mocks/svg.ts"),
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
      "\\.svg$": path.resolve(__dirname, "src/__tests__/mocks/svg.ts"),
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
        "src/components/PromptDrawer/PromptDrawer.tsx",
      ],
      // These thresholds were declared at 80% but never actually enforced in CI (`pnpm
      // test:coverage` wasn't wired into any workflow), so real coverage drifted far below
      // that target unnoticed. Measured floor as of 2026-08-22: lines 49.7%, statements
      // 46.79%, functions 45.23%, branches 42.95%. Set just under that floor (not 80%) so
      // CI can enforce "don't regress" starting now without a red build on day one — ratchet
      // these up incrementally as coverage genuinely improves, rather than lowering them
      // again if a change makes CI red.
      thresholds: {
        lines: 49,
        functions: 45,
        branches: 42,
        statements: 46,
      },
    },
  },
});
