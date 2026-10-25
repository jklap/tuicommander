# Italian speech investigation — 1996-3cff

## Scope

Boss authorized isolated speech output with microphone capture disabled on 2026-10-25. The worktree used `agent-test-1996`, port `19996`, and its own Unix socket. No daily-instance API, configuration, microphone, or installed executable was accessed.

The target was a throwaway PTY running `/bin/cat` with an explicit test agent identity, not a second AI worker. Model notices and earcons were off. The text was:

> Ciao, questa è una risposta in italiano. Una sola voce legge questa frase.

## Observations

The initial effective configuration reported `speech_engine=edge`. An HTTP save retained that value, selected `language=it` and `speech_edge_voice=it-IT-ElsaNeural`, and added an inactive external command `["say", "-o", "{out}", "{text}"]`.

The subsequent configuration read unexpectedly reported `speech_engine=external`. The saved raw engine remained empty. At `2026-10-25T17:15:42.131817Z`, the actual reply logged:

```text
Speech synthesis start source="dictation" engine="External speech engine" voice="" language=
```

It failed at `17:15:42.683108Z`; `say` reported `Opening output file failed: fmt?`. This is a wrong-engine start, not two successful outputs. The command was introduced by this controlled reproduction; it is not evidence of Boss's historical configuration.

After a second save explicitly changed External to Edge, the same text logged:

```text
2026-10-25T17:16:36.376200Z Speech synthesis start source="dictation" engine="Microsoft Edge" voice="it-IT-ElsaNeural" language=it
2026-10-25T17:16:43.226309Z Speech synthesis complete source="dictation" engine="Microsoft Edge" voice="it-IT-ElsaNeural" language=it succeeded=true
```

The utterance reached `finished`, with no error and one synthesis start for that request. The coordinator relayed Boss's assessment, “audio italiano perfetto”. This is non-reproduction evidence for the historical two-English-voices report.

Hands-free reported `armed=true`, `listening=false`; a push-to-talk request was refused as non-owner. The instance retained non-owner global-input status. Cleanup disarmed the conversation, deleted its PTY and stopped only the named supervisor.

## Proven defect and fix

`get_dictation_config` resolves an empty stored engine into a user-visible engine. The old save applied the caller's delta to the unnormalized disk structure. Because the form's unchanged Edge choice was absent from that delta, the stored engine stayed empty. Adding an inactive command then caused legacy resolution to choose External, ignoring the Italian Edge voice.

The fix normalizes the latest configuration while holding both configuration locks, then applies only the fields the caller changed. This pins the resolved choice without overwriting a concurrent explicit engine selection. Recovery still rechecks the latest document under the lock.

The RED regression is `saving_an_italian_edge_reply_never_selects_a_stale_system_command_1996`. On pre-fix HEAD `5e1f47c686facc3ca0467914a311d918abb052ac` plus the uncommitted diagnostics harness and regression, rb selected one test and failed at the intended assertion: actual `external`, expected `edge`. Compilation took 5m33s; the test took 0.039s. The source was not normalized until after this failure.

Command:

```sh
build-slot.sh --remote cargo nextest run --manifest-path src-tauri/Cargo.toml --features dictation --lib -E 'test(saving_an_italian_edge_reply_never_selects_a_stale_system_command_1996)'
```

The runner reported `RB_BOX_DIRTY=no` and source-tree drift because documentation and adjacent tests were added after synchronization; the regression and pre-fix save implementation were unchanged. No post-fix GREEN test run is claimed: batch integration owns that validation.

## Remaining uncertainty

The historical failing build, its configuration, and its complete tool-call sequence have not been captured. There is no evidence here that two engines start for one request, or that this configuration-save defect caused Boss's original report. Do not close that attribution criterion based on the successful Italian reply or the newly reproduced save defect.

Local raw evidence is retained under `~/Gits/.tmp/results/tuic-1996-evidence/`, `tuic-1996-agent-test.log`, and `tuic-1996-red.log`; rb result `20261025T181323Z-61766`.
