# Plugin desktop checks — 1756-4d42

Date: 2026-10-20. Owner: desk-1756. Core: `cc43d554c`; plugins: `56659df3cf53edb1e1fc04a70b19f88d1a6c9b06`.

## Scope and contract

Use the real desktop plugin loader, native storage and live frontend registries. Do not replace host methods or vendor clients. Local account **bindings** have no credentials and make no vendor request. This proves registration and settings persistence, not authenticated vendor behavior.

The isolated desktop was built and started with:

```sh
export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
make agent-test AGENT_TEST_INSTANCE=at-1756 AGENT_TEST_PORT=19882
```

`mbx doctor` reported zero failures. The native application used embedded frontend assets. No Rust test, rb run, Vite server or browser session was started. The build/runtime logs are `~/Gits/.tmp/desk-1756-agent-test.log` and `~/Gits/.tmp/desk-1756-agent-test2.log`.

TCP requests returned 401. After coordinator authorization, all probes used only the instance's own Unix socket:

```sh
curl --unix-socket "$HOME/Gits/.tmp/tuic-at/643c8eb2d49026ad/tuic-mcp-643c8eb2d49026ad.sock" \
  http://localhost/health
```

Health returned `ok:true`, zero sessions, version `2.0.0`, native target `aarch64-apple-darwin`, binary SHA-256 `d329ea5dee6893413218c7cb3365a9b57d8c4780bb4b7ca5d54c8a02e9d110dd`.

The WebView answered `POST /debug/invoke_js` and wrote results to `/logs?source=eval_js`. One `POST /debug/reload_webview` was issued after an initial empty result. Both the initial delayed evaluation and subsequent evaluations returned. This is not a claim that story 2090's hidden-window liveness problem is fixed.

## Candidate 3 — PASS for the specified local E2E

The real GitHub Work, GitLab and Jira packages were zipped from the pinned plugin tree and installed through native `install_plugin_from_zip`. Each first loaded without accounts and reported its explicit setup warning.

Then native `write_plugin_data` wrote non-secret `work-items.json` bindings:

- GitHub: account ID `1756-github-local`, repository `local/1756`.
- GitLab: account ID `1756-gitlab-local`, host `https://gitlab.com`, bearer kind.
- Jira: account ID `1756-jira-local`, host `https://1756.invalid`, bearer kind.

These are local configuration inputs, not authenticated accounts or vendor fixtures. No list/detail/action method was invoked. The unchanged production loader's `reloadPlugin` was awaited twice for each plugin. After **each** reload, the actual `workItemProviderRegistry.list(providerId)` contained exactly one matching account registration and the plugin had `loaded:true`, `error:null`. No duplicate-registration exception occurred.

`provider-reloads.js` records the exact native-WebView check. The compiled chunk export names are specific to this build; they were resolved from its generated assets, not guessed or replaced. Importing an already-loaded asset returns the production module singleton.

An installed, real error-path plugin used this entry point:

```js
export default {
  id: "live-1756-failure",
  async onload() {
    await Promise.resolve();
    throw new Error("1756 deliberate async startup rejection");
  },
  onunload() {},
};
```

Its manifest used version/minimum `1.0.0` and no capabilities. The live plugin state settled to `loaded:false`, with that exact error. Plugin logs contained `onload failed: 1756 deliberate async startup rejection`. No mocked host or external server was used.

Evidence: `providers-result.json`, `settled-result.json`, `plugin-log.json`.

## Candidate 4 — BLOCKED by published catalogue prerequisite

The real native `fetch_plugin_registry` returned the public catalogue. GitHub Work, GitLab and Jira entries each have version `1.0.0`, minimum `1.8.0`, and `net:http`, but **no `contentSha256`**. Installed manifests have backend-computed digests. In the actual production `registryStore`, `hasUpdate(id, version, digest)` returned `null` for all three.

The pinned local `plugins/registry.json` contains digests, but that is not the published source the running catalogue fetches. This session did not inject a replacement catalogue, change the production URL, publish a release, or claim that source and installed bytes were compared. Required next step: publish the digest-bearing catalogue and matching release payloads under separate authorization, then run the stale-same-version check against that real source.

Evidence: `catalogue-result.json`. This is a missing distribution prerequisite, not proof that the implemented digest comparator failed.

## Candidate 9 — PARTIAL; vendor use still open

The native Settings UI displayed separate GitLab and Jira plugin pages with the declared default-project fields. Through each real input's `input` and `blur` handlers, the check saved:

- Jira `projectId = LOCAL-1756`.
- GitLab `project = local/1756`.

The native settings service wrote the matching `data/config.json` values. Jira's input also displayed its persisted value. Reloading the actual providers succeeded with those saved settings. No plugin data file was edited to set these defaults.

This proves the UI and persistence path. It does **not** prove that a vendor accepts or uses the project, or that account-specific defaults win in a live vendor request. The original criterion remains unchecked because its E2E says “set and use a project ID.” Such use requires Boss's authorized vendor account.

Evidence: `jira-dom.json`, `jira-saved.json`, `providers-result.json`.

## Candidate 11 — PASS for native refusal, combined with existing metadata evidence

The real installed/provider catalogue entries exposed minimum `1.8.0` and `net:http`. Existing story evidence covers catalogue/manifest parity and the SQLite ES2022 packaging contract; `plugins/sqlite-viewer/README.md` still declares the explicit `--target=es2022` build command. This session did not rebuild the SQLite vendor bundle.

A native ZIP install used a real plugin with ID `live-1756-incompatible`, package version `1.0.0`, `minAppVersion:9999.0.0`, and no capabilities. Its `onload` would throw `must never run` if evaluated. Installation stored the package, but the production loader refused to load it and logged:

> plugin "live-1756-incompatible" requires app version 9999.0.0, current is 2.0.0

No `must never run` error occurred; the plugin was absent from the loaded plugin store. The refusal is at the loader boundary, not a claim that ZIP extraction rejects incompatible files.

Evidence: `plugin-log.json`, `settled-result.json`.

## Remaining vendor-dependent checks

- Candidate 9: use the saved project ID/path in an authenticated provider request.
- Candidate 10: resolve a real Jira URL and cancel its in-flight vendor requests.
- Candidate 12: route a real GitHub review/action through core and the plugin.

No login, credential read, vendor write, global setting mutation, or daily-instance interaction was performed. No additional candidate was claimed complete from module evidence.

## Cleanup and validation

`make agent-test-stop AGENT_TEST_INSTANCE=at-1756 AGENT_TEST_PORT=19882` succeeded after each launch. After the final stop:

```sh
ps -axo pid=,command= | grep "$PWD/.tmp/agent-test" | grep -v '[g]rep'
```

returned no processes. Test-only plugin packages and configuration remain within `instances/at-1756`; the instance is stopped. No Vite or agent-browser process was started. Required evidence is retained here; scratch ZIPs and helper files are removed after this report.

The instruction-link checker passed. `fmt-changed.sh` is the relevant final static gate for this evidence-only change. Story completion remains blocked by candidates 4, 9, 10 and 12; no unchecked criterion is silently skipped.
