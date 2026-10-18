# Chat code Copy layout — #2045-848c

The existing production session Chat appearance fixture renders the real shared
Transcript and ContentRenderer at 390 × 844. Transcript answer-region tagging
includes fenced `pre` blocks. Its later spacing reset set their padding to zero,
overriding the normal code inset. An absolutely positioned Copy button then
extended below a one-line block and caused vertical overflow.

The fix excludes `pre` from that spacing reset. It restores existing code padding,
not a fixed height, a new overflow clamp or another rendering path.

## Recorded geometry

- Before, all three short-block presentations: block height 18.84375 px;
  button height 17.5 px starting 4 px down; button extends 2.65625 px below the
  block. `scrollHeight=22`, `clientHeight=19`.
- After: block height 34.84375 px, with the complete button inside it;
  `scrollHeight=35`, `clientHeight=35`.
- Long-code boundary: block expands to 129.0625 px;
  `scrollHeight=129`, `clientHeight=129`.
- Document width remains 390 px in every capture.

[Before measurements](before.json) · [Corrected measurements](measurements.json)

## Corrected captures

- [Mobile dark](mobile-dark.png)
- [Mobile light](mobile-light.png)
- [Desktop narrow dark](desktop-narrow-dark.png)
- [Mobile long-code boundary](mobile-long-code.png)

The pre-fix browser regression `short_code_copy_fits` failed on
`cae10e767fc4676ff278e721bf4e1301d3f1817e` for all three short-block presentations.
It asserts observable button containment, label containment and no vertical
code-block overflow. It does not inspect CSS source or mock browser geometry.

Run the focused browser check through the build slot:

```sh
python3 scripts/check-chat-code-copy-layout.py --evidence .tmp/code-copy-check
```

The evidence directory must not exist. The script uses a unique named stealth
browser tab, an isolated worktree Vite on port 15970, and cleans up its own
processes. `--capture-only` collects images and geometry without running the test
assertions. The post-fix operation used that mode as requested for screenshots;
the coordinator owns the final GREEN run and pixel review. No physical-phone,
WebKit, clipboard transport or performance claim follows from this fixture.
