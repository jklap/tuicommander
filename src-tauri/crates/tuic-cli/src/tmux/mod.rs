//! `tuic`-as-`tmux` compatibility shim.
//!
//! Entered from `main()` when `argv[0]`'s file name is `tmux`/`tmux.exe`
//! (installed via `tuic alias`). Structure, per `tmux-swarm-shim.md`
//! (repo root):
//!
//! - [`args`] — pure argv parsing: global options (`-L`/`-S`/`-V`, which
//!   always precede the subcommand) and per-subcommand flags, into a
//!   backend-agnostic [`args::TmuxOp`].
//! - [`format`] — pure `#{...}` format-string rendering for `-F`/`-p`.
//! - [`target`] — pure `-t` target parsing and resolution against a fetched
//!   topology snapshot.
//! - [`exec`] — the only I/O: executes one `TmuxOp` against a
//!   [`exec::TuicBackend`] (a real IPC client in production, an in-memory
//!   fake in tests) and returns an [`exec::Outcome`] — lines to print, never
//!   printed directly.
//!
//! This module owns argv parsing, `#{…}` rendering, target resolution and
//! exit codes. The pane *topology* itself (sessions → windows → panes → TUIC
//! session uuid) lives app-side, behind `/tmux/*` HTTP routes
//! (`src-tauri/src/mcp_http/tmux_routes.rs`) — see that module's doc comment
//! for why.

pub(crate) mod args;
mod exec;
mod format;
mod target;

use std::io::Write;

/// Entry point called from `main()`. Never returns.
pub(crate) fn tmux_compat() -> ! {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    log_invocation(&argv);
    let outcome = run(&argv, &exec::IpcBackend);
    log_outcome(&argv, &outcome);

    for line in &outcome.stdout {
        println!("{line}");
    }
    for line in &outcome.stderr {
        eprintln!("{line}");
    }
    // `std::process::exit` skips the runtime's own stdout/stderr flush — the
    // pre-refactor `has-session` arm called `process::exit` straight out of
    // its match arm and got away with it only because nothing else ran
    // after. Flush explicitly now that exit codes are plain data.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(outcome.exit);
}

/// Pure-argv-in, `Outcome`-out. Split out from [`tmux_compat`] so tests can
/// drive the whole parse→execute pipeline against a fake backend without
/// touching `std::env::args` or `std::process::exit`.
fn run(argv: &[String], backend: &dyn exec::TuicBackend) -> exec::Outcome {
    match args::parse_tmux(argv) {
        Ok((globals, op)) => exec::execute(op, &globals, backend),
        Err(e) => exec::Outcome::usage_error(&e),
    }
}

// ---------------------------------------------------------------------------
// Invocation logging (dual sink) — see `docs/user-guide/cli.md`'s tmux
// section for the user-facing description.
// ---------------------------------------------------------------------------

fn log_enabled() -> LogMode {
    match std::env::var("TUIC_TMUX_LOG") {
        Ok(v) if v == "0" => LogMode::Off,
        Ok(v) if v.eq_ignore_ascii_case("stderr") => LogMode::FileAndStderr,
        _ => LogMode::File,
    }
}

enum LogMode {
    Off,
    File,
    FileAndStderr,
}

fn log_line(level: &str, message: &str) {
    match log_enabled() {
        LogMode::Off => return,
        LogMode::FileAndStderr => eprintln!("tuic-tmux-shim [{level}]: {message}"),
        LogMode::File => {}
    }
    append_log_file(level, message);
    post_app_log(level, message);
}

fn append_log_file(level: &str, message: &str) {
    let dir = crate::ipc::config_dir().join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("tmux-shim.log");
    rotate_if_large(&path);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("{now} [{level}] {message}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Size-rotate at 1 MiB, keeping one previous generation (`.1`) — enough to
/// answer "what did Claude Code just call" without growing unbounded across
/// a long swarm session.
fn rotate_if_large(path: &std::path::Path) {
    const MAX_BYTES: u64 = 1024 * 1024;
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < MAX_BYTES {
        return;
    }
    let rotated = path.with_extension("log.1");
    let _ = std::fs::rename(path, rotated);
}

/// Best-effort: lands the entry in the in-app Logs panel and the daily
/// `tuic.log` file via the existing `POST /logs` route
/// (`mcp_http/log_routes.rs`) — but only when TUICommander is actually
/// running, which the file sink above does not require.
fn post_app_log(level: &str, message: &str) {
    if !crate::ipc::is_running() {
        return;
    }
    let level = match level {
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    };
    let body = serde_json::json!({
        "level": level,
        "source": "tmux-shim",
        "message": message,
    });
    let _ = crate::ipc::post("/logs", &body.to_string());
}

fn log_invocation(argv: &[String]) {
    let cwd = std::env::current_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    log_line("info", &format!("tmux {} (cwd: {cwd})", argv.join(" ")));
}

fn log_outcome(argv: &[String], outcome: &exec::Outcome) {
    // The whole point of this shim's logging: an unhandled subcommand must
    // never disappear silently. This is also how a future session answers
    // `tmux-swarm-shim.md`'s deferred §5.0 (finding the real teammate-spawn
    // trigger) — by reading what actually got called. The `warn` line
    // repeats the full argv (not just a cross-reference to the paired
    // `info` invocation line) so it is self-contained when filtered on its
    // own via `GET /logs?source=tmux-shim&level=warn`.
    if outcome.exit == 0 {
        log_line(
            "info",
            &format!("exit=0 stdout_lines={}", outcome.stdout.len()),
        );
        return;
    }
    log_line(
        "warn",
        &format!(
            "exit={} argv=[{}] stderr={:?}",
            outcome.exit,
            argv.join(" "),
            outcome.stderr
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::exec::TuicBackend;
    use super::run;
    use serde_json::{Value, json};
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// In-memory backend recording every call, for full parse→execute
    /// pipeline tests with no socket.
    #[derive(Default)]
    struct FakeBackend {
        sessions: RefCell<Vec<Value>>,
        topology: RefCell<HashMap<String, Value>>,
        writes: RefCell<Vec<(String, String)>>,
        next_uuid: RefCell<u32>,
        /// Simulates a freshly-materialized PTY that dies before it can be
        /// written to (confirmed live, see `RespawnPane`'s doc comment in
        /// `exec.rs`): the next N `write()` calls fail with "Session not
        /// found" regardless of session id, then writes succeed normally.
        fail_next_n_writes: RefCell<u32>,
        /// Tags recorded by `dispatch_legacy` so tests can confirm the
        /// byte-identical arms (`list-sessions`, `capture-pane`,
        /// `attach-session`, bare `tmux`) actually reach it, without needing
        /// `Command` to implement `Debug`/`PartialEq`.
        legacy_calls: RefCell<Vec<&'static str>>,
        /// `repo` from every legacy `Command::New` seen, in order — kept
        /// separate from `legacy_calls` (which only tags *which* legacy
        /// command ran) so a test can assert `resolve_cwd()`'s fallback
        /// never leaks into the legacy `tuic alias` path, which must stay
        /// byte-identical for pre-existing non-swarm users.
        legacy_new_repo: RefCell<Vec<Option<String>>>,
    }

    impl FakeBackend {
        fn mint_uuid(&self) -> String {
            let mut n = self.next_uuid.borrow_mut();
            *n += 1;
            format!("11111111-1111-1111-1111-{:012}", *n)
        }

        fn topology_for(&self, label: &str) -> Value {
            self.topology
                .borrow()
                .get(label)
                .cloned()
                .unwrap_or_else(|| json!({ "sessions": [] }))
        }
    }

    impl TuicBackend for FakeBackend {
        fn is_running(&self) -> bool {
            true
        }
        fn write(&self, session_id: &str, data: &str) -> Result<(), String> {
            let mut remaining = self.fail_next_n_writes.borrow_mut();
            if *remaining > 0 {
                *remaining -= 1;
                return Err("Session not found".to_string());
            }
            self.writes
                .borrow_mut()
                .push((session_id.to_string(), data.to_string()));
            Ok(())
        }
        fn get_topology(&self, label: &str) -> Result<Value, String> {
            Ok(self.topology_for(label))
        }
        fn create_tmux_session(
            &self,
            label: &str,
            name: &str,
            window_name: Option<&str>,
            cwd: Option<&str>,
        ) -> Result<Value, String> {
            let mut topo = self.topology_for(label);
            let sid = format!(
                "${}",
                topo["sessions"].as_array().map(|a| a.len()).unwrap_or(0)
            );
            let wid = "@0".to_string();
            let pid = "%0".to_string();
            let session = json!({
                "id": sid, "name": name, "active_window": wid,
                "windows": [{
                    "id": wid, "name": window_name.unwrap_or("0"), "index": 0,
                    "active_pane": pid,
                    "panes": [{ "id": pid, "index": 0, "title": Value::Null, "cwd": cwd, "tuic_session_id": Value::Null }]
                }]
            });
            topo["sessions"].as_array_mut().unwrap().push(session);
            self.topology.borrow_mut().insert(label.to_string(), topo);
            Ok(json!({ "session_id": sid, "window_id": wid, "pane_id": pid }))
        }
        fn delete_tmux_session(&self, label: &str, session_id: &str) -> Result<(), String> {
            let mut topo = self.topology_for(label);
            let mut materialized_ids = Vec::new();
            if let Some(arr) = topo["sessions"].as_array_mut() {
                arr.retain(|s| {
                    let keep = s["id"].as_str() != Some(session_id);
                    if !keep {
                        for window in s["windows"].as_array().into_iter().flatten() {
                            for pane in window["panes"].as_array().into_iter().flatten() {
                                if let Some(id) = pane["tuic_session_id"].as_str() {
                                    materialized_ids.push(id.to_string());
                                }
                            }
                        }
                    }
                    keep
                });
            }
            self.topology.borrow_mut().insert(label.to_string(), topo);
            // Mirrors the real handler (tmux_routes.rs), which closes each
            // removed session's materialised panes' PTYs too.
            self.sessions.borrow_mut().retain(|s| {
                !materialized_ids
                    .iter()
                    .any(|id| s["session_id"].as_str() == Some(id))
            });
            Ok(())
        }
        fn create_tmux_window(
            &self,
            label: &str,
            session_id: &str,
            name: Option<&str>,
            cwd: Option<&str>,
        ) -> Result<Value, String> {
            let mut topo = self.topology_for(label);
            let arr = topo["sessions"].as_array_mut().unwrap();
            let session = arr
                .iter_mut()
                .find(|s| s["id"].as_str() == Some(session_id))
                .ok_or("no such session")?;
            let idx = session["windows"].as_array().unwrap().len();
            let wid = format!("@{idx}");
            let pid = format!("%{idx}0");
            session["windows"].as_array_mut().unwrap().push(json!({
                "id": wid, "name": name.unwrap_or("0"), "index": idx,
                "active_pane": pid,
                "panes": [{ "id": pid, "index": 0, "title": Value::Null, "cwd": cwd, "tuic_session_id": Value::Null }]
            }));
            self.topology.borrow_mut().insert(label.to_string(), topo);
            Ok(json!({ "window_id": wid, "pane_id": pid }))
        }
        fn create_tmux_pane(
            &self,
            label: &str,
            window_id: &str,
            cwd: Option<&str>,
        ) -> Result<Value, String> {
            let mut topo = self.topology_for(label);
            let mut found = false;
            let mut pane_id = String::new();
            let mut tuic_id = String::new();
            for session in topo["sessions"].as_array_mut().unwrap() {
                for window in session["windows"].as_array_mut().unwrap() {
                    if window["id"].as_str() == Some(window_id) {
                        let idx = window["panes"].as_array().unwrap().len();
                        pane_id = format!("%{window_id}-{idx}");
                        tuic_id = self.mint_uuid();
                        self.sessions
                            .borrow_mut()
                            .push(json!({ "session_id": tuic_id, "display_name": Value::Null }));
                        window["panes"].as_array_mut().unwrap().push(json!({
                            "id": pane_id, "index": idx, "title": Value::Null,
                            "cwd": cwd, "tuic_session_id": tuic_id,
                        }));
                        found = true;
                    }
                }
            }
            if !found {
                return Err("no such window".to_string());
            }
            self.topology.borrow_mut().insert(label.to_string(), topo);
            Ok(json!({ "pane_id": pane_id, "tuic_session_id": tuic_id }))
        }
        fn materialize_pane(
            &self,
            label: &str,
            pane_id: &str,
            _cwd: Option<&str>,
        ) -> Result<String, String> {
            let mut topo = self.topology_for(label);
            let uuid = self.mint_uuid();
            let mut found = None;
            for session in topo["sessions"].as_array_mut().unwrap() {
                for window in session["windows"].as_array_mut().unwrap() {
                    for pane in window["panes"].as_array_mut().unwrap() {
                        if pane["id"].as_str() == Some(pane_id) {
                            pane["tuic_session_id"] = json!(uuid);
                            found = Some(uuid.clone());
                        }
                    }
                }
            }
            self.topology.borrow_mut().insert(label.to_string(), topo);
            match found {
                Some(id) => {
                    self.sessions
                        .borrow_mut()
                        .push(json!({ "session_id": id, "display_name": Value::Null }));
                    Ok(id)
                }
                None => Err("no such pane".to_string()),
            }
        }
        fn rename_pane(
            &self,
            label: &str,
            pane_id: &str,
            title: Option<&str>,
        ) -> Result<(), String> {
            let mut topo = self.topology_for(label);
            for session in topo["sessions"].as_array_mut().unwrap() {
                for window in session["windows"].as_array_mut().unwrap() {
                    for pane in window["panes"].as_array_mut().unwrap() {
                        if pane["id"].as_str() == Some(pane_id) {
                            pane["title"] = json!(title);
                        }
                    }
                }
            }
            self.topology.borrow_mut().insert(label.to_string(), topo);
            Ok(())
        }
        fn kill_pane(&self, label: &str, pane_id: &str) -> Result<(), String> {
            let mut topo = self.topology_for(label);
            for session in topo["sessions"].as_array_mut().unwrap() {
                for window in session["windows"].as_array_mut().unwrap() {
                    window["panes"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|p| p["id"].as_str() != Some(pane_id));
                }
            }
            self.topology.borrow_mut().insert(label.to_string(), topo);
            Ok(())
        }
        fn dispatch_legacy(&self, cmd: crate::Command) -> Result<(), String> {
            let tag = match &cmd {
                crate::Command::New { .. } => "New",
                crate::Command::Ls { .. } => "Ls",
                crate::Command::Capture { .. } => "Capture",
                crate::Command::Kill { .. } => "Kill",
                crate::Command::Resize { .. } => "Resize",
                crate::Command::Send { .. } => "Send",
                _ => "Other",
            };
            if let crate::Command::New { repo, .. } = &cmd {
                self.legacy_new_repo.borrow_mut().push(repo.clone());
            }
            self.legacy_calls.borrow_mut().push(tag);
            Ok(())
        }
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Prepend `-L <label>` — the global option every swarm-path call
    /// carries ahead of its subcommand — and stringify the rest.
    fn under_label(label: &str, rest: &[&str]) -> Vec<String> {
        let mut argv = vec!["-L".to_string(), label.to_string()];
        argv.extend(rest.iter().map(|s| s.to_string()));
        argv
    }

    #[test]
    fn version_answers_locally_without_touching_the_backend() {
        let backend = FakeBackend::default();
        let outcome = run(&s(&["-V"]), &backend);
        assert_eq!(outcome.stdout, vec!["tmux 3.4".to_string()]);
        assert_eq!(outcome.exit, 0);
    }

    #[test]
    fn full_swarm_flow_end_to_end() {
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-1";

        // has-session: nothing yet.
        let o = run(
            &under_label(LABEL, &["has-session", "-t", "claude-swarm"]),
            &backend,
        );
        assert_eq!(o.exit, 1);

        // new-session -P -F '#{pane_id}' -- cat
        let o = run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        let initial_pane = o.stdout[0].clone();
        assert_eq!(initial_pane, "%0");

        // has-session now succeeds.
        let o = run(
            &under_label(LABEL, &["has-session", "-t", "claude-swarm"]),
            &backend,
        );
        assert_eq!(o.exit, 0);

        // list-panes on swarm-view: exactly the virtual initial pane.
        let o = run(
            &under_label(
                LABEL,
                &[
                    "list-panes",
                    "-t",
                    "claude-swarm:swarm-view",
                    "-F",
                    "#{pane_id}",
                ],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["%0".to_string()]);

        // split-window -P -F '#{pane_id}' — first teammate's real pane.
        let o = run(
            &under_label(
                LABEL,
                &[
                    "split-window",
                    "-d",
                    "-t",
                    "%0",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        let teammate_pane = o.stdout[0].clone();
        assert_ne!(teammate_pane, initial_pane);

        // select-pane -T names it.
        let o = run(
            &under_label(
                LABEL,
                &["select-pane", "-t", &teammate_pane, "-T", "teammate-1"],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);

        // respawn-pane -k delivers the real command.
        let o = run(
            &under_label(
                LABEL,
                &[
                    "respawn-pane",
                    "-k",
                    "-t",
                    &teammate_pane,
                    "--",
                    "echo",
                    "hi",
                ],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        // The command text and the submitting Enter must arrive as TWO
        // separate writes (matching cmd_agent's AgentAction::Type framing) —
        // a raw-mode Ink teammate agent treats a combined "text\r" in one
        // write as an unsubmitted prefill, not a submitted command.
        let writes = backend.writes.borrow();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].1, "\x03echo hi");
        assert_eq!(writes[1].1, "\r");
        drop(writes);

        // kill-pane removes it.
        let o = run(
            &under_label(LABEL, &["kill-pane", "-t", &teammate_pane]),
            &backend,
        );
        assert_eq!(o.exit, 0);

        // A second Claude Code process under a DIFFERENT -L sees an empty world.
        let o = run(
            &under_label("claude-swarm-2", &["has-session", "-t", "claude-swarm"]),
            &backend,
        );
        assert_eq!(o.exit, 1);
    }

    #[test]
    fn set_option_and_friends_are_explicit_noops() {
        let backend = FakeBackend::default();
        for argv in [
            s(&["set-option", "-p", "-t", "%3", "window-style", "bg=red"]),
            s(&["select-layout", "-t", "claude-swarm:swarm-view", "tiled"]),
        ] {
            let o = run(&argv, &backend);
            assert_eq!(o.exit, 0);
            assert!(o.stdout.is_empty());
        }
    }

    #[test]
    fn unknown_subcommand_is_an_honest_failure() {
        let backend = FakeBackend::default();
        let o = run(&s(&["totally-made-up"]), &backend);
        assert_eq!(o.exit, 1);
        assert!(!o.stderr.is_empty());
    }

    #[test]
    fn resize_pane_zoom_alone_is_a_silent_success() {
        let backend = FakeBackend::default();
        let o = run(&s(&["resize-pane", "-Z", "-t", "%1"]), &backend);
        assert_eq!(o.exit, 0);
        assert!(o.stdout.is_empty());
    }

    #[test]
    fn byte_identical_legacy_arms_reach_dispatch_legacy() {
        // attach-session is deliberately excluded here — it's handled locally
        // (opens a deep link) and never reaches dispatch_legacy at all; see
        // `attach_session_opens_the_deep_link_without_touching_the_backend`.
        let backend = FakeBackend::default();
        for (argv, expected_tag) in [
            (s(&["list-sessions"]), "Ls"),
            (s(&["ls"]), "Ls"),
            (s(&["capture-pane", "-t", "build"]), "Capture"),
            (s(&[]), "New"), // bare `tmux`
        ] {
            let o = run(&argv, &backend);
            assert_eq!(o.exit, 0, "argv={argv:?}");
            assert_eq!(
                backend.legacy_calls.borrow().last().copied(),
                Some(expected_tag),
                "argv={argv:?}"
            );
        }
    }

    #[test]
    fn legacy_new_session_without_dash_c_never_gets_resolve_cwds_fallback() {
        // Bare `tuic alias` usage (no -P) goes through dispatch_legacy and
        // must stay byte-identical to pre-swarm-shim behavior: `repo` is
        // `None` when no `-c` was given, never resolve_cwd()'s
        // std::env::current_dir() fallback (which is swarm-path-only,
        // gated on `-P`). A code-review finding, 2026-09-04: nothing
        // protected this invariant before, since FakeBackend::dispatch_legacy
        // discarded Command::New's fields entirely.
        let backend = FakeBackend::default();
        let o = run(&s(&["new-session", "-d", "-s", "legacy-session"]), &backend);
        assert_eq!(o.exit, 0);
        assert_eq!(backend.legacy_calls.borrow().last().copied(), Some("New"));
        assert_eq!(backend.legacy_new_repo.borrow().last().cloned(), Some(None));
    }

    #[test]
    fn attach_session_opens_the_deep_link_without_touching_the_backend() {
        let backend = FakeBackend::default();
        let o = run(&s(&["attach-session"]), &backend);
        assert_eq!(o.exit, 0);
        assert!(backend.legacy_calls.borrow().is_empty());
    }

    #[test]
    fn new_window_standalone_creates_a_window_on_an_existing_session() {
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-nw";
        let o = run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["claude-swarm".to_string()]);

        let o = run(
            &under_label(
                LABEL,
                &[
                    "new-window",
                    "-t",
                    "claude-swarm",
                    "-n",
                    "extra",
                    "-P",
                    "-F",
                    "#{window_id}",
                ],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        assert_eq!(o.stdout, vec!["@1".to_string()]);
    }

    #[test]
    fn new_window_against_an_unknown_session_is_an_error_not_a_panic() {
        let backend = FakeBackend::default();
        let o = run(
            &under_label("claude-swarm-nw2", &["new-window", "-t", "no-such-session"]),
            &backend,
        );
        assert_eq!(o.exit, 1);
        assert!(!o.stderr.is_empty());
    }

    #[test]
    fn display_message_renders_against_real_topology() {
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-dm";
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        let o = run(
            &under_label(
                LABEL,
                &[
                    "display-message",
                    "-t",
                    "claude-swarm",
                    "-p",
                    "#{session_name}:#{window_name}",
                ],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["claude-swarm:swarm-view".to_string()]);
    }

    #[test]
    fn list_windows_reports_every_window_in_a_session() {
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-lw";
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        run(
            &under_label(LABEL, &["new-window", "-t", "claude-swarm", "-n", "logs"]),
            &backend,
        );
        let o = run(
            &under_label(
                LABEL,
                &["list-windows", "-t", "claude-swarm", "-F", "#{window_name}"],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["swarm-view".to_string(), "logs".to_string()]);
    }

    #[test]
    fn display_message_and_list_panes_report_the_targeted_sessions_own_name() {
        // Regression: DisplayMessage and ListPanes both used to derive
        // #{session_name} from topology.sessions[0] unconditionally, which
        // happened to be invisible under the normal one-session-per-label
        // swarm flow but was wrong the moment a label held more than one
        // session — exactly what this test sets up.
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-multi";
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "session-a",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "session-b",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );

        let o = run(
            &under_label(
                LABEL,
                &[
                    "display-message",
                    "-t",
                    "session-b",
                    "-p",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["session-b".to_string()]);

        let o = run(
            &under_label(
                LABEL,
                &["list-panes", "-t", "session-b", "-F", "#{session_name}"],
            ),
            &backend,
        );
        assert_eq!(o.stdout, vec!["session-b".to_string()]);
    }

    #[test]
    fn kill_server_clears_only_the_current_labels_sessions_and_topology() {
        // Regression: kill-server used to close EVERY TUIC session app-wide
        // (via an unscoped list_sessions()) and wipe topology for EVERY
        // label — a swarm under one -L running routine kill-server cleanup
        // could nuke a concurrent swarm's (or the user's own manually
        // opened tabs') live sessions. Real tmux's `-L a kill-server` never
        // touches `-L b`.
        let backend = FakeBackend::default();
        const LABEL_A: &str = "claude-swarm-ks-a";
        const LABEL_B: &str = "claude-swarm-ks-b";
        for label in [LABEL_A, LABEL_B] {
            run(
                &under_label(
                    label,
                    &[
                        "new-session",
                        "-d",
                        "-s",
                        "claude-swarm",
                        "-P",
                        "-F",
                        "#{pane_id}",
                    ],
                ),
                &backend,
            );
            run(&under_label(label, &["split-window", "-t", "%0"]), &backend);
        }
        assert_eq!(backend.sessions.borrow().len(), 2);

        let o = run(&under_label(LABEL_A, &["kill-server"]), &backend);
        assert_eq!(o.exit, 0);

        // Label A's session is gone and its topology cleared...
        assert!(
            backend
                .topology
                .borrow()
                .get(LABEL_A)
                .is_none_or(|t| t["sessions"].as_array().is_none_or(|a| a.is_empty()))
        );
        // ...but label B's session and topology are completely untouched.
        assert_eq!(backend.sessions.borrow().len(), 1);
        assert!(
            !backend.topology.borrow()[LABEL_B]["sessions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn kill_session_falls_back_to_legacy_resolution_when_not_in_topology() {
        let backend = FakeBackend::default();
        let o = run(
            &under_label(
                "claude-swarm-fb",
                &["kill-session", "-t", "some-plain-tuic-session"],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        assert_eq!(backend.legacy_calls.borrow().last().copied(), Some("Kill"));
    }

    #[test]
    fn send_keys_materializes_a_virtual_pane_before_writing() {
        let backend = FakeBackend::default();
        const LABEL: &str = "claude-swarm-sk";
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-P",
                    "-F",
                    "#{pane_id}",
                ],
            ),
            &backend,
        );
        // The initial pane is virtual (no PTY yet) until something writes to it.
        assert!(backend.sessions.borrow().is_empty());

        let o = run(
            &under_label(LABEL, &["send-keys", "-t", "%0", "echo hi", "Enter"]),
            &backend,
        );
        assert_eq!(o.exit, 0);
        assert!(
            !backend.sessions.borrow().is_empty(),
            "materialize should have spawned a session"
        );
        let writes = backend.writes.borrow();
        assert_eq!(writes.last().unwrap().1, "echo hi\r");
    }

    #[test]
    fn send_keys_falls_back_to_legacy_resolution_when_not_in_topology() {
        let backend = FakeBackend::default();
        let o = run(
            &under_label(
                "claude-swarm-sk2",
                &["send-keys", "-t", "some-plain-tuic-session", "hi"],
            ),
            &backend,
        );
        assert_eq!(o.exit, 0);
        assert_eq!(backend.legacy_calls.borrow().last().copied(), Some("Send"));
    }

    #[test]
    fn has_session_and_kill_session_reject_a_missing_target_without_panicking() {
        let backend = FakeBackend::default();
        let o = run(&s(&["has-session"]), &backend);
        assert_eq!(o.exit, 1);

        let o = run(&s(&["kill-session"]), &backend);
        assert_eq!(o.exit, 1);
        assert!(!o.stderr.is_empty());
    }

    #[test]
    fn select_pane_and_kill_pane_against_an_unresolvable_target_error_cleanly() {
        let backend = FakeBackend::default();
        let o = run(&s(&["select-pane", "-t", "%999", "-T", "x"]), &backend);
        assert_eq!(o.exit, 1);

        let o = run(&s(&["kill-pane", "-t", "%999"]), &backend);
        assert_eq!(o.exit, 1);
    }

    #[test]
    fn respawn_pane_against_an_unresolvable_target_errors_cleanly() {
        let backend = FakeBackend::default();
        let o = run(
            &s(&["respawn-pane", "-k", "-t", "%999", "--", "echo", "hi"]),
            &backend,
        );
        assert_eq!(o.exit, 1);
        assert!(backend.writes.borrow().is_empty());
    }

    // ---- respawn-pane: freshly-materialized-pane race (found live, 2026-09-03) --

    #[test]
    fn respawn_pane_never_sends_the_kill_byte_against_a_still_virtual_pane() {
        // A pane `new-session -d`'s initial slot creates stays virtual until
        // something writes to it — respawn-pane is that something, so THIS
        // call is what materializes it. There is no prior foreground process
        // to interrupt, unlike a split-window pane a teammate reuses later.
        let backend = FakeBackend::default();
        run(
            &s(&[
                "new-session",
                "-d",
                "-s",
                "claude-swarm",
                "-n",
                "swarm-view",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            &backend,
        );
        let o = run(
            &s(&["respawn-pane", "-k", "-t", "%0", "--", "echo", "hi"]),
            &backend,
        );
        assert_eq!(o.exit, 0);
        let writes = backend.writes.borrow();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].1, "echo hi", "no leading kill byte");
        assert_eq!(writes[1].1, "\r");
    }

    #[test]
    fn respawn_pane_retries_once_when_a_freshly_materialized_ptys_first_write_fails() {
        // Confirmed live: a pane materialized for the first time by THIS
        // call can have its PTY die (app log: "Session closed: process
        // exited") within milliseconds — before this call's own write ever
        // reaches it. The retry must re-materialize (not reuse the dead id)
        // and succeed against the fresh one.
        let backend = FakeBackend::default();
        run(
            &s(&[
                "new-session",
                "-d",
                "-s",
                "claude-swarm",
                "-n",
                "swarm-view",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            &backend,
        );
        *backend.fail_next_n_writes.borrow_mut() = 1;
        let o = run(
            &s(&["respawn-pane", "-k", "-t", "%0", "--", "echo", "hi"]),
            &backend,
        );
        assert_eq!(o.exit, 0, "must recover via the retry: {:?}", o.stderr);
        let writes = backend.writes.borrow();
        // Only the surviving attempt's two writes are recorded — the failed
        // first write to the dead id was never pushed onto `writes`.
        assert_eq!(writes.len(), 2);
        assert_eq!(
            writes[0].1, "echo hi",
            "still no kill byte — attempt 2 is also a fresh PTY"
        );
        assert_eq!(writes[1].1, "\r");
    }

    #[test]
    fn respawn_pane_gives_up_after_the_retry_also_fails() {
        let backend = FakeBackend::default();
        run(
            &s(&[
                "new-session",
                "-d",
                "-s",
                "claude-swarm",
                "-n",
                "swarm-view",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            &backend,
        );
        *backend.fail_next_n_writes.borrow_mut() = 2;
        let o = run(
            &s(&["respawn-pane", "-k", "-t", "%0", "--", "echo", "hi"]),
            &backend,
        );
        assert_eq!(o.exit, 1);
        assert!(o.stderr[0].contains("Session not found"));
        assert!(backend.writes.borrow().is_empty());
    }

    #[test]
    fn respawn_pane_still_sends_the_kill_byte_against_an_already_live_pane() {
        // A split-window pane materializes eagerly, so by the time a later
        // respawn-pane targets it, there IS a real foreground process (the
        // placeholder shell) to interrupt — unlike the still-virtual
        // new-session initial pane covered above. Must not regress this:
        // `full_swarm_flow_end_to_end` already covers the happy path: this
        // test isolates the kill-byte behavior on its own.
        let backend = FakeBackend::default();
        run(
            &s(&[
                "new-session",
                "-d",
                "-s",
                "claude-swarm",
                "-n",
                "swarm-view",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            &backend,
        );
        let o = run(
            &s(&[
                "split-window",
                "-d",
                "-t",
                "%0",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            &backend,
        );
        let teammate_pane = o.stdout[0].clone();
        let o = run(
            &s(&[
                "respawn-pane",
                "-k",
                "-t",
                &teammate_pane,
                "--",
                "echo",
                "hi",
            ]),
            &backend,
        );
        assert_eq!(o.exit, 0);
        let writes = backend.writes.borrow();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].1, "\x03echo hi");
        assert_eq!(writes[1].1, "\r");
    }

    #[test]
    fn new_session_without_dash_c_falls_back_to_the_real_cwd_not_none() {
        // Claude Code's swarm path never passes -c on new-session (confirmed
        // empirically — see resolve_cwd's doc comment in exec.rs). Without
        // the fallback this landed on the wrong repo entirely: a live
        // 2026-09-04 swarm spawned from `commerce-journal` had all 4
        // teammate panes land under `databricks-sql-cli` instead, because
        // `cwd: None` let spawn_pty_session inherit the app process's own
        // cwd, which matched no registered repo, so the frontend fell back
        // to whichever repo was active in the sidebar at that moment.
        const LABEL: &str = "claude-swarm-cwd-1";
        let backend = FakeBackend::default();
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        let topology = backend.get_topology(LABEL).expect("topology must exist");
        let cwd = topology["sessions"][0]["windows"][0]["panes"][0]["cwd"]
            .as_str()
            .expect("cwd must be populated, not null");
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(cwd, expected);
    }

    #[test]
    fn split_window_without_dash_c_falls_back_to_the_real_cwd_not_none() {
        const LABEL: &str = "claude-swarm-cwd-2";
        let backend = FakeBackend::default();
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        run(
            &under_label(
                LABEL,
                &[
                    "split-window",
                    "-d",
                    "-t",
                    "%0",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        let topology = backend.get_topology(LABEL).expect("topology must exist");
        let cwd = topology["sessions"][0]["windows"][0]["panes"][1]["cwd"]
            .as_str()
            .expect("cwd must be populated, not null");
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(cwd, expected);
    }

    #[test]
    fn new_session_with_dash_c_still_honors_the_explicit_value() {
        const LABEL: &str = "claude-swarm-cwd-3";
        let backend = FakeBackend::default();
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-c",
                    "/explicit/repo/path",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "--",
                    "cat",
                ],
            ),
            &backend,
        );
        let topology = backend.get_topology(LABEL).expect("topology must exist");
        let cwd = topology["sessions"][0]["windows"][0]["panes"][0]["cwd"]
            .as_str()
            .expect("cwd must be populated");
        assert_eq!(cwd, "/explicit/repo/path");
    }

    #[test]
    fn new_window_without_dash_c_falls_back_to_the_real_cwd_not_none() {
        // new-window is a rarer edge of the real swarm flow (only reached
        // when the swarm-view window is missing — see tmux-shim.html's
        // "conditions" section) but goes through the same resolve_cwd()
        // fallback as new-session/split-window and must not regress
        // independently of them.
        const LABEL: &str = "claude-swarm-cwd-4";
        let backend = FakeBackend::default();
        run(
            &under_label(
                LABEL,
                &[
                    "new-session",
                    "-d",
                    "-s",
                    "claude-swarm",
                    "-P",
                    "-F",
                    "#{session_name}",
                ],
            ),
            &backend,
        );
        run(
            &under_label(
                LABEL,
                &[
                    "new-window",
                    "-t",
                    "claude-swarm",
                    "-n",
                    "swarm-view",
                    "-P",
                    "-F",
                    "#{window_id}",
                ],
            ),
            &backend,
        );
        let topology = backend.get_topology(LABEL).expect("topology must exist");
        let cwd = topology["sessions"][0]["windows"][1]["panes"][0]["cwd"]
            .as_str()
            .expect("cwd must be populated, not null");
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(cwd, expected);
    }
}
