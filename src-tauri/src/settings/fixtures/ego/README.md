Recorded from installed ego 0.1.0 on 2026-10-13, with HOME, EGO_HOME and XDG paths isolated under ~/Gits/.tmp/ego-1775-capabilities. Commands: config ls --profile=alpha --json; models --json (first model retained); doctor --json (credentials section retained); auth status (empty stdout). The stored profile was populated through config set. The Python replay process emulates only these observed argument and JSON contracts; real installed-owner set/readback probes were also run. It never reads credentials.

The populated auth-status capture used a dummy api_key entry in the throwaway EGO_HOME only. Its stdout is `openrouter<TAB>api_key`; the dummy key did not appear. No production store was read.

Replay mutations use Python tomllib to parse the emitted assignment, so a malformed TOML argument fails independently of the adapter encoder. Installed-owner probes confirmed profile set/unset/ls, network, roots, thinking, mode, sandbox, budget, judge, and exec environment vocabulary.
