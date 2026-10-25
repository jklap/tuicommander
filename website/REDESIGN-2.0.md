# Website redesign for TUICommander 2.0: proposal

- **Status:** proposal (phase 1). No site code is changed yet.
- **Date:** 2026-10-25. **Branch:** `feat/website-2.0`.
- **Request (Boss, 2026-10-25):** redesign the site, put the new things in evidence, add the presentation video, restructure it and give room to more content.
- **Sources read:** `website/` (all files), `plans/archive/marketing-website.md`, `DESIGN.md`, `PRODUCT.md`, `CHANGELOG.md` headings for `[2.0.0-beta.1]`/`[2.0.0-beta.2]`, `docs/user-guide/*`, `docs/FEATURES.md`, `src/agents.ts`, `.github/workflows/website.yml`, `tuic-presentation/script-v3.md` (as-built table) and `tuic-presentation/docs/2.0-feature-analysis.md` (a scored map of the 2.0 changelog).

## 1. The current site and its gaps

### Site map today

| URL | Content |
|---|---|
| `/` (`index.html`, 885 lines) | Nav · hero (tagline, three pillars, Download/GitHub/Nightly, install tabs, screenshot with a YouTube facade for the 1.x video `YBtD9g-PFJs`) · agent grid · "The problem" · 12 feature blocks (parallel sessions, worktrees, observability, Progress, AI Chat, diffs, global workspace, remote, MCP + plugins, automations, customize, voice) · terminal features · 14 FAQ entries · bottom CTA · footer |
| `/blog.html` + `/blog/*.html` | 31 posts, hand-written list |
| `/docs/` | mdBook built from `docs/` by `scripts/build-docs.sh` in the same Pages workflow |
| `/install.sh`, `/install.ps1` | Installers |

Deploy: `.github/workflows/website.yml`, on push to `main` under `website/**` or `docs/**`, to GitHub Pages. The installers and the film use `tuicommander.com`, so a custom domain is set in the Pages settings (inferred: no `CNAME` file in the repo).

### Gaps

| # | Gap | Evidence |
|---|---|---|
| G1 | One page holds everything. Twelve features at equal weight, no hierarchy, no room to explain any of them. A 2.0 feature would be block 13. | `index.html` sections 213–671 |
| G2 | Nothing of 2.0 is visible: no Stories/Kanban, Workflow designer, Tasks (GitHub/Jira/GitLab), PR line review, CLI \| Chat, rich sidebar cards, `+` agent menu, Progress Flow, Design Mode, secrets, themes, settings search. | grep of `index.html` |
| G3 | The video is the 1.x YouTube cut. The 2.0 film (3:37) is not on the site. | `index.html:133-153`, script at the end |
| G4 | Agent grid shows 8 agents and claims "10". `src/agents.ts` has 11: Claude Code, Gemini CLI, OpenCode, Codex CLI, Amp, Cursor Agent, Goose, Grok, Droid, pi, ego. Grok, pi and ego are missing; `website/assets/agents/` still has `aider.svg`, `jules.svg`, `ona.svg`, `warp.svg`. | `index.html:158-195`, `src/agents.ts` |
| G5 | `og:image` and `twitter:image` point at `/og.png`, which does not exist: every shared link has no preview. | `ls website/public` |
| G6 | `sitemap.xml`, `robots.txt` and `og:url` use `sstraus.github.io/tuicommander/`; the installers and the film use `tuicommander.com`. The sitemap lists 2 of 31 posts. | `public/sitemap.xml`, `public/robots.txt` |
| G7 | Two "Docs" links disagree: nav → `/docs/`, footer → GitHub `docs/FEATURES.md`. | `index.html:49`, `:800` |
| G8 | The site breaks two `DESIGN.md` rules: `backdrop-filter: blur(12px)` on the nav (No-Glassmorphism) and a `radial-gradient` glow in the hero (no gradients). | `style.css:104`, `:171` |
| G9 | No download page: system requirements (macOS 12 minimum), beta channel, nightly and the per-OS assets are mixed into the hero. | `index.html:84-131` |
| G10 | Mobile: the nav links collapse with no menu below 768 px (to verify in phase 2). | `style.css:1132` |

## 2. New information architecture

Principle: the home page sells the idea in one scroll and routes to depth. Each product area gets its own page with room for screenshots, a short "how it works", and a link into the matching user guide. The docs stay the reference; the site does not copy them.

### Pages

| URL | Purpose | Main content |
|---|---|---|
| `/` | Pitch + routing | Hero with the 2.0 film · agent grid (11) · "One window, one orchestrator" loop in five steps (plan → spawn → watch → review → ship), each linking to its area page · six highlight bands with a real screenshot each · "New in 2.0" strip · install · short FAQ (6) · CTA |
| `/2.0.html` | The 2.0 release page | Full film with chapters · what changed since 1.7, grouped by area · "Not filmed but shipped" (resume after update, hibernation, tray, native scrollback, Windows CI, secrets) · upgrade notes · link to CHANGELOG |
| `/features/orchestration.html` | Agents and observability | `+` agent menu, rich sidebar cards, CLI \| Chat, question and rate-limit detection, Activity Dashboard, agents spawn agents, peer mail, Progress List/Flow, AI Chat (ego over ACP), resume after update |
| `/features/planning.html` | From idea to running work | Ideas, native Plans and Stories with List/Kanban, Workflow designer and runs, Automations (cron, templates, Run now, history) |
| `/features/git-and-review.html` | Ship the agent's work | Worktrees and archive/merge lifecycle, Git panel (diff, stage, smart commit), PR popover with CI, CI auto-heal, PR line review threads, Tasks: GitHub Issues/Projects, Jira and GitLab |
| `/features/workspace.html` | Everything around the terminal | File Browser (glob search, previews), Markdown panel (Edit · Live · Diff), Design Mode, editor, themes (Commander, Paper, Ink), settings search, terminal power features, keyboard-first navigation |
| `/features/anywhere.html` | Remote, mobile, voice | Mobile companion (CLI \| Chat, question cards, push), browser/PWA mode, SSH remote machines with `tuic-remote`, Telegram, push-to-talk and hands-free voice, private secret forms |
| `/features/extend.html` | Open platform | MCP Proxy Hub, plugins in JavaScript (and the plugins repo), HTTP API, `tuic` CLI, TUIC SDK, MDKB integration |
| `/download.html` | Install | Per-OS buttons from the latest release, Homebrew / shell / PowerShell, system requirements, beta and nightly channels, link to release notes |
| `/blog.html` | Unchanged content | Restyled with the shared layout; posts grouped by area tag |
| `/docs/` | Unchanged | mdBook |

Six area pages instead of one page per feature: about 40 features would give 40 thin pages to keep current. If one area grows too big later (Tasks is the likely one), it splits then.

### Navigation

- Desktop: `Features ▾` (the six area pages) · `2.0` · `Docs` · `Blog` · `GitHub` · **Download** button.
- Mobile (≤ 768 px): a menu button opens the same list in an opaque panel.
- Footer: the same links plus Plugins, Plugin API, Discord, Releases, Changelog, licence.
- Every feature section ends with "Read the guide →" pointing at the matching `docs/user-guide/*.md` page under `/docs/`.

### Shared layout

The nav and footer move into two partial files, injected at build time by a small `transformIndexHtml` step in `vite.config.ts` (the file already does this for `%DISCORD_URL%`). Without it, nine pages repeat the same nav and drift. Area pages are added as Vite entries like the blog posts are. Existing blog posts keep their own markup in this pass; only `style.css` changes under them.

## 2a. Narrative backbone: the 2.0 film (Boss, 2026-10-25)

The site tells the film's story, in the film's order, with its captions as headlines. The outlook is **"TUICommander gives you superpowers"**: one person runs ten projects and dozens of agents and only follows one orchestrator. Each band reuses a film still where it reads better than a new screenshot (stills from the 1080p master at caption-free frames, or stage stills of the same shot).

| Site section (home band → area page) | Film beat (time, caption) |
|---|---|
| Hero: "10 projects. Dozens of agents. **One** of you." → "Meet TUICommander 2.0" + film | 1.1–2.1 (0:00–0:14), lockup 12.2 |
| Before / after strip: twelve loose windows vs "All ten. **One window.**" | 1.2, 2.2 (0:04–0:18) |
| **One window** → `/features/one-window.html`: "The sidebar tells you **everything**." · "Any agent. **One gesture.**" · "Read it like a **chat**." | 3.1–3.3 (0:14–0:30) |
| **Plan** → `/features/planning.html`: Ideas · "Plan your day **out loud**." · "Ideas become **stories**." · "Your pipeline, as a **graph**." · "Agents on a **schedule**." | 4.1–4.5 (0:30–1:07) |
| **Orchestrate** → `/features/orchestration.html`: "You ask **one** agent." · "Agents spawn **agents**." · "Agents talk to **each other**." · "See **who said what**." | 5.1–5.5 (1:07–1:25) |
| **Your tools** → `/features/workspace.html`: Files · "Your own tools, in **JavaScript**." · Markdown · Design Mode · memory | 6.1–6.5 (1:25–1:52) |
| **You just watch** (orchestration page, second half): "It knows **which** agent." · "What it's **doing**. How much **credit** is left." · "50 agents. One Mac. **No sweat.**" | 7.1–7.3 (1:52–2:04) |
| **Ship** → `/features/git-and-review.html`: "Your tracker, **next to your agents**." · "CI breaks. **The agent fixes it.**" · "Review the PR **in place**." · "From idea to **PR**." | 8.1–8.5, 10.3 (2:04–2:45, 3:07–3:19) |
| **Anywhere** → `/features/anywhere.html`: "Even when you're **out**." · "It pings you only when it **matters**." · voice | 9.1–9.2 (2:45–2:57), 4.2 |
| AI Chat band: "What shipped today?" + ego session settings | 10.1–10.2 (2:57–3:07) |
| "In **one day**." recap: the five-step loop (plan → spawn → watch → review → ship) | 10.4 (3:19–3:23) |
| **Extend** → `/features/extend.html`: "**MCP.** **Remote.** **Themes.** **Search.** **Plugins.**" | 11 (3:23–3:29) |
| Closing CTA: "Run ten projects. **Follow one orchestrator.**" · "Open source · macOS · Linux · Windows" · `brew install sstraus/tap/tuicommander` | 12.2 (3:29–3:37) |

Consequences for the rest of this document: the area pages follow the film acts (one-window, planning, orchestration, workspace, git-and-review, anywhere, extend: seven, not six); the hero line in section 6 is the film's opening and lockup; the highlight word in each headline is styled in Observation Blue bold (the film uses violet; the site keeps the app accent). Features the film does not show (resume after update, hibernation, tray, secrets, Telegram, Windows CI) go under "Shipped, not filmed" on `/2.0.html` and in the matching area page, never in a headline.

## 3. The video

### Placement

- **Home hero:** the film replaces the 1.x YouTube facade. A poster frame and a play button; nothing loads before the click (`preload="none"`). The film has music, so it never autoplays.
- **`/2.0.html`:** the same film with a chapter list. Each chapter seeks the video and links to the area page.
- **Optional, not proposed:** a silent 10–15 s loop for the hero. It adds a second asset to maintain; skip unless Fable or Boss want it.

### Chapters (from the as-built table in `script-v3.md`)

| Time | Chapter | Area page |
|---|---|---|
| 0:00 | One window for ten projects | home |
| 0:14 | Sidebar cards, `+` agent menu, CLI \| Chat | orchestration |
| 0:30 | Ideas and voice | planning / anywhere |
| 0:49 | Stories, Kanban, Workflow designer, Automations | planning |
| 1:07 | Agents spawn agents, mail, Progress Flow | orchestration |
| 1:25 | Files, plugins, Markdown, Design Mode, MDKB | workspace / extend |
| 1:52 | Questions, rate limits, Activity Dashboard | orchestration |
| 2:04 | Tasks: GitHub, Jira, GitLab | git-and-review |
| 2:24 | PR, CI auto-heal, line review | git-and-review |
| 2:45 | Phone | anywhere |
| 2:57 | AI Chat, ego session settings | orchestration |
| 3:07 | Git: diff, stage, commit | git-and-review |
| 3:23 | MCP, Remote, Prompts, Plugins, Themes, Search | extend |

### Web encode (measured)

Source: 1920×1080, 60 fps, H.264 4.27 Mb/s + AAC, 216.8 s, 121.6 MB. Trial encodes with `libx264 -preset slow -profile:v high -movflags +faststart`, AAC 96 kb/s, in `~/Gits/.tmp/website-video/` (not in the repo):

| Encode | Size |
|---|---|
| 1280×720, 30 fps, CRF 26 | **19.2 MB** |
| 1920×1080, 30 fps, CRF 28 | 25.9 MB |

Recommendation: **720p30, 19.2 MB**. It is under the 25 MB target. 30 fps loses nothing that matters: the film's motion is UI transitions. 1080p is sharper on small UI text in full screen but is just over the target; CRF 29 would bring it under, still to be checked by eye. Poster: a JPEG of the lockup or of the wide "All ten. One window." frame, about 80–150 KB.

### Where the file lives — decision for Boss

| Option | For | Against |
|---|---|---|
| **A. `website/public/video/` in the repo** (recommended) | Same origin, range requests and caching from Pages, no third party, no tracking, works offline in `vite preview`. One `<video>` tag. | +19 MB in git history, permanent; every re-encode adds again. Below the 50 MB GitHub warning. |
| B. YouTube (Boss uploads) | Adaptive bitrate, zero repo weight, the site already uses a YouTube facade. | Boss must upload; third-party embed and cookies; YouTube UI and recommendations around the film. |
| C. GitHub release asset | Zero repo weight. | The download URL redirects to `objects.githubusercontent.com` with an attachment disposition; seeking and Safari playback are not reliable for `<video>`. Not recommended. |

A and B combine well: self-host for the site, YouTube for sharing.

## 4. Feature evidence

Each feature below was checked on this branch (base `670429cd7`, current `main`). Only these go on the site.

| Feature | Verified in |
|---|---|
| Plans and Stories, List/Kanban, Workflow designer, runs | `docs/user-guide/native-stories.md`, `src/components/WorkflowDesigner/` |
| Automations | `docs/user-guide/automations.md` |
| Progress List/Flow | `docs/user-guide/project-progress.md` |
| CLI \| Chat | `docs/user-guide/terminals.md` |
| Tasks with GitHub, Jira, GitLab | `docs/user-guide/github-integration.md`, `plugins.md:44,262` — **Jira and GitLab are plugin providers**: the copy says so |
| PR review threads, CI auto-heal | `docs/user-guide/github-integration.md`, `docs/FEATURES.md` §8.5 |
| Design Mode | `docs/user-guide/design-mode.md` |
| Secrets | `docs/user-guide/secrets.md` |
| tuic-remote, SSH remote machines, mobile | `docs/user-guide/remote-access.md` (tuic-remote is labelled **Beta**: the copy says so) |
| Suspend Tab, hibernation | `docs/user-guide/terminals.md` |
| Voice | `docs/user-guide/dictation.md`: transcription runs locally with Whisper. **Spoken replies default to Microsoft Edge voices (network); Pocket TTS is the local option** with six bundled languages. The film's "on-device" caption must not be repeated as a blanket claim. |
| MDKB | An external tool TUIC integrates with (launch playbook, dashboard plugin). Shown under "extend", not as a built-in. |

Items marked unverified until phase 2: the six theme names in the UI, the settings search box, Telegram reply flow wording.

## 5. Screenshots

Every area page needs 3–6 real 2.0 images. Options, in order of preference:

1. **Stills from the presentation stage** (`tuic-presentation`, `render.mjs --stills`). The stage mounts the real 2.0.0-beta.2 components with fictional Halcyon data, so it gives clean, consistent, caption-free frames at 2× with no personal data. Stage-drawn parts (terminal replica, split panes, mail packets) are avoided or named as illustrations.
2. **`make agent-test`** for anything the stage does not cover (Design Mode, settings search, themes), with throwaway repositories.
3. Frames from the film only as a fallback: they are 1080p and most carry captions.

Never Boss's live instance. Images are WebP with a PNG fallback only if needed, lazy-loaded, with width and height set.

## 6. Content to write

| Item | Size |
|---|---|
| Home copy: new hero line, the five-step loop, six highlight bands, "New in 2.0" strip | ~600 words |
| `/2.0.html`: release story, area summaries, "shipped but not filmed", upgrade notes | ~900 words |
| Six area pages, each 4–8 features with a heading, two-sentence "why", a screenshot and a guide link | ~500–800 words each |
| `/download.html` | ~250 words |
| FAQ: drop the stale entries, update the rest (Aider removed, agent count, voice engines, Windows) | 6 on home |
| `og.png` 1200×630 from the lockup frame; meta descriptions per page; sitemap with every page and post; `robots.txt` and `og:url` on `tuicommander.com` | — |

Hero line proposal, from the film's lockup: **"Run ten projects. Follow one orchestrator."** with the current line "The IDE that understands AI agents" kept as the sub-heading. Marketing copy goes through `wiz:ls-style-editor`-style review rather than STE, per the repository language rule.

## 7. Visual direction

- Keep the brand: the `DESIGN.md` palette (Cool Stone / Slate neutrals, Observation Blue ≤ 10 % of a screen, semantic colours for status), JetBrains Mono for code and data, system sans for text, 3–4 px radii, 1 px Steel borders, flat surfaces.
- Fix the two rule breaks (G8): an opaque nav and no hero glow.
- The film highlights words in violet. The site keeps Observation Blue as its one accent so the site and the app look the same; violet stays the "merged" semantic colour.
- Marketing pages need a larger type scale than the app (the app's 12–14 px "no display sizes" rule is for the product UI). Proposal: a 16 px body, 40–56 px h1, 28 px h2, one weight step for emphasis, no gradient text.
- Layout: alternating text/screenshot bands on area pages; screenshots framed with a 1 px border and no shadow; a sticky in-page section index on area pages at ≥ 1024 px.
- Status dots and badges from the app appear in screenshots and in small inline "live state" chips, never as decoration.
- Accessibility: WCAG AA contrast, keyboard-reachable menu and video controls, captions are not needed (the film has no voice-over) but every chapter has a text label.

## 8. Phase 2 plan (after the go)

1. Shared layout partials, nav with mobile menu, CSS cleanup (G7, G8, G10).
2. Home and `/2.0.html` with the video (G1–G3).
3. Six area pages and `/download.html` (G9).
4. Agent grid from `src/agents.ts` (G4), `og.png`, sitemap, robots, meta (G5, G6).
5. Screenshots from the stage, then `agent-test` for the remainder, into `website/public/shots/`.
6. Checks: `pnpm build`; `vite preview` driven by agent-browser at 1440 px and 390 px; video plays and seeks; a link check over `dist/`; window screenshots into `website/.review/` (add it to `.gitignore`; it is not ignored today).
7. Local Conventional Commits on `feat/website-2.0`. No push, no deploy.

## 9. Decisions needed

1. Video host: **A (repo `public/`)**, B (YouTube), or A + B.
2. Encode: **720p30 19.2 MB** or 1080p30 CRF 29 (about 22–24 MB, to measure).
3. Seven area pages following the film acts (section 2a), or fewer.
4. Screenshot source: **presentation stage stills** first, `agent-test` for the rest.
5. Hero: the film opening "10 projects. Dozens of agents. One of you." with the lockup "Run ten projects. Follow one orchestrator." as CTA (section 2a), "The IDE that understands AI agents" as meta description.
