use crate::pty::spawn_reader_thread;
use crate::{AppState, MAX_CONCURRENT_SESSIONS, PtySession};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

use super::guards::{Authenticated, require_local_or_auth};
use super::types::*;

pub(super) async fn detect_agents() -> impl IntoResponse {
    let mut results = Vec::new();
    for name in crate::agent::KNOWN_AGENT_BINARIES {
        let detection = crate::agent::detect_agent_binary(name.to_string()).await;
        results.push(serde_json::json!({
            "name": name,
            "path": detection.path,
            "version": detection.version,
            "supports_no_alt_screen": detection.supports_no_alt_screen,
        }));
    }
    Json(results)
}

pub(super) async fn detect_agent_binary_http(Query(q): Query<DetectBinaryQuery>) -> Response {
    if !crate::agent::KNOWN_AGENT_BINARIES.contains(&q.binary.as_str()) {
        return Json(serde_json::json!({"error": "Unknown agent"})).into_response();
    }
    let detection = crate::agent::detect_agent_binary(q.binary).await;
    Json(serde_json::json!({
        "path": detection.path,
        "version": detection.version,
        "supports_no_alt_screen": detection.supports_no_alt_screen,
    }))
    .into_response()
}

pub(super) async fn prepare_agent_launch_args_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<PrepareAgentLaunchArgsRequest>,
) -> Response {
    if let Err(response) = require_local_or_auth(&addr, auth.is_some()) {
        return response.into_response();
    }
    Json(
        crate::agent_hook_launch::prepare_agent_launch_args(
            body.agent_type,
            body.binary_path,
            body.args,
        )
        .await,
    )
    .into_response()
}

pub(super) async fn detect_installed_ides_http() -> impl IntoResponse {
    Json(crate::agent::detect_installed_ides())
}

pub(super) async fn process_prompt_http(
    Json(body): Json<ProcessPromptRequest>,
) -> impl IntoResponse {
    Json(crate::prompt::process_prompt_content(
        body.content,
        body.variables,
    ))
}

pub(super) async fn extract_prompt_variables_http(
    Json(body): Json<ExtractVariablesRequest>,
) -> impl IntoResponse {
    Json(crate::prompt::extract_prompt_variables(body.content))
}

pub(super) async fn resolve_context_variables_http(
    Json(body): Json<serde_json::Value>,
) -> Response {
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    match crate::prompt::resolve_context_variables(repo_path).await {
        Ok(vars) => Json(vars).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn resolve_prompt_variables_http(Json(body): Json<serde_json::Value>) -> Response {
    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match crate::prompt::resolve_prompt_variables(content, repo_path).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn execute_headless_prompt_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let command = match body
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        Some(c) => c.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "missing required field 'command'").into_response();
        }
    };
    let args: Vec<String> = body
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let stdin_content = body
        .get("stdinContent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let timeout_ms = body
        .get("timeoutMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(300_000);
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let env: Option<std::collections::HashMap<String, String>> = body
        .get("env")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    match crate::smart_prompt::execute_headless_prompt(
        command,
        args,
        stdin_content,
        timeout_ms,
        repo_path,
        env,
    )
    .await
    {
        Ok(output) => Json(output).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn verify_agent_session_http(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agent_type = body
        .get("agentType")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let session_id = body
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cwd = body
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let agent_pid = body
        .get("agentPid")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let env_overrides = body
        .get("envOverrides")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Json(crate::agent_session::verify_agent_session(
        agent_type,
        session_id,
        cwd,
        agent_pid,
        env_overrides,
    ))
}

pub(super) async fn spawn_agent_session(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SpawnAgentRequest>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        )
            .into_response();
    }

    // Determine binary path
    let binary_path = if let Some(ref path) = body.binary_path {
        let p = std::path::Path::new(path);
        if !p.is_absolute() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "binary_path must be an absolute path"})),
            )
                .into_response();
        }
        if !p.is_file() {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "binary_path does not point to an existing file"}),
                ),
            )
                .into_response();
        }
        path.clone()
    } else if let Some(ref agent_type) = body.agent_type {
        let detection = crate::agent::detect_agent_binary(agent_type.clone()).await;
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Agent binary '{}' not found", agent_type)})),
                ).into_response()
            }
        }
    } else {
        // Default to claude
        let detection = crate::agent::detect_agent_binary("claude".to_string()).await;
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code"})),
                ).into_response()
            }
        }
    };

    // Mirrors the binary-resolution branches above: an explicit `agent_type`
    // wins; otherwise, when we defaulted the binary to claude too (no
    // `binary_path`), the effective type is "claude"; a caller-supplied
    // `binary_path` with no `agent_type` stays unresolved, same as before.
    // Used below for `hook_instrumented_for`/`session_state.agent_type` (which
    // used to only fire when the caller passed `agent_type` explicitly, never
    // for the common default-to-claude case) and for
    // `should_defer_prompt_for_mcp_bind`.
    let effective_agent_type: Option<String> = body
        .agent_type
        .clone()
        .or_else(|| body.binary_path.is_none().then(|| "claude".to_string()));

    // Only Claude's CLI accepts a bare positional prompt. For other agents the
    // no-args default below would produce an argument-parse error and an immediate
    // exit (clap exit code 2), so reject up front with an actionable message rather
    // than spawning a doomed process.
    if body.args.is_none()
        && body
            .agent_type
            .as_deref()
            .is_some_and(|t| !crate::agent::agent_accepts_bare_prompt(t))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!(
                "Agent '{name}' cannot be spawned with a bare prompt (only Claude's CLI accepts a positional prompt; other agents exit with code 2). Pass explicit args, or spawn via the MCP agent tool with a configured run config.",
                name = body.agent_type.as_deref().unwrap_or_default()
            )})),
        )
            .into_response();
    }

    let rows = body.rows.unwrap_or(24);
    let cols = body.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response();
    }
    let session_id = Uuid::new_v4().to_string();

    // See `mcp_transport.rs`'s `should_defer_prompt_for_mcp_bind` doc comment for why this
    // is scoped to claude, non-print-mode spawns only, and threaded through
    // `spawn_deferred_prompt_delivery` after the session is registered below.
    // Only the default argv shape embeds the prompt; explicit `args` never carry
    // it, so there is nothing to withhold (and nothing must be delivered later).
    let defer_prompt_for_mcp_bind = body.args.is_none()
        && crate::mcp_http::mcp_transport::should_defer_prompt_for_mcp_bind(
            effective_agent_type.as_deref(),
            body.print_mode.unwrap_or(false),
        );

    let spawn_binary_path = binary_path.clone();
    let spawn_args = body.args.clone();
    let spawn_prompt = body.prompt.clone();
    let spawn_model = body.model.clone();
    let spawn_output_format = body.output_format.clone();
    let spawn_print_mode = body.print_mode;
    let spawn_cwd = body.cwd.clone();
    let spawn_env = body.env.clone();
    let spawn_state = Arc::clone(&state);
    let spawn_session_id = session_id.clone();
    let spawn_agent_type = body
        .agent_type
        .clone()
        .unwrap_or_else(|| "claude".to_string());
    let (pair, child) = match crate::pty::spawn_pty_pair_with_retry_async(
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        move || {
            let mut cmd = CommandBuilder::new(&spawn_binary_path);
            crate::pty::sanitize_pty_parent_env(&mut cmd);

            let mut launch_args = Vec::new();
            if let Some(ref args) = spawn_args {
                launch_args.extend(args.iter().cloned());
            } else {
                if spawn_print_mode.unwrap_or(false) {
                    launch_args.push("--print".to_string());
                }
                if let Some(ref format) = spawn_output_format {
                    launch_args.push("--output-format".to_string());
                    launch_args.push(format.clone());
                }
                if let Some(ref model) = spawn_model {
                    launch_args.push("--model".to_string());
                    launch_args.push(model.clone());
                }
                if !defer_prompt_for_mcp_bind {
                    launch_args.push(spawn_prompt.clone());
                }
            }
            crate::pty::apply_agent_screen_env(&mut cmd, &spawn_env);
            for arg in crate::agent_hook_launch::augment_args(
                &spawn_agent_type,
                &spawn_binary_path,
                &launch_args,
                &crate::config::config_dir(),
            ) {
                cmd.arg(arg);
            }

            // Derived TUIC_* context first, so caller env can still override it.
            crate::pty::inject_worktree_env(&mut cmd, spawn_cwd.as_deref());
            for (key, value) in &spawn_env {
                cmd.env(key, value);
            }

            crate::pty::bind_pty_identity(&spawn_state, &mut cmd, &spawn_session_id, None);

            if let Some(ref cwd) = spawn_cwd {
                cmd.cwd(crate::cli::expand_tilde(cwd));
            }
            cmd
        },
    )
    .await
    {
        Ok(pair_and_child) => pair_and_child,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)})),
            )
                .into_response();
        }
    };

    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)})),
            )
                .into_response();
        }
    };

    let paused = Arc::new(AtomicBool::new(false));
    // Pre-set the session's agent type so the PTY reader's agent_active gate turns
    // on immediately and intent/suggest protocol tokens are parsed from the first
    // line of output. Seeded before registration because that is what publishes
    // `session-created`.
    let mut session_state = crate::state::SessionState {
        spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
        ..Default::default()
    };
    if let Some(ref agent_type) = effective_agent_type {
        session_state.hook_instrumented = crate::pty::hook_instrumented_for(
            &crate::config::load_agents_config(),
            Some(agent_type.as_str()),
        );
        session_state.seed_configured_agent(Some(agent_type.clone()));
    }
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), session_state);

    // Buffers, alias, metrics, grid watch and the session-created broadcast,
    // sharing one helper with session::spawn_pty_session so the VT screen can only
    // ever be built at the geometry the PTY was opened with.
    super::session::register_pty_session(
        &state,
        &session_id,
        PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd: body.cwd.clone(),
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            // Agent-created unless our own client says a human launched it —
            // the same `is_remote = !user_initiated` rule as `POST /sessions`.
            is_remote: !body.user_initiated,
            shell: binary_path.clone(),
        },
        rows,
        cols,
        effective_agent_type.clone(),
        None,
        None,
        true,
    );

    // `register_pty_session` above already announced on both transports,
    // carrying the real `agent_type` — previously this route's separate
    // desktop-half emit dropped it (always sent `null`), even though the
    // bus half had it.
    spawn_reader_thread(reader, paused, session_id.clone(), state.clone(), None);

    // Withheld from argv above by `defer_prompt_for_mcp_bind` — deliver it once this
    // session's own MCP identity binds (bounded, fail-open). See
    // `spawn_deferred_prompt_delivery`'s doc comment. `from_tuic_session: None` — this is
    // an HTTP-originated spawn with no caller-agent identity to attribute the message to.
    if defer_prompt_for_mcp_bind {
        crate::mcp_http::mcp_transport::spawn_deferred_prompt_delivery(
            state,
            session_id.clone(),
            None,
            body.prompt.clone(),
        );
    }

    let mut response = serde_json::json!({"session_id": session_id});
    if defer_prompt_for_mcp_bind {
        response["prompt_delivery"] = serde_json::json!(
            "queued — the child must reach its ready prompt first; delivered via mailbox once its MCP identity binds"
        );
    }

    (StatusCode::CREATED, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_request() -> SpawnAgentRequest {
        SpawnAgentRequest {
            rows: Some(24),
            cols: Some(80),
            cwd: None,
            prompt: "test prompt".into(),
            model: None,
            print_mode: None,
            output_format: None,
            agent_type: None,
            binary_path: Some(
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            args: Some(vec!["--help".into()]),
            env: Default::default(),
            user_initiated: false,
        }
    }

    #[tokio::test]
    async fn http_typed_agent_launch_uses_the_same_rust_screen_policy() {
        let script = crate::test_support::fake_ssh_script(
            "http-terminal-screen-args",
            "printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let request = PrepareAgentLaunchArgsRequest {
            agent_type: "codex".into(),
            binary_path: script.to_string_lossy().into_owned(),
            args: vec!["resume".into()],
        };
        let response =
            prepare_agent_launch_args_http(ConnectInfo(loopback()), None, Json(request)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_json(response).await,
            serde_json::json!(["--no-alt-screen", "resume"])
        );

        let forbidden = PrepareAgentLaunchArgsRequest {
            agent_type: "codex".into(),
            binary_path: script.to_string_lossy().into_owned(),
            args: vec!["resume".into()],
        };
        let response =
            prepare_agent_launch_args_http(ConnectInfo(lan()), None, Json(forbidden)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn launch_args_request_rejects_removed_screen_override() {
        let request = serde_json::json!({
            "agentType": "codex",
            "binaryPath": "codex",
            "args": [],
            "allowAltScreen": true,
        });
        assert!(serde_json::from_value::<PrepareAgentLaunchArgsRequest>(request).is_err());
    }

    #[test]
    fn spawn_request_rejects_removed_screen_override() {
        for key in ["allow_alt_screen", "allowAltScreen"] {
            let mut request = serde_json::json!({
                "rows": 24,
                "cols": 80,
                "prompt": "work",
            });
            request[key] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<SpawnAgentRequest>(request).is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn spawn_request_accepts_browser_transport_body() {
        let body = include_str!("../../tests/fixtures/spawn_agent_http_body.json");
        let parsed: SpawnAgentRequest = serde_json::from_str(body).expect("HTTP spawn request");
        assert_eq!(parsed.rows, Some(30));
        assert_eq!(parsed.cols, Some(100));
        assert_eq!(parsed.cwd.as_deref(), Some("/agent"));
        assert_eq!(parsed.env.get("PROFILE").map(String::as_str), Some("work"));
        assert!(
            !parsed.user_initiated,
            "absent user_initiated means agent-created"
        );
    }

    #[test]
    fn spawn_request_accepts_user_initiated() {
        let parsed: SpawnAgentRequest =
            serde_json::from_value(serde_json::json!({"prompt": "", "user_initiated": true}))
                .expect("user_initiated is a known field");
        assert!(parsed.user_initiated);
    }

    #[tokio::test]
    async fn http_agent_spawn_uses_per_agent_screen_setting() {
        let script = crate::test_support::fake_ssh_script(
            "http-agent-screen-choice",
            "if [ \"$1\" = '--help' ]; then printf '%s\\n' '--no-alt-screen'; else printf 'ARGS=%s\\nTUIC_SESSION=%s\\n' \"$*\" \"$TUIC_SESSION\"; read unused; fi",
            "if \"%1\"==\"--help\" (echo --no-alt-screen) else (echo ARGS=%* & echo TUIC_SESSION=%TUIC_SESSION% & set /p HOLD=)",
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let body: SpawnAgentRequest = serde_json::from_value(serde_json::json!({
            "rows": 24,
            "cols": 80,
            "prompt": "ignored",
            "agent_type": "codex",
            "binary_path": script.to_string_lossy(),
            "args": ["resume"],
        }))
        .unwrap();
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let session_id = response_json(response).await["session_id"]
            .as_str()
            .unwrap()
            .to_string();
        // Catches: HTTP agent creation labels a direct program as a shell,
        // so its own foreground root would revoke the configured identity.
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(&session_id)
                .unwrap()
                .spawn_root_role,
            crate::state::SpawnRootRole::DirectProgram
        );
        assert!(
            state
                .session_maps
                .sessions
                .get(&session_id)
                .unwrap()
                .lock()
                ._child
                .process_id()
                .is_some()
        );
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(buffer) = state.grid.vt_log_buffers.get(&session_id) {
                    let text = buffer.lock().screen_rows().join("\n");
                    if text.contains("ARGS=") {
                        break text;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake agent output");
        assert!(output.contains("ARGS=--no-alt-screen resume"), "{output}");
        assert!(
            output.contains(&format!("TUIC_SESSION={session_id}")),
            "the spawned process must know its terminal identity: {output}"
        );
        let row = super::super::session::local_session_rows(&state)
            .into_iter()
            .find(|row| row.session_id == session_id)
            .unwrap();
        assert_eq!(row.tuic_session.as_deref(), Some(session_id.as_str()));
        super::super::session::close_session(State(state), axum::extract::Path(session_id)).await;
    }

    #[tokio::test]
    async fn http_agent_spawn_accepts_model_and_env_with_ipc_field_names() {
        let script = crate::test_support::fake_ssh_script(
            "http-agent-model-env",
            "printf 'ARGS=%s\nSPAWN_VALUE=%s\nTUIC_SESSION=%s\n' \"$*\" \"$SPAWN_VALUE\" \"$TUIC_SESSION\"; read unused",
            "echo ARGS=%* & echo SPAWN_VALUE=%SPAWN_VALUE% & echo TUIC_SESSION=%TUIC_SESSION% & set /p HOLD=",
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let body: SpawnAgentRequest = serde_json::from_value(serde_json::json!({
            "prompt": "task",
            "agent_type": "claude",
            "binary_path": script.to_string_lossy(),
            "model": "sonnet",
            "env": {"SPAWN_VALUE": "caller"},
        }))
        .unwrap();
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let session_id = response_json(response).await["session_id"]
            .as_str()
            .unwrap()
            .to_string();
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(buffer) = state.grid.vt_log_buffers.get(&session_id) {
                    let text = buffer.lock().screen_rows().join("\n");
                    if text.contains("SPAWN_VALUE=") {
                        break text;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake agent output");
        // An interactive claude spawn withholds the prompt from argv until the
        // child's MCP identity binds (`should_defer_prompt_for_mcp_bind`).
        assert!(output.contains("ARGS=--model sonnet"), "{output}");
        assert!(!output.contains("ARGS=--model sonnet task"), "{output}");
        assert!(output.contains("SPAWN_VALUE=caller"), "{output}");
        assert!(
            output.contains(&format!("TUIC_SESSION={session_id}")),
            "{output}"
        );
        super::super::session::close_session(State(state), axum::extract::Path(session_id)).await;
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn loopback() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
    fn lan() -> SocketAddr {
        "192.168.1.2:1".parse().unwrap()
    }

    fn authed() -> Option<Extension<Authenticated>> {
        Some(Extension(Authenticated))
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_rejects_unauthenticated_non_loopback() {
        let resp = execute_headless_prompt_http(
            ConnectInfo(lan()),
            None,
            Json(serde_json::json!({ "command": "echo", "args": ["x"] })),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_loopback_passes_guard() {
        // Loopback with missing 'command' field must yield 400 from the validator,
        // proving the 403 guard is NOT fired for loopback callers.
        let resp = execute_headless_prompt_http(
            ConnectInfo(loopback()),
            None,
            Json(serde_json::json!({})),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_authenticated_remote_passes_guard() {
        // Authenticated remote (full-trust, story 059) must pass the guard:
        // empty body yields 400 from the validator, not 403.
        let resp =
            execute_headless_prompt_http(ConnectInfo(lan()), authed(), Json(serde_json::json!({})))
                .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn spawn_agent_rejects_unauthenticated_remote_before_validation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = spawn_agent_session(
            State(state),
            ConnectInfo(lan()),
            None,
            Json(spawn_request()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn spawn_agent_rejects_relative_and_missing_binary_paths() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut relative = spawn_request();
        relative.binary_path = Some("bin/agent".into());
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(relative),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await["error"],
            "binary_path must be an absolute path"
        );

        let mut missing = spawn_request();
        missing.binary_path = Some(
            std::env::temp_dir()
                .join("definitely-not-a-tuic-agent")
                .to_string_lossy()
                .into_owned(),
        );
        let response =
            spawn_agent_session(State(state), ConnectInfo(loopback()), None, Json(missing)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await["error"],
            "binary_path does not point to an existing file"
        );
    }

    #[tokio::test]
    async fn spawn_agent_rejects_non_claude_bare_prompt_before_pty_creation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut request = spawn_request();
        request.agent_type = Some("codex".into());
        request.args = None;

        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(request),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response_json(response).await["error"]
                .as_str()
                .unwrap()
                .contains("bare prompt")
        );
        assert!(state.session_maps.sessions.is_empty());
    }

    #[tokio::test]
    async fn spawn_agent_rejects_invalid_terminal_dimensions_before_pty_creation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut request = spawn_request();
        request.rows = Some(0);

        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(request),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response_json(response).await["error"].is_string());
        assert!(state.session_maps.sessions.is_empty());
    }

    // --- MCP handshake readiness race coverage ---------------------------
    //
    // No test above ever reaches PTY creation (every one asserts a 400/403
    // that fires first). These are the first end-to-end spawns in this file.
    // Mirrors the equivalent coverage already proven for
    // `mcp__tuicommander__agent action=spawn` in `mcp_transport.rs`'s own
    // test module (`agent_spawn_defers_claude_prompt_until_mcp_identity_binds`,
    // `agent_spawn_defers_claude_prompt_via_the_ordinary_no_binary_path_call_shape`,
    // `agent_spawn_does_not_defer_a_print_mode_claude_prompt`).

    #[cfg(unix)]
    const LONG_LIVED_TEST_BINARY: &str = "/bin/cat";

    /// Answers `--version`/`-v` instantly (unlike a bare `/bin/cat` symlink,
    /// which would hang forever reading stdin for those flags — BSD `cat -v`
    /// with no file operand never returns), then `exec`s into
    /// `LONG_LIVED_TEST_BINARY` for the real invocation. Needed because
    /// `detect_agent_binary`'s `get_binary_version` probes every resolved
    /// binary with `--version`, and the spawned process must stay alive long
    /// enough for the deferred-delivery task (and, in the `$TUIC_SESSION`
    /// test, for the env-observing shell command) to actually run. Unlike
    /// `mcp_transport.rs`'s equivalent tests, this route never calls
    /// `agent_hook_launch::augment_args`, so there is no `--settings` flag to
    /// choke on and no `native_status_signals`/config-dir-override dance
    /// needed here.
    #[cfg(unix)]
    fn write_fake_claude_script(dir: &std::path::Path) -> std::path::PathBuf {
        let fake_claude = dir.join("claude");
        std::fs::write(
            &fake_claude,
            format!("#!/bin/sh\ncase \"$1\" in\n  --version|-v) exit 0 ;;\nesac\nexec {LONG_LIVED_TEST_BINARY}\n"),
        )
        .expect("write fake claude script");
        let mut perms = std::fs::metadata(&fake_claude)
            .expect("stat fake claude")
            .permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&fake_claude, perms).expect("chmod fake claude");
        fake_claude
    }

    #[cfg(unix)]
    struct PathGuard(Option<String>);
    #[cfg(unix)]
    impl Drop for PathGuard {
        fn drop(&mut self) {
            match &self.0 {
                Some(p) => unsafe { std::env::set_var("PATH", p) },
                None => unsafe { std::env::remove_var("PATH") },
            }
        }
    }

    #[cfg(unix)]
    fn prepend_fake_claude_to_path(dir: &std::path::Path) -> PathGuard {
        let prior = std::env::var("PATH").ok();
        let guard = PathGuard(prior.clone());
        let new_path = match &prior {
            Some(p) => format!("{}:{p}", dir.display()),
            None => dir.display().to_string(),
        };
        unsafe { std::env::set_var("PATH", new_path) };
        guard
    }

    /// The ordinary call shape (no `binary_path`, no `agent_type`) resolves
    /// `effective_agent_type` to `"claude"` via `detect_agent_binary`'s real
    /// PATH lookup — the exact shape a code review caught as the one that
    /// mattered for the sibling MCP-tool fix (a first version there only
    /// deferred when a caller explicitly passed `binary_path`, which masked
    /// the gap because both its own tests happened to pass one). Asserts the
    /// prompt is withheld until this session's MCP identity binds, then
    /// delivered via the mailbox.
    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_agent_session_defers_claude_prompt_until_mcp_identity_binds() {
        let bin_dir = tempfile::TempDir::new().expect("bin tempdir");
        write_fake_claude_script(bin_dir.path());
        let _path_guard = prepend_fake_claude_to_path(bin_dir.path());

        let state = crate::mcp_http::tests::test_state();
        let mut request = spawn_request();
        request.binary_path = None;
        request.agent_type = None;
        request.args = None;
        request.prompt = "the real deferred task".to_string();

        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(request),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        let session_id = body["session_id"].as_str().expect("session_id").to_string();
        assert_eq!(
            body["prompt_delivery"].as_str(),
            Some(
                "queued — the child must reach its ready prompt first; delivered via mailbox once its MCP identity binds"
            ),
            "response must surface the deferral, matching the MCP tool's spawn_response field"
        );

        assert!(
            state
                .agent_inbox
                .get(&session_id)
                .is_none_or(|inbox| inbox.is_empty()),
            "the prompt must not be delivered before the MCP identity binds"
        );

        assert!(
            crate::mcp_http::mcp_transport::apply_initialize_identity(
                &state,
                "mcp-http-defer-child",
                Some(&session_id),
            ),
            "apply_initialize_identity must accept this session's own (valid UUID) id"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            if state
                .agent_inbox
                .get(&session_id)
                .is_some_and(|inbox| !inbox.is_empty())
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "prompt was never delivered after the MCP identity bound"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let inbox = state.agent_inbox.get(&session_id).unwrap();
        assert_eq!(inbox.back().unwrap().content, "the real deferred task");
    }

    /// Print-mode is one-shot with no later delivery opportunity — the prompt
    /// must stay in launch argv unchanged, never deferred.
    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_agent_session_does_not_defer_a_print_mode_claude_prompt() {
        let bin_dir = tempfile::TempDir::new().expect("bin tempdir");
        write_fake_claude_script(bin_dir.path());
        let _path_guard = prepend_fake_claude_to_path(bin_dir.path());

        let state = crate::mcp_http::tests::test_state();
        let mut request = spawn_request();
        request.binary_path = None;
        request.agent_type = None;
        request.args = None;
        request.print_mode = Some(true);

        let response =
            spawn_agent_session(State(state), ConnectInfo(loopback()), None, Json(request)).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        assert!(
            body.get("prompt_delivery").is_none(),
            "print-mode spawns must never defer, so this field must be absent: {body}"
        );
    }

    /// `spawn_agent_session` used to be the one session-creating path in this
    /// codebase that never called `bind_pty_identity` at all — its children
    /// got no `$TUIC_SESSION`, so any eventual MCP bind would have landed
    /// under a fresh, unrelated UUID, never this PTY's own `session_id`.
    /// Proves the real child process now receives it.
    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_agent_session_binds_tuic_session_identity_on_the_real_child() {
        let bin_dir = tempfile::TempDir::new().expect("bin tempdir");
        let out_dir = tempfile::TempDir::new().expect("out tempdir");
        let out_file = out_dir.path().join("tuic_session.txt");
        let fake_claude = bin_dir.path().join("probe");
        std::fs::write(
            &fake_claude,
            format!(
                "#!/bin/sh\necho \"$TUIC_SESSION\" > '{}'\nexec {LONG_LIVED_TEST_BINARY}\n",
                out_file.display()
            ),
        )
        .expect("write probe script");
        let mut perms = std::fs::metadata(&fake_claude)
            .expect("stat probe script")
            .permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&fake_claude, perms).expect("chmod probe script");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut request = spawn_request();
        request.binary_path = Some(fake_claude.to_string_lossy().into_owned());
        request.args = Some(vec![]);

        let response =
            spawn_agent_session(State(state), ConnectInfo(loopback()), None, Json(request)).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        let session_id = body["session_id"].as_str().expect("session_id").to_string();

        // 60s, not 5s — the same real-subprocess-under-full-workspace-load margin
        // problem `mcp_transport.rs`'s own `agent_spawn_defers_claude_prompt_via_the_ordinary_no_binary_path_call_shape`
        // test documents (confirmed here too: failed once at 5s with a 7.2s actual
        // duration under a full `cargo nextest run --no-fail-fast`, never in a
        // smaller/isolated run). A real fork+exec+shell-script-write is not bounded
        // by anything this test controls once dozens of other tests are spawning
        // real processes concurrently.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            if let Ok(contents) = std::fs::read_to_string(&out_file)
                && !contents.trim().is_empty()
            {
                assert_eq!(
                    contents.trim(),
                    session_id,
                    "the child's $TUIC_SESSION must equal this PTY's own session_id"
                );
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "child never wrote its $TUIC_SESSION to the probe file"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

/// Body of `POST /agents/detect-all`.
#[derive(serde::Deserialize)]
pub(super) struct DetectAllBinariesRequest {
    pub binaries: Vec<String>,
}

/// `POST /agents/detect-all` — mirror of the `detect_all_agent_binaries`
/// command. Returns the same binary-name -> detection map.
pub(super) async fn detect_all_agent_binaries_http(
    Json(body): Json<DetectAllBinariesRequest>,
) -> impl IntoResponse {
    Json(crate::agent::detect_all_agent_binaries(body.binaries).await)
}

/// Body of `POST /agents/open-in-app`.
#[derive(serde::Deserialize)]
pub(super) struct OpenInAppRequest {
    pub path: String,
    pub app: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
}

/// `POST /agents/open-in-app` — mirror of the `open_in_app` command. The
/// command resolves to nothing, so this answers the `null` IPC returns.
pub(super) async fn open_in_app_http(Json(body): Json<OpenInAppRequest>) -> Response {
    super::json_result(crate::agent::open_in_app(
        body.path, body.app, body.line, body.col,
    ))
}
