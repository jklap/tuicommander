//! Write OSC 7770 `verb=payload` pairs to the resolved tty in one `write_all`.

use crate::tty;
use std::io::Write;

/// One verb/payload pair to emit. `verbatim` is for fixed enum/numeric
/// payloads (`state`, `toolfail`) — kept byte-identical to the original shell
/// hooks' wire format for backward compatibility with any still-installed
/// old-format hook, and safe because they can never contain a byte
/// percent-encoding would need to touch. `encoded` percent-encodes at
/// construction time, for free-text payloads (`cwd`, `tool`, `notify`, …).
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    // All TUIC_HOOK_TTY-touching cases share one `#[test]` fn, sequential
    // within it — the harness runs tests in parallel threads within this
    // binary, and TUIC_HOOK_TTY is a process-global env var (same hazard
    // `tty::tests::tuic_hook_tty_env_seam` documents for itself). Each case
    // below points at its own fresh tempfile so cases can't read back
    // another case's bytes even if this function's own env mutations were
    // somehow interleaved with a concurrent test elsewhere in the crate.
    //
    // `#[serial_test::serial]` (bare, no key — the crate's one shared group)
    // makes that "somehow" impossible rather than just unlikely: every
    // TUIC_HOOK_TTY-touching test in this binary (this fn, the one below, and
    // `tty::tests::tuic_hook_tty_env_seam`) carries the same attribute, so
    // cargo test's default parallel threads can't interleave any of them.
    // Without it, correctness depended entirely on nextest's one-process-
    // per-test default (`.config/nextest.toml`) rather than anything in code.
    #[test]
    #[serial_test::serial]
    fn emit_writes_expected_bytes_via_the_tty_seam() {
        let dir = std::env::temp_dir();

        // Golden bytes for a single verbatim emission.
        let path1 = dir.join(format!("tuic-hook-emit-test-{}-1", std::process::id()));
        fs::write(&path1, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path1) };
        emit(&[Emission::verbatim("state", "busy")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path1)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;state=busy\x1b\\");
        fs::remove_file(&path1).ok();

        // Two pairs concatenate into two complete sequences, in the order given.
        let path2 = dir.join(format!("tuic-hook-emit-test-{}-2", std::process::id()));
        fs::write(&path2, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path2) };
        emit(&[
            Emission::verbatim("toolfail", "1"),
            Emission::verbatim("state", "idle"),
        ]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path2)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;toolfail=1\x1b\\\x1b]7770;state=idle\x1b\\");
        fs::remove_file(&path2).ok();

        // `encoded()` percent-encodes at construction time — reserved OSC
        // delimiter and control bytes must not reach the wire literally.
        let path3 = dir.join(format!("tuic-hook-emit-test-{}-3", std::process::id()));
        fs::write(&path3, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path3) };
        emit(&[Emission::encoded("notify", "a;b\x1bc")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path3)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;notify=a%3Bb%1Bc\x1b\\");
        fs::remove_file(&path3).ok();

        // Empty slice: no write at all, file stays exactly as it was.
        let path4 = dir.join(format!("tuic-hook-emit-test-{}-4", std::process::id()));
        fs::write(&path4, b"untouched").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path4) };
        emit(&[]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path4)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "untouched");
        fs::remove_file(&path4).ok();
    }

    #[test]
    #[serial_test::serial]
    fn emit_is_a_silent_noop_when_the_tty_cannot_be_opened() {
        // A target that does not exist: OpenOptions::open (no .create(true))
        // fails, and emit() must swallow that rather than panicking.
        let path = std::env::temp_dir().join(format!(
            "tuic-hook-emit-test-{}-missing-{}",
            std::process::id(),
            std::process::id()
        ));
        fs::remove_file(&path).ok();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path) };
        emit(&[Emission::verbatim("state", "busy")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        assert!(!path.exists(), "must never create the target file");
    }
}
