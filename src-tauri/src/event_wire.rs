//! The wire shape of an `AppEvent`: its SSE type name, its JSON payload, and
//! whether/how it rides the desktop window's `app.emit`.
//!
//! Moved out of `mcp_http::sse_routes` (which owned these when only the SSE
//! side needed them) so `AppState::emit_dual` — the single dual-emit choke
//! point, see its doc comment in `state.rs` — can call them without depending
//! on `mcp_http`.

use crate::state::AppEvent;

/// Extract the normalized event type name (matches SSE `event:` field).
///
/// Borrowed rather than `&'static str`: a mirrored event carries the far
/// daemon's own name, so the name is data rather than a constant.
pub(crate) fn event_type_name(event: &AppEvent) -> &str {
    match event {
        AppEvent::HeadChanged { .. } => "head-changed",
        AppEvent::RepoChanged { .. } => "repo-changed",
        AppEvent::SessionCreated { .. } => "session-created",
        AppEvent::SessionClosed { .. } => "session-closed",
        AppEvent::DesignModeChanged { .. } => "design-mode-changed",
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
        AppEvent::SessionSuspendRequested { .. } => "session-suspend-requested",
        AppEvent::TermAliasAssigned { .. } => "term-alias-assigned",
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
        AppEvent::ConflictAssistStatus { .. } => "conflict-assist-status",
        AppEvent::ProgressRecorded { .. } => "progress-recorded",
        AppEvent::WorkflowRunChanged { .. } => "workflow-run-changed",
        AppEvent::ReviewProgress { .. } => "review-progress",
        AppEvent::ProposalsReady { .. } => "proposals-ready",
        AppEvent::WorktreeSyncStarted { .. } => "worktree-sync-started",
        AppEvent::WorktreeSyncProgress { .. } => "worktree-sync-progress",
        AppEvent::WorktreeSyncCompleted { .. } => "worktree-sync-completed",
        AppEvent::WorktreeWarmStarted { .. } => "worktree-warm-started",
        AppEvent::WorktreeWarmProgress { .. } => "worktree-warm-progress",
        AppEvent::WorktreeWarmCompleted { .. } => "worktree-warm-completed",
        AppEvent::WorktreeSetupScriptCompleted { .. } => "worktree-setup-script-completed",
        AppEvent::SessionStateChanged { .. } => "session-state-changed",
        AppEvent::SessionStandby { .. } => "session-standby",
        AppEvent::AiSuggestion { .. } => "ai-suggestion",
        AppEvent::WatcherStatusChanged { .. } => "watcher-status",
        AppEvent::ThemesChanged => "themes-changed",
        AppEvent::PtyClipboardStore { .. } => "pty-clipboard-store",
        AppEvent::RemoteConnectionStatusChanged { .. } => "remote-connection-status",
        // Not "remote-mirrored": a client must not be able to tell a mirrored
        // event from a local one, and a `?types=` filter has to match the name
        // the client asked for.
        AppEvent::RemoteMirrored { event, .. } => event,
        #[cfg(feature = "dictation")]
        AppEvent::DictationDownloadProgress { .. } => "dictation-download-progress",
        #[cfg(feature = "dictation")]
        AppEvent::SpeechDownloadProgress { .. } => "speech-download-progress",
        #[cfg(feature = "dictation")]
        AppEvent::SpeechUtterance { .. } => "speech-utterance",
    }
}

/// Extract just the payload (without the wrapping `event`/`payload` tags).
/// The SSE `event:` field already carries the type, so we only need the inner data.
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
            parent_session,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "cwd": cwd,
                "agent_type": agent_type,
                "display_name": display_name,
                "parent_session": parent_session,
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
        AppEvent::DesignModeChanged {
            repo_path,
            session_id,
            status,
        } => {
            serde_json::json!({ "repo_path": repo_path, "session_id": session_id, "status": status })
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
            name,
            is_custom,
        } => {
            serde_json::json!({ "session_id": session_id, "name": name, "is_custom": is_custom })
        }
        AppEvent::SessionSuspendRequested {
            session_id,
            request_id,
        } => {
            serde_json::json!({ "session_id": session_id, "request_id": request_id })
        }
        AppEvent::TermAliasAssigned { session_id, alias } => {
            serde_json::json!({ "session_id": session_id, "alias": alias })
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
        AppEvent::ConflictAssistStatus { repo_path, payload }
        | AppEvent::ProgressRecorded { repo_path, payload }
        | AppEvent::WorkflowRunChanged { repo_path, payload }
        | AppEvent::ReviewProgress { repo_path, payload }
        | AppEvent::ProposalsReady { repo_path, payload } => {
            serde_json::json!({ "repo_path": repo_path, "payload": payload })
        }
        AppEvent::SessionStateChanged { session_id, state } => {
            // Built by the same function the desktop window emit uses, so the
            // two transports cannot drift into two shapes for one thing.
            crate::state::session_state_payload(session_id, state)
        }
        // Already the shape the desktop window emit carries: the publisher builds
        // it once and hands the same value to both transports.
        AppEvent::RemoteConnectionStatusChanged { payload } => payload.clone(),
        // The daemon's own body, untouched.
        AppEvent::RemoteMirrored { payload, .. } => payload.clone(),
        // Built once by the producer and handed to both transports, for the
        // same reason as `SessionStateChanged` above: the frontend applies one
        // shape, and a pair built from separate code in separate files drifts.
        #[cfg(feature = "dictation")]
        AppEvent::DictationDownloadProgress { payload }
        | AppEvent::SpeechDownloadProgress { payload }
        | AppEvent::SpeechUtterance { payload } => payload.clone(),
        AppEvent::WorktreeSyncStarted { repo_path, branch } => {
            // camelCase keys mirror the Tauri window `worktree-sync-started`
            // event — see `worktree::run_worktree_file_sync`.
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
        // The warm events share their builders with the Tauri window emits
        // (`worktree::warm_with_events`), like the setup-script event below.
        AppEvent::WorktreeWarmStarted {
            repo_path,
            branch,
            worktree_path,
            total,
        } => crate::state::worktree_warm_started_payload(repo_path, branch, worktree_path, *total),
        AppEvent::WorktreeWarmProgress {
            repo_path,
            branch,
            worktree_path,
            copied,
            total,
            current,
        } => crate::state::worktree_warm_progress_payload(
            repo_path,
            branch,
            worktree_path,
            *copied,
            *total,
            current.as_deref(),
        ),
        AppEvent::WorktreeWarmCompleted {
            repo_path,
            branch,
            worktree_path,
            warmed,
            warnings,
        } => crate::state::worktree_warm_completed_payload(
            repo_path,
            branch,
            worktree_path,
            *warmed,
            warnings,
        ),
        AppEvent::WorktreeSetupScriptCompleted {
            repo_path,
            branch,
            worktree_path,
            outcome,
            exit_code,
            error,
        } => {
            // Same builder the Tauri window `worktree-setup-script-completed`
            // emit uses (`worktree::spawn_worktree_setup_chain`): one payload,
            // two carriers, so the camelCase keys cannot drift.
            crate::state::worktree_setup_script_completed_payload(
                repo_path,
                branch,
                worktree_path,
                *outcome,
                *exit_code,
                error.as_deref(),
            )
        }
        AppEvent::SessionStandby {
            session_id,
            standby,
        } => {
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
    }
}

/// The desktop window event name for `event`, or `None` for a variant with its
/// own dedicated, suffixed desktop event name and its own hand-written
/// dual-emit call site — these must NEVER go through `AppState::emit_dual`,
/// which would emit them under the wrong (generic, unsuffixed) name. A local
/// event's SSE name is never interpolated, so a per-session suffixed name
/// (`pty-osc133-{id}`, etc.) simply has no representation here.
///
/// `RemoteMirrored` is also `None`: it carries a far daemon's event under that
/// daemon's own name, and `remote_mirror` owns its (filtered) window emit —
/// repeating it here would hand remote events to handlers that mutate local
/// state (see `remote_mirror::WINDOW_MIRRORABLE_EVENTS`).
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
pub(crate) fn window_event_name(event: &AppEvent) -> Option<&str> {
    match event {
        AppEvent::PtyActivity { .. }
        | AppEvent::PluginWatcherLines { .. }
        | AppEvent::PtyParsed { .. }
        | AppEvent::PtyOsc133 { .. }
        | AppEvent::PtyCwd { .. }
        | AppEvent::PtyImagePlacement { .. }
        | AppEvent::PtyImagePlacementsCleared { .. }
        | AppEvent::PtyImageDecoded { .. }
        | AppEvent::PtyExit { .. }
        | AppEvent::RemoteMirrored { .. } => None,
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
}
