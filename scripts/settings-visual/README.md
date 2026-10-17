# Isolated Settings visual regressions

Run from a worktree with its own installed dependencies. The fixture mounts the real SettingsPanel, page components, theme JSON and controls. Vite aliases intercept RPC, events and logging before application imports. No native backend, user config or external service is used. Chromium blocks non-local requests.

Requirements: the bundled Chrome for Testing path in `capture.mjs`, Node with WebSocket support, and free localhost port 15391. The server uses `--strictPort`; the runner stops its own Vite and Chrome processes.

Run a single regression with the normal build admission wrapper:

```sh
VISUAL_TEST=settings_text_contrast_on_themed_surfaces VISUAL_RUN=text \
  ~/Gits/personal/orchestrator/tools/build/build-slot.sh bash scripts/settings-visual/run.sh
```

Other named regressions:

- `settings_agent_initials_contrast`: catches white identity letters on bright agent fills.
- `settings_appearance_legend_light_text_contrast`: catches unreadable semantic marker text in the real legend/sidebar components.
- `settings_help_badges_share_label_line`: catches sibling help placed beneath its label.

Each checks Commander and Paper at 1400x900 and 390x844. Assertions and screenshots go under `.tmp/settings-visual/$VISUAL_RUN/`. Every run asserts shared row/help geometry, compact page selection, repository label order, shortcut spacing, and equal page-header widths. Results include the measured bounding boxes in `geometry-assertions.json` and the per-theme JSON reports. `VISUAL_TEST` adds the named contrast or help regression. The fixture also captures a dynamic plugin page in ready/loading/error/stale-revision states, the AI Chat folder editor, and a custom launcher editor. These are synthetic RPC fixtures, not live service or persistence evidence. `VISUAL_PAGES` restricts a capture to comma-separated tab IDs; `VISUAL_ELEMENTS=1` also crops label/help pairs, agent icons and PR markers.

The contrast oracle rasterizes CSS colors to resolve `color(srgb ...)` and composites translucent ancestor backgrounds. It uses raw ratios for the threshold, not the rounded report value. Help question buttons have a 3:1 non-text threshold; semantic question text has a 4.5:1 threshold. Reduced motion makes marker captures deterministic. It does not claim native WebKit rendering or subjective design approval. The Actions table's screen-reader-only heading is intentionally clipped.
