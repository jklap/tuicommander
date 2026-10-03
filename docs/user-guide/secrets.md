# Private secret forms

An agent can request named username, password, OTP or SSO-link fields with the
`secret` MCP tool. TUICommander opens a separate native window containing only
those fields. The tool receives names and `stored` or `declined`, never values.
SSO links are displayed as text; they do not navigate automatically.

The form states: **the agent cannot read this value, but a command you approve
can send it anywhere**. Approve only commands you trust. This boundary protects
TUICommander tool results and its entry UI; it is not an OS sandbox against an
agent running as the same user, or protection against arbitrary transformations
or exfiltration by the approved executable.

Values exist only in backend memory. Removal and normal application exit
zeroize them. Restart clears values and templates; `run` reports `missing`.
There is no Keychain entry or configuration file for this feature.

## Run and consent

`secret action=run names=["GITHUB_TOKEN"] argv=["gh","api","user"] cwd="/absolute/project"`
executes the resolved executable directly, with no shell. Requested values are
injected only into that child's environment. The child inherits only core
path, home, locale and temporary-directory variables. It has no terminal input,
and stdout/stderr are captured in pipes rather than a PTY or a tcap stream.

Commands outside the in-memory allowlist show the exact resolved argv, directory
and secret names for consent. You can retain a template until exit. Templates
fix the executable and first subcommand. A whole `{arg}` placeholder accepts
one non-option argument containing ASCII letters, digits or `_./:@+=,-`; it
cannot inject spaces, shell operators, another subcommand or another argument.
In this slice, placeholders are supported only after `gh api`, whose command
prefix is known. Other programs use exact argv templates.
Consent also binds the names and canonical working directory.

Shell/interpreter evaluation (`-c`, `-e` and equivalents), `env` and `printenv`
are denied even with approval. Evaluation-like options are rejected regardless
of the executable name, so renaming an interpreter does not bypass the policy. Captured output masks exact values, standard and
URL-safe base64, lower/upper hex and URL/form encodings, including line breaks
inside an echo. Output over 1 MiB per stream or a 120-second timeout is withheld.
No raw output is logged, journalled, or added to a terminal ring.

## HTTP and phone entry

The private window shows a capability link for a dedicated, temporary web
origin. The link carries a one-time nonce in its fragment; the form removes it
from history immediately. A nonce is bound to one open form and is consumed
only by a valid submit. Replay and guessed nonces are rejected. Responses are
not cached or frameable. The private origin does not serve a service worker.

Phone entry requires the host's existing Tailscale HTTPS configuration. Without
that configuration, the private listener binds loopback and the link works only
in a local browser. Plaintext LAN password submission is not enabled. A desktop
host must open the request; headless daemons cannot initiate this form yet.

While the private native window is open, TUIC blocks `ui`, `debug`, and upstream
MCP calls, including proxied screenshots. In-flight inspection results are
withheld if a form opened during the call. The separate form has no App stores,
debug globals, logging handlers, plugin bridge or terminal. Bootstrap checks the
native window identity; the main WebView cannot acquire its nonce. Inputs are
cleared and unmounted before submit begins, and the native window is destroyed
after successful entry. OS-level screen capture outside TUIC is not controlled
by this mechanism.
