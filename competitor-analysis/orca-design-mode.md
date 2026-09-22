# Orca Design Mode — decision record

The earlier competitor-analysis file referenced by `plans/design-mode.md` was not present in this checkout or its available Git history. This record preserves the Design Mode decision and the evidence needed to review it.

## 6. Interaction architecture

**Option B is selected:** open a separate headed Chrome window with a dedicated per-repository profile. TUICommander attaches over CDP, arms Chrome's native element inspector, and captures a selected element. The grab is pasted into the terminal that started inspection without submitting the agent prompt. A second start for the same repository rebinds that window to the new agent session.

An embedded CDP screencast was rejected. It would require TUICommander to redraw frames and forward pointer, keyboard, clipboard and IME input, adding latency and input fidelity work to a feature whose primary action is a single click. The [Edge Tools screencast CPU report](https://github.com/microsoft/vscode-edge-devtools/issues/931) documents jank and high CPU while a screencast is visible. The [deprecated Browser Preview project](https://github.com/auchenberg/vscode-browser-preview) illustrates the separate headless-process and embedded-preview approach; neither is necessary when native Chrome can display and highlight the page itself. The Edge report is evidence of a concrete risk, not a universal benchmark for every screencast implementation.

React source lookup must not assume `_debugSource` exists on every React 19 fiber. [React issue #32574](https://github.com/facebook/react/issues/32574) records its removal in React 19. Development builds that expose `_debugStack` need their JSX call-site frame mapped through the bundler's source map; `_debugSource` remains a fallback for older or transformed builds. [show-component's implementation](https://github.com/sidorares/show-component) demonstrates the `_debugStack` plus source-map approach. Production React builds may expose neither hint, so source location is optional rather than fabricated.

The native-window choice also keeps hover highlighting inside Chrome and avoids a second live rendering pipeline in TUICommander. Cross-origin iframe selection remains deferred until a real use case establishes that the main CDP target is insufficient.
