// Catches: Settings rows reuse long MCP paragraphs or omit newly registered tools.
#[test]
fn native_tool_catalog_summaries_are_short_and_not_description_prefixes() {
    let definitions = native_tool_definitions(true, true);
    let tools = definitions.as_array().unwrap();
    let catalog = native_tool_catalog();
    assert!(!tools.is_empty());
    assert_eq!(catalog.len(), tools.len());
    assert_eq!(NATIVE_TOOL_SUMMARIES.len(), tools.len());
    for (tool, entry) in tools.iter().zip(&catalog) {
        let name = tool["name"].as_str().unwrap();
        let description = tool["description"].as_str().unwrap();
        let summary = entry["summary"].as_str().unwrap();
        assert_eq!(entry["name"], tool["name"]);
        assert_eq!(entry["description"], tool["description"]);
        assert!(!summary.trim().is_empty(), "{name} needs a summary");
        assert!(summary.chars().count() <= 70, "{name} summary is too long");
        assert!(
            !summary.contains('\n'),
            "{name} summary must be one sentence"
        );
        assert!(
            !description.starts_with(summary),
            "{name} summary must not reuse its MCP description"
        );
        assert!(
            tool.get("summary").is_none(),
            "{name} leaks app metadata into MCP"
        );
        assert_eq!(
            NATIVE_TOOL_SUMMARIES
                .iter()
                .filter(|(key, _)| *key == name)
                .count(),
            1,
            "{name} must have exactly one declared summary"
        );
    }
}

/// Catches: the answers view stays empty for agents outside the orchestrator
/// repo because TUIC never teaches the 💬 marker, even with the intent and
/// suggest markers switched off.
#[test]
fn mcp_instructions_teach_the_answer_marker_regardless_of_other_markers() {
    let state = test_state();
    {
        let mut cfg = state.config.write();
        cfg.intent_tab_title = false;
        cfg.suggest_followups = false;
    }
    for collapse in [false, true] {
        let instructions = build_mcp_instructions_for_mode(&state, None, collapse);
        assert!(
            instructions.contains(
                "- `💬 ` — prefix every sentence that directly answers the user's question"
            )
        );
    }
}

#[test]
fn mcp_instructions_request_current_intent_at_task_start_and_phase_changes() {
    let state = test_state();
    let instructions = build_mcp_instructions_for_mode(&state, None, true);

    assert!(instructions.contains("at the start of every user task"));
    assert!(instructions.contains("on each material work-phase change"));
    assert!(instructions.contains("currently in progress in present tense"));

    state.config.write().intent_tab_title = false;
    let disabled = build_mcp_instructions_for_mode(&state, None, true);
    assert!(!disabled.contains("`intent: <desc> (<title>)`"));
}

#[test]
fn mcp_instructions_omit_suggest_when_globally_disabled() {
    // Symmetric to the intent case above — suggest_followups had no direct
    // coverage of its own global gate.
    let state = test_state();
    let enabled = build_mcp_instructions_for_mode(&state, None, true);
    assert!(enabled.contains("suggest: [ A | B | C ]"));

    state.config.write().suggest_followups = false;
    let disabled = build_mcp_instructions_for_mode(&state, None, true);
    assert!(!disabled.contains("suggest: [ A | B | C ]"));
}

#[test]
fn marker_flags_for_agent_and_rule_cannot_be_widened_by_either_side() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = test_state();

    // No per-agent entry: effective flags follow the global toggle alone.
    state.config.write().intent_tab_title = true;
    state.config.write().suggest_followups = true;
    assert_eq!(marker_flags_for_agent(&state, Some("claude")), (true, true));

    // Per-agent override can narrow a globally-on marker off.
    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            intent_tab_title: Some(false),
            suggest_followups: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::load_agents_config(), agents_cfg).unwrap();
    assert_eq!(
        marker_flags_for_agent(&state, Some("claude")),
        (false, false),
        "a per-agent override must be able to narrow a globally-on marker"
    );

    // A per-agent override cannot widen a globally-off marker back on — the
    // rule is an AND, not an OR, in either direction.
    state.config.write().intent_tab_title = false;
    state.config.write().suggest_followups = false;
    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            intent_tab_title: Some(true),
            suggest_followups: Some(true),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::load_agents_config(), agents_cfg).unwrap();
    assert_eq!(
        marker_flags_for_agent(&state, Some("claude")),
        (false, false),
        "the global toggle must win when off, even if the per-agent override says on"
    );

    // An agent type with no entry in agents.json follows the global toggle alone.
    assert_eq!(marker_flags_for_agent(&state, Some("codex")), (false, false));
}

// --- the voice tool (817-f67c) ---

fn native_tool_named(name: &str) -> serde_json::Value {
    native_tool_definitions(true, true)
        .as_array()
        .expect("the definitions are an array")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("no native tool named {name}"))
        .clone()
}

/// Discovery must work on the surface the client is already looking at.
///
/// `notifications/tools/list_changed` is not a mechanism we can rely on —
/// Claude Code does not refetch on it — so arming cannot be what makes the
/// tool appear. It is always in the list, always in the search corpus and
/// always fetchable by name; whether it can *do* anything is an answer the
/// tool itself gives.
#[tokio::test]
async fn voice_is_discoverable_by_list_search_and_name_without_a_list_change() {
    let state = test_state();
    spawn_tool_search_index_updater(Arc::clone(&state));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let definition = native_tool_named("voice");
    assert_eq!(
        definition["inputSchema"]["required"],
        serde_json::json!(["action"])
    );
    for action in ["speak", "stop", "status"] {
        assert!(
            definition["inputSchema"]["properties"]["action"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains(action),
            "the action list must name {action}"
        );
    }

    let found = handle_search_tools(
        &state,
        &serde_json::json!({"query": "speak a reply out loud to the user", "limit": 5}),
    );
    let names: Vec<&str> = found["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(
        names.contains(&"voice"),
        "a model describing what it wants must find the tool: {names:?}"
    );

    let schema = handle_get_tool_schema(&state, &serde_json::json!({"tool_name": "voice"}));
    assert_eq!(schema["inputSchema"], definition["inputSchema"]);
}

/// Per docs/sync-matrix.md a tool-surface change must land in the listing AND
/// the search corpus — a tool missing from the latter is invisible under
/// `collapse_tools` and to lazy discovery.
#[test]
fn the_task_tool_is_listed_and_searchable() {
    let state = test_state();

    let listed = merged_tool_definitions(&state, None, None);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "task"),
        "task must appear in tools/list"
    );
    assert!(
        searchable_tool_definitions(&state)
            .iter()
            .any(|t| t["name"] == "task"),
        "task must appear in the search corpus"
    );
}

#[test]
fn meta_tool_definitions_returns_exactly_three_tools_with_expected_names() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);
    let names = tool_names(&defs);
    assert_eq!(names.len(), 3, "meta_tool_definitions must return 3 tools");
    assert_eq!(names, vec!["search_tools", "get_tool_schema", "call_tool"]);
    // Each must have a non-empty description and an inputSchema object.
    for tool in defs.as_array().unwrap() {
        assert!(
            tool["description"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "meta tool {:?} missing description",
            tool["name"]
        );
        assert!(
            tool["inputSchema"].is_object(),
            "meta tool {:?} missing inputSchema",
            tool["name"]
        );
    }
}

#[test]
fn meta_tool_names_constant_matches_definitions() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);
    let names = tool_names(&defs);
    let expected: Vec<String> = META_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected);
}

/// Criterion 2 of story 789-f6ed: exactly one tool family is registered.
///
/// This list used to carry 13 `ai_terminal_*` tools after `debug`. They
/// overlapped this family without being equivalent, and six of them needed
/// a filesystem sandbox only the embedded agent loop creates - so they
/// refused every external caller before dispatch. The comparison behind the
/// deletion is `plans/ego-integration/tool-family-comparison.md`.
///
/// These names are a public contract: they appear in users' ego rule files,
/// so renaming one silently stops a user's policy from matching.
#[tokio::test]
async fn native_tool_definitions_are_the_one_surviving_family() {
    let state = test_state();
    {
        let mut config = state.config.write();
        config.disabled_native_tools.clear();
        config.collapse_tools = false;
    }
    let listed = tools_list_result(
        &state,
        HeaderMap::new(),
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    )
    .await;
    let names = tool_names(&listed["result"]["tools"]);
    assert_eq!(
        names,
        vec![
            "automations",
            "secret",
            "telegram",
            "session",
            "agent",
            "task",
            "remote",
            "repo",
            "story",
            "workflow_run",
            "workflow_story_create",
            "workflow_report",
            "workflow_launch",
            "progress",
            "ui",
            "plugin_dev_guide",
            "config",
            "debug",
            "voice",
        ],
        "native_tool_definitions must return exactly the one family, in order"
    );
    let called = handle_mcp_tool_call(
        &state,
        loopback_addr(),
        "telegram",
        &serde_json::json!({"action":"send","text":"hello"}),
        None,
    )
    .await;
    assert_eq!(
        called["error"], "telegram_invalid_state",
        "unbound Telegram invocation must reach its native authority check"
    );
}

/// Claude Code defers MCP tools behind ToolSearch: the model sees only the
/// name and must load the schema first, which in practice it never does
/// for `progress` — so no Claude terminal ever reported done/blocked while
/// Codex, which loads every tool, did. `anthropic/alwaysLoad` exempts the
/// tool from deferral. Only `progress` carries it: it is the one tool the
/// protocol makes mandatory, and every always-loaded schema costs tokens.
#[test]
fn progress_is_the_only_tool_claude_code_must_not_defer() {
    let defs = native_tool_definitions(true, true);
    let always_loaded: Vec<&str> = defs
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["_meta"]["anthropic/alwaysLoad"] == serde_json::json!(true))
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(always_loaded, vec!["progress"]);
}

/// What ego actually receives, rather than only which flag was computed.
///
/// The collapsed surface is three meta-tools plus `progress`, which stays
/// directly callable because reporting must not require a discovery call
/// first. Everything else is reachable through `call_tool`, so this is a
/// smaller list and not a smaller capability.
#[test]
fn the_collapsed_surface_is_the_meta_tools_plus_progress() {
    let state = test_state();

    let collapsed = tool_names(&merged_tool_definitions_for_mode(&state, None, true));
    assert_eq!(
        collapsed,
        ["search_tools", "get_tool_schema", "call_tool", "progress"]
            .map(String::from)
            .to_vec(),
        "ego must get the discovery surface, not the catalogue"
    );

    let full = tool_names(&merged_tool_definitions_for_mode(&state, None, false));
    assert!(
        full.len() > collapsed.len(),
        "the uncollapsed surface must be the larger one, or this test proves \
             nothing about the saving: {full:?}"
    );
    assert!(
        full.iter().any(|name| name == "session"),
        "the uncollapsed surface is the native catalogue: {full:?}"
    );
}

#[tokio::test]
async fn disabled_progress_is_rejected_by_direct_and_collapsed_dispatch() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["progress".to_string()];
    rebuild_tool_search_index(&state);
    let args = serde_json::json!({"type":"done", "text":"Must not persist."});

    let direct = handle_mcp_tool_call(&state, loopback_addr(), "progress", &args, None).await;
    assert!(direct["error"].as_str().unwrap().contains("disabled"));
    let collapsed = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({"tool_name":"progress", "arguments": args}),
        None,
        None,
    )
    .await;
    assert!(collapsed["error"].as_str().unwrap().contains("disabled"));
    assert!(
        !tool_names(&merged_tool_definitions_for_mode(&state, None, true))
            .contains(&"progress".to_string())
    );
}

#[test]
fn session_description_mentions_tmux_pane_semantics() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let desc = session["description"].as_str().unwrap();
    assert!(
        desc.contains("tmux"),
        "session description must reference tmux for discoverability"
    );
    assert!(
        desc.contains("send-keys") || desc.contains("send_keys"),
        "session description must mention send-keys equivalent"
    );
    assert!(
        desc.contains("capture-pane") || desc.contains("capture_pane"),
        "session description must mention capture-pane equivalent"
    );
}

#[test]
fn agent_tool_includes_messaging_actions() {
    let defs = native_tool_definitions(true, true);
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let action_desc = agent["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &["register", "list_peers", "send", "inbox", "wait"] {
        assert!(
            action_desc.contains(action),
            "agent action description must include '{action}'"
        );
    }
    assert!(
        agent["inputSchema"]["properties"]["args"]["description"]
            .as_str()
            .unwrap()
            .contains("prevent_alt_screen"),
        "spawn schema must name the per-agent screen setting"
    );
    assert!(
        agent["inputSchema"]["properties"]["limit"].is_object(),
        "inbox accepts limit, so the MCP schema must advertise it"
    );
}

#[test]
fn agent_tool_description_carries_orchestration_crash_course() {
    // Tool semantics belong in descriptions, available when discovered.
    // Initial model visibility of descriptions and initialize instructions
    // depends on the harness (see docs/backend/mcp-http.md). The primer
    // and wait/send delivery semantics must live here.
    let defs = native_tool_definitions(true, true);
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let desc = agent["description"].as_str().unwrap();
    assert!(
        desc.contains("Managed PTYs auto-bind"),
        "must state the condition for auto-identity"
    );
    assert!(
        desc.contains("headerless external caller"),
        "must explain how external callers establish identity"
    );
    assert!(desc.contains("wait"), "must mention the wait primitive");
    assert!(
        desc.contains("the payload is never typed into the recipient's composer"),
        "must state the mail-stays-mail invariant: a send is never keystrokes"
    );
    assert!(
        desc.contains("generic `agent action=inbox` wake"),
        "must explain payload-free wake delivery"
    );
    assert!(
        desc.contains("do NOT poll"),
        "must discourage polling loops"
    );
    assert!(desc.contains("state only"));
    assert!(desc.contains("must report task output"));
}

// ---- agent tool schema gating on prefer_tuic_spawning/messaging ---------
//
// build_mcp_instructions's prose recommendation ("Prefer TUICommander for
// peers/teams") was already gated on these two flags. The `agent` tool's
// own baked-in schema description — what a client actually reads via
// `tools/list`, independently of the connect-time instructions — was not,
// and a live Claude Code teammate-spawn request confirmed that gap: it
// still called `agent action=spawn` with both preferences persisted as
// `false`, because this description told it to regardless. These tests
// cover the fix (`agent_tool_definition`) and the plumbing that resolves
// it per MCP connection (`resolve_prefer_tuic_flags_for_session`).

#[test]
fn agent_tool_definition_omits_spawn_recommendation_when_spawning_not_preferred() {
    let agent = agent_tool_definition(false, true);
    let desc = agent["description"].as_str().unwrap();
    assert!(
        !desc.contains("Spawn a named peer: spawn name=worker"),
        "must not steer toward spawn when spawning is not preferred"
    );
    assert!(
        desc.contains("prefer that over `spawn` here"),
        "must explicitly redirect to the host's own native spawning"
    );
    // Messaging guidance is untouched by the spawning flag alone.
    assert!(desc.contains("Managed PTYs auto-bind"));
    assert!(desc.contains("Talk to it: send to=<peer>"));
    // The action itself remains callable — "prefer" is not "disable".
    let action_enum = agent["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(action_enum.contains("spawn"));
    assert!(
        desc.contains("- spawn: Launch agent in new PTY"),
        "the spawn action bullet itself must stay documented"
    );
}

#[test]
fn agent_tool_definition_omits_messaging_recommendation_when_messaging_not_preferred() {
    let agent = agent_tool_definition(true, false);
    let desc = agent["description"].as_str().unwrap();
    assert!(
        !desc.contains("Managed PTYs auto-bind"),
        "must not steer toward register when messaging is not preferred"
    );
    assert!(
        !desc.contains("Talk to it: send to=<peer>"),
        "must not steer toward send when messaging is not preferred"
    );
    assert!(
        !desc.contains("Lifecycle notifications carry state only"),
        "the messaging-only lifecycle-reporting step must be dropped"
    );
    assert!(
        desc.contains("prefer that over `register`/`list_peers`/`send`/`inbox` here"),
        "must explicitly redirect to the host's own native messaging"
    );
    // Spawning guidance is untouched by the messaging flag alone.
    assert!(desc.contains("Spawn a named peer: spawn name=worker"));
    // The messaging actions remain callable.
    for action in &["register", "list_peers", "send", "inbox"] {
        assert!(
            desc.contains(&format!("- {action}:")),
            "action bullet for '{action}' must stay documented"
        );
    }
}

#[test]
fn agent_tool_definition_drops_the_whole_walkthrough_when_neither_preferred() {
    let agent = agent_tool_definition(false, false);
    let desc = agent["description"].as_str().unwrap();
    assert!(
        !desc.contains("Orchestration in"),
        "no orchestration walkthrough survives when nothing is TUIC-preferred"
    );
    // detect/stats/metrics moved to HTTP (`removed_native_action_replacement`),
    // so the header no longer advertises them.
    assert!(desc.starts_with("AI agent peer administration."));
    assert!(
        desc.contains("prefer those over `spawn`/`register`/`list_peers`/`send`/`inbox` here")
    );
    // Every action bullet — and the schema itself — is still fully intact.
    for action in &[
        "spawn",
        "wait",
        "register",
        "list_peers",
        "send",
        "inbox",
    ] {
        assert!(
            desc.contains(&format!("- {action}:")),
            "action bullet for '{action}' must survive even with both preferences off"
        );
    }
    let action_enum = agent["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &["spawn", "register", "list_peers", "send", "inbox"] {
        assert!(action_enum.contains(action));
    }
}

/// Regression, found live 2026-09-03: a live Claude Code session, even
/// with `spawn` recommended, reasoned that `agent action=spawn` was "a
/// different thing entirely — those are terminal sessions running an
/// agent binary, not Claude Code teammates" and passed it over. The
/// description never used the word "teammate" anywhere — only
/// "peer"/"worker" — so nothing told the model these are the same
/// concept, just a different backend. Fixed by explicitly saying so
/// wherever spawning is actually recommended.
#[test]
fn agent_tool_definition_calls_a_spawned_peer_a_teammate_wherever_spawn_is_recommended() {
    for (prefer_spawning, prefer_messaging) in [(true, true), (true, false)] {
        let agent = agent_tool_definition(prefer_spawning, prefer_messaging);
        let desc = agent["description"].as_str().unwrap();
        assert!(
            desc.contains("teammate"),
            "({prefer_spawning}, {prefer_messaging}): spawn is recommended here, so the \
            description must connect it to the word a host's own agent-teams feature uses \
            — got: {desc:?}"
        );
    }
    // Never claim the peer/teammate equivalence when spawning is NOT
    // being recommended — that would contradict "prefer your own native
    // teammate feature instead" in the same breath.
    for (prefer_spawning, prefer_messaging) in [(false, true), (false, false)] {
        let agent = agent_tool_definition(prefer_spawning, prefer_messaging);
        let desc = agent["description"].as_str().unwrap();
        assert!(
            !desc.contains("teammate"),
            "({prefer_spawning}, {prefer_messaging}): spawn is NOT recommended here — must \
            not simultaneously claim it's equivalent to the host's own teammate feature"
        );
    }
}

/// The connect-time `build_mcp_instructions` prose already said "use TUIC's `agent
/// action=spawn` MCP tool (not your host's native subagent/Task/team tool)" — but that's
/// shown once, at `initialize`, and never re-read. The `agent` tool's own schema
/// description (re-read every time the model considers calling it) never carried the same
/// explicit "use this instead of your own" directive. Added 2026-09-03 at the user's
/// request, directly targeting the exact reasoning trap the teammate-wording fix alone
/// didn't close.
#[test]
fn agent_tool_definition_says_to_use_it_instead_of_the_hosts_own_spawning_tool_when_recommended()
 {
    for (prefer_spawning, prefer_messaging) in [(true, true), (true, false)] {
        let agent = agent_tool_definition(prefer_spawning, prefer_messaging);
        let desc = agent["description"].as_str().unwrap();
        assert!(
            desc.contains("not your own built-in agent-spawning tool"),
            "({prefer_spawning}, {prefer_messaging}): spawn is recommended here — the \
            description must explicitly say to use this instead of the host's own tool, \
            not just that they're equivalent concepts — got: {desc:?}"
        );
    }
    // Never tell the model to prefer TUIC's spawn over its own tool when spawning is NOT
    // being recommended — that's the opposite of what the surrounding text says.
    for (prefer_spawning, prefer_messaging) in [(false, true), (false, false)] {
        let agent = agent_tool_definition(prefer_spawning, prefer_messaging);
        let desc = agent["description"].as_str().unwrap();
        assert!(
            !desc.contains("not your own built-in agent-spawning tool"),
            "({prefer_spawning}, {prefer_messaging}): spawn is NOT recommended here — must \
            not simultaneously tell the model to prefer it over its own tool"
        );
    }
}

#[test]
fn merged_tool_definitions_agent_description_reflects_the_calling_sessions_prefer_flags() {
    // End-to-end regression test for the live bug: a connection whose
    // resolved agent type has both preferences off must see a
    // non-recommending `agent` tool description from `tools/list` — not
    // just from `build_mcp_instructions`'s prose.
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_spawning: Some(false),
            prefer_tuic_messaging: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    let state = test_state();
    let mcp_sid = "mcp-agent-tool-gating-test";
    state.mcp.sessions.insert(
        mcp_sid.to_string(),
        crate::state::McpSessionMeta {
            last_activity: std::time::Instant::now(),
            is_claude_code: true,
            requires_meta_tools: false,
            has_sse_stream: false,
            sse_generation: 0,
            repo_path: None,
            agent_type: Some("claude".to_string()),
            prompt_instructions: None,
        },
    );

    let tools = merged_tool_definitions(&state, Some(mcp_sid), None);
    let agent = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let desc = agent["description"].as_str().unwrap();
    assert!(
        !desc.contains("Spawn a named peer: spawn name=worker"),
        "tools/list must not recommend spawn for a connection with prefer_tuic_spawning=false"
    );
    assert!(
        !desc.contains("Talk to it: send to=<peer>"),
        "tools/list must not recommend messaging for a connection with prefer_tuic_messaging=false"
    );
}

#[test]
fn merged_tool_definitions_without_session_context_stays_fully_permissive() {
    // No mcp_session_id (or an unknown one) must fall back to the
    // pre-existing, fully-permissive description — never fail closed.
    let state = test_state();
    let tools = merged_tool_definitions(&state, None, None);
    let agent = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let desc = agent["description"].as_str().unwrap();
    assert!(desc.contains("Spawn a named peer: spawn name=worker"));
    assert!(desc.contains("Talk to it: send to=<peer>"));
}

#[test]
fn session_tool_description_includes_wait() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let action_desc = session["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(action_desc.contains("wait"), "session must advertise wait");
    assert!(
        session["inputSchema"]["properties"]["until"].is_object(),
        "session wait needs an 'until' param"
    );
}

#[test]
fn repo_tool_lists_retained_worktree_actions_and_branch_parameter() {
    let defs = native_tool_definitions(true, true);
    let repo = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "repo")
        .unwrap();
    let action_desc = repo["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &[
        "list",
        "active",
        "status",
        "worktree_list",
        "worktree_create",
        "worktree_remove",
        "worktree_setup_status",
    ] {
        assert!(
            action_desc.contains(action),
            "repo action description must include '{action}'"
        );
    }
    let params = &repo["inputSchema"]["properties"];
    assert!(params.get("issue_number").is_none());
    assert!(params.get("filter").is_none());
    assert!(
        params["branch"]["description"]
            .as_str()
            .unwrap()
            .contains("worktree_remove"),
        "branch's description must name worktree_remove as a consumer: {params}"
    );
}

#[test]
fn repo_progress_schema_advertises_paging_and_filters() {
    let defs = native_tool_definitions(true, true);
    let repo = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "repo")
        .unwrap();
    let input = &repo["inputSchema"]["properties"]["input"];
    assert_eq!(input["additionalProperties"], false);
    for field in ["blockedOnly", "ptyId", "limit", "cursor"] {
        assert!(
            input["properties"][field].is_object(),
            "missing {field} in repo progress input"
        );
    }
    assert!(input["description"].as_str().unwrap().contains("8 entries"));
}

#[test]
fn ui_tool_includes_notify_actions() {
    let defs = native_tool_definitions(true, true);
    let ui = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ui")
        .unwrap();
    let action_desc = ui["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &["tab", "toast", "confirm"] {
        assert!(
            action_desc.contains(action),
            "ui action description must include '{action}'"
        );
    }
}

#[test]
fn debug_tool_includes_sessions_action() {
    let defs = native_tool_definitions(true, true);
    let debug = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "debug")
        .unwrap();
    let action_desc = debug["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(
        action_desc.contains("sessions"),
        "debug action description must include 'sessions'"
    );
}

#[test]
fn merged_tools_collapse_false_returns_all_native_tools() {
    let state = test_state();
    assert!(!state.config.read().collapse_tools);

    let merged = merged_tool_definitions(&state, None, None);
    let names = tool_names(&merged);

    let native = tool_names(&native_tool_definitions(true, true));
    assert_eq!(
        names, native,
        "collapse_tools=false should return all native tools"
    );
    assert!(
        names.len() > 3,
        "baseline native tool set must exceed 3 tools"
    );
}

#[test]
fn merged_tools_collapse_true_keeps_progress_directly_available() {
    let state = test_state();
    state.config.write().collapse_tools = true;

    let merged = merged_tool_definitions(&state, None, None);
    let names = tool_names(&merged);

    assert_eq!(names.len(), 4);
    assert_eq!(
        names,
        vec!["search_tools", "get_tool_schema", "call_tool", "progress"]
    );
}

#[test]
fn grok_session_uses_meta_tools_without_mutating_global_config() {
    let state = test_state();
    assert!(!state.config.read().collapse_tools);
    state.mcp.sessions.insert(
        "grok-session".to_string(),
        crate::state::McpSessionMeta {
            prompt_instructions: None,
            last_activity: std::time::Instant::now(),
            is_claude_code: false,
            requires_meta_tools: true,
            has_sse_stream: false,
            sse_generation: 0,
            repo_path: None,
            agent_type: None,
        },
    );

    let merged = merged_tool_definitions(&state, Some("grok-session"), None);
    assert_eq!(
        tool_names(&merged),
        vec!["search_tools", "get_tool_schema", "call_tool", "progress"]
    );
    assert!(!state.config.read().collapse_tools);
}

/// Sanity check on the token-reduction claim for lazy tool loading.
/// Measured on the native-only test state (no upstreams registered):
/// baseline ≈ 11 KiB, collapsed ≈ 1.7 KiB — roughly 6.7× reduction.
/// In production with typical upstreams (100+ tools) the baseline is
/// ~35 KiB, pushing the real reduction toward ~20×. Thresholds here
/// are regression guards, not targets, so they use the conservative
/// native-only numbers.
#[test]
fn collapse_tools_payload_size_meets_reduction_target() {
    let state = test_state();

    let baseline = serde_json::to_vec(&merged_tool_definitions(&state, None, None))
        .expect("serialize baseline")
        .len();

    state.config.write().collapse_tools = true;
    let collapsed = serde_json::to_vec(&merged_tool_definitions(&state, None, None))
        .expect("serialize collapsed")
        .len();

    assert!(
        collapsed < 4096,
        "collapsed tools/list must stay under 4 KiB, got {collapsed} bytes"
    );
    assert!(
        baseline >= collapsed * 5,
        "expected >=5x reduction on native-only state, baseline={baseline} collapsed={collapsed}"
    );
}

#[test]
fn merged_tools_collapse_true_hides_disabled_progress() {
    let state = test_state();
    state.config.write().collapse_tools = true;
    state.config.write().disabled_native_tools = vec!["progress".to_string()];

    let merged = merged_tool_definitions(&state, None, None);
    assert_eq!(tool_names(&merged).len(), 3);
    assert_eq!(
        tool_names(&merged),
        vec!["search_tools", "get_tool_schema", "call_tool"]
    );
}

/// The obligation and the tool are one switch, not two. A listed tool with
/// no stated duty is the shipped defect (39 repositories, zero entries); a
/// duty naming a tool the client cannot see is worse, because the agent
/// keeps trying. Both directions are asserted here so neither can drift.
#[test]
fn the_progress_obligation_and_the_progress_tool_appear_and_vanish_together() {
    let ctx = InstructionContext {
        repos: Vec::new(),
        sessions: Vec::new(),
        peer_count: 0,
    };
    let markers = (true, true);

    let on = render_mcp_instructions(None, false, markers, true, &ctx, &default_tool_gates());
    let off = render_mcp_instructions(None, false, markers, false, &ctx, &default_tool_gates());
    assert!(on.contains("## Progress — mandatory"));
    assert!(on.contains("Call `progress`"));
    // The word itself still occurs — the `intent:` marker is described as
    // "the work currently in progress" — so the assertion is on the
    // obligation and on naming the tool, not on the substring.
    assert!(!off.contains("## Progress — mandatory"));
    assert!(
        !off.contains("`progress`"),
        "an obligation must never name a tool the client was not listed"
    );

    let state = test_state();
    let listed = |state: &Arc<AppState>| {
        tool_names(&serde_json::Value::Array(filtered_native_tools(state, true, true)))
            .contains(&"progress".to_string())
    };

    state.config.write().progress_tracking = true;
    assert!(listed(&state));

    state.config.write().progress_tracking = false;
    assert!(!listed(&state));
}

// search_tools

#[test]
fn search_tools_requires_query() {
    let state = test_state();
    let r = handle_search_tools(&state, &serde_json::json!({}));
    assert!(r["error"].as_str().unwrap().contains("query"));

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "" }));
    assert!(r["error"].as_str().unwrap().contains("query"));

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "   " }));
    assert!(r["error"].as_str().unwrap().contains("query"));
}

#[test]
fn search_tools_returns_ranked_results_for_session_query() {
    let state = test_state();
    // Query targets the PTY multiplexer specifically, so the ranking is
    // asserted against a phrase only `session` describes.
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "PTY multiplexer tmux pane lifecycle" }),
    );
    let results = r["results"].as_array().unwrap();
    assert!(!results.is_empty(), "expected non-empty results");
    assert_eq!(results[0]["name"], "session");
    // summary is the first sentence of the description — must be populated.
    assert!(
        results[0]["summary"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    );
}

#[test]
fn search_tools_returns_ranked_results_for_github_query() {
    let state = test_state();
    let r = handle_search_tools(&state, &serde_json::json!({ "query": "github PR status" }));
    let results = r["results"].as_array().unwrap();
    assert_eq!(results[0]["name"], "repo");
}

#[test]
fn search_tools_excludes_disabled_native_tools() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    rebuild_tool_search_index(&state);

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "terminal session" }));
    let results = r["results"].as_array().unwrap();
    // "session" must not appear at all.
    let has_session = results.iter().any(|v| v["name"] == "session");
    assert!(
        !has_session,
        "disabled 'session' tool must be absent from search results"
    );
}

#[test]
fn search_tools_nonsense_query_returns_empty() {
    let state = test_state();
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "xyzzyplugh nonsense qqq" }),
    );
    let results = r["results"].as_array().unwrap();
    assert_eq!(results.len(), 0);
    assert_eq!(r["count"], 0);
}

#[test]
fn search_tools_respects_limit() {
    let state = test_state();
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "action", "limit": 2 }),
    );
    let results = r["results"].as_array().unwrap();
    assert!(results.len() <= 2);
}

// get_tool_schema

#[test]
fn get_tool_schema_requires_tool_name() {
    let state = test_state();
    let r = handle_get_tool_schema(&state, &serde_json::json!({}));
    assert!(r["error"].as_str().unwrap().contains("tool_name"));
}

#[test]
fn get_tool_schema_returns_full_definition_for_native_tool() {
    let state = test_state();
    let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "session" }));
    assert_eq!(r["name"], "session");
    assert!(r["description"].as_str().is_some());
    assert!(r["inputSchema"].is_object());
    assert_eq!(r["inputSchema"]["type"], "object");
}

#[test]
fn get_tool_schema_returns_error_for_unknown_tool() {
    let state = test_state();
    let r = handle_get_tool_schema(
        &state,
        &serde_json::json!({ "tool_name": "does_not_exist" }),
    );
    let err = r["error"].as_str().unwrap();
    assert!(err.contains("not found"));
    assert!(
        err.contains("search_tools"),
        "error should guide user to search_tools"
    );
}

#[test]
fn get_tool_schema_excludes_disabled_native_tools() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["debug".to_string()];
    rebuild_tool_search_index(&state);
    let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "debug" }));
    assert!(r["error"].as_str().is_some());
}

// call_tool

#[tokio::test]
async fn call_tool_requires_tool_name() {
    let state = test_state();
    let r = handle_call_tool(&state, loopback_addr(), &serde_json::json!({}), None, None).await;
    assert!(r["error"].as_str().unwrap().contains("tool_name"));
}

#[tokio::test]
async fn call_tool_blocks_meta_tool_recursion() {
    let state = test_state();
    for meta in META_TOOL_NAMES {
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": meta, "arguments": { "query": "x" } }),
            None,
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        assert!(
            err.contains("cannot invoke meta-tool"),
            "meta '{meta}' should be blocked: {err}"
        );
    }
}

#[tokio::test]
async fn call_tool_rejects_disabled_native_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["repo".to_string()];
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "repo", "arguments": { "action": "active" } }),
        None,
        None,
    )
    .await;
    assert!(r["error"].as_str().unwrap().contains("disabled"));
}

#[tokio::test]
async fn call_tool_returns_unknown_tool_error_for_bogus_name() {
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "nonsense_xyz", "arguments": {} }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    assert!(err.contains("Unknown tool"));
}

#[tokio::test]
async fn call_tool_propagates_addr_for_localhost_only_tools() {
    // config save is restricted to loopback addresses. call_tool must propagate
    // the caller's addr so the restriction still fires through the meta layer.
    let state = test_state();
    let r = handle_call_tool(
        &state,
        non_loopback_addr(),
        &serde_json::json!({
            "tool_name": "config",
            "arguments": { "action": "save", "config": {} }
        }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    assert!(
        err.contains("localhost"),
        "non-loopback config save must be rejected via addr propagation: {err}"
    );
}

#[tokio::test]
async fn call_tool_missing_arguments_defaults_to_empty_object() {
    // Omitting 'arguments' must not crash — handler receives {} and produces
    // its own missing-action error.
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "session" }),
        None,
        None,
    )
    .await;
    assert!(r["error"].as_str().unwrap().contains("action"));
}

#[tokio::test]
async fn call_tool_routes_unknown_upstream_prefixed_name_through_proxy() {
    // No upstreams are registered in tests — any tool_name with "__" falls
    // through to proxy_tool_call, which errors out. We just verify that the
    // error comes from the upstream path (not the native unknown-tool branch).
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "fake_upstream__some_tool", "arguments": {} }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    // proxy_tool_call returns an error string — just assert it's an error and
    // that the native unknown-tool message is NOT what we got.
    assert!(
        !err.contains("Unknown tool"),
        "upstream-prefixed name must not hit native fallthrough: {err}"
    );
}

// ---- build_mcp_instructions collapse mode (story 1081) -------------------

/// Classic mode keeps a `## Tools` section, but it holds cross-tool rules
/// rather than a catalogue — `tools/list` already carries every name and
/// action in the same turn. It must also stay free of the meta-tool names,
/// which are not callable in this mode.
#[test]
fn instructions_collapse_off_points_at_tool_descriptions() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(out.contains("## Tools\n"), "expected classic Tools section");
    assert!(
        out.contains("Read each tool's description for actions and rules."),
        "classic mode must delegate to the tool descriptions"
    );
    assert!(!out.contains("## Tools — Lazy Discovery"));
    assert!(!out.contains("search_tools"));
}

/// The ack rule is protocol and belongs to the instructions: no tool
/// description can state a rule about the *first assistant message*.
///
/// Retargeted for #754-affa: the swarm/lifecycle/reporting assertions that
/// used to sit here moved to `instructions_do_not_repeat_what_tool_descriptions_already_say`,
/// which asserts them on the `agent` description — the surface that reaches
/// clients ignoring `instructions` too.
#[test]
fn instructions_scope_ack_to_the_connection() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(out.contains("once per MCP connection or reconnect"));
    assert!(out.contains("Not repeated on later turns"));
    assert!(out.contains("requested by TUICommander, the local terminal application"));
    assert!(out.contains("Subagents (Task tool) and in-process teammates must NOT emit these"));
    assert!(!out.contains("Aliases \"swarm\""));
}

#[test]
fn instructions_collapse_on_describes_search_schema_call_flow() {
    let state = test_state();
    state.config.write().collapse_tools = true;
    let out = build_mcp_instructions(&state, None);

    // Slim section referencing meta-tools (detail lives in tool descriptions).
    assert!(out.contains("## Tools"), "expected tools header");
    assert!(out.contains("`search_tools`"), "must mention search_tools");
    assert!(
        out.contains("`get_tool_schema`"),
        "must mention get_tool_schema"
    );
    assert!(out.contains("`call_tool`"), "must mention call_tool");
    assert!(out.contains("worktree"), "must mention worktree caveat");
    // The concrete tools list and legacy workflow must NOT appear — those
    // reference tool names the model cannot invoke directly in collapse mode.
    assert!(
        !out.contains("- `session` ("),
        "tools list must be suppressed in collapse mode"
    );
    assert!(
        !out.contains("## Workflow"),
        "legacy workflow must be suppressed in collapse mode"
    );
}

#[test]
fn initialize_instructions_pin_the_one_call_submit_rule_in_both_modes() {
    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    let collapsed = build_mcp_instructions_for_mode(&state, None, true);

    assert!(classic.contains(
            "**Submit:** `session action=submit session_id=<id> input=<text>` once; never split text/Enter; never poll."
        ));
    assert!(collapsed.contains(
            "**Submit:** `call_tool tool_name=session arguments={action:submit,session_id,input}` once; never split text/Enter; never poll."
        ));
    assert!(!classic.contains("text then"));
    assert!(!collapsed.contains("status after"));
}

// ---- build_mcp_instructions per-tool gating (disabled_native_tools) ------

#[test]
fn instructions_omit_disabled_session_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    let out = build_mcp_instructions(&state, None);

    assert!(
        !out.contains("- `session` ("),
        "session bullet must be omitted from Tools"
    );
    assert!(
        !out.contains("`session action=create` (shell)"),
        "session clause must be omitted from Workflow Spawn bullet"
    );
    assert!(
        !out.contains("`session action=status|output`"),
        "session clause must be omitted from Workflow Observe bullet"
    );
    // Other tools stay advertised.
    assert!(out.contains("- `agent` (AI peers"));
    assert!(out.contains("## Multi-Agent Work"));
}

#[test]
fn instructions_omit_disabled_agent_tool_and_multi_agent_section() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["agent".to_string()];
    let out = build_mcp_instructions(&state, None);

    assert!(
        !out.contains("- `agent` ("),
        "agent bullet must be omitted from Tools"
    );
    assert!(
        !out.contains("## Multi-Agent Work"),
        "Multi-Agent Work must be omitted entirely when agent tool is disabled"
    );
    assert!(
        !out.contains("- **Coordinate:**"),
        "agent-only Coordinate bullet must be omitted from Workflow"
    );
    // session/repo spawn guidance stays.
    assert!(out.contains("`session action=create` (shell)"));
    assert!(out.contains("`repo action=worktree_create` (isolated)"));
}

#[test]
fn instructions_omit_disabled_repo_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["repo".to_string()];
    let out = build_mcp_instructions(&state, None);

    assert!(
        !out.contains("- `repo` ("),
        "repo bullet must be omitted from Tools"
    );
    assert!(
        !out.contains("**Worktrees:**"),
        "Worktrees tip must be omitted when repo tool is disabled"
    );
    assert!(
        !out.contains("- **Isolated branches:**"),
        "repo-only Isolated branches bullet must be omitted"
    );
    assert!(
        !out.contains("Single isolated task (CC only)"),
        "repo-only CC worktree bullet must be omitted"
    );
}

#[test]
fn instructions_omit_disabled_ui_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["ui".to_string()];
    let out = build_mcp_instructions(&state, None);

    assert!(!out.contains("- `ui` ("), "ui bullet must be omitted");
    assert!(
        !out.contains("**UI feedback:**"),
        "UI feedback tip must be omitted when ui tool is disabled"
    );
}

#[test]
fn instructions_omit_disabled_plugin_dev_guide_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["plugin_dev_guide".to_string()];
    let out = build_mcp_instructions(&state, None);

    assert!(
        !out.contains("plugin_dev_guide`: plugin authoring reference"),
        "plugin_dev_guide bullet must be omitted"
    );
}

#[test]
fn instructions_workflow_header_omitted_when_session_agent_repo_all_disabled() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec![
        "session".to_string(),
        "agent".to_string(),
        "repo".to_string(),
    ];
    let out = build_mcp_instructions(&state, None);

    assert!(
        !out.contains("## Workflow"),
        "Workflow header must not render with zero possible bullets"
    );
    // ui/plugin_dev_guide are unaffected and still listed.
    assert!(out.contains("- `ui` ("));
    assert!(out.contains("plugin_dev_guide`: plugin authoring reference"));
}

#[test]
fn instructions_peer_count_included_when_peers_registered() {
    let state = test_state();
    register_peer(&state, TEST_UUID_A, "worker-1", "mcp-1");
    let out = build_mcp_instructions(&state, None);

    assert!(
        out.contains("**1** peer agent(s) connected"),
        "expected peer count sentence, got: {out}"
    );
}

#[test]
#[serial_test::serial]
fn instructions_single_isolated_task_bullet_only_for_claude_code_client() {
    let state = test_state();

    let generic = build_mcp_instructions(&state, Some("some-other-agent"));
    assert!(
        !generic.contains("Single isolated task (CC only)"),
        "CC-only bullet must not appear for non-Claude-Code clients"
    );

    let claude = build_mcp_instructions(&state, Some("claude-code"));
    assert!(
        claude.contains("Single isolated task (CC only)"),
        "CC-only bullet must appear for Claude Code clients"
    );
}

// ---- build_mcp_instructions prefer_tuic_messaging gating ------------------

#[test]
#[serial_test::serial]
fn resolve_prefer_tuic_messaging_defaults_true_with_no_override() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    assert!(resolve_prefer_tuic_messaging(Some("claude-code")));
    assert!(resolve_prefer_tuic_messaging(None));
}

#[test]
#[serial_test::serial]
fn resolve_prefer_tuic_messaging_respects_per_agent_override() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_messaging: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    assert!(!resolve_prefer_tuic_messaging(Some("claude-code")));
    // A different, unconfigured agent type is unaffected.
    assert!(resolve_prefer_tuic_messaging(Some("codex")));
}

#[test]
#[serial_test::serial]
fn instructions_messaging_disabled_keeps_spawn_but_omits_messaging_guidance() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_messaging: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    let state = test_state();
    let out = build_mcp_instructions(&state, Some("claude-code"));

    // Spawn guidance stays.
    assert!(out.contains("`agent action=spawn` (AI)"));
    assert!(
        out.contains("- **Same repo:** TUIC `agent action=spawn` peers to work in this repo.")
    );
    assert!(out.contains("- **Isolated branches:**"));

    // Messaging guidance is gone.
    assert!(
        out.contains("- `agent` (AI peers): spawn, detect, stats, metrics"),
        "expected trimmed agent bullet without messaging verbs, got: {out}"
    );
    assert!(!out.contains("agent action=register"));
    assert!(!out.contains("- **Coordinate:**"));
    assert!(!out.contains("`agent action=inbox`"));
    assert!(!out.contains("- **Identity:**"));
}

// ---- build_mcp_instructions prefer_tuic_spawning gating (independent of
// prefer_tuic_messaging — an agent type can prefer either, both, or
// neither, in any combination) ---------------------------------------

#[test]
#[serial_test::serial]
fn resolve_prefer_tuic_spawning_defaults_true_with_no_override() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    assert!(resolve_prefer_tuic_spawning(Some("claude-code")));
    assert!(resolve_prefer_tuic_spawning(None));
}

#[test]
#[serial_test::serial]
fn resolve_prefer_tuic_spawning_respects_per_agent_override_independent_of_messaging() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_spawning: Some(false),
            prefer_tuic_messaging: Some(true),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    assert!(!resolve_prefer_tuic_spawning(Some("claude-code")));
    assert!(resolve_prefer_tuic_messaging(Some("claude-code")));
}

#[test]
#[serial_test::serial]
fn instructions_spawning_disabled_keeps_messaging_but_omits_spawn_guidance() {
    // The mirror image of the messaging-disabled test above: spawn off,
    // messaging on. Neither preference should ever imply the other.
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_spawning: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    let state = test_state();
    let out = build_mcp_instructions(&state, Some("claude-code"));

    // Messaging guidance stays.
    assert!(out.contains("- `agent` (AI peers + messaging): wait, detect, stats, metrics, register, list_peers, send, inbox"));
    assert!(out.contains("- **Identity:**"));
    assert!(out.contains("`agent action=inbox`"));
    assert!(out.contains("- **Coordinate:**"));
    assert!(out.contains(
        "- **Same repo:** wait with `agent action=wait`, then read `agent action=inbox`. Lifecycle notifications carry state only; workers must report results with `agent action=send`.\n"
    ));

    // Spawn guidance is gone.
    assert!(!out.contains("`agent action=spawn` (AI)"));
    assert!(!out.contains("- **Isolated branches:**"));
    assert!(!out.contains("Prefer TUICommander for peers/teams"));
    assert!(!out.contains("Single isolated task (CC only)"));
    // The anomaly-fallback clause is spawn-specific ("a child failed to
    // send its result" presumes TUIC did the spawning) — must not leak
    // into the messaging-only Same-repo bullet.
    assert!(!out.contains("anomaly fallback"));
}

/// Regression coverage for the 2026-09-04 wording change (see
/// plans/docs/agent-teams-wording-investigation.md, main checkout): an A/B
/// harness found the old hedged phrasing ("use TUIC's `agent
/// action=spawn` MCP tool (not your host's native subagent/Task/team
/// tool) whenever spawning an AI peer that should be observable...")
/// never worked against a real Opus session (0/10), while a direct,
/// unhedged directive worked every time (10/10). Asserts the new
/// directive text is present and the old hedged phrasing is gone, so a
/// future edit can't silently revert to the measured-ineffective
/// wording.
#[test]
fn instructions_prefer_tuicommander_bullet_uses_the_unhedged_directive() {
    let state = test_state();
    let out = build_mcp_instructions(&state, Some("claude-code"));

    assert!(
        out.contains(
            "every agent-teams teammate is created with TUIC's `agent action=spawn` MCP tool — do not use your own built-in agent-spawning tool for this"
        ),
        "expected the unhedged directive wording, got: {out}"
    );
    // The label must survive unchanged — other tests grep for it.
    assert!(out.contains("- **Prefer TUICommander for peers/teams:**"));
    // The old hedged phrasing must not reappear.
    assert!(!out.contains("whenever spawning an AI peer that should be observable"));
}

#[test]
#[serial_test::serial]
fn instructions_omit_multi_agent_work_when_neither_spawning_nor_messaging_preferred() {
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_spawning: Some(false),
            prefer_tuic_messaging: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    let state = test_state();
    let out = build_mcp_instructions(&state, Some("claude-code"));

    assert!(
        !out.contains("## Multi-Agent Work"),
        "section must be omitted entirely when there's nothing TUIC-preferred left to say"
    );
    // The agent tool is still enabled and still listed, just with neither
    // spawn nor messaging verbs — pure peer-admin only.
    assert!(out.contains("- `agent` (AI peers): detect, stats, metrics"));
    // Workflow still renders (session/repo/agent-detect content survives).
    assert!(!out.contains("- **Coordinate:**"));
    assert!(!out.contains("`agent action=spawn` (AI)"));
}

#[test]
#[serial_test::serial]
fn instructions_disabled_agent_tool_wins_over_an_explicit_prefer_true_override() {
    // `spawn_preferred`/`messaging_preferred` are `agent_enabled && resolve_prefer_tuic_*(...)`
    // — a hard AND, not an OR. An explicit `Some(true)` override must NOT resurrect
    // Multi-Agent Work guidance once the `agent` tool itself is disabled; there's
    // nothing to prefer when the tool can't be reached at all.
    let dir = TempDir::new().unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

    let mut agents_cfg = crate::config::AgentsConfig::default();
    agents_cfg.agents.insert(
        "claude".to_string(),
        crate::config::AgentSettings {
            prefer_tuic_spawning: Some(true),
            prefer_tuic_messaging: Some(true),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents_cfg).unwrap();

    let state = test_state();
    state.config.write().disabled_native_tools = vec!["agent".to_string()];
    let out = build_mcp_instructions(&state, Some("claude-code"));

    assert!(
        !out.contains("## Multi-Agent Work"),
        "an explicit prefer_tuic_*=true override must not override agent_enabled=false"
    );
    assert!(
        !out.contains("- `agent` ("),
        "agent bullet must still be omitted from Tools"
    );
    assert!(!out.contains("- **Coordinate:**"));
    // session/repo spawn guidance is unaffected by the agent-only override.
    assert!(out.contains("`session action=create` (shell)"));
    assert!(out.contains("`repo action=worktree_create` (isolated)"));
}

/// There are two instruction surfaces and they are not equals. Every client
/// receives the tool descriptions; a client such as Codex never surfaces
/// initialize `instructions`. So a rule that a tool description already
/// carries must not be repeated in the instructions — the client that reads
/// both pays for it twice, and the client that reads one must not be the one
/// left without it.
#[test]
fn instructions_do_not_repeat_what_tool_descriptions_already_say() {
    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    let defs = native_tool_definitions(true, true);

    // 1. The tool catalogue is `tools/list` restated in prose.
    for bullet in [
        "- `session` (",
        "- `agent` (",
        "- `task` (",
        "- `repo` (",
        "- `ui` (",
        "- `plugin_dev_guide`:",
    ] {
        assert!(
            !classic.contains(bullet),
            "instructions must not restate the tool catalogue: {bullet}"
        );
    }

    // 2. The Workflow section restated the `agent` description's primer …
    assert!(!classic.contains("## Workflow"));
    let agent = tool_description(&defs, "agent");
    assert!(agent.contains("There is no separate swarm action"));
    assert!(agent.contains("Lifecycle notifications carry state only"));
    assert!(agent.contains("Every worker must report task output or blockers with send"));

    // 3. … and the UI-feedback line restated the `ui` description's Use: block.
    assert!(!classic.contains("**UI feedback:**"));
    let ui = tool_description(&defs, "ui");
    assert!(ui.contains("confirm BEFORE destructive ops"));
    assert!(ui.contains(">20-line structured output"));
    assert!(ui.contains("screenshot to visually verify"));
}

/// The orchestrator role, its wake capability and the identity rules used to
/// be instructions-only, which left them invisible to exactly the clients
/// that most need them (a headerless caller reading no instructions cannot
/// learn that `register` is how it gets an identity at all).
#[test]
fn agent_description_owns_the_orchestrator_role_and_wake_contract() {
    let defs = native_tool_definitions(true, true);
    let agent = tool_description(&defs, "agent");
    assert!(
        agent.contains("orchestrator=true"),
        "agent description must say how the role is declared"
    );
    assert!(
        agent.contains("spawning a child never infers it"),
        "agent description must deny role inference from spawn"
    );
    assert!(
        agent.contains("mail_wake=managed_pty_lifecycle"),
        "agent description must name the wake capability field"
    );
    assert!(
        agent.contains("headerless external caller calls register without tuic_session"),
        "agent description must keep the identity rule"
    );

    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    assert!(
        !classic.contains("**Orchestrator role:**"),
        "the orchestrator role moved to the agent description; it must not be repeated"
    );
    assert!(
        !classic.contains("- **Identity:**"),
        "the identity rule moved to the agent description; it must not be repeated"
    );
}

#[test]
fn agent_send_schema_and_description_explain_urgent_mail_without_exposing_payload() {
    let definitions = native_tool_definitions(true, true);
    let agent = definitions
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["name"] == "agent")
        .unwrap();
    assert_eq!(
        agent["inputSchema"]["properties"]["urgency"]["enum"],
        serde_json::json!(["normal", "urgent"])
    );
    let description = agent["description"].as_str().unwrap();
    assert!(description.contains("Normal is the default"));
    assert!(description.contains("recipient must change course before its next step"));
    assert!(description.contains("payload is never typed into the recipient's composer"));
    assert!(description.contains("urgent_delivered"));
    assert!(description.contains("urgent_fallback_reason"));
}

/// A recorded project outcome and a transient notification are different
/// things. `progress` persists and toasts; `ui action=toast` only toasts.
/// The `ui` description is where a caller reaching for a toast finds out.
#[test]
fn ui_description_routes_semantic_outcomes_to_the_progress_tool() {
    let defs = native_tool_definitions(true, true);
    let ui = tool_description(&defs, "ui");
    assert!(
        ui.contains("`progress` tool"),
        "ui description must name the progress tool"
    );
    assert!(
        ui.contains("A project outcome"),
        "ui description must say which outcomes belong to progress"
    );
    assert!(
        ui.contains("keeps no record of it"),
        "ui description must say what a toast does not do"
    );
}

/// AC3: discovery must not be demanded for a definition the caller already
/// holds, and must not be skipped for a name it has never seen.
#[test]
fn meta_tools_do_not_demand_a_schema_the_caller_already_has() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);

    let call = tool_description(&defs, "call_tool");
    assert!(
        call.contains("call it straight away with the schema you already have"),
        "call_tool must allow reusing a known schema"
    );
    assert!(
        call.contains("notifications/tools/list_changed"),
        "call_tool must name the one event that invalidates a known schema"
    );
    assert!(
        call.contains("Never skip discovery for a name you have NOT seen"),
        "call_tool must still forbid inventing tool names"
    );
    assert!(
        !call.contains("fetch it via `get_tool_schema` first"),
        "the unconditional pre-fetch instruction must be gone"
    );

    let search = tool_description(&defs, "search_tools");
    assert!(
        search.contains("without searching for it again"),
        "search_tools must exempt a tool the caller already knows"
    );
    assert!(
        !search.contains("before calling any tool"),
        "search_tools must not demand a search before every call"
    );
}

/// AC1: reproducible size measurement of every instruction surface an MCP
/// client receives.
///
/// Reproducible because nothing here reads the host: the instructions
/// render from an explicit [`InstructionContext`] instead of the user's
/// `repositories.json` and live PTY list, and the spawn response renders
/// from fixed ids instead of a launched process.
///
/// The upstream dimension measures TUIC's own contribution only — injected
/// upstream tools carry stub descriptions, and a real upstream's payload is
/// its own bytes, not ours. What the numbers show is that those bytes leave
/// `tools/list` entirely under collapse.
///
/// Set `TUIC_MCP_SURFACE_DUMP=<path>` to write every measured string out for
/// out-of-band tokenizer counts (see docs/backend/mcp-http.md).
#[test]
fn mcp_instruction_surface_bytes_stay_within_budget() {
    let state = test_state();

    let empty = InstructionContext {
        repos: Vec::new(),
        sessions: Vec::new(),
        peer_count: 0,
    };
    let loaded = InstructionContext {
        repos: (0..8)
            .map(|i| (format!("repo-{i}"), format!("/Users/dev/src/repo-{i}")))
            .collect(),
        sessions: (0..12)
            .map(|i| {
                (
                    format!("sess{i:04}"),
                    format!("/Users/dev/src/repo-{i}"),
                    format!("feature/branch-{i}"),
                )
            })
            .collect(),
        peer_count: 6,
    };

    let mut dump = serde_json::Map::new();
    let mut record = |key: &str, text: String| {
        let bytes = text.len();
        dump.insert(key.to_string(), serde_json::json!(text));
        bytes
    };

    let markers = (true, true);
    let instructions_classic_empty = record(
        "instructions.classic.empty",
        render_mcp_instructions(None, false, markers, true, &empty, &default_tool_gates()),
    );
    let instructions_classic_loaded = record(
        "instructions.classic.loaded",
        render_mcp_instructions(None, false, markers, true, &loaded, &default_tool_gates()),
    );
    let instructions_collapsed_empty = record(
        "instructions.collapsed.empty",
        render_mcp_instructions(None, true, markers, true, &empty, &default_tool_gates()),
    );
    record(
        "instructions.collapsed.loaded",
        render_mcp_instructions(None, true, markers, true, &loaded, &default_tool_gates()),
    );

    // Discovered schemas: what `get_tool_schema` hands back, per tool.
    for tool in native_tool_definitions(true, true).as_array().unwrap() {
        let name = tool["name"].as_str().unwrap().to_string();
        record(
            &format!("schema.{name}"),
            serde_json::to_string(tool).unwrap(),
        );
    }

    // register response — the other place the workflow prose lives.
    let register = handle_messaging(
        &state,
        &serde_json::json!({"action": "register", "name": "surface"}),
        Some("surface-register"),
    );
    let register_bytes = record(
        "response.register",
        serde_json::to_string(&register).unwrap(),
    );

    // spawn response — no static prose block, so it stays small by design.
    let spawn_bytes = record(
        "response.spawn",
        serde_json::to_string(&spawn_response(
            "11111111-2222-3333-4444-555555555555",
            "66666666-7777-8888-9999-000000000000",
            "worker",
            1_700_000_000_000,
            Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
            None,
            false,
        ))
        .unwrap(),
    );

    // tools/list, with and without upstreams, classic and collapsed.
    let tools_classic = record(
        "tools.classic.no_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, false)).unwrap(),
    );
    let tools_collapsed = record(
        "tools.collapsed.no_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, true)).unwrap(),
    );
    state
        .mcp
        .upstream_registry
        .inject_ready_upstream("alpha", &["one", "two", "three"]);
    state
        .mcp
        .upstream_registry
        .inject_ready_upstream("beta", &["four", "five"]);
    let tools_classic_upstreams = record(
        "tools.classic.with_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, false)).unwrap(),
    );
    let tools_collapsed_upstreams = record(
        "tools.collapsed.with_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, true)).unwrap(),
    );

    if let Ok(path) = std::env::var("TUIC_MCP_SURFACE_DUMP") {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::Value::Object(dump.clone())).unwrap(),
        )
        .expect("write surface dump");
    }

    let measured: Vec<String> = dump
        .iter()
        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap().len()))
        .collect();
    let measured = measured.join(" ");

    // Budgets are regression guards on the surfaces this story shrank, not
    // targets. Each is the measured value rounded up to the next 64 bytes.
    //
    // The collapsed budget moved 1600 -> 1664 when Progress gained its
    // reporting obligation. That section is the point of the feature — the
    // shipped build had only a tool description, and 39 repositories
    // recorded zero entries — and it is paid for elsewhere: `repo` lost
    // eight progress actions in the same change. Raising a budget for prose
    // that carries no obligation is not the same trade.
    assert!(
        instructions_classic_empty <= 1600,
        "classic instructions grew past their budget — {measured}"
    );
    assert!(
        instructions_collapsed_empty <= 1664,
        "collapsed instructions grew past their budget — {measured}"
    );
    assert!(
        instructions_classic_loaded - instructions_classic_empty <= 1600,
        "the dynamic repo/session block grew past its budget — {measured}"
    );
    assert!(
        tools_collapsed < tools_classic / 5,
        "collapse must stay a >=5x reduction — {measured}"
    );
    assert!(
        tools_collapsed_upstreams < tools_classic_upstreams / 5,
        "collapse must stay a >=5x reduction with upstreams — {measured}"
    );
    assert!(
        spawn_bytes <= 832,
        "spawn response must stay free of static prose — {measured}"
    );
    assert!(
        register_bytes <= 4608,
        "register response grew past its budget — {measured}"
    );
}

// ---- Swarm Layer 4: MCP tool descriptions (#1165-b124) -------------------

#[test]
fn session_description_includes_status_action() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let desc = session["description"].as_str().unwrap();
    assert!(
        desc.contains("status:"),
        "session description must document the status action"
    );
    let action_enum = session["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(
        action_enum.contains("status"),
        "session action enum must include status"
    );
}

#[test]
fn session_submit_schema_pins_receipt_and_raw_input_semantics() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    let properties = &session["inputSchema"]["properties"];

    assert!(description.contains("Use one call; never split text and Enter; never poll after it."));
    assert!(description.contains(
            "Acknowledgement means child terminal movement after Enter, not semantic application acceptance."
        ));
    assert!(
        description.contains("composer_state (tracked InputLineBuffer, not application state)")
    );
    assert!(description.contains(
            "Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing."
        ));
    assert!(description.contains(
            "input: Raw text/key compatibility surface. Send text and/or special_key; ok confirms PTY write only."
        ));
    assert_eq!(
        properties["input"]["description"],
        "Non-empty command (action=submit) or raw text (action=input)"
    );
    assert_eq!(
        properties["timeout_ms"]["description"],
        "action=submit: acknowledgement wait, clamped 250-10000ms, default 3000. action=wait: max wait, default 60000; values at or above 300000 run as 295000."
    );
}

#[test]
fn session_keep_open_schema_explains_the_managed_child_toggle() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    let keep_open = description
        .lines()
        .find(|line| line.starts_with("- keep_open:"))
        .expect("session tool must document keep_open for MCP clients");
    assert!(keep_open.contains("managed child"));
    assert!(keep_open.contains("enabled=true"));
    assert!(keep_open.contains("enabled=false"));
    assert!(
        session["inputSchema"]["properties"]["session_id"]["description"]
            .as_str()
            .unwrap()
            .contains("keep_open"),
        "keep_open must list session_id as required"
    );
}

#[test]
fn session_description_requires_list_for_global_overview() {
    let defs = native_tool_definitions(true, true);
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    assert!(description.contains("All active sessions and states in one call"));
    assert!(description.contains("never fan out per-session status calls"));
}

#[test]
fn print_mode_description_clarifies_visible_vs_headless() {
    let defs = native_tool_definitions(true, true);
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let pm_desc = agent["inputSchema"]["properties"]["print_mode"]["description"]
        .as_str()
        .unwrap();
    assert!(
        pm_desc.contains("visible") || pm_desc.contains("TUI tab"),
        "print_mode must mention visible TUI tab"
    );
    assert!(
        pm_desc.contains("headless"),
        "print_mode must mention headless mode"
    );
}

/// Monitoring guidance is delegated, not dropped.
///
/// Retargeted for #754-affa. The original assertion was
/// `instructions.contains("status")`, which the removed tool catalogue
/// happened to satisfy — a substring that loose cannot tell "documented"
/// from "coincidence". The invariant worth keeping is that an orchestrator
/// reaching for the observation path still finds it, on the surface that
/// every client receives.
#[test]
fn monitoring_guidance_lives_in_the_tool_descriptions() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(
        out.contains("Read each tool's description for actions and rules."),
        "instructions must send the reader to the descriptions"
    );

    let defs = native_tool_definitions(true, true);
    let session = tool_description(&defs, "session");
    let agent = tool_description(&defs, "agent");
    assert!(
        session.contains("- status:"),
        "session description must document the status action"
    );
    assert!(
        agent.contains("agent action=wait") || agent.contains("wait: Block"),
        "agent description must document the blocking wait"
    );
    assert!(
        agent.contains("inbox:"),
        "agent description must document inbox"
    );
}

/// Every action a tool advertises must be documented by the tool itself.
///
/// Generalised for #754-affa from `instructions_tools_and_definitions_in_sync_for_session_actions`,
/// which checked one tool against the initialize instructions. The
/// instructions are the surface a client may never read (Codex ignores
/// them); the description is the surface every client receives, so that is
/// where the action list has to agree with the schema. This is the gate
/// that caught nine `progress_*` actions advertised by `REPO_ACTIONS` and
/// by the `repo` schema enum while the description body named none of them.
#[test]
fn every_documented_action_constant_matches_schema_and_description() {
    let defs = native_tool_definitions(true, true);

    // (tool, action constant, description documents each action)
    // `debug` is the one exemption: its description points at `action=help`,
    // which returns the full usage guide, so duplicating five action bullets
    // into a tool that is disabled by default would buy nothing.
    let cases: [(&str, &str, bool); 8] = [
        ("session", SESSION_ACTIONS, true),
        ("agent", AGENT_ACTIONS, true),
        ("task", TASK_ACTIONS, true),
        ("repo", REPO_ACTIONS, true),
        ("ui", UI_ACTIONS, true),
        ("config", CONFIG_ACTIONS, true),
        ("voice", VOICE_ACTIONS, true),
        ("debug", DEBUG_ACTIONS, false),
    ];

    for (tool, actions, documented) in cases {
        let def = defs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == tool)
            .unwrap_or_else(|| panic!("native tool '{tool}' missing"));
        let enum_desc = def["inputSchema"]["properties"]["action"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("tool '{tool}' has no action description"));
        let schema_actions: std::collections::BTreeSet<&str> = enum_desc
            .strip_prefix("One of: ")
            .unwrap_or_else(|| {
                panic!(
                    "tool '{tool}' action description must start with 'One of: ', got: {enum_desc}"
                )
            })
            .split(", ")
            .collect();
        let constant_actions: std::collections::BTreeSet<&str> = actions.split(", ").collect();
        assert_eq!(
            schema_actions, constant_actions,
            "tool '{tool}': schema enum and its action constant disagree"
        );

        if !documented {
            continue;
        }
        let description = def["description"].as_str().unwrap();
        for action in &constant_actions {
            assert!(
                description.contains(&format!("{action}:")),
                "tool '{tool}' advertises action '{action}' but its description never documents it"
            );
        }
    }
}

/// After `rebuild_tool_search_index`, the cache contains every native
/// tool from `native_tool_definitions(true, true)`.
#[test]
fn rebuild_tool_search_index_populates_all_native_tools() {
    let state = test_state();
    rebuild_tool_search_index(&state);
    let idx = state.mcp.tool_search_index.read();
    let native_count = native_tool_definitions(true, true)
        .as_array()
        .unwrap()
        .len();
    assert_eq!(idx.len(), native_count);
    // Spot-check a few well-known native tools by name.
    assert!(idx.get_schema("session").is_some());
    assert!(idx.get_schema("repo").is_some());
    assert!(idx.get_schema("agent").is_some());
}

/// After mutating `disabled_native_tools` and rebuilding, the disabled
/// tool no longer appears in the cached index.
#[test]
fn rebuild_tool_search_index_respects_disabled_native_tools() {
    let state = test_state();
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    rebuild_tool_search_index(&state);
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_none()
    );
}

/// The background updater task subscribes to `mcp_tools_changed` and
/// rebuilds the cached index on every signal. This is what wires upstream
/// add/remove, native-tool toggle, and collapse-tools toggle events into
/// the cache without each call site having to rebuild manually.
#[tokio::test]
async fn tool_search_index_rebuilds_on_broadcast() {
    let state = test_state();

    // Start the updater — it does its own initial rebuild, then loops on the broadcast.
    spawn_tool_search_index_updater(state.clone());
    // Give the initial build a moment to land.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );

    // Mutate config and fire the signal; the updater must rebuild.
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    let _ = state.mcp.tools_changed.send(());

    // Poll for the rebuild with a short deadline — the task is async.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_none()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("tool_search_index was not rebuilt after mcp_tools_changed signal");
}

/// Toggling `collapse_tools` must not corrupt the searchable corpus:
/// the cache always holds the full tool list regardless of the collapse
/// state (collapse only affects what the client sees via tools/list).
#[tokio::test]
async fn tool_search_index_ignores_collapse_tools_toggle() {
    let state = test_state();
    spawn_tool_search_index_updater(state.clone());
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let before = state.mcp.tool_search_index.read().len();

    state.config.write().collapse_tools = true;
    let _ = state.mcp.tools_changed.send(());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let after = state.mcp.tool_search_index.read().len();
    assert_eq!(
        before, after,
        "collapse_tools toggle must not change searchable corpus size"
    );
    // And native tools must still be searchable.
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );
}

mod secret_form_dispatch {
    //! Critic tests for story 1435: the secret-form inspection block must hold on
    //! every dispatch path, not only the ones that reach `handle_mcp_tool_call_with_context`.
    use crate::mcp_http::mcp_transport::mcp_post;
    use crate::mcp_http::tests::test_state;
    use crate::secrets::{Field, FieldKind, Form};
    use axum::{
        Json,
        extract::{ConnectInfo, State},
        http::HeaderMap,
        response::IntoResponse,
    };

    async fn tools_call_text(state: std::sync::Arc<crate::AppState>, name: &str) -> String {
        let response = mcp_post(
            State(state),
            ConnectInfo("127.0.0.1:12345".parse().unwrap()),
            HeaderMap::new(),
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": name, "arguments": {}}
            })),
        )
        .await
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        body["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn open_form(state: &crate::AppState) {
        let form = Form::request(
            vec![Field {
                name: "TOKEN".into(),
                kind: FieldKind::Password,
                display: None,
            }],
            "critic".into(),
        )
        .unwrap();
        state.secrets.open(form).unwrap();
    }

    /// Catches: a direct `tools/call` of an upstream-namespaced tool (maccontrol
    /// capture_screen / capture_window / do / shell) skips the secret-form block
    /// because the `__` branch of `mcp_post` proxies without consulting the store,
    /// so an agent can screenshot or keystroke the private form.
    #[tokio::test]
    async fn direct_upstream_tools_call_is_refused_while_secret_form_is_open() {
        let state = test_state();
        open_form(&state);
        let text = tools_call_text(state, "maccontrol__capture_screen").await;
        assert!(
            text.contains("private secret form"),
            "upstream tool was not gated by the open secret form: {text}"
        );
    }

    /// Catches: gating only the native names while `call_tool` wrapping an
    /// upstream name slips through (control for the case above).
    #[tokio::test]
    async fn call_tool_wrapped_upstream_is_refused_while_secret_form_is_open() {
        let state = test_state();
        open_form(&state);
        let response = super::mcp_transport::handle_mcp_tool_call(
            &state,
            "127.0.0.1:12345".parse().unwrap(),
            "call_tool",
            &serde_json::json!({"tool_name": "maccontrol__capture_screen", "arguments": {}}),
            None,
        )
        .await;
        assert!(
            response["error"]
                .as_str()
                .unwrap_or_default()
                .contains("private secret form"),
            "{response}"
        );
    }
}
