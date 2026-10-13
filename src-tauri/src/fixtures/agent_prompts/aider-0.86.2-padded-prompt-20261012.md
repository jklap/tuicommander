# Aider 0.86.2 padded prompt

Captured on 2026-10-12 from the agent-vm marker matrix (run aider-1791844512,
session bd07e009-3060-484b-a784-6287937a9e13, Aider v0.86.2 on Linux). The
`.tcap` is the unchanged capture: 41 rows, 128 columns, output records only.

Aider printed its banner and `Readonly: canary-rule.md`, then prompt_toolkit
painted the `>` prompt followed by about 32 blank rows and moved the cursor back
up (`ESC[3A`, later `ESC[32A`). The matrix then got HTTP 409
`agent_not_ready composer_state=empty` on the first submission (story 1822-5e0f).

The capture holds no credentials: the key was passed in the environment and
Aider never echoes it. Checked with a grep for common key prefixes.
