# Mobile PWA

- The terminal is the focus of the session screen. Preserve its visible area when adding controls. Put search, progress, tasks, logos, attachments, and menus in the existing top bar, an overflow menu, or a transient overlay. If a change costs terminal rows, report the count at a 360×800 viewport and why no alternative works.
- `mobile.html` loads `src/mobile/index.tsx`, which mounts `MobileApp.tsx`. Session list and detail screens live in `screens/`; shared mobile controls live in `components/`.
- Focused mobile tests live in `src/mobile/__tests__/`. Run them from the repository root with `scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/<test>.test.tsx` and a narrow filter.
- Record live phone checks in the repository root `to-test.md`. A desktop browser or component test does not prove the phone layout or touch behavior.
- `mobile.html` has a 100 KiB gzip budget (`scripts/report-frontend-bundles.mjs --check`). A module the mobile entry reaches eagerly must not import the i18n catalog (`src/i18n`, `src/i18n/t`): it pulls all of `en.json` (~20 KB gzip). Code that only sets the locale imports `src/i18n/locale`. `src/mobile/__tests__/mobileEagerGraph.test.ts` enforces this.
