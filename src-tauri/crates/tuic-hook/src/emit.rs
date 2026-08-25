//! Write OSC 7770 `verb=payload` pairs to the resolved tty in one `write_all`.

use crate::tty;
use std::io::Write;

/// One verb/payload pair to emit. `verbatim` is for fixed enum/numeric
/// payloads (`state`, `toolfail`) — kept byte-identical to the original shell
/// hooks' wire format for backward compatibility with any still-installed
/// old-format hook, and safe because they can never contain a byte
/// percent-encoding would need to touch. `encoded` percent-encodes at
/// construction time, for free-text payloads (`cwd`, `tool`, `notify`, …).
pub struct Emission {
    pub verb: &'static str,
    pub payload: String,
}

impl Emission {
    pub fn verbatim(verb: &'static str, payload: impl Into<String>) -> Self {
        Self {
            verb,
            payload: payload.into(),
        }
    }

    pub fn encoded(verb: &'static str, payload: impl AsRef<str>) -> Self {
        Self {
            verb,
            payload: crate::payload::encode(payload.as_ref()),
        }
    }
}

/// Emit every pair as one OSC 7770 sequence per pair, all delivered in a
/// single `write_all` call against the resolved tty. Silent no-op (never an
/// error, never a panic) if the tty can't be resolved or opened — a hook must
/// never block or fail the agent.
pub fn emit(pairs: &[Emission]) {
    if pairs.is_empty() {
        return;
    }
    let Some(path) = tty::resolve() else {
        return;
    };
    if std::env::var("TUIC_HOOK_DEBUG").is_ok_and(|v| !v.is_empty()) {
        eprintln!("tuic-hook: resolved tty = {}", path.display());
    }
    // Not `.create(true)` and not `.truncate(true)`: this must never create a
    // device node, and on the rare non-device target (the TUIC_HOOK_TTY test
    // seam) truncating would erase whatever another hook invocation in the
    // same test already wrote there.
    let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(&path) else {
        return;
    };

    let mut buf = Vec::new();
    for pair in pairs {
        buf.extend_from_slice(b"\x1b]7770;");
        buf.extend_from_slice(pair.verb.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(pair.payload.as_bytes());
        buf.extend_from_slice(b"\x1b\\");
    }
    let _ = file.write_all(&buf);
}
