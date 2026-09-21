//! The wire shape of an `AppEvent`: its SSE type name, its JSON payload, and
//! whether/how it rides the desktop window's `app.emit`.
//!
//! Moved out of `mcp_http::sse_routes` (which owned these when only the SSE
//! side needed them) so `AppState::emit_dual` — the single dual-emit choke
//! point, see its doc comment in `state.rs` — can call them without depending
//! on `mcp_http`.

use crate::state::AppEvent;

/// Extract the normalized event type name (matches SSE `event:` field).
pub(crate) fn event_type_name(event: &AppEvent) -> &'static str {
    match event {
        AppEvent::HeadChanged { .. } => "head-changed",
        AppEvent::RepoChanged { .. } => "repo-changed",
        AppEvent::SessionCreated { .. } => "session-created",
        AppEvent::SessionClosed { .. } => "session-closed",
        AppEvent::PtyParsed { .. } => "pty-parsed",
        AppEvent::PtyExit { .. } => "pty-exit",
        AppEvent::PtyActivity { .. } => "pty-activity",
        AppEvent::SessionFocusRequested { .. } => "session-focus-requested",
        AppEvent::UiActionRequested { .. } => "ui-action-requested",
        AppEvent::PtyOsc133 { .. } => "pty-osc133",
        AppEvent::PtyCwd { .. } => "pty-cwd",
        AppEvent::PtyOpenUrl { .. } => "pty-open-url",
        AppEvent::PtyImagePlacement { .. } => "pty-image-placement",
        AppEvent::PtyImagePlacementsCleared { .. } => "pty-image-placements-cleared",
        AppEvent::PtyImageDecoded { .. } => "pty-image-decoded",
        AppEvent::PluginWatcherLines { .. } => "plugin-watcher-lines",
        AppEvent::PtyDescriptionChanged { .. } => "pty-description-changed",
        AppEvent::SessionRenamed { .. } => "session-renamed",
        AppEvent::SessionAccentColorChanged { .. } => "session-accent-color-changed",
        AppEvent::TmuxWindowLayoutRequested { .. } => "tmux-window-layout-requested",
        AppEvent::PluginChanged { .. } => "plugin-changed",
        AppEvent::UpstreamStatusChanged { .. } => "upstream-status-changed",
        AppEvent::McpOAuthStart { .. } => "mcp-oauth-start",
        AppEvent::McpToast { .. } => "mcp-toast",
        AppEvent::McpConfirm { .. } => "mcp-confirm",
        AppEvent::McpConfirmResolved { .. } => "mcp-confirm-resolved",
        AppEvent::AcpNotice(_) => "acp-notice",
        AppEvent::RepositoriesChanged => "repositories-changed",
        AppEvent::DirChanged { .. } => "dir-changed",
        AppEvent::WorktreeCreated { .. } => "worktree-created",
        AppEvent::WorktreeRemoved { .. } => "worktree-removed",
        AppEvent::PeerRegistered { .. } => "peer-registered",
        AppEvent::PeerUnregistered { .. } => "peer-unregistered",
        AppEvent::UiTab { .. } => "ui-tab",
        AppEvent::GitHubPrUpdate { .. } => "github-pr-update",
        AppEvent::GitHubTransition { .. } => "github-transition",
        AppEvent::GitHubIssuesUpdate { .. } => "github-issues-update",
        AppEvent::CloseHtmlTabs { .. } => "close-html-tabs",
        AppEvent::ScheduledJobCompleted { .. } => "scheduled-job-completed",
        AppEvent::DiffTriageProgress { .. } => "triage-progress",
        AppEvent::ReviewProgress { .. } => "review-progress",
        AppEvent::ConflictAssistStatus { .. } => "conflict-assist-status",
        AppEvent::ProgressRecorded { .. } => "progress-recorded",
        AppEvent::ProposalsReady { .. } => "proposals-ready",
        AppEvent::WorktreeSyncStarted { .. } => "worktree-sync-started",
        AppEvent::WorktreeSyncProgress { .. } => "worktree-sync-progress",
        AppEvent::WorktreeSyncCompleted { .. } => "worktree-sync-completed",
        AppEvent::WorktreeSetupScriptCompleted { .. } => "worktree-setup-script-completed",
        AppEvent::WorktreeWarmStarted { .. } => "worktree-warm-started",
        AppEvent::WorktreeWarmProgress { .. } => "worktree-warm-progress",
        AppEvent::WorktreeWarmCompleted { .. } => "worktree-warm-completed",
        AppEvent::SessionStateChanged { .. } => "session-state-changed",
        AppEvent::SessionStandby { .. } => "session-standby",
        AppEvent::AiSuggestion { .. } => "ai-suggestion",
        AppEvent::WatcherStatusChanged { .. } => "watcher-status",
        AppEvent::ThemesChanged => "themes-changed",
        AppEvent::PtyClipboardStore { .. } => "pty-clipboard-store",
        AppEvent::TermAliasAssigned { .. } => "term-alias-assigned",
    }
}

/// Extract just the payload (without the wrapping `event`/`payload` tags).
/// The SSE `event:` field already carries the type, so we only need the inner data.
/// Let another module's test compare this payload against the desktop one.
///
/// The two are built by different code in different files, which is exactly why
/// they drift; a test that can only see one of them cannot catch it.
pub(crate) fn event_payload(event: &AppEvent) -> serde_json::Value {
    match event {
        AppEvent::HeadChanged { repo_path, branch } => {
            serde_json::json!({ "repo_path": repo_path, "branch": branch })
        }
        AppEvent::RepoChanged { repo_path, kind } => {
            serde_json::json!({ "repo_path": repo_path, "kind": kind })
        }
        AppEvent::SessionCreated {
            session_id,
            cwd,
            agent_type,
            display_name,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "cwd": cwd,
                "agent_type": agent_type,
                "display_name": display_name,
            })
        }
        AppEvent::SessionClosed {
            session_id,
            reason,
            agent_type,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "reason": reason,
                "agent_type": agent_type,
            })
        }
        AppEvent::PtyParsed { session_id, parsed } => {
            serde_json::json!({ "session_id": session_id, "parsed": parsed })
        }
        AppEvent::PtyExit { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PtyActivity { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::SessionFocusRequested { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::UiActionRequested { name } => {
            serde_json::json!({ "name": name })
        }
        AppEvent::PtyOsc133 {
            session_id,
            marker,
            line,
            exit_code,
            on_alt_screen,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "marker": marker,
                "line": line,
                "exit_code": exit_code,
                "on_alt_screen": on_alt_screen,
            })
        }
        AppEvent::PtyCwd { session_id, cwd } => {
            serde_json::json!({ "session_id": session_id, "cwd": cwd })
        }
        AppEvent::PtyOpenUrl { session_id, url } => {
            serde_json::json!({ "session_id": session_id, "url": url })
        }
        AppEvent::PtyImagePlacement {
            session_id,
            placement_id,
            image_id,
            abs_row,
            col,
            rows,
            cols,
            z_index,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "placement_id": placement_id,
                "image_id": image_id,
                "abs_row": abs_row,
                "col": col,
                "rows": rows,
                "cols": cols,
                "z_index": z_index,
            })
        }
        AppEvent::PtyImagePlacementsCleared { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PtyImageDecoded {
            session_id,
            image_id,
        } => {
            serde_json::json!({ "session_id": session_id, "image_id": image_id })
        }
        AppEvent::PluginWatcherLines { session_id, lines } => {
            serde_json::json!({ "session_id": session_id, "lines": lines })
        }
        AppEvent::PtyDescriptionChanged {
            session_id,
            description,
        } => {
            serde_json::json!({ "session_id": session_id, "description": description })
        }
        AppEvent::SessionRenamed {
            session_id,
            display_name,
            is_custom,
        } => {
            serde_json::json!({ "session_id": session_id, "display_name": display_name, "is_custom": is_custom })
        }
        AppEvent::SessionAccentColorChanged { session_id, color } => {
            serde_json::json!({ "session_id": session_id, "color": color })
        }
        AppEvent::TmuxWindowLayoutRequested {
            session_ids,
            layout,
        } => {
            serde_json::json!({ "session_ids": session_ids, "layout": layout })
        }
        AppEvent::PluginChanged { plugin_ids } => {
            serde_json::json!({ "plugin_ids": plugin_ids })
        }
        AppEvent::UpstreamStatusChanged { name, status } => {
            serde_json::json!({ "name": name, "status": status })
        }
        AppEvent::McpOAuthStart {
            name,
            authorization_url,
        } => {
            serde_json::json!({ "name": name, "authorization_url": authorization_url })
        }
        AppEvent::McpToast {
            title,
            message,
            level,
            sound,
            origin_repo_path,
            origin_session_id,
        } => {
            serde_json::json!({
                "title": title,
                "message": message,
                "level": level,
                "sound": sound,
                "origin_repo_path": origin_repo_path,
                "origin_session_id": origin_session_id,
            })
        }
        AppEvent::McpConfirm {
            request_id,
            title,
            message,
            origin_repo_path,
            origin_session_id,
        } => {
            serde_json::json!({
                "request_id": request_id,
                "title": title,
                "message": message,
                "origin_repo_path": origin_repo_path,
                "origin_session_id": origin_session_id,
            })
        }
        AppEvent::McpConfirmResolved {
            request_id,
            confirmed,
        } => {
            serde_json::json!({ "request_id": request_id, "confirmed": confirmed })
        }
        // Forwarded whole: the notice IS the payload, in the same camelCase the
        // `/acp` routes use, so a client needs no per-transport translation.
        AppEvent::AcpNotice(notice) => serde_json::json!(notice),
        // Payload-free: the receiver re-reads `repositories.json` itself.
        AppEvent::RepositoriesChanged => serde_json::json!({}),
        AppEvent::DirChanged { dir_path } => {
            serde_json::json!({ "dir_path": dir_path })
        }
        // Forwarded whole, like `AcpNotice`: the desktop `emit` in
        // `notify_worktree_created`/`notify_worktree_removed` serializes this
        // same struct, so the two transports cannot spell a field differently.
        AppEvent::WorktreeCreated(payload) => serde_json::json!(payload),
        AppEvent::WorktreeRemoved(payload) => serde_json::json!(payload),
        AppEvent::PeerRegistered { tuic_session, name } => {
            serde_json::json!({ "tuic_session": tuic_session, "name": name })
        }
        AppEvent::PeerUnregistered { tuic_session } => {
            serde_json::json!({ "tuic_session": tuic_session })
        }
        AppEvent::UiTab {
            id,
            title,
            html,
            url,
            pinned,
            focus,
            origin_repo_path,
        } => {
            let mut v = serde_json::json!({ "id": id, "title": title, "html": html, "pinned": pinned, "focus": focus });
            if let Some(u) = url {
                v["url"] = serde_json::Value::String(u.clone());
            }
            if let Some(p) = origin_repo_path {
                v["origin_repo_path"] = serde_json::Value::String(p.clone());
            }
            v
        }
        AppEvent::GitHubPrUpdate {
            repo_path,
            statuses,
        } => {
            serde_json::json!({ "repo_path": repo_path, "statuses": statuses })
        }
        AppEvent::GitHubTransition { transition } => {
            serde_json::to_value(transition).unwrap_or_default()
        }
        AppEvent::GitHubIssuesUpdate { repo_path, issues } => {
            serde_json::json!({ "repo_path": repo_path, "issues": issues })
        }
        AppEvent::CloseHtmlTabs { tab_ids } => {
            serde_json::json!({ "tab_ids": tab_ids })
        }
        AppEvent::ScheduledJobCompleted {
            job_id,
            goal,
            timed_out,
        } => {
            serde_json::json!({ "job_id": job_id, "goal": goal, "timed_out": timed_out })
        }
        AppEvent::DiffTriageProgress {
            repo_path,
            summary,
            files,
            phase,
            done,
            llm_used,
            llm_model,
        } => {
            serde_json::json!({
                "repo_path": repo_path,
                "summary": summary,
                "files": files,
                "phase": phase,
                "done": done,
                "llm_used": llm_used,
                "llm_model": llm_model,
            })
        }
        AppEvent::ReviewProgress { repo_path, payload }
        | AppEvent::ConflictAssistStatus { repo_path, payload }
        | AppEvent::ProgressRecorded { repo_path, payload }
        | AppEvent::ProposalsReady { repo_path, payload } => {
            serde_json::json!({ "repo_path": repo_path, "payload": payload })
        }
        AppEvent::SessionStateChanged { session_id, state } => {
            // Built by the same function the desktop window emit uses, so the
            // two transports cannot drift into two shapes for one thing.
            crate::state::session_state_payload(session_id, state)
        }
        AppEvent::WorktreeSyncStarted { repo_path, branch } => {
            // camelCase keys mirror the Tauri window `worktree-sync-started`
            // event — see `worktree::spawn_worktree_file_sync`.
            serde_json::json!({ "repoPath": repo_path, "branch": branch })
        }
        AppEvent::WorktreeSyncProgress {
            repo_path,
            branch,
            copied,
            total,
        } => {
            serde_json::json!({ "repoPath": repo_path, "branch": branch, "copied": copied, "total": total })
        }
        AppEvent::WorktreeSyncCompleted {
            repo_path,
            branch,
            copied,
            total,
            errors,
        } => {
            serde_json::json!({
                "repoPath": repo_path,
                "branch": branch,
                "copied": copied,
                "total": total,
                "errors": errors,
            })
        }
        AppEvent::WorktreeSetupScriptCompleted {
            repo_path,
            branch,
            worktree_path,
            exit_code,
            error,
        } => {
            // camelCase keys mirror the Tauri window
            // `worktree-setup-script-completed` event — see
            // `worktree::spawn_worktree_setup_chain`.
            serde_json::json!({
                "repoPath": repo_path,
                "branch": branch,
                "worktreePath": worktree_path,
                "exitCode": exit_code,
                "error": error,
            })
        }
        AppEvent::WorktreeWarmStarted {
            repo_path,
            branch,
            total,
        } => {
            // camelCase keys mirror the Tauri window `worktree-warm-started`
            // event — see `worktree::run_worktree_warm`.
            serde_json::json!({ "repoPath": repo_path, "branch": branch, "total": total })
        }
        AppEvent::WorktreeWarmProgress {
            repo_path,
            branch,
            copied,
            total,
            current,
        } => {
            serde_json::json!({
                "repoPath": repo_path,
                "branch": branch,
                "copied": copied,
                "total": total,
                "current": current,
            })
        }
        AppEvent::WorktreeWarmCompleted {
            repo_path,
            branch,
            warmed,
            warnings,
        } => {
            serde_json::json!({
                "repoPath": repo_path,
                "branch": branch,
                "warmed": warmed,
                "warnings": warnings,
            })
        }
        AppEvent::SessionStandby { session_id, standby } => {
            serde_json::json!({ "session_id": session_id, "standby": standby })
        }
        AppEvent::AiSuggestion {
            session_id,
            trigger_reason,
            proposed_goal,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "trigger_reason": trigger_reason,
                "proposed_goal": proposed_goal,
            })
        }
        AppEvent::WatcherStatusChanged {
            id,
            status,
            fire_count,
            session_id,
        } => {
            serde_json::json!({
                "id": id,
                "status": status,
                "fire_count": fire_count,
                "session_id": session_id,
            })
        }
        AppEvent::ThemesChanged => serde_json::json!({}),
        AppEvent::PtyClipboardStore { session_id, text } => {
            serde_json::json!({ "session_id": session_id, "text": text })
        }
        AppEvent::TermAliasAssigned { session_id, alias } => {
            serde_json::json!({ "session_id": session_id, "alias": alias })
        }
    }
}

/// The desktop window event name for `event`, or `None` for a variant with its
/// own dedicated, suffixed desktop event name and its own hand-written
/// dual-emit call site — these must NEVER go through `AppState::emit_dual`,
/// which would emit them under the wrong (generic, unsuffixed) name. SSE names
/// are `&'static str`, never interpolated, so a per-session suffixed name
/// (`pty-osc133-{id}`, etc.) simply has no representation here.
///
/// Suffixed-and-excluded today: `PtyActivity` (`pty-activity-{id}`),
/// `PluginWatcherLines` (`pty-watcher-lines-{id}` on desktop, note the SSE name
/// differs: `plugin-watcher-lines`), `PtyParsed` (`pty-parsed-{id}`), `PtyOsc133`
/// (`pty-osc133-{id}`), `PtyCwd` (`pty-cwd-{id}`), `PtyImagePlacement`
/// (`pty-image-placement-{id}`), `PtyImagePlacementsCleared`
/// (`pty-image-placements-cleared-{id}`), `PtyImageDecoded`
/// (`pty-image-decoded-{id}`), `PtyExit` (`pty-exit-{id}`).
///
/// `SessionClosed` returns `Some` here even though `pty_session_id()` also
/// returns `Some` for it — it is NOT one of the suffixed events above (its
/// desktop name is the plain, unsuffixed `"session-closed"`), so it goes
/// through `emit_dual` normally.
///
/// Every other variant: `Some(event_type_name(event))`.
pub(crate) fn window_event_name(event: &AppEvent) -> Option<&'static str> {
    match event {
        AppEvent::PtyActivity { .. }
        | AppEvent::PluginWatcherLines { .. }
        | AppEvent::PtyParsed { .. }
        | AppEvent::PtyOsc133 { .. }
        | AppEvent::PtyCwd { .. }
        | AppEvent::PtyImagePlacement { .. }
        | AppEvent::PtyImagePlacementsCleared { .. }
        | AppEvent::PtyImageDecoded { .. }
        | AppEvent::PtyExit { .. } => None,
        other => Some(event_type_name(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppEvent;

    /// Every suffixed-desktop-name variant must return `None` — this is the
    /// guard that keeps a future `emit_dual` call on one of them from firing
    /// under the wrong (generic, unsuffixed) name.
    #[test]
    fn session_scoped_suffixed_events_return_none_from_window_event_name() {
        let cases = [
            AppEvent::PtyActivity {
                session_id: "s".into(),
            },
            AppEvent::PtyExit {
                session_id: "s".into(),
            },
            AppEvent::PtyCwd {
                session_id: "s".into(),
                cwd: "/tmp".into(),
            },
        ];
        for event in cases {
            assert_eq!(window_event_name(&event), None);
        }
    }

    #[test]
    fn session_closed_is_not_treated_as_a_suffixed_event() {
        let event = AppEvent::SessionClosed {
            session_id: "s".into(),
            reason: "closed".into(),
            agent_type: None,
        };
        assert_eq!(window_event_name(&event), Some("session-closed"));
    }

    /// D.11 (part 1) — table-driven wire-contract pin for every event this
    /// desktop↔HTTP-parity plan added or changed the payload of. Asserts the
    /// exact SSE `event:` name AND the exact JSON key set — a name pinned
    /// without its keys would let a payload field silently rename/disappear
    /// with nothing red.
    #[test]
    fn new_and_changed_events_have_the_documented_name_and_payload_keys() {
        fn payload_keys(event: &AppEvent) -> Vec<String> {
            let mut keys: Vec<String> = match event_payload(event) {
                serde_json::Value::Object(map) => map.keys().cloned().collect(),
                other => panic!("expected an object payload, got {other:?}"),
            };
            keys.sort_unstable();
            keys
        }

        let cases: Vec<(AppEvent, &str, Vec<&str>)> = vec![
            (
                AppEvent::SessionStandby {
                    session_id: "s".into(),
                    standby: true,
                },
                "session-standby",
                vec!["session_id", "standby"],
            ),
            (
                AppEvent::AiSuggestion {
                    session_id: "s".into(),
                    trigger_reason: "idle".into(),
                    proposed_goal: "keep going".into(),
                },
                "ai-suggestion",
                vec!["proposed_goal", "session_id", "trigger_reason"],
            ),
            (
                AppEvent::WatcherStatusChanged {
                    id: "r1".into(),
                    status: "paused".into(),
                    fire_count: 3,
                    session_id: Some("s".into()),
                },
                "watcher-status",
                vec!["fire_count", "id", "session_id", "status"],
            ),
            (AppEvent::ThemesChanged, "themes-changed", vec![]),
            (
                AppEvent::PtyClipboardStore {
                    session_id: "s".into(),
                    text: "clip".into(),
                },
                "pty-clipboard-store",
                vec!["session_id", "text"],
            ),
            (
                AppEvent::TermAliasAssigned {
                    session_id: "s".into(),
                    alias: "tc-1".into(),
                },
                "term-alias-assigned",
                vec!["alias", "session_id"],
            ),
            (
                AppEvent::SessionClosed {
                    session_id: "s".into(),
                    reason: "closed".into(),
                    agent_type: Some("claude".into()),
                },
                "session-closed",
                vec!["agent_type", "reason", "session_id"],
            ),
        ];

        for (event, expected_name, expected_keys) in cases {
            let mut expected_keys: Vec<String> =
                expected_keys.into_iter().map(String::from).collect();
            expected_keys.sort_unstable();
            assert_eq!(
                event_type_name(&event),
                expected_name,
                "name mismatch for {event:?}"
            );
            assert_eq!(
                payload_keys(&event),
                expected_keys,
                "payload key set mismatch for {event:?}"
            );
        }
    }

    /// D.11 (part 2) — no hand-rolled second `app.emit(...)` for any event this
    /// plan converged onto `AppState::emit_dual`. A future edit that re-adds a
    /// one-off desktop emit for one of these (bypassing `emit_dual`, and so
    /// silently reintroducing desktop/SSE payload drift) fails here instead of
    /// waiting for a bug report.
    #[test]
    fn no_stray_app_emit_literal_for_events_converged_onto_emit_dual() {
        const CONVERGED_EVENTS: &[&str] = &[
            "session-created",
            "session-closed",
            "session-standby",
            "ai-suggestion",
            "watcher-status",
            "themes-changed",
            "pty-clipboard-store",
            "term-alias-assigned",
        ];
        // `event_wire.rs` (this file) legitimately names every one of these in
        // its own match arms/doc comments; `state.rs` owns `emit_dual` itself,
        // which calls `app.emit(name, ...)` with a *variable*, not a literal,
        // so it can never match this pattern anyway — excluded only to keep
        // the failure list free of misleading noise if that ever changes.
        const EXCLUDE_FILES: &[&str] = &["event_wire.rs", "state.rs"];

        fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }

        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&src_dir, &mut files);
        assert!(
            files.len() > 50,
            "only found {} .rs files — the walk looks broken",
            files.len()
        );

        let mut offenders = Vec::new();
        for path in files {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if EXCLUDE_FILES.contains(&file_name) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            for name in CONVERGED_EVENTS {
                let needle = format!(".emit(\"{name}\"");
                if contents.contains(&needle) {
                    offenders.push(format!("{}: {needle}", path.display()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "found a hand-rolled app.emit(...) literal for an event that must go through \
             AppState::emit_dual: {offenders:#?}"
        );
    }

    /// D.11 — listener-side sibling of the COMMAND_TABLE → router parity gate
    /// (`mcp_http::tests::command_table_paths_all_hit_a_registered_route`).
    /// `frontend_listen_event_names.txt` is generated by
    /// `src/__tests__/transport.test.ts` ("frontend listener → SSE arm parity
    /// (D.11)") from every bare-string `listen("name", ...)` call across the
    /// whole frontend. Every name in it must either be a real
    /// `event_type_name` output (this file's own match arms, above) or be
    /// explicitly allowlisted here as desktop-only, with a reason — mirrors
    /// that test's own `DESKTOP_ONLY_EVENTS` set, which this list must stay
    /// in sync with (this test does not cross-check the two lists against
    /// each other; the TS-side test already guards against a name being in
    /// both).
    #[test]
    fn frontend_listen_event_names_all_have_an_sse_arm_or_are_allowlisted() {
        const NAMES: &str = include_str!("mcp_http/frontend_listen_event_names.txt");
        let names: Vec<&str> = NAMES
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert!(
            names.len() > 30,
            "frontend_listen_event_names.txt yielded only {} names — regenerate it with \
             `pnpm vitest run src/__tests__/transport.test.ts -u`",
            names.len()
        );

        // Every RHS string literal `event_type_name`'s match arms can produce,
        // extracted from this file's own source rather than duplicated by hand
        // — the two would otherwise be free to drift silently.
        let source = include_str!("event_wire.rs");
        let known: std::collections::HashSet<&str> = source
            .lines()
            .filter_map(|line| {
                let idx = line.find("=> \"")?;
                let rest = &line[idx + 4..];
                let end = rest.find('"')?;
                Some(&rest[..end])
            })
            .collect();
        assert!(
            known.len() > 30,
            "known event_type_name output set looked too small: {known:?}"
        );

        const DESKTOP_ONLY_EVENTS: &[&str] = &[
            // Native menu bridge — OS menu bar, host-only.
            "ctrl-tab",
            "menu-action",
            "file-open",
            // DOM-level custom key-combo events, synthesized from raw native
            // keydown — never touch the backend at all.
            "fn-key-down",
            "fn-key-up",
            "native-key-down",
            // Dictation — desktop-only local Whisper pipeline.
            "dictation-backend-info",
            "dictation-download-progress",
            "dictation-partial",
            // Content search streaming (fs.rs) — deliberately desktop-only: the
            // HTTP route computes the same result and returns it in the
            // response body instead of streaming.
            "content-search-batch",
            "content-search-error",
            // Detached secondary-window bridge — multi-window management is a
            // desktop-only concept.
            "panel-action",
            "panel-resync-request",
            "panel-window-closed",
            // FloatingTerminal.tsx's window-to-window `emitTo("main", ...)` —
            // pure desktop multi-window IPC, no backend involvement at all.
            "reattach-terminal",
            // Screenshot capture request (`ui action=screenshot`) — targets the
            // desktop window directly via `AppHandle.emit`, no meaning for a
            // headless/browser client.
            "screenshot-request",
            // OS sleep/wake notification — host-level power-management signal.
            "system-wake",
            // Deliberately desktop-only (D.9): an IMPERATIVE event
            // (auto-executes an agent/prompt) — dual-emitting it would make a
            // desktop window and an open browser tab both execute the same
            // fire.
            "watcher-fire",
        ];

        let orphaned: Vec<&str> = names
            .into_iter()
            .filter(|name| !known.contains(name) && !DESKTOP_ONLY_EVENTS.contains(name))
            .collect();
        assert!(
            orphaned.is_empty(),
            "frontend listens for these events with no event_type_name arm and no \
             DESKTOP_ONLY_EVENTS allowlist entry: {orphaned:#?}\n\
             Add an AppEvent variant + event_type_name arm (AGENTS.md → IPC/HTTP Parity), \
             or allowlist it here with a reason."
        );
    }
}
